use image::{DynamicImage, GenericImageView};
use rapidraw_lib::app_settings::AppSettings;
use rapidraw_lib::hdr_panorama::stitch_hdr_panorama_native;
use rapidraw_lib::panorama_stitching::stitch_images_native_with_radiance;
use rapidraw_lib::panorama_utils::camera_model::PanoramaProjection;
use std::path::Path;
use std::time::Instant;

fn measure_border_black_pct(img: &image::RgbImage) -> (f64, f64) {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return (0.0, 0.0);
    }
    let mut border_total = 0usize;
    let mut border_black = 0usize;
    for x in 0..w {
        for &y in &[0, h - 1] {
            border_total += 1;
            let p = img.get_pixel(x, y);
            if p[0] == 0 && p[1] == 0 && p[2] == 0 {
                border_black += 1;
            }
        }
    }
    for y in 1..h.saturating_sub(1) {
        for &x in &[0, w - 1] {
            border_total += 1;
            let p = img.get_pixel(x, y);
            if p[0] == 0 && p[1] == 0 && p[2] == 0 {
                border_black += 1;
            }
        }
    }
    let mut all_black = 0usize;
    for p in img.pixels() {
        if p[0] == 0 && p[1] == 0 && p[2] == 0 {
            all_black += 1;
        }
    }
    (
        100.0 * border_black as f64 / border_total.max(1) as f64,
        100.0 * all_black as f64 / (w as usize * h as usize).max(1) as f64,
    )
}

fn save_inspection_crops(img: &image::RgbImage, out_dir: &Path, prefix: &str) {
    let (w, h) = img.dimensions();
    if w < 200 || h < 200 {
        return;
    }
    let cw = (w / 3).clamp(400, 1400).min(w);
    let ch = (h / 2).clamp(400, 1000).min(h);

    // Left-center overlap / foreground crop
    let x_left = (w / 4).saturating_sub(cw / 2).min(w - cw);
    let y_mid = (h / 2).saturating_sub(ch / 2).min(h - ch);
    let crop_left = image::imageops::crop_imm(img, x_left, y_mid, cw, ch).to_image();
    let _ = crop_left.save(out_dir.join(format!("{}_crop_left_mid.jpg", prefix)));

    // Center crop (seam & subject area)
    let x_center = (w / 2).saturating_sub(cw / 2).min(w - cw);
    let crop_center = image::imageops::crop_imm(img, x_center, y_mid, cw, ch).to_image();
    let _ = crop_center.save(out_dir.join(format!("{}_crop_center.jpg", prefix)));

    // Bottom-center foreground crop (tracks / handrail)
    let y_bot = h.saturating_sub(ch);
    let crop_bot = image::imageops::crop_imm(img, x_center, y_bot, cw, ch).to_image();
    let _ = crop_bot.save(out_dir.join(format!("{}_crop_bottom_fg.jpg", prefix)));

    // Top-center sky crop
    let crop_sky = image::imageops::crop_imm(img, x_center, 0, cw, ch).to_image();
    let _ = crop_sky.save(out_dir.join(format!("{}_crop_top_sky.jpg", prefix)));
}

#[test]
fn run_upgraded_panorama_benchmarks() {
    let base_dir = Path::new(r"D:\neapdirbti");
    if !base_dir.exists() {
        println!("Skipping benchmark: D:\\neapdirbti not found");
        return;
    }
    let out_dir = Path::new(r"d:\RapidRAW_V.03\scratch\pano_upgraded");
    std::fs::create_dir_all(out_dir).unwrap();

    let settings = AppSettings::default();

    // 1. SDR Panorama: 3980..3983
    let sdr_paths: Vec<String> = (3980..=3983)
        .map(|n| format!(r"D:\neapdirbti\IMG_{}.CR2", n))
        .collect();
    if sdr_paths.iter().all(|p| Path::new(p).exists()) {
        println!("\n=== [1/6] Rendering SDR Panorama 3980..3983 ===");
        let t0 = Instant::now();
        let (disp, _lin) = stitch_images_native_with_radiance(
            sdr_paths,
            PanoramaProjection::Cylindrical,
            0.5,
            true,
            None,
            None,
        )
        .expect("SDR 3980..3983 failed");
        let rgb8 = disp.to_rgb8();
        let (b_pct, all_pct) = measure_border_black_pct(&rgb8);
        println!(
            "=== SDR 3980..3983 done in {:.2?}: {}x{}, border_black={:.4}%, total_black={:.4}% ===",
            t0.elapsed(),
            rgb8.width(),
            rgb8.height(),
            b_pct,
            all_pct
        );
        rgb8.save(out_dir.join("pano_3980_3983_upgraded.jpg")).unwrap();
        save_inspection_crops(&rgb8, out_dir, "pano_3980_3983");
    }

    // 2. HDR Panorama sets
    let hdr_sets: Vec<(&str, std::ops::RangeInclusive<u32>, f32)> = vec![
        ("4029_4034", 4029..=4034, 0.5),
        ("4044_4052", 4044..=4052, 0.5),
        ("4746_4751", 4746..=4751, 0.5),
        ("5021_5032", 5021..=5032, 0.5),
        ("4065_4088", 4065..=4088, 0.5),
    ];

    for (idx, (tag, range, warp)) in hdr_sets.into_iter().enumerate() {
        let paths: Vec<String> = range
            .map(|n| format!(r"D:\neapdirbti\IMG_{}.CR2", n))
            .collect();
        if !paths.iter().all(|p| Path::new(p).exists()) {
            println!("Skipping {} (missing files)", tag);
            continue;
        }
        println!(
            "\n=== [{}/6] Rendering HDR Panorama {} ({} frames) ===",
            idx + 2,
            tag,
            paths.len()
        );
        let t0 = Instant::now();
        let (tone_mapped, _lin) = stitch_hdr_panorama_native(
            &paths,
            PanoramaProjection::Cylindrical,
            warp,
            true,
            &settings,
            None,
            None,
        )
        .unwrap_or_else(|e| panic!("HDR {} failed: {}", tag, e));
        let rgb8 = DynamicImage::ImageRgb32F(tone_mapped).to_rgb8();
        let (b_pct, all_pct) = measure_border_black_pct(&rgb8);
        println!(
            "=== HDR {} done in {:.2?}: {}x{}, border_black={:.4}%, total_black={:.4}% ===",
            tag,
            t0.elapsed(),
            rgb8.width(),
            rgb8.height(),
            b_pct,
            all_pct
        );
        rgb8.save(out_dir.join(format!("hdr_pano_{}_upgraded.jpg", tag)))
            .unwrap();
        save_inspection_crops(&rgb8, out_dir, &format!("hdr_pano_{}", tag));
    }
}
