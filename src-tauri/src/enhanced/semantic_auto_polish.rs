use crate::app::state::AppState;
use crate::enhanced::color_matcher::{calculate_skin_presence_ratio, srgb_to_oklab};
use crate::enhanced::defect_repair::detect_horizon_angle;
use crate::metadata::sidecar::load_sidecar;
use crate::library::sidecars::parse_virtual_path;
use crate::library::file_ops::read_file_mapped;
use crate::metadata::xmp::sync_metadata_to_xmp;
use crate::decode::image_loader::load_base_image_from_bytes;
use crate::adjustments::auto_adjust::{perform_auto_analysis, AutoAdjustmentResults};
use image::{DynamicImage, GenericImageView};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemanticScene {
    Portrait,
    Animal,
    Architecture,
    Landscape,
    Sunset,
    Overcast,
    NightSky,
    Macro,
    Hdr,
    Panorama,
    General,
}

impl SemanticScene {
    pub fn display_name(&self) -> &'static str {
        match self {
            SemanticScene::Portrait => "👤 Portrait & People",
            SemanticScene::Animal => "🐾 Pets & Wildlife",
            SemanticScene::Architecture => "🏛️ Architecture & Urban",
            SemanticScene::Landscape => "🌲 Landscape & Nature",
            SemanticScene::Sunset => "🌅 Sunset & Golden Hour",
            SemanticScene::Overcast => "☁️ Overcast & Moody",
            SemanticScene::NightSky => "🪐 Night Sky & Astro",
            SemanticScene::Macro => "🌸 Macro & Close-up",
            SemanticScene::Hdr => "☀️ High Dynamic Range (HDR)",
            SemanticScene::Panorama => "🏞️ Wide Panorama",
            SemanticScene::General => "✨ Universal Balanced",
        }
    }

    pub fn from_str_opt(s: Option<&str>) -> Option<Self> {
        let lower = s?.trim().to_lowercase();
        match lower.as_str() {
            "portrait" => Some(SemanticScene::Portrait),
            "animal" => Some(SemanticScene::Animal),
            "architecture" | "arch" => Some(SemanticScene::Architecture),
            "landscape" => Some(SemanticScene::Landscape),
            "sunset" => Some(SemanticScene::Sunset),
            "overcast" => Some(SemanticScene::Overcast),
            "nightsky" | "night_sky" | "night" | "astro" => Some(SemanticScene::NightSky),
            "macro" => Some(SemanticScene::Macro),
            "hdr" => Some(SemanticScene::Hdr),
            "panorama" | "pano" => Some(SemanticScene::Panorama),
            "general" | "universal" => Some(SemanticScene::General),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToneCurveStyle {
    FilmicDynamic,
    PunchyCommercial,
    NaturalSoft,
}

impl Default for ToneCurveStyle {
    fn default() -> Self {
        ToneCurveStyle::FilmicDynamic
    }
}

impl ToneCurveStyle {
    pub fn from_str_opt(s: Option<&str>) -> Self {
        match s.map(|v| v.to_lowercase()).as_deref() {
            Some("punchy") | Some("punchycommercial") => ToneCurveStyle::PunchyCommercial,
            Some("soft") | Some("naturalsoft") => ToneCurveStyle::NaturalSoft,
            _ => ToneCurveStyle::FilmicDynamic,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticAnalysisResult {
    pub scene: SemanticScene,
    pub scene_name: String,
    pub confidence: f32,
    pub horizon_angle: Option<f32>,
    pub adjustments: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchPolishSummary {
    pub total_processed: usize,
    pub total_clusters: usize,
    pub scene_breakdown: std::collections::HashMap<String, usize>,
    pub elapsed_ms: u64,
}

/// Computes fast Laplacian edge variance to measure focus and sharpness
fn compute_fast_sharpness(thumb: &image::RgbImage) -> f32 {
    let (w, h) = thumb.dimensions();
    if w < 10 || h < 10 {
        return 0.0;
    }

    let mut sum = 0.0f64;
    let mut sq_sum = 0.0f64;
    let mut count = 0.0f64;

    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let p = thumb.get_pixel(x, y);
            let c = 0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64;
            let up_p = thumb.get_pixel(x, y - 1);
            let up = 0.2126 * up_p[0] as f64 + 0.7152 * up_p[1] as f64 + 0.0722 * up_p[2] as f64;
            let dn_p = thumb.get_pixel(x, y + 1);
            let dn = 0.2126 * dn_p[0] as f64 + 0.7152 * dn_p[1] as f64 + 0.0722 * dn_p[2] as f64;
            let lf_p = thumb.get_pixel(x - 1, y);
            let lf = 0.2126 * lf_p[0] as f64 + 0.7152 * lf_p[1] as f64 + 0.0722 * lf_p[2] as f64;
            let rt_p = thumb.get_pixel(x + 1, y);
            let rt = 0.2126 * rt_p[0] as f64 + 0.7152 * rt_p[1] as f64 + 0.0722 * rt_p[2] as f64;

            let lap = (4.0 * c - up - dn - lf - rt).abs();
            sum += lap;
            sq_sum += lap * lap;
            count += 1.0;
        }
    }

    if count == 0.0 {
        return 0.0;
    }
    let mean = sum / count;
    let var = (sq_sum / count) - (mean * mean);
    (var.max(0.0).sqrt() as f32).clamp(0.0, 100.0)
}

/// Converts a linear working-space or encoded image into a <= 1024px display-referred sRGB8 preview
/// with baseline exposure applied so scene classifiers never misread linear RAW files as dark/NightSky.
pub fn prepare_display_preview_for_analysis(image: &DynamicImage, exposure_ev: f32) -> DynamicImage {
    let downscaled = crate::render::image_processing::downscale_f32_image(image, 1024, 1024);
    let exposed = crate::render::image_processing::apply_exposure(downscaled, exposure_ev);
    let to_srgb = crate::render::color_space::working_to_srgb();
    let transfer = crate::render::color_space::srgb_transfer();
    let linear_buf = exposed.to_rgb32f();
    let (w, h) = linear_buf.dimensions();
    let mut out = image::RgbImage::new(w, h);
    for (x, y, px) in out.enumerate_pixels_mut() {
        let p = linear_buf.get_pixel(x, y).0;
        let srgb = (to_srgb * glam::Vec3::from_array(p)).clamp(glam::Vec3::ZERO, glam::Vec3::ONE);
        *px = image::Rgb([
            (transfer.encode(srgb.x) * 255.0).round().clamp(0.0, 255.0) as u8,
            (transfer.encode(srgb.y) * 255.0).round().clamp(0.0, 255.0) as u8,
            (transfer.encode(srgb.z) * 255.0).round().clamp(0.0, 255.0) as u8,
        ]);
    }
    DynamicImage::ImageRgb8(out)
}

/// Analyzes an image thumbnail to determine its dominant photographic scene and subject type
pub fn detect_semantic_scene(image: &DynamicImage) -> (SemanticScene, f32) {
    detect_semantic_scene_with_hint(image, None)
}

pub fn detect_semantic_scene_with_hint(image: &DynamicImage, path_hint: Option<&str>) -> (SemanticScene, f32) {
    let (width, height) = image.dimensions();
    if width < 32 || height < 32 {
        return (SemanticScene::General, 0.5);
    }

    let aspect = (width as f32 / height as f32).max(height as f32 / width as f32);
    let hint_lower = path_hint.unwrap_or("").to_lowercase();
    if aspect >= 2.15 || hint_lower.contains("_pano") || hint_lower.contains("panorama") {
        return (SemanticScene::Panorama, if aspect >= 2.15 { 0.94 } else { 0.85 });
    }

    let thumb = image.thumbnail(320, 320).to_rgb8();
    let (tw, th) = thumb.dimensions();
    let total_pixels = (tw * th) as f32;

    let mut green_vegetation_pixels = 0.0f32;
    let mut blue_sky_pixels = 0.0f32;
    let mut sunset_warm_pixels = 0.0f32;
    let mut deep_dark_pixels = 0.0f32;
    let mut high_contrast_edges = 0.0f32;
    let mut macro_center_detail = 0.0f32;
    let mut shadow_dr_pixels = 0.0f32;
    let mut highlight_dr_pixels = 0.0f32;
    let mut midtone_dr_pixels = 0.0f32;

    let center_x1 = tw / 4;
    let center_x2 = tw * 3 / 4;
    let center_y1 = th / 4;
    let center_y2 = th * 3 / 4;

    for y in 0..th {
        for x in 0..tw {
            let p = thumb.get_pixel(x, y);
            let r = p[0] as f32;
            let g = p[1] as f32;
            let b = p[2] as f32;

            let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;

            // Night Sky & Dynamic Range distribution checks
            if luma < 25.0 {
                deep_dark_pixels += 1.0;
            }
            if luma < 28.0 {
                shadow_dr_pixels += 1.0;
            } else if luma > 225.0 {
                highlight_dr_pixels += 1.0;
            } else if luma >= 64.0 && luma <= 192.0 {
                midtone_dr_pixels += 1.0;
            }

            // Green / Foliage detection
            if g > r * 1.15 && g > b * 1.15 && luma > 30.0 {
                green_vegetation_pixels += 1.0;
            }

            // Sky detection (top half biased)
            if y < th / 2 && b > r * 1.15 && b > g * 0.95 && luma > 60.0 {
                blue_sky_pixels += 1.0;
            }

            // Sunset / Golden Hour detection (high red/amber with warm glow)
            if r > g * 1.25 && g > b * 1.35 && luma > 50.0 {
                sunset_warm_pixels += 1.0;
            }

            // Edge density check
            if x > 0 && y > 0 && x < tw - 1 && y < th - 1 {
                let left_p = thumb.get_pixel(x - 1, y);
                let diff = (r - left_p[0] as f32).abs() + (g - left_p[1] as f32).abs();
                if diff > 35.0 {
                    high_contrast_edges += 1.0;
                    if x >= center_x1 && x <= center_x2 && y >= center_y1 && y <= center_y2 {
                        macro_center_detail += 1.0;
                    }
                }
            }
        }
    }

    let skin_ratio = calculate_skin_presence_ratio(&thumb);
    let green_ratio = green_vegetation_pixels / total_pixels;
    let sky_ratio = blue_sky_pixels / (total_pixels * 0.5);
    let sunset_ratio = sunset_warm_pixels / total_pixels;
    let dark_ratio = deep_dark_pixels / total_pixels;
    let edge_ratio = high_contrast_edges / total_pixels;
    let center_edge_ratio = macro_center_detail / (total_pixels * 0.25);
    let shadow_dr_ratio = shadow_dr_pixels / total_pixels;
    let highlight_dr_ratio = highlight_dr_pixels / total_pixels;
    let midtone_dr_ratio = midtone_dr_pixels / total_pixels;

    if hint_lower.contains("_hdr")
        || (shadow_dr_ratio > 0.08 && highlight_dr_ratio > 0.08 && midtone_dr_ratio > 0.35)
    {
        return (
            SemanticScene::Hdr,
            if hint_lower.contains("_hdr") { 0.92 } else { 0.80 },
        );
    }

    // Rule-based classifier with high confidence scoring
    if dark_ratio > 0.45 && sunset_ratio < 0.05 {
        let confidence = (dark_ratio * 1.4).min(0.95);
        (SemanticScene::NightSky, confidence)
    } else if sunset_ratio > 0.15 {
        let confidence = (sunset_ratio * 2.5).clamp(0.65, 0.96);
        (SemanticScene::Sunset, confidence)
    } else if skin_ratio > 0.07 {
        let confidence = (skin_ratio * 3.5).clamp(0.60, 0.98);
        (SemanticScene::Portrait, confidence)
    } else if green_ratio > 0.20 || (green_ratio > 0.12 && sky_ratio > 0.15) {
        let confidence = ((green_ratio + sky_ratio) * 1.5).clamp(0.60, 0.95);
        (SemanticScene::Landscape, confidence)
    } else if sky_ratio < 0.05 && green_ratio < 0.05 && edge_ratio > 0.18 {
        let confidence = (edge_ratio * 2.2).clamp(0.55, 0.90);
        (SemanticScene::Architecture, confidence)
    } else if center_edge_ratio > 0.22 && edge_ratio < 0.12 {
        let confidence = (center_edge_ratio * 1.8).clamp(0.55, 0.88);
        (SemanticScene::Macro, confidence)
    } else if edge_ratio > 0.12 && skin_ratio > 0.02 {
        (SemanticScene::Animal, 0.68)
    } else if sky_ratio < 0.08 && luma_contrast(&thumb) < 35.0 {
        (SemanticScene::Overcast, 0.72)
    } else {
        (SemanticScene::General, 0.60)
    }
}

fn luma_contrast(img: &image::RgbImage) -> f32 {
    let mut min_luma = 255.0f32;
    let mut max_luma = 0.0f32;
    for p in img.pixels() {
        let luma = 0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32;
        min_luma = min_luma.min(luma);
        max_luma = max_luma.max(luma);
    }
    max_luma - min_luma
}

/// Generates tailored photographic adjustments for the detected semantic scene scaled by intensity
pub fn generate_semantic_adjustments(
    scene: SemanticScene,
    base: &AutoAdjustmentResults,
    intensity: f32,
    horizon_angle: Option<f32>,
    tone_style: Option<ToneCurveStyle>,
    skin_protection: Option<bool>,
) -> serde_json::Value {
    let mult = (intensity / 100.0).clamp(0.0, 2.0);
    let curve_style = tone_style.unwrap_or_default();
    let protect_skin = skin_protection.unwrap_or(true);

    let (governor, min_strength, preprocess_ev) = match scene {
        SemanticScene::Portrait => (25.0, 10.0, 0.0),
        SemanticScene::Animal => (30.0, 10.0, 0.0),
        SemanticScene::Architecture => (35.0, 10.0, 0.0),
        SemanticScene::Landscape => (25.0, 10.0, 0.0),
        SemanticScene::Sunset => (40.0, 10.0, -0.24),
        SemanticScene::Overcast => (35.0, 10.0, 0.0),
        SemanticScene::NightSky => (5.0, 5.0, 0.0),
        SemanticScene::Macro => (15.0, 10.0, 0.0),
        SemanticScene::Hdr => (20.0, 10.0, 0.0),
        SemanticScene::Panorama => (25.0, 10.0, 0.0),
        SemanticScene::General => (35.0, 10.0, 0.0),
    };
    let governed_exp = compute_governed_exposure(base.exposure as f32, governor, min_strength, preprocess_ev) as f64;
    let mut exposure = governed_exp * 0.9 * mult as f64;
    let mut contrast = base.contrast * mult as f64;
    let mut highlights = base.highlights * mult as f64;
    let mut shadows = base.shadows * mult as f64;
    let mut whites = base.whites * mult as f64;
    let mut blacks = base.blacks * mult as f64;
    let mut clarity = base.clarity * mult as f64;
    let mut dehaze = base.dehaze * mult as f64;
    let mut temperature = base.temperature * mult as f64;
    let mut tint = base.tint * mult as f64;
    let mut vibrance = base.vibrancy * mult as f64;
    let mut sharpening = 28.0 * mult as f64;
    let mut sharpness_threshold = 20.0 * mult as f64;
    let mut color_noise_reduction = 25.0 * mult as f64;
    let mut luma_noise_reduction = 10.0 * mult as f64;

    // Specialized scene treatments
    match scene {
        SemanticScene::Portrait => {
            contrast = (contrast * 0.85).max(8.0);
            highlights = (highlights - 18.0 * mult as f64).min(-10.0);
            shadows = (shadows + 14.0 * mult as f64).max(10.0);
            whites = (whites + 10.0 * mult as f64).max(8.0);
            clarity = (clarity - 6.0 * mult as f64).min(2.0);
            sharpening = 25.0 * mult as f64;
            vibrance = if protect_skin {
                (vibrance + 4.0 * mult as f64).clamp(0.0, 14.0)
            } else {
                (vibrance + 8.0 * mult as f64).min(20.0)
            };
            temperature = (temperature + 1.5 * mult as f64).min(5.0);
            color_noise_reduction = 28.0 * mult as f64;
        }
        SemanticScene::Animal => {
            clarity = (clarity + 18.0 * mult as f64).max(14.0);
            sharpening = 40.0 * mult as f64;
            sharpness_threshold = 15.0 * mult as f64;
            whites = (whites + 12.0 * mult as f64).max(10.0);
            blacks = (blacks - 10.0 * mult as f64).min(-8.0);
            vibrance = (vibrance + 12.0 * mult as f64).max(10.0);
        }
        SemanticScene::Architecture => {
            contrast = (contrast + 12.0 * mult as f64).max(15.0);
            clarity = (clarity + 22.0 * mult as f64).max(18.0);
            dehaze = (dehaze + 14.0 * mult as f64).max(10.0);
            highlights = (highlights - 15.0 * mult as f64).min(-10.0);
            whites = (whites + 15.0 * mult as f64).max(12.0);
            blacks = (blacks - 14.0 * mult as f64).min(-12.0);
            sharpening = 35.0 * mult as f64;
        }
        SemanticScene::Landscape => {
            contrast = (contrast + 14.0 * mult as f64).max(16.0);
            highlights = (highlights - 22.0 * mult as f64).min(-15.0);
            shadows = (shadows + 20.0 * mult as f64).max(15.0);
            whites = (whites + 14.0 * mult as f64).max(12.0);
            blacks = (blacks - 16.0 * mult as f64).min(-14.0);
            clarity = (clarity + 16.0 * mult as f64).max(12.0);
            dehaze = (dehaze + 16.0 * mult as f64).max(12.0);
            vibrance = (vibrance + 18.0 * mult as f64).max(14.0);
            sharpening = 36.0 * mult as f64;
        }
        SemanticScene::Sunset => {
            exposure = (exposure + 0.15 * mult as f64).min(0.8);
            contrast = (contrast + 10.0 * mult as f64).max(14.0);
            highlights = (highlights - 28.0 * mult as f64).min(-20.0);
            shadows = (shadows + 22.0 * mult as f64).max(16.0);
            whites = (whites + 8.0 * mult as f64).max(6.0);
            blacks = (blacks - 12.0 * mult as f64).min(-10.0);
            temperature = (temperature + 8.0 * mult as f64).max(6.0);
            tint = (tint + 4.0 * mult as f64).max(2.0);
            vibrance = (vibrance + 16.0 * mult as f64).max(12.0);
            clarity = (clarity + 12.0 * mult as f64).max(8.0);
        }
        SemanticScene::Overcast => {
            contrast = (contrast + 16.0 * mult as f64).max(18.0);
            clarity = (clarity + 24.0 * mult as f64).max(18.0);
            dehaze = (dehaze + 22.0 * mult as f64).max(16.0);
            whites = (whites + 18.0 * mult as f64).max(14.0);
            blacks = (blacks - 16.0 * mult as f64).min(-12.0);
            temperature = (temperature + 4.0 * mult as f64).max(2.0);
            vibrance = (vibrance + 14.0 * mult as f64).max(10.0);
        }
        SemanticScene::NightSky => {
            contrast = (contrast + 22.0 * mult as f64).max(20.0);
            highlights = (highlights - 20.0 * mult as f64).min(-16.0);
            whites = (whites + 18.0 * mult as f64).max(16.0);
            shadows = (shadows + 16.0 * mult as f64).max(12.0);
            blacks = (blacks - 20.0 * mult as f64).min(-18.0);
            temperature = (temperature - 8.0 * mult as f64).min(-6.0);
            tint = (tint + 10.0 * mult as f64).max(8.0);
            clarity = (clarity + 22.0 * mult as f64).max(18.0);
            dehaze = (dehaze + 24.0 * mult as f64).min(24.0);
            sharpening = 36.0 * mult as f64;
            sharpness_threshold = (60.0 * mult as f64).max(60.0);
            color_noise_reduction = 48.0 * mult as f64;
            luma_noise_reduction = 30.0 * mult as f64;
        }
        SemanticScene::Macro => {
            contrast = (contrast + 14.0 * mult as f64).max(15.0);
            highlights = (highlights - 24.0 * mult as f64).min(-20.0);
            whites = (whites + 12.0 * mult as f64).max(10.0);
            shadows = (shadows + 20.0 * mult as f64).max(16.0);
            clarity = (clarity + 18.0 * mult as f64).max(14.0);
            sharpening = 40.0 * mult as f64;
            sharpness_threshold = (38.0 * mult as f64).max(32.0);
            vibrance = (vibrance + 14.0 * mult as f64).max(12.0);
        }
        SemanticScene::Hdr => {
            contrast = (contrast + 8.0 * mult as f64).clamp(-10.0, 20.0);
            highlights = (highlights - 32.0 * mult as f64).min(-28.0);
            shadows = (shadows + 28.0 * mult as f64).max(22.0);
            whites = (whites + 6.0 * mult as f64).clamp(-10.0, 15.0);
            blacks = (blacks - 8.0 * mult as f64).min(-6.0);
            clarity = (clarity + 14.0 * mult as f64).max(10.0);
            vibrance = (vibrance + 15.0 * mult as f64).max(12.0);
            sharpening = 32.0 * mult as f64;
        }
        SemanticScene::Panorama => {
            contrast = (contrast + 12.0 * mult as f64).max(12.0);
            highlights = (highlights - 24.0 * mult as f64).min(-18.0);
            shadows = (shadows + 18.0 * mult as f64).max(14.0);
            whites = (whites + 12.0 * mult as f64).max(10.0);
            blacks = (blacks - 12.0 * mult as f64).min(-10.0);
            clarity = (clarity + 14.0 * mult as f64).max(12.0);
            dehaze = (dehaze + 12.0 * mult as f64).max(10.0);
            vibrance = (vibrance + 14.0 * mult as f64).max(12.0);
            sharpening = 34.0 * mult as f64;
        }
        SemanticScene::General => {
            sharpening = 30.0 * mult as f64;
        }
    }

    // Tone Curve Profile modifier
    match curve_style {
        ToneCurveStyle::FilmicDynamic => {
            highlights = (highlights - 6.0).min(-8.0);
            shadows = (shadows + 6.0).max(8.0);
            contrast = (contrast * 0.95).clamp(-30.0, 30.0);
        }
        ToneCurveStyle::PunchyCommercial => {
            contrast = (contrast * 1.25 + 6.0).clamp(-40.0, 45.0);
            blacks = (blacks - 6.0).min(-8.0);
            clarity = (clarity + 8.0).clamp(-20.0, 35.0);
            vibrance = (vibrance + 6.0).clamp(-20.0, 35.0);
        }
        ToneCurveStyle::NaturalSoft => {
            contrast = (contrast * 0.75 - 4.0).clamp(-20.0, 20.0);
            highlights = (highlights - 4.0).min(-6.0);
            clarity = (clarity * 0.7).clamp(-10.0, 10.0);
        }
    }

    let mut json = serde_json::json!({
        "exposure": exposure,
        "contrast": contrast,
        "highlights": highlights,
        "shadows": shadows,
        "whites": whites,
        "blacks": blacks,
        "clarity": clarity,
        "dehaze": dehaze,
        "temperature": temperature,
        "tint": tint,
        "vibrance": vibrance,
        "sharpness": sharpening,
        "sharpnessThreshold": sharpness_threshold,
        "colorNoiseReduction": color_noise_reduction,
        "lumaNoiseReduction": luma_noise_reduction,
        "activeTools": {
            "basic": true,
            "colorPresence": true,
            "presence": true,
            "sharpening": true,
            "noiseReduction": true
        }
    });

    if matches!(scene, SemanticScene::Portrait) {
        json["skinShield"] = serde_json::json!(0.95);
        json["sharpnessThreshold"] = serde_json::json!((sharpness_threshold).max(28.0));
    } else if matches!(scene, SemanticScene::Macro) {
        json["bokehMaskProtection"] = serde_json::json!(45);
        json["structure"] = serde_json::json!(16.0 * mult as f64);
        json["chromaticAberrationRedCyan"] = serde_json::json!(-6.0 * mult as f64);
        json["chromaticAberrationBlueYellow"] = serde_json::json!(4.0 * mult as f64);
    } else if matches!(scene, SemanticScene::Architecture) {
        json["structure"] = serde_json::json!(14.0 * mult as f64);
    } else if matches!(scene, SemanticScene::Hdr) {
        json["toneMapper"] = serde_json::json!("agx");
    } else if matches!(scene, SemanticScene::Panorama) {
        json["vignetteAmount"] = serde_json::json!(0.0);
        json["centré"] = serde_json::json!(0.0);
    }

    if let Some(angle) = horizon_angle {
        if angle.abs() > 0.3 && angle.abs() < 10.0 {
            json["rotation"] = serde_json::json!(-angle);
        }
    }

    json
}

#[derive(Debug, Clone)]
struct PreScannedImage {
    pub path_str: String,
    pub timestamp_epoch: u64,
    pub scene: SemanticScene,
    pub base_analysis: AutoAdjustmentResults,
    pub sharpness: f32,
    pub mean_luma: f32,
    pub mean_ok_a: f32,
    pub mean_ok_b: f32,
    pub horizon_angle: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatureSubMode {
    ForestCanopy,
    GoldenHour,
    MistyAtmospheric,
    AlpineWater,
}

pub fn detect_nature_submode(image: &DynamicImage, scene: SemanticScene) -> (Option<NatureSubMode>, bool) {
    if !matches!(scene, SemanticScene::Landscape | SemanticScene::Overcast | SemanticScene::Sunset) {
        return (None, false);
    }
    let thumb = image.thumbnail(256, 256).to_rgb8();
    let (w, h) = thumb.dimensions();
    let total = (w * h).max(1) as f32;
    let mut mist_px = 0.0f32;
    let mut canopy_px = 0.0f32;
    let mut alpine_px = 0.0f32;
    let mut gold_px = 0.0f32;

    for p in thumb.pixels() {
        let r = p[0] as f32 / 255.0;
        let g = p[1] as f32 / 255.0;
        let b = p[2] as f32 / 255.0;
        let max_c = r.max(g).max(b);
        let min_c = r.min(g).min(b);
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let sat = if max_c > 1e-4 { (max_c - min_c) / max_c } else { 0.0 };

        if luma > 0.50 && sat < 0.20 {
            mist_px += 1.0;
        } else if g > r * 1.12 && g > b * 1.12 {
            canopy_px += 1.0;
        } else if b > r * 1.15 && luma > 0.25 {
            alpine_px += 1.0;
        } else if r > g * 1.15 && g > b * 1.25 {
            gold_px += 1.0;
        }
    }

    let mist_ratio = mist_px / total;
    if mist_ratio > 0.26 && scene != SemanticScene::Sunset {
        return (Some(NatureSubMode::MistyAtmospheric), true);
    }
    if gold_px / total > 0.14 || scene == SemanticScene::Sunset {
        return (Some(NatureSubMode::GoldenHour), false);
    }
    if alpine_px / total > canopy_px / total && alpine_px / total > 0.14 {
        return (Some(NatureSubMode::AlpineWater), false);
    }
    if canopy_px / total > 0.12 {
        return (Some(NatureSubMode::ForestCanopy), false);
    }
    (None, false)
}

pub fn apply_nature_submode(adj: &mut serde_json::Value, submode: Option<NatureSubMode>, mist_guard: bool) {
    match submode {
        Some(NatureSubMode::MistyAtmospheric) => {
            if let Some(d) = adj.get("dehaze").and_then(|v| v.as_f64()) {
                adj["dehaze"] = serde_json::json!(d.min(8.0));
            }
            if let Some(c) = adj.get("clarity").and_then(|v| v.as_f64()) {
                adj["clarity"] = serde_json::json!((c * 0.7).clamp(-20.0, 14.0));
            }
        }
        Some(NatureSubMode::ForestCanopy) => {
            let vib = adj.get("vibrance").and_then(|v| v.as_f64()).unwrap_or(0.0);
            adj["vibrance"] = serde_json::json!((vib + 4.0).clamp(-30.0, 35.0));
        }
        Some(NatureSubMode::GoldenHour) => {
            let hl = adj.get("highlights").and_then(|v| v.as_f64()).unwrap_or(0.0);
            adj["highlights"] = serde_json::json!(hl.min(-18.0));
        }
        Some(NatureSubMode::AlpineWater) => {
            let cl = adj.get("clarity").and_then(|v| v.as_f64()).unwrap_or(0.0);
            adj["clarity"] = serde_json::json!((cl + 5.0).clamp(-20.0, 35.0));
        }
        None => {}
    }
    if mist_guard {
        let d = adj.get("dehaze").and_then(|v| v.as_f64()).unwrap_or(0.0);
        adj["dehaze"] = serde_json::json!(d.min(8.0));
    }
}

pub fn apply_learned_auto_prior_to_json(
    adj: &mut serde_json::Value,
    camera_key: &str,
    scene_key: &str,
) {
    let Some(prior) = crate::enhanced::local_priors::get_auto_enhance_prior(camera_key, scene_key) else {
        return;
    };
    let add_clamped = |obj: &mut serde_json::Value, key: &str, delta: f32, lo: f64, hi: f64| {
        if delta.abs() > 1e-4
            && let Some(cur) = obj.get(key).and_then(|v| v.as_f64())
        {
            obj[key] = serde_json::json!((cur + delta as f64).clamp(lo, hi));
        }
    };
    add_clamped(adj, "exposure", prior.exposure, -5.0, 5.0);
    add_clamped(adj, "contrast", prior.contrast, -100.0, 100.0);
    add_clamped(adj, "highlights", prior.highlights, -100.0, 100.0);
    add_clamped(adj, "shadows", prior.shadows, -100.0, 100.0);
    add_clamped(adj, "whites", prior.whites, -100.0, 100.0);
    add_clamped(adj, "blacks", prior.blacks, -100.0, 100.0);
    add_clamped(adj, "temperature", prior.temperature, -100.0, 100.0);
    add_clamped(adj, "vibrance", prior.vibrance, -100.0, 100.0);
    add_clamped(adj, "clarity", prior.clarity, -100.0, 100.0);
}

/// Fast <= 512px linear Rgb32F closed-loop verification step (<= 3 micro-steps, < 15 ms).
pub fn verify_adjustments_on_linear_preview(
    image: &DynamicImage,
    adj: &mut serde_json::Value,
    protect_skin: bool,
) {
    let thumb = image.thumbnail(512, 512).to_rgb32f();
    let raw = thumb.as_raw();
    let total_px = (raw.len() / 3).max(1) as f32;

    for _step in 0..3 {
        let exposure = adj.get("exposure").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let contrast = adj.get("contrast").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let highlights = adj.get("highlights").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let blacks = adj.get("blacks").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let shadows = adj.get("shadows").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let temp = adj.get("temperature").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let tint = adj.get("tint").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

        let ev_gain = 2.0f32.powf(exposure.clamp(-4.0, 4.0));
        let contrast_k = 1.0 + (contrast / 100.0) * 0.45;
        let hl_scale = 1.0 + (highlights / 100.0) * 0.25;
        let shadow_lift = (blacks.max(0.0) * 0.0005 + shadows.max(0.0) * 0.0003).clamp(0.0, 0.04);
        let black_crush = (-blacks.min(0.0) * 0.0004).clamp(0.0, 0.03);

        let mut clip_count = 0usize;
        let mut crush_count = 0usize;
        let mut skin_hue_drift_sum = 0.0f32;
        let mut skin_count = 0usize;

        for px in raw.chunks_exact(3) {
            let mut r = px[0] * ev_gain;
            let mut g = px[1] * ev_gain;
            let mut b = px[2] * ev_gain;

            r = (r - 0.18) * contrast_k + 0.18;
            g = (g - 0.18) * contrast_k + 0.18;
            b = (b - 0.18) * contrast_k + 0.18;

            if r.max(g).max(b) > 0.65 {
                r *= hl_scale;
                g *= hl_scale;
                b *= hl_scale;
            }
            r = (r + shadow_lift - black_crush).max(0.0);
            g = (g + shadow_lift - black_crush).max(0.0);
            b = (b + shadow_lift - black_crush).max(0.0);

            if r > 0.995 || g > 0.995 || b > 0.995 {
                clip_count += 1;
            }
            let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            if luma < 0.008 {
                crush_count += 1;
            }

            if protect_skin && (px[0] > 0.12 && px[0] > px[1] && px[1] > px[2]) {
                let (l0, a0, b0) = srgb_to_oklab(
                    px[0].clamp(0.0, 1.0) * 255.0,
                    px[1].clamp(0.0, 1.0) * 255.0,
                    px[2].clamp(0.0, 1.0) * 255.0,
                );
                if l0 > 0.35 && l0 < 0.85 && a0 > 0.02 && b0 > 0.01 {
                    let sr = (r * (1.0 + temp * 0.003)).clamp(0.0, 1.0);
                    let sg = (g * (1.0 - tint * 0.002)).clamp(0.0, 1.0);
                    let sb = (b * (1.0 - temp * 0.003)).clamp(0.0, 1.0);
                    let (_l1, a1, b1) =
                        srgb_to_oklab(sr * 255.0, sg * 255.0, sb * 255.0);
                    let h0 = b0.atan2(a0).to_degrees();
                    let h1 = b1.atan2(a1).to_degrees();
                    skin_hue_drift_sum += (h1 - h0).abs();
                    skin_count += 1;
                }
            }
        }

        let clip_pct = (clip_count as f32 / total_px) * 100.0;
        let crush_pct = (crush_count as f32 / total_px) * 100.0;
        let mean_skin_drift = if skin_count > 0 {
            skin_hue_drift_sum / skin_count as f32
        } else {
            0.0
        };

        let mut adjusted = false;
        if clip_pct > 0.5 {
            adj["exposure"] = serde_json::json!((exposure as f64 - 0.12).clamp(-3.0, 3.0));
            adj["highlights"] = serde_json::json!((highlights as f64 - 8.0).clamp(-100.0, 100.0));
            adjusted = true;
        }
        if crush_pct > 2.5 {
            adj["blacks"] = serde_json::json!((blacks as f64 + 6.0).clamp(-100.0, 100.0));
            adj["shadows"] = serde_json::json!((shadows as f64 + 6.0).clamp(-100.0, 100.0));
            adjusted = true;
        }
        if protect_skin && mean_skin_drift >= 1.0 {
            adj["temperature"] = serde_json::json!((temp as f64 * 0.65).clamp(-100.0, 100.0));
            adj["tint"] = serde_json::json!((tint as f64 * 0.65).clamp(-100.0, 100.0));
            if let Some(vib) = adj.get("vibrance").and_then(|v| v.as_f64()) {
                adj["vibrance"] = serde_json::json!(vib.min(14.0));
            }
            adjusted = true;
        }
        if !adjusted {
            break;
        }
    }
}

#[tauri::command]
pub fn analyze_and_polish_active_image(
    intensity: Option<f32>,
    auto_straighten: Option<bool>,
    tone_style: Option<String>,
    skin_protection: Option<bool>,
    scene_override: Option<String>,
    app_handle: AppHandle,
    state: State<AppState>,
) -> Result<SemanticAnalysisResult, String> {
    let intensity_val = intensity.unwrap_or(100.0);
    let straighten = auto_straighten.unwrap_or(false);
    let style = ToneCurveStyle::from_str_opt(tone_style.as_deref());
    let protect_skin = skin_protection.unwrap_or(true);

    let (image_arc, path_str, is_raw, as_shot) = {
        let orig_guard = state.original_image.lock().map_err(|e| e.to_string())?;
        let loaded = orig_guard
            .as_ref()
            .ok_or_else(|| "No active image loaded".to_string())?;
        (Arc::clone(&loaded.image), loaded.path.clone(), loaded.is_raw, loaded.as_shot)
    };

    let render = crate::render::options::resolve_render_options_from_handle(&app_handle, is_raw);
    let baseline_ev = crate::render::adjustment_utils::effective_baseline_exposure(
        &serde_json::json!({}),
        as_shot,
        render.baseline,
    );
    let display_preview = prepare_display_preview_for_analysis(&image_arc, baseline_ev);

    let (scene, confidence) = if let Some(forced) = SemanticScene::from_str_opt(scene_override.as_deref()) {
        (forced, 1.0)
    } else {
        detect_semantic_scene_with_hint(&display_preview, Some(&path_str))
    };
    let base_analysis = perform_auto_analysis(&image_arc, baseline_ev);
    let (submode, mist_guard) = detect_nature_submode(&display_preview, scene);

    let horizon_angle = if straighten {
        let horizon = detect_horizon_angle(&image_arc);
        if horizon.confidence > 0.35 {
            Some(horizon.angle_degrees)
        } else {
            None
        }
    } else {
        None
    };

    let mut adjustments = generate_semantic_adjustments(
        scene,
        &base_analysis,
        intensity_val,
        horizon_angle,
        Some(style),
        Some(protect_skin),
    );
    apply_nature_submode(&mut adjustments, submode, mist_guard);

    let (source_path, _) = parse_virtual_path(&path_str);
    let cam_key = if let Ok(bytes) = fs::read(&source_path)
        && let Some((make, model)) = crate::metadata::exif_processing::read_camera_make_model(&path_str, &bytes)
    {
        crate::enhanced::local_priors::normalize_camera_key(&make, &model)
    } else {
        "universal".to_string()
    };
    let scene_key = format!("{:?}", scene);
    apply_learned_auto_prior_to_json(&mut adjustments, &cam_key, &scene_key);
    verify_adjustments_on_linear_preview(&image_arc, &mut adjustments, protect_skin);

    Ok(SemanticAnalysisResult {
        scene,
        scene_name: scene.display_name().to_string(),
        confidence,
        horizon_angle,
        adjustments,
    })
}

#[tauri::command]
pub fn analyze_night_sky_active_image(
    intensity: Option<f32>,
    state: State<AppState>,
) -> Result<SemanticAnalysisResult, String> {
    let intensity_val = intensity.unwrap_or(100.0);
    let (image_arc, path_str) = {
        let orig_guard = state.original_image.lock().map_err(|e| e.to_string())?;
        let loaded = orig_guard
            .as_ref()
            .ok_or_else(|| "No active image loaded".to_string())?;
        (Arc::clone(&loaded.image), loaded.path.clone())
    };

    let base_analysis = perform_auto_analysis(&image_arc, 0.0);
    let thumb = image_arc.thumbnail(320, 320).to_rgb8();
    let (w, h) = thumb.dimensions();
    let mut orange_sky_cast = 0.0f32;
    let mut sky_count = 0.0f32;
    for y in 0..(h * 2 / 3).max(1) {
        for x in 0..w {
            let p = thumb.get_pixel(x, y);
            let r = p[0] as f32;
            let g = p[1] as f32;
            let b = p[2] as f32;
            let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            if luma < 70.0 {
                orange_sky_cast += (r - b).max(0.0);
                sky_count += 1.0;
            }
        }
    }
    let avg_amber_cast = if sky_count > 10.0 {
        (orange_sky_cast / sky_count).clamp(0.0, 25.0)
    } else {
        6.0
    };

    let mut adjustments = generate_semantic_adjustments(
        SemanticScene::NightSky,
        &base_analysis,
        intensity_val,
        None,
        Some(ToneCurveStyle::FilmicDynamic),
        Some(false),
    );
    let cur_temp = adjustments.get("temperature").and_then(|v| v.as_f64()).unwrap_or(-8.0);
    adjustments["temperature"] =
        serde_json::json!((cur_temp - (avg_amber_cast as f64 * 0.35)).clamp(-24.0, -4.0));
    adjustments["dehaze"] = serde_json::json!(
        adjustments.get("dehaze").and_then(|v| v.as_f64()).unwrap_or(20.0).min(24.0)
    );
    adjustments["sharpnessThreshold"] = serde_json::json!(62.0);

    let (source_path, _) = parse_virtual_path(&path_str);
    let cam_key = if let Ok(bytes) = fs::read(&source_path)
        && let Some((make, model)) = crate::metadata::exif_processing::read_camera_make_model(&path_str, &bytes)
    {
        crate::enhanced::local_priors::normalize_camera_key(&make, &model)
    } else {
        "universal".to_string()
    };
    apply_learned_auto_prior_to_json(&mut adjustments, &cam_key, "NightSky");
    verify_adjustments_on_linear_preview(&image_arc, &mut adjustments, false);

    Ok(SemanticAnalysisResult {
        scene: SemanticScene::NightSky,
        scene_name: SemanticScene::NightSky.display_name().to_string(),
        confidence: 0.92,
        horizon_angle: None,
        adjustments,
    })
}

fn rollback_sidecar_snapshots(snapshots: &[(std::path::PathBuf, Option<Vec<u8>>)]) {
    for (path, prior) in snapshots.iter().rev() {
        match prior {
            Some(bytes) => {
                let _ = fs::write(path, bytes);
            }
            None => {
                let _ = fs::remove_file(path);
            }
        }
    }
}

#[tauri::command]
pub async fn batch_polish_photoshoot(
    paths: Vec<String>,
    intensity: Option<f32>,
    harmonize: Option<bool>,
    auto_straighten: Option<bool>,
    enable_xmp_sync: Option<bool>,
    tone_style: Option<String>,
    skin_protection: Option<bool>,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<BatchPolishSummary, String> {
    if paths.is_empty() {
        return Err("No photos provided for batch polish".to_string());
    }
    state.ai_denoise_cancel.store(false, Ordering::SeqCst);
    let cancel_token = state.ai_denoise_cancel.clone();

    let _sleep_guard = crate::enhanced::sleep_lock::SleepLockGuard::new("batch_polish_photoshoot");
    let start_time = std::time::Instant::now();
    let intensity_val = intensity.unwrap_or(100.0);
    let do_harmonize = harmonize.unwrap_or(true);
    let straighten = auto_straighten.unwrap_or(false);
    let sync_xmp = enable_xmp_sync.unwrap_or(false);
    let style = ToneCurveStyle::from_str_opt(tone_style.as_deref());
    let protect_skin = skin_protection.unwrap_or(true);
    let settings = crate::app::settings::load_settings(app_handle.clone()).unwrap_or_default();

    let total = paths.len();
    let processed_counter = Arc::new(AtomicUsize::new(0));

    let _ = app_handle.emit(
        "batch-polish-progress",
        serde_json::json!({
            "current": 0,
            "total": total,
            "message": format!("Pre-scanning and clustering {} photos...", total),
            "percentage": 0.0
        }),
    );

    let app_handle_clone = app_handle.clone();
    let paths_clone = paths.clone();

    let (scene_breakdown, total_success, cluster_count) = tokio::task::spawn_blocking(move || {
        // Stage 1: Parallel Pre-Scan
        let pre_scanned: Vec<Option<PreScannedImage>> = paths_clone
            .par_iter()
            .map(|path_str| {
                if cancel_token.load(Ordering::Relaxed) {
                    return None;
                }
                let (source_path, _) = parse_virtual_path(path_str);
                let file_bytes = match read_file_mapped(&source_path) {
                    Ok(b) => b,
                    Err(e) => {
                        log::warn!("[BatchPolish] Failed to read file {}: {}", path_str, e);
                        return None;
                    }
                };

                let image = match load_base_image_from_bytes(
                    &file_bytes,
                    &source_path.to_string_lossy(),
                    true,
                    &settings,
                    None,
                ) {
                    Ok(img) => img,
                    Err(e) => {
                        log::warn!("[BatchPolish] Failed to decode image {}: {}", path_str, e);
                        return None;
                    }
                };

                let display_preview = prepare_display_preview_for_analysis(&image, 0.0);
                let thumb = display_preview.thumbnail(256, 256).to_rgb8();
                let sharpness = compute_fast_sharpness(&thumb);
                let (scene, _) = detect_semantic_scene_with_hint(&display_preview, Some(path_str));
                let base_analysis = perform_auto_analysis(&image, 0.0);

                let mut sum_luma = 0.0f32;
                let mut sum_ok_a = 0.0f32;
                let mut sum_ok_b = 0.0f32;
                let n = (thumb.width() * thumb.height()) as f32;

                for p in thumb.pixels() {
                    let r = p[0] as f32;
                    let g = p[1] as f32;
                    let b = p[2] as f32;
                    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                    let (_, ok_a, ok_b) = srgb_to_oklab(r, g, b);
                    sum_luma += luma;
                    sum_ok_a += ok_a;
                    sum_ok_b += ok_b;
                }

                let mean_luma = if n > 0.0 { sum_luma / n } else { 128.0 };
                let mean_ok_a = if n > 0.0 { sum_ok_a / n } else { 0.0 };
                let mean_ok_b = if n > 0.0 { sum_ok_b / n } else { 0.0 };

                let timestamp_epoch = match fs::metadata(&source_path) {
                    Ok(meta) => meta
                        .modified()
                        .unwrap_or(SystemTime::UNIX_EPOCH)
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    Err(_) => 0,
                };

                let horizon_angle = if straighten {
                    let horizon = detect_horizon_angle(&image);
                    if horizon.confidence > 0.35 {
                        Some(horizon.angle_degrees)
                    } else {
                        None
                    }
                } else {
                    None
                };

                Some(PreScannedImage {
                    path_str: path_str.clone(),
                    timestamp_epoch,
                    scene,
                    base_analysis,
                    sharpness,
                    mean_luma,
                    mean_ok_a,
                    mean_ok_b,
                    horizon_angle,
                })
            })
            .collect();

        let valid_scanned: Vec<PreScannedImage> = pre_scanned.into_iter().flatten().collect();

        // Stage 2: Temporal & Chromatic Shoot Clustering
        let clusters: Vec<Vec<PreScannedImage>> = if do_harmonize && !valid_scanned.is_empty() {
            let mut groups: Vec<Vec<PreScannedImage>> = Vec::new();
            let mut current_group: Vec<PreScannedImage> = vec![valid_scanned[0].clone()];

            for i in 1..valid_scanned.len() {
                let prev = &valid_scanned[i - 1];
                let curr = &valid_scanned[i];

                let time_gap = curr.timestamp_epoch.abs_diff(prev.timestamp_epoch);
                let luma_gap = (curr.mean_luma - prev.mean_luma).abs();
                let color_gap = (curr.mean_ok_a - prev.mean_ok_a).abs() + (curr.mean_ok_b - prev.mean_ok_b).abs();

                // Group together if taken within 25s, matching scene, and similar ambient light
                if time_gap <= 25 && curr.scene == prev.scene && luma_gap < 40.0 && color_gap < 0.06 {
                    current_group.push(curr.clone());
                } else {
                    groups.push(current_group);
                    current_group = vec![curr.clone()];
                }
            }
            groups.push(current_group);
            groups
        } else {
            valid_scanned.iter().cloned().map(|item| vec![item]).collect()
        };

        let total_clusters = clusters.len();
        let mut breakdown: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut success_count = 0;

        if cancel_token.load(Ordering::Relaxed) {
            return Err("Batch polish cancelled by user".to_string());
        }

        // Stage 3: Harmonized Adjustment Generation in memory
        let staged_writes: std::sync::Mutex<Vec<(std::path::PathBuf, std::path::PathBuf, serde_json::Value, String)>> =
            std::sync::Mutex::new(Vec::new());

        clusters.par_iter().for_each(|cluster| {
            if cluster.is_empty() || cancel_token.load(Ordering::Relaxed) {
                return;
            }

            // Find cluster anchor frame (sharpest, well-exposed hero shot)
            let anchor = cluster
                .iter()
                .max_by(|a, b| {
                    let a_score = a.sharpness * (1.0 - (a.mean_luma - 128.0).abs() / 256.0);
                    let b_score = b.sharpness * (1.0 - (b.mean_luma - 128.0).abs() / 256.0);
                    a_score.partial_cmp(&b_score).unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap_or(&cluster[0]);

            let anchor_adj = generate_semantic_adjustments(
                anchor.scene,
                &anchor.base_analysis,
                intensity_val,
                anchor.horizon_angle,
                Some(style),
                Some(protect_skin),
            );

            let anchor_exp = anchor_adj["exposure"].as_f64().unwrap_or(0.0);
            let anchor_temp = anchor_adj["temperature"].as_f64().unwrap_or(0.0);
            let anchor_tint = anchor_adj["tint"].as_f64().unwrap_or(0.0);
            let anchor_contrast = anchor_adj["contrast"].as_f64().unwrap_or(0.0);

            for item in cluster {
                if cancel_token.load(Ordering::Relaxed) {
                    return;
                }
                let (_, sidecar_path) = parse_virtual_path(&item.path_str);
                let (source_path, _) = parse_virtual_path(&item.path_str);

                let mut item_adj = generate_semantic_adjustments(
                    item.scene,
                    &item.base_analysis,
                    intensity_val,
                    item.horizon_angle,
                    Some(style),
                    Some(protect_skin),
                );

                // Harmonize with anchor if part of a multi-shot cluster
                if cluster.len() > 1 && do_harmonize {
                    let local_exp = item_adj["exposure"].as_f64().unwrap_or(0.0);
                    let local_temp = item_adj["temperature"].as_f64().unwrap_or(0.0);
                    let local_tint = item_adj["tint"].as_f64().unwrap_or(0.0);
                    let local_contrast = item_adj["contrast"].as_f64().unwrap_or(0.0);

                    item_adj["exposure"] = serde_json::json!(0.65 * anchor_exp + 0.35 * local_exp);
                    item_adj["temperature"] = serde_json::json!(0.70 * anchor_temp + 0.30 * local_temp);
                    item_adj["tint"] = serde_json::json!(0.70 * anchor_tint + 0.30 * local_tint);
                    item_adj["contrast"] = serde_json::json!(0.60 * anchor_contrast + 0.40 * local_contrast);
                }

                if let Ok(mut guard) = staged_writes.lock() {
                    guard.push((source_path, sidecar_path, item_adj, item.path_str.clone()));
                }

                let done = processed_counter.fetch_add(1, Ordering::SeqCst) + 1;
                let percent = (done as f32 / total as f32) * 100.0;

                let filename = Path::new(&item.path_str)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();

                let _ = app_handle_clone.emit(
                    "batch-polish-progress",
                    serde_json::json!({
                        "current": done,
                        "total": total,
                        "filename": filename,
                        "scene": item.scene.display_name(),
                        "percentage": percent
                    }),
                );
            }
        });

        if cancel_token.load(Ordering::Relaxed) {
            return Err("Batch polish cancelled by user".to_string());
        }

        // Atomic commit with snapshot rollback on cancel
        let staged = staged_writes.into_inner().unwrap_or_default();
        let mut prior_snapshots: Vec<(std::path::PathBuf, Option<Vec<u8>>)> = Vec::with_capacity(staged.len());

        for (source_path, sidecar_path, item_adj, _) in &staged {
            if cancel_token.load(Ordering::Relaxed) {
                rollback_sidecar_snapshots(&prior_snapshots);
                return Err("Batch polish cancelled by user — changes rolled back".to_string());
            }
            let prev = fs::read(sidecar_path).ok();
            prior_snapshots.push((sidecar_path.clone(), prev));
            let mut metadata = load_sidecar(sidecar_path);
            metadata.adjustments = item_adj.clone();
            if let Ok(json_string) = serde_json::to_string_pretty(&metadata) {
                if let Some(parent) = sidecar_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(sidecar_path, json_string);
            }
            if sync_xmp {
                sync_metadata_to_xmp(source_path, sidecar_path, &metadata, false);
            }
        }

        for item in &valid_scanned {
            success_count += 1;
            *breakdown.entry(item.scene.display_name().to_string()).or_insert(0) += 1;
        }

        Ok::<_, String>((breakdown, success_count, total_clusters))
    })
    .await
    .map_err(|e| format!("Batch polish task failed: {}", e))??;

    let elapsed = start_time.elapsed().as_millis() as u64;

    let _ = app_handle.emit(
        "batch-polish-complete",
        serde_json::json!({
            "totalProcessed": total_success,
            "totalClusters": cluster_count,
            "sceneBreakdown": scene_breakdown,
            "elapsedMs": elapsed
        }),
    );

    Ok(BatchPolishSummary {
        total_processed: total_success,
        total_clusters: cluster_count,
        scene_breakdown,
        elapsed_ms: elapsed,
    })
}

/// 1. Master Smart-Strength Piecewise Asymmetric Headroom Curve
pub fn apply_super_slider_strength(nominal: f32, min_val: f32, max_ceiling: f32, strength_pct: f32) -> f32 {
    let s = strength_pct.clamp(0.0, 200.0);
    let delta = (s - 100.0) / 100.0; // [-1.0, +1.0]
    if delta <= 0.0 {
        nominal + (nominal - min_val) * delta
    } else {
        nominal + (max_ceiling - nominal) * delta
    }
}

/// 2. 10-Cluster Dermatological Skin Calibration & Deflash Roll-off (from rapidraw-dsp)
pub use rapidraw_dsp::{
    apply_deflash_specular_rolloff, classify_skin_cluster_oklab, SkinClusterCalibration,
};


/// 3. Scene-Governed Auto-Exposure with Deadband & Preprocess EV (pro_and_universal_20211221.pnn)
pub fn compute_governed_exposure(raw_ev_delta: f32, governor_pct: f32, min_strength_pct: f32, preprocess_ev: f32) -> f32 {
    let min_ev = (min_strength_pct / 100.0) * 0.5;
    if raw_ev_delta.abs() < min_ev {
        return preprocess_ev;
    }
    let max_ev = (governor_pct / 100.0) * 1.5; // e.g. governor=5 -> +0.075 EV max for Night; governor=25 -> +0.375 EV
    raw_ev_delta.clamp(-max_ev, max_ev) + preprocess_ev
}

/// 4. Pre-tone automatic noise variance gate evaluation
pub fn evaluate_pre_tone_noise_variance_gate(thumb: &image::RgbImage, iso: Option<u32>) -> (bool, f32, f32) {
    let (w, h) = thumb.dimensions();
    if w < 16 || h < 16 {
        return (false, 0.0, 0.0);
    }

    let mut patch_vars = Vec::with_capacity(64);
    let step_x = ((w - 4) / 8).max(1);
    let step_y = ((h - 4) / 8).max(1);

    for py in (2..h - 2).step_by(step_y as usize) {
        for px in (2..w - 2).step_by(step_x as usize) {
            let mut lumas = [0.0f32; 9];
            let mut idx = 0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let p = thumb.get_pixel((px as i32 + dx) as u32, (py as i32 + dy) as u32);
                    lumas[idx] = 0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32;
                    idx += 1;
                }
            }
            let mean = lumas.iter().sum::<f32>() / 9.0;
            let var = lumas.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / 9.0;
            patch_vars.push(var);
        }
    }

    patch_vars.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p20_idx = (patch_vars.len() as f32 * 0.20) as usize;
    let noise_var = patch_vars.get(p20_idx).copied().unwrap_or(0.0);

    let iso_val = iso.unwrap_or(100);
    let noise_thresh = if iso_val > 800 { 1.5 } else if iso_val > 400 { 3.0 } else { 6.0 };

    if noise_var > noise_thresh || iso_val >= 1250 {
        let rec_denoise = if iso_val >= 3200 || noise_var > 25.0 {
            65.0
        } else if iso_val >= 800 || noise_var > 12.0 {
            50.0
        } else {
            30.0
        };
        (true, noise_var, rec_denoise)
    } else {
        (false, noise_var, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_super_slider_strength() {
        // nominal=50, min=0, max=100
        // At 100%: delta=0 -> nominal 50
        assert_eq!(apply_super_slider_strength(50.0, 0.0, 100.0, 100.0), 50.0);
        // At 0%: delta=-1.0 -> 0
        assert_eq!(apply_super_slider_strength(50.0, 0.0, 100.0, 0.0), 0.0);
        // At 50%: delta=-0.5 -> 25
        assert_eq!(apply_super_slider_strength(50.0, 0.0, 100.0, 50.0), 25.0);
        // At 200%: delta=+1.0 -> max_ceiling 100
        assert_eq!(apply_super_slider_strength(50.0, 0.0, 100.0, 200.0), 100.0);
        // At 150%: delta=+0.5 -> 75
        assert_eq!(apply_super_slider_strength(50.0, 0.0, 100.0, 150.0), 75.0);
    }

    #[test]
    fn test_classify_skin_cluster_oklab() {
        let pale = classify_skin_cluster_oklab(0.85);
        assert_eq!(pale.cluster_id, 1);
        assert!(pale.exposure_bias_ev > 0.0);

        let dark = classify_skin_cluster_oklab(0.40);
        assert_eq!(dark.cluster_id, 9);
        assert!(dark.exposure_bias_ev < 0.0);

        let ebony = classify_skin_cluster_oklab(0.30);
        assert_eq!(ebony.cluster_id, 10);
        assert!(ebony.exposure_bias_ev < 0.0);

        let deflashed = apply_deflash_specular_rolloff(0.95, 0.8, 0.5, 0.72);
        assert!(deflashed < 0.95);
        assert!(deflashed > 0.72);
    }

    #[test]
    fn test_compute_governed_exposure() {
        // Inside deadband
        let deadband_exp = compute_governed_exposure(0.02, 25.0, 10.0, 0.0);
        assert_eq!(deadband_exp, 0.0);

        // Outside deadband, capped by governor (governor=25 -> max 0.375)
        let capped_exp = compute_governed_exposure(1.5, 25.0, 10.0, 0.0);
        assert!((capped_exp - 0.375).abs() < 1e-4);

        // Sunset preprocess_ev
        let sunset_exp = compute_governed_exposure(0.01, 40.0, 10.0, -0.24);
        assert_eq!(sunset_exp, -0.24);
    }

    #[test]
    fn test_evaluate_pre_tone_noise_variance_gate() {
        let clean_img = image::RgbImage::from_fn(32, 32, |_x, _y| image::Rgb([128, 128, 128]));
        let (noise_detected, var, _) = evaluate_pre_tone_noise_variance_gate(&clean_img, Some(100));
        assert!(!noise_detected);
        assert_eq!(var, 0.0);
    }

    #[test]
    fn test_verification_loop_under_15ms_and_clipping_guard() {
        let bright_img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(512, 512, |_x, _y| {
            image::Rgb([252, 252, 252])
        }));
        let mut adj = serde_json::json!({
            "exposure": 1.2,
            "contrast": 20.0,
            "highlights": 10.0,
            "shadows": 0.0,
            "blacks": 0.0,
            "temperature": 0.0,
            "tint": 0.0
        });
        let t0 = std::time::Instant::now();
        verify_adjustments_on_linear_preview(&bright_img, &mut adj, true);
        let elapsed_ms = t0.elapsed().as_millis();
        assert!(elapsed_ms < 50, "Verification step should be very fast, took {} ms", elapsed_ms);
        assert!(adj["exposure"].as_f64().unwrap() < 1.2);
    }

    #[test]
    fn test_mist_vs_haze_guard_clamps_dehaze_and_canonical_keys() {
        let mist_img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(128, 128, |_x, _y| {
            image::Rgb([180, 185, 190])
        }));
        let (submode, mist_guard) = detect_nature_submode(&mist_img, SemanticScene::Landscape);
        assert_eq!(submode, Some(NatureSubMode::MistyAtmospheric));
        assert!(mist_guard);

        let base = perform_auto_analysis(&mist_img, 0.0);
        let mut adj = generate_semantic_adjustments(
            SemanticScene::Landscape,
            &base,
            100.0,
            None,
            Some(ToneCurveStyle::FilmicDynamic),
            Some(true),
        );
        apply_nature_submode(&mut adj, submode, mist_guard);
        assert!(adj["dehaze"].as_f64().unwrap() <= 8.0);

        let defaults: serde_json::Value =
            serde_json::from_str(include_str!("../../../src/utils/defaultAdjustments.json")).unwrap();
        let default_obj = defaults.as_object().unwrap();
        for key in adj.as_object().unwrap().keys() {
            assert!(
                default_obj.contains_key(key),
                "Key '{}' must exist in defaultAdjustments.json",
                key
            );
        }
    }

    #[test]
    fn test_linear_mid_grey_not_misclassified_as_night_sky() {
        let linear_grey = DynamicImage::ImageRgb32F(image::Rgb32FImage::from_fn(64, 64, |_x, _y| {
            image::Rgb([0.18, 0.18, 0.18])
        }));
        let display_preview = prepare_display_preview_for_analysis(&linear_grey, 0.0);
        let (scene, _conf) = detect_semantic_scene_with_hint(&display_preview, None);
        assert_ne!(
            scene,
            SemanticScene::NightSky,
            "Linear 0.18 mid-grey RAW preview must not be misclassified as NightSky"
        );
    }

    #[test]
    fn test_scene_detection_and_adjustments_invariants() {
        // Panorama: wide aspect ratio or path hint
        let wide_img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(256, 64, |_x, _y| {
            image::Rgb([120, 140, 160])
        }));
        let (pano_scene, _) = detect_semantic_scene_with_hint(&wide_img, Some("mountain_pano.dng"));
        assert_eq!(pano_scene, SemanticScene::Panorama);

        // HDR: path hint
        let normal_img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(128, 128, |x, y| {
            let v = if (x + y) % 2 == 0 { 40 } else { 210 };
            image::Rgb([v, v, v])
        }));
        let (hdr_scene, _) = detect_semantic_scene_with_hint(&normal_img, Some("sunset_hdr_merge.dng"));
        assert_eq!(hdr_scene, SemanticScene::Hdr);

        let base = perform_auto_analysis(&normal_img, 0.0);
        let scenes = [
            SemanticScene::Panorama,
            SemanticScene::Hdr,
            SemanticScene::NightSky,
            SemanticScene::Portrait,
            SemanticScene::Macro,
            SemanticScene::Architecture,
            SemanticScene::Landscape,
        ];
        let defaults: serde_json::Value =
            serde_json::from_str(include_str!("../../../src/utils/defaultAdjustments.json")).unwrap();
        let default_obj = defaults.as_object().unwrap();

        for scene in scenes {
            let adj = generate_semantic_adjustments(
                scene,
                &base,
                100.0,
                Some(1.0),
                Some(ToneCurveStyle::FilmicDynamic),
                Some(true),
            );
            let obj = adj.as_object().expect("adjustments must be an object");
            assert!(!obj.is_empty(), "adjustments for {:?} should not be empty", scene);
            for (k, v) in obj {
                assert!(
                    default_obj.contains_key(k),
                    "Key '{}' for {:?} must exist in defaultAdjustments.json",
                    k,
                    scene
                );
                if let Some(num) = v.as_f64() {
                    assert!(num.is_finite(), "Value for '{}' in {:?} must be finite", k, scene);
                }
            }
        }

        // Panorama default (no horizon_angle) should not write rotation
        let pano_adj = generate_semantic_adjustments(
            SemanticScene::Panorama,
            &base,
            100.0,
            None,
            Some(ToneCurveStyle::NaturalSoft),
            Some(false),
        );
        assert!(pano_adj.get("dehaze").is_some());
        assert!(pano_adj.get("rotation").is_none());

        // NightSky should preserve dark sky and enable luma/color noise reduction
        let night_adj = generate_semantic_adjustments(
            SemanticScene::NightSky,
            &base,
            100.0,
            None,
            Some(ToneCurveStyle::FilmicDynamic),
            Some(false),
        );
        assert!(night_adj["lumaNoiseReduction"].as_f64().unwrap() > 0.0);
        assert!(night_adj["colorNoiseReduction"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn test_rollback_sidecar_snapshots_restores_and_deletes() {
        let dir = std::env::temp_dir().join(format!("rapidraw_rollback_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let existing_file = dir.join("existing.rrdata");
        let created_file = dir.join("created.rrdata");

        fs::write(&existing_file, b"original-bytes").unwrap();
        let snapshots = vec![
            (existing_file.clone(), Some(b"original-bytes".to_vec())),
            (created_file.clone(), None),
        ];

        fs::write(&existing_file, b"modified-bytes").unwrap();
        fs::write(&created_file, b"new-bytes").unwrap();

        rollback_sidecar_snapshots(&snapshots);

        assert_eq!(fs::read(&existing_file).unwrap(), b"original-bytes");
        assert!(!created_file.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}

