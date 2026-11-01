//! Adversarial empirical challenge and stress harness for Milestone 1 (R1 & R3).
//! Validates TIFF tone-mapping parity, extreme dynamic range gradation, floating point edge cases,
//! and progress regex matching.

use image::{Rgb, Rgb32FImage};
use rapidraw_lib::export_processing::save_tiff_compressed;
use std::fs::File;

#[test]
fn test_adversarial_tiff_tone_mapping_parity_and_extreme_ranges() {
    let width = 64u32;
    let height = 64u32;
    let mut test_img = Rgb32FImage::new(width, height);

    // Populate test image with key test zones:
    // Row 0-7:   0.000 (Pure Black)
    // Row 8-15:  0.001 (Deep Shadow, Zone I)
    // Row 16-23: 0.180 (Linear 18% Gray - Zone V linear)
    // Row 24-31: 0.500 (Tone-mapped sRGB midtone - Zone V perceptual display)
    // Row 32-39: 1.000 (Peak Highlight, Zone X)
    // Row 40-47: 1.500 (Super-white / Over-range float)
    // Row 48-55: 100.0 (Extreme over-range float)
    // Row 56-63: -0.50 (Negative out-of-gamut float)
    for y in 0..height {
        let val = match y {
            0..=7 => 0.0f32,
            8..=15 => 0.001f32,
            16..=23 => 0.18f32,
            24..=31 => 0.50f32,
            32..=39 => 1.00f32,
            40..=47 => 1.50f32,
            48..=55 => 100.0f32,
            _ => -0.50f32,
        };
        for x in 0..width {
            test_img.put_pixel(x, y, Rgb([val, val, val]));
        }
    }

    let temp_dir = std::env::temp_dir();
    let tiff_path = temp_dir.join("adversarial_test_m1_ranges.tiff");

    // 1. Must save successfully without panic or error
    let save_res = save_tiff_compressed(&tiff_path, &test_img);
    assert!(save_res.is_ok(), "save_tiff_compressed failed: {:?}", save_res);

    // 2. Decode back via standard TIFF decoder
    let file = File::open(&tiff_path).expect("Failed to open saved TIFF");
    let mut decoder = tiff::decoder::Decoder::new(file).expect("Failed to create TIFF decoder");

    let (dec_w, dec_h) = decoder.dimensions().expect("Failed to read dimensions");
    assert_eq!(dec_w, width);
    assert_eq!(dec_h, height);

    let colortype = decoder.colortype().expect("Failed to read colortype");
    assert_eq!(colortype, tiff::ColorType::RGB(16));

    let decoded_data = match decoder.read_image().expect("Failed to decode TIFF") {
        tiff::decoder::DecodingResult::U16(vec) => vec,
        _ => panic!("Expected U16 TIFF decoding result"),
    };

    let get_sample = |x: u32, y: u32, ch: usize| -> u16 {
        let idx = ((y * width + x) * 3) as usize + ch;
        decoded_data[idx]
    };

    // Empirical Verification A: Pure Black (0.0)
    for x in 0..width {
        let s = get_sample(x, 4, 0);
        assert!(s <= 1, "Pure black (0.0) must be <= 1 ADU, got {}", s);
    }

    // Empirical Verification B: Deep Shadow (0.001)
    // 0.001 * 65535 = 65.535, with TPDF dither [-0.5, 0.5] -> 65 or 66 ADU
    for x in 0..width {
        let s = get_sample(x, 12, 0);
        assert!(
            (s as i32 - 66).abs() <= 2,
            "Deep shadow (0.001) must map to ~65-66 ADU, got {}",
            s
        );
        assert!(s > 0, "Deep shadow must preserve non-zero gradation");
    }

    // Empirical Verification C: Linear 18% Gray vs Tone-Mapped Midtone
    // 0.18 linear * 65535 = 11796. If un-tone-mapped linear radiance were saved,
    // standard sRGB viewing gamma (2.2) collapses it to (11796/65535)^2.2 ~ 0.0229 * 65535 ~ 1500!
    let linear_sample = get_sample(10, 20, 0);
    assert!(
        (linear_sample as i32 - 11796).abs() <= 3,
        "Linear 0.18 should map to ~11796 in integer storage, got {}",
        linear_sample
    );

    // But tone-mapped preview midtone is 0.50:
    // 0.50 * 65535 = 32767.5 -> 32768 ADU.
    // Under sRGB display gamma, (32768/65535)^2.2 ~ 0.18 Zone V!
    let tm_sample = get_sample(10, 28, 0);
    assert!(
        (tm_sample as i32 - 32768).abs() <= 2,
        "Tone-mapped midtone (0.50) must map to ~32768 ADU, got {}",
        tm_sample
    );
    assert!(
        tm_sample > 30000,
        "Tone-mapped TIFF midtone MUST NOT collapse to ~1500 or ~11796, got {}",
        tm_sample
    );

    // Empirical Verification D: Peak Highlight (1.0)
    for x in 0..width {
        let s = get_sample(x, 36, 0);
        assert_eq!(s, 65535, "Peak highlight (1.0) must map to 65535 ADU, got {}", s);
    }

    // Empirical Verification E: Super-white / Over-range floats (1.5, 100.0)
    // Must clamp to 65535 without wrapping/overflow
    for x in 0..width {
        let s_1_5 = get_sample(x, 44, 0);
        assert_eq!(s_1_5, 65535, "Over-range 1.5 must clamp cleanly to 65535, got {}", s_1_5);

        let s_100 = get_sample(x, 52, 0);
        assert_eq!(s_100, 65535, "Extreme over-range 100.0 must clamp cleanly to 65535, got {}", s_100);
    }

    // Empirical Verification F: Negative floats (-0.50)
    // Must clamp to 0 without underflowing to 65535
    for x in 0..width {
        let s_neg = get_sample(x, 60, 0);
        assert_eq!(s_neg, 0, "Negative float must clamp cleanly to 0, got {}", s_neg);
    }

    let _ = std::fs::remove_file(&tiff_path);
}

#[test]
fn test_adversarial_continuous_gradation_shadow_ramp() {
    let width = 256u32;
    let height = 8u32;
    let mut test_img = Rgb32FImage::new(width, height);

    // Create a fine gradient in the deepest shadows from 0.00000 to 0.00500
    for x in 0..width {
        let t = x as f32 / (width - 1) as f32;
        let v = t * 0.00500;
        for y in 0..height {
            test_img.put_pixel(x, y, Rgb([v, v, v]));
        }
    }

    let temp_dir = std::env::temp_dir();
    let tiff_path = temp_dir.join("adversarial_test_m1_ramp.tiff");

    save_tiff_compressed(&tiff_path, &test_img).expect("Failed to save shadow ramp TIFF");

    let file = File::open(&tiff_path).expect("Failed to open saved TIFF");
    let mut decoder = tiff::decoder::Decoder::new(file).expect("Failed to create TIFF decoder");
    let data = match decoder.read_image().expect("Failed to decode TIFF") {
        tiff::decoder::DecodingResult::U16(vec) => vec,
        _ => panic!("Expected U16"),
    };

    // Calculate average value at start (x=0) and end (x=255)
    let avg_start: f64 = (0..height).map(|y| data[((y * width) * 3) as usize] as f64).sum::<f64>() / height as f64;
    let avg_end: f64 = (0..height).map(|y| data[((y * width + (width - 1)) * 3) as usize] as f64).sum::<f64>() / height as f64;

    assert!(avg_start <= 1.0, "Start average must be near 0");
    // 0.005 * 65535 = 327.675 ADU
    assert!(
        (avg_end - 327.675).abs() < 5.0,
        "End average must be near 328 ADU, got {}",
        avg_end
    );

    // Check monotonic trend across 16 segments
    let seg_size = (width / 16) as usize;
    let mut prev_seg_avg = 0.0;
    for s in 0..16 {
        let mut sum = 0.0;
        let mut count = 0;
        for x in (s * seg_size)..((s + 1) * seg_size) {
            for y in 0..height {
                sum += data[((y * width + x as u32) * 3) as usize] as f64;
                count += 1;
            }
        }
        let seg_avg = sum / count as f64;
        if s > 0 {
            assert!(
                seg_avg >= prev_seg_avg - 0.5,
                "Shadow gradation must be monotonically increasing on average across segments: seg {} ({}) < seg {} ({})",
                s, seg_avg, s - 1, prev_seg_avg
            );
        }
        prev_seg_avg = seg_avg;
    }

    let _ = std::fs::remove_file(&tiff_path);
}

#[test]
fn test_adversarial_floating_point_special_values() {
    let width = 8u32;
    let height = 8u32;
    let mut test_img = Rgb32FImage::new(width, height);

    // Put NaN and Infs
    for y in 0..height {
        for x in 0..width {
            let val = match x % 3 {
                0 => f32::NAN,
                1 => f32::INFINITY,
                _ => f32::NEG_INFINITY,
            };
            test_img.put_pixel(x, y, Rgb([val, val, val]));
        }
    }

    let temp_dir = std::env::temp_dir();
    let tiff_path = temp_dir.join("adversarial_test_m1_special_floats.tiff");

    // Rust clamp implementation:
    // NaN.clamp(0.0, 1.0) is NaN, NaN as u16 is 0.
    // +INF.clamp(0.0, 1.0) is 1.0, 1.0 * 65535 is 65535.
    // -INF.clamp(0.0, 1.0) is 0.0, 0.0 * 65535 is 0.
    let save_res = save_tiff_compressed(&tiff_path, &test_img);
    assert!(save_res.is_ok(), "save_tiff_compressed must not panic on NaN or Inf");

    let file = File::open(&tiff_path).expect("Failed to open saved TIFF");
    let mut decoder = tiff::decoder::Decoder::new(file).expect("Failed to create TIFF decoder");
    let data = match decoder.read_image().expect("Failed to decode TIFF") {
        tiff::decoder::DecodingResult::U16(vec) => vec,
        _ => panic!("Expected U16"),
    };

    for y in 0..height {
        for x in 0..width {
            let sample = data[((y * width + x) * 3) as usize];
            match x % 3 {
                0 => assert_eq!(sample, 0, "NaN must safely quantize to 0"),
                1 => assert_eq!(sample, 65535, "+Inf must safely clamp to 65535"),
                _ => assert_eq!(sample, 0, "-Inf must safely clamp to 0"),
            }
        }
    }

    let _ = std::fs::remove_file(&tiff_path);
}

#[test]
fn test_adversarial_progress_regex_stress_and_edge_cases() {
    let re = regex::Regex::new(r"(\d+)%").expect("Valid regex pattern");

    // 1. Edge Case: Bracket filenames containing percentage signs and numbers
    // In HdrModal.tsx: const pctMatch = progressMessage?.match(/(\d+)%/);
    // If filename has percentage, e.g. "IMG_50%_001.CR2", what happens?
    let problematic_msg = "Decoding raw exposure 1/3 (IMG_50%_001.CR2)... 11%";
    let caps = re.captures(problematic_msg).unwrap();
    let first_match = &caps[1];
    println!("Filename with % captures: '{}' vs real progress '11'", first_match);
    // Note: This reveals that /(\d+)%/ without anchoring captures the FIRST occurrence!
    // In typical usage, filenames rarely have "%", but if they do, 50% is captured instead of 11%.
    // To be 100% robust, the frontend or backend could match the LAST percentage:
    // e.g. /.*?(\d+)%\s*$/ or backend emits percentage at end.

    // 2. Edge Case: Boundary percentages 0% and 100%
    let msg_0 = "Initializing pipeline... 0%";
    let caps_0 = re.captures(msg_0).unwrap();
    assert_eq!(&caps_0[1], "0");
    let val_0: u32 = caps_0[1].parse().unwrap();
    assert_eq!(val_0, 0);

    let msg_100 = "HDR synthesis complete... 100%";
    let caps_100 = re.captures(msg_100).unwrap();
    assert_eq!(&caps_100[1], "100");
    let val_100: u32 = caps_100[1].parse().unwrap();
    assert_eq!(val_100, 100);

    // 3. Monotonic Progression in Handheld Pipeline
    let handheld_sequence = [
        ("Decoding raw exposure 1/3 (IMG_001.CR2)... 5%", 5),
        ("Decoding raw exposure 2/3 (IMG_002.CR2)... 11%", 11),
        ("Decoding raw exposure 3/3 (IMG_003.CR2)... 18%", 18),
        ("Aligning handheld bracket 1/3 (EV ratio 1.00x)... 25%", 25),
        ("Aligning handheld bracket 2/3 (EV ratio 2.00x)... 33%", 33),
        ("Aligning handheld bracket 3/3 (EV ratio 4.00x)... 41%", 41),
        ("Synthesizing 32-bit linear radiance map... 52%", 52),
        ("Merging dynamic range highlights & deep shadows... 60%", 60),
        ("Linear radiance map complete... 65%", 65),
        ("Synthesizing HDR tones: Multi-scale Laplacian pyramid... 68%", 68),
        ("Constructing multi-scale Gaussian weight pyramids... 72%", 72),
        ("Synthesizing HDR tones: Multi-scale fusion channel 1/3... 76%", 76),
        ("Synthesizing HDR tones: Multi-scale fusion channel 2/3... 82%", 82),
        ("Synthesizing HDR tones: Multi-scale fusion channel 3/3... 88%", 88),
        ("Applying Oklab perceptual grading & dynamic range anchor... 94%", 94),
        ("Creating high-fidelity preview... 98%", 98),
        ("HDR synthesis complete... 100%", 100),
    ];

    let mut last_pct = 0;
    for (msg, expected_pct) in handheld_sequence {
        let c = re.captures(msg).expect("Must match");
        let parsed: u32 = c[1].parse().expect("Valid u32");
        assert_eq!(parsed, expected_pct);
        assert!(
            parsed >= last_pct,
            "Progress percentage must be monotonically increasing: {} < {}",
            parsed,
            last_pct
        );
        last_pct = parsed;
    }
}
