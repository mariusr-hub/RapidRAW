//! Ultra-Fast Speed-Demon AI Culler & Multi-Face Loupe Engine for RapidRAW
//!
//! Provides zero-latency frame telemetry, face/eye detection, 100% crop extraction,
//! and automated photoshoot triage culling.

use crate::library::sidecars::{parse_virtual_path, set_rating_for_paths};
use crate::library::file_ops::read_file_mapped;
use crate::decode::image_loader::load_base_image_from_bytes;
use base64::{engine::general_purpose, Engine as _};
use image::{DynamicImage, GenericImageView, ImageFormat};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::Path;
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceLoupeCrop {
    pub face_index: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub sharpness_score: f32,
    pub is_eyes_open: bool,
    pub is_sharp: bool,
    pub crop_data_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CullingFrameAnalysis {
    pub file_path: String,
    pub sharpness_score: f32,
    pub exposure_score: f32,
    pub is_blurry: bool,
    pub faces: Vec<FaceLoupeCrop>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TriageCullResult {
    pub total_scanned: usize,
    pub rejected_blurry_count: usize,
    pub rejected_blink_count: usize,
    pub recommended_picks_count: usize,
    pub flagged_paths: Vec<String>,
}

fn compute_sharpness_variance(img: &DynamicImage) -> f32 {
    rapidraw_dsp::calculate_laplacian_variance(&img.to_rgb8()).sqrt().clamp(0.0, 100.0)
}

/// Detects multiple distinct skin-tone face clusters and extracts 100% crop patches
pub(crate) fn detect_face_loupes(img: &DynamicImage) -> Vec<FaceLoupeCrop> {
    let (w, h) = img.dimensions();
    if w < 100 || h < 100 {
        return Vec::new();
    }

    let rgb = img.to_rgb8();
    let cell_size = 16u32.max(w.min(h) / 64);
    let gw = w.div_ceil(cell_size) as usize;
    let gh = h.div_ceil(cell_size) as usize;
    let mut grid = vec![false; gw * gh];

    for cy in 0..gh {
        let y = ((cy as u32) * cell_size + cell_size / 2).min(h - 1);
        for cx in 0..gw {
            let x = ((cx as u32) * cell_size + cell_size / 2).min(w - 1);
            let p = rgb.get_pixel(x, y);
            let r = p[0] as f32;
            let g = p[1] as f32;
            let b = p[2] as f32;
            if r > 60.0 && g > 40.0 && b > 20.0
                && (r - g).abs() > 15.0
                && r > g && g >= b
                && (r / (g + 0.001)) < 2.5
            {
                grid[cy * gw + cx] = true;
            }
        }
    }

    // Connected-component clustering over skin grid cells (separates group-shot faces)
    let mut visited = vec![false; gw * gh];
    let mut clusters: Vec<(u32, u32, u32, u32, usize)> = Vec::new();
    let mut stack = Vec::new();

    for start_y in 0..gh {
        for start_x in 0..gw {
            let idx = start_y * gw + start_x;
            if !grid[idx] || visited[idx] {
                continue;
            }
            visited[idx] = true;
            stack.push((start_x, start_y));
            let mut min_cx = start_x;
            let mut max_cx = start_x;
            let mut min_cy = start_y;
            let mut max_cy = start_y;
            let mut count = 0usize;

            while let Some((x, y)) = stack.pop() {
                count += 1;
                min_cx = min_cx.min(x);
                max_cx = max_cx.max(x);
                min_cy = min_cy.min(y);
                max_cy = max_cy.max(y);

                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        if nx >= 0 && ny >= 0 && (nx as usize) < gw && (ny as usize) < gh {
                            let nidx = ny as usize * gw + nx as usize;
                            if grid[nidx] && !visited[nidx] {
                                visited[nidx] = true;
                                stack.push((nx as usize, ny as usize));
                            }
                        }
                    }
                }
            }

            if count >= 4 {
                let x0 = (min_cx as u32 * cell_size).min(w - 1);
                let y0 = (min_cy as u32 * cell_size).min(h - 1);
                let x1 = (((max_cx + 1) as u32) * cell_size).min(w);
                let y1 = (((max_cy + 1) as u32) * cell_size).min(h);
                clusters.push((x0, y0, x1, y1, count));
            }
        }
    }

    clusters.sort_by(|a, b| b.4.cmp(&a.4));
    clusters.truncate(6);

    let mut loupes = Vec::new();
    for (face_index, (x0, y0, x1, y1, _)) in clusters.into_iter().enumerate() {
        let cluster_w = (x1 - x0).max(60);
        let cluster_h = (y1 - y0).max(60);
        let face_w = (cluster_w as f32 * 0.85).clamp(80.0, w as f32 * 0.45);
        let face_h = (cluster_h as f32 * 0.85).clamp(80.0, h as f32 * 0.45);
        let cx = (x0 + x1) as f32 / 2.0;
        let cy = (y0 + y1) as f32 / 2.0;

        let crop_x = (cx - face_w / 2.0).clamp(0.0, (w as f32 - face_w).max(0.0)) as u32;
        let crop_y = (cy - face_h / 2.0).clamp(0.0, (h as f32 - face_h).max(0.0)) as u32;
        let crop_w = (face_w as u32).min(w.saturating_sub(crop_x));
        let crop_h = (face_h as u32).min(h.saturating_sub(crop_y));

        if crop_w < 40 || crop_h < 40 {
            continue;
        }

        let cropped_patch = img.crop_imm(crop_x, crop_y, crop_w, crop_h);
        let patch_sharpness = compute_sharpness_variance(&cropped_patch);
        let is_sharp = patch_sharpness > 18.0;

        let top_half = cropped_patch.crop_imm(
            0,
            (crop_h as f32 * 0.2) as u32,
            crop_w,
            (crop_h as f32 * 0.35) as u32,
        );
        let eye_contrast = compute_sharpness_variance(&top_half);
        let is_eyes_open = eye_contrast > 12.0;

        let thumb = cropped_patch.resize_to_fill(200, 200, image::imageops::FilterType::Triangle);
        let mut buf = Cursor::new(Vec::new());
        let _ = thumb.to_rgb8().write_to(&mut buf, ImageFormat::Jpeg);
        let b64 = general_purpose::STANDARD.encode(buf.get_ref());
        let crop_data_url = format!("data:image/jpeg;base64,{}", b64);

        loupes.push(FaceLoupeCrop {
            face_index,
            x: crop_x as f32 / w as f32,
            y: crop_y as f32 / h as f32,
            width: crop_w as f32 / w as f32,
            height: crop_h as f32 / h as f32,
            sharpness_score: patch_sharpness,
            is_eyes_open,
            is_sharp,
            crop_data_url,
        });
    }
    loupes
}

#[tauri::command]
pub fn analyze_culling_frame(
    path: String,
    app_handle: AppHandle,
    state: State<crate::app::state::AppState>,
) -> Result<CullingFrameAnalysis, String> {
    let (source_path, _) = parse_virtual_path(&path);
    let path_str = source_path.to_string_lossy().to_string();

    let bytes = read_file_mapped(Path::new(&path_str)).map_err(|e| e.to_string())?;
    let settings = crate::app::settings::load_settings(app_handle).unwrap_or_default();
    let img = load_base_image_from_bytes(&bytes, &path_str, false, &settings, None)
        .map_err(|e| e.to_string())?;

    let sharpness = compute_sharpness_variance(&img);
    let is_blurry = sharpness < 14.0;
    let (w, h) = img.dimensions();

    // Prefer cached 478-point face landmarks from face_masking.rs when available
    let mut faces: Vec<FaceLoupeCrop> = Vec::new();
    if let Ok(ai_guard) = state.ai_state.lock()
        && let Some(ai) = ai_guard.as_ref()
        && let Some(analysis) = &ai.face_analysis
        && crate::ai::jobs::source_id(&source_path).as_deref() == Some(analysis.path_hash.as_str())
    {
        for (face_index, face) in analysis.faces.iter().take(6).enumerate() {
            let box_size = (face.size * 1.4).clamp(80.0, (w.min(h) as f32) * 0.6);
            let crop_x = (face.center[0] - box_size * 0.5).clamp(0.0, (w as f32 - box_size).max(0.0)) as u32;
            let crop_y = (face.center[1] - box_size * 0.5).clamp(0.0, (h as f32 - box_size).max(0.0)) as u32;
            let crop_w = (box_size as u32).min(w.saturating_sub(crop_x)).max(1);
            let crop_h = (box_size as u32).min(h.saturating_sub(crop_y)).max(1);
            let cropped = img.crop_imm(crop_x, crop_y, crop_w, crop_h);
            let face_sharp = compute_sharpness_variance(&cropped);
            let eye_band = cropped.crop_imm(0, (crop_h as f32 * 0.2) as u32, crop_w, (crop_h as f32 * 0.35).max(1.0) as u32);
            let eye_sharp = compute_sharpness_variance(&eye_band);
            let thumb = cropped.resize_to_fill(200, 200, image::imageops::FilterType::Triangle);
            let mut buf = Cursor::new(Vec::new());
            let _ = thumb.to_rgb8().write_to(&mut buf, ImageFormat::Jpeg);
            faces.push(FaceLoupeCrop {
                face_index,
                x: crop_x as f32 / w as f32,
                y: crop_y as f32 / h as f32,
                width: crop_w as f32 / w as f32,
                height: crop_h as f32 / h as f32,
                sharpness_score: face_sharp,
                is_eyes_open: eye_sharp > 12.0,
                is_sharp: face_sharp > 18.0,
                crop_data_url: format!("data:image/jpeg;base64,{}", general_purpose::STANDARD.encode(buf.get_ref())),
            });
        }
    }
    if faces.is_empty() {
        faces = detect_face_loupes(&img);
    }

    // Compute basic exposure rating
    let gray = img.to_luma8();
    let total = (gray.width() * gray.height()) as f32;
    let mut good_midtones = 0.0f32;
    for p in gray.pixels() {
        if p[0] > 45 && p[0] < 220 {
            good_midtones += 1.0;
        }
    }
    let exposure_score = ((good_midtones / total) * 100.0).clamp(0.0, 100.0);

    Ok(CullingFrameAnalysis {
        file_path: path,
        sharpness_score: sharpness,
        exposure_score,
        is_blurry,
        faces,
    })
}

#[tauri::command]
pub fn batch_auto_triage_cull(
    paths: Vec<String>,
    blur_threshold: Option<f32>,
    reject_blinks: Option<bool>,
    app_handle: AppHandle,
    state: State<crate::app::state::AppState>,
) -> Result<TriageCullResult, String> {
    let _sleep_guard = crate::enhanced::sleep_lock::SleepLockGuard::new("batch_auto_triage_cull");
    let min_sharpness = blur_threshold.unwrap_or(14.0);
    let check_blinks = reject_blinks.unwrap_or(true);
    let total_scanned = paths.len();

    let mut rejected_paths = Vec::new();
    let mut rejected_blurry_count = 0usize;
    let mut rejected_blink_count = 0usize;
    let mut recommended_picks_count = 0usize;

    for (idx, path_str) in paths.iter().enumerate() {
        let _ = app_handle.emit(
            "triage-cull-progress",
            serde_json::json!({
                "current": idx + 1,
                "total": total_scanned,
                "path": path_str
            }),
        );

        if let Ok(analysis) = analyze_culling_frame(path_str.clone(), app_handle.clone(), state.clone()) {
            let mut reject = false;
            if analysis.sharpness_score < min_sharpness {
                rejected_blurry_count += 1;
                reject = true;
            } else if check_blinks && !analysis.faces.is_empty() && !analysis.faces[0].is_eyes_open {
                rejected_blink_count += 1;
                reject = true;
            } else if analysis.sharpness_score > 35.0 && analysis.exposure_score > 60.0 {
                recommended_picks_count += 1;
            }

            if reject {
                rejected_paths.push(path_str.clone());
            }
        }
    }

    // Auto-mark rejected paths with 1-star (rejected tag)
    if !rejected_paths.is_empty() {
        let _ = set_rating_for_paths(rejected_paths.clone(), 1, app_handle.clone());
    }

    Ok(TriageCullResult {
        total_scanned,
        rejected_blurry_count,
        rejected_blink_count,
        recommended_picks_count,
        flagged_paths: rejected_paths,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawFocusPeakingData {
    pub width: u32,
    pub height: u32,
    pub max_peak_intensity: f32,
    pub peaking_mask_data_url: String,
}

#[tauri::command]
pub fn extract_green_cfa_focus_peaking(
    path: String,
    threshold: Option<f32>,
    app_handle: AppHandle,
    _state: State<crate::app::state::AppState>,
) -> Result<RawFocusPeakingData, String> {
    let (source_path, _) = parse_virtual_path(&path);
    let path_str = source_path.to_string_lossy().to_string();
    let bytes = read_file_mapped(Path::new(&path_str)).map_err(|e| e.to_string())?;

    let settings = crate::app::settings::load_settings(app_handle).unwrap_or_default();
    let img = load_base_image_from_bytes(&bytes, &path_str, true, &settings, None)
        .map_err(|e| e.to_string())?;

    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let mut peak_mask = image::RgbaImage::new(w, h);
    let edge_thresh = threshold.unwrap_or(24.0);
    let mut max_intensity = 0.0f32;

    // Green channel gradient edge detector (CFA green dominance)
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let g_c = rgb.get_pixel(x, y)[1] as f32;
            let g_l = rgb.get_pixel(x - 1, y)[1] as f32;
            let g_r = rgb.get_pixel(x + 1, y)[1] as f32;
            let g_u = rgb.get_pixel(x, y - 1)[1] as f32;
            let g_d = rgb.get_pixel(x, y + 1)[1] as f32;

            let lap = (4.0 * g_c - g_l - g_r - g_u - g_d).abs();
            if lap > max_intensity {
                max_intensity = lap;
            }

            if lap > edge_thresh {
                let alpha = ((lap / (lap + 20.0)) * 255.0).clamp(60.0, 255.0) as u8;
                peak_mask.put_pixel(x, y, image::Rgba([0, 255, 120, alpha]));
            }
        }
    }

    let mut buf = Cursor::new(Vec::new());
    let _ = peak_mask.write_to(&mut buf, ImageFormat::Png);
    let b64 = general_purpose::STANDARD.encode(buf.get_ref());
    let peaking_mask_data_url = format!("data:image/png;base64,{}", b64);

    Ok(RawFocusPeakingData {
        width: w,
        height: h,
        max_peak_intensity: max_intensity,
        peaking_mask_data_url,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurstGroup {
    pub group_id: usize,
    pub photo_paths: Vec<String>,
    pub hero_path: String,
    pub similarity_score: f32,
    pub hero_score: f32,
    pub frame_count: usize,
}

#[derive(Debug, Clone)]
pub struct FrameBurstMeta {
    pub path: String,
    pub timestamp: f64,
    pub dhash: u64,
    pub hero_score: f32,
    pub sharpness_score: f32,
    pub exposure_score: f32,
}

/// Computes the hero score for a candidate frame based on the industry-standard formula:
/// Score = 0.60 * LaplacianSharpness + 0.25 * ExposureBalance + 0.15 * EyeFocus
pub fn compute_hero_score(sharpness_score: f32, exposure_score: f32, faces: &[FaceLoupeCrop]) -> f32 {
    let sharpness_norm = sharpness_score.clamp(0.0, 100.0);
    let exposure_norm = exposure_score.clamp(0.0, 100.0);

    let eye_focus = if faces.is_empty() {
        75.0f32
    } else {
        let any_blink = faces.iter().any(|f| !f.is_eyes_open);
        if any_blink {
            20.0f32
        } else {
            let sharp_eyes_count = faces.iter().filter(|f| f.is_sharp && f.is_eyes_open).count();
            if sharp_eyes_count == faces.len() {
                100.0f32
            } else {
                70.0f32
            }
        }
    };

    0.60 * sharpness_norm + 0.25 * exposure_norm + 0.15 * eye_focus
}

fn extract_frame_timestamp(source_path: &Path, file_bytes: &[u8]) -> f64 {
    let exif_map = crate::metadata::sidecar::read_exif_data(&source_path.to_string_lossy(), file_bytes);

    let sub_sec: f64 = exif_map
        .get("SubSecTimeOriginal")
        .or_else(|| exif_map.get("SubSecTime"))
        .or_else(|| exif_map.get("SubsecTimeDigitized"))
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map(|val| {
            let digits = (val.log10().floor() as i32 + 1).max(1);
            val / 10f64.powi(digits)
        })
        .unwrap_or(0.0);

    let base_ts = exif_map
        .get("DateTimeOriginal")
        .or_else(|| exif_map.get("DateTime"))
        .and_then(|s| crate::enhanced::hdr_panorama::parse_exif_timestamp(s))
        .map(|ts| ts as f64)
        .unwrap_or_else(|| {
            std::fs::metadata(source_path)
                .and_then(|m| m.modified())
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64())
                .unwrap_or(0.0)
        });

    base_ts + sub_sec
}

pub fn cluster_burst_frames(
    mut entries: Vec<FrameBurstMeta>,
    time_window_secs: f64,
    max_hamming_dist: u32,
) -> Vec<BurstGroup> {
    if entries.len() < 2 {
        return Vec::new();
    }

    entries.sort_by(|a, b| {
        a.timestamp.partial_cmp(&b.timestamp).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut groups: Vec<BurstGroup> = Vec::new();
    let mut current_cluster: Vec<FrameBurstMeta> = Vec::new();

    for item in entries {
        if let Some(last) = current_cluster.last() {
            let dt = (item.timestamp - last.timestamp).abs();
            let dist = hamming_distance(item.dhash, last.dhash);
            let sim_to_anchor = hamming_distance(item.dhash, current_cluster[0].dhash);

            if dt <= time_window_secs && (dist <= max_hamming_dist || sim_to_anchor <= max_hamming_dist + 2) {
                current_cluster.push(item);
            } else {
                if current_cluster.len() > 1 {
                    let hero = current_cluster
                        .iter()
                        .max_by(|a, b| a.hero_score.partial_cmp(&b.hero_score).unwrap())
                        .unwrap();

                    groups.push(BurstGroup {
                        group_id: groups.len() + 1,
                        photo_paths: current_cluster.iter().map(|f| f.path.clone()).collect(),
                        hero_path: hero.path.clone(),
                        similarity_score: 90.0,
                        hero_score: hero.hero_score,
                        frame_count: current_cluster.len(),
                    });
                }
                current_cluster.clear();
                current_cluster.push(item);
            }
        } else {
            current_cluster.push(item);
        }
    }

    if current_cluster.len() > 1 {
        let hero = current_cluster
            .iter()
            .max_by(|a, b| a.hero_score.partial_cmp(&b.hero_score).unwrap())
            .unwrap();

        groups.push(BurstGroup {
            group_id: groups.len() + 1,
            photo_paths: current_cluster.iter().map(|f| f.path.clone()).collect(),
            hero_path: hero.path.clone(),
            similarity_score: 92.0,
            hero_score: hero.hero_score,
            frame_count: current_cluster.len(),
        });
    }

    groups
}

/// Computes 64-bit difference hash (DHash) for fast visual similarity comparison
pub fn compute_dhash(img: &DynamicImage) -> u64 {
    let thumb = img.thumbnail_exact(9, 8).to_luma8();
    let mut hash = 0u64;

    for y in 0..8 {
        for x in 0..8 {
            let left = thumb.get_pixel(x, y)[0];
            let right = thumb.get_pixel(x + 1, y)[0];
            if left > right {
                hash |= 1 << (y * 8 + x);
            }
        }
    }
    hash
}

/// Calculates Hamming distance between two 64-bit perceptual hashes
pub fn hamming_distance(h1: u64, h2: u64) -> u32 {
    (h1 ^ h2).count_ones()
}

/// Groups burst sequences based on capture time window gating (dt <= 2.0s) and hash similarity
#[tauri::command]
pub fn group_burst_photos(
    paths: Vec<String>,
    time_window_secs: Option<f64>,
    max_hamming_dist: Option<u32>,
    app_handle: AppHandle,
    _state: State<crate::app::state::AppState>,
) -> Result<Vec<BurstGroup>, String> {
    let time_window = time_window_secs.unwrap_or(2.0).clamp(0.5, 10.0);
    let max_dist = max_hamming_dist.unwrap_or(12);

    let entries: Vec<FrameBurstMeta> = paths
        .par_iter()
        .filter_map(|path| {
            let (source_path, _) = parse_virtual_path(path);
            let path_str = source_path.to_string_lossy().to_string();
            let bytes = read_file_mapped(Path::new(&path_str)).ok()?;
            let timestamp = extract_frame_timestamp(&source_path, &bytes);

            let settings = crate::app::settings::load_settings(app_handle.clone()).unwrap_or_default();
            let img = load_base_image_from_bytes(&bytes, &path_str, false, &settings, None).ok()?;

            let dhash = compute_dhash(&img);
            let sharpness = compute_sharpness_variance(&img);

            let gray = img.to_luma8();
            let total = (gray.width() * gray.height()) as f32;
            let good_midtones = if total > 0.0 {
                gray.pixels().filter(|p| p[0] > 45 && p[0] < 220).count() as f32
            } else {
                0.0
            };
            let exposure = if total > 0.0 {
                ((good_midtones / total) * 100.0).clamp(0.0, 100.0)
            } else {
                50.0
            };

            let faces = detect_face_loupes(&img);
            let hero_score = compute_hero_score(sharpness, exposure, &faces);

            Some(FrameBurstMeta {
                path: path.clone(),
                timestamp,
                dhash,
                hero_score,
                sharpness_score: sharpness,
                exposure_score: exposure,
            })
        })
        .collect();

    Ok(cluster_burst_frames(entries, time_window, max_dist))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn test_compute_sharpness_variance() {
        let (w, h) = (64u32, 64u32);
        let mut sharp_img = RgbImage::new(w, h);
        let mut flat_img = RgbImage::new(w, h);

        for y in 0..h {
            for x in 0..w {
                flat_img.put_pixel(x, y, Rgb([128, 128, 128]));
                let val = if (x / 4) % 2 == 0 { 220 } else { 30 };
                sharp_img.put_pixel(x, y, Rgb([val, val, val]));
            }
        }

        let sharp_dyn = DynamicImage::ImageRgb8(sharp_img);
        let flat_dyn = DynamicImage::ImageRgb8(flat_img);

        let s_score = compute_sharpness_variance(&sharp_dyn);
        let f_score = compute_sharpness_variance(&flat_dyn);

        assert!(s_score > f_score, "Sharp image must score higher than flat image");
        assert_eq!(f_score, 0.0, "Flat image variance should be zero");
    }

    #[test]
    fn test_detect_face_loupes_on_skin_patch() {
        let (w, h) = (200u32, 200u32);
        let mut portrait = RgbImage::new(w, h);

        for y in 0..h {
            for x in 0..w {
                // Background is neutral gray
                portrait.put_pixel(x, y, Rgb([100, 100, 100]));
                // Place a 80x80 skin tone face patch in the center
                if x >= 60 && x < 140 && y >= 60 && y < 140 {
                    portrait.put_pixel(x, y, Rgb([210, 160, 125]));
                }
            }
        }

        let dyn_img = DynamicImage::ImageRgb8(portrait);
        let loupes = detect_face_loupes(&dyn_img);

        assert!(!loupes.is_empty(), "Face loupe should detect central skin patch");
        let face = &loupes[0];
        assert!(face.width >= 0.2 && face.height >= 0.2, "Extracted normalized face size must be sufficient: got {}x{}", face.width, face.height);
        assert!(face.crop_data_url.starts_with("data:image/jpeg;base64,"), "Loupe must generate valid JPEG base64 URL");
    }

    #[test]
    fn test_cluster_burst_frames_temporal_and_hero_selection() {
        let entries = vec![
            FrameBurstMeta {
                path: "frame_001.raw".to_string(),
                timestamp: 1000.0,
                dhash: 0b1010101010101010,
                hero_score: 55.0,
                sharpness_score: 50.0,
                exposure_score: 70.0,
            },
            FrameBurstMeta {
                path: "frame_002.raw".to_string(),
                timestamp: 1000.3, // 300ms later (burst)
                dhash: 0b1010101010101011, // 1 bit diff
                hero_score: 88.0, // Best sharpness & focus!
                sharpness_score: 90.0,
                exposure_score: 85.0,
            },
            FrameBurstMeta {
                path: "frame_003.raw".to_string(),
                timestamp: 1000.6, // 300ms later (burst)
                dhash: 0b1010101010101010,
                hero_score: 62.0,
                sharpness_score: 60.0,
                exposure_score: 70.0,
            },
            FrameBurstMeta {
                path: "frame_004.raw".to_string(),
                timestamp: 1060.0, // 60 seconds later (isolated shot)
                dhash: 0b1010101010101010,
                hero_score: 70.0,
                sharpness_score: 70.0,
                exposure_score: 70.0,
            },
            FrameBurstMeta {
                path: "frame_005.raw".to_string(),
                timestamp: 1200.0, // Burst 2
                dhash: 0b1111000011110000,
                hero_score: 65.0,
                sharpness_score: 60.0,
                exposure_score: 70.0,
            },
            FrameBurstMeta {
                path: "frame_006.raw".to_string(),
                timestamp: 1200.4, // Burst 2
                dhash: 0b1111000011110001,
                hero_score: 79.0, // Hero for Burst 2
                sharpness_score: 80.0,
                exposure_score: 75.0,
            },
        ];

        let groups = cluster_burst_frames(entries, 2.0, 12);
        assert_eq!(groups.len(), 2, "Should detect exactly 2 distinct burst groups");

        // First burst group verification
        assert_eq!(groups[0].group_id, 1);
        assert_eq!(groups[0].frame_count, 3);
        assert_eq!(groups[0].hero_path, "frame_002.raw");
        assert_eq!(groups[0].hero_score, 88.0);
        assert_eq!(
            groups[0].photo_paths,
            vec!["frame_001.raw", "frame_002.raw", "frame_003.raw"]
        );

        // Second burst group verification
        assert_eq!(groups[1].group_id, 2);
        assert_eq!(groups[1].frame_count, 2);
        assert_eq!(groups[1].hero_path, "frame_006.raw");
        assert_eq!(groups[1].hero_score, 79.0);
        assert_eq!(groups[1].photo_paths, vec!["frame_005.raw", "frame_006.raw"]);
    }

    #[test]
    fn test_hero_score_formula_penalizes_blinks() {
        let open_eyes_face = vec![FaceLoupeCrop {
            face_index: 0,
            x: 0.5,
            y: 0.5,
            width: 0.2,
            height: 0.2,
            sharpness_score: 80.0,
            is_eyes_open: true,
            is_sharp: true,
            crop_data_url: "".to_string(),
        }];

        let blink_face = vec![FaceLoupeCrop {
            face_index: 0,
            x: 0.5,
            y: 0.5,
            width: 0.2,
            height: 0.2,
            sharpness_score: 80.0,
            is_eyes_open: false, // Closed eyes / blink!
            is_sharp: true,
            crop_data_url: "".to_string(),
        }];

        let score_open = compute_hero_score(80.0, 75.0, &open_eyes_face);
        let score_blink = compute_hero_score(80.0, 75.0, &blink_face);

        assert!(
            score_open > score_blink + 10.0,
            "Blink face must score significantly lower than open-eyes face with equal sharpness: {} vs {}",
            score_open,
            score_blink
        );
    }
}
