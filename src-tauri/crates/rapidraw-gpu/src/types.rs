use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Pod, Zeroable, Default)]
#[repr(C)]
pub struct GpuPoint {
    pub x: f32,
    pub y: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Pod, Zeroable, Default)]
#[repr(C)]
pub struct GpuHslColor {
    pub hue: f32,
    pub saturation: f32,
    pub luminance: f32,
    pub _pad: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Pod, Zeroable, Default)]
#[repr(C)]
pub struct GpuColorGradeSettings {
    pub hue: f32,
    pub saturation: f32,
    pub luminance: f32,
    pub _pad: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Pod, Zeroable, Default)]
#[repr(C)]
pub struct GpuColorCalibrationSettings {
    pub shadows_tint: f32,
    pub red_hue: f32,
    pub red_saturation: f32,
    pub green_hue: f32,
    pub green_saturation: f32,
    pub blue_hue: f32,
    pub blue_saturation: f32,
    pub _pad1: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Pod, Zeroable, Default)]
#[repr(C)]
pub struct GpuMat3 {
    pub x_axis: [f32; 3],
    pub _pad0: f32,
    pub y_axis: [f32; 3],
    pub _pad1: f32,
    pub z_axis: [f32; 3],
    pub _pad2: f32,
}

/// GPU Global Adjustments uniform buffer layout for RapidRAW WGSL pipeline.
/// Strictly 16-byte aligned for std140/WGSL compatibility.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Pod, Zeroable, Default)]
#[repr(C)]
pub struct GpuGlobalAdjustments {
    pub exposure: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub saturation: f32,
    pub vibrance: f32,
    pub hue: f32,
    pub wb_log_gain_l: f32,
    pub wb_log_gain_m: f32,
    pub wb_log_gain_s: f32,
    pub centre_x: f32,
    pub centre_y: f32,

    pub sharpness: f32,
    pub luma_noise_reduction: f32,
    pub color_noise_reduction: f32,
    pub clarity: f32,
    pub dehaze: f32,
    pub structure: f32,
    pub centré: f32,
    pub vignette_amount: f32,
    pub vignette_midpoint: f32,
    pub vignette_roundness: f32,
    pub vignette_feather: f32,
    pub grain_amount: f32,
    pub grain_size: f32,
    pub grain_roughness: f32,

    pub chromatic_aberration_red_cyan: f32,
    pub chromatic_aberration_blue_yellow: f32,
    pub show_clipping: u32,
    pub is_raw_image: u32,
    pub _pad_ca1: f32,

    pub has_lut: u32,
    pub lut_intensity: f32,
    pub tonemapper_mode: u32,
    pub lut_is_scene_referred: u32,
    pub vignette_center_x: f32,
    pub vignette_center_y: f32,
    pub _pad_lut5: f32,

    pub _pad_agx1: f32,
    pub _pad_agx2: f32,
    pub _pad_agx3: f32,
    pub agx_pipe_to_rendering_matrix: GpuMat3,
    pub agx_rendering_to_pipe_matrix: GpuMat3,
    pub wb_rgb_to_lms_matrix: GpuMat3,
    pub wb_lms_to_rgb_matrix: GpuMat3,

    pub _pad_cg1: f32,
    pub _pad_cg2: f32,
    pub _pad_cg3: f32,
    pub _pad_cg4: f32,
    pub color_grading_shadows: GpuColorGradeSettings,
    pub color_grading_midtones: GpuColorGradeSettings,
    pub color_grading_highlights: GpuColorGradeSettings,
    pub color_grading_global: GpuColorGradeSettings,
    pub color_grading_blending: f32,
    pub color_grading_balance: f32,
    pub _pad2: f32,
    pub _pad3: f32,

    pub color_calibration: GpuColorCalibrationSettings,

    pub hsl: [GpuHslColor; 8],
    pub luma_curve: [GpuPoint; 16],
    pub red_curve: [GpuPoint; 16],
    pub green_curve: [GpuPoint; 16],
    pub blue_curve: [GpuPoint; 16],
    pub luma_curve_count: u32,
    pub red_curve_count: u32,
    pub green_curve_count: u32,
    pub blue_curve_count: u32,
    pub _pad_end1: f32,
    pub _pad_end2: f32,
    pub _pad_end3: f32,
    pub _pad_end4: f32,

    pub glow_amount: f32,
    pub halation_amount: f32,
    pub flare_amount: f32,
    pub sharpness_threshold: f32,

    // Phase 1: SNS-HDR Lights Sub-Pipeline & Halo-Free Contrast Cleaner (16 bytes)
    pub lights_brightness: f32,
    pub lights_contrast: f32,
    pub lights_details: f32,
    pub contrast_cleaner_strength: f32,

    // Phase 1: Lights Dedicated White Balance & Saturation (16 bytes)
    pub lights_wb_temp: f32,
    pub lights_wb_tint: f32,
    pub lights_saturation: f32,
    pub lights_vibrance: f32,

    // Phase 1: Supercontrast 3-Band Parametric Cubic Hermite Pivots (16 bytes)
    pub supercontrast_shadows: f32,
    pub supercontrast_midtones: f32,
    pub supercontrast_highlights: f32,
    pub supercontrast_pivots: f32,

    // Phase 1: Skin-Shielded Structure & Orton Soft-Light Spread (16 bytes)
    pub skin_shield_strength: f32,
    pub orton_spread: f32,
    pub foliage_enhancer: f32,
    pub _pad_phase1_end: f32,

    // Phase 1: Real-Time GPU 6th-Order Lens Distortion & AutoCrop (16 bytes)
    pub lens_k1: f32,
    pub lens_k2: f32,
    pub lens_k3: f32,
    pub lens_autocrop_scale: f32,

    // Phase 1: Lens Center & Feature Enable Flag (16 bytes)
    pub lens_center_x: f32,
    pub lens_center_y: f32,
    pub lens_distortion_enabled: u32,
    pub _pad_lens: f32,

    // Phase 3 & 4: Spatially-Varying Lens Softness & HLR (16 bytes)
    pub corner_sharpness_boost: f32,
    pub bokeh_mask_protection: f32,
    pub hlr_enabled: u32,
    pub hlr_clip_ceiling: f32,

    // Phase 5 & 6: ClearView Plus & MicroContrast V3 (16 bytes)
    pub clearview_strength: f32,
    pub clearview_t0: f32,
    pub micro_contrast: f32,
    pub _pad_reserved: f32,
}

const _: () = assert!(std::mem::size_of::<GpuGlobalAdjustments>() % 16 == 0);
