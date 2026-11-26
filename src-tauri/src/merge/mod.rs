pub mod align;
pub mod blending;
pub mod dng;
pub mod features;
pub mod focus;
pub mod hdr;
pub mod homography;
pub mod matching;
pub mod output;
pub mod panorama;
pub mod raster;
pub mod source;

use crate::app::memory_budget::MemoryBudget;
use crate::app::settings::{AppSettings, load_settings};
use crate::app::state::AppState;
use crate::library::sidecars::parse_virtual_path;
use base64::{Engine as _, engine::general_purpose};
use output::{MergeOutput, MergePreview, save_output, write_output};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use source::{SourceLoader, source_paths};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeBrushStroke {
    pub points: Vec<[f32; 2]>,
    pub radius_norm: f32,
    #[serde(default)]
    pub frame_index: Option<usize>,
    #[serde(default)]
    pub seam_role: Option<String>,
}

pub(crate) fn stroke_mask_hash(strokes: &[MergeBrushStroke]) -> u64 {
    let mut hasher = DefaultHasher::new();
    strokes.len().hash(&mut hasher);
    for s in strokes {
        s.points.len().hash(&mut hasher);
        for &[nx, ny] in &s.points {
            (nx * 10_000.0).round().to_bits().hash(&mut hasher);
            (ny * 10_000.0).round().to_bits().hash(&mut hasher);
        }
        (s.radius_norm * 10_000.0).round().to_bits().hash(&mut hasher);
        s.frame_index.hash(&mut hasher);
        s.seam_role.hash(&mut hasher);
    }
    hasher.finish()
}

fn dist_sq_point_segment(px: f32, py: f32, a: [f32; 2], b: [f32; 2]) -> f32 {
    let vx = b[0] - a[0];
    let vy = b[1] - a[1];
    let len2 = vx * vx + vy * vy;
    if len2 <= 1e-6 {
        let dx = px - a[0];
        let dy = py - a[1];
        return dx * dx + dy * dy;
    }
    let t = (((px - a[0]) * vx + (py - a[1]) * vy) / len2).clamp(0.0, 1.0);
    let cx = a[0] + t * vx;
    let cy = a[1] + t * vy;
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy
}

/// Rasterizes normalized `[0..1]` preview-space strokes matching `filter` onto `(full_w, full_h)`
/// mapped through `crop_rect`. Returns feathered mask values in `[0.0, 1.0]`.
pub(crate) fn rasterize_strokes_for_rect(
    strokes: &[MergeBrushStroke],
    full_w: usize,
    full_h: usize,
    crop_rect: &raster::Rect,
    filter: impl Fn(&MergeBrushStroke) -> bool,
) -> Vec<f32> {
    let mut out = vec![0.0f32; full_w * full_h];
    if full_w == 0 || full_h == 0 || crop_rect.width == 0 || crop_rect.height == 0 {
        return out;
    }
    let ref_dim = crop_rect.width.min(crop_rect.height) as f32;
    let cw = crop_rect.width as f32;
    let ch = crop_rect.height as f32;
    let ox = crop_rect.x as f32;
    let oy = crop_rect.y as f32;

    for stroke in strokes {
        if !filter(stroke) || stroke.points.is_empty() {
            continue;
        }
        let r = (stroke.radius_norm * ref_dim).max(2.0);
        let r2 = r * r;
        let inner = r * 0.55;
        let pts: Vec<[f32; 2]> = stroke
            .points
            .iter()
            .map(|&[nx, ny]| [ox + nx.clamp(0.0, 1.0) * cw, oy + ny.clamp(0.0, 1.0) * ch])
            .collect();

        let (mut min_x, mut max_x) = (f32::INFINITY, f32::NEG_INFINITY);
        let (mut min_y, mut max_y) = (f32::INFINITY, f32::NEG_INFINITY);
        for &[px, py] in &pts {
            min_x = min_x.min(px);
            max_x = max_x.max(px);
            min_y = min_y.min(py);
            max_y = max_y.max(py);
        }
        let pad = r.ceil();
        let x0 = ((min_x - pad).floor() as isize).max(0) as usize;
        let x1 = ((max_x + pad).ceil() as isize).min(full_w as isize - 1).max(0) as usize;
        let y0 = ((min_y - pad).floor() as isize).max(0) as usize;
        let y1 = ((max_y + pad).ceil() as isize).min(full_h as isize - 1).max(0) as usize;
        if x0 > x1 || y0 > y1 {
            continue;
        }

        for y in y0..=y1 {
            let py = y as f32 + 0.5;
            let row = y * full_w;
            for x in x0..=x1 {
                let px = x as f32 + 0.5;
                let mut best_d2 = f32::INFINITY;
                if pts.len() == 1 {
                    best_d2 = dist_sq_point_segment(px, py, pts[0], pts[0]);
                } else {
                    for seg in pts.windows(2) {
                        best_d2 = best_d2.min(dist_sq_point_segment(px, py, seg[0], seg[1]));
                    }
                }
                if best_d2 < r2 {
                    let d = best_d2.sqrt();
                    let t = ((d - inner) / (r - inner).max(1e-3)).clamp(0.0, 1.0);
                    let w = 1.0 - t * t * (3.0 - 2.0 * t);
                    let slot = &mut out[row + x];
                    if w > *slot {
                        *slot = w;
                    }
                }
            }
        }
    }
    out
}

pub trait MergeSession: Send {
    fn preview(&mut self, settings: &Value) -> Result<MergePreview, String>;
    fn render(&mut self, settings: &Value, context: &MergeContext) -> Result<MergeOutput, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeKind {
    Panorama,
    Hdr,
    FocusStack,
}

pub struct MergeContext<'a> {
    pub settings: &'a AppSettings,
    pub memory_budget: &'a Arc<MemoryBudget>,
    pub app_handle: &'a AppHandle,
    pub progress: &'a (dyn Fn(String) + Sync),
    pub is_cancelled: &'a (dyn Fn() -> bool + Sync),
}

impl MergeContext<'_> {
    pub fn loader(&self, linear_raw: bool) -> SourceLoader<'_> {
        self.loader_with_options(linear_raw, false, true)
    }

    pub fn loader_with_half_size(&self, linear_raw: bool, half_size: bool) -> SourceLoader<'_> {
        self.loader_with_options(linear_raw, half_size, true)
    }

    pub fn loader_with_options(
        &self,
        linear_raw: bool,
        half_size: bool,
        apply_edits: bool,
    ) -> SourceLoader<'_> {
        SourceLoader::new(
            self.settings,
            self.memory_budget,
            self.app_handle,
            linear_raw,
            half_size,
            apply_edits,
        )
    }

    pub fn loader_for_hdr_brackets(
        &self,
        linear_raw: bool,
        half_size: bool,
        apply_edits: bool,
    ) -> SourceLoader<'_> {
        self.loader_with_options(linear_raw, half_size, apply_edits)
            .with_hdr_linear(linear_raw)
    }

    pub fn report(&self, message: impl Into<String>) {
        let message = message.into();
        log::info!("merge: {}", message);
        (self.progress)(message);
    }

    pub fn check_cancelled(&self) -> Result<(), String> {
        if (self.is_cancelled)() {
            Err("The merge was cancelled.".to_string())
        } else {
            Ok(())
        }
    }
}

pub fn parse_settings<T: DeserializeOwned>(settings: &Value) -> Result<T, String> {
    serde_json::from_value(settings.clone()).map_err(|e| format!("Invalid merge settings: {}", e))
}

fn build_session(
    kind: MergeKind,
    paths: &[String],
    half_size: bool,
    settings: &Value,
    context: &MergeContext,
) -> Result<Box<dyn MergeSession>, String> {
    let apply_edits = settings
        .get("applyEdits")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    Ok(match kind {
        MergeKind::Panorama => Box::new(panorama::build_session(paths, half_size, settings, context)?),
        MergeKind::Hdr => Box::new(hdr::build_session(paths, half_size, apply_edits, context)?),
        MergeKind::FocusStack => Box::new(focus::build_session(paths, half_size, context)?),
    })
}

#[tauri::command]
pub async fn start_merge(
    kind: MergeKind,
    paths: Vec<String>,
    settings: Value,
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<MergePreview, String> {
    if paths.len() < 2 {
        return Err("Please select at least two images.".to_string());
    }
    let generation = state.merge_generation.fetch_add(1, Ordering::SeqCst) + 1;
    *state.merge_session.lock().unwrap() = None;
    let slot = state.merge_session.clone();
    let current_generation = state.merge_generation.clone();
    let memory_budget = state.memory_budget.clone();
    let paths = source_paths(&paths);
    let half_size = settings
        .get("halfSize")
        .or_else(|| settings.get("fastDraft"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    tokio::task::spawn_blocking(move || {
        let app_settings = load_settings(app_handle.clone()).unwrap_or_default();
        let progress = |message: String| {
            let _ = app_handle.emit("merge-progress", message);
        };
        let is_cancelled = || current_generation.load(Ordering::SeqCst) != generation;
        let context = MergeContext {
            settings: &app_settings,
            memory_budget: &memory_budget,
            app_handle: &app_handle,
            progress: &progress,
            is_cancelled: &is_cancelled,
        };
        let start = std::time::Instant::now();
        let mut session = build_session(kind, &paths, half_size, &settings, &context)?;
        context.check_cancelled()?;
        context.report("Creating preview...");
        let preview = session.preview(&settings)?;
        context.check_cancelled()?;
        log::info!(
            "{:?} merge of {} images took {:.2?}",
            kind,
            paths.len(),
            start.elapsed()
        );
        let mut guard = slot.lock().unwrap();
        if current_generation.load(Ordering::SeqCst) != generation {
            return Err("The merge was cancelled.".to_string());
        }
        *guard = Some(session);
        Ok(preview)
    })
    .await
    .map_err(|e| format!("Merge task failed: {}", e))?
}

#[tauri::command]
pub async fn update_merge_preview(
    settings: Value,
    state: tauri::State<'_, AppState>,
) -> Result<MergePreview, String> {
    let slot = state.merge_session.clone();
    tokio::task::spawn_blocking(move || {
        let mut guard = slot.lock().unwrap();
        let session = guard
            .as_mut()
            .ok_or("The merge is no longer available. Please run it again.")?;
        session.preview(&settings)
    })
    .await
    .map_err(|e| format!("Merge preview task failed: {}", e))?
}

#[tauri::command]
pub async fn save_merge(
    settings: Value,
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let slot = state.merge_session.clone();
    let current_generation = state.merge_generation.clone();
    let generation = current_generation.load(Ordering::SeqCst);
    let memory_budget = state.memory_budget.clone();
    let mut session = slot.lock().unwrap().take().ok_or_else(|| {
        "No merged image found in memory to save. It might have already been saved.".to_string()
    })?;

    tokio::task::spawn_blocking(move || {
        let app_settings = load_settings(app_handle.clone()).unwrap_or_default();
        let progress = |message: String| {
            let _ = app_handle.emit("merge-progress", message);
        };
        let is_cancelled = || current_generation.load(Ordering::SeqCst) != generation;
        let context = MergeContext {
            settings: &app_settings,
            memory_budget: &memory_budget,
            app_handle: &app_handle,
            progress: &progress,
            is_cancelled: &is_cancelled,
        };
        let start = std::time::Instant::now();
        let format = output::MergeOutputFormat::from_settings(&settings);
        match session.render(&settings, &context) {
            Ok(output) => {
                context.report("Saving...");
                match save_output(output, format) {
                    Ok(path) => {
                        drop(session);
                        log::info!("Full resolution merge saved in {:.2?}", start.elapsed());
                        Ok(path)
                    }
                    Err(e) => {
                        if current_generation.load(Ordering::SeqCst) == generation {
                            *slot.lock().unwrap() = Some(session);
                        }
                        Err(e)
                    }
                }
            }
            Err(e) => {
                if current_generation.load(Ordering::SeqCst) == generation {
                    *slot.lock().unwrap() = Some(session);
                }
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| format!("Merge save task failed: {}", e))?
}

#[tauri::command]
pub async fn discard_merge(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.merge_generation.fetch_add(1, Ordering::SeqCst);
    let session = state.merge_session.lock().unwrap().take();
    drop(session);
    Ok(())
}

#[tauri::command]
pub async fn save_collage(base64_data: String, first_path_str: String) -> Result<String, String> {
    let data_url_prefix = "data:image/png;base64,";
    if !base64_data.starts_with(data_url_prefix) {
        return Err("Invalid base64 data format".to_string());
    }
    let encoded_data = &base64_data[data_url_prefix.len()..];

    let decoded_bytes = general_purpose::STANDARD
        .decode(encoded_data)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;

    let (first_path, _) = parse_virtual_path(&first_path_str);
    let output_path = write_output(
        &first_path.to_string_lossy(),
        "Collage",
        "png",
        &decoded_bytes,
    )?;
    Ok(output_path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rasterize_strokes_maps_normalized_crop_coords() {
        let crop_rect = raster::Rect {
            x: 10,
            y: 20,
            width: 40,
            height: 40,
        };
        let strokes = vec![MergeBrushStroke {
            points: vec![[0.5, 0.5]],
            radius_norm: 0.15,
            frame_index: Some(1),
            seam_role: None,
        }];
        let mask = rasterize_strokes_for_rect(&strokes, 80, 80, &crop_rect, |s| {
            s.frame_index == Some(1)
        });
        assert!(mask[40 * 80 + 30] > 0.9, "stroke midpoint inside crop must be saturated");
        assert!(mask[2 * 80 + 2] == 0.0, "far corner must remain zero");
    }
}
