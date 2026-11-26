use crate::ai::models::gpu_inference;
use anyhow::Result;
use image::{DynamicImage, Rgb32FImage};
use ndarray::Array4;
use ort::session::Session;
use ort::value::Tensor;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct TileProgress {
    pub progress: f32,
    pub tile_index: usize,
    pub total_tiles: usize,
    pub eta_sec: Option<u32>,
    pub stage: &'static str,
    pub provider: &'static str,
}

#[derive(Clone, Copy)]
struct TileParams {
    cs: usize,
    margin: usize,
    overlap: usize,
}

const DENOISE_TILE: TileParams = TileParams {
    cs: 504,
    margin: 40,
    overlap: 56,
};

pub const UPSCALE_TILE_SIZE: usize = 544;

const UPSCALE_TILE: TileParams = TileParams {
    cs: UPSCALE_TILE_SIZE,
    margin: 16,
    overlap: 16,
};

#[inline]
fn mirror_coord(c: i32, size: i32) -> i32 {
    if c < 0 {
        (-c).min(size - 1)
    } else if c >= size {
        (2 * size - 1 - c).max(0)
    } else {
        c
    }
}

fn extract_tile_mirror(img: &Rgb32FImage, x0: i32, y0: i32, cs: usize) -> Array4<f32> {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let src = img.as_raw();
    let plane = cs * cs;
    let mut data = vec![0.0f32; 3 * plane];
    let (r, rest) = data.split_at_mut(plane);
    let (g, b) = rest.split_at_mut(plane);
    r.par_chunks_exact_mut(cs)
        .zip(g.par_chunks_exact_mut(cs))
        .zip(b.par_chunks_exact_mut(cs))
        .enumerate()
        .for_each(|(dy, ((r, g), b))| {
            let sy = mirror_coord(y0 + dy as i32, h) as usize;
            let row = &src[sy * w as usize * 3..(sy + 1) * w as usize * 3];
            for dx in 0..cs {
                let sx = mirror_coord(x0 + dx as i32, w) as usize * 3;
                r[dx] = row[sx];
                g[dx] = row[sx + 1];
                b[dx] = row[sx + 2];
            }
        });
    Array4::from_shape_vec((1, 3, cs, cs), data).expect("tile buffer matches its shape")
}

fn tile_origins(len: usize, params: TileParams) -> Vec<i32> {
    let valid = params.cs - 2 * params.margin;
    let step = (valid - params.overlap).max(1);
    let mut origins = Vec::new();
    let mut start = 0;
    loop {
        origins.push(start as i32 - params.margin as i32);
        if start + valid >= len {
            break;
        }
        start += step;
    }
    origins
}

fn tile_blend_weights(params: TileParams) -> Vec<f32> {
    let valid = params.cs - 2 * params.margin;
    let ramp = |d: usize| ((d as f32 + 0.5) / params.overlap.max(1) as f32).min(1.0);
    (0..valid)
        .map(|i| {
            let t = ramp(i).min(ramp(valid - 1 - i));
            t * t * (3.0 - 2.0 * t)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn accumulate_tile(
    tile: &Array4<f32>,
    x0: i32,
    y0: i32,
    out_w: usize,
    out_h: usize,
    params: TileParams,
    scale: u32,
    weights: &[f32],
    accumulator: &mut [f32],
    weight_sum: &mut [f32],
) -> Result<()> {
    let s = scale as usize;
    let valid = params.cs - 2 * params.margin;
    let (tile_h, tile_w) = (tile.dim().2, tile.dim().3);
    if tile_h < params.cs * s || tile_w < params.cs * s {
        return Err(anyhow::anyhow!(
            "Unexpected output tile size {:?} for a {} px input",
            tile.dim(),
            params.cs
        ));
    }
    let data = tile
        .as_slice()
        .ok_or_else(|| anyhow::anyhow!("Output tile is not contiguous"))?;
    let plane = tile_h * tile_w;

    let (ox, oy, om) = (x0 * scale as i32, y0 * scale as i32, params.margin * s);
    let top = oy + om as i32;
    let first = top.max(0) as usize;
    let last = (top + (valid * s) as i32).clamp(0, out_h as i32) as usize;
    if first >= last {
        return Ok(());
    }
    accumulator[first * out_w * 3..last * out_w * 3]
        .par_chunks_exact_mut(out_w * 3)
        .zip(weight_sum[first * out_w..last * out_w].par_chunks_exact_mut(out_w))
        .enumerate()
        .for_each(|(row, (acc, wsum))| {
            let ty = ((first + row) as i32 - top) as usize;
            let line = (om + ty) * tile_w + om;
            for tx in 0..valid * s {
                let gx = ox + (om + tx) as i32;
                if gx < 0 || gx >= out_w as i32 {
                    continue;
                }
                let w = weights[ty] * weights[tx];
                let gx = gx as usize;
                for c in 0..3 {
                    let v = data[c * plane + line + tx].clamp(0.0, 1.0);
                    acc[gx * 3 + c] += v * w;
                }
                wsum[gx] += w;
            }
        });
    Ok(())
}

fn run_single_tile_on_session(
    sess: &mut Session,
    crop: Array4<f32>,
) -> Result<Array4<f32>> {
    let t_input = Tensor::from_array(crop)?;
    let outputs = gpu_inference(sess.run(ort::inputs![t_input]))?;
    outputs[0]
        .try_extract_array::<f32>()?
        .as_standard_layout()
        .into_owned()
        .into_dimensionality::<ndarray::Ix4>()
        .map_err(|e| anyhow::anyhow!("Unexpected output shape: {}", e))
}

fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|c| c.load(Ordering::Relaxed))
}

#[allow(clippy::too_many_arguments)]
fn run_tiled_with_control(
    img: &Rgb32FImage,
    session: &Mutex<Session>,
    gpu_arc_session: Option<&Arc<Mutex<Session>>>,
    params: TileParams,
    scale: u32,
    cancel: Option<&AtomicBool>,
    is_gpu: bool,
    on_progress: &dyn Fn(TileProgress),
) -> Result<Rgb32FImage> {
    let s = scale as usize;
    let (width, height) = (img.width() as usize, img.height() as usize);
    let (out_w, out_h) = (width * s, height * s);
    let weights = tile_blend_weights(TileParams {
        cs: params.cs * s,
        margin: params.margin * s,
        overlap: params.overlap * s,
    });
    let xs = tile_origins(width, params);
    let ys = tile_origins(height, params);
    let total = xs.len() * ys.len();

    let mut accumulator = vec![0.0f32; out_w * out_h * 3];
    let mut weight_sum = vec![0.0f32; out_w * out_h];
    let origins: Vec<(i32, i32)> = ys
        .iter()
        .flat_map(|&y| xs.iter().map(move |&x| (y, x)))
        .collect();

    let provider = if is_gpu { "DirectML GPU" } else { "CPU" };
    let run_start = Instant::now();
    let mut start_idx = 0usize;

    if is_gpu && let (Some(arc_sess), Some(&(y0, x0))) = (gpu_arc_session, origins.first()) {
        if is_cancelled(cancel) {
            return Err(anyhow::anyhow!("AI denoise cancelled"));
        }
        on_progress(TileProgress {
            progress: 0.0,
            tile_index: 0,
            total_tiles: total,
            eta_sec: None,
            stage: "Running first DirectML tile...",
            provider,
        });
        let crop_0 = extract_tile_mirror(img, x0, y0, params.cs);
        let sess_clone = Arc::clone(arc_sess);
        let (tx, rx) = std::sync::mpsc::channel();
        let tile0_start = Instant::now();
        std::thread::spawn(move || {
            let res = {
                let mut sess = sess_clone.lock().unwrap();
                run_single_tile_on_session(&mut sess, crop_0)
            };
            let _ = tx.send(res);
        });
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(tile0)) => {
                if tile0_start.elapsed() > Duration::from_secs(10) {
                    crate::ai::models::mark_gpu_failed();
                    return Err(anyhow::anyhow!("DIRECTML_FIRST_TILE_SLOW"));
                }
                accumulate_tile(
                    &tile0,
                    x0,
                    y0,
                    out_w,
                    out_h,
                    params,
                    scale,
                    &weights,
                    &mut accumulator,
                    &mut weight_sum,
                )?;
                start_idx = 1;
                let elapsed = run_start.elapsed().as_secs_f32();
                let eta_sec = total
                    .saturating_sub(1)
                    .checked_mul(1)
                    .map(|rem| (elapsed * rem as f32).ceil() as u32);
                on_progress(TileProgress {
                    progress: 1.0 / total.max(1) as f32,
                    tile_index: 1,
                    total_tiles: total,
                    eta_sec,
                    stage: "Denoising tiles...",
                    provider,
                });
            }
            Ok(Err(e)) => {
                crate::ai::models::mark_gpu_failed();
                return Err(anyhow::anyhow!("DIRECTML_FIRST_TILE_SLOW: {}", e));
            }
            Err(_) => {
                crate::ai::models::mark_gpu_failed();
                return Err(anyhow::anyhow!("DIRECTML_FIRST_TILE_SLOW"));
            }
        }
    }

    if start_idx < origins.len() {
        let remaining = &origins[start_idx..];
        std::thread::scope(|scope| -> Result<()> {
            let (sender, receiver) = std::sync::mpsc::sync_channel::<Result<Array4<f32>>>(1);
            scope.spawn(move || {
                for &(y0, x0) in remaining {
                    if is_cancelled(cancel) {
                        let _ = sender.send(Err(anyhow::anyhow!("AI denoise cancelled")));
                        break;
                    }
                    let tile = (|| -> Result<Array4<f32>> {
                        let crop = extract_tile_mirror(img, x0, y0, params.cs);
                        let mut sess = session.lock().unwrap();
                        run_single_tile_on_session(&mut sess, crop)
                    })();
                    let failed = tile.is_err();
                    if sender.send(tile).is_err() || failed {
                        break;
                    }
                }
            });

            for (offset, (&(y0, x0), tile)) in remaining.iter().zip(receiver).enumerate() {
                if is_cancelled(cancel) {
                    return Err(anyhow::anyhow!("AI denoise cancelled"));
                }
                let completed = start_idx + offset;
                let eta_sec = if completed > 0 {
                    let per_tile = run_start.elapsed().as_secs_f32() / completed as f32;
                    Some(((total.saturating_sub(completed)) as f32 * per_tile).ceil() as u32)
                } else {
                    None
                };
                on_progress(TileProgress {
                    progress: completed as f32 / total.max(1) as f32,
                    tile_index: completed + 1,
                    total_tiles: total,
                    eta_sec,
                    stage: "Denoising tiles...",
                    provider,
                });
                let tile = tile?;
                accumulate_tile(
                    &tile,
                    x0,
                    y0,
                    out_w,
                    out_h,
                    params,
                    scale,
                    &weights,
                    &mut accumulator,
                    &mut weight_sum,
                )?;
            }
            Ok(())
        })?;
    }

    accumulator
        .par_chunks_exact_mut(3)
        .zip(weight_sum.par_iter())
        .for_each(|(px, &w)| {
            let inv = if w > 0.0 { 1.0 / w } else { 0.0 };
            for v in px {
                *v = (*v * inv).clamp(0.0, 1.0);
            }
        });
    Rgb32FImage::from_raw(out_w as u32, out_h as u32, accumulator)
        .ok_or_else(|| anyhow::anyhow!("Tiled output does not match its dimensions"))
}

fn run_tiled(
    img: &Rgb32FImage,
    session: &Mutex<Session>,
    params: TileParams,
    scale: u32,
    on_progress: &dyn Fn(f32),
) -> Result<Rgb32FImage> {
    run_tiled_with_control(img, session, None, params, scale, None, false, &|tp| {
        on_progress(tp.progress)
    })
}

pub fn run_ai_denoise(
    rgb_img: &Rgb32FImage,
    session: &Mutex<Session>,
    on_progress: &dyn Fn(f32),
) -> Result<DynamicImage> {
    let out = run_tiled(rgb_img, session, DENOISE_TILE, 1, on_progress)?;
    Ok(DynamicImage::ImageRgb32F(out))
}

pub fn run_ai_denoise_with_control(
    rgb_img: &Rgb32FImage,
    session: &Arc<Mutex<Session>>,
    cancel: Option<&AtomicBool>,
    is_gpu: bool,
    on_progress: &dyn Fn(TileProgress),
) -> Result<DynamicImage> {
    let out = run_tiled_with_control(
        rgb_img,
        session.as_ref(),
        Some(session),
        DENOISE_TILE,
        1,
        cancel,
        is_gpu,
        on_progress,
    )?;
    Ok(DynamicImage::ImageRgb32F(out))
}

pub fn run_ai_upscale(
    rgb_img: &Rgb32FImage,
    session: &Mutex<Session>,
    scale: u32,
    on_progress: &dyn Fn(f32),
) -> Result<Rgb32FImage> {
    run_tiled(rgb_img, session, UPSCALE_TILE, scale, on_progress)
}

pub fn run_ai_upscale_with_cancel(
    rgb_img: &Rgb32FImage,
    session: &Mutex<Session>,
    scale: u32,
    cancel: Option<&AtomicBool>,
    on_progress: &dyn Fn(f32),
) -> Result<Rgb32FImage> {
    run_tiled_with_control(img_ref(rgb_img), session, None, UPSCALE_TILE, scale, cancel, false, &|tp| {
        on_progress(tp.progress)
    })
}

#[inline]
fn img_ref(img: &Rgb32FImage) -> &Rgb32FImage {
    img
}
