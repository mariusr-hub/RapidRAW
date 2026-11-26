use crate::app::state::AppState;
use base64::Engine as _;
use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba, RgbaImage};
use serde_json::Value;
use std::io::Cursor;
use tauri::State;

/// Generates a fast JPEG base64 geometry preview for LensCorrectionModal and TransformModal.
#[tauri::command]
pub fn preview_geometry_transform(
    params: Value,
    js_adjustments: Option<Value>,
    show_lines: Option<bool>,
    state: State<AppState>,
) -> Result<String, String> {
    let source_img: DynamicImage = {
        let orig_guard = state.original_image.lock().map_err(|e| e.to_string())?;
        let loaded = orig_guard.as_ref().ok_or("No active image loaded")?;
        loaded.image.thumbnail(1024, 1024)
    };

    let mut thumb = source_img.thumbnail(1024, 1024);
    if params
        .get("defringeEnabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let purple_amount = params
            .get("defringePurpleAmount")
            .and_then(|v| v.as_f64())
            .unwrap_or(50.0) as f32;
        let green_amount = params
            .get("defringeGreenAmount")
            .and_then(|v| v.as_f64())
            .unwrap_or(50.0) as f32;
        let edge_threshold = params
            .get("defringeEdgeThreshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.12) as f32;
        if purple_amount >= 0.5 || green_amount >= 0.5 {
            let mut rgb32f = thumb.to_rgb32f();
            crate::enhanced::denoising::apply_chromatic_defringe_edge_aware(
                &mut rgb32f,
                purple_amount,
                green_amount,
                edge_threshold,
            );
            thumb = DynamicImage::ImageRgb32F(rgb32f);
        }
    }
    let (w, h) = thumb.dimensions();
    if w == 0 || h == 0 {
        return Err("Empty image dimensions".to_string());
    }

    let k1 = params
        .get("k1")
        .or_else(|| js_adjustments.as_ref().and_then(|j| j.get("lensDistortionParams")).and_then(|p| p.get("k1")))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;
    let k2 = params
        .get("k2")
        .or_else(|| js_adjustments.as_ref().and_then(|j| j.get("lensDistortionParams")).and_then(|p| p.get("k2")))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;
    let k3 = params
        .get("k3")
        .or_else(|| js_adjustments.as_ref().and_then(|j| j.get("lensDistortionParams")).and_then(|p| p.get("k3")))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;

    let rotate_deg = params
        .get("transformRotate")
        .or_else(|| params.get("rotation"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;
    let scale = params
        .get("transformScale")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0)
        .max(0.1) as f32;
    let vertical = params
        .get("transformVertical")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32
        / 100.0;
    let horizontal = params
        .get("transformHorizontal")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32
        / 100.0;

    let rgba = thumb.to_rgba8();
    let mut out: RgbaImage = ImageBuffer::new(w, h);
    let cx = w as f32 * 0.5;
    let cy = h as f32 * 0.5;
    let half_diag = (cx * cx + cy * cy).sqrt().max(1.0);
    let angle = rotate_deg.to_radians();
    let cos_a = angle.cos();
    let sin_a = angle.sin();

    let needs_warp = k1.abs() > 1e-6
        || k2.abs() > 1e-6
        || k3.abs() > 1e-6
        || rotate_deg.abs() > 1e-4
        || (scale - 1.0).abs() > 1e-4
        || vertical.abs() > 1e-4
        || horizontal.abs() > 1e-4;

    if needs_warp {
        for y in 0..h {
            for x in 0..w {
                let mut dx = (x as f32 - cx) / scale;
                let mut dy = (y as f32 - cy) / scale;

                // Perspective keystone approximation
                let persp_div = (1.0 + vertical * (dy / cy.max(1.0)) * 0.35 + horizontal * (dx / cx.max(1.0)) * 0.35).max(0.2);
                dx /= persp_div;
                dy /= persp_div;

                // Rotation
                let rx = dx * cos_a - dy * sin_a;
                let ry = dx * sin_a + dy * cos_a;

                // Radial distortion
                let r = (rx * rx + ry * ry).sqrt() / half_diag;
                let r2 = r * r;
                let r4 = r2 * r2;
                let r6 = r4 * r2;
                let radial = 1.0 + k1 * r2 + k2 * r4 + k3 * r6;

                let sx = cx + rx * radial;
                let sy = cy + ry * radial;
                if sx >= 0.0 && sx < (w - 1) as f32 && sy >= 0.0 && sy < (h - 1) as f32 {
                    let x0 = sx.floor() as u32;
                    let y0 = sy.floor() as u32;
                    let fx = sx - x0 as f32;
                    let fy = sy - y0 as f32;
                    let p00 = rgba.get_pixel(x0, y0).0;
                    let p10 = rgba.get_pixel(x0 + 1, y0).0;
                    let p01 = rgba.get_pixel(x0, y0 + 1).0;
                    let p11 = rgba.get_pixel(x0 + 1, y0 + 1).0;
                    let mut px = [0u8; 4];
                    for c in 0..4 {
                        let top = p00[c] as f32 * (1.0 - fx) + p10[c] as f32 * fx;
                        let bot = p01[c] as f32 * (1.0 - fx) + p11[c] as f32 * fx;
                        px[c] = (top * (1.0 - fy) + bot * fy).round().clamp(0.0, 255.0) as u8;
                    }
                    out.put_pixel(x, y, Rgba(px));
                } else {
                    out.put_pixel(x, y, Rgba([18, 18, 18, 255]));
                }
            }
        }
    } else {
        out = rgba;
    }

    if show_lines.unwrap_or(false) {
        for g in 1..4 {
            let gx = (w * g) / 4;
            let gy = (h * g) / 4;
            for y in 0..h {
                out.put_pixel(gx.min(w - 1), y, Rgba([255, 255, 255, 160]));
            }
            for x in 0..w {
                out.put_pixel(x, gy.min(h - 1), Rgba([255, 255, 255, 160]));
            }
        }
    }

    let mut buf = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(out)
        .to_rgb8()
        .write_to(&mut buf, image::ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode geometry preview: {}", e))?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(buf.into_inner())
    ))
}
