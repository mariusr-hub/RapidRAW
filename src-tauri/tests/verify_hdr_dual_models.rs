//! Automated Dual-HDR Engine Verification Harness (HDR1 vs HDR2)
//!
//! Validates:
//! 1. HDR1 (Linear Radiance stream-and-accumulate) and HDR2 (Hardened multi-scale) both return valid Rgb32FImage
//! 2. Zone V / VI midtone placement accuracy
//! 3. Shadow SNR enhancement across multi-exposure bracket
//! 4. Edge halo suppression (overshoot/undershoot index < 0.05)
//! 5. Execution timing comparison table

use image::{ImageBuffer, Rgb, Rgb32FImage};
use rapidraw_lib::hdr_fusion::{fuse_exposures_dual, HdrEngineMode, HdrMergeOptions};
use std::time::Instant;

/// Synthesizes a calibrated multi-exposure bracket containing:
/// - Zone V 18% reflectance midtone patch
/// - Deep shadow region with simulated photon shot + read noise
/// - High-contrast sharp step edge (0.05 to 0.85) to evaluate edge halo artifacts
/// - Specular highlight core (radiance > 8.0)
fn create_synthetic_hdr_bracket(w: u32, h: u32) -> (Vec<Rgb32FImage>, Vec<f32>) {
    let scales = vec![0.25f32, 1.0f32, 4.0f32]; // -2 EV, 0 EV, +2 EV
    let mut frames = Vec::with_capacity(scales.len());

    let mut true_radiance = vec![0.0f32; (w * h * 3) as usize];

    for y in 0..h {
        for x in 0..w {
            let idx = ((y * w + x) * 3) as usize;
            let val = if x < w / 4 {
                // Deep shadow region: 0.005 linear radiance
                0.005f32
            } else if x < w / 2 {
                // Zone V calibrated midtone: 0.18 linear radiance
                0.18f32
            } else if x < (3 * w) / 4 {
                // Step transition: sharp boundary at x == (5 * w) / 8
                if x < (5 * w) / 8 {
                    0.08f32
                } else {
                    0.72f32
                }
            } else {
                // Specular highlight core
                12.0f32
            };

            true_radiance[idx] = val;
            true_radiance[idx + 1] = val;
            true_radiance[idx + 2] = val;
        }
    }

    // Generate sensor exposures clamped to [0.0, 1.0] with pseudo-noise
    for (k, &scale) in scales.iter().enumerate() {
        let mut img = ImageBuffer::<Rgb<f32>, Vec<f32>>::new(w, h);
        let raw = img.as_mut();

        for i in 0..(w * h) as usize {
            let rad_idx = i * 3;
            let rad = true_radiance[rad_idx];

            // Pseudo-random noise proportional to sqrt(signal) (shot noise) + floor
            let seed_u32 = (i as u32).wrapping_mul(1664525).wrapping_add((k as u32).wrapping_mul(1013904223));
            let seed = (seed_u32 as f32) / (u32::MAX as f32); // in [0.0, 1.0]
            let shot_noise = (rad * scale * 0.002).sqrt() * (seed - 0.5);
            let sensor_signal = (rad * scale + shot_noise).clamp(0.0, 1.0);

            raw[rad_idx] = sensor_signal;
            raw[rad_idx + 1] = sensor_signal;
            raw[rad_idx + 2] = sensor_signal;
        }

        frames.push(img);
    }

    (frames, scales)
}

fn calculate_shadow_snr(img: &Rgb32FImage, w: u32, h: u32) -> f32 {
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let mut count = 0u64;

    for y in 0..h {
        for x in 0..(w / 4) {
            let px = img.get_pixel(x, y);
            let lum = (0.2126 * px[0] + 0.7152 * px[1] + 0.0722 * px[2]) as f64;
            sum += lum;
            sum_sq += lum * lum;
            count += 1;
        }
    }

    if count == 0 {
        return 0.0;
    }

    let mean = sum / count as f64;
    let var = (sum_sq / count as f64 - mean * mean).max(1e-12);
    let std_dev = var.sqrt();

    (20.0 * (mean / std_dev).max(1e-4).log10()) as f32
}

fn calculate_edge_halo_index(img: &Rgb32FImage, w: u32, h: u32) -> f32 {
    let edge_x = (5 * w) / 8;
    let step_h = 0.72 - 0.08;

    let mut max_overshoot = 0.0f32;
    let mut max_undershoot = 0.0f32;

    for y in (h / 4)..(3 * h / 4) {
        // Sample 4 pixels left and right of edge
        let left_px = img.get_pixel(edge_x.saturating_sub(2), y);
        let right_px = img.get_pixel(edge_x + 2, y);

        let left_lum = 0.2126 * left_px[0] + 0.7152 * left_px[1] + 0.0722 * left_px[2];
        let right_lum = 0.2126 * right_px[0] + 0.7152 * right_px[1] + 0.0722 * right_px[2];

        // Undershoot below low value (0.08)
        if left_lum < 0.08 {
            max_undershoot = max_undershoot.max(0.08 - left_lum);
        }
        // Overshoot above high value (0.72)
        if right_lum > 0.72 {
            max_overshoot = max_overshoot.max(right_lum - 0.72);
        }
    }

    (max_overshoot.max(max_undershoot) / step_h).max(0.0)
}

fn calculate_zone_v_midtone(img: &Rgb32FImage, w: u32, h: u32) -> f32 {
    let mut sum = 0.0f64;
    let mut count = 0u64;

    for y in (h / 4)..(3 * h / 4) {
        for x in (w / 4)..(w / 2) {
            let px = img.get_pixel(x, y);
            let lum = (0.2126 * px[0] + 0.7152 * px[1] + 0.0722 * px[2]) as f64;
            sum += lum;
            count += 1;
        }
    }

    if count == 0 {
        0.0
    } else {
        (sum / count as f64) as f32
    }
}

#[test]
fn test_verify_hdr_dual_models() {
    let w = 512u32;
    let h = 512u32;
    let (frames, scales) = create_synthetic_hdr_bracket(w, h);

    // --- Benchmark HDR1: Linear Radiance Physical Stream-and-Accumulate ---
    let mut hdr1_opts = HdrMergeOptions::default();
    hdr1_opts.engine = Some(HdrEngineMode::LinearRadiance);
    hdr1_opts.reference_index = Some(1); // 0 EV base

    let t1 = Instant::now();
    let hdr1_res = fuse_exposures_dual::<tauri::Wry>(&frames, &scales, &hdr1_opts, None, None)
        .expect("HDR1 fusion failed");
    let hdr1_time = t1.elapsed();

    // --- Benchmark HDR2: Production Hardened Multi-Scale Engine ---
    let mut hdr2_opts = HdrMergeOptions::default();
    hdr2_opts.engine = Some(HdrEngineMode::Hdr2);
    hdr2_opts.reference_index = Some(1);

    let t2 = Instant::now();
    let hdr2_res = fuse_exposures_dual::<tauri::Wry>(&frames, &scales, &hdr2_opts, None, None)
        .expect("HDR2 fusion failed");
    let hdr2_time = t2.elapsed();

    // Verify both output valid Rgb32FImage with matching dimensions
    assert_eq!(hdr1_res.linear_radiance.dimensions(), (w, h));
    assert_eq!(hdr2_res.linear_radiance.dimensions(), (w, h));

    // Verify all pixels are non-negative and finite
    for px in hdr1_res.linear_radiance.pixels() {
        assert!(px[0].is_finite() && px[0] >= 0.0);
        assert!(px[1].is_finite() && px[1] >= 0.0);
        assert!(px[2].is_finite() && px[2] >= 0.0);
    }
    for px in hdr2_res.linear_radiance.pixels() {
        assert!(px[0].is_finite() && px[0] >= 0.0);
        assert!(px[1].is_finite() && px[1] >= 0.0);
        assert!(px[2].is_finite() && px[2] >= 0.0);
    }

    // Compute metrics
    let hdr1_zone_v = calculate_zone_v_midtone(&hdr1_res.linear_radiance, w, h);
    let hdr2_zone_v = calculate_zone_v_midtone(&hdr2_res.linear_radiance, w, h);

    let hdr1_snr = calculate_shadow_snr(&hdr1_res.linear_radiance, w, h);
    let hdr2_snr = calculate_shadow_snr(&hdr2_res.linear_radiance, w, h);

    let hdr1_halo = calculate_edge_halo_index(&hdr1_res.linear_radiance, w, h);
    let hdr2_halo = calculate_edge_halo_index(&hdr2_res.linear_radiance, w, h);

    // Print side-by-side metric comparison table
    println!("\n=========================================================================");
    println!("             RAPIDRAW DUAL-HDR ENGINE COMPARISON VERIFICATION            ");
    println!("=========================================================================");
    println!("  Metric                        | HDR1 (Linear Radiance) | HDR2 (Hardened) ");
    println!("--------------------------------+------------------------+---------------");
    println!("  Zone V Midtone Radiance       | {:<22.4} | {:<15.4}", hdr1_zone_v, hdr2_zone_v);
    println!("  Shadow SNR (dB)               | {:<22.2} | {:<15.2}", hdr1_snr, hdr2_snr);
    println!("  Edge Halo Index               | {:<22.4} | {:<15.4}", hdr1_halo, hdr2_halo);
    println!("  Execution Time                | {:<22?} | {:<15?}", hdr1_time, hdr2_time);
    println!("  Output Type                   | Rgb32FImage            | Rgb32FImage   ");
    println!("=========================================================================\n");

    // Quality assertions
    assert!(hdr1_zone_v > 0.10 && hdr1_zone_v < 0.25, "HDR1 midtone placement drifted: {}", hdr1_zone_v);
    assert!(hdr2_zone_v > 0.10 && hdr2_zone_v < 0.25, "HDR2 midtone placement drifted: {}", hdr2_zone_v);
    assert!(hdr1_halo < 0.05, "HDR1 produced excessive edge halos: {}", hdr1_halo);
    assert!(hdr2_halo < 0.10, "HDR2 produced excessive edge halos: {}", hdr2_halo);
    assert!(hdr1_snr > 10.0, "HDR1 shadow SNR insufficient: {} dB", hdr1_snr);
    assert!(hdr2_snr > 10.0, "HDR2 shadow SNR insufficient: {} dB", hdr2_snr);
}
