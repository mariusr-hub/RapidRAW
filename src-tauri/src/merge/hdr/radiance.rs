use crate::enhanced::hdr_deghosting::solve_tv_l1_optical_flow_level;
use crate::merge::raster::{Raster, gaussian_blur, map_pixels, max_filter};
use crate::render::color_space::luminance;
use rayon::prelude::*;

const KNEE: (f32, f32) = (0.7, 0.9);
const NOISE_FLOOR: f32 = 0.003;
const DARKEST_FALLBACK_WEIGHT: f32 = 1e-6;
const REFERENCE_FALLBACK_WEIGHT: f32 = 1e-9;
const COMPARISON_BLUR: f32 = 1.2;
const GHOST_THRESHOLD: f32 = 0.25;
const GHOST_SPREAD: usize = 4;
const MIN_RATIO_SAMPLES: usize = 200;

#[derive(Clone, Copy)]
pub struct FrameRole {
    pub exposure: f32,
    pub darkest: bool,
    pub reference: bool,
}

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn levels(image: &Raster, clip: f32, spread: usize) -> Raster {
    let level = map_pixels(image, 1, |p, o| o[0] = p[0].max(p[1]).max(p[2]) / clip);
    max_filter(&level, spread)
}

#[inline]
fn weight(role: FrameRole, level: f32, valid: f32) -> f32 {
    let usable = if role.darkest {
        1.0
    } else if role.exposure > 1.05 {
        1.0 - smoothstep(0.18, KNEE.1, level)
    } else {
        1.0 - smoothstep(KNEE.0, KNEE.1, level)
    };
    let fallback = if role.darkest {
        DARKEST_FALLBACK_WEIGHT
    } else if role.reference {
        REFERENCE_FALLBACK_WEIGHT
    } else {
        0.0
    };
    role.exposure * (usable * valid + fallback)
}

#[inline]
fn recover_saturated_darkest(rgb: [f32; 3], clip: f32) -> [f32; 3] {
    let [mut r, mut g, mut b] = rgb;
    let max_c = r.max(g).max(b);
    let soft = 0.85 * clip;
    if max_c <= soft {
        return rgb;
    }
    let outer_blend = smoothstep(soft, 1.15 * clip, max_c);
    let magenta = (r.min(b) - g).max(0.0);
    if magenta > 0.0 {
        let target_g = r.min(b) * 0.82 + (r + b) * 0.5 * 0.18;
        let correction = (target_g - g).max(0.0);
        let magenta_weight = smoothstep(0.0, 0.22, magenta / max_c.max(1e-5));
        g += correction * outer_blend * magenta_weight;
    }
    let residual = (r.min(b) - g).max(0.0);
    if residual > 0.0 {
        g += residual * outer_blend;
    }
    let new_max = r.max(g).max(b);
    let min_c = r.min(g).min(b);
    let knee = smoothstep(0.92 * clip, 1.40 * clip, new_max);
    if knee > 0.0 {
        let neutrality = (min_c / new_max.max(1e-5)).clamp(0.0, 1.0);
        let core_burn = smoothstep(1.0 * clip, 2.2 * clip, new_max);
        let desat = (knee * (neutrality * 0.85 + core_burn * 0.15)).clamp(0.0, 1.0);
        let smooth_desat = desat * desat * (3.0 - 2.0 * desat);
        r = r * (1.0 - smooth_desat) + new_max * smooth_desat;
        g = g * (1.0 - smooth_desat) + new_max * smooth_desat;
        b = b * (1.0 - smooth_desat) + new_max * smooth_desat;
    }
    [r, g, b]
}

#[inline]
fn contributes(role: FrameRole, level: f32) -> bool {
    role.darkest || level < KNEE.1
}

pub struct Accumulator {
    width: usize,
    height: usize,
    numerator: Vec<f32>,
    denominator: Vec<f32>,
}

impl Accumulator {
    pub fn new(width: usize, height: usize) -> Self {
        Accumulator {
            width,
            height,
            numerator: vec![0.0; width * height * 3],
            denominator: vec![0.0; width * height],
        }
    }

    pub fn add(
        &mut self,
        clip: f32,
        role: FrameRole,
        image: &Raster,
        valid: &[f32],
        ghost: Option<&Raster>,
    ) {
        self.add_with_override(clip, role, image, valid, ghost, None, None);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_with_override(
        &mut self,
        clip: f32,
        role: FrameRole,
        image: &Raster,
        valid: &[f32],
        ghost: Option<&Raster>,
        override_self: Option<&[f32]>,
        override_any: Option<&[f32]>,
    ) {
        assert_eq!(image.width, self.width, "frame width must match the merge");
        assert_eq!(
            image.height, self.height,
            "frame height must match the merge"
        );
        let level = levels(image, clip, 1);
        let width = self.width;
        let (gx, gy) = ghost.map_or((0.0, 0.0), |g| {
            (
                g.width as f32 / width as f32,
                g.height as f32 / self.height as f32,
            )
        });
        let inverse_exposure = 1.0 / role.exposure;
        self.numerator
            .par_chunks_mut(width * 3)
            .zip(self.denominator.par_chunks_mut(width))
            .enumerate()
            .for_each(|(y, (num, den))| {
                let mut sample = [0.0f32; 1];
                for x in 0..width {
                    let i = y * width + x;
                    let mut w = weight(role, level.data[i], valid[i]);
                    if let Some(g) = ghost {
                        g.bilinear(
                            (x as f32 + 0.5) * gx - 0.5,
                            (y as f32 + 0.5) * gy - 0.5,
                            &mut sample,
                        );
                        w *= 1.0 - sample[0].clamp(0.0, 1.0);
                    }
                    if let Some(any) = override_any
                        && i < any.len()
                    {
                        w *= (1.0 - any[i]).clamp(0.0, 1.0);
                    }
                    if let Some(ovr) = override_self
                        && i < ovr.len()
                        && ovr[i] > 0.0
                    {
                        w += ovr[i].clamp(0.0, 1.0) * 64.0 * role.exposure.max(0.01) * valid[i].max(0.05);
                    }
                    if w <= 0.0 {
                        continue;
                    }
                    let p = image.pixel(x, y);
                    let px = if role.darkest {
                        recover_saturated_darkest(
                            [p[0].max(0.0), p[1].max(0.0), p[2].max(0.0)],
                            clip,
                        )
                    } else {
                        [p[0].max(0.0), p[1].max(0.0), p[2].max(0.0)]
                    };
                    let wr = w * inverse_exposure;
                    for c in 0..3 {
                        num[x * 3 + c] += wr * px[c];
                    }
                    den[x] += w;
                }
            });
    }

    fn estimate(&self) -> Raster {
        let mut data = self.numerator.clone();
        data.par_chunks_mut(3)
            .zip(self.denominator.par_iter())
            .for_each(|(p, &d)| {
                let inv = if d > 0.0 { 1.0 / d } else { 0.0 };
                p.iter_mut().for_each(|v| *v *= inv);
            });
        Raster::from_data(self.width, self.height, 3, data)
    }

    pub fn finish(self) -> Raster {
        self.estimate()
    }
}

pub struct PreviewFrame<'a> {
    pub image: &'a Raster,
    pub valid: &'a [f32],
    pub role: FrameRole,
}

fn clean_mask(raw: Vec<f32>, width: usize, height: usize) -> Raster {
    let inverted = Raster::from_data(width, height, 1, raw.iter().map(|v| 1.0 - v).collect());
    let mut opened = max_filter(&inverted, 1);
    opened.data.iter_mut().for_each(|v| *v = 1.0 - *v);
    let dilated = max_filter(&opened, GHOST_SPREAD);
    let mut feathered = gaussian_blur(&dilated, GHOST_SPREAD as f32);
    feathered
        .data
        .iter_mut()
        .for_each(|v| *v = v.clamp(0.0, 1.0));
    feathered
}

pub fn ghost_masks(
    clip: f32,
    frames: &[PreviewFrame],
    reference: usize,
    sensitivity: crate::enhanced::hdr_fusion::DeghostSensitivity,
) -> Vec<Option<Raster>> {
    let mut masks: Vec<Option<Raster>> = (0..frames.len()).map(|_| None).collect();
    let threshold = match sensitivity {
        crate::enhanced::hdr_fusion::DeghostSensitivity::Off => return masks,
        crate::enhanced::hdr_fusion::DeghostSensitivity::Low => 0.42f32,
        crate::enhanced::hdr_fusion::DeghostSensitivity::Medium => GHOST_THRESHOLD,
        crate::enhanced::hdr_fusion::DeghostSensitivity::High => 0.14f32,
    };
    let (width, height) = (frames[0].image.width, frames[0].image.height);
    let mut order: Vec<usize> = (0..frames.len()).filter(|&k| k != reference).collect();
    order.sort_by(|&a, &b| {
        let distance = |k: usize| {
            (frames[k].role.exposure / frames[reference].role.exposure)
                .ln()
                .abs()
        };
        distance(a).total_cmp(&distance(b)).then(a.cmp(&b))
    });

    let mut accumulator = Accumulator::new(width, height);
    let base = &frames[reference];
    accumulator.add(clip, base.role, base.image, base.valid, None);
    let floor = NOISE_FLOOR * clip;

    for k in order {
        let frame = &frames[k];
        let estimate = gaussian_blur(&accumulator.estimate(), COMPARISON_BLUR);
        let radiance = gaussian_blur(frame.image, COMPARISON_BLUR);
        let level = levels(frame.image, clip, 2);
        let delta = floor / frame.role.exposure;
        let min_support = 1e-3 * frame.role.exposure.min(base.role.exposure);
        let raw: Vec<f32> = (0..width * height)
            .into_par_iter()
            .map(|i| {
                let comparable = frame.valid[i] >= 1.0
                    && accumulator.denominator[i] > min_support
                    && contributes(frame.role, level.data[i]);
                let ghost = comparable
                    && (0..3).any(|c| {
                        let measured = radiance.data[i * 3 + c] / frame.role.exposure;
                        ((measured + delta) / (estimate.data[i * 3 + c] + delta))
                            .ln()
                            .abs()
                            > threshold
                    });
                if ghost { 1.0 } else { 0.0 }
            })
            .collect();
        let mask = clean_mask(raw, width, height);
        accumulator.add(clip, frame.role, frame.image, frame.valid, Some(&mask));
        masks[k] = Some(mask);
    }
    masks
}

pub fn exposure_ratio(
    clip: f32,
    darker: (&Raster, &[f32]),
    brighter: (&Raster, &[f32]),
) -> Option<f64> {
    let floor = 4.0 * NOISE_FLOOR * clip;
    let dark_level = levels(darker.0, clip, 1);
    let bright_level = levels(brighter.0, clip, 1);
    let mut ratios: Vec<f32> = (0..darker.1.len())
        .into_par_iter()
        .filter_map(|i| {
            if darker.1[i] < 1.0 || brighter.1[i] < 1.0 {
                return None;
            }
            if dark_level.data[i] >= KNEE.0 || bright_level.data[i] >= KNEE.0 {
                return None;
            }
            let yd = luminance(darker.0.pixel_at(i));
            let yb = luminance(brighter.0.pixel_at(i));
            (yd > floor && yb > 0.0).then(|| (yb / yd).ln())
        })
        .collect();
    if ratios.len() < MIN_RATIO_SAMPLES {
        return None;
    }
    let middle = ratios.len() / 2;
    let (_, median, _) = ratios.select_nth_unstable_by(middle, |a, b| a.total_cmp(b));
    Some((*median as f64).exp())
}

fn downscale_luma_for_flow(
    image: &Raster,
    exposure: f32,
    joint_clip: f32,
    dst_w: usize,
    dst_h: usize,
) -> Vec<f32> {
    let mut luma = vec![0.0f32; dst_w * dst_h];
    let sx = image.width as f32 / dst_w.max(1) as f32;
    let sy = image.height as f32 / dst_h.max(1) as f32;
    let inv_exp = 1.0 / exposure.max(1e-4);
    let inv_clip = 1.0 / joint_clip.max(1e-5);
    for y in 0..dst_h {
        let fy = ((y as f32 + 0.5) * sy - 0.5).clamp(0.0, (image.height - 1) as f32);
        let y0 = fy.floor() as usize;
        for x in 0..dst_w {
            let fx = ((x as f32 + 0.5) * sx - 0.5).clamp(0.0, (image.width - 1) as f32);
            let x0 = fx.floor() as usize;
            let p = image.pixel(x0, y0);
            let lum = (luminance(p) * inv_exp).max(0.0).min(joint_clip);
            let norm = lum * inv_clip;
            // Jointly-clamped log luma keeps structural edges invariant across bracket stops
            // and prevents clipped skies from triggering false motion against darker exposures.
            luma[y * dst_w + x] = (1.0 + norm * 16.0).ln();
        }
    }
    luma
}

#[inline]
fn sample_flow(field: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let xc = x.clamp(0.0, (w - 1) as f32);
    let yc = y.clamp(0.0, (h - 1) as f32);
    let x0 = xc.floor() as usize;
    let y0 = yc.floor() as usize;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let fx = xc - x0 as f32;
    let fy = yc - y0 as f32;
    let v00 = field[y0 * w + x0];
    let v10 = field[y0 * w + x1];
    let v01 = field[y1 * w + x0];
    let v11 = field[y1 * w + x1];
    (v00 * (1.0 - fx) + v10 * fx) * (1.0 - fy) + (v01 * (1.0 - fx) + v11 * fx) * fy
}

/// Patch-based Optical-Flow & Structure-Guided (Forward-Backward TV-L1 + 5x5 NCC + Radiance) Ghost Rejection.
pub fn ghost_masks_optical_flow(
    clip: f32,
    frames: &[PreviewFrame],
    reference: usize,
    sensitivity: crate::enhanced::hdr_fusion::DeghostSensitivity,
) -> Vec<Option<Raster>> {
    let mut masks: Vec<Option<Raster>> = (0..frames.len()).map(|_| None).collect();
    let (tau_rad, tau_flow, tau_ncc) = match sensitivity {
        crate::enhanced::hdr_fusion::DeghostSensitivity::Off => return masks,
        crate::enhanced::hdr_fusion::DeghostSensitivity::Low => (0.40f32, 2.5f32, 0.45f32),
        crate::enhanced::hdr_fusion::DeghostSensitivity::Medium => (0.24f32, 1.4f32, 0.30f32),
        crate::enhanced::hdr_fusion::DeghostSensitivity::High => (0.14f32, 0.85f32, 0.20f32),
    };

    let (width, height) = (frames[0].image.width, frames[0].image.height);
    if width < 16 || height < 16 {
        return ghost_masks(clip, frames, reference, sensitivity);
    }

    let mut order: Vec<usize> = (0..frames.len()).filter(|&k| k != reference).collect();
    order.sort_by(|&a, &b| {
        let dist = |k: usize| (frames[k].role.exposure / frames[reference].role.exposure).ln().abs();
        dist(a).total_cmp(&dist(b)).then(a.cmp(&b))
    });

    // Flow lattice capped at <= 256px for responsive interactive preview updates
    let scale = (256.0f32 / width.max(height) as f32).min(1.0);
    let fw = ((width as f32 * scale).round() as usize).max(24);
    let fh = ((height as f32 * scale).round() as usize).max(24);
    let zeros = vec![0.0f32; fw * fh];

    let mut accumulator = Accumulator::new(width, height);
    let base = &frames[reference];
    accumulator.add(clip, base.role, base.image, base.valid, None);
    let ref_level = levels(base.image, clip, 2);
    let floor = NOISE_FLOOR * clip;

    for k in order {
        let frame = &frames[k];
        let joint_clip = 0.65 * clip / base.role.exposure.max(frame.role.exposure);
        let ref_luma = downscale_luma_for_flow(base.image, base.role.exposure, joint_clip, fw, fh);
        let target_luma =
            downscale_luma_for_flow(frame.image, frame.role.exposure, joint_clip, fw, fh);

        // 1. Forward (ref -> k) and Backward (k -> ref) TV-L1 optical flow consistency
        let (u_fwd, v_fwd) = solve_tv_l1_optical_flow_level(&ref_luma, &target_luma, fw, fh, &zeros, &zeros, 16, 30.0);
        let (u_bwd, v_bwd) = solve_tv_l1_optical_flow_level(&target_luma, &ref_luma, fw, fh, &zeros, &zeros, 16, 30.0);

        let mut flow_mismatch_grid = vec![0.0f32; fw * fh];
        let mut ncc_mismatch_grid = vec![0.0f32; fw * fh];

        for y in 0..fh {
            for x in 0..fw {
                let idx = y * fw + x;
                let uf = u_fwd[idx];
                let vf = v_fwd[idx];
                let ub = sample_flow(&u_bwd, fw, fh, x as f32 + uf, y as f32 + vf);
                let vb = sample_flow(&v_bwd, fw, fh, x as f32 + uf, y as f32 + vf);
                let consistency_err = ((uf + ub).powi(2) + (vf + vb).powi(2)).sqrt();
                let motion_mag = (uf * uf + vf * vf).sqrt();
                if consistency_err > tau_flow || motion_mag > tau_flow * 1.15 {
                    flow_mismatch_grid[idx] = 1.0;
                }

                // 2. Local 5x5 Zero-Mean Normalized Cross-Correlation (exposure-invariant structural check)
                if y >= 2 && y + 2 < fh && x >= 2 && x + 2 < fw {
                    let mut sum_r = 0.0f32;
                    let mut sum_t = 0.0f32;
                    let n_patch = 25.0f32;
                    for dy in -2i32..=2 {
                        for dx in -2i32..=2 {
                            let pi = ((y as i32 + dy) as usize) * fw + (x as i32 + dx) as usize;
                            sum_r += ref_luma[pi];
                            sum_t += target_luma[pi];
                        }
                    }
                    let mu_r = sum_r / n_patch;
                    let mu_t = sum_t / n_patch;
                    let mut cov = 0.0f32;
                    let mut var_r = 0.0f32;
                    let mut var_t = 0.0f32;
                    for dy in -2i32..=2 {
                        for dx in -2i32..=2 {
                            let pi = ((y as i32 + dy) as usize) * fw + (x as i32 + dx) as usize;
                            let dr = ref_luma[pi] - mu_r;
                            let dt = target_luma[pi] - mu_t;
                            cov += dr * dt;
                            var_r += dr * dr;
                            var_t += dt * dt;
                        }
                    }
                    let denom = (var_r * var_t).sqrt();
                    if denom > 1e-4 {
                        let ncc = (cov / denom).clamp(-1.0, 1.0);
                        if 1.0 - ncc > tau_ncc {
                            ncc_mismatch_grid[idx] = 1.0;
                        }
                    }
                }
            }
        }

        // 3. Combine with exposure-normalized radiance mismatch at preview resolution
        let estimate = gaussian_blur(&accumulator.estimate(), COMPARISON_BLUR);
        let radiance = gaussian_blur(frame.image, COMPARISON_BLUR);
        let level = levels(frame.image, clip, 2);
        let delta = floor / frame.role.exposure;
        let min_support = 1e-3 * frame.role.exposure.min(base.role.exposure);

        let raw: Vec<f32> = (0..width * height)
            .into_par_iter()
            .map(|i| {
                let x = i % width;
                let y = i / width;
                let comparable = frame.valid[i] >= 1.0
                    && accumulator.denominator[i] > min_support
                    && contributes(frame.role, level.data[i]);
                if !comparable {
                    return 0.0;
                }
                let fx = (x as f32 + 0.5) / width as f32 * fw as f32 - 0.5;
                let fy = (y as f32 + 0.5) / height as f32 * fh as f32 - 0.5;
                let flow_hit = sample_flow(&flow_mismatch_grid, fw, fh, fx, fy) > 0.35;
                let ncc_hit = sample_flow(&ncc_mismatch_grid, fw, fh, fx, fy) > 0.35;
                let is_darker = frame.role.exposure < base.role.exposure;
                let rad_diff = (0..3)
                    .map(|c| {
                        let measured = radiance.data[i * 3 + c] / frame.role.exposure;
                        ((measured + delta) / (estimate.data[i * 3 + c] + delta))
                            .ln()
                            .abs()
                    })
                    .fold(0.0f32, f32::max);
                let rad_hit = rad_diff > tau_rad;

                if is_darker && ref_level.data[i] >= 0.65 {
                    let est_luma = luminance(&estimate.data[i * 3..i * 3 + 3]);
                    let meas_luma =
                        luminance(&radiance.data[i * 3..i * 3 + 3]) / frame.role.exposure;
                    // In clipped/near-clipped reference highlights, only reject dark moving
                    // occluders; never reject recovered highlight skies or clouds.
                    if meas_luma < est_luma * 0.82 && rad_hit {
                        return 1.0;
                    }
                    return 0.0;
                }

                if !is_darker {
                    let est_luma = luminance(&estimate.data[i * 3..i * 3 + 3]);
                    if est_luma * frame.role.exposure < 4.0 * floor && rad_diff < tau_rad * 1.8 {
                        return 0.0;
                    }
                }

                if (rad_hit && (flow_hit || ncc_hit || tau_rad < 0.20)) || rad_diff > tau_rad * 2.0
                {
                    1.0
                } else {
                    0.0
                }
            })
            .collect();

        let mask = clean_mask(raw, width, height);
        accumulator.add(clip, frame.role, frame.image, frame.valid, Some(&mask));
        masks[k] = Some(mask);
    }
    masks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enhanced::hdr_fusion::DeghostSensitivity;

    #[test]
    fn test_optical_flow_ghost_mask_rejects_moving_object_and_keeps_static_bg() {
        let (w, h) = (64usize, 64usize);
        let mut ref_data = vec![0.25f32; w * h * 3];
        let mut mov_data = vec![0.25f32; w * h * 3];
        // Static textured grid across background so NCC has clean structure everywhere
        for y in 0..h {
            for x in 0..w {
                let bg = 0.2 + 0.06 * ((x as f32 * 0.4).sin() * (y as f32 * 0.4).cos());
                let idx = (y * w + x) * 3;
                ref_data[idx..idx + 3].copy_from_slice(&[bg, bg, bg]);
                mov_data[idx..idx + 3].copy_from_slice(&[bg, bg, bg]);
            }
        }
        // Bright moving object at (18, 32) in ref vs shifted to (44, 32) in target frame
        for y in 24..40 {
            for x in 12..24 {
                let idx = (y * w + x) * 3;
                ref_data[idx..idx + 3].copy_from_slice(&[0.85, 0.25, 0.15]);
            }
            for x in 38..50 {
                let idx = (y * w + x) * 3;
                mov_data[idx..idx + 3].copy_from_slice(&[0.85, 0.25, 0.15]);
            }
        }

        let r0 = Raster::from_data(w, h, 3, ref_data);
        let r1 = Raster::from_data(w, h, 3, mov_data);
        let valid = vec![1.0f32; w * h];
        let frames = [
            PreviewFrame {
                image: &r0,
                valid: &valid,
                role: FrameRole { exposure: 1.0, darkest: false, reference: true },
            },
            PreviewFrame {
                image: &r1,
                valid: &valid,
                role: FrameRole { exposure: 1.0, darkest: true, reference: false },
            },
        ];

        let masks = ghost_masks_optical_flow(1.0, &frames, 0, DeghostSensitivity::High);
        let ghost_mask = masks[1].as_ref().expect("Target frame mask generated");

        let static_corner = ghost_mask.data[4 * w + 4];
        let moved_pos = ghost_mask.data[32 * w + 44];
        assert!(
            static_corner < 0.15,
            "Static background must remain unmasked, got {}",
            static_corner
        );
        assert!(
            moved_pos > 0.65,
            "Moved object location must be rejected by optical-flow ghost mask, got {}",
            moved_pos
        );
    }

    #[test]
    fn test_optical_flow_ghost_mask_preserves_static_clipped_sky_highlight_recovery() {
        let (w, h) = (64usize, 64usize);
        let mut ref_data = vec![0.0f32; w * h * 3];
        let mut dark_data = vec![0.0f32; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) * 3;
                if y < 32 {
                    // Sky: 0 EV clips at 1.0, while -2 EV (exposure = 0.25) holds textured clouds
                    ref_data[idx..idx + 3].copy_from_slice(&[1.0, 1.0, 1.0]);
                    let cloud = 0.60 + 0.15 * ((x as f32 * 0.35).sin() * (y as f32 * 0.35).cos());
                    dark_data[idx..idx + 3].copy_from_slice(&[cloud * 0.85, cloud * 0.95, cloud]);
                } else {
                    // Ground: unclipped static texture
                    let g = 0.20 + 0.04 * ((x as f32 * 0.4).sin() * (y as f32 * 0.4).cos());
                    ref_data[idx..idx + 3].copy_from_slice(&[g, g, g]);
                    dark_data[idx..idx + 3].copy_from_slice(&[g * 0.25, g * 0.25, g * 0.25]);
                }
            }
        }
        let r_ref = Raster::from_data(w, h, 3, ref_data);
        let r_dark = Raster::from_data(w, h, 3, dark_data);
        let valid = vec![1.0f32; w * h];
        let frames = [
            PreviewFrame {
                image: &r_dark,
                valid: &valid,
                role: FrameRole { exposure: 0.25, darkest: true, reference: false },
            },
            PreviewFrame {
                image: &r_ref,
                valid: &valid,
                role: FrameRole { exposure: 1.0, darkest: false, reference: true },
            },
        ];

        let masks = ghost_masks_optical_flow(1.0, &frames, 1, DeghostSensitivity::Medium);
        let dark_mask = masks[0].as_ref().expect("Dark frame mask generated");
        let sky_mask = dark_mask.data[10 * w + 32];
        assert!(
            sky_mask < 0.1,
            "Static clipped sky in -2 EV must not be masked as ghost, got {}",
            sky_mask
        );

        let mut acc = Accumulator::new(w, h);
        acc.add(1.0, frames[1].role, frames[1].image, &valid, None);
        acc.add(1.0, frames[0].role, frames[0].image, &valid, Some(dark_mask));
        let merged = acc.finish();
        let sky_px = merged.pixel(32, 10);
        assert!(
            sky_px[2] > 2.2,
            "Merged sky should recover > 2.2 radiance from -2 EV frame, got {:?}",
            sky_px
        );
    }

    #[test]
    fn test_darkest_frame_saturation_removes_magenta_cast() {
        let (w, h) = (8usize, 8usize);
        let mut ref_data = Vec::with_capacity(w * h * 3);
        let mut dark_data = Vec::with_capacity(w * h * 3);
        for _ in 0..(w * h) {
            ref_data.extend_from_slice(&[1.0, 1.0, 1.0]);
            dark_data.extend_from_slice(&[1.8, 0.98, 1.4]);
        }
        let r_ref = Raster::from_data(w, h, 3, ref_data);
        let r_dark = Raster::from_data(w, h, 3, dark_data);
        let valid = vec![1.0f32; w * h];
        let mut acc = Accumulator::new(w, h);
        acc.add(
            1.0,
            FrameRole { exposure: 1.0, darkest: false, reference: true },
            &r_ref,
            &valid,
            None,
        );
        acc.add(
            1.0,
            FrameRole { exposure: 0.25, darkest: true, reference: false },
            &r_dark,
            &valid,
            None,
        );
        let merged = acc.finish();
        let px = merged.pixel(4, 4);
        assert!(
            px[1] >= px[0].min(px[2]) - 0.05,
            "Saturated darkest frame must not produce magenta cast: {:?}",
            px
        );
    }

    #[test]
    fn test_hdr_override_brush_locks_chosen_bracket() {
        let (w, h) = (16usize, 16usize);
        let r_ref = Raster::from_data(w, h, 3, vec![0.1; w * h * 3]);
        let r_alt = Raster::from_data(w, h, 3, vec![0.9; w * h * 3]);
        let valid = vec![1.0f32; w * h];
        let mut override_alt = vec![0.0f32; w * h];
        for y in 4..12 {
            for x in 4..12 {
                override_alt[y * w + x] = 1.0;
            }
        }
        let mut acc = Accumulator::new(w, h);
        acc.add_with_override(
            1.0,
            FrameRole { exposure: 1.0, darkest: false, reference: true },
            &r_ref,
            &valid,
            None,
            None,
            Some(&override_alt),
        );
        acc.add_with_override(
            1.0,
            FrameRole { exposure: 1.0, darkest: false, reference: false },
            &r_alt,
            &valid,
            None,
            Some(&override_alt),
            Some(&override_alt),
        );
        let merged = acc.finish();
        assert!(merged.pixel(8, 8)[0] > 0.85, "painted region should lock alt bracket");
        assert!(merged.pixel(1, 1)[0] < 0.65);
    }
}
