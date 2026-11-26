use super::raster::{Raster, pyr_down, pyr_up};
use rayon::prelude::*;

const BLEND_STRENGTH: f64 = 0.05;
const MAX_BANDS: usize = 7;
const WEIGHT_EPSILON: f32 = 1e-6;

pub fn band_count(width: usize, height: usize) -> usize {
    let blend_width = ((width * height) as f64).sqrt() * BLEND_STRENGTH;
    if blend_width < 2.0 {
        return 1;
    }
    ((blend_width.log2().ceil() as usize).saturating_sub(1)).clamp(1, MAX_BANDS)
}

pub struct MultiBandBlender {
    bands: usize,
    levels: Vec<Raster>,
}

impl MultiBandBlender {
    pub fn alignment(bands: usize) -> usize {
        1 << bands
    }

    pub fn new(width: usize, height: usize, bands: usize) -> Self {
        let align = Self::alignment(bands);
        let mut w = width.div_ceil(align) * align;
        let mut h = height.div_ceil(align) * align;
        let mut levels = Vec::with_capacity(bands + 1);
        for _ in 0..=bands {
            levels.push(Raster::new(w, h, 4));
            w /= 2;
            h /= 2;
        }
        MultiBandBlender { bands, levels }
    }

    pub fn feed(&mut self, color: &Raster, weight: &[f32], x0: usize, y0: usize) {
        let align = Self::alignment(self.bands);
        assert!(
            x0.is_multiple_of(align) && y0.is_multiple_of(align),
            "blend layers must be aligned to the pyramid"
        );
        let w = color.width.div_ceil(align) * align;
        let h = color.height.div_ceil(align) * align;
        let w = w.min(self.levels[0].width - x0);
        let h = h.min(self.levels[0].height - y0);

        let mut padded = Raster::new(w, h, 3);
        let mut padded_weight = Raster::new(w, h, 1);
        padded
            .data
            .par_chunks_mut(w * 3)
            .zip(padded_weight.data.par_chunks_mut(w))
            .enumerate()
            .for_each(|(y, (row, weight_row))| {
                let sy = y.min(color.height - 1);
                for x in 0..w {
                    let sx = x.min(color.width - 1);
                    row[x * 3..x * 3 + 3].copy_from_slice(color.pixel(sx, sy));
                    if x < color.width && y < color.height {
                        weight_row[x] = weight[y * color.width + x];
                    }
                }
            });

        let mut gaussian = padded;
        let mut weights = padded_weight;
        for level in 0..=self.bands {
            let (lap, next_gaussian) = if level < self.bands {
                let down = pyr_down(&gaussian);
                let up = pyr_up(&down, gaussian.width, gaussian.height);
                let mut lap = gaussian;
                lap.data
                    .par_iter_mut()
                    .zip(up.data.par_iter())
                    .for_each(|(l, u)| *l -= u);
                (lap, Some(down))
            } else {
                (gaussian, None)
            };

            let lx = x0 >> level;
            let ly = y0 >> level;
            let target = &mut self.levels[level];
            let tw = target.width;
            let lw = lap.width.min(tw - lx);
            let lh = lap.height.min(target.height - ly);
            target.data[ly * tw * 4..(ly + lh) * tw * 4]
                .par_chunks_mut(tw * 4)
                .enumerate()
                .for_each(|(y, row)| {
                    for x in 0..lw {
                        let wv = weights.data[y * weights.width + x];
                        if wv <= 0.0 {
                            continue;
                        }
                        let src = lap.pixel(x, y);
                        let dst = &mut row[(lx + x) * 4..(lx + x) * 4 + 4];
                        dst[0] += src[0] * wv;
                        dst[1] += src[1] * wv;
                        dst[2] += src[2] * wv;
                        dst[3] += wv;
                    }
                });

            match next_gaussian {
                Some(down) => {
                    gaussian = down;
                    weights = pyr_down(&weights);
                }
                None => break,
            }
        }
    }

    pub fn finish(mut self) -> Raster {
        for level in self.levels.iter_mut() {
            level.data.par_chunks_mut(4).for_each(|p| {
                if p[3] > WEIGHT_EPSILON {
                    let inv = 1.0 / p[3];
                    p[0] *= inv;
                    p[1] *= inv;
                    p[2] *= inv;
                } else {
                    p[0] = 0.0;
                    p[1] = 0.0;
                    p[2] = 0.0;
                }
            });
        }

        let to_rgb = |level: &Raster| {
            let data = level
                .data
                .par_chunks(4)
                .flat_map_iter(|p| [p[0], p[1], p[2]])
                .collect();
            Raster::from_data(level.width, level.height, 3, data)
        };

        let mut result = to_rgb(&self.levels[self.bands]);
        for level in (0..self.bands).rev() {
            let target = &self.levels[level];
            let mut up = pyr_up(&result, target.width, target.height);
            up.data
                .par_chunks_mut(3)
                .zip(target.data.par_chunks(4))
                .for_each(|(u, t)| {
                    u[0] += t[0];
                    u[1] += t[1];
                    u[2] += t[2];
                });
            result = up;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multiband_wraparound_matches_left_and_right_edges() {
        let width = 64usize;
        let height = 16usize;
        let bands = 2usize;
        let align = MultiBandBlender::alignment(bands); // 4
        let halo = 2 * align; // 8
        let ext_width = width + 2 * halo;

        let run_periodic = |seam_shift: usize| {
            let mut img_a = Raster::new(ext_width, height, 3);
            let mut img_b = Raster::new(ext_width, height, 3);
            let mut w_a = vec![0.0f32; ext_width * height];
            let mut w_b = vec![0.0f32; ext_width * height];

            for y in 0..height {
                for gx in 0..ext_width {
                    let wx = ((gx as isize - halo as isize).rem_euclid(width as isize)) as usize;
                    let shifted = (wx + width - seam_shift) % width;
                    let phase = (shifted as f32 / width as f32) * std::f32::consts::TAU;
                    let base = 0.5 + 0.1 * phase.cos();
                    let va = [base + 0.15, base, base - 0.10];
                    let vb = [base - 0.15, base, base + 0.10];
                    let idx = (y * ext_width + gx) * 3;
                    img_a.data[idx..idx + 3].copy_from_slice(&va);
                    img_b.data[idx..idx + 3].copy_from_slice(&vb);
                    let use_a = shifted < width / 2;
                    w_a[y * ext_width + gx] = if use_a { 1.0 } else { 0.0 };
                    w_b[y * ext_width + gx] = if use_a { 0.0 } else { 1.0 };
                }
            }

            let mut blender = MultiBandBlender::new(ext_width, height, bands);
            blender.feed(&img_a, &w_a, 0, 0);
            blender.feed(&img_b, &w_b, 0, 0);
            blender.finish()
        };

        // Case 1: Continuous wrap across x=0 <-> x=width-1 (seam at width/2) -> edges must match
        let continuous = run_periodic(width / 4);
        for y in 0..height {
            let left = continuous.pixel(halo, y);
            let right = continuous.pixel(halo + width - 1, y);
            for c in 0..3 {
                assert!(
                    (left[c] - right[c]).abs() < 0.015,
                    "Continuous wrap mismatch at row {} ch {}: left={} right={}",
                    y,
                    c,
                    left[c],
                    right[c]
                );
            }
        }

        // Case 2: Seam placed directly at x=0 <-> x=width-1 must match the same seam shifted into the interior
        let wrap_seam = run_periodic(0);
        let interior_seam = run_periodic(width / 2);
        for y in 0..height {
            let wrap_p0 = wrap_seam.pixel(halo, y);
            let int_p0 = interior_seam.pixel(halo + width / 2, y);
            for c in 0..3 {
                assert!(
                    (wrap_p0[c] - int_p0[c]).abs() < 1e-3,
                    "Wrap seam differs from interior seam at row {} ch {}: wrap={} interior={}",
                    y,
                    c,
                    wrap_p0[c],
                    int_p0[c]
                );
            }
        }
    }
}
