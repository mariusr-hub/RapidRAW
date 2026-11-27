use regex::Regex;
use regex::regex;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use uuid::Uuid;

use crate::decode::white_balance::{MIRED_PER_RELATIVE_UNIT, TINT_PER_RELATIVE_UNIT};
use crate::library::presets::Preset;

#[derive(Copy, Clone, Debug)]
enum Num {
    I(i64),
    F(f64),
}

fn parse_num(s: &str) -> Option<Num> {
    if let Ok(i) = s.parse::<i64>() {
        Some(Num::I(i))
    } else if let Ok(f) = s.parse::<f64>() {
        Some(Num::F(f))
    } else {
        None
    }
}

fn num_to_json(num: Num) -> Option<Value> {
    match num {
        Num::I(i) => Some(Value::Number(i.into())),
        Num::F(f) => serde_json::Number::from_f64(f).map(Value::Number),
    }
}

fn get_attr_as_f64(attrs: &HashMap<String, String>, key: &str) -> Option<f64> {
    attrs
        .get(key)
        .and_then(|s| s.trim_start_matches('+').parse::<f64>().ok())
}

fn extract_xmp_name(xmp_content: &str) -> Option<String> {
    regex!(r#"(?s)<crs:Name>.*?<rdf:Alt>.*?<rdf:li[^>]*>([^<]+)</rdf:li>.*?</crs:Name>"#)
        .captures(xmp_content)
        .and_then(|c| c.get(1).map(|m| m.as_str().trim().to_string()))
}

fn extract_tone_curve_points(xmp_str: &str, curve_name: &str) -> Option<Vec<Value>> {
    let pattern = format!(
        r"(?s)<crs:{}>\s*<rdf:Seq>(.*?)</rdf:Seq>\s*</crs:{}>",
        curve_name, curve_name
    );
    let re = Regex::new(&pattern).ok()?;
    let captures = re.captures(xmp_str)?;
    let seq_content = captures.get(1)?.as_str();

    let point_re = regex!(r"<rdf:li>(\d+),\s*(\d+)</rdf:li>");
    let mut points = Vec::new();

    for point_cap in point_re.captures_iter(seq_content) {
        let x: u32 = point_cap.get(1)?.as_str().parse().ok()?;
        let y: u32 = point_cap.get(2)?.as_str().parse().ok()?;

        let mut final_y = y;
        if curve_name == "ToneCurvePV2012" {
            const SHADOW_RANGE_END: f64 = 64.0;
            const SHADOW_DAMPEN_START: f64 = 0.8;
            const SHADOW_DAMPEN_END: f64 = 1.0;

            let x_f64 = x as f64;
            let y_f64 = y as f64;

            if y_f64 > x_f64 && x_f64 < SHADOW_RANGE_END {
                let lift_amount = y_f64 - x_f64;
                let progress = x_f64 / SHADOW_RANGE_END;
                let dampening_factor =
                    SHADOW_DAMPEN_START + (SHADOW_DAMPEN_END - SHADOW_DAMPEN_START) * progress;

                let new_y = x_f64 + (lift_amount * dampening_factor);
                final_y = new_y.round().clamp(0.0, 255.0) as u32;
            }
        }

        let mut point = Map::new();
        point.insert("x".to_string(), Value::Number(x.into()));
        point.insert("y".to_string(), Value::Number(final_y.into()));
        points.push(Value::Object(point));
    }

    if points.is_empty() {
        None
    } else {
        Some(points)
    }
}

pub fn convert_xmp_to_preset(xmp_content: &str) -> Result<Preset, String> {
    let xmp_one_line = xmp_content.split('\n').collect::<Vec<_>>().join(" ");

    let attr_re = regex!(r#"crs:([A-Za-z0-9]+)="([^"]*)""#);
    let mut attrs: HashMap<String, String> = HashMap::new();
    for cap in attr_re.captures_iter(&xmp_one_line) {
        attrs.insert(cap[1].to_string(), cap[2].to_string());
    }

    let mut curves_map = Map::new();
    let curve_mappings = [
        ("ToneCurvePV2012", "luma"),
        ("ToneCurvePV2012Red", "red"),
        ("ToneCurvePV2012Green", "green"),
        ("ToneCurvePV2012Blue", "blue"),
    ];
    for (xmp_curve, rr_curve) in curve_mappings {
        if let Some(points) = extract_tone_curve_points(xmp_content, xmp_curve) {
            curves_map.insert(rr_curve.to_string(), Value::Array(points));
        }
    }

    let preset_name =
        extract_xmp_name(xmp_content).unwrap_or_else(|| "Imported Preset".to_string());

    build_preset_from_maps(preset_name, &attrs, curves_map)
}

fn dampen_pv2012_y(curve_name: &str, x: u32, y: u32) -> u32 {
    if curve_name == "ToneCurvePV2012" {
        const SHADOW_RANGE_END: f64 = 64.0;
        const SHADOW_DAMPEN_START: f64 = 0.8;
        const SHADOW_DAMPEN_END: f64 = 1.0;

        let x_f64 = x as f64;
        let y_f64 = y as f64;
        if y_f64 > x_f64 && x_f64 < SHADOW_RANGE_END {
            let lift_amount = y_f64 - x_f64;
            let progress = x_f64 / SHADOW_RANGE_END;
            let dampening_factor =
                SHADOW_DAMPEN_START + (SHADOW_DAMPEN_END - SHADOW_DAMPEN_START) * progress;
            return (x_f64 + lift_amount * dampening_factor)
                .round()
                .clamp(0.0, 255.0) as u32;
        }
    }
    y
}

fn build_preset_from_maps(
    preset_name: String,
    attrs: &HashMap<String, String>,
    curves_map: Map<String, Value>,
) -> Result<Preset, String> {
    let mut adjustments = Map::new();
    let mut hsl_map = Map::new();
    let mut color_grading_map = Map::new();

    let mappings = vec![
        ("Exposure2012", "exposure"),
        ("Contrast2012", "contrast"),
        ("Highlights2012", "highlights"),
        ("Whites2012", "whites"),
        ("Blacks2012", "blacks"),
        ("Clarity2012", "clarity"),
        ("Dehaze", "dehaze"),
        ("Vibrance", "vibrance"),
        ("Saturation", "saturation"),
        ("Texture", "structure"),
        ("SharpenRadius", "sharpenRadius"),
        ("SharpenDetail", "sharpenDetail"),
        ("SharpenEdgeMasking", "sharpenMasking"),
        ("LuminanceSmoothing", "lumaNoiseReduction"),
        ("ColorNoiseReduction", "colorNoiseReduction"),
        ("ColorNoiseReductionDetail", "colorNoiseDetail"),
        ("ColorNoiseReductionSmoothness", "colorNoiseSmoothness"),
        ("ChromaticAberrationRedCyan", "chromaticAberrationRedCyan"),
        (
            "ChromaticAberrationBlueYellow",
            "chromaticAberrationBlueYellow",
        ),
        ("PostCropVignetteAmount", "vignetteAmount"),
        ("PostCropVignetteMidpoint", "vignetteMidpoint"),
        ("PostCropVignetteFeather", "vignetteFeather"),
        ("PostCropVignetteRoundness", "vignetteRoundness"),
        ("GrainAmount", "grainAmount"),
        ("GrainSize", "grainSize"),
        ("GrainFrequency", "grainRoughness"),
        ("ColorGradeBlending", "blending"),
    ];

    for (xmp_key, rr_key) in mappings {
        if let Some(raw_val) = attrs.get(xmp_key)
            && let Some(num) = parse_num(raw_val.trim_start_matches('+'))
            && let Some(json_val) = num_to_json(num)
        {
            if rr_key == "blending" {
                color_grading_map.insert(rr_key.to_string(), json_val);
            } else {
                adjustments.insert(rr_key.to_string(), json_val);
            }
        }
    }

    if let Some(shadows_val) = get_attr_as_f64(attrs, "Shadows2012") {
        let adjusted_shadows = (shadows_val * 1.5).clamp(-100.0, 100.0);
        adjustments.insert("shadows".to_string(), json!(adjusted_shadows));
    }

    if let Some(sharpness_val) = get_attr_as_f64(attrs, "Sharpness") {
        let scaled_sharpness = (sharpness_val / 150.0) * 100.0;
        adjustments.insert(
            "sharpness".to_string(),
            json!(scaled_sharpness.clamp(0.0, 100.0)),
        );
    }

    if let Some(adjusted_k) = get_attr_as_f64(attrs, "Temperature") {
        const AS_SHOT_DEFAULT: f64 = 5500.0;
        let as_shot_k = get_attr_as_f64(attrs, "AsShotTemperature").unwrap_or(AS_SHOT_DEFAULT);
        let mired_adjusted = 1_000_000.0 / adjusted_k;
        let mired_as_shot = 1_000_000.0 / as_shot_k;
        let mired_delta = mired_adjusted - mired_as_shot;
        let temp_value = -mired_delta / MIRED_PER_RELATIVE_UNIT;
        adjustments.insert(
            "temperature".to_string(),
            json!(temp_value.clamp(-100.0, 100.0)),
        );
    }

    if let Some(tint_val) = get_attr_as_f64(attrs, "Tint") {
        let scaled_tint = tint_val / TINT_PER_RELATIVE_UNIT;
        adjustments.insert("tint".to_string(), json!(scaled_tint.clamp(-100.0, 100.0)));
    }

    // Parametric Tone Curve (ParametricDarks / ParametricLights / ParametricShadows / ParametricHighlights)
    let mut param_luma = Map::new();
    for (src_key, dst_key) in [
        ("ParametricDarks", "darks"),
        ("ParametricShadows", "shadows"),
        ("ParametricLights", "lights"),
        ("ParametricHighlights", "highlights"),
    ] {
        if let Some(val) = get_attr_as_f64(attrs, src_key) {
            param_luma.insert(dst_key.to_string(), json!(val.clamp(-100.0, 100.0)));
        }
    }
    if !param_luma.is_empty() {
        param_luma.entry("darks").or_insert_with(|| json!(0));
        param_luma.entry("shadows").or_insert_with(|| json!(0));
        param_luma.entry("lights").or_insert_with(|| json!(0));
        param_luma.entry("highlights").or_insert_with(|| json!(0));
        param_luma.insert("whiteLevel".to_string(), json!(100));
        param_luma.insert("blackLevel".to_string(), json!(0));
        param_luma.insert("split1".to_string(), json!(25));
        param_luma.insert("split2".to_string(), json!(50));
        param_luma.insert("split3".to_string(), json!(75));
        let default_ch = json!({
            "darks": 0, "shadows": 0, "lights": 0, "highlights": 0,
            "whiteLevel": 100, "blackLevel": 0, "split1": 25, "split2": 50, "split3": 75
        });
        adjustments.insert(
            "parametricCurve".to_string(),
            json!({
                "luma": Value::Object(param_luma),
                "red": default_ch,
                "green": default_ch,
                "blue": default_ch,
            }),
        );
    }

    // Camera Calibration (ShadowTint + Red/Green/Blue primaries)
    let mut calib_map = Map::new();
    for (src_key, dst_key) in [
        ("ShadowTint", "shadowsTint"),
        ("RedHue", "redHue"),
        ("RedSaturation", "redSaturation"),
        ("GreenHue", "greenHue"),
        ("GreenSaturation", "greenSaturation"),
        ("BlueHue", "blueHue"),
        ("BlueSaturation", "blueSaturation"),
    ] {
        if let Some(val) = get_attr_as_f64(attrs, src_key) {
            calib_map.insert(dst_key.to_string(), json!(val.clamp(-100.0, 100.0)));
        }
    }
    if !calib_map.is_empty() {
        adjustments.insert("colorCalibration".to_string(), Value::Object(calib_map));
    }

    let colors = [
        ("Red", "reds"),
        ("Orange", "oranges"),
        ("Yellow", "yellows"),
        ("Green", "greens"),
        ("Aqua", "aquas"),
        ("Blue", "blues"),
        ("Purple", "purples"),
        ("Magenta", "magentas"),
    ];
    for (src, dst) in colors {
        let mut color_map = Map::new();
        if let Some(raw) = attrs.get(&format!("HueAdjustment{}", src))
            && let Some(num) = parse_num(raw.trim_start_matches('+'))
            && let Some(Value::Number(n)) = num_to_json(num)
            && let Some(val_f64) = n.as_f64()
        {
            color_map.insert("hue".to_string(), json!(val_f64 * 0.75));
        }
        if let Some(raw) = attrs.get(&format!("SaturationAdjustment{}", src))
            && let Some(num) = parse_num(raw.trim_start_matches('+'))
            && let Some(json_val) = num_to_json(num)
        {
            color_map.insert("saturation".to_string(), json_val);
        }
        if let Some(raw) = attrs.get(&format!("LuminanceAdjustment{}", src))
            && let Some(num) = parse_num(raw.trim_start_matches('+'))
            && let Some(json_val) = num_to_json(num)
        {
            color_map.insert("luminance".to_string(), json_val);
        }
        if !color_map.is_empty() {
            hsl_map.insert(dst.to_string(), Value::Object(color_map));
        }
    }
    if !hsl_map.is_empty() {
        adjustments.insert("hsl".to_string(), Value::Object(hsl_map));
    }

    let grayscale = attrs
        .get("ConvertToGrayscale")
        .map(|v| v.eq_ignore_ascii_case("true"));
    if let Some(is_monochrome) = grayscale {
        adjustments.insert("monochromeEnabled".to_string(), json!(is_monochrome));
    }
    if grayscale == Some(true) {
        let mut mix_map = Map::new();
        for (src, dst) in colors {
            if let Some(val) = get_attr_as_f64(attrs, &format!("GrayMixer{}", src)) {
                mix_map.insert(dst.to_string(), json!(val.clamp(-100.0, 100.0)));
            }
        }
        if !mix_map.is_empty() {
            adjustments.insert("monochromeMix".to_string(), Value::Object(mix_map));
        }

        let mut toning_map = Map::new();
        for (xmp_key, rr_key) in [
            ("SplitToningShadowHue", "shadowHue"),
            ("SplitToningShadowSaturation", "shadowSaturation"),
            ("SplitToningHighlightHue", "highlightHue"),
            ("SplitToningHighlightSaturation", "highlightSaturation"),
            ("SplitToningBalance", "balance"),
        ] {
            if let Some(val) = get_attr_as_f64(attrs, xmp_key) {
                toning_map.insert(rr_key.to_string(), json!(val));
            }
        }
        if !toning_map.is_empty() {
            adjustments.insert("monochromeToning".to_string(), Value::Object(toning_map));
        }
    }

    let mut shadows_map = Map::new();
    let mut midtones_map = Map::new();
    let mut highlights_map = Map::new();
    let mut global_map = Map::new();
    for (src_key, dst_key) in [
        ("SplitToningShadowHue", "hue"),
        ("SplitToningShadowSaturation", "saturation"),
        ("ColorGradeShadowLum", "luminance"),
    ] {
        if let Some(raw) = attrs.get(src_key)
            && let Some(num) = parse_num(raw)
            && let Some(json_val) = num_to_json(num)
        {
            shadows_map.insert(dst_key.to_string(), json_val);
        }
    }
    for (src_key, dst_key) in [
        ("ColorGradeMidtoneHue", "hue"),
        ("ColorGradeMidtoneSat", "saturation"),
        ("ColorGradeMidtoneLum", "luminance"),
    ] {
        if let Some(raw) = attrs.get(src_key)
            && let Some(num) = parse_num(raw)
            && let Some(json_val) = num_to_json(num)
        {
            midtones_map.insert(dst_key.to_string(), json_val);
        }
    }
    for (src_key, dst_key) in [
        ("SplitToningHighlightHue", "hue"),
        ("SplitToningHighlightSaturation", "saturation"),
        ("ColorGradeHighlightLum", "luminance"),
    ] {
        if let Some(raw) = attrs.get(src_key)
            && let Some(num) = parse_num(raw)
            && let Some(json_val) = num_to_json(num)
        {
            highlights_map.insert(dst_key.to_string(), json_val);
        }
    }
    for (src_key, dst_key) in [
        ("ColorGradeGlobalHue", "hue"),
        ("ColorGradeGlobalSat", "saturation"),
        ("ColorGradeGlobalLum", "luminance"),
    ] {
        if let Some(raw) = attrs.get(src_key)
            && let Some(num) = parse_num(raw)
            && let Some(json_val) = num_to_json(num)
        {
            global_map.insert(dst_key.to_string(), json_val);
        }
    }
    if let Some(raw) = attrs.get("SplitToningBalance")
        && let Some(num) = parse_num(raw)
        && let Some(json_val) = num_to_json(num)
    {
        color_grading_map.insert("balance".to_string(), json_val);
    }
    if !shadows_map.is_empty() {
        color_grading_map.insert("shadows".to_string(), Value::Object(shadows_map));
    }
    if !midtones_map.is_empty() {
        color_grading_map.insert("midtones".to_string(), Value::Object(midtones_map));
    }
    if !highlights_map.is_empty() {
        color_grading_map.insert("highlights".to_string(), Value::Object(highlights_map));
    }
    if !global_map.is_empty() {
        color_grading_map.insert("global".to_string(), Value::Object(global_map));
    }
    if !color_grading_map.is_empty() {
        adjustments.insert("colorGrading".to_string(), Value::Object(color_grading_map));
    }

    if !curves_map.is_empty() {
        adjustments.insert("curves".to_string(), Value::Object(curves_map));
    }

    Ok(Preset {
        id: Uuid::new_v4().to_string(),
        name: preset_name,
        adjustments: Value::Object(adjustments),
        include_masks: Some(false),
        include_crop_transform: Some(false),
        preset_type: Some("style".to_string()),
    })
}

/// Parses a Lightroom Lua `.lrtemplate` (`s = { title = "...", value = { settings = { ... } } }`) preset.
pub fn convert_lrtemplate_to_preset(
    content: &str,
    fallback_name: &str,
) -> Result<Preset, String> {
    let mut attrs: HashMap<String, String> = HashMap::new();
    let mut curves_map = Map::new();

    let preset_name = regex!(r#"(?m)\btitle\s*=\s*"([^"]+)""#)
        .captures(content)
        .and_then(|c| c.get(1).map(|m| m.as_str().trim().to_string()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_name.to_string());

    // Scalar key = value pairs (numbers, booleans, or quoted strings)
    let scalar_re = regex!(r#"(?m)^\s*([A-Za-z0-9_]+)\s*=\s*([+-]?\d+(?:\.\d+)?|true|false|"[^"]*"),?"#);
    for cap in scalar_re.captures_iter(content) {
        let key = cap[1].to_string();
        let val = cap[2].trim_matches('"').to_string();
        attrs.insert(key, val);
    }

    // ToneCurvePV2012 flat arrays: ToneCurvePV2012 = { 0, 0, 64, 72, 255, 255 }
    for (lua_key, rr_curve) in [
        ("ToneCurvePV2012", "luma"),
        ("ToneCurvePV2012Red", "red"),
        ("ToneCurvePV2012Green", "green"),
        ("ToneCurvePV2012Blue", "blue"),
    ] {
        let pattern = format!(r"(?s)\b{}\s*=\s*\{{([^}}]*)\}}", regex::escape(lua_key));
        if let Ok(re) = Regex::new(&pattern)
            && let Some(caps) = re.captures(content)
        {
            let nums: Vec<u32> = caps[1]
                .split([',', ' ', '\t', '\r', '\n'])
                .filter_map(|tok| tok.trim().parse::<f64>().ok())
                .map(|v| v.round().clamp(0.0, 255.0) as u32)
                .collect();
            let mut pts = Vec::new();
            for pair in nums.chunks_exact(2) {
                let x = pair[0];
                let y = dampen_pv2012_y(lua_key, x, pair[1]);
                pts.push(json!({ "x": x, "y": y }));
            }
            if !pts.is_empty() {
                curves_map.insert(rr_curve.to_string(), Value::Array(pts));
            }
        }
    }

    if attrs.is_empty() && curves_map.is_empty() {
        return Err("Unrecognized .lrtemplate preset format (no settings found)".to_string());
    }

    build_preset_from_maps(preset_name, &attrs, curves_map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_lrtemplate_parses_scalars_curves_parametric_and_calibration() {
        let sample = r#"
s = {
    id = "ABC-123",
    internalName = "Portra Warm",
    title = "Portra 400 Warm",
    type = "Develop",
    value = {
        settings = {
            Exposure2012 = 0.35,
            Contrast2012 = 12,
            ParametricDarks = -8,
            ParametricShadows = 10,
            ParametricLights = 5,
            ParametricHighlights = -15,
            ShadowTint = 4,
            RedHue = 6,
            RedSaturation = -10,
            BlueHue = -12,
            BlueSaturation = 14,
            ToneCurvePV2012 = {
                0, 12,
                64, 72,
                255, 245,
            },
        },
        uuid = "ABC-123",
    },
    version = 0,
}
"#;
        let preset = convert_lrtemplate_to_preset(sample, "fallback").expect("lrtemplate parse");
        assert_eq!(preset.name, "Portra 400 Warm");
        assert!((preset.adjustments["exposure"].as_f64().unwrap() - 0.35).abs() < 1e-4);
        assert_eq!(preset.adjustments["parametricCurve"]["luma"]["darks"], json!(-8.0));
        assert_eq!(preset.adjustments["parametricCurve"]["luma"]["highlights"], json!(-15.0));
        assert_eq!(preset.adjustments["colorCalibration"]["shadowsTint"], json!(4.0));
        assert_eq!(preset.adjustments["colorCalibration"]["redHue"], json!(6.0));
        assert_eq!(preset.adjustments["colorCalibration"]["blueSaturation"], json!(14.0));
        assert_eq!(preset.adjustments["curves"]["luma"].as_array().unwrap().len(), 3);
    }
}
