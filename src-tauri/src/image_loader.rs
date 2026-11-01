use crate::Cursor;
use crate::app_settings::{AppSettings, load_settings};
use crate::app_state::{AppState, LoadedImage};
use crate::baseline_exposure::BaselineOptions;
use crate::exif_processing;
pub use crate::file_management::{
    SafeFileBuffer, is_removable_or_remote_drive, parse_virtual_path, read_file_mapped,
    read_file_safe,
};
use crate::formats::is_raw_file;
use crate::image_processing::ImageMetadata;
use crate::image_processing::{
    apply_orientation, apply_srgb_to_linear, effective_baseline_exposure,
    remove_raw_artifacts_and_enhance,
};
use crate::mask_generation::{MaskDefinition, SubMask, generate_mask_bitmap};
use crate::white_balance::WhiteBalance;
use anyhow::{Context, Result, anyhow};
use base64::{Engine as _, engine::general_purpose};
use exif::{Reader as ExifReader, Tag};
use image::{DynamicImage, GenericImageView, ImageReader, imageops};
use rawler::Orientation;
use rayon::prelude::*;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::panic;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Instant;

#[derive(serde::Serialize)]
pub struct LoadImageResult {
    pub width: u32,
    pub height: u32,
    pub metadata: ImageMetadata,
    pub exif: HashMap<String, String>,
    pub is_raw: bool,
    pub as_shot_white_balance: WhiteBalance,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchMaskInfo {
    id: String,
    name: String,
    #[serde(default)]
    invert: bool,
    #[serde(default)]
    sub_masks: Vec<SubMask>,
}

fn srgb_to_linear_lut() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut lut = [0.0f32; 256];
        for (i, v) in lut.iter_mut().enumerate() {
            let x = i as f32 / 255.0;
            *v = if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            };
        }
        lut
    })
}

pub fn load_and_composite(
    base_image: &[u8],
    path: &str,
    adjustments: &Value,
    use_fast_raw_dev: bool,
    settings: &AppSettings,
    cancel_token: Option<(Arc<AtomicUsize>, usize)>,
) -> Result<DynamicImage> {
    let base_image =
        load_base_image_from_bytes(base_image, path, use_fast_raw_dev, settings, cancel_token)?;
    prepare_base_image(&base_image, path, adjustments)
}

pub fn prepare_base_image(
    base_image: &DynamicImage,
    path: &str,
    adjustments: &Value,
) -> Result<DynamicImage> {
    let has_patches = adjustments
        .get("aiPatches")
        .and_then(|v| v.as_array())
        .is_some_and(|a| !a.is_empty());

    match crate::ai_denoise::apply_ai_denoise(base_image, path, adjustments) {
        std::borrow::Cow::Owned(denoised) if !has_patches => Ok(denoised),
        denoised => composite_patches_on_image(&denoised, adjustments),
    }
}

/// Probes TIFF IFDs to determine whether a DNG file is an already-demosaiced Linear DNG
/// (`DNGVersion` 0xC612 present, `PhotometricInterpretation` == 34892 `LinearRaw`, `SamplesPerPixel` >= 3,
/// and no Bayer/X-Trans CFA IFDs). Rejects Canon CR2 and all non-DNG camera RAWs with zero heap allocations.
pub fn is_linear_dng_probe(bytes: &[u8]) -> bool {
    if bytes.len() < 16 {
        return false;
    }

    let le = match &bytes[..4] {
        [0x49, 0x49, 0x2A, 0x00] => true,
        [0x4D, 0x4D, 0x00, 0x2A] => false,
        _ => return false,
    };

    // Canon CR2 starts with standard TIFF magic followed by "CR\x02\x00" at offset 8
    if bytes.get(8..10) == Some(b"CR") {
        return false;
    }

    let rd16 = |o: usize| -> Option<u16> {
        let b: [u8; 2] = bytes.get(o..o + 2)?.try_into().ok()?;
        Some(if le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
    };

    let rd32 = |o: usize| -> Option<u32> {
        let b: [u8; 4] = bytes.get(o..o + 4)?.try_into().ok()?;
        Some(if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    };

    let ifd0_offset = match rd32(4) {
        Some(o) if o >= 8 && (o as usize) < bytes.len() => o as usize,
        _ => return false,
    };

    let mut queue = [0usize; 16];
    queue[0] = ifd0_offset;
    let mut q_len = 1;
    let mut visited = [0usize; 16];
    let mut v_len = 0;

    let mut has_dng_version = false;
    let mut has_linear_raw_ifd = false;
    let mut has_cfa = false;

    while q_len > 0 {
        q_len -= 1;
        let ifd_off = queue[q_len];

        let mut already_visited = false;
        for i in 0..v_len {
            if visited[i] == ifd_off {
                already_visited = true;
                break;
            }
        }
        if already_visited {
            continue;
        }
        if v_len < visited.len() {
            visited[v_len] = ifd_off;
            v_len += 1;
        }

        let entry_count = match rd16(ifd_off) {
            Some(n) if n <= 512 => n as usize,
            _ => continue,
        };

        let mut samples_per_pixel: Option<u32> = None;
        let mut photometric_interp: Option<u32> = None;
        let mut subfile_type: Option<u32> = None;

        for i in 0..entry_count {
            let entry_off = ifd_off + 2 + i * 12;
            let (tag, typ, count, val_or_off) = match (
                rd16(entry_off),
                rd16(entry_off + 2),
                rd32(entry_off + 4),
                rd32(entry_off + 8),
            ) {
                (Some(t), Some(ty), Some(c), Some(v)) => (t, ty, c, v),
                _ => break,
            };

            let scalar_u32 = if typ == 3 && count == 1 {
                rd16(entry_off + 8).map(|v| v as u32)
            } else {
                Some(val_or_off)
            };

            match tag {
                // TAG_NEW_SUBFILE_TYPE = 0x00FE
                0x00FE => {
                    subfile_type = scalar_u32;
                }
                // TAG_PHOTOMETRIC_INTERP = 0x0106
                0x0106 => {
                    photometric_interp = scalar_u32;
                }
                // TAG_SAMPLES_PER_PIXEL = 0x0115
                0x0115 => {
                    samples_per_pixel = scalar_u32;
                }
                // TAG_CFA_REPEAT_PATTERN_DIM = 0x828D, TAG_CFA_PATTERN = 0x828E
                0x828D | 0x828E => {
                    has_cfa = true;
                }
                // TAG_DNG_VERSION = 0xC612 (mandatory in IFD0 of all Adobe DNG files)
                0xC612 => {
                    has_dng_version = true;
                }
                // TAG_SUB_IFDS = 0x014A
                0x014A => {
                    if count == 1 && (val_or_off as usize) < bytes.len() {
                        if q_len < queue.len() {
                            queue[q_len] = val_or_off as usize;
                            q_len += 1;
                        }
                    } else if count > 1 && count <= 8 {
                        let sub_ptr = val_or_off as usize;
                        for j in 0..count as usize {
                            if let Some(sub_off) = rd32(sub_ptr + j * 4) {
                                if (sub_off as usize) < bytes.len() && q_len < queue.len() {
                                    queue[q_len] = sub_off as usize;
                                    q_len += 1;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if photometric_interp == Some(32803) {
            has_cfa = true;
        }

        // Only true LinearRaw (34892) full-resolution IFDs with >= 3 color channels qualify as Linear DNG.
        // Never treat PhotometricInterpretation == 2 (RGB preview JPEG in Canon CR2/NEF/ARW) as LinearRaw.
        let is_subfile_main = (subfile_type.unwrap_or(0) & 1) == 0;
        let spp = samples_per_pixel.unwrap_or(if photometric_interp == Some(34892) { 3 } else { 1 });
        if photometric_interp == Some(34892) && is_subfile_main && spp >= 3 {
            has_linear_raw_ifd = true;
        }

        let next_ifd_pos = ifd_off + 2 + entry_count * 12;
        if let Some(next_ifd) = rd32(next_ifd_pos) {
            if next_ifd > 0 && (next_ifd as usize) < bytes.len() && q_len < queue.len() {
                queue[q_len] = next_ifd as usize;
                q_len += 1;
            }
        }
    }

    has_dng_version && has_linear_raw_ifd && !has_cfa
}

pub fn is_linear_dng_file(path: &str, bytes: &[u8]) -> bool {
    let base_path = path.split("?vc=").next().unwrap_or(path);
    let is_dng_ext = Path::new(base_path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("dng"));
    is_dng_ext && is_linear_dng_probe(bytes)
}

fn try_decode_uncompressed_linear_dng(bytes: &[u8]) -> Option<DynamicImage> {
    if bytes.len() < 16 {
        return None;
    }
    let le = match &bytes[..4] {
        [0x49, 0x49, 0x2A, 0x00] => true,
        [0x4D, 0x4D, 0x00, 0x2A] => false,
        _ => return None,
    };

    let rd16 = |o: usize| -> Option<u16> {
        let b: [u8; 2] = bytes.get(o..o + 2)?.try_into().ok()?;
        Some(if le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
    };
    let rd32 = |o: usize| -> Option<u32> {
        let b: [u8; 4] = bytes.get(o..o + 4)?.try_into().ok()?;
        Some(if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    };
    let read_u32_vec = |typ: u16, count: u32, entry_off: usize, val_or_off: u32| -> Option<Vec<u32>> {
        let c = count as usize;
        if c == 0 || c > 65536 {
            return None;
        }
        let mut out = Vec::with_capacity(c);
        if typ == 3 {
            if c <= 2 {
                for j in 0..c {
                    out.push(rd16(entry_off + 8 + j * 2)? as u32);
                }
            } else {
                let base = val_or_off as usize;
                for j in 0..c {
                    out.push(rd16(base + j * 2)? as u32);
                }
            }
        } else if typ == 4 {
            if c == 1 {
                out.push(val_or_off);
            } else {
                let base = val_or_off as usize;
                for j in 0..c {
                    out.push(rd32(base + j * 4)?);
                }
            }
        } else {
            return None;
        }
        Some(out)
    };

    let ifd0_offset = rd32(4)? as usize;
    if ifd0_offset >= bytes.len() {
        return None;
    }

    let mut queue = vec![ifd0_offset];
    let mut visited = Vec::with_capacity(16);

    while let Some(ifd_off) = queue.pop() {
        if visited.contains(&ifd_off) || visited.len() >= 16 {
            continue;
        }
        visited.push(ifd_off);

        let Some(entry_count) = rd16(ifd_off) else { continue };
        if entry_count > 512 {
            continue;
        }

        let mut width = 0u32;
        let mut height = 0u32;
        let mut compression = 1u32;
        let mut photometric = 0u32;
        let mut orientation = 1u32;
        let mut spp = 3u32;
        let mut planar = 1u32;
        let mut bits_per_sample = 0u32;
        let mut sample_format = 1u32;
        let mut strip_offsets: Vec<u32> = Vec::new();
        let mut strip_byte_counts: Vec<u32> = Vec::new();

        for i in 0..entry_count as usize {
            let e = ifd_off + 2 + i * 12;
            let (Some(tag), Some(typ), Some(count), Some(val_or_off)) =
                (rd16(e), rd16(e + 2), rd32(e + 4), rd32(e + 8))
            else {
                break;
            };
            let scalar = if typ == 3 && count == 1 {
                rd16(e + 8).map(|v| v as u32).unwrap_or(val_or_off)
            } else {
                val_or_off
            };

            match tag {
                0x0100 => width = scalar,
                0x0101 => height = scalar,
                0x0102 => {
                    if let Some(v) = read_u32_vec(typ, count, e, val_or_off)
                        && let Some(&first) = v.first()
                    {
                        bits_per_sample = first;
                    }
                }
                0x0103 => compression = scalar,
                0x0106 => photometric = scalar,
                0x0111 => {
                    if let Some(v) = read_u32_vec(typ, count, e, val_or_off) {
                        strip_offsets = v;
                    }
                }
                0x0112 => orientation = scalar,
                0x0115 => spp = scalar,
                0x0117 => {
                    if let Some(v) = read_u32_vec(typ, count, e, val_or_off) {
                        strip_byte_counts = v;
                    }
                }
                0x011C => planar = scalar,
                0x0153 => {
                    if let Some(v) = read_u32_vec(typ, count, e, val_or_off)
                        && let Some(&first) = v.first()
                    {
                        sample_format = first;
                    }
                }
                0x014A => {
                    if let Some(subs) = read_u32_vec(typ, count, e, val_or_off) {
                        for sub in subs {
                            if (sub as usize) < bytes.len() {
                                queue.push(sub as usize);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let next_pos = ifd_off + 2 + (entry_count as usize) * 12;
        if let Some(next_ifd) = rd32(next_pos)
            && next_ifd > 0
            && (next_ifd as usize) < bytes.len()
        {
            queue.push(next_ifd as usize);
        }

        if photometric == 34892
            && compression == 1
            && planar == 1
            && width > 0
            && height > 0
            && (spp == 3 || spp == 4)
            && !strip_offsets.is_empty()
            && strip_offsets.len() == strip_byte_counts.len()
        {
            let pixel_count = (width as usize).checked_mul(height as usize)?;
            let bytes_per_sample = (bits_per_sample / 8) as usize;
            let expected_bytes = pixel_count.checked_mul(spp as usize)?.checked_mul(bytes_per_sample)?;
            let total_strip_bytes: usize = strip_byte_counts.iter().map(|&b| b as usize).sum();
            if total_strip_bytes < expected_bytes || bytes_per_sample == 0 {
                continue;
            }

            let mut rgb = Vec::with_capacity(pixel_count * 3);
            let step = (spp as usize) * bytes_per_sample;
            let mut remaining_pixels = pixel_count;

            for (&off, &bc) in strip_offsets.iter().zip(strip_byte_counts.iter()) {
                let slice = bytes.get(off as usize..(off as usize).checked_add(bc as usize)?)?;
                for px in slice.chunks_exact(step) {
                    if remaining_pixels == 0 {
                        break;
                    }
                    remaining_pixels -= 1;
                    match (bits_per_sample, sample_format) {
                        (32, 3) => {
                            for c in 0..3 {
                                let b: [u8; 4] = px[c * 4..c * 4 + 4].try_into().ok()?;
                                let v = if le { f32::from_le_bytes(b) } else { f32::from_be_bytes(b) };
                                rgb.push(if v.is_finite() { v.max(0.0) } else { 0.0 });
                            }
                        }
                        (16, _) => {
                            for c in 0..3 {
                                let b: [u8; 2] = px[c * 2..c * 2 + 2].try_into().ok()?;
                                let v = if le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) };
                                rgb.push(v as f32 / 65535.0);
                            }
                        }
                        (8, _) => {
                            for c in 0..3 {
                                rgb.push(px[c] as f32 / 255.0);
                            }
                        }
                        _ => return None,
                    }
                }
            }

            if rgb.len() == pixel_count * 3 {
                let img = image::Rgb32FImage::from_raw(width, height, rgb)?;
                let dyn_img = DynamicImage::ImageRgb32F(img);
                let oriented = match orientation {
                    2 => dyn_img.fliph(),
                    3 => dyn_img.rotate180(),
                    4 => dyn_img.flipv(),
                    5 => dyn_img.rotate90().fliph(),
                    6 => dyn_img.rotate90(),
                    7 => dyn_img.rotate270().fliph(),
                    8 => dyn_img.rotate270(),
                    _ => dyn_img,
                };
                return Some(oriented);
            }
        }
    }

    None
}

fn normalize_linear_dng_tags(bytes: &[u8]) -> Vec<u8> {
    let mut patched = bytes.to_vec();
    if patched.len() < 16 {
        return patched;
    }

    let le = match &patched[..4] {
        [0x49, 0x49, 0x2A, 0x00] => true,
        [0x4D, 0x4D, 0x00, 0x2A] => false,
        _ => return patched,
    };

    let rd16 = |buf: &[u8], o: usize| -> Option<u16> {
        let b: [u8; 2] = buf.get(o..o + 2)?.try_into().ok()?;
        Some(if le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
    };

    let rd32 = |buf: &[u8], o: usize| -> Option<u32> {
        let b: [u8; 4] = buf.get(o..o + 4)?.try_into().ok()?;
        Some(if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    };

    let ifd0_offset = match rd32(&patched, 4) {
        Some(o) if (o as usize) < patched.len() => o as usize,
        _ => return patched,
    };

    let mut queue = [0usize; 16];
    queue[0] = ifd0_offset;
    let mut q_len = 1;
    let mut visited = [0usize; 16];
    let mut v_len = 0;

    while q_len > 0 {
        q_len -= 1;
        let ifd_off = queue[q_len];

        let mut already_visited = false;
        for i in 0..v_len {
            if visited[i] == ifd_off {
                already_visited = true;
                break;
            }
        }
        if already_visited {
            continue;
        }
        if v_len < visited.len() {
            visited[v_len] = ifd_off;
            v_len += 1;
        }

        let entry_count = match rd16(&patched, ifd_off) {
            Some(n) => n as usize,
            None => continue,
        };

        for i in 0..entry_count {
            let entry_off = ifd_off + 2 + i * 12;
            let (tag, typ, count, val_or_off) = match (
                rd16(&patched, entry_off),
                rd16(&patched, entry_off + 2),
                rd32(&patched, entry_off + 4),
                rd32(&patched, entry_off + 8),
            ) {
                (Some(t), Some(ty), Some(c), Some(v)) => (t, ty, c, v),
                _ => break,
            };

            if tag == 0x0106 {
                let val_u32 = if typ == 3 && count == 1 {
                    rd16(&patched, entry_off + 8).unwrap_or(0) as u32
                } else {
                    val_or_off
                };
                if val_u32 == 34892 {
                    if typ == 3 {
                        let rep = if le { 2u16.to_le_bytes() } else { 2u16.to_be_bytes() };
                        patched[entry_off + 8..entry_off + 10].copy_from_slice(&rep);
                    } else {
                        let rep = if le { 2u32.to_le_bytes() } else { 2u32.to_be_bytes() };
                        patched[entry_off + 8..entry_off + 12].copy_from_slice(&rep);
                    }
                }
            } else if tag == 0x014A {
                if count == 1 && (val_or_off as usize) < patched.len() {
                    if q_len < queue.len() {
                        queue[q_len] = val_or_off as usize;
                        q_len += 1;
                    }
                } else if count > 1 && count <= 8 {
                    let sub_ptr = val_or_off as usize;
                    for j in 0..count as usize {
                        if let Some(sub_off) = rd32(&patched, sub_ptr + j * 4) {
                            if (sub_off as usize) < patched.len() && q_len < queue.len() {
                                queue[q_len] = sub_off as usize;
                                q_len += 1;
                            }
                        }
                    }
                }
            }
        }

        let next_ifd_pos = ifd_off + 2 + entry_count * 12;
        if let Some(next_ifd) = rd32(&patched, next_ifd_pos) {
            if next_ifd > 0 && (next_ifd as usize) < patched.len() && q_len < queue.len() {
                queue[q_len] = next_ifd as usize;
                q_len += 1;
            }
        }
    }

    patched
}

pub fn load_linear_dng(
    bytes: &[u8],
    cancel_token: Option<(Arc<AtomicUsize>, usize)>,
) -> Result<DynamicImage> {
    if let Some(img) = try_decode_uncompressed_linear_dng(bytes) {
        return Ok(img);
    }

    if let Ok(img) = load_image_with_orientation(bytes, cancel_token.clone()) {
        return Ok(img);
    }

    let patched_bytes = normalize_linear_dng_tags(bytes);
    if let Ok(img) = load_image_with_orientation(&patched_bytes, cancel_token) {
        return Ok(img);
    }

    use std::io::Cursor;
    use tiff::decoder::{Decoder, DecodingResult};

    let mut decoder = Decoder::new(Cursor::new(patched_bytes.as_slice()))
        .or_else(|_| Decoder::new(Cursor::new(bytes)))
        .map_err(|e| anyhow!("TIFF decoder error: {}", e))?;

    let mut best_img: Option<DynamicImage> = None;
    let mut best_area: u64 = 0;

    loop {
        if let Ok((width, height)) = decoder.dimensions()
            && let Ok(result) = decoder.read_image()
        {
            let candidate = match result {
                DecodingResult::F32(vec) => {
                    let px = (width as usize) * (height as usize);
                    if vec.len() == px * 3 {
                        image::Rgb32FImage::from_raw(width, height, vec)
                            .map(DynamicImage::ImageRgb32F)
                    } else if vec.len() == px * 4 {
                        let rgb: Vec<f32> = vec
                            .chunks_exact(4)
                            .flat_map(|c| [c[0], c[1], c[2]])
                            .collect();
                        image::Rgb32FImage::from_raw(width, height, rgb)
                            .map(DynamicImage::ImageRgb32F)
                    } else {
                        None
                    }
                }
                DecodingResult::U16(vec) => {
                    image::ImageBuffer::<image::Rgb<u16>, Vec<u16>>::from_raw(width, height, vec)
                        .map(DynamicImage::ImageRgb16)
                }
                DecodingResult::U8(vec) => {
                    image::RgbImage::from_raw(width, height, vec).map(DynamicImage::ImageRgb8)
                }
                _ => None,
            };
            if let Some(img) = candidate {
                let area = (width as u64) * (height as u64);
                if area > best_area {
                    best_area = area;
                    best_img = Some(img);
                }
            }
        }
        if decoder.next_image().is_err() {
            break;
        }
    }

    if let Some(dynamic_img) = best_img {
        return Ok(DynamicImage::ImageRgb32F(dynamic_img.to_rgb32f()));
    }

    if let Some(preview) = largest_tiff_jpeg_preview(bytes) {
        return Ok(DynamicImage::ImageRgb32F(preview.to_rgb32f()));
    }

    Err(anyhow!("Unsupported Linear DNG encoding or color layout"))
}

pub fn load_base_image_from_bytes(
    bytes: &[u8],
    path_for_ext_check: &str,
    use_fast_raw_dev: bool,
    settings: &AppSettings,
    cancel_token: Option<(Arc<AtomicUsize>, usize)>,
) -> Result<DynamicImage> {
    let highlight_compression = settings.raw_highlight_compression.unwrap_or(2.5);
    let linear_mode = settings.linear_raw_mode.clone();
    let color_nr_setting = settings.raw_preprocessing_color_nr.unwrap_or(0.0);
    let color_nr_amount = if color_nr_setting <= 0.0 {
        0.0
    } else {
        let x = color_nr_setting.clamp(0.01, 1.0);
        (12.0 / x - 10.0).max(0.1)
    };
    let sharpening_amount = settings.raw_preprocessing_sharpening.unwrap_or(0.0);
    let apply_to_non_raws = settings.apply_preprocessing_to_non_raws.unwrap_or(false);

    crate::exif_processing::persist_exif_if_missing(
        Path::new(path_for_ext_check),
        path_for_ext_check,
        bytes,
    );

    if is_raw_file(path_for_ext_check) {
        if is_linear_dng_file(path_for_ext_check, bytes) {
            log::info!(
                "Detected Linear DNG for '{}', bypassing rawler demosaicing",
                path_for_ext_check
            );
            let mut image = load_linear_dng(bytes, cancel_token)?;
            if !use_fast_raw_dev && (color_nr_amount > 0.0 || sharpening_amount > 0.0) {
                let start = Instant::now();
                remove_raw_artifacts_and_enhance(
                    &mut image,
                    color_nr_amount,
                    sharpening_amount,
                );
                let duration = start.elapsed();
                log::info!(
                    "Raw enhancing for '{}' took {:?}",
                    path_for_ext_check,
                    duration
                );
            }
            return Ok(image);
        }

        match panic::catch_unwind(move || {
            crate::raw_processing::develop_raw_image(
                bytes,
                use_fast_raw_dev,
                highlight_compression,
                linear_mode,
                cancel_token,
            )
        }) {
            Ok(Ok(mut image)) => {
                if !use_fast_raw_dev && (color_nr_amount > 0.0 || sharpening_amount > 0.0) {
                    let start = Instant::now();
                    remove_raw_artifacts_and_enhance(
                        &mut image,
                        color_nr_amount,
                        sharpening_amount,
                    );
                    let duration = start.elapsed();
                    log::info!(
                        "Raw enhancing for '{}' took {:?}",
                        path_for_ext_check,
                        duration
                    );
                }
                Ok(image)
            }
            Ok(Err(e)) => {
                let classified = classify_raw_develop_error(path_for_ext_check, e);

                if classified.to_string().contains("Load cancelled") {
                    return Err(classified);
                }

                log::warn!(
                    "Error developing RAW file '{}': {}",
                    path_for_ext_check,
                    classified
                );
                if let Some(preview) = safe_embedded_preview_fallback(bytes, path_for_ext_check) {
                    log::warn!(
                        "Using embedded preview fallback for '{}' ({}x{})",
                        path_for_ext_check,
                        preview.width(),
                        preview.height()
                    );

                    return Ok(linearize_embedded_preview(preview));
                }
                Err(classified)
            }
            Err(_) => {
                log::error!("Panic while processing RAW file: {}", path_for_ext_check);
                if let Some(preview) = safe_embedded_preview_fallback(bytes, path_for_ext_check) {
                    log::warn!(
                        "Using embedded preview fallback for '{}' after RAW decoder panic ({}x{})",
                        path_for_ext_check,
                        preview.width(),
                        preview.height()
                    );

                    return Ok(linearize_embedded_preview(preview));
                }
                Err(anyhow!(
                    "Failed to process RAW file: {}",
                    path_for_ext_check
                ))
            }
        }
    } else {
        let mut image = load_image_with_orientation(bytes, cancel_token)?;

        if apply_to_non_raws
            && !use_fast_raw_dev
            && (color_nr_amount > 0.0 || sharpening_amount > 0.0)
        {
            let start = Instant::now();
            remove_raw_artifacts_and_enhance(&mut image, color_nr_amount, sharpening_amount);
            let duration = start.elapsed();
            log::info!(
                "Enhancing non-RAW '{}' took {:?}",
                path_for_ext_check,
                duration
            );
        }

        Ok(image)
    }
}

fn classify_raw_develop_error(path: &str, err: anyhow::Error) -> anyhow::Error {
    let error_text = err.to_string();
    let lowered = error_text.to_ascii_lowercase();
    let unsupported_compression =
        lowered.contains("nef compression") && lowered.contains("not supported");

    if unsupported_compression {
        return anyhow!(
            "Unsupported RAW compression format for '{}'. Original error: {}",
            path,
            error_text
        );
    }

    err
}

fn largest_tiff_jpeg_preview(buf: &[u8]) -> Option<DynamicImage> {
    let le = match buf.get(..4)? {
        [0x49, 0x49, 0x2A, 0x00] => true,
        [0x4D, 0x4D, 0x00, 0x2A] => false,
        _ => return None,
    };
    let rd16 = |o: usize| -> Option<u64> {
        let b: [u8; 2] = buf.get(o..o + 2)?.try_into().ok()?;
        Some(if le {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        } as u64)
    };
    let rd32 = |o: usize| -> Option<u64> {
        let b: [u8; 4] = buf.get(o..o + 4)?.try_into().ok()?;
        Some(if le {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        } as u64)
    };

    let mut candidates: Vec<(u64, u64)> = Vec::new();
    let mut queue: Vec<u64> = vec![rd32(4)?];
    let mut seen = HashMap::new();

    while let Some(ifd) = queue.pop() {
        if seen.insert(ifd, ()).is_some() || seen.len() > 64 {
            continue;
        }
        let Some(n) = rd16(ifd as usize) else {
            continue;
        };

        let mut compression: u64 = 0;
        let mut strip: Option<(u64, u64)> = None;
        let mut old_jpeg: Option<(u64, u64)> = None;

        for i in 0..n {
            let e = ifd as usize + 2 + (i as usize) * 12;
            let (Some(tag), Some(typ), Some(count), Some(raw_val)) =
                (rd16(e), rd16(e + 2), rd32(e + 4), rd32(e + 8))
            else {
                continue;
            };
            let val = if typ == 3 && count == 1 {
                rd16(e + 8).unwrap_or(raw_val)
            } else {
                raw_val
            };
            match tag {
                259 => compression = val,
                273 if count == 1 => strip = Some((val, strip.map_or(0, |s| s.1))),
                279 if count == 1 => strip = strip.map(|s| (s.0, val)).or(Some((0, val))),
                513 => old_jpeg = Some((val, old_jpeg.map_or(0, |s| s.1))),
                514 => old_jpeg = old_jpeg.map(|s| (s.0, val)).or(Some((0, val))),
                330 => {
                    if count == 1 {
                        queue.push(val);
                    } else {
                        for j in 0..count.min(8) {
                            if let Some(p) = rd32(val as usize + (j as usize) * 4) {
                                queue.push(p);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if matches!(compression, 6 | 7)
            && let Some(s) = strip
        {
            candidates.push(s);
        }
        if let Some(oj) = old_jpeg {
            candidates.push(oj);
        }
        if let Some(next) = rd32(ifd as usize + 2 + (n as usize) * 12)
            && next != 0
        {
            queue.push(next);
        }
    }

    candidates.sort_by_key(|&(_, len)| std::cmp::Reverse(len));

    for (off, len) in candidates {
        if let Some(bytes) = buf.get(off as usize..(off + len) as usize)
            && let Ok(img) = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg)
        {
            return Some(img);
        }
    }

    None
}

fn embedded_preview_fallback(bytes: &[u8]) -> Option<DynamicImage> {
    let Some(img) = largest_tiff_jpeg_preview(bytes) else {
        return crate::raw_processing::extract_embedded_preview(bytes);
    };

    let orientation = ExifReader::new()
        .read_from_container(&mut Cursor::new(bytes))
        .ok()
        .and_then(|exif| {
            exif.get_field(Tag::Orientation, exif::In::PRIMARY)?
                .value
                .get_uint(0)
        });

    Some(match orientation {
        Some(o) if o > 1 => apply_orientation(img, Orientation::from_u16(o as u16)),
        _ => img,
    })
}

pub fn safe_embedded_preview_fallback(bytes: &[u8], path: &str) -> Option<DynamicImage> {
    match panic::catch_unwind(panic::AssertUnwindSafe(|| embedded_preview_fallback(bytes))) {
        Ok(preview) => preview,
        Err(_) => {
            log::warn!("Embedded RAW preview extraction panicked for '{}'", path);
            None
        }
    }
}

fn linearize_embedded_preview(preview: DynamicImage) -> DynamicImage {
    let preview = DynamicImage::ImageRgb32F(preview.to_rgb32f());
    let mut linear_preview = apply_srgb_to_linear(preview).into_rgb32f();
    for pixel in linear_preview.pixels_mut() {
        pixel[0] *= 0.4;
        pixel[1] *= 0.4;
        pixel[2] *= 0.4;
    }
    DynamicImage::ImageRgb32F(linear_preview)
}

pub fn load_image_with_orientation(
    bytes: &[u8],
    cancel_token: Option<(Arc<AtomicUsize>, usize)>,
) -> Result<DynamicImage> {
    let check_cancel = || -> Result<()> {
        if let Some((tracker, generation)) = &cancel_token
            && tracker.load(Ordering::SeqCst) != *generation
        {
            return Err(anyhow!("Load cancelled"));
        }
        Ok(())
    };

    let cursor = Cursor::new(bytes);
    let mut reader = ImageReader::new(cursor.clone())
        .with_guessed_format()
        .context("Failed to guess image format")?;

    reader.no_limits();

    check_cancel()?;

    let image = reader.decode().context("Failed to decode image")?;
    check_cancel()?;

    let oriented_image = {
        let exif_reader = ExifReader::new();
        if let Ok(exif) = exif_reader.read_from_container(&mut cursor.clone()) {
            if let Some(orientation) = exif
                .get_field(Tag::Orientation, exif::In::PRIMARY)
                .and_then(|f| f.value.get_uint(0))
            {
                check_cancel()?;
                apply_orientation(image, Orientation::from_u16(orientation as u16))
            } else {
                image
            }
        } else {
            image
        }
    };

    Ok(DynamicImage::ImageRgb32F(oriented_image.to_rgb32f()))
}

pub fn composite_patches_on_image(
    base_image: &DynamicImage,
    current_adjustments: &Value,
) -> Result<DynamicImage> {
    let patches_val = match current_adjustments.get("aiPatches") {
        Some(val) => val,
        None => return Ok(base_image.clone()),
    };

    let patches_arr = match patches_val.as_array() {
        Some(arr) if !arr.is_empty() => arr,
        _ => return Ok(base_image.clone()),
    };

    let visible_patches: Vec<&Value> = patches_arr
        .par_iter()
        .filter(|patch_obj| {
            let is_visible = patch_obj
                .get("visible")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            if !is_visible {
                return false;
            }
            patch_obj
                .get("patchData")
                .and_then(|data| data.get("color"))
                .and_then(|color| color.as_str())
                .is_some_and(|s| !s.is_empty())
        })
        .collect();

    if visible_patches.is_empty() {
        return Ok(base_image.clone());
    }

    let (base_w, base_h) = base_image.dimensions();

    struct DecodedPatch {
        offset_x: Option<u32>,
        offset_y: Option<u32>,
        mask: image::GrayImage,
        color: image::RgbImage,
        is_srgb_encoded: bool,
    }

    let decoded_patches: Result<Vec<DecodedPatch>> = visible_patches
        .par_iter()
        .map(|patch_obj| {
            let patch_data = patch_obj.get("patchData").context("Missing patchData")?;
            let offset_x = patch_data
                .get("offsetX")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
            let offset_y = patch_data
                .get("offsetY")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
            let is_cropped = offset_x.is_some() && offset_y.is_some();

            let is_srgb_encoded = patch_data
                .get("isSrgbEncoded")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let mask_bitmap = if let Some(mask_b64) = patch_data
                .get("mask")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                let mask_bytes = general_purpose::STANDARD.decode(mask_b64)?;
                let mask_img = image::load_from_memory(&mask_bytes)?.to_luma8();
                if !is_cropped && (mask_img.width() != base_w || mask_img.height() != base_h) {
                    imageops::resize(&mask_img, base_w, base_h, imageops::FilterType::Lanczos3)
                } else {
                    mask_img
                }
            } else {
                let patch_info: PatchMaskInfo = serde_json::from_value((*patch_obj).clone())
                    .context("Failed to deserialize patch info for mask generation")?;

                let mask_def = MaskDefinition {
                    id: patch_info.id,
                    name: patch_info.name,
                    visible: true,
                    invert: patch_info.invert,
                    opacity: 100.0,
                    adjustments: Value::Null,
                    sub_masks: patch_info.sub_masks,
                };

                let orientation_steps = current_adjustments
                    .get("orientationSteps")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u8;
                let (trans_w, trans_h) = if orientation_steps % 2 == 1 {
                    (base_h, base_w)
                } else {
                    (base_w, base_h)
                };

                let mut gen_mask =
                    generate_mask_bitmap(&mask_def, trans_w, trans_h, 1.0, (0.0, 0.0), None)
                        .context("Failed to generate mask from sub_masks for compositing")?;

                gen_mask =
                    crate::image_processing::inverse_transform_mask(gen_mask, current_adjustments);

                if let (Some(ox), Some(oy)) = (offset_x, offset_y) {
                    let w = patch_data
                        .get("width")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32)
                        .unwrap_or(base_w);
                    let h = patch_data
                        .get("height")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32)
                        .unwrap_or(base_h);
                    let crop_w = w.min(base_w.saturating_sub(ox));
                    let crop_h = h.min(base_h.saturating_sub(oy));
                    gen_mask = imageops::crop_imm(&gen_mask, ox, oy, crop_w, crop_h).to_image();
                }
                gen_mask
            };

            let color_b64 = patch_data
                .get("color")
                .and_then(|v| v.as_str())
                .context("Missing color data")?;
            let color_bytes = general_purpose::STANDARD.decode(color_b64)?;
            let color_image_u8 = image::load_from_memory(&color_bytes)?.to_rgb8();

            let (patch_w, patch_h) = color_image_u8.dimensions();
            let final_color = if !is_cropped && (base_w != patch_w || base_h != patch_h) {
                imageops::resize(
                    &color_image_u8,
                    base_w,
                    base_h,
                    imageops::FilterType::Lanczos3,
                )
            } else {
                color_image_u8
            };

            Ok(DecodedPatch {
                offset_x,
                offset_y,
                mask: mask_bitmap,
                color: final_color,
                is_srgb_encoded,
            })
        })
        .collect();

    let decoded_patches = decoded_patches?;

    let mut composited_image = base_image.clone();
    let lut = srgb_to_linear_lut();

    let get_color = |patch: &DecodedPatch, r: u8, g: u8, b: u8| -> (f32, f32, f32) {
        if patch.is_srgb_encoded {
            (lut[r as usize], lut[g as usize], lut[b as usize])
        } else {
            (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
        }
    };

    match &mut composited_image {
        DynamicImage::ImageRgb32F(img_buf) => {
            for patch in decoded_patches {
                let mask_raw = patch.mask.as_raw();
                let color_raw = patch.color.as_raw();
                let patch_w = patch.mask.width() as usize;

                if let (Some(ox), Some(oy)) = (patch.offset_x, patch.offset_y) {
                    let max_x = (ox + patch.mask.width()).min(base_w);
                    let max_y = (oy + patch.mask.height()).min(base_h);

                    let crop_w = max_x.saturating_sub(ox) as usize;
                    let crop_h = max_y.saturating_sub(oy) as usize;

                    if crop_w == 0 || crop_h == 0 {
                        continue;
                    }

                    let base_w_usize = base_w as usize;
                    let ox_usize = ox as usize;
                    let oy_usize = oy as usize;

                    img_buf
                        .par_chunks_mut(base_w_usize * 3)
                        .enumerate()
                        .skip(oy_usize)
                        .take(crop_h)
                        .for_each(|(y, row)| {
                            let py = y - oy_usize;
                            let patch_row_start = py * patch_w;

                            for x in ox_usize..(ox_usize + crop_w) {
                                let px = x - ox_usize;
                                let mask_idx = patch_row_start + px;
                                let mask_value = mask_raw[mask_idx];

                                if mask_value > 0 {
                                    let color_idx = mask_idx * 3;
                                    let pr_u8 = color_raw[color_idx];
                                    let pg_u8 = color_raw[color_idx + 1];
                                    let pb_u8 = color_raw[color_idx + 2];

                                    let (pr, pg, pb) = get_color(&patch, pr_u8, pg_u8, pb_u8);

                                    let alpha = mask_value as f32 / 255.0;
                                    let one_minus_alpha = 1.0 - alpha;

                                    let base_idx = x * 3;
                                    row[base_idx] = pr * alpha + row[base_idx] * one_minus_alpha;
                                    row[base_idx + 1] =
                                        pg * alpha + row[base_idx + 1] * one_minus_alpha;
                                    row[base_idx + 2] =
                                        pb * alpha + row[base_idx + 2] * one_minus_alpha;
                                }
                            }
                        });
                } else {
                    img_buf
                        .par_chunks_mut((base_w * 3) as usize)
                        .enumerate()
                        .for_each(|(y, row)| {
                            let patch_row_start = y * patch_w;
                            for x in 0..base_w as usize {
                                let mask_idx = patch_row_start + x;
                                let mask_value = mask_raw[mask_idx];
                                if mask_value > 0 {
                                    let color_idx = mask_idx * 3;
                                    let pr_u8 = color_raw[color_idx];
                                    let pg_u8 = color_raw[color_idx + 1];
                                    let pb_u8 = color_raw[color_idx + 2];

                                    let (pr, pg, pb) = get_color(&patch, pr_u8, pg_u8, pb_u8);

                                    let alpha = mask_value as f32 / 255.0;
                                    let one_minus_alpha = 1.0 - alpha;

                                    row[x * 3] = pr * alpha + row[x * 3] * one_minus_alpha;
                                    row[x * 3 + 1] = pg * alpha + row[x * 3 + 1] * one_minus_alpha;
                                    row[x * 3 + 2] = pb * alpha + row[x * 3 + 2] * one_minus_alpha;
                                }
                            }
                        });
                }
            }
        }
        DynamicImage::ImageRgba32F(img_buf) => {
            for patch in decoded_patches {
                let mask_raw = patch.mask.as_raw();
                let color_raw = patch.color.as_raw();
                let patch_w = patch.mask.width() as usize;

                if let (Some(ox), Some(oy)) = (patch.offset_x, patch.offset_y) {
                    let max_x = (ox + patch.mask.width()).min(base_w);
                    let max_y = (oy + patch.mask.height()).min(base_h);

                    let crop_w = max_x.saturating_sub(ox) as usize;
                    let crop_h = max_y.saturating_sub(oy) as usize;

                    if crop_w == 0 || crop_h == 0 {
                        continue;
                    }

                    let base_w_usize = base_w as usize;
                    let ox_usize = ox as usize;
                    let oy_usize = oy as usize;

                    img_buf
                        .par_chunks_mut(base_w_usize * 4)
                        .enumerate()
                        .skip(oy_usize)
                        .take(crop_h)
                        .for_each(|(y, row)| {
                            let py = y - oy_usize;
                            let patch_row_start = py * patch_w;

                            for x in ox_usize..(ox_usize + crop_w) {
                                let px = x - ox_usize;
                                let mask_idx = patch_row_start + px;
                                let mask_value = mask_raw[mask_idx];

                                if mask_value > 0 {
                                    let color_idx = mask_idx * 3;
                                    let pr_u8 = color_raw[color_idx];
                                    let pg_u8 = color_raw[color_idx + 1];
                                    let pb_u8 = color_raw[color_idx + 2];

                                    let (pr, pg, pb) = get_color(&patch, pr_u8, pg_u8, pb_u8);
                                    let alpha = mask_value as f32 / 255.0;
                                    let one_minus_alpha = 1.0 - alpha;

                                    let base_idx = x * 4;
                                    row[base_idx] = pr * alpha + row[base_idx] * one_minus_alpha;
                                    row[base_idx + 1] =
                                        pg * alpha + row[base_idx + 1] * one_minus_alpha;
                                    row[base_idx + 2] =
                                        pb * alpha + row[base_idx + 2] * one_minus_alpha;
                                }
                            }
                        });
                } else {
                    img_buf
                        .par_chunks_mut((base_w * 4) as usize)
                        .enumerate()
                        .for_each(|(y, row)| {
                            let patch_row_start = y * patch_w;
                            for x in 0..base_w as usize {
                                let mask_idx = patch_row_start + x;
                                let mask_value = mask_raw[mask_idx];
                                if mask_value > 0 {
                                    let color_idx = mask_idx * 3;
                                    let pr_u8 = color_raw[color_idx];
                                    let pg_u8 = color_raw[color_idx + 1];
                                    let pb_u8 = color_raw[color_idx + 2];

                                    let (pr, pg, pb) = get_color(&patch, pr_u8, pg_u8, pb_u8);
                                    let alpha = mask_value as f32 / 255.0;
                                    let one_minus_alpha = 1.0 - alpha;

                                    row[x * 4] = pr * alpha + row[x * 4] * one_minus_alpha;
                                    row[x * 4 + 1] = pg * alpha + row[x * 4 + 1] * one_minus_alpha;
                                    row[x * 4 + 2] = pb * alpha + row[x * 4 + 2] * one_minus_alpha;
                                }
                            }
                        });
                }
            }
        }
        _ => {
            let mut rgba32_img = composited_image.to_rgba32f();
            for patch in decoded_patches {
                let mask_raw = patch.mask.as_raw();
                let color_raw = patch.color.as_raw();
                let patch_w = patch.mask.width() as usize;

                if let (Some(ox), Some(oy)) = (patch.offset_x, patch.offset_y) {
                    let max_x = (ox + patch.mask.width()).min(base_w);
                    let max_y = (oy + patch.mask.height()).min(base_h);

                    let crop_w = max_x.saturating_sub(ox) as usize;
                    let crop_h = max_y.saturating_sub(oy) as usize;

                    if crop_w == 0 || crop_h == 0 {
                        continue;
                    }

                    let base_w_usize = base_w as usize;
                    let ox_usize = ox as usize;
                    let oy_usize = oy as usize;

                    rgba32_img
                        .par_chunks_mut(base_w_usize * 4)
                        .enumerate()
                        .skip(oy_usize)
                        .take(crop_h)
                        .for_each(|(y, row)| {
                            let py = y - oy_usize;
                            let patch_row_start = py * patch_w;

                            for x in ox_usize..(ox_usize + crop_w) {
                                let px = x - ox_usize;
                                let mask_idx = patch_row_start + px;
                                let mask_value = mask_raw[mask_idx];

                                if mask_value > 0 {
                                    let color_idx = mask_idx * 3;
                                    let pr_u8 = color_raw[color_idx];
                                    let pg_u8 = color_raw[color_idx + 1];
                                    let pb_u8 = color_raw[color_idx + 2];

                                    let (pr, pg, pb) = get_color(&patch, pr_u8, pg_u8, pb_u8);
                                    let alpha = mask_value as f32 / 255.0;
                                    let one_minus_alpha = 1.0 - alpha;

                                    let base_idx = x * 4;
                                    row[base_idx] = pr * alpha + row[base_idx] * one_minus_alpha;
                                    row[base_idx + 1] =
                                        pg * alpha + row[base_idx + 1] * one_minus_alpha;
                                    row[base_idx + 2] =
                                        pb * alpha + row[base_idx + 2] * one_minus_alpha;
                                }
                            }
                        });
                } else {
                    rgba32_img
                        .par_chunks_mut((base_w * 4) as usize)
                        .enumerate()
                        .for_each(|(y, row)| {
                            let patch_row_start = y * patch_w;
                            for x in 0..base_w as usize {
                                let mask_idx = patch_row_start + x;
                                let mask_value = mask_raw[mask_idx];
                                if mask_value > 0 {
                                    let color_idx = mask_idx * 3;
                                    let pr_u8 = color_raw[color_idx];
                                    let pg_u8 = color_raw[color_idx + 1];
                                    let pb_u8 = color_raw[color_idx + 2];

                                    let (pr, pg, pb) = get_color(&patch, pr_u8, pg_u8, pb_u8);
                                    let alpha = mask_value as f32 / 255.0;
                                    let one_minus_alpha = 1.0 - alpha;

                                    row[x * 4] = pr * alpha + row[x * 4] * one_minus_alpha;
                                    row[x * 4 + 1] = pg * alpha + row[x * 4 + 1] * one_minus_alpha;
                                    row[x * 4 + 2] = pb * alpha + row[x * 4 + 2] * one_minus_alpha;
                                }
                            }
                        });
                }
            }
            composited_image = DynamicImage::ImageRgba32F(rgba32_img);
        }
    }

    Ok(composited_image)
}

#[tauri::command]
pub fn is_image_cached(path: String, state: tauri::State<'_, AppState>) -> bool {
    let (source_path, _) = parse_virtual_path(&path);
    let source_path_str = source_path.to_string_lossy().to_string();
    state
        .decoded_image_cache
        .lock()
        .unwrap()
        .get(&source_path_str)
        .is_some()
}

#[tauri::command]
pub async fn load_image(
    path: String,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<LoadImageResult, String> {
    let my_generation = state.load_image_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let generation_tracker = state.load_image_generation.clone();
    let cancel_token = Some((generation_tracker.clone(), my_generation));

    {
        *state
            .original_image
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        *state
            .cached_preview
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        *state
            .gpu_image_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        *state
            .full_warped_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        *state
            .full_transformed_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        crate::cache_utils::clear_preview_stage_caches(&state);

        state.mask_cache.lock().unwrap().clear();
        state.patch_cache.lock().unwrap().clear();
        state.geometry_cache.lock().unwrap().clear();

        *state.denoise_result.lock().unwrap() = None;
        *state.hdr_result.lock().unwrap() = None;
        *state.panorama_result.lock().unwrap() = None;
    }

    let (source_path, sidecar_path) = parse_virtual_path(&path);
    let source_path_str = source_path.to_string_lossy().to_string();

    let metadata: ImageMetadata = crate::exif_processing::load_sidecar(&sidecar_path);

    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let baseline_options = BaselineOptions::from_settings(&settings);

    let path_clone = source_path_str.clone();

    let cached_data = state
        .decoded_image_cache
        .lock()
        .unwrap()
        .get(&source_path_str);

    let (pristine_arc, exif_data) = if let Some((cached_img, cached_exif)) = cached_data {
        (cached_img, cached_exif)
    } else {
        if crate::file_management::is_cloud_placeholder(&source_path) {
            return Err(format!(
                "'{}' is stored in iCloud and hasn't been downloaded yet. Download it in Finder, then try again.",
                source_path_str
            ));
        }

        let (pristine_img, exif_data_loaded) = tokio::task::spawn_blocking(move || {
            if generation_tracker.load(Ordering::SeqCst) != my_generation {
                return Err("Load cancelled".to_string());
            }

            let result: Result<(DynamicImage, HashMap<String, String>), String> =
                (|| match read_file_mapped(Path::new(&path_clone)) {
                    Ok(mmap) => {
                        if generation_tracker.load(Ordering::SeqCst) != my_generation {
                            return Err("Load cancelled".to_string());
                        }

                        let img = load_base_image_from_bytes(
                            &mmap,
                            &path_clone,
                            false,
                            &settings,
                            cancel_token.clone(),
                        )
                        .map_err(|e| e.to_string())?;
                        let exif = exif_processing::read_exif_data(&path_clone, &mmap);
                        let _ = crate::as_shot::as_shot_with_bytes(&path_clone, &mmap);
                        Ok((img, exif))
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to memory-map file '{}': {}. Falling back to standard read.",
                            path_clone,
                            e
                        );
                        let bytes = fs::read(&path_clone).map_err(|io_err| {
                            format!("Fallback read failed for {}: {}", path_clone, io_err)
                        })?;

                        if generation_tracker.load(Ordering::SeqCst) != my_generation {
                            return Err("Load cancelled".to_string());
                        }

                        let img = load_base_image_from_bytes(
                            &bytes,
                            &path_clone,
                            false,
                            &settings,
                            cancel_token.clone(),
                        )
                        .map_err(|e| e.to_string())?;
                        let exif = exif_processing::read_exif_data(&path_clone, &bytes);
                        let _ = crate::as_shot::as_shot_with_bytes(&path_clone, &bytes);
                        Ok((img, exif))
                    }
                })();
            result
        })
        .await
        .map_err(|e| e.to_string())??;

        let arc_img = Arc::new(pristine_img);

        state.decoded_image_cache.lock().unwrap().insert(
            source_path_str.clone(),
            arc_img.clone(),
            exif_data_loaded.clone(),
        );

        (arc_img, exif_data_loaded)
    };

    if state.load_image_generation.load(Ordering::SeqCst) != my_generation {
        return Err("Load cancelled".to_string());
    }

    let is_raw = is_raw_file(&source_path_str);

    if state.load_image_generation.load(Ordering::SeqCst) != my_generation {
        return Err("Load cancelled".to_string());
    }

    let (orig_width, orig_height) = pristine_arc.dimensions();
    let screen_proxy = Arc::new(crate::fast_resizer::fast_downscale_dynamic(&pristine_arc, 2560, 2560));
    let as_shot = crate::as_shot::as_shot(&source_path_str);
    if is_raw {
        log::info!(
            "Baseline exposure for '{}': {:+.2} EV (DNG baseline {:+.2} EV, highlight preservation {:+.2} EV)",
            source_path_str,
            effective_baseline_exposure(&metadata.adjustments, as_shot, baseline_options),
            as_shot.exposure.dng_baseline,
            as_shot.exposure.highlight_preservation
        );
    }

    *state
        .original_image
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(LoadedImage {
        path,
        image: pristine_arc,
        is_raw,
        screen_proxy: Some(screen_proxy),
        as_shot,
    });

    Ok(LoadImageResult {
        width: orig_width,
        height: orig_height,
        metadata,
        exif: exif_data,
        is_raw,
        as_shot_white_balance: as_shot.white_balance,
    })
}

#[tauri::command]
pub async fn prefetch_adjacent_image(
    path: String,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let (source_path, _) = parse_virtual_path(&path);
    let source_path_str = source_path.to_string_lossy().to_string();

    if state.decoded_image_cache.lock().unwrap_or_else(|e| e.into_inner()).get(&source_path_str).is_some() {
        return Ok(());
    }

    let start_gen = state.load_image_generation.load(Ordering::SeqCst);
    let settings = load_settings(app_handle).unwrap_or_default();
    let path_clone = source_path_str.clone();
    let gen_tracker = state.load_image_generation.clone();

    let res = tokio::task::spawn_blocking(move || -> Result<(DynamicImage, HashMap<String, String>), String> {
        if gen_tracker.load(Ordering::SeqCst) != start_gen {
            return Err("Prefetch superseded".to_string());
        }
        let bytes = fs::read(&path_clone).map_err(|e| e.to_string())?;
        if gen_tracker.load(Ordering::SeqCst) != start_gen {
            return Err("Prefetch superseded".to_string());
        }
        let cancel_token = Some((gen_tracker.clone(), start_gen));
        let img = load_base_image_from_bytes(&bytes, &path_clone, false, &settings, cancel_token)
            .map_err(|e| e.to_string())?;
        let exif = exif_processing::read_exif_data(&path_clone, &bytes);
        let _ = crate::as_shot::as_shot_with_bytes(&path_clone, &bytes);
        Ok((img, exif))
    })
    .await
    .map_err(|e| e.to_string())?;

    if let Ok((img, exif)) = res {
        if state.load_image_generation.load(Ordering::SeqCst) == start_gen {
            state.decoded_image_cache.lock().unwrap_or_else(|e| e.into_inner()).insert(
                source_path_str,
                Arc::new(img),
                exif,
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod linear_dng_and_cr2_tests {
    use super::*;

    #[test]
    fn rejects_canon_cr2_and_non_dng_rgb_ifds() {
        // Synthetic Canon CR2 header: "II*\0" + IFD0 at 16 + "CR\x02\x00" at 8,
        // with IFD0 having PhotometricInterpretation = 2 (RGB) and SamplesPerPixel = 3
        let mut cr2 = vec![0u8; 64];
        cr2[0..4].copy_from_slice(&[0x49, 0x49, 0x2A, 0x00]);
        cr2[4..8].copy_from_slice(&16u32.to_le_bytes());
        cr2[8..12].copy_from_slice(b"CR\x02\x00");
        cr2[16..18].copy_from_slice(&2u16.to_le_bytes());
        // Entry 0: PhotometricInterpretation (0x0106) = SHORT(3), count=1, val=2
        cr2[18..20].copy_from_slice(&0x0106u16.to_le_bytes());
        cr2[20..22].copy_from_slice(&3u16.to_le_bytes());
        cr2[22..26].copy_from_slice(&1u32.to_le_bytes());
        cr2[26..28].copy_from_slice(&2u16.to_le_bytes());
        // Entry 1: SamplesPerPixel (0x0115) = SHORT(3), count=1, val=3
        cr2[30..32].copy_from_slice(&0x0115u16.to_le_bytes());
        cr2[32..34].copy_from_slice(&3u16.to_le_bytes());
        cr2[34..38].copy_from_slice(&1u32.to_le_bytes());
        cr2[38..40].copy_from_slice(&3u16.to_le_bytes());

        assert!(!is_linear_dng_probe(&cr2));
        assert!(!is_linear_dng_file("IMG_3976.CR2", &cr2));

        // Even without "CR" magic (e.g. Nikon NEF / Sony ARW IFD0 RGB preview), missing DNGVersion (0xC612)
        // and PhotometricInterpretation == 2 must NEVER be classified as Linear DNG
        cr2[8..12].copy_from_slice(&[0, 0, 0, 0]);
        assert!(!is_linear_dng_probe(&cr2));
        assert!(!is_linear_dng_file("DSC_0001.NEF", &cr2));
        assert!(!is_linear_dng_file("DSC_0001.DNG", &cr2));
    }

    #[test]
    fn roundtrips_32bit_float_linear_dng_preserving_hdr_headroom() {
        let mut hdr = image::Rgb32FImage::new(8, 6);
        for (x, y, px) in hdr.enumerate_pixels_mut() {
            *px = image::Rgb([
                0.25 + (x as f32) * 0.5, // exceeds 1.0 (up to 3.75 HDR linear radiance)
                0.15 + (y as f32) * 0.2,
                2.45,
            ]);
        }

        let dng_bytes = crate::dng_encoder::encode_linear_dng(&hdr, None)
            .expect("encode_linear_dng must succeed");

        assert!(is_linear_dng_probe(&dng_bytes));
        assert!(is_linear_dng_file("merged_hdr.dng", &dng_bytes));
        assert!(is_linear_dng_file("merged_hdr.DNG?vc=1", &dng_bytes));
        assert!(!is_linear_dng_file("not_a_dng.CR2", &dng_bytes));

        let as_shot = crate::raw_processing::read_as_shot(&dng_bytes);
        assert!(as_shot.is_some());
        assert_eq!(
            crate::raw_processing::get_fast_demosaic_scale_factor(&dng_bytes, 8, 6),
            1.0
        );

        let decoded = load_linear_dng(&dng_bytes, None).expect("load_linear_dng must succeed");
        let rgb32 = decoded.to_rgb32f();
        assert_eq!(rgb32.dimensions(), (8, 6));
        let sample = rgb32.get_pixel(7, 5);
        assert!((sample[0] - 3.75).abs() < 1e-5, "HDR float R lost: {}", sample[0]);
        assert!((sample[2] - 2.45).abs() < 1e-5, "HDR float B lost: {}", sample[2]);
    }

    #[test]
    fn readonly_real_canon_cr2_thumbnail_and_raw_decode_regression() {
        let cr2_path = Path::new(r"D:\neapdirbti\IMG_3976.CR2");
        if !cr2_path.exists() {
            return;
        }

        // 1. Fast seek-based embedded preview extraction (thumbnails)
        let t0 = Instant::now();
        let thumb = crate::file_management::try_load_embedded_raw_preview(cr2_path, 480)
            .expect("Canon CR2 embedded JPEG preview must extract cleanly");
        let thumb_ms = t0.elapsed().as_millis();
        assert!(
            thumb.width().max(thumb.height()) >= 480,
            "Unexpected CR2 preview dimensions: {:?}",
            thumb.dimensions()
        );
        println!(
            "Verified Canon CR2 embedded preview {:?} in {} ms",
            thumb.dimensions(),
            thumb_ms
        );

        // 2. Full RAW decode via rawler (single photo edit) — strictly read-only
        let bytes = std::fs::read(cr2_path).expect("Read CR2 bytes");
        assert!(!is_linear_dng_probe(&bytes));
        assert!(!is_linear_dng_file(r"D:\neapdirbti\IMG_3976.CR2", &bytes));

        let fast_decoded = crate::raw_processing::develop_raw_image(
            &bytes,
            true,
            2.5,
            "calibrated".to_string(),
            None,
        )
        .expect("Canon CR2 fast rawler decode must succeed without TIFF photometric error");
        assert!(fast_decoded.width() >= 1000 && fast_decoded.height() >= 900);

        let full_decoded = crate::raw_processing::develop_raw_image(
            &bytes,
            false,
            2.5,
            "calibrated".to_string(),
            None,
        )
        .expect("Canon CR2 full-res rawler decode must succeed without TIFF photometric error");
        println!("Verified Canon CR2 full RAW decode dimensions: {:?}", full_decoded.dimensions());
        assert!(full_decoded.width() >= 5900 && full_decoded.height() >= 3900);
    }
}
