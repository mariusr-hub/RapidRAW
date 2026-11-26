use crate::app::settings::load_settings;
use crate::app::state::AppState;
use crate::render::image_processing::{apply_linear_to_srgb, apply_srgb_to_linear};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb, Rgb32FImage};
use ndarray::Array4;
use ort::session::Session;
use ort::value::Tensor;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;
use std::path::Path;
use tauri::{AppHandle, State};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StarSeparationExportResult {
    pub starless_path: String,
    pub stars_path: String,
    pub engine: String,
    pub message: String,
}

#[inline]
fn clamp_coord(c: i32, max_v: usize) -> usize {
    c.clamp(0, max_v.saturating_sub(1) as i32) as usize
}

/// One pass of 2D separable à-trous B3-spline convolution (`[1/16, 1/4, 3/8, 1/4, 1/16]`)
/// with dilation step `step` across RGB interleaved plane.
fn atrous_b3_smooth_rgb(src: &[f32], w: usize, h: usize, step: usize) -> Vec<f32> {
    const K: [f32; 5] = [1.0 / 16.0, 0.25, 0.375, 0.25, 1.0 / 16.0];
    let step_i = step as i32;
    let mut tmp = vec![0.0f32; w * h * 3];

    tmp.par_chunks_exact_mut(w * 3)
        .enumerate()
        .for_each(|(y, out_row)| {
            let src_row = &src[y * w * 3..(y + 1) * w * 3];
            for x in 0..w {
                let xi = x as i32;
                let mut acc = [0.0f32; 3];
                for (ki, &kw) in K.iter().enumerate() {
                    let offset = (ki as i32 - 2) * step_i;
                    let sx = clamp_coord(xi + offset, w) * 3;
                    acc[0] += src_row[sx] * kw;
                    acc[1] += src_row[sx + 1] * kw;
                    acc[2] += src_row[sx + 2] * kw;
                }
                let dst = x * 3;
                out_row[dst] = acc[0];
                out_row[dst + 1] = acc[1];
                out_row[dst + 2] = acc[2];
            }
        });

    let mut dst = vec![0.0f32; w * h * 3];
    dst.par_chunks_exact_mut(w * 3)
        .enumerate()
        .for_each(|(y, out_row)| {
            let yi = y as i32;
            let row_indices = [
                clamp_coord(yi - 2 * step_i, h) * w * 3,
                clamp_coord(yi - step_i, h) * w * 3,
                y * w * 3,
                clamp_coord(yi + step_i, h) * w * 3,
                clamp_coord(yi + 2 * step_i, h) * w * 3,
            ];
            for x in 0..w {
                let cx = x * 3;
                let mut acc = [0.0f32; 3];
                for (ri, &row_off) in row_indices.iter().enumerate() {
                    let kw = K[ri];
                    let p = &tmp[row_off + cx..row_off + cx + 3];
                    acc[0] += p[0] * kw;
                    acc[1] += p[1] * kw;
                    acc[2] += p[2] * kw;
                }
                out_row[cx] = acc[0];
                out_row[cx + 1] = acc[1];
                out_row[cx + 2] = acc[2];
            }
        });

    dst
}

fn estimate_luma_mad_sigma(w0: &[f32], w: usize, h: usize) -> f32 {
    let total = w * h;
    let stride = (total / 4096).max(1);
    let mut abs_vals = Vec::with_capacity((total / stride) + 1);
    for i in (0..total).step_by(stride) {
        let p = i * 3;
        let lum = (0.2126 * w0[p] + 0.7152 * w0[p + 1] + 0.0722 * w0[p + 2]).abs();
        abs_vals.push(lum);
    }
    if abs_vals.is_empty() {
        return 0.005;
    }
    abs_vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    (abs_vals[abs_vals.len() / 2] * 1.4826).max(0.002)
}

/// Separates astronomical point sources from continuous nebulosity and gradients using a
/// pure-Rust multi-scale à-trous B3-spline wavelet + morphological FWHM/roundness star mask +
/// harmonic background inpainting.
///
/// Energy conservation guarantee: for every pixel and channel,
/// `starless[i] + stars_only[i] == img[i]`.
pub fn separate_stars_and_background(img: &Rgb32FImage) -> (Rgb32FImage, Rgb32FImage) {
    let (wu, hu) = img.dimensions();
    let w = wu as usize;
    let h = hu as usize;
    let raw = img.as_raw();
    if w < 8 || h < 8 {
        let zeros = ImageBuffer::from_raw(wu, hu, vec![0.0f32; w * h * 3]).unwrap_or_else(|| img.clone());
        return (img.clone(), zeros);
    }

    // 1. Multi-scale à-trous wavelet pyramid (steps 1, 2, 4, 8)
    let c1 = atrous_b3_smooth_rgb(raw, w, h, 1);
    let c2 = atrous_b3_smooth_rgb(&c1, w, h, 2);
    let c3 = atrous_b3_smooth_rgb(&c2, w, h, 4);
    let c4 = atrous_b3_smooth_rgb(&c3, w, h, 8);

    let mut detail0 = vec![0.0f32; raw.len()];
    for i in 0..raw.len() {
        detail0[i] = raw[i] - c1[i];
    }
    let sigma_noise = estimate_luma_mad_sigma(&detail0, w, h);

    // 2. Build morphological star-core + halo mask with nebula elongation guard.
    let mut star_mask = vec![0.0f32; w * h];
    star_mask
        .par_chunks_exact_mut(w)
        .enumerate()
        .for_each(|(y, mask_row)| {
            if y < 3 || y + 3 >= h {
                return;
            }
            for x in 3..(w - 3) {
                let idx = (y * w + x) * 3;
                let lum_raw = 0.2126 * raw[idx] + 0.7152 * raw[idx + 1] + 0.0722 * raw[idx + 2];
                let lum_bg = 0.2126 * c3[idx] + 0.7152 * c3[idx + 1] + 0.0722 * c3[idx + 2];
                let lum_fine = 0.2126 * c1[idx] + 0.7152 * c1[idx + 1] + 0.0722 * c1[idx + 2];
                let residual_small = lum_raw - lum_fine;
                let residual_med = lum_fine - lum_bg;
                let total_excess = lum_raw - lum_bg;

                // Compact stars have concentrated high-frequency peak energy (residual_small > 0),
                // whereas extended nebula wisps live predominantly in coarse residual scales.
                let thresh = (sigma_noise * 4.0).max(0.012);
                if total_excess > thresh && (residual_small > thresh * 0.45 || residual_med > thresh * 0.85) {
                    // Check ring contrast vs outer 3-px ring to avoid erasing bright nebula cores.
                    let mut ring_sum = 0.0f32;
                    let mut ring_min = f32::MAX;
                    let offsets = [(-3i32, 0i32), (3, 0), (0, -3), (0, 3), (-2, -2), (2, -2), (-2, 2), (2, 2)];
                    for (ox, oy) in offsets {
                        let rx = (x as i32 + ox) as usize;
                        let ry = (y as i32 + oy) as usize;
                        let ridx = (ry * w + rx) * 3;
                        let rlum = 0.2126 * raw[ridx] + 0.7152 * raw[ridx + 1] + 0.0722 * raw[ridx + 2];
                        ring_sum += rlum;
                        ring_min = ring_min.min(rlum);
                    }
                    let ring_avg = ring_sum / offsets.len() as f32;
                    let drop = lum_raw - ring_min;
                    let symmetry_drop = lum_raw - ring_avg;
                    if drop > thresh * 0.65 && symmetry_drop > drop * 0.25 {
                        mask_row[x] = ((total_excess / thresh).clamp(1.0, 4.0) / 4.0).clamp(0.4, 1.0);
                    }
                }
            }
        });

    // Dilate mask by 2px radial disk to cleanly capture star Seeing halos without clipping diffraction skirts.
    let mut dilated = vec![0.0f32; w * h];
    dilated
        .par_chunks_exact_mut(w)
        .enumerate()
        .for_each(|(y, out_row)| {
            let y0 = y.saturating_sub(2);
            let y1 = (y + 2).min(h - 1);
            for x in 0..w {
                let x0 = x.saturating_sub(2);
                let x1 = (x + 2).min(w - 1);
                let mut mx = 0.0f32;
                for ny in y0..=y1 {
                    let dy = ny as isize - y as isize;
                    for nx in x0..=x1 {
                        let dx = nx as isize - x as isize;
                        if dx * dx + dy * dy <= 5 {
                            mx = mx.max(star_mask[ny * w + nx]);
                        }
                    }
                }
                out_row[x] = mx;
            }
        });

    // Soft feather pass over 3x3 neighborhood.
    let mut feathered = vec![0.0f32; w * h];
    feathered
        .par_chunks_exact_mut(w)
        .enumerate()
        .for_each(|(y, out_row)| {
            let y0 = y.saturating_sub(1);
            let y1 = (y + 1).min(h - 1);
            for x in 0..w {
                let x0 = x.saturating_sub(1);
                let x1 = (x + 1).min(w - 1);
                let mut sum = 0.0f32;
                let mut cnt = 0.0f32;
                for ny in y0..=y1 {
                    for nx in x0..=x1 {
                        sum += dilated[ny * w + nx];
                        cnt += 1.0;
                    }
                }
                out_row[x] = (dilated[y * w + x].max(sum / cnt)).clamp(0.0, 1.0);
            }
        });

    // 3. Harmonic background inpainting initialized from the coarse scale `c4` on masked pixels.
    let mut active_indices: Vec<usize> = feathered
        .iter()
        .enumerate()
        .filter_map(|(i, &m)| if m > 0.02 { Some(i) } else { None })
        .collect();

    let mut inpaint = raw.to_vec();
    for &i in &active_indices {
        let p = i * 3;
        inpaint[p] = c4[p].min(raw[p]);
        inpaint[p + 1] = c4[p + 1].min(raw[p + 1]);
        inpaint[p + 2] = c4[p + 2].min(raw[p + 2]);
    }

    if !active_indices.is_empty() {
        let mut next_vals = vec![[0.0f32; 3]; active_indices.len()];
        for _iter in 0..14 {
            next_vals
                .par_iter_mut()
                .zip(active_indices.par_iter())
                .for_each(|(dst_rgb, &i)| {
                    let gx = i % w;
                    let gy = i / w;
                    let left = (gy * w + gx.saturating_sub(1)) * 3;
                    let right = (gy * w + (gx + 1).min(w - 1)) * 3;
                    let up = (gy.saturating_sub(1) * w + gx) * 3;
                    let down = ((gy + 1).min(h - 1) * w + gx) * 3;
                    for c in 0..3 {
                        dst_rgb[c] = 0.25
                            * (inpaint[left + c]
                                + inpaint[right + c]
                                + inpaint[up + c]
                                + inpaint[down + c]);
                    }
                });
            for (k, &i) in active_indices.iter().enumerate() {
                let p = i * 3;
                inpaint[p] = next_vals[k][0];
                inpaint[p + 1] = next_vals[k][1];
                inpaint[p + 2] = next_vals[k][2];
            }
        }
    }
    active_indices.clear();

    // 4. Construct strictly energy-conserving starless & stars-only layers.
    let mut starless_raw = vec![0.0f32; raw.len()];
    let mut stars_raw = vec![0.0f32; raw.len()];
    starless_raw
        .par_chunks_exact_mut(3)
        .zip(stars_raw.par_chunks_exact_mut(3))
        .enumerate()
        .for_each(|(i, (sl, st))| {
            let m = feathered[i];
            let p = i * 3;
            for c in 0..3 {
                let orig = raw[p + c].max(0.0);
                let bg = inpaint[p + c].clamp(0.0, orig);
                let val_starless = (orig * (1.0 - m) + bg * m).clamp(0.0, orig);
                sl[c] = val_starless;
                st[c] = (orig - val_starless).max(0.0);
            }
        });

    let starless_img = ImageBuffer::from_raw(wu, hu, starless_raw).unwrap_or_else(|| img.clone());
    let stars_img = ImageBuffer::from_raw(wu, hu, stars_raw)
        .unwrap_or_else(|| ImageBuffer::from_raw(wu, hu, vec![0.0f32; raw.len()]).unwrap());
    (starless_img, stars_img)
}

/// Optional StarNet2 tiled ONNX inference path.
/// Expects NCHW `(1, 3, 512, 512)` linear/stretched RGB input; falls back cleanly on any mismatch.
pub fn run_starnet_onnx_tiled(img: &Rgb32FImage, model_path: &Path) -> Result<(Rgb32FImage, Rgb32FImage), String> {
    if !model_path.is_file() {
        return Err("StarNet2 ONNX file does not exist".to_string());
    }

    let mut sess = Session::builder()
        .map_err(|e| format!("ORT session builder error: {}", e))?
        .with_intra_threads(4)
        .map_err(|e| format!("ORT threads error: {}", e))?
        .with_execution_providers([ort::execution_providers::CPUExecutionProvider::default().build()])
        .map_err(|e| format!("ORT EP error: {}", e))?
        .commit_from_file(model_path)
        .map_err(|e| format!("Failed to load StarNet2 ONNX: {}", e))?;

    let (wu, hu) = img.dimensions();
    let w = wu as usize;
    let h = hu as usize;
    let tile_size = 512usize;
    let overlap = 64usize;
    let step = tile_size - overlap;

    let mut acc = vec![0.0f32; w * h * 3];
    let mut wsum = vec![0.0f32; w * h];
    let src = img.as_raw();

    let xs: Vec<usize> = {
        let mut v = Vec::new();
        let mut x = 0;
        loop {
            v.push(x);
            if x + tile_size >= w {
                break;
            }
            x += step;
        }
        v
    };
    let ys: Vec<usize> = {
        let mut v = Vec::new();
        let mut y = 0;
        loop {
            v.push(y);
            if y + tile_size >= h {
                break;
            }
            y += step;
        }
        v
    };

    let ramp: Vec<f32> = (0..tile_size)
        .map(|i| {
            let d = i.min(tile_size - 1 - i) as f32;
            let t = ((d + 0.5) / overlap as f32).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        })
        .collect();

    let plane = tile_size * tile_size;
    for &y0 in &ys {
        for &x0 in &xs {
            let mut input_data = vec![0.0f32; 3 * plane];
            for dy in 0..tile_size {
                let sy = (y0 + dy).min(h - 1);
                for dx in 0..tile_size {
                    let sx = (x0 + dx).min(w - 1);
                    let p = (sy * w + sx) * 3;
                    let dst = dy * tile_size + dx;
                    input_data[dst] = src[p];
                    input_data[plane + dst] = src[p + 1];
                    input_data[2 * plane + dst] = src[p + 2];
                }
            }
            let tensor = Tensor::from_array(
                Array4::from_shape_vec((1, 3, tile_size, tile_size), input_data)
                    .map_err(|e| format!("Shape error: {}", e))?,
            )
            .map_err(|e| format!("Tensor error: {}", e))?;

            let outputs = sess
                .run(ort::inputs![tensor])
                .map_err(|e| format!("StarNet2 inference error: {}", e))?;
            let out_arr = outputs[0]
                .try_extract_array::<f32>()
                .map_err(|e| format!("StarNet2 output extract error: {}", e))?;
            let slice = out_arr
                .as_slice()
                .ok_or_else(|| "StarNet2 output non-contiguous".to_string())?;
            if slice.len() < 3 * plane {
                return Err("Unexpected StarNet2 output tensor dimensions".to_string());
            }

            for dy in 0..tile_size {
                let gy = y0 + dy;
                if gy >= h {
                    continue;
                }
                for dx in 0..tile_size {
                    let gx = x0 + dx;
                    if gx >= w {
                        continue;
                    }
                    let weight = ramp[dy] * ramp[dx];
                    let t_idx = dy * tile_size + dx;
                    let g_idx = gy * w + gx;
                    for c in 0..3 {
                        acc[g_idx * 3 + c] += slice[c * plane + t_idx] * weight;
                    }
                    wsum[g_idx] += weight;
                }
            }
        }
    }

    let mut starless_raw = vec![0.0f32; src.len()];
    let mut stars_raw = vec![0.0f32; src.len()];
    for i in 0..(w * h) {
        let wt = wsum[i].max(1e-6);
        let p = i * 3;
        for c in 0..3 {
            let orig = src[p + c].max(0.0);
            let pred = (acc[p + c] / wt).clamp(0.0, orig);
            starless_raw[p + c] = pred;
            stars_raw[p + c] = (orig - pred).max(0.0);
        }
    }

    let starless_img = ImageBuffer::from_raw(wu, hu, starless_raw)
        .ok_or_else(|| "Failed to allocate StarNet2 starless output".to_string())?;
    let stars_img = ImageBuffer::from_raw(wu, hu, stars_raw)
        .ok_or_else(|| "Failed to allocate StarNet2 stars output".to_string())?;
    Ok((starless_img, stars_img))
}

pub fn separate_stars_with_optional_onnx(
    img: &Rgb32FImage,
    starnet_model_path: Option<&str>,
) -> (Rgb32FImage, Rgb32FImage, &'static str) {
    if let Some(model_str) = starnet_model_path {
        let trimmed = model_str.trim();
        if !trimmed.is_empty() {
            match run_starnet_onnx_tiled(img, Path::new(trimmed)) {
                Ok((sl, st)) => return (sl, st, "StarNet2 ONNX"),
                Err(e) => {
                    log::warn!("StarNet2 ONNX unavailable/failed ({}), falling back to wavelet engine.", e);
                }
            }
        }
    }
    let (sl, st) = separate_stars_and_background(img);
    (sl, st, "Multi-Scale À-Trous Wavelet")
}

/// Non-destructive Star / Starless recomposition hook inside image decode & merge pipelines.
pub fn apply_astro_star_separation_if_enabled<'a>(
    image: Cow<'a, DynamicImage>,
    adjustments: &Value,
) -> Cow<'a, DynamicImage> {
    if !adjustments["astroStarlessEnabled"].as_bool().unwrap_or(false) {
        return image;
    }

    let star_mix = (adjustments["astroStarMix"].as_f64().unwrap_or(100.0) as f32 / 100.0).clamp(0.0, 1.0);
    let star_reduction = (adjustments["astroStarReduction"].as_f64().unwrap_or(0.0) as f32 / 100.0).clamp(0.0, 1.0);
    let nebula_boost = (adjustments["astroNebulaBoost"].as_f64().unwrap_or(0.0) as f32 / 100.0).clamp(0.0, 1.0);

    // No-op optimization when all mixing sliders are neutral.
    if (star_mix - 1.0).abs() < 1e-4 && star_reduction < 1e-4 && nebula_boost < 1e-4 {
        return image;
    }

    let rgb32f = image.to_rgb32f();
    let (w, h) = rgb32f.dimensions();
    let (starless, stars_only) = separate_stars_and_background(&rgb32f);

    let sl_raw = starless.as_raw();
    let st_raw = stars_only.as_raw();
    let mut out_raw = vec![0.0f32; sl_raw.len()];

    out_raw
        .par_chunks_exact_mut(3)
        .enumerate()
        .for_each(|(i, dst)| {
            let p = i * 3;
            let mut br = sl_raw[p];
            let mut bg = sl_raw[p + 1];
            let mut bb = sl_raw[p + 2];

            // Gentle S-curve & chroma lift restricted strictly to nebulosity / background layer.
            if nebula_boost > 1e-4 {
                let lum = (0.2126 * br + 0.7152 * bg + 0.0722 * bb).max(1e-6);
                let mid = (lum / (lum + 0.25)).clamp(0.0, 1.0);
                let luma_gain = 1.0 + nebula_boost * 0.45 * mid;
                let sat_gain = 1.0 + nebula_boost * 0.30;
                br = (lum * luma_gain + (br - lum) * sat_gain).max(0.0);
                bg = (lum * luma_gain + (bg - lum) * sat_gain).max(0.0);
                bb = (lum * luma_gain + (bb - lum) * sat_gain).max(0.0);
            }

            let mut sr = st_raw[p];
            let mut sg = st_raw[p + 1];
            let mut sb = st_raw[p + 2];

            if star_reduction > 1e-4 {
                let s_lum = (0.2126 * sr + 0.7152 * sg + 0.0722 * sb).max(0.0);
                if s_lum > 1e-5 {
                    let shrink = (s_lum / (s_lum + 0.06 * star_reduction)).powf(0.55 * star_reduction);
                    sr *= shrink;
                    sg *= shrink;
                    sb *= shrink;
                }
            }

            dst[0] = (br + sr * star_mix).clamp(0.0, 1.0);
            dst[1] = (bg + sg * star_mix).clamp(0.0, 1.0);
            dst[2] = (bb + sb * star_mix).clamp(0.0, 1.0);
        });

    let buf = ImageBuffer::<Rgb<f32>, _>::from_raw(w, h, out_raw).unwrap_or_else(|| rgb32f);
    Cow::Owned(DynamicImage::ImageRgb32F(buf))
}

#[tauri::command]
pub async fn export_star_separation_layers(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<StarSeparationExportResult, String> {
    let (img_clone, loaded_path) = {
        let orig_guard = state.original_image.lock().map_err(|e| e.to_string())?;
        let loaded = orig_guard
            .as_ref()
            .ok_or_else(|| "No active image loaded for star separation".to_string())?;
        (loaded.image.clone(), loaded.path.clone())
    };
    let settings = load_settings(app_handle).unwrap_or_default();
    let starnet_path = settings.starnet_model_path.clone();

    tokio::task::spawn_blocking(move || {
        let (source_str, _) = crate::library::sidecars::parse_virtual_path(&loaded_path);
        let src_path = Path::new(&source_str);
        let parent = src_path.parent().unwrap_or_else(|| Path::new("."));
        let stem = src_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("astro_export");

        let is_raw = crate::decode::formats::is_raw_file(&source_str);
        let cloned = (*img_clone).clone();
        let lin_img = if !is_raw {
            apply_srgb_to_linear(cloned)
        } else {
            cloned
        };
        let rgb32f = lin_img.to_rgb32f();
        let (starless, stars, engine) =
            separate_stars_with_optional_onnx(&rgb32f, starnet_path.as_deref());

        let starless_out = parent.join(format!("{}_Starless.tiff", stem));
        let stars_out = parent.join(format!("{}_Stars.tiff", stem));

        let starless_disp = apply_linear_to_srgb(DynamicImage::ImageRgb32F(starless)).to_rgb16();
        let stars_disp = apply_linear_to_srgb(DynamicImage::ImageRgb32F(stars)).to_rgb16();

        starless_disp
            .save_with_format(&starless_out, ImageFormat::Tiff)
            .map_err(|e| format!("Failed saving starless TIFF: {}", e))?;
        stars_disp
            .save_with_format(&stars_out, ImageFormat::Tiff)
            .map_err(|e| format!("Failed saving stars TIFF: {}", e))?;

        let sl_str = starless_out.to_string_lossy().to_string();
        let st_str = stars_out.to_string_lossy().to_string();
        Ok(StarSeparationExportResult {
            starless_path: sl_str,
            stars_path: st_str,
            engine: engine.to_string(),
            message: format!("Exported Starless & Stars 16-bit TIFF layers via {}", engine),
        })
    })
    .await
    .map_err(|e| format!("Star separation task join error: {}", e))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_starless_and_stars_sum_reconstructs_original_energy() {
        let (w, h) = (96u32, 96u32);
        let mut img = Rgb32FImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                // Smooth nebula ramp background
                let mut r = 0.04 + 0.06 * (x as f32 / w as f32);
                let mut g = 0.03 + 0.05 * (y as f32 / h as f32);
                let mut b = 0.06 + 0.08 * ((x + y) as f32 / (w + h) as f32);

                // Inject 3 Gaussian point stars across the field
                for &(sx, sy, amp) in &[(24.0f32, 24.0f32, 0.72f32), (60.0, 32.0, 0.55), (48.0, 72.0, 0.80)] {
                    let dx = x as f32 - sx;
                    let dy = y as f32 - sy;
                    let g_val = amp * (-((dx * dx + dy * dy) / (2.0 * 1.4 * 1.4))).exp();
                    r = (r + g_val).min(1.0);
                    g = (g + g_val * 0.92).min(1.0);
                    b = (b + g_val * 0.85).min(1.0);
                }
                img.put_pixel(x, y, Rgb([r, g, b]));
            }
        }

        let (starless, stars) = separate_stars_and_background(&img);

        // 1. Exact per-pixel energy conservation
        let mut max_err = 0.0f32;
        let mut total_stars_energy = 0.0f32;
        for (o, (sl, st)) in img.pixels().zip(starless.pixels().zip(stars.pixels())) {
            for c in 0..3 {
                let diff = (o[c] - (sl[c] + st[c])).abs();
                max_err = max_err.max(diff);
                total_stars_energy += st[c];
            }
        }
        assert!(
            max_err < 1e-5,
            "starless + stars must reconstruct original within float precision, max_err={}",
            max_err
        );
        assert!(
            total_stars_energy > 1.0,
            "Stars layer should isolate injected stellar energy, got {}",
            total_stars_energy
        );

        // 2. Peak star core at (48, 72) should be strongly attenuated in starless while nebula surrounds remain intact
        let orig_star = img.get_pixel(48, 72)[0];
        let starless_center = starless.get_pixel(48, 72)[0];
        assert!(
            starless_center < orig_star * 0.4,
            "Starless image should suppress star peak (orig={}, starless={})",
            orig_star,
            starless_center
        );
    }
}
