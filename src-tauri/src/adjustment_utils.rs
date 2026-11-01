use image::DynamicImage;
use std::borrow::Cow;
use std::collections::HashMap;

use crate::app_state::AppState;
use crate::image_processing::{
    Crop, IntoCowImage, apply_coarse_rotation, apply_crop, apply_flip, apply_geometry_warp,
    apply_rotation,
};

pub fn hydrate_sub_masks(
    sub_masks: &mut Vec<serde_json::Value>,
    cache: &mut HashMap<String, serde_json::Value>,
) {
    for sub_mask in sub_masks {
        let id = sub_mask
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        if id.is_empty() {
            continue;
        }

        if let Some(params) = sub_mask
            .get_mut("parameters")
            .and_then(|p| p.as_object_mut())
        {
            let keys_to_check = ["mask_data_base64", "maskDataBase64"];
            for key in keys_to_check {
                if params.contains_key(key) {
                    let val = params.get(key).unwrap();
                    if !val.is_null() {
                        cache.insert(id.clone(), val.clone());
                    } else {
                        if let Some(cached_data) = cache.get(&id) {
                            params.insert(key.to_string(), cached_data.clone());
                        }
                    }
                }
            }
        }
    }
}

pub fn hydrate_adjustments(state: &tauri::State<AppState>, adjustments: &mut serde_json::Value) {
    let mut cache = state.patch_cache.lock().unwrap();

    if let Some(patches) = adjustments
        .get_mut("aiPatches")
        .and_then(|v| v.as_array_mut())
    {
        for patch in patches {
            let id = patch
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            if !id.is_empty() {
                let has_data = patch.get("patchData").is_some_and(|v| !v.is_null());

                if has_data {
                    if let Some(data) = patch.get("patchData") {
                        cache.insert(id.clone(), data.clone());
                    }
                } else {
                    if let Some(cached_data) = cache.get(&id) {
                        patch["patchData"] = cached_data.clone();
                    }
                }
            }

            if let Some(sub_masks) = patch.get_mut("subMasks").and_then(|v| v.as_array_mut()) {
                hydrate_sub_masks(sub_masks, &mut cache);
            }
        }
    }

    if let Some(masks) = adjustments.get_mut("masks").and_then(|v| v.as_array_mut()) {
        for mask_container in masks {
            if let Some(sub_masks) = mask_container
                .get_mut("subMasks")
                .and_then(|v| v.as_array_mut())
            {
                hydrate_sub_masks(sub_masks, &mut cache);
            }
        }
    }
}

pub fn apply_orientation_and_flip<'a, I: IntoCowImage<'a>>(
    image: I,
    adjustments: &serde_json::Value,
) -> Cow<'a, DynamicImage> {
    let orientation_steps = adjustments["orientationSteps"].as_u64().unwrap_or(0) as u8;
    let flip_horizontal = adjustments["flipHorizontal"].as_bool().unwrap_or(false);
    let flip_vertical = adjustments["flipVertical"].as_bool().unwrap_or(false);

    let coarse_rotated_image = apply_coarse_rotation(image.into_cow(), orientation_steps);
    apply_flip(coarse_rotated_image, flip_horizontal, flip_vertical)
}

pub fn apply_spatial_transformations<'a, I: IntoCowImage<'a>>(
    image: I,
    adjustments: &serde_json::Value,
) -> (Cow<'a, DynamicImage>, (f32, f32)) {
    let rotation_degrees = adjustments["rotation"].as_f64().unwrap_or(0.0) as f32;

    let flipped_image = apply_orientation_and_flip(image, adjustments);
    let rotated_image = apply_rotation(flipped_image, rotation_degrees);

    let crop_data: Option<Crop> = serde_json::from_value(adjustments["crop"].clone()).ok();
    let crop_json = serde_json::to_value(crop_data).unwrap_or(serde_json::Value::Null);
    let cropped_image = apply_crop(rotated_image, &crop_json);

    let unscaled_crop_offset = crop_data.map_or((0.0, 0.0), |c| (c.x as f32, c.y as f32));

    (cropped_image, unscaled_crop_offset)
}

pub fn apply_all_transformations<'a, I: IntoCowImage<'a>>(
    image: I,
    adjustments: &serde_json::Value,
) -> (Cow<'a, DynamicImage>, (f32, f32)) {
    let start_time = std::time::Instant::now();
    let image = image.into_cow();

    let defringed_image = crate::denoising::apply_chromatic_defringe_if_enabled(image, adjustments);
    let warped_image = apply_geometry_warp(defringed_image, adjustments);
    let relit_image = crate::relight::apply_relight(warped_image, adjustments);
    let fogged_image = crate::fog::apply_fog(relit_image, adjustments);
    let blurred_image = crate::lens_blur::apply_lens_blur(fogged_image, adjustments);
    let tilt_shifted_image = crate::bokeh_simulator::apply_tilt_shift_if_enabled(blurred_image, adjustments);

    let (cropped_image, unscaled_crop_offset) =
        apply_spatial_transformations(tilt_shifted_image, adjustments);

    let total_duration = start_time.elapsed();
    log::info!("apply_all_transformations took {:.2?}", total_duration);

    (cropped_image, unscaled_crop_offset)
}


#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PhotographicReferencePreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub adjustments: serde_json::Value,
}

/// The 8 core photographic reference presets reverse-engineered from SNS-HDR Pro 2.
pub fn get_sns_hdr_reference_presets() -> Vec<PhotographicReferencePreset> {
    vec![
        PhotographicReferencePreset {
            id: "sns-natural".to_string(),
            name: "SNS Natural".to_string(),
            description: "Balanced natural HDR tone mapping with zero boundary halos and faithful colorimetry".to_string(),
            adjustments: serde_json::json!({
                "contrast": 0.0,
                "highlights": -15.0,
                "shadows": 20.0,
                "whites": 5.0,
                "blacks": -5.0,
                "clarity": 5.0,
                "structure": 10.0,
                "contrastCleaner": 1.0,
                "lightsBrightness": 0.0,
                "lightsContrast": 0.05,
                "lightsDetails": 0.15,
                "lightsTemperature": 0.0,
                "lightsTint": 0.0,
                "lightsSaturation": 0.0,
                "lightsVibrance": 0.05,
                "supercontrastShadows": 0.05,
                "supercontrastMidtones": 0.10,
                "supercontrastHighlights": 0.05,
                "supercontrastPivots": 0.0,
                "skinShield": 0.90,
                "foliageEnhancer": 0.0,
                "ortonSpread": 0.0
            }),
        },
        PhotographicReferencePreset {
            id: "sns-interior".to_string(),
            name: "SNS Interior".to_string(),
            description: "Real-estate & architectural interior exposure fusion balancing exterior window sunlight with warm indoor ambient light".to_string(),
            adjustments: serde_json::json!({
                "contrast": -5.0,
                "highlights": -40.0,
                "shadows": 45.0,
                "whites": -10.0,
                "blacks": 5.0,
                "clarity": 10.0,
                "structure": 15.0,
                "contrastCleaner": 1.25,
                "lightsBrightness": -0.25,
                "lightsContrast": 0.18,
                "lightsDetails": 0.28,
                "lightsTemperature": 0.12,
                "lightsTint": 0.0,
                "lightsSaturation": -0.05,
                "lightsVibrance": 0.0,
                "supercontrastShadows": 0.15,
                "supercontrastMidtones": 0.08,
                "supercontrastHighlights": 0.20,
                "supercontrastPivots": 0.15,
                "skinShield": 0.85,
                "foliageEnhancer": 0.0,
                "ortonSpread": 0.0
            }),
        },
        PhotographicReferencePreset {
            id: "sns-landscape".to_string(),
            name: "SNS Landscape".to_string(),
            description: "Vivid nature and landscape profile with enhanced foliage greens, deep polarized sky, and rich surface micro-textures".to_string(),
            adjustments: serde_json::json!({
                "contrast": 10.0,
                "highlights": -20.0,
                "shadows": 25.0,
                "whites": 10.0,
                "blacks": -8.0,
                "clarity": 18.0,
                "structure": 25.0,
                "contrastCleaner": 1.10,
                "foliageEnhancer": 0.35,
                "vibrance": 15.0,
                "saturation": 8.0,
                "lightsBrightness": -0.08,
                "lightsContrast": 0.12,
                "lightsDetails": 0.30,
                "lightsTemperature": 0.0,
                "lightsTint": 0.0,
                "lightsSaturation": 0.15,
                "lightsVibrance": 0.12,
                "supercontrastShadows": 0.18,
                "supercontrastMidtones": 0.12,
                "supercontrastHighlights": 0.22,
                "supercontrastPivots": -0.05,
                "skinShield": 0.80,
                "ortonSpread": 0.0
            }),
        },
        PhotographicReferencePreset {
            id: "sns-ldr".to_string(),
            name: "SNS LDR".to_string(),
            description: "Low Dynamic Range tone mapping for single high-contrast exposures without shadow noise amplification".to_string(),
            adjustments: serde_json::json!({
                "contrast": 8.0,
                "highlights": -18.0,
                "shadows": 22.0,
                "whites": 4.0,
                "blacks": -4.0,
                "clarity": 8.0,
                "structure": 12.0,
                "contrastCleaner": 0.90,
                "lightsBrightness": 0.05,
                "lightsContrast": 0.08,
                "lightsDetails": 0.14,
                "lightsTemperature": 0.0,
                "lightsTint": 0.0,
                "lightsSaturation": 0.02,
                "lightsVibrance": 0.06,
                "supercontrastShadows": 0.10,
                "supercontrastMidtones": 0.18,
                "supercontrastHighlights": 0.10,
                "supercontrastPivots": 0.10,
                "skinShield": 0.88,
                "foliageEnhancer": 0.0,
                "ortonSpread": 0.0
            }),
        },
        PhotographicReferencePreset {
            id: "sns-night".to_string(),
            name: "SNS Night".to_string(),
            description: "Nocturnal cityscapes and astrophotography with deep inky blacks, highlight flare control, and delicate bloom".to_string(),
            adjustments: serde_json::json!({
                "contrast": 12.0,
                "highlights": -25.0,
                "shadows": 15.0,
                "whites": 12.0,
                "blacks": -15.0,
                "clarity": 12.0,
                "structure": 18.0,
                "contrastCleaner": 1.20,
                "glowAmount": 12.0,
                "ortonSpread": 0.18,
                "lightsBrightness": -0.15,
                "lightsContrast": 0.25,
                "lightsDetails": 0.12,
                "lightsTemperature": -0.05,
                "lightsTint": 0.0,
                "lightsSaturation": 0.10,
                "lightsVibrance": 0.08,
                "supercontrastShadows": -0.08,
                "supercontrastMidtones": 0.05,
                "supercontrastHighlights": 0.28,
                "supercontrastPivots": -0.20,
                "skinShield": 0.85,
                "foliageEnhancer": 0.0
            }),
        },
        PhotographicReferencePreset {
            id: "sns-dramatic".to_string(),
            name: "SNS Dramatic".to_string(),
            description: "Punchy high-impact tonal drama with deep shadows, intense micro-structure, and zero halo artifacts".to_string(),
            adjustments: serde_json::json!({
                "contrast": 20.0,
                "highlights": -30.0,
                "shadows": 30.0,
                "whites": 18.0,
                "blacks": -18.0,
                "clarity": 28.0,
                "structure": 45.0,
                "contrastCleaner": 1.50,
                "vibrance": 18.0,
                "saturation": -5.0,
                "lightsBrightness": -0.10,
                "lightsContrast": 0.28,
                "lightsDetails": 0.42,
                "lightsTemperature": 0.02,
                "lightsTint": 0.0,
                "lightsSaturation": 0.12,
                "lightsVibrance": 0.16,
                "supercontrastShadows": 0.32,
                "supercontrastMidtones": 0.15,
                "supercontrastHighlights": 0.36,
                "supercontrastPivots": 0.0,
                "skinShield": 0.75,
                "foliageEnhancer": 0.0,
                "ortonSpread": 0.0
            }),
        },
        PhotographicReferencePreset {
            id: "sns-soft".to_string(),
            name: "SNS Soft".to_string(),
            description: "Ethereal portrait and dreamy landscape rendering with silky smooth skin tones, reduced grit, and gentle Orton glow".to_string(),
            adjustments: serde_json::json!({
                "contrast": -12.0,
                "highlights": -10.0,
                "shadows": 18.0,
                "whites": -5.0,
                "blacks": 4.0,
                "clarity": -12.0,
                "structure": -15.0,
                "contrastCleaner": 0.80,
                "glowAmount": 25.0,
                "ortonSpread": 0.40,
                "skinShield": 1.00,
                "lightsBrightness": 0.08,
                "lightsContrast": -0.05,
                "lightsDetails": -0.08,
                "lightsTemperature": 0.04,
                "lightsTint": 0.02,
                "lightsSaturation": -0.04,
                "lightsVibrance": 0.05,
                "supercontrastShadows": -0.10,
                "supercontrastMidtones": -0.05,
                "supercontrastHighlights": -0.12,
                "supercontrastPivots": 0.05,
                "foliageEnhancer": 0.0
            }),
        },
        PhotographicReferencePreset {
            id: "sns-neutral".to_string(),
            name: "SNS Neutral".to_string(),
            description: "Reference zero-point linear baseline for color-accurate studio and scientific reproduction".to_string(),
            adjustments: serde_json::json!({
                "contrast": 0.0,
                "highlights": 0.0,
                "shadows": 0.0,
                "whites": 0.0,
                "blacks": 0.0,
                "clarity": 0.0,
                "structure": 0.0,
                "contrastCleaner": 1.0,
                "lightsBrightness": 0.0,
                "lightsContrast": 0.0,
                "lightsDetails": 0.0,
                "lightsTemperature": 0.0,
                "lightsTint": 0.0,
                "lightsSaturation": 0.0,
                "lightsVibrance": 0.0,
                "supercontrastShadows": 0.0,
                "supercontrastMidtones": 0.0,
                "supercontrastHighlights": 0.0,
                "supercontrastPivots": 0.0,
                "skinShield": 0.90,
                "foliageEnhancer": 0.0,
                "ortonSpread": 0.0
            }),
        },
    ]
}
