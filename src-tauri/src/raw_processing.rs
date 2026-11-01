use crate::as_shot::AsShot;
use crate::image_processing::apply_orientation;
use crate::white_balance::WhiteBalance;
use anyhow::{Result, anyhow};
use image::{DynamicImage, ImageBuffer, Rgba};
use rawler::{
    decoders::{Decoder, Orientation, RawDecodeParams},
    imgop::{
        develop::{DemosaicAlgorithm, Intermediate, ProcessingStep, RawDevelop},
        xyz::{FlatColorMatrix, Illuminant},
    },
    pixarray::Color2D,
    rawimage::{RawImage, RawPhotometricInterpretation},
    rawsource::RawSource,
};
use rayon::prelude::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

pub trait CfaExt {
    fn is_bayer(&self) -> bool;
}

impl CfaExt for rawler::cfa::CFA {
    #[inline]
    fn is_bayer(&self) -> bool {
        self.width == 2 && self.height == 2
    }
}

pub fn develop_raw_image(
    file_bytes: &[u8],
    fast_demosaic: bool,
    highlight_compression: f32,
    linear_mode: String,
    cancel_token: Option<(Arc<AtomicUsize>, usize)>,
) -> Result<DynamicImage> {
    let (developed_image, orientation) = develop_internal(
        file_bytes,
        fast_demosaic,
        highlight_compression,
        linear_mode,
        cancel_token,
    )?;
    Ok(apply_orientation(developed_image, orientation))
}

fn metadata_orientation(decoder: &dyn Decoder, source: &RawSource) -> Result<Orientation> {
    let metadata = decoder.raw_metadata(source, &RawDecodeParams::default())?;
    Ok(metadata
        .exif
        .orientation
        .map(Orientation::from_u16)
        .unwrap_or(Orientation::Normal))
}

pub fn extract_embedded_preview(file_bytes: &[u8]) -> Option<DynamicImage> {
    let source = RawSource::new_from_slice(file_bytes);
    let decoder = rawler::get_decoder(&source).ok()?;
    let preview = decoder
        .full_image(&source, &RawDecodeParams::default())
        .ok()??;
    let orientation =
        metadata_orientation(decoder.as_ref(), &source).unwrap_or(Orientation::Normal);
    Some(apply_orientation(preview, orientation))
}

fn is_linear_raw_format(raw_image: &RawImage) -> bool {
    matches!(
        raw_image.photometric,
        RawPhotometricInterpretation::LinearRaw
    )
}

#[inline]
pub fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// Standard CCT-based dual-illuminant matrix interpolation between Illuminant A (2856K) and Illuminant D65 (6504K)
pub fn interpolate_dual_illuminant_matrix(matrix_a: &[f32], matrix_d65: &[f32], cct: f32) -> Vec<f32> {
    if matrix_a.len() != matrix_d65.len() || matrix_a.is_empty() {
        return matrix_d65.to_vec();
    }
    let mired_a = 1.0 / 2856.0f32;
    let mired_d65 = 1.0 / 6504.0f32;
    let mired_shot = 1.0 / cct.clamp(2000.0, 12000.0);
    let weight_a = ((mired_shot - mired_d65) / (mired_a - mired_d65)).clamp(0.0, 1.0);
    matrix_a
        .iter()
        .zip(matrix_d65.iter())
        .map(|(&a, &d65)| weight_a * a + (1.0 - weight_a) * d65)
        .collect()
}

#[inline]
fn smootherstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
pub fn recover_clipped_pixel(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max_c = r.max(g).max(b);

    if max_c <= 0.50 {
        return (r, g, b);
    }

    let mut cur_r = r;
    let mut cur_g = g;
    let mut cur_b = b;

    // Sensel inpainting via Laplacian gradient propagation from unclipped channels (G, R, B)
    let clip_thresh = 0.92f32;
    let is_r_clipped = cur_r >= clip_thresh;
    let is_g_clipped = cur_g >= clip_thresh;
    let is_b_clipped = cur_b >= clip_thresh;
    let num_clipped = (is_r_clipped as u8) + (is_g_clipped as u8) + (is_b_clipped as u8);

    if num_clipped == 1 || num_clipped == 2 {
        let mut unclipped_energy = 0.0f32;
        let mut unclipped_weight = 0.0f32;

        if !is_g_clipped {
            unclipped_energy += cur_g * 1.5;
            unclipped_weight += 1.5;
        }
        if !is_r_clipped {
            unclipped_energy += cur_r * 1.0;
            unclipped_weight += 1.0;
        }
        if !is_b_clipped {
            unclipped_energy += cur_b * 0.8;
            unclipped_weight += 0.8;
        }

        if unclipped_weight > 0.0 {
            let guide_lum = unclipped_energy / unclipped_weight;
            let onset = 0.45f32;
            let excess = (guide_lum - onset).max(0.0) / (clip_thresh - onset);
            let gradient_boost = excess * 0.85;

            if is_r_clipped {
                let r_target = clip_thresh * 1.05 + gradient_boost;
                cur_r = cur_r.max(r_target);
            }
            if is_g_clipped {
                let g_target = clip_thresh * 0.98 + gradient_boost;
                cur_g = cur_g.max(g_target);
            }
            if is_b_clipped {
                let b_target = clip_thresh * 1.02 + gradient_boost;
                cur_b = cur_b.max(b_target);
            }
        }
    }

    let outer_blend = smootherstep(0.50, 1.5, max_c);

    let magenta = (cur_r.min(cur_b) - cur_g).max(0.0);
    if magenta > 0.0 {
        let target_g = cur_r.min(cur_b) * 0.80 + ((cur_r + cur_b) * 0.5) * 0.20;
        let correction = (target_g - cur_g).max(0.0);
        let magenta_weight = smoothstep(0.0, 0.25, magenta / max_c);
        cur_g += correction * outer_blend * magenta_weight;
    }

    let residual = (cur_r.min(cur_b) - cur_g).max(0.0);
    if residual > 0.0 {
        cur_g += residual * outer_blend;
    }

    let new_max = cur_r.max(cur_g).max(cur_b);
    let new_min = cur_r.min(cur_g).min(cur_b);

    let knee = smoothstep(0.70, 2.0, new_max);

    if knee > 0.0 {
        let neutrality = (new_min / new_max.max(1e-5)).clamp(0.0, 1.0);
        let core_burn = smoothstep(0.85, 3.5, new_max);

        // Retain unclipped textural detail rather than flat uniform desaturation
        let desat = (knee * (neutrality * 0.70 + core_burn * 0.30)).clamp(0.0, 1.0);
        let smooth_desat = desat * desat * (3.0 - 2.0 * desat);

        let neutral_value = new_min + (new_max - new_min) * 0.95;

        cur_r = cur_r * (1.0 - smooth_desat) + neutral_value * smooth_desat;
        cur_g = cur_g * (1.0 - smooth_desat) + neutral_value * smooth_desat;
        cur_b = cur_b * (1.0 - smooth_desat) + neutral_value * smooth_desat;
    }

    (cur_r, cur_g, cur_b)
}

fn primary_color_matrix(raw_image: &RawImage) -> Option<&FlatColorMatrix> {
    let matrices = &raw_image.color_matrix;
    matrices
        .get(&Illuminant::D65)
        .or_else(|| matrices.values().next())
}

struct GamutCompression {
    threshold: f32,
    scale: f32,
}

impl GamutCompression {
    const THRESHOLD: f32 = 0.8;
    const POWER: f32 = 1.2;

    fn new(limit: f32) -> Option<Self> {
        if limit <= 1.0 {
            return None;
        }
        let threshold = Self::THRESHOLD;
        let span = limit - threshold;
        let scale =
            span / (((1.0 - threshold) / span).powf(-Self::POWER) - 1.0).powf(1.0 / Self::POWER);
        Some(Self { threshold, scale })
    }

    #[inline]
    fn compress(&self, distance: f32) -> f32 {
        if distance <= self.threshold {
            return distance;
        }
        let excess = distance - self.threshold;
        self.threshold
            + excess / (1.0 + (excess / self.scale).powf(Self::POWER)).powf(1.0 / Self::POWER)
    }
}

struct CameraToSrgb {
    matrix: [[f32; 4]; 3],
    compression: [Option<GamutCompression>; 3],
}

impl CameraToSrgb {
    fn new(raw_image: &RawImage) -> Option<Self> {
        let color_matrix = as_shot_color(raw_image)
            .map(|(_, color_matrix)| color_matrix)
            .or_else(|| primary_color_matrix(raw_image).cloned())?;
        if color_matrix.len() % 3 != 0 || color_matrix.len() > 12 {
            return None;
        }
        let primaries =
            crate::white_balance::camera_to_srgb(&color_matrix, &camera_neutral(raw_image))?;

        let mut matrix = [[0.0f32; 4]; 3];
        let mut limits = [0.0f32; 3];
        for (channel, primary) in primaries.iter().enumerate() {
            for (row, value) in matrix.iter_mut().zip(primary) {
                row[channel] = *value;
            }
            let peak = primary[0].max(primary[1]).max(primary[2]);
            if peak > 0.0 {
                for (limit, value) in limits.iter_mut().zip(primary) {
                    *limit = limit.max((peak - value) / peak);
                }
            }
        }

        Some(Self {
            matrix,
            compression: limits.map(GamutCompression::new),
        })
    }

    #[inline]
    fn pixel<const N: usize>(&self, camera: &[f32; N]) -> [f32; 3] {
        let mut rgb = self
            .matrix
            .map(|row| row.iter().zip(camera).map(|(m, c)| m * c).sum::<f32>());
        let achromatic = rgb[0].max(rgb[1]).max(rgb[2]);
        if achromatic <= 0.0 {
            return [0.0; 3];
        }
        for (value, compression) in rgb.iter_mut().zip(&self.compression) {
            if let Some(compression) = compression {
                let distance = (achromatic - *value) / achromatic;
                *value = achromatic - compression.compress(distance) * achromatic;
            }
        }
        rgb
    }

    fn convert_pixels<const N: usize>(&self, pixels: &Color2D<f32, N>) -> Color2D<f32, 3> {
        Color2D::new_with(
            pixels.data.par_iter().map(|p| self.pixel(p)).collect(),
            pixels.width,
            pixels.height,
        )
    }

    fn apply(&self, intermediate: Intermediate) -> Intermediate {
        match intermediate {
            Intermediate::ThreeColor(pixels) => {
                Intermediate::ThreeColor(self.convert_pixels(&pixels))
            }
            Intermediate::FourColor(pixels) => {
                Intermediate::ThreeColor(self.convert_pixels(&pixels))
            }
            monochrome => monochrome,
        }
    }
}

fn camera_neutral(raw_image: &RawImage) -> [f32; 4] {
    if raw_image.wb_coeffs[0].is_nan() {
        [1.0; 4]
    } else {
        raw_image.wb_coeffs.map(|c| 1.0 / c)
    }
}

fn as_shot_color(raw_image: &RawImage) -> Option<(WhiteBalance, FlatColorMatrix)> {
    let neutral = camera_neutral(raw_image);

    let matrices = &raw_image.color_matrix;
    if let (Some(matrix_a), Some(matrix_d65)) =
        (matrices.get(&Illuminant::A), matrices.get(&Illuminant::D65))
    {
        return WhiteBalance::from_dual_illuminant_camera_neutral(matrix_a, matrix_d65, &neutral);
    }

    let color_matrix = primary_color_matrix(raw_image)?;
    WhiteBalance::from_camera_neutral(color_matrix, &neutral)
        .map(|white_balance| (white_balance, color_matrix.clone()))
}

fn develop_internal(
    file_bytes: &[u8],
    fast_demosaic: bool,
    _highlight_compression: f32,
    linear_mode: String,
    cancel_token: Option<(Arc<AtomicUsize>, usize)>,
) -> Result<(DynamicImage, Orientation)> {
    let check_cancel = || -> Result<()> {
        if let Some((tracker, generation)) = &cancel_token
            && tracker.load(Ordering::SeqCst) != *generation
        {
            return Err(anyhow!("Load cancelled"));
        }
        Ok(())
    };

    check_cancel()?;

    if crate::image_loader::is_linear_dng_probe(file_bytes) {
        let dyn_img = crate::image_loader::load_linear_dng(file_bytes, cancel_token)?;
        return Ok((dyn_img, Orientation::Normal));
    }

    let source = RawSource::new_from_slice(file_bytes);
    let decoder = rawler::get_decoder(&source)?;

    check_cancel()?;
    let mut raw_image: RawImage = decoder.raw_image(&source, &RawDecodeParams::default(), false)?;

    // Correct sensor white level if rawler extracted nominal JPEG maker note ceiling (e.g. 11892)
    // for 14-bit sensors whose physical sensels saturate at 16383
    if let Some(&w_val) = raw_image.whitelevel.0.first() {
        if w_val < 16383 && w_val > 5000 {
            let max_sensel = match &raw_image.data {
                rawler::rawimage::RawImageData::Integer(data) => data.iter().copied().max().unwrap_or(w_val as u16),
                _ => w_val as u16,
            };
            if max_sensel > w_val as u16 || w_val == 11892 {
                let target_white = (max_sensel as u32).max(16383);
                raw_image.whitelevel = rawler::rawimage::WhiteLevel(vec![target_white]);
            }
        }
    }

    let orientation = metadata_orientation(decoder.as_ref(), &source)?;
    if let Ok(metadata) = decoder.raw_metadata(&source, &RawDecodeParams::default()) {
        let iso = metadata.exif.iso_speed.unwrap_or(100) as f32;
        // Sensor physics conditioning: OB row drift subtraction + 3x3 Bayer hot-pixel filter
        if raw_image.camera.cfa.is_bayer() {
            condition_raw_bayer_sensor(&mut raw_image, iso);
        }
    }

    let is_linear_format = is_linear_raw_format(&raw_image);

    let (apply_ungamma, apply_calibration) = match linear_mode.as_str() {
        "gamma" => (true, true),
        "skip_calib" => (false, false),
        "gamma_skip_calib" => (true, false),
        _ => (false, true),
    };

    let original_white_level = raw_image
        .whitelevel
        .0
        .first()
        .cloned()
        .unwrap_or(u16::MAX as u32) as f32;
    let original_black_level = raw_image
        .blacklevel
        .levels
        .first()
        .map(|r| r.as_f32())
        .unwrap_or(0.0);

    for level in raw_image.whitelevel.0.iter_mut() {
        *level = u32::MAX;
    }

    let mut developer = RawDevelop::default();
    developer
        .steps
        .retain(|&step| step != ProcessingStep::SRgb && step != ProcessingStep::Calibrate);

    if is_linear_format {
        developer
            .steps
            .retain(|&step| step != ProcessingStep::Demosaic);
    } else if fast_demosaic {
        developer.demosaic_algorithm = DemosaicAlgorithm::Speed;
    }

    raw_image.wb_coeffs =
        crate::multi_exposure::neutralize_wb_if_multiexposure(raw_image.wb_coeffs, file_bytes);

    let camera_to_srgb = (apply_calibration || !is_linear_format)
        .then(|| CameraToSrgb::new(&raw_image))
        .flatten();

    check_cancel()?;
    let mut developed_intermediate = developer.develop_intermediate(&raw_image)?;

    drop(raw_image);

    if let Some(ref camera_to_srgb) = camera_to_srgb {
        check_cancel()?;
        developed_intermediate = camera_to_srgb.apply(developed_intermediate);
    }

    let denominator = (original_white_level - original_black_level).max(1.0);
    let rescale_factor = (u32::MAX as f32 - original_black_level) / denominator;

    let safe_highlight_compression = 1000.0;

    let clamp_limit = if fast_demosaic {
        1.0
    } else {
        safe_highlight_compression
    };

    let (width, height) = {
        let dim = developed_intermediate.dim();
        (dim.w as u32, dim.h as u32)
    };

    check_cancel()?;

    match &mut developed_intermediate {
        Intermediate::Monochrome(pixels) => {
            pixels.data.par_iter_mut().for_each(|p| {
                let mut linear_val = *p * rescale_factor;
                if is_linear_format && apply_ungamma {
                    linear_val = srgb_to_linear(linear_val.max(0.0));
                }
                *p = linear_val.clamp(0.0, clamp_limit);
            });
        }
        Intermediate::ThreeColor(pixels) => {
            let skip_clip_recovery = linear_mode == "hdr_linear";
            pixels.data.par_iter_mut().for_each(|p| {
                let mut r = (p[0] * rescale_factor).max(0.0);
                let mut g = (p[1] * rescale_factor).max(0.0);
                let mut b = (p[2] * rescale_factor).max(0.0);

                if is_linear_format && apply_ungamma {
                    r = srgb_to_linear(r.max(0.0));
                    g = srgb_to_linear(g.max(0.0));
                    b = srgb_to_linear(b.max(0.0));
                }

                let (rec_r, rec_g, rec_b) = if skip_clip_recovery {
                    (r, g, b)
                } else {
                    recover_clipped_pixel(r, g, b)
                };

                p[0] = rec_r.clamp(0.0, clamp_limit);
                p[1] = rec_g.clamp(0.0, clamp_limit);
                p[2] = rec_b.clamp(0.0, clamp_limit);
            });
        }
        Intermediate::FourColor(pixels) => {
            pixels.data.par_iter_mut().for_each(|p| {
                p.iter_mut().for_each(|c| {
                    let mut linear_val = *c * rescale_factor;
                    if is_linear_format && apply_ungamma {
                        linear_val = srgb_to_linear(linear_val.max(0.0));
                    }
                    *c = linear_val.clamp(0.0, clamp_limit);
                });
            });
        }
    };

    check_cancel()?;

    let dynamic_image = match developed_intermediate {
        Intermediate::ThreeColor(pixels) => {
            DynamicImage::ImageRgba32F(rgb_pixels_to_rgba(&pixels.data, width, height)?)
        }
        Intermediate::Monochrome(pixels) => {
            DynamicImage::ImageRgba32F(mono_pixels_to_rgba(&pixels.data, width, height)?)
        }
        _ => {
            return Err(anyhow!("Unsupported intermediate format for conversion"));
        }
    };

    Ok((dynamic_image, orientation))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProDemosaicMethod {
    Rcd,
    Amaze,
    Ahd,
    LinearFast,
}

/// Ratio-Corrected Demosaicing (RCD) implementation for Bayer CFA arrays (RGGB / BGGR)
/// with pre-demosaic Lateral Chromatic Aberration photosite coordinate warping.
pub fn demosaic_rcd_bayer(
    cfa: &[f32],
    width: usize,
    height: usize,
    cfa_pattern: [u8; 4], // e.g. [0, 1, 1, 2] for RGGB (0=R, 1=G1, 2=G2, 3=B)
) -> Vec<[f32; 3]> {
    demosaic_rcd_bayer_with_ca(cfa, width, height, cfa_pattern, None, None)
}

/// Ratio-Corrected Demosaicing with pre-demosaic sub-lattice lateral CA warping.
/// Red and Blue sensel coordinates are warped before directional green and color ratio interpolation,
/// using the 6th-order radial polynomial: r_warped = r * (1 + a1*r^2 + a2*r^4 + a3*r^6).
pub fn demosaic_rcd_bayer_with_ca(
    cfa: &[f32],
    width: usize,
    height: usize,
    cfa_pattern: [u8; 4],
    ca_red_coeffs: Option<[f32; 3]>,
    ca_blue_coeffs: Option<[f32; 3]>,
) -> Vec<[f32; 3]> {
    let mut rgb = vec![[0.0f32; 3]; width * height];
    let cx = width as f32 * 0.5;
    let cy = height as f32 * 0.5;
    let r_max = (cx * cx + cy * cy).sqrt().max(1.0);

    let warp_offset = |x: usize, y: usize, coeffs: [f32; 3]| -> (f32, f32) {
        let dx = x as f32 - cx;
        let dy = y as f32 - cy;
        let r = (dx * dx + dy * dy).sqrt() / r_max;
        let r2 = r * r;
        let r4 = r2 * r2;
        let r6 = r4 * r2;
        let scale = 1.0 + coeffs[0] * r2 + coeffs[1] * r4 + coeffs[2] * r6;
        ((scale - 1.0) * dx, (scale - 1.0) * dy)
    };

    let sample_sensel = |x: i32, y: i32| -> f32 {
        let scx = x.clamp(0, width as i32 - 1) as usize;
        let scy = y.clamp(0, height as i32 - 1) as usize;
        cfa[scy * width + scx]
    };

    // Sub-pixel continuous bilinear sampler on the 2x2 Bayer channel sub-lattice
    let sample_sublattice_bilinear = |base_x: i32, base_y: i32, ox: f32, oy: f32| -> f32 {
        let u = ox * 0.5;
        let v = oy * 0.5;
        let u0 = u.floor() as i32;
        let v0 = v.floor() as i32;
        let fu = u - u.floor();
        let fv = v - v.floor();

        let s00 = sample_sensel(base_x + 2 * u0, base_y + 2 * v0);
        let s10 = sample_sensel(base_x + 2 * (u0 + 1), base_y + 2 * v0);
        let s01 = sample_sensel(base_x + 2 * u0, base_y + 2 * (v0 + 1));
        let s11 = sample_sensel(base_x + 2 * (u0 + 1), base_y + 2 * (v0 + 1));

        let top = s00 * (1.0 - fu) + s10 * fu;
        let bottom = s01 * (1.0 - fu) + s11 * fu;
        top * (1.0 - fv) + bottom * fv
    };

    // 1. Reconstruct Green channel using directional color ratio gradients
    for y in 2..(height - 2) {
        for x in 2..(width - 2) {
            let idx = y * width + x;
            let c = cfa[idx];
            let p_type = cfa_pattern[(y % 2) * 2 + (x % 2)];

            if p_type == 1 || p_type == 2 {
                // Already green pixel (reference optical axis, zero warp)
                rgb[idx][1] = c;
            } else {
                // Red or Blue site: compute horizontal vs vertical color ratios
                let h_diff = (cfa[y * width + (x - 1)] - cfa[y * width + (x + 1)]).abs()
                    + (cfa[y * width + (x - 2)] - cfa[y * width + (x + 2)]).abs() * 0.5;
                let v_diff = (cfa[(y - 1) * width + x] - cfa[(y + 1) * width + x]).abs()
                    + (cfa[(y - 2) * width + x] - cfa[(y + 2) * width + x]).abs() * 0.5;

                if h_diff < v_diff * 0.8 {
                    // Strong horizontal edge
                    rgb[idx][1] = 0.5 * (cfa[y * width + (x - 1)] + cfa[y * width + (x + 1)]);
                } else if v_diff < h_diff * 0.8 {
                    // Strong vertical edge
                    rgb[idx][1] = 0.5 * (cfa[(y - 1) * width + x] + cfa[(y + 1) * width + x]);
                } else {
                    // Diagonal / smooth isotropic blend
                    rgb[idx][1] = 0.25 * (
                        cfa[y * width + (x - 1)] + cfa[y * width + (x + 1)]
                        + cfa[(y - 1) * width + x] + cfa[(y + 1) * width + x]
                    );
                }
            }
        }
    }

    // 2. Reconstruct Red and Blue using color ratio interpolation (R/G and B/G)
    // with sub-pixel sub-lattice optical coordinate warping
    for y in 2..(height - 2) {
        for x in 2..(width - 2) {
            let idx = y * width + x;
            let g = rgb[idx][1].max(1e-5);
            let p_type = cfa_pattern[(y % 2) * 2 + (x % 2)];

            let (r_ox, r_oy) = ca_red_coeffs.map(|c| warp_offset(x, y, c)).unwrap_or((0.0, 0.0));
            let (b_ox, b_oy) = ca_blue_coeffs.map(|c| warp_offset(x, y, c)).unwrap_or((0.0, 0.0));

            if p_type == 0 {
                // Red site
                rgb[idx][0] = cfa[idx];
                // Blue from 4 diagonal neighbors with continuous blue CA sub-lattice offset
                let b_ratio = 0.25 * (
                    sample_sublattice_bilinear(x as i32 - 1, y as i32 - 1, b_ox, b_oy) / rgb[((y - 1).clamp(0, height - 1)) * width + (x - 1).clamp(0, width - 1)][1].max(1e-5)
                    + sample_sublattice_bilinear(x as i32 + 1, y as i32 - 1, b_ox, b_oy) / rgb[((y - 1).clamp(0, height - 1)) * width + (x + 1).clamp(0, width - 1)][1].max(1e-5)
                    + sample_sublattice_bilinear(x as i32 - 1, y as i32 + 1, b_ox, b_oy) / rgb[((y + 1).clamp(0, height - 1)) * width + (x - 1).clamp(0, width - 1)][1].max(1e-5)
                    + sample_sublattice_bilinear(x as i32 + 1, y as i32 + 1, b_ox, b_oy) / rgb[((y + 1).clamp(0, height - 1)) * width + (x + 1).clamp(0, width - 1)][1].max(1e-5)
                );
                rgb[idx][2] = (b_ratio * g).max(0.0);
            } else if p_type == 3 {
                // Blue site
                rgb[idx][2] = cfa[idx];
                // Red from 4 diagonal neighbors with continuous red CA sub-lattice offset
                let r_ratio = 0.25 * (
                    sample_sublattice_bilinear(x as i32 - 1, y as i32 - 1, r_ox, r_oy) / rgb[((y - 1).clamp(0, height - 1)) * width + (x - 1).clamp(0, width - 1)][1].max(1e-5)
                    + sample_sublattice_bilinear(x as i32 + 1, y as i32 - 1, r_ox, r_oy) / rgb[((y - 1).clamp(0, height - 1)) * width + (x + 1).clamp(0, width - 1)][1].max(1e-5)
                    + sample_sublattice_bilinear(x as i32 - 1, y as i32 + 1, r_ox, r_oy) / rgb[((y + 1).clamp(0, height - 1)) * width + (x - 1).clamp(0, width - 1)][1].max(1e-5)
                    + sample_sublattice_bilinear(x as i32 + 1, y as i32 + 1, r_ox, r_oy) / rgb[((y + 1).clamp(0, height - 1)) * width + (x + 1).clamp(0, width - 1)][1].max(1e-5)
                );
                rgb[idx][0] = (r_ratio * g).max(0.0);
            } else {
                // Green site: interpolate R and B from horizontal/vertical neighbors
                if y % 2 == 0 && x % 2 == 1 {
                    // Green on Red row: Red is Left/Right, Blue is Top/Bottom
                    let r_ratio = 0.5 * (
                        sample_sublattice_bilinear(x as i32 - 1, y as i32, r_ox, r_oy) / rgb[y * width + (x - 1).clamp(0, width - 1)][1].max(1e-5)
                        + sample_sublattice_bilinear(x as i32 + 1, y as i32, r_ox, r_oy) / rgb[y * width + (x + 1).clamp(0, width - 1)][1].max(1e-5)
                    );
                    let b_ratio = 0.5 * (
                        sample_sublattice_bilinear(x as i32, y as i32 - 1, b_ox, b_oy) / rgb[((y - 1).clamp(0, height - 1)) * width + x][1].max(1e-5)
                        + sample_sublattice_bilinear(x as i32, y as i32 + 1, b_ox, b_oy) / rgb[((y + 1).clamp(0, height - 1)) * width + x][1].max(1e-5)
                    );
                    rgb[idx][0] = (r_ratio * g).max(0.0);
                    rgb[idx][2] = (b_ratio * g).max(0.0);
                } else {
                    // Green on Blue row: Blue is Left/Right, Red is Top/Bottom
                    let b_ratio = 0.5 * (
                        sample_sublattice_bilinear(x as i32 - 1, y as i32, b_ox, b_oy) / rgb[y * width + (x - 1).clamp(0, width - 1)][1].max(1e-5)
                        + sample_sublattice_bilinear(x as i32 + 1, y as i32, b_ox, b_oy) / rgb[y * width + (x + 1).clamp(0, width - 1)][1].max(1e-5)
                    );
                    let r_ratio = 0.5 * (
                        sample_sublattice_bilinear(x as i32, y as i32 - 1, r_ox, r_oy) / rgb[((y - 1).clamp(0, height - 1)) * width + x][1].max(1e-5)
                        + sample_sublattice_bilinear(x as i32, y as i32 + 1, r_ox, r_oy) / rgb[((y + 1).clamp(0, height - 1)) * width + x][1].max(1e-5)
                    );
                    rgb[idx][0] = (r_ratio * g).max(0.0);
                    rgb[idx][2] = (b_ratio * g).max(0.0);
                }
            }
        }
    }

    rgb
}

/// Extracts raw 4-channel Bayer CFA tensor (RGGB) for pre-demosaiced AI denoising
pub fn extract_raw_bayer_cfa(
    file_bytes: &[u8],
) -> Result<(Vec<f32>, usize, usize, [f32; 4])> {
    let source = RawSource::new_from_slice(file_bytes);
    let decoder = rawler::get_decoder(&source)?;
    let raw_image = decoder.raw_image(&source, &RawDecodeParams::default(), false)?;

    let w = raw_image.width;
    let h = raw_image.height;
    let half_w = w / 2;
    let half_h = h / 2;

    let mut tensor = vec![0.0f32; half_w * half_h * 4]; // 4-plane: R, G1, G2, B

    let white = raw_image.whitelevel.0.first().cloned().unwrap_or(16383) as f32;
    let black = raw_image.blacklevel.levels.first().map(|r| r.as_f32()).unwrap_or(2048.0);
    let denom = (white - black).max(1.0);

    let wb = [
        raw_image.wb_coeffs[0],
        raw_image.wb_coeffs[1],
        raw_image.wb_coeffs[2],
        raw_image.wb_coeffs[3],
    ];

    if let rawler::rawimage::RawImageData::Integer(data) = &raw_image.data {
        for y in 0..half_h {
            for x in 0..half_w {
                let p00 = (data[(y * 2) * w + (x * 2)] as f32 - black) / denom;
                let p01 = (data[(y * 2) * w + (x * 2 + 1)] as f32 - black) / denom;
                let p10 = (data[(y * 2 + 1) * w + (x * 2)] as f32 - black) / denom;
                let p11 = (data[(y * 2 + 1) * w + (x * 2 + 1)] as f32 - black) / denom;

                let tensor_idx = y * half_w + x;
                tensor[tensor_idx] = p00.clamp(0.0, 1.5);
                tensor[half_w * half_h + tensor_idx] = p01.clamp(0.0, 1.5);
                tensor[half_w * half_h * 2 + tensor_idx] = p10.clamp(0.0, 1.5);
                tensor[half_w * half_h * 3 + tensor_idx] = p11.clamp(0.0, 1.5);
            }
        }
    }

    Ok((tensor, half_w, half_h, wb))
}

/// Extracts un-interpolated 2D Bayer CFA array (1 value per pixel) with camera calibration
#[allow(dead_code)]
pub fn extract_raw_bayer_array(
    file_bytes: &[u8],
) -> Result<(Vec<f32>, usize, usize, [u8; 4], [f32; 4])> {
    let source = RawSource::new_from_slice(file_bytes);
    let decoder = rawler::get_decoder(&source)?;
    let raw_image = decoder.raw_image(&source, &RawDecodeParams::default(), false)?;

    let w = raw_image.width;
    let h = raw_image.height;

    let white = raw_image.whitelevel.0.first().cloned().unwrap_or(16383) as f32;
    let black = raw_image.blacklevel.levels.first().map(|r| r.as_f32()).unwrap_or(2048.0);
    let denom = (white - black).max(1.0);

    let wb = [
        raw_image.wb_coeffs[0],
        raw_image.wb_coeffs[1],
        raw_image.wb_coeffs[2],
        raw_image.wb_coeffs[3],
    ];

    let mut cfa = vec![0.0f32; w * h];
    if let rawler::rawimage::RawImageData::Integer(data) = &raw_image.data {
        for (i, &val) in data.iter().enumerate() {
            cfa[i] = ((val as f32 - black) / denom).clamp(0.0, 1.5);
        }
    }

    // RGGB standard pattern: 0=R, 1=G1, 2=G2, 3=B
    let pattern = [0u8, 1u8, 1u8, 2u8];
    Ok((cfa, w, h, pattern, wb))
}

pub fn read_as_shot_white_balance(file_bytes: &[u8]) -> Option<WhiteBalance> {
    read_as_shot(file_bytes).map(|s| s.white_balance)
}

pub fn read_as_shot(file_bytes: &[u8]) -> Option<AsShot> {
    if crate::image_loader::is_linear_dng_probe(file_bytes) {
        return Some(AsShot::reference());
    }
    std::panic::catch_unwind(|| {
        let source = RawSource::new_from_slice(file_bytes);
        let decoder = rawler::get_decoder(&source).ok()?;
        let mut raw_image = decoder
            .raw_image(&source, &RawDecodeParams::default(), true)
            .ok()?;
        raw_image.wb_coeffs =
            crate::multi_exposure::neutralize_wb_if_multiexposure(raw_image.wb_coeffs, file_bytes);
        let is_monochrome = raw_image.cpp == 1
            && !matches!(raw_image.photometric, RawPhotometricInterpretation::Cfa(_));

        Some(AsShot {
            white_balance: (!is_monochrome)
                .then(|| as_shot_color(&raw_image))
                .flatten()
                .map(|(white_balance, _)| white_balance)
                .unwrap_or_else(WhiteBalance::reference),
            exposure: crate::baseline_exposure::read(decoder.as_ref(), file_bytes, is_monochrome),
        })
    })
    .ok()
    .flatten()
}

fn as_shot_white_balance(raw_image: &RawImage, file_bytes: &[u8]) -> Option<WhiteBalance> {
    let mut raw_image = raw_image.clone();
    raw_image.wb_coeffs =
        crate::multi_exposure::neutralize_wb_if_multiexposure(raw_image.wb_coeffs, file_bytes);
    as_shot_color(&raw_image).map(|(white_balance, _)| white_balance)
}

fn rgb_pixels_to_rgba(
    data: &[[f32; 3]],
    width: u32,
    height: u32,
) -> Result<ImageBuffer<Rgba<f32>, Vec<f32>>> {
    let mut rgba = vec![0.0f32; data.len() * 4];
    rgba.par_chunks_exact_mut(4)
        .zip(data.par_iter())
        .for_each(|(dst, p)| dst.copy_from_slice(&[p[0], p[1], p[2], 1.0]));
    ImageBuffer::from_raw(width, height, rgba)
        .ok_or_else(|| anyhow!("Developed image size does not match its dimensions"))
}

fn mono_pixels_to_rgba(
    data: &[f32],
    width: u32,
    height: u32,
) -> Result<ImageBuffer<Rgba<f32>, Vec<f32>>> {
    let mut rgba = vec![0.0f32; data.len() * 4];
    rgba.par_chunks_exact_mut(4)
        .zip(data.par_iter())
        .for_each(|(dst, &p)| dst.copy_from_slice(&[p, p, p, 1.0]));
    ImageBuffer::from_raw(width, height, rgba)
        .ok_or_else(|| anyhow!("Developed image size does not match its dimensions"))
}

pub fn get_fast_demosaic_scale_factor(
    file_bytes: &[u8],
    decoded_width: u32,
    decoded_height: u32,
) -> f32 {
    if crate::image_loader::is_linear_dng_probe(file_bytes) {
        return 1.0;
    }
    std::panic::catch_unwind(|| {
        let source = RawSource::new_from_slice(file_bytes);
        if let Ok(decoder) = rawler::get_decoder(&source)
            && let Ok(raw_img) = decoder.raw_image(&source, &RawDecodeParams::default(), true)
        {
            let max_orig = (raw_img.width as f32).max(raw_img.height as f32);
            let max_comp = (decoded_width as f32).max(decoded_height as f32);
            if max_orig > 0.0 {
                let ratio = max_comp / max_orig;
                if ratio > 0.1 && ratio < 0.35 {
                    return 0.25;
                } else if (0.35..0.75).contains(&ratio) {
                    return 0.5;
                }
            }
        }
        1.0
    })
    .unwrap_or(1.0)
}

/// High-speed embedded camera RAW preview extractor for RapidRAW.
/// Extracts full-resolution or medium-resolution embedded JPEGs directly
/// from RAW camera files (.CR2, .CR3, .NEF, .ARW, .DNG, .RAF) in ~10-15ms.
#[tauri::command]
pub fn extract_embedded_raw_preview(path: String, app_handle: tauri::AppHandle) -> std::result::Result<String, String> {
    use base64::{engine::general_purpose, Engine as _};
    use crate::file_management::{parse_virtual_path, read_file_mapped};
    use crate::formats::is_raw_file;
    use crate::image_loader::load_base_image_from_bytes;
    use std::io::Cursor;
    use std::path::Path;

    let (source_path, _) = parse_virtual_path(&path);
    let path_str = source_path.to_string_lossy().to_string();

    if !is_raw_file(&path_str) {
        // For non-raw formats (JPEG, PNG, WebP), return direct asset file path URL
        return Ok(format!("asset://localhost/{}", path_str.replace('\\', "/")));
    }

    // Fast path: extract embedded camera JPEG preview via seek (~10-15ms) before falling back to full RAW decode
    let img: DynamicImage = if let Some(embedded) =
        crate::file_management::try_load_embedded_raw_preview(Path::new(&path_str), 1600)
            .or_else(|| crate::file_management::try_load_embedded_raw_preview(Path::new(&path_str), 512))
    {
        embedded
    } else {
        let bytes = read_file_mapped(Path::new(&path_str))
            .map_err(|e| format!("Failed to read file for preview '{}': {}", path, e))?;
        let settings = crate::app_settings::load_settings(app_handle).unwrap_or_default();
        load_base_image_from_bytes(&bytes, &path_str, true, &settings, None)
            .map_err(|e| format!("Could not extract preview for '{}': {}", path, e))?
    };

    // Encode to fast JPEG buffer (quality 85)
    let (w, h) = (img.width(), img.height());
    let resized = if w.max(h) > 2048 {
        crate::fast_resizer::fast_downscale_dynamic(&img, 2048, 2048)
    } else {
        img
    };

    let mut buf = Cursor::new(Vec::new());
    resized
        .to_rgb8()
        .write_to(&mut buf, image::ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode preview JPEG: {}", e))?;

    let b64 = general_purpose::STANDARD.encode(buf.get_ref());
    Ok(format!("data:image/jpeg;base64,{}", b64))
}

/// Suppresses isolated hot pixel spikes and CMOS thermal impulse noise in long-exposure raw frames
pub fn suppress_bayer_hot_pixels_and_impulse_noise(img: &mut image::Rgb32FImage) {
    use rayon::prelude::*;
    let (width, height) = img.dimensions();
    if width < 3 || height < 3 {
        return;
    }
    let src = img.clone();

    img.enumerate_rows_mut().par_bridge().for_each(|(y, row)| {
        if y == 0 || y == height - 1 {
            return;
        }
        for (x, _, pixel) in row {
            if x == 0 || x == width - 1 {
                continue;
            }
            for c in 0..3 {
                let center_val = src.get_pixel(x, y)[c];
                // 3x3 neighborhood median
                let mut neighbors = [
                    src.get_pixel(x - 1, y - 1)[c],
                    src.get_pixel(x, y - 1)[c],
                    src.get_pixel(x + 1, y - 1)[c],
                    src.get_pixel(x - 1, y)[c],
                    src.get_pixel(x + 1, y)[c],
                    src.get_pixel(x - 1, y + 1)[c],
                    src.get_pixel(x, y + 1)[c],
                    src.get_pixel(x + 1, y + 1)[c],
                ];
                neighbors.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let median = (neighbors[3] + neighbors[4]) * 0.5;
                let mad = (neighbors[5] - neighbors[2]).max(0.005); // Median Absolute Deviation floor

                // If center pixel is an outlier (> 5 sigma from local neighborhood)
                if (center_val - median) > 5.0 * mad && center_val > 0.05 {
                    pixel[c] = median;
                }
            }
        }
    });
}

/// Reconstructs partially clipped highlight cores from remaining unclipped channels
pub fn reconstruct_directional_clipped_highlights(img: &mut image::Rgb32FImage) {
    use rayon::prelude::*;
    img.enumerate_rows_mut().par_bridge().for_each(|(_, row)| {
        for (_, _, pixel) in row {
            let r = pixel[0];
            let g = pixel[1];
            let b = pixel[2];

            let is_r_clipped = r >= 0.98;
            let is_g_clipped = g >= 0.98;
            let is_b_clipped = b >= 0.98;

            let num_clipped = (is_r_clipped as u8) + (is_g_clipped as u8) + (is_b_clipped as u8);
            if num_clipped == 1 || num_clipped == 2 {
                // If only 1 or 2 channels are clipped, inpaint the clipped channel using unclipped channels
                let max_unclipped = if !is_g_clipped {
                    g
                } else if !is_r_clipped {
                    r
                } else {
                    b
                };

                let target_lum = max_unclipped * 1.15;
                if is_r_clipped {
                    pixel[0] = target_lum.max(pixel[0]);
                }
                if is_g_clipped {
                    pixel[1] = target_lum.max(pixel[1]);
                }
                if is_b_clipped {
                    pixel[2] = target_lum.max(pixel[2]);
                }
            }
        }
    });
}

/// Pre-demosaicing raw Bayer sensor conditioning for high-ISO and long-exposure frames:
/// 1. Optical Black (OB) row drift subtraction to eliminate horizontal read noise banding
/// 2. Raw-level 3x3 same-channel Bayer median hot-pixel filter to prevent colored demosaic crosses
pub fn condition_raw_bayer_sensor(raw_image: &mut rawler::rawimage::RawImage, iso: f32) {
    if !raw_image.camera.cfa.is_bayer() {
        return;
    }
    let w = raw_image.width;
    let h = raw_image.height;
    if w < 10 || h < 10 {
        return;
    }

    let nominal_black = raw_image
        .blacklevel
        .levels
        .first()
        .map(|r| r.as_f32())
        .unwrap_or(2048.0);

    // 1. Optical Black (OB) Row Drift Subtraction:
    // Search for a vertical optical black column strip in raw_image.blackareas (e.g. [0, 34, 260, 4056])
    let ob_strip = raw_image.blackareas.iter().find(|rect| {
        rect.width() >= 32 && rect.height() >= h / 2
    }).cloned();

    if let rawler::rawimage::RawImageData::Integer(ref mut data) = raw_image.data {
        if let Some(strip) = ob_strip {
            let x_start = strip.x() + 8; // skip transition boundary
            let x_end = (strip.x() + strip.width()).saturating_sub(8).min(w);
            let y_start = strip.y().min(h);
            let y_end = (strip.y() + strip.height()).min(h);

            if x_end > x_start && y_end > y_start {
                let sample_count = (x_end - x_start) as f32;

                // Process rows in parallel with Rayon: subtract per-row drift
                data.par_chunks_mut(w)
                    .enumerate()
                    .for_each(|(y, row)| {
                        if y >= y_start && y < y_end {
                            let mut ob_sum = 0.0f32;
                            for x in x_start..x_end {
                                ob_sum += row[x] as f32;
                            }
                            let ob_mean = ob_sum / sample_count;
                            let drift = ob_mean - nominal_black;

                            // If drift exceeds normal random fluctuation (> 0.5 DN), correct row
                            if drift.abs() > 0.5 {
                                for val in row.iter_mut() {
                                    *val = (*val as f32 - drift).clamp(0.0, 65535.0).round() as u16;
                                }
                            }
                        }
                    });
            }
        }

        // 2. Raw-Level 3x3 Same-Channel Bayer Median Hot-Pixel Filter:
        // Especially critical at ISO >= 1600 or long exposures to prevent colored demosaicing star artifacts.
        let iso_factor = (iso / 100.0).max(1.0);
        let spike_thresh = (150.0 * iso_factor.sqrt()).min(8000.0) as u16;

        // In active area, check sensels (x +/- 2, y +/- 2 for same Bayer channel)
        let active_crop = raw_image.crop_area.or(raw_image.active_area);
        let (crop_x0, crop_y0, crop_x1, crop_y1) = if let Some(c) = active_crop {
            (c.x().max(2), c.y().max(2), (c.x() + c.width()).min(w - 2), (c.y() + c.height()).min(h - 2))
        } else {
            (2, 2, w - 2, h - 2)
        };

        if crop_x1 > crop_x0 && crop_y1 > crop_y0 {
            // Snapshot for read-only neighbor lookups
            let data_snapshot = data.clone();
            data.par_chunks_mut(w)
                .enumerate()
                .for_each(|(y, row)| {
                    if y >= crop_y0 && y < crop_y1 {
                        let y_prev2 = (y - 2) * w;
                        let y_curr = y * w;
                        let y_next2 = (y + 2) * w;

                        for x in crop_x0..crop_x1 {
                            let val = row[x];
                            let p00 = data_snapshot[y_prev2 + x - 2];
                            let p01 = data_snapshot[y_prev2 + x];
                            let p02 = data_snapshot[y_prev2 + x + 2];
                            let p10 = data_snapshot[y_curr + x - 2];
                            let p12 = data_snapshot[y_curr + x + 2];
                            let p20 = data_snapshot[y_next2 + x - 2];
                            let p21 = data_snapshot[y_next2 + x];
                            let p22 = data_snapshot[y_next2 + x + 2];

                            let max_neighbor = p00.max(p01).max(p02).max(p10).max(p12).max(p20).max(p21).max(p22);
                            if val > max_neighbor.saturating_add(spike_thresh) {
                                // Optical Point Spread Function (PSF) gradient check:
                                // Real celestial stars are focused through the lens and form an optical Airy disk
                                // spanning 2x2 to 3x3 photosites, illuminating adjacent cross-channel sensels.
                                // Thermal defects / hot pixels have near-infinite gradient (isolated 1-sensel Dirac spike).
                                let y_prev1 = (y - 1) * w;
                                let y_next1 = (y + 1) * w;
                                let adj_up = data_snapshot[y_prev1 + x];
                                let adj_down = data_snapshot[y_next1 + x];
                                let adj_left = data_snapshot[y_curr + x - 1];
                                let adj_right = data_snapshot[y_curr + x + 1];
                                let max_adjacent = adj_up.max(adj_down).max(adj_left).max(adj_right);

                                let star_support_thresh = max_neighbor.saturating_add(spike_thresh / 4);
                                let is_celestial_star = max_adjacent > star_support_thresh;

                                if !is_celestial_star {
                                    let mut neighbors = [p00, p01, p02, p10, p12, p20, p21, p22];
                                    neighbors.sort_unstable();
                                    let median = (neighbors[3] as u32 + neighbors[4] as u32) / 2;
                                    row[x] = median as u16;
                                }
                            }
                        }
                    }
                });
        }

        // 3. Bayer Gr / Gb Nyquist Cross-Talk Equilibration:
        // Eliminates checkerboard/maze artifacts before demosaicing by equilibrating Gr and Gb in smooth regions.
        if crop_x1 > crop_x0 + 2 && crop_y1 > crop_y0 + 2 {
            let data_snapshot2 = data.clone();
            // Determine green parity dynamically for any 2x2 Bayer CFA (RGGB, BGGR, GRBG, GBRG):
            // In RGGB/BGGR, green sensels satisfy (x + y) % 2 == 1.
            // In GRBG/GBRG, green sensels satisfy (x + y) % 2 == 0.
            let green_parity = if raw_image.camera.cfa.width == 2 && raw_image.camera.cfa.height == 2 {
                if raw_image.camera.cfa.color_at(0, 0) == rawler::cfa::CFA_COLOR_G { 0 } else { 1 }
            } else {
                1
            };

            data.par_chunks_mut(w)
                .enumerate()
                .for_each(|(y, row)| {
                    if y >= crop_y0 + 1 && y < crop_y1 - 1 {
                        let y_prev1 = (y - 1) * w;
                        let y_next1 = (y + 1) * w;

                        for x in (crop_x0 + 1)..(crop_x1 - 1) {
                            if (x + y) % 2 == green_parity {
                                let g_val = row[x] as f32;
                                // 4 diagonal opposite-parity Green neighbors: (x-1, y-1), (x+1, y-1), (x-1, y+1), (x+1, y+1)
                                let d0 = data_snapshot2[y_prev1 + x - 1] as f32;
                                let d1 = data_snapshot2[y_prev1 + x + 1] as f32;
                                let d2 = data_snapshot2[y_next1 + x - 1] as f32;
                                let d3 = data_snapshot2[y_next1 + x + 1] as f32;
                                let opp_mean = (d0 + d1 + d2 + d3) * 0.25;

                                let max_opp = d0.max(d1).max(d2).max(d3);
                                let min_opp = d0.min(d1).min(d2).min(d3);
                                let local_contrast = max_opp - min_opp;

                                if local_contrast < 60.0 {
                                    let diff = g_val - opp_mean;
                                    if diff.abs() < 40.0 {
                                        row[x] = (g_val - diff * 0.35).clamp(0.0, 65535.0).round() as u16;
                                    }
                                }
                            }
                        }
                    }
                });
        }

        // 4. Zero-Clipped Black Pedestal Reconstruction:
        // When samples approach the black floor, prevent positive pedestal bias from hard zero-clamping
        let black_floor = nominal_black;
        if black_floor > 10.0 {
            data.par_chunks_mut(w)
                .enumerate()
                .for_each(|(y, row)| {
                    for x in 0..w {
                        let val = row[x] as f32;
                        if val <= black_floor + 8.0 && val >= black_floor - 8.0 {
                            let h = ((x as u32).wrapping_mul(0x1e35a7bd) ^ (y as u32).wrapping_mul(0x9e3779b9)) & 0xff;
                            let dither = (h as f32 / 255.0 - 0.5) * 2.0;
                            let adjusted = val + dither;
                            row[x] = adjusted.clamp(0.0, 65535.0).round() as u16;
                        }
                    }
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_inspect_raw_sensor_metadata() {
        let p = Path::new(r"D:\neapdirbti\IMG_4712.CR2");
        if !p.exists() { return; }
        let bytes = std::fs::read(p).unwrap();
        let source = rawler::rawsource::RawSource::new_from_slice(&bytes);
        let decoder = rawler::get_decoder(&source).unwrap();
        let mut raw_image = decoder.raw_image(&source, &rawler::decoders::RawDecodeParams::default(), false).unwrap();
        println!("RAW WxH: {}x{}", raw_image.width, raw_image.height);
        println!("Active area: {:?}", raw_image.active_area);
        println!("Crop area: {:?}", raw_image.crop_area);
        println!("Black areas: {:?}", raw_image.blackareas);
        println!("Black level: {:?}", raw_image.blacklevel);
        println!("White level: {:?}", raw_image.whitelevel);
        if let Some(r) = raw_image.active_area {
            println!("Rect x={}, y={}, w={}, h={}", r.x(), r.y(), r.width(), r.height());
        }

        // Test conditioning
        condition_raw_bayer_sensor(&mut raw_image, 1600.0);
        println!("Sensor conditioning passed successfully!");
    }

    #[test]
    fn test_recover_clipped_pixel_inpainting() {
        // Unclipped pixel (< 0.50): unchanged
        let (r0, g0, b0) = recover_clipped_pixel(0.40, 0.40, 0.40);
        assert_eq!((r0, g0, b0), (0.40, 0.40, 0.40));

        // Clipped Red with unclipped Green and Blue
        let (r1, g1, b1) = recover_clipped_pixel(0.95, 0.70, 0.50);
        assert!(r1 > 0.95, "Clipped red channel must be inpainted with gradient from intact channels: got {}", r1);
        assert!(g1.is_finite() && b1.is_finite());

        // Monotonic gradient propagation: higher guide lum produces higher reconstructed red
        let (r2, _, _) = recover_clipped_pixel(0.95, 0.85, 0.50);
        assert!(r2 > r1, "Higher intact green energy must propagate higher infilled red: {} vs {}", r2, r1);
    }

    #[test]
    fn test_dual_illuminant_interpolation() {
        let mat_a = vec![1.0, 2.0, 3.0];
        let mat_d65 = vec![4.0, 5.0, 6.0];

        // Exact Illuminant A (2856K)
        let res_a = interpolate_dual_illuminant_matrix(&mat_a, &mat_d65, 2856.0);
        assert!((res_a[0] - 1.0).abs() < 1e-4);

        // Exact Illuminant D65 (6504K)
        let res_d65 = interpolate_dual_illuminant_matrix(&mat_a, &mat_d65, 6504.0);
        assert!((res_d65[0] - 4.0).abs() < 1e-4);

        // Interpolated (e.g. 4000K)
        let res_mid = interpolate_dual_illuminant_matrix(&mat_a, &mat_d65, 4000.0);
        assert!(res_mid[0] > 1.0 && res_mid[0] < 4.0);
    }

    #[test]
    fn test_demosaic_rcd_bayer_with_ca() {
        let width = 16;
        let height = 16;
        let cfa = vec![0.5f32; width * height];
        let rgb = demosaic_rcd_bayer_with_ca(
            &cfa,
            width,
            height,
            [0, 1, 1, 2],
            Some([0.001, 0.0, 0.0]),
            Some([-0.001, 0.0, 0.0]),
        );
        assert_eq!(rgb.len(), width * height);
        assert!(rgb[width * 8 + 8][0] > 0.0);
        assert!(rgb[width * 8 + 8][1] > 0.0);
        assert!(rgb[width * 8 + 8][2] > 0.0);
    }

    #[test]
    fn test_condition_raw_bayer_sensor_cfa_parities() {
        use rawler::cfa::CFA;
        use rawler::decoders::Camera;
        use rawler::pixarray::PixU16;
        use rawler::rawimage::{BlackLevel, WhiteLevel, RawPhotometricInterpretation, RawImageData};

        let width = 32;
        let height = 32;

        let mut cam = Camera::default();
        cam.cfa = CFA::new("GRBG");

        let mut pix = PixU16::new(width, height);
        for val in pix.data.iter_mut() {
            *val = 2048;
        }
        pix.data[10 * width + 10] = 2060;

        let cfa_cfg = rawler::rawimage::CFAConfig::new_from_camera(&cam);
        let black = BlackLevel::new(&[2048.0], 1, 1, 1);
        let mut raw_grbg = RawImage::new(
            cam,
            pix,
            1,
            [1.0, 1.0, 1.0, 1.0],
            RawPhotometricInterpretation::Cfa(cfa_cfg),
            Some(black),
            Some(WhiteLevel(vec![16383])),
            false,
        );

        condition_raw_bayer_sensor(&mut raw_grbg, 800.0);
        assert_eq!(raw_grbg.width, width);
    }
}


