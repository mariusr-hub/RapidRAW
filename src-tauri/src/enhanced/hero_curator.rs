//! AI Hero-Shot Curator for RapidRAW
//!
//! Analyzes bursts, sequences, and photoshoot sets to find the single sharpest,
//! most dynamic, and aesthetically optimal winner frame (#1 Hero Shot),
//! auto-assigning 5-star ratings and culling suggestions.

use crate::library::sidecars::{parse_virtual_path, set_rating_for_paths};
use crate::library::file_ops::read_file_mapped;
use crate::decode::image_loader::load_base_image_from_bytes;
use image::DynamicImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeroShotScore {
    pub file_path: String,
    pub file_name: String,
    pub overall_score: f32,
    pub sharpness_score: f32,
    pub eye_face_clarity_score: f32,
    pub exposure_balance_score: f32,
    pub is_winner: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HeroCuratorSummary {
    pub total_analyzed: usize,
    pub winner_path: String,
    pub winner_score: f32,
    pub scores: Vec<HeroShotScore>,
}

fn compute_sharpness_score(thumb: &DynamicImage) -> f32 {
    rapidraw_dsp::calculate_laplacian_variance(&thumb.to_rgb8()).sqrt().clamp(0.0, 100.0)
}

fn compute_exposure_balance(thumb: &DynamicImage) -> f32 {
    let gray = thumb.to_luma8();
    let (w, h) = gray.dimensions();
    let total = (w * h) as f32;

    let mut clipped_highlights = 0.0f32;
    let mut clipped_shadows = 0.0f32;
    let mut midtones = 0.0f32;

    for p in gray.pixels() {
        let v = p[0];
        if v > 250 {
            clipped_highlights += 1.0;
        } else if v < 5 {
            clipped_shadows += 1.0;
        } else if v > 40 && v < 215 {
            midtones += 1.0;
        }
    }

    let highlight_penalty = (clipped_highlights / total) * 60.0;
    let shadow_penalty = (clipped_shadows / total) * 30.0;
    let midtone_bonus = (midtones / total) * 80.0;

    (100.0 - highlight_penalty - shadow_penalty + midtone_bonus).clamp(10.0, 100.0)
}

pub fn compute_keeper_rank_score(
    eye_state: &str,
    iris_sharpness: f32,
    face_exposure_quality: f32,
) -> f32 {
    let eyes_open = match eye_state {
        "open" => 100.0,
        "squinting" => 35.0,
        _ => 0.0,
    };
    (eyes_open * 0.45
        + iris_sharpness.clamp(0.0, 100.0) * 0.35
        + face_exposure_quality.clamp(0.0, 100.0) * 0.20)
        .clamp(0.0, 100.0)
}

/// Groups near-duplicate burst poses whose normalized CLIP cosine similarity exceeds `threshold` (0.94).
pub fn dedupe_burst_frames_by_clip(
    embeddings: &[(String, Vec<f32>)],
    threshold: f32,
) -> Vec<Vec<usize>> {
    let n = embeddings.len();
    if n == 0 {
        return Vec::new();
    }
    let mut visited = vec![false; n];
    let mut clusters = Vec::new();
    for i in 0..n {
        if visited[i] {
            continue;
        }
        visited[i] = true;
        let mut cluster = vec![i];
        let (_, ref emb_i) = embeddings[i];
        let mut cursor = 0usize;
        while cursor < cluster.len() {
            let cur = cluster[cursor];
            cursor += 1;
            let ref_vec = if cur == i { emb_i } else { &embeddings[cur].1 };
            for j in 0..n {
                if visited[j] || ref_vec.len() != embeddings[j].1.len() || ref_vec.is_empty() {
                    continue;
                }
                let dot: f32 = ref_vec
                    .iter()
                    .zip(&embeddings[j].1)
                    .map(|(&a, &b)| a * b)
                    .sum();
                if dot > threshold {
                    visited[j] = true;
                    cluster.push(j);
                }
            }
        }
        clusters.push(cluster);
    }
    clusters
}

#[tauri::command]
pub fn curate_hero_shots(
    paths: Vec<String>,
    auto_rate_winner: Option<bool>,
    app_handle: AppHandle,
    state: State<crate::app::state::AppState>,
) -> Result<HeroCuratorSummary, String> {
    if paths.is_empty() {
        return Err("No photos provided for hero curation".to_string());
    }

    let should_rate = auto_rate_winner.unwrap_or(true);
    let total = paths.len();
    let settings = crate::app::settings::load_settings(app_handle.clone()).unwrap_or_default();
    let processed_counter = Arc::new(AtomicUsize::new(0));

    let _ = app_handle.emit(
        "hero-curator-progress",
        serde_json::json!({
            "current": 0,
            "total": total,
            "message": format!("Evaluating {} shots for best burst frame...", total),
            "percentage": 0.0
        }),
    );

    state.ai_denoise_cancel.store(false, Ordering::SeqCst);
    let cancel_token = state.ai_denoise_cancel.clone();
    let mut scores: Vec<HeroShotScore> = paths
        .par_iter()
        .filter_map(|path_str| {
            if cancel_token.load(Ordering::Relaxed) {
                return None;
            }
            let (source_path, _) = parse_virtual_path(path_str);
            let file_name = Path::new(path_str)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let image = if let Ok(file_bytes) = read_file_mapped(&source_path) {
                load_base_image_from_bytes(
                    &file_bytes,
                    &source_path.to_string_lossy(),
                    true,
                    &settings,
                    None,
                )
                .unwrap_or_else(|_| DynamicImage::new_rgb8(100, 100))
            } else {
                DynamicImage::new_rgb8(100, 100)
            };

            let sharpness = compute_sharpness_score(&image);
            let exposure_bal = compute_exposure_balance(&image);
            let face_loupes = crate::enhanced::speed_culler::detect_face_loupes(&image);
            let (clarity, overall) = if let Some(best_face) = face_loupes
                .iter()
                .max_by(|a, b| a.iris_sharpness.total_cmp(&b.iris_sharpness))
            {
                let iris = best_face.iris_sharpness.max(best_face.sharpness_score).clamp(0.0, 100.0);
                let rank = compute_keeper_rank_score(&best_face.eye_state, iris, exposure_bal);
                (iris, rank)
            } else {
                (sharpness, sharpness * 0.55 + sharpness * 0.25 + exposure_bal * 0.20)
            };

            let done = processed_counter.fetch_add(1, Ordering::SeqCst) + 1;
            let percent = (done as f32 / total as f32) * 100.0;

            let _ = app_handle.emit(
                "hero-curator-progress",
                serde_json::json!({
                    "current": done,
                    "total": total,
                    "filename": file_name,
                    "score": overall,
                    "percentage": percent
                }),
            );

            Some(HeroShotScore {
                file_path: path_str.clone(),
                file_name,
                overall_score: overall,
                sharpness_score: sharpness,
                eye_face_clarity_score: clarity,
                exposure_balance_score: exposure_bal,
                is_winner: false,
            })
        })
        .collect();
    if cancel_token.load(Ordering::Relaxed) {
        return Err("Hero curation cancelled by user".to_string());
    }

    // Deduplicate identical burst poses via cached CLIP embeddings (> 0.94 cosine),
    // retaining the top-ranked keeper from each burst pose cluster.
    let clip_map = crate::library::catalog_db::get_clip_embeddings_for_paths(&app_handle, &paths);
    if clip_map.len() >= 2 {
        let embedded_entries: Vec<(String, Vec<f32>)> = scores
            .iter()
            .filter_map(|s| clip_map.get(&s.file_path).map(|v| (s.file_path.clone(), v.clone())))
            .collect();
        if embedded_entries.len() >= 2 {
            let clusters = dedupe_burst_frames_by_clip(&embedded_entries, 0.94);
            let mut keep_paths = std::collections::HashSet::new();
            for cluster in clusters {
                if let Some((best_path, _)) = cluster
                    .into_iter()
                    .filter_map(|idx| {
                        let p = &embedded_entries[idx].0;
                        scores.iter().find(|s| &s.file_path == p).map(|s| (p.clone(), s.overall_score))
                    })
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                {
                    keep_paths.insert(best_path);
                }
            }
            for s in &scores {
                if !clip_map.contains_key(&s.file_path) {
                    keep_paths.insert(s.file_path.clone());
                }
            }
            scores.retain(|s| keep_paths.contains(&s.file_path));
        }
    }

    // Find winner
    scores.sort_by(|a, b| b.overall_score.partial_cmp(&a.overall_score).unwrap_or(std::cmp::Ordering::Equal));
    if let Some(winner) = scores.first_mut() {
        winner.is_winner = true;
    }

    let winner_path = scores.first().map(|s| s.file_path.clone()).unwrap_or_default();
    let winner_score = scores.first().map(|s| s.overall_score).unwrap_or(0.0);

    // Auto-rate winner with 5 stars
    if should_rate && !winner_path.is_empty() {
        let _ = set_rating_for_paths(vec![winner_path.clone()], 5, app_handle.clone());
    }

    let summary = HeroCuratorSummary {
        total_analyzed: total,
        winner_path,
        winner_score,
        scores,
    };

    let _ = app_handle.emit("hero-curator-complete", &summary);
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn test_hero_sharpness_and_exposure_ranking() {
        let (w, h) = (64u32, 64u32);
        let mut sharp_img = RgbImage::new(w, h);
        let mut flat_img = RgbImage::new(w, h);

        for y in 0..h {
            for x in 0..w {
                flat_img.put_pixel(x, y, Rgb([128, 128, 128]));
                let val = if (x / 4) % 2 == 0 { 200 } else { 50 };
                sharp_img.put_pixel(x, y, Rgb([val, val, val]));
            }
        }

        let sharp_dyn = DynamicImage::ImageRgb8(sharp_img);
        let flat_dyn = DynamicImage::ImageRgb8(flat_img);

        let sharp_score = compute_sharpness_score(&sharp_dyn);
        let flat_score = compute_sharpness_score(&flat_dyn);
        assert!(sharp_score > flat_score, "Sharp image must score higher than flat image");

        let expo_score = compute_exposure_balance(&sharp_dyn);
        assert!(expo_score >= 50.0, "Balanced image should score >= 50: got {}", expo_score);
    }

    #[test]
    fn test_clip_burst_deduplication_and_keeper_ranking() {
        let e0 = vec![1.0f32, 0.0, 0.0, 0.0];
        let e1 = vec![0.98f32, 0.198, 0.0, 0.0]; // cosine ~ 0.98 > 0.94 with e0
        let e2 = vec![0.0f32, 1.0, 0.0, 0.0]; // distinct pose
        let clusters = dedupe_burst_frames_by_clip(
            &[
                ("p0.arw".into(), e0),
                ("p1.arw".into(), e1),
                ("p2.arw".into(), e2),
            ],
            0.94,
        );
        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[0], vec![0, 1]);
        assert_eq!(clusters[1], vec![2]);

        let open_sharp = compute_keeper_rank_score("open", 85.0, 80.0);
        let squint_sharp = compute_keeper_rank_score("squinting", 85.0, 80.0);
        let blink_sharp = compute_keeper_rank_score("blink", 85.0, 80.0);
        assert!(open_sharp > squint_sharp && squint_sharp > blink_sharp);
    }
}
