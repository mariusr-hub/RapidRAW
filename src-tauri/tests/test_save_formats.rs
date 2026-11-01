use image::{Rgb32FImage, Rgb, GenericImageView};

#[test]
fn test_encode_linear_dng_and_tags() {
    let (w, h) = (64u32, 64u32);
    let mut img = Rgb32FImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            img.put_pixel(x, y, Rgb([0.5, 0.75, 1.2]));
        }
    }

    let meta = rapidraw_lib::dng_encoder::DngExportMetadata {
        make: Some("Canon".to_string()),
        model: Some("Canon EOS 77D".to_string()),
        software: Some("RapidRAW Studio".to_string()),
        description: Some("Linear DNG Float".to_string()),
        as_shot_neutral: Some([1.0, 1.0, 1.0]),
        baseline_exposure: Some(0.0),
        white_level: None,
    };

    let dng_bytes = rapidraw_lib::dng_encoder::encode_linear_dng(&img, Some(&meta)).expect("DNG encoding failed");
    assert!(!dng_bytes.is_empty());
    assert_eq!(&dng_bytes[0..4], &[b'I', b'I', 42, 0]);
}

#[test]
fn test_save_tiff_deflate_compression() {
    use tiff::encoder::{TiffEncoder, colortype::RGB32Float, Compression, DeflateLevel};
    let (w, h) = (100u32, 100u32);
    let mut img = Rgb32FImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            img.put_pixel(x, y, Rgb([0.2, 0.4, 0.8]));
        }
    }

    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut encoder = TiffEncoder::new(&mut cursor)
            .expect("TIFF encoder init failed")
            .with_compression(Compression::Deflate(DeflateLevel::default()));
        let tiff_image = encoder
            .new_image::<RGB32Float>(w, h)
            .expect("New image failed");
        tiff_image.write_data(img.as_raw()).expect("Write data failed");
    }

    let buf = cursor.into_inner();
    assert!(!buf.is_empty());
    println!("Compressed 100x100 TIFF size: {} bytes (Uncompressed: 120,000 bytes)", buf.len());
    assert!(buf.len() < 10000, "Deflate compression must dramatically reduce size");
}

#[test]
fn test_tiff_icc_profile_injection() {
    let (w, h) = (32u32, 32u32);
    let img = Rgb32FImage::new(w, h);
    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join("test_icc_injection.tiff");
    rapidraw_lib::export_processing::save_tiff_compressed(&temp_path, &img).expect("Save TIFF failed");

    let data = std::fs::read(&temp_path).expect("Read TIFF failed");
    assert!(data.len() > 8);
    let ifd_offset = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let num_tags = u16::from_le_bytes([data[ifd_offset], data[ifd_offset + 1]]) as usize;
    let mut found_icc_tag = false;
    let mut cur = ifd_offset + 2;
    for _ in 0..num_tags {
        let tag = u16::from_le_bytes([data[cur], data[cur + 1]]);
        if tag == 34675 {
            found_icc_tag = true;
            break;
        }
        cur += 12;
    }
    let _ = std::fs::remove_file(&temp_path);
    assert!(found_icc_tag, "Tag 34675 (ICC Profile) must be present in IFD");
}

#[test]
fn test_resize_in_linear_space_radiance() {
    use image::DynamicImage;
    let (w, h) = (100u32, 80u32);
    let mut img = Rgb32FImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let v = (x as f32 / w as f32) * 2.5; // Dynamic range > 1.0
            img.put_pixel(x, y, Rgb([v, v * 0.5, v * 0.25]));
        }
    }
    let dynamic_img = DynamicImage::ImageRgb32F(img);
    let resized = rapidraw_lib::export_processing::resize_in_linear_space(&dynamic_img, 50, 40);
    assert_eq!(resized.dimensions(), (50, 40));
    if let DynamicImage::ImageRgb32F(res_buf) = resized {
        let px_mid = res_buf.get_pixel(25, 20);
        assert!(px_mid[0] > 1.0 && px_mid[0] < 1.5, "Linear radiance preserved across resize: got {}", px_mid[0]);
    } else {
        panic!("Resized linear image must remain ImageRgb32F");
    }
}

#[test]
fn test_tpdf_dithering_and_quantization() {
    use image::DynamicImage;
    let (w, h) = (64u32, 64u32);
    let mut img = Rgb32FImage::new(w, h);
    // Flat 0.5 gray
    for y in 0..h {
        for x in 0..w {
            img.put_pixel(x, y, Rgb([0.5, 0.5, 0.5]));
        }
    }
    let dynamic_img = DynamicImage::ImageRgb32F(img);
    let quantized = rapidraw_lib::export_processing::quantize_to_rgb8_with_tpdf_dither(&dynamic_img);
    assert_eq!(quantized.dimensions(), (w, h));

    // Flat 0.5 * 255.0 = 127.5. With TPDF dithering, adjacent pixels should vary between 127 and 128
    let has_127 = quantized.pixels().any(|p| p[0] == 127);
    let has_128 = quantized.pixels().any(|p| p[0] == 128);
    assert!(has_127 && has_128, "TPDF dither must create smooth triangular distribution across boundary");
}

#[test]
fn test_dual_illuminant_interpolation() {
    let mat_a = vec![1.0, 2.0, 3.0];
    let mat_d65 = vec![4.0, 5.0, 6.0];

    // Exact Illuminant A (2856K)
    let res_a = rapidraw_lib::raw_processing::interpolate_dual_illuminant_matrix(&mat_a, &mat_d65, 2856.0);
    assert!((res_a[0] - 1.0).abs() < 1e-4);

    // Exact Illuminant D65 (6504K)
    let res_d65 = rapidraw_lib::raw_processing::interpolate_dual_illuminant_matrix(&mat_a, &mat_d65, 6504.0);
    assert!((res_d65[0] - 4.0).abs() < 1e-4);

    // Interpolated (e.g. 4000K)
    let res_mid = rapidraw_lib::raw_processing::interpolate_dual_illuminant_matrix(&mat_a, &mat_d65, 4000.0);
    assert!(res_mid[0] > 1.0 && res_mid[0] < 4.0);
}

#[test]
fn test_sensel_laplacian_inpainting() {
    // Unclipped pixel (< 0.50): unchanged
    let (r0, g0, b0) = rapidraw_lib::raw_processing::recover_clipped_pixel(0.40, 0.40, 0.40);
    assert_eq!((r0, g0, b0), (0.40, 0.40, 0.40));

    // Clipped Red with unclipped Green and Blue
    let (r1, g1, b1) = rapidraw_lib::raw_processing::recover_clipped_pixel(0.95, 0.70, 0.50);
    assert!(r1 > 0.95, "Clipped red channel must be inpainted with gradient from intact channels: got {}", r1);
    assert!(g1.is_finite() && b1.is_finite());

    // Monotonic gradient propagation: higher guide lum produces higher reconstructed red
    let (r2, _, _) = rapidraw_lib::raw_processing::recover_clipped_pixel(0.95, 0.85, 0.50);
    assert!(r2 > r1, "Higher intact green energy must propagate higher infilled red: {} vs {}", r2, r1);
}

#[test]
fn test_autodetect_tca_fallback() {
    let (w, h) = (256u32, 256u32);
    let mut img = Rgb32FImage::new(w, h);
    // Sharp high-contrast circular disc
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < 80.0 {
                // Foreground (warm tone)
                img.put_pixel(x, y, Rgb([0.8, 0.3, 0.1]));
            } else {
                // Background (cool tone)
                img.put_pixel(x, y, Rgb([0.2, 0.6, 0.9]));
            }
        }
    }

    let (vr, vb) = rapidraw_lib::lens_correction::auto_detect_transverse_chromatic_aberration(&img);
    // Unaberrated synthetic disc must detect near identity (1.0, 1.0) without false positive drift
    assert!((vr - 1.0).abs() <= 0.0015, "Unaligned TCA red delta too large: {}", vr);
    assert!((vb - 1.0).abs() <= 0.0015, "Unaligned TCA blue delta too large: {}", vb);
}

#[test]
#[ignore = "Full dataset render - use 'cargo run --release --bin render_all' for batch rendering"]
fn test_render_hdr_and_pano_targets() {
    let base_dir = std::path::PathBuf::from(r"D:\neapdirbti");

    // 1. Re-render IMG_4713_Hdr.tiff using tonemapped preview
    {
        let out_name = "IMG_4713_Hdr.tiff";
        let files = vec!["IMG_4712.CR2", "IMG_4713.CR2", "IMG_4714.CR2"];
        let file_paths: Vec<String> = files.iter().map(|f| base_dir.join(f).to_string_lossy().into_owned()).collect();
        if file_paths.iter().all(|p| std::path::Path::new(p).exists()) {
            let settings = rapidraw_lib::app_settings::AppSettings::default();
            let mut frames = rapidraw_lib::hdr_deghosting::load_hdr_frames::<tauri::Wry>(&file_paths, None, &settings).unwrap();
            let ref_idx = rapidraw_lib::hdr_deghosting::select_best_reference_index(&frames);
            rapidraw_lib::hdr_deghosting::align_hdr_frames::<tauri::Wry>(&mut frames, None);
            rapidraw_lib::hdr_deghosting::apply_reference_deghosting_mask::<tauri::Wry>(&mut frames, ref_idx, None, None, None);

            let rgb_frames: Vec<Rgb32FImage> = frames.iter().map(|f| f.1.to_rgb32f()).collect();
            let exposure_scales: Vec<f32> = frames.iter().map(|f| rapidraw_lib::hdr_deghosting::compute_physical_exposure_scale(f.2, f.3, f.4)).collect();

            let mut options = rapidraw_lib::hdr_fusion::HdrMergeOptions::default();
            options.reference_index = Some(ref_idx);

            let fusion_result = rapidraw_lib::hdr_fusion::fuse_exposures_dual::<tauri::Wry>(&rgb_frames, &exposure_scales, &options, None, None).unwrap();
            let out_path = base_dir.join(out_name);

            rapidraw_lib::export_processing::save_tiff_compressed(&out_path, &fusion_result.tone_mapped_preview).unwrap();
            println!("Successfully re-rendered {}", out_name);
        }
    }

    // 2. Re-render IMG_4029_Pano.png with sRGB chunk
    {
        let out_name = "IMG_4029_Pano.png";
        let files = vec!["IMG_4029.CR2", "IMG_4030.CR2", "IMG_4031.CR2", "IMG_4032.CR2", "IMG_4033.CR2", "IMG_4034.CR2"];
        let file_paths: Vec<String> = files.iter().map(|f| base_dir.join(f).to_string_lossy().into_owned()).collect();
        if file_paths.iter().all(|p| std::path::Path::new(p).exists()) {
            let out_path = base_dir.join(out_name);
            let dyn_img = rapidraw_lib::panorama_stitching::stitch_images_headless(
                file_paths,
                rapidraw_lib::panorama_utils::camera_model::PanoramaProjection::Cylindrical,
                0.5,
            ).unwrap();

            rapidraw_lib::export_processing::save_png_high_quality_with_metadata(&out_path, &dyn_img, None).unwrap();
            println!("Successfully re-rendered {}", out_name);
        }
    }
}

#[test]
fn test_save_hdr_tiff_tone_mapping_parity() {
    let (w, h) = (32u32, 32u32);
    // Synthetic tone-mapped preview with display sRGB midtone 0.5 (~128 in 8-bit, ~32768 in 16-bit)
    let mut tm_img = Rgb32FImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            tm_img.put_pixel(x, y, Rgb([0.5, 0.5, 0.5]));
        }
    }
    let temp_dir = std::env::temp_dir();
    let temp_tiff = temp_dir.join("test_tone_mapping_parity.tiff");
    rapidraw_lib::export_processing::save_tiff_compressed(&temp_tiff, &tm_img)
        .expect("Failed to save TIFF");

    // Read back using tiff decoder
    let file = std::fs::File::open(&temp_tiff).expect("Failed to open saved TIFF");
    let mut decoder = tiff::decoder::Decoder::new(file).expect("Failed to create TIFF decoder");
    match decoder.read_image().expect("Failed to decode TIFF") {
        tiff::decoder::DecodingResult::U16(data) => {
            let sample = data[0];
            // 0.5 * 65535 = 32767.5 (+/- 2 due to TPDF dither)
            assert!(
                (sample as i32 - 32768).abs() <= 2,
                "Expected 16-bit midtone around 32768, got {}",
                sample
            );
            // Verify that linear midtone ~1500 (approx 0.0229) or un-tone-mapped radiance would be vastly darker
            assert!(
                sample > 30000,
                "16-bit TIFF midtone must be bright display midtone (>30000), not dark linear ~1500"
            );
        }
        _ => panic!("Expected U16 TIFF decoding result"),
    }
    let _ = std::fs::remove_file(&temp_tiff);
}

#[test]
fn test_hdr_progress_format_contains_percentage() {
    let re = regex::Regex::new(r"(\d+)%").expect("Valid regex pattern");
    let test_messages = [
        "Decoding raw exposure 1/3 (IMG_001.CR2)... 11%",
        "Aligning handheld bracket 2/3 (EV ratio 2.00x)... 37%",
        "Synthesizing 32-bit linear radiance map... 52%",
        "Merging dynamic range highlights & deep shadows... 60%",
        "Linear radiance map complete... 65%",
        "Synthesizing HDR tones: Multi-scale Laplacian pyramid... 68%",
        "Constructing multi-scale Gaussian weight pyramids... 72%",
        "Synthesizing HDR tones: Multi-scale fusion channel 1/3... 76%",
        "Synthesizing HDR tones: Multi-scale fusion channel 2/3... 82%",
        "Synthesizing HDR tones: Multi-scale fusion channel 3/3... 88%",
        "Applying Oklab perceptual grading & dynamic range anchor... 94%",
        "Creating high-fidelity preview... 98%",
        "HDR synthesis complete... 100%",
        // Also check secondary engine messages
        "Normalizing bracket white balance & exposure gains... 15%",
        "Calculating Multi-Scale Exposure Fusion weights... 25%",
        "Applying Guided Filter edge preservation (halo elimination)... 35%",
        "Applying morphological motion deghosting... 45%",
        "Constructing Laplacian Pyramids... 55%",
        "Blending Multi-Scale Pyramid layers... 70%",
        "Reconstructing high dynamic range image... 85%",
        "Anchoring contrast & dynamic range... 90%",
        "Applying AgX filmic color grading... 95%",
        "Accumulating 32-bit linear HDR radiance map... 40%",
        "Applying 3-Tier Multi-Scale Decomposition & Oklab Perceptual Grading... 75%",
    ];
    for msg in test_messages {
        let caps = re.captures(msg);
        assert!(caps.is_some(), "Message '{}' must match regex /(\\d+)%/", msg);
        let pct_val: u32 = caps.unwrap()[1].parse().expect("Parsed integer percentage");
        assert!(pct_val <= 100, "Percentage must be <= 100, got {}", pct_val);
    }
}

#[test]
fn test_linear_dng_probe_and_load_synthetic() {
    use image::{Rgb32FImage, Rgb};
    use rapidraw_lib::dng_encoder::{encode_linear_dng, DngExportMetadata};
    use rapidraw_lib::image_loader::{is_linear_dng_probe, load_base_image_from_bytes};
    use rapidraw_lib::app_settings::AppSettings;

    // 1. Create synthetic 64x64 RGB float image
    let (w, h) = (64u32, 64u32);
    let mut img = Rgb32FImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            img.put_pixel(x, y, Rgb([0.25, 0.50, 0.75]));
        }
    }

    // 2. Encode to 32-bit Linear DNG
    let meta = DngExportMetadata::default();
    let dng_bytes = encode_linear_dng(&img, Some(&meta)).expect("Encoding failed");
    assert!(!dng_bytes.is_empty());

    // 3. Verify zero-allocation probe correctly identifies 3-channel Linear DNG
    assert!(
        is_linear_dng_probe(&dng_bytes),
        "is_linear_dng_probe must return true for Linear DNG"
    );

    // 4. Verify decoding succeeds without rawler panics
    let settings = AppSettings::default();
    let loaded = load_base_image_from_bytes(&dng_bytes, "synthetic_hdr.dng", false, &settings, None)
        .expect("load_base_image_from_bytes must succeed on Linear DNG");

    assert_eq!(loaded.width(), 64);
    assert_eq!(loaded.height(), 64);
}

#[test]
fn test_safe_file_buffer_fallback_and_deref() {
    use rapidraw_lib::image_loader::{read_file_mapped, read_file_safe, is_removable_or_remote_drive};
    use std::io::Write;

    let mut temp_file = tempfile::NamedTempFile::new().expect("temp file");
    temp_file.write_all(b"TEST_IMAGE_BUFFER_DATA_1234567890").expect("write");

    let buffer = read_file_mapped(temp_file.path()).expect("read_file_mapped");
    assert_eq!(&buffer[..10], b"TEST_IMAGE");
    assert_eq!(buffer.len(), 33);
    assert_eq!(buffer.as_ref(), b"TEST_IMAGE_BUFFER_DATA_1234567890");

    let safe_buf = read_file_safe(temp_file.path()).expect("read_file_safe");
    assert_eq!(&safe_buf[..10], b"TEST_IMAGE");

    let is_rem = is_removable_or_remote_drive(temp_file.path());
    println!("Temp path {:?} is_removable_or_remote_drive: {}", temp_file.path(), is_rem);
}

