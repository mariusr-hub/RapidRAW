use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

pub const SCHEMA_VERSION: u32 = 1;
pub const EMA_ALPHA: f32 = 0.15;
pub const MIN_SAMPLES: u32 = 5;
pub const DUST_CONFIRM_RADIUS: f32 = 0.008;
pub const DUST_SUPPRESS_RADIUS: f32 = 0.012;
pub const MIN_EDIT_DURATION_SECS: f32 = 5.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoEnhancePrior {
    pub samples: u32,
    pub rejections: u32,
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub temperature: f32,
    pub vibrance: f32,
    pub clarity: f32,
}

impl Default for AutoEnhancePrior {
    fn default() -> Self {
        Self {
            samples: 0,
            rejections: 0,
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            temperature: 0.0,
            vibrance: 0.0,
            clarity: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DenoisePrior {
    pub samples: u32,
    pub ai_denoise_amount: f32,
    pub ai_denoise_detail: f32,
    pub luma_noise_reduction: f32,
    pub color_noise_reduction: f32,
}

impl Default for DenoisePrior {
    fn default() -> Self {
        Self {
            samples: 0,
            ai_denoise_amount: 100.0,
            ai_denoise_detail: 15.0,
            luma_noise_reduction: 0.0,
            color_noise_reduction: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IsoNoiseSample {
    pub iso: u32,
    pub sigma: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EmpiricalSensorFit {
    pub samples: Vec<IsoNoiseSample>,
    pub fitted_base_read_noise: Option<f32>,
    pub fitted_shot_noise_coeff: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IsoNoiseProfileBucket {
    pub iso_bucket: u32,
    pub samples: u32,
    pub read_noise: f32,
    pub shot_noise: f32,
    pub chroma_ratio: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DustSpotEntry {
    pub norm_x: f32,
    pub norm_y: f32,
    pub radius: f32,
    pub confirmed_count: u32,
    pub rejected_count: u32,
    #[serde(default)]
    pub seen_sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CullerSharpnessPrior {
    pub pick_count: u32,
    pub pick_sum: f32,
    pub reject_count: u32,
    pub reject_sum: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HdrScenePreference {
    pub intent_counts: HashMap<String, u32>,
    pub ema_strength: f32,
    pub samples: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NegativeDefaultsPrior {
    pub base_mask_r: f32,
    pub base_mask_g: f32,
    pub base_mask_b: f32,
    pub film_profile: String,
    pub confirmed_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPriors {
    pub schema_version: u32,
    #[serde(default)]
    pub auto_enhance: HashMap<String, AutoEnhancePrior>,
    #[serde(default)]
    pub denoise: HashMap<String, DenoisePrior>,
    #[serde(default)]
    pub empirical_sensors: HashMap<String, EmpiricalSensorFit>,
    #[serde(default)]
    pub iso_noise_buckets: HashMap<String, Vec<IsoNoiseProfileBucket>>,
    #[serde(default)]
    pub sensor_dust: HashMap<String, Vec<DustSpotEntry>>,
    #[serde(default)]
    pub culler_sharpness: HashMap<String, CullerSharpnessPrior>,
    #[serde(default)]
    pub hdr_preferences: HashMap<String, HdrScenePreference>,
    #[serde(default)]
    pub film_stock_rankings: HashMap<String, HashMap<String, u32>>,
    #[serde(default)]
    pub negative_defaults: Option<NegativeDefaultsPrior>,
}

impl Default for LocalPriors {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            auto_enhance: HashMap::new(),
            denoise: HashMap::new(),
            empirical_sensors: HashMap::new(),
            iso_noise_buckets: HashMap::new(),
            sensor_dust: HashMap::new(),
            culler_sharpness: HashMap::new(),
            hdr_preferences: HashMap::new(),
            film_stock_rankings: HashMap::new(),
            negative_defaults: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPriorsSummary {
    pub enabled: bool,
    pub total_samples: u32,
    pub auto_enhance_profiles: usize,
    pub auto_enhance_samples: u32,
    pub denoise_profiles: usize,
    pub denoise_samples: u32,
    pub dust_spots_tracked: usize,
    pub culler_profiles: usize,
    pub negative_confirmed: u32,
    pub camera_keys: Vec<String>,
}

static PRIORS_STORE: LazyLock<Arc<RwLock<LocalPriors>>> =
    LazyLock::new(|| Arc::new(RwLock::new(LocalPriors::default())));
static PRIORS_PATH: LazyLock<Mutex<Option<PathBuf>>> = LazyLock::new(|| Mutex::new(None));
static FLUSH_DIRTY: AtomicBool = AtomicBool::new(false);
static FLUSH_SCHEDULED: AtomicBool = AtomicBool::new(false);
static LEARNING_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn normalize_camera_key(make: &str, model: &str) -> String {
    let raw = format!("{} {}", make.trim(), model.trim());
    let cleaned: String = raw
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_");
    if cleaned.is_empty() {
        "universal".to_string()
    } else {
        cleaned
    }
}

pub fn normalize_lens_key(lens: &str) -> String {
    let cleaned: String = lens
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_");
    if cleaned.is_empty() {
        "unknown_lens".to_string()
    } else {
        cleaned
    }
}

pub fn iso_bucket(iso: u32) -> &'static str {
    if iso <= 400 {
        "iso_le_400"
    } else if iso <= 1600 {
        "iso_401_1600"
    } else if iso <= 6400 {
        "iso_1601_6400"
    } else {
        "iso_gt_6400"
    }
}

pub fn set_learning_enabled(enabled: bool) {
    LEARNING_ENABLED.store(enabled, Ordering::Release);
}

pub fn is_learning_enabled() -> bool {
    LEARNING_ENABLED.load(Ordering::Acquire)
}

pub fn init_local_priors_path(app_data_dir: PathBuf, enabled: bool) {
    set_learning_enabled(enabled);
    let file_path = app_data_dir.join("local_priors.json");
    if let Ok(mut guard) = PRIORS_PATH.lock() {
        *guard = Some(file_path.clone());
    }
    if let Ok(meta) = fs::metadata(&file_path)
        && meta.len() > 0
        && let Ok(text) = fs::read_to_string(&file_path)
        && let Ok(parsed) = serde_json::from_str::<LocalPriors>(&text)
        && parsed.schema_version == SCHEMA_VERSION
        && let Ok(mut store) = PRIORS_STORE.write()
    {
        *store = parsed;
    }
}

fn flush_now() {
    FLUSH_DIRTY.store(false, Ordering::Release);
    let path_opt = PRIORS_PATH.lock().ok().and_then(|g| g.clone());
    let Some(path) = path_opt else {
        return;
    };
    let snapshot = match PRIORS_STORE.read() {
        Ok(guard) => guard.clone(),
        Err(_) => return,
    };
    if let Ok(json_bytes) = serde_json::to_vec_pretty(&snapshot) {
        let _ = crate::library::file_ops::write_file_atomic(&path, &json_bytes);
    }
}

fn schedule_flush() {
    FLUSH_DIRTY.store(true, Ordering::Release);
    if FLUSH_SCHEDULED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(400));
            FLUSH_SCHEDULED.store(false, Ordering::Release);
            if FLUSH_DIRTY.load(Ordering::Acquire) {
                flush_now();
            }
        });
    }
}

fn valid_delta(delta: f32, bound: f32) -> Option<f32> {
    if !delta.is_finite() || delta.abs() > 3.0 * bound {
        None
    } else {
        Some(delta.clamp(-bound, bound))
    }
}

fn ema_step(prev: f32, sample: f32, bound: f32) -> f32 {
    ((1.0 - EMA_ALPHA) * prev + EMA_ALPHA * sample).clamp(-bound, bound)
}

pub fn auto_enhance_key(camera_key: &str, scene_key: &str) -> String {
    format!("{}|{}", camera_key, scene_key.trim().to_lowercase())
}

pub fn get_auto_enhance_prior(camera_key: &str, scene_key: &str) -> Option<AutoEnhancePrior> {
    if !is_learning_enabled() {
        return None;
    }
    let key = auto_enhance_key(camera_key, scene_key);
    let guard = PRIORS_STORE.read().ok()?;
    let prior = guard.auto_enhance.get(&key)?;
    if prior.samples >= MIN_SAMPLES {
        Some(prior.clone())
    } else {
        None
    }
}

#[derive(Debug, Clone, Default)]
pub struct AutoEnhanceDelta {
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub temperature: f32,
    pub vibrance: f32,
    pub clarity: f32,
}

pub fn record_auto_enhance_confirmation(
    camera_key: &str,
    scene_key: &str,
    delta: AutoEnhanceDelta,
    edit_duration_secs: Option<f32>,
) {
    if !is_learning_enabled() {
        return;
    }
    if edit_duration_secs.is_some_and(|d| d < MIN_EDIT_DURATION_SECS) {
        return;
    }
    let key = auto_enhance_key(camera_key, scene_key);
    if let Ok(mut guard) = PRIORS_STORE.write() {
        let entry = guard.auto_enhance.entry(key).or_default();
        if let Some(v) = valid_delta(delta.exposure, 0.5) {
            entry.exposure = ema_step(entry.exposure, v, 0.5);
        }
        if let Some(v) = valid_delta(delta.contrast, 15.0) {
            entry.contrast = ema_step(entry.contrast, v, 15.0);
        }
        if let Some(v) = valid_delta(delta.highlights, 15.0) {
            entry.highlights = ema_step(entry.highlights, v, 15.0);
        }
        if let Some(v) = valid_delta(delta.shadows, 15.0) {
            entry.shadows = ema_step(entry.shadows, v, 15.0);
        }
        if let Some(v) = valid_delta(delta.whites, 15.0) {
            entry.whites = ema_step(entry.whites, v, 15.0);
        }
        if let Some(v) = valid_delta(delta.blacks, 15.0) {
            entry.blacks = ema_step(entry.blacks, v, 15.0);
        }
        if let Some(v) = valid_delta(delta.temperature, 15.0) {
            entry.temperature = ema_step(entry.temperature, v, 15.0);
        }
        if let Some(v) = valid_delta(delta.vibrance, 15.0) {
            entry.vibrance = ema_step(entry.vibrance, v, 15.0);
        }
        if let Some(v) = valid_delta(delta.clarity, 15.0) {
            entry.clarity = ema_step(entry.clarity, v, 15.0);
        }
        entry.samples = entry.samples.saturating_add(1);
    }
    schedule_flush();
}

pub fn denoise_key(camera_key: &str, iso: u32) -> String {
    format!("{}|{}", camera_key, iso_bucket(iso))
}

pub fn get_denoise_prior(camera_key: &str, iso: u32) -> Option<DenoisePrior> {
    if !is_learning_enabled() {
        return None;
    }
    let key = denoise_key(camera_key, iso);
    let guard = PRIORS_STORE.read().ok()?;
    let prior = guard.denoise.get(&key)?;
    if prior.samples >= MIN_SAMPLES {
        Some(prior.clone())
    } else {
        None
    }
}

pub fn record_denoise_confirmation(
    camera_key: &str,
    iso: u32,
    ai_denoise_amount: f32,
    ai_denoise_detail: f32,
    luma_nr: f32,
    color_nr: f32,
) {
    if !is_learning_enabled() {
        return;
    }
    let key = denoise_key(camera_key, iso);
    if let Ok(mut guard) = PRIORS_STORE.write() {
        let entry = guard.denoise.entry(key).or_default();
        if ai_denoise_amount.is_finite() {
            entry.ai_denoise_amount = ((1.0 - EMA_ALPHA) * entry.ai_denoise_amount
                + EMA_ALPHA * ai_denoise_amount.clamp(0.0, 100.0))
            .clamp(0.0, 100.0);
        }
        if ai_denoise_detail.is_finite() {
            entry.ai_denoise_detail = ((1.0 - EMA_ALPHA) * entry.ai_denoise_detail
                + EMA_ALPHA * ai_denoise_detail.clamp(0.0, 100.0))
            .clamp(0.0, 100.0);
        }
        if luma_nr.is_finite() {
            entry.luma_noise_reduction = ((1.0 - EMA_ALPHA) * entry.luma_noise_reduction
                + EMA_ALPHA * luma_nr.clamp(0.0, 100.0))
            .clamp(0.0, 100.0);
        }
        if color_nr.is_finite() {
            entry.color_noise_reduction = ((1.0 - EMA_ALPHA) * entry.color_noise_reduction
                + EMA_ALPHA * color_nr.clamp(0.0, 100.0))
            .clamp(0.0, 100.0);
        }
        entry.samples = entry.samples.saturating_add(1);
    }
    schedule_flush();
}

pub fn record_empirical_sensor_noise(camera_key: &str, iso: u32, sigma: f32) {
    if !is_learning_enabled() || !sigma.is_finite() || sigma <= 0.0 || iso < 25 {
        return;
    }
    if let Ok(mut guard) = PRIORS_STORE.write() {
        let fit = guard
            .empirical_sensors
            .entry(camera_key.to_string())
            .or_default();
        if fit.samples.len() >= 32 {
            fit.samples.remove(0);
        }
        fit.samples.push(IsoNoiseSample { iso, sigma });

        let min_iso = fit.samples.iter().map(|s| s.iso).min().unwrap_or(iso);
        let max_iso = fit.samples.iter().map(|s| s.iso).max().unwrap_or(iso);
        if max_iso.saturating_sub(min_iso) >= 100 && fit.samples.len() >= 2 {
            // Simple linear regression on (iso, sigma^2) => var = base^2 + shot * iso
            let n = fit.samples.len() as f64;
            let mut sum_x = 0.0;
            let mut sum_y = 0.0;
            let mut sum_xx = 0.0;
            let mut sum_xy = 0.0;
            for s in &fit.samples {
                let x = s.iso as f64;
                let y = (s.sigma as f64).powi(2);
                sum_x += x;
                sum_y += y;
                sum_xx += x * x;
                sum_xy += x * y;
            }
            let denom = n * sum_xx - sum_x * sum_x;
            if denom.abs() > 1e-9 {
                let slope = ((n * sum_xy - sum_x * sum_y) / denom).max(1e-8);
                let intercept = ((sum_y - slope * sum_x) / n).max(1e-7);
                fit.fitted_shot_noise_coeff = Some((slope.sqrt() as f32).clamp(0.0001, 0.01));
                fit.fitted_base_read_noise = Some((intercept.sqrt() as f32).clamp(0.0002, 0.02));
            }
        }
    }
    schedule_flush();
}

pub fn get_empirical_sensor_fit(camera_key: &str) -> Option<(f32, f32)> {
    if !is_learning_enabled() {
        return None;
    }
    let guard = PRIORS_STORE.read().ok()?;
    let fit = guard.empirical_sensors.get(camera_key)?;
    match (fit.fitted_base_read_noise, fit.fitted_shot_noise_coeff) {
        (Some(read), Some(shot)) => Some((read, shot)),
        _ => None,
    }
}

pub fn quantize_iso_bucket(iso: u32) -> u32 {
    const STOPS: &[u32] = &[
        50, 100, 125, 160, 200, 250, 320, 400, 500, 640, 800, 1000, 1250, 1600, 2000, 2500, 3200,
        4000, 5000, 6400, 8000, 10000, 12800, 25600, 51200, 102400,
    ];
    let iso_c = iso.max(25) as f32;
    let log_iso = iso_c.log2();
    let mut best = STOPS[0];
    let mut best_dist = f32::INFINITY;
    for &stop in STOPS {
        let d = ((stop as f32).log2() - log_iso).abs();
        if d < best_dist {
            best_dist = d;
            best = stop;
        }
    }
    best
}

pub fn record_iso_noise_bucket(
    camera_key: &str,
    iso: u32,
    read_noise: f32,
    shot_noise: f32,
    chroma_ratio: f32,
) {
    if !is_learning_enabled()
        || iso < 25
        || !read_noise.is_finite()
        || !shot_noise.is_finite()
        || !chroma_ratio.is_finite()
        || read_noise <= 0.0
    {
        return;
    }
    let bucket_iso = quantize_iso_bucket(iso);
    let r = read_noise.clamp(0.0001, 0.08);
    let s = shot_noise.max(0.0).clamp(0.0, 0.05);
    let c = chroma_ratio.clamp(0.25, 3.0);

    if let Ok(mut guard) = PRIORS_STORE.write() {
        let list = guard
            .iso_noise_buckets
            .entry(camera_key.to_string())
            .or_default();
        if let Some(existing) = list.iter_mut().find(|b| b.iso_bucket == bucket_iso) {
            let w = (1.0 / ((existing.samples + 1).min(8) as f32)).clamp(0.12, 1.0);
            existing.read_noise = (1.0 - w) * existing.read_noise + w * r;
            existing.shot_noise = (1.0 - w) * existing.shot_noise + w * s;
            existing.chroma_ratio = (1.0 - w) * existing.chroma_ratio + w * c;
            existing.samples = existing.samples.saturating_add(1);
        } else {
            list.push(IsoNoiseProfileBucket {
                iso_bucket: bucket_iso,
                samples: 1,
                read_noise: r,
                shot_noise: s,
                chroma_ratio: c,
            });
            list.sort_by_key(|b| b.iso_bucket);
        }
    }
    schedule_flush();
}

pub fn interpolate_iso_buckets(
    buckets: &[IsoNoiseProfileBucket],
    iso: u32,
) -> Option<IsoNoiseProfileBucket> {
    if buckets.is_empty() {
        return None;
    }
    let mut sorted = buckets.to_vec();
    sorted.sort_by_key(|b| b.iso_bucket);

    // Preserve exact stops cleanly so dual-native-ISO cliffs never blur into adjacent stops
    if let Some(exact) = sorted.iter().find(|b| b.iso_bucket == iso) {
        return Some(exact.clone());
    }
    if iso <= sorted[0].iso_bucket {
        return Some(sorted[0].clone());
    }
    let last = sorted.len() - 1;
    if iso >= sorted[last].iso_bucket {
        return Some(sorted[last].clone());
    }

    for pair in sorted.windows(2) {
        let lo = &pair[0];
        let hi = &pair[1];
        if iso > lo.iso_bucket && iso < hi.iso_bucket {
            let l0 = (lo.iso_bucket.max(1) as f32).log2();
            let l1 = (hi.iso_bucket.max(1) as f32).log2();
            let lx = (iso.max(1) as f32).log2();
            let t = ((lx - l0) / (l1 - l0).max(1e-5)).clamp(0.0, 1.0);
            return Some(IsoNoiseProfileBucket {
                iso_bucket: iso,
                samples: lo.samples.min(hi.samples),
                read_noise: lo.read_noise + (hi.read_noise - lo.read_noise) * t,
                shot_noise: lo.shot_noise + (hi.shot_noise - lo.shot_noise) * t,
                chroma_ratio: lo.chroma_ratio + (hi.chroma_ratio - lo.chroma_ratio) * t,
            });
        }
    }
    Some(sorted[last].clone())
}

pub fn lookup_iso_noise_profile(camera_key: &str, iso: u32) -> Option<IsoNoiseProfileBucket> {
    if !is_learning_enabled() {
        return None;
    }
    let guard = PRIORS_STORE.read().ok()?;
    let buckets = guard.iso_noise_buckets.get(camera_key)?;
    interpolate_iso_buckets(buckets, iso)
}

pub fn record_dust_spot_confirmation(camera_key: &str, norm_x: f32, norm_y: f32, radius: f32, source_tag: &str) {
    if !is_learning_enabled()
        || !norm_x.is_finite()
        || !norm_y.is_finite()
        || !(0.0..=1.0).contains(&norm_x)
        || !(0.0..=1.0).contains(&norm_y)
    {
        return;
    }
    if let Ok(mut guard) = PRIORS_STORE.write() {
        let spots = guard.sensor_dust.entry(camera_key.to_string()).or_default();
        if let Some(existing) = spots.iter_mut().find(|s| {
            (s.norm_x - norm_x).hypot(s.norm_y - norm_y) <= DUST_CONFIRM_RADIUS
        }) {
            existing.norm_x = 0.5 * existing.norm_x + 0.5 * norm_x;
            existing.norm_y = 0.5 * existing.norm_y + 0.5 * norm_y;
            existing.radius = 0.5 * existing.radius + 0.5 * radius.clamp(1.0, 64.0);
            if !source_tag.is_empty() && !existing.seen_sources.iter().any(|s| s == source_tag) {
                if existing.seen_sources.len() < 16 {
                    existing.seen_sources.push(source_tag.to_string());
                }
                existing.confirmed_count = existing.confirmed_count.saturating_add(1);
            } else if source_tag.is_empty() {
                existing.confirmed_count = existing.confirmed_count.saturating_add(1);
            }
        } else if spots.len() < 256 {
            let mut seen_sources = Vec::new();
            if !source_tag.is_empty() {
                seen_sources.push(source_tag.to_string());
            }
            spots.push(DustSpotEntry {
                norm_x,
                norm_y,
                radius: radius.clamp(1.0, 64.0),
                confirmed_count: 1,
                rejected_count: 0,
                seen_sources,
            });
        }
    }
    schedule_flush();
}

pub fn is_dust_spot_suppressed(camera_key: &str, norm_x: f32, norm_y: f32) -> bool {
    if !is_learning_enabled() {
        return false;
    }
    let Ok(guard) = PRIORS_STORE.read() else {
        return false;
    };
    let Some(spots) = guard.sensor_dust.get(camera_key) else {
        return false;
    };
    spots.iter().any(|s| {
        (s.norm_x - norm_x).hypot(s.norm_y - norm_y) <= DUST_SUPPRESS_RADIUS
            && (s.rejected_count > s.confirmed_count || (s.rejected_count >= 1 && s.confirmed_count == 0))
    })
}

pub fn persistent_dust_candidates(camera_key: &str) -> Vec<DustSpotEntry> {
    if !is_learning_enabled() {
        return Vec::new();
    }
    let Ok(guard) = PRIORS_STORE.read() else {
        return Vec::new();
    };
    guard
        .sensor_dust
        .get(camera_key)
        .map(|spots| {
            spots
                .iter()
                .filter(|s| s.confirmed_count >= 3 && s.confirmed_count > s.rejected_count)
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

pub fn record_dust_rejection_internal(camera_key: &str, norm_x: f32, norm_y: f32) {
    if !is_learning_enabled()
        || !norm_x.is_finite()
        || !norm_y.is_finite()
        || !(0.0..=1.0).contains(&norm_x)
        || !(0.0..=1.0).contains(&norm_y)
    {
        return;
    }
    if let Ok(mut guard) = PRIORS_STORE.write() {
        let spots = guard.sensor_dust.entry(camera_key.to_string()).or_default();
        if let Some(existing) = spots.iter_mut().find(|s| {
            (s.norm_x - norm_x).hypot(s.norm_y - norm_y) <= DUST_SUPPRESS_RADIUS
        }) {
            existing.rejected_count = existing.rejected_count.saturating_add(1);
        } else if spots.len() < 256 {
            spots.push(DustSpotEntry {
                norm_x,
                norm_y,
                radius: 4.0,
                confirmed_count: 0,
                rejected_count: 1,
                seen_sources: Vec::new(),
            });
        }
    }
    schedule_flush();
}

pub fn culler_key(camera_key: &str, lens_key: &str) -> String {
    format!("{}|{}", camera_key, lens_key)
}

pub fn record_culler_flag(camera_key: &str, lens_key: &str, is_pick: bool, sharpness_score: f32) {
    if !is_learning_enabled() || !sharpness_score.is_finite() {
        return;
    }
    let clamped = sharpness_score.clamp(1.0, 100.0);
    let key = culler_key(camera_key, lens_key);
    if let Ok(mut guard) = PRIORS_STORE.write() {
        let entry = guard.culler_sharpness.entry(key).or_default();
        if is_pick {
            entry.pick_count = entry.pick_count.saturating_add(1);
            entry.pick_sum += clamped;
        } else {
            entry.reject_count = entry.reject_count.saturating_add(1);
            entry.reject_sum += clamped;
        }
    }
    schedule_flush();
}

pub fn get_culler_sharpness_cutoff(camera_key: &str, lens_key: &str) -> Option<f32> {
    if !is_learning_enabled() {
        return None;
    }
    let guard = PRIORS_STORE.read().ok()?;
    let entry = guard.culler_sharpness.get(&culler_key(camera_key, lens_key))?;
    let total = entry.pick_count + entry.reject_count;
    if total < MIN_SAMPLES {
        return None;
    }
    let pick_mean = if entry.pick_count > 0 {
        entry.pick_sum / entry.pick_count as f32
    } else {
        30.0
    };
    let reject_mean = if entry.reject_count > 0 {
        entry.reject_sum / entry.reject_count as f32
    } else {
        (pick_mean * 0.6).max(5.0)
    };
    let cutoff = 0.5 * (pick_mean + reject_mean);
    Some(cutoff.clamp(5.0, 80.0))
}

pub fn record_hdr_preference(scene_key: &str, intent: &str, strength: f32) {
    if !is_learning_enabled() || !strength.is_finite() {
        return;
    }
    if let Ok(mut guard) = PRIORS_STORE.write() {
        let entry = guard
            .hdr_preferences
            .entry(scene_key.trim().to_lowercase())
            .or_default();
        *entry.intent_counts.entry(intent.to_string()).or_insert(0) += 1;
        entry.ema_strength = ((1.0 - EMA_ALPHA) * entry.ema_strength
            + EMA_ALPHA * strength.clamp(0.0, 2.0))
        .clamp(0.0, 2.0);
        entry.samples = entry.samples.saturating_add(1);
    }
    schedule_flush();
}

pub fn record_film_stock_usage(scene_key: &str, preset_id: &str) {
    if !is_learning_enabled() || preset_id.trim().is_empty() {
        return;
    }
    if let Ok(mut guard) = PRIORS_STORE.write() {
        let map = guard
            .film_stock_rankings
            .entry(scene_key.trim().to_lowercase())
            .or_default();
        *map.entry(preset_id.to_string()).or_insert(0) += 1;
    }
    schedule_flush();
}

pub fn record_negative_mask_confirmation(
    base_mask_r: f32,
    base_mask_g: f32,
    base_mask_b: f32,
    film_profile: &str,
) {
    if !is_learning_enabled()
        || !base_mask_r.is_finite()
        || !base_mask_g.is_finite()
        || !base_mask_b.is_finite()
    {
        return;
    }
    if let Ok(mut guard) = PRIORS_STORE.write() {
        let prev = guard.negative_defaults.get_or_insert_with(Default::default);
        prev.base_mask_r = base_mask_r.clamp(0.01, 1.0);
        prev.base_mask_g = base_mask_g.clamp(0.01, 1.0);
        prev.base_mask_b = base_mask_b.clamp(0.01, 1.0);
        prev.film_profile = film_profile.to_string();
        prev.confirmed_count = prev.confirmed_count.saturating_add(1);
    }
    schedule_flush();
}

pub fn summary_snapshot() -> LocalPriorsSummary {
    let enabled = is_learning_enabled();
    let Ok(guard) = PRIORS_STORE.read() else {
        return LocalPriorsSummary {
            enabled,
            total_samples: 0,
            auto_enhance_profiles: 0,
            auto_enhance_samples: 0,
            denoise_profiles: 0,
            denoise_samples: 0,
            dust_spots_tracked: 0,
            culler_profiles: 0,
            negative_confirmed: 0,
            camera_keys: Vec::new(),
        };
    };

    let auto_enhance_samples: u32 = guard.auto_enhance.values().map(|p| p.samples).sum();
    let denoise_samples: u32 = guard.denoise.values().map(|p| p.samples).sum();
    let dust_spots_tracked: usize = guard.sensor_dust.values().map(|v| v.len()).sum();
    let culler_samples: u32 = guard
        .culler_sharpness
        .values()
        .map(|c| c.pick_count + c.reject_count)
        .sum();
    let hdr_samples: u32 = guard.hdr_preferences.values().map(|h| h.samples).sum();
    let negative_confirmed = guard
        .negative_defaults
        .as_ref()
        .map(|n| n.confirmed_count)
        .unwrap_or(0);

    let mut cams = HashSet::new();
    for k in guard.auto_enhance.keys() {
        if let Some((cam, _)) = k.split_once('|') {
            cams.insert(cam.to_string());
        }
    }
    for k in guard.denoise.keys() {
        if let Some((cam, _)) = k.split_once('|') {
            cams.insert(cam.to_string());
        }
    }
    for k in guard.sensor_dust.keys() {
        cams.insert(k.clone());
    }
    let mut camera_keys: Vec<String> = cams.into_iter().collect();
    camera_keys.sort();

    LocalPriorsSummary {
        enabled,
        total_samples: auto_enhance_samples
            + denoise_samples
            + culler_samples
            + hdr_samples
            + negative_confirmed
            + dust_spots_tracked as u32,
        auto_enhance_profiles: guard.auto_enhance.len(),
        auto_enhance_samples,
        denoise_profiles: guard.denoise.len(),
        denoise_samples,
        dust_spots_tracked,
        culler_profiles: guard.culler_sharpness.len(),
        negative_confirmed,
        camera_keys,
    }
}

pub fn reset_priors_internal(category: Option<&str>, camera_key: Option<&str>) {
    if let Ok(mut guard) = PRIORS_STORE.write() {
        match (category, camera_key) {
            (None, None) => *guard = LocalPriors::default(),
            (Some("auto_enhance"), Some(cam)) => {
                guard.auto_enhance.retain(|k, _| !k.starts_with(&format!("{}|", cam)));
            }
            (Some("auto_enhance"), None) => guard.auto_enhance.clear(),
            (Some("denoise"), Some(cam)) => {
                guard.denoise.retain(|k, _| !k.starts_with(&format!("{}|", cam)));
                guard.empirical_sensors.remove(cam);
                guard.iso_noise_buckets.remove(cam);
            }
            (Some("denoise"), None) => {
                guard.denoise.clear();
                guard.empirical_sensors.clear();
                guard.iso_noise_buckets.clear();
            }
            (Some("sensor_dust"), Some(cam)) => {
                guard.sensor_dust.remove(cam);
            }
            (Some("sensor_dust"), None) => guard.sensor_dust.clear(),
            (Some("culler"), _) => guard.culler_sharpness.clear(),
            (Some("preferences"), _) => {
                guard.hdr_preferences.clear();
                guard.film_stock_rankings.clear();
                guard.negative_defaults = None;
            }
            _ => *guard = LocalPriors::default(),
        }
    }
    flush_now();
}

#[tauri::command]
pub fn get_local_priors_summary(app_handle: AppHandle) -> LocalPriorsSummary {
    let settings = crate::app::settings::load_settings(app_handle).unwrap_or_default();
    set_learning_enabled(settings.enable_local_adaptive_learning.unwrap_or(true));
    summary_snapshot()
}

#[tauri::command]
pub fn reset_local_priors(category: Option<String>, camera_key: Option<String>) -> LocalPriorsSummary {
    reset_priors_internal(category.as_deref(), camera_key.as_deref());
    summary_snapshot()
}

#[tauri::command]
pub fn record_dust_spot_rejection(
    camera_key: Option<String>,
    path: Option<String>,
    norm_x: f32,
    norm_y: f32,
) {
    let cam = if let Some(k) = camera_key.filter(|s| !s.trim().is_empty()) {
        k
    } else if let Some(p) = path.as_deref()
        && let Ok(bytes) = fs::read(p)
        && let Some((make, model)) = crate::metadata::exif_processing::read_camera_make_model(p, &bytes)
    {
        normalize_camera_key(&make, &model)
    } else {
        "universal".to_string()
    };
    record_dust_rejection_internal(&cam, norm_x, norm_y);
}

#[tauri::command]
pub fn record_auto_edit_rejection(
    path: Option<String>,
    scene: Option<String>,
    app_handle: AppHandle,
) {
    if !is_learning_enabled() {
        return;
    }
    let camera_key = if let Some(p) = path.as_deref() {
        let (source_path, _) = crate::library::sidecars::parse_virtual_path(p);
        let s = source_path.to_string_lossy().to_string();
        if let Ok(bytes) = fs::read(&source_path)
            && let Some((make, model)) = crate::metadata::exif_processing::read_camera_make_model(&s, &bytes)
        {
            normalize_camera_key(&make, &model)
        } else {
            let state = app_handle.state::<crate::app::state::AppState>();
            let path_match = state.original_image.lock().ok().and_then(|g| g.as_ref().map(|l| l.path.clone()));
            let _ = path_match;
            "universal".to_string()
        }
    } else {
        "universal".to_string()
    };
    let scene_key = scene.unwrap_or_else(|| "general".to_string());
    let key = auto_enhance_key(&camera_key, &scene_key);
    if let Ok(mut guard) = PRIORS_STORE.write() {
        let entry = guard.auto_enhance.entry(key).or_default();
        entry.rejections = entry.rejections.saturating_add(1);
        entry.exposure *= 0.5;
        entry.contrast *= 0.5;
        entry.highlights *= 0.5;
        entry.shadows *= 0.5;
        entry.whites *= 0.5;
        entry.blacks *= 0.5;
        entry.temperature *= 0.5;
        entry.vibrance *= 0.5;
        entry.clarity *= 0.5;
    }
    schedule_flush();
}

pub fn record_confirmed_photo_edits(source_path: &std::path::Path, adjustments: &serde_json::Value) {
    if !is_learning_enabled() {
        return;
    }
    let Ok(meta) = fs::metadata(source_path) else {
        return;
    };
    if meta.len() == 0 {
        return;
    }
    let path_str = source_path.to_string_lossy().to_string();
    let Ok(bytes) = fs::read(source_path) else {
        return;
    };
    if bytes.is_empty() {
        return;
    }
    let (make, model) = crate::metadata::exif_processing::read_camera_make_model(&path_str, &bytes)
        .unwrap_or_default();
    let cam_key = normalize_camera_key(&make, &model);

    // 1. Target A: Auto-Enhance prior confirmation (only if tone adjustments were meaningfully applied)
    let exp = adjustments.get("exposure").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let con = adjustments.get("contrast").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let hl = adjustments.get("highlights").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let sh = adjustments.get("shadows").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let wh = adjustments.get("whites").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let bl = adjustments.get("blacks").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let temp = adjustments.get("temperature").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let vib = adjustments.get("vibrance").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let cl = adjustments.get("clarity").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

    if adjustments.get("activeTools").and_then(|t| t.get("basic")).and_then(|b| b.as_bool()).unwrap_or(false)
        || exp.abs() > 0.05
        || con.abs() > 2.0
    {
        record_auto_enhance_confirmation(
            &cam_key,
            "general",
            AutoEnhanceDelta {
                exposure: exp.clamp(-0.5, 0.5),
                contrast: con.clamp(-15.0, 15.0),
                highlights: hl.clamp(-15.0, 15.0),
                shadows: sh.clamp(-15.0, 15.0),
                whites: wh.clamp(-15.0, 15.0),
                blacks: bl.clamp(-15.0, 15.0),
                temperature: temp.clamp(-15.0, 15.0),
                vibrance: vib.clamp(-15.0, 15.0),
                clarity: cl.clamp(-15.0, 15.0),
            },
            None,
        );
    }

    // 2. Target B: Denoise prior confirmation
    if adjustments
        .get("aiDenoiseEnabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let iso = crate::metadata::exif_processing::read_iso(&path_str, &bytes).unwrap_or(400);
        let amt = adjustments.get("aiDenoiseAmount").and_then(|v| v.as_f64()).unwrap_or(100.0) as f32;
        let det = adjustments.get("aiDenoiseDetail").and_then(|v| v.as_f64()).unwrap_or(15.0) as f32;
        let luma = adjustments.get("lumaNoiseReduction").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let col = adjustments.get("colorNoiseReduction").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        record_denoise_confirmation(&cam_key, iso, amt, det, luma, col);
    }

    // 3. Target C: Sensor dust spot confirmation from retained aiPatches
    if let Some(patches) = adjustments.get("aiPatches").and_then(|v| v.as_array()) {
        for patch in patches {
            let nx = patch.get("x").and_then(|v| v.as_f64()).map(|v| v as f32);
            let ny = patch.get("y").and_then(|v| v.as_f64()).map(|v| v as f32);
            let rad = patch.get("radius").and_then(|v| v.as_f64()).unwrap_or(4.0) as f32;
            if let (Some(nx), Some(ny)) = (nx, ny) {
                record_dust_spot_confirmation(&cam_key, nx, ny, rad, &path_str);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_min_samples_and_bounded_clamping_and_disable_reset() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_priors_internal(None, None);
        set_learning_enabled(true);
        let cam = "canon_eos_77d";
        let scene = "landscape";

        // Below min_samples (4 samples): must return None
        for _ in 0..4 {
            record_auto_enhance_confirmation(
                cam,
                scene,
                AutoEnhanceDelta {
                    exposure: 0.3,
                    contrast: 10.0,
                    ..Default::default()
                },
                Some(12.0),
            );
        }
        assert!(get_auto_enhance_prior(cam, scene).is_none());

        // Fast-browsing < 5s edits must be ignored
        record_auto_enhance_confirmation(
            cam,
            scene,
            AutoEnhanceDelta {
                exposure: 0.4,
                contrast: 12.0,
                ..Default::default()
            },
            Some(1.5),
        );
        assert!(get_auto_enhance_prior(cam, scene).is_none());

        // Adversarial outlier (+100/-100) must be ignored by outlier guard; valid 5th sample activates prior
        record_auto_enhance_confirmation(
            cam,
            scene,
            AutoEnhanceDelta {
                exposure: 100.0,
                contrast: -100.0,
                ..Default::default()
            },
            Some(10.0),
        );
        let active = get_auto_enhance_prior(cam, scene).expect("should activate on 5th confirm");
        assert!(active.exposure.abs() <= 0.5);
        assert!(active.contrast.abs() <= 15.0);

        // Disabling immediately bypasses prior
        set_learning_enabled(false);
        assert!(get_auto_enhance_prior(cam, scene).is_none());
        set_learning_enabled(true);

        // Reset clears prior
        reset_priors_internal(None, None);
        assert!(get_auto_enhance_prior(cam, scene).is_none());
    }

    #[test]
    fn test_sensor_dust_positive_and_negative_feedback() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_priors_internal(None, None);
        set_learning_enabled(true);
        let cam = "sony_a7iv";

        record_dust_spot_confirmation(cam, 0.25, 0.50, 5.0, "IMG_001.ARW");
        record_dust_spot_confirmation(cam, 0.251, 0.501, 5.0, "IMG_002.ARW");
        assert!(persistent_dust_candidates(cam).is_empty());
        record_dust_spot_confirmation(cam, 0.2505, 0.5005, 5.0, "IMG_003.ARW");
        assert_eq!(persistent_dust_candidates(cam).len(), 1);

        // Rejecting another coordinate suppresses within 0.012 radius
        record_dust_rejection_internal(cam, 0.80, 0.80);
        assert!(is_dust_spot_suppressed(cam, 0.805, 0.804));
        assert!(!is_dust_spot_suppressed(cam, 0.85, 0.85));
        reset_priors_internal(None, None);
    }

    #[test]
    fn test_culler_sharpness_cutoff_and_denoise_empirical_fit() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_priors_internal(None, None);
        set_learning_enabled(true);
        let cam = "nikon_z6";
        let lens = "50mm_1.8";

        for _ in 0..3 {
            record_culler_flag(cam, lens, true, 48.0);
            record_culler_flag(cam, lens, false, 12.0);
        }
        let cutoff = get_culler_sharpness_cutoff(cam, lens).expect("cutoff ready after 6 flags");
        assert!((cutoff - 30.0).abs() < 1.0, "Expected midpoint ~30, got {}", cutoff);

        record_empirical_sensor_noise(cam, 100, 0.0020);
        record_empirical_sensor_noise(cam, 3200, 0.0120);
        let (base_read, shot) = get_empirical_sensor_fit(cam).expect("empirical fit ready");
        assert!(base_read > 0.0 && shot > 0.0);
        reset_priors_internal(None, None);
    }

    #[test]
    fn test_no_calibration_vault_regression() {
        let tmp = std::env::temp_dir().join(format!("rr_vault_check_{}", std::process::id()));
        let _ = fs::create_dir_all(&tmp);
        let vault_path = tmp.join("calibration_vault.json");
        fs::write(&vault_path, b"{\"legacy\":true}").unwrap();
        assert!(vault_path.exists());
        crate::enhanced::auto_tune_loop::init_calibration_vault_path(tmp.clone());
        assert!(!vault_path.exists(), "calibration_vault.json must always be purged on init");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_iso_noise_bucket_log_interpolation_preserves_dual_gain_step() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_priors_internal(None, None);
        set_learning_enabled(true);
        let cam = "sony_a7siii_dcg";

        // Dual-native-ISO camera: ISO 400 has higher read noise before the second gain circuit engages at ISO 800
        record_iso_noise_bucket(cam, 100, 0.0020, 0.0003, 1.0);
        record_iso_noise_bucket(cam, 400, 0.0065, 0.0008, 1.1);
        record_iso_noise_bucket(cam, 800, 0.0018, 0.0004, 1.0);
        record_iso_noise_bucket(cam, 3200, 0.0052, 0.0012, 1.3);

        // Exact stops must preserve dual-native-ISO discontinuity without blurring across the step
        let at_400 = lookup_iso_noise_profile(cam, 400).expect("ISO 400 bucket");
        let at_800 = lookup_iso_noise_profile(cam, 800).expect("ISO 800 bucket");
        assert!(
            (at_400.read_noise - 0.0065).abs() < 1e-5,
            "ISO 400 exact bucket must stay 0.0065, got {}",
            at_400.read_noise
        );
        assert!(
            (at_800.read_noise - 0.0018).abs() < 1e-5,
            "ISO 800 DCG drop must stay 0.0018, got {}",
            at_800.read_noise
        );

        // Log2 midpoint between 100 and 400 is ISO 200 -> exact arithmetic midpoint of log stops
        let at_200 = lookup_iso_noise_profile(cam, 200).expect("ISO 200 interpolated");
        assert!(
            (at_200.read_noise - 0.00425).abs() < 1e-4,
            "ISO 200 log2-midpoint should be ~0.00425, got {}",
            at_200.read_noise
        );

        // Log2 midpoint between 800 and 3200 is ISO 1600
        let at_1600 = lookup_iso_noise_profile(cam, 1600).expect("ISO 1600 interpolated");
        assert!(
            (at_1600.read_noise - 0.0035).abs() < 1e-4,
            "ISO 1600 log2-midpoint should be ~0.0035, got {}",
            at_1600.read_noise
        );

        reset_priors_internal(Some("denoise"), Some(cam));
        assert!(lookup_iso_noise_profile(cam, 800).is_none());
    }
}
