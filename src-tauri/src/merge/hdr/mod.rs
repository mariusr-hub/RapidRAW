mod exposure_fusion;
mod local_laplacian;
mod radiance;

use crate::decode::formats::is_raw_file;
use crate::enhanced::hdr_fusion::{DeghostSensitivity, HdrMergeOptions, HdrToneProfile};
use crate::enhanced::hdr_handheld::tone_map_virtual_exposure_pyramid;
use crate::merge::align::{
    AlignmentFeatures, downscaled, estimate_transform, max_displacement, warp,
};
use crate::merge::output::{
    MergeOutput, MergePreview, Rendition, compute_hdr_peak_nits, encode_hdr10_pq_png_base64,
    encode_hdr_headroom_heatmap_base64, encode_jpeg_base64,
};
use crate::merge::raster::{Raster, Rect, crop, crop_rects, fit_size, map_pixels, resize_area};
use crate::merge::source::{SourceLoader, ensure_same_size, file_name};
use crate::merge::{MergeContext, MergeSession, parse_settings};
use crate::metadata::exif_processing::{read_exposure_time_secs, read_f_number, read_iso, read_rrexif_sidecar};
use crate::render::color_space::{OutputTransform, fit_gamut, luminance, srgb_encode, working_to_srgb};
use exposure_fusion::ExposureFusion;
use glam::Vec3;
use image::Rgb32FImage;
use local_laplacian::{LocalLaplacianOptions, fast_local_laplacian_tonemap};
use nalgebra::Matrix3;
use radiance::{Accumulator, FrameRole, PreviewFrame, exposure_ratio, ghost_masks, ghost_masks_optical_flow};
use rayon::prelude::*;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Mutex;

const PREVIEW_MAX_DIMENSION: usize = 1600;
const DETECTION_MAX_DIMENSION: usize = 2000;
const IDENTITY_DISPLACEMENT: f64 = 0.3;
const CLIP_PERCENTILE: f64 = 0.999;
const MAX_RATIO_CORRECTION: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeghostMode {
    Fast,
    #[default]
    OpticalFlow,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HdrSettings {
    pub auto_crop: bool,
    pub half_size: bool,
    pub apply_edits: bool,
    pub profile: String,
    pub hdr_strength: f32,
    pub deghost_sensitivity: DeghostSensitivity,
    pub deghost_mode: DeghostMode,
    #[serde(default)]
    pub preview_mode: String,
    pub exposure_bias: f32,
    pub highlight_recovery: f32,
    pub shadow_lift: f32,
    pub detail_boost: f32,
}

impl Default for HdrSettings {
    fn default() -> Self {
        HdrSettings {
            auto_crop: true,
            half_size: false,
            apply_edits: true,
            profile: "natural".to_string(),
            hdr_strength: 100.0,
            deghost_sensitivity: DeghostSensitivity::Medium,
            deghost_mode: DeghostMode::OpticalFlow,
            preview_mode: "sdr".to_string(),
            exposure_bias: 0.0,
            highlight_recovery: 50.0,
            shadow_lift: 30.0,
            detail_boost: 1.05,
        }
    }
}

fn parse_tone_profile(profile: &str) -> Option<HdrToneProfile> {
    match profile.to_lowercase().as_str() {
        "agxscene" | "agx_scene" | "agx" => None,
        "interior" => Some(HdrToneProfile::Interior),
        "dramatic" => Some(HdrToneProfile::Dramatic),
        "vivid" => Some(HdrToneProfile::Vivid),
        _ => Some(HdrToneProfile::Natural),
    }
}

pub(crate) fn tonemap_hdr_raster(
    linear_working: &Raster,
    base_exposure: f32,
    profile: &str,
    hdr_strength: f32,
    exposure_bias: f32,
    highlight_recovery: f32,
    shadow_lift: f32,
    detail_boost: f32,
) -> (Raster, Rendition) {
    let use_local_laplacian = matches!(
        profile.to_lowercase().as_str(),
        "locallaplacian" | "local_laplacian" | "laplacian"
    );
    let tone_profile = if use_local_laplacian {
        HdrToneProfile::Natural
    } else {
        match parse_tone_profile(profile) {
            Some(p) => p,
            None => {
                return (
                    linear_working.clone(),
                    Rendition::Scene {
                        exposure: base_exposure + exposure_bias,
                        agx: true,
                    },
                );
            }
        }
    };

    let (w, h) = (linear_working.width, linear_working.height);
    let gain = base_exposure.exp2();
    let to_srgb = working_to_srgb();
    let srgb_out = OutputTransform::srgb(true);
    let srgb_lin_raw: Vec<f32> = linear_working
        .data
        .par_chunks_exact(3)
        .flat_map_iter(|p| {
            let clean = Vec3::from_array([
                if p[0].is_finite() { p[0].max(0.0) } else { 0.0 },
                if p[1].is_finite() { p[1].max(0.0) } else { 0.0 },
                if p[2].is_finite() { p[2].max(0.0) } else { 0.0 },
            ]);
            let v = fit_gamut((to_srgb * (clean * gain)).to_array(), &srgb_out.compression);
            [v[0].max(0.0), v[1].max(0.0), v[2].max(0.0)]
        })
        .collect();
    let srgb_lin_img = Rgb32FImage::from_raw(w as u32, h as u32, srgb_lin_raw)
        .expect("HDR sRGB buffer matches dimensions");

    let tm_img = if use_local_laplacian {
        let ll_opts = LocalLaplacianOptions {
            sigma_r: 0.16,
            alpha: (1.0 - (detail_boost - 1.0) * 0.65).clamp(0.35, 1.4),
            beta: 0.45,
            exposure_bias,
            shadow_lift,
            highlight_recovery,
        };
        fast_local_laplacian_tonemap(&srgb_lin_img, &ll_opts)
    } else {
        let opts = HdrMergeOptions {
            profile: Some(tone_profile),
            exposure_bias: Some(exposure_bias),
            highlight_recovery: Some(highlight_recovery),
            shadow_lift: Some(shadow_lift),
            detail_boost: Some(detail_boost),
            ..Default::default()
        };
        tone_map_virtual_exposure_pyramid(&srgb_lin_img, &opts)
    };
    let t = (hdr_strength / 100.0).clamp(0.0, 1.0);
    if t >= 0.999 {
        return (
            Raster::from_data(w, h, 3, tm_img.into_raw()),
            Rendition::SrgbDisplay,
        );
    }

    // Baseline scene-referred AgX sRGB rendition (for hdr_strength < 100%)
    let agx_tm = crate::render::agx::agx();
    let agx_out = OutputTransform::srgb(false);
    let total_gain = (base_exposure + exposure_bias).exp2();
    let mut blended = tm_img.into_raw();
    blended
        .par_chunks_exact_mut(3)
        .zip(linear_working.data.par_chunks_exact(3))
        .for_each(|(tm, lin)| {
            let clean = Vec3::from_array([
                if lin[0].is_finite() { lin[0].max(0.0) } else { 0.0 },
                if lin[1].is_finite() { lin[1].max(0.0) } else { 0.0 },
                if lin[2].is_finite() { lin[2].max(0.0) } else { 0.0 },
            ]);
            let neutral = agx_out.apply_srgb_encoded(agx_tm.tonemap(clean * total_gain).to_array());
            for c in 0..3 {
                tm[c] = (neutral[c] + (tm[c] - neutral[c]) * t).clamp(0.0, 1.0);
            }
        });

    (Raster::from_data(w, h, 3, blended), Rendition::SrgbDisplay)
}

struct Frame {
    path: String,
    exposure: f64,
    transform: Matrix3<f64>,
    preview: Raster,
    valid: Vec<f32>,
}

enum Merge {
    Radiance {
        clip: f32,
        masks: Vec<Option<Raster>>,
    },
    Fusion,
}

pub struct HdrSession {
    frames: Vec<Frame>,
    reference: usize,
    merge: Merge,
    rendition: Rendition,
    width: usize,
    height: usize,
    unaligned: usize,
    half_size: bool,
    apply_edits: bool,
    name_source: String,
    merged_preview: Raster,
    deghost_sensitivity: DeghostSensitivity,
    deghost_mode: DeghostMode,
    reference_base64: Option<String>,
}

struct Loaded {
    path: String,
    exif_exposure: Option<f64>,
    preview: Raster,
    detection: Raster,
    width: usize,
    height: usize,
}

fn exif_exposure(path: &str, bytes: &[u8]) -> Option<f64> {
    let time = read_exposure_time_secs(path, bytes)? as f64;
    let iso = read_iso(path, bytes).unwrap_or(100) as f64;
    let f_number = read_f_number(path, bytes).unwrap_or(5.6).max(0.5) as f64;
    (time > 0.0 && iso > 0.0).then_some((time * iso) / (f_number * f_number))
}

fn median(mut values: Vec<f32>) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let middle = values.len() / 2;
    *values
        .select_nth_unstable_by(middle, |a, b| a.total_cmp(b))
        .1
}

fn percentile(mut values: Vec<f32>, fraction: f64) -> f32 {
    if values.is_empty() {
        return 1.0;
    }
    let index = ((values.len() - 1) as f64 * fraction) as usize;
    *values
        .select_nth_unstable_by(index, |a, b| a.total_cmp(b))
        .1
}

fn detection_image(detection: &Raster, relative_exposure: f64, scale: f32) -> Raster {
    let gain = scale / relative_exposure as f32;
    map_pixels(detection, 1, |p, o| {
        let x = p[0].max(0.0) * gain;
        o[0] = srgb_encode(x / (1.0 + x));
    })
}

fn frame_role(frames: &[Frame], reference: usize, index: usize) -> FrameRole {
    FrameRole {
        exposure: frames[index].exposure as f32,
        darkest: index == 0,
        reference: index == reference,
    }
}

fn load_frames(
    loader: &SourceLoader,
    paths: &[String],
    context: &MergeContext,
) -> Result<Vec<Loaded>, String> {
    let loaded = loader.load_all(paths, |_, path| {
        context.check_cancelled()?;
        context.report(format!("Processing '{}'", file_name(path)));
        let decoded = loader.decode(path)?;
        context.check_cancelled()?;
        let exif_exposure = exif_exposure(path, &decoded.file_bytes);
        let image = decoded.image;
        let (width, height) = (image.width, image.height);
        let (dw, dh) = fit_size(width, height, DETECTION_MAX_DIMENSION);
        let detection = map_pixels(&resize_area(&image, dw, dh), 1, |p, o| o[0] = luminance(p));
        let (pw, ph) = fit_size(width, height, PREVIEW_MAX_DIMENSION);
        let preview = resize_area(&image, pw, ph);
        Ok(Loaded {
            path: path.to_string(),
            exif_exposure,
            preview,
            detection,
            width,
            height,
        })
    })?;
    ensure_same_size(loaded.iter().map(|l| (l.path.as_str(), l.width, l.height)))?;
    Ok(loaded)
}

fn align_frames(loaded: Vec<Loaded>, mut exposures: Vec<f64>) -> (Vec<Frame>, usize, usize) {
    let medians: Vec<f32> = loaded
        .iter()
        .map(|l| median(l.detection.data.clone()))
        .collect();
    let mut order: Vec<usize> = (0..loaded.len()).collect();
    order.sort_by(|&a, &b| {
        exposures[a]
            .total_cmp(&exposures[b])
            .then(medians[a].total_cmp(&medians[b]))
    });
    let mut slots: Vec<Option<Loaded>> = loaded.into_iter().map(Some).collect();
    let loaded: Vec<Loaded> = order.iter().map(|&i| slots[i].take().unwrap()).collect();
    exposures = order.iter().map(|&i| exposures[i]).collect();
    let reference = (loaded.len() - 1) / 2;
    let reference_exposure = exposures[reference];
    exposures.iter_mut().for_each(|e| *e /= reference_exposure);

    let (width, height) = (loaded[0].width, loaded[0].height);
    let detection_scale = 0.36 / medians[order[reference]].max(1e-6);
    let features: Vec<AlignmentFeatures> = loaded
        .par_iter()
        .zip(&exposures)
        .map(|(l, &e)| {
            AlignmentFeatures::detect(
                &detection_image(&l.detection, e, detection_scale),
                width,
                height,
            )
        })
        .collect();
    let mut transforms = vec![Matrix3::identity(); loaded.len()];
    let mut unaligned = 0;
    let neighbours = (0..reference)
        .rev()
        .map(|k| (k, k + 1))
        .chain((reference + 1..loaded.len()).map(|k| (k, k - 1)));
    for (k, toward) in neighbours {
        let step = estimate_transform(&features[toward], &features[k]);
        if step.is_none() {
            unaligned += 1;
        }
        transforms[k] = step.unwrap_or_else(Matrix3::identity) * transforms[toward];
    }
    drop(features);
    for t in transforms.iter_mut() {
        if max_displacement(t, width as f64, height as f64) < IDENTITY_DISPLACEMENT {
            *t = Matrix3::identity();
        }
    }

    let preview_scale = width as f64 / loaded[0].preview.width as f64;
    let frames = loaded
        .into_par_iter()
        .zip(transforms.into_par_iter())
        .zip(exposures.into_par_iter())
        .map(|((l, transform), exposure)| {
            let (preview, valid) = if transform == Matrix3::identity() {
                let valid = vec![1.0; l.preview.width * l.preview.height];
                (l.preview, valid)
            } else {
                let local = downscaled(&transform, preview_scale);
                warp(&l.preview, &local, l.preview.width, l.preview.height, false)
            };
            Frame {
                path: l.path,
                exposure,
                transform,
                preview,
                valid,
            }
        })
        .collect();
    (frames, reference, unaligned)
}

fn refine_exposures(clip: f32, frames: &mut [Frame], reference: usize) {
    let n = frames.len();
    let steps: Vec<f64> = (0..n - 1)
        .map(|k| {
            let nominal = frames[k + 1].exposure / frames[k].exposure;
            match exposure_ratio(
                clip,
                (&frames[k].preview, &frames[k].valid),
                (&frames[k + 1].preview, &frames[k + 1].valid),
            ) {
                Some(r) if (r / nominal).ln().abs() < MAX_RATIO_CORRECTION.ln() => r,
                _ => nominal,
            }
        })
        .collect();
    let mut refined = vec![1.0f64; n];
    for k in (0..reference).rev() {
        refined[k] = refined[k + 1] / steps[k];
    }
    for k in reference + 1..n {
        refined[k] = refined[k - 1] * steps[k - 1];
    }
    for (frame, exposure) in frames.iter_mut().zip(refined) {
        frame.exposure = exposure;
    }
}

fn merge_radiance(
    frames: &mut [Frame],
    reference: usize,
    sensitivity: DeghostSensitivity,
    deghost_mode: DeghostMode,
) -> (Merge, Raster) {
    let clip = frames
        .iter()
        .map(|f| {
            let green: Vec<f32> = f
                .preview
                .data
                .as_chunks::<3>()
                .0
                .iter()
                .map(|p| p[1])
                .collect();
            percentile(green, CLIP_PERCENTILE)
        })
        .fold(0.0f32, f32::max)
        .max(1e-3);
    refine_exposures(clip, frames, reference);
    recompute_radiance_preview(clip, frames, reference, sensitivity, deghost_mode)
}

fn recompute_radiance_preview(
    clip: f32,
    frames: &[Frame],
    reference: usize,
    sensitivity: DeghostSensitivity,
    deghost_mode: DeghostMode,
) -> (Merge, Raster) {
    let previews: Vec<PreviewFrame> = (0..frames.len())
        .map(|i| PreviewFrame {
            image: &frames[i].preview,
            valid: &frames[i].valid,
            role: frame_role(frames, reference, i),
        })
        .collect();
    let masks = match (sensitivity, deghost_mode) {
        (DeghostSensitivity::Off, _) => (0..frames.len()).map(|_| None).collect(),
        (_, DeghostMode::Fast) => ghost_masks(clip, &previews, reference, sensitivity),
        (_, DeghostMode::OpticalFlow) => {
            ghost_masks_optical_flow(clip, &previews, reference, sensitivity)
        }
    };
    let (pw, ph) = (frames[0].preview.width, frames[0].preview.height);
    let mut accumulator = Accumulator::new(pw, ph);
    for (frame, mask) in previews.iter().zip(&masks) {
        accumulator.add(clip, frame.role, frame.image, frame.valid, mask.as_ref());
    }
    (Merge::Radiance { clip, masks }, accumulator.finish())
}

fn physical_exif_exposure(path: &str) -> Option<f64> {
    let (source_path, _) = crate::library::sidecars::parse_virtual_path(path);
    let path_str = source_path.to_string_lossy();
    let mut exp_t = None;
    let mut iso = None;
    let mut f_num = None;

    if let Some(map) = read_rrexif_sidecar(&source_path) {
        exp_t = read_exposure_time_secs(&path_str, &[]);
        iso = read_iso(&path_str, &[]);
        f_num = map
            .get("FNumber")
            .and_then(|s| s.trim_start_matches("f/").trim().parse::<f32>().ok());
    }
    if exp_t.is_none() || iso.is_none() || f_num.is_none() {
        if let Ok(mapped) = crate::library::file_ops::read_file_mapped(&source_path) {
            if exp_t.is_none() {
                exp_t = read_exposure_time_secs(&path_str, &mapped);
            }
            if iso.is_none() {
                iso = read_iso(&path_str, &mapped);
            }
            if f_num.is_none() {
                f_num = read_f_number(&path_str, &mapped);
            }
        }
    }
    let t = exp_t? as f64;
    let i = iso.unwrap_or(100) as f64;
    let f = f_num.unwrap_or(5.6).max(0.5) as f64;
    (t > 0.0 && i > 0.0).then_some((t * i) / (f * f))
}

fn fuse_previews(frames: &[Frame]) -> Raster {
    let (pw, ph) = (frames[0].preview.width, frames[0].preview.height);
    let weights: Vec<Vec<f32>> = frames
        .iter()
        .map(|f| exposure_fusion::weights(&f.preview, &f.valid))
        .collect();
    let total = sum_maps(&weights, pw * ph);
    let mut fusion = ExposureFusion::new(pw, ph);
    for (frame, w) in frames.iter().zip(&weights) {
        fusion.add(&frame.preview, w, &total);
    }
    fusion.finish()
}

fn sum_maps(maps: &[Vec<f32>], len: usize) -> Vec<f32> {
    let mut total = vec![0.0f32; len];
    for map in maps {
        add_into(&mut total, map);
    }
    total
}

fn add_into(total: &mut [f32], map: &[f32]) {
    total
        .par_iter_mut()
        .zip(map.par_iter())
        .for_each(|(t, &v)| *t += v);
}

pub fn build_session(
    paths: &[String],
    half_size: bool,
    apply_edits: bool,
    context: &MergeContext,
) -> Result<HdrSession, String> {
    let scene = paths.iter().all(is_raw_file);
    let loader = if scene && paths.len() >= 2 {
        context.loader_for_hdr_brackets(true, half_size, apply_edits)
    } else {
        context.loader_with_options(scene, half_size, apply_edits)
    };
    context.report("Loading images...");
    let loaded = load_frames(&loader, paths, context)?;
    let exposures: Vec<f64> = if loaded.iter().all(|l| l.exif_exposure.is_some()) {
        loaded.iter().filter_map(|l| l.exif_exposure).collect()
    } else {
        loaded
            .iter()
            .map(|l| median(l.detection.data.clone()).max(1e-6) as f64)
            .collect()
    };
    let (width, height) = (loaded[0].width, loaded[0].height);

    context.check_cancelled()?;
    context.report("Aligning images...");
    let (mut frames, reference, unaligned) = align_frames(loaded, exposures);

    context.check_cancelled()?;
    context.report("Merging exposures...");
    let default_deghost = DeghostSensitivity::Medium;
    let default_deghost_mode = DeghostMode::OpticalFlow;
    let (merge, merged_preview) = if scene {
        merge_radiance(&mut frames, reference, default_deghost, default_deghost_mode)
    } else {
        (Merge::Fusion, fuse_previews(&frames))
    };
    let rendition = Rendition::for_sources(scene, &frames[reference].path, context.settings)
        .with_agx(context.settings);
    let reference_base64 = encode_jpeg_base64(&rendition.to_srgb8(&frames[reference].preview)).ok();

    Ok(HdrSession {
        frames,
        reference,
        merge,
        rendition,
        width,
        height,
        unaligned,
        half_size,
        apply_edits,
        name_source: paths[0].clone(),
        merged_preview,
        deghost_sensitivity: default_deghost,
        deghost_mode: default_deghost_mode,
        reference_base64,
    })
}

pub(crate) struct PreviewHdrPanel {
    pub preview: Raster,
    pub full_width: usize,
    pub full_height: usize,
    pub ref_exif_exposure: Option<f64>,
    pub metadata_source: String,
}

pub(crate) fn build_preview_hdr_panel(
    paths: &[String],
    half_size: bool,
    apply_edits: bool,
    context: &MergeContext,
) -> Result<PreviewHdrPanel, String> {
    if paths.len() == 1 {
        let scene = is_raw_file(&paths[0]);
        let loader = context.loader_with_options(scene, half_size, apply_edits);
        let decoded = loader.decode(&paths[0])?;
        let (full_width, full_height) = (decoded.image.width, decoded.image.height);
        let (pw, ph) = fit_size(full_width, full_height, PREVIEW_MAX_DIMENSION);
        let preview = resize_area(&decoded.image, pw, ph);
        return Ok(PreviewHdrPanel {
            preview,
            full_width,
            full_height,
            ref_exif_exposure: physical_exif_exposure(&paths[0]),
            metadata_source: paths[0].clone(),
        });
    }
    let session = build_session(paths, half_size, apply_edits, context)?;
    let metadata_source = session.frames[session.reference].path.clone();
    Ok(PreviewHdrPanel {
        preview: session.merged_preview,
        full_width: session.width,
        full_height: session.height,
        ref_exif_exposure: physical_exif_exposure(&metadata_source),
        metadata_source,
    })
}

pub(crate) fn build_linear_hdr_panel(
    paths: &[String],
    half_size: bool,
    apply_edits: bool,
    gain_scale: f32,
    context: &MergeContext,
) -> Result<(Raster, Rendition, String), String> {
    if paths.len() == 1 {
        let scene = is_raw_file(&paths[0]);
        let loader = context.loader_with_options(scene, half_size, apply_edits);
        let mut decoded = loader.decode(&paths[0])?;
        if (gain_scale - 1.0).abs() > 1e-4 {
            decoded.image.data.par_iter_mut().for_each(|v| *v *= gain_scale);
        }
        let rendition = Rendition::for_sources(scene, &paths[0], context.settings)
            .with_agx(context.settings);
        return Ok((decoded.image, rendition, paths[0].clone()));
    }
    let session = build_session(paths, half_size, apply_edits, context)?;
    let mut merged = match &session.merge {
        Merge::Radiance { clip, masks } => session.render_radiance(
            *clip,
            masks,
            &context.loader_for_hdr_brackets(true, session.half_size, session.apply_edits),
            context,
        )?,
        Merge::Fusion => session.render_fusion(
            &context.loader_with_options(false, session.half_size, session.apply_edits),
            context,
        )?,
    };
    if (gain_scale - 1.0).abs() > 1e-4 {
        merged.data.par_iter_mut().for_each(|v| *v *= gain_scale);
    }
    let metadata_source = session.frames[session.reference].path.clone();
    Ok((merged, session.rendition, metadata_source))
}

impl HdrSession {
    fn crop_rects(&self, settings: &HdrSettings) -> (Rect, Rect) {
        let valid: Vec<&[f32]> = self.frames.iter().map(|f| f.valid.as_slice()).collect();
        crop_rects(
            &valid,
            (self.merged_preview.width, self.merged_preview.height),
            (self.width, self.height),
            settings.auto_crop,
        )
    }

    fn load_aligned(
        &self,
        loader: &SourceLoader,
        index: usize,
    ) -> Result<(Raster, Vec<f32>), String> {
        let decoded = loader.decode(&self.frames[index].path)?;
        let transform = self.frames[index].transform;
        Ok(if transform == Matrix3::identity() {
            let valid = vec![1.0; self.width * self.height];
            (decoded.image, valid)
        } else {
            warp(&decoded.image, &transform, self.width, self.height, true)
        })
    }

    fn render_radiance(
        &self,
        clip: f32,
        masks: &[Option<Raster>],
        loader: &SourceLoader,
        context: &MergeContext,
    ) -> Result<Raster, String> {
        let accumulator = Mutex::new(Accumulator::new(self.width, self.height));
        let paths: Vec<String> = self.frames.iter().map(|f| f.path.clone()).collect();
        loader.load_all(&paths, |index, path| {
            context.check_cancelled()?;
            context.report(format!("Merging '{}'", file_name(path)));
            let (aligned, valid) = self.load_aligned(loader, index)?;
            accumulator.lock().unwrap().add(
                clip,
                frame_role(&self.frames, self.reference, index),
                &aligned,
                &valid,
                masks[index].as_ref(),
            );
            Ok(())
        })?;
        Ok(accumulator
            .into_inner()
            .map_err(|_| "Merging the images failed unexpectedly.".to_string())?
            .finish())
    }

    fn render_fusion(
        &self,
        loader: &SourceLoader,
        context: &MergeContext,
    ) -> Result<Raster, String> {
        const SINGLE_PASS_FUSION_BUDGET_BYTES: usize = 1_500_000_000;
        let paths: Vec<String> = self.frames.iter().map(|f| f.path.clone()).collect();
        let estimated_bytes = paths
            .len()
            .saturating_mul(self.width)
            .saturating_mul(self.height)
            .saturating_mul(16);

        if estimated_bytes <= SINGLE_PASS_FUSION_BUDGET_BYTES {
            let aligned_frames = Mutex::new(Vec::with_capacity(paths.len()));
            let total = Mutex::new(vec![0.0f32; self.width * self.height]);
            loader.load_all(&paths, |index, path| {
                context.check_cancelled()?;
                context.report(format!("Aligning & Weighting '{}'", file_name(path)));
                let (aligned, valid) = self.load_aligned(loader, index)?;
                let weights = exposure_fusion::weights(&aligned, &valid);
                add_into(&mut total.lock().unwrap(), &weights);
                aligned_frames.lock().unwrap().push((aligned, weights));
                Ok(())
            })?;
            let total = total
                .into_inner()
                .map_err(|_| "Merging the images failed unexpectedly.".to_string())?;
            let mut fusion = ExposureFusion::new(self.width, self.height);
            for (aligned, weights) in aligned_frames
                .into_inner()
                .map_err(|_| "Merging the images failed unexpectedly.".to_string())?
            {
                context.check_cancelled()?;
                fusion.add(&aligned, &weights, &total);
            }
            Ok(fusion.finish())
        } else {
            // Streaming 2-pass fallback for huge multi-frame TIFF stacks (keeps only 1 frame in RAM).
            let total = Mutex::new(vec![0.0f32; self.width * self.height]);
            loader.load_all(&paths, |index, path| {
                context.check_cancelled()?;
                context.report(format!("Weighting '{}'", file_name(path)));
                let (aligned, valid) = self.load_aligned(loader, index)?;
                let weights = exposure_fusion::weights(&aligned, &valid);
                add_into(&mut total.lock().unwrap(), &weights);
                Ok(())
            })?;
            let total = total
                .into_inner()
                .map_err(|_| "Merging the images failed unexpectedly.".to_string())?;
            let fusion = Mutex::new(ExposureFusion::new(self.width, self.height));
            loader.load_all(&paths, |index, path| {
                context.check_cancelled()?;
                context.report(format!("Blending '{}'", file_name(path)));
                let (aligned, valid) = self.load_aligned(loader, index)?;
                let weights = exposure_fusion::weights(&aligned, &valid);
                fusion.lock().unwrap().add(&aligned, &weights, &total);
                Ok(())
            })?;
            Ok(fusion
                .into_inner()
                .map_err(|_| "Merging the images failed unexpectedly.".to_string())?
                .finish())
        }
    }
}

impl HdrSession {
    fn sync_radiance_deghost(&mut self, settings: &HdrSettings) {
        if let Merge::Radiance { clip, .. } = self.merge
            && (settings.deghost_sensitivity != self.deghost_sensitivity
                || settings.deghost_mode != self.deghost_mode)
        {
            let (merge, preview_raster) = recompute_radiance_preview(
                clip,
                &self.frames,
                self.reference,
                settings.deghost_sensitivity,
                settings.deghost_mode,
            );
            self.merge = merge;
            self.merged_preview = preview_raster;
            self.deghost_sensitivity = settings.deghost_sensitivity;
            self.deghost_mode = settings.deghost_mode;
        }
    }
}

fn build_ghost_overlay_base64(
    masks: &[Option<Raster>],
    display: &image::RgbImage,
    rect: &Rect,
) -> Option<String> {
    let cropped_masks: Vec<Raster> = masks
        .iter()
        .filter_map(|m| m.as_ref().map(|r| crop(r, rect)))
        .collect();
    if cropped_masks.is_empty() {
        return None;
    }
    let w = display.width() as usize;
    let h = display.height() as usize;
    let mut out = display.clone();
    let raw = out.as_mut();
    for y in 0..h {
        for x in 0..w {
            let mut max_mask = 0.0f32;
            for m in &cropped_masks {
                if x < m.width && y < m.height {
                    max_mask = max_mask.max(m.get(x, y, 0));
                }
            }
            let alpha = (max_mask * 0.65).clamp(0.0, 0.72);
            if alpha > 0.01 {
                let idx = (y * w + x) * 3;
                let inv = 1.0 - alpha;
                raw[idx] = (raw[idx] as f32 * inv + 245.0 * alpha).round() as u8;
                raw[idx + 1] = (raw[idx + 1] as f32 * inv + 55.0 * alpha).round() as u8;
                raw[idx + 2] = (raw[idx + 2] as f32 * inv + 70.0 * alpha).round() as u8;
            }
        }
    }
    encode_jpeg_base64(&out).ok()
}

impl MergeSession for HdrSession {
    fn preview(&mut self, settings: &Value) -> Result<MergePreview, String> {
        let settings: HdrSettings = parse_settings(settings)?;
        self.sync_radiance_deghost(&settings);
        let is_scene = matches!(self.merge, Merge::Radiance { .. });
        let (rect, full) = self.crop_rects(&settings);
        let cropped_preview = crop(&self.merged_preview, &rect);
        let (tm_raster, tm_rendition) = tonemap_hdr_raster(
            &cropped_preview,
            self.rendition.scene_exposure(),
            &settings.profile,
            settings.hdr_strength,
            settings.exposure_bias,
            settings.highlight_recovery,
            settings.shadow_lift,
            settings.detail_boost,
        );
        let display = tm_rendition.to_srgb8(&tm_raster);
        let overlay_base64 = match &self.merge {
            Merge::Radiance { masks, .. } => build_ghost_overlay_base64(masks, &display, &rect),
            Merge::Fusion => None,
        };
        let (hdr_preview_base64, headroom_map_base64, hdr_peak_nits) = if is_scene {
            let pq_exp = self.rendition.scene_exposure() + settings.exposure_bias;
            let peak = compute_hdr_peak_nits(&cropped_preview, pq_exp);
            let mode = settings.preview_mode.trim().to_ascii_lowercase();
            let hdr_b64 = if mode == "hdr10" {
                encode_hdr10_pq_png_base64(&cropped_preview, pq_exp)
                    .ok()
                    .map(|(b64, _)| b64)
            } else {
                None
            };
            let headroom = if mode == "headroom" {
                encode_hdr_headroom_heatmap_base64(&cropped_preview, pq_exp).ok()
            } else {
                None
            };
            (hdr_b64, headroom, Some(peak))
        } else {
            (None, None, None)
        };
        let ref_cropped = if self.reference_base64.is_some() {
            let ref_crop = crop(&self.frames[self.reference].preview, &rect);
            encode_jpeg_base64(&self.rendition.to_srgb8(&ref_crop)).ok()
        } else {
            None
        };
        Ok(MergePreview {
            base64: encode_jpeg_base64(&display)?,
            overlay_base64,
            reference_base64: ref_cropped,
            hdr_preview_base64,
            headroom_map_base64,
            hdr_peak_nits,
            is_scene_referred: is_scene,
            width: full.width,
            height: full.height,
            image_count: self.frames.len(),
            unaligned_count: self.unaligned,
            ..Default::default()
        })
    }

    fn render(&mut self, settings: &Value, context: &MergeContext) -> Result<MergeOutput, String> {
        let settings: HdrSettings = parse_settings(settings)?;
        self.sync_radiance_deghost(&settings);
        let is_scene = matches!(self.merge, Merge::Radiance { .. });
        let merged = match &self.merge {
            Merge::Radiance { clip, masks } => self.render_radiance(
                *clip,
                masks,
                &context.loader_for_hdr_brackets(true, self.half_size, settings.apply_edits),
                context,
            )?,
            Merge::Fusion => self.render_fusion(
                &context.loader_with_options(false, self.half_size, settings.apply_edits),
                context,
            )?,
        };
        let (_, rect) = self.crop_rects(&settings);
        let mut final_image = crop(&merged, &rect);
        if settings.half_size && !self.half_size {
            final_image = resize_area(
                &final_image,
                (final_image.width / 2).max(1),
                (final_image.height / 2).max(1),
            );
        }
        let base_exposure = self.rendition.scene_exposure();
        let (output_image, output_rendition, linear_scene) = {
            let (tm_raster, tm_rendition) = tonemap_hdr_raster(
                &final_image,
                base_exposure,
                &settings.profile,
                settings.hdr_strength,
                settings.exposure_bias,
                settings.highlight_recovery,
                settings.shadow_lift,
                settings.detail_boost,
            );
            (tm_raster, tm_rendition, is_scene.then_some(final_image))
        };
        Ok(MergeOutput {
            image: output_image,
            linear_scene,
            scene_exposure: base_exposure,
            hdr_exposure_bias: settings.exposure_bias,
            rendition: output_rendition,
            suffix: "HDR",
            name_source: self.name_source.clone(),
            metadata_source: self.frames[self.reference].path.clone(),
            keeps_lens_geometry: true,
            is_scene_referred: is_scene,
            is_360_equirectangular: false,
            tone_mapper: is_scene.then_some("agx"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tonemap_hdr_raster_no_highlight_washout_at_partial_strength() {
        let (w, h) = (32usize, 32usize);
        let build_scene = |sky_rgb: [f32; 3]| -> Raster {
            let mut data = Vec::with_capacity(w * h * 3);
            for y in 0..h {
                for x in 0..w {
                    if y < 2 && x < 6 {
                        // Small specular glint (~1.2% of pixels) to anchor the 99.85th percentile white point
                        data.extend_from_slice(&[8.0, 8.0, 8.0]);
                    } else if y < h / 4 {
                        data.extend_from_slice(&sky_rgb);
                    } else if y < (3 * h) / 4 {
                        // Neutral 18% gray midtone
                        data.extend_from_slice(&[0.18, 0.18, 0.18]);
                    } else {
                        // Shadow
                        data.extend_from_slice(&[0.02, 0.02, 0.02]);
                    }
                }
            }
            Raster::from_data(w, h, 3, data)
        };

        // Bright blue sky within AgX white point where blue (> 1.333) previously hard-clipped
        let raster = build_scene([0.85, 1.00, 1.38]);
        for strength in [0.0f32, 65.0f32, 100.0f32] {
            let (tm, rendition) = tonemap_hdr_raster(
                &raster,
                0.0,
                "natural",
                strength,
                0.0,
                50.0,
                30.0,
                1.05,
            );
            assert_eq!(rendition, Rendition::SrgbDisplay);
            for (i, c) in tm.data.iter().enumerate() {
                assert!(c.is_finite() && *c >= 0.0 && *c <= 1.0, "channel {} out of range: {}", i, c);
            }
            let sky = tm.pixel(w / 2, h / 8);
            assert!(
                sky[2] > sky[0] + 0.05 && sky[0] < 0.95 && sky[1] < 0.99 && sky[2] < 0.998,
                "Sky should retain blue hue without blowing out at strength {}: {:?}",
                strength,
                sky
            );
        }

        // Brighter HDR sky ([1.6, 1.8, 2.2]) where the previous x / (1 + 0.25 * x) baseline clipped all 3 channels to 1.0
        let bright_raster = build_scene([1.6, 1.8, 2.2]);
        for strength in [65.0f32, 100.0f32] {
            let (tm, rendition) = tonemap_hdr_raster(
                &bright_raster,
                0.0,
                "natural",
                strength,
                0.0,
                50.0,
                30.0,
                1.05,
            );
            assert_eq!(rendition, Rendition::SrgbDisplay);
            let sky = tm.pixel(w / 2, h / 8);
            let sky_luma = 0.2126 * sky[0] + 0.7152 * sky[1] + 0.0722 * sky[2];
            assert!(
                sky[2] > sky[0] + 0.03 && sky_luma < 0.98,
                "Bright sky washed out at strength {}: {:?} (luma={})",
                strength,
                sky,
                sky_luma
            );
        }
    }
}
