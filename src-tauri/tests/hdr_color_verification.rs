use rapidraw_lib::hdr_color_science::*;

#[test]
fn test_pq_round_trip_exactness() {
    // Test across a massive dynamic range from deep shadow to 100x diffuse white
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
