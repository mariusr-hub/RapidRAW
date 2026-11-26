use crate::app::memory_budget::MemoryBudget;
use crate::app::settings::AppSettings;
use crate::decode::formats::is_raw_file;
use crate::library::sidecars::parse_virtual_path;
use crate::merge::raster::{Raster, srgb_decoded};
use crate::render::image_processing::apply_cpu_default_raw_processing;
use rayon::prelude::*;
use std::borrow::Cow;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tauri::AppHandle;

pub struct SourceLoader<'a> {
    settings: Cow<'a, AppSettings>,
    memory_budget: &'a Arc<MemoryBudget>,
    app_handle: &'a AppHandle,
    linear_raw: bool,
    half_size: bool,
    apply_edits: bool,
}

pub struct DecodedSource {
    pub file_bytes: Vec<u8>,
    pub image: Raster,
}

fn srgb_linear_to_working(mut raster: Raster) -> Raster {
    let m = crate::render::color_space::srgb_to_working();
    raster.data.par_chunks_exact_mut(3).for_each(|p| {
        let v = m * glam::Vec3::new(p[0], p[1], p[2]);
        p[0] = v.x.max(0.0);
        p[1] = v.y.max(0.0);
        p[2] = v.z.max(0.0);
    });
    raster
}

impl<'a> SourceLoader<'a> {
    pub fn new(
        settings: &'a AppSettings,
        memory_budget: &'a Arc<MemoryBudget>,
        app_handle: &'a AppHandle,
        linear_raw: bool,
        half_size: bool,
        apply_edits: bool,
    ) -> Self {
        let settings = if linear_raw {
            let mut unprocessed = settings.clone();
            unprocessed.raw_preprocessing_color_nr = Some(0.0);
            unprocessed.raw_preprocessing_sharpening = Some(0.0);
            Cow::Owned(unprocessed)
        } else {
            Cow::Borrowed(settings)
        };
        SourceLoader {
            settings,
            memory_budget,
            app_handle,
            linear_raw,
            half_size,
            apply_edits,
        }
    }

    pub fn with_hdr_linear(mut self, hdr_linear: bool) -> Self {
        if hdr_linear && self.linear_raw {
            let mut owned = self.settings.into_owned();
            owned.linear_raw_mode = "hdr_linear".to_string();
            self.settings = Cow::Owned(owned);
        }
        self
    }

    pub fn load_all<T: Send>(
        &self,
        paths: &[String],
        load: impl Fn(usize, &str) -> Result<T, String> + Sync,
    ) -> Result<Vec<T>, String> {
        let load = &load;
        std::thread::scope(|scope| {
            let workers: Vec<_> = paths
                .iter()
                .enumerate()
                .map(|(index, path)| {
                    let estimate = crate::export::tasks::estimate_export_memory(
                        path,
                        is_raw_file(path),
                        None,
                        1,
                        0,
                        "tiff",
                        0,
                    );
                    let reservation = self.memory_budget.reserve(estimate, None);
                    scope.spawn(move || {
                        let _reservation = reservation;
                        load(index, path)
                    })
                })
                .collect();
            workers
                .into_iter()
                .map(|worker| {
                    worker
                        .join()
                        .map_err(|_| "Loading an image failed unexpectedly.".to_string())?
                })
                .collect()
        })
    }

    pub fn decode(&self, path: &str) -> Result<DecodedSource, String> {
        let is_raw = is_raw_file(path);
        let file_bytes =
            fs::read(path).map_err(|e| format!("Failed to read image {}: {}", path, e))?;
        let linear_raw = is_raw && self.linear_raw;

        let sidecar_adjustments = if self.apply_edits {
            let meta = crate::metadata::sidecar::load_primary_metadata(Path::new(path));
            if meta.adjustments.is_object() && !meta.adjustments.as_object().is_none_or(|m| m.is_empty()) {
                Some(meta.adjustments)
            } else {
                None
            }
        } else {
            None
        };

        let mut dynamic_image = crate::decode::image_loader::load_base_image_from_bytes(
            &file_bytes,
            path,
            self.half_size,
            &self.settings,
            None,
        )
        .map_err(|e| format!("Failed to load image {}: {}", path, e))?;
        if !is_raw && self.half_size {
            let (w, h) = (dynamic_image.width(), dynamic_image.height());
            if w.max(h) > 2048 {
                dynamic_image = dynamic_image.resize_exact(
                    (w / 2).max(1),
                    (h / 2).max(1),
                    image::imageops::FilterType::Triangle,
                );
            }
        }

        if !linear_raw
            && sidecar_adjustments.is_some()
            && let Some(edited) = crate::export::render_image_with_edits(
                path,
                &dynamic_image,
                sidecar_adjustments.as_ref(),
                &self.settings,
                self.app_handle,
                true,
            )
        {
            let rgb = edited.into_rgb32f();
            let (width, height) = (rgb.width() as usize, rgb.height() as usize);
            let image = srgb_linear_to_working(srgb_decoded(Raster::from_data(width, height, 3, rgb.into_raw())));
            return Ok(DecodedSource { file_bytes, image });
        }
        if let Some(ref adj) = sidecar_adjustments {
            let img_cow = crate::decode::camera_profile::apply_camera_profile_if_enabled(Cow::Owned(dynamic_image), adj);
            let img_cow = crate::enhanced::defect_repair::apply_sensor_dust_heal_if_enabled(img_cow, adj);
            let img_cow = crate::enhanced::denoising::apply_chromatic_defringe_if_enabled(img_cow, adj);
            let img_cow = crate::enhanced::astro_stacking::apply_astro_lp_if_enabled(img_cow, adj);
            let img_cow = crate::enhanced::astro_stacking::apply_astro_pcc_if_enabled(img_cow, adj);
            let img_cow = crate::enhanced::astro_stacking::apply_astro_adc_if_enabled(img_cow, adj);
            dynamic_image = crate::enhanced::astro_starless::apply_astro_star_separation_if_enabled(img_cow, adj).into_owned();
        }
        if is_raw && !linear_raw {
            apply_cpu_default_raw_processing(&mut dynamic_image);
        } else if !is_raw {
            crate::decode::hdr_source::roll_off_encoded(
                &mut dynamic_image,
                crate::decode::as_shot::as_shot_from_bytes(path, &file_bytes).source_peak,
            );
        }

        let rgb = dynamic_image.into_rgb32f();
        let (width, height) = (rgb.width() as usize, rgb.height() as usize);
        let mut image = Raster::from_data(width, height, 3, rgb.into_raw());
        if linear_raw {
            image.data.par_iter_mut().for_each(|v| *v = v.max(0.0));
        } else {
            image = srgb_decoded(image);
        }
        Ok(DecodedSource { file_bytes, image })
    }
}

pub fn source_paths(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .map(|p| parse_virtual_path(p).0.to_string_lossy().into_owned())
        .collect()
}

pub fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

pub fn ensure_same_size<'a>(
    images: impl IntoIterator<Item = (&'a str, usize, usize)>,
) -> Result<(), String> {
    let mut images = images.into_iter();
    let Some((first, width, height)) = images.next() else {
        return Ok(());
    };
    match images.find(|&(_, w, h)| w != width || h != height) {
        Some((other, w, h)) => Err(format!(
            "All images must have the same size and orientation. '{}' is {}x{}, but '{}' is {}x{}.",
            file_name(first),
            width,
            height,
            file_name(other),
            w,
            h
        )),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_srgb_linear_to_working_roundtrips_with_working_to_srgb() {
        let samples = [
            [1.0f32, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.5, 0.5, 0.5],
        ];
        let working_to_srgb = crate::render::color_space::working_to_srgb();
        let flat: Vec<f32> = samples.iter().flatten().copied().collect();
        let working = srgb_linear_to_working(Raster::from_data(4, 1, 3, flat));

        // Red primary in Rec.2020 must be strictly less than 1.0 (proves primaries matrix applied)
        assert!(
            working.data[0] < 1.0,
            "Expected Rec.2020 red < 1.0 for sRGB [1,0,0], got {}",
            working.data[0]
        );

        for (i, orig) in samples.iter().enumerate() {
            let w = glam::Vec3::new(
                working.data[i * 3],
                working.data[i * 3 + 1],
                working.data[i * 3 + 2],
            );
            let back = working_to_srgb * w;
            for (c, &expected) in orig.iter().enumerate() {
                assert!(
                    (back[c] - expected).abs() < 1e-3,
                    "Roundtrip mismatch at sample {} ch {}: got {}, expected {}",
                    i,
                    c,
                    back[c],
                    expected
                );
            }
        }
    }
}
