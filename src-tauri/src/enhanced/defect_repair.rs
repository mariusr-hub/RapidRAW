use crate::app::state::AppState;
use crate::metadata::sidecar::load_sidecar;
use crate::library::sidecars::parse_virtual_path;
use crate::library::file_ops::{read_file_mapped};
use crate::decode::image_loader::load_base_image_from_bytes;

use image::{DynamicImage, GenericImageView, Rgb};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::f32::consts::PI;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchDustSummary {
    pub total_processed: usize,
    pub total_spots_healed: usize,
    pub elapsed_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AutoHorizonResult {
    pub angle_degrees: f32,
    pub confidence: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SaliencyCropResult {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub target_aspect_ratio: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DustSpotHealResult {
    pub spots_detected: usize,
    pub message: String,
}

fn default_defect_kind() -> String {
    "dust".to_string()
}

fn default_routing() -> String {
    "analytical".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedSpot {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub norm_x: f32,
    pub norm_y: f32,
    #[serde(default = "default_defect_kind")]
    pub kind: String,
    #[serde(default)]
    pub end_x: Option<f32>,
    #[serde(default)]
    pub end_y: Option<f32>,
    #[serde(default)]
    pub norm_end_x: Option<f32>,
    #[serde(default)]
    pub norm_end_y: Option<f32>,
    #[serde(default)]
    pub points: Vec<[f32; 2]>,
    #[serde(default = "default_routing")]
    pub routing: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScratchCandidate {
    pub norm_start_x: f32,
    pub norm_start_y: f32,
    pub norm_end_x: f32,
    pub norm_end_y: f32,
    pub width_px: f32,
    pub contrast: f32,
    #[serde(default)]
    pub points: Vec<[f32; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorDustMapSpot {
    pub id: String,
    pub sensor_x: f32,
    pub sensor_y: f32,
    pub median_radius: f32,
    pub hit_count: usize,
    pub frame_count: usize,
    pub confidence: f32,
    pub camera_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShootDustMapReport {
    pub camera_key: String,
    pub total_frames: usize,
    pub spots: Vec<SensorDustMapSpot>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoDefectRepairResult {
    pub spots: Vec<DetectedSpot>,
    pub total_detected: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorDustCandidate {
    pub norm_x: f32,
    pub norm_y: f32,
    pub radius: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgePatrolCrop {
    pub crop_x: u32,
    pub crop_y: u32,
    pub width: u32,
    pub height: u32,
    pub edge_distractions_found: bool,
}

/// Calculates the exact maximum inscribed rectangle (w_inner, h_inner) and offset (crop_x, crop_y)
/// that fits inside an image of size (w, h) rotated by `angle_deg` degrees, eliminating black corner wedges.
pub fn calculate_inscribed_crop(w: u32, h: u32, angle_deg: f32) -> (u32, u32, u32, u32) {
    let theta = (angle_deg.abs() * PI / 180.0).clamp(0.0, PI / 4.0);
    if theta < 0.0001 {
        return (0, 0, w, h);
    }

    let cos_t = theta.cos();
    let sin_t = theta.sin();
    let w_f = w as f32;
    let h_f = h as f32;

    // Scale factor ensuring the inner box with original aspect ratio fits within rotated frame
    let s1 = w_f / (w_f * cos_t + h_f * sin_t);
    let s2 = h_f / (h_f * cos_t + w_f * sin_t);
    let s = s1.min(s2).clamp(0.05, 1.0);

    let inner_w = ((w_f * s).floor() as u32).min(w);
    let inner_h = ((h_f * s).floor() as u32).min(h);

    let crop_x = (w.saturating_sub(inner_w)) / 2;
    let crop_y = (h.saturating_sub(inner_h)) / 2;

    (crop_x, crop_y, inner_w, inner_h)
}

/// Detects the dominant horizon angle within [-15.0, 15.0] degrees.
pub fn detect_horizon_angle(image: &DynamicImage) -> AutoHorizonResult {
    let (width, height) = image.dimensions();
    if width < 32 || height < 32 {
        return AutoHorizonResult { angle_degrees: 0.0, confidence: 0.0 };
    }

    // Work on a fast downscaled grayscale representation
    let thumb = image.thumbnail(512, 512).to_luma8();
    let (tw, th) = thumb.dimensions();

    let mut angle_histogram = vec![0.0f32; 301]; // -15.0 to +15.0 degrees in 0.1 deg steps

    let rad_to_deg = 180.0 / PI;

    for y in 1..(th - 1) {
        for x in 1..(tw - 1) {
            let p_left = thumb.get_pixel(x - 1, y)[0] as f32;
            let p_right = thumb.get_pixel(x + 1, y)[0] as f32;
            let p_top = thumb.get_pixel(x, y - 1)[0] as f32;
            let p_bot = thumb.get_pixel(x, y + 1)[0] as f32;

            let gx = p_right - p_left;
            let gy = p_bot - p_top;
            let mag = (gx * gx + gy * gy).sqrt();

            if mag > 15.0 {
                // Gradient is perpendicular to the edge; edge angle is atan2(gy, gx) - 90 deg
                let raw_angle_rad = gy.atan2(gx);
                let edge_angle_deg = (raw_angle_rad * rad_to_deg) - 90.0;
                let mut norm_angle = edge_angle_deg;
                while norm_angle > 90.0 { norm_angle -= 180.0; }
                while norm_angle < -90.0 { norm_angle += 180.0; }

                if norm_angle >= -15.0 && norm_angle <= 15.0 {
                    let bin = ((norm_angle + 15.0) * 10.0).round().clamp(0.0, 300.0) as usize;
                    angle_histogram[bin] += mag;
                }
            }
        }
    }

    // Find smoothed peak in histogram
    let mut smoothed = vec![0.0f32; angle_histogram.len()];
    for i in 2..(angle_histogram.len() - 2) {
        smoothed[i] = (angle_histogram[i - 2] * 0.1)
            + (angle_histogram[i - 1] * 0.2)
            + (angle_histogram[i] * 0.4)
            + (angle_histogram[i + 1] * 0.2)
            + (angle_histogram[i + 2] * 0.1);
    }

    let mut max_val = 0.0f32;
    let mut best_bin = 150; // default 0.0 degrees
    let total_energy: f32 = smoothed.iter().sum();

    for (bin, &val) in smoothed.iter().enumerate() {
        if val > max_val {
            max_val = val;
            best_bin = bin;
        }
    }

    let detected_angle = (best_bin as f32 / 10.0) - 15.0;
    let confidence = if total_energy > 0.0 { (max_val / total_energy) * 10.0 } else { 0.0 }.clamp(0.0, 1.0);

    AutoHorizonResult {
        angle_degrees: -detected_angle, // Counter-rotation to level horizon
        confidence,
    }
}

/// Calculates rule-of-thirds & golden-ratio saliency crop recommendation.
pub fn calculate_saliency_crop(image: &DynamicImage, aspect_ratio: f32) -> SaliencyCropResult {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return SaliencyCropResult {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            target_aspect_ratio: 1.0,
        };
    }

    let thumb = image.thumbnail(256, 256).to_luma8();
    let (tw, th) = thumb.dimensions();

    let mut total_saliency = 0.0f32;
    let mut cx_sum = 0.0f32;
    let mut cy_sum = 0.0f32;

    if tw > 2 && th > 2 {
        for y in 1..(th - 1) {
            for x in 1..(tw - 1) {
                let p = thumb.get_pixel(x, y)[0] as f32;
                let p_r = thumb.get_pixel(x + 1, y)[0] as f32;
                let p_b = thumb.get_pixel(x, y + 1)[0] as f32;
                let grad = ((p_r - p).abs() + (p_b - p).abs()).max(0.0);
                
                // Prioritize center-weighted and high-contrast regions
                let nx = (x as f32 / tw as f32) - 0.5;
                let ny = (y as f32 / th as f32) - 0.5;
                let dist_center = 1.0 - (nx * nx + ny * ny).sqrt() * 0.5;

                let sal = grad * dist_center;
                total_saliency += sal;
                cx_sum += (x as f32 / tw as f32) * sal;
                cy_sum += (y as f32 / th as f32) * sal;
            }
        }
    }

    let focal_x = if total_saliency > 0.0 { cx_sum / total_saliency } else { 0.5 };
    let focal_y = if total_saliency > 0.0 { cy_sum / total_saliency } else { 0.5 };

    let img_ar = width as f32 / height as f32;
    let target_ar = if aspect_ratio > 0.01 { aspect_ratio } else { img_ar };

    // Ratio of target aspect ratio to image aspect ratio
    let r = target_ar / img_ar;

    // Standard high-quality crop box coverage (88% of constraining dimension)
    let (crop_w, crop_h) = if r >= 1.0 {
        let w = 0.88f32;
        let h = (w / r).clamp(0.05, 1.0);
        (w, h)
    } else {
        let h = 0.88f32;
        let w = (h * r).clamp(0.05, 1.0);
        (w, h)
    };

    // Compositional anchor placement:
    // Place focal point at golden ratio (~0.382 / 0.618) or center (0.5)
    let anchor_x = if focal_x < 0.45 {
        0.382f32
    } else if focal_x > 0.55 {
        0.618f32
    } else {
        0.5f32
    };

    let anchor_y = if focal_y < 0.45 {
        0.382f32
    } else if focal_y > 0.55 {
        0.618f32
    } else {
        0.5f32
    };

    let max_x = (1.0f32 - crop_w).max(0.0);
    let max_y = (1.0f32 - crop_h).max(0.0);

    let crop_x = (focal_x - crop_w * anchor_x).clamp(0.0, max_x);
    let crop_y = (focal_y - crop_h * anchor_y).clamp(0.0, max_y);

    SaliencyCropResult {
        x: crop_x * 100.0,
        y: crop_y * 100.0,
        width: crop_w * 100.0,
        height: crop_h * 100.0,
        target_aspect_ratio: target_ar,
    }
}

fn build_defect_analysis_luma(image: &DynamicImage) -> (u32, u32, Vec<f32>, bool) {
    let (width, height) = image.dimensions();
    let max_dim = width.max(height);
    let analysis = if max_dim > 1600 {
        let scale = 1600.0 / max_dim as f32;
        let nw = ((width as f32 * scale).round() as u32).max(64);
        let nh = ((height as f32 * scale).round() as u32).max(64);
        std::borrow::Cow::Owned(image.resize_exact(nw, nh, image::imageops::FilterType::Triangle))
    } else {
        std::borrow::Cow::Borrowed(image)
    };
    let (aw, ah) = analysis.dimensions();
    let rgb_img = analysis.to_rgb32f();
    let raw_px = rgb_img.as_raw();
    let mut luma = Vec::with_capacity((aw * ah) as usize);
    let mut dark_count = 0usize;
    let mut bright_pinpoints = 0usize;
    for px in raw_px.chunks_exact(3) {
        let l = (0.2126 * px[0] + 0.7152 * px[1] + 0.0722 * px[2]).clamp(0.0, 2.0);
        if l < 0.08 {
            dark_count += 1;
        } else if l > 0.45 {
            bright_pinpoints += 1;
        }
        luma.push(l);
    }
    let total = luma.len().max(1);
    let is_starfield_scene =
        (dark_count as f32 / total as f32) > 0.55 && bright_pinpoints >= 8;
    (aw, ah, luma, is_starfield_scene)
}

pub fn canonicalize_to_sensor_coords(norm_x: f32, norm_y: f32, orientation: u8) -> (f32, f32) {
    let x = norm_x.clamp(0.0, 1.0);
    let y = norm_y.clamp(0.0, 1.0);
    match orientation {
        2 => (1.0 - x, y),
        3 => (1.0 - x, 1.0 - y),
        4 => (x, 1.0 - y),
        5 => (y, x),
        6 => (y, 1.0 - x),
        7 => (1.0 - y, 1.0 - x),
        8 => (1.0 - y, x),
        _ => (x, y),
    }
}

pub fn sensor_to_display_coords(sensor_x: f32, sensor_y: f32, orientation: u8) -> (f32, f32) {
    let sx = sensor_x.clamp(0.0, 1.0);
    let sy = sensor_y.clamp(0.0, 1.0);
    match orientation {
        2 => (1.0 - sx, sy),
        3 => (1.0 - sx, 1.0 - sy),
        4 => (sx, 1.0 - sy),
        5 => (sy, sx),
        6 => (1.0 - sy, sx),
        7 => (1.0 - sy, 1.0 - sx),
        8 => (sy, 1.0 - sx),
        _ => (sx, sy),
    }
}

fn parse_exif_orientation(exif: &std::collections::HashMap<String, String>) -> u8 {
    let Some(raw) = exif.get("Orientation") else {
        return 1;
    };
    let trimmed = raw.trim().trim_matches('"');
    if let Ok(code) = trimmed.parse::<u8>()
        && (1..=8).contains(&code)
    {
        return code;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("90 cw") || lower.contains("rotate 90") {
        6
    } else if lower.contains("270 cw") || lower.contains("90 ccw") {
        8
    } else if lower.contains("180") {
        3
    } else {
        1
    }
}

pub fn cluster_shoot_sensor_dust(
    frame_spots: &[(usize, Vec<SensorDustCandidate>)],
    min_frame_fraction: f32,
    camera_key: &str,
) -> Vec<SensorDustMapSpot> {
    let total_frames = frame_spots.len();
    if total_frames == 0 {
        return Vec::new();
    }
    let frac = min_frame_fraction.clamp(0.1, 1.0);
    let min_hits = if total_frames <= 1 {
        1
    } else {
        ((total_frames as f32 * frac).ceil() as usize)
            .max(2)
            .min(total_frames)
    };

    struct Cluster {
        sum_x: f32,
        sum_y: f32,
        radii: Vec<u32>,
        frames: Vec<usize>,
    }

    let mut clusters: Vec<Cluster> = Vec::new();
    for (frame_idx, spots) in frame_spots {
        for s in spots {
            let best_idx = clusters
                .iter()
                .enumerate()
                .filter_map(|(ci, c)| {
                    if c.frames.contains(frame_idx) {
                        return None;
                    }
                    let n = c.frames.len() as f32;
                    let cx = c.sum_x / n;
                    let cy = c.sum_y / n;
                    let d = (cx - s.norm_x).hypot(cy - s.norm_y);
                    (d <= 0.008).then_some((ci, d))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(ci, _)| ci);

            if let Some(ci) = best_idx {
                let c = &mut clusters[ci];
                c.sum_x += s.norm_x;
                c.sum_y += s.norm_y;
                c.radii.push(s.radius);
                c.frames.push(*frame_idx);
            } else {
                clusters.push(Cluster {
                    sum_x: s.norm_x,
                    sum_y: s.norm_y,
                    radii: vec![s.radius],
                    frames: vec![*frame_idx],
                });
            }
        }
    }

    clusters
        .into_iter()
        .enumerate()
        .filter_map(|(idx, mut c)| {
            let hit_count = c.frames.len();
            if hit_count < min_hits {
                return None;
            }
            let sensor_x = (c.sum_x / hit_count as f32).clamp(0.0, 1.0);
            let sensor_y = (c.sum_y / hit_count as f32).clamp(0.0, 1.0);
            if crate::enhanced::local_priors::is_dust_spot_suppressed(camera_key, sensor_x, sensor_y) {
                return None;
            }
            c.radii.sort_unstable();
            let median_radius = c.radii[c.radii.len() / 2] as f32;
            Some(SensorDustMapSpot {
                id: format!("dust_map_{}", idx),
                sensor_x,
                sensor_y,
                median_radius,
                hit_count,
                frame_count: total_frames,
                confidence: (hit_count as f32 / total_frames as f32).clamp(0.0, 1.0),
                camera_key: camera_key.to_string(),
            })
        })
        .collect()
}

/// Detects candidate dark and bright circular sensor dust spots full-frame across multiple scales,
/// guarded by low surrounding ring variance and starfield protection.
pub fn detect_dust_candidate_spots(image: &DynamicImage) -> Vec<SensorDustCandidate> {
    let (orig_w, orig_h) = image.dimensions();
    if orig_w < 64 || orig_h < 64 {
        return Vec::new();
    }

    let (width, height, luma, is_starfield_scene) = build_defect_analysis_luma(image);
    if width < 32 || height < 32 {
        return Vec::new();
    }

    let w_usize = width as usize;
    const RADII: [f32; 4] = [3.0, 6.0, 12.0, 20.0];
    let mut scored: Vec<(f32, SensorDustCandidate)> = Vec::new();

    for &r in &RADII {
        let margin = (r as u32 + 4).max(10);
        if width <= margin * 2 || height <= margin * 2 {
            continue;
        }
        let ring_offsets: [(f32, f32); 8] = std::array::from_fn(|s| {
            let angle = (s as f32 / 8.0) * 2.0 * PI;
            (angle.cos() * r, angle.sin() * r)
        });

        for y in margin..(height - margin) {
            for x in margin..(width - margin) {
                let lum_center = luma[y as usize * w_usize + x as usize];

                let mut ring_sum = 0.0f32;
                let mut ring_sq_sum = 0.0f32;
                for &(ox, oy) in &ring_offsets {
                    let sx = (x as f32 + ox).round().clamp(0.0, (width - 1) as f32) as usize;
                    let sy = (y as f32 + oy).round().clamp(0.0, (height - 1) as f32) as usize;
                    let rl = luma[sy * w_usize + sx];
                    ring_sum += rl;
                    ring_sq_sum += rl * rl;
                }
                let ring_mean = ring_sum / 8.0;
                let ring_var = (ring_sq_sum / 8.0 - ring_mean * ring_mean).max(0.0);
                let ring_std = ring_var.sqrt();

                if ring_std > 0.045 {
                    continue;
                }

                let dark_diff = ring_mean - lum_center;
                let bright_diff = lum_center - ring_mean;

                let is_dark_dust = dark_diff > 0.035 && dark_diff < 0.55 && ring_mean >= 0.06;
                let is_bright_dust = bright_diff > 0.045
                    && bright_diff < 0.85
                    && !(is_starfield_scene && ring_mean < 0.09);

                if is_dark_dust || is_bright_dust {
                    let peak_diff = if is_dark_dust { dark_diff } else { bright_diff };
                    let mut support = 0u32;
                    let thresh = peak_diff * 0.45;
                    for ny in -1i32..=1 {
                        for nx in -1i32..=1 {
                            if nx == 0 && ny == 0 {
                                continue;
                            }
                            let nl = luma[(y as i32 + ny) as usize * w_usize + (x as i32 + nx) as usize];
                            let nd = if is_dark_dust { ring_mean - nl } else { nl - ring_mean };
                            if nd >= thresh {
                                support += 1;
                            }
                        }
                    }
                    if support < 2 {
                        continue;
                    }

                    let norm_x = x as f32 / width as f32;
                    let norm_y = y as f32 / height as f32;
                    let score = peak_diff / (ring_std + 0.01);
                    scored.push((
                        score,
                        SensorDustCandidate {
                            norm_x,
                            norm_y,
                            radius: r.round() as u32,
                        },
                    ));
                }
            }
        }
    }

    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut spot_coords: Vec<SensorDustCandidate> = Vec::new();
    for (_, cand) in scored {
        let min_dx = (cand.radius as f32 * 1.5).max(12.0) / width as f32;
        let min_dy = (cand.radius as f32 * 1.5).max(12.0) / height as f32;
        let overlaps = spot_coords.iter().any(|s| {
            let ddx = (s.radius.max(cand.radius) as f32 * 1.5).max(12.0) / width as f32;
            let ddy = (s.radius.max(cand.radius) as f32 * 1.5).max(12.0) / height as f32;
            (s.norm_x - cand.norm_x).abs() < ddx.max(min_dx)
                && (s.norm_y - cand.norm_y).abs() < ddy.max(min_dy)
        });
        if !overlaps {
            spot_coords.push(cand);
        }
    }

    spot_coords
}

fn gaussian_blur_luma(src: &[f32], width: usize, height: usize, sigma: f32) -> Vec<f32> {
    let radius = (sigma * 2.5).ceil() as isize;
    let kernel: Vec<f32> = (-radius..=radius)
        .map(|d| (-(d * d) as f32 / (2.0 * sigma * sigma)).exp())
        .collect();
    let w_sum: f32 = kernel.iter().sum();
    let weights: Vec<f32> = kernel.into_iter().map(|v| v / w_sum).collect();

    let mut tmp = vec![0.0f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let mut acc = 0.0f32;
            for (ki, d) in (-radius..=radius).enumerate() {
                let sx = (x as isize + d).clamp(0, width as isize - 1) as usize;
                acc += src[y * width + sx] * weights[ki];
            }
            tmp[y * width + x] = acc;
        }
    }

    let mut dst = vec![0.0f32; width * height];
    for y in 0..height {
        for x in 0..width {
            let mut acc = 0.0f32;
            for (ki, d) in (-radius..=radius).enumerate() {
                let sy = (y as isize + d).clamp(0, height as isize - 1) as usize;
                acc += tmp[sy * width + x] * weights[ki];
            }
            dst[y * width + x] = acc;
        }
    }
    dst
}

fn detect_hessian_curved_scratches(
    luma: &[f32],
    width: u32,
    height: u32,
    is_starfield_scene: bool,
    existing: &mut Vec<ScratchCandidate>,
) {
    if is_starfield_scene || width < 48 || height < 48 {
        return;
    }
    let w = width as usize;
    let h = height as usize;
    let w_i = width as i32;
    let h_i = height as i32;
    let sample_raw = |x: i32, y: i32| -> f32 {
        let cx = x.clamp(0, w_i - 1) as usize;
        let cy = y.clamp(0, h_i - 1) as usize;
        luma[cy * w + cx]
    };

    const SCALES: [f32; 3] = [1.0, 2.0, 3.0];
    let n = w * h;
    let mut best_strength = vec![0.0f32; n];
    let mut best_sign = vec![0i8; n];
    let mut best_sigma = vec![1.0f32; n];
    let mut best_flank = vec![0.0f32; n];

    for &sigma in &SCALES {
        let smoothed = gaussian_blur_luma(luma, w, h, sigma);
        let margin = (sigma * 4.0).ceil() as i32 + 2;
        if w_i <= margin * 2 || h_i <= margin * 2 {
            continue;
        }
        let s2 = sigma * sigma;

        let mut ridge_map = vec![0.0f32; n];
        let mut nx_map = vec![0.0f32; n];
        let mut ny_map = vec![0.0f32; n];
        let mut sign_map = vec![0i8; n];
        let mut flank_map = vec![0.0f32; n];

        for y in margin..(h_i - margin) {
            for x in margin..(w_i - margin) {
                let idx = y as usize * w + x as usize;
                let c = smoothed[idx];
                let lxx = (smoothed[idx + 1] - 2.0 * c + smoothed[idx - 1]) * s2;
                let lyy = (smoothed[(y + 1) as usize * w + x as usize]
                    - 2.0 * c
                    + smoothed[(y - 1) as usize * w + x as usize])
                    * s2;
                let lxy = 0.25
                    * (smoothed[(y + 1) as usize * w + (x + 1) as usize]
                        - smoothed[(y + 1) as usize * w + (x - 1) as usize]
                        - smoothed[(y - 1) as usize * w + (x + 1) as usize]
                        + smoothed[(y - 1) as usize * w + (x - 1) as usize])
                    * s2;

                let trace = lxx + lyy;
                let disc = (lxx - lyy).hypot(2.0 * lxy);
                let e1 = 0.5 * (trace + disc);
                let e2 = 0.5 * (trace - disc);
                let (lambda1, lambda2) = if e1.abs() >= e2.abs() { (e1, e2) } else { (e2, e1) };

                if lambda1.abs() < 0.055 || (lambda2.abs() / lambda1.abs().max(1e-6)) > 0.35 {
                    continue;
                }

                // Principal eigenvector corresponding to lambda1 (normal across the scratch/hair)
                let (nx, ny) = if lxy.abs() > 1e-6 || (lxx - lambda1).abs() > 1e-6 {
                    let vx = lxy;
                    let vy = lambda1 - lxx;
                    let len = vx.hypot(vy);
                    if len > 1e-6 { (vx / len, vy / len) } else { (1.0, 0.0) }
                } else {
                    (0.0, 1.0)
                };

                let d1 = (2.5 * sigma).max(3.0);
                let d2 = (3.5 * sigma).max(4.0);
                let fa = sample_raw((x as f32 + nx * d1).round() as i32, (y as f32 + ny * d1).round() as i32);
                let fb = sample_raw((x as f32 - nx * d1).round() as i32, (y as f32 - ny * d1).round() as i32);
                let fa2 = sample_raw((x as f32 + nx * d2).round() as i32, (y as f32 + ny * d2).round() as i32);
                let fb2 = sample_raw((x as f32 - nx * d2).round() as i32, (y as f32 - ny * d2).round() as i32);
                if (fa - fb).abs() > 0.055
                    || (fa2 - fb2).abs() > 0.065
                    || (fa - fa2).abs() > 0.05
                    || (fb - fb2).abs() > 0.05
                {
                    continue;
                }
                let flank = 0.25 * (fa + fb + fa2 + fb2);
                let center_raw = sample_raw(x, y);
                let delta = center_raw - flank;
                if delta.abs() < 0.055 {
                    continue;
                }

                ridge_map[idx] = lambda1.abs();
                nx_map[idx] = nx;
                ny_map[idx] = ny;
                sign_map[idx] = if delta >= 0.0 { 1 } else { -1 };
                flank_map[idx] = flank;
            }
        }

        // Non-maximum suppression along normal direction
        for y in margin..(h_i - margin) {
            for x in margin..(w_i - margin) {
                let idx = y as usize * w + x as usize;
                let s = ridge_map[idx];
                if s <= 0.0 {
                    continue;
                }
                let nx = nx_map[idx].round() as i32;
                let ny = ny_map[idx].round() as i32;
                let p_idx = (y + ny) as usize * w + (x + nx) as usize;
                let n_idx = (y - ny) as usize * w + (x - nx) as usize;
                if s >= ridge_map[p_idx] && s >= ridge_map[n_idx] && s > best_strength[idx] {
                    best_strength[idx] = s;
                    best_sign[idx] = sign_map[idx];
                    best_sigma[idx] = sigma;
                    best_flank[idx] = flank_map[idx];
                }
            }
        }
    }

    // Trace 8-connected components of same-sign ridge pixels
    let mut visited = vec![false; n];
    for start_idx in 0..n {
        if visited[start_idx] || best_sign[start_idx] == 0 {
            continue;
        }
        let target_sign = best_sign[start_idx];
        let mut stack = vec![start_idx];
        visited[start_idx] = true;
        let mut comp: Vec<usize> = Vec::new();
        while let Some(cur) = stack.pop() {
            comp.push(cur);
            let cx = (cur % w) as i32;
            let cy = (cur / w) as i32;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = cx + dx;
                    let ny = cy + dy;
                    if nx < 0 || nx >= w_i || ny < 0 || ny >= h_i {
                        continue;
                    }
                    let nidx = ny as usize * w + nx as usize;
                    if !visited[nidx] && best_sign[nidx] == target_sign {
                        visited[nidx] = true;
                        stack.push(nidx);
                    }
                }
            }
        }

        if comp.len() < 10 {
            continue;
        }
        let m = comp.len() as f32;
        let mean_f = comp.iter().map(|&idx| best_flank[idx]).sum::<f32>() / m;
        let var_f = comp
            .iter()
            .map(|&idx| {
                let d = best_flank[idx] - mean_f;
                d * d
            })
            .sum::<f32>()
            / m;
        if var_f >= 0.0020 {
            continue;
        }

        // Longest path ordering inside component via 2-pass BFS diameter
        let mut comp_map = std::collections::HashMap::with_capacity(comp.len());
        for (li, &gi) in comp.iter().enumerate() {
            comp_map.insert(gi, li);
        }
        let bfs_furthest = |seed_local: usize| -> (usize, Vec<Option<usize>>) {
            let mut parent = vec![None; comp.len()];
            let mut dist = vec![usize::MAX; comp.len()];
            let mut q = std::collections::VecDeque::new();
            dist[seed_local] = 0;
            q.push_back(seed_local);
            let mut far = seed_local;
            while let Some(u) = q.pop_front() {
                if dist[u] > dist[far] {
                    far = u;
                }
                let gi = comp[u];
                let cx = (gi % w) as i32;
                let cy = (gi / w) as i32;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let nxx = cx + dx;
                        let nyy = cy + dy;
                        if nxx < 0 || nxx >= w_i || nyy < 0 || nyy >= h_i {
                            continue;
                        }
                        if let Some(&v) = comp_map.get(&(nyy as usize * w + nxx as usize))
                            && dist[v] == usize::MAX
                        {
                            dist[v] = dist[u] + 1;
                            parent[v] = Some(u);
                            q.push_back(v);
                        }
                    }
                }
            }
            (far, parent)
        };
        let (end_a, _) = bfs_furthest(0);
        let (end_b, parent_b) = bfs_furthest(end_a);
        let mut path_local = Vec::new();
        let mut cur = Some(end_b);
        while let Some(u) = cur {
            path_local.push(u);
            cur = parent_b[u];
        }
        path_local.reverse();
        if path_local.len() < 8 {
            continue;
        }

        let step = (path_local.len() / 8).max(3).min(6);
        let mut points: Vec<[f32; 2]> = path_local
            .iter()
            .enumerate()
            .filter(|&(i, _)| i == 0 || i + 1 == path_local.len() || i % step == 0)
            .map(|(_, &li)| {
                let gi = comp[li];
                [(gi % w) as f32 / width as f32, (gi / w) as f32 / height as f32]
            })
            .collect();
        if points.len() < 2 {
            continue;
        }
        let first = points[0];
        let last = *points.last().unwrap();
        let mid_x = 0.5 * (first[0] + last[0]);
        let mid_y = 0.5 * (first[1] + last[1]);
        let already = existing.iter().any(|ex| {
            let ex_mx = 0.5 * (ex.norm_start_x + ex.norm_end_x);
            let ex_my = 0.5 * (ex.norm_start_y + ex.norm_end_y);
            (ex_mx - mid_x).hypot(ex_my - mid_y) < 0.025
        });
        if already {
            continue;
        }
        let avg_sigma = comp.iter().map(|&gi| best_sigma[gi]).sum::<f32>() / m;
        let max_contrast = comp
            .iter()
            .map(|&gi| best_strength[gi])
            .fold(0.0f32, f32::max);
        existing.push(ScratchCandidate {
            norm_start_x: first[0],
            norm_start_y: first[1],
            norm_end_x: last[0],
            norm_end_y: last[1],
            width_px: (avg_sigma * 2.0).clamp(1.5, 8.0),
            contrast: max_contrast,
            points: std::mem::take(&mut points),
        });
    }
}

/// Detects anisotropic linear scratch candidates and curved hairs across multiple scales,
/// guarded by smooth flanking bands so step edges, textures, and starfields
/// produce zero false positives.
pub fn detect_linear_scratch_candidates(image: &DynamicImage) -> Vec<ScratchCandidate> {
    let (orig_w, orig_h) = image.dimensions();
    if orig_w < 64 || orig_h < 64 {
        return Vec::new();
    }

    let (width, height, luma, is_starfield) = build_defect_analysis_luma(image);
    if width < 32 || height < 32 {
        return Vec::new();
    }
    let w_i = width as i32;
    let h_i = height as i32;
    let sample = |x: i32, y: i32| -> f32 {
        let cx = x.clamp(0, w_i - 1) as usize;
        let cy = y.clamp(0, h_i - 1) as usize;
        luma[cy * width as usize + cx]
    };

    const DIRS: [((i32, i32), (i32, i32)); 4] = [
        ((1, 0), (0, 1)),
        ((0, 1), (1, 0)),
        ((1, 1), (-1, 1)),
        ((1, -1), (1, 1)),
    ];

    let mut results: Vec<ScratchCandidate> = Vec::new();

    for ((tx, ty), (nx, ny)) in DIRS {
        let ridge_signal = |x: i32, y: i32| -> Option<(f32, f32)> {
            if x < 5 || x >= w_i - 5 || y < 5 || y >= h_i - 5 {
                return None;
            }
            let fa = sample(x + nx * 3, y + ny * 3);
            let fb = sample(x - nx * 3, y - ny * 3);
            let fa2 = sample(x + nx * 4, y + ny * 4);
            let fb2 = sample(x - nx * 4, y - ny * 4);
            if (fa - fb).abs() > 0.055 || (fa2 - fb2).abs() > 0.065 || (fa - fa2).abs() > 0.05 || (fb - fb2).abs() > 0.05 {
                return None;
            }
            let flank = 0.25 * (fa + fb + fa2 + fb2);
            let c0 = sample(x, y);
            let delta = c0 - flank;
            if delta.abs() < 0.065 {
                return None;
            }
            Some((delta, flank))
        };

        let mut starts: Vec<(i32, i32)> = Vec::new();
        if tx == 1 && ty == 0 {
            for y in 5..(h_i - 5) {
                starts.push((5, y));
            }
        } else if tx == 0 && ty == 1 {
            for x in 5..(w_i - 5) {
                starts.push((x, 5));
            }
        } else if tx == 1 && ty == 1 {
            for x in 5..(w_i - 5) {
                starts.push((x, 5));
            }
            for y in 6..(h_i - 5) {
                starts.push((5, y));
            }
        } else {
            for x in 5..(w_i - 5) {
                starts.push((x, h_i - 6));
            }
            for y in 5..(h_i - 6) {
                starts.push((5, y));
            }
        }

        for (sx, sy) in starts {
            let mut x = sx;
            let mut y = sy;
            let mut run_start: Option<(i32, i32)> = None;
            let mut run_end = (sx, sy);
            let mut run_len = 0u32;
            let mut sign = 0.0f32;
            let mut sum_delta = 0.0f32;
            let mut flanks: Vec<f32> = Vec::new();

            let flush_run = |results: &mut Vec<ScratchCandidate>,
                             run_start: &mut Option<(i32, i32)>,
                             run_end: (i32, i32),
                             run_len: &mut u32,
                             sum_delta: &mut f32,
                             flanks: &mut Vec<f32>| {
                if let Some((rx0, ry0)) = *run_start
                    && *run_len >= 10
                {
                    let n = flanks.len().max(1) as f32;
                    let mean_f = flanks.iter().sum::<f32>() / n;
                    let var_f = flanks
                        .iter()
                        .map(|v| (v - mean_f) * (v - mean_f))
                        .sum::<f32>()
                        / n;
                    if var_f < 0.0020 {
                        let nsx = rx0 as f32 / width as f32;
                        let nsy = ry0 as f32 / height as f32;
                        let nex = run_end.0 as f32 / width as f32;
                        let ney = run_end.1 as f32 / height as f32;
                        let already = results.iter().any(|existing| {
                            let d1 = (existing.norm_start_x - nsx).hypot(existing.norm_start_y - nsy);
                            let d2 = (existing.norm_end_x - nex).hypot(existing.norm_end_y - ney);
                            (d1 < 0.04 && d2 < 0.04)
                                || (existing.norm_start_x - 0.5 * (nsx + nex))
                                    .hypot(existing.norm_start_y - 0.5 * (nsy + ney))
                                    < 0.02
                        });
                        if !already {
                            results.push(ScratchCandidate {
                                norm_start_x: nsx,
                                norm_start_y: nsy,
                                norm_end_x: nex,
                                norm_end_y: ney,
                                width_px: 2.0,
                                contrast: (*sum_delta / (*run_len as f32)).abs(),
                                points: vec![[nsx, nsy], [nex, ney]],
                            });
                        }
                    }
                }
                *run_start = None;
                *run_len = 0;
                *sum_delta = 0.0;
                flanks.clear();
            };

            while x >= 5 && x < w_i - 5 && y >= 5 && y < h_i - 5 {
                if let Some((delta, flank)) = ridge_signal(x, y) {
                    let s = delta.signum();
                    if run_start.is_none() || (sign != s) {
                        flush_run(
                            &mut results,
                            &mut run_start,
                            run_end,
                            &mut run_len,
                            &mut sum_delta,
                            &mut flanks,
                        );
                        run_start = Some((x, y));
                        sign = s;
                    }
                    run_end = (x, y);
                    run_len += 1;
                    sum_delta += delta;
                    flanks.push(flank);
                } else {
                    flush_run(
                        &mut results,
                        &mut run_start,
                        run_end,
                        &mut run_len,
                        &mut sum_delta,
                        &mut flanks,
                    );
                }
                x += tx;
                y += ty;
            }
            flush_run(
                &mut results,
                &mut run_start,
                run_end,
                &mut run_len,
                &mut sum_delta,
                &mut flanks,
            );
        }
    }

    detect_hessian_curved_scratches(&luma, width, height, is_starfield, &mut results);

    results
}

pub fn compute_ring_gradient_energy(
    image: &DynamicImage,
    norm_x: f32,
    norm_y: f32,
    radius_px: f32,
) -> f32 {
    let (w, h) = image.dimensions();
    if w < 16 || h < 16 {
        return 0.0;
    }
    let rgb = image.to_rgb32f();
    let cx = (norm_x * w as f32).round();
    let cy = (norm_y * h as f32).round();
    let r_in = radius_px.max(2.0) + 2.0;
    let r_out = r_in + 3.0;
    let min_x = (cx - r_out).max(1.0) as u32;
    let max_x = (cx + r_out).min((w - 2) as f32) as u32;
    let min_y = (cy - r_out).max(1.0) as u32;
    let max_y = (cy + r_out).min((h - 2) as f32) as u32;

    let luma_at = |x: u32, y: u32| -> f32 {
        let p = rgb.get_pixel(x, y);
        0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
    };

    let mut sum = 0.0f32;
    let mut count = 0u32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let d = (x as f32 - cx).hypot(y as f32 - cy);
            if d >= r_in && d <= r_out {
                let gx = luma_at(x + 1, y) - luma_at(x - 1, y);
                let gy = luma_at(x, y + 1) - luma_at(x, y - 1);
                sum += gx.hypot(gy);
                count += 1;
            }
        }
    }
    if count == 0 { 0.0 } else { sum / count as f32 }
}

pub fn should_route_defect_to_lama(radius_px: f32, ring_grad_energy: f32, kind: &str) -> bool {
    radius_px > 14.0 || ring_grad_energy > 0.06 || (kind == "scratch" && radius_px > 4.5)
}

/// Verifies dust spots across images in a photoshoot burst.
/// A spot is verified if at least one other image has a candidate within normalized distance <= 0.008.
/// If peer_spots is empty (single photo), all candidates are kept.
pub fn cross_frame_verify_dust(
    current_spots: &[SensorDustCandidate],
    peer_spots: &[Vec<SensorDustCandidate>],
) -> Vec<SensorDustCandidate> {
    if peer_spots.is_empty() {
        return current_spots.to_vec();
    }

    current_spots
        .iter()
        .filter(|spot| {
            peer_spots.iter().any(|frame_spots| {
                frame_spots.iter().any(|peer| {
                    let dx = spot.norm_x - peer.norm_x;
                    let dy = spot.norm_y - peer.norm_y;
                    (dx * dx + dy * dy).sqrt() <= 0.008
                })
            })
        })
        .cloned()
        .collect()
}

fn heal_scratch_polyline(rgb_img: &mut image::Rgb32FImage, points_px: &[(f32, f32)], radius: f32) {
    if points_px.len() < 2 {
        return;
    }
    let (width, height) = rgb_img.dimensions();
    let r = radius.clamp(1.5, 24.0);
    let orig = rgb_img.clone();

    for seg in points_px.windows(2) {
        let (x0, y0) = seg[0];
        let (x1, y1) = seg[1];
        let seg_dx = x1 - x0;
        let seg_dy = y1 - y0;
        let seg_len = seg_dx.hypot(seg_dy);
        if seg_len < 0.5 {
            continue;
        }
        let (tx, ty) = (seg_dx / seg_len, seg_dy / seg_len);
        let (nx, ny) = (-ty, tx);

        let pad = (r + 2.0).ceil() as i32;
        let min_px = (x0.min(x1).floor() as i32 - pad).clamp(0, width as i32 - 1);
        let max_px = (x0.max(x1).ceil() as i32 + pad).clamp(0, width as i32 - 1);
        let min_py = (y0.min(y1).floor() as i32 - pad).clamp(0, height as i32 - 1);
        let max_py = (y0.max(y1).ceil() as i32 + pad).clamp(0, height as i32 - 1);

        for py in min_py..=max_py {
            for px in min_px..=max_px {
                let rx = px as f32 - x0;
                let ry = py as f32 - y0;
                let along = (rx * tx + ry * ty).clamp(0.0, seg_len);
                let closest_x = x0 + tx * along;
                let closest_y = y0 + ty * along;
                let dist = (px as f32 - closest_x).hypot(py as f32 - closest_y);
                if dist <= r {
                    let flank_dist = r + 2.5;
                    let fx_a = (closest_x + nx * flank_dist).round().clamp(0.0, (width - 1) as f32) as u32;
                    let fy_a = (closest_y + ny * flank_dist).round().clamp(0.0, (height - 1) as f32) as u32;
                    let fx_b = (closest_x - nx * flank_dist).round().clamp(0.0, (width - 1) as f32) as u32;
                    let fy_b = (closest_y - ny * flank_dist).round().clamp(0.0, (height - 1) as f32) as u32;
                    let pa = orig.get_pixel(fx_a, fy_a);
                    let pb = orig.get_pixel(fx_b, fy_b);
                    let target = [0.5 * (pa[0] + pb[0]), 0.5 * (pa[1] + pb[1]), 0.5 * (pa[2] + pb[2])];
                    let orig_p = orig.get_pixel(px as u32, py as u32);
                    let w = (1.0 - (dist / r).clamp(0.0, 1.0)).powi(2);
                    rgb_img.put_pixel(
                        px as u32,
                        py as u32,
                        Rgb([
                            orig_p[0] * (1.0 - w) + target[0] * w,
                            orig_p[1] * (1.0 - w) + target[1] * w,
                            orig_p[2] * (1.0 - w) + target[2] * w,
                        ]),
                    );
                }
            }
        }
    }
}

/// Inpaints verified sensor dust spots on the image.
pub fn heal_verified_dust_spots(
    image: &mut DynamicImage,
    verified_spots: &[SensorDustCandidate],
) -> DustSpotHealResult {
    let (width, height) = image.dimensions();
    if width < 64 || height < 64 || verified_spots.is_empty() {
        return DustSpotHealResult {
            spots_detected: verified_spots.len(),
            message: "No spots to heal".to_string(),
        };
    }

    let mut rgb_img = image.to_rgb32f();

    for spot in verified_spots {
        let sx = (spot.norm_x * width as f32).round().clamp(0.0, width as f32 - 1.0) as u32;
        let sy = (spot.norm_y * height as f32).round().clamp(0.0, height as f32 - 1.0) as u32;
        let r = spot.radius;
        let r_f = r as f32;

        for dy in -(r as i32)..=(r as i32) {
            for dx in -(r as i32)..=(r as i32) {
                let dist = ((dx * dx + dy * dy) as f32).sqrt();
                if dist <= r_f {
                    let px = (sx as i32 + dx).clamp(0, width as i32 - 1) as u32;
                    let py = (sy as i32 + dy).clamp(0, height as i32 - 1) as u32;

                    // Interpolate smoothly from outer boundary ring
                    let factor = (dist / r_f).clamp(0.0, 1.0);
                    let edge_x = (sx as f32 + (dx as f32 / dist.max(0.1)) * (r_f + 2.0)).clamp(0.0, width as f32 - 1.0) as u32;
                    let edge_y = (sy as f32 + (dy as f32 / dist.max(0.1)) * (r_f + 2.0)).clamp(0.0, height as f32 - 1.0) as u32;
                    let edge_p = *rgb_img.get_pixel(edge_x, edge_y);

                    let orig_p = rgb_img.get_pixel(px, py);
                    let blend_w = (1.0 - factor).powi(2);

                    let healed = Rgb([
                        orig_p[0] * (1.0 - blend_w) + edge_p[0] * blend_w,
                        orig_p[1] * (1.0 - blend_w) + edge_p[1] * blend_w,
                        orig_p[2] * (1.0 - blend_w) + edge_p[2] * blend_w,
                    ]);
                    rgb_img.put_pixel(px, py, healed);
                }
            }
        }
    }

    *image = DynamicImage::ImageRgb32F(rgb_img);

    DustSpotHealResult {
        spots_detected: verified_spots.len(),
        message: format!("Successfully healed {} verified dust spot(s)", verified_spots.len()),
    }
}

/// Applies non-destructive analytical dust & scratch healing stored in `aiPatches` without precomputed `patchData`.
pub fn apply_sensor_dust_heal_if_enabled<'a>(
    image: std::borrow::Cow<'a, DynamicImage>,
    adjustments: &serde_json::Value,
) -> std::borrow::Cow<'a, DynamicImage> {
    let Some(patches) = adjustments.get("aiPatches").and_then(|v| v.as_array()) else {
        return image;
    };
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return image;
    }

    let mut spots: Vec<SensorDustCandidate> = Vec::new();
    let mut scratches: Vec<(Vec<(f32, f32)>, f32)> = Vec::new();

    for p in patches {
        if p.get("type").and_then(|v| v.as_str()) != Some("heal")
            || !p.get("patchData").is_none_or(|v| v.is_null())
            || p.get("visible").and_then(|v| v.as_bool()) == Some(false)
        {
            continue;
        }
        let kind = p.get("kind").and_then(|v| v.as_str()).unwrap_or("dust");
        let radius = p.get("radius").and_then(|v| v.as_f64()).unwrap_or(6.0) as f32;

        if kind == "scratch" {
            let mut pts: Vec<(f32, f32)> = p
                .get("points")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|pt| {
                            if let Some(pair) = pt.as_array()
                                && pair.len() >= 2
                            {
                                Some((
                                    pair[0].as_f64()? as f32,
                                    pair[1].as_f64()? as f32,
                                ))
                            } else {
                                Some((
                                    pt.get("x")?.as_f64()? as f32,
                                    pt.get("y")?.as_f64()? as f32,
                                ))
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            if pts.len() < 2
                && let (Some(x), Some(y), Some(ex), Some(ey)) = (
                    p.get("x").and_then(|v| v.as_f64()),
                    p.get("y").and_then(|v| v.as_f64()),
                    p.get("endX").and_then(|v| v.as_f64()),
                    p.get("endY").and_then(|v| v.as_f64()),
                )
            {
                pts = vec![(x as f32, y as f32), (ex as f32, ey as f32)];
            }
            if pts.len() >= 2 {
                scratches.push((pts, radius));
            }
        } else if let (Some(x), Some(y)) = (
            p.get("x").and_then(|v| v.as_f64()),
            p.get("y").and_then(|v| v.as_f64()),
        ) {
            spots.push(SensorDustCandidate {
                norm_x: (x as f32 / w as f32).clamp(0.0, 1.0),
                norm_y: (y as f32 / h as f32).clamp(0.0, 1.0),
                radius: radius.round().max(1.0) as u32,
            });
        }
    }

    if spots.is_empty() && scratches.is_empty() {
        return image;
    }

    let mut owned = image.into_owned();
    if !spots.is_empty() {
        let _ = heal_verified_dust_spots(&mut owned, &spots);
    }
    if !scratches.is_empty() {
        let mut rgb = owned.to_rgb32f();
        for (pts, r) in &scratches {
            heal_scratch_polyline(&mut rgb, pts, *r);
        }
        owned = DynamicImage::ImageRgb32F(rgb);
    }
    std::borrow::Cow::Owned(owned)
}

pub fn heal_sky_dust_spots(image: &mut DynamicImage) -> DustSpotHealResult {
    let spots = detect_dust_candidate_spots(image);
    heal_verified_dust_spots(image, &spots)
}

/// Scans the peripheral 3% perimeter for high-contrast truncation artifacts (cut-off limbs, poles)
/// and returns an inward crop box (1% - 3%) if distractions are present.
pub fn scan_peripheral_edge_patrol(image: &DynamicImage) -> EdgePatrolCrop {
    let (width, height) = image.dimensions();
    if width < 64 || height < 64 {
        return EdgePatrolCrop { crop_x: 0, crop_y: 0, width, height, edge_distractions_found: false };
    }

    let thumb = image.thumbnail(256, 256).to_luma8();
    let (tw, th) = thumb.dimensions();
    let margin_x = ((tw as f32) * 0.03).max(1.0) as u32;
    let margin_y = ((th as f32) * 0.03).max(1.0) as u32;

    let mut high_contrast_boundary = false;

    // Check top & bottom borders
    for x in 0..tw {
        for y in 0..margin_y {
            let p1 = thumb.get_pixel(x, y)[0] as i32;
            let p2 = thumb.get_pixel(x, (y + 1).min(th - 1))[0] as i32;
            if (p1 - p2).abs() > 45 {
                high_contrast_boundary = true;
                break;
            }
        }
        if high_contrast_boundary { break; }
        for y in (th - margin_y)..th {
            let p1 = thumb.get_pixel(x, y)[0] as i32;
            let p2 = thumb.get_pixel(x, y.saturating_sub(1))[0] as i32;
            if (p1 - p2).abs() > 45 {
                high_contrast_boundary = true;
                break;
            }
        }
        if high_contrast_boundary { break; }
    }

    // Check left & right borders
    if !high_contrast_boundary {
        for y in 0..th {
            for x in 0..margin_x {
                let p1 = thumb.get_pixel(x, y)[0] as i32;
                let p2 = thumb.get_pixel((x + 1).min(tw - 1), y)[0] as i32;
                if (p1 - p2).abs() > 45 {
                    high_contrast_boundary = true;
                    break;
                }
            }
            if high_contrast_boundary { break; }
            for x in (tw - margin_x)..tw {
                let p1 = thumb.get_pixel(x, y)[0] as i32;
                let p2 = thumb.get_pixel(x.saturating_sub(1), y)[0] as i32;
                if (p1 - p2).abs() > 45 {
                    high_contrast_boundary = true;
                    break;
                }
            }
            if high_contrast_boundary { break; }
        }
    }

    if high_contrast_boundary {
        let inset_x = ((width as f32) * 0.02).round() as u32;
        let inset_y = ((height as f32) * 0.02).round() as u32;
        let new_w = width.saturating_sub(inset_x * 2);
        let new_h = height.saturating_sub(inset_y * 2);
        EdgePatrolCrop {
            crop_x: inset_x,
            crop_y: inset_y,
            width: new_w,
            height: new_h,
            edge_distractions_found: true,
        }
    } else {
        EdgePatrolCrop {
            crop_x: 0,
            crop_y: 0,
            width,
            height,
            edge_distractions_found: false,
        }
    }
}

/// Guarantees framing with >= 6.5% headroom and >= 4.5% lateral limb margin around primary subject.
pub fn calculate_convex_hull_safe_framing(image: &DynamicImage, aspect_ratio: f32) -> SaliencyCropResult {
    let base_crop = calculate_saliency_crop(image, aspect_ratio);

    let mut crop_y = base_crop.y;
    let mut crop_h = base_crop.height;
    let mut crop_x = base_crop.x;
    let mut crop_w = base_crop.width;

    // Minimum headroom clamp
    if crop_y < 6.5 {
        crop_y = 0.0;
        crop_h = (crop_h + 6.5).min(100.0);
    }

    // Minimum limb buffer clamp
    if crop_x < 4.5 {
        crop_x = 0.0;
        crop_w = (crop_w + 4.5).min(100.0);
    }
    if (crop_x + crop_w) > 95.5 {
        crop_w = (100.0 - crop_x).min(100.0);
    }

    SaliencyCropResult {
        x: crop_x,
        y: crop_y,
        width: crop_w,
        height: crop_h,
        target_aspect_ratio: base_crop.target_aspect_ratio,
    }
}

#[tauri::command]
pub fn detect_auto_horizon(state: State<AppState>) -> Result<AutoHorizonResult, String> {
    if let Ok(preview_guard) = state.cached_preview.lock()
        && let Some(cached) = &*preview_guard
    {
        return Ok(detect_horizon_angle(&cached.preview.image));
    }

    let orig_guard = state.original_image.lock().map_err(|e| e.to_string())?;
    if let Some(loaded_image) = &*orig_guard {
        Ok(detect_horizon_angle(&loaded_image.image))
    } else {
        Err("No active image loaded".to_string())
    }
}

#[tauri::command]
pub fn get_saliency_crop_recommendation(
    aspect_ratio: f32,
    orientation_steps: Option<u8>,
    flip_horizontal: Option<bool>,
    flip_vertical: Option<bool>,
    state: State<AppState>,
) -> Result<SaliencyCropResult, String> {
    let orig_guard = state.original_image.lock().map_err(|e| e.to_string())?;
    if let Some(loaded_image) = &*orig_guard {
        let mut cow_img = Cow::Borrowed(loaded_image.image.as_ref());
        let flip_h = flip_horizontal.unwrap_or(false);
        let flip_v = flip_vertical.unwrap_or(false);
        if flip_h || flip_v {
            cow_img = crate::render::geometry::apply_flip(cow_img, flip_h, flip_v);
        }
        if let Some(steps) = orientation_steps {
            if steps > 0 {
                cow_img = crate::render::geometry::apply_coarse_rotation(cow_img, steps);
            }
        }
        Ok(calculate_saliency_crop(&cow_img, aspect_ratio))
    } else {
        Err("No active image loaded".to_string())
    }
}

#[tauri::command]
pub fn auto_remove_dust_and_blemishes(
    state: State<AppState>,
) -> Result<AutoDefectRepairResult, String> {
    let orig_guard = state.original_image.lock().map_err(|e| e.to_string())?;
    if let Some(loaded_image) = &*orig_guard {
        let (width, height) = loaded_image.image.dimensions();
        let path_str = loaded_image.path.clone();
        let (src_path, _) = crate::library::sidecars::parse_virtual_path(&path_str);
        let cam_key = if let Some(map) =
            crate::metadata::exif_processing::read_rrexif_sidecar(&src_path)
            && let (Some(make), Some(model)) = (map.get("Make"), map.get("Model"))
        {
            crate::enhanced::local_priors::normalize_camera_key(make, model)
        } else if let Ok(mapped) = crate::library::file_ops::read_file_mapped(&src_path)
            && let Some((make, model)) =
                crate::metadata::exif_processing::read_camera_make_model(&path_str, &mapped)
        {
            crate::enhanced::local_priors::normalize_camera_key(&make, &model)
        } else {
            "universal".to_string()
        };

        let dust_spots = detect_dust_candidate_spots(&loaded_image.image);
        let scratches = detect_linear_scratch_candidates(&loaded_image.image);

        let mut spots: Vec<DetectedSpot> = dust_spots
            .into_iter()
            .filter(|s| !crate::enhanced::local_priors::is_dust_spot_suppressed(&cam_key, s.norm_x, s.norm_y))
            .map(|s| {
                let rad = s.radius as f32;
                let grad = compute_ring_gradient_energy(&loaded_image.image, s.norm_x, s.norm_y, rad);
                let routing = if should_route_defect_to_lama(rad, grad, "dust") {
                    "lama".to_string()
                } else {
                    "analytical".to_string()
                };
                DetectedSpot {
                    x: s.norm_x * width as f32,
                    y: s.norm_y * height as f32,
                    radius: rad,
                    norm_x: s.norm_x,
                    norm_y: s.norm_y,
                    kind: "dust".to_string(),
                    end_x: None,
                    end_y: None,
                    norm_end_x: None,
                    norm_end_y: None,
                    points: Vec::new(),
                    routing,
                }
            })
            .collect();

        for persistent in crate::enhanced::local_priors::persistent_dust_candidates(&cam_key) {
            let already = spots.iter().any(|s| {
                s.kind == "dust"
                    && (s.norm_x - persistent.norm_x).hypot(s.norm_y - persistent.norm_y)
                        <= crate::enhanced::local_priors::DUST_CONFIRM_RADIUS
            });
            if !already {
                let rad = persistent.radius;
                let grad = compute_ring_gradient_energy(&loaded_image.image, persistent.norm_x, persistent.norm_y, rad);
                let routing = if should_route_defect_to_lama(rad, grad, "dust") {
                    "lama".to_string()
                } else {
                    "analytical".to_string()
                };
                spots.push(DetectedSpot {
                    x: persistent.norm_x * width as f32,
                    y: persistent.norm_y * height as f32,
                    radius: rad,
                    norm_x: persistent.norm_x,
                    norm_y: persistent.norm_y,
                    kind: "dust".to_string(),
                    end_x: None,
                    end_y: None,
                    norm_end_x: None,
                    norm_end_y: None,
                    points: Vec::new(),
                    routing,
                });
            }
        }
        let dust_count = spots.iter().filter(|s| s.kind == "dust").count();
        let scratch_count = scratches.len();

        for sc in scratches {
            let mx = 0.5 * (sc.norm_start_x + sc.norm_end_x);
            let my = 0.5 * (sc.norm_start_y + sc.norm_end_y);
            let rad = (sc.width_px * 1.5).max(3.0);
            let grad = compute_ring_gradient_energy(&loaded_image.image, mx, my, rad);
            let routing = if should_route_defect_to_lama(rad, grad, "scratch") {
                "lama".to_string()
            } else {
                "analytical".to_string()
            };
            let px_points: Vec<[f32; 2]> = sc
                .points
                .iter()
                .map(|p| [p[0] * width as f32, p[1] * height as f32])
                .collect();
            spots.push(DetectedSpot {
                x: sc.norm_start_x * width as f32,
                y: sc.norm_start_y * height as f32,
                radius: rad,
                norm_x: sc.norm_start_x,
                norm_y: sc.norm_start_y,
                kind: "scratch".to_string(),
                end_x: Some(sc.norm_end_x * width as f32),
                end_y: Some(sc.norm_end_y * height as f32),
                norm_end_x: Some(sc.norm_end_x),
                norm_end_y: Some(sc.norm_end_y),
                points: px_points,
                routing,
            });
        }

        let count = spots.len();
        Ok(AutoDefectRepairResult {
            spots,
            total_detected: count,
            message: format!(
                "Detected {} defect(s): {} dust spot(s), {} scratch/hair segment(s)",
                count, dust_count, scratch_count
            ),
        })
    } else {
        Err("No active image loaded".to_string())
    }
}

fn read_frame_camera_and_orientation(path_str: &str) -> (String, u8) {
    let (src_path, _) = parse_virtual_path(path_str);
    if let Some(map) = crate::metadata::exif_processing::read_rrexif_sidecar(&src_path) {
        let cam = match (map.get("Make"), map.get("Model")) {
            (Some(make), Some(model)) => {
                crate::enhanced::local_priors::normalize_camera_key(make, model)
            }
            _ => "universal".to_string(),
        };
        let orient = parse_exif_orientation(&map);
        return (cam, orient);
    }
    if let Ok(mapped) = read_file_mapped(&src_path) {
        let cam = crate::metadata::exif_processing::read_camera_make_model(path_str, &mapped)
            .map(|(mk, md)| crate::enhanced::local_priors::normalize_camera_key(&mk, &md))
            .unwrap_or_else(|| "universal".to_string());
        let exif_map = crate::metadata::sidecar::read_exif_data(path_str, &mapped);
        return (cam, parse_exif_orientation(&exif_map));
    }
    ("universal".to_string(), 1)
}

#[tauri::command]
pub async fn build_shoot_sensor_dust_map(
    paths: Vec<String>,
    min_frame_fraction: Option<f32>,
    app_handle: AppHandle,
) -> Result<ShootDustMapReport, String> {
    if paths.is_empty() {
        return Err("No photos provided to build sensor dust map".to_string());
    }
    let settings = crate::app::settings::load_settings(app_handle.clone()).unwrap_or_default();
    let frac = min_frame_fraction.unwrap_or(0.4).clamp(0.1, 1.0);

    tokio::task::spawn_blocking(move || {
        let per_frame: Vec<(usize, String, Vec<SensorDustCandidate>)> = paths
            .par_iter()
            .enumerate()
            .filter_map(|(idx, path_str)| {
                let (source_path, _) = parse_virtual_path(path_str);
                let (cam_key, orient) = read_frame_camera_and_orientation(path_str);
                let file_bytes = read_file_mapped(&source_path).ok()?;
                let img = load_base_image_from_bytes(
                    &file_bytes,
                    &source_path.to_string_lossy(),
                    true,
                    &settings,
                    None,
                )
                .ok()?;
                let raw_spots = detect_dust_candidate_spots(&img);
                let canonical: Vec<SensorDustCandidate> = raw_spots
                    .into_iter()
                    .map(|s| {
                        let (sx, sy) = canonicalize_to_sensor_coords(s.norm_x, s.norm_y, orient);
                        SensorDustCandidate {
                            norm_x: sx,
                            norm_y: sy,
                            radius: s.radius,
                        }
                    })
                    .collect();
                Some((idx, cam_key, canonical))
            })
            .collect();

        let camera_key = per_frame
            .first()
            .map(|x| x.1.clone())
            .unwrap_or_else(|| "universal".to_string());
        let frame_inputs: Vec<(usize, Vec<SensorDustCandidate>)> =
            per_frame.into_iter().map(|(i, _, s)| (i, s)).collect();
        let total_frames = frame_inputs.len();
        let spots = cluster_shoot_sensor_dust(&frame_inputs, frac, &camera_key);
        let msg = format!(
            "Mapped {} persistent sensor dust spot(s) across {} frame(s)",
            spots.len(),
            total_frames
        );
        Ok(ShootDustMapReport {
            camera_key,
            total_frames,
            spots,
            message: msg,
        })
    })
    .await
    .map_err(|e| format!("Build shoot dust map task failed: {}", e))?
}

#[tauri::command]
pub async fn apply_sensor_dust_map_to_shoot(
    paths: Vec<String>,
    spots: Vec<SensorDustMapSpot>,
    app_handle: AppHandle,
) -> Result<BatchDustSummary, String> {
    if paths.is_empty() {
        return Err("No photos provided".to_string());
    }
    let start_time = std::time::Instant::now();
    let total = paths.len();
    let settings = crate::app::settings::load_settings(app_handle.clone()).unwrap_or_default();

    let total_spots = tokio::task::spawn_blocking(move || {
        let counter = AtomicUsize::new(0);
        paths.par_iter().for_each(|path_str| {
            let (source_path, sidecar_path) = parse_virtual_path(path_str);
            let (cam_key, orient) = read_frame_camera_and_orientation(path_str);
            let mut metadata = load_sidecar(&sidecar_path);

            let (iw, ih) = if let Ok(file_bytes) = read_file_mapped(&source_path)
                && let Ok(img) = load_base_image_from_bytes(
                    &file_bytes,
                    &source_path.to_string_lossy(),
                    true,
                    &settings,
                    None,
                ) {
                img.dimensions()
            } else {
                (1000, 1000)
            };

            let entries: Vec<serde_json::Value> = spots
                .iter()
                .enumerate()
                .map(|(idx, sp)| {
                    let (dx, dy) = sensor_to_display_coords(sp.sensor_x, sp.sensor_y, orient);
                    crate::enhanced::local_priors::record_dust_spot_confirmation(
                        &cam_key,
                        sp.sensor_x,
                        sp.sensor_y,
                        sp.median_radius,
                        path_str,
                    );
                    serde_json::json!({
                        "id": format!("shoot_dust_{}_{}", idx, sp.id),
                        "name": format!("Shoot Dust #{}", idx + 1),
                        "isLoading": false,
                        "invert": false,
                        "patchData": serde_json::Value::Null,
                        "prompt": "",
                        "subMasks": [],
                        "visible": true,
                        "type": "heal",
                        "kind": "dust",
                        "label": "Shoot Sensor Dust Map",
                        "x": dx * iw as f32,
                        "y": dy * ih as f32,
                        "radius": sp.median_radius,
                    })
                })
                .collect();

            if let Some(obj) = metadata.adjustments.as_object_mut() {
                obj.insert(
                    "sensorDustHealedCount".to_string(),
                    serde_json::json!(entries.len()),
                );
                let existing = obj
                    .entry("aiPatches".to_string())
                    .or_insert_with(|| serde_json::json!([]));
                if let Some(arr) = existing.as_array_mut() {
                    arr.retain(|p| {
                        p.get("label").and_then(|v| v.as_str()) != Some("Shoot Sensor Dust Map")
                    });
                    arr.extend(entries);
                }
            }
            if let Ok(json_str) = serde_json::to_string_pretty(&metadata) {
                if let Some(parent) = sidecar_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = crate::library::file_ops::write_file_atomic(
                    &sidecar_path,
                    json_str.as_bytes(),
                );
            }
            counter.fetch_add(spots.len(), Ordering::SeqCst);
        });
        counter.load(Ordering::SeqCst)
    })
    .await
    .map_err(|e| format!("Apply shoot dust map failed: {}", e))?;

    Ok(BatchDustSummary {
        total_processed: total,
        total_spots_healed: total_spots,
        elapsed_ms: start_time.elapsed().as_millis() as u64,
    })
}

fn rasterize_defects_mask(width: u32, height: u32, spots: &[DetectedSpot]) -> image::GrayImage {
    let mut mask = image::GrayImage::new(width, height);
    let paint_disc = |m: &mut image::GrayImage, cx: f32, cy: f32, r: f32| {
        let rr = r.clamp(2.0, 64.0);
        let x0 = (cx - rr).floor().max(0.0) as u32;
        let x1 = (cx + rr).ceil().min((width - 1) as f32) as u32;
        let y0 = (cy - rr).floor().max(0.0) as u32;
        let y1 = (cy + rr).ceil().min((height - 1) as f32) as u32;
        for py in y0..=y1 {
            for px in x0..=x1 {
                if (px as f32 - cx).hypot(py as f32 - cy) <= rr {
                    m.put_pixel(px, py, image::Luma([255]));
                }
            }
        }
    };

    for sp in spots {
        if sp.kind == "scratch" {
            let pts: Vec<[f32; 2]> = if sp.points.len() >= 2 {
                sp.points.clone()
            } else if let (Some(ex), Some(ey)) = (sp.end_x, sp.end_y) {
                vec![[sp.x, sp.y], [ex, ey]]
            } else {
                vec![[sp.x, sp.y]]
            };
            if pts.len() >= 2 {
                for seg in pts.windows(2) {
                    let steps = (seg[1][0] - seg[0][0])
                        .hypot(seg[1][1] - seg[0][1])
                        .ceil()
                        .max(1.0) as usize;
                    for i in 0..=steps {
                        let t = i as f32 / steps as f32;
                        let px = seg[0][0] + (seg[1][0] - seg[0][0]) * t;
                        let py = seg[0][1] + (seg[1][1] - seg[0][1]) * t;
                        paint_disc(&mut mask, px, py, sp.radius.max(4.0));
                    }
                }
            } else {
                paint_disc(&mut mask, sp.x, sp.y, sp.radius.max(4.0));
            }
        } else {
            paint_disc(&mut mask, sp.x, sp.y, sp.radius.max(6.0));
        }
    }
    mask
}

#[tauri::command]
pub async fn heal_defects_with_lama(
    spots: Vec<DetectedSpot>,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    if spots.is_empty() {
        return Err("No defects provided for LaMa inpainting".to_string());
    }
    let loaded = {
        let guard = state.original_image.lock().map_err(|e| e.to_string())?;
        guard.clone().ok_or("No active image loaded")?
    };

    let lama_model = crate::ai::models::get_or_init_lama_model(
        &app_handle,
        &state.ai_state,
        &state.ai_init_lock,
    )
    .await
    .map_err(|e| e.to_string())?;

    tokio::task::spawn_blocking(move || {
        let is_raw = crate::decode::formats::is_raw_file(&loaded.path);
        let mut source_image = if is_raw {
            crate::render::image_processing::apply_linear_to_srgb((*loaded.image).clone())
        } else {
            (*loaded.image).clone()
        };
        crate::render::image_processing::apply_working_display_to_srgb(&mut source_image, false);

        let (w, h) = source_image.dimensions();
        let mask = rasterize_defects_mask(w, h, &spots);
        let (fill_mask, blend_mask) = crate::retouch::inpainting::generation_masks(&mask)?;
        let (final_patch, patch_x, patch_y) =
            crate::ai::lama::run_lama_inpainting(&source_image, &fill_mask, &lama_model)
                .map_err(|e| e.to_string())?;

        crate::retouch::inpainting::encode_generated_patch(
            &source_image,
            &final_patch,
            (patch_x, patch_y),
            &blend_mask,
            is_raw,
        )
    })
    .await
    .map_err(|e| format!("LaMa defect repair task failed: {}", e))?
}

#[tauri::command]
pub async fn batch_heal_sensor_dust(
    paths: Vec<String>,
    app_handle: AppHandle,
    _state: State<'_, AppState>,
) -> Result<BatchDustSummary, String> {
    if paths.is_empty() {
        return Err("No photos provided for dust healing".to_string());
    }

    let _sleep_guard = crate::enhanced::sleep_lock::SleepLockGuard::new("batch_heal_sensor_dust");
    let start_time = std::time::Instant::now();
    let total = paths.len();
    let settings = crate::app::settings::load_settings(app_handle.clone()).unwrap_or_default();
    let processed_counter = Arc::new(AtomicUsize::new(0));
    let spots_counter = Arc::new(AtomicUsize::new(0));

    let app_handle_clone = app_handle.clone();
    let paths_clone = paths.clone();
    let processed_counter_clone = processed_counter.clone();
    let spots_counter_clone = spots_counter.clone();

    let total_spots = tokio::task::spawn_blocking(move || {
        // When >= 2 frames are provided, build cross-frame sensor-space consensus to avoid healing moving birds/stars.
        let consensus_opt: Option<Vec<SensorDustMapSpot>> = if paths_clone.len() >= 2 {
            let per_frame: Vec<(usize, String, Vec<SensorDustCandidate>)> = paths_clone
                .par_iter()
                .enumerate()
                .filter_map(|(idx, path_str)| {
                    let (source_path, _) = parse_virtual_path(path_str);
                    let (cam_key, orient) = read_frame_camera_and_orientation(path_str);
                    let file_bytes = read_file_mapped(&source_path).ok()?;
                    let image = load_base_image_from_bytes(
                        &file_bytes,
                        &source_path.to_string_lossy(),
                        true,
                        &settings,
                        None,
                    )
                    .ok()?;
                    let canonical = detect_dust_candidate_spots(&image)
                        .into_iter()
                        .map(|s| {
                            let (sx, sy) = canonicalize_to_sensor_coords(s.norm_x, s.norm_y, orient);
                            SensorDustCandidate {
                                norm_x: sx,
                                norm_y: sy,
                                radius: s.radius,
                            }
                        })
                        .collect();
                    Some((idx, cam_key, canonical))
                })
                .collect();
            let cam_key = per_frame
                .first()
                .map(|x| x.1.clone())
                .unwrap_or_else(|| "universal".to_string());
            let pairs: Vec<(usize, Vec<SensorDustCandidate>)> =
                per_frame.into_iter().map(|(i, _, s)| (i, s)).collect();
            let mapped = cluster_shoot_sensor_dust(&pairs, 0.4, &cam_key);
            (!mapped.is_empty()).then_some(mapped)
        } else {
            None
        };

        paths_clone.par_iter().for_each(|path_str| {
            let (source_path, sidecar_path) = parse_virtual_path(path_str);
            let (cam_key, orient) = read_frame_camera_and_orientation(path_str);
            if let Ok(file_bytes) = read_file_mapped(&source_path) {
                if let Ok(image) = load_base_image_from_bytes(
                    &file_bytes,
                    &source_path.to_string_lossy(),
                    true,
                    &settings,
                    None,
                ) {
                    let spot_candidates: Vec<SensorDustCandidate> = if let Some(ref shared) = consensus_opt {
                        shared
                            .iter()
                            .map(|sp| {
                                let (dx, dy) = sensor_to_display_coords(sp.sensor_x, sp.sensor_y, orient);
                                crate::enhanced::local_priors::record_dust_spot_confirmation(
                                    &cam_key,
                                    sp.sensor_x,
                                    sp.sensor_y,
                                    sp.median_radius,
                                    path_str,
                                );
                                SensorDustCandidate {
                                    norm_x: dx,
                                    norm_y: dy,
                                    radius: sp.median_radius.round().max(1.0) as u32,
                                }
                            })
                            .collect()
                    } else {
                        detect_dust_candidate_spots(&image)
                    };
                    let count = spot_candidates.len();
                    if count > 0 {
                        spots_counter_clone.fetch_add(count, Ordering::SeqCst);

                        let mut metadata = load_sidecar(&sidecar_path);
                        let (iw, ih) = image.dimensions();
                        if let Some(obj) = metadata.adjustments.as_object_mut() {
                            obj.insert(
                                "sensorDustHealedCount".to_string(),
                                serde_json::json!(count),
                            );
                            let spot_entries: Vec<serde_json::Value> = spot_candidates
                                .iter()
                                .enumerate()
                                .map(|(idx, spot)| {
                                    serde_json::json!({
                                        "id": format!("auto_dust_{}", idx),
                                        "name": format!("Auto Dust #{}", idx + 1),
                                        "isLoading": false,
                                        "invert": false,
                                        "patchData": serde_json::Value::Null,
                                        "prompt": "",
                                        "subMasks": [],
                                        "visible": true,
                                        "type": "heal",
                                        "kind": "dust",
                                        "label": "Auto Dust & Blemishes",
                                        "x": spot.norm_x * iw as f32,
                                        "y": spot.norm_y * ih as f32,
                                        "radius": spot.radius,
                                    })
                                })
                                .collect();
                            let existing_patches = obj
                                .entry("aiPatches".to_string())
                                .or_insert_with(|| serde_json::json!([]));
                            if let Some(arr) = existing_patches.as_array_mut() {
                                arr.retain(|p| {
                                    p.get("label").and_then(|v| v.as_str())
                                        != Some("Auto Dust & Blemishes")
                                });
                                arr.extend(spot_entries);
                            }
                        }
                        if let Ok(json_str) = serde_json::to_string_pretty(&metadata) {
                            if let Some(parent) = sidecar_path.parent() {
                                let _ = std::fs::create_dir_all(parent);
                            }
                            let _ = crate::library::file_ops::write_file_atomic(
                                &sidecar_path,
                                json_str.as_bytes(),
                            );
                        }
                    }
                }
            }

            let done = processed_counter_clone.fetch_add(1, Ordering::SeqCst) + 1;
            let percent = (done as f32 / total as f32) * 100.0;
            let _ = app_handle_clone.emit(
                "batch-dust-progress",
                serde_json::json!({
                    "current": done,
                    "total": total,
                    "percentage": percent
                }),
            );
        });

        spots_counter.load(Ordering::SeqCst)
    })
    .await
    .map_err(|e| format!("Batch dust repair task failed: {}", e))?;

    let summary = BatchDustSummary {
        total_processed: total,
        total_spots_healed: total_spots,
        elapsed_ms: start_time.elapsed().as_millis() as u64,
    };

    let _ = app_handle.emit("batch-dust-complete", &summary);
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn test_detect_horizon_angle_horizontal_flat() {
        let (w, h) = (128u32, 128u32);
        let mut img = RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                // Sky is bright (200), sea/ground is dark (50) with sharp horizontal line at y = 64
                let val = if y < 64 { 200 } else { 50 };
                img.put_pixel(x, y, Rgb([val, val, val]));
            }
        }
        let dynamic_img = DynamicImage::ImageRgb8(img);
        let res = detect_horizon_angle(&dynamic_img);
        assert!(res.angle_degrees.abs() < 1.0, "Horizontal flat horizon should have near-zero angle: got {}", res.angle_degrees);
        assert!(res.confidence > 0.0, "Confidence should be positive on sharp line");
    }

    #[test]
    fn test_dust_spot_detection_and_healing() {
        let (w, h) = (128u32, 128u32);
        let mut img = RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                img.put_pixel(x, y, Rgb([200, 200, 200]));
            }
        }

        // Add a dark dust spot at (64, 32) in sky region with radius 3
        for dy in -3i32..=3i32 {
            for dx in -3i32..=3i32 {
                if dx * dx + dy * dy <= 9 {
                    img.put_pixel((64 + dx) as u32, (32 + dy) as u32, Rgb([140, 140, 140]));
                }
            }
        }

        let mut dynamic_img = DynamicImage::ImageRgb8(img);
        let heal_res = heal_sky_dust_spots(&mut dynamic_img);

        // Dust spot should be detected and healed
        assert!(heal_res.spots_detected >= 1, "Dust spot should be detected: healed count {}", heal_res.spots_detected);
        let center_after = dynamic_img.get_pixel(64, 32);
        assert!(center_after[0] > 140, "Healed pixel should blend back brighter than 140: got {}", center_after[0]);
    }

    #[test]
    fn test_calculate_inscribed_crop() {
        let (w, h) = (6000u32, 4000u32);
        let (cx, cy, cw, ch) = calculate_inscribed_crop(w, h, 0.0);
        assert_eq!((cx, cy, cw, ch), (0, 0, 6000, 4000));

        let (cx_rot, cy_rot, cw_rot, ch_rot) = calculate_inscribed_crop(w, h, 3.5);
        assert!(cw_rot < 6000, "Inscribed crop width must be scaled down to prevent corner wedges");
        assert!(ch_rot < 4000, "Inscribed crop height must be scaled down to prevent corner wedges");
        assert!(cx_rot > 0 && cy_rot > 0, "Offsets should be centered");
        // Check aspect ratio preservation within 0.5%
        let orig_ar = w as f32 / h as f32;
        let crop_ar = cw_rot as f32 / ch_rot as f32;
        assert!((orig_ar - crop_ar).abs() < 0.02, "Aspect ratio should be preserved");
    }

    #[test]
    fn test_cross_frame_dust_vs_bird() {
        // Spot 1 is static sensor dust (appears at norm_x: 0.5, norm_y: 0.2 in both frames)
        let spot_static = SensorDustCandidate { norm_x: 0.5, norm_y: 0.2, radius: 6 };
        // Spot 2 is a flying bird (appears at norm_x: 0.3, norm_y: 0.15 in frame 1, but moving/absent in frame 2)
        let spot_bird = SensorDustCandidate { norm_x: 0.3, norm_y: 0.15, radius: 6 };

        let frame1_spots = vec![spot_static.clone(), spot_bird];
        let frame2_spots = vec![spot_static.clone()]; // bird is gone or in different location

        let verified = cross_frame_verify_dust(&frame1_spots, &[frame2_spots]);
        assert_eq!(verified.len(), 1, "Only static spot should be verified across frames");
        assert_eq!(verified[0].norm_x, 0.5);
        assert_eq!(verified[0].norm_y, 0.2);
    }

    #[test]
    fn test_scan_peripheral_edge_patrol() {
        let (w, h) = (100u32, 100u32);
        let mut img = RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                img.put_pixel(x, y, Rgb([100, 100, 100]));
            }
        }
        let dyn_clean = DynamicImage::ImageRgb8(img.clone());
        let res_clean = scan_peripheral_edge_patrol(&dyn_clean);
        assert!(!res_clean.edge_distractions_found, "Clean image should not trigger edge patrol crop");

        // Add a distracting high-contrast boundary artifact on the rightmost edge
        for y in 0..h {
            img.put_pixel(99, y, Rgb([255, 255, 255]));
        }
        let dyn_distracted = DynamicImage::ImageRgb8(img);
        let res_distracted = scan_peripheral_edge_patrol(&dyn_distracted);
        assert!(res_distracted.edge_distractions_found, "High-contrast edge artifact should trigger edge patrol");
        assert!(res_distracted.crop_x > 0 || res_distracted.width < w, "Should inset crop boundaries");
    }

    #[test]
    fn test_full_frame_dark_bright_dust_and_scratch_recall_with_starfield_guard() {
        let (w, h) = (160u32, 160u32);
        let mut img = RgbImage::from_pixel(w, h, Rgb([140, 140, 140]));

        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                if dx * dx + dy * dy <= 4 {
                    img.put_pixel((50 + dx) as u32, (135 + dy) as u32, Rgb([60, 60, 60]));
                    img.put_pixel((110 + dx) as u32, (80 + dy) as u32, Rgb([235, 235, 235]));
                }
            }
        }
        for x in 30..80 {
            img.put_pixel(x, 30, Rgb([45, 45, 45]));
            img.put_pixel(x, 31, Rgb([55, 55, 55]));
        }

        let dyn_img = DynamicImage::ImageRgb8(img);
        let dust = detect_dust_candidate_spots(&dyn_img);
        let scratches = detect_linear_scratch_candidates(&dyn_img);

        assert!(
            dust.iter().any(|s| s.norm_y > 0.75 && (s.norm_x - 50.0 / 160.0).abs() < 0.05),
            "Must recall dark dust in lower frame (>65%), got {:?}",
            dust
        );
        assert!(
            dust.iter().any(|s| (s.norm_x - 110.0 / 160.0).abs() < 0.05 && (s.norm_y - 80.0 / 160.0).abs() < 0.05),
            "Must recall bright dust in midtone, got {:?}",
            dust
        );
        assert!(!scratches.is_empty(), "Must recall 2px linear hairline scratch");

        let mut starfield = RgbImage::from_pixel(w, h, Rgb([5, 5, 8]));
        for i in 0..25u32 {
            let sx = 14 + (i * 23) % (w - 28);
            let sy = 14 + (i * 37) % (h - 28);
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx * dx + dy * dy <= 1 {
                        starfield.put_pixel((sx as i32 + dx) as u32, (sy as i32 + dy) as u32, Rgb([250, 248, 240]));
                    }
                }
            }
        }
        let dyn_stars = DynamicImage::ImageRgb8(starfield);
        assert_eq!(detect_dust_candidate_spots(&dyn_stars).len(), 0, "Starfield must produce 0 dust false positives");
        assert_eq!(detect_linear_scratch_candidates(&dyn_stars).len(), 0, "Starfield must produce 0 scratch false positives");

        let mut checker = RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = if ((x / 8) + (y / 8)) % 2 == 0 { 200 } else { 80 };
                checker.put_pixel(x, y, Rgb([v, v, v]));
            }
        }
        assert_eq!(detect_linear_scratch_candidates(&DynamicImage::ImageRgb8(checker)).len(), 0, "Checkerboard texture must produce 0 scratch false positives");
    }

    #[test]
    fn test_sensor_coords_roundtrip_across_exif_orientations() {
        let (dx, dy) = (0.23f32, 0.71f32);
        for orient in 1..=8u8 {
            let (sx, sy) = canonicalize_to_sensor_coords(dx, dy, orient);
            let (rx, ry) = sensor_to_display_coords(sx, sy, orient);
            assert!(
                (rx - dx).abs() < 1e-5 && (ry - dy).abs() < 1e-5,
                "Orientation {} roundtrip failed: ({}, {}) vs ({}, {})",
                orient,
                rx,
                ry,
                dx,
                dy
            );
        }
        let (dx6, dy6) = sensor_to_display_coords(0.0, 0.0, 6);
        assert!((dx6 - 1.0).abs() < 1e-5 && dy6.abs() < 1e-5);
    }

    #[test]
    fn test_shoot_dust_map_clusters_persistent_spots_and_rejects_transients() {
        let frame_spots = vec![
            (0usize, vec![
                SensorDustCandidate { norm_x: 0.400, norm_y: 0.250, radius: 6 },
                SensorDustCandidate { norm_x: 0.150, norm_y: 0.800, radius: 4 },
            ]),
            (1usize, vec![
                SensorDustCandidate { norm_x: 0.4015, norm_y: 0.251, radius: 8 },
            ]),
            (2usize, vec![
                SensorDustCandidate { norm_x: 0.399, norm_y: 0.2495, radius: 6 },
                SensorDustCandidate { norm_x: 0.720, norm_y: 0.600, radius: 5 },
            ]),
            (3usize, Vec::new()),
        ];
        let map = cluster_shoot_sensor_dust(&frame_spots, 0.4, "__test_camera_never_exists__");
        assert_eq!(map.len(), 1, "Expected 1 persistent spot, got {:?}", map);
        assert_eq!(map[0].hit_count, 3);
        assert!(map[0].confidence >= 0.7);
        assert!((map[0].sensor_x - 0.400).abs() < 0.005);
    }

    #[test]
    fn test_multiscale_hessian_detects_diagonal_and_curved_hair() {
        let (w, h) = (200u32, 200u32);
        let mut img = RgbImage::from_pixel(w, h, Rgb([145, 145, 145]));
        // Draw a gentle curved hair arc from x=30..150, not aligned to any cardinal or 45deg grid direction
        for step in 0..400 {
            let t = step as f32 / 399.0;
            let px = 30.0 + t * 120.0;
            let py = 60.0 + (t * std::f32::consts::PI).sin() * 28.0 + t * 18.0;
            for oy in -1..=1 {
                for ox in -1..=1 {
                    if ox * ox + oy * oy <= 1 {
                        let ix = (px + ox as f32).round() as u32;
                        let iy = (py + oy as f32).round() as u32;
                        if ix < w && iy < h {
                            img.put_pixel(ix, iy, Rgb([55, 55, 55]));
                        }
                    }
                }
            }
        }
        let scratches = detect_linear_scratch_candidates(&DynamicImage::ImageRgb8(img));
        assert!(
            scratches.iter().any(|s| s.points.len() >= 3),
            "Expected curved hair polyline with >=3 vertices, got {:?}",
            scratches
        );
    }

    #[test]
    fn test_defect_routing_sends_textured_or_large_spots_to_lama_tier() {
        assert!(!should_route_defect_to_lama(6.0, 0.01, "dust"));
        assert!(should_route_defect_to_lama(18.0, 0.01, "dust"));
        assert!(should_route_defect_to_lama(5.0, 0.12, "dust"));
        assert!(should_route_defect_to_lama(6.0, 0.01, "scratch"));
    }

    #[test]
    fn portrait_blemish_detector_gates_protected_zones_and_rejects_freckle_grids() {
        let (w, h) = (128u32, 128u32);
        let mut rgb = RgbImage::from_pixel(w, h, Rgb([205, 165, 145]));
        let skin = image::GrayImage::from_pixel(w, h, image::Luma([255]));
        let mut protected = image::GrayImage::new(w, h);
        // Protect left half (eyes/lips proxy)
        for y in 40..70 {
            for x in 20..50 {
                protected.put_pixel(x, y, image::Luma([255]));
            }
        }
        // Pimple inside protected zone (must be ignored)
        for dy in -3i32..=3 {
            for dx in -3i32..=3 {
                if dx * dx + dy * dy <= 9 {
                    rgb.put_pixel((35 + dx) as u32, (55 + dy) as u32, Rgb([195, 90, 90]));
                }
            }
        }
        // Isolated red blemish in clean skin at (92, 64) (must be detected)
        for dy in -3i32..=3 {
            for dx in -3i32..=3 {
                if dx * dx + dy * dy <= 9 {
                    rgb.put_pixel((92 + dx) as u32, (64 + dy) as u32, Rgb([205, 95, 95]));
                }
            }
        }
        let spots = detect_portrait_blemishes(&DynamicImage::ImageRgb8(rgb), &skin, &protected);
        assert!(
            spots.iter().any(|s| (s.x - 92.0).abs() < 6.0 && (s.y - 64.0).abs() < 6.0),
            "expected isolated skin blemish near (92, 64), got {:?}",
            spots
        );
        assert!(
            !spots.iter().any(|s| (s.x - 35.0).abs() < 8.0 && (s.y - 55.0).abs() < 8.0),
            "protected zone mark must be excluded: {:?}",
            spots
        );
    }
}

fn srgb_pixel_to_oklab(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let lin = |u: u8| -> f32 {
        let s = u as f32 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    let (lr, lg, lb) = (lin(r), lin(g), lin(b));
    let l = 0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb;
    let m = 0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb;
    let s = 0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb;
    let (l_, m_, s_) = (l.max(0.0).cbrt(), m.max(0.0).cbrt(), s.max(0.0).cbrt());
    let ok_l = 0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_;
    let ok_a = 1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_;
    let ok_b = 0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_;
    (ok_l, ok_a, ok_b)
}

fn box_mean_map(src: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    let stride = w + 1;
    let mut sat = vec![0.0f64; (w + 1) * (h + 1)];
    for y in 0..h {
        let mut row_sum = 0.0f64;
        for x in 0..w {
            row_sum += src[y * w + x] as f64;
            sat[(y + 1) * stride + (x + 1)] = sat[y * stride + (x + 1)] + row_sum;
        }
    }
    let mut out = vec![0.0f32; w * h];
    for y in 0..h {
        let y0 = y.saturating_sub(radius);
        let y1 = (y + radius + 1).min(h);
        for x in 0..w {
            let x0 = x.saturating_sub(radius);
            let x1 = (x + radius + 1).min(w);
            let area = ((y1 - y0) * (x1 - x0)).max(1) as f64;
            let sum = sat[y1 * stride + x1] - sat[y0 * stride + x1] - sat[y1 * stride + x0]
                + sat[y0 * stride + x0];
            out[y * w + x] = (sum / area) as f32;
        }
    }
    out
}

/// Detects compact portrait skin blemishes (2-14 px OKLab redness/luminance DoG peaks) strictly
/// gated inside `skin_mask` outside `protected_mask`, rejecting dense freckle distributions.
pub fn detect_portrait_blemishes(
    image: &DynamicImage,
    skin_mask: &image::GrayImage,
    protected_mask: &image::GrayImage,
) -> Vec<DetectedSpot> {
    let (orig_w, orig_h) = image.dimensions();
    if orig_w < 16 || orig_h < 16 {
        return Vec::new();
    }
    let max_dim = orig_w.max(orig_h);
    let scale = if max_dim > 1600 {
        1600.0 / max_dim as f32
    } else {
        1.0
    };
    let aw = ((orig_w as f32 * scale).round() as u32).max(16);
    let ah = ((orig_h as f32 * scale).round() as u32).max(16);

    let analysis_rgb: image::RgbImage = if (aw, ah) == (orig_w, orig_h) {
        image.to_rgb8()
    } else {
        image
            .resize_exact(aw, ah, image::imageops::FilterType::Triangle)
            .to_rgb8()
    };
    let skin = if skin_mask.dimensions() == (aw, ah) {
        Cow::Borrowed(skin_mask)
    } else {
        Cow::Owned(image::imageops::resize(skin_mask, aw, ah, image::imageops::FilterType::Triangle))
    };
    let prot = if protected_mask.dimensions() == (aw, ah) {
        Cow::Borrowed(protected_mask)
    } else {
        Cow::Owned(image::imageops::resize(protected_mask, aw, ah, image::imageops::FilterType::Triangle))
    };

    let (w, h) = (aw as usize, ah as usize);
    let mut ch_l = Vec::with_capacity(w * h);
    let mut ch_a = Vec::with_capacity(w * h);
    for px in analysis_rgb.as_raw().chunks_exact(3) {
        let (ok_l, ok_a, _) = srgb_pixel_to_oklab(px[0], px[1], px[2]);
        ch_l.push(ok_l);
        ch_a.push(ok_a);
    }

    let l_small = box_mean_map(&ch_l, w, h, 2);
    let l_large = box_mean_map(&ch_l, w, h, 8);
    let a_small = box_mean_map(&ch_a, w, h, 2);
    let a_large = box_mean_map(&ch_a, w, h, 8);

    let skin_raw = skin.as_raw();
    let prot_raw = prot.as_raw();
    let mut scores = vec![0.0f32; w * h];
    let mut skin_scores = Vec::new();
    for i in 0..(w * h) {
        if skin_raw[i] >= 128 && prot_raw[i] < 48 {
            let da = (a_small[i] - a_large[i]).max(0.0);
            let dl = (l_small[i] - l_large[i]).abs();
            let s = da * 2.4 + dl * 1.4;
            scores[i] = s;
            skin_scores.push(s);
        }
    }
    if skin_scores.len() < 32 {
        return Vec::new();
    }
    let mid = skin_scores.len() / 2;
    let (_, median_val, _) = skin_scores.select_nth_unstable_by(mid, |a, b| a.total_cmp(b));
    let median = *median_val;
    let mut abs_devs: Vec<f32> = skin_scores.iter().map(|&v| (v - median).abs()).collect();
    let (_, mad_val, _) = abs_devs.select_nth_unstable_by(mid, |a, b| a.total_cmp(b));
    let mad = (*mad_val * 1.4826).max(0.004);
    let thresh = (median + 4.5 * mad).max(0.055);

    let mut raw_peaks: Vec<(usize, usize, f32, f32)> = Vec::new();
    for y in 4..h.saturating_sub(4) {
        for x in 4..w.saturating_sub(4) {
            let idx = y * w + x;
            let val = scores[idx];
            if val < thresh {
                continue;
            }
            let mut is_max = true;
            'nms: for ny in (y - 3)..=(y + 3) {
                for nx in (x - 3)..=(x + 3) {
                    if (nx != x || ny != y) && scores[ny * w + nx] > val {
                        is_max = false;
                        break 'nms;
                    }
                }
            }
            if is_max {
                // Estimate compact radius where response stays > 40% of peak (clamped 2..14 px)
                let half = val * 0.4;
                let mut r_est = 3usize;
                for r in 2..=12 {
                    let s_r = scores.get(y * w + (x + r).min(w - 1)).copied().unwrap_or(0.0)
                        + scores.get((y + r).min(h - 1) * w + x).copied().unwrap_or(0.0);
                    if s_r * 0.5 < half {
                        r_est = r;
                        break;
                    }
                    r_est = r;
                }
                let radius = (r_est as f32 / scale).clamp(2.0, 14.0);
                raw_peaks.push((x, y, val, radius));
            }
        }
    }

    // Reject dense uniform freckle distributions (> 6 neighbors within 24 px)
    raw_peaks.sort_by(|a, b| b.2.total_cmp(&a.2));
    let mut kept = Vec::new();
    for &(px, py, _val, radius) in &raw_peaks {
        let nearby = raw_peaks
            .iter()
            .filter(|&&(qx, qy, _, _)| {
                let dx = px as f32 - qx as f32;
                let dy = py as f32 - qy as f32;
                let d2 = dx * dx + dy * dy;
                d2 > 0.0 && d2 <= 24.0 * 24.0
            })
            .count();
        if nearby > 6 {
            continue;
        }
        let full_x = (px as f32 / scale).clamp(0.0, (orig_w - 1) as f32);
        let full_y = (py as f32 / scale).clamp(0.0, (orig_h - 1) as f32);
        let routing = if radius >= 8.0 {
            "lama".to_string()
        } else {
            "analytical".to_string()
        };
        kept.push(DetectedSpot {
            x: full_x,
            y: full_y,
            radius,
            norm_x: full_x / orig_w as f32,
            norm_y: full_y / orig_h as f32,
            kind: "blemish".to_string(),
            end_x: None,
            end_y: None,
            norm_end_x: None,
            norm_end_y: None,
            points: Vec::new(),
            routing,
        });
        if kept.len() >= 48 {
            break;
        }
    }
    kept
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn scan_portrait_blemishes(
    js_adjustments: serde_json::Value,
    path: String,
    rotation: f32,
    flip_horizontal: bool,
    flip_vertical: bool,
    orientation_steps: u8,
    state: State<'_, AppState>,
    app_handle: AppHandle,
) -> Result<AutoDefectRepairResult, String> {
    let models = crate::ai::models::get_or_init_ai_models(
        &app_handle,
        &state.ai_state,
        &state.ai_init_lock,
        crate::ai::models::AiMaskModel::Face,
    )
    .await
    .map_err(|e| e.to_string())?;

    let warped_image = crate::render::preview::get_cached_full_warped_image(&state, &js_adjustments)?;
    let transform = crate::ai::commands::MaskTransform {
        rotation,
        flip_horizontal,
        flip_vertical,
        orientation_steps,
    };
    let cache_key = crate::ai::commands::ai_cache_key(&path, &js_adjustments);
    let analysis = crate::ai::commands::cached_face_analysis(
        &state.ai_state,
        &models,
        &cache_key,
        &warped_image,
        transform,
    )?;

    if analysis.faces.is_empty() {
        return Ok(AutoDefectRepairResult {
            spots: Vec::new(),
            total_detected: 0,
            message: "No faces detected for portrait blemish scan.".to_string(),
        });
    }

    let spots = tokio::task::spawn_blocking(move || {
        use crate::masks::face::{FacePart, compose_face_mask};
        use crate::masks::morphology::grayscale_dilate;

        let skin_mask = compose_face_mask(
            &warped_image,
            &analysis,
            &[FacePart::FaceSkin, FacePart::BodySkin],
            None,
        );
        let features = compose_face_mask(
            &warped_image,
            &analysis,
            &[
                FacePart::Iris,
                FacePart::EyeWhites,
                FacePart::Eyebrows,
                FacePart::Lips,
                FacePart::Nose,
            ],
            None,
        );
        let protected = grayscale_dilate(&features, 4);
        detect_portrait_blemishes(&warped_image, &skin_mask, &protected)
    })
    .await
    .map_err(|e| e.to_string())?;

    let total_detected = spots.len();
    let message = if total_detected == 0 {
        "No portrait skin blemishes detected.".to_string()
    } else {
        format!("Detected {total_detected} portrait blemish(es).")
    };
    Ok(AutoDefectRepairResult {
        spots,
        total_detected,
        message,
    })
}

