//! HDR Color Science primitives for PQ/HLG and BT.2020 wide-gamut pipelines.
//!
//! Pinned constants and canonical transfer functions adhering to:
//! - SMPTE ST 2084 (Perceptual Quantizer / PQ)
//! - ITU-R BT.2100 (Hybrid Log-Gamma / HLG)
//! - ISO 22028-5 & ITU-R Report BT.2408 (Reference diffuse white anchored at 203 cd/m²)

#![allow(clippy::excessive_precision)]

/// Linear `1.0` (diffuse white) maps to this absolute luminance per ISO 22028-5.
pub const REFERENCE_WHITE_NITS: f32 = 203.0;

/// PQ code `1.0` represents 10,000 cd/m² peak luminance.
pub const PQ_MAX_NITS: f32 = 10000.0;

/// HLG nominal peak ratio is 12x diffuse white.
pub const HLG_PEAK_RATIO: f32 = 12.0;

// --- SMPTE ST 2084 (PQ) Canonical Constants ---
const PQ_M1: f32 = 0.1593017578125; // 2610 / 16384
const PQ_M2: f32 = 78.84375;        // (2523 / 4096) * 128
const PQ_C1: f32 = 0.8359375;       // 3424 / 4096
const PQ_C2: f32 = 18.8515625;      // (2413 / 4096) * 32
const PQ_C3: f32 = 18.6875;         // (2392 / 4096) * 32

// --- ITU-R BT.2100 HLG Canonical Constants ---
const HLG_A: f32 = 0.17883277;
const HLG_B: f32 = 0.28466892;
const HLG_C: f32 = 0.55991073;

/// PQ inverse-EOTF (OETF): normalized linear luminance `L` (1.0 == 10,000 cd/m²) -> non-linear PQ code in [0, 1].
#[inline]
pub fn pq_inverse_eotf(l_norm: f32) -> f32 {
    let lp = l_norm.max(0.0).powf(PQ_M1);
    ((PQ_C1 + PQ_C2 * lp) / (1.0 + PQ_C3 * lp)).powf(PQ_M2)
}

/// PQ EOTF (decode): PQ code in [0, 1] -> normalized linear luminance (1.0 == 10,000 cd/m²).
/// Exact analytical inverse of [`pq_inverse_eotf`].
#[inline]
pub fn pq_eotf(e: f32) -> f32 {
    let ep = e.clamp(0.0, 1.0).powf(1.0 / PQ_M2);
    let num = (ep - PQ_C1).max(0.0);
    let den = PQ_C2 - PQ_C3 * ep;
    if den <= 0.0 {
        return 1.0;
    }
    (num / den).powf(1.0 / PQ_M1)
}

/// Maps linear scene-referred value (diffuse white = 1.0) -> PQ code using ISO 22028-5 203-nit anchor.
#[inline]
pub fn linear_scene_to_pq(linear: f32) -> f32 {
    pq_inverse_eotf(linear * (REFERENCE_WHITE_NITS / PQ_MAX_NITS))
}

/// Decodes PQ code back to linear scene-referred value (diffuse white = 1.0).
#[inline]
pub fn pq_to_linear_scene(pq: f32) -> f32 {
    pq_eotf(pq) * (PQ_MAX_NITS / REFERENCE_WHITE_NITS)
}

/// Converts normalized PQ float in [0.0, 1.0] to a 16-bit integer for container storage.
#[inline]
pub fn pq_normalized_to_u16(val: f32) -> u16 {
    (val.clamp(0.0, 1.0) * 65535.0).round() as u16
}

/// HLG OETF: scene-linear `E` in [0, 1] -> HLG non-linear signal in [0, 1].
#[inline]
pub fn hlg_oetf(e: f32) -> f32 {
    let e = e.max(0.0);
    if e <= 1.0 / 12.0 {
        (3.0 * e).sqrt()
    } else {
        HLG_A * (12.0 * e - HLG_B).ln() + HLG_C
    }
}

/// HLG inverse-OETF: HLG signal in [0, 1] -> scene-linear `E`.
#[inline]
pub fn hlg_inverse_oetf(signal: f32) -> f32 {
    let s = signal.clamp(0.0, 1.0);
    if s <= 0.5 {
        (s * s) / 3.0
    } else {
        (((s - HLG_C) / HLG_A).exp() + HLG_B) / 12.0
    }
}

/// Maps linear scene-referred value (diffuse white = 1.0) -> HLG signal normalized by peak headroom (12.0x).
#[inline]
pub fn linear_scene_to_hlg(linear: f32) -> f32 {
    hlg_oetf(linear / HLG_PEAK_RATIO)
}

/// Decodes HLG signal back to linear scene-referred value.
#[inline]
pub fn hlg_to_linear_scene(signal: f32) -> f32 {
    hlg_inverse_oetf(signal) * HLG_PEAK_RATIO
}

/// Transforms linear sRGB / Rec.709 (D65) coordinates to linear BT.2020.
/// Matrix derived from ITU-R BT.2087 canonical specification.
#[inline]
pub fn srgb_to_bt2020(rgb: [f32; 3]) -> [f32; 3] {
    [
        0.6274040 * rgb[0] + 0.3292820 * rgb[1] + 0.0433136 * rgb[2],
        0.0690970 * rgb[0] + 0.9195400 * rgb[1] + 0.0113612 * rgb[2],
        0.0163916 * rgb[0] + 0.0880132 * rgb[1] + 0.8955950 * rgb[2],
    ]
}

/// Transforms linear BT.2020 coordinates to linear sRGB / Rec.709 (D65).
#[inline]
pub fn bt2020_to_srgb(rgb: [f32; 3]) -> [f32; 3] {
    [
        1.6604910 * rgb[0] - 0.5876411 * rgb[1] - 0.0728499 * rgb[2],
        -0.1245505 * rgb[0] + 1.1328999 * rgb[1] - 0.0083494 * rgb[2],
        -0.0181508 * rgb[0] - 0.1005789 * rgb[1] + 1.1187297 * rgb[2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pq_round_trip_exactness() {
        let test_values = [
            0.00001f32, 0.0001, 0.001, 0.01, 0.05, 0.18, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 49.25,
        ];

        for &val in &test_values {
            let pq = linear_scene_to_pq(val);
            assert!(
                (0.0..=1.0).contains(&pq),
                "PQ value out of bounds: {} for input {}",
                pq,
                val
            );
            let round_trip = pq_to_linear_scene(pq);
            let rel_err = (round_trip - val).abs() / val;
            assert!(
                rel_err < 1e-4,
                "PQ round-trip failed for val={}: got {}, rel_err={}",
                val,
                round_trip,
                rel_err
            );
        }
    }

    #[test]
    fn test_hlg_round_trip_exactness() {
        let test_values = [
            0.0001f32, 0.001, 0.01, 0.1, 0.18, 0.5, 1.0, 2.0, 4.0, 8.0, 11.9,
        ];

        for &val in &test_values {
            let hlg = linear_scene_to_hlg(val);
            assert!(
                (0.0..=1.0).contains(&hlg),
                "HLG value out of bounds: {} for input {}",
                hlg,
                val
            );
            let round_trip = hlg_to_linear_scene(hlg);
            let rel_err = (round_trip - val).abs() / val;
            assert!(
                rel_err < 1e-4,
                "HLG round-trip failed for val={}: got {}, rel_err={}",
                val,
                round_trip,
                rel_err
            );
        }
    }

    #[test]
    fn test_hlg_transition_point_continuity() {
        let trans = 1.0 / 12.0;
        let eps = 1e-6;
        let left = hlg_oetf(trans - eps);
        let right = hlg_oetf(trans + eps);
        assert!(
            (left - right).abs() < 1e-4,
            "HLG discontinuity detected at transition point: left={}, right={}",
            left,
            right
        );
    }

    #[test]
    fn test_bt2020_srgb_matrix_invertibility() {
        let test_colors = [
            [1.0f32, 1.0, 1.0],
            [0.5, 0.2, 0.8],
            [0.18, 0.18, 0.18],
            [0.9, 0.05, 0.02],
        ];

        for &c in &test_colors {
            let bt2020 = srgb_to_bt2020(c);
            let reconstructed = bt2020_to_srgb(bt2020);
            for ch in 0..3 {
                let diff = (reconstructed[ch] - c[ch]).abs();
                assert!(
                    diff < 1e-4,
                    "Matrix round-trip failed on channel {}: expected {}, got {}",
                    ch,
                    c[ch],
                    reconstructed[ch]
                );
            }
        }
    }
}
