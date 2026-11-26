pub mod alignment;
pub mod camera;
pub mod compositor;
pub mod layout;
pub mod maxflow;
pub mod photometric;
pub mod projection;
pub mod seams;

use crate::decode::formats::is_raw_file;
use crate::enhanced::hdr_panorama::cluster_hdr_brackets_robust;
use crate::merge::features::{FeatureSet, detect_features};
use crate::merge::hdr::{build_linear_hdr_panel, build_preview_hdr_panel, tonemap_hdr_raster};
use crate::merge::output::{MergeOutput, MergePreview, Rendition, encode_jpeg_base64};
use crate::merge::raster::{Raster, encoded_luminance, srgb_decoded, tone_compressed_luminance};
use crate::merge::source::{SourceLoader, file_name};
use crate::merge::{MergeContext, MergeSession, parse_settings};
use crate::metadata::exif_processing::read_focal_length_35mm;
use alignment::{Alignment, AlignmentImage, align_images_with_options};
use compositor::{
    CompositionPlan, Interpolation, PlanKey, Scene, SourceImage, StitchSettings, build_levels,
    level_for_magnification, level_for_size, plan_composition, render_composition,
};
use rayon::prelude::*;
use serde_json::Value;
use std::time::Instant;

const DETECTION_MAX_DIMENSION: usize = 2000;
const PHOTOMETRIC_MAX_DIMENSION: usize = 512;
const MIN_LEVEL_DIMENSION: usize = 128;
const SESSION_MAX_DIMENSION: usize = 2048;
const MAX_FEATURES: usize = 3000;
const PREVIEW_MAX_DIMENSION: f64 = 1600.0;

pub struct PanoramaSession {
    paths: Vec<String>,
    name_source: String,
    sources: Vec<SourceImage>,
    hdr_groups: Option<Vec<Vec<String>>>,
    hdr_panel_gains: Vec<f32>,
    alignment: Alignment,
    excluded_count: usize,
    half_size: bool,
    apply_edits: bool,
    is_hdr_panorama: bool,
    rendition: Rendition,
    plan: Option<(PlanKey, CompositionPlan)>,
    stitched_preview: Option<(PlanKey, u64, Raster)>,
}

struct LoadedSource {
    source: SourceImage,
    features: FeatureSet,
    detection_scale: f64,
    width: f64,
    height: f64,
    focal_35mm: Option<f64>,
}

impl PanoramaSession {
    fn scene_with_cameras<'a>(
        &'a self,
        sources: &'a [SourceImage],
        cameras: &'a [camera::Camera],
    ) -> Scene<'a> {
        Scene {
            sources: sources.iter().collect(),
            cameras,
            lens: self.alignment.lens,
            photometric: &self.alignment.photometric,
            orientation: self.alignment.orientation,
        }
    }

    fn prepare_plan(&mut self, settings: &StitchSettings) -> Result<(), String> {
        let key = settings.plan_key();
        if self
            .plan
            .as_ref()
            .is_none_or(|(cached_key, _)| *cached_key != key)
        {
            let cameras = camera::apply_user_rotation(
                &self.alignment.cameras,
                settings.yaw_deg,
                settings.pitch_deg,
                settings.roll_deg,
            );
            let plan = plan_composition(&self.scene_with_cameras(&self.sources, &cameras), settings)?;
            self.plan = Some((key, plan));
            self.stitched_preview = None;
        }
        Ok(())
    }

    fn plan(&self) -> &CompositionPlan {
        &self.plan.as_ref().expect("plan is prepared before use").1
    }

    fn scene_referred(&self) -> bool {
        matches!(self.rendition, Rendition::Scene { .. })
    }
}

impl MergeSession for PanoramaSession {
    fn preview(&mut self, settings: &Value) -> Result<MergePreview, String> {
        let settings: StitchSettings = parse_settings(settings)?;
        let key = settings.plan_key();
        self.prepare_plan(&settings)?;
        let (scale, width, height, projection, fov_deg, projection_note, wraps) = {
            let plan = self.plan();
            let (w, h) = plan.output_size(1.0, self.alignment.orientation);
            let target_max_dim = if settings.fast_draft || settings.half_size {
                PREVIEW_MAX_DIMENSION * 0.5
            } else {
                PREVIEW_MAX_DIMENSION
            };
            let scale = (target_max_dim / w.max(h) as f64).min(plan.full_scale());
            let (width, height) = plan.output_size(plan.full_scale(), self.alignment.orientation);
            (
                scale,
                width,
                height,
                plan.projection(),
                plan.fov_deg,
                plan.projection_note.clone(),
                plan.wraps(),
            )
        };
        let scale_bits = scale.to_bits();
        let needs_stitch = self
            .stitched_preview
            .as_ref()
            .is_none_or(|(k, sb, _)| *k != key || *sb != scale_bits);
        if needs_stitch {
            let cameras = camera::apply_user_rotation(
                &self.alignment.cameras,
                settings.yaw_deg,
                settings.pitch_deg,
                settings.roll_deg,
            );
            let plan = self.plan();
            let stitched = srgb_decoded(render_composition(
                &self.scene_with_cameras(&self.sources, &cameras),
                plan,
                scale,
                Interpolation::Bilinear,
                &|_| {},
            ));
            self.stitched_preview = Some((key, scale_bits, stitched));
        }
        let image = &self.stitched_preview.as_ref().unwrap().2;
        let preview_rgb = if self.is_hdr_panorama && self.scene_referred() {
            let (tm_raster, tm_rendition) = tonemap_hdr_raster(
                image,
                self.rendition.scene_exposure(),
                &settings.hdr_profile,
                settings.hdr_strength,
                settings.exposure_bias,
                settings.highlight_recovery,
                settings.shadow_lift,
                settings.detail_boost,
            );
            tm_rendition.to_srgb8(&tm_raster)
        } else {
            self.rendition.to_srgb8(image)
        };
        Ok(MergePreview {
            base64: encode_jpeg_base64(&preview_rgb)?,
            is_scene_referred: self.scene_referred(),
            width,
            height,
            image_count: self.sources.len(),
            excluded_count: self.excluded_count,
            projection: Some(projection),
            fov_deg: Some(fov_deg),
            projection_note,
            wraps,
            ..Default::default()
        })
    }

    fn render(&mut self, settings: &Value, context: &MergeContext) -> Result<MergeOutput, String> {
        let settings: StitchSettings = parse_settings(settings)?;
        self.prepare_plan(&settings)?;
        let hdr_groups = self.hdr_groups.clone();
        let hdr_panel_gains = self.hdr_panel_gains.clone();
        let loader = context.loader_with_options(self.scene_referred(), self.half_size, self.apply_edits);
        let plan = self.plan();
        let is_360_equirectangular =
            plan.wraps() && plan.projection() == projection::Projection::Spherical;
        let use_half = settings.half_size || settings.fast_draft;
        let scale = if use_half && !self.half_size {
            plan.full_scale() * 0.5
        } else {
            plan.full_scale()
        };
        let cameras = camera::apply_user_rotation(
            &self.alignment.cameras,
            settings.yaw_deg,
            settings.pitch_deg,
            settings.roll_deg,
        );
        let sources: Vec<SourceImage> = if let Some(groups) = hdr_groups {
            let total = groups.len();
            let mut panel_sources = Vec::with_capacity(total);
            for (index, group_paths) in groups.into_iter().enumerate() {
                context.check_cancelled()?;
                context.report(format!(
                    "Fusing full-res HDR panel {}/{} ({} exposures)...",
                    index + 1,
                    total,
                    group_paths.len()
                ));
                let gain = hdr_panel_gains.get(index).copied().unwrap_or(1.0);
                let (panel, _rendition, _meta) = build_linear_hdr_panel(
                    &group_paths,
                    self.half_size,
                    self.apply_edits,
                    gain,
                    context,
                )?;
                let camera = &cameras[index];
                panel_sources.push(SourceImage {
                    levels: vec![level_for_magnification(
                        panel,
                        camera,
                        plan.magnification(camera, scale),
                        MIN_LEVEL_DIMENSION,
                    )],
                });
            }
            panel_sources
        } else {
            context.report("Loading full resolution images...");
            loader.load_all(&self.paths, |index, path| {
                context.check_cancelled()?;
                let camera = &cameras[index];
                load_for_render(&loader, path, camera, plan.magnification(camera, scale))
            })?
        };
        context.check_cancelled()?;
        let progress = |message: String| context.report(message);
        let image = srgb_decoded(render_composition(
            &self.scene_with_cameras(&sources, &cameras),
            plan,
            scale,
            Interpolation::Bicubic,
            &progress,
        ));
        drop(sources);

        let is_scene = self.scene_referred();
        let base_exposure = self.rendition.scene_exposure();
        let (out_image, out_rendition, linear_scene) = if self.is_hdr_panorama && is_scene {
            let (tm_raster, tm_rendition) = tonemap_hdr_raster(
                &image,
                base_exposure,
                &settings.hdr_profile,
                settings.hdr_strength,
                settings.exposure_bias,
                settings.highlight_recovery,
                settings.shadow_lift,
                settings.detail_boost,
            );
            (tm_raster, tm_rendition, is_scene.then_some(image))
        } else {
            let lin = is_scene.then(|| image.clone());
            (image, self.rendition, lin)
        };

        Ok(MergeOutput {
            image: out_image,
            linear_scene,
            scene_exposure: base_exposure,
            hdr_exposure_bias: if self.is_hdr_panorama { settings.exposure_bias } else { 0.0 },
            rendition: out_rendition,
            suffix: if self.is_hdr_panorama { "HDR_Pano" } else { "Pano" },
            name_source: self.name_source.clone(),
            metadata_source: self.paths[0].clone(),
            keeps_lens_geometry: false,
            is_scene_referred: is_scene,
            is_360_equirectangular,
            tone_mapper: (self.is_hdr_panorama && is_scene).then_some("agx"),
        })
    }
}

fn load_for_render(
    loader: &SourceLoader,
    path: &str,
    camera: &camera::Camera,
    magnification: f64,
) -> Result<SourceImage, String> {
    let decoded = loader.decode(path)?;
    Ok(SourceImage {
        levels: vec![level_for_magnification(
            decoded.image,
            camera,
            magnification,
            MIN_LEVEL_DIMENSION,
        )],
    })
}

fn load_source_from_raster_with_full_size(
    image: Raster,
    full_width: usize,
    full_height: usize,
    focal_35mm: Option<f64>,
    is_hdr: bool,
    path: &str,
) -> LoadedSource {
    let levels = build_levels(image, MIN_LEVEL_DIMENSION, SESSION_MAX_DIMENSION);

    let detection_level = level_for_size(&levels, DETECTION_MAX_DIMENSION);
    let feature_luma = if is_hdr {
        tone_compressed_luminance(detection_level)
    } else {
        encoded_luminance(detection_level)
    };
    let features = detect_features(&feature_luma, MAX_FEATURES);
    log::info!("Found {} features in '{}'", features.len(), path);
    let detection_scale = full_width as f64 / detection_level.width as f64;

    LoadedSource {
        detection_scale,
        source: SourceImage { levels },
        features,
        width: full_width as f64,
        height: full_height as f64,
        focal_35mm,
    }
}

fn load_source_from_raster(
    image: Raster,
    focal_35mm: Option<f64>,
    path: &str,
) -> LoadedSource {
    let (width, height) = (image.width, image.height);
    load_source_from_raster_with_full_size(image, width, height, focal_35mm, false, path)
}

fn compute_panel_exposure_gains(exposures: &[Option<f64>]) -> Vec<f32> {
    let mut valid: Vec<f64> = exposures
        .iter()
        .flatten()
        .copied()
        .filter(|&e| e > 0.0)
        .collect();
    if valid.is_empty() {
        return vec![1.0; exposures.len()];
    }
    valid.sort_by(|a, b| a.total_cmp(b));
    let anchor = valid[valid.len() / 2];
    exposures
        .iter()
        .map(|exp| match exp {
            Some(e) if *e > 0.0 => ((anchor / *e) as f32).clamp(0.0625, 16.0),
            _ => 1.0,
        })
        .collect()
}

fn load_source(loader: &SourceLoader, path: &str) -> Result<LoadedSource, String> {
    let decoded = loader.decode(path)?;
    let focal_35mm = read_focal_length_35mm(&decoded.file_bytes).map(|f| f as f64);
    Ok(load_source_from_raster(decoded.image, focal_35mm, path))
}

pub fn build_session(
    paths: &[String],
    half_size: bool,
    settings: &Value,
    context: &MergeContext,
) -> Result<PanoramaSession, String> {
    let stitch_settings: StitchSettings = parse_settings(settings).unwrap_or_default();
    let apply_edits = stitch_settings.apply_edits;
    let scene_referred = paths.iter().all(is_raw_file);
    let loader = context.loader_with_options(scene_referred, half_size, apply_edits);

    let (groups, _) = cluster_hdr_brackets_robust(paths);
    let detected_hdr_pano = groups.len() >= 2
        && groups.iter().all(|g| g.paths.len() >= 2)
        && crate::enhanced::hdr_panorama::has_real_hdr_exposure_spread(&groups);
    let use_hdr_pano = stitch_settings.hdr_panorama.unwrap_or(detected_hdr_pano) && groups.len() >= 2;

    let start_time = Instant::now();
    let (mut loaded, mut hdr_groups_opt, mut hdr_gains, panel_paths): (
        Vec<LoadedSource>,
        Option<Vec<Vec<String>>>,
        Vec<f32>,
        Vec<String>,
    ) = if use_hdr_pano {
        let mut raw_panels = Vec::with_capacity(groups.len());
        let mut raw_groups = Vec::with_capacity(groups.len());
        let total_panels = groups.len();
        for (idx, group) in groups.into_iter().enumerate() {
            context.check_cancelled()?;
            context.report(format!(
                "Fusing HDR preview panel {}/{} ({} exposures)...",
                idx + 1,
                total_panels,
                group.paths.len()
            ));
            let panel = build_preview_hdr_panel(&group.paths, half_size, apply_edits, context)?;
            raw_panels.push(panel);
            raw_groups.push(group.paths);
        }
        let gains = if scene_referred {
            let exps: Vec<Option<f64>> = raw_panels.iter().map(|p| p.ref_exif_exposure).collect();
            compute_panel_exposure_gains(&exps)
        } else {
            vec![1.0; raw_panels.len()]
        };
        let mut panel_loaded = Vec::with_capacity(raw_panels.len());
        let mut panel_refs = Vec::with_capacity(raw_panels.len());
        for (mut panel, &gain) in raw_panels.into_iter().zip(&gains) {
            if (gain - 1.0).abs() > 1e-4 {
                panel.preview.data.par_iter_mut().for_each(|v| *v *= gain);
            }
            let focal_35mm = crate::library::file_ops::read_file_mapped(std::path::Path::new(&panel.metadata_source))
                .ok()
                .and_then(|bytes| read_focal_length_35mm(&bytes).map(|f| f as f64));
            panel_loaded.push(load_source_from_raster_with_full_size(
                panel.preview,
                panel.full_width,
                panel.full_height,
                focal_35mm,
                scene_referred,
                &panel.metadata_source,
            ));
            panel_refs.push(panel.metadata_source);
        }
        (panel_loaded, Some(raw_groups), gains, panel_refs)
    } else {
        context.report("Loading images and detecting features...");
        let loaded = loader.load_all(paths, |_, path| {
            context.check_cancelled()?;
            context.report(format!("Processing '{}'", file_name(path)));
            let res = load_source(&loader, path)?;
            context.check_cancelled()?;
            Ok(res)
        })?;
        (loaded, None, Vec::new(), paths.to_vec())
    };
    context.check_cancelled()?;
    log::info!(
        "Image loading and feature detection completed in {:.2?}",
        start_time.elapsed()
    );

    let start_time = Instant::now();
    let inputs: Vec<AlignmentImage> = loaded
        .iter()
        .map(|l| AlignmentImage {
            features: &l.features,
            detection_scale: l.detection_scale,
            width: l.width,
            height: l.height,
            focal_35mm: l.focal_35mm,
            photometric_level: level_for_size(&l.source.levels, PHOTOMETRIC_MAX_DIMENSION),
        })
        .collect();
    let progress = |message: String| context.report(message);
    let alignment = align_images_with_options(&inputs, use_hdr_pano && scene_referred, &progress)?;
    drop(inputs);
    log::info!("Alignment completed in {:.2?}", start_time.elapsed());

    let excluded_count = loaded.len() - alignment.included.len();
    if excluded_count > 0 {
        let excluded: Vec<String> = (0..panel_paths.len())
            .filter(|i| !alignment.included.contains(i))
            .map(|i| file_name(&panel_paths[i]))
            .collect();
        log::warn!(
            "{} image(s) could not be matched and will be excluded: {}",
            excluded_count,
            excluded.join(", ")
        );
    }

    let mut loaded_opts: Vec<Option<LoadedSource>> = loaded.drain(..).map(Some).collect();
    let sources = alignment
        .included
        .iter()
        .map(|&i| loaded_opts[i].take().unwrap().source)
        .collect();

    if let Some(ref mut groups) = hdr_groups_opt {
        let mut group_opts: Vec<Option<Vec<String>>> = groups.drain(..).map(Some).collect();
        *groups = alignment
            .included
            .iter()
            .filter_map(|&i| group_opts[i].take())
            .collect();
        hdr_gains = alignment
            .included
            .iter()
            .map(|&i| hdr_gains.get(i).copied().unwrap_or(1.0))
            .collect();
    }

    let included_paths: Vec<String> = alignment
        .included
        .iter()
        .map(|&i| panel_paths[i].clone())
        .collect();
    let rendition = Rendition::for_sources(scene_referred, &included_paths[0], context.settings);

    Ok(PanoramaSession {
        paths: included_paths,
        name_source: paths[0].clone(),
        sources,
        hdr_groups: hdr_groups_opt,
        hdr_panel_gains: hdr_gains,
        alignment,
        excluded_count,
        half_size,
        apply_edits,
        is_hdr_panorama: use_hdr_pano,
        rendition,
        plan: None,
        stitched_preview: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_key_ignores_hdr_tone_fields() {
        let mut a = StitchSettings::default();
        let mut b = StitchSettings::default();
        b.hdr_profile = "dramatic".to_string();
        b.hdr_strength = 95.0;
        b.exposure_bias = 1.25;
        b.highlight_recovery = 80.0;
        b.shadow_lift = 60.0;
        b.detail_boost = 1.5;
        assert_eq!(a.plan_key(), b.plan_key());

        a.boundary_warp = 25.0;
        assert_ne!(a.plan_key(), b.plan_key());
    }

    #[test]
    fn test_compute_panel_exposure_gains_normalizes_auto_exposure_sweep() {
        // Panel 0 at 1/60s, Panel 1 at 1/240s (2 EV brighter scene -> needs 4.0x gain to match Panel 0)
        let gains = compute_panel_exposure_gains(&[Some(1.0 / 60.0), Some(1.0 / 240.0), None]);
        assert!((gains[0] - 1.0).abs() < 1e-4);
        assert!((gains[1] - 4.0).abs() < 1e-3);
        assert!((gains[2] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_compute_panel_exposure_gains_uses_median_anchor() {
        let gains = compute_panel_exposure_gains(&[
            Some(1.0 / 4000.0),
            Some(1.0 / 125.0),
            Some(1.0 / 125.0),
            Some(1.0 / 125.0),
            Some(1.0 / 125.0),
        ]);
        for i in 1..5 {
            assert!(
                (gains[i] - 1.0).abs() < 1e-4,
                "Median panels 1..4 should stay 1.0 instead of anchoring to outlier panel 0, got {:?}",
                gains
            );
        }
        assert!((gains[0] - 16.0).abs() < 1e-3);
    }
}
