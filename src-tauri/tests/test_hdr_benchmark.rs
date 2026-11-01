use image::{DynamicImage, ImageFormat, Rgb32FImage};
use std::fs;
use std::path::Path;
use std::time::Instant;

#[test]
fn test_hdr_all_benchmarks() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let base_dir = Path::new("D:/neapdirbti");
            if !base_dir.exists() {
                eprintln!("Test directory {:?} does not exist. Skipping benchmark.", base_dir);
                return;
            }

            let out_dir = base_dir.join("debug_hdr");
            let _ = fs::create_dir_all(&out_dir);

            let test_sets = [
                ("4002", vec!["IMG_4002.CR2", "IMG_4003.CR2", "IMG_4004.CR2"]),
                ("4068", vec!["IMG_4068.CR2", "IMG_4069.CR2", "IMG_4070.CR2"]),
                ("4659", vec!["IMG_4659.CR2", "IMG_4660.CR2", "IMG_4661.CR2"]),
                ("4821", vec!["IMG_4821.CR2", "IMG_4822.CR2", "IMG_4823.CR2"]),
                ("4515", vec!["IMG_4515.CR2", "IMG_4516.CR2", "IMG_4517.CR2"]),
                ("4221", vec!["IMG_4221.CR2", "IMG_4222.CR2", "IMG_4223.CR2"]),
            ];

            let settings = rapidraw_lib::app_settings::AppSettings::default();

            for (name, files) in test_sets {
                let file_paths: Vec<String> = files
                    .iter()
                    .map(|f| base_dir.join(f).to_string_lossy().into_owned())
                    .collect();

                let all_exist = file_paths.iter().all(|p| Path::new(p).exists());
                if !all_exist {
                    println!("Skipping set {} - files not found", name);
                    continue;
                }

                println!("============================================================");
                println!("TESTING HDR SET: {}", name);
                let start = Instant::now();

                let mut frames = match rapidraw_lib::hdr_deghosting::load_hdr_frames::<tauri::Wry>(&file_paths, None, &settings) {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("Failed to load frames for set {}: {}", name, e);
                        continue;
                    }
                };

                let ref_idx = rapidraw_lib::hdr_deghosting::select_best_reference_index(&frames);
                println!("Set {}: Selected best reference frame index: {}", name, ref_idx);

                println!("Set {}: Running 3-DoF Euclidean alignment...", name);
                rapidraw_lib::hdr_deghosting::align_hdr_frames::<tauri::Wry>(&mut frames, None);

                println!("Set {}: Running Dinic Graph-Cut deghosting...", name);
                rapidraw_lib::hdr_deghosting::apply_reference_deghosting_mask::<tauri::Wry>(&mut frames, ref_idx, None, None, None);

                let rgb_frames: Vec<Rgb32FImage> = frames.iter().map(|f| f.1.to_rgb32f()).collect();
                let exposure_scales: Vec<f32> = frames
                    .iter()
                    .map(|f| rapidraw_lib::hdr_deghosting::compute_physical_exposure_scale(f.2, f.3, f.4))
                    .collect();

                // 1. Test Physical Linear Radiance Engine (Halo-Free, True 16+ EV)
                println!("Set {}: Running Linear Radiance Physical Engine...", name);
                let mut rad_options = rapidraw_lib::hdr_fusion::HdrMergeOptions::default();
                rad_options.reference_index = Some(ref_idx);
                rad_options.engine = Some(rapidraw_lib::hdr_fusion::HdrEngineMode::LinearRadiance);

                let rad_fused = match rapidraw_lib::hdr_fusion::fuse_exposures_mertens::<tauri::Wry>(
                    &rgb_frames,
                    &exposure_scales,
                    &rad_options,
                    None,
                    None,
                ) {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("Linear Radiance fusion failed for {}: {}", name, e);
                        continue;
                    }
                };

                let out_rad_tiff = out_dir.join(format!("{}_Hdr_LinearRadiance.tiff", name));
                let rad_dyn = DynamicImage::ImageRgb32F(rad_fused);
                let _ = rad_dyn.save_with_format(&out_rad_tiff, ImageFormat::Tiff);
                let out_rad_jpg = out_dir.join(format!("{}_Hdr_LinearRadiance_preview.jpg", name));
                let _ = rad_dyn.to_rgb8().save_with_format(&out_rad_jpg, ImageFormat::Jpeg);
                println!("Saved Linear Radiance to {:?}", out_rad_jpg);

                // 2. Test Fixed Mertens Guided Filter Engine
                println!("Set {}: Running Mertens Guided Filter Engine...", name);
                let mut mertens_options = rapidraw_lib::hdr_fusion::HdrMergeOptions::default();
                mertens_options.reference_index = Some(ref_idx);
                mertens_options.engine = Some(rapidraw_lib::hdr_fusion::HdrEngineMode::MertensLaplacian);

                let mertens_fused = match rapidraw_lib::hdr_fusion::fuse_exposures_mertens::<tauri::Wry>(
                    &rgb_frames,
                    &exposure_scales,
                    &mertens_options,
                    None,
                    None,
                ) {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("Mertens fusion failed for {}: {}", name, e);
                        continue;
                    }
                };

                let out_mertens_tiff = out_dir.join(format!("{}_Hdr_MertensGuided.tiff", name));
                let mertens_dyn = DynamicImage::ImageRgb32F(mertens_fused);
                let _ = mertens_dyn.save_with_format(&out_mertens_tiff, ImageFormat::Tiff);
                let out_mertens_jpg = out_dir.join(format!("{}_Hdr_MertensGuided_preview.jpg", name));
                let _ = mertens_dyn.to_rgb8().save_with_format(&out_mertens_jpg, ImageFormat::Jpeg);
                println!("Saved Mertens Guided to {:?}", out_mertens_jpg);

                let elapsed = start.elapsed();
                println!("Set {} completed in {:.2?}", name, elapsed);
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn test_hdr_linear_radiance_focused() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let base_dir = Path::new("D:/neapdirbti");
            if !base_dir.exists() {
                return;
            }
            let out_dir = base_dir.join("debug_hdr");
            let _ = fs::create_dir_all(&out_dir);

            let test_sets = [
                ("4002", vec!["IMG_4002.CR2", "IMG_4003.CR2", "IMG_4004.CR2"]),
                ("4068", vec!["IMG_4068.CR2", "IMG_4069.CR2", "IMG_4070.CR2"]),
                ("4659", vec!["IMG_4659.CR2", "IMG_4660.CR2", "IMG_4661.CR2"]),
                ("4821", vec!["IMG_4821.CR2", "IMG_4822.CR2", "IMG_4823.CR2"]),
                ("4515", vec!["IMG_4515.CR2", "IMG_4516.CR2", "IMG_4517.CR2"]),
                ("4221", vec!["IMG_4221.CR2", "IMG_4222.CR2", "IMG_4223.CR2"]),
            ];

            let settings = rapidraw_lib::app_settings::AppSettings::default();

            for (name, files) in test_sets {
                let file_paths: Vec<String> = files
                    .iter()
                    .map(|f| base_dir.join(f).to_string_lossy().into_owned())
                    .collect();

                let all_exist = file_paths.iter().all(|p| Path::new(p).exists());
                if !all_exist {
                    continue;
                }

                println!("============================================================");
                println!("TESTING FOCUSED LINEAR RADIANCE: {}", name);
                let start = Instant::now();

                let mut frames = match rapidraw_lib::hdr_deghosting::load_hdr_frames::<tauri::Wry>(&file_paths, None, &settings) {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("Failed to load frames for {}: {}", name, e);
                        continue;
                    }
                };
                let ref_idx = rapidraw_lib::hdr_deghosting::select_best_reference_index(&frames);
                rapidraw_lib::hdr_deghosting::align_hdr_frames::<tauri::Wry>(&mut frames, None);
                rapidraw_lib::hdr_deghosting::apply_reference_deghosting_mask::<tauri::Wry>(&mut frames, ref_idx, None, None, None);

                let rgb_frames: Vec<Rgb32FImage> = frames.iter().map(|f| f.1.to_rgb32f()).collect();
                let exposure_scales: Vec<f32> = frames
                    .iter()
                    .map(|f| rapidraw_lib::hdr_deghosting::compute_physical_exposure_scale(f.2, f.3, f.4))
                    .collect();

                let mut rad_options = rapidraw_lib::hdr_fusion::HdrMergeOptions::default();
                rad_options.reference_index = Some(ref_idx);
                rad_options.engine = Some(rapidraw_lib::hdr_fusion::HdrEngineMode::LinearRadiance);

                let rad_fused = rapidraw_lib::hdr_fusion::fuse_exposures_mertens::<tauri::Wry>(
                    &rgb_frames,
                    &exposure_scales,
                    &rad_options,
                    None,
                    None,
                ).unwrap();

                let out_rad_jpg = out_dir.join(format!("{}_Hdr_LinearRadiance_preview.jpg", name));
                let rad_dyn = DynamicImage::ImageRgb32F(rad_fused);
                let _ = rad_dyn.to_rgb8().save_with_format(&out_rad_jpg, ImageFormat::Jpeg);
                println!("Saved focused Linear Radiance to {:?} in {:.2?}", out_rad_jpg, start.elapsed());
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn test_handheld_hdr_reference_locked_benchmark() {
    let base_dir = Path::new("D:/neapdirbti");
    if !base_dir.exists() {
        eprintln!("D:/neapdirbti not found, skipping.");
        return;
    }
    let scratch_dir = Path::new("d:/RapidRAW_V.03/scratch");
    let _ = fs::create_dir_all(scratch_dir);

    let settings = rapidraw_lib::app_settings::AppSettings::default();
    let opts = rapidraw_lib::hdr_fusion::HdrMergeOptions::default();

    // Verify temporal burst separator on the 6-file mixed burst (4164..4169)
    let mixed_paths: Vec<String> = (4164..=4169)
        .map(|i| base_dir.join(format!("IMG_{}.CR2", i)).to_string_lossy().into_owned())
        .collect();
    let filtered_first = rapidraw_lib::hdr_handheld::filter_coherent_burst(&mixed_paths, Some(0));
    let filtered_second = rapidraw_lib::hdr_handheld::filter_coherent_burst(&mixed_paths, Some(3));
    println!("Temporal Burst Filter on 4164..4169:");
    println!("  Burst 1 (anchor=0): {:?}", filtered_first);
    println!("  Burst 2 (anchor=3): {:?}", filtered_second);
    assert_eq!(filtered_first.len(), 3);
    assert_eq!(filtered_second.len(), 3);

    struct BenchCase<'a> {
        name: &'a str,
        files: Vec<&'a str>,
        crops: Vec<(&'a str, u32, u32, u32, u32)>,
    }

    let cases = vec![
        BenchCase {
            name: "4221",
            files: vec!["IMG_4221.CR2", "IMG_4222.CR2", "IMG_4223.CR2"],
            crops: vec![
                ("new_crop4221_cyclist.jpg", 1280, 1960, 1200, 1200),
                ("new_crop4221_sky_domes.jpg", 2780, 180, 2700, 1600),
            ],
        },
        BenchCase {
            name: "4167",
            files: vec!["IMG_4167.CR2", "IMG_4168.CR2", "IMG_4169.CR2"],
            crops: vec![
                ("new_crop4164_bokeh_mold.jpg", 1200, 0, 3000, 2000),
                ("new_crop4164_filaments.jpg", 2400, 760, 3000, 2800),
            ],
        },
        BenchCase {
            name: "4164",
            files: vec!["IMG_4164.CR2", "IMG_4165.CR2", "IMG_4166.CR2"],
            crops: vec![
                ("new_crop4164_blossoms.jpg", 1500, 800, 2400, 1800),
            ],
        },
        BenchCase {
            name: "4224",
            files: vec!["IMG_4224.CR2", "IMG_4225.CR2", "IMG_4226.CR2"],
            crops: vec![
                ("new_crop4224_boat_hull.jpg", 1380, 520, 4200, 2200),
                ("new_crop4224_twigs_boat.jpg", 500, 920, 2700, 2000),
            ],
        },
        BenchCase {
            name: "4782",
            files: vec!["IMG_4782.CR2", "IMG_4783.CR2", "IMG_4784.CR2"],
            crops: vec![
                ("new_crop4782_flowers_building.jpg", 800, 1200, 2400, 2400),
            ],
        },
    ];

    let filter = std::env::var("HDR_SET_FILTER").ok();

    for case in cases {
        if let Some(ref f) = filter {
            if !case.name.contains(f.as_str()) {
                continue;
            }
        }
        let paths: Vec<String> = case
            .files
            .iter()
            .map(|f| base_dir.join(f).to_string_lossy().into_owned())
            .collect();
        if !paths.iter().all(|p| Path::new(p).exists()) {
            println!("Skipping {} (missing files)", case.name);
            continue;
        }

        println!("============================================================");
        println!("RUNNING REFERENCE-LOCKED HANDHELD HDR ON SET: {}", case.name);
        let t0 = Instant::now();
        let res = rapidraw_lib::hdr_handheld::merge_handheld_hdr_pipeline::<tauri::Wry>(
            &paths,
            &opts,
            &settings,
            None,
            None,
        )
        .expect("Handheld HDR pipeline failed");
        println!("Set {} merged in {:.2?}", case.name, t0.elapsed());

        let rgb8 = DynamicImage::ImageRgb32F(res.tone_mapped_preview).to_rgb8();
        let full_out = scratch_dir.join(format!("new_full_{}.jpg", case.name));
        rgb8.save_with_format(&full_out, ImageFormat::Jpeg).unwrap();

        for (crop_name, cx, cy, cw, ch) in case.crops {
            let x0 = cx.min(rgb8.width().saturating_sub(1));
            let y0 = cy.min(rgb8.height().saturating_sub(1));
            let w_c = cw.min(rgb8.width() - x0);
            let h_c = ch.min(rgb8.height() - y0);
            let sub = image::imageops::crop_imm(&rgb8, x0, y0, w_c, h_c).to_image();
            let crop_path = scratch_dir.join(crop_name);
            sub.save_with_format(&crop_path, ImageFormat::Jpeg).unwrap();
            println!("  Saved crop {:?} ({}x{})", crop_path, w_c, h_c);
        }
    }
}

