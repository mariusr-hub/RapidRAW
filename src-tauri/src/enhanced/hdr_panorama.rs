//! Unified 1-Click HDR Panorama Engine for RapidRAW Pro
//!
//! Merges multi-bracket exposure sets (e.g. 3 brackets x 5 angles = 15 images)
//! directly into 32-bit linear floating-point HDR panels and seamlessly stitches
//! them using Cylindrical, Spherical, or Planar projections.

use std::collections::HashMap;
use std::fs;

use image::{DynamicImage, Rgb32FImage};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::app::settings::load_settings;
use crate::metadata::exif_processing::{read_exposure_time_secs, read_iso, read_f_number};
use crate::library::sidecars::parse_virtual_path;
#[allow(unused_imports)]
use crate::enhanced::hdr_deghosting::{align_hdr_frames, compute_physical_exposure_scale, load_hdr_frames};
use std::time::Duration;
use crate::enhanced::panorama_utils::ImageInfo;
use crate::enhanced::panorama_utils::camera_model::{CameraPose, PanoramaProjection};
use crate::enhanced::panorama_utils::stitching::ray_traced_multiband_stitcher;
use crate::enhanced::sleep_lock::SleepLockGuard;
use crate::app::state::AppState;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HdrPanoramaRequest {
    pub paths: Vec<String>,
    pub projection: PanoramaProjection,
    pub enable_multiband_blending: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExposureBracketGroup {
    pub position_index: usize,
    pub paths: Vec<String>,
}

/// Parses capture timestamp string (YYYY:MM:DD HH:MM:SS) into seconds for temporal grouping
pub fn parse_exif_timestamp(datetime_str: &str) -> Option<u64> {
    let parts: Vec<&str> = datetime_str.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let d_parts: Vec<u32> = parts[0].split(':').filter_map(|s| s.parse().ok()).collect();
    let t_parts: Vec<u32> = parts[1].split(':').filter_map(|s| s.parse().ok()).collect();
    if d_parts.len() == 3 && t_parts.len() == 3 {
        // Approximate epoch seconds calculation for relative delta comparison
        let days = d_parts[0] as u64 * 365 + d_parts[1] as u64 * 30 + d_parts[2] as u64;
        let secs = days * 86400 + t_parts[0] as u64 * 3600 + t_parts[1] as u64 * 60 + t_parts[2] as u64;
        Some(secs)
    } else {
        None
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HdrPanoGroupingReport {
    pub total_files: usize,
    pub panels: Vec<ExposureBracketGroup>,
    pub excluded_outliers: Vec<String>,
    pub is_consistent_bracket_size: bool,
    pub detected_bracket_size: usize,
}

#[derive(Clone, Debug)]
struct BracketItemMeta {
    path: String,
    timestamp: u64,
    physical_scale: f32,
    has_valid_exposure: bool,
}

fn read_real_physical_scale(path: &str) -> Option<f32> {
    let (source_path, _) = parse_virtual_path(path);
    let path_str = source_path.to_string_lossy();
    let sidecar_map = crate::metadata::exif_processing::read_rrexif_sidecar(&source_path);
    let mut exposure_opt = read_exposure_time_secs(&path_str, &[]);
    let mut iso_opt = read_iso(&path_str, &[]);
    let mut f_num_opt = sidecar_map
        .as_ref()
        .and_then(|m| m.get("FNumber").or_else(|| m.get("ApertureValue")))
        .and_then(|s| s.replace("f/", "").replace("F/", "").trim().parse::<f32>().ok())
        .filter(|&v| v > 0.1 && v < 128.0);

    if exposure_opt.is_none() || iso_opt.is_none() || f_num_opt.is_none() {
        if let Ok(mapped) = crate::library::file_ops::read_file_mapped(&source_path) {
            if exposure_opt.is_none() {
                exposure_opt = read_exposure_time_secs(&path_str, &mapped);
            }
            if iso_opt.is_none() {
                iso_opt = read_iso(&path_str, &mapped);
            }
            if f_num_opt.is_none() {
                f_num_opt = read_f_number(&path_str, &mapped);
            }
        }
    }

    let exposure_sec = exposure_opt?;
    let iso = iso_opt.map(|g| g as f32).unwrap_or(100.0);
    let f_number = f_num_opt.unwrap_or(5.6);
    Some(compute_physical_exposure_scale(Duration::from_secs_f32(exposure_sec), iso, f_number))
}

fn groups_have_real_spread(scales: &[Vec<Option<f32>>]) -> bool {
    if scales.len() < 2 {
        return false;
    }
    scales.iter().all(|group| {
        if group.len() < 2 {
            return false;
        }
        let valid: Vec<f32> = group
            .iter()
            .filter_map(|v| v.filter(|&s| s.is_finite() && s > 0.0))
            .collect();
        if valid.len() < 2 {
            return false;
        }
        let min_s = valid.iter().copied().fold(f32::INFINITY, f32::min);
        let max_s = valid.iter().copied().fold(0.0f32, f32::max);
        min_s > 0.0 && (max_s / min_s) >= 1.45
    })
}

pub fn has_real_hdr_exposure_spread(groups: &[ExposureBracketGroup]) -> bool {
    if groups.len() < 2 {
        return false;
    }
    let scales: Vec<Vec<Option<f32>>> = groups
        .iter()
        .map(|g| g.paths.iter().map(|p| read_real_physical_scale(p)).collect())
        .collect();
    groups_have_real_spread(&scales)
}

/// Automatically clusters arbitrary RAW photo selections into HDR exposure bracket groups
/// by physical exposure scale E = (t * ISO) / F^2, temporal burst proximity, and outlier isolation.
pub fn cluster_hdr_brackets_robust(paths: &[String]) -> (Vec<ExposureBracketGroup>, Vec<String>) {
    if paths.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut items: Vec<BracketItemMeta> = Vec::with_capacity(paths.len());

    for path in paths {
        let (source_path, _) = parse_virtual_path(path);
        let path_str = source_path.to_string_lossy();
        let sidecar_map = crate::metadata::exif_processing::read_rrexif_sidecar(&source_path);
        let mut exposure_opt = read_exposure_time_secs(&path_str, &[]);
        let mut iso_opt = read_iso(&path_str, &[]);
        let mut f_num_opt = sidecar_map
            .as_ref()
            .and_then(|m| m.get("FNumber").or_else(|| m.get("ApertureValue")))
            .and_then(|s| s.replace("f/", "").replace("F/", "").trim().parse::<f32>().ok())
            .filter(|&v| v > 0.1 && v < 128.0);
        let has_sidecar_ts = sidecar_map
            .as_ref()
            .and_then(|m| m.get("DateTimeOriginal").or_else(|| m.get("DateTime")))
            .and_then(|s| parse_exif_timestamp(s))
            .is_some();

        let mapped = if exposure_opt.is_none() || iso_opt.is_none() || f_num_opt.is_none() || !has_sidecar_ts {
            crate::library::file_ops::read_file_mapped(&source_path).ok()
        } else {
            None
        };
        let bytes_slice: &[u8] = mapped.as_deref().unwrap_or(&[]);

        if exposure_opt.is_none() {
            exposure_opt = read_exposure_time_secs(&path_str, bytes_slice);
        }
        if iso_opt.is_none() {
            iso_opt = read_iso(&path_str, bytes_slice);
        }
        if f_num_opt.is_none() {
            f_num_opt = read_f_number(&path_str, bytes_slice);
        }

        let has_valid_exposure = exposure_opt.is_some();
        let exposure_sec = exposure_opt.unwrap_or(0.01);
        let iso = iso_opt.map(|g| g as f32).unwrap_or(100.0);
        let f_number = f_num_opt.unwrap_or(5.6);
        let physical_scale = compute_physical_exposure_scale(Duration::from_secs_f32(exposure_sec), iso, f_number);

        let exif_map = sidecar_map.unwrap_or_else(|| {
            crate::metadata::sidecar::read_exif_data(&path_str, bytes_slice)
        });
        let timestamp = exif_map
            .get("DateTimeOriginal")
            .or_else(|| exif_map.get("DateTime"))
            .and_then(|s| parse_exif_timestamp(s))
            .or_else(|| {
                if !bytes_slice.is_empty() {
                    let disk_exif = crate::metadata::sidecar::read_exif_data(&path_str, bytes_slice);
                    disk_exif
                        .get("DateTimeOriginal")
                        .or_else(|| disk_exif.get("DateTime"))
                        .and_then(|s| parse_exif_timestamp(s))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                fs::metadata(&source_path)
                    .and_then(|m| m.modified())
                    .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
                    .unwrap_or(0)
            });

        items.push(BracketItemMeta {
            path: path.clone(),
            timestamp,
            physical_scale,
            has_valid_exposure,
        });
    }

    cluster_bracket_items(paths, items)
}

fn cluster_bracket_items(
    paths: &[String],
    mut items: Vec<BracketItemMeta>,
) -> (Vec<ExposureBracketGroup>, Vec<String>) {
    // Sort chronologically, then by filename
    items.sort_by(|a, b| {
        if a.timestamp != b.timestamp && a.timestamp > 0 && b.timestamp > 0 {
            a.timestamp.cmp(&b.timestamp)
        } else {
            a.path.cmp(&b.path)
        }
    });

    // Detect and isolate temporal outliers (e.g. disconnected shots separated by > 60s from the burst sequence)
    let mut cohesive_items = Vec::new();
    let mut excluded_outliers = Vec::new();

    let n = items.len();
    for i in 0..n {
        let t = items[i].timestamp;
        let mut is_outlier = false;

        if n >= 4 && t > 0 {
            let prev_gap = if i > 0 && items[i - 1].timestamp > 0 {
                (t as i64 - items[i - 1].timestamp as i64).abs()
            } else {
                0
            };
            let next_gap = if i + 1 < n && items[i + 1].timestamp > 0 {
                (items[i + 1].timestamp as i64 - t as i64).abs()
            } else {
                0
            };

            // Tail outlier: last frame shot > 60 seconds after preceding burst
            if i == n - 1 && prev_gap > 60 {
                is_outlier = true;
            }
            // Head outlier: first frame shot > 60 seconds before following burst
            if i == 0 && next_gap > 60 {
                is_outlier = true;
            }
        }

        if is_outlier {
            println!("  [HDR Pano] Isolating outlier frame {} (temporal delta > 60s)", items[i].path);
            excluded_outliers.push(items[i].path.clone());
        } else {
            cohesive_items.push(items[i].clone());
        }
    }

    let valid_scales: Vec<f32> = cohesive_items
        .iter()
        .filter(|it| it.has_valid_exposure && it.physical_scale.is_finite() && it.physical_scale > 0.0)
        .map(|it| it.physical_scale)
        .collect();
    let any_valid = !valid_scales.is_empty();
    let spread = if valid_scales.len() >= 2 {
        let min_s = valid_scales.iter().copied().fold(f32::INFINITY, f32::min);
        let max_s = valid_scales.iter().copied().fold(0.0f32, f32::max);
        if min_s > 0.0 { max_s / min_s } else { 1.0 }
    } else {
        1.0
    };

    let mut groups: Vec<ExposureBracketGroup> = Vec::new();
    let mut current_group: Vec<BracketItemMeta> = Vec::new();

    for item in cohesive_items {
        let mut start_new_group = false;

        if !current_group.is_empty() {
            let prev_item = current_group.last().unwrap();

            // 1. Check temporal gap: if timestamp delta > 15 seconds, likely a different angle or pause
            if prev_item.timestamp > 0 && item.timestamp > 0 {
                let dt = (item.timestamp as i64 - prev_item.timestamp as i64).abs();
                if dt > 15 {
                    start_new_group = true;
                }
            }

            // 2. Exposure collision check: in an HDR bracket, all frames must have distinct exposures.
            // If current_group already contains a frame with a similar exposure scale (ratio in [0.80, 1.25]),
            // this new frame cannot be part of the same bracket and must start the next bracket.
            if !start_new_group {
                let has_duplicate_exposure = current_group.iter().any(|existing| {
                    let r = if existing.physical_scale > 0.0 {
                        item.physical_scale / existing.physical_scale
                    } else {
                        1.0
                    };
                    r >= 0.80 && r <= 1.25
                });

                if has_duplicate_exposure {
                    start_new_group = true;
                }
            }

            // 3. Bracket length limit: standard brackets are 3 frames (or max 5 frames).
            // If we have reached 3 frames and total cohesive count is a multiple of 3, start new group.
            let total_cohesive = paths.len().saturating_sub(excluded_outliers.len());
            if !start_new_group && current_group.len() >= 3 {
                if total_cohesive % 3 == 0 || current_group.len() >= 5 {
                    start_new_group = true;
                }
            }
        }

        if start_new_group && !current_group.is_empty() {
            groups.push(ExposureBracketGroup {
                position_index: groups.len(),
                paths: current_group.iter().map(|it| it.path.clone()).collect(),
            });
            current_group.clear();
        }

        current_group.push(item);
    }

    if !current_group.is_empty() {
        groups.push(ExposureBracketGroup {
            position_index: groups.len(),
            paths: current_group.iter().map(|it| it.path.clone()).collect(),
        });
    }

    // Smart fallback: If grouping produced single-frame orphans or only 1 group,
    // but the cohesive files count is divisible by 3 and >= 6, chunk by 3 —
    // unless real EXIF exposures were read and show a flat single-exposure panorama (< 0.5 EV spread).
    let total_cohesive = paths.len().saturating_sub(excluded_outliers.len());
    let has_orphans = groups.iter().any(|g| g.paths.len() < 2);
    if (groups.len() <= 1 || has_orphans)
        && total_cohesive >= 6
        && total_cohesive % 3 == 0
        && !(any_valid && spread < 1.45)
    {
        let mut fallback_paths = Vec::new();
        for path in paths {
            if !excluded_outliers.contains(path) {
                fallback_paths.push(path.clone());
            }
        }
        let fallback_groups: Vec<ExposureBracketGroup> = fallback_paths
            .chunks(3)
            .enumerate()
            .map(|(i, chunk)| ExposureBracketGroup {
                position_index: i,
                paths: chunk.to_vec(),
            })
            .collect();
        return (fallback_groups, excluded_outliers);
    }

    (groups, excluded_outliers)
}

pub fn cluster_hdr_brackets(paths: &[String]) -> Vec<ExposureBracketGroup> {
    cluster_hdr_brackets_robust(paths).0
}

/// Executes the native Reference-Locked Linear-First HDR Panorama pipeline:
/// 1. Clusters input paths into exposure bracket groups per camera angle.
/// 2. Merges each bracket group into a ghost-free 32-bit LinearHdrPanel via `merge_handheld_hdr_linear_panel`.
/// 3. Normalizes all linear panels to a common physical exposure reference (compensating for inter-panel
///    aperture/shutter/ISO shifts such as f/8 -> f/11 in handheld sweeps).
/// 4. Stitches the linear radiance panels using joint intrinsic/extrinsic Bundle Adjustment,
///    SO(3)-invariant horizon leveling, APAP mesh warps, 2D Dinic Graph-Cut seams, seam-ribbon
///    de-parallax, and 6-level multi-band Laplacian pyramid blending.
/// 5. Applies the global Virtual-Exposure Laplacian Pyramid tone-mapping ONCE to the stitched linear
///    radiance canvas, guaranteeing zero inter-panel tone curve cliffs or sky blotches.
pub fn stitch_hdr_panorama_native(
    paths: &[String],
    projection: PanoramaProjection,
    boundary_warp: f32,
    half_size: bool,
    settings: &crate::app::settings::AppSettings,
    app_handle: Option<&AppHandle>,
    cancel_token: Option<&std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<(Rgb32FImage, Rgb32FImage), String> {
    let (bracket_groups, _outliers) = cluster_hdr_brackets_robust(paths);
    let num_positions = bracket_groups.len();
    if num_positions < 2 {
        return Err(
            "Could not detect at least 2 distinct panorama angles in the selection.".to_string(),
        );
    }

    let mut linear_panels = Vec::with_capacity(num_positions);
    for (pos_idx, group) in bracket_groups.iter().enumerate() {
        if let Some(tok) = cancel_token {
            if tok.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("HDR Panorama cancelled by user.".to_string());
            }
        }
        if let Some(handle) = app_handle {
            let progress = 5.0 + (pos_idx as f32 / num_positions as f32) * 40.0;
            let _ = handle.emit(
                "panorama-progress",
                format!(
                    "Merging linear HDR bracket {} of {}... ({:.0}%)",
                    pos_idx + 1,
                    num_positions,
                    progress
                ),
            );
        }
        println!(
            "  [HDR Pano] Merging linear HDR panel {}/{} ({} frames)...",
            pos_idx + 1,
            num_positions,
            group.paths.len()
        );
        let panel = crate::enhanced::hdr_handheld::merge_handheld_hdr_linear_panel(
            &group.paths,
            half_size,
            false,
            None,
            settings,
            app_handle,
            cancel_token,
        )?;
        linear_panels.push(panel);
    }

    // Physical inter-panel exposure normalization in 32-bit linear radiance space
    let mut exps: Vec<f32> = linear_panels
        .iter()
        .map(|p| p.nominal_exposure.max(1e-7))
        .collect();
    exps.sort_by(|a, b| a.total_cmp(b));
    let ref_exp = exps[exps.len() / 2];

    for (idx, panel) in linear_panels.iter_mut().enumerate() {
        let scale = (ref_exp / panel.nominal_exposure.max(1e-7)).clamp(0.125, 8.0);
        if (scale - 1.0).abs() > 0.01 {
            println!(
                "  [HDR Pano] Normalizing linear panel {} exposure by {:.3}x (E_panel={:.6}, E_ref={:.6})",
                idx + 1,
                scale,
                panel.nominal_exposure,
                ref_exp
            );
            panel
                .linear_radiance
                .as_mut()
                .par_iter_mut()
                .for_each(|v| *v *= scale);
        }
    }

    let panel_optics: Vec<(f64, Option<f64>)> = linear_panels
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let (real_p, _) = parse_virtual_path(&bracket_groups[i].paths[0]);
            let f_mm =
                crate::metadata::exif_processing::extract_focal_length_from_file(&real_p).unwrap_or(28.0);
            (f_mm, Some(p.f_number as f64))
        })
        .collect();

    let panels: Vec<Rgb32FImage> = linear_panels
        .into_iter()
        .map(|p| p.linear_radiance)
        .collect();

    if let Some(handle) = app_handle {
        let _ = handle.emit(
            "panorama-progress",
            "Stitching linear HDR panels with Bundle Adjustment & Graph-Cut... 50%",
        );
    }

    let stitched_linear = stitch_hdr_panels_with_optics(
        &panels,
        projection,
        boundary_warp,
        Some(&panel_optics),
        app_handle,
    )?;

    if let Some(handle) = app_handle {
        let _ = handle.emit(
            "panorama-progress",
            "Global Virtual-Exposure Laplacian Pyramid tone-mapping... 92%",
        );
    }
    println!("  [HDR Pano] Applying global Virtual-Exposure Laplacian Pyramid tone-mapping...");
    let to_srgb = crate::render::color_space::working_to_srgb();
    let srgb_out = crate::render::color_space::OutputTransform::srgb(true);
    let (sw, sh) = stitched_linear.dimensions();
    let mut srgb_lin = Rgb32FImage::new(sw, sh);
    srgb_lin
        .as_mut()
        .par_chunks_exact_mut(3)
        .zip(stitched_linear.as_raw().par_chunks_exact(3))
        .for_each(|(dst, src)| {
            let clean = glam::Vec3::new(
                if src[0].is_finite() { src[0].max(0.0) } else { 0.0 },
                if src[1].is_finite() { src[1].max(0.0) } else { 0.0 },
                if src[2].is_finite() { src[2].max(0.0) } else { 0.0 },
            );
            let mapped = crate::render::color_space::fit_gamut(
                (to_srgb * clean).to_array(),
                &srgb_out.compression,
            );
            dst[0] = mapped[0].max(0.0);
            dst[1] = mapped[1].max(0.0);
            dst[2] = mapped[2].max(0.0);
        });
    let tone_mapped = crate::enhanced::hdr_handheld::tone_map_virtual_exposure_pyramid(
        &srgb_lin,
        &crate::enhanced::hdr_handheld::HdrMergeOptions::default(),
    );

    Ok((tone_mapped, stitched_linear))
}

#[tauri::command]
pub async fn stitch_hdr_panorama(
    paths: Vec<String>,
    projection: Option<PanoramaProjection>,
    boundary_warp: Option<f32>,
    half_size: Option<bool>,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let _sleep_guard = SleepLockGuard::new("stitch_hdr_panorama");

    if paths.len() < 3 {
        return Err("At least 3 photos are required for an HDR Panorama.".to_string());
    }

    state
        .panorama_cancellation_token
        .store(false, std::sync::atomic::Ordering::Relaxed);
    let selected_projection = projection.unwrap_or(PanoramaProjection::Cylindrical);
    let warp_strength = boundary_warp.unwrap_or(0.5);
    let panorama_result_handle = state.hdr_panorama_result.clone();
    let panorama_linear_radiance_handle = state.panorama_linear_radiance.clone();
    let hdr_result_handle = state.hdr_result.clone();
    let hdr_linear_handle = state.hdr_linear_radiance.clone();
    let hdr_preview_handle = state.hdr_preview_radiance.clone();
    let hdr_last_options_handle = state.hdr_last_options.clone();
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let handle_for_task = app_handle.clone();
    let is_half_size = half_size.unwrap_or(false);

    let cancel_token = state.panorama_cancellation_token.clone();
    let task = tokio::task::spawn_blocking(move || {
        let (tone_mapped_pano, stitched_linear_pano) = match stitch_hdr_panorama_native(
            &paths,
            selected_projection,
            warp_strength,
            is_half_size,
            &settings,
            Some(&handle_for_task),
            Some(&cancel_token),
        ) {
            Ok(pair) => pair,
            Err(native_err) => {
                println!(
                    "  [HDR Pano] Native linear-first pipeline returned '{}', trying Hugin fallback...",
                    native_err
                );
                let hugin_pano = crate::enhanced::hugin_engine::run_hugin_hdr_panorama(
                    &paths,
                    selected_projection,
                    &settings,
                    is_half_size,
                    Some(&handle_for_task),
                    Some(&cancel_token),
                )?;
                (hugin_pano.clone(), hugin_pano)
            }
        };

        // Create high-resolution preview directly from the tone-mapped output
        let _ = handle_for_task.emit("panorama-progress", "Creating HDR Panorama preview... 98%");
        let master_dyn = DynamicImage::ImageRgb32F(tone_mapped_pano);
        let master_rgb8 = master_dyn.to_rgb8();
        let (w, h) = (master_rgb8.width(), master_rgb8.height());

        let (new_w, new_h) = if w.max(h) <= 1600 {
            (w.max(1), h.max(1))
        } else if w > h {
            (1600, ((1600.0 * h as f32 / w as f32).round() as u32).max(1))
        } else {
            (((1600.0 * w as f32 / h as f32).round() as u32).max(1), 1600)
        };

        let preview_u8 = if (new_w, new_h) == (w, h) {
            master_rgb8
        } else {
            image::imageops::resize(&master_rgb8, new_w, new_h, image::imageops::FilterType::Triangle)
        };

        let jpeg_bytes = mozjpeg_rs::Encoder::new(mozjpeg_rs::Preset::BaselineFastest)
            .quality(88)
            .encode_rgb(&preview_u8.into_raw(), new_w, new_h)
            .map_err(|e| format!("Failed to encode HDR panorama JPEG preview: {}", e))?;

        use base64::{engine::general_purpose, Engine as _};
        let base64_str = general_purpose::STANDARD.encode(jpeg_bytes);
        let final_base64 = format!("data:image/jpeg;base64,{}", base64_str);

        *hdr_linear_handle.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *hdr_preview_handle.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *hdr_last_options_handle.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *hdr_result_handle.lock().unwrap_or_else(|e| e.into_inner()) = Some(master_dyn.clone());
        *panorama_result_handle.lock().unwrap_or_else(|e| e.into_inner()) = Some(master_dyn);
        *panorama_linear_radiance_handle.lock().unwrap_or_else(|e| e.into_inner()) = Some(stitched_linear_pano);

        let _ = handle_for_task.emit(
            "panorama-complete",
            serde_json::json!({
                "base64": final_base64,
                "isHdr": true,
                "projection": format!("{:?}", selected_projection),
            }),
        );

        Ok::<(), String>(())
    });

    match task.await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => {
            let _ = app_handle.emit("panorama-error", e.clone());
            Err(e)
        }
        Err(join_err) => {
            let msg = format!("HDR Panorama task failed: {}", join_err);
            let _ = app_handle.emit("panorama-error", msg.clone());
            Err(msg)
        }
    }
}

/// Converts 32-bit linear floating-point HDR panel to a high-contrast,
/// perceptually uniform 8-bit grayscale image optimized for multi-scale ORB/FAST feature detection.
pub fn hdr_to_feature_grayscale(panel: &Rgb32FImage) -> image::GrayImage {
    let (w, h) = panel.dimensions();
    let mut gray = image::GrayImage::new(w, h);

    // 1. Calculate luminance L = 0.2126*R + 0.7152*G + 0.0722*B
    // and sample 1st and 99th percentiles for robust adaptive range normalization
    let mut lums: Vec<f32> = Vec::with_capacity((w * h / 16) as usize);
    for y in (0..h).step_by(4) {
        for x in (0..w).step_by(4) {
            let p = panel.get_pixel(x, y);
            let lum = 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
            if lum > 0.00001 && lum.is_finite() {
                lums.push(lum);
            }
        }
    }

    let (p01, p99) = if lums.is_empty() {
        (0.0f32, 1.0f32)
    } else {
        lums.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx01 = (lums.len() as f32 * 0.01) as usize;
        let idx99 = ((lums.len() as f32 * 0.99) as usize).min(lums.len() - 1);
        (lums[idx01].max(0.0), lums[idx99].max(0.001))
    };

    let span = (p99 - p01).max(1e-5);
    let log_denom = (1.0f32 + 20.0f32).ln();

    // 2. Perceptual log-luminance mapping + sRGB gamma 2.2 to preserve sharp feature gradients
    // across deep shadow foliage, iron bridge details, and bright skies simultaneously.
    for y in 0..h {
        for x in 0..w {
            let p = panel.get_pixel(x, y);
            let lum = (0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]).max(0.0);
            let relative = ((lum - p01) / span).clamp(0.0, 1.0);
            let norm = (1.0 + 20.0 * relative).ln() / log_denom;
            let val = (norm.powf(1.0 / 2.2) * 255.0).clamp(0.0, 255.0) as u8;
            gray.put_pixel(x, y, image::Luma([val]));
        }
    }

    crate::enhanced::panorama_utils::processing::normalize_grayscale(&gray)
}

/// Stitches pre-merged 32-bit float HDR image panels using robust multi-scale ORB matching,
/// scaled homography RANSAC, MST anchor selection, and bundle adjustment.
pub fn stitch_hdr_panels(
    panels: &[Rgb32FImage],
    projection: PanoramaProjection,
    boundary_warp: f32,
    optical_params: Option<(f64, Option<f64>)>,
    app_handle: Option<&AppHandle>,
) -> Result<Rgb32FImage, String> {
    let repeated: Option<Vec<(f64, Option<f64>)>> =
        optical_params.map(|opt| vec![opt; panels.len()]);
    stitch_hdr_panels_with_optics(
        panels,
        projection,
        boundary_warp,
        repeated.as_deref(),
        app_handle,
    )
}

/// Stitches pre-merged 32-bit float HDR image panels using per-panel EXIF optical parameters,
/// multi-scale ORB matching, Wahba/Kabsch SO(3) rotation initialization, and bundle adjustment.
pub fn stitch_hdr_panels_with_optics(
    panels: &[Rgb32FImage],
    projection: PanoramaProjection,
    boundary_warp: f32,
    panel_optics: Option<&[(f64, Option<f64>)]>,
    app_handle: Option<&AppHandle>,
) -> Result<Rgb32FImage, String> {
    if panels.is_empty() {
        return Err("No HDR panels available to stitch.".to_string());
    }
    if panels.len() == 1 {
        return Ok(panels[0].clone());
    }

    let brief_pairs = crate::enhanced::panorama_utils::processing::generate_brief_pairs();

    // Extract perceptual grayscale representations once for all multi-scale matching passes
    let grays_full: Vec<image::GrayImage> = panels.iter().map(|p| hdr_to_feature_grayscale(p)).collect();

    let image_infos: Vec<ImageInfo> = panels
        .iter()
        .enumerate()
        .map(|(i, panel)| {
            let gray_full = &grays_full[i];
            let (w, h) = gray_full.dimensions();
            let (new_w, new_h, scale_factor) = crate::enhanced::panorama_utils::processing::calculate_downscale_dimensions(w, h);
            let gray_small = image::imageops::resize(gray_full, new_w, new_h, image::imageops::FilterType::Triangle);
            let low_detail_mask = crate::enhanced::panorama_utils::processing::generate_low_detail_mask(gray_full);
            let features = crate::enhanced::panorama_utils::processing::find_features(&gray_small, &brief_pairs);
            println!("  [HDR Pano] Angle {}: extracted {} features (scale: {:.2})", i + 1, features.len(), scale_factor);

            let proxy_dyn = DynamicImage::ImageRgb32F(panel.clone()).resize_exact(new_w, new_h, image::imageops::FilterType::Triangle);
            let proxy_image = proxy_dyn.to_rgb32f();

            ImageInfo {
                id: i,
                filename: format!("HDR_Angle_{:02}", i + 1),
                full_width: w,
                full_height: h,
                proxy_image,
                full_image: Some(panel.clone()),
                low_detail_mask,
                scale_factor,
                features,
                exposure_gain: 1.0,
            }
        })
        .collect();

    let min_inliers = crate::enhanced::panorama_utils::processing::MIN_INLIERS_FOR_CONNECTION; // Enforce strict 15 inlier floor for robust graph topology

    // Match features pairwise in parallel with homography scaling & refinement
    let pairs_to_check: Vec<(usize, usize)> = (0..image_infos.len())
        .flat_map(|i| (i + 1..image_infos.len()).map(move |j| (i, j)))
        .collect();

    let match_results: Vec<Option<((usize, usize), crate::enhanced::panorama_utils::MatchInfo)>> = pairs_to_check
        .par_iter()
        .map(|&(i, j)| {
            let features1 = &image_infos[i].features;
            let features2 = &image_infos[j].features;

            let initial_matches = crate::enhanced::panorama_utils::processing::match_features(features1, features2);
            println!("  [HDR Pano] Pair ({}, {}): {} initial matches", i + 1, j + 1, initial_matches.len());
            let mut best_match: Option<crate::enhanced::panorama_utils::MatchInfo> = None;

            if initial_matches.len() >= min_inliers {
                let keypoints1: Vec<crate::enhanced::hdr_alignment::KeyPoint> = features1.iter().map(|f| f.keypoint).collect();
                let keypoints2: Vec<crate::enhanced::hdr_alignment::KeyPoint> = features2.iter().map(|f| f.keypoint).collect();

                if let Some((_h_small, inliers)) = crate::enhanced::panorama_utils::processing::find_homography_ransac_with_min_inliers(&initial_matches, &keypoints1, &keypoints2, min_inliers)
                    && inliers.len() >= min_inliers
                {
                    let inlier_points: Vec<(nalgebra::Point2<f64>, nalgebra::Point2<f64>)> = inliers
                        .iter()
                        .map(|m| {
                            let p1 = keypoints1[m.index1];
                            let p2 = keypoints2[m.index2];
                            (
                                nalgebra::Point2::new(p1.x as f64, p1.y as f64),
                                nalgebra::Point2::new(p2.x as f64, p2.y as f64),
                            )
                        })
                        .collect();

                    if let Some(h_refined) = crate::enhanced::panorama_utils::processing::compute_homography(&inlier_points) {
                        let s1 = image_infos[i].scale_factor;
                        let s2 = image_infos[j].scale_factor;
                        let scale_mat_i_inv = nalgebra::Matrix3::new(1.0 / s1, 0.0, 0.0, 0.0, 1.0 / s1, 0.0, 0.0, 0.0, 1.0);
                        let scale_mat_j = nalgebra::Matrix3::new(s2, 0.0, 0.0, 0.0, s2, 0.0, 0.0, 0.0, 1.0);
                        let h_full = scale_mat_j * h_refined * scale_mat_i_inv;

                        best_match = Some(crate::enhanced::panorama_utils::MatchInfo {
                            homography: h_full,
                            inliers: inliers.len(),
                            inlier_matches: inliers,
                            is_subpixel: false,
                        });
                    }
                }
            }

            // High-resolution fallback pass if pair is borderline or has < 25 inliers (e.g. low-contrast rails/foliage)
            let inlier_count = best_match.as_ref().map(|m| m.inliers).unwrap_or(0);
            if inlier_count < 25 {
                let gray_full_1 = &grays_full[i];
                let gray_full_2 = &grays_full[j];
                let (w1, h1) = gray_full_1.dimensions();
                let (w2, h2) = gray_full_2.dimensions();
                let (hd_w1, hd_h1, hd_s1) = crate::enhanced::panorama_utils::processing::calculate_downscale_dimensions_capped(w1, h1, 2400);
                let (hd_w2, hd_h2, hd_s2) = crate::enhanced::panorama_utils::processing::calculate_downscale_dimensions_capped(w2, h2, 2400);

                let hd_img1 = image::imageops::resize(gray_full_1, hd_w1, hd_h1, image::imageops::FilterType::Triangle);
                let hd_img2 = image::imageops::resize(gray_full_2, hd_w2, hd_h2, image::imageops::FilterType::Triangle);

                let hd_feat1 = crate::enhanced::panorama_utils::processing::find_features_tuned(&hd_img1, &brief_pairs, 7, 12.0);
                let hd_feat2 = crate::enhanced::panorama_utils::processing::find_features_tuned(&hd_img2, &brief_pairs, 7, 12.0);

                let hd_matches = crate::enhanced::panorama_utils::processing::match_features(&hd_feat1, &hd_feat2);
                if hd_matches.len() >= min_inliers {
                    let hd_kp1: Vec<crate::enhanced::hdr_alignment::KeyPoint> = hd_feat1.iter().map(|f| f.keypoint).collect();
                    let hd_kp2: Vec<crate::enhanced::hdr_alignment::KeyPoint> = hd_feat2.iter().map(|f| f.keypoint).collect();

                    if let Some((_h_small, hd_inliers)) = crate::enhanced::panorama_utils::processing::find_homography_ransac_with_min_inliers(&hd_matches, &hd_kp1, &hd_kp2, min_inliers)
                        && hd_inliers.len() > inlier_count
                    {
                        println!("  [HDR Pano] Adaptive HD fallback for pair ({}, {}): {} matches, {} inliers (previously {})",
                            i + 1, j + 1, hd_matches.len(), hd_inliers.len(), inlier_count);

                        let inlier_points: Vec<(nalgebra::Point2<f64>, nalgebra::Point2<f64>)> = hd_inliers
                            .iter()
                            .map(|m| {
                                let p1 = hd_kp1[m.index1];
                                let p2 = hd_kp2[m.index2];
                                (
                                    nalgebra::Point2::new(p1.x as f64, p1.y as f64),
                                    nalgebra::Point2::new(p2.x as f64, p2.y as f64),
                                )
                            })
                            .collect();

                        if let Some(h_refined) = crate::enhanced::panorama_utils::processing::compute_homography(&inlier_points) {
                            let scale_mat_i_inv = nalgebra::Matrix3::new(1.0 / hd_s1, 0.0, 0.0, 0.0, 1.0 / hd_s1, 0.0, 0.0, 0.0, 1.0);
                            let scale_mat_j = nalgebra::Matrix3::new(hd_s2, 0.0, 0.0, 0.0, hd_s2, 0.0, 0.0, 0.0, 1.0);
                            let h_full = scale_mat_j * h_refined * scale_mat_i_inv;

                            best_match = Some(crate::enhanced::panorama_utils::MatchInfo {
                                homography: h_full,
                                inliers: hd_inliers.len(),
                                inlier_matches: hd_inliers,
                                is_subpixel: false,
                            });
                        }
                    }
                }
            }

            if let Some(ref m) = best_match {
                println!("  [HDR Pano] Final accepted match pair ({}, {}): {} inliers", i + 1, j + 1, m.inliers);
                Some(((i, j), best_match.unwrap()))
            } else {
                None
            }
        })
        .collect();

    let mut pairwise_matches: HashMap<(usize, usize), crate::enhanced::panorama_utils::MatchInfo> = HashMap::new();
    for (pair, info) in match_results.into_iter().flatten() {
        pairwise_matches.insert(pair, info);
    }
    println!("  [HDR Pano] Total matched pairs found: {}", pairwise_matches.len());

    if pairwise_matches.is_empty() {
        return Err("Could not align enough overlapping HDR panels. Ensure at least 30% overlap between adjacent shots.".to_string());
    }

    let (mut ordered_indices, global_homographies) = crate::enhanced::panorama_utils::build_stitching_order(&image_infos, &pairwise_matches);

    if ordered_indices.len() < 2 {
        return Err("Could not align enough overlapping HDR panels. Ensure at least 30% overlap between adjacent shots.".to_string());
    }

    // If panels have differing focal lengths (e.g. 100mm wide base + 126mm/188mm zoomed-in shots),
    // place the widest-FOV panel (smallest f_mm) first as anchor Pose 0 so the wide base defines the canvas.
    if let Some(optics) = panel_optics {
        let min_f = ordered_indices
            .iter()
            .filter_map(|&id| optics.get(id).map(|o| o.0))
            .fold(f64::INFINITY, f64::min);
        let max_f = ordered_indices
            .iter()
            .filter_map(|&id| optics.get(id).map(|o| o.0))
            .fold(f64::NEG_INFINITY, f64::max);
        if max_f > min_f * 1.05 {
            ordered_indices.sort_by(|&a, &b| {
                let fa = optics.get(a).map(|o| o.0).unwrap_or(28.0);
                let fb = optics.get(b).map(|o| o.0).unwrap_or(28.0);
                fa.total_cmp(&fb)
            });
        }
    }

    let stitched_infos: Vec<&ImageInfo> = ordered_indices.iter().map(|&i| &image_infos[i]).collect();

    // 1. Initialize camera poses with per-panel optical physics & vignetting compensation
    let mut camera_poses = Vec::with_capacity(stitched_infos.len());

    for (k, &img_info) in stitched_infos.iter().enumerate() {
        let (w, h) = (img_info.full_width, img_info.full_height);
        let opt_k = panel_optics.and_then(|s| s.get(img_info.id).copied());
        let f_px = if let Some((f_mm, _)) = opt_k {
            let px = CameraPose::focal_length_from_exif(f_mm, None, w);
            println!("  - HDR Pano panel {} (id {}): physical focal {:.1} mm -> {:.1} px", k + 1, img_info.id + 1, f_mm, px);
            px
        } else {
            w.max(h) as f64 * 1.25
        };
        let mut pose = CameraPose::new(k, w, h, Some(f_px));
        if let Some((f_mm, fn_opt)) = opt_k {
            pose = pose.with_vignetting_from_optical_params(f_mm, fn_opt);
        }
        if let Some(h_global) = global_homographies.get(&img_info.id) {
            let (yaw, pitch, roll) = crate::enhanced::panorama_utils::camera_model::homography_to_relative_rotation(h_global, f_px, w, h);
            pose.yaw = yaw;
            pose.pitch = pitch;
            pose.roll = roll;
        }
        camera_poses.push(pose);
    }

    // 2. Levenberg-Marquardt Bundle Adjustment on 3D camera poses for HDR panels
    let mut tie_points: Vec<crate::enhanced::panorama_utils::camera_model::MatchTiePoint> = Vec::new();
    let index_map: HashMap<usize, usize> = ordered_indices.iter().enumerate().map(|(k, &id)| (id, k)).collect();

    for (&(id1, id2), match_info) in &pairwise_matches {
        if let (Some(&k1), Some(&k2)) = (index_map.get(&id1), index_map.get(&id2)) {
            let s1 = image_infos[id1].scale_factor;
            let s2 = image_infos[id2].scale_factor;
            let f1 = &image_infos[id1].features;
            let f2 = &image_infos[id2].features;
            let img1 = &image_infos[id1].proxy_image;
            let img2 = &image_infos[id2].proxy_image;

            for m in &match_info.inlier_matches {
                let p1 = f1[m.index1].keypoint;
                let p2 = f2[m.index2].keypoint;

                let p1_proxy = nalgebra::Point2::new(p1.x as f64, p1.y as f64);
                let p2_proxy = nalgebra::Point2::new(p2.x as f64, p2.y as f64);

                let p2_refined_proxy = crate::enhanced::panorama_utils::processing::refine_match_klt_subpixel(img1, p1_proxy, img2, p2_proxy);
                let p1_full = nalgebra::Point2::new(p1.x as f64 * s1, p1.y as f64 * s1);
                let p2_refined_full = nalgebra::Point2::new(p2_refined_proxy.x * s2, p2_refined_proxy.y * s2);

                tie_points.push(crate::enhanced::panorama_utils::camera_model::MatchTiePoint {
                    img1: k1,
                    img2: k2,
                    p1: p1_full,
                    p2: p2_refined_full,
                });
            }
        }
    }

    if !tie_points.is_empty() {
        crate::enhanced::panorama_utils::camera_model::bundle_adjust_poses(&mut camera_poses, &tie_points, 15);
    }

    // 3. Automatic Horizon Roll & Pitch Leveling
    crate::enhanced::panorama_utils::camera_model::auto_level_camera_poses(&mut camera_poses);

    // 4. Compute local APAP mesh deformation grids to eliminate parallax in HDR panels
    let mesh_warps = crate::enhanced::panorama_utils::camera_model::compute_apap_mesh_warps(&camera_poses, &tie_points);

    let pano = ray_traced_multiband_stitcher(
        &stitched_infos,
        &camera_poses,
        &mesh_warps,
        projection,
        boundary_warp,
        app_handle,
        None,
        None,
    )?;
    Ok(pano)
}

#[tauri::command]
pub async fn inspect_hdr_pano_grouping(paths: Vec<String>) -> Result<HdrPanoGroupingReport, String> {
    tokio::task::spawn_blocking(move || {
        let (panels, excluded_outliers) = cluster_hdr_brackets_robust(&paths);
        let total_files = paths.len();
        let detected_bracket_size = if !panels.is_empty() { panels[0].paths.len() } else { 0 };
        let is_consistent_bracket_size =
            !panels.is_empty() && panels.iter().all(|p| p.paths.len() == detected_bracket_size);

        Ok(HdrPanoGroupingReport {
            total_files,
            panels,
            excluded_outliers,
            is_consistent_bracket_size,
            detected_bracket_size,
        })
    })
    .await
    .map_err(|e| format!("HDR panorama grouping task failed: {}", e))?
}

#[tauri::command]
pub async fn inspect_panorama_brackets(paths: Vec<String>) -> serde_json::Value {
    tokio::task::spawn_blocking(move || {
        let (panels, excluded_outliers) = cluster_hdr_brackets_robust(&paths);
        let panel_count = panels.len();
        let bracket_size = panels.first().map(|p| p.paths.len()).unwrap_or(0);
        let is_hdr_bracket_set =
            panel_count >= 2 && bracket_size >= 2 && has_real_hdr_exposure_spread(&panels);
        let summary = if is_hdr_bracket_set {
            format!(
                "Detected {} panorama panels x {} exposure brackets ({} total frames){}",
                panel_count,
                bracket_size,
                paths.len(),
                if excluded_outliers.is_empty() {
                    String::new()
                } else {
                    format!(", {} outlier(s)", excluded_outliers.len())
                }
            )
        } else {
            "Standard single-exposure panorama set".to_string()
        };

        serde_json::json!({
            "isHdrBracketSet": is_hdr_bracket_set,
            "panelCount": panel_count,
            "bracketSize": bracket_size,
            "excludedOutliers": excluded_outliers.len(),
            "summary": summary,
        })
    })
    .await
    .unwrap_or_else(|_| serde_json::json!({
        "isHdrBracketSet": false,
        "panelCount": 0,
        "bracketSize": 0,
        "excludedOutliers": 0,
        "summary": "Standard single-exposure panorama set",
    }))
}

#[tauri::command]
pub async fn update_hdr_panorama_preview(
    options: Option<crate::enhanced::hdr_fusion::HdrMergeOptions>,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let radiance_opt = state
        .panorama_linear_radiance
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let radiance = radiance_opt.ok_or_else(|| "No HDR panorama radiance buffer available".to_string())?;
    let opts = options.unwrap_or_default();
    let app_handle_bg = app_handle.clone();
    let pano_result_handle = state.hdr_panorama_result.clone();
    let hdr_result_handle = state.hdr_result.clone();

    tokio::task::spawn_blocking(move || {
        let tone_mapped = crate::enhanced::hdr_fusion::tone_map_radiance_image(
            &radiance,
            &opts,
            Some(&app_handle_bg),
        );
        let dyn_img = DynamicImage::ImageRgb32F(tone_mapped);
        *pano_result_handle.lock().unwrap_or_else(|e| e.into_inner()) = Some(dyn_img.clone());
        *hdr_result_handle.lock().unwrap_or_else(|e| e.into_inner()) = Some(dyn_img.clone());

        let rgb8 = dyn_img.to_rgb8();
        let (w, h) = (rgb8.width(), rgb8.height());
        let (nw, nh) = if w.max(h) <= 1600 {
            (w.max(1), h.max(1))
        } else if w > h {
            (1600, ((1600.0 * h as f32 / w as f32).round() as u32).max(1))
        } else {
            (((1600.0 * w as f32 / h as f32).round() as u32).max(1), 1600)
        };
        let resized = if (nw, nh) == (w, h) {
            rgb8
        } else {
            image::imageops::resize(&rgb8, nw, nh, image::imageops::FilterType::Triangle)
        };
        let jpeg_bytes = mozjpeg_rs::Encoder::new(mozjpeg_rs::Preset::BaselineFastest)
            .quality(88)
            .encode_rgb(&resized.into_raw(), nw, nh)
            .map_err(|e| format!("Failed to encode HDR panorama preview: {}", e))?;
        use base64::{Engine as _, engine::general_purpose};
        Ok(format!(
            "data:image/jpeg;base64,{}",
            general_purpose::STANDARD.encode(jpeg_bytes)
        ))
    })
    .await
    .map_err(|e| format!("Failed to update HDR panorama preview: {}", e))?
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PanoramaSequenceGroup {
    pub id: String,
    pub paths: Vec<String>,
    pub is_hdr: bool,
    pub count: usize,
    pub focal_length: Option<f64>,
}

fn parse_focal_length_mm(raw: &str) -> Option<f64> {
    raw.split(|c: char| !c.is_ascii_digit() && c != '.')
        .find(|s| !s.is_empty())
        .and_then(|s| s.parse::<f64>().ok())
}

fn group_consecutive_sequences(
    mut items: Vec<(String, Option<u64>, Option<f64>)>,
) -> Vec<(Vec<String>, Option<f64>)> {
    items.sort_by(|a, b| (a.1.unwrap_or(u64::MAX), a.0.as_str()).cmp(&(b.1.unwrap_or(u64::MAX), b.0.as_str())));

    let mut result: Vec<(Vec<String>, Option<f64>)> = Vec::new();
    let mut current_group: Vec<String> = Vec::new();
    let mut last_timestamp: Option<u64> = None;
    let mut last_focal: Option<f64> = None;

    for (path_str, timestamp, focal) in items {
        let is_consecutive = match (last_timestamp, timestamp, last_focal, focal) {
            (Some(lt), Some(ct), Some(lf), Some(cf)) => {
                ct.abs_diff(lt) <= 5 && (cf - lf).abs() < 1.0
            }
            (Some(lt), Some(ct), _, _) => ct.abs_diff(lt) <= 4,
            _ => false,
        };

        if is_consecutive {
            current_group.push(path_str);
        } else {
            if current_group.len() >= 2 {
                result.push((current_group, last_focal));
            }
            current_group = vec![path_str];
        }

        last_timestamp = timestamp;
        last_focal = focal;
    }

    if current_group.len() >= 2 {
        result.push((current_group, last_focal));
    }

    result
}

#[tauri::command]
pub async fn detect_panorama_sequences(
    image_paths: Vec<String>,
) -> Result<Vec<PanoramaSequenceGroup>, String> {
    if image_paths.len() < 2 {
        return Ok(Vec::new());
    }

    tokio::task::spawn_blocking(move || {
        let items: Vec<(String, Option<u64>, Option<f64>)> = image_paths
            .into_iter()
            .map(|path_str| {
                let (real_path, _) = parse_virtual_path(&path_str);
                let real_str = real_path.to_string_lossy();
                let meta = crate::metadata::exif_processing::read_rrexif_sidecar(&real_path)
                    .unwrap_or_else(|| {
                        let mapped = crate::library::file_ops::read_file_mapped(&real_path).ok();
                        crate::metadata::sidecar::read_exif_data(&real_str, mapped.as_deref().unwrap_or(&[]))
                    });

                let timestamp = meta
                    .get("DateTimeOriginal")
                    .or_else(|| meta.get("DateTime"))
                    .and_then(|s| parse_exif_timestamp(s));
                let focal = meta
                    .get("FocalLength")
                    .and_then(|fl| parse_focal_length_mm(fl));

                (path_str, timestamp, focal)
            })
            .collect();

        let sequences = group_consecutive_sequences(items);
        let groups: Vec<PanoramaSequenceGroup> = sequences
            .into_iter()
            .enumerate()
            .map(|(i, (paths, focal_length))| {
                let count = paths.len();
                let (bracket_groups, _) = cluster_hdr_brackets_robust(&paths);
                let is_hdr = bracket_groups.len() >= 2
                    && bracket_groups.iter().all(|g| g.paths.len() >= 2)
                    && has_real_hdr_exposure_spread(&bracket_groups);
                PanoramaSequenceGroup {
                    id: format!("pano_{}_{}", i + 1, count),
                    paths,
                    is_hdr,
                    count,
                    focal_length,
                }
            })
            .collect();

        Ok(groups)
    })
    .await
    .map_err(|e| format!("Panorama sequence detection failed: {}", e))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_hdr_brackets_chunking_fallback() {
        let dummy_paths = vec![
            "IMG_0001.CR2".to_string(),
            "IMG_0002.CR2".to_string(),
            "IMG_0003.CR2".to_string(),
            "IMG_0004.CR2".to_string(),
            "IMG_0005.CR2".to_string(),
            "IMG_0006.CR2".to_string(),
        ];

        let groups = cluster_hdr_brackets(&dummy_paths);
        assert!(!groups.is_empty(), "Should generate bracket groups");
        let total_clustered: usize = groups.iter().map(|g| g.paths.len()).sum();
        assert_eq!(total_clustered, 6, "All 6 photos must be assigned to bracket groups without data loss");
    }

    #[test]
    fn test_hdr_to_feature_grayscale_contrast_and_texture() {
        let mut hdr_panel = Rgb32FImage::new(100, 100);
        for y in 0..100 {
            for x in 0..100 {
                // Wide dynamic range from deep shadow (0.005) to highlight (8.5)
                let lum = 0.005 + (x as f32 / 100.0) * 8.5;
                hdr_panel.put_pixel(x, y, image::Rgb([lum, lum, lum]));
            }
        }

        let gray = hdr_to_feature_grayscale(&hdr_panel);
        let min_val = gray.iter().copied().min().unwrap();
        let max_val = gray.iter().copied().max().unwrap();

        assert_eq!(min_val, 0, "Grayscale floor should be mapped to 0");
        assert_eq!(max_val, 255, "Grayscale ceiling should reach 255 for feature contrast");

        // Check that 18% gray (approx 0.18) is mapped into middle range rather than crushed near 0
        let mid_pixel = gray.get_pixel(50, 50)[0];
        assert!(mid_pixel > 80, "Midtone details must be lifted above dark floor for ORB detector, got {}", mid_pixel);
    }

    #[test]
    fn test_detect_panorama_sequences_empty_or_unreadable() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let empty = rt.block_on(detect_panorama_sequences(Vec::new())).unwrap();
        assert!(empty.is_empty(), "Empty input should yield zero panorama sequences");

        let no_exif = rt
            .block_on(detect_panorama_sequences(vec![
                "d:/Rapidraw_04/nonexistent_pano_1.jpg".to_string(),
                "d:/Rapidraw_04/nonexistent_pano_2.jpg".to_string(),
            ]))
            .unwrap();
        assert!(no_exif.is_empty(), "Files without EXIF timestamps should yield zero sequences");
    }

    #[test]
    fn test_single_exposure_6_frame_pano_not_misclassified_as_hdr() {
        let paths: Vec<String> = (1..=6).map(|i| format!("P{}.CR2", i)).collect();
        let items: Vec<BracketItemMeta> = (1..=6)
            .map(|i| BracketItemMeta {
                path: format!("P{}.CR2", i),
                timestamp: 1000 + (i as u64 - 1) * 2,
                physical_scale: 1.0,
                has_valid_exposure: true,
            })
            .collect();

        let (groups, excluded) = cluster_bracket_items(&paths, items);
        assert!(excluded.is_empty());
        assert!(
            groups.iter().all(|g| g.paths.len() <= 1),
            "Flat 6-frame single-exposure panorama must not be chunked into multi-frame HDR brackets: {:?}",
            groups
        );
        let total: usize = groups.iter().map(|g| g.paths.len()).sum();
        assert_eq!(total, 6);

        assert!(!groups_have_real_spread(&[
            vec![Some(1.0), Some(1.0), Some(1.0)],
            vec![Some(1.0), Some(1.0), Some(1.0)],
        ]));
    }

    #[test]
    fn test_real_brackets_detected_as_hdr_spread() {
        assert!(groups_have_real_spread(&[
            vec![Some(0.25), Some(1.0), Some(4.0)],
            vec![Some(0.25), Some(1.0), Some(4.0)],
        ]));
        assert!(!groups_have_real_spread(&[
            vec![None, None, None],
            vec![None, None, None],
        ]));
    }

    #[test]
    fn test_parse_focal_length_mm_and_chronological_order() {
        assert_eq!(parse_focal_length_mm("24mm"), Some(24.0));
        assert_eq!(parse_focal_length_mm("24.0 mm"), Some(24.0));
        assert_eq!(parse_focal_length_mm("50 mm (35mm equivalent: 75 mm)"), Some(50.0));
        assert_eq!(parse_focal_length_mm("abc"), None);

        let sequences = group_consecutive_sequences(vec![
            ("C".to_string(), Some(104), Some(24.0)),
            ("A".to_string(), Some(100), Some(24.0)),
            ("B".to_string(), Some(102), Some(24.0)),
            ("Z".to_string(), Some(500), Some(24.0)),
        ]);
        assert_eq!(sequences.len(), 1);
        assert_eq!(sequences[0].0, vec!["A", "B", "C"]);
    }
}
