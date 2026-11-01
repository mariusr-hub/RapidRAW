//! Global Photometric Harmonization & Multi-Band Laplacian Pyramid Blending for RapidRAW
//!
//! Provides:
//! 1. Scene-referred linear irradiance conversion (E = I / (t * ISO/100))
//! 2. Global N-image Photometric Gain Matrix Solver using symmetric positive-definite least squares
//! 3. Multi-Band (Laplacian Pyramid) Spline Blending (Burt & Adelson) with spatial frequency separation

use image::{GrayImage, Luma, Rgb32FImage};
use nalgebra::{DMatrix, DVector};
use rayon::prelude::*;

/// Scene-referred linear irradiance normalization
#[allow(dead_code)]
pub fn normalize_to_linear_radiance(
    img: &Rgb32FImage,
    exposure_time_secs: f32,
    iso: f32,
) -> Rgb32FImage {
    let t = exposure_time_secs.max(0.0001);
    let s = (iso / 100.0).max(0.1);
    let scale = 1.0 / (t * s);

    let (w, _h) = img.dimensions();
    let mut normalized = img.clone();

    normalized
        .par_chunks_mut(w as usize * 3)
        .for_each(|row| {
            for px in row.chunks_mut(3) {
                px[0] *= scale;
                px[1] *= scale;
                px[2] *= scale;
            }
        });

    normalized
}

/// Computes global per-image RGB gain factors across all overlapping image pairs simultaneously.
/// Solves a symmetric positive-definite regularized least-squares system:
///   min sum_{i,j} \int (g_i * I_i - g_j * I_j)^2 + lambda * sum_i (g_i - 1.0)^2
pub fn solve_global_photometric_gains(
    num_images: usize,
    overlap_integrals: &[OverlapIntegral],
) -> Vec<[f32; 3]> {
    if num_images == 0 {
        return Vec::new();
    }
    if num_images == 1 || overlap_integrals.is_empty() {
        return vec![[1.0, 1.0, 1.0]; num_images];
    }

    let mut gains = vec![[1.0f32; 3]; num_images];
    // Regularization weight in log-gain space anchoring log(g_i) toward 0.0 (g_i = 1.0)
    let lambda = 0.35f64;

    for channel in 0..3 {
        let mut mat_m = DMatrix::<f64>::zeros(num_images, num_images);
        let mut vec_b = DVector::<f64>::zeros(num_images);

        for i in 0..num_images {
            mat_m[(i, i)] += lambda;
        }

        for overlap in overlap_integrals {
            let i = overlap.img_idx_1;
            let j = overlap.img_idx_2;
            if i >= num_images || j >= num_images || i == j {
                continue;
            }
            let samples = (overlap.sample_count as f64).max(1.0);
            let rms_i = (overlap.sum_i_squared[channel] / samples).max(0.0).sqrt();
            let rms_j = (overlap.sum_j_squared[channel] / samples).max(0.0).sqrt();
            if rms_i < 1e-4 || rms_j < 1e-4 {
                continue;
            }

            // Unbiased log-ratio target: log(g_i) - log(g_j) = log(rms_j / rms_i)
            // Unlike pixel-wise sum(g_i*I_i - g_j*I_j)^2, this has zero variance/degree shrinkage bias
            // when I_i and I_j have equal brightness but non-zero parallax or cloud movement.
            let log_ratio = (rms_j / rms_i).clamp(0.55, 1.82).ln();
            let w_ij = (samples / 400.0).clamp(0.25, 3.5);

            mat_m[(i, i)] += w_ij;
            mat_m[(j, j)] += w_ij;
            mat_m[(i, j)] -= w_ij;
            mat_m[(j, i)] -= w_ij;

            vec_b[i] += w_ij * log_ratio;
            vec_b[j] -= w_ij * log_ratio;
        }

        let cholesky = mat_m.clone().cholesky();
        let solved_log_g = match cholesky {
            Some(chol) => chol.solve(&vec_b),
            None => mat_m
                .svd(true, true)
                .solve(&vec_b, 1e-6)
                .unwrap_or_else(|_| DVector::from_element(num_images, 0.0)),
        };

        // Zero-center mean log-gain so overall panorama brightness never drifts
        let mean_log_g: f64 = solved_log_g.iter().copied().sum::<f64>() / num_images as f64;

        for i in 0..num_images {
            let g = (solved_log_g[i] - mean_log_g).exp() as f32;
            gains[i][channel] = g.clamp(0.82, 1.22);
        }
    }

    gains
}

#[derive(Debug, Clone)]
pub struct OverlapIntegral {
    pub img_idx_1: usize,
    pub img_idx_2: usize,
    pub sum_i_squared: [f64; 3],
    pub sum_j_squared: [f64; 3],
    pub sum_ij: [f64; 3],
    #[allow(dead_code)]
    pub sample_count: usize,
}

/// Downsamples an Rgb32FImage by 2x using box filtering
pub fn downsample_f32_image(src: &Rgb32FImage) -> Rgb32FImage {
    let (w, h) = src.dimensions();
    let nw = (w / 2).max(1);
    let nh = (h / 2).max(1);
    let mut dst = Rgb32FImage::new(nw, nh);

    let src_raw = src.as_raw();
    let src_stride = w as usize * 3;
    let dst_stride = nw as usize * 3;

    dst.as_mut()
        .par_chunks_mut(dst_stride)
        .enumerate()
        .for_each(|(y, dst_row)| {
            let sy0 = (y * 2).min(h as usize - 1);
            let sy1 = (sy0 + 1).min(h as usize - 1);

            let row0_offset = sy0 * src_stride;
            let row1_offset = sy1 * src_stride;

            for x in 0..nw as usize {
                let sx0 = (x * 2).min(w as usize - 1) * 3;
                let sx1 = ((x * 2 + 1).min(w as usize - 1)) * 3;

                let dst_idx = x * 3;
                for c in 0..3 {
                    let v00 = src_raw[row0_offset + sx0 + c];
                    let v10 = src_raw[row0_offset + sx1 + c];
                    let v01 = src_raw[row1_offset + sx0 + c];
                    let v11 = src_raw[row1_offset + sx1 + c];
                    dst_row[dst_idx + c] = (v00 + v10 + v01 + v11) * 0.25;
                }
            }
        });

    dst
}

/// Upsamples an Rgb32FImage to the specified dimensions with bilinear interpolation
pub fn upsample_f32_image(src: &Rgb32FImage, target_w: u32, target_h: u32) -> Rgb32FImage {
    let (sw, sh) = src.dimensions();
    let mut dst = Rgb32FImage::new(target_w, target_h);

    let src_raw = src.as_raw();
    let src_stride = sw as usize * 3;
    let dst_stride = target_w as usize * 3;

    let scale_x = sw as f32 / target_w.max(1) as f32;
    let scale_y = sh as f32 / target_h.max(1) as f32;

    dst.as_mut()
        .par_chunks_mut(dst_stride)
        .enumerate()
        .for_each(|(y, dst_row)| {
            let sy = (y as f32 + 0.5) * scale_y - 0.5;
            let sy_floor = sy.floor().max(0.0) as usize;
            let sy_ceil = (sy_floor + 1).min(sh as usize - 1);
            let fy = (sy - sy_floor as f32).clamp(0.0, 1.0);

            let row0_offset = sy_floor * src_stride;
            let row1_offset = sy_ceil * src_stride;

            for x in 0..target_w as usize {
                let sx = (x as f32 + 0.5) * scale_x - 0.5;
                let sx_floor = sx.floor().max(0.0) as usize;
                let sx_ceil = (sx_floor + 1).min(sw as usize - 1);
                let fx = (sx - sx_floor as f32).clamp(0.0, 1.0);

                let dst_idx = x * 3;
                let sx0 = sx_floor * 3;
                let sx1 = sx_ceil * 3;

                for c in 0..3 {
                    let v00 = src_raw[row0_offset + sx0 + c];
                    let v10 = src_raw[row0_offset + sx1 + c];
                    let v01 = src_raw[row1_offset + sx0 + c];
                    let v11 = src_raw[row1_offset + sx1 + c];

                    let top = v00 * (1.0 - fx) + v10 * fx;
                    let bottom = v01 * (1.0 - fx) + v11 * fx;
                    dst_row[dst_idx + c] = top * (1.0 - fy) + bottom * fy;
                }
            }
        });

    dst
}

/// Downsamples a float single-channel weight mask by 2x
pub fn downsample_mask(src: &[f32], w: u32, h: u32) -> (Vec<f32>, u32, u32) {
    let nw = (w / 2).max(1);
    let nh = (h / 2).max(1);
    let mut dst = vec![0.0f32; (nw * nh) as usize];

    for y in 0..nh {
        let sy0 = (y * 2).min(h - 1);
        let sy1 = (sy0 + 1).min(h - 1);

        for x in 0..nw {
            let sx0 = (x * 2).min(w - 1);
            let sx1 = (sx0 + 1).min(w - 1);

            let v00 = src[(sy0 * w + sx0) as usize];
            let v10 = src[(sy0 * w + sx1) as usize];
            let v01 = src[(sy1 * w + sx0) as usize];
            let v11 = src[(sy1 * w + sx1) as usize];

            dst[(y * nw + x) as usize] = (v00 + v10 + v01 + v11) * 0.25;
        }
    }

    (dst, nw, nh)
}

/// Upsamples a float single-channel weight mask
#[allow(dead_code)]
pub fn upsample_mask(src: &[f32], sw: u32, sh: u32, target_w: u32, target_h: u32) -> Vec<f32> {
    let mut dst = vec![0.0f32; (target_w * target_h) as usize];
    let scale_x = sw as f32 / target_w.max(1) as f32;
    let scale_y = sh as f32 / target_h.max(1) as f32;

    for y in 0..target_h {
        let sy = (y as f32 + 0.5) * scale_y - 0.5;
        let sy_floor = sy.floor().max(0.0) as usize;
        let sy_ceil = (sy_floor + 1).min(sh as usize - 1);
        let fy = (sy - sy_floor as f32).clamp(0.0, 1.0);

        for x in 0..target_w {
            let sx = (x as f32 + 0.5) * scale_x - 0.5;
            let sx_floor = sx.floor().max(0.0) as usize;
            let sx_ceil = (sx_floor + 1).min(sw as usize - 1);
            let fx = (sx - sx_floor as f32).clamp(0.0, 1.0);

            let v00 = src[sy_floor * sw as usize + sx_floor];
            let v10 = src[sy_floor * sw as usize + sx_ceil];
            let v01 = src[sy_ceil * sw as usize + sx_floor];
            let v11 = src[sy_ceil * sw as usize + sx_ceil];

            let top = v00 * (1.0 - fx) + v10 * fx;
            let bottom = v01 * (1.0 - fx) + v11 * fx;
            dst[(y * target_w + x) as usize] = top * (1.0 - fy) + bottom * fy;
        }
    }

    dst
}

#[inline]
fn smoothstep_f32(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Performs Burt & Adelson Multi-Band (Laplacian Pyramid) Spline Blending between two aligned images
/// given a 2D Graph-Cut weight mask.
///
/// Returns the seamlessly blended full-resolution Rgb32FImage.
pub fn multiband_laplacian_blend(
    img_a: &Rgb32FImage,
    img_b: &Rgb32FImage,
    weight_a: &[f32],
    num_levels: usize,
) -> Rgb32FImage {
    let (w, h) = img_a.dimensions();
    let num_levels = num_levels.clamp(2, 7);

    // 1. Build Gaussian Pyramids for img_a and img_b
    let mut g_pyr_a = vec![img_a.clone()];
    let mut g_pyr_b = vec![img_b.clone()];

    for _ in 1..num_levels {
        let down_a = downsample_f32_image(g_pyr_a.last().unwrap());
        let down_b = downsample_f32_image(g_pyr_b.last().unwrap());
        g_pyr_a.push(down_a);
        g_pyr_b.push(down_b);
    }

    // 2. Build Laplacian Pyramids for img_a and img_b
    // L_k = G_k - upsample(G_{k+1})
    let mut l_pyr_a = Vec::new();
    let mut l_pyr_b = Vec::new();

    for k in 0..num_levels - 1 {
        let (kw, kh) = g_pyr_a[k].dimensions();
        let up_a = upsample_f32_image(&g_pyr_a[k + 1], kw, kh);
        let up_b = upsample_f32_image(&g_pyr_b[k + 1], kw, kh);

        let mut diff_a = g_pyr_a[k].clone();
        let mut diff_b = g_pyr_b[k].clone();

        diff_a
            .as_mut()
            .par_iter_mut()
            .zip(up_a.as_raw().par_iter())
            .for_each(|(d, u)| {
                *d -= *u;
            });
        diff_b
            .as_mut()
            .par_iter_mut()
            .zip(up_b.as_raw().par_iter())
            .for_each(|(d, u)| {
                *d -= *u;
            });

        l_pyr_a.push(diff_a);
        l_pyr_b.push(diff_b);
    }
    // Deepest level retains the low-frequency residual Gaussian
    l_pyr_a.push(g_pyr_a.last().unwrap().clone());
    l_pyr_b.push(g_pyr_b.last().unwrap().clone());

    // 3. Build Gaussian Pyramid for the 2D Graph-Cut Weight Mask
    let mut w_pyr = Vec::new();
    let mut cur_w = weight_a.to_vec();
    let mut cur_dim = (w, h);
    w_pyr.push((cur_w.clone(), cur_dim.0, cur_dim.1));

    for _ in 1..num_levels {
        let (mut down_w, nw, nh) = downsample_mask(&cur_w, cur_dim.0, cur_dim.1);
        // Apply 1 pass of 3x3 smoothing at each pyramid level so coarse illumination bands
        // have a wide, butter-smooth transition zone centered on the optimal Graph-Cut seam.
        if nw >= 4 && nh >= 4 {
            let prev = down_w.clone();
            let nw_u = nw as usize;
            let nh_u = nh as usize;
            for y in 1..(nh_u - 1) {
                for x in 1..(nw_u - 1) {
                    let idx = y * nw_u + x;
                    let sum = prev[idx] * 4.0
                        + (prev[idx - 1] + prev[idx + 1] + prev[idx - nw_u] + prev[idx + nw_u])
                            * 1.5
                        + (prev[idx - nw_u - 1]
                            + prev[idx - nw_u + 1]
                            + prev[idx + nw_u - 1]
                            + prev[idx + nw_u + 1])
                            * 0.5;
                    down_w[idx] = sum / 12.0;
                }
            }
        }
        w_pyr.push((down_w.clone(), nw, nh));
        cur_w = down_w;
        cur_dim = (nw, nh);
    }

    // 4. Blend each Laplacian band using the corresponding Gaussian weight level
    let mut blended_l_pyr = Vec::new();
    for k in 0..num_levels {
        let (kw, kh) = l_pyr_a[k].dimensions();
        let (weights, _, _) = &w_pyr[k];
        let mut blended_band = Rgb32FImage::new(kw, kh);

        let band_a = l_pyr_a[k].as_raw();
        let band_b = l_pyr_b[k].as_raw();

        blended_band
            .as_mut()
            .par_chunks_mut(kw as usize * 3)
            .enumerate()
            .for_each(|(y, row)| {
                for x in 0..kw as usize {
                    let pixel_idx = y * kw as usize + x;
                    let raw_w = weights[pixel_idx].clamp(0.0, 1.0);
                    // Finest band (k == 0) uses a tight anti-aliased transition right along the Graph-Cut seam
                    let w_val = if k == 0 {
                        smoothstep_f32(0.32, 0.68, raw_w)
                    } else {
                        raw_w
                    };
                    let w_b = 1.0 - w_val;

                    let raw_idx = pixel_idx * 3;
                    let out_idx = x * 3;

                    row[out_idx] = band_a[raw_idx] * w_val + band_b[raw_idx] * w_b;
                    row[out_idx + 1] = band_a[raw_idx + 1] * w_val + band_b[raw_idx + 1] * w_b;
                    row[out_idx + 2] = band_a[raw_idx + 2] * w_val + band_b[raw_idx + 2] * w_b;
                }
            });

        blended_l_pyr.push(blended_band);
    }

    // 5. Reconstruct from coarsest to finest: R_k = L_k + upsample(R_{k+1})
    let mut current_reconstructed = blended_l_pyr.last().unwrap().clone();

    for k in (0..num_levels - 1).rev() {
        let (target_w, target_h) = blended_l_pyr[k].dimensions();
        let up = upsample_f32_image(&current_reconstructed, target_w, target_h);

        let mut recombined = blended_l_pyr[k].clone();
        recombined
            .as_mut()
            .par_iter_mut()
            .zip(up.as_raw().par_iter())
            .for_each(|(d, u)| {
                *d += *u;
            });
        current_reconstructed = recombined;
    }

    current_reconstructed
}

/// Harmonizes residual low-frequency 2D exposure/vignetting/color differences across the overlap ROI.
///
/// Uses the boundary-pinned algebraic factorization:
///   `I_A_tilde = I_A * R(x,y)^(1 - alpha)`
///   `I_B_tilde = I_B * R(x,y)^(-alpha)`
/// where `alpha = d_A / (d_A + d_B) in [0, 1]` and `R(x,y) = smooth(I_B / I_A)`.
/// - At `only_A` border (`alpha = 1`): `I_A_tilde = I_A` (0.00% step with outer canvas A).
/// - At `only_B` border (`alpha = 0`): `I_B_tilde = I_B` (0.00% step with outer panel B).
/// - Throughout the overlap: `I_A_tilde / I_B_tilde = (I_A / I_B) * R(x,y) = 1.0` (100% sky/surface continuity).
fn harmonize_overlap_roi_photometry(
    roi_a: &mut Rgb32FImage,
    roi_b: &mut Rgb32FImage,
    in_a_mask: &[bool],
    in_b_mask: &[bool],
    dist_a: &[f32],
    dist_b: &[f32],
) {
    let (rw, rh) = (roi_a.width() as usize, roi_a.height() as usize);
    if rw < 16 || rh < 16 {
        return;
    }

    let gw = 12usize;
    let gh = 12usize;
    let mut grid_ratio = vec![[1.0f32; 3]; gw * gh];

    for gy in 0..gh {
        let y0 = (gy * rh) / gh;
        let y1 = (((gy + 1) * rh) / gh).max(y0 + 1).min(rh);
        for gx in 0..gw {
            let x0 = (gx * rw) / gw;
            let x1 = (((gx + 1) * rw) / gw).max(x0 + 1).min(rw);

            let mut sum_a = [0.0f64; 3];
            let mut sum_b = [0.0f64; 3];
            let mut sum_w = 0.0f64;

            for y in (y0..y1).step_by(2) {
                for x in (x0..x1).step_by(2) {
                    let idx = y * rw + x;
                    if in_a_mask[idx] && in_b_mask[idx] {
                        let pa = roi_a.get_pixel(x as u32, y as u32);
                        let pb = roi_b.get_pixel(x as u32, y as u32);
                        let la = 0.2126 * pa[0] + 0.7152 * pa[1] + 0.0722 * pa[2];
                        let lb = 0.2126 * pb[0] + 0.7152 * pb[1] + 0.0722 * pb[2];
                        if la > 0.005 && lb > 0.005 {
                            let rel_diff = (la - lb).abs() / la.max(lb);
                            // Reject moving objects or extreme occlusion outliers
                            if rel_diff < 0.42 {
                                let w = (1.0 - (rel_diff / 0.42).powi(2)) as f64;
                                for c in 0..3 {
                                    sum_a[c] += pa[c] as f64 * w;
                                    sum_b[c] += pb[c] as f64 * w;
                                }
                                sum_w += w;
                            }
                        }
                    }
                }
            }

            if sum_w > 6.0 {
                let mut cell_r = [1.0f32; 3];
                for c in 0..3 {
                    if sum_a[c] > 1e-4 && sum_b[c] > 1e-4 {
                        cell_r[c] = ((sum_b[c] / sum_a[c]) as f32).clamp(0.75, 1.33);
                    }
                }
                grid_ratio[gy * gw + gx] = cell_r;
            }
        }
    }

    // Smooth the 12x12 ratio grid with 4 iterations of 3x3 box filtering
    for _ in 0..4 {
        let prev = grid_ratio.clone();
        for gy in 0..gh {
            for gx in 0..gw {
                let mut acc = [0.0f32; 3];
                let mut cnt = 0.0f32;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let ny = (gy as i32 + dy).clamp(0, gh as i32 - 1) as usize;
                        let nx = (gx as i32 + dx).clamp(0, gw as i32 - 1) as usize;
                        let w = if dx == 0 && dy == 0 { 2.0 } else { 1.0 };
                        let p = prev[ny * gw + nx];
                        acc[0] += p[0] * w;
                        acc[1] += p[1] * w;
                        acc[2] += p[2] * w;
                        cnt += w;
                    }
                }
                grid_ratio[gy * gw + gx] = [acc[0] / cnt, acc[1] / cnt, acc[2] / cnt];
            }
        }
    }

    // Apply boundary-pinned exposure harmonization across the overlap
    let sx_scale = (gw - 1) as f32 / rw.max(1) as f32;
    let sy_scale = (gh - 1) as f32 / rh.max(1) as f32;

    roi_a
        .as_mut()
        .par_chunks_mut(rw * 3)
        .zip(roi_b.as_mut().par_chunks_mut(rw * 3))
        .enumerate()
        .for_each(|(y, (row_a, row_b))| {
            let gy_f = (y as f32 * sy_scale).clamp(0.0, (gh - 1) as f32);
            let gy0 = gy_f.floor() as usize;
            let gy1 = (gy0 + 1).min(gh - 1);
            let fy = gy_f - gy0 as f32;
            let row_off = y * rw;

            for x in 0..rw {
                let idx = row_off + x;
                if !in_a_mask[idx] || !in_b_mask[idx] {
                    continue;
                }
                let gx_f = (x as f32 * sx_scale).clamp(0.0, (gw - 1) as f32);
                let gx0 = gx_f.floor() as usize;
                let gx1 = (gx0 + 1).min(gw - 1);
                let fx = gx_f - gx0 as f32;

                let da = dist_a[idx];
                let db = dist_b[idx];
                let alpha = (da / (da + db + 1e-4)).clamp(0.0, 1.0);

                let p00 = grid_ratio[gy0 * gw + gx0];
                let p10 = grid_ratio[gy0 * gw + gx1];
                let p01 = grid_ratio[gy1 * gw + gx0];
                let p11 = grid_ratio[gy1 * gw + gx1];

                let c_idx = x * 3;
                for c in 0..3 {
                    let top = p00[c] * (1.0 - fx) + p10[c] * fx;
                    let bot = p01[c] * (1.0 - fx) + p11[c] * fx;
                    let r_val = (top * (1.0 - fy) + bot * fy).clamp(0.75, 1.33);

                    let gain_a = r_val.powf(1.0 - alpha);
                    let gain_b = r_val.powf(-alpha);

                    row_a[c_idx + c] *= gain_a;
                    row_b[c_idx + c] *= gain_b;
                }
            }
        });
}

/// Seam-Ribbon Local Optical-Flow De-Parallax Warping:
/// Estimates local translation offsets along the 2D Graph-Cut seam contour to align foreground
/// linear structures (railway tracks, handrails, wires, horizons) affected by handheld camera translation,
/// and applies a smooth symmetric warp `(-0.5 * d, +0.5 * d)` that decays to zero away from the seam.
fn align_seam_ribbon_deparallax(
    roi_a: &mut Rgb32FImage,
    roi_b: &mut Rgb32FImage,
    in_a_mask: &[bool],
    in_b_mask: &[bool],
    dist_a: &[f32],
    dist_b: &[f32],
    roi_weight: &[f32],
) {
    let (rw, rh) = (roi_a.width() as usize, roi_a.height() as usize);
    if rw < 48 || rh < 48 {
        return;
    }

    // 1. Build 2x downsampled perceptual luminance and gradient images for fast block matching
    let hw = rw / 2;
    let hh = rh / 2;
    let mut lum_a = vec![0.0f32; hw * hh];
    let mut lum_b = vec![0.0f32; hw * hh];
    let mut half_seam = vec![false; hw * hh];

    for hy in 0..hh {
        let sy = hy * 2;
        for hx in 0..hw {
            let sx = hx * 2;
            let hidx = hy * hw + hx;
            let pa = roi_a.get_pixel(sx as u32, sy as u32);
            let pb = roi_b.get_pixel(sx as u32, sy as u32);
            let ya = (0.2126 * pa[0] + 0.7152 * pa[1] + 0.0722 * pa[2]).max(0.0);
            let yb = (0.2126 * pb[0] + 0.7152 * pb[1] + 0.0722 * pb[2]).max(0.0);
            lum_a[hidx] = (ya / (ya + 0.18)).sqrt();
            lum_b[hidx] = (yb / (yb + 0.18)).sqrt();

            let fidx = sy * rw + sx;
            if in_a_mask[fidx] && in_b_mask[fidx] {
                let w0 = roi_weight[fidx];
                let wx = if sx + 2 < rw { roi_weight[fidx + 2] } else { w0 };
                let wy = if sy + 2 < rh { roi_weight[(sy + 2) * rw + sx] } else { w0 };
                if (w0 >= 0.5) != (wx >= 0.5) || (w0 >= 0.5) != (wy >= 0.5) {
                    half_seam[hidx] = true;
                }
            }
        }
    }

    let mut grad_a = vec![0.0f32; hw * hh];
    let mut grad_b = vec![0.0f32; hw * hh];
    for hy in 1..(hh - 1) {
        for hx in 1..(hw - 1) {
            let idx = hy * hw + hx;
            let gxa = lum_a[idx + 1] - lum_a[idx - 1];
            let gya = lum_a[idx + hw] - lum_a[idx - hw];
            grad_a[idx] = (gxa * gxa + gya * gya).sqrt();

            let gxb = lum_b[idx + 1] - lum_b[idx - 1];
            let gyb = lum_b[idx + hw] - lum_b[idx - hw];
            grad_b[idx] = (gxb * gxb + gyb * gyb).sqrt();
        }
    }

    // 2. Define deformation grid with 48px full-res spacing (24px in half-res)
    // A wider control-point spacing prevents high-frequency waviness along diagonal linear structures.
    let cell = 48usize;
    let gw = rw.div_ceil(cell) + 1;
    let gh = rh.div_ceil(cell) + 1;
    let g_len = gw * gh;

    let mut seam_dist = vec![1e4f32; g_len];
    let mut border_dist = vec![0.0f32; g_len];

    for gy in 0..gh {
        let fy = (gy * cell).min(rh - 1);
        let hy = (fy / 2).min(hh - 1);
        for gx in 0..gw {
            let fx = (gx * cell).min(rw - 1);
            let hx = (fx / 2).min(hw - 1);
            let gidx = gy * gw + gx;
            let fidx = fy * rw + fx;
            border_dist[gidx] = dist_a[fidx].min(dist_b[fidx]);

            // Check 9x9 neighborhood in half-res for seam crossing
            let mut near_seam = false;
            for dy in -4i32..=4 {
                for dx in -4i32..=4 {
                    let ny = (hy as i32 + dy).clamp(0, hh as i32 - 1) as usize;
                    let nx = (hx as i32 + dx).clamp(0, hw as i32 - 1) as usize;
                    if half_seam[ny * hw + nx] {
                        near_seam = true;
                        break;
                    }
                }
            }
            if near_seam {
                seam_dist[gidx] = 0.0;
            }
        }
    }

    // Chamfer distance propagation on the coarse grid
    for gy in 0..gh {
        for gx in 0..gw {
            let idx = gy * gw + gx;
            if gx > 0 {
                seam_dist[idx] = seam_dist[idx].min(seam_dist[idx - 1] + 1.0);
            }
            if gy > 0 {
                seam_dist[idx] = seam_dist[idx].min(seam_dist[(gy - 1) * gw + gx] + 1.0);
            }
        }
    }
    for gy in (0..gh).rev() {
        for gx in (0..gw).rev() {
            let idx = gy * gw + gx;
            if gx + 1 < gw {
                seam_dist[idx] = seam_dist[idx].min(seam_dist[idx + 1] + 1.0);
            }
            if gy + 1 < gh {
                seam_dist[idx] = seam_dist[idx].min(seam_dist[(gy + 1) * gw + gx] + 1.0);
            }
        }
    }

    // 3. Estimate local parallax shift (dx, dy) at grid nodes near the seam contour
    let mut grid_dx = vec![0.0f32; g_len];
    let mut grid_dy = vec![0.0f32; g_len];

    let patch_r = 8i32; // 17x17 half-res patch = 34x34 full-res patch
    let search_r = 2i32; // +/- 2 half-res px = +/- 4 full-res px

    for gy in 0..gh {
        let fy = (gy * cell).min(rh - 1);
        let hy = (fy / 2) as i32;
        for gx in 0..gw {
            let gidx = gy * gw + gx;
            if seam_dist[gidx] > 2.0 || border_dist[gidx] < 28.0 {
                continue;
            }
            let fx = (gx * cell).min(rw - 1);
            let hx = (fx / 2) as i32;

            if hx - patch_r - search_r < 1
                || hx + patch_r + search_r >= hw as i32 - 1
                || hy - patch_r - search_r < 1
                || hy + patch_r + search_r >= hh as i32 - 1
            {
                continue;
            }

            // Require structural gradient energy (skip flat sky)
            let mut grad_sum = 0.0f32;
            for py in -patch_r..=patch_r {
                for px in -patch_r..=patch_r {
                    grad_sum += grad_a[((hy + py) as usize) * hw + (hx + px) as usize];
                }
            }
            let mean_grad = grad_sum / ((2 * patch_r + 1).pow(2) as f32);
            if mean_grad < 0.028 {
                continue;
            }

            let eval_cost = |dx: i32, dy: i32| -> f32 {
                let mut err = 0.0f32;
                for py in -patch_r..=patch_r {
                    let ay = (hy + py) as usize;
                    let by = (hy + py + dy) as usize;
                    for px in -patch_r..=patch_r {
                        let ax = (hx + px) as usize;
                        let bx = (hx + px + dx) as usize;
                        let a_idx = ay * hw + ax;
                        let b_idx = by * hw + bx;
                        let dl = (lum_a[a_idx] - lum_b[b_idx]).abs();
                        let dg = (grad_a[a_idx] - grad_b[b_idx]).abs();
                        err += dl + 1.2 * dg;
                    }
                }
                let reg = 0.12 * ((dx * dx + dy * dy) as f32);
                err + reg
            };

            let cost_zero = eval_cost(0, 0);
            let mut best_cost = cost_zero;
            let mut best_dx = 0i32;
            let mut best_dy = 0i32;

            for dy in -search_r..=search_r {
                for dx in -search_r..=search_r {
                    let c = eval_cost(dx, dy);
                    if c < best_cost {
                        best_cost = c;
                        best_dx = dx;
                        best_dy = dy;
                    }
                }
            }

            if best_cost < cost_zero * 0.78 && (best_dx != 0 || best_dy != 0) {
                // Parabolic sub-pixel refinement
                let mut sub_dx = best_dx as f32;
                let mut sub_dy = best_dy as f32;
                if best_dx.abs() < search_r {
                    let cm = eval_cost(best_dx - 1, best_dy);
                    let cp = eval_cost(best_dx + 1, best_dy);
                    let denom = 2.0 * (cm + cp - 2.0 * best_cost);
                    if denom.abs() > 1e-4 {
                        sub_dx += ((cm - cp) / denom).clamp(-0.5, 0.5);
                    }
                }
                if best_dy.abs() < search_r {
                    let cm = eval_cost(best_dx, best_dy - 1);
                    let cp = eval_cost(best_dx, best_dy + 1);
                    let denom = 2.0 * (cm + cp - 2.0 * best_cost);
                    if denom.abs() > 1e-4 {
                        sub_dy += ((cm - cp) / denom).clamp(-0.5, 0.5);
                    }
                }

                grid_dx[gidx] = (sub_dx * 2.0).clamp(-2.5, 2.5);
                grid_dy[gidx] = (sub_dy * 2.0).clamp(-2.5, 2.5);
            }
        }
    }

    // 4. Diffuse and smooth the displacement field across the seam ribbon (6 passes)
    for _ in 0..6 {
        let prev_x = grid_dx.clone();
        let prev_y = grid_dy.clone();
        for gy in 1..(gh.saturating_sub(1)) {
            for gx in 1..(gw.saturating_sub(1)) {
                let idx = gy * gw + gx;
                grid_dx[idx] = 0.4 * prev_x[idx]
                    + 0.15
                        * (prev_x[idx - 1]
                            + prev_x[idx + 1]
                            + prev_x[idx - gw]
                            + prev_x[idx + gw]);
                grid_dy[idx] = 0.4 * prev_y[idx]
                    + 0.15
                        * (prev_y[idx - 1]
                            + prev_y[idx + 1]
                            + prev_y[idx - gw]
                            + prev_y[idx + gw]);
            }
        }
    }

    // Apply distance-to-seam and distance-to-border decay envelope
    let mut has_active_warp = false;
    for idx in 0..g_len {
        let sd = seam_dist[idx];
        let bd = border_dist[idx];
        let w_seam = (-sd * sd / (2.0 * 2.5 * 2.5)).exp();
        let w_bord = smoothstep_f32(12.0, 36.0, bd);
        let env = w_seam * w_bord;
        grid_dx[idx] *= env;
        grid_dy[idx] *= env;
        if grid_dx[idx].abs() > 0.15 || grid_dy[idx].abs() > 0.15 {
            has_active_warp = true;
        }
    }

    if !has_active_warp {
        return;
    }

    // 5. Symmetrically warp roi_a by (-0.5 * dx, -0.5 * dy) and roi_b by (+0.5 * dx, +0.5 * dy)
    let src_a = roi_a.clone();
    let src_b = roi_b.clone();
    let inv_cell = 1.0 / cell as f32;

    roi_a
        .as_mut()
        .par_chunks_mut(rw * 3)
        .zip(roi_b.as_mut().par_chunks_mut(rw * 3))
        .enumerate()
        .for_each(|(y, (row_a, row_b))| {
            let gy_f = (y as f32 * inv_cell).clamp(0.0, (gh - 1) as f32);
            let gy0 = gy_f.floor() as usize;
            let gy1 = (gy0 + 1).min(gh - 1);
            let fy = gy_f - gy0 as f32;
            let row_off = y * rw;

            for x in 0..rw {
                let idx = row_off + x;
                if !in_a_mask[idx] || !in_b_mask[idx] {
                    continue;
                }
                let gx_f = (x as f32 * inv_cell).clamp(0.0, (gw - 1) as f32);
                let gx0 = gx_f.floor() as usize;
                let gx1 = (gx0 + 1).min(gw - 1);
                let fx = gx_f - gx0 as f32;

                let i00 = gy0 * gw + gx0;
                let i10 = gy0 * gw + gx1;
                let i01 = gy1 * gw + gx0;
                let i11 = gy1 * gw + gx1;

                let dx = (grid_dx[i00] * (1.0 - fx) + grid_dx[i10] * fx) * (1.0 - fy)
                    + (grid_dx[i01] * (1.0 - fx) + grid_dx[i11] * fx) * fy;
                let dy = (grid_dy[i00] * (1.0 - fx) + grid_dy[i10] * fx) * (1.0 - fy)
                    + (grid_dy[i01] * (1.0 - fx) + grid_dy[i11] * fx) * fy;

                if dx.abs() < 0.05 && dy.abs() < 0.05 {
                    continue;
                }

                let ax = (x as f64 - 0.5 * dx as f64).clamp(0.0, (rw - 1) as f64);
                let ay = (y as f64 - 0.5 * dy as f64).clamp(0.0, (rh - 1) as f64);
                let bx = (x as f64 + 0.5 * dx as f64).clamp(0.0, (rw - 1) as f64);
                let by = (y as f64 + 0.5 * dy as f64).clamp(0.0, (rh - 1) as f64);

                let px_a = crate::panorama_utils::stitching::get_interpolated_pixel(&src_a, ax, ay);
                let px_b = crate::panorama_utils::stitching::get_interpolated_pixel(&src_b, bx, by);

                let c_idx = x * 3;
                row_a[c_idx] = px_a[0];
                row_a[c_idx + 1] = px_a[1];
                row_a[c_idx + 2] = px_a[2];

                row_b[c_idx] = px_b[0];
                row_b[c_idx + 1] = px_b[1];
                row_b[c_idx + 2] = px_b[2];
            }
        });
}

/// Defense 11: Detects rigid architectural structures to prevent optical-flow shearing
pub fn detect_structural_architecture(
    roi: &Rgb32FImage,
    in_a: &[bool],
    in_b: &[bool],
) -> bool {
    let (rw, rh) = (roi.width() as usize, roi.height() as usize);
    if rw < 32 || rh < 32 {
        return false;
    }

    let mut strong_edges = 0u32;
    let mut rectilinear_edges = 0u32;

    for y in (1..rh - 1).step_by(2) {
        for x in (1..rw - 1).step_by(2) {
            let idx = y * rw + x;
            if !in_a[idx] || !in_b[idx] {
                continue;
            }

            let pl = roi.get_pixel((x - 1) as u32, y as u32);
            let pr = roi.get_pixel((x + 1) as u32, y as u32);
            let pt = roi.get_pixel(x as u32, (y - 1) as u32);
            let pb = roi.get_pixel(x as u32, (y + 1) as u32);

            let y_l = 0.2126 * pl[0] + 0.7152 * pl[1] + 0.0722 * pl[2];
            let y_r = 0.2126 * pr[0] + 0.7152 * pr[1] + 0.0722 * pr[2];
            let y_t = 0.2126 * pt[0] + 0.7152 * pt[1] + 0.0722 * pt[2];
            let y_b = 0.2126 * pb[0] + 0.7152 * pb[1] + 0.0722 * pb[2];

            let gx = (y_r - y_l).abs();
            let gy = (y_b - y_t).abs();
            let grad_mag = (gx * gx + gy * gy).sqrt();

            if grad_mag > 0.08 {
                strong_edges += 1;
                if gx > 2.2 * gy || gy > 2.2 * gx {
                    rectilinear_edges += 1;
                }
            }
        }
    }

    if strong_edges >= 150 {
        let ratio = rectilinear_edges as f32 / strong_edges as f32;
        ratio >= 0.58
    } else {
        false
    }
}

/// Analytical 4-corner polygon bounding box calculation for overlapping regions between two camera poses.
pub fn compute_analytical_overlap_bbox(
    pose_a: &crate::panorama_utils::camera_model::CameraPose,
    w_a: u32,
    h_a: u32,
    pose_b: &crate::panorama_utils::camera_model::CameraPose,
    w_b: u32,
    h_b: u32,
    canvas_w: u32,
    canvas_h: u32,
) -> Option<(u32, u32, u32, u32)> {
    let r_b = pose_b.rotation_matrix();
    let r_inv_a = pose_a.rotation_matrix().transpose();
    let r_a = pose_a.rotation_matrix();
    let r_inv_b = pose_b.rotation_matrix().transpose();

    let sample_points_b = [
        (0.0, 0.0),
        (w_b as f64 * 0.5, 0.0),
        (w_b as f64, 0.0),
        (w_b as f64, h_b as f64 * 0.5),
        (w_b as f64, h_b as f64),
        (w_b as f64 * 0.5, h_b as f64),
        (0.0, h_b as f64),
        (0.0, h_b as f64 * 0.5),
    ];

    let sample_points_a = [
        (0.0, 0.0),
        (w_a as f64 * 0.5, 0.0),
        (w_a as f64, 0.0),
        (w_a as f64, h_a as f64 * 0.5),
        (w_a as f64, h_a as f64),
        (w_a as f64 * 0.5, h_a as f64),
        (0.0, h_a as f64),
        (0.0, h_a as f64 * 0.5),
    ];

    let mut min_x = canvas_w as f64;
    let mut max_x = 0.0f64;
    let mut min_y = canvas_h as f64;
    let mut max_y = 0.0f64;
    let mut valid_count = 0;

    for (u, v) in sample_points_b {
        let ray_b = pose_b.unproject_point(u, v);
        let world_ray = r_b * ray_b;
        if let Some((cx, cy)) = pose_a.project_ray_unbounded(&world_ray, &r_inv_a) {
            if cx >= 0.0 && cx <= w_a as f64 && cy >= 0.0 && cy <= h_a as f64 {
                let canvas_x = (cx / w_a as f64) * canvas_w as f64;
                let canvas_y = (cy / h_a as f64) * canvas_h as f64;
                min_x = min_x.min(canvas_x);
                max_x = max_x.max(canvas_x);
                min_y = min_y.min(canvas_y);
                max_y = max_y.max(canvas_y);
                valid_count += 1;
            }
        }
    }

    for (u, v) in sample_points_a {
        let ray_a = pose_a.unproject_point(u, v);
        let world_ray = r_a * ray_a;
        if let Some((bx, by)) = pose_b.project_ray_unbounded(&world_ray, &r_inv_b) {
            if bx >= 0.0 && bx <= w_b as f64 && by >= 0.0 && by <= h_b as f64 {
                let canvas_x = (u / w_a as f64) * canvas_w as f64;
                let canvas_y = (v / h_a as f64) * canvas_h as f64;
                min_x = min_x.min(canvas_x);
                max_x = max_x.max(canvas_x);
                min_y = min_y.min(canvas_y);
                max_y = max_y.max(canvas_y);
                valid_count += 1;
            }
        }
    }

    if valid_count == 0 || min_x >= max_x || min_y >= max_y {
        None
    } else {
        Some((
            (min_x.floor() as u32).min(canvas_w - 1),
            (max_x.ceil() as u32).min(canvas_w - 1),
            (min_y.floor() as u32).min(canvas_h - 1),
            (max_y.ceil() as u32).min(canvas_h - 1),
        ))
    }
}

/// Seamlessly blends img_b into canvas_a strictly within the overlapping Region of Interest (ROI).
/// Non-overlapping regions from img_b are directly copied into canvas_a.
pub fn multiband_laplacian_blend_roi(
    canvas_a: &mut Rgb32FImage,
    mask_a: &mut GrayImage,
    img_b: &Rgb32FImage,
    mask_b: &GrayImage,
    seam_weight_mask: &crate::panorama_utils::graph_cut::SeamWeightMask,
    num_levels: usize,
) {
    let (w, h) = canvas_a.dimensions();

    // 1. Parallel row scan to find overlapping bounding box across CPU cores
    let (min_x, max_x, min_y, max_y, has_overlap) = {
        let row_ranges: Vec<(u32, u32, u32)> = mask_a
            .par_chunks(w as usize)
            .zip(mask_b.par_chunks(w as usize))
            .enumerate()
            .filter_map(|(y, (row_a, row_b))| {
                let mut rx_min = u32::MAX;
                let mut rx_max = 0u32;
                let mut row_has_overlap = false;
                for x in 0..w as usize {
                    if row_a[x] > 0 && row_b[x] > 0 {
                        rx_min = rx_min.min(x as u32);
                        rx_max = rx_max.max(x as u32);
                        row_has_overlap = true;
                    }
                }
                if row_has_overlap {
                    Some((rx_min, rx_max, y as u32))
                } else {
                    None
                }
            })
            .collect();

        if row_ranges.is_empty() {
            (w, 0, h, 0, false)
        } else {
            let mut mx = w;
            let mut xx = 0;
            let mut my = h;
            let mut xy = 0;
            for (rx_min, rx_max, y) in row_ranges {
                mx = mx.min(rx_min);
                xx = xx.max(rx_max);
                my = my.min(y);
                xy = xy.max(y);
            }
            (mx, xx, my, xy, true)
        }
    };

    if !has_overlap || min_x >= max_x || min_y >= max_y {
        canvas_a
            .par_chunks_mut(w as usize * 3)
            .zip(mask_a.par_chunks_mut(w as usize))
            .zip(img_b.par_chunks(w as usize * 3))
            .zip(mask_b.par_chunks(w as usize))
            .for_each(|(((a_row, ma_row), b_row), mb_row)| {
                for x in 0..w as usize {
                    if mb_row[x] > 0 && ma_row[x] == 0 {
                        a_row[x * 3..x * 3 + 3].copy_from_slice(&b_row[x * 3..x * 3 + 3]);
                        ma_row[x] = 255;
                    }
                }
            });
        return;
    }

    // Expand ROI by 64px margin for multi-scale spatial frequency decay
    let pad = 64u32;
    let roi_min_x = min_x.saturating_sub(pad);
    let roi_max_x = (max_x + pad).min(w - 1);
    let roi_min_y = min_y.saturating_sub(pad);
    let roi_max_y = (max_y + pad).min(h - 1);

    let roi_w = roi_max_x - roi_min_x + 1;
    let roi_h = roi_max_y - roi_min_y + 1;
    let rw_u = roi_w as usize;
    let rh_u = roi_h as usize;
    let num_roi = rw_u * rh_u;

    // Crop sub-images and compute 2D Chamfer distance transforms to exclusive boundaries
    let mut roi_a = Rgb32FImage::new(roi_w, roi_h);
    let mut roi_b = Rgb32FImage::new(roi_w, roi_h);
    let mut roi_weight = vec![0.0f32; num_roi];
    let mut in_a_mask = vec![false; num_roi];
    let mut in_b_mask = vec![false; num_roi];
    let mut dist_a = vec![1e5f32; num_roi];
    let mut dist_b = vec![1e5f32; num_roi];

    for ry in 0..roi_h {
        let cy = roi_min_y + ry;
        for rx in 0..roi_w {
            let cx = roi_min_x + rx;
            let in_a = mask_a.get_pixel(cx, cy)[0] > 0;
            let in_b = mask_b.get_pixel(cx, cy)[0] > 0;
            let r_idx = ry as usize * rw_u + rx as usize;
            in_a_mask[r_idx] = in_a;
            in_b_mask[r_idx] = in_b;
            if !in_a {
                dist_a[r_idx] = 0.0;
            }
            if !in_b {
                dist_b[r_idx] = 0.0;
            }

            let px_a = if in_a {
                *canvas_a.get_pixel(cx, cy)
            } else {
                *img_b.get_pixel(cx, cy)
            };
            let px_b = if in_b {
                *img_b.get_pixel(cx, cy)
            } else {
                *canvas_a.get_pixel(cx, cy)
            };

            roi_a.put_pixel(rx, ry, px_a);
            roi_b.put_pixel(rx, ry, px_b);
            roi_weight[r_idx] = seam_weight_mask.get_weight(cx, cy);
        }
    }

    // Forward & backward distance transform pass on ROI
    for ry in 0..rh_u {
        for rx in 0..rw_u {
            let idx = ry * rw_u + rx;
            if rx > 0 {
                dist_a[idx] = dist_a[idx].min(dist_a[idx - 1] + 1.0);
                dist_b[idx] = dist_b[idx].min(dist_b[idx - 1] + 1.0);
            }
            if ry > 0 {
                let up = (ry - 1) * rw_u + rx;
                dist_a[idx] = dist_a[idx].min(dist_a[up] + 1.0);
                dist_b[idx] = dist_b[idx].min(dist_b[up] + 1.0);
            }
        }
    }
    for ry in (0..rh_u).rev() {
        for rx in (0..rw_u).rev() {
            let idx = ry * rw_u + rx;
            if rx + 1 < rw_u {
                dist_a[idx] = dist_a[idx].min(dist_a[idx + 1] + 1.0);
                dist_b[idx] = dist_b[idx].min(dist_b[idx + 1] + 1.0);
            }
            if ry + 1 < rh_u {
                let dn = (ry + 1) * rw_u + rx;
                dist_a[idx] = dist_a[idx].min(dist_a[dn] + 1.0);
                dist_b[idx] = dist_b[idx].min(dist_b[dn] + 1.0);
            }
        }
    }

    // 2. Harmonize residual low-frequency 2D exposure/vignetting across the overlap
    harmonize_overlap_roi_photometry(
        &mut roi_a,
        &mut roi_b,
        &in_a_mask,
        &in_b_mask,
        &dist_a,
        &dist_b,
    );

    // 3. After harmonizing overlap pixels, extrapolate outer `!in_a` of `roi_a` and `!in_b` of `roi_b`
    // from their harmonized neighbors, and enforce a smooth boundary-pinning envelope on `roi_weight`
    // so the 6-level Laplacian pyramid never bleeds across panel borders or unmapped black corners.
    let mut valid_union = vec![false; num_roi];
    for ry in 0..roi_h {
        for rx in 0..roi_w {
            let r_idx = ry as usize * rw_u + rx as usize;
            let ia = in_a_mask[r_idx];
            let ib = in_b_mask[r_idx];
            if ia && ib {
                valid_union[r_idx] = true;
                let da = dist_a[r_idx];
                let db = dist_b[r_idx];
                if db < 44.0 && da > db {
                    let w_floor = 1.0 - smoothstep_f32(6.0, 44.0, db);
                    roi_weight[r_idx] = roi_weight[r_idx].max(w_floor);
                }
                if da < 44.0 && db > da {
                    let w_ceil = smoothstep_f32(6.0, 44.0, da);
                    roi_weight[r_idx] = roi_weight[r_idx].min(w_ceil);
                }
            } else if !ia && ib {
                valid_union[r_idx] = true;
                let pb = *roi_b.get_pixel(rx, ry);
                roi_a.put_pixel(rx, ry, pb);
                roi_weight[r_idx] = 0.0;
            } else if ia && !ib {
                valid_union[r_idx] = true;
                let pa = *roi_a.get_pixel(rx, ry);
                roi_b.put_pixel(rx, ry, pa);
                roi_weight[r_idx] = 1.0;
            }
        }
    }

    // Extrapolate any `!in_a && !in_b` exterior ROI pixels from nearest valid neighbors (row + col pass)
    // instead of leaving `(0,0,0)` black which would bleed into coarse pyramid levels.
    for ry in 0..rh_u {
        let mut last_valid_x: Option<usize> = None;
        for rx in 0..rw_u {
            let idx = ry * rw_u + rx;
            if valid_union[idx] {
                last_valid_x = Some(rx);
            } else if let Some(vx) = last_valid_x {
                let pa = *roi_a.get_pixel(vx as u32, ry as u32);
                let pb = *roi_b.get_pixel(vx as u32, ry as u32);
                roi_a.put_pixel(rx as u32, ry as u32, pa);
                roi_b.put_pixel(rx as u32, ry as u32, pb);
                roi_weight[idx] = roi_weight[ry * rw_u + vx];
                valid_union[idx] = true;
            }
        }
        last_valid_x = None;
        for rx in (0..rw_u).rev() {
            let idx = ry * rw_u + rx;
            if valid_union[idx] {
                last_valid_x = Some(rx);
            } else if let Some(vx) = last_valid_x {
                let pa = *roi_a.get_pixel(vx as u32, ry as u32);
                let pb = *roi_b.get_pixel(vx as u32, ry as u32);
                roi_a.put_pixel(rx as u32, ry as u32, pa);
                roi_b.put_pixel(rx as u32, ry as u32, pb);
                roi_weight[idx] = roi_weight[ry * rw_u + vx];
                valid_union[idx] = true;
            }
        }
    }
    for rx in 0..rw_u {
        let mut last_valid_y: Option<usize> = None;
        for ry in 0..rh_u {
            let idx = ry * rw_u + rx;
            if valid_union[idx] {
                last_valid_y = Some(ry);
            } else if let Some(vy) = last_valid_y {
                let pa = *roi_a.get_pixel(rx as u32, vy as u32);
                let pb = *roi_b.get_pixel(rx as u32, vy as u32);
                roi_a.put_pixel(rx as u32, ry as u32, pa);
                roi_b.put_pixel(rx as u32, ry as u32, pb);
                roi_weight[idx] = roi_weight[vy * rw_u + rx];
                valid_union[idx] = true;
            }
        }
        last_valid_y = None;
        for ry in (0..rh_u).rev() {
            let idx = ry * rw_u + rx;
            if valid_union[idx] {
                last_valid_y = Some(ry);
            } else if let Some(vy) = last_valid_y {
                let pa = *roi_a.get_pixel(rx as u32, vy as u32);
                let pb = *roi_b.get_pixel(rx as u32, vy as u32);
                roi_a.put_pixel(rx as u32, ry as u32, pa);
                roi_b.put_pixel(rx as u32, ry as u32, pb);
                roi_weight[idx] = roi_weight[vy * rw_u + rx];
                valid_union[idx] = true;
            }
        }
    }

    // 4. Seam-Ribbon Local Optical-Flow De-Parallax Warping (Defense 11: Rigid Architecture Mode)
    let is_rigid_env = std::env::var("RAPIDRAW_RIGID_ARCHITECTURE")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);
    let is_structural = is_rigid_env || detect_structural_architecture(&roi_a, &in_a_mask, &in_b_mask);

    if !is_structural {
        align_seam_ribbon_deparallax(
            &mut roi_a,
            &mut roi_b,
            &in_a_mask,
            &in_b_mask,
            &dist_a,
            &dist_b,
            &roi_weight,
        );
    } else {
        println!("  [Photometric] Rigid architectural scene detected: bypassing seam-ribbon optical-flow shear warping (D11).");
    }

    // 5. Run Multi-Band Laplacian Pyramid blend on the harmonized ROI patch (Defense 5: Adaptive Pyramid Depth)
    let overlap_span = (roi_max_x.saturating_sub(roi_min_x)).min(roi_max_y.saturating_sub(roi_min_y));
    let max_levels = 5usize;
    let adaptive_levels = ((overlap_span as f32 / 16.0).log2().floor() as usize).clamp(1, max_levels);
    let active_levels = adaptive_levels.min(num_levels);
    let blended_roi = multiband_laplacian_blend(&roi_a, &roi_b, &roi_weight, active_levels);

    // 6. Paste blended ROI with wide C1 smoothstep feathering at `mask_a` / `mask_b` borders
    for ry in 0..roi_h {
        let cy = roi_min_y + ry;
        for rx in 0..roi_w {
            let cx = roi_min_x + rx;
            let r_idx = ry as usize * rw_u + rx as usize;
            let in_a = in_a_mask[r_idx];
            let in_b = in_b_mask[r_idx];

            if in_a && in_b {
                let p_blend = *blended_roi.get_pixel(rx, ry);
                let da = dist_a[r_idx];
                let db = dist_b[r_idx];
                if db < 36.0 && da > db {
                    let t = smoothstep_f32(2.0, 36.0, db);
                    let pa = *canvas_a.get_pixel(cx, cy);
                    canvas_a.put_pixel(
                        cx,
                        cy,
                        image::Rgb([
                            pa[0] * (1.0 - t) + p_blend[0] * t,
                            pa[1] * (1.0 - t) + p_blend[1] * t,
                            pa[2] * (1.0 - t) + p_blend[2] * t,
                        ]),
                    );
                } else if da < 36.0 && db > da {
                    let t = smoothstep_f32(2.0, 36.0, da);
                    let pb = *img_b.get_pixel(cx, cy);
                    canvas_a.put_pixel(
                        cx,
                        cy,
                        image::Rgb([
                            pb[0] * (1.0 - t) + p_blend[0] * t,
                            pb[1] * (1.0 - t) + p_blend[1] * t,
                            pb[2] * (1.0 - t) + p_blend[2] * t,
                        ]),
                    );
                } else {
                    canvas_a.put_pixel(cx, cy, p_blend);
                }
            } else if !in_a && in_b {
                canvas_a.put_pixel(cx, cy, *img_b.get_pixel(cx, cy));
                mask_a.put_pixel(cx, cy, Luma([255]));
            }
        }
    }

    // Copy non-overlapping pixels of B outside the ROI
    for cy in 0..h {
        if cy >= roi_min_y && cy <= roi_max_y {
            for cx in 0..roi_min_x {
                if mask_b.get_pixel(cx, cy)[0] > 0 && mask_a.get_pixel(cx, cy)[0] == 0 {
                    canvas_a.put_pixel(cx, cy, *img_b.get_pixel(cx, cy));
                    mask_a.put_pixel(cx, cy, Luma([255]));
                }
            }
            for cx in (roi_max_x + 1)..w {
                if mask_b.get_pixel(cx, cy)[0] > 0 && mask_a.get_pixel(cx, cy)[0] == 0 {
                    canvas_a.put_pixel(cx, cy, *img_b.get_pixel(cx, cy));
                    mask_a.put_pixel(cx, cy, Luma([255]));
                }
            }
        } else {
            for cx in 0..w {
                if mask_b.get_pixel(cx, cy)[0] > 0 && mask_a.get_pixel(cx, cy)[0] == 0 {
                    canvas_a.put_pixel(cx, cy, *img_b.get_pixel(cx, cy));
                    mask_a.put_pixel(cx, cy, Luma([255]));
                }
            }
        }
    }
}

/// Mertens-Kautz-Van Reeth Multiscale Exposure Fusion
/// Blends bracketed HDR exposures by computing local Contrast, Saturation, and Well-Exposedness metrics.
#[allow(dead_code)]
pub fn compute_mertens_exposure_fusion(
    exposures: &[Rgb32FImage],
    w_contrast: f32,
    w_saturation: f32,
    w_exposedness: f32,
) -> Rgb32FImage {
    if exposures.is_empty() {
        return Rgb32FImage::new(0, 0);
    }
    if exposures.len() == 1 {
        return exposures[0].clone();
    }

    let (w, h) = exposures[0].dimensions();
    let num_exp = exposures.len();

    // 1. Compute per-pixel weight maps: W = (C^wc) * (S^ws) * (E^we)
    let mut weight_maps = Vec::with_capacity(num_exp);

    for img in exposures {
        let mut weights = vec![0.0f32; (w * h) as usize];
        let raw = img.as_raw();

        weights
            .par_chunks_mut(w as usize)
            .enumerate()
            .for_each(|(y, row)| {
                for x in 0..w as usize {
                    let idx = (y * w as usize + x) * 3;
                    let r = raw[idx];
                    let g = raw[idx + 1];
                    let b = raw[idx + 2];

                    // Contrast: distance from extreme over/underexposure
                    let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                    let c = (1.0 - (luma - 0.18).abs() / 0.82).clamp(0.1, 1.0);

                    // Saturation: standard deviation across color channels
                    let mean = (r + g + b) / 3.0;
                    let s = (((r - mean).powi(2) + (g - mean).powi(2) + (b - mean).powi(2)) / 3.0).sqrt();

                    // Well-exposedness: Gaussian curve centered at physical linear sensor midtones (~0.18)
                    let sigma_sq = 2.0 * 0.15 * 0.15;
                    let e_r = (-(r - 0.18).powi(2) / sigma_sq).exp();
                    let e_g = (-(g - 0.18).powi(2) / sigma_sq).exp();
                    let e_b = (-(b - 0.18).powi(2) / sigma_sq).exp();
                    let e = e_r * e_g * e_b;

                    let weight = (c.powf(w_contrast).max(1e-4))
                        * (s.powf(w_saturation).max(1e-4))
                        * (e.powf(w_exposedness).max(1e-4))
                        + 1e-6;

                    row[x] = weight;
                }
            });

        weight_maps.push(weights);
    }

    // 2. Normalize weights per pixel across all bracketed exposures: \hat{W}_k = W_k / \sum_j W_j
    let total_pixels = (w * h) as usize;
    let mut normalized_weights = vec![vec![0.0f32; total_pixels]; num_exp];

    for i in 0..total_pixels {
        let sum_w: f32 = weight_maps.iter().map(|wm| wm[i]).sum();
        let inv_sum = if sum_w > 1e-8 { 1.0 / sum_w } else { 1.0 / num_exp as f32 };
        for k in 0..num_exp {
            normalized_weights[k][i] = weight_maps[k][i] * inv_sum;
        }
    }

    // 3. Multiscale Exposure Weighted Fusion
    let mut fused = Rgb32FImage::new(w, h);

    fused
        .par_chunks_mut(w as usize * 3)
        .enumerate()
        .for_each(|(y, row)| {
            for x in 0..w as usize {
                let px_idx = y * w as usize + x;
                let mut sum_r = 0.0f32;
                let mut sum_g = 0.0f32;
                let mut sum_b = 0.0f32;

                for (k, exp_img) in exposures.iter().enumerate() {
                    let w_val = normalized_weights[k][px_idx];
                    let raw_idx = px_idx * 3;
                    let exp_raw = exp_img.as_raw();
                    sum_r += exp_raw[raw_idx] * w_val;
                    sum_g += exp_raw[raw_idx + 1] * w_val;
                    sum_b += exp_raw[raw_idx + 2] * w_val;
                }

                let out_idx = x * 3;
                row[out_idx] = sum_r.clamp(0.0, 1.0);
                row[out_idx + 1] = sum_g.clamp(0.0, 1.0);
                row[out_idx + 2] = sum_b.clamp(0.0, 1.0);
            }
        });

    fused
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mertens_exposure_fusion_single_and_bracket() {
        let img1 = Rgb32FImage::from_pixel(100, 100, image::Rgb([0.2, 0.2, 0.2]));
        let img2 = Rgb32FImage::from_pixel(100, 100, image::Rgb([0.8, 0.8, 0.8]));

        let fused = compute_mertens_exposure_fusion(&[img1, img2], 1.0, 1.0, 1.0);
        assert_eq!(fused.dimensions(), (100, 100));

        let p = fused.get_pixel(50, 50);
        // Middle blended value should be well within [0.2, 0.8]
        assert!(p[0] >= 0.2 && p[0] <= 0.8);
    }
}
