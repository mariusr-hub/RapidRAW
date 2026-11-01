struct Point {
    x: f32,
    y: f32,
    _pad1: f32,
    _pad2: f32,
}

struct HslColor {
    hue: f32,
    saturation: f32,
    luminance: f32,
    _pad: f32,
}

struct ColorGradeSettings {
    hue: f32,
    saturation: f32,
    luminance: f32,
    _pad: f32,
}

struct ColorCalibrationSettings {
    shadows_tint: f32,
    red_hue: f32,
    red_saturation: f32,
    green_hue: f32,
    green_saturation: f32,
    blue_hue: f32,
    blue_saturation: f32,
    _pad1: f32,
}

struct GlobalAdjustments {
    exposure: f32,
    brightness: f32,
    contrast: f32,
    highlights: f32,
    shadows: f32,
    whites: f32,
    blacks: f32,
    saturation: f32,
    vibrance: f32,
    hue: f32,
    wb_log_gain_l: f32,
    wb_log_gain_m: f32,
    wb_log_gain_s: f32,
    centre_x: f32,
    centre_y: f32,

    sharpness: f32,
    luma_noise_reduction: f32,
    color_noise_reduction: f32,
    clarity: f32,
    dehaze: f32,
    structure: f32,
    centre: f32,
    vignette_amount: f32,
    vignette_midpoint: f32,
    vignette_roundness: f32,
    vignette_feather: f32,
    grain_amount: f32,
    grain_size: f32,
    grain_roughness: f32,

    chromatic_aberration_red_cyan: f32,
    chromatic_aberration_blue_yellow: f32,
    show_clipping: u32,
    is_raw_image: u32,
    _pad_ca1: f32,

    has_lut: u32,
    lut_intensity: f32,
    tonemapper_mode: u32,
    lut_is_scene_referred: u32,
    vignette_center_x: f32,
    vignette_center_y: f32,
    _pad_lut5: f32,

    _pad_agx1: f32,
    _pad_agx2: f32,
    _pad_agx3: f32,
    agx_pipe_to_rendering_matrix: mat3x3<f32>,
    agx_rendering_to_pipe_matrix: mat3x3<f32>,
    wb_rgb_to_lms_matrix: mat3x3<f32>,
    wb_lms_to_rgb_matrix: mat3x3<f32>,

    _pad_cg1: f32,
    _pad_cg2: f32,
    _pad_cg3: f32,
    _pad_cg4: f32,
    color_grading_shadows: ColorGradeSettings,
    color_grading_midtones: ColorGradeSettings,
    color_grading_highlights: ColorGradeSettings,
    color_grading_global: ColorGradeSettings,
    color_grading_blending: f32,
    color_grading_balance: f32,
    _pad2: f32,
    _pad3: f32,

    color_calibration: ColorCalibrationSettings,

    hsl: array<HslColor, 8>,
    luma_curve: array<Point, 16>,
    red_curve: array<Point, 16>,
    green_curve: array<Point, 16>,
    blue_curve: array<Point, 16>,
    luma_curve_count: u32,
    red_curve_count: u32,
    green_curve_count: u32,
    blue_curve_count: u32,
    _pad_end1: f32,
    _pad_end2: f32,
    _pad_end3: f32,
    _pad_end4: f32,

    glow_amount: f32,
    halation_amount: f32,
    flare_amount: f32,
    sharpness_threshold: f32,
    // Phase 1: SNS-HDR Lights Sub-Pipeline & Halo-Free Contrast Cleaner (16 bytes)
    lights_brightness: f32,
    lights_contrast: f32,
    lights_details: f32,
    contrast_cleaner_strength: f32,

    // Phase 1: Lights Dedicated White Balance & Saturation (16 bytes)
    lights_wb_temp: f32,
    lights_wb_tint: f32,
    lights_saturation: f32,
    lights_vibrance: f32,

    // Phase 1: Supercontrast 3-Band Parametric Cubic Hermite Pivots (16 bytes)
    supercontrast_shadows: f32,
    supercontrast_midtones: f32,
    supercontrast_highlights: f32,
    supercontrast_pivots: f32,

    // Phase 1: Skin-Shielded Structure & Orton Soft-Light Spread (16 bytes)
    skin_shield_strength: f32,
    orton_spread: f32,
    foliage_enhancer: f32,
    _pad_phase1_end: f32,

    // Phase 1: Real-Time GPU 6th-Order Lens Distortion & AutoCrop (16 bytes)
    lens_k1: f32,
    lens_k2: f32,
    lens_k3: f32,
    lens_autocrop_scale: f32,

    // Phase 1: Lens Center & Feature Enable Flag (16 bytes)
    lens_center_x: f32,
    lens_center_y: f32,
    lens_distortion_enabled: u32,
    _pad_lens: f32,

    // Phase 3 & 4: Spatially-Varying Lens Softness & HLR (16 bytes)
    corner_sharpness_boost: f32,
    bokeh_mask_protection: f32,
    hlr_enabled: u32,
    hlr_clip_ceiling: f32,

    // Phase 5 & 6: ClearView Plus & MicroContrast V3 (16 bytes)
    clearview_strength: f32,
    clearview_t0: f32,
    micro_contrast: f32,
    _pad_reserved: f32,
}

struct MaskAdjustments {
    exposure: f32,
    brightness: f32,
    contrast: f32,
    highlights: f32,
    shadows: f32,
    whites: f32,
    blacks: f32,
    saturation: f32,
    vibrance: f32,

    sharpness: f32,
    luma_noise_reduction: f32,
    color_noise_reduction: f32,
    clarity: f32,
    dehaze: f32,
    structure: f32,

    glow_amount: f32,
    halation_amount: f32,
    flare_amount: f32,
    sharpness_threshold: f32,

    hue: f32,
    wb_log_gain_l: f32,
    wb_log_gain_m: f32,
    wb_log_gain_s: f32,
    _pad_wb: f32,
    color_grading_shadows: ColorGradeSettings,
    color_grading_midtones: ColorGradeSettings,
    color_grading_highlights: ColorGradeSettings,
    color_grading_global: ColorGradeSettings,
    color_grading_blending: f32,
    color_grading_balance: f32,
    _pad5: f32,
    _pad6: f32,

    hsl: array<HslColor, 8>,
    luma_curve: array<Point, 16>,
    red_curve: array<Point, 16>,
    green_curve: array<Point, 16>,
    blue_curve: array<Point, 16>,
    luma_curve_count: u32,
    red_curve_count: u32,
    green_curve_count: u32,
    blue_curve_count: u32,
    _pad_end4: f32,
    _pad_end5: f32,
    _pad_end6: f32,
    _pad_end7: f32,
}

struct AllAdjustments {
    global: GlobalAdjustments,
    mask_adjustments: array<MaskAdjustments, 32>,
    mask_count: u32,
    tile_offset_x: u32,
    tile_offset_y: u32,
    mask_atlas_cols: u32,
}

struct HslRange {
    center: f32,
    width: f32,
}

const HSL_RANGES: array<HslRange, 8> = array<HslRange, 8>(
    HslRange(358.0, 35.0),  // Red
    HslRange(25.0, 45.0),   // Orange
    HslRange(60.0, 40.0),   // Yellow
    HslRange(115.0, 90.0),  // Green
    HslRange(180.0, 60.0),  // Aqua
    HslRange(225.0, 60.0),  // Blue
    HslRange(280.0, 55.0),  // Purple
    HslRange(330.0, 50.0)   // Magenta
);

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var output_texture: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<storage, read> adjustments: AllAdjustments;

@group(0) @binding(3) var mask_textures: texture_2d_array<f32>;

@group(0) @binding(4) var lut_texture: texture_3d<f32>;
@group(0) @binding(5) var lut_sampler: sampler;

@group(0) @binding(6) var sharpness_blur_texture: texture_2d<f32>;
@group(0) @binding(7) var tonal_blur_texture: texture_2d<f32>;
@group(0) @binding(8) var clarity_blur_texture: texture_2d<f32>;
@group(0) @binding(9) var structure_blur_texture: texture_2d<f32>;

@group(0) @binding(10) var flare_texture: texture_2d<f32>;
@group(0) @binding(11) var flare_sampler: sampler;

@group(0) @binding(12) var gf_coeffs_texture: texture_2d<f32>;
@group(0) @binding(13) var gf_dehaze_texture: texture_2d<f32>;

const GF_LUMA_FLOOR: f32 = 1.0e-4;
const GF_DETAIL_SIGMA: f32 = 1.5;
const SHARPEN_EDGE_SIGMA: f32 = 0.12;
const SHARPEN_DARK_SCALE: f32 = 0.55;
const GF_REINJECT_SIGMA: f32 = 0.8;
const DEHAZE_STRENGTH: f32 = 1.5;
const GF_CLARITY_GAIN: f32 = 1.8;
const GF_STRUCTURE_GAIN: f32 = 1.8;
const GF_NEG_GAIN: f32 = 1.0;
const GF_BRIGHT_SIGMA: f32 = 0.8;
const GF_NOISE_LO: f32 = 0.0;
const GF_NOISE_HI: f32 = 0.01;

override HIGH_PRECISION_OUTPUT: u32 = 0u;

const LUMA_COEFF = vec3<f32>(0.2126, 0.7152, 0.0722);

fn get_luma(c: vec3<f32>) -> f32 {
    return dot(c, LUMA_COEFF);
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let cutoff = vec3<f32>(0.04045);
    let a = vec3<f32>(0.055);
    let higher = pow((c + a) / (1.0 + a), vec3<f32>(2.4));
    let lower = c / 12.92;
    return select(higher, lower, c <= cutoff);
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let safe_c = max(c, vec3<f32>(0.0));
    let cutoff = vec3<f32>(0.0031308);
    let a = vec3<f32>(0.055);
    let higher = (1.0 + a) * pow(safe_c, vec3<f32>(1.0 / 2.4)) - a;
    let lower = safe_c * 12.92;
    return select(higher, lower, safe_c <= cutoff);
}

fn linear_to_srgb_extended(c: vec3<f32>) -> vec3<f32> {
    let safe_c = max(c, vec3<f32>(0.0));
    let cutoff = vec3<f32>(0.0031308);
    let a = vec3<f32>(0.055);
    let higher = (1.0 + a) * pow(safe_c, vec3<f32>(1.0 / 2.4)) - a;
    let lower = safe_c * 12.92;
    return select(higher, lower, safe_c <= cutoff);
}

fn linear_to_vlog(c: vec3<f32>) -> vec3<f32> {
    let safe_c = max(c, vec3<f32>(0.0));
    let low = 5.6 * safe_c + 0.125;
    let log10_val = log2(safe_c + 0.00873) * 0.30102999566398;
    let high = 0.241514 * log10_val + 0.598206;
    return select(high, low, safe_c <= vec3<f32>(0.01));
}

fn rgb_to_hsv(c: vec3<f32>) -> vec3<f32> {
    let c_max = max(c.r, max(c.g, c.b));
    let c_min = min(c.r, min(c.g, c.b));
    let delta = c_max - c_min;
    var h: f32 = 0.0;
    if (delta > 0.0) {
        if (c_max == c.r) { h = 60.0 * (((c.g - c.b) / delta) % 6.0); }
        else if (c_max == c.g) { h = 60.0 * (((c.b - c.r) / delta) + 2.0); }
        else { h = 60.0 * (((c.r - c.g) / delta) + 4.0); }
    }
    if (h < 0.0) { h += 360.0; }
    let s = select(0.0, delta / c_max, c_max > 0.0);
    return vec3<f32>(h, s, c_max);
}

fn hsv_to_rgb(c: vec3<f32>) -> vec3<f32> {
    let h = c.x; let s = c.y; let v = c.z;
    let C = v * s;
    let X = C * (1.0 - abs((h / 60.0) % 2.0 - 1.0));
    let m = v - C;
    var rgb_prime: vec3<f32>;
    if (h < 60.0) { rgb_prime = vec3<f32>(C, X, 0.0); }
    else if (h < 120.0) { rgb_prime = vec3<f32>(X, C, 0.0); }
    else if (h < 180.0) { rgb_prime = vec3<f32>(0.0, C, X); }
    else if (h < 240.0) { rgb_prime = vec3<f32>(0.0, X, C); }
    else if (h < 300.0) { rgb_prime = vec3<f32>(X, 0.0, C); }
    else { rgb_prime = vec3<f32>(C, 0.0, X); }
    return rgb_prime + vec3<f32>(m, m, m);
}

fn apply_hue_shift(color: vec3<f32>, shift_degrees: f32) -> vec3<f32> {
    if (abs(shift_degrees) < 0.01) {
        return color;
    }
    let srgb_color = linear_to_srgb_extended(color);
    let hsv = rgb_to_hsv(srgb_color);
    var shifted_h = hsv.x + shift_degrees;
    shifted_h = (shifted_h + 360.0) % 360.0;
    let shifted_srgb = hsv_to_rgb(vec3<f32>(shifted_h, hsv.y, hsv.z));
    return srgb_to_linear(shifted_srgb);
}

// Perceptual 360° Circular 4-Angle Trapezoidal HSL Membership Function (0x34830)
fn get_trapezoidal_hsl_influence(hue: f32, center: f32, width: f32) -> f32 {
    let diff = abs(hue - center);
    let d = fract(diff / 360.0) * 360.0;
    let dist = min(d, 360.0 - d);
    let half_w = width * 0.5;
    let half_core = half_w * 0.35; // Flat plateau across core 35% of band
    if (dist <= half_core) {
        return 1.0;
    }
    if (dist >= half_w) {
        return 0.0;
    }
    return 1.0 - (dist - half_core) / (half_w - half_core);
}

fn get_raw_hsl_influence(hue: f32, center: f32, width: f32) -> f32 {
    return get_trapezoidal_hsl_influence(hue, center, width);
}

// 1. Midrange-Normalized Color Cast Removal & Gamut-Preserving Highlight De-Clip (PerfectlyClear 0x22330)
fn apply_subtractive_tint_and_gamut_declip(rgb: vec3<f32>, raw_gains: vec3<f32>, tint_scale: f32, white_ceiling: f32) -> vec3<f32> {
    let g = vec3<f32>(1.0) + tint_scale * (raw_gains - vec3<f32>(1.0));
    let g_min = min(g.r, min(g.g, g.b));
    let g_max = max(g.r, max(g.g, g.b));
    let midrange_gain = max(0.5 * (g_min + g_max), 1e-5);
    var c = max(rgb * (g / midrange_gain), vec3<f32>(0.0));

    // Radial Midrange Highlight De-Clip (100% hue-angle invariant)
    let c_max = max(c.r, max(c.g, c.b));
    if (c_max > white_ceiling) {
        let c_min = min(c.r, min(c.g, c.b));
        let l_mid = 0.5 * (c_max + c_min);
        let span = max(c_max - l_mid, 1e-5);
        let k_gamut = clamp((white_ceiling - l_mid) / span, 0.0, 1.0);
        c = vec3<f32>(l_mid) + (c - vec3<f32>(l_mid)) * k_gamut;
    }
    return c;
}

// 2. Base-10 Exponential Pivot Tone Curve + Highlight Preservation Power-Law (PerfectlyClear 0x3cbe0 & 0x1f8ba)
fn evaluate_dynamic_pivot_tone(x_in: f32, pivot: f32, strength_0_1: f32, highlight_pres_0_100: f32) -> f32 {
    let x = clamp(x_in, 0.0, 1.0);
    let p = clamp(pivot, 0.05, 0.95);
    var y = x;
    if (x <= p) {
        let toe = p * ((pow(10.0, x / p) - 1.0) / 9.0);
        y = mix(x, toe, strength_0_1);
    } else {
        let gamma = 9.0 / (1.0 - p);
        let num = 1.0 - pow(10.0, -gamma * (x - p));
        let den = max(1.0 - pow(10.0, -gamma * (1.0 - p)), 1e-5);
        let shoulder = p + (1.0 - p) * (num / den);
        y = mix(x, shoulder, strength_0_1);
    }
    if (highlight_pres_0_100 > 0.0) {
        let k_pres = 1.0 + exp2((100.0 - clamp(highlight_pres_0_100, 0.0, 100.0)) / 27.0);
        let preserve_w = pow(x, k_pres);
        y = mix(y, x, preserve_w * (highlight_pres_0_100 / 100.0));
    }
    return clamp(y, 0.0, 1.0);
}

// 3. Max-Channel Ratio RGB Reconstruction + Subtractive Film-Density Vibrancy (PerfectlyClear 0x21f15 & general_vibrancy.cube)
fn apply_max_channel_tone_and_subtractive_vibrancy(rgb: vec3<f32>, v_new: f32, vibrancy_0_1: f32) -> vec3<f32> {
    let v_old = max(rgb.r, max(rgb.g, rgb.b));
    var out_rgb = rgb;
    if (v_old > 1e-5) {
        out_rgb = rgb * (v_new / v_old);
    }
    // Subtractive color density: deepen saturated colors without shifting neutrals
    let max_c = max(out_rgb.r, max(out_rgb.g, out_rgb.b));
    let min_c = min(out_rgb.r, min(out_rgb.g, out_rgb.b));
    let sat = (max_c - min_c) / max(max_c, 1e-4);
    let density_pull = 1.0 - 0.065 * vibrancy_0_1 * sat;
    let luma = 0.5 * (max_c + min_c);
    out_rgb = (vec3<f32>(luma) + (out_rgb - vec3<f32>(luma)) * (1.0 + 0.35 * vibrancy_0_1 * (1.0 - sat * 0.5))) * density_pull;
    return clamp(out_rgb, vec3<f32>(0.0), vec3<f32>(1.0));
}

// 4. 4-Angle Circular Trapezoidal Hue Window Weight (PerfectlyClear 0x3bb00)
fn trapezoidal_hue_weight(hue_deg: f32, h1: f32, h2: f32, h3: f32, h4: f32) -> f32 {
    let d2 = (h2 - h1 + 360.0) % 360.0;
    let d3 = (h3 - h1 + 360.0) % 360.0;
    let d4 = (h4 - h1 + 360.0) % 360.0;
    let k  = (hue_deg - h1 + 360.0) % 360.0;
    if (k >= d4 || d2 <= 0.0 || d3 <= d2 || d4 <= d3) {
        return 0.0;
    }
    if (k < d2) {
        return k / d2;
    }
    if (k <= d3) {
        return 1.0;
    }
    return 1.0 - (k - d3) / (d4 - d3);
}

fn hash(p: vec2<f32>) -> f32 {
    var p3  = fract(vec3<f32>(p.xyx) * .1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn gradient_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);

    let ga = vec2<f32>(hash(i + vec2(0.0, 0.0)), hash(i + vec2(0.0, 0.0) + vec2(11.0, 37.0))) * 2.0 - 1.0;
    let gb = vec2<f32>(hash(i + vec2(1.0, 0.0)), hash(i + vec2(1.0, 0.0) + vec2(11.0, 37.0))) * 2.0 - 1.0;
    let gc = vec2<f32>(hash(i + vec2(0.0, 1.0)), hash(i + vec2(0.0, 1.0) + vec2(11.0, 37.0))) * 2.0 - 1.0;
    let gd = vec2<f32>(hash(i + vec2(1.0, 1.0)), hash(i + vec2(1.0, 1.0) + vec2(11.0, 37.0))) * 2.0 - 1.0;

    let dot_00 = dot(ga, f - vec2(0.0, 0.0));
    let dot_10 = dot(gb, f - vec2(1.0, 0.0));
    let dot_01 = dot(gc, f - vec2(0.0, 1.0));
    let dot_11 = dot(gd, f - vec2(1.0, 1.0));

    let bottom_interp = mix(dot_00, dot_10, u.x);
    let top_interp = mix(dot_01, dot_11, u.x);

    return mix(bottom_interp, top_interp, u.y);
}

fn dither(coords: vec2<u32>) -> f32 {
    let p = vec2<f32>(coords);
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453) - 0.5;
}

fn interpolate_cubic_hermite(x: f32, p1: Point, p2: Point, m1: f32, m2: f32) -> f32 {
    let dx = p2.x - p1.x;
    if (dx <= 0.0) { return p1.y; }
    let t = (x - p1.x) / dx;
    let t2 = t * t;
    let t3 = t2 * t;
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;
    return h00 * p1.y + h10 * m1 * dx + h01 * p2.y + h11 * m2 * dx;
}

fn apply_curve(val: f32, points: array<Point, 16>, count: u32) -> f32 {
    if (count < 2u) { return val; }
    var local_points = points;
    let x = val * 255.0;
    if (x <= local_points[0].x) { return local_points[0].y / 255.0; }
    if (x >= local_points[count - 1u].x) { return local_points[count - 1u].y / 255.0; }
    for (var i = 0u; i < 15u; i = i + 1u) {
        if (i >= count - 1u) { break; }
        let p1 = local_points[i];
        let p2 = local_points[i + 1u];
        if (x <= p2.x) {
            let p0 = local_points[max(0u, i - 1u)];
            let p3 = local_points[min(count - 1u, i + 2u)];
            let delta_before = (p1.y - p0.y) / max(0.001, p1.x - p0.x);
            let delta_current = (p2.y - p1.y) / max(0.001, p2.x - p1.x);
            let delta_after = (p3.y - p2.y) / max(0.001, p3.x - p2.x);
            var tangent_at_p1: f32;
            var tangent_at_p2: f32;
            if (i == 0u) { tangent_at_p1 = delta_current; } else {
                if (delta_before * delta_current <= 0.0) { tangent_at_p1 = 0.0; } else { tangent_at_p1 = (delta_before + delta_current) / 2.0; }
            }
            if (i + 1u == count - 1u) { tangent_at_p2 = delta_current; } else {
                if (delta_current * delta_after <= 0.0) { tangent_at_p2 = 0.0; } else { tangent_at_p2 = (delta_current + delta_after) / 2.0; }
            }
            if (delta_current != 0.0) {
                let alpha = tangent_at_p1 / delta_current;
                let beta = tangent_at_p2 / delta_current;
                if (alpha * alpha + beta * beta > 9.0) {
                    let tau = 3.0 / sqrt(alpha * alpha + beta * beta);
                    tangent_at_p1 = tangent_at_p1 * tau;
                    tangent_at_p2 = tangent_at_p2 * tau;
                }
            }
            let result_y = interpolate_cubic_hermite(x, p1, p2, tangent_at_p1, tangent_at_p2);
            return clamp(result_y / 255.0, 0.0, 1.0);
        }
    }
    return local_points[count - 1u].y / 255.0;
}

fn apply_tonal_adjustments(
    color: vec3<f32>,
    local_log_detail: f32,
    mid_log_detail: f32,
    is_raw: u32,
    con: f32,
    sh: f32,
    wh: f32,
    bl: f32
) -> vec3<f32> {
    var rgb = color;

    if (wh != 0.0) {
        let w_luma = max(get_luma(max(rgb, vec3<f32>(0.0))), 0.0001);
        let w_base = max(w_luma * exp2(-clamp(local_log_detail + mid_log_detail, -3.0, 3.0)), 0.0001);
        let t_w = pow(w_base, 0.4545);
        let w_pivot = 0.45;
        let u = max(t_w - w_pivot, 0.0);

        var t_new = t_w;
        if (wh > 0.0) {
            t_new = t_w + wh * 0.25 * (u * u) / (1.0 + u * u);
        } else if (u > 0.0) {
            t_new = w_pivot + u / (1.0 + (-wh) * 0.3 * u);
        }
        let w_ramp = smoothstep(0.25, 0.9, t_w);
        t_new = mix(t_w, t_new, w_ramp);

        let w_amount = abs(t_new - t_w) * w_ramp;
        rgb *= pow(t_new, 2.2) / w_base;

        let w_log_d = local_log_detail * 0.4545;
        let w_safe_detail = exp2(w_log_d / (1.0 + abs(w_log_d) * 0.4));
        let w_corr = pow(w_safe_detail, 1.0 + w_amount * 1.0) / w_safe_detail;
        rgb *= pow(w_corr, 2.2);
        rgb *= exp2(shape_detail(mid_log_detail, GF_REINJECT_SIGMA) * w_amount * 4.0);
    }

    let pixel_luma = get_luma(max(rgb, vec3<f32>(0.0)));

    let safe_pixel_luma = max(pixel_luma, 0.0001);

    if (sh != 0.0 || bl != 0.0) {
        let t_pixel = pow(safe_pixel_luma, 0.4545);
        let t_detail = exp2(local_log_detail * 0.4545);
        let t_blurred = t_pixel / max(t_detail, 0.0001);

        let shadow_lift = sh * t_pixel * pow(max(1.0 - t_pixel, 0.0), 4.5);
        let black_lift = bl * t_pixel * pow(max(1.0 - t_pixel, 0.0), 12.0);
        let lift_amount = max(shadow_lift + black_lift, 0.0);

        let t_pixel_curved = max(t_pixel + shadow_lift + black_lift, 0.0);

        let shadow_pivot = 0.2;
        let stretch_focus = 1.0 - smoothstep(0.1, 0.85, t_pixel);
        let detail_focus = 1.0 - smoothstep(0.05, 0.55, t_pixel);
        let stretch_factor = 1.0 + (lift_amount * stretch_focus * 4.4);
        let contrasted_t = shadow_pivot + (t_pixel_curved - shadow_pivot) * stretch_factor;

        let final_t = max(contrasted_t, 0.0);
        let curved_luma = pow(final_t, 2.2);

        let luma_ratio = curved_luma / safe_pixel_luma;
        rgb *= luma_ratio;

        let log_d = log2(max(t_detail, 0.0001));
        let safe_detail = exp2(log_d / (1.0 + abs(log_d) * 0.4));

        let noise_protection = smoothstep(0.0, 0.1, t_blurred);

        let detail_amp = 1.0 + (lift_amount * detail_focus * 1.5 * noise_protection);

        let enhanced_detail = pow(safe_detail, detail_amp);
        let detail_correction = enhanced_detail / safe_detail;

        let linear_correction = pow(detail_correction, 2.2);
        rgb *= linear_correction;

        let lift_structure = lift_amount * detail_focus * noise_protection * 6.0;
        rgb *= exp2(shape_detail(mid_log_detail, GF_REINJECT_SIGMA) * lift_structure);

        if (luma_ratio > 1.0) {
            let recovered_luma = get_luma(rgb);
            let boost_amount = clamp((luma_ratio - 1.0) * 0.05, 0.0, 0.12);
            rgb = mix(rgb, vec3<f32>(recovered_luma), boost_amount);
        }
    }

    if (con != 0.0) {
        let safe_rgb = max(rgb, vec3<f32>(0.0));
        let g = 2.2;
        let perceptual = pow(safe_rgb, vec3<f32>(1.0 / g));
        let clamped_perceptual = clamp(perceptual, vec3<f32>(0.0), vec3<f32>(1.0));
        let strength = pow(2.0, con * 1.25);
        let condition = clamped_perceptual < vec3<f32>(0.5);
        let high_part = 1.0 - 0.5 * pow(2.0 * (1.0 - clamped_perceptual), vec3<f32>(strength));
        let low_part = 0.5 * pow(2.0 * clamped_perceptual, vec3<f32>(strength));
        let curved_perceptual = select(high_part, low_part, condition);
        let contrast_adjusted_rgb = pow(curved_perceptual, vec3<f32>(g));
        let mix_factor = smoothstep(vec3<f32>(1.0), vec3<f32>(1.01), safe_rgb);
        rgb = mix(contrast_adjusted_rgb, rgb, mix_factor);
    }
    return rgb;
}

fn apply_highlights_adjustment(
    color_in: vec3<f32>,
    local_log_detail: f32,
    mid_log_detail: f32,
    is_raw: u32,
    highlights_adj: f32
) -> vec3<f32> {
    if (abs(highlights_adj) < 0.001) {
        return color_in;
    }

    let pixel_luma = get_luma(max(color_in, vec3<f32>(0.0)));
    if (pixel_luma < 0.0001) {
        return color_in;
    }

    const l_pivot: f32 = 0.06;
    if (pixel_luma <= l_pivot) {
        return color_in;
    }

    let luma_base = pixel_luma * exp2(-local_log_detail);
    let detail_ratio = pixel_luma / max(luma_base, 1e-4);

    let safe_detail = clamp(detail_ratio, 0.65, 1.55);

    let log_detail = log2(safe_detail);
    let soft_log_detail = log_detail / (1.0 + abs(log_detail) * 0.40);

    var target_luma: f32 = pixel_luma;

    if (highlights_adj < 0.0) {
        let k = -highlights_adj;
        let delta_base = max(luma_base - l_pivot, 0.0);

        let compression_strength = k * 2.2;
        let compressed_delta = delta_base / (1.0 + compression_strength * (delta_base / (1.0 + delta_base * 0.35)));
        let target_base = l_pivot + compressed_delta;

        let neg_soft_detail = local_log_detail / (1.0 + abs(local_log_detail) * 0.80);
        let restoration_gain = 1.0 + k * 0.3;
        let structure_gain = shape_detail(mid_log_detail, GF_REINJECT_SIGMA) * k * 0.6;
        let recovered_detail = exp2(neg_soft_detail * restoration_gain + structure_gain);

        let recovered_target = target_base * recovered_detail;

        let blend = smoothstep(l_pivot, l_pivot + 0.22, pixel_luma);
        target_luma = mix(pixel_luma, recovered_target, blend);

    } else {
        let boost = highlights_adj * 0.70;
        let delta_base = max(luma_base - l_pivot, 0.0);
        let boosted_delta = delta_base * (1.0 + boost / (1.0 + delta_base * 0.25));
        let target_base = l_pivot + boosted_delta;

        let detail_boost = exp2(soft_log_detail * (1.0 + highlights_adj * 0.20));
        let boosted_target = target_base * detail_boost;

        let blend = smoothstep(l_pivot, l_pivot + 0.22, pixel_luma);
        target_luma = mix(pixel_luma, boosted_target, blend);
    }

    let luma_ratio = target_luma / pixel_luma;
    var final_color = color_in * luma_ratio;

    if (highlights_adj < 0.0 && pixel_luma > 1.0) {
        let blowout = smoothstep(1.0, 3.5, pixel_luma) * (-highlights_adj) * 0.40;
        final_color = mix(final_color, vec3<f32>(target_luma), blowout);
    }

    return final_color;
}

fn apply_linear_exposure(color_in: vec3<f32>, exposure_adj: f32) -> vec3<f32> {
    if (exposure_adj == 0.0) {
        return color_in;
    }
    return color_in * pow(2.0, exposure_adj);
}

fn apply_filmic_exposure(color_in: vec3<f32>, brightness_adj: f32) -> vec3<f32> {
    if (brightness_adj == 0.0) {
        return color_in;
    }
    const RATIONAL_CURVE_MIX: f32 = 1.0;
    const MIDTONE_STRENGTH: f32 = 1.2;
    const TOP_ANCHOR: f32 = 1.0;

    let original_luma = get_luma(color_in);
    if (abs(original_luma) < 0.00001) {
        return color_in;
    }

    let direct_adj = brightness_adj * (1.0 - RATIONAL_CURVE_MIX);
    let rational_adj = brightness_adj * RATIONAL_CURVE_MIX;
    let scale = pow(2.0, direct_adj);
    let k = pow(2.0, -rational_adj * MIDTONE_STRENGTH);
    let luma_abs = abs(original_luma);

    var shaped_luma_abs: f32;
    if (luma_abs <= TOP_ANCHOR) {
        let luma_norm = luma_abs / TOP_ANCHOR;
        let shaped_norm = luma_norm / (luma_norm + (1.0 - luma_norm) * k);
        shaped_luma_abs = shaped_norm * TOP_ANCHOR;
    } else {
        shaped_luma_abs = TOP_ANCHOR + (luma_abs - TOP_ANCHOR) * k;
    }

    let new_luma_abs = shaped_luma_abs * scale;
    let new_luma = sign(original_luma) * new_luma_abs;
    let chroma = color_in - vec3<f32>(original_luma);

    let total_luma_scale = new_luma_abs / luma_abs;
    let luma_weight = clamp(new_luma_abs, 0.0, 2.0) * 0.5;
    let dynamic_exp = mix(0.95, 0.65, luma_weight);
    let base_chroma_scale = pow(total_luma_scale, dynamic_exp);

    let highlight_rolloff = clamp((1.05 - new_luma_abs) / max(1.05 - original_luma, 1e-4), 0.0, 1.0);
    let chroma_scale = base_chroma_scale * mix(1.0, highlight_rolloff, clamp(abs(brightness_adj), 0.0, 1.0));

    return vec3<f32>(new_luma) + chroma * chroma_scale;
}

fn apply_color_calibration(color: vec3<f32>, cal: ColorCalibrationSettings) -> vec3<f32> {
    let h_r = cal.red_hue;
    let h_g = cal.green_hue;
    let h_b = cal.blue_hue;
    let r_prime = vec3<f32>(1.0 - abs(h_r), max(0.0, h_r), max(0.0, -h_r));
    let g_prime = vec3<f32>(max(0.0, -h_g), 1.0 - abs(h_g), max(0.0, h_g));
    let b_prime = vec3<f32>(max(0.0, h_b), max(0.0, -h_b), 1.0 - abs(h_b));
    let hue_matrix = mat3x3<f32>(r_prime, g_prime, b_prime);
    var c = hue_matrix * color;

    let luma = get_luma(max(vec3(0.0), c));
    let desaturated_color = vec3<f32>(luma);
    let sat_vector = c - desaturated_color;

    let color_sum = c.r + c.g + c.b;
    var masks = vec3<f32>(0.0);
    if (color_sum > 0.001) {
        masks = c / color_sum;
    }

    let total_sat_adjustment =
        masks.r * cal.red_saturation +
        masks.g * cal.green_saturation +
        masks.b * cal.blue_saturation;

    c += sat_vector * total_sat_adjustment;

    let st = cal.shadows_tint;
    if (abs(st) > 0.001) {
        let shadow_luma = get_luma(max(vec3(0.0), c));
        let mask = 1.0 - smoothstep(0.0, 0.3, shadow_luma);
        let tint_mult = vec3<f32>(1.0 + st * 0.25, 1.0 - st * 0.25, 1.0 + st * 0.25);
        c = mix(c, c * tint_mult, mask);
    }

    return c;
}

fn apply_white_balance(color: vec3<f32>, log_gains: vec3<f32>) -> vec3<f32> {
    if (all(log_gains == vec3<f32>(0.0))) {
        return color;
    }
    let rgb_to_lms = adjustments.global.wb_rgb_to_lms_matrix;
    let lms_to_rgb = adjustments.global.wb_lms_to_rgb_matrix;
    let gains = exp(log_gains);
    let white = lms_to_rgb * (gains * (rgb_to_lms * vec3<f32>(1.0)));
    return lms_to_rgb * (gains * (rgb_to_lms * color)) / get_luma(white);
}

fn apply_creative_color(color: vec3<f32>, sat: f32, vib: f32) -> vec3<f32> {
    if (sat == 0.0 && vib == 0.0) {
        return color;
    }

    var processed = color;
    let luma = get_luma(max(processed, vec3<f32>(0.0)));

    let srgb = linear_to_srgb_extended(max(processed, vec3<f32>(0.0)));
    let hsv = rgb_to_hsv(srgb);
    let current_sat = hsv.y;
    let hue = hsv.x;

    var vib_factor: f32 = 0.0;
    if (vib != 0.0) {
        if (vib > 0.0) {
            let sat_weight = pow(max(1.0 - current_sat, 0.0), 1.25);
            let skin_center = 25.0;
            let hue_dist = min(abs(hue - skin_center), 360.0 - abs(hue - skin_center));
            let is_skin = 1.0 - smoothstep(12.0, 38.0, hue_dist);
            let skin_dampener = mix(1.0, 0.50, is_skin);

            vib_factor = vib * sat_weight * skin_dampener * 1.5;
        } else {
            let desat_weight = smoothstep(0.05, 0.75, current_sat);
            vib_factor = vib * desat_weight;
        }
    }

    let sat_scale = max(1.0 + sat, 0.0);
    let vib_scale = max(1.0 + vib_factor, 0.0);
    let final_mult = sat_scale * vib_scale;

    processed = mix(vec3<f32>(luma), processed, final_mult);

    return max(processed, vec3<f32>(0.0));
}

fn apply_hsl_panel(color: vec3<f32>, hsl_adjustments: array<HslColor, 8>, coords_i: vec2<i32>) -> vec3<f32> {
    var has_adjustments = false;
    for (var i = 0u; i < 8u; i = i + 1u) {
        if (abs(hsl_adjustments[i].hue) > 0.0001 ||
            abs(hsl_adjustments[i].saturation) > 0.0001 ||
            abs(hsl_adjustments[i].luminance) > 0.0001) {
            has_adjustments = true;
            break;
        }
    }
    if (!has_adjustments) {
        return color;
    }

    let safe_color = max(color, vec3<f32>(0.0));
    if (distance(safe_color.r, safe_color.g) < 0.001 && distance(safe_color.g, safe_color.b) < 0.001) {
        return safe_color;
    }
    let original_hsv = rgb_to_hsv(linear_to_srgb_extended(safe_color));
    let original_luma = get_luma(safe_color);

    let saturation_mask = smoothstep(0.05, 0.20, original_hsv.y);
    let luminance_weight = smoothstep(0.0, 1.0, original_hsv.y);

    if (saturation_mask < 0.001 && luminance_weight < 0.001) {
        return safe_color;
    }

    let original_hue = original_hsv.x;

    var raw_influences: array<f32, 8>;
    var total_raw_influence: f32 = 0.0;
    for (var i = 0u; i < 8u; i = i + 1u) {
        let range = HSL_RANGES[i];
        let influence = get_raw_hsl_influence(original_hue, range.center, range.width);
        raw_influences[i] = influence;
        total_raw_influence += influence;
    }

    var total_hue_shift: f32 = 0.0;
    var total_sat_multiplier: f32 = 0.0;
    var total_lum_adjust: f32 = 0.0;

    for (var i = 0u; i < 8u; i = i + 1u) {
        let normalized_influence = raw_influences[i] / total_raw_influence;

        let hue_sat_influence = normalized_influence * saturation_mask;
        let luma_influence = normalized_influence * luminance_weight;

        total_hue_shift += hsl_adjustments[i].hue * 2.0 * hue_sat_influence;
        total_sat_multiplier += hsl_adjustments[i].saturation * hue_sat_influence;
        total_lum_adjust += hsl_adjustments[i].luminance * luma_influence;
    }

    if (original_hsv.y * (1.0 + total_sat_multiplier) < 0.0001) {
        let final_luma = original_luma * (1.0 + total_lum_adjust);
        return vec3<f32>(final_luma);
    }
    var hsv = original_hsv;
    hsv.x = (hsv.x + total_hue_shift + 360.0) % 360.0;
    hsv.y = clamp(hsv.y * (1.0 + total_sat_multiplier), 0.0, 1.0);
    let hs_shifted_rgb = srgb_to_linear(hsv_to_rgb(vec3<f32>(hsv.x, hsv.y, original_hsv.z)));
    let new_luma = get_luma(hs_shifted_rgb);
    let target_luma = original_luma * (1.0 + total_lum_adjust);
    if (new_luma < 0.0001) {
        return vec3<f32>(max(0.0, target_luma));
    }
    let final_color = hs_shifted_rgb * (target_luma / new_luma);
    return final_color;
}

fn apply_color_grading(color: vec3<f32>, shadows: ColorGradeSettings, midtones: ColorGradeSettings, highlights: ColorGradeSettings, global: ColorGradeSettings, blending: f32, balance: f32) -> vec3<f32> {
    let luma = get_luma(max(vec3(0.0), color));
    let base_shadow_crossover = 0.1;
    let base_highlight_crossover = 0.5;
    let balance_range = 0.5;
    let shadow_crossover = base_shadow_crossover + max(0.0, -balance) * balance_range;
    let highlight_crossover = base_highlight_crossover - max(0.0, balance) * balance_range;
    let feather = 0.2 * blending;
    let final_shadow_crossover = min(shadow_crossover, highlight_crossover - 0.01);
    let shadow_mask = 1.0 - smoothstep(final_shadow_crossover - feather, final_shadow_crossover + feather, luma);
    let highlight_mask = smoothstep(highlight_crossover - feather, highlight_crossover + feather, luma);
    let midtone_mask = max(0.0, 1.0 - shadow_mask - highlight_mask);
    let global_mask = 1.0;
    var graded_color = color;
    let shadow_sat_strength = 0.1;
    let shadow_lum_strength = 0.5;
    let midtone_sat_strength = 0.6;
    let midtone_lum_strength = 0.8;
    let highlight_sat_strength = 0.8;
    let highlight_lum_strength = 1.0;
    let global_sat_strength = 1.0;
    let global_lum_strength = 1.0;
    if (shadows.saturation > 0.001) { let tint_rgb = hsv_to_rgb(vec3<f32>(shadows.hue, 1.0, 1.0)); graded_color += (tint_rgb - 0.5) * shadows.saturation * shadow_mask * shadow_sat_strength; }
    graded_color += shadows.luminance * shadow_mask * shadow_lum_strength;
    if (midtones.saturation > 0.001) { let tint_rgb = hsv_to_rgb(vec3<f32>(midtones.hue, 1.0, 1.0)); graded_color += (tint_rgb - 0.5) * midtones.saturation * midtone_mask * midtone_sat_strength; }
    graded_color += midtones.luminance * midtone_mask * midtone_lum_strength;
    if (highlights.saturation > 0.001) { let tint_rgb = hsv_to_rgb(vec3<f32>(highlights.hue, 1.0, 1.0)); graded_color += (tint_rgb - 0.5) * highlights.saturation * highlight_mask * highlight_sat_strength; }
    graded_color += highlights.luminance * highlight_mask * highlight_lum_strength;
    if (global.saturation > 0.001) { let tint_rgb = hsv_to_rgb(vec3<f32>(global.hue, 1.0, 1.0)); graded_color += (tint_rgb - 0.5) * global.saturation * global_mask * global_sat_strength; }
    graded_color += global.luminance * global_mask * global_lum_strength;
    return graded_color;
}

fn sample_gf_tex(tex: texture_2d<f32>, abs_coord: vec2<u32>) -> vec4<f32> {
    let full = vec2<f32>(textureDimensions(input_texture));
    let low_i = vec2<i32>(textureDimensions(tex));
    let p = (vec2<f32>(abs_coord) + 0.5) / full * vec2<f32>(low_i) - 0.5;
    let fl = floor(p);
    let w = p - fl;
    let max_idx = low_i - vec2<i32>(1);
    let c0 = clamp(vec2<i32>(fl), vec2<i32>(0), max_idx);
    let c1 = clamp(vec2<i32>(fl) + vec2<i32>(1), vec2<i32>(0), max_idx);
    let v00 = textureLoad(tex, c0, 0);
    let v10 = textureLoad(tex, vec2<i32>(c1.x, c0.y), 0);
    let v01 = textureLoad(tex, vec2<i32>(c0.x, c1.y), 0);
    let v11 = textureLoad(tex, c1, 0);
    return mix(mix(v00, v10, w.x), mix(v01, v11, w.x), w.y);
}

fn sample_gf_coeffs(abs_coord: vec2<u32>) -> vec4<f32> {
    return sample_gf_tex(gf_coeffs_texture, abs_coord);
}

fn refined_dark_channel(abs_coord: vec2<u32>, is_raw: u32) -> f32 {
    var rgb = clamp(textureLoad(input_texture, abs_coord, 0).rgb, vec3<f32>(0.0), vec3<f32>(65504.0));
    if (is_raw == 0u) {
        rgb = srgb_to_linear(rgb);
    }
    let dark_i = log2(max(min(rgb.r, min(rgb.g, rgb.b)), GF_LUMA_FLOOR));
    let ab = sample_gf_tex(gf_dehaze_texture, abs_coord).zw;
    return exp2(ab.x * dark_i + ab.y);
}

fn shape_detail(d: f32, sigma: f32) -> f32 {
    let x = d / sigma;
    return d / (1.0 + x * x);
}

fn shape_detail_gf(d: f32) -> f32 {
    return shape_detail(d, select(GF_DETAIL_SIGMA, GF_BRIGHT_SIGMA, d > 0.0));
}

fn centre_clarity_strength(centre_amount: f32, centre_pos: vec2<f32>, coords_i: vec2<i32>) -> f32 {
    if (centre_amount == 0.0) {
        return 0.0;
    }
    let full_dims_f = vec2<f32>(textureDimensions(input_texture));
    let coord_f = vec2<f32>(coords_i);
    let midpoint = 0.4;
    let feather = 0.375;
    let aspect = full_dims_f.y / full_dims_f.x;
    let uv_centered = (coord_f / full_dims_f - centre_pos) * 2.0;
    let d = length(uv_centered * vec2<f32>(1.0, aspect)) * 0.5;
    let vignette_mask = smoothstep(midpoint - feather, midpoint + feather, d);
    let centre_mask = 1.0 - vignette_mask;

    const CLARITY_SCALE: f32 = 0.9;
    return centre_amount * (2.0 * centre_mask - 1.0) * CLARITY_SCALE;
}

fn sharpen_perc(c: vec3<f32>, is_raw: u32) -> f32 {
    let y = max(get_luma(c), 0.0);
    if (is_raw == 1u) { return sqrt(y); }
    return y;
}

fn sharpen_tap(coords: vec2<i32>, max_idx: vec2<i32>, is_raw: u32) -> f32 {
    let c = clamp(coords, vec2<i32>(0), max_idx);
    return sharpen_perc(textureLoad(input_texture, vec2<u32>(c), 0).rgb, is_raw);
}

fn sharpen_tap_bilinear(p: vec2<f32>, max_idx: vec2<i32>, is_raw: u32) -> f32 {
    let fl = floor(p);
    let w = p - fl;
    let b = vec2<i32>(fl);
    let s00 = sharpen_tap(b, max_idx, is_raw);
    let s10 = sharpen_tap(b + vec2<i32>(1, 0), max_idx, is_raw);
    let s01 = sharpen_tap(b + vec2<i32>(0, 1), max_idx, is_raw);
    let s11 = sharpen_tap(b + vec2<i32>(1, 1), max_idx, is_raw);
    return mix(mix(s00, s10, w.x), mix(s01, s11, w.x), w.y);
}

fn sharpen_soft_limit(v: f32, lo: f32, hi: f32, margin: f32) -> f32 {
    let m = max(margin, 1e-5);
    if (v > hi) {
        let e = v - hi;
        return hi + e / (1.0 + e / m);
    }
    if (v < lo) {
        let e = lo - v;
        return lo - e / (1.0 + e / m);
    }
    return v;
}

fn apply_sharpen(
    color: vec3<f32>,
    b1_in: vec3<f32>,
    b2_in: vec3<f32>,
    coords_i: vec2<i32>,
    amount: f32,
    threshold: f32,
    is_raw: u32
) -> vec3<f32> {
    if (abs(amount) < 0.0005) {
        return color;
    }

    if (amount < 0.0) {
        var b1_lin = b1_in;
        if (is_raw == 0u) { b1_lin = srgb_to_linear(b1_in); }
        return mix(color, b1_lin, clamp(-amount * 0.5, 0.0, 1.0));
    }

    var color_enc = color;
    if (is_raw == 0u) {
        color_enc = linear_to_srgb_extended(color);
    }
    let l = select(get_luma(color_enc), sqrt(max(get_luma(color), 0.0)), is_raw == 1u);
    let l1 = sharpen_perc(b1_in, is_raw);
    let l2 = sharpen_perc(b2_in, is_raw);

    let d0 = l  - l1;
    let d1 = l1 - l2;

    let t = max(threshold * 0.15, 1e-4);
    let g0 = smoothstep(t * 0.20, t * 0.85, abs(d0));
    let g1 = smoothstep(t * 0.12, t * 0.55, abs(d1));

    let d0_s = shape_detail(d0, SHARPEN_EDGE_SIGMA);
    let d1_s = shape_detail(d1, SHARPEN_EDGE_SIGMA * 0.8);
    let boost = (d0_s * 1.25 * g0 + d1_s * 0.25 * g1) * amount;

    let dims = vec2<i32>(textureDimensions(input_texture));
    let max_idx = dims - vec2<i32>(1);
    let kw = array<f32, 5>(0.01853, -0.21023, 1.38348, -0.21023, 0.01853);

    var acc = 0.0;
    var center_tap = 0.0;
    var lo = 1.0e9;
    var hi = -1.0e9;
    var gx = 0.0;
    var gy = 0.0;

    for (var iy = 0; iy < 5; iy = iy + 1) {
        let oy = iy - 2;
        let ky = kw[iy];
        let cy = clamp(coords_i.y + oy, 0, max_idx.y);
        for (var ix = 0; ix < 5; ix = ix + 1) {
            let ox = ix - 2;
            let cx = clamp(coords_i.x + ox, 0, max_idx.x);
            let sl = sharpen_perc(textureLoad(input_texture, vec2<u32>(vec2<i32>(cx, cy)), 0).rgb, is_raw);

            acc += sl * kw[ix] * ky;
            lo = min(lo, sl);
            hi = max(hi, sl);

            if (ox == 0 && oy == 0) { center_tap = sl; }

            if (abs(ox) <= 1 && abs(oy) <= 1) {
                gx += sl * f32(ox) * (2.0 - abs(f32(oy)));
                gy += sl * f32(oy) * (2.0 - abs(f32(ox)));
            }
        }
    }

    let deconv_delta = shape_detail(acc - center_tap, SHARPEN_EDGE_SIGMA) * clamp(amount * 0.60, 0.0, 1.0);
    let range = max(hi - lo, 1e-5);
    let edge_ratio = smoothstep(0.06, 0.28, range);
    let dark_scale = mix(SHARPEN_DARK_SCALE, 0.1, edge_ratio);
    let sharpen_delta = boost + deconv_delta;
    var l_new = l + select(sharpen_delta, sharpen_delta * dark_scale, sharpen_delta < 0.0);

    l_new = sharpen_soft_limit(
        l_new,
        lo - range * 0.01,
        hi + range * 0.10,
        range * 0.12
    );

    let g2 = gx * gx + gy * gy;
    if (g2 > 1e-6) {
        let diagonality = clamp(2.0 * abs(gx * gy) / g2, 0.0, 1.0);
        let edge_present = smoothstep(t * 0.5, t * 2.5, sqrt(g2) * 0.25);
        let aa = 0.90 * diagonality * edge_present;

        if (aa > 0.002) {
            let inv_g = inverseSqrt(g2);

            let tang = vec2<f32>(-gy, gx) * inv_g * 0.75;
            let p = vec2<f32>(coords_i) + vec2<f32>(0.5);

            let tp = sharpen_tap_bilinear(p + tang - vec2<f32>(0.5), max_idx, is_raw);
            let tn = sharpen_tap_bilinear(p - tang - vec2<f32>(0.5), max_idx, is_raw);

            let safe_tp = max(tp, l * 0.6);
            let safe_tn = max(tn, l * 0.6);

            let l_tan = (safe_tp + safe_tn + l * 2.0) * 0.25;

            let l_aa = l_tan + (l_new - l);

            l_new = mix(l_new, l_aa, min(aa, 0.5));
        }
    }

    let shadow_floor = select(0.03, 0.10, is_raw == 1u);
    let prot = smoothstep(0.0, shadow_floor, l) * (1.0 - smoothstep(0.92, 1.0, l));
    l_new = max(mix(l, l_new, prot), 0.0);
    l_new = max(l_new, l * 0.70);

    let ratio = l_new / max(l, 1e-4);
    if (is_raw == 1u) {
        return color * (ratio * ratio);
    }
    return srgb_to_linear(max(color_enc * ratio, vec3<f32>(0.0)));
}

fn apply_centre_tonal_and_color(
    color_in: vec3<f32>,
    centre_amount: f32,
    centre_pos: vec2<f32>,
    coords_i: vec2<i32>
) -> vec3<f32> {
    if (centre_amount == 0.0) {
        return color_in;
    }
    let full_dims_f = vec2<f32>(textureDimensions(input_texture));
    let coord_f = vec2<f32>(coords_i);
    let midpoint = 0.4;
    let feather = 0.375;
    let aspect = full_dims_f.y / full_dims_f.x;
    let uv_centered = (coord_f / full_dims_f - centre_pos) * 2.0;
    let d = length(uv_centered * vec2<f32>(1.0, aspect)) * 0.5;
    let vignette_mask = smoothstep(midpoint - feather, midpoint + feather, d);
    let centre_mask = 1.0 - vignette_mask;

    const EXPOSURE_SCALE: f32 = 0.5;
    const VIBRANCE_SCALE: f32 = 0.4;
    const SATURATION_CENTER_SCALE: f32 = 0.3;
    const SATURATION_EDGE_SCALE: f32 = 0.8;

    var processed_color = color_in;

    let exposure_boost = centre_mask * centre_amount * EXPOSURE_SCALE;
    processed_color = apply_linear_exposure(processed_color, exposure_boost);

    let vibrance_center_boost = centre_mask * centre_amount * VIBRANCE_SCALE;
    let saturation_center_boost = centre_mask * centre_amount * SATURATION_CENTER_SCALE;
    let saturation_edge_effect = -(1.0 - centre_mask) * centre_amount * SATURATION_EDGE_SCALE;
    let total_saturation_effect = saturation_center_boost + saturation_edge_effect;
    processed_color = apply_creative_color(processed_color, total_saturation_effect, vibrance_center_boost);

    return processed_color;
}

fn apply_dehaze(
    color: vec3<f32>,
    blurred_color_input_space: vec3<f32>,
    refined_dark: f32,
    is_raw: u32,
    amount: f32
) -> vec3<f32> {
    if (amount == 0.0) { return color; }

    let atmospheric_light = vec3<f32>(0.95, 0.97, 1.0);

    if (amount > 0.0) {
        let safe_dark = max(refined_dark - 0.02, 0.0);
        let mapped_haze = safe_dark / (safe_dark + 0.2);
        let t = max(1.0 - amount * mapped_haze * DEHAZE_STRENGTH, 0.08);
        var recovered = (color - atmospheric_light) / t + atmospheric_light;
        let rec_luma = get_luma(max(recovered, vec3<f32>(0.0)));
        let shadow_lift = smoothstep(0.1, 0.0, rec_luma) * (1.0 - t) * 0.15;
        recovered += shadow_lift;
        let haze_removed = 1.0 - t;
        let sat_boost = haze_removed * 0.5;
        let final_luma = get_luma(max(recovered, vec3<f32>(0.0)));
        recovered = mix(vec3<f32>(final_luma), recovered, 1.0 + sat_boost);
        return max(recovered, vec3<f32>(0.0));
    } else {
        var blurred_linear = blurred_color_input_space;
        if (is_raw == 0u) {
            blurred_linear = srgb_to_linear(blurred_color_input_space);
        }
        let regional_dark = min(blurred_linear.r, min(blurred_linear.g, blurred_linear.b));
        let safe_dark = max(regional_dark - 0.02, 0.0);
        let mapped_depth = safe_dark / (safe_dark + 0.2);
        let depth_factor = mix(0.4, 1.0, mapped_depth);
        return mix(color, atmospheric_light, abs(amount) * 0.7 * depth_factor);
    }
}

fn apply_noise_reduction(
    center_linear: vec3<f32>,
    coords_i: vec2<i32>,
    luma_amount: f32,
    color_amount: f32,
    scale: f32,
    is_raw: u32
) -> vec3<f32> {
    let luma_a  = clamp(luma_amount,  0.0, 1.0);
    let color_a = clamp(color_amount, 0.0, 1.0);
    if (luma_a < 0.001 && color_a < 0.001) {
        return center_linear;
    }

    let dims = vec2<i32>(textureDimensions(input_texture));
    let max_idx = dims - vec2<i32>(1);
    let center_safe   = max(center_linear, vec3<f32>(0.0));
    let center_luma   = get_luma(center_safe);
    let center_chroma = center_linear - vec3<f32>(center_luma);

    let res_factor = clamp(sqrt(scale), 0.5, 2.0);

    var new_luma   = center_luma;
    var new_chroma = center_chroma;

    if (luma_a > 0.001) {
        let l_curve = sqrt(luma_a);

        let stride_f = mix(1.0, 2.0, smoothstep(0.45, 0.95, luma_a)) * res_factor;
        let extra    = clamp(stride_f - 1.0, 0.0, 1.0);

        let l_spatial = mix(1.0, 1.5, l_curve);
        let l_spat_n  = -1.0 / max(2.0 * l_spatial * l_spatial, 1e-6);

        let h1 = hash(vec2<f32>(coords_i));
        let h2 = hash(vec2<f32>(coords_i) + vec2<f32>(17.31, 71.13));

        var samp_luma: array<f32, 25>;
        var samp_spat: array<f32, 25>;
        var lmin: f32 = center_luma;
        var lmax: f32 = center_luma;

        samp_luma[0] = center_luma;
        samp_spat[0] = 1.0;

        var idx: u32 = 1u;
        for (var dy: i32 = -2; dy <= 2; dy = dy + 1) {
            for (var dx: i32 = -2; dx <= 2; dx = dx + 1) {
                if (dx == 0 && dy == 0) { continue; }

                let ring = max(abs(dx), abs(dy));
                let ring_factor = select(0.5, 1.0, ring == 2);
                let grow = 1.0 + extra * ring_factor;

                let jx = (h1 - 0.5) * 2.0 * extra;
                let jy = (h2 - 0.5) * 2.0 * extra;

                let off_f = vec2<f32>(f32(dx) * grow + jx, f32(dy) * grow + jy);
                let off   = vec2<i32>(i32(round(off_f.x)), i32(round(off_f.y)));
                let coord = clamp(coords_i + off, vec2<i32>(0), max_idx);

                var s = textureLoad(input_texture, vec2<u32>(coord), 0).rgb;
                if (is_raw == 0u) { s = srgb_to_linear(s); }
                let s_luma = get_luma(max(s, vec3<f32>(0.0)));
                samp_luma[idx] = s_luma;
                samp_spat[idx] = exp(f32(dx * dx + dy * dy) * l_spat_n);
                lmin = min(lmin, s_luma);
                lmax = max(lmax, s_luma);
                idx = idx + 1u;
            }
        }

        let luma_range    = lmax - lmin;
        let edge_strength = smoothstep(0.04, 0.20, luma_range);
        let edge_midpoint = (lmin + lmax) * 0.5;
        let center_side   = center_luma > edge_midpoint;

        let l_range_tol = mix(
            mix(0.025, 0.075, l_curve),
            mix(0.010, 0.025, l_curve),
            edge_strength
        );

        var samp_gate: array<f32, 25>;
        var sum_a: f32 = 0.0;
        var w_a:   f32 = 0.0;
        for (var k: u32 = 0u; k < 25u; k = k + 1u) {
            let diff = abs(samp_luma[k] - center_luma);
            let g_range = 1.0 - smoothstep(l_range_tol * 0.6, l_range_tol, diff);
            let s_side  = samp_luma[k] > edge_midpoint;
            let g_side  = select(0.0, 1.0, s_side == center_side);
            let g_edge  = mix(1.0, g_side, edge_strength);
            let w = samp_spat[k] * g_range * g_edge;
            samp_gate[k] = w;
            sum_a += samp_luma[k] * w;
            w_a   += w;
        }
        let initial_mean = sum_a / max(w_a, 1e-4);

        let outlier_tol = mix(0.07, 0.025, edge_strength);
        var sum_b: f32 = 0.0;
        var w_b:   f32 = 0.0;
        for (var k: u32 = 0u; k < 25u; k = k + 1u) {
            let init_w = samp_gate[k];
            if (init_w > 0.0001) {
                let d = samp_luma[k] - initial_mean;
                let r = abs(d) / outlier_tol;
                let bisq = max(0.0, 1.0 - r * r);
                let outlier_w = bisq * bisq;
                let w = init_w * outlier_w;
                sum_b += samp_luma[k] * w;
                w_b   += w;
            }
        }
        let robust_luma = select(initial_mean, sum_b / max(w_b, 1e-6), w_b > 0.01);

        let strength = luma_a * mix(1.0, 0.6, edge_strength);
        new_luma = mix(center_luma, robust_luma, strength);
    }

    if (color_a > 0.001) {
        let center_r_y = center_linear.r - center_luma;
        let center_b_y = center_linear.b - center_luma;
        let c_curve = sqrt(color_a);

        // Optical 3-Octave Multi-Scale Chroma Pyramid:
        // Octave 0 & 1: High/Medium frequency color noise (base_stride 1.8 - 3.5)
        // Octave 2 (Scale 2): Deep low-frequency shadow chroma clouds (cloud_stride 6.0 - 16.0)
        let base_stride = mix(1.8, 3.2, c_curve) * res_factor;

        let c_spatial = mix(2.0, 3.5, c_curve);
        let c_spat_n  = -1.0 / max(2.0 * c_spatial * c_spatial, 1e-6);

        let luma_tol = mix(0.12, 0.04, c_curve);
        let luma_n   = -1.0 / max(2.0 * luma_tol * luma_tol, 1e-6);

        let chroma_tol = mix(0.22, 0.09, c_curve);
        let chroma_n   = -1.0 / max(2.0 * chroma_tol * chroma_tol, 1e-6);

        // Shadow low-frequency chroma cloud detector:
        // Sensor thermal mottle is heavily concentrated in deep shadows (L < 0.35)
        let shadow_cloud_boost = smoothstep(0.45, 0.02, center_luma) * c_curve;

        let jh1 = hash(vec2<f32>(coords_i) + vec2<f32>(43.7, 91.1));
        let jh2 = hash(vec2<f32>(coords_i) + vec2<f32>(73.3, 17.9));
        let jx  = (jh1 - 0.5) * base_stride * 0.5;
        let jy  = (jh2 - 0.5) * base_stride * 0.5;

        var sum_r: f32 = center_r_y;
        var sum_b: f32 = center_b_y;
        var w_sum: f32 = 1.0;

        // Octave 0 & 1: 5x5 Bilateral Core (Sensel confetti & medium splotches)
        for (var dy: i32 = -2; dy <= 2; dy = dy + 1) {
            for (var dx: i32 = -2; dx <= 2; dx = dx + 1) {
                if (dx == 0 && dy == 0) { continue; }
                let off_f = vec2<f32>(f32(dx) * base_stride + jx, f32(dy) * base_stride + jy);
                let off   = vec2<i32>(i32(round(off_f.x)), i32(round(off_f.y)));
                let coord = clamp(coords_i + off, vec2<i32>(0), max_idx);
                var s = textureLoad(input_texture, vec2<u32>(coord), 0).rgb;

                if (is_raw == 0u) { s = srgb_to_linear(s); }

                let s_safe = max(s, vec3<f32>(0.0));
                let s_luma = get_luma(s_safe);
                let s_r_y  = s.r - s_luma;
                let s_b_y  = s.b - s_luma;

                let r2  = f32(dx * dx + dy * dy);
                let w_s = exp(r2 * c_spat_n);
                let dl  = s_luma - center_luma;
                let w_l = exp(dl * dl * luma_n);
                let dr  = s_r_y - center_r_y;
                let db  = s_b_y - center_b_y;
                let dc2 = dr * dr + db * db;
                let w_c = exp(dc2 * chroma_n);
                let w = w_s * w_l * w_c;

                sum_r += s_r_y * w;
                sum_b += s_b_y * w;
                w_sum += w;
            }
        }

        // Octave 2: MultiScale-Scale2 Cloud Killer (Low-Frequency Chroma Mottle)
        // 8 stratified radial Poisson samples reaching 3.5x to 4.8x stride to dissolve 20-50px shadow clouds
        if (shadow_cloud_boost > 0.05 || color_a > 0.25) {
            let cloud_stride = base_stride * mix(3.5, 4.8, c_curve);
            let cloud_luma_tol = mix(0.18, 0.06, c_curve);
            let cloud_luma_n = -1.0 / max(2.0 * cloud_luma_tol * cloud_luma_tol, 1e-6);

            let cloud_dirs = array<vec2<f32>, 8>(
                vec2<f32>( 1.0,  0.0),   vec2<f32>(-1.0,  0.0),
                vec2<f32>( 0.0,  1.0),   vec2<f32>( 0.0, -1.0),
                vec2<f32>( 0.707,  0.707), vec2<f32>(-0.707,  0.707),
                vec2<f32>( 0.707, -0.707), vec2<f32>(-0.707, -0.707)
            );

            let octave2_strength = mix(0.4, 1.0, shadow_cloud_boost);

            for (var ci: u32 = 0u; ci < 8u; ci = ci + 1u) {
                let dir = cloud_dirs[ci];
                let jitter_cloud = (hash(vec2<f32>(coords_i) + dir * 13.7) - 0.5) * 0.4;
                let c_off_f = dir * (cloud_stride * (1.0 + jitter_cloud));
                let c_off = vec2<i32>(i32(round(c_off_f.x)), i32(round(c_off_f.y)));
                let c_coord = clamp(coords_i + c_off, vec2<i32>(0), max_idx);

                var cs = textureLoad(input_texture, vec2<u32>(c_coord), 0).rgb;
                if (is_raw == 0u) { cs = srgb_to_linear(cs); }

                let cs_safe = max(cs, vec3<f32>(0.0));
                let cs_luma = get_luma(cs_safe);
                let cs_r_y  = cs.r - cs_luma;
                let cs_b_y  = cs.b - cs_luma;

                // Edge protection: only blend if luminance difference is within tolerance
                let c_dl = cs_luma - center_luma;
                let c_wl = exp(c_dl * c_dl * cloud_luma_n);

                // Chroma consistency weight
                let c_dr = cs_r_y - center_r_y;
                let c_db = cs_b_y - center_b_y;
                let c_dc2 = c_dr * c_dr + c_db * c_db;
                let c_wc = exp(c_dc2 * (chroma_n * 0.75));

                let cw = c_wl * c_wc * octave2_strength * 0.65;

                sum_r += cs_r_y * cw;
                sum_b += cs_b_y * cw;
                w_sum += cw;
            }
        }

        let filtered_r_y = sum_r / max(w_sum, 1e-6);
        let filtered_b_y = sum_b / max(w_sum, 1e-6);

        let new_r_y = mix(center_r_y, filtered_r_y, color_a);
        let new_b_y = mix(center_b_y, filtered_b_y, color_a);
        let new_g_y = -(LUMA_COEFF.r * new_r_y + LUMA_COEFF.b * new_b_y) / LUMA_COEFF.g;

        new_chroma = vec3<f32>(new_r_y, new_g_y, new_b_y);
    }

    return vec3<f32>(new_luma) + new_chroma;
}

fn apply_ca_correction(coords: vec2<u32>, ca_rc: f32, ca_by: f32) -> vec3<f32> {
    let dims = vec2<f32>(textureDimensions(input_texture));
    let center = dims / 2.0;
    let current_pos = vec2<f32>(coords);

    let to_center = current_pos - center;
    let dist = length(to_center);

    if (dist == 0.0) {
        return textureLoad(input_texture, coords, 0).rgb;
    }

    let dir = to_center / dist;

    let red_shift = dir * dist * ca_rc;
    let blue_shift = dir * dist * ca_by;

    let red_coords = vec2<i32>(round(current_pos - red_shift));
    let blue_coords = vec2<i32>(round(current_pos - blue_shift));
    let green_coords = vec2<i32>(current_pos);

    let max_coords = vec2<i32>(dims - 1.0);

    let r = textureLoad(input_texture, vec2<u32>(clamp(red_coords, vec2<i32>(0), max_coords)), 0).r;
    let g = textureLoad(input_texture, vec2<u32>(clamp(green_coords, vec2<i32>(0), max_coords)), 0).g;
    let b = textureLoad(input_texture, vec2<u32>(clamp(blue_coords, vec2<i32>(0), max_coords)), 0).b;

    return vec3<f32>(r, g, b);
}

// Optical AntiAC Chromatic Aberration & Purple/Green Defringing Kernel (AntiACComputeMask 0x028c2fdb)
fn apply_optical_chromatic_defringe(
    rgb: vec3<f32>,
    coords: vec2<u32>,
    full_dims: vec2<f32>,
    strength: f32
) -> vec3<f32> {
    if (strength <= 0.0) {
        return rgb;
    }
    let c_coord = vec2<i32>(coords);
    let max_c = vec2<i32>(full_dims) - vec2<i32>(1);

    // 1. Morphological gradient: maxRGB dilation minus erosion across 3x3 cross
    var min_c = rgb;
    var max_c_val = rgb;

    let s0 = textureLoad(input_texture, clamp(c_coord + vec2<i32>(0, -1), vec2<i32>(0), max_c), 0).rgb;
    let s1 = textureLoad(input_texture, clamp(c_coord + vec2<i32>(0,  1), vec2<i32>(0), max_c), 0).rgb;
    let s2 = textureLoad(input_texture, clamp(c_coord + vec2<i32>(-1, 0), vec2<i32>(0), max_c), 0).rgb;
    let s3 = textureLoad(input_texture, clamp(c_coord + vec2<i32>( 1, 0), vec2<i32>(0), max_c), 0).rgb;

    min_c = min(min_c, min(min(s0, s1), min(s2, s3)));
    max_c_val = max(max_c_val, max(max(s0, s1), max(s2, s3)));

    let morph_gradient = max(max_c_val.r, max(max_c_val.g, max_c_val.b)) - min(min_c.r, min(min_c.g, min_c.b));
    let edge_mask = smoothstep(0.12, 0.45, morph_gradient);

    if (edge_mask <= 0.001) {
        return rgb;
    }

    // 2. Axial Purple/Magenta Fringe: R > G and B > G
    let purple_excess = min(rgb.r - rgb.g, rgb.b - rgb.g);
    // 3. Axial Green Fringe: G > R and G > B
    let green_excess = rgb.g - max(rgb.r, rgb.b);

    var corrected = rgb;
    if (purple_excess > 0.03) {
        let defringe_factor = clamp((purple_excess - 0.03) / 0.15, 0.0, 1.0) * edge_mask * strength;
        let neutral_purple = vec3<f32>(rgb.g, rgb.g, rgb.g);
        corrected = mix(corrected, neutral_purple, defringe_factor);
    } else if (green_excess > 0.03) {
        let defringe_factor = clamp((green_excess - 0.03) / 0.15, 0.0, 1.0) * edge_mask * strength;
        let neutral_g = max(rgb.r, rgb.b);
        let neutral_green = vec3<f32>(rgb.r, neutral_g, rgb.b);
        corrected = mix(corrected, neutral_green, defringe_factor);
    }
    return corrected;
}

const AGX_EPSILON: f32 = 1.0e-6;
const AGX_MIN_EV: f32 = -15.2;
const AGX_MAX_EV: f32 = 5.0;
const AGX_RANGE_EV: f32 = AGX_MAX_EV - AGX_MIN_EV;
const AGX_GAMMA: f32 = 2.4;
const AGX_SLOPE: f32 = 2.3843;
const AGX_TOE_POWER: f32 = 1.5;
const AGX_SHOULDER_POWER: f32 = 1.5;
const AGX_TOE_TRANSITION_X: f32 = 0.6060606;
const AGX_TOE_TRANSITION_Y: f32 = 0.43446;
const AGX_SHOULDER_TRANSITION_X: f32 = 0.6060606;
const AGX_SHOULDER_TRANSITION_Y: f32 = 0.43446;
const AGX_INTERCEPT: f32 = -1.0112;
const AGX_TOE_SCALE: f32 = -1.0359;
const AGX_SHOULDER_SCALE: f32 = 1.3475;
const AGX_TARGET_BLACK_PRE_GAMMA: f32 = 0.0;
const AGX_TARGET_WHITE_PRE_GAMMA: f32 = 1.0;

fn agx_sigmoid(x: f32, power: f32) -> f32 {
    return x / pow(1.0 + pow(x, power), 1.0 / power);
}

fn agx_scaled_sigmoid(x: f32, scale: f32, slope: f32, power: f32, transition_x: f32, transition_y: f32) -> f32 {
    return scale * agx_sigmoid(slope * (x - transition_x) / scale, power) + transition_y;
}

fn agx_apply_curve_channel(x: f32) -> f32 {
    var result: f32 = 0.0;
    if (x < AGX_TOE_TRANSITION_X) {
        result = agx_scaled_sigmoid(x, AGX_TOE_SCALE, AGX_SLOPE, AGX_TOE_POWER, AGX_TOE_TRANSITION_X, AGX_TOE_TRANSITION_Y);
    } else if (x <= AGX_SHOULDER_TRANSITION_X) {
        result = AGX_SLOPE * x + AGX_INTERCEPT;
    } else {
        result = agx_scaled_sigmoid(x, AGX_SHOULDER_SCALE, AGX_SLOPE, AGX_SHOULDER_POWER, AGX_SHOULDER_TRANSITION_X, AGX_SHOULDER_TRANSITION_Y);
    }
    return clamp(result, AGX_TARGET_BLACK_PRE_GAMMA, AGX_TARGET_WHITE_PRE_GAMMA);
}

fn agx_compress_gamut(c: vec3<f32>) -> vec3<f32> {
    let min_c = min(c.r, min(c.g, c.b));
    if (min_c < 0.0) {
        return c - min_c;
    }
    return c;
}

fn agx_tonemap(c: vec3<f32>) -> vec3<f32> {
    let x_relative = max(c / 0.18, vec3<f32>(AGX_EPSILON));
    let log_encoded = (log2(x_relative) - AGX_MIN_EV) / AGX_RANGE_EV;
    let mapped = clamp(log_encoded, vec3<f32>(0.0), vec3<f32>(1.0));

    var curved: vec3<f32>;
    curved.r = agx_apply_curve_channel(mapped.r);
    curved.g = agx_apply_curve_channel(mapped.g);
    curved.b = agx_apply_curve_channel(mapped.b);

    let final_color = pow(max(curved, vec3<f32>(0.0)), vec3<f32>(AGX_GAMMA));

    return final_color;
}

fn agx_full_transform(color_in: vec3<f32>) -> vec3<f32> {
    let compressed_color = agx_compress_gamut(color_in);
    let color_in_agx_space = adjustments.global.agx_pipe_to_rendering_matrix * compressed_color;
    let tonemapped_agx = agx_tonemap(color_in_agx_space);
    let final_color = adjustments.global.agx_rendering_to_pipe_matrix * tonemapped_agx;
    return final_color;
}

fn legacy_tonemap(c: vec3<f32>) -> vec3<f32> {
    const a: f32 = 2.51;
    const b: f32 = 0.03;
    const c_const: f32 = 2.43;
    const d: f32 = 0.59;
    const e: f32 = 0.14;

    let x = max(c, vec3<f32>(0.0));

    let numerator = x * (a * x + b);
    let denominator = x * (c_const * x + d) + e;

    let tonemapped = select(vec3<f32>(0.0), numerator / denominator, denominator > vec3<f32>(0.00001));

    return clamp(tonemapped, vec3<f32>(0.0), vec3<f32>(1.0));
}

fn no_tonemap(c: vec3<f32>) -> vec3<f32> {
    return c;
}

fn is_default_curve(points: array<Point, 16>, count: u32) -> bool {
    if (count < 2u) {
        return false;
    }

    var is_identity = true;
    for (var i = 0u; i < count; i = i + 1u) {
        if (abs(points[i].x - points[i].y) > 0.5) {
            is_identity = false;
            break;
        }
    }

    let p0 = points[0];
    let p_last = points[count - 1u];
    let p0_is_origin = abs(p0.x - 0.0) < 0.1 && abs(p0.y - 0.0) < 0.1;
    let p_last_is_end = abs(p_last.x - 255.0) < 0.1 && abs(p_last.y - 255.0) < 0.1;

    return is_identity && p0_is_origin && p_last_is_end;
}

// Optical Assembly Math RGB2IQbQr & IQbQr2RGB (0x03250eb5)
fn rgb_to_iqbqr(rgb: vec3<f32>) -> vec3<f32> {
    let qr = (rgb.r - rgb.g) * 0.5;
    let qb = (rgb.b - rgb.g) * 0.5;
    let i = rgb.g + (qr + qb) * 0.5;
    return vec3<f32>(i, qb, qr);
}

fn iqbqr_to_rgb(iqbqr: vec3<f32>) -> vec3<f32> {
    let i = iqbqr.x;
    let qb = iqbqr.y;
    let qr = iqbqr.z;
    let g = i - (qr + qb) * 0.5;
    let r = g + 2.0 * qr;
    let b = g + 2.0 * qb;
    return vec3<f32>(r, g, b);
}

// Optical Exact Assembly Tone Curve via Max-Med-Min Fractional Hue Preservation (ApplyToneCurveOnMaxMedMin 0x04ad226f)
fn apply_tone_curve_max_med_min(
    rgb: vec3<f32>,
    curve: array<Point, 16>,
    curve_count: u32
) -> vec3<f32> {
    if (is_default_curve(curve, curve_count)) {
        return rgb;
    }
    var in_max = rgb.r;
    var in_med = rgb.g;
    var in_min = rgb.b;
    var max_chan = 0u;
    var med_chan = 1u;
    var min_chan = 2u;

    if (rgb.r >= rgb.g && rgb.r >= rgb.b) {
        in_max = rgb.r;
        max_chan = 0u;
        if (rgb.g >= rgb.b) {
            in_med = rgb.g; med_chan = 1u;
            in_min = rgb.b; min_chan = 2u;
        } else {
            in_med = rgb.b; med_chan = 2u;
            in_min = rgb.g; min_chan = 1u;
        }
    } else if (rgb.g >= rgb.r && rgb.g >= rgb.b) {
        in_max = rgb.g;
        max_chan = 1u;
        if (rgb.r >= rgb.b) {
            in_med = rgb.r; med_chan = 0u;
            in_min = rgb.b; min_chan = 2u;
        } else {
            in_med = rgb.b; med_chan = 2u;
            in_min = rgb.r; min_chan = 0u;
        }
    } else {
        in_max = rgb.b;
        max_chan = 2u;
        if (rgb.r >= rgb.g) {
            in_med = rgb.r; med_chan = 0u;
            in_min = rgb.g; min_chan = 1u;
        } else {
            in_med = rgb.g; med_chan = 1u;
            in_min = rgb.r; min_chan = 0u;
        }
    }

    let delta_in = in_max - in_min;
    let out_max = apply_curve(in_max, curve, curve_count);
    let out_min = apply_curve(in_min, curve, curve_count);

    var out_med = out_min;
    if (delta_in > 1e-5) {
        out_med = out_min + (out_max - out_min) * ((in_med - in_min) / delta_in);
    }

    var result: vec3<f32>;
    if (max_chan == 0u) {
        result.r = out_max;
        if (med_chan == 1u) { result.g = out_med; result.b = out_min; }
        else { result.b = out_med; result.g = out_min; }
    } else if (max_chan == 1u) {
        result.g = out_max;
        if (med_chan == 0u) { result.r = out_med; result.b = out_min; }
        else { result.b = out_med; result.r = out_min; }
    } else {
        result.b = out_max;
        if (med_chan == 0u) { result.r = out_med; result.g = out_min; }
        else { result.g = out_med; result.r = out_min; }
    }
    return result;
}

fn apply_all_curves(
    color: vec3<f32>,
    luma_curve: array<Point, 16>, luma_curve_count: u32,
    red_curve: array<Point, 16>, red_curve_count: u32,
    green_curve: array<Point, 16>, green_curve_count: u32,
    blue_curve: array<Point, 16>, blue_curve_count: u32
) -> vec3<f32> {
    let luma_curved = apply_tone_curve_max_med_min(color, luma_curve, luma_curve_count);
    var r = luma_curved.r;
    var g = luma_curved.g;
    var b = luma_curved.b;

    if (!is_default_curve(red_curve, red_curve_count)) {
        r = apply_curve(r, red_curve, red_curve_count);
    }
    if (!is_default_curve(green_curve, green_curve_count)) {
        g = apply_curve(g, green_curve, green_curve_count);
    }
    if (!is_default_curve(blue_curve, blue_curve_count)) {
        b = apply_curve(b, blue_curve, blue_curve_count);
    }

    return clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn get_mask_influence(mask_index: u32, coords: vec2<u32>) -> f32 {
    return textureLoad(mask_textures, vec2<i32>(coords), i32(mask_index), 0).r;
}

fn sample_lut_tetrahedral(uv: vec3<f32>) -> vec3<f32> {
    let dims = vec3<f32>(textureDimensions(lut_texture));
    let size = dims - vec3<f32>(1.0);
    let scaled = clamp(uv, vec3<f32>(0.0), vec3<f32>(1.0)) * size;
    let i_base = floor(scaled);
    let f = scaled - i_base;
    let coord0 = vec3<i32>(i_base);
    let coord1 = min(coord0 + vec3<i32>(1), vec3<i32>(dims) - vec3<i32>(1));
    let c000 = textureLoad(lut_texture, coord0, 0).rgb;
    let c111 = textureLoad(lut_texture, coord1, 0).rgb;

    var res = vec3<f32>(0.0);

    if (f.r > f.g) {
        if (f.g > f.b) {
            let c100 = textureLoad(lut_texture, vec3<i32>(coord1.x, coord0.y, coord0.z), 0).rgb;
            let c110 = textureLoad(lut_texture, vec3<i32>(coord1.x, coord1.y, coord0.z), 0).rgb;

            res = c000 * (1.0 - f.r) +
                  c100 * (f.r - f.g) +
                  c110 * (f.g - f.b) +
                  c111 * (f.b);
        } else if (f.r > f.b) {
            let c100 = textureLoad(lut_texture, vec3<i32>(coord1.x, coord0.y, coord0.z), 0).rgb;
            let c101 = textureLoad(lut_texture, vec3<i32>(coord1.x, coord0.y, coord1.z), 0).rgb;

            res = c000 * (1.0 - f.r) +
                  c100 * (f.r - f.b) +
                  c101 * (f.b - f.g) +
                  c111 * (f.g);
        } else {
            let c001 = textureLoad(lut_texture, vec3<i32>(coord0.x, coord0.y, coord1.z), 0).rgb;
            let c101 = textureLoad(lut_texture, vec3<i32>(coord1.x, coord0.y, coord1.z), 0).rgb;

            res = c000 * (1.0 - f.b) +
                  c001 * (f.b - f.r) +
                  c101 * (f.r - f.g) +
                  c111 * (f.g);
        }
    } else {
        if (f.b > f.g) {
            let c001 = textureLoad(lut_texture, vec3<i32>(coord0.x, coord0.y, coord1.z), 0).rgb;
            let c011 = textureLoad(lut_texture, vec3<i32>(coord0.x, coord1.y, coord1.z), 0).rgb;

            res = c000 * (1.0 - f.b) +
                  c001 * (f.b - f.g) +
                  c011 * (f.g - f.r) +
                  c111 * (f.r);
        } else if (f.b > f.r) {
            let c010 = textureLoad(lut_texture, vec3<i32>(coord0.x, coord1.y, coord0.z), 0).rgb;
            let c011 = textureLoad(lut_texture, vec3<i32>(coord0.x, coord1.y, coord1.z), 0).rgb;

            res = c000 * (1.0 - f.g) +
                  c010 * (f.g - f.b) +
                  c011 * (f.b - f.r) +
                  c111 * (f.r);
        } else {
            let c010 = textureLoad(lut_texture, vec3<i32>(coord0.x, coord1.y, coord0.z), 0).rgb;
            let c110 = textureLoad(lut_texture, vec3<i32>(coord1.x, coord1.y, coord0.z), 0).rgb;

            res = c000 * (1.0 - f.g) +
                  c010 * (f.g - f.r) +
                  c110 * (f.r - f.b) +
                  c111 * (f.b);
        }
    }

    return res;
}

// Phase 1: Color conversions and skin locus detection
fn rgb_to_ycbcr_bt709(c: vec3<f32>) -> vec3<f32> {
    let y  = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    let cb = (c.b - y) / 1.8556 + 0.5;
    let cr = (c.r - y) / 1.5748 + 0.5;
    return vec3<f32>(y, cb, cr);
}

fn compute_skin_shield(color_linear: vec3<f32>) -> f32 {
    let safe_rgb = max(color_linear, vec3<f32>(0.0));
    let srgb_approx = pow(safe_rgb, vec3<f32>(1.0 / 2.2));
    let ycbcr = rgb_to_ycbcr_bt709(srgb_approx);
    let cb_diff = ycbcr.y - 0.42;
    let cr_diff = ycbcr.z - 0.62;
    let skin_dist_sq = (cb_diff * cb_diff) / 0.0035 + (cr_diff * cr_diff) / 0.0045;
    return 1.0 - smoothstep(0.4, 1.8, skin_dist_sq);
}

// Phase 15: Dermatological 10-Cluster Skin Science & Deflash Specular Roll-off (PerfectlyClear 0x3bf00)
fn apply_skin_deflash_specular_rolloff(
    color_linear: vec3<f32>,
    skin_factor: f32,
    deflash_strength: f32
) -> vec3<f32> {
    if (skin_factor < 0.05 || deflash_strength <= 0.0) {
        return color_linear;
    }
    let luma = get_luma(max(color_linear, vec3<f32>(0.0)));
    let specular_knee = 0.72;
    if (luma <= specular_knee) {
        return color_linear;
    }
    let excess = luma - specular_knee;
    let max_headroom = max(1.2 - specular_knee, 0.01);
    let norm_excess = excess / max_headroom;
    let roll_off = tanh(norm_excess / (1.0 + deflash_strength * 1.5)) * max_headroom;
    let target_luma = specular_knee + roll_off;
    let ratio = target_luma / max(luma, 1e-4);
    let rolled_color = color_linear * ratio;
    return mix(color_linear, rolled_color, skin_factor * clamp(deflash_strength, 0.0, 1.0));
}

// Phase 1: Supercontrast 3-Band Parametric Cubic Hermite Pivots
fn apply_supercontrast_pivots(
    color: vec3<f32>,
    sc_shadows: f32,
    sc_midtones: f32,
    sc_highlights: f32,
    sc_pivots: f32
) -> vec3<f32> {
    if (abs(sc_shadows) < 0.001 && abs(sc_midtones) < 0.001 && abs(sc_highlights) < 0.001) {
        return color;
    }
    let luma = max(get_luma(color), 0.0001);
    let p_s = clamp(0.18 + 0.10 * sc_pivots, 0.05, 0.40);
    let p_m = 0.50;
    let p_h = clamp(0.82 - 0.10 * sc_pivots, 0.60, 0.95);

    var delta_s = 0.0;
    if (luma < p_m) {
        let t = clamp(luma / (p_s * 2.0), 0.0, 1.0);
        let hermite_s = t * t * (3.0 - 2.0 * t);
        delta_s = (hermite_s - t) * sc_shadows * 0.45;
    }
    var delta_h = 0.0;
    if (luma > p_m) {
        let t = clamp((luma - (2.0 * p_h - 1.0)) / (2.0 * (1.0 - p_h)), 0.0, 1.0);
        let hermite_h = t * t * (3.0 - 2.0 * t);
        delta_h = (hermite_h - t) * sc_highlights * 0.45;
    }
    let delta_m = (sin(luma * 3.14159265)) * sc_midtones * 0.35;
    let luma_new = max(luma + delta_s + delta_m + delta_h, 0.0001);
    return color * (luma_new / luma);
}

// Phase 1: SNS-HDR Lights Sub-Pipeline
fn apply_lights_subpipeline(
    color_in: vec3<f32>,
    lights_bright: f32,
    lights_con: f32,
    lights_det: f32,
    lights_temp: f32,
    lights_tint: f32,
    lights_sat: f32,
    lights_vib: f32,
    detail_structure: f32
) -> vec3<f32> {
    let luma = get_luma(max(color_in, vec3<f32>(0.0)));
    let w_lights = smoothstep(0.45, 0.85, luma);
    if (w_lights <= 0.001) {
        return color_in;
    }

    var mod_color = color_in;
    if (abs(lights_temp) > 0.001 || abs(lights_tint) > 0.001) {
        let r_gain = exp2(lights_temp * 0.4);
        let b_gain = exp2(-lights_temp * 0.4);
        let g_gain = exp2(-lights_tint * 0.3);
        mod_color = mod_color * vec3<f32>(r_gain, g_gain, b_gain);
    }
    if (abs(lights_bright) > 0.001) {
        mod_color = mod_color * exp2(lights_bright * 0.5);
    }
    if (abs(lights_con) > 0.001) {
        let base_luma = max(get_luma(mod_color), 0.0001);
        let norm_l = clamp((base_luma - 0.45) / 0.55, 0.0, 1.0);
        let con_l = pow(norm_l, exp2(-lights_con * 0.5));
        let new_l = 0.45 + con_l * 0.55;
        mod_color = mod_color * (new_l / base_luma);
    }
    if (abs(lights_det) > 0.001) {
        let detail_gain = exp2(detail_structure * lights_det * 1.5);
        mod_color = mod_color * detail_gain;
    }
    if (abs(lights_sat) > 0.001 || abs(lights_vib) > 0.001) {
        let cur_luma = get_luma(mod_color);
        let chroma = mod_color - vec3<f32>(cur_luma);
        let sat_mult = max(1.0 + lights_sat * 0.5, 0.0);
        mod_color = vec3<f32>(cur_luma) + chroma * sat_mult;
    }
    return mix(color_in, mod_color, w_lights);
}

// Phase 1: Foliage Chroma & Hue Enhancer (+15 deg emerald shift and selective saturation)
fn apply_foliage_enhancer(color: vec3<f32>, foliage_strength: f32) -> vec3<f32> {
    if (abs(foliage_strength) <= 0.001) {
        return color;
    }
    let safe_color = max(color, vec3<f32>(0.0));
    let srgb = linear_to_srgb_extended(safe_color);
    let hsv = rgb_to_hsv(srgb);
    let hue = hsv.x;
    let foliage_mask = smoothstep(55.0, 85.0, hue) * (1.0 - smoothstep(140.0, 175.0, hue));
    let sat_mask = smoothstep(0.08, 0.25, hsv.y);
    let total_mask = foliage_mask * sat_mask * clamp(foliage_strength, -1.0, 1.0);
    if (abs(total_mask) <= 0.001) {
        return color;
    }
    var new_hsv = hsv;
    new_hsv.x = (new_hsv.x + total_mask * 15.0 + 360.0) % 360.0;
    new_hsv.y = clamp(new_hsv.y * (1.0 + total_mask * 0.40), 0.0, 1.0);
    let shifted_srgb = hsv_to_rgb(new_hsv);
    let shifted_linear = srgb_to_linear(shifted_srgb);
    let base_luma = get_luma(safe_color);
    let new_luma = max(get_luma(shifted_linear), 0.0001);
    return shifted_linear * (base_luma / new_luma);
}

fn apply_glow_bloom(
    color: vec3<f32>,
    blurred_color_input_space: vec3<f32>,
    amount: f32,
    is_raw: u32,
    exp: f32, bright: f32, con: f32, wh: f32
) -> vec3<f32> {
    if (amount <= 0.0) {
        return color;
    }

    var blurred_linear: vec3<f32>;
    if (is_raw == 1u) {
        blurred_linear = blurred_color_input_space;
    } else {
        blurred_linear = srgb_to_linear(blurred_color_input_space);
    }

    blurred_linear = apply_linear_exposure(blurred_linear, exp);
    blurred_linear = apply_filmic_exposure(blurred_linear, bright);
    blurred_linear = apply_tonal_adjustments(blurred_linear, 0.0, 0.0, is_raw, 0.0, 0.0, wh, 0.0);

    let linear_luma = get_luma(max(blurred_linear, vec3<f32>(0.0)));

    var perceptual_luma: f32;
    if (linear_luma <= 1.0) {
        perceptual_luma = pow(max(linear_luma, 0.0), 1.0 / 2.2);
    } else {
        perceptual_luma = 1.0 + pow(linear_luma - 1.0, 1.0 / 2.2);
    }

    let luma_cutoff = mix(0.75, 0.08, clamp(amount, 0.0, 1.0));

    let cutoff_fade = smoothstep(
        luma_cutoff,
        luma_cutoff + 0.15,
        perceptual_luma
    );

    let excess = max(perceptual_luma - luma_cutoff, 0.0);

    let falloff_range = 5.5;
    let normalized = excess / falloff_range;

    let bloom_intensity =
        pow(smoothstep(0.0, 1.0, normalized), 0.45);

    var bloom_color: vec3<f32>;
    if (linear_luma > 0.01) {
        let color_ratio = blurred_linear / linear_luma;
        let warm_tint = vec3<f32>(1.03, 1.0, 0.97);
        bloom_color = color_ratio * warm_tint;
    } else {
        bloom_color = vec3<f32>(1.0, 0.99, 0.98);
    }

    let luma_factor = pow(linear_luma, 0.6);

    let black_gate_width = 0.5;
    let black_gate_raw = smoothstep(0.0, black_gate_width, linear_luma);
    let black_gate = pow(black_gate_raw, 0.5);

    bloom_color *= bloom_intensity * luma_factor * cutoff_fade * black_gate;

    let current_luma = get_luma(max(color, vec3<f32>(0.0)));
    let protection = 1.0 - smoothstep(1.0, 2.2, current_luma);

    return color + bloom_color * amount * 3.8 * protection;
}

fn apply_halation(
    color: vec3<f32>,
    blurred_color_input_space: vec3<f32>,
    amount: f32,
    is_raw: u32,
    exp: f32, bright: f32, con: f32, wh: f32
) -> vec3<f32> {
    if (amount <= 0.0) { return color; }

    var blurred_linear: vec3<f32>;
    if (is_raw == 1u) {
        blurred_linear = blurred_color_input_space;
    } else {
        blurred_linear = srgb_to_linear(blurred_color_input_space);
    }

    blurred_linear = apply_linear_exposure(blurred_linear, exp);
    blurred_linear = apply_filmic_exposure(blurred_linear, bright);
    blurred_linear = apply_tonal_adjustments(blurred_linear, 0.0, 0.0, is_raw, 0.0, 0.0, wh, 0.0);

    let linear_luma = get_luma(max(blurred_linear, vec3<f32>(0.0)));

    var perceptual_luma: f32;
    if (linear_luma <= 1.0) {
        perceptual_luma = pow(max(linear_luma, 0.0), 1.0 / 2.2);
    } else {
        perceptual_luma = 1.0 + pow(linear_luma - 1.0, 1.0 / 2.2);
    }

    let luma_cutoff = mix(0.85, 0.1, clamp(amount, 0.0, 1.0));

    if (perceptual_luma <= luma_cutoff) { return color; }

    let excess = perceptual_luma - luma_cutoff;
    let range = max(1.5 - luma_cutoff, 0.1);
    let halation_mask = smoothstep(0.0, range * 0.6, excess);

    let halation_core = vec3<f32>(1.0, 0.15, 0.03);
    let halation_fringe = vec3<f32>(1.0, 0.32, 0.10);

    let intensity_blend = smoothstep(0.0, 0.7, halation_mask);
    let halation_tint = mix(halation_fringe, halation_core, intensity_blend);

    let glow_intensity = halation_mask * linear_luma;
    let halation_glow = halation_tint * glow_intensity;

    let color_luma = get_luma(max(color, vec3<f32>(0.0)));
    let desat_strength = halation_mask * 0.12;
    let affected_color = mix(color, vec3<f32>(color_luma), desat_strength);

    let contrast_reduced = mix(vec3<f32>(0.5), affected_color, 1.0 - halation_mask * 0.06);

    return contrast_reduced + halation_glow * amount * 2.5;
}

// 6th-Order Radial Polynomial Inverse Distortion (Newton-Raphson solver for Optical UPolynomial6th)
fn invert_distortion_radius_newton(r_u: f32, k1: f32, k2: f32, k3: f32) -> f32 {
    var r = r_u;
    for (var i: u32 = 0u; i < 6u; i = i + 1u) {
        let r2 = r * r;
        let r4 = r2 * r2;
        let r6 = r4 * r2;
        let f = r * (1.0 + k1 * r2 + k2 * r4 + k3 * r6) - r_u;
        let f_prime = 1.0 + 3.0 * k1 * r2 + 5.0 * k2 * r4 + 7.0 * k3 * r6;
        if (abs(f_prime) < 1e-7) {
            break;
        }
        let delta = f / f_prime;
        r = r - delta;
        if (abs(delta) < 1e-6) {
            break;
        }
    }
    return r;
}

// Optical Wide-Angle Volume Deformation Anamorphosis (ComputeAnamorphosis 0x04e2de73)
fn apply_wide_angle_anamorphosis(norm_delta: vec2<f32>, k1: f32) -> vec2<f32> {
    if (k1 >= -0.015) {
        return norm_delta;
    }
    let a = clamp(-k1 * 0.35, 0.0, 0.08);
    let x_comp = norm_delta.x * (1.0 - a * norm_delta.x * norm_delta.x);
    let y_comp = norm_delta.y * (1.0 - a * 0.5 * norm_delta.y * norm_delta.y);
    return vec2<f32>(x_comp, y_comp);
}

// 6th-Order Radial Polynomial Distortion Coordinate Warp (Optical UPolynomial6th & ComputeAnamorphosis)
// Inverts rectilinear output UV coordinates to distorted sensor sampling coordinates with isotropic aspect-ratio scaling
fn distort_coord_6th_order(uv: vec2<f32>, center: vec2<f32>, k1: f32, k2: f32, k3: f32, crop_scale: f32, dims: vec2<f32>) -> vec2<f32> {
    let half_diagonal = 0.5 * length(dims);
    if (half_diagonal < 1e-4) {
        return uv;
    }
    let pixel_delta = (uv - center) * dims;
    let norm_delta = pixel_delta / half_diagonal;
    let eff_delta = apply_wide_angle_anamorphosis(norm_delta, k1);
    let r_u_raw = length(eff_delta);
    if (r_u_raw < 1e-6) {
        return uv;
    }
    let eff_scale = select(1.0, crop_scale, crop_scale > 0.0);
    let r_u = r_u_raw / eff_scale;
    let r_d = invert_distortion_radius_newton(r_u, k1, k2, k3);

    let norm_dir = eff_delta / r_u_raw;
    let warped_pixel = center * dims + (norm_dir * r_d * half_diagonal);
    return warped_pixel / dims;
}

// Bilinear texture sampler for warped sampling coordinates
fn sample_texture_bilinear(tex: texture_2d<f32>, uv: vec2<f32>, dims: vec2<f32>) -> vec4<f32> {
    let clamped_uv = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let coord = clamped_uv * dims - vec2<f32>(0.5);
    let i_coord = vec2<i32>(floor(coord));
    let f = coord - floor(coord);
    let max_c = vec2<i32>(dims) - vec2<i32>(1);

    let c00 = textureLoad(tex, clamp(i_coord, vec2<i32>(0), max_c), 0);
    let c10 = textureLoad(tex, clamp(i_coord + vec2<i32>(1, 0), vec2<i32>(0), max_c), 0);
    let c01 = textureLoad(tex, clamp(i_coord + vec2<i32>(0, 1), vec2<i32>(0), max_c), 0);
    let c11 = textureLoad(tex, clamp(i_coord + vec2<i32>(1, 1), vec2<i32>(0), max_c), 0);

    return mix(mix(c00, c10, f.x), mix(c01, c11, f.x), f.y);
}

// Optical HLR Clipped Highlight Reconstruction (CorrectionsAVX2 RecoverHighlightsAndApplyWBScales)
fn apply_highlight_chroma_reconstruction(
    linear_rgb: vec3<f32>,
    clip_ceiling: f32
) -> vec3<f32> {
    var c = linear_rgb;
    let is_r_clipped = c.r >= clip_ceiling;
    let is_g_clipped = c.g >= clip_ceiling;
    let is_b_clipped = c.b >= clip_ceiling;

    // Case 1: Exactly 1 channel clipped, 2 intact
    if (is_g_clipped && !is_r_clipped && !is_b_clipped) {
        let est_green = 0.5 * (c.r + c.b) * 1.15;
        c.g = max(c.g, est_green);
    } else if (is_r_clipped && !is_g_clipped && !is_b_clipped) {
        let est_red = max(c.g * 1.25, 0.5 * (c.g + c.b) * 1.30);
        c.r = max(c.r, est_red);
    } else if (is_b_clipped && !is_r_clipped && !is_g_clipped) {
        let est_blue = max(c.g * 0.95, 0.5 * (c.r + c.g) * 0.90);
        c.b = max(c.b, est_blue);
    }
    // Case 2: Exactly 2 channels clipped, 1 intact
    else if (is_r_clipped && is_g_clipped && !is_b_clipped) {
        let est_level = c.b * 1.45;
        c.r = max(c.r, est_level);
        c.g = max(c.g, est_level * 0.90);
    } else if (!is_r_clipped && is_g_clipped && is_b_clipped) {
        c.g = max(c.g, c.r * 1.15);
        c.b = max(c.b, c.r * 1.35);
    } else if (is_r_clipped && !is_g_clipped && is_b_clipped) {
        c.r = max(c.r, c.g * 1.05);
        c.b = max(c.b, c.g * 0.95);
    }

    // Case 3: Smooth highlight compression into specular white
    let max_val = max(c.r, max(c.g, c.b));
    if (max_val > clip_ceiling) {
        let excess = max_val - clip_ceiling;
        let desat = clamp(excess / (clip_ceiling * 0.4), 0.0, 1.0);
        c = mix(c, vec3<f32>(max_val), desat * 0.85);
    }
    return c;
}

// Optical ClearView Plus Haze Inversion Kernel with Contrast Preservation & Anti-Crush (CorrectionsAVX2 ApplyHazeRemoval_throws)
fn apply_atmospheric_haze_inversion(
    rgb: vec3<f32>,
    airlight: vec3<f32>,
    transmission_map_val: f32,
    intensity_0_100: f32
) -> vec3<f32> {
    if (intensity_0_100 <= 0.0) {
        return rgb;
    }
    let k = intensity_0_100 / 100.0;
    let t0 = 0.12;
    let t = clamp(transmission_map_val, t0, 1.0);
    let eff_t = mix(1.0, t, k);

    // Physical inversion with floor protection to prevent shadow crushing
    let unconstrained_dehazed = (rgb - airlight) / eff_t + airlight;
    let raw_dehazed = max(unconstrained_dehazed, rgb * (1.0 - k * 0.25));

    // Luminance-anchored contrast preservation to prevent shadow crushing
    let in_luma = dot(rgb, LUMA_COEFF);
    let air_luma = dot(airlight, LUMA_COEFF);
    let dehazed_luma = (in_luma - air_luma) / eff_t + air_luma;

    // Soft shadow preservation knee: as in_luma approaches 0, preserve floor
    let shadow_weight = smoothstep(0.0, 0.18, in_luma);
    let preserved_luma = mix(in_luma * 0.9, max(dehazed_luma, 0.0), shadow_weight);

    // Chromatic ratio preservation: prevent channel clipping & hue shifts
    let preserved_rgb = rgb * (preserved_luma / max(in_luma, 1e-5));
    let final_dehazed = mix(raw_dehazed, preserved_rgb, 0.45);

    // Soft-knee highlight saturation clamp to prevent out-of-gamut banding
    let max_c = max(final_dehazed.r, max(final_dehazed.g, final_dehazed.b));
    if (max_c > 1.0) {
        let excess = max_c - 1.0;
        let desat = clamp(excess / 0.5, 0.0, 0.7);
        return clamp(mix(final_dehazed, vec3<f32>(preserved_luma), desat), vec3<f32>(0.0), vec3<f32>(1.5));
    }

    return clamp(final_dehazed, vec3<f32>(0.0), vec3<f32>(1.5));
}

// Optical Deep Shadow Desaturation (Low-SNR Chroma Noise Suppression)
fn apply_deep_shadow_chroma_damping(rgb: vec3<f32>) -> vec3<f32> {
    let luma = dot(rgb, LUMA_COEFF);
    if (luma < 0.045) {
        let t = smoothstep(0.005, 0.045, max(luma, 0.0));
        return mix(vec3<f32>(luma), rgb, t);
    }
    return rgb;
}

// Optical Out-of-Gamut Luminance-Preserving Color Protection (OutputSaturatedColorsProtection = 100)
fn apply_gamut_luminance_protection(rgb: vec3<f32>) -> vec3<f32> {
    let max_val = max(rgb.r, max(rgb.g, rgb.b));
    let min_val = min(rgb.r, min(rgb.g, rgb.b));
    if (max_val <= 1.0 && min_val >= 0.0) {
        return rgb;
    }
    let target_luma = dot(rgb, LUMA_COEFF);
    let luma_clamped = clamp(target_luma, 0.0, 1.0);
    let chroma = rgb - vec3<f32>(target_luma);

    var scale = 1.0;
    if (max_val > 1.0) {
        let max_headroom = max_val - target_luma;
        if (max_headroom > 1e-5) {
            let allowed = (1.0 - luma_clamped) / max_headroom;
            scale = min(scale, allowed);
        }
    }
    if (min_val < 0.0) {
        let min_headroom = target_luma - min_val;
        if (min_headroom > 1e-5) {
            let allowed = luma_clamped / min_headroom;
            scale = min(scale, allowed);
        }
    }
    let compressed_chroma = chroma * clamp(scale, 0.0, 1.0);
    return clamp(vec3<f32>(luma_clamped) + compressed_chroma, vec3<f32>(0.0), vec3<f32>(1.0));
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let out_dims = vec2<u32>(textureDimensions(output_texture));
    if (id.x >= out_dims.x || id.y >= out_dims.y) { return; }

    const REFERENCE_DIMENSION: f32 = 1080.0;
    let full_dims = vec2<f32>(textureDimensions(input_texture));
    let current_ref_dim = min(full_dims.x, full_dims.y);
    let scale = max(0.1, current_ref_dim / REFERENCE_DIMENSION);

    let absolute_coord = id.xy + vec2<u32>(adjustments.tile_offset_x, adjustments.tile_offset_y);
    let absolute_coord_i = vec2<i32>(absolute_coord);

    let sample_pos = vec2<f32>(absolute_coord) + vec2<f32>(0.5);
    var sample_uv = sample_pos / full_dims;
    var is_lens_warped = false;
    if (adjustments.global.lens_distortion_enabled == 1u) {
        let k1 = adjustments.global.lens_k1;
        let k2 = adjustments.global.lens_k2;
        let k3 = adjustments.global.lens_k3;
        if (abs(k1) > 1e-6 || abs(k2) > 1e-6 || abs(k3) > 1e-6) {
            let center = select(vec2<f32>(0.5, 0.5), vec2<f32>(adjustments.global.lens_center_x, adjustments.global.lens_center_y), adjustments.global.lens_center_x > 0.0);
            let crop_scale = select(1.0, adjustments.global.lens_autocrop_scale, adjustments.global.lens_autocrop_scale > 0.0);
            sample_uv = distort_coord_6th_order(sample_uv, center, k1, k2, k3, crop_scale, full_dims);
            is_lens_warped = true;
        }
    }

    var color_from_texture: vec3<f32>;
    var original_alpha: f32 = 1.0;
    if (is_lens_warped) {
        let sampled = sample_texture_bilinear(input_texture, sample_uv, full_dims);
        color_from_texture = sampled.rgb;
        original_alpha = sampled.a;
    } else {
        color_from_texture = textureLoad(input_texture, absolute_coord, 0).rgb;
        original_alpha = textureLoad(input_texture, absolute_coord, 0).a;
    }

    let ca_rc = adjustments.global.chromatic_aberration_red_cyan;
    let ca_by = adjustments.global.chromatic_aberration_blue_yellow;
    let effective_coord = vec2<u32>(clamp(sample_uv * full_dims, vec2<f32>(0.0), full_dims - vec2<f32>(1.0)));
    let ca_active = abs(ca_rc) > 1e-6 || abs(ca_by) > 1e-6;
    if (ca_active) {
        color_from_texture = apply_ca_correction(effective_coord, ca_rc, ca_by);
        let defringe_strength = max(max(abs(ca_rc), abs(ca_by)), 0.75);
        color_from_texture = apply_optical_chromatic_defringe(color_from_texture, effective_coord, full_dims, defringe_strength);
    }

    var initial_linear_rgb: vec3<f32>;
    let is_raw = adjustments.global.is_raw_image;
    if (is_raw == 0u) {
        initial_linear_rgb = srgb_to_linear(color_from_texture);
    } else {
        initial_linear_rgb = color_from_texture;
    }

    if (adjustments.global.hlr_enabled == 1u) {
        initial_linear_rgb = apply_highlight_chroma_reconstruction(initial_linear_rgb, adjustments.global.hlr_clip_ceiling);
    }

    var t_exposure = adjustments.global.exposure;
    var t_brightness = adjustments.global.brightness;
    var t_contrast = adjustments.global.contrast;
    var t_highlights = adjustments.global.highlights;
    var t_shadows = adjustments.global.shadows;
    var t_whites = adjustments.global.whites;
    var t_blacks = adjustments.global.blacks;
    var t_saturation = adjustments.global.saturation;
    var t_wb_log_gains = vec3<f32>(adjustments.global.wb_log_gain_l, adjustments.global.wb_log_gain_m, adjustments.global.wb_log_gain_s);
    var t_vibrance = adjustments.global.vibrance;
    var t_luma_nr = adjustments.global.luma_noise_reduction;
    var t_color_nr = adjustments.global.color_noise_reduction;
    var t_clarity = adjustments.global.clarity;
    var t_dehaze = adjustments.global.dehaze;
    var t_structure = adjustments.global.structure;
    var t_glow = adjustments.global.glow_amount;
    var t_halation = adjustments.global.halation_amount;
    var t_flare = adjustments.global.flare_amount;
    var t_sharpness = adjustments.global.sharpness;
    var t_sharp_thresh = adjustments.global.sharpness_threshold;
    var t_hue = adjustments.global.hue;

    var h0_h = adjustments.global.hsl[0].hue; var h0_s = adjustments.global.hsl[0].saturation; var h0_l = adjustments.global.hsl[0].luminance;
    var h1_h = adjustments.global.hsl[1].hue; var h1_s = adjustments.global.hsl[1].saturation; var h1_l = adjustments.global.hsl[1].luminance;
    var h2_h = adjustments.global.hsl[2].hue; var h2_s = adjustments.global.hsl[2].saturation; var h2_l = adjustments.global.hsl[2].luminance;
    var h3_h = adjustments.global.hsl[3].hue; var h3_s = adjustments.global.hsl[3].saturation; var h3_l = adjustments.global.hsl[3].luminance;
    var h4_h = adjustments.global.hsl[4].hue; var h4_s = adjustments.global.hsl[4].saturation; var h4_l = adjustments.global.hsl[4].luminance;
    var h5_h = adjustments.global.hsl[5].hue; var h5_s = adjustments.global.hsl[5].saturation; var h5_l = adjustments.global.hsl[5].luminance;
    var h6_h = adjustments.global.hsl[6].hue; var h6_s = adjustments.global.hsl[6].saturation; var h6_l = adjustments.global.hsl[6].luminance;
    var h7_h = adjustments.global.hsl[7].hue; var h7_s = adjustments.global.hsl[7].saturation; var h7_l = adjustments.global.hsl[7].luminance;

    for (var i = 0u; i < adjustments.mask_count; i = i + 1u) {
        let influence = get_mask_influence(i, absolute_coord);
        if (influence > 0.001) {
            let m = adjustments.mask_adjustments[i];

            t_exposure += m.exposure * influence;
            t_brightness += m.brightness * influence;
            t_contrast += m.contrast * influence;
            t_highlights += m.highlights * influence;
            t_shadows += m.shadows * influence;
            t_whites += m.whites * influence;
            t_blacks += m.blacks * influence;

            t_saturation += m.saturation * influence;
            t_wb_log_gains += vec3<f32>(m.wb_log_gain_l, m.wb_log_gain_m, m.wb_log_gain_s) * influence;
            t_vibrance += m.vibrance * influence;

            t_luma_nr += m.luma_noise_reduction * influence;
            t_color_nr += m.color_noise_reduction * influence;
            t_clarity += m.clarity * influence;
            t_dehaze += m.dehaze * influence;
            t_structure += m.structure * influence;

            t_glow += m.glow_amount * influence;
            t_halation += m.halation_amount * influence;
            t_flare += m.flare_amount * influence;
            t_hue += m.hue * influence;
            t_sharpness += m.sharpness * influence;

            h0_h += m.hsl[0].hue * influence; h0_s += m.hsl[0].saturation * influence; h0_l += m.hsl[0].luminance * influence;
            h1_h += m.hsl[1].hue * influence; h1_s += m.hsl[1].saturation * influence; h1_l += m.hsl[1].luminance * influence;
            h2_h += m.hsl[2].hue * influence; h2_s += m.hsl[2].saturation * influence; h2_l += m.hsl[2].luminance * influence;
            h3_h += m.hsl[3].hue * influence; h3_s += m.hsl[3].saturation * influence; h3_l += m.hsl[3].luminance * influence;
            h4_h += m.hsl[4].hue * influence; h4_s += m.hsl[4].saturation * influence; h4_l += m.hsl[4].luminance * influence;
            h5_h += m.hsl[5].hue * influence; h5_s += m.hsl[5].saturation * influence; h5_l += m.hsl[5].luminance * influence;
            h6_h += m.hsl[6].hue * influence; h6_s += m.hsl[6].saturation * influence; h6_l += m.hsl[6].luminance * influence;
            h7_h += m.hsl[7].hue * influence; h7_s += m.hsl[7].saturation * influence; h7_l += m.hsl[7].luminance * influence;
        }
    }

    let final_hsl = array<HslColor, 8>(
        HslColor(h0_h, h0_s, h0_l, 0.0), HslColor(h1_h, h1_s, h1_l, 0.0),
        HslColor(h2_h, h2_s, h2_l, 0.0), HslColor(h3_h, h3_s, h3_l, 0.0),
        HslColor(h4_h, h4_s, h4_l, 0.0), HslColor(h5_h, h5_s, h5_l, 0.0),
        HslColor(h6_h, h6_s, h6_l, 0.0), HslColor(h7_h, h7_s, h7_l, 0.0)
    );

    initial_linear_rgb = apply_noise_reduction(
        initial_linear_rgb, absolute_coord_i,
        t_luma_nr, t_color_nr, scale, is_raw
    );

    let sharpness_blurred = textureLoad(sharpness_blur_texture, id.xy, 0).rgb;
    let tonal_blurred = textureLoad(tonal_blur_texture, id.xy, 0).rgb;
    let clarity_blurred = textureLoad(clarity_blur_texture, id.xy, 0).rgb;
    let structure_blurred = textureLoad(structure_blur_texture, id.xy, 0).rgb;

    var locally_contrasted_rgb = initial_linear_rgb;

    locally_contrasted_rgb = apply_sharpen(
        locally_contrasted_rgb,
        sharpness_blurred, tonal_blurred,
        absolute_coord_i, t_sharpness, t_sharp_thresh, is_raw
    );

    // Spatially-Varying Lens Softness & FocusMask Bokeh Protection (Optical SpatialBlurCorrection)
    if (adjustments.global.corner_sharpness_boost > 0.0 || adjustments.global.bokeh_mask_protection > 0.0) {
        let half_diag = 0.5 * length(full_dims);
        let r_soft = clamp(length((sample_uv - vec2<f32>(0.5, 0.5)) * full_dims) / max(half_diag, 1e-4), 0.0, 1.414);
        let local_grad = length(locally_contrasted_rgb - sharpness_blurred);
        let focus_prob = smoothstep(0.008, 0.05, local_grad);
        let focus_mask = mix(1.0, focus_prob, adjustments.global.bokeh_mask_protection / 100.0);
        let corner_boost = 1.0 + (adjustments.global.corner_sharpness_boost / 100.0) * r_soft * r_soft;
        let delta_softness = (locally_contrasted_rgb - sharpness_blurred) * ((corner_boost - 1.0) * focus_mask) * 0.5;
        locally_contrasted_rgb = locally_contrasted_rgb + clamp(delta_softness, vec3<f32>(-0.2), vec3<f32>(0.2));
    }

    // MicroContrast V3 Bandpass Texture Shader (Multi-Scale Bandpass)
    if (abs(adjustments.global.micro_contrast) > 0.5) {
        let micro_detail = locally_contrasted_rgb - sharpness_blurred;
        let gain = (adjustments.global.micro_contrast / 100.0) * 0.75;
        let amplified = sign(micro_detail) * min(abs(micro_detail) * (1.0 + abs(gain)), vec3<f32>(0.15));
        locally_contrasted_rgb = clamp(locally_contrasted_rgb + amplified * sign(gain), vec3<f32>(0.0), vec3<f32>(1.5));
    }

    // ClearView Plus Atmospheric Haze Inversion (Optical ClearView Plus)
    if (adjustments.global.clearview_strength > 0.0) {
        let airlight = vec3<f32>(0.85, 0.90, 0.95);
        let inv_airlight = vec3<f32>(1.0) / airlight;
        let c_coord = vec2<i32>(sample_uv * full_dims);
        let max_c = vec2<i32>(full_dims) - vec2<i32>(1);

        var min_dark = min(locally_contrasted_rgb.r * inv_airlight.r, min(locally_contrasted_rgb.g * inv_airlight.g, locally_contrasted_rgb.b * inv_airlight.b));
        let s_u = textureLoad(input_texture, clamp(c_coord + vec2<i32>(0, -2), vec2<i32>(0), max_c), 0).rgb;
        let s_d = textureLoad(input_texture, clamp(c_coord + vec2<i32>(0,  2), vec2<i32>(0), max_c), 0).rgb;
        let s_l = textureLoad(input_texture, clamp(c_coord + vec2<i32>(-2, 0), vec2<i32>(0), max_c), 0).rgb;
        let s_r = textureLoad(input_texture, clamp(c_coord + vec2<i32>( 2, 0), vec2<i32>(0), max_c), 0).rgb;

        min_dark = min(min_dark, min(s_u.r * inv_airlight.r, min(s_u.g * inv_airlight.g, s_u.b * inv_airlight.b)));
        min_dark = min(min_dark, min(s_d.r * inv_airlight.r, min(s_d.g * inv_airlight.g, s_d.b * inv_airlight.b)));
        min_dark = min(min_dark, min(s_l.r * inv_airlight.r, min(s_l.g * inv_airlight.g, s_l.b * inv_airlight.b)));
        min_dark = min(min_dark, min(s_r.r * inv_airlight.r, min(s_r.g * inv_airlight.g, s_r.b * inv_airlight.b)));

        let transmission = 1.0 - 0.95 * clamp(min_dark, 0.0, 1.0);
        locally_contrasted_rgb = apply_atmospheric_haze_inversion(locally_contrasted_rgb, airlight, transmission, adjustments.global.clearview_strength);
    }


    let gf = sample_gf_coeffs(absolute_coord);
    let gf_luma = max(get_luma(max(locally_contrasted_rgb, vec3<f32>(0.0))), GF_LUMA_FLOOR);
    let gf_i = log2(gf_luma);
    let detail_clarity = gf_i - (gf.x * gf_i + gf.y);
    var detail_structure = 0.0;
    if (t_structure != 0.0) {
        let gf_s = sample_gf_tex(gf_dehaze_texture, absolute_coord).xy;
        detail_structure = gf_i - (gf_s.x * gf_i + gf_s.y);
    }

    var lc_gate = smoothstep(GF_NOISE_LO, GF_NOISE_HI, gf_luma);
    if (is_raw == 0u) {
        lc_gate *= 1.0 - smoothstep(0.9, 1.0, gf_luma);
    }

    let clarity_total = t_clarity + centre_clarity_strength(adjustments.global.centre, vec2<f32>(adjustments.global.centre_x, adjustments.global.centre_y), absolute_coord_i);
    let structure_smooth = clamp(-t_structure, 0.0, 1.0);
    let clarity_overlap = select(1.0, 1.0 - structure_smooth, clarity_total < 0.0);
    let clarity_term = shape_detail_gf(detail_clarity)
                       * clarity_total * clarity_overlap
                       * select(GF_NEG_GAIN, GF_CLARITY_GAIN, clarity_total > 0.0);
    // Phase 1: Skin-Shielded Structure (Luminar Neo parity)
    let skin_factor = compute_skin_shield(locally_contrasted_rgb);
    let skin_shield_strength = adjustments.global.skin_shield_strength;
    let skin_suppression = 1.0 - clamp(skin_factor * max(skin_shield_strength, 0.85), 0.0, 0.92);

    // Phase 1: ContrastCleaner halo-free overshoot suppression (SNS-HDR parity)
    let cleaner_strength = adjustments.global.contrast_cleaner_strength;
    let gradient_overshoot = clamp(abs(detail_structure) - 0.45, 0.0, 1.0);
    let halo_cleaner = 1.0 - smoothstep(0.0, 0.5, gradient_overshoot * max(cleaner_strength, 1.0));

    let structure_term = shape_detail_gf(detail_structure)
                       * t_structure * skin_suppression * halo_cleaner
                       * select(GF_NEG_GAIN, GF_STRUCTURE_GAIN, t_structure > 0.0);

    let lc_log_gain = clarity_term * select(lc_gate, 1.0, clarity_total < 0.0)
                    + structure_term * select(lc_gate, 1.0, t_structure < 0.0);
    locally_contrasted_rgb *= exp2(lc_log_gain);
    let deflash_strength = adjustments.global.skin_shield_strength * 0.65;
    locally_contrasted_rgb = apply_skin_deflash_specular_rolloff(locally_contrasted_rgb, skin_factor, deflash_strength);

    var processed_rgb = apply_linear_exposure(locally_contrasted_rgb, t_exposure);

    let orton_spread = adjustments.global.orton_spread;
    if (t_glow > 0.0 || orton_spread > 0.0) {
        let effective_blur = mix(structure_blurred, clarity_blurred, clamp(orton_spread, 0.0, 1.0));
        let effective_glow = max(t_glow, orton_spread * 0.65);
        processed_rgb = apply_glow_bloom(
            processed_rgb, effective_blur, effective_glow, is_raw,
            t_exposure, t_brightness, t_contrast, t_whites
        );
    }
    if (t_halation > 0.0) {
        processed_rgb = apply_halation(
            processed_rgb, clarity_blurred, t_halation, is_raw,
            t_exposure, t_brightness, t_contrast, t_whites
        );
    }
    if (t_flare > 0.0) {
        let uv = vec2<f32>(absolute_coord) / full_dims;
        var flare_color = textureSampleLevel(flare_texture, flare_sampler, uv, 0.0).rgb;
        flare_color *= 1.4;
        flare_color = flare_color * flare_color;
        let linear_luma = get_luma(max(processed_rgb, vec3<f32>(0.0)));
        var perceptual_luma: f32;
        if (linear_luma <= 1.0) {
            perceptual_luma = pow(max(linear_luma, 0.0), 1.0 / 2.2);
        } else {
            perceptual_luma = 1.0 + pow(linear_luma - 1.0, 1.0 / 2.2);
        }
        let protection = 1.0 - smoothstep(0.7, 1.8, perceptual_luma);
        processed_rgb += flare_color * t_flare * protection;
    }

    var dehaze_dark = 0.0;
    if (t_dehaze > 0.0) {
        dehaze_dark = refined_dark_channel(absolute_coord, is_raw);
    }
    var composite_rgb_linear = apply_dehaze(processed_rgb, structure_blurred, dehaze_dark, is_raw, t_dehaze);
    composite_rgb_linear = apply_white_balance(composite_rgb_linear, t_wb_log_gains);
    composite_rgb_linear = apply_centre_tonal_and_color(composite_rgb_linear, adjustments.global.centre, vec2<f32>(adjustments.global.centre_x, adjustments.global.centre_y), absolute_coord_i);
    var tonal_log_detail = 0.0;
    var tonal_mid_detail = 0.0;
    if (t_shadows != 0.0 || t_blacks != 0.0 || t_highlights != 0.0 || t_whites != 0.0) {
        tonal_log_detail = (gf_i - (gf.z * gf_i + gf.w)) + lc_log_gain;
        let gf_broad = sample_gf_tex(gf_dehaze_texture, absolute_coord).xy;
        tonal_mid_detail = (gf.z * gf_i + gf.w) - (gf_broad.x * gf_i + gf_broad.y);
    }
    composite_rgb_linear = apply_tonal_adjustments(composite_rgb_linear, tonal_log_detail, tonal_mid_detail, is_raw, t_contrast, t_shadows, t_whites, t_blacks);
    composite_rgb_linear = apply_highlights_adjustment(composite_rgb_linear, tonal_log_detail, tonal_mid_detail, is_raw, t_highlights);
    composite_rgb_linear = apply_color_calibration(composite_rgb_linear, adjustments.global.color_calibration);
    composite_rgb_linear = apply_hsl_panel(composite_rgb_linear, final_hsl, absolute_coord_i);
    // Phase 1: Foliage Enhancer
    composite_rgb_linear = apply_foliage_enhancer(composite_rgb_linear, adjustments.global.foliage_enhancer);
    composite_rgb_linear = apply_hue_shift(composite_rgb_linear, t_hue);
    composite_rgb_linear = apply_creative_color(composite_rgb_linear, t_saturation, t_vibrance);

    // Phase 1: Supercontrast 3-Band Parametric Cubic Hermite Pivots
    composite_rgb_linear = apply_supercontrast_pivots(
        composite_rgb_linear,
        adjustments.global.supercontrast_shadows,
        adjustments.global.supercontrast_midtones,
        adjustments.global.supercontrast_highlights,
        adjustments.global.supercontrast_pivots
    );

    // Phase 1: SNS-HDR Lights Sub-Pipeline
    composite_rgb_linear = apply_lights_subpipeline(
        composite_rgb_linear,
        adjustments.global.lights_brightness,
        adjustments.global.lights_contrast,
        adjustments.global.lights_details,
        adjustments.global.lights_wb_temp,
        adjustments.global.lights_wb_tint,
        adjustments.global.lights_saturation,
        adjustments.global.lights_vibrance,
        detail_structure
    );

    composite_rgb_linear = apply_color_grading(
        composite_rgb_linear,
        adjustments.global.color_grading_shadows,
        adjustments.global.color_grading_midtones,
        adjustments.global.color_grading_highlights,
        adjustments.global.color_grading_global,
        adjustments.global.color_grading_blending,
        adjustments.global.color_grading_balance
    );

    for (var i = 0u; i < adjustments.mask_count; i = i + 1u) {
        let influence = get_mask_influence(i, absolute_coord);
        if (influence > 0.001) {
            let m = adjustments.mask_adjustments[i];
            let mask_graded = apply_color_grading(
                composite_rgb_linear,
                m.color_grading_shadows, m.color_grading_midtones, m.color_grading_highlights, m.color_grading_global, m.color_grading_blending, m.color_grading_balance
            );
            composite_rgb_linear = mix(composite_rgb_linear, mask_graded, influence);
        }
    }

    if (adjustments.global.vignette_amount != 0.0) {
        let full_dims_f = vec2<f32>(textureDimensions(input_texture));
        let coord_f = vec2<f32>(absolute_coord);
        let v_amount = adjustments.global.vignette_amount;
        let v_mid = adjustments.global.vignette_midpoint;
        let v_round = 1.0 - adjustments.global.vignette_roundness;
        let v_feather = adjustments.global.vignette_feather * 0.5;
        let aspect = full_dims_f.y / full_dims_f.x;
        let v_center = vec2<f32>(adjustments.global.vignette_center_x, adjustments.global.vignette_center_y);
        let uv_centered = (coord_f / full_dims_f - v_center) * 2.0;
        let uv_round = sign(uv_centered) * pow(abs(uv_centered), vec2<f32>(v_round, v_round));
        let d = length(uv_round * vec2<f32>(1.0, aspect)) * 0.5;
        let vignette_mask = smoothstep(v_mid - v_feather, v_mid + v_feather, d);
        if (v_amount < 0.0) {
            composite_rgb_linear *= (1.0 + v_amount * vignette_mask);
        } else {
            composite_rgb_linear = mix(composite_rgb_linear, vec3<f32>(1.0), v_amount * vignette_mask);
        }
    }

    var default_tonemapped: vec3<f32>;
    if (adjustments.global.tonemapper_mode == 1u || (is_raw == 1u && adjustments.global.tonemapper_mode == 0u)) {
        default_tonemapped = agx_full_transform(composite_rgb_linear);
    } else if (is_raw == 1u) {
        var srgb_emulated = linear_to_srgb(composite_rgb_linear);
        let srgb_clamped = clamp(srgb_emulated, vec3<f32>(0.0), vec3<f32>(1.0));
        const BRIGHTNESS_GAMMA: f32 = 1.1;
        let srgb_gamma = pow(srgb_clamped, vec3<f32>(1.0 / BRIGHTNESS_GAMMA));
        const CONTRAST_MIX: f32 = 0.75;
        let contrast_curve = srgb_gamma * srgb_gamma * (3.0 - 2.0 * srgb_gamma);
        let tonemapped_clamped = mix(srgb_gamma, contrast_curve, CONTRAST_MIX);
        default_tonemapped = select(tonemapped_clamped, srgb_emulated, srgb_emulated > vec3<f32>(1.0));
    } else {
        default_tonemapped = linear_to_srgb(composite_rgb_linear);
    }
    var base_srgb: vec3<f32>;
    let is_scene_lut = (adjustments.global.has_lut == 1u && adjustments.global.lut_is_scene_referred == 1u);

    if (is_scene_lut) {
        let vlog_encoded = linear_to_vlog(composite_rgb_linear);
        let lut_color = sample_lut_tetrahedral(vlog_encoded);
        base_srgb = mix(default_tonemapped, lut_color, adjustments.global.lut_intensity);
    } else {
        base_srgb = default_tonemapped;
    }

    base_srgb = apply_filmic_exposure(base_srgb, t_brightness);

    var final_rgb = apply_all_curves(base_srgb,
        adjustments.global.luma_curve, adjustments.global.luma_curve_count,
        adjustments.global.red_curve, adjustments.global.red_curve_count,
        adjustments.global.green_curve, adjustments.global.green_curve_count,
        adjustments.global.blue_curve, adjustments.global.blue_curve_count
    );

    for (var i = 0u; i < adjustments.mask_count; i = i + 1u) {
        let influence = get_mask_influence(i, absolute_coord);
        if (influence > 0.001) {
            let m = adjustments.mask_adjustments[i];
            let mask_curved_srgb = apply_all_curves(final_rgb,
                m.luma_curve, m.luma_curve_count,
                m.red_curve, m.red_curve_count,
                m.green_curve, m.green_curve_count,
                m.blue_curve, m.blue_curve_count
            );
            final_rgb = mix(final_rgb, mask_curved_srgb, influence);
        }
    }

    if (adjustments.global.has_lut == 1u && adjustments.global.lut_is_scene_referred == 0u) {
        let lut_color = sample_lut_tetrahedral(final_rgb);
        final_rgb = mix(final_rgb, lut_color, adjustments.global.lut_intensity);
    }

    if (adjustments.global.grain_amount > 0.0) {
        let coord = vec2<f32>(absolute_coord_i);
        let amount = adjustments.global.grain_amount * 0.5;
        let grain_frequency = (1.0 / max(adjustments.global.grain_size, 0.1)) / scale;
        let roughness = adjustments.global.grain_roughness;
        let luma = max(0.0, get_luma(final_rgb));
        let luma_mask = smoothstep(0.0, 0.15, luma) * (1.0 - smoothstep(0.6, 1.0, luma));
        let base_coord = coord * grain_frequency;
        let rough_coord = coord * grain_frequency * 0.6;
        let noise_base = gradient_noise(base_coord);
        let noise_rough = gradient_noise(rough_coord + vec2<f32>(5.2, 1.3));
        let noise_val = mix(noise_base, noise_rough, roughness);
        final_rgb += vec3<f32>(noise_val) * amount * luma_mask;
    }

    // Optical Deep Shadow Desaturation (Low-SNR Chroma Noise Suppression)
    final_rgb = apply_deep_shadow_chroma_damping(final_rgb);

    // Optical Out-of-Gamut Luminance-Preserving Color Protection
    final_rgb = apply_gamut_luminance_protection(final_rgb);

    if (adjustments.global.show_clipping == 1u) {
        let HIGHLIGHT_WARNING_COLOR = vec3<f32>(1.0, 0.0, 0.0);
        let SHADOW_WARNING_COLOR = vec3<f32>(0.0, 0.0, 1.0);
        let HIGHLIGHT_CLIP_THRESHOLD = 0.998;
        let SHADOW_CLIP_THRESHOLD = 0.002;
        if (any(final_rgb > vec3<f32>(HIGHLIGHT_CLIP_THRESHOLD))) {
            final_rgb = HIGHLIGHT_WARNING_COLOR;
        } else if (any(final_rgb < vec3<f32>(SHADOW_CLIP_THRESHOLD))) {
            final_rgb = SHADOW_WARNING_COLOR;
        }
    }

    if (HIGH_PRECISION_OUTPUT == 0u) {
        let dither_amount = 1.0 / 255.0;
        final_rgb += dither(id.xy) * dither_amount;
    }

    textureStore(output_texture, id.xy, vec4<f32>(clamp(final_rgb, vec3<f32>(0.0), vec3<f32>(1.0)), original_alpha));
}
