use image::DynamicImage;
use std::borrow::Cow;
use std::collections::HashMap;

use crate::app::state::AppState;
use crate::decode::as_shot::AsShot;
use crate::decode::baseline_exposure::BaselineOptions;
use crate::render::geometry::{Crop, IntoCowImage, apply_coarse_rotation, apply_flip, crop_rect};
use serde_json::{Value, json};
use std::sync::OnceLock;

const EFFECT_MAP_KEYS: [&str; 4] = [
    "lensBlurDepthMap",
    "fogDepthMap",
    "motionBlurDepthMap",
    "relightNormalMap",
];
const CACHED_EFFECT_MAP_PREFIX: &str = "__cached__:";

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

    let effect_map_ids = adjustments
        .as_object_mut()
        .and_then(|obj| obj.remove("effectMapIds"));

    for key in EFFECT_MAP_KEYS {
        let cached_id = adjustments
            .get(key)
            .and_then(|v| v.as_str())
            .and_then(|v| v.strip_prefix(CACHED_EFFECT_MAP_PREFIX))
            .map(str::to_owned);

        if let Some(id) = cached_id {
            adjustments[key] = cache
                .get(&format!("effectMap:{}", id))
                .cloned()
                .unwrap_or(serde_json::Value::Null);
        } else if let Some(id) = effect_map_ids
            .as_ref()
            .and_then(|ids| ids.get(key))
            .and_then(|v| v.as_u64())
            && let Some(value) = adjustments.get(key).filter(|v| v.is_string())
        {
            cache.insert(format!("effectMap:{}", id), value.clone());
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

pub fn transformed_dimensions(
    width: u32,
    height: u32,
    adjustments: &serde_json::Value,
) -> (u32, u32) {
    let orientation_steps = adjustments["orientationSteps"].as_u64().unwrap_or(0) as u8;
    let (w, h) = if matches!(orientation_steps, 1 | 3) {
        (height, width)
    } else {
        (width, height)
    };

    serde_json::from_value::<Crop>(adjustments["crop"].clone())
        .ok()
        .and_then(|crop| crop_rect(&crop, (w, h)))
        .map_or((w, h), |(_, _, width, height)| (width, height))
}

const BASELINE_EXPOSURE_KEY: &str = "baselineExposure";

pub fn mark_baseline_exposure(adjustments: &mut Value) {
    if let Some(object) = adjustments.as_object_mut() {
        object.insert(BASELINE_EXPOSURE_KEY.to_string(), Value::Bool(true));
    }
}

pub fn default_adjustments(path: &std::path::Path) -> Value {
    let mut adjustments = json!({});
    mark_baseline_exposure(&mut adjustments);
    crate::adjustments::develop_defaults::apply_overlay(&mut adjustments, path);
    adjustments
}

pub fn differs_from_default(adjustments: &Value, key: &str, path: &std::path::Path) -> bool {
    let factory = default_adjustment_values().and_then(|factory| factory.get(key));
    let overlay = crate::adjustments::develop_defaults::overlay(path);
    let stored = adjustments.get(key).filter(|v| !v.is_null()).or(factory);
    let default = overlay.get(key).or(factory);
    match (stored, default) {
        (Some(stored), Some(default)) => !json_values_equal(stored, default),
        (stored, default) => stored != default,
    }
}

pub fn factory_adjustments() -> Option<&'static serde_json::Map<String, Value>> {
    default_adjustment_values()
}

pub fn neutral_adjustments() -> Value {
    let mut adjustments = default_adjustment_values()
        .cloned()
        .map_or_else(|| json!({}), Value::Object);
    mark_baseline_exposure(&mut adjustments);
    adjustments
}

pub fn is_unedited(adjustments: &Value) -> bool {
    match adjustments {
        Value::Null => true,
        Value::Object(map) => {
            map.len() == 1 && map.get(BASELINE_EXPOSURE_KEY).and_then(Value::as_bool) == Some(true)
        }
        _ => false,
    }
}

fn default_adjustment_values() -> Option<&'static serde_json::Map<String, Value>> {
    static DEFAULTS: OnceLock<Option<serde_json::Map<String, Value>>> = OnceLock::new();
    DEFAULTS
        .get_or_init(|| {
            serde_json::from_str(include_str!("../../../src/utils/defaultAdjustments.json"))
                .map_err(|e| log::error!("Failed to parse defaultAdjustments.json: {}", e))
                .ok()
        })
        .as_ref()
}

fn json_values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(x, y)| json_values_equal(x, y))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(key, x)| y.get(key).is_some_and(|y| json_values_equal(x, y)))
        }
        _ => a == b,
    }
}

fn is_null_or_all_true(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Object(map) => map.values().all(|v| v.as_bool() == Some(true)),
        _ => false,
    }
}

pub fn is_default_adjustments(adjustments: &Value, path: &std::path::Path) -> bool {
    let map = match adjustments {
        Value::Null => return true,
        Value::Object(map) => map,
        _ => return false,
    };
    let Some(factory) = default_adjustment_values() else {
        return false;
    };
    let overlay = crate::adjustments::develop_defaults::overlay(path);

    if map.get(BASELINE_EXPOSURE_KEY).and_then(Value::as_bool) != Some(true) {
        return false;
    }

    let overlay_satisfied = overlay.iter().all(|(key, value)| {
        map.contains_key(key)
            || factory
                .get(key)
                .is_some_and(|factory| json_values_equal(value, factory))
    });

    overlay_satisfied
        && map.iter().all(|(key, value)| match key.as_str() {
            BASELINE_EXPOSURE_KEY => true,
            "activeTools" => is_null_or_all_true(value),
            _ => overlay
                .get(key)
                .or_else(|| factory.get(key))
                .is_some_and(|default| json_values_equal(value, default)),
        })
}

fn uses_baseline_exposure(adjustments: &Value) -> bool {
    adjustments.is_null()
        || adjustments
            .get(BASELINE_EXPOSURE_KEY)
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

pub fn effective_baseline_exposure(
    adjustments: &Value,
    as_shot: AsShot,
    options: BaselineOptions,
) -> f32 {
    if uses_baseline_exposure(adjustments) {
        as_shot.exposure.compensation(options)
    } else {
        0.0
    }
}

fn is_section_visible(adjustments: &serde_json::Value, section: &str) -> bool {
    adjustments
        .get("sectionVisibility")
        .and_then(|v| v.get(section))
        .and_then(|s| s.as_bool())
        .unwrap_or(true)
}

const LEGACY_SECTION_TOOLS: &[(&str, &[&str])] = &[
    ("basic", &["basic"]),
    ("curves", &["curves"]),
    (
        "color",
        &[
            "whiteBalance",
            "colorPresence",
            "hue",
            "colorGrading",
            "colorMixer",
            "colorCalibration",
        ],
    ),
    (
        "details",
        &[
            "sharpening",
            "presence",
            "noiseReduction",
            "superResolution",
            "chromaticAberration",
            "negativeConversion",
        ],
    ),
    (
        "effects",
        &[
            "creative",
            "motionBlur",
            "lensBlur",
            "relight",
            "fog",
            "lut",
            "vignette",
            "grain",
        ],
    ),
];

pub fn is_tool_active(adjustments: &serde_json::Value, tool: &str) -> bool {
    match adjustments.get("activeTools") {
        Some(active_tools) => active_tools
            .get(tool)
            .and_then(|active| active.as_bool())
            .unwrap_or(true),
        None => LEGACY_SECTION_TOOLS
            .iter()
            .find(|(_, tools)| tools.contains(&tool))
            .is_none_or(|(section, _)| is_section_visible(adjustments, section)),
    }
}

pub fn active_tools(adjustments: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    match adjustments
        .get("activeTools")
        .and_then(|tools| tools.as_object())
    {
        Some(active_tools) => active_tools.clone(),
        None => LEGACY_SECTION_TOOLS
            .iter()
            .flat_map(|(_, tools)| tools.iter())
            .map(|tool| (tool.to_string(), json!(is_tool_active(adjustments, tool))))
            .collect(),
    }
}

pub fn is_effect_enabled(adjustments: &serde_json::Value, tool: &str, enabled_key: &str) -> bool {
    adjustments[enabled_key].as_bool().unwrap_or(false) && is_tool_active(adjustments, tool)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PhotographicReferencePreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub adjustments: serde_json::Value,
}

/// The 8 core HDRish photographic reference presets.
pub fn get_sns_hdr_reference_presets() -> Vec<PhotographicReferencePreset> {
    vec![
        PhotographicReferencePreset {
            id: "sns-natural".to_string(),
            name: "HDRish Natural".to_string(),
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
            name: "HDRish Interior".to_string(),
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
            name: "HDRish Landscape".to_string(),
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
            name: "HDRish LDR".to_string(),
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
            name: "HDRish Night".to_string(),
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
            name: "HDRish Dramatic".to_string(),
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
            name: "HDRish Soft".to_string(),
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
            name: "HDRish Neutral".to_string(),
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
