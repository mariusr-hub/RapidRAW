use crate::merge::raster::{Raster, gaussian_blur, map_pixels, pyr_down, pyr_up};
use crate::render::color_space::luminance;
use rayon::prelude::*;
use serde::Deserialize;

const MAX_LEVELS: usize = 8;
const MIN_LEVEL_DIMENSION: usize = 16;
const NOISE_SCALE: f32 = 4.0;
const MIN_VALID: f32 = 1e-3;
const DEPTH_LEVEL: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Method {
    #[default]
    Detail,
    Smooth,
    DepthMap,
}

impl Method {
    fn pooling(self) -> f32 {
        match self {
            Method::Detail | Method::DepthMap => 1.0,
            Method::Smooth => 4.0,
        }
    }

    fn exponent(self, level: usize) -> f32 {
        let fine = match self {
            Method::Detail | Method::DepthMap => 12.0,
            Method::Smooth => 8.0,
        };
        (fine * 0.75f32.powi(level as i32)).max(3.0)
    }
}

pub fn level_count(width: usize, height: usize) -> usize {
    let mut count = 1;
    let (mut w, mut h) = (width, height);
    while count < MAX_LEVELS && w.min(h) > 2 * MIN_LEVEL_DIMENSION {
        w = w.div_ceil(2);
        h = h.div_ceil(2);
        count += 1;
    }
    count
}

fn luma(image: &Raster) -> Raster {
    map_pixels(image, 1, |p, o| o[0] = luminance(p))
}

fn subtract(a: &Raster, b: &Raster) -> Raster {
    let mut out = a.clone();
    out.data
        .par_iter_mut()
        .zip(b.data.par_iter())
        .for_each(|(o, v)| *o -= v);
    out
}

fn squared(a: &Raster) -> Raster {
    let mut out = a.clone();
    out.data.par_iter_mut().for_each(|v| *v *= *v);
    out
}

fn detail_bands(luma: &Raster) -> (Raster, Raster) {
    let g1 = pyr_down(luma);
    let g2 = pyr_down(&g1);
    let d0 = subtract(luma, &pyr_up(&g1, luma.width, luma.height));
    let d1 = subtract(&g1, &pyr_up(&g2, g1.width, g1.height));
    (d0, d1)
}

fn focus_measure(luma: &Raster, pooling: f32) -> Raster {
    let (d0, d1) = detail_bands(luma);
    let e0 = gaussian_blur(&squared(&d0), pooling);
    let e1 = gaussian_blur(&squared(&d1), pooling);
    let mut measure = pyr_up(&e1, luma.width, luma.height);
    measure
        .data
        .par_iter_mut()
        .zip(e0.data.par_iter())
        .for_each(|(m, e)| *m += e);
    measure
}

pub fn noise_floor(image: &Raster) -> f32 {
    let (d0, _) = detail_bands(&luma(image));
    let mut magnitudes: Vec<f32> = d0.data.iter().map(|v| v.abs()).collect();
    if magnitudes.is_empty() {
        return 1e-6;
    }
    let middle = magnitudes.len() / 2;
    let median = *magnitudes
        .select_nth_unstable_by(middle, |a, b| a.total_cmp(b))
        .1;
    let sigma = median / 0.6745;
    (NOISE_SCALE * sigma * sigma).max(1e-10)
}

pub struct Contribution {
    bands: Vec<Raster>,
    log_weights: Vec<Vec<f32>>,
    measure: Option<Raster>,
}

pub fn prepare(
    image: &Raster,
    valid: &[f32],
    method: Method,
    noise: f32,
    keep_measure: bool,
) -> Contribution {
    let levels = level_count(image.width, image.height);
    let measure = focus_measure(&luma(image), method.pooling());

    let mut gaussians = vec![image.clone()];
    let mut measures = vec![measure];
    let mut validity = vec![Raster::from_data(
        image.width,
        image.height,
        1,
        valid.to_vec(),
    )];
    for _ in 1..levels {
        gaussians.push(pyr_down(gaussians.last().unwrap()));
        measures.push(pyr_down(measures.last().unwrap()));
        validity.push(pyr_down(validity.last().unwrap()));
    }

    let mut bands = Vec::with_capacity(levels);
    for l in 0..levels {
        if l + 1 < levels {
            let (w, h) = (gaussians[l].width, gaussians[l].height);
            bands.push(subtract(&gaussians[l], &pyr_up(&gaussians[l + 1], w, h)));
        } else {
            bands.push(gaussians[l].clone());
        }
    }

    let log_weights = (0..levels)
        .map(|l| {
            let exponent = method.exponent(l);
            measures[l]
                .data
                .par_iter()
                .zip(validity[l].data.par_iter())
                .map(|(&m, &v)| {
                    if v < MIN_VALID {
                        f32::NEG_INFINITY
                    } else {
                        exponent * (m.max(0.0) / noise).ln_1p() + 4.0 * v.min(1.0).ln()
                    }
                })
                .collect()
        })
        .collect();

    let depth_level = DEPTH_LEVEL.min(levels - 1);
    Contribution {
        bands,
        log_weights,
        measure: keep_measure.then(|| measures.swap_remove(depth_level)),
    }
}

struct Level {
    width: usize,
    height: usize,
    numerator: Vec<f32>,
    denominator: Vec<f32>,
    maximum: Vec<f32>,
}

pub struct Depth {
    pub width: usize,
    pub height: usize,
    pub labels: Vec<u16>,
    pub best: Vec<f32>,
}

pub struct Fusion {
    levels: Vec<Level>,
    depth: Option<Depth>,
}

impl Fusion {
    pub fn new(width: usize, height: usize, track_depth: bool) -> Self {
        let count = level_count(width, height);
        let mut levels = Vec::with_capacity(count);
        let (mut w, mut h) = (width, height);
        for _ in 0..count {
            levels.push(Level {
                width: w,
                height: h,
                numerator: vec![0.0; w * h * 3],
                denominator: vec![0.0; w * h],
                maximum: vec![f32::NEG_INFINITY; w * h],
            });
            w = w.div_ceil(2);
            h = h.div_ceil(2);
        }
        let depth_level = &levels[DEPTH_LEVEL.min(count - 1)];
        let (dw, dh) = (depth_level.width, depth_level.height);
        Fusion {
            depth: track_depth.then(|| Depth {
                width: dw,
                height: dh,
                labels: vec![0; dw * dh],
                best: vec![f32::NEG_INFINITY; dw * dh],
            }),
            levels,
        }
    }

    pub fn accumulate(&mut self, index: usize, contribution: Contribution) {
        assert_eq!(
            contribution.bands.len(),
            self.levels.len(),
            "contribution must match the fusion pyramid"
        );
        for ((level, band), log_weights) in self
            .levels
            .iter_mut()
            .zip(&contribution.bands)
            .zip(&contribution.log_weights)
        {
            assert_eq!(band.width, level.width, "band width must match its level");
            level
                .numerator
                .par_chunks_mut(3)
                .zip(level.denominator.par_iter_mut())
                .zip(level.maximum.par_iter_mut())
                .zip(band.data.par_chunks(3))
                .zip(log_weights.par_iter())
                .for_each(|((((num, den), max), value), &lw)| {
                    if lw == f32::NEG_INFINITY {
                        return;
                    }
                    let weight = if lw > *max {
                        let scale = (*max - lw).exp();
                        num.iter_mut().for_each(|n| *n *= scale);
                        *den *= scale;
                        *max = lw;
                        1.0
                    } else {
                        (lw - *max).exp()
                    };
                    for c in 0..3 {
                        num[c] += weight * value[c];
                    }
                    *den += weight;
                });
        }
        if let (Some(depth), Some(measure)) = (&mut self.depth, &contribution.measure) {
            let log_weights = &contribution.log_weights[DEPTH_LEVEL.min(self.levels.len() - 1)];
            depth
                .best
                .par_iter_mut()
                .zip(depth.labels.par_iter_mut())
                .zip(measure.data.par_iter())
                .zip(log_weights.par_iter())
                .for_each(|(((b, l), &m), &lw)| {
                    if lw > f32::NEG_INFINITY && m > *b {
                        *b = m;
                        *l = index as u16;
                    }
                });
        }
    }

    pub fn finish(self) -> (Raster, Option<Depth>) {
        let mut result: Option<Raster> = None;
        for level in self.levels.into_iter().rev() {
            let mut band = level.numerator;
            band.par_chunks_mut(3)
                .zip(level.denominator.par_iter())
                .for_each(|(p, &d)| {
                    let inv = if d > 0.0 { 1.0 / d } else { 0.0 };
                    p.iter_mut().for_each(|v| *v *= inv);
                });
            let band = Raster::from_data(level.width, level.height, 3, band);
            result = Some(match result {
                None => band,
                Some(coarse) => {
                    let mut up = pyr_up(&coarse, level.width, level.height);
                    up.data
                        .par_iter_mut()
                        .zip(band.data.par_iter())
                        .for_each(|(u, b)| *u += b);
                    up
                }
            });
        }
        (result.expect("fusion has at least one level"), self.depth)
    }
}

#[inline]
pub fn depth_slice_weight(frame_index: usize, depth_index: f32) -> f32 {
    let diff = frame_index as f32 - depth_index;
    (-0.5 * (diff / 0.65).powi(2)).exp()
}

pub fn compute_regularized_depth_index(
    frames: &[(&Raster, &[f32])],
) -> (Vec<f32>, Depth) {
    let (w, h) = (frames[0].0.width, frames[0].0.height);
    let len = w * h;
    let max_idx = frames.len().saturating_sub(1);
    let measures: Vec<Raster> = frames
        .par_iter()
        .map(|(img, _)| focus_measure(&luma(img), 2.0))
        .collect();
    let guide_luma = luma(frames[frames.len() / 2].0);

    let mut winner = vec![0u16; len];
    let mut best_scores = vec![0.0f32; len];
    for i in 0..len {
        let mut best_k = 0usize;
        let mut best_m = f32::NEG_INFINITY;
        for (k, (meas, (_, valid))) in measures.iter().zip(frames.iter()).enumerate() {
            if valid[i] < MIN_VALID {
                continue;
            }
            let m = meas.data[i];
            if m > best_m {
                best_m = m;
                best_k = k;
            }
        }
        winner[i] = best_k as u16;
        best_scores[i] = best_m.max(0.0);
    }

    // 3x3 median filter on winner depth index
    let mut median_idx = vec![0.0f32; len];
    median_idx
        .par_chunks_exact_mut(w)
        .enumerate()
        .for_each(|(y, out_row)| {
            let mut window = [0u16; 9];
            for x in 0..w {
                let mut count = 0usize;
                for dy in -1isize..=1 {
                    let yy = (y as isize + dy).clamp(0, h as isize - 1) as usize;
                    for dx in -1isize..=1 {
                        let xx = (x as isize + dx).clamp(0, w as isize - 1) as usize;
                        window[count] = winner[yy * w + xx];
                        count += 1;
                    }
                }
                window[..count].sort_unstable();
                out_row[x] = window[count / 2] as f32;
            }
        });

    // Edge-aware joint bilateral depth smoothing
    const RADIUS: isize = 4;
    const INV_2_SPATIAL2: f32 = 1.0 / (2.0 * 9.0);
    const INV_2_RANGE2: f32 = 1.0 / (2.0 * 0.0064);
    let mut smooth_idx = vec![0.0f32; len];
    smooth_idx
        .par_chunks_exact_mut(w)
        .enumerate()
        .for_each(|(y, out_row)| {
            for x in 0..w {
                let lc = guide_luma.data[y * w + x];
                let mut acc = 0.0f32;
                let mut w_sum = 0.0f32;
                for dy in -RADIUS..=RADIUS {
                    let yy = (y as isize + dy).clamp(0, h as isize - 1) as usize;
                    for dx in -RADIUS..=RADIUS {
                        let xx = (x as isize + dx).clamp(0, w as isize - 1) as usize;
                        let nidx = yy * w + xx;
                        let dl = guide_luma.data[nidx] - lc;
                        let ds2 = (dx * dx + dy * dy) as f32;
                        let wt = (-ds2 * INV_2_SPATIAL2 - (dl * dl) * INV_2_RANGE2).exp();
                        acc += median_idx[nidx] * wt;
                        w_sum += wt;
                    }
                }
                out_row[x] = if w_sum > 1e-6 {
                    (acc / w_sum).clamp(0.0, max_idx as f32)
                } else {
                    median_idx[y * w + x]
                };
            }
        });

    let labels = smooth_idx
        .iter()
        .map(|&d| d.round().clamp(0.0, max_idx as f32) as u16)
        .collect();
    let depth = Depth {
        width: w,
        height: h,
        labels,
        best: best_scores,
    };
    (smooth_idx, depth)
}

pub fn fuse_depth_map_from_previews(frames: &[(&Raster, &[f32])]) -> (Raster, Depth) {
    let (w, h) = (frames[0].0.width, frames[0].0.height);
    let (smooth_idx, depth) = compute_regularized_depth_index(frames);
    let mut num = vec![0.0f32; w * h * 3];
    let mut den = vec![0.0f32; w * h];

    num.par_chunks_exact_mut(w * 3)
        .zip(den.par_chunks_exact_mut(w))
        .enumerate()
        .for_each(|(y, (num_row, den_row))| {
            let row_off = y * w;
            for x in 0..w {
                let d = smooth_idx[row_off + x];
                let mut acc = [0.0f32; 3];
                let mut w_tot = 0.0f32;
                for (k, (img, valid)) in frames.iter().enumerate() {
                    if valid[row_off + x] < MIN_VALID {
                        continue;
                    }
                    let wt = depth_slice_weight(k, d) * valid[row_off + x];
                    let px = img.pixel(x, y);
                    acc[0] += px[0] * wt;
                    acc[1] += px[1] * wt;
                    acc[2] += px[2] * wt;
                    w_tot += wt;
                }
                if w_tot > 1e-6 {
                    let inv = 1.0 / w_tot;
                    num_row[x * 3] = acc[0] * inv;
                    num_row[x * 3 + 1] = acc[1] * inv;
                    num_row[x * 3 + 2] = acc[2] * inv;
                    den_row[x] = w_tot;
                }
            }
        });

    (Raster::from_data(w, h, 3, num), depth)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_depth_map_focus_fusion_suppresses_halo_and_selects_sharp_slice() {
        let (w, h) = (32usize, 32usize);
        let mk_img = |sharp_left: bool| -> Raster {
            let mut data = Vec::with_capacity(w * h * 3);
            for y in 0..h {
                for x in 0..w {
                    let v = if x < 16 {
                        if sharp_left && (x + y) % 2 == 0 {
                            0.9
                        } else if sharp_left {
                            0.2
                        } else {
                            0.55
                        }
                    } else if !sharp_left && (x + y) % 2 == 0 {
                        0.85
                    } else if !sharp_left {
                        0.15
                    } else {
                        0.50
                    };
                    data.extend_from_slice(&[v, v, v]);
                }
            }
            Raster::from_data(w, h, 3, data)
        };
        let f0 = mk_img(true);
        let f1 = mk_img(false);
        let valid = vec![1.0f32; w * h];
        let (_fused, depth) = fuse_depth_map_from_previews(&[(&f0, &valid), (&f1, &valid)]);
        assert_eq!(depth.labels[16 * w + 4], 0);
        assert_eq!(depth.labels[16 * w + 28], 1);
    }
}
