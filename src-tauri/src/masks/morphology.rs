use image::GrayImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

pub(crate) fn grayscale_dilate(image: &GrayImage, k: u8) -> GrayImage {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return image.clone();
    }
    let w = width as usize;
    let h = height as usize;
    let r = k as i32;
    let src = image.as_raw();

    let mut temp = vec![0u8; w * h];
    let mut out = vec![0u8; w * h];

    temp.par_chunks_mut(w)
        .zip(src.par_chunks(w))
        .for_each(|(temp_row, src_row)| {
            for (x, slot) in temp_row.iter_mut().enumerate() {
                let mut max_val = 0;
                let start = (x as i32 - r).max(0) as usize;
                let end = (x as i32 + r).min((w - 1) as i32) as usize;
                for &value in &src_row[start..=end] {
                    max_val = max_val.max(value);
                }
                *slot = max_val;
            }
        });

    let r = r as usize;
    out.par_chunks_mut(w).enumerate().for_each(|(y, out_row)| {
        let start = y.saturating_sub(r);
        let end = (y + r).min(h - 1);
        out_row.copy_from_slice(&temp[start * w..(start + 1) * w]);
        for yi in (start + 1)..=end {
            for (o, &t) in out_row.iter_mut().zip(&temp[yi * w..(yi + 1) * w]) {
                *o = (*o).max(t);
            }
        }
    });

    GrayImage::from_raw(width, height, out).unwrap()
}

pub(crate) fn grayscale_erode(image: &GrayImage, k: u8) -> GrayImage {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return image.clone();
    }
    let w = width as usize;
    let h = height as usize;
    let r = k as i32;
    let src = image.as_raw();

    let mut temp = vec![0u8; w * h];
    let mut out = vec![0u8; w * h];

    temp.par_chunks_mut(w)
        .zip(src.par_chunks(w))
        .for_each(|(temp_row, src_row)| {
            for (x, slot) in temp_row.iter_mut().enumerate() {
                let mut min_val = 255;
                let start = (x as i32 - r).max(0) as usize;
                let end = (x as i32 + r).min((w - 1) as i32) as usize;
                for &value in &src_row[start..=end] {
                    min_val = min_val.min(value);
                }
                *slot = min_val;
            }
        });

    let r = r as usize;
    out.par_chunks_mut(w).enumerate().for_each(|(y, out_row)| {
        let start = y.saturating_sub(r);
        let end = (y + r).min(h - 1);
        out_row.copy_from_slice(&temp[start * w..(start + 1) * w]);
        for yi in (start + 1)..=end {
            for (o, &t) in out_row.iter_mut().zip(&temp[yi * w..(yi + 1) * w]) {
                *o = (*o).min(t);
            }
        }
    });

    GrayImage::from_raw(width, height, out).unwrap()
}

fn gaussian_kernel(sigma: f32) -> Vec<f32> {
    let radius = (2.0 * sigma).ceil() as usize;
    let pdf =
        |x: f32| (sigma * (2.0 * PI).sqrt()).recip() * (-x.powi(2) / (2.0 * sigma.powi(2))).exp();
    let mut kernel = vec![0.0f32; 2 * radius + 1];
    for i in 0..=radius {
        let value = pdf(i as f32);
        kernel[radius + i] = value;
        kernel[radius - i] = value;
    }
    let sum: f32 = kernel.iter().sum();
    kernel.iter_mut().for_each(|k| *k /= sum);
    kernel
}

fn saturate_u8(value: f32) -> u8 {
    if value < 255.0 {
        if value > 0.0 { value as u8 } else { 0 }
    } else {
        255
    }
}

pub(crate) fn gaussian_blur_mask(mask: &GrayImage, sigma: f32) -> GrayImage {
    let (w, h) = (mask.width() as usize, mask.height() as usize);
    if w == 0 || h == 0 {
        return mask.clone();
    }
    let kernel = gaussian_kernel(sigma);
    let radius = (kernel.len() / 2) as isize;
    let tap = |center: usize, offset: usize, len: usize| {
        (center as isize + offset as isize - radius).clamp(0, len as isize - 1) as usize
    };

    let mut horizontal = vec![0u8; w * h];
    horizontal
        .par_chunks_exact_mut(w)
        .zip(mask.as_raw().par_chunks_exact(w))
        .for_each(|(out, row)| {
            for (x, value) in out.iter_mut().enumerate() {
                let mut acc = 0.0f32;
                for (i, k) in kernel.iter().enumerate() {
                    acc += row[tap(x, i, w)] as f32 * k;
                }
                *value = saturate_u8(acc);
            }
        });

    let mut blurred = vec![0u8; w * h];
    blurred
        .par_chunks_exact_mut(w)
        .enumerate()
        .for_each(|(y, out)| {
            let mut acc = vec![0.0f32; w];
            for (i, k) in kernel.iter().enumerate() {
                let row = &horizontal[tap(y, i, h) * w..][..w];
                for (a, &v) in acc.iter_mut().zip(row) {
                    *a += v as f32 * k;
                }
            }
            for (value, a) in out.iter_mut().zip(acc) {
                *value = saturate_u8(a);
            }
        });

    GrayImage::from_raw(w as u32, h as u32, blurred).expect("mask buffer size")
}

pub fn apply_grow_and_feather(
    mask: &mut GrayImage,
    grow: f32,
    feather: f32,
    width: u32,
    height: u32,
) {
    let base_dimension = width.min(height) as f32;

    if grow.abs() > 0.01 {
        const MAX_GROW_PERCENTAGE: f32 = 0.01;
        let grow_pixels = (grow / 100.0) * base_dimension * MAX_GROW_PERCENTAGE;

        let amount = grow_pixels.abs().round() as u8;

        if amount > 0 {
            if grow_pixels > 0.0 {
                *mask = grayscale_dilate(mask, amount);
            } else {
                *mask = grayscale_erode(mask, amount);
            }
        }
    }

    if feather > 0.0 {
        const MAX_FEATHER_SIGMA_PERCENTAGE: f32 = 0.005;
        let sigma = (feather / 100.0) * base_dimension * MAX_FEATHER_SIGMA_PERCENTAGE;

        if sigma > 0.01 {
            *mask = gaussian_blur_mask(mask, sigma);
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct GrowFeatherParameters {
    #[serde(default)]
    pub grow: f32,
    #[serde(default)]
    pub feather: f32,
}

pub fn for_each_mask_row(mask: &mut GrayImage, fill_row: impl Fn(u32, &mut [u8]) + Sync) {
    let width = mask.width() as usize;
    if width == 0 {
        return;
    }
    mask.par_chunks_mut(width)
        .enumerate()
        .for_each(|(y, row)| fill_row(y as u32, row));
}

pub(crate) fn refine_mask_bitmap(
    mask: &mut GrayImage,
    expand: f32,
    feather: f32,
    contrast: f32,
    scale: f32,
) {
    let s = scale.max(0.15);
    if expand.abs() > 0.2 {
        let radius = ((expand.abs() / 100.0) * 32.0 * s).round().clamp(1.0, 48.0) as u8;
        if expand > 0.0 {
            *mask = grayscale_dilate(mask, radius);
        } else {
            *mask = grayscale_erode(mask, radius);
        }
    }

    if feather > 0.2 {
        let sigma = ((feather / 100.0) * 24.0 * s).clamp(0.4, 32.0);
        *mask = gaussian_blur_mask(mask, sigma);
    }

    if contrast.abs() > 0.5 {
        let k = (contrast / 100.0).clamp(-1.0, 1.0);
        if k > 0.0 {
            let gain = 1.0 + k * 7.0;
            let norm = (gain * 0.5).tanh().max(1e-5);
            mask.par_iter_mut().for_each(|px| {
                let x = *px as f32 / 255.0;
                let shaped = 0.5 + 0.5 * (gain * (x - 0.5)).tanh() / norm;
                *px = (shaped.clamp(0.0, 1.0) * 255.0).round() as u8;
            });
        } else {
            let slope = 1.0 + k * 0.7;
            mask.par_iter_mut().for_each(|px| {
                let x = *px as f32 / 255.0;
                let shaped = 0.5 + (x - 0.5) * slope;
                *px = (shaped.clamp(0.0, 1.0) * 255.0).round() as u8;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refine_mask_expand_grows_and_contract_shrinks() {
        let mut base = GrayImage::new(32, 32);
        for y in 12..20 {
            for x in 12..20 {
                base.put_pixel(x, y, image::Luma([255]));
            }
        }
        let mut expanded = base.clone();
        refine_mask_bitmap(&mut expanded, 50.0, 0.0, 0.0, 1.0);
        let mut contracted = base.clone();
        refine_mask_bitmap(&mut contracted, -50.0, 0.0, 0.0, 1.0);

        let count = |img: &GrayImage| img.as_raw().iter().filter(|&&v| v > 127).count();
        assert!(count(&expanded) > count(&base));
        assert!(count(&contracted) < count(&base));
    }

    #[test]
    fn refine_mask_contrast_steepens_midtones() {
        let mut img = GrayImage::from_raw(3, 1, vec![64, 128, 192]).unwrap();
        refine_mask_bitmap(&mut img, 0.0, 0.0, 100.0, 1.0);
        assert!(img.get_pixel(0, 0)[0] < 32);
        assert!((125..=132).contains(&img.get_pixel(1, 0)[0]));
        assert!(img.get_pixel(2, 0)[0] > 223);
    }
}
