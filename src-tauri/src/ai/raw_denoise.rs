use crate::decode::raw_processing::{demosaic_rcd_bayer, extract_linear_bayer_cfa};
use crate::render::geometry::apply_orientation;
use image::{DynamicImage, Rgb32FImage};
use ndarray::Array4;
use ort::session::Session;
use ort::value::Tensor;
use rayon::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone)]
pub struct RawDenoiseOptions {
    pub strength: f32,
    pub read_noise: f32,
    pub shot_noise: f32,
    pub model_path: Option<String>,
}

impl Default for RawDenoiseOptions {
    fn default() -> Self {
        Self {
            strength: 0.45,
            read_noise: 0.0020,
            shot_noise: 0.00050,
            model_path: None,
        }
    }
}

/// Packs an even-dimensioned single-channel Bayer CFA mosaic into 4 sub-sampled planes
/// preserving the exact 2x2 CFA phase order `[top-left, top-right, bottom-left, bottom-right]`.
pub fn pack_bayer_4ch(cfa: &[f32], width: usize, height: usize) -> (Vec<[f32; 4]>, usize, usize) {
    let half_w = width / 2;
    let half_h = height / 2;
    if half_w == 0 || half_h == 0 || cfa.len() < width * height {
        return (Vec::new(), 0, 0);
    }
    let mut packed = vec![[0.0f32; 4]; half_w * half_h];
    packed
        .par_chunks_exact_mut(half_w)
        .enumerate()
        .for_each(|(py, row)| {
            let y0 = py * 2;
            let r0 = &cfa[y0 * width..(y0 + 1) * width];
            let r1 = &cfa[(y0 + 1) * width..(y0 + 2) * width];
            for (px, dst) in row.iter_mut().enumerate() {
                let x0 = px * 2;
                *dst = [r0[x0], r0[x0 + 1], r1[x0], r1[x0 + 1]];
            }
        });
    (packed, half_w, half_h)
}

/// Unpacks 4-channel sub-sampled Bayer planes back into the interleaved 2x2 CFA mosaic.
pub fn unpack_bayer_4ch(packed: &[[f32; 4]], half_w: usize, half_h: usize) -> Vec<f32> {
    let width = half_w * 2;
    let height = half_h * 2;
    if width == 0 || height == 0 || packed.len() < half_w * half_h {
        return Vec::new();
    }
    let mut cfa = vec![0.0f32; width * height];
    cfa.par_chunks_exact_mut(width * 2)
        .enumerate()
        .for_each(|(py, rows)| {
            let src = &packed[py * half_w..(py + 1) * half_w];
            let (r0, r1) = rows.split_at_mut(width);
            for (px, p) in src.iter().enumerate() {
                let x0 = px * 2;
                r0[x0] = p[0];
                r0[x0 + 1] = p[1];
                r1[x0] = p[2];
                r1[x0 + 1] = p[3];
            }
        });
    cfa
}

/// Forward Generalized Anscombe Transform (GAT) variance-stabilizing Poisson-Gaussian noise
/// (`Var(z) = alpha * E[z] + sigma2`) to unit-variance homoscedastic Gaussian noise.
#[inline]
pub fn gat_forward(z: f32, alpha: f32, sigma2: f32) -> f32 {
    let s2 = sigma2.max(0.0);
    if !alpha.is_finite() || alpha <= 1e-6 {
        let sigma = s2.sqrt().max(1e-6);
        return z.max(0.0) / sigma;
    }
    let x = z.max(0.0) / alpha;
    let var_n = s2 / (alpha * alpha);
    2.0 * (x + 0.375 + var_n).max(0.0).sqrt()
}

/// Closed-form Makitalo–Foi algebraic/unbiased inverse Generalized Anscombe Transform.
#[inline]
pub fn gat_inverse(y: f32, alpha: f32, sigma2: f32) -> f32 {
    let s2 = sigma2.max(0.0);
    let yc = y.max(0.0);
    if !alpha.is_finite() || alpha <= 1e-6 {
        let sigma = s2.sqrt().max(1e-6);
        return (yc * sigma).max(0.0);
    }
    let var_n = s2 / (alpha * alpha);
    let base_sq = (yc * 0.5).powi(2) - 0.375 - var_n;
    let x_est = if yc > 2.0 {
        let inv = 1.0 / yc;
        let inv2 = inv * inv;
        let inv3 = inv2 * inv;
        let s32 = 1.5_f32.sqrt();
        base_sq + 0.25 * s32 * inv - 1.375 * inv2 + 0.625 * s32 * inv3 - 0.125
    } else {
        base_sq
    };
    (x_est * alpha).max(0.0)
}

#[inline]
fn atrous_b3_step(src: &[f32], w: usize, h: usize, step: usize) -> Vec<f32> {
    const K: [f32; 5] = [1.0 / 16.0, 0.25, 0.375, 0.25, 1.0 / 16.0];
    let mut temp = vec![0.0f32; w * h];
    let s = step as isize;
    temp.par_chunks_exact_mut(w).enumerate().for_each(|(y, out)| {
        let row = &src[y * w..(y + 1) * w];
        for x in 0..w {
            let mut sum = 0.0f32;
            for (i, &wt) in K.iter().enumerate() {
                let dx = (i as isize - 2) * s;
                let sx = (x as isize + dx).clamp(0, w as isize - 1) as usize;
                sum += row[sx] * wt;
            }
            out[x] = sum;
        }
    });

    let mut dst = vec![0.0f32; w * h];
    dst.par_chunks_exact_mut(w).enumerate().for_each(|(y, out)| {
        for x in 0..w {
            let mut sum = 0.0f32;
            for (i, &wt) in K.iter().enumerate() {
                let dy = (i as isize - 2) * s;
                let sy = (y as isize + dy).clamp(0, h as isize - 1) as usize;
                sum += temp[sy * w + x] * wt;
            }
            out[x] = sum;
        }
    });
    dst
}

fn estimate_mad_sigma(detail: &[f32]) -> f32 {
    if detail.is_empty() {
        return 1.0;
    }
    let step = (detail.len() / 65_536).max(1);
    let mut abs_vals: Vec<f32> = detail
        .iter()
        .step_by(step)
        .map(|v| v.abs())
        .filter(|v| v.is_finite())
        .collect();
    if abs_vals.is_empty() {
        return 1.0;
    }
    let mid = abs_vals.len() / 2;
    let (_, med, _) = abs_vals.select_nth_unstable_by(mid, |a, b| a.total_cmp(b));
    (*med / 0.67448975).max(0.1)
}

fn wiener_shrink_level(detail: &mut [f32], w: usize, h: usize, sigma: f32) {
    let sigma2 = sigma * sigma;
    if sigma2 <= 1e-10 {
        return;
    }
    let orig = detail.to_vec();
    detail
        .par_chunks_exact_mut(w)
        .enumerate()
        .for_each(|(y, row)| {
            let y0 = y.saturating_sub(1);
            let y1 = (y + 1).min(h - 1);
            for (x, dst) in row.iter_mut().enumerate() {
                let x0 = x.saturating_sub(1);
                let x1 = (x + 1).min(w - 1);
                let mut sum_sq = 0.0f32;
                let mut count = 0.0f32;
                for yy in y0..=y1 {
                    let off = yy * w;
                    for xx in x0..=x1 {
                        let v = orig[off + xx];
                        sum_sq += v * v;
                        count += 1.0;
                    }
                }
                let local_var = sum_sq / count.max(1.0);
                let sig_var = (local_var - sigma2).max(0.0);
                let gain = sig_var / (sig_var + sigma2 + 1e-8);
                *dst *= gain;
            }
        });
}

fn denoise_4ch_wavelet_wiener(
    planes: &mut [Vec<f32>; 4],
    w: usize,
    h: usize,
    strength: f32,
) {
    let s = strength.clamp(0.0, 1.5);
    if s <= 1e-4 {
        return;
    }
    planes.par_iter_mut().for_each(|plane| {
        let a1 = atrous_b3_step(plane, w, h, 1);
        let mut w1: Vec<f32> = plane.par_iter().zip(&a1).map(|(p, l)| p - l).collect();
        let a2 = atrous_b3_step(&a1, w, h, 2);
        let mut w2: Vec<f32> = a1.par_iter().zip(&a2).map(|(p, l)| p - l).collect();
        let a3 = atrous_b3_step(&a2, w, h, 4);
        let mut w3: Vec<f32> = a2.par_iter().zip(&a3).map(|(p, l)| p - l).collect();

        // In GAT normalized space white noise has unit variance; blend MAD with theoretical prior 1.0
        let emp_sigma = estimate_mad_sigma(&w1);
        let sigma1 = (0.5 * emp_sigma + 0.5).clamp(0.35, 3.0) * (0.35 + s * 1.35);
        wiener_shrink_level(&mut w1, w, h, sigma1);
        wiener_shrink_level(&mut w2, w, h, sigma1 * 0.62);
        wiener_shrink_level(&mut w3, w, h, sigma1 * 0.34);

        plane
            .par_iter_mut()
            .enumerate()
            .for_each(|(i, dst)| *dst = a3[i] + w1[i] + w2[i] + w3[i]);
    });
}

fn try_onnx_bayer_4ch(
    model_path: &str,
    planes: &[Vec<f32>; 4],
    w: usize,
    h: usize,
    noise_level: f32,
) -> Result<[Vec<f32>; 4], String> {
    let mut session = Session::builder()
        .map_err(|e| e.to_string())?
        .commit_from_file(model_path)
        .map_err(|e| format!("Failed loading RAW denoise ONNX '{}': {}", model_path, e))?;

    let input_channels = session
        .inputs
        .first()
        .and_then(|input| input.input_type.tensor_shape())
        .and_then(|shape| shape.get(1).copied())
        .filter(|&c| c == 4 || c == 5)
        .unwrap_or(4) as usize;

    // Normalize stabilized GAT signal into [0, 1] for CNN tile evaluation and invert back cleanly
    let max_val = planes
        .iter()
        .flat_map(|p| p.iter())
        .copied()
        .fold(1.0f32, f32::max)
        .max(1.0);
    let inv_max = 1.0 / max_val;

    const TILE: usize = 512;
    const OVERLAP: usize = 32;
    let step = TILE - OVERLAP * 2;
    let mut accum = [
        vec![0.0f32; w * h],
        vec![0.0f32; w * h],
        vec![0.0f32; w * h],
        vec![0.0f32; w * h],
    ];
    let mut weight_sum = vec![0.0f32; w * h];

    let mut y0 = 0usize;
    while y0 < h {
        let th = TILE.min(h - y0);
        let pad_h = (th + 7) & !7;
        let mut x0 = 0usize;
        while x0 < w {
            let tw = TILE.min(w - x0);
            let pad_w = (tw + 7) & !7;
            let mut input = Array4::<f32>::zeros((1, input_channels, pad_h, pad_w));
            for c in 0..4 {
                for dy in 0..pad_h {
                    let sy = y0 + dy.min(th - 1);
                    for dx in 0..pad_w {
                        let sx = x0 + dx.min(tw - 1);
                        input[[0, c, dy, dx]] = (planes[c][sy * w + sx] * inv_max).clamp(0.0, 1.0);
                    }
                }
            }
            if input_channels == 5 {
                let nl = (noise_level * inv_max).clamp(0.0, 1.0);
                for dy in 0..pad_h {
                    for dx in 0..pad_w {
                        input[[0, 4, dy, dx]] = nl;
                    }
                }
            }

            let tensor = Tensor::from_array(input).map_err(|e| e.to_string())?;
            let outputs = session.run(ort::inputs![tensor]).map_err(|e| e.to_string())?;
            let (_, out_slice) = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| e.to_string())?;
            let plane_stride = pad_h * pad_w;
            if out_slice.len() < 4 * plane_stride {
                return Err("Unexpected ONNX output tensor size for 4-ch RAW denoiser".into());
            }

            for dy in 0..th {
                let wy = if y0 > 0 && dy < OVERLAP {
                    (dy as f32 + 0.5) / OVERLAP as f32
                } else if y0 + th < h && dy >= th - OVERLAP {
                    ((th - dy) as f32 - 0.5).max(0.5) / OVERLAP as f32
                } else {
                    1.0
                };
                for dx in 0..tw {
                    let wx = if x0 > 0 && dx < OVERLAP {
                        (dx as f32 + 0.5) / OVERLAP as f32
                    } else if x0 + tw < w && dx >= tw - OVERLAP {
                        ((tw - dx) as f32 - 0.5).max(0.5) / OVERLAP as f32
                    } else {
                        1.0
                    };
                    let wt = wx * wy;
                    let idx = (y0 + dy) * w + (x0 + dx);
                    weight_sum[idx] += wt;
                    for c in 0..4 {
                        let val = out_slice[c * plane_stride + dy * pad_w + dx] * max_val;
                        accum[c][idx] += val * wt;
                    }
                }
            }

            if x0 + tw >= w {
                break;
            }
            x0 += step;
        }
        if y0 + th >= h {
            break;
        }
        y0 += step;
    }

    for c in 0..4 {
        accum[c]
            .par_iter_mut()
            .zip(weight_sum.par_iter())
            .for_each(|(v, &w_tot)| {
                if w_tot > 1e-6 {
                    *v /= w_tot;
                }
            });
    }
    Ok(accum)
}

/// Denoises single-channel linear Bayer CFA data in 4-channel GAT variance-stabilized space.
pub fn denoise_bayer_4ch_gat(
    cfa: &[f32],
    width: usize,
    height: usize,
    opts: &RawDenoiseOptions,
) -> Vec<f32> {
    let (packed, hw, hh) = pack_bayer_4ch(cfa, width, height);
    if hw == 0 || hh == 0 {
        return cfa.to_vec();
    }

    let alpha = opts.shot_noise.max(0.0);
    let sigma2 = opts.read_noise.max(1e-5).powi(2);

    let mut planes: [Vec<f32>; 4] = std::array::from_fn(|c| {
        packed
            .par_iter()
            .map(|p| gat_forward(p[c], alpha, sigma2))
            .collect()
    });

    let onnx_result = opts
        .model_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty() && Path::new(p).exists())
        .and_then(|path| {
            let noise_lvl = (0.5 + opts.strength * 1.5).clamp(0.1, 3.0);
            match try_onnx_bayer_4ch(path, &planes, hw, hh, noise_lvl) {
                Ok(out) => Some(out),
                Err(err) => {
                    log::warn!("Pluggable 4-ch RAW ONNX denoise failed ({}); falling back to Bayer GAT Wavelet-Wiener.", err);
                    None
                }
            }
        });

    if let Some(denoised_planes) = onnx_result {
        let blend = opts.strength.clamp(0.0, 1.0);
        for c in 0..4 {
            planes[c]
                .par_iter_mut()
                .zip(denoised_planes[c].par_iter())
                .for_each(|(orig, &dn)| *orig = *orig + (dn - *orig) * blend);
        }
    } else {
        denoise_4ch_wavelet_wiener(&mut planes, hw, hh, opts.strength);
    }

    let out_packed: Vec<[f32; 4]> = (0..hw * hh)
        .into_par_iter()
        .map(|i| {
            [
                gat_inverse(planes[0][i], alpha, sigma2),
                gat_inverse(planes[1][i], alpha, sigma2),
                gat_inverse(planes[2][i], alpha, sigma2),
                gat_inverse(planes[3][i], alpha, sigma2),
            ]
        })
        .collect();

    unpack_bayer_4ch(&out_packed, hw, hh)
}

/// Runs full pre-demosaic Bayer 4-ch GAT denoise directly from RAW bytes, followed by
/// RapidRAW's RCD demosaic, white-balance, camera-to-working color transform, and orientation.
pub fn denoise_raw_bytes_pre_demosaic(
    file_bytes: &[u8],
    opts: &RawDenoiseOptions,
    cancel: Option<&AtomicBool>,
) -> Result<Rgb32FImage, String> {
    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        return Err("RAW pre-demosaic denoise cancelled".to_string());
    }

    let frame = extract_linear_bayer_cfa(file_bytes)
        .map_err(|e| format!("Pre-demosaic RAW CFA extraction failed: {}", e))?;

    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        return Err("RAW pre-demosaic denoise cancelled".to_string());
    }

    let denoised_cfa = denoise_bayer_4ch_gat(&frame.data, frame.width, frame.height, opts);

    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        return Err("RAW pre-demosaic denoise cancelled".to_string());
    }

    let cfa_flat = [
        frame.cfa[0][0],
        frame.cfa[0][1],
        frame.cfa[1][0],
        frame.cfa[1][1],
    ];
    let demosaiced = demosaic_rcd_bayer(&denoised_cfa, frame.width, frame.height, cfa_flat);

    let m = frame.cam_to_working;
    let wb = frame.wb;
    let mut rgb_raw = vec![0.0f32; frame.width * frame.height * 3];
    rgb_raw
        .par_chunks_exact_mut(3)
        .zip(demosaiced.par_iter())
        .for_each(|(dst, cam)| {
            let cr = cam[0] * wb[0];
            let cg = cam[1] * wb[1];
            let cb = cam[2] * wb[2];
            for row in 0..3 {
                let v = m[row][0] * cr + m[row][1] * cg + m[row][2] * cb;
                dst[row] = if v.is_finite() { v.max(0.0) } else { 0.0 };
            }
        });

    let img = Rgb32FImage::from_raw(frame.width as u32, frame.height as u32, rgb_raw)
        .ok_or_else(|| "Failed to allocate pre-demosaic denoised RGB buffer".to_string())?;
    Ok(apply_orientation(DynamicImage::ImageRgb32F(img), frame.orientation).into_rgb32f())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gat_forward_inverse_roundtrip_accuracy() {
        let cases = [
            (0.0005f32, 0.002f32 * 0.002f32),
            (0.002f32, 0.0035f32 * 0.0035f32),
            (0.0f32, 0.002f32 * 0.002f32), // pure Gaussian edge case
        ];
        for &(alpha, sigma2) in &cases {
            for step in 1..=90 {
                let z = step as f32 * 0.01;
                let y = gat_forward(z, alpha, sigma2);
                let z_rec = gat_inverse(y, alpha, sigma2);
                assert!(
                    (z_rec - z).abs() < 1e-3,
                    "GAT roundtrip error too high for alpha={}, sigma2={}, z={}: y={}, rec={}",
                    alpha,
                    sigma2,
                    z,
                    y,
                    z_rec
                );
            }
        }
    }

    #[test]
    fn test_bayer_4ch_pack_unpack_preserves_exact_cfa_layout() {
        let (w, h) = (16usize, 12usize);
        let cfa: Vec<f32> = (0..(w * h))
            .map(|i| {
                let y = i / w;
                let x = i % w;
                ((y * 100 + x) as f32) * 0.0001
            })
            .collect();
        let (packed, hw, hh) = pack_bayer_4ch(&cfa, w, h);
        assert_eq!((hw, hh), (8, 6));
        assert_eq!(packed.len(), 48);

        let unpacked = unpack_bayer_4ch(&packed, hw, hh);
        assert_eq!(unpacked.len(), w * h);
        for i in 0..cfa.len() {
            assert!(
                (unpacked[i] - cfa[i]).abs() < 1e-7,
                "CFA layout mismatch at index {}: expected {}, got {}",
                i,
                cfa[i],
                unpacked[i]
            );
        }
    }
}
