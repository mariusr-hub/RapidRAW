use crate::decode::as_shot::AsShot;
use crate::decode::demosaic::{Mosaic, white_level};
use crate::decode::develop::{self, Camera, Finish};
use crate::decode::hot_pixels::repair_hot_pixels;
use crate::decode::white_balance::WhiteBalance;
use crate::render::color_space::{GamutCompression, fit_gamut, gamut_limits, srgb_to_working};
use crate::render::geometry::apply_orientation;
use anyhow::{Result, anyhow};
use glam::Vec3;
use image::{DynamicImage, ImageBuffer, Rgb};
use rawler::{
    decoders::{Decoder, Orientation, RawDecodeParams},
    imgop::{
        Dim2, Point, Rect,
        develop::{DemosaicAlgorithm, Intermediate, ProcessingStep, RawDevelop},
        xyz::{FlatColorMatrix, Illuminant},
    },
    pixarray::Color2D,
    rawimage::{RawImage, RawImageData, RawPhotometricInterpretation},
    rawsource::RawSource,
};
use rayon::prelude::*;
use std::borrow::Cow;
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn metadata_orientation(decoder: &dyn Decoder, source: &RawSource) -> Result<Orientation> {
    let metadata = decoder.raw_metadata(source, &RawDecodeParams::default())?;
    Ok(metadata
        .exif
        .orientation
        .map(Orientation::from_u16)
        .unwrap_or(Orientation::Normal))
}

const HIGHLIGHT_HEADROOM: f32 = 1000.0;

thread_local! {
    static READING_PREFIX: Cell<bool> = const { Cell::new(false) };
    static PREFIX_WARNED: Cell<bool> = const { Cell::new(false) };
}

pub fn is_reading_prefix() -> bool {
    READING_PREFIX.with(Cell::get)
}

pub fn swallow_prefix_log(metadata: &log::Metadata) -> bool {
    if !metadata.target().starts_with("rawler") || !is_reading_prefix() {
        return false;
    }
    if metadata.level() <= log::Level::Warn {
        PREFIX_WARNED.with(|warned| warned.set(true));
    }
    true
}

pub fn read_from_prefix<T>(
    file_bytes: &[u8],
    read: impl Fn(&[u8], bool) -> Option<T>,
) -> Option<T> {
    let mut len = 256 * 1024;
    while len < file_bytes.len() && len <= 16 * 1024 * 1024 {
        READING_PREFIX.with(|reading| reading.set(true));
        PREFIX_WARNED.with(|warned| warned.set(false));
        let found = catch_unwind(AssertUnwindSafe(|| read(&file_bytes[..len], false)));
        READING_PREFIX.with(|reading| reading.set(false));
        if let Ok(Some(found)) = found
            && !PREFIX_WARNED.with(Cell::get)
        {
            return Some(found);
        }
        len *= 4;
    }
    read(file_bytes, true)
}

pub fn extract_embedded_preview(file_bytes: &[u8]) -> Option<DynamicImage> {
    read_from_prefix(file_bytes, |bytes, whole_file| {
        let source = RawSource::new_from_slice(bytes);
        let decoder = rawler::get_decoder(&source).ok()?;
        let orientation = match metadata_orientation(decoder.as_ref(), &source) {
            Ok(orientation) => orientation,
            Err(_) if whole_file => Orientation::Normal,
            Err(_) => return None,
        };
        let preview = decoder
            .full_image(&source, &RawDecodeParams::default())
            .ok()??;
        Some(apply_orientation(preview, orientation))
    })
}

fn is_linear_raw_format(raw_image: &RawImage) -> bool {
    matches!(
        raw_image.photometric,
        RawPhotometricInterpretation::LinearRaw
    )
}

#[inline]
fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(3.0)
    }
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
fn recover_clipped_pixel(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max_c = r.max(g).max(b);

    if max_c <= 0.50 {
        return (r, g, b);
    }

    let mut cur_r = r;
    let mut cur_g = g;
    let mut cur_b = b;

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
    let min_c = cur_r.min(cur_g).min(cur_b);

    let knee = smoothstep(0.50, 1.5, new_max);

    if knee > 0.0 {
        let neutrality = (min_c / new_max.max(1e-5)).clamp(0.0, 1.0);

        let core_burn = smoothstep(0.60, 3.0, new_max);

        let desat = (knee * (neutrality * 0.85 + core_burn * 0.15)).clamp(0.0, 1.0);
        let smooth_desat = desat * desat * (3.0 - 2.0 * desat);

        let neutral_value = min_c + (new_max - min_c) * 1.0;

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

struct CameraToWorking {
    matrix: [[f32; 4]; 3],
    compression: [Option<GamutCompression>; 3],
}

impl CameraToWorking {
    fn new(raw_image: &RawImage) -> Option<Self> {
        let color_matrix = as_shot_color(raw_image)
            .map(|(_, color_matrix)| color_matrix)
            .or_else(|| primary_color_matrix(raw_image).cloned())?;
        if color_matrix.len() % 3 != 0 || color_matrix.len() > 12 {
            return None;
        }
        let primaries = crate::decode::white_balance::camera_to_working(
            &color_matrix,
            &camera_neutral(raw_image),
        )?;

        let mut matrix = [[0.0f32; 4]; 3];
        for (channel, primary) in primaries.iter().enumerate() {
            for (row, value) in matrix.iter_mut().zip(primary) {
                row[channel] = *value;
            }
        }

        Some(Self {
            matrix,
            compression: gamut_limits(&primaries).map(GamutCompression::new),
        })
    }

    #[inline]
    fn pixel<const N: usize>(&self, camera: &[f32; N]) -> [f32; 3] {
        let rgb = self
            .matrix
            .map(|row| row.iter().zip(camera).map(|(m, c)| m * c).sum::<f32>());
        fit_gamut(rgb, &self.compression)
    }

    fn camera(&self, wb: [f32; 3], clamp_limit: f32) -> Camera {
        Camera {
            wb,
            matrix: self.matrix.map(|row| [row[0], row[1], row[2]]),
            compression: self.compression,
            clamp_limit,
        }
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

#[allow(clippy::too_many_arguments)]
pub fn develop_raw_image(
    file_bytes: &[u8],
    fast_demosaic: bool,
    _highlight_compression: f32,
    linear_mode: String,
    denoise: f32,
    sharpen: f32,
    remove_hot_pixels: bool,
    cancel_token: Option<(Arc<AtomicUsize>, usize)>,
) -> Result<DynamicImage> {
    let check_cancel = || -> Result<()> {
        if let Some((tracker, generation)) = &cancel_token
            && tracker.load(Ordering::SeqCst) != *generation
        {
            return Err(anyhow!("Load cancelled"));
        }
        Ok(())
    };

    check_cancel()?;

    let source = RawSource::new_from_slice(file_bytes);
    let decoder = rawler::get_decoder(&source)?;

    check_cancel()?;
    let mut raw_image: RawImage = decoder.raw_image(&source, &RawDecodeParams::default(), false)?;
    if remove_hot_pixels {
        check_cancel()?;
        repair_hot_pixels(&mut raw_image);
    }

    let orientation = metadata_orientation(decoder.as_ref(), &source)?;

    let is_linear_format = is_linear_raw_format(&raw_image);
    let is_floating_point = matches!(raw_image.data, RawImageData::Float(_));

    let is_hdr_linear = linear_mode == "hdr_linear";
    let (apply_ungamma, apply_calibration) = match linear_mode.as_str() {
        "gamma" => (true, true),
        "skip_calib" => (false, false),
        "gamma_skip_calib" => (true, false),
        _ => (false, true),
    };

    let original_white_level = white_level(&raw_image);
    let original_black_level = raw_image
        .blacklevel
        .levels
        .first()
        .map(|r| r.as_f32())
        .unwrap_or(0.0);

    raw_image.wb_coeffs = crate::decode::multi_exposure::neutralize_wb_if_multiexposure(
        raw_image.wb_coeffs,
        file_bytes,
    );

    let camera_to_working = (apply_calibration || !is_linear_format)
        .then(|| CameraToWorking::new(&raw_image))
        .flatten();
    let cfa_balance = (!is_linear_format && camera_to_working.is_some())
        .then(|| cfa_white_balance(&raw_image))
        .flatten();
    let (clamp_limit, denoise, sharpen) = if fast_demosaic {
        (1.0, 0.0, 0.0)
    } else {
        (HIGHLIGHT_HEADROOM, denoise, sharpen)
    };
    let finish = |camera: Option<Camera>| Finish {
        camera,
        orientation,
        denoise,
        sharpen,
        recover_highlights: !is_hdr_linear,
    };

    if let (Some(wb), Some(camera_to_working)) = (cfa_balance, &camera_to_working)
        && let Some(mosaic) = Mosaic::new(&raw_image, wb)
    {
        check_cancel()?;
        let finish = finish(Some(camera_to_working.camera(wb, clamp_limit)));
        let crop = |dim| {
            default_crop(
                raw_image.crop_area,
                raw_image.active_area,
                raw_image.dim(),
                dim,
            )
        };
        let image = if fast_demosaic {
            let (data, dim) = mosaic.bin();
            check_cancel()?;
            let crop = crop(dim);
            develop::finish_rgb(
                &crop_rows(&data, dim.w, crop),
                crop.d.w as u32,
                crop.d.h as u32,
                &finish,
            )
        } else {
            let domain = mosaic.domain();
            let crop = crop(domain.d);
            let crop = Rect::new(
                Point::new(domain.p.x + crop.p.x, domain.p.y + crop.p.y),
                crop.d,
            );
            mosaic.develop(crop, &finish, check_cancel)?
        };
        return Ok(DynamicImage::ImageRgb32F(image));
    }

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

    if cfa_balance.is_some() {
        developer
            .steps
            .retain(|&step| step != ProcessingStep::CropDefault);
    }

    check_cancel()?;
    let mut developed_intermediate = developer.develop_intermediate(&raw_image)?;

    let (crop_area, active_area, raw_dim) =
        (raw_image.crop_area, raw_image.active_area, raw_image.dim());
    drop(raw_image);

    if cfa_balance.is_none()
        && let Some(camera_to_working) = &camera_to_working
    {
        check_cancel()?;
        developed_intermediate = camera_to_working.apply(developed_intermediate);
    }

    let denominator = (original_white_level - original_black_level).max(1.0);
    let rescale_factor = (u32::MAX as f32 - original_black_level) / denominator;

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
        Intermediate::ThreeColor(pixels) if cfa_balance.is_some() => {
            let wb = cfa_balance.unwrap();
            let camera = camera_to_working.as_ref().unwrap().camera(wb, clamp_limit);
            let crop = default_crop(crop_area, active_area, raw_dim, pixels.dim());
            let mut data = crop_rows(&pixels.data, pixels.width, crop).into_owned();
            data.par_iter_mut().for_each(|p| {
                for c in 0..3 {
                    p[c] = (p[c] * rescale_factor).max(0.0) * wb[c];
                }
            });
            check_cancel()?;

            let image = develop::finish_rgb(
                &data,
                crop.d.w as u32,
                crop.d.h as u32,
                &finish(Some(camera)),
            );
            return Ok(DynamicImage::ImageRgb32F(image));
        }
        Intermediate::ThreeColor(pixels) => {
            let uncalibrated_to_working = camera_to_working.is_none().then(srgb_to_working);
            pixels.data.par_iter_mut().for_each(|p| {
                let mut r = (p[0] * rescale_factor).max(0.0);
                let mut g = (p[1] * rescale_factor).max(0.0);
                let mut b = (p[2] * rescale_factor).max(0.0);

                if is_linear_format && apply_ungamma {
                    r = srgb_to_linear(r.max(0.0));
                    g = srgb_to_linear(g.max(0.0));
                    b = srgb_to_linear(b.max(0.0));
                }

                let (rec_r, rec_g, rec_b) = if is_floating_point || is_hdr_linear {
                    (r, g, b)
                } else {
                    recover_clipped_pixel(r, g, b)
                };
                let recovered = Vec3::new(rec_r, rec_g, rec_b);
                let rgb = uncalibrated_to_working.map_or(recovered, |matrix| matrix * recovered);

                *p = rgb.clamp(Vec3::ZERO, Vec3::splat(clamp_limit)).to_array();
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
    }

    check_cancel()?;

    let dynamic_image = match developed_intermediate {
        Intermediate::ThreeColor(pixels) => {
            DynamicImage::ImageRgb32F(rgb_pixels_to_rgb(pixels.data, width, height)?)
        }
        Intermediate::Monochrome(pixels) => {
            DynamicImage::ImageRgb32F(mono_pixels_to_rgb(&pixels.data, width, height)?)
        }
        _ => {
            return Err(anyhow!("Unsupported intermediate format for conversion"));
        }
    };

    Ok(develop::enhance(
        apply_orientation(dynamic_image, orientation),
        denoise,
        sharpen,
    ))
}

fn crop_rows(data: &[[f32; 3]], stride: usize, crop: Rect) -> Cow<'_, [[f32; 3]]> {
    let rows = &data[crop.p.y * stride..(crop.p.y + crop.d.h) * stride];
    if crop.d.w == stride {
        return Cow::Borrowed(rows);
    }
    Cow::Owned(
        rows.par_chunks_exact(stride)
            .flat_map_iter(|row| row[crop.p.x..crop.p.x + crop.d.w].iter().copied())
            .collect(),
    )
}

fn default_crop(
    crop_area: Option<Rect>,
    active_area: Option<Rect>,
    raw_dim: Dim2,
    dim: Dim2,
) -> Rect {
    let full = Rect::new(Point::zero(), dim);
    let Some(mut crop) = crop_area else {
        return full;
    };
    if let Some(active_area) = active_area {
        crop = crop.intersection(&active_area).adapt(&active_area);
    }
    let original_width = active_area.map(|area| area.d.w).unwrap_or(raw_dim.w);
    if original_width > 0 {
        let scale_factor = dim.w as f32 / original_width as f32;
        if (scale_factor - 1.0).abs() > 1e-6 {
            crop.scale(scale_factor);
        }
    }
    let in_bounds = crop.p.x + crop.d.w <= dim.w && crop.p.y + crop.d.h <= dim.h;
    if crop.is_empty() || crop.d == dim || !in_bounds {
        full
    } else {
        crop
    }
}

fn cfa_white_balance(raw_image: &RawImage) -> Option<[f32; 3]> {
    let RawPhotometricInterpretation::Cfa(config) = &raw_image.photometric else {
        return None;
    };
    if raw_image.cpp != 1 || !config.cfa.is_rgb() || config.cfa.unique_colors() != 3 {
        return None;
    }
    if raw_image.wb_coeffs[0].is_nan() {
        Some([1.0; 3])
    } else {
        Some([
            raw_image.wb_coeffs[0],
            raw_image.wb_coeffs[1],
            raw_image.wb_coeffs[2],
        ])
    }
}

#[derive(Clone)]
pub(crate) struct LinearBayerFrame {
    pub data: Vec<f32>,
    pub width: usize,
    pub height: usize,
    pub cfa: [[u8; 2]; 2],
    pub wb: [f32; 3],
    pub cam_to_working: [[f32; 3]; 3],
    pub orientation: Orientation,
}

pub(crate) fn extract_linear_bayer_cfa(file_bytes: &[u8]) -> Result<LinearBayerFrame> {
    let source = RawSource::new_from_slice(file_bytes);
    let decoder = rawler::get_decoder(&source)?;
    let orientation = metadata_orientation(decoder.as_ref(), &source).unwrap_or(Orientation::Normal);
    let mut raw_image: RawImage = decoder.raw_image(&source, &RawDecodeParams::default(), false)?;

    raw_image.wb_coeffs = crate::decode::multi_exposure::neutralize_wb_if_multiexposure(
        raw_image.wb_coeffs,
        file_bytes,
    );

    let RawPhotometricInterpretation::Cfa(config) = &raw_image.photometric else {
        return Err(anyhow!("RAW file does not contain a mosaic CFA"));
    };
    if raw_image.cpp != 1 || config.cfa.width != 2 || config.cfa.height != 2 || !config.cfa.is_rgb() {
        return Err(anyhow!("CFA layout is not a 2x2 RGB Bayer pattern"));
    }
    let RawImageData::Integer(raw_u16) = &raw_image.data else {
        return Err(anyhow!("Expected integer raw CFA samples"));
    };
    if raw_u16.len() < raw_image.width * raw_image.height {
        return Err(anyhow!("Truncated RAW CFA buffer"));
    }

    let domain = crate::decode::demosaic::active_domain(&raw_image, 64)
        .ok_or_else(|| anyhow!("Invalid active RAW domain"))?;

    // Preserve Bayer phase parity by aligning origin and dimensions to even boundaries.
    let x0 = domain.p.x + (domain.p.x & 1);
    let y0 = domain.p.y + (domain.p.y & 1);
    let width = (domain.p.x + domain.d.w).saturating_sub(x0) & !1;
    let height = (domain.p.y + domain.d.h).saturating_sub(y0) & !1;
    if width < 64 || height < 64 {
        return Err(anyhow!("Active Bayer region too small"));
    }

    let cfa: [[u8; 2]; 2] =
        std::array::from_fn(|y| std::array::from_fn(|x| config.cfa.color_at(y, x) as u8));
    let black = raw_image.blacklevel.as_bayer_array();
    let white = white_level(&raw_image);
    let wb = cfa_white_balance(&raw_image).unwrap_or([1.0; 3]);

    let cam_to_working = CameraToWorking::new(&raw_image)
        .map(|c| {
            [
                [c.matrix[0][0], c.matrix[0][1], c.matrix[0][2]],
                [c.matrix[1][0], c.matrix[1][1], c.matrix[1][2]],
                [c.matrix[2][0], c.matrix[2][1], c.matrix[2][2]],
            ]
        })
        .unwrap_or([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);

    let mut data = vec![0.0f32; width * height];
    let stride = raw_image.width;
    data.par_chunks_exact_mut(width)
        .enumerate()
        .for_each(|(dy, row)| {
            let sy = y0 + dy;
            let src_row = &raw_u16[sy * stride + x0..sy * stride + x0 + width];
            let b0 = black[(dy & 1) * 2];
            let b1 = black[(dy & 1) * 2 + 1];
            let s0 = 1.0 / (white - b0).max(1.0);
            let s1 = 1.0 / (white - b1).max(1.0);
            for (dx, dst) in row.iter_mut().enumerate() {
                let (b, s) = if (dx & 1) == 0 { (b0, s0) } else { (b1, s1) };
                *dst = ((src_row[dx] as f32 - b) * s).max(0.0);
            }
        });

    Ok(LinearBayerFrame {
        data,
        width,
        height,
        cfa,
        wb,
        cam_to_working,
        orientation,
    })
}

pub fn read_as_shot(file_bytes: &[u8]) -> Option<AsShot> {
    let source = RawSource::new_from_slice(file_bytes);
    let decoder = rawler::get_decoder(&source).ok()?;
    let mut raw_image = decoder
        .raw_image(&source, &RawDecodeParams::default(), true)
        .ok()?;
    raw_image.wb_coeffs = crate::decode::multi_exposure::neutralize_wb_if_multiexposure(
        raw_image.wb_coeffs,
        file_bytes,
    );
    let is_monochrome = raw_image.cpp == 1
        && !matches!(raw_image.photometric, RawPhotometricInterpretation::Cfa(_));

    Some(AsShot {
        white_balance: (!is_monochrome)
            .then(|| as_shot_color(&raw_image))
            .flatten()
            .map(|(white_balance, _)| white_balance)
            .unwrap_or_else(WhiteBalance::reference),
        exposure: crate::decode::baseline_exposure::read(
            decoder.as_ref(),
            file_bytes,
            is_monochrome,
        ),
        source_peak: 1.0,
    })
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

fn rgb_pixels_to_rgb(
    data: Vec<[f32; 3]>,
    width: u32,
    height: u32,
) -> Result<ImageBuffer<Rgb<f32>, Vec<f32>>> {
    ImageBuffer::from_raw(width, height, data.into_flattened())
        .ok_or_else(|| anyhow!("Developed image size does not match its dimensions"))
}

fn mono_pixels_to_rgb(
    data: &[f32],
    width: u32,
    height: u32,
) -> Result<ImageBuffer<Rgb<f32>, Vec<f32>>> {
    let mut rgb = vec![0.0f32; data.len() * 3];
    rgb.par_chunks_exact_mut(3)
        .zip(data.par_iter())
        .for_each(|(dst, &p)| dst.fill(p));
    ImageBuffer::from_raw(width, height, rgb)
        .ok_or_else(|| anyhow!("Developed image size does not match its dimensions"))
}

pub fn raw_dimensions(file_bytes: &[u8]) -> Option<(usize, usize)> {
    let source = RawSource::new_from_slice(file_bytes);
    let decoder = rawler::get_decoder(&source).ok()?;
    let raw_img = decoder
        .raw_image(&source, &RawDecodeParams::default(), true)
        .ok()?;
    Some((raw_img.width, raw_img.height))
}

pub fn get_fast_demosaic_scale_factor(
    file_bytes: &[u8],
    decoded_width: u32,
    decoded_height: u32,
) -> f32 {
    if let Some((raw_width, raw_height)) = raw_dimensions(file_bytes) {
        let max_orig = (raw_width as f32).max(raw_height as f32);
        let max_comp = (decoded_width as f32).max(decoded_height as f32);
        if max_orig > 0.0 {
            let ratio = max_comp / max_orig;
            if ratio > 0.1 && ratio < 0.29 {
                return 0.25;
            } else if (0.29..0.4).contains(&ratio) {
                return 1.0 / 3.0;
            } else if (0.4..0.75).contains(&ratio) {
                return 0.5;
            }
        }
    }
    1.0
}

/// Ratio-Corrected Demosaicing (RCD) for Bayer CFA sensels.
pub fn demosaic_rcd_bayer(
    cfa: &[f32],
    width: usize,
    height: usize,
    cfa_pattern: [u8; 4],
) -> Vec<[f32; 3]> {
    demosaic_rcd_bayer_with_ca(cfa, width, height, cfa_pattern, None, None)
}

/// Ratio-Corrected Demosaicing with pre-demosaic sub-lattice lateral CA warping.
pub fn demosaic_rcd_bayer_with_ca(
    cfa: &[f32],
    width: usize,
    height: usize,
    cfa_pattern: [u8; 4],
    ca_red_coeffs: Option<[f32; 3]>,
    ca_blue_coeffs: Option<[f32; 3]>,
) -> Vec<[f32; 3]> {
    let mut rgb = vec![[0.0f32; 3]; width * height];
    if width < 5 || height < 5 {
        return rgb;
    }
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

    for y in 2..(height - 2) {
        for x in 2..(width - 2) {
            let idx = y * width + x;
            let c = cfa[idx];
            let p_type = cfa_pattern[(y % 2) * 2 + (x % 2)];

            if p_type == 1 || p_type == 2 {
                rgb[idx][1] = c;
            } else {
                let h_diff = (cfa[y * width + (x - 1)] - cfa[y * width + (x + 1)]).abs()
                    + (cfa[y * width + (x - 2)] - cfa[y * width + (x + 2)]).abs() * 0.5;
                let v_diff = (cfa[(y - 1) * width + x] - cfa[(y + 1) * width + x]).abs()
                    + (cfa[(y - 2) * width + x] - cfa[(y + 2) * width + x]).abs() * 0.5;

                if h_diff < v_diff * 0.8 {
                    rgb[idx][1] = 0.5 * (cfa[y * width + (x - 1)] + cfa[y * width + (x + 1)]);
                } else if v_diff < h_diff * 0.8 {
                    rgb[idx][1] = 0.5 * (cfa[(y - 1) * width + x] + cfa[(y + 1) * width + x]);
                } else {
                    rgb[idx][1] = 0.25
                        * (cfa[y * width + (x - 1)]
                            + cfa[y * width + (x + 1)]
                            + cfa[(y - 1) * width + x]
                            + cfa[(y + 1) * width + x]);
                }
            }
        }
    }

    for y in 2..(height - 2) {
        for x in 2..(width - 2) {
            let idx = y * width + x;
            let g = rgb[idx][1].max(1e-5);
            let p_type = cfa_pattern[(y % 2) * 2 + (x % 2)];

            let (r_ox, r_oy) = ca_red_coeffs.map(|c| warp_offset(x, y, c)).unwrap_or((0.0, 0.0));
            let (b_ox, b_oy) = ca_blue_coeffs.map(|c| warp_offset(x, y, c)).unwrap_or((0.0, 0.0));

            if p_type == 0 {
                rgb[idx][0] = cfa[idx];
                let b_ratio = 0.25
                    * (sample_sublattice_bilinear(x as i32 - 1, y as i32 - 1, b_ox, b_oy)
                        / rgb[(y - 1) * width + (x - 1)][1].max(1e-5)
                        + sample_sublattice_bilinear(x as i32 + 1, y as i32 - 1, b_ox, b_oy)
                            / rgb[(y - 1) * width + (x + 1)][1].max(1e-5)
                        + sample_sublattice_bilinear(x as i32 - 1, y as i32 + 1, b_ox, b_oy)
                            / rgb[(y + 1) * width + (x - 1)][1].max(1e-5)
                        + sample_sublattice_bilinear(x as i32 + 1, y as i32 + 1, b_ox, b_oy)
                            / rgb[(y + 1) * width + (x + 1)][1].max(1e-5));
                rgb[idx][2] = (b_ratio * g).max(0.0);
            } else if p_type == 3 {
                rgb[idx][2] = cfa[idx];
                let r_ratio = 0.25
                    * (sample_sublattice_bilinear(x as i32 - 1, y as i32 - 1, r_ox, r_oy)
                        / rgb[(y - 1) * width + (x - 1)][1].max(1e-5)
                        + sample_sublattice_bilinear(x as i32 + 1, y as i32 - 1, r_ox, r_oy)
                            / rgb[(y - 1) * width + (x + 1)][1].max(1e-5)
                        + sample_sublattice_bilinear(x as i32 - 1, y as i32 + 1, r_ox, r_oy)
                            / rgb[(y + 1) * width + (x - 1)][1].max(1e-5)
                        + sample_sublattice_bilinear(x as i32 + 1, y as i32 + 1, r_ox, r_oy)
                            / rgb[(y + 1) * width + (x + 1)][1].max(1e-5));
                rgb[idx][0] = (r_ratio * g).max(0.0);
            } else if y % 2 == 0 && x % 2 == 1 {
                let r_ratio = 0.5
                    * (sample_sublattice_bilinear(x as i32 - 1, y as i32, r_ox, r_oy)
                        / rgb[y * width + (x - 1)][1].max(1e-5)
                        + sample_sublattice_bilinear(x as i32 + 1, y as i32, r_ox, r_oy)
                            / rgb[y * width + (x + 1)][1].max(1e-5));
                let b_ratio = 0.5
                    * (sample_sublattice_bilinear(x as i32, y as i32 - 1, b_ox, b_oy)
                        / rgb[(y - 1) * width + x][1].max(1e-5)
                        + sample_sublattice_bilinear(x as i32, y as i32 + 1, b_ox, b_oy)
                            / rgb[(y + 1) * width + x][1].max(1e-5));
                rgb[idx][0] = (r_ratio * g).max(0.0);
                rgb[idx][2] = (b_ratio * g).max(0.0);
            } else {
                let b_ratio = 0.5
                    * (sample_sublattice_bilinear(x as i32 - 1, y as i32, b_ox, b_oy)
                        / rgb[y * width + (x - 1)][1].max(1e-5)
                        + sample_sublattice_bilinear(x as i32 + 1, y as i32, b_ox, b_oy)
                            / rgb[y * width + (x + 1)][1].max(1e-5));
                let r_ratio = 0.5
                    * (sample_sublattice_bilinear(x as i32, y as i32 - 1, r_ox, r_oy)
                        / rgb[(y - 1) * width + x][1].max(1e-5)
                        + sample_sublattice_bilinear(x as i32, y as i32 + 1, r_ox, r_oy)
                            / rgb[(y + 1) * width + x][1].max(1e-5));
                rgb[idx][0] = (r_ratio * g).max(0.0);
                rgb[idx][2] = (b_ratio * g).max(0.0);
            }
        }
    }

    rgb
}

pub trait CfaExt {
    fn is_bayer(&self) -> bool;
}

impl CfaExt for rawler::cfa::CFA {
    fn is_bayer(&self) -> bool {
        self.width == 2 && self.height == 2
    }
}

pub fn interpolate_dual_illuminant_matrix(
    matrix_a: &[f32],
    matrix_d65: &[f32],
    cct: f32,
) -> Vec<f32> {
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
