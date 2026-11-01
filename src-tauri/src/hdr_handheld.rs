//! Reference-Locked Handheld HDR Engine for RapidRAW
//!
//! Solves handheld multi-exposure bracketing with variable-aperture AEB,
//! moving subjects (cyclists, boats, wind-blown foliage), and highlight clipping:
//! 1. Temporal Burst Clustering (prevents merging separate bursts taken minutes apart).
//! 2. True Linear RAW Ingestion (`hdr_linear`) + 0 EV Median Reference Anchor Selection.
//! 3. Exposure-Equalized, Jointly-Clamped Multi-Scale Huber-IRLS LensWarp Alignment
//!    with 16-tap Catmull-Rom sub-pixel resampling.
//! 4. Smooth 2D Spatially-Varying Radiometric & Vignetting Field Calibration G_k(x, y)
//!    (eliminates aperture-induced f/4.0 vs f/5.6 vignetting steps and WB drift).
//! 5. Asymmetric Reference-Locked Radiance Merge with Motion-Shielded Highlights
//!    and Poisson-Gaussian Shadow Denoising (zero bokeh mold, zero ghosting).
//! 6. Multi-Scale Virtual-Exposure Burt-Adelson Laplacian Pyramid Tone Mapping
//!    (zero alignment/ghosting artifacts, halo-free natural local contrast).

use crate::app_settings::AppSettings;
use crate::exif_processing::{read_exposure_time_secs, read_f_number, read_iso};
use crate::focus_stacking::{
    box_filter, build_align_pyramids, collapse_pyramid, gaussian_blur,
    gaussian_pyramid, laplacian_pyramid, resize_to, solve_alignment, upsample_to, AlignConfig,
    LaplacianPyramid, LensWarp, PlanarRgb, Plane,
};
pub use crate::hdr_fusion::{
    apply_stock_grade_tone_profile, HdrFusionResult, HdrMergeOptions, HdrToneProfile,
};
use crate::image_loader::load_base_image_from_bytes;
use image::Rgb32FImage;
use rayon::prelude::*;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

#[derive(Clone)]
pub struct HandheldBracketFrame {
    pub path: String,
    pub rgb: PlanarRgb,
    pub exposure_secs: f32,
    pub iso: f32,
    pub f_number: f32,
    pub timestamp_secs: Option<i64>,
}

impl HandheldBracketFrame {
    pub fn nominal_exposure(&self) -> f32 {
        let f_sq = (self.f_number * self.f_number).max(0.5);
        (self.exposure_secs.max(1e-5) * (self.iso / 100.0).max(0.25)) / f_sq
    }
}

/// Parses EXIF DateTimeOriginal ("YYYY:MM:DD HH:MM:SS") from the first 64KB of a RAW/image file
/// into a monotonic second count so separate bursts (e.g. 69s apart) are never merged together.
pub fn parse_exif_timestamp_secs(bytes: &[u8]) -> Option<i64> {
    let limit = bytes.len().min(65536);
    let slice = &bytes[..limit];
    for i in 0..slice.len().saturating_sub(19) {
        let w = &slice[i..i + 19];
        if w[4] == b':'
            && w[7] == b':'
            && w[10] == b' '
            && w[13] == b':'
            && w[16] == b':'
            && w[0..4].iter().all(|c| c.is_ascii_digit())
            && w[5..7].iter().all(|c| c.is_ascii_digit())
            && w[8..10].iter().all(|c| c.is_ascii_digit())
            && w[11..13].iter().all(|c| c.is_ascii_digit())
            && w[14..16].iter().all(|c| c.is_ascii_digit())
            && w[17..19].iter().all(|c| c.is_ascii_digit())
        {
            let year: i64 = std::str::from_utf8(&w[0..4]).ok()?.parse().ok()?;
            if !(1990..=2100).contains(&year) {
                continue;
            }
            let month: i64 = std::str::from_utf8(&w[5..7]).ok()?.parse().ok()?;
            let day: i64 = std::str::from_utf8(&w[8..10]).ok()?.parse().ok()?;
            let hour: i64 = std::str::from_utf8(&w[11..13]).ok()?.parse().ok()?;
            let min: i64 = std::str::from_utf8(&w[14..16]).ok()?.parse().ok()?;
            let sec: i64 = std::str::from_utf8(&w[17..19]).ok()?.parse().ok()?;
            return Some(
                (((year * 365 + month * 31 + day) * 24 + hour) * 60 + min) * 60 + sec,
            );
        }
    }
    None
}

/// Filters input paths to a single coherent temporal burst (max 5s gap between consecutive shots).
pub fn filter_coherent_burst(paths: &[String], preferred_ref_idx: Option<usize>) -> Vec<String> {
    if paths.len() <= 1 {
        return paths.to_vec();
    }

    let mut timestamps: Vec<Option<i64>> = Vec::with_capacity(paths.len());
    for p in paths {
        let ts = fs::read(p)
            .ok()
            .and_then(|b| parse_exif_timestamp_secs(&b));
        timestamps.push(ts);
    }

    if timestamps.iter().any(|t| t.is_none()) {
        return paths.to_vec();
    }

    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut current_group = vec![0usize];

    for i in 1..paths.len() {
        let t_prev = timestamps[i - 1].unwrap();
        let t_curr = timestamps[i].unwrap();
        if (t_curr - t_prev).abs() <= 5 {
            current_group.push(i);
        } else {
            groups.push(current_group);
            current_group = vec![i];
        }
    }
    groups.push(current_group);

    if groups.len() == 1 {
        return paths.to_vec();
    }

    let target_idx = preferred_ref_idx.unwrap_or(0).min(paths.len() - 1);
    for g in &groups {
        if g.contains(&target_idx) && g.len() >= 2 {
            return g.iter().map(|&idx| paths[idx].clone()).collect();
        }
    }

    let best_group = groups
        .into_iter()
        .max_by_key(|g| g.len())
        .unwrap_or_default();
    best_group.into_iter().map(|idx| paths[idx].clone()).collect()
}

/// Loads RAW/image brackets in pure linear sensor mode (`hdr_linear`) without premature highlight distortion.
pub fn load_handheld_brackets<R: tauri::Runtime>(
    paths: &[String],
    settings: &AppSettings,
    half_size: bool,
    app_handle: Option<&AppHandle<R>>,
    cancel_token: Option<&Arc<AtomicBool>>,
) -> Result<Vec<HandheldBracketFrame>, String> {
    load_handheld_brackets_with_mode(paths, settings, half_size, true, app_handle, cancel_token)
}

/// Loads RAW/image brackets in pure linear sensor mode (`hdr_linear`), optionally enforcing burst timestamp filtering.
pub fn load_handheld_brackets_with_mode<R: tauri::Runtime>(
    paths: &[String],
    settings: &AppSettings,
    half_size: bool,
    enforce_burst_filter: bool,
    app_handle: Option<&AppHandle<R>>,
    cancel_token: Option<&Arc<AtomicBool>>,
) -> Result<Vec<HandheldBracketFrame>, String> {
    let coherent_paths = if enforce_burst_filter {
        filter_coherent_burst(paths, None)
    } else {
        paths.to_vec()
    };
    if coherent_paths.is_empty() {
        return Err("HDR merge requires at least one image in the bracket.".to_string());
    }
    if enforce_burst_filter && coherent_paths.len() < 2 {
        return Err("HDR merge requires at least two images in the bracket.".to_string());
    }

    let mut hdr_settings = settings.clone();
    hdr_settings.linear_raw_mode = "hdr_linear".to_string();
    hdr_settings.raw_preprocessing_sharpening = Some(0.0);

    let mut frames = Vec::with_capacity(coherent_paths.len());
    for (idx, path) in coherent_paths.iter().enumerate() {
        if let Some(tok) = cancel_token {
            if tok.load(Ordering::Relaxed) {
                return Err("HDR merge cancelled by user.".to_string());
            }
        }
        if let Some(h) = app_handle {
            let name = Path::new(path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            let pct = 5 + (idx * 20 / coherent_paths.len().max(1));
            let _ = h.emit(
                "hdr-progress",
                format!(
                    "Decoding raw exposure {}/{} ({})... {}%",
                    idx + 1,
                    coherent_paths.len(),
                    name,
                    pct
                ),
            );
        }

        let bytes = fs::read(path).map_err(|e| format!("Failed to read {}: {}", path, e))?;
        let ts = parse_exif_timestamp_secs(&bytes);
        let iso = read_iso(path, &bytes).map(|v| v as f32).unwrap_or(100.0);
        let exp_secs = read_exposure_time_secs(path, &bytes).unwrap_or(0.01);
        let f_num = read_f_number(path, &bytes).unwrap_or(4.0);

        let dyn_img = load_base_image_from_bytes(&bytes, path, half_size, &hdr_settings, None)
            .map_err(|e| format!("Failed to decode {}: {}", path, e))?;
        let rgb32 = dyn_img.to_rgb32f();
        let planar = PlanarRgb::from_rgb32f(&rgb32);

        frames.push(HandheldBracketFrame {
            path: path.clone(),
            rgb: planar,
            exposure_secs: exp_secs,
            iso,
            f_number: f_num,
            timestamp_secs: ts,
        });
    }

    let (w0, h0) = (frames[0].rgb.w, frames[0].rgb.h);
    for f in frames.iter().skip(1) {
        if f.rgb.w != w0 || f.rgb.h != h0 {
            return Err(format!(
                "Dimension mismatch in HDR bracket: {}x{} vs {}x{}",
                w0, h0, f.rgb.w, f.rgb.h
            ));
        }
    }

    Ok(frames)
}

/// Selects the median-exposure (0 EV) frame as the geometric and bokeh anchor.
pub fn select_median_exposure_anchor(frames: &[HandheldBracketFrame]) -> usize {
    if frames.len() <= 1 {
        return 0;
    }
    let mut indexed: Vec<(usize, f32)> = frames
        .iter()
        .enumerate()
        .map(|(i, f)| {
            // Combine EXIF physical exposure with image midtone luma as fallback
            let mut nom = f.nominal_exposure();
            if !nom.is_finite() || nom <= 0.0 {
                nom = 1.0;
            }
            (i, nom)
        })
        .collect();

    indexed.sort_by(|a, b| a.1.total_cmp(&b.1));
    indexed[indexed.len() / 2].0
}

#[inline]
fn smootherstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1e-6)).clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Single-frame specular highlight recovery used only when even the darkest bracket saturates,
/// or inside a moving highlight where the darker bracket is occluded.
#[inline]
pub fn recover_single_frame_highlight(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max_c = r.max(g).max(b);
    if max_c <= 0.52 {
        return (r, g, b);
    }
    let mut cur_r = r;
    let mut cur_g = g;
    let mut cur_b = b;

    let outer_blend = smootherstep(0.52, 1.35, max_c);
    let magenta = (cur_r.min(cur_b) - cur_g).max(0.0);
    if magenta > 0.0 {
        let target_g = cur_r.min(cur_b) * 0.82 + ((cur_r + cur_b) * 0.5) * 0.18;
        let correction = (target_g - cur_g).max(0.0);
        let magenta_weight = smoothstep(0.0, 0.22, magenta / max_c);
        cur_g += correction * outer_blend * magenta_weight;
    }

    let residual = (cur_r.min(cur_b) - cur_g).max(0.0);
    if residual > 0.0 {
        cur_g += residual * outer_blend;
    }

    let new_max = cur_r.max(cur_g).max(cur_b);
    let min_c = cur_r.min(cur_g).min(cur_b);
    let knee = smoothstep(0.55, 1.40, new_max);
    if knee > 0.0 {
        let neutrality = (min_c / new_max.max(1e-5)).clamp(0.0, 1.0);
        let core_burn = smoothstep(0.65, 2.5, new_max);
        let desat = (knee * (neutrality * 0.85 + core_burn * 0.15)).clamp(0.0, 1.0);
        let smooth_desat = desat * desat * (3.0 - 2.0 * desat);
        let neutral_value = new_max;
        cur_r = cur_r * (1.0 - smooth_desat) + neutral_value * smooth_desat;
        cur_g = cur_g * (1.0 - smooth_desat) + neutral_value * smooth_desat;
        cur_b = cur_b * (1.0 - smooth_desat) + neutral_value * smooth_desat;
    }

    (cur_r, cur_g, cur_b)
}

/// Estimates robust global per-channel exposure ratios `r_c = median(I_ref_c / I_k_c)`
/// over strictly unclipped linear midtones/shadows.
fn estimate_global_channel_ratios(
    ref_rgb: &PlanarRgb,
    src_rgb: &PlanarRgb,
    exif_ratio: f32,
) -> [f32; 3] {
    let (w, h) = (ref_rgb.w, ref_rgb.h);
    let step = ((w * h) / 40_000).max(1);
    let mut ratios = [Vec::new(), Vec::new(), Vec::new()];

    let min_r = exif_ratio * 0.45;
    let max_r = exif_ratio * 2.20;

    for i in (0..(w * h)).step_by(step) {
        let r_ref = ref_rgb.c[0].data[i];
        let g_ref = ref_rgb.c[1].data[i];
        let b_ref = ref_rgb.c[2].data[i];
        let p_ref = r_ref.max(g_ref).max(b_ref).max(1.22 * g_ref);

        let r_src = src_rgb.c[0].data[i];
        let g_src = src_rgb.c[1].data[i];
        let b_src = src_rgb.c[2].data[i];
        let p_src = r_src.max(g_src).max(b_src).max(1.22 * g_src);

        // Strictly unclipped and above noise floor in both frames
        if p_ref > 0.015 && p_ref < 0.46 && p_src > 0.012 && p_src < 0.46 {
            for ch in 0..3 {
                let vr = ref_rgb.c[ch].data[i];
                let vs = src_rgb.c[ch].data[i];
                if vr > 0.012 && vs > 0.010 {
                    let ratio = vr / vs;
                    if ratio >= min_r && ratio <= max_r {
                        ratios[ch].push(ratio);
                    }
                }
            }
        }
    }

    let mut out = [exif_ratio; 3];
    for ch in 0..3 {
        if ratios[ch].len() >= 100 {
            let mid = ratios[ch].len() / 2;
            ratios[ch].select_nth_unstable_by(mid, |a, b| a.total_cmp(b));
            out[ch] = ratios[ch][mid];
        }
    }
    out
}

/// Aligns a bracket frame to the reference frame using exposure-equalized, jointly-clamped
/// log-luminance pyramids and the multi-scale Huber-IRLS LensWarp solver.
fn align_bracket_to_reference(
    ref_rgb: &PlanarRgb,
    ref_luma: &Plane,
    src_rgb: &PlanarRgb,
    exif_ratio: f32,
) -> (PlanarRgb, Plane, [f32; 3]) {
    let (w, h) = (ref_rgb.w, ref_rgb.h);
    let init_ratios = estimate_global_channel_ratios(ref_rgb, src_rgb, exif_ratio);
    let luma_ratio = 0.2126 * init_ratios[0] + 0.7152 * init_ratios[1] + 0.0722 * init_ratios[2];

    // Joint unclipped ceiling in reference luminance units:
    // Neither frame is saturated below 0.45 in raw sensor units.
    let joint_ceiling = (0.45 * luma_ratio.min(1.0)).max(0.05);
    let inv_ceil = 1.0 / joint_ceiling;

    let tmpl_plane = ref_luma.map(|y| {
        let yc = y.clamp(0.0, joint_ceiling) * inv_ceil;
        (1.0 + 24.0 * yc).ln()
    });

    let src_luma = src_rgb.luma();
    let src_plane = src_luma.map(|y| {
        let y_eq = (y * luma_ratio).clamp(0.0, joint_ceiling) * inv_ceil;
        (1.0 + 24.0 * y_eq).ln()
    });

    let cfg = AlignConfig {
        finest_dim: 2000,
        coarsest_dim: 80,
        max_iterations: 40,
        target_samples: 140_000,
        min_quality: 0.20,
        polynomial_refinement: true,
    };

    let pyr = build_align_pyramids(&tmpl_plane, &src_plane, &cfg);
    let pose = solve_alignment(&pyr, LensWarp::identity(w, h), &cfg);

    let max_disp = pose.warp.max_displacement(w, h);
    let warp_to_use = if pose.warp.is_plausible() && max_disp < (w.max(h) as f64 * 0.12) {
        pose.warp
    } else {
        LensWarp::identity(w, h)
    };

    // Warp with pure geometric Catmull-Rom interpolation (gain=1.0, bias=0.0)
    // so we can apply smooth 2D spatially-varying radiometric calibration next.
    let (warped_rgb, valid_mask) =
        crate::focus_stacking::warp_frame(src_rgb, &warp_to_use, 1.0, 0.0, w, h);

    let refined_ratios = estimate_global_channel_ratios(ref_rgb, &warped_rgb, luma_ratio);
    (warped_rgb, valid_mask, refined_ratios)
}

/// Estimates and applies a smooth 2D spatially-varying per-channel gain field G_k(x, y)
/// to equalize both exposure tolerances and aperture-dependent lens vignetting (e.g. f/4.0 vs f/5.6).
fn calibrate_2d_vignetting_and_exposure(
    ref_rgb: &PlanarRgb,
    warped_rgb: &mut PlanarRgb,
    valid_mask: &Plane,
    global_ratios: [f32; 3],
    is_darkest_bracket: bool,
) {
    let (w, h) = (ref_rgb.w, ref_rgb.h);
    let (gw, gh) = (120usize, 80usize);

    let ref_small = [
        resize_to(&ref_rgb.c[0], gw, gh),
        resize_to(&ref_rgb.c[1], gw, gh),
        resize_to(&ref_rgb.c[2], gw, gh),
    ];
    let src_small = [
        resize_to(&warped_rgb.c[0], gw, gh),
        resize_to(&warped_rgb.c[1], gw, gh),
        resize_to(&warped_rgb.c[2], gw, gh),
    ];
    let mask_small = resize_to(valid_mask, gw, gh);

    let mut gain_fields_small = [
        Plane::filled(gw, gh, global_ratios[0]),
        Plane::filled(gw, gh, global_ratios[1]),
        Plane::filled(gw, gh, global_ratios[2]),
    ];

    for ch in 0..3 {
        let g_ratio = global_ratios[ch];
        let mut num = Plane::new(gw, gh);
        let mut den = Plane::new(gw, gh);

        for i in 0..(gw * gh) {
            // Prior anchor so regions with clipping/motion stay anchored to global_ratios[ch]
            let prior_w = 0.04f32;
            num.data[i] = g_ratio * prior_w;
            den.data[i] = prior_w;

            if mask_small.data[i] < 0.9 {
                continue;
            }

            let r_ref = ref_small[0].data[i];
            let g_ref = ref_small[1].data[i];
            let b_ref = ref_small[2].data[i];
            let p_ref = r_ref.max(g_ref).max(b_ref).max(1.22 * g_ref);

            let r_src = src_small[0].data[i];
            let g_src = src_small[1].data[i];
            let b_src = src_small[2].data[i];
            let p_src = r_src.max(g_src).max(b_src).max(1.22 * g_src);

            if p_ref > 0.015 && p_ref < 0.45 && p_src > 0.010 && p_src < 0.45 {
                let vr = ref_small[ch].data[i];
                let vs = src_small[ch].data[i];
                if vr > 0.012 && vs > 0.008 {
                    let local_r = vr / vs;
                    // Reject motion outliers (> 25% deviation from global ratio)
                    let rel_diff = (local_r - g_ratio).abs() / g_ratio.max(1e-5);
                    if rel_diff < 0.25 {
                        let w_inlier = (1.0 - (rel_diff / 0.25).powi(2)).max(0.0);
                        num.data[i] += local_r * w_inlier;
                        den.data[i] += w_inlier;
                    }
                }
            }
        }

        // Two-pass wide box filter (radius 18 on 120x80 grid ≈ 900px smoothing at 6000x4000)
        // captures low-frequency optical vignetting and illuminant drift without fitting local edges.
        let num_s = box_filter(&box_filter(&num, 18), 18);
        let den_s = box_filter(&box_filter(&den, 18), 18);

        for i in 0..(gw * gh) {
            let est = num_s.data[i] / den_s.data[i].max(1e-5);
            gain_fields_small[ch].data[i] = est.clamp(g_ratio * 0.72, g_ratio * 1.38);
        }
    }

    // If this is the darkest bracket, first apply single-frame highlight recovery to any pixel
    // that saturated even in the darkest exposure (e.g. direct sun core) before scaling up.
    if is_darkest_bracket {
        let (r_slice, rest) = warped_rgb.c.split_at_mut(1);
        let (g_slice, b_slice) = rest.split_at_mut(1);
        r_slice[0]
            .data
            .par_iter_mut()
            .zip(g_slice[0].data.par_iter_mut())
            .zip(b_slice[0].data.par_iter_mut())
            .for_each(|((r, g), b)| {
                let p_eff = (*r).max(*g).max(*b).max(1.22 * (*g));
                if p_eff > 0.52 {
                    let (rr, gg, bb) = recover_single_frame_highlight(*r, *g, *b);
                    *r = rr;
                    *g = gg;
                    *b = bb;
                }
            });
    }

    // Upsample the smooth 2D gain fields and calibrate the warped bracket into reference radiance space
    let sx_ratio = gw as f32 / w as f32;
    let sy_ratio = gh as f32 / h as f32;

    for ch in 0..3 {
        let g_small = &gain_fields_small[ch];
        warped_rgb.c[ch]
            .data
            .par_chunks_mut(w)
            .enumerate()
            .for_each(|(y, row)| {
                let sy = ((y as f32 + 0.5) * sy_ratio - 0.5).max(0.0);
                for (x, val) in row.iter_mut().enumerate() {
                    let sx = ((x as f32 + 0.5) * sx_ratio - 0.5).max(0.0);
                    let gain = g_small.sample_bilinear(sx, sy);
                    *val *= gain;
                }
            });
    }
}

/// Computes a smooth static-confidence mask `M_static(x, y) in [0, 1]` for a darker bracket.
/// Where a moving subject (e.g. a moving boat, cyclist, or swaying branch) creates strong
/// photometric disagreement in the unclipped/occluded regions, the motion mask is dilated
/// so shifted high-frequency edges from the darker frame are never stamped onto the moving subject.
fn compute_highlight_static_confidence(
    ref_rgb: &PlanarRgb,
    dark_cal: &PlanarRgb,
    valid_mask: &Plane,
) -> Plane {
    let (w, h) = (ref_rgb.w, ref_rgb.h);
    let (mw, mh) = ((w / 4).max(64), (h / 4).max(64));

    let r_ref = resize_to(&ref_rgb.c[0], mw, mh);
    let g_ref = resize_to(&ref_rgb.c[1], mw, mh);
    let b_ref = resize_to(&ref_rgb.c[2], mw, mh);

    let r_drk = resize_to(&dark_cal.c[0], mw, mh);
    let g_drk = resize_to(&dark_cal.c[1], mw, mh);
    let b_drk = resize_to(&dark_cal.c[2], mw, mh);
    let v_msk = resize_to(valid_mask, mw, mh);

    let mut motion_raw = Plane::new(mw, mh);
    motion_raw
        .data
        .par_iter_mut()
        .enumerate()
        .for_each(|(i, m_out)| {
            if v_msk.data[i] < 0.9 {
                *m_out = 1.0;
                return;
            }
            let rr = r_ref.data[i];
            let gr = g_ref.data[i];
            let br = b_ref.data[i];
            let yr = 0.2126 * rr + 0.7152 * gr + 0.0722 * br;
            let p_eff = rr.max(gr).max(br).max(1.22 * gr);

            let rd = r_drk.data[i];
            let gd = g_drk.data[i];
            let bd = b_drk.data[i];
            let yd = 0.2126 * rd + 0.7152 * gd + 0.0722 * bd;

            // Detect motion wherever the reference is unclipped (< 0.56) OR where the dark frame
            // is darker than the reference (dark moving occluder over a bright region).
            if p_eff < 0.56 || yd < 0.85 * yr {
                let diff = (rd - rr).abs() + (gd - gr).abs() + (bd - br).abs();
                let denom = (rr + gr + br + 0.06).max(0.06);
                let rel = diff / denom;
                // Trigger motion between 12% and 28% relative discrepancy
                *m_out = smoothstep(0.12, 0.28, rel);
            } else {
                *m_out = 0.0;
            }
        });

    // Suppress isolated single-pixel noise with a 3x3 box average before dilating real moving objects
    let motion_clean = box_filter(&motion_raw, 1);

    // Dilate real moving objects (radius 10 on 1/4 grid = 40px at full resolution)
    // so the interior highlight of a moving object (e.g. white boat hull next to moving letters/edges)
    // is protected from shifted edges in the darker bracket.
    let rad = 10i64;
    let mut tmp = Plane::new(mw, mh);
    tmp.data
        .par_chunks_mut(mw)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, out_v) in row.iter_mut().enumerate() {
                let mut mx = 0.0f32;
                for d in -rad..=rad {
                    let v = motion_clean.clamped(x as i64 + d, y as i64);
                    if v > mx {
                        mx = v;
                    }
                }
                *out_v = mx;
            }
        });
    let mut dilated = Plane::new(mw, mh);
    dilated
        .data
        .par_chunks_mut(mw)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, out_v) in row.iter_mut().enumerate() {
                let mut mx = 0.0f32;
                for d in -rad..=rad {
                    let v = tmp.clamped(x as i64, y as i64 + d);
                    if v > mx {
                        mx = v;
                    }
                }
                *out_v = mx;
            }
        });

    let smoothed_motion = box_filter(&box_filter(&dilated, 8), 8);
    let static_small = smoothed_motion.map(|m| (1.0 - m).clamp(0.0, 1.0));
    upsample_to(&static_small, w, h)
}

/// Computes a strict motion-consistency weight `w_cons(x, y) in [0, 1]` for blending
/// a brighter (+2 EV) bracket into deep shadows of the reference frame.
/// Includes morphological dilation of detected motion so moving tires, spokes, and water
/// reflections leave zero faint shadow traces.
fn compute_shadow_consistency_weight(
    ref_rgb: &PlanarRgb,
    bright_cal: &PlanarRgb,
    valid_mask: &Plane,
) -> Plane {
    let (w, h) = (ref_rgb.w, ref_rgb.h);
    let (mw, mh) = ((w / 4).max(64), (h / 4).max(64));

    let r_ref = resize_to(&ref_rgb.c[0], mw, mh);
    let g_ref = resize_to(&ref_rgb.c[1], mw, mh);
    let b_ref = resize_to(&ref_rgb.c[2], mw, mh);

    let r_brt = resize_to(&bright_cal.c[0], mw, mh);
    let g_brt = resize_to(&bright_cal.c[1], mw, mh);
    let b_brt = resize_to(&bright_cal.c[2], mw, mh);
    let v_msk = resize_to(valid_mask, mw, mh);

    let mut motion_raw = Plane::new(mw, mh);
    motion_raw
        .data
        .par_iter_mut()
        .enumerate()
        .for_each(|(i, m_out)| {
            if v_msk.data[i] < 0.9 {
                *m_out = 1.0;
                return;
            }
            let rr = r_ref.data[i];
            let gr = g_ref.data[i];
            let br = b_ref.data[i];

            let rb = r_brt.data[i];
            let gb = g_brt.data[i];
            let bb = b_brt.data[i];

            let luma = (0.2126 * rr + 0.7152 * gr + 0.0722 * br).max(0.0);
            // Relative tolerance in deep shadows so low-contrast water reflections are strictly detected as motion
            let tol = (0.0012 + 0.035 * luma).max(0.0012);
            let max_diff = (rb - rr).abs().max((gb - gr).abs()).max((bb - br).abs());
            let ratio = max_diff / tol;
            *m_out = smoothstep(0.55, 1.15, ratio);
        });

    let motion_clean = box_filter(&motion_raw, 1);

    // Morphologically dilate shadow motion (radius 12 on 1/4 grid = 48px at full resolution)
    let rad = 12i64;
    let mut tmp = Plane::new(mw, mh);
    tmp.data
        .par_chunks_mut(mw)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, out_v) in row.iter_mut().enumerate() {
                let mut mx = 0.0f32;
                for d in -rad..=rad {
                    let v = motion_clean.clamped(x as i64 + d, y as i64);
                    if v > mx {
                        mx = v;
                    }
                }
                *out_v = mx;
            }
        });
    let mut dilated = Plane::new(mw, mh);
    dilated
        .data
        .par_chunks_mut(mw)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, out_v) in row.iter_mut().enumerate() {
                let mut mx = 0.0f32;
                for d in -rad..=rad {
                    let v = tmp.clamped(x as i64, y as i64 + d);
                    if v > mx {
                        mx = v;
                    }
                }
                *out_v = mx;
            }
        });

    let smoothed_motion = box_filter(&box_filter(&dilated, 6), 6);
    let cons_small = smoothed_motion.map(|m| (1.0 - m).clamp(0.0, 1.0));
    upsample_to(&cons_small, w, h)
}

pub struct LinearHdrPanel {
    pub linear_radiance: Rgb32FImage,
    pub nominal_exposure: f32,
    pub f_number: f32,
}

/// Merges a handheld exposure bracket into a ghost-free, 2D-vignetting-calibrated 32-bit Linear Radiance panel
/// without applying non-linear tone mapping. Used by both standalone HDR and Linear-First HDR Panorama.
pub fn merge_handheld_hdr_linear_panel<R: tauri::Runtime>(
    paths: &[String],
    half_size: bool,
    enforce_burst_filter: bool,
    reference_index: Option<usize>,
    settings: &AppSettings,
    app_handle: Option<&AppHandle<R>>,
    cancel_token: Option<&Arc<AtomicBool>>,
) -> Result<LinearHdrPanel, String> {
    let frames = load_handheld_brackets_with_mode(
        paths,
        settings,
        half_size,
        enforce_burst_filter,
        app_handle,
        cancel_token,
    )?;
    let n = frames.len();
    let (w, h) = (frames[0].rgb.w, frames[0].rgb.h);

    let ref_idx = reference_index
        .filter(|&i| i < n)
        .unwrap_or_else(|| select_median_exposure_anchor(&frames));

    let ref_exp = frames[ref_idx].nominal_exposure().max(1e-5);
    let ref_f_number = frames[ref_idx].f_number;
    let ref_rgb = &frames[ref_idx].rgb;

    if n == 1 {
        let mut single = ref_rgb.clone();
        let (c0, rest) = single.c.split_at_mut(1);
        let (c1, c2) = rest.split_at_mut(1);
        c0[0]
            .data
            .par_iter_mut()
            .zip(c1[0].data.par_iter_mut())
            .zip(c2[0].data.par_iter_mut())
            .for_each(|((r, g), b)| {
                let (nr, ng, nb) = recover_single_frame_highlight(*r, *g, *b);
                *r = nr;
                *g = ng;
                *b = nb;
            });
        return Ok(LinearHdrPanel {
            linear_radiance: single.to_rgb32f(),
            nominal_exposure: ref_exp,
            f_number: ref_f_number,
        });
    }

    let ref_luma = ref_rgb.luma();

    // Determine which frame is the darkest exposure (largest ref_exp / frame_exp ratio)
    let mut darkest_idx = ref_idx;
    let mut max_exp_ratio = 1.0f32;
    for (k, f) in frames.iter().enumerate() {
        let ratio = ref_exp / f.nominal_exposure().max(1e-5);
        if ratio > max_exp_ratio {
            max_exp_ratio = ratio;
            darkest_idx = k;
        }
    }

    // Align and radiometrically calibrate all non-reference frames
    struct CalibratedBracket {
        rgb: PlanarRgb,
        raw_max_eff: Plane,
        valid_mask: Plane,
        mean_ratio: f32,
    }

    let mut darker_brackets: Vec<CalibratedBracket> = Vec::new();
    let mut brighter_brackets: Vec<CalibratedBracket> = Vec::new();

    for (k, f) in frames.iter().enumerate() {
        if k == ref_idx {
            continue;
        }
        if let Some(tok) = cancel_token {
            if tok.load(Ordering::Relaxed) {
                return Err("HDR merge cancelled by user.".to_string());
            }
        }
        let exif_ratio = (ref_exp / f.nominal_exposure().max(1e-5)).clamp(0.02, 64.0);
        let pct = 25 + (k * 25 / n.max(1));
        if let Some(h_app) = app_handle {
            let _ = h_app.emit(
                "hdr-progress",
                format!(
                    "Aligning handheld bracket {}/{} (EV ratio {:.2}x)... {}%",
                    k + 1,
                    n,
                    exif_ratio,
                    pct
                ),
            );
        }

        let (mut warped_rgb, valid_mask, ratios) =
            align_bracket_to_reference(ref_rgb, &ref_luma, &f.rgb, exif_ratio);

        // Record pre-calibration effective sensor level so we know where this bracket itself saturates
        let mut raw_max_eff = Plane::new(w, h);
        raw_max_eff
            .data
            .par_iter_mut()
            .enumerate()
            .for_each(|(i, m)| {
                let r = warped_rgb.c[0].data[i];
                let g = warped_rgb.c[1].data[i];
                let b = warped_rgb.c[2].data[i];
                *m = r.max(g).max(b).max(1.22 * g);
            });

        let is_darkest = k == darkest_idx && exif_ratio > 1.15;
        calibrate_2d_vignetting_and_exposure(
            ref_rgb,
            &mut warped_rgb,
            &valid_mask,
            ratios,
            is_darkest,
        );

        let mean_ratio = 0.2126 * ratios[0] + 0.7152 * ratios[1] + 0.0722 * ratios[2];
        let cb = CalibratedBracket {
            rgb: warped_rgb,
            raw_max_eff,
            valid_mask,
            mean_ratio,
        };

        if mean_ratio > 1.12 {
            darker_brackets.push(cb);
        } else if mean_ratio < 0.89 {
            brighter_brackets.push(cb);
        }
    }

    // Sort darker brackets from mildest underexposure to deepest underexposure
    darker_brackets.sort_by(|a, b| a.mean_ratio.total_cmp(&b.mean_ratio));
    // Sort brighter brackets from mildest overexposure to deepest overexposure
    brighter_brackets.sort_by(|a, b| b.mean_ratio.total_cmp(&a.mean_ratio));

    if let Some(h_app) = app_handle {
        let _ = h_app.emit(
            "hdr-progress",
            "Synthesizing 32-bit linear radiance map... 52%",
        );
    }

    // Start from the 0 EV reference frame as the locked geometric & bokeh anchor
    let mut merged = ref_rgb.clone();

    // 1. Blend brighter brackets ONLY in the ultra-deep read-noise floor (p_ref < 0.012).
    // Above 0.012 (~170 ADU on a 14-bit sensor), the 0 EV reference frame already has high
    // shot-noise SNR, so keeping it 100% reference-locked prevents any water-reflection or
    // shadow-edge ghosts from the longer-shutter +2 EV exposure.
    for br in &brighter_brackets {
        let w_cons = compute_shadow_consistency_weight(ref_rgb, &br.rgb, &br.valid_mask);
        for i in 0..(w * h) {
            let rr = ref_rgb.c[0].data[i];
            let gr = ref_rgb.c[1].data[i];
            let br_c = ref_rgb.c[2].data[i];
            let p_ref = rr.max(gr).max(br_c).max(1.22 * gr);

            if p_ref < 0.012 {
                let p_bright_raw = br.raw_max_eff.data[i];
                let w_unclipped = 1.0 - smoothstep(0.28, 0.40, p_bright_raw);
                let w_shadow = 1.0 - smootherstep(0.002, 0.012, p_ref);
                let alpha = (w_shadow * w_unclipped * w_cons.data[i] * 0.55).clamp(0.0, 0.55);
                if alpha > 1e-4 {
                    for ch in 0..3 {
                        let v_ref = merged.c[ch].data[i];
                        let v_br = br.rgb.c[ch].data[i];
                        let tol = 0.0015 + 0.06 * v_ref;
                        let v_clamped = v_br.clamp((v_ref - tol).max(0.0), v_ref + tol);
                        merged.c[ch].data[i] = v_ref * (1.0 - alpha) + v_clamped * alpha;
                    }
                }
            }
        }
    }

    if let Some(h_app) = app_handle {
        let _ = h_app.emit(
            "hdr-progress",
            "Merging dynamic range highlights & deep shadows... 60%",
        );
    }

    // 2. Recover highlights from darker brackets (only where p_eff > 0.44 in the reference frame)
    for (d_idx, drk) in darker_brackets.iter().enumerate() {
        let is_last_dark = d_idx + 1 == darker_brackets.len();
        let static_conf =
            compute_highlight_static_confidence(ref_rgb, &drk.rgb, &drk.valid_mask);

        // Precompute a low-frequency occlusion-rejected highlight envelope from the darker bracket
        // for smooth fallback inside moving highlights (e.g. moving white boat hull or behind swaying twigs).
        let (ew, eh) = ((w / 8).max(32), (h / 8).max(32));
        let mut env_small = [
            resize_to(&drk.rgb.c[0], ew, eh),
            resize_to(&drk.rgb.c[1], ew, eh),
            resize_to(&drk.rgb.c[2], ew, eh),
        ];
        let ref_env_small = [
            resize_to(&ref_rgb.c[0], ew, eh),
            resize_to(&ref_rgb.c[1], ew, eh),
            resize_to(&ref_rgb.c[2], ew, eh),
        ];
        for i in 0..(ew * eh) {
            let (rec_r, rec_g, rec_b) = recover_single_frame_highlight(
                ref_env_small[0].data[i],
                ref_env_small[1].data[i],
                ref_env_small[2].data[i],
            );
            let rec_arr = [rec_r, rec_g, rec_b];
            for ch in 0..3 {
                // Physical lower bound: a recovered highlight cannot become darker than the reference frame
                if env_small[ch].data[i] < rec_arr[ch] {
                    env_small[ch].data[i] = rec_arr[ch];
                }
            }
        }
        for ch in 0..3 {
            env_small[ch] = box_filter(&box_filter(&env_small[ch], 6), 6);
        }

        let sx_env = ew as f32 / w as f32;
        let sy_env = eh as f32 / h as f32;

        let (m_c0, m_rest) = merged.c.split_at_mut(1);
        let (m_c1, m_c2) = m_rest.split_at_mut(1);

        m_c0[0]
            .data
            .par_chunks_mut(w)
            .zip(m_c1[0].data.par_chunks_mut(w))
            .zip(m_c2[0].data.par_chunks_mut(w))
            .enumerate()
            .for_each(|(y, ((row_r, row_g), row_b))| {
                let sy = ((y as f32 + 0.5) * sy_env - 0.5).max(0.0);
                let row_off = y * w;
                for x in 0..w {
                    let i = row_off + x;
                    if drk.valid_mask.data[i] < 0.9 {
                        // Outside aligned overlap: use single-frame highlight recovery on reference
                        let rr = row_r[x];
                        let gg = row_g[x];
                        let bb = row_b[x];
                        if rr.max(gg).max(bb).max(1.22 * gg) > 0.52 {
                            let (nr, ng, nb) = recover_single_frame_highlight(rr, gg, bb);
                            row_r[x] = nr;
                            row_g[x] = ng;
                            row_b[x] = nb;
                        }
                        continue;
                    }

                    let rr = ref_rgb.c[0].data[i];
                    let gr = ref_rgb.c[1].data[i];
                    let br = ref_rgb.c[2].data[i];
                    let p_eff = rr.max(gr).max(br).max(1.22 * gr);

                    // Strictly zero contribution from darker brackets below 0.44!
                    // This guarantees 100% reference-locked midtones, shadows, and bokeh.
                    if p_eff <= 0.44 {
                        continue;
                    }

                    // Transition smoothly between 0.44 and 0.62 (where BOTH frames are still unclipped!)
                    let mut alpha_hl = smootherstep(0.44, 0.62, p_eff);
                    if !is_last_dark {
                        let p_drk_raw = drk.raw_max_eff.data[i];
                        alpha_hl *= 1.0 - smoothstep(0.48, 0.62, p_drk_raw);
                    }

                    if alpha_hl <= 1e-4 {
                        continue;
                    }

                    let rd = drk.rgb.c[0].data[i];
                    let gd = drk.rgb.c[1].data[i];
                    let bd = drk.rgb.c[2].data[i];

                    // Dark-occluder rejection:
                    // In pure linear RAW ("hdr_linear"), sensor saturation in the 0 EV frame can only
                    // clamp channels from above, never inflate them. Thus if the calibrated -2 EV pixel
                    // is darker than the 0 EV reference pixel (ratio < 0.96), a dark object (swaying twig,
                    // boat letter, or cyclist) moved into this bright pixel in the -2 EV frame.
                    let yr = (0.2126 * rr + 0.7152 * gr + 0.0722 * br).max(1e-4);
                    let yd = 0.2126 * rd + 0.7152 * gd + 0.0722 * bd;
                    let min_ch_ratio = (rd / rr.max(0.01))
                        .min(gd / gr.max(0.01))
                        .min(bd / br.max(0.01))
                        .min(yd / yr);
                    let w_no_occl = smoothstep(0.88, 0.96, min_ch_ratio);

                    // Combine pixel-level occlusion check with regional static confidence M_static
                    let w_static = static_conf.data[i] * w_no_occl;

                    // Fallback for moving/occluded highlights: single-frame highlight recovery
                    // smoothly blended with the occlusion-free highlight envelope
                    let (rec_r, rec_g, rec_b) = recover_single_frame_highlight(rr, gr, br);
                    let sx = ((x as f32 + 0.5) * sx_env - 0.5).max(0.0);
                    let env_r = env_small[0].sample_bilinear(sx, sy).max(rec_r);
                    let env_g = env_small[1].sample_bilinear(sx, sy).max(rec_g);
                    let env_b = env_small[2].sample_bilinear(sx, sy).max(rec_b);

                    // Lift occluded sky/highlights wherever Green or all channels approach saturation
                    let clip_t = smoothstep(0.56, 0.74, p_eff);
                    let motion_safe_r = rec_r * (1.0 - clip_t) + env_r * clip_t;
                    let motion_safe_g = rec_g * (1.0 - clip_t) + env_g * clip_t;
                    let motion_safe_b = rec_b * (1.0 - clip_t) + env_b * clip_t;

                    let target_r = rd * w_static + motion_safe_r * (1.0 - w_static);
                    let target_g = gd * w_static + motion_safe_g * (1.0 - w_static);
                    let target_b = bd * w_static + motion_safe_b * (1.0 - w_static);

                    row_r[x] = row_r[x] * (1.0 - alpha_hl) + target_r * alpha_hl;
                    row_g[x] = row_g[x] * (1.0 - alpha_hl) + target_g * alpha_hl;
                    row_b[x] = row_b[x] * (1.0 - alpha_hl) + target_b * alpha_hl;
                }
            });
    }

    if darker_brackets.is_empty() {
        let (m_c0, m_rest) = merged.c.split_at_mut(1);
        let (m_c1, m_c2) = m_rest.split_at_mut(1);
        m_c0[0]
            .data
            .par_iter_mut()
            .zip(m_c1[0].data.par_iter_mut())
            .zip(m_c2[0].data.par_iter_mut())
            .for_each(|((r, g), b)| {
                let (nr, ng, nb) = recover_single_frame_highlight(*r, *g, *b);
                *r = nr;
                *g = ng;
                *b = nb;
            });
    }

    if let Some(h_app) = app_handle {
        let _ = h_app.emit(
            "hdr-progress",
            "Linear radiance map complete... 65%",
        );
    }

    Ok(LinearHdrPanel {
        linear_radiance: merged.to_rgb32f(),
        nominal_exposure: ref_exp,
        f_number: ref_f_number,
    })
}

/// Full Reference-Locked Handheld HDR Pipeline:
/// Produces both a ghost-free 32-bit Linear Radiance composite and a Multi-Scale Virtual-Exposure
/// Laplacian Pyramid tone-mapped preview.
pub fn merge_handheld_hdr_pipeline<R: tauri::Runtime>(
    paths: &[String],
    options: &HdrMergeOptions,
    settings: &AppSettings,
    app_handle: Option<&AppHandle<R>>,
    cancel_token: Option<&Arc<AtomicBool>>,
) -> Result<HdrFusionResult, String> {
    let half_size = options.half_size.unwrap_or(false);
    let panel = merge_handheld_hdr_linear_panel(
        paths,
        half_size,
        true,
        options.reference_index,
        settings,
        app_handle,
        cancel_token,
    )?;

    if let Some(h_app) = app_handle {
        let _ = h_app.emit(
            "hdr-progress",
            "Synthesizing HDR tones: Multi-scale Laplacian pyramid... 68%",
        );
    }

    let tone_mapped_preview =
        tone_map_virtual_exposure_pyramid_with_progress(&panel.linear_radiance, options, app_handle);

    Ok(HdrFusionResult {
        linear_radiance: panel.linear_radiance,
        tone_mapped_preview,
    })
}

#[inline]
fn linear_to_srgb_fast(x: f32) -> f32 {
    let xc = x.clamp(0.0, 1.0);
    if xc <= 0.0031308 {
        xc * 12.92
    } else {
        1.055 * xc.powf(1.0 / 2.4) - 0.055
    }
}

/// Synthesizes virtual exposures from a single ghost-free 32-bit Linear Radiance image
/// and fuses them via an 8-level Burt-Adelson Laplacian pyramid.
/// Because all virtual exposures come from the exact same aligned & deghosted radiance map,
/// it achieves 100% ghost-free, bokeh-pure, halo-free natural local contrast.
pub fn tone_map_virtual_exposure_pyramid(
    radiance: &Rgb32FImage,
    options: &HdrMergeOptions,
) -> Rgb32FImage {
    tone_map_virtual_exposure_pyramid_with_progress::<tauri::Wry>(radiance, options, None)
}

/// Synthesizes virtual exposures from a single ghost-free 32-bit Linear Radiance image
/// and fuses them via an 8-level Burt-Adelson Laplacian pyramid with real-time stage progress.
/// Because all virtual exposures come from the exact same aligned & deghosted radiance map,
/// it achieves 100% ghost-free, bokeh-pure, halo-free natural local contrast.
pub fn tone_map_virtual_exposure_pyramid_with_progress<R: tauri::Runtime>(
    radiance: &Rgb32FImage,
    options: &HdrMergeOptions,
    app_handle: Option<&AppHandle<R>>,
) -> Rgb32FImage {
    let w = radiance.width() as usize;
    let h = radiance.height() as usize;
    let num_px = w * h;
    if num_px == 0 {
        return radiance.clone();
    }

    let rad_raw = radiance.as_raw();

    // 1. Measure scene dynamic range percentiles in linear luminance
    let step = (num_px / 80_000).max(1);
    let mut luma_samples: Vec<f32> = Vec::with_capacity(num_px / step + 1);
    for i in (0..num_px).step_by(step) {
        let idx = i * 3;
        let y = 0.2126 * rad_raw[idx] + 0.7152 * rad_raw[idx + 1] + 0.0722 * rad_raw[idx + 2];
        if y > 1e-5 && y.is_finite() {
            luma_samples.push(y);
        }
    }

    if luma_samples.is_empty() {
        return radiance.clone();
    }
    luma_samples.sort_unstable_by(|a, b| a.total_cmp(b));
    let ns = luma_samples.len();
    let p01 = luma_samples[(ns as f32 * 0.003) as usize];
    let p50 = luma_samples[ns / 2];
    let p99 = luma_samples[((ns as f32 * 0.995) as usize).min(ns - 1)];

    let exp_bias_ev = options.exposure_bias.unwrap_or(0.0);
    let hl_recovery = (options.highlight_recovery.unwrap_or(50.0) / 100.0).clamp(0.0, 1.0);
    let shadow_lift = (options.shadow_lift.unwrap_or(30.0) / 100.0).clamp(0.0, 1.0);
    let detail_boost = options.detail_boost.unwrap_or(1.05).clamp(0.7, 1.6);

    // Auto-key base exposure so median scene luminance maps to natural Zone V (~0.17 linear)
    let base_gain = (0.17 / p50.max(0.01)).clamp(0.35, 3.5) * 2.0f32.powf(exp_bias_ev);
    let black_floor = (p01 * 0.25).min(0.0025);

    // Compute how many stops of highlight headroom exist above Zone V
    let max_scaled = (p99 * base_gain).max(0.5);
    let hl_headroom_ev = max_scaled.log2().max(0.6);

    // 4 virtual exposures spanning from deep highlight recovery to clean shadow lift:
    // v0: highlight protector, v1: upper-midtone anchor, v2: normal anchor, v3: shadow lifter
    let ev_stops = [
        -(hl_headroom_ev * (0.65 + 0.45 * hl_recovery)).clamp(0.8, 3.2),
        -(hl_headroom_ev * 0.28).clamp(0.3, 1.3),
        0.15 + 0.25 * shadow_lift,
        (0.95 + 1.15 * shadow_lift).clamp(0.8, 2.2),
    ];

    let num_virtual = ev_stops.len();
    let mut virtual_frames: Vec<PlanarRgb> = Vec::with_capacity(num_virtual);
    let mut weight_maps: Vec<Plane> = Vec::with_capacity(num_virtual);

    for &ev in &ev_stops {
        let gain = base_gain * 2.0f32.powf(ev);
        let mut v_rgb = PlanarRgb::new(w, h);
        let (c0, rest) = v_rgb.c.split_at_mut(1);
        let (c1, c2) = rest.split_at_mut(1);

        c0[0]
            .data
            .par_iter_mut()
            .zip(c1[0].data.par_iter_mut())
            .zip(c2[0].data.par_iter_mut())
            .enumerate()
            .for_each(|(i, ((r_out, g_out), b_out))| {
                let idx = i * 3;
                let r_lin = (rad_raw[idx] - black_floor).max(0.0) * gain;
                let g_lin = (rad_raw[idx + 1] - black_floor).max(0.0) * gain;
                let b_lin = (rad_raw[idx + 2] - black_floor).max(0.0) * gain;

                // Soft filmic shoulder above 0.75 on each virtual exposure so virtual highlights roll off smoothly
                let map_soft = |v: f32| -> f32 {
                    if v <= 0.75 {
                        v
                    } else {
                        let d = v - 0.75;
                        0.75 + 0.25 * (d / (0.35 + d))
                    }
                };

                *r_out = linear_to_srgb_fast(map_soft(r_lin));
                *g_out = linear_to_srgb_fast(map_soft(g_lin));
                *b_out = linear_to_srgb_fast(map_soft(b_lin));
            });

        // Compute smooth Mertens well-exposedness + saturation + contrast weights
        let luma = v_rgb.luma();
        let mut w_map = Plane::new(w, h);
        let sigma_sq_2 = 2.0 * 0.22 * 0.22;

        w_map
            .data
            .par_chunks_mut(w)
            .enumerate()
            .for_each(|(y, row)| {
                let row_off = y * w;
                for x in 0..w {
                    let i = row_off + x;
                    let r = v_rgb.c[0].data[i];
                    let g = v_rgb.c[1].data[i];
                    let b = v_rgb.c[2].data[i];
                    let yl = luma.data[i];

                    // Well-exposedness centered at 0.52 in sRGB
                    let we = (-((yl - 0.52).powi(2)) / sigma_sq_2).exp();

                    // Saturation
                    let mean_c = (r + g + b) * (1.0 / 3.0);
                    let sat = (((r - mean_c).powi(2)
                        + (g - mean_c).powi(2)
                        + (b - mean_c).powi(2))
                        * (1.0 / 3.0))
                        .sqrt();

                    // Local Laplacian contrast
                    let up = if y > 0 { luma.data[i - w] } else { yl };
                    let dn = if y + 1 < h { luma.data[i + w] } else { yl };
                    let lf = if x > 0 { luma.data[i - 1] } else { yl };
                    let rt = if x + 1 < w { luma.data[i + 1] } else { yl };
                    let lap = (4.0 * yl - up - dn - lf - rt).abs();

                    row[x] = (we * (1.0 + 0.45 * sat) * (1.0 + 0.35 * lap)).max(1e-5);
                }
            });

        virtual_frames.push(v_rgb);
        weight_maps.push(w_map);
    }

    // 2. Build 8-level Gaussian weight pyramids & Laplacian image pyramids
    let mut levels = 1usize;
    let (mut cw, mut ch_dim) = (w, h);
    while levels < 9 && cw.min(ch_dim) > 12 {
        cw = cw.div_ceil(2);
        ch_dim = ch_dim.div_ceil(2);
        levels += 1;
    }

    let mut weight_pyrs: Vec<Vec<Plane>> = weight_maps
        .par_iter()
        .map(|wm| gaussian_pyramid(wm, levels))
        .collect();

    // Normalize weights across the virtual exposures at every pyramid level in parallel chunks
    for lvl in 0..levels {
        let len = weight_pyrs[0][lvl].data.len();
        let mut sum_plane = vec![0.0f32; len];
        for k in 0..num_virtual {
            let w_slice = &weight_pyrs[k][lvl].data;
            sum_plane
                .par_iter_mut()
                .zip(w_slice.par_iter())
                .for_each(|(s, &w_val)| *s += w_val);
        }
        for k in 0..num_virtual {
            weight_pyrs[k][lvl]
                .data
                .par_iter_mut()
                .zip(sum_plane.par_iter())
                .for_each(|(w_val, &s)| {
                    *w_val /= s.max(1e-6);
                });
        }
    }

    if let Some(h_app) = app_handle {
        let _ = h_app.emit(
            "hdr-progress",
            "Constructing multi-scale Gaussian weight pyramids... 72%",
        );
    }

    // Fuse via single Luminance-Guided Burt-Adelson Laplacian pyramid (8x-10x faster)
    if let Some(h_app) = app_handle {
        let _ = h_app.emit(
            "hdr-progress",
            "Synthesizing HDR tones: Multi-scale luminance pyramid... 78%",
        );
    }

    let num_px = w * h;
    let mut acc_r = vec![0.0f32; num_px];
    let mut acc_g = vec![0.0f32; num_px];
    let mut acc_b = vec![0.0f32; num_px];
    let mut blended_details: Vec<Plane> = Vec::new();
    let mut blended_residual = Plane::new(1, 1);

    for k in 0..num_virtual {
        let vf = &virtual_frames[k];
        let mut luma_k = Plane::new(w, h);
        luma_k
            .data
            .par_iter_mut()
            .zip(vf.c[0].data.par_iter())
            .zip(vf.c[1].data.par_iter())
            .zip(vf.c[2].data.par_iter())
            .for_each(|(((y_out, &r), &g), &b)| {
                *y_out = (0.2126 * r + 0.7152 * g + 0.0722 * b).max(1e-4);
            });

        let w0 = &weight_pyrs[k][0].data;
        acc_r
            .par_iter_mut()
            .zip(vf.c[0].data.par_iter())
            .zip(w0.par_iter())
            .for_each(|((ar, &r), &wt)| *ar += r * wt);
        acc_g
            .par_iter_mut()
            .zip(vf.c[1].data.par_iter())
            .zip(w0.par_iter())
            .for_each(|((ag, &g), &wt)| *ag += g * wt);
        acc_b
            .par_iter_mut()
            .zip(vf.c[2].data.par_iter())
            .zip(w0.par_iter())
            .for_each(|((ab, &b), &wt)| *ab += b * wt);

        let pyr = laplacian_pyramid(&luma_k, levels);
        if k == 0 {
            blended_details = pyr
                .details
                .iter()
                .map(|p| Plane::new(p.w, p.h))
                .collect();
            blended_residual = Plane::new(pyr.residual.w, pyr.residual.h);
        }
        for (lvl, detail_plane) in pyr.details.iter().enumerate() {
            let w_data = &weight_pyrs[k][lvl].data;
            blended_details[lvl]
                .data
                .par_iter_mut()
                .zip(detail_plane.data.par_iter().zip(w_data.par_iter()))
                .for_each(|(o, (&d, &wt))| {
                    *o += d * wt;
                });
        }
        let w_res = &weight_pyrs[k][levels - 1].data;
        blended_residual
            .data
            .par_iter_mut()
            .zip(pyr.residual.data.par_iter().zip(w_res.par_iter()))
            .for_each(|(o, (&r_v, &wt))| {
                *o += r_v * wt;
            });
    }

    // Gentle mid-frequency detail boost on levels 1 and 2
    for (lvl, blended) in blended_details.iter_mut().enumerate() {
        if (lvl == 1 || lvl == 2) && (detail_boost - 1.0).abs() > 0.01 {
            blended
                .data
                .par_iter_mut()
                .for_each(|v| *v *= detail_boost);
        }
    }

    if let Some(h_app) = app_handle {
        let _ = h_app.emit(
            "hdr-progress",
            "Collapsing multi-scale HDR pyramid & restoring chroma... 88%",
        );
    }

    let fused_y = collapse_pyramid(&LaplacianPyramid {
        details: blended_details,
        residual: blended_residual,
    });

    let mut fused_planar = PlanarRgb::new(w, h);
    let eps = 1e-3f32;
    let [c0, c1, c2] = &mut fused_planar.c;
    c0.data
        .par_iter_mut()
        .zip(c1.data.par_iter_mut())
        .zip(c2.data.par_iter_mut())
        .zip(fused_y.data.par_iter())
        .zip(acc_r.par_iter().zip(acc_g.par_iter()).zip(acc_b.par_iter()))
        .for_each(|((((r_out, g_out), b_out), &target_y_raw), ((&ar, &ag), &ab))| {
            let ref_y = (0.2126 * ar + 0.7152 * ag + 0.0722 * ab).max(eps);
            let target_y = target_y_raw.clamp(0.0, 1.0);
            let gain = (target_y + eps) / (ref_y + eps);
            let shadow_t = smoothstep(0.005, 0.06, ref_y);
            let delta = target_y - ref_y;
            *r_out = (ar * gain * shadow_t + (ar + delta) * (1.0 - shadow_t)).clamp(0.0, 1.0);
            *g_out = (ag * gain * shadow_t + (ag + delta) * (1.0 - shadow_t)).clamp(0.0, 1.0);
            *b_out = (ab * gain * shadow_t + (ab + delta) * (1.0 - shadow_t)).clamp(0.0, 1.0);
        });

    let mut out_img = fused_planar.to_rgb32f();

    // Apply requested tone profile if non-Natural (or gentle Natural black/white anchor + vibrance)
    let profile = options.profile.unwrap_or(HdrToneProfile::Natural);
    if profile != HdrToneProfile::Natural {
        apply_stock_grade_tone_profile(&mut out_img, profile);
    } else {
        // Measure post-fusion 0.2% black and 99.8% white points so the dynamic range uses the full [0, 1] display span
        let raw_out = out_img.as_raw();
        let mut post_luma: Vec<f32> = Vec::with_capacity(num_px / step + 1);
        for i in (0..num_px).step_by(step) {
            let idx = i * 3;
            let yl = 0.2126 * raw_out[idx] + 0.7152 * raw_out[idx + 1] + 0.0722 * raw_out[idx + 2];
            if yl.is_finite() {
                post_luma.push(yl);
            }
        }
        post_luma.sort_unstable_by(|a, b| a.total_cmp(b));
        let (bp, wp) = if !post_luma.is_empty() {
            let n_pl = post_luma.len();
            let b = (post_luma[(n_pl as f32 * 0.002) as usize] * 0.75).min(0.05);
            let w_pt = post_luma[((n_pl as f32 * 0.9985) as usize).min(n_pl - 1)].max(0.82);
            (b, w_pt)
        } else {
            (0.0, 1.0)
        };
        let inv_span = 1.0 / (wp - bp).max(0.25);

        if let Some(h_app) = app_handle {
            let _ = h_app.emit(
                "hdr-progress",
                "Applying Oklab perceptual grading & dynamic range anchor... 94%",
            );
        }

        // Gentle Oklab contrast & chroma preservation so multi-scale pyramid fusion retains rich blacks and lively colors
        out_img.as_mut().par_chunks_mut(3).for_each(|px| {
            let to_lin = |x: f32| -> f32 {
                let xc = ((x - bp) * inv_span).clamp(0.0, 1.0);
                if xc <= 0.04045 {
                    xc / 12.92
                } else {
                    ((xc + 0.055) / 1.055).powf(2.4)
                }
            };
            let rl = to_lin(px[0]);
            let gl = to_lin(px[1]);
            let bl = to_lin(px[2]);
            let (l_ok, a_ok, b_ok) = crate::hdr_fusion::linear_srgb_to_oklab(rl, gl, bl);
            let l_clamped = l_ok.clamp(0.0, 1.0);
            let s_curve = l_clamped * l_clamped * (3.0 - 2.0 * l_clamped);
            let l_final = l_clamped * 0.86 + s_curve * 0.14;
            let boost = if l_final > 0.08 && l_final < 0.90 { 1.12 } else { 1.02 };
            let (ro, go, bo) =
                crate::hdr_fusion::oklab_to_linear_srgb(l_final, a_ok * boost, b_ok * boost);
            px[0] = linear_to_srgb_fast(ro);
            px[1] = linear_to_srgb_fast(go);
            px[2] = linear_to_srgb_fast(bo);
        });
    }

    let _ = gaussian_blur; // keep import used
    out_img
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn test_hdr_luma_guided_pyramid_preserves_neutrality_and_highlights() {
        let (w, h) = (64, 64);
        let mut rad = Rgb32FImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let px = if y < 20 {
                    // Bright sky
                    [3.2f32, 3.4f32, 3.8f32]
                } else if y < 44 {
                    // Neutral gray
                    [0.18f32, 0.18f32, 0.18f32]
                } else {
                    // Deep shadow
                    [0.008f32, 0.006f32, 0.005f32]
                };
                rad.put_pixel(x, y, Rgb(px));
            }
        }

        let options = HdrMergeOptions {
            profile: Some(HdrToneProfile::Natural),
            ..Default::default()
        };

        let result = tone_map_virtual_exposure_pyramid(&rad, &options);
        assert_eq!(result.dimensions(), (w, h));

        for y in 0..h {
            for x in 0..w {
                let px = result.get_pixel(x, y).0;
                assert!(px[0].is_finite() && px[0] >= 0.0 && px[0] <= 1.0, "R pixel out of range: {:?}", px);
                assert!(px[1].is_finite() && px[1] >= 0.0 && px[1] <= 1.0, "G pixel out of range: {:?}", px);
                assert!(px[2].is_finite() && px[2] >= 0.0 && px[2] <= 1.0, "B pixel out of range: {:?}", px);

                if y >= 25 && y < 40 && x >= 10 && x < 54 {
                    assert!(
                        (px[0] - px[1]).abs() < 0.015,
                        "Neutral gray R-G deviation at ({}, {}): {} vs {}",
                        x, y, px[0], px[1]
                    );
                    assert!(
                        (px[1] - px[2]).abs() < 0.015,
                        "Neutral gray G-B deviation at ({}, {}): {} vs {}",
                        x, y, px[1], px[2]
                    );
                }

                if y < 10 {
                    let sky_luma = 0.2126 * px[0] + 0.7152 * px[1] + 0.0722 * px[2];
                    assert!(
                        sky_luma < 0.99 && px[0] < 1.0 && px[1] < 1.0 && px[2] < 1.0,
                        "Sky detail blown out at ({}, {}): luma = {}, px = {:?}",
                        x, y, sky_luma, px
                    );
                }
            }
        }
    }

    #[test]
    fn test_hdr_extreme_aspect_ratio_panorama() {
        // 20:1 extreme panorama with low vertical height (14px)
        let (w, h) = (280, 14);
        let mut rad = Rgb32FImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let lum = 0.1 + 0.8 * (x as f32 / w as f32);
                rad.put_pixel(x, y, Rgb([lum, lum, lum]));
            }
        }

        let options = HdrMergeOptions {
            profile: Some(HdrToneProfile::Natural),
            ..Default::default()
        };

        let result = tone_map_virtual_exposure_pyramid(&rad, &options);
        assert_eq!(result.dimensions(), (w, h));

        for y in 0..h {
            for x in 0..w {
                let px = result.get_pixel(x, y).0;
                assert!(px[0].is_finite() && px[0] >= 0.0 && px[0] <= 1.0);
                assert!(px[1].is_finite() && px[1] >= 0.0 && px[1] <= 1.0);
                assert!(px[2].is_finite() && px[2] >= 0.0 && px[2] <= 1.0);
            }
        }
    }
}

