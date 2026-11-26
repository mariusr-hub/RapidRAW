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
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter};

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
