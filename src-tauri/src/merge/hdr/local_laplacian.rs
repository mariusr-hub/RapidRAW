use crate::render::color_space::{OutputColorSpace, srgb_encode};
use image::Rgb32FImage;
use rayon::prelude::*;

const NUM_DISCRETIZATION_LEVELS: usize = 10;
const PYRAMID_LEVELS: usize = 5;

#[derive(Debug, Clone, Copy)]
pub struct LocalLaplacianOptions {
    pub sigma_r: f32,
    pub alpha: f32,
    pub beta: f32,
    pub exposure_bias: f32,
    pub shadow_lift: f32,
    pub highlight_recovery: f32,
}

impl Default for LocalLaplacianOptions {
    fn default() -> Self {
        Self {
            sigma_r: 0.18,
            alpha: 0.85,
            beta: 0.45,
            exposure_bias: 0.0,
            shadow_lift: 30.0,
            highlight_recovery: 50.0,
        }
    }
}

fn downsample_5tap(src: &[f32], w: usize, h: usize) -> (Vec<f32>, usize, usize) {
    const K: [f32; 5] = [1.0 / 16.0, 0.25, 0.375, 0.25, 1.0 / 16.0];
    let mut blur_h = vec![0.0f32; w * h];
    blur_h.par_chunks_exact_mut(w).enumerate().for_each(|(y, out_row)| {
        let row = &src[y * w..(y + 1) * w];
        for x in 0..w {
            let mut sum = 0.0f32;
            for (ki, &kw) in K.iter().enumerate() {
                let sx = (x as isize + ki as isize - 2).clamp(0, w as isize - 1) as usize;
                sum += row[sx] * kw;
            }
            out_row[x] = sum;
        }
    });

    let nw = (w / 2).max(1);
    let nh = (h / 2).max(1);
    let mut dst = vec![0.0f32; nw * nh];
    for dy in 0..nh {
        let sy = (dy * 2).min(h - 1);
        for dx in 0..nw {
            let sx = (dx * 2).min(w - 1);
            let mut sum = 0.0f32;
            for (ki, &kw) in K.iter().enumerate() {
                let yy = (sy as isize + ki as isize - 2).clamp(0, h as isize - 1) as usize;
                sum += blur_h[yy * w + sx] * kw;
            }
            dst[dy * nw + dx] = sum;
        }
    }
    (dst, nw, nh)
}

fn upsample_bilinear(src: &[f32], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; dw * dh];
    let sx_scale = sw as f32 / dw.max(1) as f32;
    let sy_scale = sh as f32 / dh.max(1) as f32;
    out.par_chunks_exact_mut(dw).enumerate().for_each(|(y, row)| {
        let fy = ((y as f32 + 0.5) * sy_scale - 0.5).clamp(0.0, (sh.saturating_sub(1)) as f32);
        let y0 = fy.floor() as usize;
        let y1 = (y0 + 1).min(sh.saturating_sub(1));
        let wy = fy - y0 as f32;
        for x in 0..dw {
            let fx = ((x as f32 + 0.5) * sx_scale - 0.5).clamp(0.0, (sw.saturating_sub(1)) as f32);
            let x0 = fx.floor() as usize;
            let x1 = (x0 + 1).min(sw.saturating_sub(1));
            let wx = fx - x0 as f32;
            let v00 = src[y0 * sw + x0];
            let v10 = src[y0 * sw + x1];
            let v01 = src[y1 * sw + x0];
            let v11 = src[y1 * sw + x1];
            let top = v00 * (1.0 - wx) + v10 * wx;
            let bot = v01 * (1.0 - wx) + v11 * wx;
            row[x] = top * (1.0 - wy) + bot * wy;
        }
    });
    out
}

fn build_gaussian_pyramid(base: Vec<f32>, w: usize, h: usize, levels: usize) -> Vec<(Vec<f32>, usize, usize)> {
    let mut pyr = Vec::with_capacity(levels);
    pyr.push((base, w, h));
    for _ in 1..levels {
        let (ref prev, pw, ph) = pyr[pyr.len() - 1];
        if pw <= 4 || ph <= 4 {
            break;
        }
        let next = downsample_5tap(prev, pw, ph);
        pyr.push(next);
    }
    pyr
}

fn build_laplacian_pyramid(base: Vec<f32>, w: usize, h: usize, levels: usize) -> Vec<(Vec<f32>, usize, usize)> {
    let gauss = build_gaussian_pyramid(base, w, h, levels);
    let n = gauss.len();
    let mut lap = Vec::with_capacity(n);
    for l in 0..n - 1 {
        let (ref cur, cw, ch) = gauss[l];
        let (ref nxt, nw, nh) = gauss[l + 1];
        let up = upsample_bilinear(nxt, nw, nh, cw, ch);
        let diff: Vec<f32> = cur.par_iter().zip(up.par_iter()).map(|(a, b)| a - b).collect();
        lap.push((diff, cw, ch));
    }
    lap.push(gauss[n - 1].clone());
    lap
}

/// Aubry–Paris Fast Local Laplacian Pyramid tone mapper operating on log-luminance
/// with per-pixel chromaticity preservation (virtually halo-free across strong step edges).
pub fn fast_local_laplacian_tonemap(
    linear_srgb: &Rgb32FImage,
    opts: &LocalLaplacianOptions,
) -> Rgb32FImage {
    let (wu, hu) = linear_srgb.dimensions();
    let w = wu as usize;
    let h = hu as usize;
    if w == 0 || h == 0 {
        return linear_srgb.clone();
    }

    let raw = linear_srgb.as_raw();
    let bias = opts.exposure_bias.exp2();
    let mut y_in = vec![0.0f32; w * h];
    let mut max_y = 1.0f32;
    for (i, dst) in y_in.iter_mut().enumerate() {
        let p = i * 3;
        let r = if raw[p].is_finite() { (raw[p] * bias).max(0.0) } else { 0.0 };
        let g = if raw[p + 1].is_finite() { (raw[p + 1] * bias).max(0.0) } else { 0.0 };
        let b = if raw[p + 2].is_finite() { (raw[p + 2] * bias).max(0.0) } else { 0.0 };
        let y = (0.2126 * r + 0.7152 * g + 0.0722 * b).max(0.0);
        *dst = y;
        if y > max_y {
            max_y = y;
        }
    }

    let log_scale = 5.0f32;
    let log_norm = 1.0 / (1.0 + max_y * log_scale).ln().max(1e-5);
    let log_in: Vec<f32> = y_in
        .par_iter()
        .map(|&y| ((1.0 + y * log_scale).ln() * log_norm).clamp(0.0, 1.0))
        .collect();

    let gauss_in = build_gaussian_pyramid(log_in.clone(), w, h, PYRAMID_LEVELS);
    let n_levels = gauss_in.len();

    let sigma_r = opts.sigma_r.clamp(0.05, 0.5);
    let alpha = opts.alpha.clamp(0.25, 1.8);
    let hl_comp = (opts.highlight_recovery / 100.0).clamp(0.0, 1.0);
    let beta = (opts.beta * (1.0 - 0.35 * hl_comp)).clamp(0.15, 0.95);

    // Precompute N discrete remapped Laplacian pyramids
    let lap_pyramids: Vec<Vec<(Vec<f32>, usize, usize)>> = (0..NUM_DISCRETIZATION_LEVELS)
        .into_par_iter()
        .map(|k| {
            let g_k = k as f32 / (NUM_DISCRETIZATION_LEVELS - 1) as f32;
            let remapped: Vec<f32> = log_in
                .iter()
                .map(|&i_val| {
                    let d = i_val - g_k;
                    let ad = d.abs();
                    let s = d.signum();
                    if ad < sigma_r {
                        let ratio = (ad / sigma_r).max(1e-5);
                        (g_k + s * sigma_r * ratio.powf(alpha)).clamp(0.0, 1.0)
                    } else {
                        (g_k + s * (beta * (ad - sigma_r) + sigma_r)).clamp(0.0, 1.0)
                    }
                })
                .collect();
            build_laplacian_pyramid(remapped, w, h, n_levels)
        })
        .collect();

    // Interpolate output Laplacian pyramid across levels
    let mut out_lap: Vec<(Vec<f32>, usize, usize)> = Vec::with_capacity(n_levels);
    for l in 0..n_levels - 1 {
        let (ref g_lvl, lw, lh) = gauss_in[l];
        let mut level_out = vec![0.0f32; lw * lh];
        level_out
            .par_iter_mut()
            .enumerate()
            .for_each(|(i, dst)| {
                let g_val = g_lvl[i].clamp(0.0, 1.0) * (NUM_DISCRETIZATION_LEVELS - 1) as f32;
                let k0 = (g_val.floor() as usize).min(NUM_DISCRETIZATION_LEVELS - 2);
                let k1 = k0 + 1;
                let t = g_val - k0 as f32;
                let c0 = lap_pyramids[k0][l].0[i];
                let c1 = lap_pyramids[k1][l].0[i];
                *dst = c0 * (1.0 - t) + c1 * t;
            });
        out_lap.push((level_out, lw, lh));
    }
    // Coarsest residual matches the effective step slope `beta_global` anchored at 0 so the
    // coarsest Gaussian blur cancels against the remapped Laplacian bands without halo bleed.
    let (ref base_coarsest, bw, bh) = gauss_in[n_levels - 1];
    let beta_global = (beta + (1.0 - beta) * sigma_r).clamp(0.2, 1.0);
    let coarse_out: Vec<f32> = base_coarsest
        .iter()
        .map(|&v| (v * beta_global).clamp(0.0, 1.0))
        .collect();
    out_lap.push((coarse_out, bw, bh));

    // Collapse top-down
    let mut current = out_lap[n_levels - 1].0.clone();
    let (mut cw, mut ch) = (out_lap[n_levels - 1].1, out_lap[n_levels - 1].2);
    for l in (0..n_levels - 1).rev() {
        let (ref detail, dw, dh) = out_lap[l];
        let up = upsample_bilinear(&current, cw, ch, dw, dh);
        current = up
            .par_iter()
            .zip(detail.par_iter())
            .map(|(u, d)| u + d)
            .collect();
        cw = dw;
        ch = dh;
    }

    // Reconstruct display-encoded sRGB output preserving per-pixel chromaticity ratios
    let srgb_curve = OutputColorSpace::Srgb.curve();
    let shadow_boost = (opts.shadow_lift / 100.0).clamp(0.0, 1.0) * 0.14;
    let mut out_raw = vec![0.0f32; raw.len()];
    out_raw
        .par_chunks_exact_mut(3)
        .enumerate()
        .for_each(|(i, dst)| {
            let p = i * 3;
            let l_norm = (current[i] / beta_global).clamp(0.0, 1.0);
            let l_disp = (l_norm + shadow_boost * (1.0 - l_norm) * l_norm.sqrt()).clamp(0.0, 1.0);
            let y_disp = srgb_curve.decode(l_disp).max(0.0);
            let yi = y_in[i];
            let ratio = if yi > 1e-6 { y_disp / yi } else { 0.0 };
            for c in 0..3 {
                let rv = raw[p + c];
                let lin = if rv.is_finite() {
                    ((rv * bias).max(0.0) * ratio).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                dst[c] = srgb_encode(lin);
            }
        });

    Rgb32FImage::from_raw(wu, hu, out_raw).unwrap_or_else(|| linear_srgb.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn test_local_laplacian_suppresses_step_edge_halo_overshoot() {
        let (w, h) = (64u32, 64u32);
        let mut hdr = Rgb32FImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let val = if x < 32 { 0.04 } else { 4.5 };
                hdr.put_pixel(x, y, Rgb([val, val, val]));
            }
        }

        let mapped = fast_local_laplacian_tonemap(
            &hdr,
            &LocalLaplacianOptions {
                sigma_r: 0.16,
                alpha: 0.75,
                beta: 0.45,
                exposure_bias: 0.0,
                shadow_lift: 20.0,
                highlight_recovery: 50.0,
            },
        );

        let dark_far = mapped.get_pixel(8, 32)[0];
        let dark_edge = mapped.get_pixel(29, 32)[0];
        let bright_edge = mapped.get_pixel(34, 32)[0];
        let bright_far = mapped.get_pixel(56, 32)[0];

        assert!(
            bright_far > dark_far + 0.35,
            "Compressed HDR step should retain clear contrast ({} vs {})",
            bright_far,
            dark_far
        );
        // Halo guard: near the dark edge shouldn't dive far below dark plateau,
        // and near the bright edge shouldn't spike far above bright plateau.
        assert!(
            (dark_edge - dark_far).abs() < 0.06,
            "Dark side should not suffer strong halo undershoot (edge={}, far={})",
            dark_edge,
            dark_far
        );
        assert!(
            bright_edge <= bright_far + 0.05,
            "Bright side should not suffer halo overshoot (edge={}, far={})",
            bright_edge,
            bright_far
        );
    }
}
