//! RapidRAW DSP - Perceptual Color, Optical Geometry, and Local Contrast DSP Kernels

use serde::{Deserialize, Serialize};

/// 1. Master Smart-Strength Piecewise Asymmetric Headroom Curve
pub fn apply_super_slider_strength(nominal: f32, min_val: f32, max_ceiling: f32, strength_pct: f32) -> f32 {
    let s = strength_pct.clamp(0.0, 200.0);
    let delta = (s - 100.0) / 100.0; // [-1.0, +1.0]
    if delta <= 0.0 {
        nominal + (nominal - min_val) * delta
    } else {
        nominal + (max_ceiling - nominal) * delta
    }
}

/// 2. 10-Cluster Dermatological Skin Calibration (0x3bf00 in PerfectlyClearPro64.dll)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SkinClusterCalibration {
    pub cluster_id: u8,
    pub exposure_bias_ev: f32,
    pub rgb_locus_delta: [f32; 3], // Applied inside skin mask
    pub deflash_strength: f32,
    pub specular_roll_off_knee: f32,
}

pub fn classify_skin_cluster_oklab(skin_l: f32) -> SkinClusterCalibration {
    // Maps OkLab L of detected face/skin pixels to 10-cluster dermatological calibration table
    if skin_l >= 0.82 {
        // Cluster 1: Alabaster / Pale (pale.cube +0.12 EV bias, warm peach shift)
        SkinClusterCalibration { cluster_id: 1, exposure_bias_ev: 0.12, rgb_locus_delta: [0.018, -0.006, -0.035], deflash_strength: 0.18, specular_roll_off_knee: 0.70 }
    } else if skin_l >= 0.76 {
        // Cluster 2: Fair / Porcelain (+0.10 EV bias)
        SkinClusterCalibration { cluster_id: 2, exposure_bias_ev: 0.10, rgb_locus_delta: [0.015, -0.005, -0.028], deflash_strength: 0.15, specular_roll_off_knee: 0.71 }
    } else if skin_l >= 0.71 {
        // Cluster 3: Light Ivory (+0.14 EV bias)
        SkinClusterCalibration { cluster_id: 3, exposure_bias_ev: 0.14, rgb_locus_delta: [0.010, -0.003, -0.018], deflash_strength: 0.13, specular_roll_off_knee: 0.72 }
    } else if skin_l >= 0.66 {
        // Cluster 4: Warm Peach / Medium Light (+0.12 EV bias)
        SkinClusterCalibration { cluster_id: 4, exposure_bias_ev: 0.12, rgb_locus_delta: [0.005, -0.002, -0.012], deflash_strength: 0.12, specular_roll_off_knee: 0.73 }
    } else if skin_l >= 0.61 {
        // Cluster 5: Golden Olive (+0.08 EV bias, slight green-yellow balance)
        SkinClusterCalibration { cluster_id: 5, exposure_bias_ev: 0.08, rgb_locus_delta: [0.001, -0.001, -0.005], deflash_strength: 0.15, specular_roll_off_knee: 0.74 }
    } else if skin_l >= 0.56 {
        // Cluster 6: Neutral Tan / Honey (+0.04 EV bias, subtle anti-orange cooling)
        SkinClusterCalibration { cluster_id: 6, exposure_bias_ev: 0.04, rgb_locus_delta: [-0.004, 0.001, 0.003], deflash_strength: 0.18, specular_roll_off_knee: 0.75 }
    } else if skin_l >= 0.50 {
        // Cluster 7: Rich Amber / Bronze (0.00 EV bias)
        SkinClusterCalibration { cluster_id: 7, exposure_bias_ev: 0.00, rgb_locus_delta: [-0.008, 0.003, 0.006], deflash_strength: 0.22, specular_roll_off_knee: 0.75 }
    } else if skin_l >= 0.44 {
        // Cluster 8: Medium Brown / Chestnut (-0.08 EV negative protection)
        SkinClusterCalibration { cluster_id: 8, exposure_bias_ev: -0.08, rgb_locus_delta: [-0.012, 0.004, 0.008], deflash_strength: 0.25, specular_roll_off_knee: 0.76 }
    } else if skin_l >= 0.37 {
        // Cluster 9: Deep Espresso (-0.14 EV protection, dark.cube anti-brick-red)
        SkinClusterCalibration { cluster_id: 9, exposure_bias_ev: -0.14, rgb_locus_delta: [-0.018, 0.006, 0.010], deflash_strength: 0.28, specular_roll_off_knee: 0.77 }
    } else {
        // Cluster 10: Deepest Ebony (-0.20 EV negative bias, rich undertone preservation)
        SkinClusterCalibration { cluster_id: 10, exposure_bias_ev: -0.20, rgb_locus_delta: [-0.022, 0.008, 0.012], deflash_strength: 0.32, specular_roll_off_knee: 0.78 }
    }
}

/// Deflash Specular Highlight Roll-off (0x3bf00)
pub fn apply_deflash_specular_rolloff(luma: f32, skin_factor: f32, deflash_strength: f32, knee: f32) -> f32 {
    if skin_factor < 0.05 || deflash_strength <= 0.0 || luma <= knee {
        return luma;
    }
    let excess = luma - knee;
    let max_headroom = (1.2 - knee).max(0.01);
    let norm_excess = excess / max_headroom;
    let roll_off = (norm_excess / (1.0 + deflash_strength * 1.5)).tanh() * max_headroom;
    let target_luma = knee + roll_off;
    luma * (1.0 - skin_factor) + target_luma * skin_factor
}

/// 3. Scene-Governed Auto-Exposure with Deadband & Preprocess EV (pro_and_universal_20211221.pnn)
pub fn compute_governed_exposure(raw_ev_delta: f32, governor_pct: f32, min_strength_pct: f32, preprocess_ev: f32) -> f32 {
    let min_ev = (min_strength_pct / 100.0) * 0.5;
    if raw_ev_delta.abs() < min_ev {
        return preprocess_ev;
    }
    let max_ev = (governor_pct / 100.0) * 1.5; // e.g. governor=5 -> +0.075 EV max for Night; governor=25 -> +0.375 EV
    raw_ev_delta.clamp(-max_ev, max_ev) + preprocess_ev
}

/// 4. Pre-tone automatic noise variance gate evaluation
pub fn evaluate_pre_tone_noise_variance_gate(thumb: &image::RgbImage, iso: Option<u32>) -> (bool, f32, f32) {
    let (w, h) = thumb.dimensions();
    if w < 16 || h < 16 {
        return (false, 0.0, 0.0);
    }

    let mut patch_vars = Vec::with_capacity(64);
    let step_x = ((w - 4) / 8).max(1);
    let step_y = ((h - 4) / 8).max(1);

    for py in (2..h - 2).step_by(step_y as usize) {
        for px in (2..w - 2).step_by(step_x as usize) {
            let mut lumas = [0.0f32; 9];
            let mut idx = 0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let p = thumb.get_pixel((px as i32 + dx) as u32, (py as i32 + dy) as u32);
                    lumas[idx] = 0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32;
                    idx += 1;
                }
            }
            let mean = lumas.iter().sum::<f32>() / 9.0;
            let var = lumas.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / 9.0;
            patch_vars.push(var);
        }
    }

    patch_vars.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p20_idx = (patch_vars.len() as f32 * 0.20) as usize;
    let noise_var = patch_vars.get(p20_idx).copied().unwrap_or(0.0);

    let iso_val = iso.unwrap_or(100);
    let noise_thresh = if iso_val > 800 { 1.5 } else if iso_val > 400 { 3.0 } else { 6.0 };

    if noise_var > noise_thresh || iso_val >= 1250 {
        let rec_denoise = if iso_val >= 3200 || noise_var > 25.0 {
            65.0
        } else if iso_val >= 800 || noise_var > 12.0 {
            50.0
        } else {
            30.0
        };
        (true, noise_var, rec_denoise)
    } else {
        (false, noise_var, 0.0)
    }
}

/// 5. 6th-Order Radial Polynomial Lens Distortion
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Polynomial6th {
    pub k1: f32, // r^2 coefficient
    pub k2: f32, // r^4 coefficient
    pub k3: f32, // r^6 coefficient
}

impl Polynomial6th {
    pub fn new(k1: f32, k2: f32, k3: f32) -> Self {
        Self { k1, k2, k3 }
    }

    pub fn undistort_radius(&self, r_distorted: f32) -> f32 {
        let r2 = r_distorted * r_distorted;
        let r4 = r2 * r2;
        let r6 = r4 * r2;
        r_distorted * (1.0 + self.k1 * r2 + self.k2 * r4 + self.k3 * r6)
    }

    /// Inverse Newton-Raphson solver to map undistorted back to distorted coordinates
    pub fn distort_radius(&self, r_undistorted: f32) -> f32 {
        let mut r = r_undistorted;
        for _ in 0..6 {
            let r2 = r * r;
            let r4 = r2 * r2;
            let r6 = r4 * r2;
            let f = r * (1.0 + self.k1 * r2 + self.k2 * r4 + self.k3 * r6) - r_undistorted;
            let f_prime = 1.0 + 3.0 * self.k1 * r2 + 5.0 * self.k2 * r4 + 7.0 * self.k3 * r6;
            if f_prime.abs() < 1e-7 {
                break;
            }
            let delta = f / f_prime;
            r -= delta;
            if delta.abs() < 1e-6 {
                break;
            }
        }
        r
    }
}

/// 6. Q15 Fixed-Point Bi-Directional IIR Local Contrast & Dispersion Normalizer
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Q15IirConfig {
    pub alpha_q15: i32,        // Filter coefficient in Q15 (e.g. 0.85 * 32768 = 27853)
    pub contrast_boost: f32,    // Detail amplification gain (0.0 .. 2.0)
    pub dispersion_floor: f32, // Minimum dispersion threshold to avoid noise amplification
}

impl Default for Q15IirConfig {
    fn default() -> Self {
        Self {
            alpha_q15: 27853, // ~0.85 in Q15
            contrast_boost: 0.40,
            dispersion_floor: 0.015,
        }
    }
}

/// 1D Bi-directional IIR filter in-place on a row/column in Q15 fixed-point using caller-provided scratch.
pub fn filter_iir_bidirectional_q15_into(
    signal: &[f32],
    alpha_q15: i32,
    forward_scratch: &mut Vec<i32>,
    out: &mut [f32],
) {
    let n = signal.len();
    if n == 0 {
        return;
    }
    let one_minus_alpha = 32768 - alpha_q15;
    forward_scratch.resize(n, 0);
    let mut state = (signal[0] * 32768.0).round() as i32;
    for i in 0..n {
        let x = (signal[i] * 32768.0).round() as i32;
        state = ((one_minus_alpha as i64 * x as i64 + alpha_q15 as i64 * state as i64) >> 15) as i32;
        forward_scratch[i] = state;
    }

    state = forward_scratch[n - 1];
    for i in (0..n).rev() {
        let x = forward_scratch[i];
        state = ((one_minus_alpha as i64 * x as i64 + alpha_q15 as i64 * state as i64) >> 15) as i32;
        out[i] = state as f32 / 32768.0;
    }
}

/// 1D Bi-directional IIR filter on a row/column in Q15 fixed-point
pub fn filter_iir_bidirectional_q15(signal: &[f32], alpha_q15: i32) -> Vec<f32> {
    let mut forward = Vec::with_capacity(signal.len());
    let mut backward = vec![0f32; signal.len()];
    filter_iir_bidirectional_q15_into(signal, alpha_q15, &mut forward, &mut backward);
    backward
}

/// Normalized Dispersion Local Contrast Enhancement (parallelized with per-worker scratch buffers)
pub fn enhance_local_contrast_q15(luma: &[f32], width: usize, height: usize, config: &Q15IirConfig) -> Vec<f32> {
    use rayon::prelude::*;

    if width == 0 || height == 0 || luma.len() != width * height {
        return luma.to_vec();
    }

    // Horizontal IIR pass (parallel by row, zero per-row heap allocations)
    let mut temp = vec![0.0f32; width * height];
    temp.par_chunks_mut(width)
        .zip(luma.par_chunks(width))
        .for_each_init(
            || Vec::<i32>::with_capacity(width),
            |scratch, (out_row, in_row)| {
                filter_iir_bidirectional_q15_into(in_row, config.alpha_q15, scratch, out_row);
            },
        );

    // Vertical IIR pass (parallel by column, reusing per-thread scratch)
    let mut base_illumination = vec![0.0f32; width * height];
    let one_minus_alpha = 32768 - config.alpha_q15;
    let alpha_q15 = config.alpha_q15;

    // Process in cache-friendly vertical strips
    let mut col_results: Vec<Vec<f32>> = (0..width)
        .into_par_iter()
        .map_init(
            || (Vec::<i32>::with_capacity(height), vec![0.0f32; height]),
            |(forward, col_out), x| {
                forward.resize(height, 0);
                let mut state = (temp[x] * 32768.0).round() as i32;
                for y in 0..height {
                    let v = (temp[y * width + x] * 32768.0).round() as i32;
                    state = ((one_minus_alpha as i64 * v as i64 + alpha_q15 as i64 * state as i64) >> 15) as i32;
                    forward[y] = state;
                }
                state = forward[height - 1];
                for y in (0..height).rev() {
                    let v = forward[y];
                    state = ((one_minus_alpha as i64 * v as i64 + alpha_q15 as i64 * state as i64) >> 15) as i32;
                    col_out[y] = state as f32 / 32768.0;
                }
                col_out.clone()
            },
        )
        .collect();

    for (x, col) in col_results.drain(..).enumerate() {
        for y in 0..height {
            base_illumination[y * width + x] = col[y];
        }
    }

    // Dispersion normalization and local contrast reconstruction (parallel)
    let mut output = vec![0.0f32; width * height];
    output
        .par_iter_mut()
        .enumerate()
        .for_each(|(i, out)| {
            let x = luma[i];
            let base = base_illumination[i];
            let detail = x - base;
            let dispersion = detail.abs().max(config.dispersion_floor);
            let normalized_detail = detail / dispersion;
            let enhanced_detail = normalized_detail.tanh() * dispersion * (1.0 + config.contrast_boost);
            *out = (base + enhanced_detail).clamp(0.0, 1.5);
        });
    output
}

/// 7. Color-Constancy Illuminant & Auto White Balance Engine
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Fc4IlluminantResult {
    pub gain_r: f32,
    pub gain_g: f32,
    pub gain_b: f32,
    pub correlated_cct_kelvin: f32,
}

pub fn estimate_fc4_illuminant(rgb_pixels: &[[f32; 3]]) -> Fc4IlluminantResult {
    if rgb_pixels.is_empty() {
        return Fc4IlluminantResult { gain_r: 1.0, gain_g: 1.0, gain_b: 1.0, correlated_cct_kelvin: 5500.0 };
    }

    let mut r_acc = 0.0f32;
    let mut g_acc = 0.0f32;
    let mut b_acc = 0.0f32;
    let mut valid_count = 0usize;

    for &[r, g, b] in rgb_pixels {
        let max_val = r.max(g).max(b);
        let min_val = r.min(g).min(b);
        if max_val > 0.03 && max_val < 0.95 && (max_val - min_val) < 0.65 {
            r_acc += r;
            g_acc += g;
            b_acc += b;
            valid_count += 1;
        }
    }

    if valid_count == 0 {
        return Fc4IlluminantResult { gain_r: 1.0, gain_g: 1.0, gain_b: 1.0, correlated_cct_kelvin: 5500.0 };
    }

    let mean_r = r_acc / valid_count as f32;
    let mean_g = g_acc / valid_count as f32;
    let mean_b = b_acc / valid_count as f32;

    let g_ref = mean_g.max(1e-4);
    let mut gain_r = g_ref / mean_r.max(1e-4);
    let mut gain_b = g_ref / mean_b.max(1e-4);

    let sum = mean_r + mean_g + mean_b;
    let x = mean_r / sum.max(1e-5);
    let y = mean_g / sum.max(1e-5);
    let n = (x - 0.3320) / (0.1858 - y);
    let cct = 449.0 * n.powi(3) + 3525.0 * n.powi(2) + 6823.3 * n + 5520.33;
    let clamped_cct = cct.clamp(2500.0, 10000.0);

    gain_r = gain_r.clamp(0.45, 2.5);
    gain_b = gain_b.clamp(0.45, 2.8);

    Fc4IlluminantResult {
        gain_r,
        gain_g: 1.0,
        gain_b,
        correlated_cct_kelvin: clamped_cct,
    }
}

/// 8. Perceptual 360° Circular 4-Angle Trapezoidal HSL Selective Color Engine
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrapezoidalHslRange {
    pub center_hue: f32,
    pub width: f32,
    pub core_width_ratio: f32,
}

impl TrapezoidalHslRange {
    pub fn new(center_hue: f32, width: f32) -> Self {
        Self {
            center_hue,
            width,
            core_width_ratio: 0.35,
        }
    }

    pub fn evaluate_weight(&self, hue: f32) -> f32 {
        let diff = (hue - self.center_hue).abs();
        let d = diff.rem_euclid(360.0);
        let dist = d.min(360.0 - d);
        let half_w = self.width * 0.5;
        let half_core = half_w * self.core_width_ratio;
        if dist <= half_core {
            1.0
        } else if dist >= half_w {
            0.0
        } else {
            1.0 - (dist - half_core) / (half_w - half_core)
        }
    }
}

/// 9. Opponent Luma-Chroma Transform (RGB2IQbQr & IQbQr2RGB)
pub fn rgb_to_iqbqr(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let qr = (r - g) * 0.5;
    let qb = (b - g) * 0.5;
    let i = g + (qr + qb) * 0.5;
    (i, qb, qr)
}

pub fn iqbqr_to_rgb(i: f32, qb: f32, qr: f32) -> (f32, f32, f32) {
    let g = i - (qr + qb) * 0.5;
    let r = g + 2.0 * qr;
    let b = g + 2.0 * qb;
    (r, g, b)
}

/// 10. Max-Med-Min Fractional Hue-Preserving Tone Curve
pub fn apply_tone_curve_on_max_med_min<F>(rgb: [f32; 3], eval_curve: F) -> [f32; 3]
where
    F: Fn(f32) -> f32,
{
    let [r, g, b] = rgb;
    let (in_max, max_chan, in_med, med_chan, in_min, min_chan) = if r >= g && r >= b {
        if g >= b { (r, 0, g, 1, b, 2) } else { (r, 0, b, 2, g, 1) }
    } else if g >= r && g >= b {
        if r >= b { (g, 1, r, 0, b, 2) } else { (g, 1, b, 2, r, 0) }
    } else {
        if r >= g { (b, 2, r, 0, g, 1) } else { (b, 2, g, 1, r, 0) }
    };

    let delta_in = in_max - in_min;
    let out_max = eval_curve(in_max);
    let out_min = eval_curve(in_min);
    let out_med = if delta_in > 1e-5 {
        out_min + (out_max - out_min) * ((in_med - in_min) / delta_in)
    } else {
        out_min
    };

    let mut out = [0.0f32; 3];
    out[max_chan] = out_max;
    out[med_chan] = out_med;
    out[min_chan] = out_min;
    out
}

/// 11. Wide-Angle Volume-Preserving Anamorphosis Projection
pub fn compute_anamorphosis(norm_x: f32, norm_y: f32, k1: f32) -> (f32, f32) {
    if k1 >= -0.015 {
        return (norm_x, norm_y);
    }
    let a = (-k1 * 0.35).clamp(0.0, 0.08);
    let x_comp = norm_x * (1.0 - a * norm_x * norm_x);
    let y_comp = norm_y * (1.0 - a * 0.5 * norm_y * norm_y);
    (x_comp, y_comp)
}

/// Canonical sRGB -> linear transfer function (IEC 61966-2-1).
#[inline]
pub fn srgb_to_linear_f32(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Canonical linear -> sRGB transfer function (IEC 61966-2-1).
#[inline]
pub fn linear_to_srgb_f32(c: f32) -> f32 {
    let c = c.max(0.0);
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Canonical Linear sRGB -> Oklab (Ottosson) transform.
#[inline]
pub fn linear_srgb_to_oklab(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).max(0.0).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).max(0.0).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).max(0.0).cbrt();

    let oklab_l = 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s;
    let oklab_a = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s;
    let oklab_b = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s;
    (oklab_l, oklab_a, oklab_b)
}

/// Canonical Oklab -> Linear sRGB (Ottosson) transform.
#[inline]
pub fn oklab_to_linear_srgb(oklab_l: f32, oklab_a: f32, oklab_b: f32) -> (f32, f32, f32) {
    let l_ = oklab_l + 0.3963377774 * oklab_a + 0.2158037573 * oklab_b;
    let m_ = oklab_l - 0.1055613458 * oklab_a - 0.0638541728 * oklab_b;
    let s_ = oklab_l - 0.0894841775 * oklab_a - 1.2914855480 * oklab_b;

    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    let r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
    let g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
    let b = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;
    (r, g, b)
}

/// Shared Laplacian variance sharpness metric for RGB8 images.
pub fn calculate_laplacian_variance(img: &image::RgbImage) -> f32 {
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 {
        return 0.0;
    }
    let raw = img.as_raw();
    let stride = (w * 3) as usize;
    let luma = |x: u32, y: u32| -> f64 {
        let idx = (y as usize) * stride + (x as usize) * 3;
        0.2126 * raw[idx] as f64 + 0.7152 * raw[idx + 1] as f64 + 0.0722 * raw[idx + 2] as f64
    };

    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let mut count = 0u64;
    for y in 1..(h - 1) {
        for x in 1..(w - 1) {
            let lap = 4.0 * luma(x, y)
                - luma(x - 1, y)
                - luma(x + 1, y)
                - luma(x, y - 1)
                - luma(x, y + 1);
            sum += lap;
            sum_sq += lap * lap;
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    let mean = sum / count as f64;
    ((sum_sq / count as f64) - mean * mean).max(0.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skin_clusters_10_and_deflash() {
        let alabaster = classify_skin_cluster_oklab(0.85);
        assert_eq!(alabaster.cluster_id, 1);
        assert!(alabaster.exposure_bias_ev > 0.0);

        let ebony = classify_skin_cluster_oklab(0.30);
        assert_eq!(ebony.cluster_id, 10);
        assert!(ebony.exposure_bias_ev < 0.0);

        // Test deflash specular roll-off
        let flat_luma = apply_deflash_specular_rolloff(0.95, 0.8, 0.5, 0.72);
        assert!(flat_luma < 0.95, "Specular shine should be compressed!");
        assert!(flat_luma > 0.72, "Should remain above knee!");
    }

    #[test]
    fn test_q15_iir_and_dispersion() {
        let signal = vec![0.1f32, 0.2, 0.3, 0.8, 0.85, 0.9, 0.4, 0.2, 0.1];
        let filtered = filter_iir_bidirectional_q15(&signal, 27853);
        assert_eq!(filtered.len(), signal.len());
        assert!(filtered[4] < 0.9);

        let config = Q15IirConfig::default();
        let enhanced = enhance_local_contrast_q15(&signal, 3, 3, &config);
        assert_eq!(enhanced.len(), 9);
    }

    #[test]
    fn test_fc4_illuminant() {
        let patches = vec![[0.4f32, 0.4, 0.4], [0.5, 0.5, 0.48], [0.3, 0.32, 0.31]];
        let ill = estimate_fc4_illuminant(&patches);
        assert!(ill.gain_r > 0.5 && ill.gain_r < 2.0);
        assert!(ill.correlated_cct_kelvin >= 2500.0 && ill.correlated_cct_kelvin <= 10000.0);
    }

    #[test]
    fn test_trapezoidal_hsl() {
        let red_band = TrapezoidalHslRange::new(0.0, 40.0);
        assert_eq!(red_band.evaluate_weight(0.0), 1.0);
        assert_eq!(red_band.evaluate_weight(5.0), 1.0); // Inside core plateau
        assert_eq!(red_band.evaluate_weight(30.0), 0.0); // Outside band
        assert!(red_band.evaluate_weight(12.0) > 0.0 && red_band.evaluate_weight(12.0) < 1.0); // Slope

        // Edge case: wrapped and negative angles
        assert_eq!(red_band.evaluate_weight(360.0), 1.0);
        assert_eq!(red_band.evaluate_weight(720.0), 1.0);
        assert_eq!(red_band.evaluate_weight(-360.0), 1.0);
        assert_eq!(red_band.evaluate_weight(365.0), 1.0); // Equivalent to +5.0
        assert_eq!(red_band.evaluate_weight(390.0), 0.0); // Equivalent to +30.0
        assert_eq!(red_band.evaluate_weight(732.0), red_band.evaluate_weight(12.0));
    }

    #[test]
    fn test_iqbqr_exact_reversibility() {
        let (r, g, b) = (0.75f32, 0.45f32, 0.20f32);
        let (i, qb, qr) = rgb_to_iqbqr(r, g, b);
        let (r_out, g_out, b_out) = iqbqr_to_rgb(i, qb, qr);
        assert!((r - r_out).abs() < 1e-6);
        assert!((g - g_out).abs() < 1e-6);
        assert!((b - b_out).abs() < 1e-6);
    }

    #[test]
    fn test_tone_curve_on_max_med_min() {
        let rgb = [0.8f32, 0.5f32, 0.2f32];
        let curved = apply_tone_curve_on_max_med_min(rgb, |x| x * x * (3.0 - 2.0 * x));
        let orig_ratio = (rgb[1] - rgb[2]) / (rgb[0] - rgb[2]);
        let curved_ratio = (curved[1] - curved[2]) / (curved[0] - curved[2]);
        assert!((orig_ratio - curved_ratio).abs() < 1e-5);
    }

    #[test]
    fn test_compute_anamorphosis() {
        let (x, _y) = compute_anamorphosis(0.8, 0.6, -0.05);
        assert!(x < 0.8, "Anamorphosis should compress wide-angle horizontal edge stretch");
    }
}

