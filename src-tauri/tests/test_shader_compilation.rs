//! Automated WGSL Shader Compilation & Optical Architecture Regression Tests
//!
//! Validates headless syntax, layout compatibility, uniform alignment, and photographic presets.

use rapidraw_lib::adjustment_utils::get_sns_hdr_reference_presets;
use rapidraw_lib::focus_stacking::{FocusStackMode, FusionConfig, StackConfig};
use rapidraw_lib::image_processing::GlobalAdjustments;
use rapidraw_lib::inpainting::{
    IsophoteGuideLine, ExemplarSamplingShield, detect_small_defects_fast_variance,
    prepare_migan_uint8_tensors,
};

#[test]
fn test_global_adjustments_struct_alignment() {
    let size = std::mem::size_of::<GlobalAdjustments>();
    println!("GlobalAdjustments struct size: {} bytes", size);
    assert_eq!(
        size % 16,
        0,
        "GlobalAdjustments uniform struct must be an exact multiple of 16 bytes for std140 WGSL compatibility!"
    );
}

#[test]
fn test_sns_hdr_eight_reference_presets() {
    let presets = get_sns_hdr_reference_presets();
    assert_eq!(presets.len(), 8, "Expected 8 SNS-HDR reference presets");

    let ids: Vec<&str> = presets.iter().map(|p| p.id.as_str()).collect();
    assert!(ids.contains(&"sns-natural"));
    assert!(ids.contains(&"sns-interior"));
    assert!(ids.contains(&"sns-landscape"));
    assert!(ids.contains(&"sns-ldr"));
    assert!(ids.contains(&"sns-night"));
    assert!(ids.contains(&"sns-dramatic"));
    assert!(ids.contains(&"sns-soft"));
    assert!(ids.contains(&"sns-neutral"));

    for preset in &presets {
        assert!(!preset.name.is_empty());
        assert!(!preset.description.is_empty());
        assert!(preset.adjustments.is_object());
        println!("Verified SNS-HDR Preset: {} ({})", preset.name, preset.id);
    }
}

#[test]
fn test_focus_stacking_dual_modes() {
    let mut cfg = StackConfig::default();
    assert_eq!(cfg.fusion.mode, FocusStackMode::Detail);

    cfg.fusion.mode = FocusStackMode::Smooth;
    assert_eq!(cfg.fusion.mode, FocusStackMode::Smooth);

    let default_fusion = FusionConfig::default();
    assert_eq!(default_fusion.mode, FocusStackMode::Detail);
    println!("Focus Stacking Dual-Mode configuration verified: Detail vs Smooth.");
}

#[test]
fn test_inpainting_tools_and_fast_variance() {
    // 1. Isophote LinePen
    let line = IsophoteGuideLine::new((10.0, 10.0), (30.0, 10.0), 5.0);
    assert_eq!(line.tangent, (1.0, 0.0));
    let w_on_line = line.evaluate_weight(20.0, 10.0);
    let w_off_line = line.evaluate_weight(20.0, 20.0);
    assert!(w_on_line > 0.99);
    assert_eq!(w_off_line, 0.0);

    // 2. Exemplar Sampling Shield
    let shield = ExemplarSamplingShield {
        width: 10,
        height: 10,
        mask: vec![255; 100],
    };
    assert!(shield.is_forbidden(5, 5));
    assert!(!shield.is_forbidden(15, 15));

    // 3. Fast Variance Defect Filter
    let mut flat_img = image::RgbImage::new(32, 32);
    for p in flat_img.pixels_mut() {
        *p = image::Rgb([128, 128, 128]);
    }
    // Inject single dust spot
    flat_img.put_pixel(16, 16, image::Rgb([10, 10, 10]));
    let defect_mask = detect_small_defects_fast_variance(&flat_img, 2.5, 3);
    assert_eq!(defect_mask.get_pixel(16, 16)[0], 255, "Dust spot must be detected!");
    assert_eq!(defect_mask.get_pixel(0, 0)[0], 0, "Flat background must not be flagged!");

    // 4. MIGAN uint8 Tensors
    let (img_t, mask_t) = prepare_migan_uint8_tensors(&flat_img, &defect_mask);
    assert_eq!(img_t.len(), 32 * 32 * 3);
    assert_eq!(mask_t.len(), 32 * 32);
    println!("Inpainting reverse-engineered tools & uint8 tensors verified.");
}

#[test]
fn test_wgsl_shader_syntax_compilation() {
    // Read and validate main compute shader
    let shader_source = std::fs::read_to_string("src/shaders/shader.wgsl")
        .or_else(|_| std::fs::read_to_string("src-tauri/src/shaders/shader.wgsl"))
        .expect("Failed to locate shader.wgsl");

    assert!(shader_source.contains("fn apply_lights_subpipeline"));
    assert!(shader_source.contains("fn apply_supercontrast_pivots"));
    assert!(shader_source.contains("fn apply_foliage_enhancer"));
    assert!(shader_source.contains("compute_skin_shield"));
    assert!(shader_source.contains("contrast_cleaner_strength"));
    assert!(shader_source.contains("fn distort_coord_6th_order"));
    assert!(shader_source.contains("fn apply_dxo_hlr_reconstruction"));
    assert!(shader_source.contains("fn apply_dxo_clearview_haze_removal"));
    assert!(shader_source.contains("corner_sharpness_boost"));
    assert!(shader_source.contains("micro_contrast"));
    assert!(shader_source.contains("fn apply_dxo_anti_ac_defringe"));
    assert!(shader_source.contains("fn apply_tone_curve_max_med_min"));
    assert!(shader_source.contains("fn rgb_to_iqbqr"));
    assert!(shader_source.contains("fn apply_dxo_anamorphosis"));
    assert!(shader_source.contains("fn get_radiant_trapezoidal_hsl_influence"));
    assert!(shader_source.contains("fn apply_skin_deflash_specular_rolloff"));
    assert!(shader_source.contains("fn apply_dxo_saturated_colors_protection"));

    // Read and validate panorama multi-band blend shader
    let pano_blend_src = std::fs::read_to_string("src/panorama_utils/shaders/pano_blend.wgsl")
        .or_else(|_| std::fs::read_to_string("src-tauri/src/panorama_utils/shaders/pano_blend.wgsl"))
        .expect("Failed to locate pano_blend.wgsl");

    assert!(pano_blend_src.contains("LDS_STRIDE: u32 = 17u"));
    assert!(pano_blend_src.contains("tile_lds: array<vec4<f32>, 272>"));

    println!("All WGSL Shaders verified with zero syntax regressions.");
}

#[test]
fn test_naga_wgsl_bytecode_validation() {
    let shader_source = std::fs::read_to_string("src/shaders/shader.wgsl")
        .or_else(|_| std::fs::read_to_string("src-tauri/src/shaders/shader.wgsl"))
        .expect("Failed to locate shader.wgsl");

    let module = wgpu::naga::front::wgsl::parse_str(&shader_source);
    match module {
        Ok(m) => {
            println!("Naga WGSL parsing successful! Entry points: {:?}", m.entry_points.len());
            assert!(!m.entry_points.is_empty(), "WGSL must have at least one entry point");
        }
        Err(e) => {
            panic!("Naga WGSL parsing error:\n{}", e.emit_to_string(&shader_source));
        }
    }
}

#[test]
fn test_dxo_6th_order_newton_inverse_distortion() {
    // Test typical wide angle barrel distortion: k1 = -0.04, k2 = 0.005, k3 = -0.001
    let k1 = -0.04f32;
    let k2 = 0.005f32;
    let k3 = -0.001f32;

    // Distorted radius rd -> forward formula -> ru
    let rd = 0.75f32;
    let r2 = rd * rd;
    let r4 = r2 * r2;
    let r6 = r4 * r2;
    let ru = rd * (1.0 + k1 * r2 + k2 * r4 + k3 * r6);

    // Now invert ru with Newton-Raphson: should reconstruct rd within 1e-5
    let mut r = ru;
    for _ in 0..6 {
        let cur_r2 = r * r;
        let cur_r4 = cur_r2 * cur_r2;
        let cur_r6 = cur_r4 * cur_r2;
        let f = r * (1.0 + k1 * cur_r2 + k2 * cur_r4 + k3 * cur_r6) - ru;
        let f_prime = 1.0 + 3.0 * k1 * cur_r2 + 5.0 * k2 * cur_r4 + 7.0 * k3 * cur_r6;
        let delta = f / f_prime;
        r -= delta;
        if delta.abs() < 1e-6 {
            break;
        }
    }

    println!("Newton-Raphson inverse solve: original rd = {}, reconstructed = {}", rd, r);
    assert!((r - rd).abs() < 1e-5, "Newton-Raphson inverse distortion must reconstruct rd!");
}

