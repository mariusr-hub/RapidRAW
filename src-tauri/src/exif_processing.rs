use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::file_management;
use crate::formats::is_raw_file;
use crate::image_processing::{ImageFlag, ImageMetadata};
use crate::tagging::{COLOR_TAG_PREFIX, USER_TAG_PREFIX};
use chrono::{DateTime, Local, LocalResult, NaiveDateTime, TimeZone, Utc};
use exif::{Exif, In, Value};
use image::ImageDecoder;
use little_exif::exif_tag::ExifTag;
use little_exif::filetype::FileExtension;
use little_exif::ifd::ExifTagGroup;
use little_exif::metadata::Metadata;
use little_exif::rational::{iR64, uR64};
use quick_xml::escape::{escape, unescape};
use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use rawler::decoders::RawMetadata;
use rawler::exif::ExifGPS;
use rawler::lens::LensDescription;
use serde::{Deserialize, Serialize};

pub fn extract_focal_length_from_file(original_path: &Path) -> Option<f64> {
    if let Ok(raw_source) = rawler::rawsource::RawSource::new(original_path) {
        let loader = rawler::RawLoader::new();
        if let Ok(decoder) = loader.get_decoder(&raw_source) {
            if let Ok(meta) = decoder.raw_metadata(&raw_source, &Default::default()) {
                if let Some(fl) = meta.exif.focal_length {
                    if fl.d > 0 {
                        return Some(fl.n as f64 / fl.d as f64);
                    }
                }
            }
        }
    }
    if let Ok(file) = std::fs::File::open(original_path) {
        let mut bufreader = std::io::BufReader::new(file);
        let exifreader = exif::Reader::new();
        if let Ok(exif) = exifreader.read_from_container(&mut bufreader) {
            if let Some(field) = exif.get_field(exif::Tag::FocalLength, exif::In::PRIMARY) {
                match field.value {
                    exif::Value::Rational(ref v) if !v.is_empty() => {
                        return Some(v[0].num as f64 / v[0].denom.max(1) as f64);
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

pub fn build_standard_exif_app1_segment(original_path: &Path) -> Option<Vec<u8>> {
    let loader = rawler::RawLoader::new();
    let raw_source = rawler::rawsource::RawSource::new(original_path).ok()?;
    let decoder = loader.get_decoder(&raw_source).ok()?;
    let meta = decoder.raw_metadata(&raw_source, &Default::default()).ok()?;

    let make = if !meta.make.is_empty() { meta.make.trim().to_string() } else { "Canon".to_string() };
    let model = if !meta.model.is_empty() { meta.model.trim().to_string() } else { "EOS Camera".to_string() };
    let software = "RapidRAW".to_string();
    let lens = meta.exif.lens_model.unwrap_or_else(|| "Standard Lens".to_string());
    let dt = meta.exif.date_time_original.unwrap_or_else(|| "2026:08:27 14:15:32".to_string());
    let iso = meta.exif.iso_speed.unwrap_or(100) as u16;
    let (exp_num, exp_den) = meta.exif.exposure_time.map(|t| (t.n, t.d)).unwrap_or((1, 100));
    let (fn_num, fn_den) = meta.exif.fnumber.map(|f| (f.n, f.d)).unwrap_or((28, 10));
    let (fl_num, fl_den) = meta.exif.focal_length.map(|fl| (fl.n, fl.d)).unwrap_or((50, 1));

    let mut tiff_buf = Vec::new();
    // 1. TIFF Header: Little Endian ("II*\0", IFD0 offset = 8)
    tiff_buf.extend_from_slice(b"II*\0");
    tiff_buf.extend_from_slice(&8u32.to_le_bytes());

    // Number of tags in IFD0: 8 (Make, Model, Orientation, XResolution, YResolution, ResolutionUnit, Software, ExifIFDPointer)
    let num_ifd0 = 8u16;
    tiff_buf.extend_from_slice(&num_ifd0.to_le_bytes());

    let mut val_offset = 8u32 + 2 + 8 * 12 + 4; // 110 bytes

    let make_bytes = format!("{}\0", make).into_bytes();
    let model_bytes = format!("{}\0", model).into_bytes();
    let software_bytes = format!("{}\0", software).into_bytes();

    let make_off = val_offset; val_offset += make_bytes.len() as u32;
    let model_off = val_offset; val_offset += model_bytes.len() as u32;
    let soft_off = val_offset; val_offset += software_bytes.len() as u32;

    // 4-byte align for resolution rationals
    val_offset = (val_offset + 3) & !3;
    let x_res_off = val_offset; val_offset += 8;
    let y_res_off = val_offset; val_offset += 8;

    let exif_ifd_off = (val_offset + 3) & !3; // 4-byte align

    // Tag 0x010F: Make (ASCII)
    tiff_buf.extend_from_slice(&0x010Fu16.to_le_bytes());
    tiff_buf.extend_from_slice(&2u16.to_le_bytes());
    tiff_buf.extend_from_slice(&(make_bytes.len() as u32).to_le_bytes());
    tiff_buf.extend_from_slice(&make_off.to_le_bytes());

    // Tag 0x0110: Model (ASCII)
    tiff_buf.extend_from_slice(&0x0110u16.to_le_bytes());
    tiff_buf.extend_from_slice(&2u16.to_le_bytes());
    tiff_buf.extend_from_slice(&(model_bytes.len() as u32).to_le_bytes());
    tiff_buf.extend_from_slice(&model_off.to_le_bytes());

    // Tag 0x0112: Orientation (SHORT, count 1, value 1)
    tiff_buf.extend_from_slice(&0x0112u16.to_le_bytes());
    tiff_buf.extend_from_slice(&3u16.to_le_bytes());
    tiff_buf.extend_from_slice(&1u32.to_le_bytes());
    tiff_buf.extend_from_slice(&1u32.to_le_bytes());

    // Tag 0x011A: XResolution (RATIONAL, count 1, [300, 1])
    tiff_buf.extend_from_slice(&0x011Au16.to_le_bytes());
    tiff_buf.extend_from_slice(&5u16.to_le_bytes());
    tiff_buf.extend_from_slice(&1u32.to_le_bytes());
    tiff_buf.extend_from_slice(&x_res_off.to_le_bytes());

    // Tag 0x011B: YResolution (RATIONAL, count 1, [300, 1])
    tiff_buf.extend_from_slice(&0x011Bu16.to_le_bytes());
    tiff_buf.extend_from_slice(&5u16.to_le_bytes());
    tiff_buf.extend_from_slice(&1u32.to_le_bytes());
    tiff_buf.extend_from_slice(&y_res_off.to_le_bytes());

    // Tag 0x0128: ResolutionUnit (SHORT, count 1, value 2 = Inches)
    tiff_buf.extend_from_slice(&0x0128u16.to_le_bytes());
    tiff_buf.extend_from_slice(&3u16.to_le_bytes());
    tiff_buf.extend_from_slice(&1u32.to_le_bytes());
    tiff_buf.extend_from_slice(&2u32.to_le_bytes());

    // Tag 0x0131: Software (ASCII)
    tiff_buf.extend_from_slice(&0x0131u16.to_le_bytes());
    tiff_buf.extend_from_slice(&2u16.to_le_bytes());
    tiff_buf.extend_from_slice(&(software_bytes.len() as u32).to_le_bytes());
    tiff_buf.extend_from_slice(&soft_off.to_le_bytes());

    // Tag 0x8769: ExifIFDPointer (LONG, count 1)
    tiff_buf.extend_from_slice(&0x8769u16.to_le_bytes());
    tiff_buf.extend_from_slice(&4u16.to_le_bytes());
    tiff_buf.extend_from_slice(&1u32.to_le_bytes());
    tiff_buf.extend_from_slice(&exif_ifd_off.to_le_bytes());

    // Next IFD offset: 0
    tiff_buf.extend_from_slice(&0u32.to_le_bytes());

    // Append IFD0 value bytes
    tiff_buf.extend_from_slice(&make_bytes);
    tiff_buf.extend_from_slice(&model_bytes);
    tiff_buf.extend_from_slice(&software_bytes);

    // Pad to x_res_off
    while tiff_buf.len() < (x_res_off as usize) {
        tiff_buf.push(0);
    }
    // Append XResolution [300, 1]
    tiff_buf.extend_from_slice(&300u32.to_le_bytes());
    tiff_buf.extend_from_slice(&1u32.to_le_bytes());

    // Pad to y_res_off
    while tiff_buf.len() < (y_res_off as usize) {
        tiff_buf.push(0);
    }
    // Append YResolution [300, 1]
    tiff_buf.extend_from_slice(&300u32.to_le_bytes());
    tiff_buf.extend_from_slice(&1u32.to_le_bytes());

    // Pad to exif_ifd_off
    while tiff_buf.len() < (exif_ifd_off as usize) {
        tiff_buf.push(0);
    }

    // Exif SubIFD (7 tags)
    let num_sub = 7u16;
    tiff_buf.extend_from_slice(&num_sub.to_le_bytes());

    let mut sub_val_offset = exif_ifd_off + 2 + 7 * 12 + 4;
    let dt_bytes = format!("{}\0", dt).into_bytes();
    let lens_bytes = format!("{}\0", lens).into_bytes();

    let exp_off = sub_val_offset; sub_val_offset += 8;
    let fn_off = sub_val_offset; sub_val_offset += 8;
    let fl_off = sub_val_offset; sub_val_offset += 8;
    let dt_off = sub_val_offset; sub_val_offset += dt_bytes.len() as u32;
    let lens_off = sub_val_offset;

    // Tag 0x829A: ExposureTime (RATIONAL)
    tiff_buf.extend_from_slice(&0x829Au16.to_le_bytes());
    tiff_buf.extend_from_slice(&5u16.to_le_bytes());
    tiff_buf.extend_from_slice(&1u32.to_le_bytes());
    tiff_buf.extend_from_slice(&exp_off.to_le_bytes());

    // Tag 0x829D: FNumber (RATIONAL)
    tiff_buf.extend_from_slice(&0x829Du16.to_le_bytes());
    tiff_buf.extend_from_slice(&5u16.to_le_bytes());
    tiff_buf.extend_from_slice(&1u32.to_le_bytes());
    tiff_buf.extend_from_slice(&fn_off.to_le_bytes());

    // Tag 0x8827: ISOSpeedRatings (SHORT)
    tiff_buf.extend_from_slice(&0x8827u16.to_le_bytes());
    tiff_buf.extend_from_slice(&3u16.to_le_bytes());
    tiff_buf.extend_from_slice(&1u32.to_le_bytes());
    tiff_buf.extend_from_slice(&(iso as u32).to_le_bytes());

    // Tag 0x9003: DateTimeOriginal (ASCII)
    tiff_buf.extend_from_slice(&0x9003u16.to_le_bytes());
    tiff_buf.extend_from_slice(&2u16.to_le_bytes());
    tiff_buf.extend_from_slice(&(dt_bytes.len() as u32).to_le_bytes());
    tiff_buf.extend_from_slice(&dt_off.to_le_bytes());

    // Tag 0x920A: FocalLength (RATIONAL)
    tiff_buf.extend_from_slice(&0x920Au16.to_le_bytes());
    tiff_buf.extend_from_slice(&5u16.to_le_bytes());
    tiff_buf.extend_from_slice(&1u32.to_le_bytes());
    tiff_buf.extend_from_slice(&fl_off.to_le_bytes());

    // Tag 0xA001: ColorSpace (SHORT, value 1 = sRGB)
    tiff_buf.extend_from_slice(&0xA001u16.to_le_bytes());
    tiff_buf.extend_from_slice(&3u16.to_le_bytes());
    tiff_buf.extend_from_slice(&1u32.to_le_bytes());
    tiff_buf.extend_from_slice(&1u32.to_le_bytes());

    // Tag 0xA434: LensModel (ASCII)
    tiff_buf.extend_from_slice(&0xA434u16.to_le_bytes());
    tiff_buf.extend_from_slice(&2u16.to_le_bytes());
    tiff_buf.extend_from_slice(&(lens_bytes.len() as u32).to_le_bytes());
    tiff_buf.extend_from_slice(&lens_off.to_le_bytes());

    // Next SubIFD offset: 0
    tiff_buf.extend_from_slice(&0u32.to_le_bytes());

    // Append SubIFD values:
    tiff_buf.extend_from_slice(&exp_num.to_le_bytes());
    tiff_buf.extend_from_slice(&exp_den.to_le_bytes());
    tiff_buf.extend_from_slice(&fn_num.to_le_bytes());
    tiff_buf.extend_from_slice(&fn_den.to_le_bytes());
    tiff_buf.extend_from_slice(&fl_num.to_le_bytes());
    tiff_buf.extend_from_slice(&fl_den.to_le_bytes());
    tiff_buf.extend_from_slice(&dt_bytes);
    tiff_buf.extend_from_slice(&lens_bytes);

    // Assemble final APP1 marker
    let payload_len = 6 + tiff_buf.len();
    let marker_len = payload_len + 2;
    let mut app1 = Vec::with_capacity(marker_len + 2);
    app1.push(0xFF);
    app1.push(0xE1);
    app1.push(((marker_len >> 8) & 0xFF) as u8);
    app1.push((marker_len & 0xFF) as u8);
    app1.extend_from_slice(b"Exif\0\0");
    app1.extend_from_slice(&tiff_buf);

    Some(app1)
}

pub const STANDARD_SRGB_ICC_PROFILE: &[u8] = &[
    0x00, 0x00, 0x01, 0xE6, 0x61, 0x70, 0x70, 0x6C, 0x02, 0x10, 0x00, 0x00, 0x6D, 0x6E, 0x74, 0x72,
    0x52, 0x47, 0x42, 0x20, 0x58, 0x59, 0x5A, 0x20, 0x07, 0xEA, 0x00, 0x08, 0x00, 0x1F, 0x00, 0x0C,
    0x00, 0x00, 0x00, 0x00, 0x61, 0x63, 0x73, 0x70, 0x4D, 0x53, 0x46, 0x54, 0x00, 0x00, 0x00, 0x00,
    0x6E, 0x6F, 0x6E, 0x65, 0x6E, 0x6F, 0x6E, 0x65, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF6, 0xD6, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0xD3, 0x2D,
    0x52, 0x50, 0x52, 0x57, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x09, 0x64, 0x65, 0x73, 0x63, 0x00, 0x00, 0x00, 0xF0, 0x00, 0x00, 0x00, 0x5F,
    0x63, 0x70, 0x72, 0x74, 0x00, 0x00, 0x01, 0x50, 0x00, 0x00, 0x00, 0x16, 0x77, 0x74, 0x70, 0x74,
    0x00, 0x00, 0x01, 0x68, 0x00, 0x00, 0x00, 0x14, 0x72, 0x58, 0x59, 0x5A, 0x00, 0x00, 0x01, 0x7C,
    0x00, 0x00, 0x00, 0x14, 0x67, 0x58, 0x59, 0x5A, 0x00, 0x00, 0x01, 0x90, 0x00, 0x00, 0x00, 0x14,
    0x62, 0x58, 0x59, 0x5A, 0x00, 0x00, 0x01, 0xA4, 0x00, 0x00, 0x00, 0x14, 0x72, 0x54, 0x52, 0x43,
    0x00, 0x00, 0x01, 0xB8, 0x00, 0x00, 0x00, 0x0E, 0x67, 0x54, 0x52, 0x43, 0x00, 0x00, 0x01, 0xC8,
    0x00, 0x00, 0x00, 0x0E, 0x62, 0x54, 0x52, 0x43, 0x00, 0x00, 0x01, 0xD8, 0x00, 0x00, 0x00, 0x0E,
    0x64, 0x65, 0x73, 0x63, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x73, 0x52, 0x47, 0x42,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x74, 0x65, 0x78, 0x74, 0x00, 0x00, 0x00, 0x00, 0x52, 0x61, 0x70, 0x69, 0x64, 0x52, 0x41, 0x57,
    0x20, 0x73, 0x52, 0x47, 0x42, 0x00, 0x00, 0x00, 0x58, 0x59, 0x5A, 0x20, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0xF6, 0xD6, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0xD3, 0x2D, 0x58, 0x59, 0x5A, 0x20,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x6F, 0xA2, 0x00, 0x00, 0x38, 0xF4, 0x00, 0x00, 0x03, 0x8F,
    0x58, 0x59, 0x5A, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x62, 0x99, 0x00, 0x00, 0xB7, 0x85,
    0x00, 0x00, 0x18, 0xD9, 0x58, 0x59, 0x5A, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x24, 0x9F,
    0x00, 0x00, 0x0F, 0x83, 0x00, 0x00, 0xB6, 0xCF, 0x63, 0x75, 0x72, 0x76, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x01, 0x02, 0x33, 0x00, 0x00, 0x63, 0x75, 0x72, 0x76, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x01, 0x02, 0x33, 0x00, 0x00, 0x63, 0x75, 0x72, 0x76, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x01, 0x02, 0x33,
];

pub fn build_standard_icc_app2_segment() -> Vec<u8> {
    let prefix = b"ICC_PROFILE\0\x01\x01";
    let payload_len = prefix.len() + STANDARD_SRGB_ICC_PROFILE.len();
    let marker_len = payload_len + 2;
    let mut app2 = Vec::with_capacity(marker_len + 2);
    app2.push(0xFF);
    app2.push(0xE2);
    app2.push(((marker_len >> 8) & 0xFF) as u8);
    app2.push((marker_len & 0xFF) as u8);
    app2.extend_from_slice(prefix);
    app2.extend_from_slice(STANDARD_SRGB_ICC_PROFILE);
    app2
}

pub fn truncate_large_exif(value: &str) -> String {
    if value.len() <= 500 {
        return value.to_string();
    }

    let mut start_idx = 200;
    while !value.is_char_boundary(start_idx) {
        start_idx -= 1;
    }

    let mut end_idx = value.len() - 200;
    while !value.is_char_boundary(end_idx) {
        end_idx += 1;
    }

    if start_idx < end_idx {
        let start_str = &value[..start_idx];
        let end_str = &value[end_idx..];
        return format!("{}...{}", start_str, end_str);
    }

    value.to_string()
}

pub fn load_sidecar(sidecar_path: &Path) -> ImageMetadata {
    if !sidecar_path.exists() {
        return ImageMetadata::default();
    }

    let Ok(content) = fs::read_to_string(sidecar_path) else {
        return ImageMetadata::default();
    };

    let mut meta = serde_json::from_str::<ImageMetadata>(&content).unwrap_or_default();
    let mut healed = false;

    if let Some(ref mut exif_map) = meta.exif {
        for val in exif_map.values_mut() {
            if val.len() > 500 {
                *val = truncate_large_exif(val);
                healed = true;
            }
        }
    }

    if healed && let Ok(json) = serde_json::to_string_pretty(&meta) {
        let _ = fs::write(sidecar_path, json);
        log::info!(
            "Auto-healed bloated sidecar for: {}",
            sidecar_path.display()
        );
    }

    meta
}

pub fn load_sidecar_with_exif(sidecar_path: &Path, source_path: &Path) -> ImageMetadata {
    let mut meta = load_sidecar(sidecar_path);

    if meta.exif.is_none() {
        if let Some(cached_exif) = read_rrexif_sidecar(source_path) {
            meta.exif = Some(cached_exif);
        } else {
            let source_path_str = source_path.to_string_lossy();
            let extracted_exif =
                if let Ok(mmap) = crate::file_management::read_file_mapped(source_path) {
                    read_exif_data(&source_path_str, &mmap)
                } else if let Ok(bytes) = std::fs::read(source_path) {
                    read_exif_data(&source_path_str, &bytes)
                } else {
                    std::collections::HashMap::new()
                };

            if !extracted_exif.is_empty() {
                meta.exif = Some(extracted_exif);
            }
        }
    }

    meta
}

pub static SIDECAR_LAYOUT_LOCK: Mutex<()> = Mutex::new(());

pub fn write_sidecar(
    sidecar_path: &Path,
    source_path: &Path,
    metadata: &ImageMetadata,
) -> std::io::Result<()> {
    if is_default_sidecar(sidecar_path, source_path, metadata) {
        let _layout = SIDECAR_LAYOUT_LOCK.lock().unwrap();
        if !has_virtual_copy_sidecars(source_path) {
            return match fs::remove_file(sidecar_path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
    }

    let json = serde_json::to_string_pretty(metadata).map_err(std::io::Error::other)?;
    fs::write(sidecar_path, json)
}

fn has_virtual_copy_sidecars(source_path: &Path) -> bool {
    let (Some(parent), Some(file_name)) = (source_path.parent(), source_path.file_name()) else {
        return true;
    };
    let file_name = file_name.to_string_lossy();

    let Ok(entries) = fs::read_dir(parent) else {
        return true;
    };
    entries.filter_map(Result::ok).any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        matches!(
            file_management::parse_sidecar_name(&name),
            Some((source, Some(_))) if source == file_name
        )
    })
}

fn is_default_sidecar(sidecar_path: &Path, source_path: &Path, metadata: &ImageMetadata) -> bool {
    if sidecar_path != get_primary_sidecar_path(source_path) {
        return false;
    }
    if metadata.rating != 0
        || metadata.flag.is_some()
        || metadata.tags.as_ref().is_some_and(|tags| !tags.is_empty())
        || !crate::image_processing::is_default_adjustments(&metadata.adjustments)
    {
        return false;
    }

    let Some(sidecar_exif) = &metadata.exif else {
        return true;
    };

    if file_management::is_cloud_placeholder(source_path) {
        return false;
    }
    let source_path_str = source_path.to_string_lossy();
    let file_exif = if let Ok(mmap) = file_management::read_file_mapped(source_path) {
        read_exif_data_from_bytes(&source_path_str, &mmap)
    } else if let Ok(bytes) = fs::read(source_path) {
        read_exif_data_from_bytes(&source_path_str, &bytes)
    } else {
        return false;
    };

    !file_exif.is_empty() && &file_exif == sidecar_exif
}

fn to_ur64(val: &exif::Rational) -> uR64 {
    uR64 {
        nominator: val.num,
        denominator: val.denom,
    }
}

fn to_ir64(val: &exif::SRational) -> iR64 {
    iR64 {
        nominator: val.num,
        denominator: val.denom,
    }
}

fn rawler_to_ur64(val: rawler::formats::tiff::Rational) -> uR64 {
    uR64 {
        nominator: val.n,
        denominator: val.d,
    }
}

fn rawler_to_ir64(val: rawler::formats::tiff::SRational) -> iR64 {
    iR64 {
        nominator: val.n,
        denominator: val.d,
    }
}

fn clean_creation_datetime_str(s: &str) -> &str {
    s.trim().trim_matches('"').trim_matches('\'').trim()
}

const EXIF_DATETIME_FORMAT: &str = "%Y:%m:%d %H:%M:%S";

fn format_creation_datetime(s: &str, format: &str) -> String {
    if let Some(dt) = parse_creation_datetime(s) {
        return dt.format(format).to_string();
    }
    clean_creation_datetime_str(s).to_string()
}

fn fmt_date_str(s: String) -> String {
    format_creation_datetime(&s, "%Y-%m-%d %H:%M:%S")
}

fn normalize_creation_datetime(s: &str) -> Option<String> {
    let normalized = s.replace('T', " ");
    let (date, time) = normalized.split_once(' ')?;
    Some(format!("{} {}", date.replace(':', "-"), time))
}

fn parse_creation_datetime(s: &str) -> Option<NaiveDateTime> {
    let clean = clean_creation_datetime_str(s);
    if clean.is_empty() {
        return None;
    }

    let normalized = normalize_creation_datetime(clean);
    for candidate in std::iter::once(clean).chain(normalized.as_deref()) {
        for format in [
            "%Y:%m:%d %H:%M:%S",
            "%Y:%m:%d %H:%M:%S%.f",
            "%Y-%m-%d %H:%M:%S",
            "%Y-%m-%d %H:%M:%S%.f",
        ] {
            if let Ok(dt) = NaiveDateTime::parse_from_str(candidate, format) {
                return Some(dt);
            }
        }
    }

    None
}

fn parse_creation_field(field: &exif::Field) -> Option<DateTime<Utc>> {
    parse_creation_datetime(&field.display_value().to_string())
        .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
}

fn parse_raw_creation_date(date_str: Option<&str>) -> Option<DateTime<Utc>> {
    parse_creation_datetime(date_str?).map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
}

fn clean_ascii_value(value: &exif::Value) -> Option<String> {
    let exif::Value::Ascii(ref components) = *value else {
        return None;
    };

    let cleaned: Vec<String> = components
        .iter()
        .map(|c| {
            String::from_utf8_lossy(c)
                .trim_matches(char::from(0))
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect();

    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.join(" "))
    }
}

fn rational_to_f32_checked(r: &exif::Rational) -> Option<f32> {
    if r.denom == 0 {
        None
    } else {
        Some(r.num as f32 / r.denom as f32)
    }
}

fn rawler_rational_to_f32_checked(r: &rawler::formats::tiff::Rational) -> Option<f32> {
    if r.d == 0 {
        None
    } else {
        Some(r.n as f32 / r.d as f32)
    }
}

fn format_min_max(min: f32, max: f32, tolerance: f32) -> String {
    if (min - max).abs() < tolerance {
        format!("{min}")
    } else {
        format!("{min}-{max}")
    }
}

fn format_lens_spec(focal_min: f32, focal_max: f32, aperture: Option<(f32, f32)>) -> String {
    let mut spec = format!("{} mm", format_min_max(focal_min, focal_max, 0.01));
    if let Some((amin, amax)) = aperture {
        spec.push_str(&format!(", f/{}", format_min_max(amin, amax, 0.01)));
    }
    spec
}

fn format_lens_specification(components: &[exif::Rational]) -> Option<String> {
    if components.len() < 4 {
        return None;
    }

    let focal_min = rational_to_f32_checked(&components[0])?;
    let focal_max = rational_to_f32_checked(&components[1])?;
    let aperture =
        rational_to_f32_checked(&components[2]).zip(rational_to_f32_checked(&components[3]));

    Some(format_lens_spec(focal_min, focal_max, aperture))
}

pub fn read_exif(file_bytes: &[u8]) -> Option<Exif> {
    let exifreader = exif::Reader::new();
    exifreader
        .read_from_container(&mut Cursor::new(file_bytes))
        .ok()
}

pub fn read_raw_metadata(file_bytes: &[u8]) -> Option<RawMetadata> {
    raw_metadata_from_source(&rawler::rawsource::RawSource::new_from_slice(file_bytes))
}

fn read_raw_metadata_from_path(path_str: &str) -> Option<RawMetadata> {
    raw_metadata_from_source(&rawler::rawsource::RawSource::new(Path::new(path_str)).ok()?)
}

fn raw_metadata_from_source(raw_source: &rawler::rawsource::RawSource) -> Option<RawMetadata> {
    let loader = rawler::RawLoader::new();
    let decoder = loader.get_decoder(raw_source).ok()?;
    decoder.raw_metadata(raw_source, &Default::default()).ok()
}

fn insert_missing_lens_fields(map: &mut HashMap<String, String>, lens: &LensDescription) {
    let rat =
        |r: &rawler::formats::tiff::Rational| rawler_rational_to_f32_checked(r).unwrap_or(0.0);

    let aperture = (rat(&lens.aperture_range[0]), rat(&lens.aperture_range[1]));
    let spec = format_lens_spec(
        rat(&lens.focal_range[0]),
        rat(&lens.focal_range[1]),
        Some(aperture).filter(|(amin, amax)| *amin > 0.0 || *amax > 0.0),
    );

    for (key, val) in [
        ("LensModel", lens.lens_model.as_str()),
        ("LensMake", lens.lens_make.as_str()),
        ("LensSpecification", spec.as_str()),
    ] {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            map.entry(key.to_string())
                .or_insert_with(|| truncate_large_exif(trimmed));
        }
    }
}

pub fn read_exposure_time_secs(path: &str, file_bytes: &[u8]) -> Option<f32> {
    if let Some(map) = read_rrexif_sidecar(Path::new(path))
        && let Some(val_str) = map.get("ExposureTime").or(map.get("ShutterSpeedValue"))
    {
        let cleaned = val_str.replace(" s", "");
        if cleaned.contains('/') {
            let parts: Vec<&str> = cleaned.split('/').collect();
            if parts.len() == 2
                && let (Ok(num), Ok(den)) = (parts[0].parse::<f32>(), parts[1].parse::<f32>())
                && den != 0.0
            {
                return Some(num / den);
            }
        } else if let Ok(val) = cleaned.parse::<f32>() {
            return Some(val);
        }
    }

    if is_raw_file(path)
        && let Some(meta) = read_raw_metadata(file_bytes)
    {
        if let Some(r) = meta.exif.exposure_time {
            return if r.d == 0 {
                None
            } else {
                Some(r.n as f32 / r.d as f32)
            };
        } else if let Some(r) = meta.exif.shutter_speed_value {
            return if r.d == 0 {
                None
            } else {
                Some(r.n as f32 / r.d as f32)
            };
        }
    }

    if let Some(exif) = read_exif(file_bytes) {
        if let Some(exposure) = exif.get_field(exif::Tag::ExposureTime, In::PRIMARY) {
            if let Value::Rational(ref r) = exposure.value {
                if r.is_empty() {
                    return None;
                }

                let val = r.first()?;

                return if val.denom == 0 {
                    None
                } else {
                    Some(val.num as f32 / val.denom as f32)
                };
            }
        } else if let Some(shutter_speed) =
            exif.get_field(exif::Tag::ShutterSpeedValue, In::PRIMARY)
            && let Value::Rational(ref r) = shutter_speed.value
        {
            if r.is_empty() {
                return None;
            }

            let val = r.first()?;

            return if val.denom == 0 {
                None
            } else {
                Some(val.num as f32 / val.denom as f32)
            };
        }
    }
    None
}

pub fn read_iso(path: &str, file_bytes: &[u8]) -> Option<u32> {
    if let Some(map) = read_rrexif_sidecar(Path::new(path))
        && let Some(val_str) = map
            .get("ISOSpeed")
            .or(map.get("PhotographicSensitivity"))
            .or(map.get("ISOSpeedRatings"))
        && let Ok(val) = val_str.parse::<u32>()
    {
        return Some(val);
    }

    if is_raw_file(path)
        && let Some(meta) = read_raw_metadata(file_bytes)
    {
        if let Some(r) = meta.exif.iso_speed {
            return Some(r);
        } else if let Some(r) = meta.exif.iso_speed_ratings {
            return Some(r as u32);
        }
    }

    if let Some(exif) = read_exif(file_bytes) {
        if let Some(r) = exif.get_field(exif::Tag::ISOSpeed, In::PRIMARY) {
            return r.value.get_uint(0);
        } else if let Some(r) = exif.get_field(exif::Tag::PhotographicSensitivity, In::PRIMARY) {
            return r.value.get_uint(0);
        }
    }
    None
}

pub fn read_f_number(path: &str, file_bytes: &[u8]) -> Option<f32> {
    if let Some(map) = read_rrexif_sidecar(Path::new(path))
        && let Some(val_str) = map.get("FNumber").or(map.get("ApertureValue"))
    {
        let cleaned = val_str.replace("f/", "").replace("F/", "").trim().to_string();
        if let Ok(val) = cleaned.parse::<f32>() {
            if val > 0.1 && val < 128.0 {
                return Some(val);
            }
        }
    }

    if is_raw_file(path)
        && let Some(meta) = read_raw_metadata(file_bytes)
    {
        if let Some(r) = meta.exif.fnumber {
            if r.d != 0 {
                let val = r.n as f32 / r.d as f32;
                if val > 0.1 && val < 128.0 {
                    return Some(val);
                }
            }
        } else if let Some(r) = meta.exif.aperture_value {
            if r.d != 0 {
                let apex = r.n as f32 / r.d as f32;
                let val = (2.0f32).powf(apex / 2.0);
                if val > 0.1 && val < 128.0 {
                    return Some(val);
                }
            }
        }
    }

    if let Some(exif) = read_exif(file_bytes) {
        if let Some(f_field) = exif.get_field(exif::Tag::FNumber, In::PRIMARY) {
            if let Value::Rational(ref r) = f_field.value {
                if let Some(val) = r.first() {
                    if val.denom != 0 {
                        let res = val.num as f32 / val.denom as f32;
                        if res > 0.1 && res < 128.0 {
                            return Some(res);
                        }
                    }
                }
            }
        } else if let Some(a_field) = exif.get_field(exif::Tag::ApertureValue, In::PRIMARY) {
            if let Value::Rational(ref r) = a_field.value {
                if let Some(val) = r.first() {
                    if val.denom != 0 {
                        let apex = val.num as f32 / val.denom as f32;
                        let res = (2.0f32).powf(apex / 2.0);
                        if res > 0.1 && res < 128.0 {
                            return Some(res);
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn read_camera_make_model(path: &str, file_bytes: &[u8]) -> Option<(String, String)> {
    if let Some(map) = read_rrexif_sidecar(Path::new(path)) {
        let make = map.get("Make").cloned().unwrap_or_default();
        let model = map.get("Model").cloned().unwrap_or_default();
        if !make.is_empty() || !model.is_empty() {
            return Some((make, model));
        }
    }

    if is_raw_file(path)
        && let Some(meta) = read_raw_metadata(file_bytes)
    {
        if !meta.make.is_empty() || !meta.model.is_empty() {
            return Some((meta.make.clone(), meta.model.clone()));
        }
    }

    if let Some(exif) = read_exif(file_bytes) {
        let make = exif.get_field(exif::Tag::Make, In::PRIMARY)
            .map(|f| f.display_value().to_string().replace('"', "").trim().to_string())
            .unwrap_or_default();
        let model = exif.get_field(exif::Tag::Model, In::PRIMARY)
            .map(|f| f.display_value().to_string().replace('"', "").trim().to_string())
            .unwrap_or_default();
        if !make.is_empty() || !model.is_empty() {
            return Some((make, model));
        }
    }
    None
}

pub fn extract_metadata(file_bytes: &[u8]) -> Option<HashMap<String, String>> {
    let mut map = HashMap::new();

    if let Some(exif_obj) = read_exif(file_bytes) {
        for field in exif_obj.fields() {
            match field.tag {
                exif::Tag::ExposureTime => {
                    if let exif::Value::Rational(ref v) = field.value
                        && !v.is_empty()
                    {
                        let r = &v[0];
                        if r.num == 1 && r.denom > 1 {
                            map.insert("ExposureTime".to_string(), format!("1/{} s", r.denom));
                        } else {
                            let val = r.num as f32 / r.denom as f32;
                            if val < 1.0 && val > 0.0 {
                                map.insert(
                                    "ExposureTime".to_string(),
                                    format!("1/{} s", (1.0 / val).round()),
                                );
                            } else {
                                map.insert("ExposureTime".to_string(), format!("{} s", val));
                            }
                        }
                    }
                }
                exif::Tag::ShutterSpeedValue => {
                    if let exif::Value::SRational(ref v) = field.value
                        && !v.is_empty()
                    {
                        let val = v[0].num as f32 / v[0].denom as f32;
                        map.insert("ShutterSpeedValue".to_string(), val.to_string());
                    }
                }
                exif::Tag::FNumber => {
                    if let exif::Value::Rational(ref v) = field.value
                        && !v.is_empty()
                    {
                        let val = v[0].num as f32 / v[0].denom as f32;
                        map.insert("FNumber".to_string(), format!("f/{}", val));
                    }
                }
                exif::Tag::ApertureValue => {
                    if let exif::Value::Rational(ref v) = field.value
                        && !v.is_empty()
                    {
                        let val = v[0].num as f32 / v[0].denom as f32;
                        map.insert("ApertureValue".to_string(), format!("f/{}", val));
                    }
                }
                exif::Tag::FocalLength => {
                    if let exif::Value::Rational(ref v) = field.value
                        && !v.is_empty()
                    {
                        let val = v[0].num as f32 / v[0].denom as f32;
                        map.insert("FocalLength".to_string(), val.to_string());
                        map.insert("FocalLengthIn35mmFilm".to_string(), val.to_string());
                    }
                }
                exif::Tag::PhotographicSensitivity | exif::Tag::ISOSpeed => {
                    map.insert(
                        "PhotographicSensitivity".to_string(),
                        field.display_value().to_string(),
                    );
                    map.insert("ISOSpeed".to_string(), field.display_value().to_string());
                }
                exif::Tag::DateTimeOriginal => {
                    map.insert(
                        "DateTimeOriginal".to_string(),
                        fmt_date_str(field.display_value().to_string()),
                    );
                }
                exif::Tag::DateTime => {
                    map.insert(
                        "CreateDate".to_string(),
                        fmt_date_str(field.display_value().to_string()),
                    );
                }
                exif::Tag::DateTimeDigitized => {
                    map.insert(
                        "ModifyDate".to_string(),
                        fmt_date_str(field.display_value().to_string()),
                    );
                }
                exif::Tag::LensSpecification => {
                    if let exif::Value::Rational(ref v) = field.value
                        && v.len() >= 4
                        && let (Some(focal_min), Some(focal_max)) = (
                            rational_to_f32_checked(&v[0]),
                            rational_to_f32_checked(&v[1]),
                        )
                    {
                        let aperture = match (
                            rational_to_f32_checked(&v[2]),
                            rational_to_f32_checked(&v[3]),
                        ) {
                            (Some(amin), Some(amax)) => Some((amin, amax)),
                            _ => read_raw_metadata(file_bytes).and_then(|meta| {
                                let lens_desc = meta.lens?;
                                let amin =
                                    rawler_rational_to_f32_checked(&lens_desc.aperture_range[0])?;
                                let amax =
                                    rawler_rational_to_f32_checked(&lens_desc.aperture_range[1])?;
                                Some((amin, amax))
                            }),
                        };

                        let aperture = aperture.filter(|(amin, amax)| *amin > 0.0 || *amax > 0.0);

                        map.insert(
                            "LensSpecification".to_string(),
                            format_lens_spec(focal_min, focal_max, aperture),
                        );
                    }
                }
                _ => match &field.value {
                    exif::Value::Ascii(_) => {
                        if let Some(val) = clean_ascii_value(&field.value) {
                            map.insert(field.tag.to_string(), val);
                        }
                    }
                    _ => {
                        let val = field.display_value().with_unit(&exif_obj).to_string();
                        if !val.trim().is_empty() {
                            map.insert(field.tag.to_string(), val);
                        }
                    }
                },
            }
        }
    }

    if !map.is_empty() {
        if !map.contains_key("LensModel")
            && let Some(lens) = read_raw_metadata(file_bytes).and_then(|meta| meta.lens)
        {
            insert_missing_lens_fields(&mut map, &lens);
        }
        return Some(map);
    }

    let metadata = read_raw_metadata(file_bytes)?;

    let exif = metadata.exif;

    let fmt_rat = |r: &rawler::formats::tiff::Rational| -> f32 {
        if r.d == 0 {
            0.0
        } else {
            r.n as f32 / r.d as f32
        }
    };

    let fmt_srat = |r: &rawler::formats::tiff::SRational| -> f32 {
        if r.d == 0 {
            0.0
        } else {
            r.n as f32 / r.d as f32
        }
    };

    let mut insert_if_present = |key: &str, val: String| {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            map.insert(key.to_string(), truncate_large_exif(trimmed));
        }
    };

    insert_if_present("Make", metadata.make);
    insert_if_present("Model", metadata.model);

    if let Some(v) = exif.artist {
        insert_if_present("Artist", v);
    }
    if let Some(v) = exif.copyright {
        insert_if_present("Copyright", v);
    }
    if let Some(v) = exif.owner_name {
        insert_if_present("OwnerName", v);
    }
    if let Some(v) = exif.serial_number {
        insert_if_present("SerialNumber", v);
    }
    if let Some(v) = exif.image_number {
        insert_if_present("ImageNumber", v.to_string());
    }
    if let Some(v) = exif.user_comment {
        insert_if_present("UserComment", v);
    }

    if let Some(v) = exif.date_time_original {
        insert_if_present("DateTimeOriginal", fmt_date_str(v));
    }
    if let Some(v) = exif.create_date {
        insert_if_present("CreateDate", fmt_date_str(v));
    }
    if let Some(v) = exif.modify_date {
        insert_if_present("ModifyDate", fmt_date_str(v));
    }

    if let Some(v) = exif.offset_time {
        insert_if_present("OffsetTime", v);
    }
    if let Some(v) = exif.offset_time_original {
        insert_if_present("OffsetTimeOriginal", v);
    }
    if let Some(v) = exif.offset_time_digitized {
        insert_if_present("OffsetTimeDigitized", v);
    }
    if let Some(v) = exif.sub_sec_time {
        insert_if_present("SubSecTime", v);
    }
    if let Some(v) = exif.sub_sec_time_original {
        insert_if_present("SubSecTimeOriginal", v);
    }
    if let Some(v) = exif.sub_sec_time_digitized {
        insert_if_present("SubSecTimeDigitized", v);
    }

    if let Some(v) = exif.lens_model {
        insert_if_present("LensModel", v);
    }

    if let Some(v) = exif.lens_make {
        insert_if_present("LensMake", v);
    }

    if let Some(v) = exif.lens_serial_number {
        insert_if_present("LensSerialNumber", v);
    }

    if let Some(v) = exif.orientation {
        insert_if_present("Orientation", v.to_string());
    }

    if let Some(r) = exif.fnumber {
        let val = fmt_rat(&r);
        insert_if_present("FNumber", format!("f/{}", val));
    }

    if let Some(r) = exif.aperture_value {
        let val = fmt_rat(&r);
        insert_if_present("ApertureValue", format!("f/{}", val));
    }

    if let Some(r) = exif.max_aperture_value {
        insert_if_present("MaxApertureValue", fmt_rat(&r).to_string());
    }

    if let Some(r) = exif.exposure_time {
        if r.n == 1 && r.d > 1 {
            insert_if_present("ExposureTime", format!("1/{} s", r.d));
        } else {
            let val = fmt_rat(&r);
            if val < 1.0 && val > 0.0 {
                insert_if_present("ExposureTime", format!("1/{} s", (1.0 / val).round()));
            } else {
                insert_if_present("ExposureTime", format!("{} s", val));
            }
        }
    }

    if let Some(r) = exif.shutter_speed_value {
        insert_if_present("ShutterSpeedValue", fmt_srat(&r).to_string());
    }

    if let Some(v) = exif.iso_speed {
        insert_if_present("PhotographicSensitivity", v.to_string());
        insert_if_present("ISOSpeed", v.to_string());
    } else if let Some(v) = exif.iso_speed_ratings {
        insert_if_present("PhotographicSensitivity", v.to_string());
        insert_if_present("ISOSpeedRatings", v.to_string());
    }

    if let Some(v) = exif.recommended_exposure_index {
        insert_if_present("RecommendedExposureIndex", v.to_string());
    }
    if let Some(v) = exif.sensitivity_type {
        insert_if_present("SensitivityType", v.to_string());
    }

    if let Some(r) = exif.focal_length {
        let val = fmt_rat(&r);
        insert_if_present("FocalLength", val.to_string());
        insert_if_present("FocalLengthIn35mmFilm", val.to_string());
    }

    if let Some(r) = exif.exposure_bias {
        insert_if_present("ExposureBiasValue", fmt_srat(&r).to_string());
    }

    if let Some(v) = exif.metering_mode {
        insert_if_present("MeteringMode", v.to_string());
    }
    if let Some(v) = exif.light_source {
        insert_if_present("LightSource", v.to_string());
    }
    if let Some(v) = exif.flash {
        insert_if_present("Flash", v.to_string());
    }
    if let Some(v) = exif.white_balance {
        insert_if_present("WhiteBalance", v.to_string());
    }
    if let Some(v) = exif.exposure_program {
        insert_if_present("ExposureProgram", v.to_string());
    }
    if let Some(v) = exif.exposure_mode {
        insert_if_present("ExposureMode", v.to_string());
    }
    if let Some(v) = exif.scene_capture_type {
        insert_if_present("SceneCaptureType", v.to_string());
    }
    if let Some(v) = exif.color_space {
        insert_if_present("ColorSpace", v.to_string());
    }
    if let Some(r) = exif.flash_energy {
        insert_if_present("FlashEnergy", fmt_rat(&r).to_string());
    }
    if let Some(r) = exif.brightness_value {
        insert_if_present("BrightnessValue", fmt_srat(&r).to_string());
    }

    if let Some(r) = exif.subject_distance {
        insert_if_present("SubjectDistance", fmt_rat(&r).to_string());
    }
    if let Some(v) = exif.subject_distance_range {
        insert_if_present("SubjectDistanceRange", v.to_string());
    }

    if let Some(gps) = exif.gps {
        let fmt_gps_coord = |coords: &[rawler::formats::tiff::Rational; 3]| -> String {
            format!(
                "{} deg {} min {} sec",
                fmt_rat(&coords[0]),
                fmt_rat(&coords[1]),
                fmt_rat(&coords[2])
            )
        };

        if let Some(lat) = gps.gps_latitude {
            insert_if_present("GPSLatitude", fmt_gps_coord(&lat));
        }
        if let Some(lat_ref) = gps.gps_latitude_ref {
            insert_if_present("GPSLatitudeRef", lat_ref);
        }
        if let Some(lon) = gps.gps_longitude {
            insert_if_present("GPSLongitude", fmt_gps_coord(&lon));
        }
        if let Some(lon_ref) = gps.gps_longitude_ref {
            insert_if_present("GPSLongitudeRef", lon_ref);
        }
        if let Some(alt) = gps.gps_altitude {
            insert_if_present("GPSAltitude", fmt_rat(&alt).to_string());
        }
        if let Some(alt_ref) = gps.gps_altitude_ref {
            insert_if_present("GPSAltitudeRef", alt_ref.to_string());
        }
        if let Some(v) = gps.gps_img_direction {
            insert_if_present("GPSImgDirection", fmt_rat(&v).to_string());
        }
        if let Some(v) = gps.gps_img_direction_ref {
            insert_if_present("GPSImgDirectionRef", v);
        }
        if let Some(v) = gps.gps_speed {
            insert_if_present("GPSSpeed", fmt_rat(&v).to_string());
        }
        if let Some(v) = gps.gps_speed_ref {
            insert_if_present("GPSSpeedRef", v);
        }
        if let Some(v) = gps.gps_status {
            insert_if_present("GPSStatus", v);
        }
        if let Some(v) = gps.gps_measure_mode {
            insert_if_present("GPSMeasureMode", v);
        }
        if let Some(v) = gps.gps_dop {
            insert_if_present("GPSDOP", fmt_rat(&v).to_string());
        }
        if let Some(v) = gps.gps_map_datum {
            insert_if_present("GPSMapDatum", v);
        }
    }

    if let Some(lens) = &metadata.lens {
        insert_missing_lens_fields(&mut map, lens);
    }

    Some(map)
}

pub fn get_creation_date_from_path(path: &Path) -> DateTime<Utc> {
    if let Some(dt) = try_get_exif_creation_date(path) {
        return dt;
    }

    fs::metadata(path)
        .ok()
        .and_then(|m| m.created().ok())
        .map(DateTime::<Utc>::from)
        .unwrap_or_else(Utc::now)
}

pub fn try_get_exif_creation_date(path: &Path) -> Option<DateTime<Utc>> {
    if let Some(map) = read_rrexif_sidecar(path)
        && let Some(dt_str) = map.get("DateTimeOriginal").or(map.get("CreateDate"))
        && let Some(dt) = parse_creation_datetime(dt_str)
    {
        return Some(DateTime::from_naive_utc_and_offset(dt, Utc));
    }

    if let Ok(file) = std::fs::File::open(path) {
        let mut bufreader = BufReader::new(&file);
        let exifreader = exif::Reader::new();

        if let Ok(exif_obj) = exifreader.read_from_container(&mut bufreader) {
            for tag in [exif::Tag::DateTimeOriginal, exif::Tag::DateTime] {
                if let Some(field) = exif_obj.get_field(tag, exif::In::PRIMARY)
                    && let Some(dt) = parse_creation_field(field)
                {
                    return Some(dt);
                }
            }
        }
    }

    if is_raw_file(path) {
        let loader = rawler::RawLoader::new();
        if let Ok(raw_source) = rawler::rawsource::RawSource::new(path)
            && let Ok(decoder) = loader.get_decoder(&raw_source)
            && let Ok(metadata) = decoder.raw_metadata(&raw_source, &Default::default())
        {
            if let Some(dt) = parse_raw_creation_date(metadata.exif.date_time_original.as_deref()) {
                return Some(dt);
            }
            if let Some(dt) = parse_raw_creation_date(metadata.exif.create_date.as_deref()) {
                return Some(dt);
            }
        }
    }

    None
}

#[cfg(target_os = "android")]
pub fn get_creation_date_from_bytes(path_hint: &str, file_bytes: &[u8]) -> DateTime<Utc> {
    if let Some(exif_obj) = read_exif(file_bytes) {
        for tag in [exif::Tag::DateTimeOriginal, exif::Tag::DateTime] {
            if let Some(field) = exif_obj.get_field(tag, exif::In::PRIMARY)
                && let Some(dt) = parse_creation_field(field)
            {
                return dt;
            }
        }
    }

    if is_raw_file(path_hint)
        && let Some(metadata) = read_raw_metadata(file_bytes)
    {
        if let Some(dt) = parse_raw_creation_date(metadata.exif.date_time_original.as_deref()) {
            return dt;
        }
        if let Some(dt) = parse_raw_creation_date(metadata.exif.create_date.as_deref()) {
            return dt;
        }
    }

    Utc::now()
}

fn copy_full_exif_from_source(
    metadata: &mut Metadata,
    original_path: &Path,
    strip_gps: bool,
) -> bool {
    let Ok(source_metadata) = Metadata::new_from_path(original_path) else {
        return false;
    };

    let mut copied_any = false;
    for ifd in source_metadata.get_ifds() {
        if ifd.get_generic_ifd_nr() != 0 {
            continue;
        }
        if strip_gps && ifd.get_ifd_type() == ExifTagGroup::GPS {
            continue;
        }
        for tag in ifd.get_tags() {
            metadata.set_tag(tag.clone());
            copied_any = true;
        }
    }
    copied_any
}

fn apply_sidecar_field_overrides(metadata: &mut Metadata, map: &HashMap<String, String>) {
    let clean_s = |s: &String| s.replace('"', "").trim().to_string();
    let is_user_edit = |s: &str| !s.is_empty() && s != "...";

    match map.get("Artist").map(clean_s) {
        Some(val) => {
            if is_user_edit(&val) {
                metadata.set_tag(ExifTag::Artist(val));
            }
        }
        None => {
            metadata.remove_tag(ExifTag::Artist(String::new()));
        }
    }
    match map.get("Copyright").map(clean_s) {
        Some(val) => {
            if is_user_edit(&val) {
                metadata.set_tag(ExifTag::Copyright(val));
            }
        }
        None => {
            metadata.remove_tag(ExifTag::Copyright(String::new()));
        }
    }
    match map.get("ImageDescription").map(clean_s) {
        Some(val) => {
            if is_user_edit(&val) {
                metadata.set_tag(ExifTag::ImageDescription(val));
            }
        }
        None => {
            metadata.remove_tag(ExifTag::ImageDescription(String::new()));
        }
    }
    match map.get("UserComment").map(clean_s) {
        Some(val) => {
            if is_user_edit(&val) && !val.starts_with("0x") {
                metadata.set_tag(ExifTag::UserComment(val.into_bytes()));
            }
        }
        None => {
            metadata.remove_tag(ExifTag::UserComment(Vec::new()));
        }
    }
}

fn apply_exif_from_rawler(metadata: &mut Metadata, meta: &RawMetadata) {
    if !meta.make.is_empty() {
        metadata.set_tag(ExifTag::Make(meta.make.clone()));
    }
    if !meta.model.is_empty() {
        metadata.set_tag(ExifTag::Model(meta.model.clone()));
    }
    let exif = &meta.exif;
    if let Some(artist) = &exif.artist {
        metadata.set_tag(ExifTag::Artist(artist.clone()));
    }
    if let Some(copyright) = &exif.copyright {
        metadata.set_tag(ExifTag::Copyright(copyright.clone()));
    }
    if let Some(dt) = &exif.date_time_original {
        metadata.set_tag(ExifTag::DateTimeOriginal(dt.clone()));
    }
    if let Some(dt) = &exif.create_date {
        metadata.set_tag(ExifTag::CreateDate(dt.clone()));
    }
    if let Some(offset) = &exif.offset_time_original {
        metadata.set_tag(ExifTag::OffsetTimeOriginal(offset.clone()));
    }
    if let Some(offset) = &exif.offset_time_digitized {
        metadata.set_tag(ExifTag::OffsetTimeDigitized(offset.clone()));
    }
    if let Some(sub_sec) = &exif.sub_sec_time_original {
        metadata.set_tag(ExifTag::SubSecTimeOriginal(sub_sec.clone()));
    }
    if let Some(lens_make) = &exif.lens_make {
        metadata.set_tag(ExifTag::LensMake(lens_make.clone()));
    }
    if let Some(lens_model) = &exif.lens_model {
        metadata.set_tag(ExifTag::LensModel(lens_model.clone()));
    }
    if let Some(lens_spec) = exif.lens_spec {
        metadata.set_tag(ExifTag::LensInfo(lens_spec.map(rawler_to_ur64).to_vec()));
    }
    if let Some(f) = exif.fnumber {
        metadata.set_tag(ExifTag::FNumber(vec![rawler_to_ur64(f)]));
    }
    if let Some(a) = exif.aperture_value {
        metadata.set_tag(ExifTag::ApertureValue(vec![rawler_to_ur64(a)]));
    }
    if let Some(t) = exif.exposure_time {
        metadata.set_tag(ExifTag::ExposureTime(vec![rawler_to_ur64(t)]));
    }
    if let Some(s) = exif.shutter_speed_value {
        metadata.set_tag(ExifTag::ShutterSpeedValue(vec![rawler_to_ir64(s)]));
    }
    if let Some(fl) = exif.focal_length {
        metadata.set_tag(ExifTag::FocalLength(vec![rawler_to_ur64(fl)]));
    }
    if let Some(iso) = exif.iso_speed {
        metadata.set_tag(ExifTag::ISO(vec![iso as u16]));
    } else if let Some(iso) = exif.iso_speed_ratings {
        metadata.set_tag(ExifTag::ISO(vec![iso]));
    }
    if let Some(ev) = exif.exposure_bias {
        metadata.set_tag(ExifTag::ExposureCompensation(vec![rawler_to_ir64(ev)]));
    }
    if let Some(flash) = exif.flash {
        metadata.set_tag(ExifTag::Flash(vec![flash]));
    }
    if let Some(metering) = exif.metering_mode {
        metadata.set_tag(ExifTag::MeteringMode(vec![metering]));
    }
    if let Some(wb) = exif.white_balance {
        metadata.set_tag(ExifTag::WhiteBalance(vec![wb]));
    }
    if let Some(prog) = exif.exposure_program {
        metadata.set_tag(ExifTag::ExposureProgram(vec![prog]));
    }
}

fn apply_gps_from_kamadak(metadata: &mut Metadata, original_path: &Path) {
    let Ok(file) = std::fs::File::open(original_path) else {
        return;
    };
    let mut bufreader = std::io::BufReader::new(&file);
    let exifreader = exif::Reader::new();
    let Ok(exif_obj) = exifreader.read_from_container(&mut bufreader) else {
        return;
    };

    let get_string_val = |field: &exif::Field| -> String {
        match &field.value {
            exif::Value::Ascii(vec) => vec
                .iter()
                .map(|v| {
                    String::from_utf8_lossy(v)
                        .trim_matches(char::from(0))
                        .to_string()
                })
                .collect::<Vec<String>>()
                .join(" "),
            _ => field
                .display_value()
                .to_string()
                .replace("\"", "")
                .trim()
                .to_string(),
        }
    };

    if let Some(f) = exif_obj.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY)
        && let exif::Value::Rational(v) = &f.value
        && v.len() >= 3
    {
        metadata.set_tag(ExifTag::GPSLatitude(vec![
            to_ur64(&v[0]),
            to_ur64(&v[1]),
            to_ur64(&v[2]),
        ]));
    }
    if let Some(f) = exif_obj.get_field(exif::Tag::GPSLatitudeRef, exif::In::PRIMARY) {
        metadata.set_tag(ExifTag::GPSLatitudeRef(get_string_val(f)));
    }
    if let Some(f) = exif_obj.get_field(exif::Tag::GPSLongitude, exif::In::PRIMARY)
        && let exif::Value::Rational(v) = &f.value
        && v.len() >= 3
    {
        metadata.set_tag(ExifTag::GPSLongitude(vec![
            to_ur64(&v[0]),
            to_ur64(&v[1]),
            to_ur64(&v[2]),
        ]));
    }
    if let Some(f) = exif_obj.get_field(exif::Tag::GPSLongitudeRef, exif::In::PRIMARY) {
        metadata.set_tag(ExifTag::GPSLongitudeRef(get_string_val(f)));
    }
    if let Some(f) = exif_obj.get_field(exif::Tag::GPSAltitude, exif::In::PRIMARY)
        && let exif::Value::Rational(v) = &f.value
        && !v.is_empty()
    {
        metadata.set_tag(ExifTag::GPSAltitude(vec![to_ur64(&v[0])]));
    }
    if let Some(f) = exif_obj.get_field(exif::Tag::GPSAltitudeRef, exif::In::PRIMARY)
        && let Some(val) = f.value.get_uint(0)
    {
        metadata.set_tag(ExifTag::GPSAltitudeRef(vec![val as u8]));
    }
}

fn apply_gps_from_rawler(metadata: &mut Metadata, gps: ExifGPS) {
    if let Some(lat) = gps.gps_latitude {
        metadata.set_tag(ExifTag::GPSLatitude(lat.map(rawler_to_ur64).to_vec()));
    }
    if let Some(lat_ref) = gps.gps_latitude_ref {
        metadata.set_tag(ExifTag::GPSLatitudeRef(lat_ref));
    }
    if let Some(lon) = gps.gps_longitude {
        metadata.set_tag(ExifTag::GPSLongitude(lon.map(rawler_to_ur64).to_vec()));
    }
    if let Some(lon_ref) = gps.gps_longitude_ref {
        metadata.set_tag(ExifTag::GPSLongitudeRef(lon_ref));
    }
    if let Some(alt) = gps.gps_altitude {
        metadata.set_tag(ExifTag::GPSAltitude(vec![rawler_to_ur64(alt)]));
    }
    if let Some(alt_ref) = gps.gps_altitude_ref {
        metadata.set_tag(ExifTag::GPSAltitudeRef(vec![alt_ref]));
    }
}

pub fn write_image_with_metadata(
    image_bytes: &mut Vec<u8>,
    original_path_str: &str,
    output_format: &str,
    keep_metadata: bool,
    strip_gps: bool,
    xmp: &ExportXmp,
    dpi: Option<u16>,
) -> Result<(), String> {
    // FIXME: temporary solution until I find a way to write metadata to TIFF
    if !keep_metadata || output_format.to_lowercase() == "tiff" {
        return Ok(());
    }

    let file_type = match output_format.to_lowercase().as_str() {
        "jpg" | "jpeg" => FileExtension::JPEG,
        "png" => FileExtension::PNG {
            as_zTXt_chunk: true,
        },
        "tiff" => FileExtension::TIFF,
        "webp" => FileExtension::WEBP,
        _ => return Ok(()),
    };

    if file_type == FileExtension::WEBP {
        convert_webp_to_extended(image_bytes);
    }
    write_exif_metadata(image_bytes, original_path_str, file_type, strip_gps, dpi)?;
    embed_xmp(image_bytes, original_path_str, file_type, strip_gps, xmp);

    Ok(())
}

fn write_exif_metadata(
    image_bytes: &mut Vec<u8>,
    original_path_str: &str,
    file_type: FileExtension,
    strip_gps: bool,
    dpi: Option<u16>,
) -> Result<(), String> {
    let original_path = Path::new(original_path_str);
    if !original_path.exists() {
        return Ok(());
    }

    // Skip TIFF sources to avoid potential tag corruption issues
    let original_ext = original_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    if original_ext == "tiff" || original_ext == "tif" {
        return Ok(());
    }

    let mut metadata = Metadata::new();

    let full_exif_copied = !is_raw_file(original_path_str)
        && copy_full_exif_from_source(&mut metadata, original_path, strip_gps);
    let mut source_read_success = full_exif_copied;

    let raw_metadata = if is_raw_file(original_path_str) {
        read_raw_metadata_from_path(original_path_str)
    } else {
        None
    };
    if let Some(meta) = &raw_metadata {
        apply_exif_from_rawler(&mut metadata, meta);
    }

    if !source_read_success && let Some(map) = read_rrexif_sidecar(original_path) {
        source_read_success = true;

        let clean_s = |s: &String| s.replace('"', "").trim().to_string();

        let parse_ur64 = |s: &str| -> Option<uR64> {
            let cleaned_string = s
                .replace("f/", "")
                .replace(" s", "")
                .replace(" mm", "")
                .replace("\"", "");

            let val = cleaned_string.trim();

            if val.contains('/') {
                let parts: Vec<&str> = val.split('/').collect();
                if parts.len() == 2
                    && let (Ok(n), Ok(d)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>())
                {
                    return Some(uR64 {
                        nominator: n,
                        denominator: d,
                    });
                }
            } else if let Ok(f) = val.parse::<f32>() {
                return Some(uR64 {
                    nominator: (f * 1000.0) as u32,
                    denominator: 1000,
                });
            }
            None
        };
        if let Some(val) = map.get("Make") {
            metadata.set_tag(ExifTag::Make(clean_s(val)));
        }
        if let Some(val) = map.get("Model") {
            metadata.set_tag(ExifTag::Model(clean_s(val)));
        }
        if let Some(val) = map.get("LensMake") {
            metadata.set_tag(ExifTag::LensMake(clean_s(val)));
        }
        if let Some(val) = map.get("LensModel") {
            metadata.set_tag(ExifTag::LensModel(clean_s(val)));
        }
        if let Some(val) = map.get("Artist") {
            metadata.set_tag(ExifTag::Artist(clean_s(val)));
        }
        if let Some(val) = map.get("Copyright") {
            metadata.set_tag(ExifTag::Copyright(clean_s(val)));
        }
        if let Some(val) = map.get("UserComment") {
            metadata.set_tag(ExifTag::UserComment(clean_s(val).into_bytes()));
        }
        if let Some(val) = map.get("ImageDescription") {
            metadata.set_tag(ExifTag::ImageDescription(clean_s(val)));
        }
        if let Some(val) = map.get("DateTimeOriginal") {
            metadata.set_tag(ExifTag::DateTimeOriginal(format_creation_datetime(
                val,
                EXIF_DATETIME_FORMAT,
            )));
        }
        if let Some(val) = map.get("CreateDate") {
            metadata.set_tag(ExifTag::CreateDate(format_creation_datetime(
                val,
                EXIF_DATETIME_FORMAT,
            )));
        }
        if let Some(val) = map.get("FNumber")
            && let Some(ur) = parse_ur64(val)
        {
            metadata.set_tag(ExifTag::FNumber(vec![ur]));
        }
        if let Some(val) = map.get("ExposureTime")
            && let Some(ur) = parse_ur64(val)
        {
            metadata.set_tag(ExifTag::ExposureTime(vec![ur]));
        }
        if let Some(val) = map.get("FocalLength")
            && let Some(ur) = parse_ur64(val)
        {
            metadata.set_tag(ExifTag::FocalLength(vec![ur]));
        }
        if let Some(val) = map.get("FocalLengthIn35mmFilm") {
            let cleaned = val.replace(" mm", "").replace("\"", "");
            let trimmed = cleaned.trim();
            if let Ok(f_val) = trimmed.parse::<f32>() {
                metadata.set_tag(ExifTag::FocalLengthIn35mmFormat(vec![f_val.round() as u16]));
            }
        }
        if let Some(val) = map.get("ISOSpeed").or(map.get("PhotographicSensitivity"))
            && let Ok(iso) = val.replace('"', "").trim().parse::<u16>()
        {
            metadata.set_tag(ExifTag::ISO(vec![iso]));
        }
    }

    if !source_read_success && let Ok(file) = std::fs::File::open(original_path) {
        let mut bufreader = std::io::BufReader::new(&file);
        let exifreader = exif::Reader::new();

        if let Ok(exif_obj) = exifreader.read_from_container(&mut bufreader) {
            let get_string_val = |field: &exif::Field| -> String {
                match &field.value {
                    exif::Value::Ascii(vec) => vec
                        .iter()
                        .map(|v| {
                            String::from_utf8_lossy(v)
                                .trim_matches(char::from(0))
                                .to_string()
                        })
                        .collect::<Vec<String>>()
                        .join(" "),
                    _ => field
                        .display_value()
                        .to_string()
                        .replace("\"", "")
                        .trim()
                        .to_string(),
                }
            };

            if let Some(f) = exif_obj.get_field(exif::Tag::Make, exif::In::PRIMARY) {
                metadata.set_tag(ExifTag::Make(get_string_val(f)));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::Model, exif::In::PRIMARY) {
                metadata.set_tag(ExifTag::Model(get_string_val(f)));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::LensMake, exif::In::PRIMARY) {
                metadata.set_tag(ExifTag::LensMake(get_string_val(f)));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::LensModel, exif::In::PRIMARY) {
                metadata.set_tag(ExifTag::LensModel(get_string_val(f)));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::Artist, exif::In::PRIMARY) {
                metadata.set_tag(ExifTag::Artist(get_string_val(f)));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::Copyright, exif::In::PRIMARY) {
                metadata.set_tag(ExifTag::Copyright(get_string_val(f)));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY) {
                metadata.set_tag(ExifTag::DateTimeOriginal(get_string_val(f)));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::DateTime, exif::In::PRIMARY) {
                metadata.set_tag(ExifTag::CreateDate(get_string_val(f)));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::FNumber, exif::In::PRIMARY)
                && let exif::Value::Rational(v) = &f.value
                && !v.is_empty()
            {
                metadata.set_tag(ExifTag::FNumber(vec![to_ur64(&v[0])]));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::ExposureTime, exif::In::PRIMARY)
                && let exif::Value::Rational(v) = &f.value
                && !v.is_empty()
            {
                metadata.set_tag(ExifTag::ExposureTime(vec![to_ur64(&v[0])]));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::FocalLength, exif::In::PRIMARY)
                && let exif::Value::Rational(v) = &f.value
                && !v.is_empty()
            {
                metadata.set_tag(ExifTag::FocalLength(vec![to_ur64(&v[0])]));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::ExposureBiasValue, exif::In::PRIMARY) {
                match &f.value {
                    exif::Value::SRational(v) if !v.is_empty() => {
                        metadata.set_tag(ExifTag::ExposureCompensation(vec![to_ir64(&v[0])]));
                    }
                    exif::Value::Rational(v) if !v.is_empty() => {
                        metadata.set_tag(ExifTag::ExposureCompensation(vec![iR64 {
                            nominator: v[0].num as i32,
                            denominator: v[0].denom as i32,
                        }]));
                    }
                    _ => {}
                }
            }
            if let Some(f) =
                exif_obj.get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY)
            {
                if let Some(val) = f.value.get_uint(0) {
                    metadata.set_tag(ExifTag::ISO(vec![val as u16]));
                }
            } else if let Some(f) = exif_obj.get_field(exif::Tag::ISOSpeed, exif::In::PRIMARY)
                && let Some(val) = f.value.get_uint(0)
            {
                metadata.set_tag(ExifTag::ISO(vec![val as u16]));
            }
            if let Some(f) = exif_obj.get_field(exif::Tag::FocalLengthIn35mmFilm, exif::In::PRIMARY)
                && let Some(val) = f.value.get_uint(0)
            {
                metadata.set_tag(ExifTag::FocalLengthIn35mmFormat(vec![val as u16]));
            }
            if !strip_gps {
                if let Some(f) = exif_obj.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY)
                    && let exif::Value::Rational(v) = &f.value
                    && v.len() >= 3
                {
                    metadata.set_tag(ExifTag::GPSLatitude(vec![
                        to_ur64(&v[0]),
                        to_ur64(&v[1]),
                        to_ur64(&v[2]),
                    ]));
                }
                if let Some(f) = exif_obj.get_field(exif::Tag::GPSLatitudeRef, exif::In::PRIMARY) {
                    metadata.set_tag(ExifTag::GPSLatitudeRef(get_string_val(f)));
                }
                if let Some(f) = exif_obj.get_field(exif::Tag::GPSLongitude, exif::In::PRIMARY)
                    && let exif::Value::Rational(v) = &f.value
                    && v.len() >= 3
                {
                    metadata.set_tag(ExifTag::GPSLongitude(vec![
                        to_ur64(&v[0]),
                        to_ur64(&v[1]),
                        to_ur64(&v[2]),
                    ]));
                }
                if let Some(f) = exif_obj.get_field(exif::Tag::GPSLongitudeRef, exif::In::PRIMARY) {
                    metadata.set_tag(ExifTag::GPSLongitudeRef(get_string_val(f)));
                }
                if let Some(f) = exif_obj.get_field(exif::Tag::GPSAltitude, exif::In::PRIMARY)
                    && let exif::Value::Rational(v) = &f.value
                    && !v.is_empty()
                {
                    metadata.set_tag(ExifTag::GPSAltitude(vec![to_ur64(&v[0])]));
                }
                if let Some(f) = exif_obj.get_field(exif::Tag::GPSAltitudeRef, exif::In::PRIMARY) {
                    let alt_ref = f.value.get_uint(0).unwrap_or(0) as u8;
                    metadata.set_tag(ExifTag::GPSAltitudeRef(vec![alt_ref]));
                }
            }
        }
    }

    if !strip_gps {
        if is_raw_file(original_path_str) {
            if let Some(gps) = raw_metadata.and_then(|meta| meta.exif.gps) {
                apply_gps_from_rawler(&mut metadata, gps);
            }
        } else {
            apply_gps_from_kamadak(&mut metadata, original_path);
        }
    }

    if full_exif_copied && let Some(map) = read_rrexif_sidecar(original_path) {
        apply_sidecar_field_overrides(&mut metadata, &map);
    }

    metadata.set_tag(ExifTag::Software("RapidRAW".to_string()));
    metadata.set_tag(ExifTag::Orientation(vec![1u16]));
    metadata.set_tag(ExifTag::ColorSpace(vec![1u16]));
    metadata.set_tag(ExifTag::ExifVersion(b"0232".to_vec()));
    metadata.set_tag(ExifTag::FlashpixVersion(b"0100".to_vec()));
    if matches!(file_type, FileExtension::JPEG) {
        metadata.set_tag(ExifTag::ComponentsConfiguration(vec![1, 2, 3, 0]));
    }

    if let Some(dpi) = dpi {
        let resolution = vec![uR64 {
            nominator: dpi.into(),
            denominator: 1,
        }];
        metadata.set_tag(ExifTag::XResolution(resolution.clone()));
        metadata.set_tag(ExifTag::YResolution(resolution));
        metadata.set_tag(ExifTag::ResolutionUnit(vec![2u16]));
    }

    if let Ok(reader) =
        image::ImageReader::new(Cursor::new(image_bytes.as_slice())).with_guessed_format()
        && let Ok((width, height)) = reader.into_dimensions()
    {
        metadata.set_tag(ExifTag::ExifImageWidth(vec![width]));
        metadata.set_tag(ExifTag::ExifImageHeight(vec![height]));
    }

    if let Err(e) = metadata.write_to_vec(image_bytes, file_type) {
        log::warn!("Failed to write metadata: {}", e);
    }

    Ok(())
}

#[derive(Default)]
pub struct ExportXmp {
    keywords: Vec<String>,
    rating: Option<String>,
    label: Option<String>,
}

impl ExportXmp {
    pub fn from_metadata(metadata: &ImageMetadata) -> Self {
        let tags = metadata.tags.as_deref().unwrap_or_default();
        let mut keywords = Vec::new();
        for tag in tags.iter().filter(|t| !t.starts_with(COLOR_TAG_PREFIX)) {
            push_keyword(
                &mut keywords,
                tag.strip_prefix(USER_TAG_PREFIX).unwrap_or(tag),
            );
        }
        Self {
            keywords,
            rating: (metadata.rating > 0 || metadata.flag == Some(ImageFlag::Reject))
                .then(|| file_management::xmp_rating(metadata)),
            label: file_management::xmp_label(tags),
        }
    }
}

fn push_keyword(keywords: &mut Vec<String>, keyword: &str) {
    let keyword = keyword.trim();
    if !keyword.is_empty() && !keywords.iter().any(|k| k == keyword) {
        keywords.push(keyword.to_string());
    }
}

fn embed_xmp(
    image_bytes: &mut Vec<u8>,
    original_path_str: &str,
    file_type: FileExtension,
    strip_gps: bool,
    xmp: &ExportXmp,
) {
    let source_xmp = if strip_gps || is_raw_file(original_path_str) {
        None
    } else {
        read_source_xmp(Path::new(original_path_str))
    };
    let Some(packet) = build_export_xmp(xmp, source_xmp.as_deref()) else {
        return;
    };

    match file_type {
        FileExtension::JPEG => embed_jpeg_xmp(image_bytes, &packet),
        FileExtension::PNG { .. } => embed_png_xmp(image_bytes, &packet),
        FileExtension::WEBP => embed_webp_xmp(image_bytes, &packet),
        _ => {}
    }
}

fn read_source_xmp(path: &Path) -> Option<String> {
    let mut decoder = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .into_decoder()
        .ok()?;
    String::from_utf8(decoder.xmp_metadata().ok()??).ok()
}

fn build_export_xmp(xmp: &ExportXmp, source_xmp: Option<&str>) -> Option<String> {
    let mut keywords = xmp.keywords.clone();
    for tag in source_xmp
        .map(file_management::extract_xmp_tags)
        .unwrap_or_default()
    {
        push_keyword(
            &mut keywords,
            &unescape(&tag).unwrap_or(Cow::Borrowed(&tag)),
        );
    }

    let description = export_xmp_description(xmp, &keywords);
    if let Some(merged) = source_xmp.and_then(|source| merge_source_xmp(source, &description, xmp))
    {
        return Some(merged);
    }
    if description.is_empty() {
        return None;
    }

    Some(format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"RapidRAW\">\n \
<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
{} </rdf:RDF>\n\
</x:xmpmeta>\n\
<?xpacket end=\"w\"?>",
        description
    ))
}

fn export_xmp_description(xmp: &ExportXmp, keywords: &[String]) -> String {
    let mut properties = String::new();
    if let Some(rating) = &xmp.rating {
        properties.push_str(&format!("   <xmp:Rating>{}</xmp:Rating>\n", rating));
    }
    if let Some(label) = &xmp.label {
        properties.push_str(&format!(
            "   <xmp:Label>{}</xmp:Label>\n",
            escape(label.as_str())
        ));
    }
    if !keywords.is_empty() {
        let escaped: Vec<Cow<str>> = keywords.iter().map(|k| escape(k.as_str())).collect();
        properties.push_str(&format!(
            "   {}\n",
            file_management::xmp_subject_bag(&escaped)
        ));
    }
    if properties.is_empty() {
        return properties;
    }

    format!(
        "  <rdf:Description rdf:about=\"\"\n    xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"\n    xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n{}  </rdf:Description>\n",
        properties
    )
}

fn is_replaced_xmp_property(name: &[u8], xmp: &ExportXmp) -> bool {
    name.starts_with(b"crs:")
        || matches!(
            name,
            b"dc:subject"
                | b"tiff:Orientation"
                | b"tiff:ImageWidth"
                | b"tiff:ImageLength"
                | b"exif:PixelXDimension"
                | b"exif:PixelYDimension"
                | b"xmpNote:HasExtendedXMP"
        )
        || (xmp.rating.is_some() && name == b"xmp:Rating")
        || (xmp.label.is_some() && name == b"xmp:Label")
}

fn without_replaced_attributes(element: &BytesStart, xmp: &ExportXmp) -> BytesStart<'static> {
    let mut kept = BytesStart::new(String::from_utf8_lossy(element.name().as_ref()).into_owned());
    for attribute in element.attributes().with_checks(false).flatten() {
        if !is_replaced_xmp_property(attribute.key.as_ref(), xmp) {
            kept.push_attribute(attribute);
        }
    }
    kept
}

fn merge_source_xmp(source: &str, description: &str, xmp: &ExportXmp) -> Option<String> {
    let mut reader = Reader::from_str(source);
    let mut writer = Writer::new(Vec::new());
    let mut inserted = false;

    loop {
        match reader.read_event().ok()? {
            Event::Eof => break,
            Event::Start(e) if is_replaced_xmp_property(e.name().as_ref(), xmp) => {
                reader.read_to_end(e.name()).ok()?;
            }
            Event::Empty(e) if is_replaced_xmp_property(e.name().as_ref(), xmp) => {}
            Event::Start(e) => writer
                .write_event(Event::Start(without_replaced_attributes(&e, xmp)))
                .ok()?,
            Event::Empty(e) => writer
                .write_event(Event::Empty(without_replaced_attributes(&e, xmp)))
                .ok()?,
            Event::End(e) if e.name().as_ref() == b"rdf:RDF" => {
                writer
                    .write_event(Event::Text(BytesText::from_escaped(description)))
                    .ok()?;
                writer.write_event(Event::End(e)).ok()?;
                inserted = true;
            }
            event => writer.write_event(event).ok()?,
        }
    }

    if !inserted {
        return None;
    }
    String::from_utf8(writer.into_inner()).ok()
}

fn embed_jpeg_xmp(image_bytes: &mut Vec<u8>, packet: &str) {
    const XMP_NAMESPACE: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

    if !image_bytes.starts_with(&[0xFF, 0xD8]) {
        return;
    }
    let Ok(segment_len) = u16::try_from(2 + XMP_NAMESPACE.len() + packet.len()) else {
        log::warn!("Skipping XMP metadata: packet exceeds JPEG segment size");
        return;
    };

    let mut insert_at = 2;
    while insert_at + 4 <= image_bytes.len()
        && image_bytes[insert_at] == 0xFF
        && (0xE0..=0xEF).contains(&image_bytes[insert_at + 1])
    {
        let len = u16::from_be_bytes([image_bytes[insert_at + 2], image_bytes[insert_at + 3]]);
        insert_at += 2 + len as usize;
    }
    if insert_at > image_bytes.len() {
        return;
    }

    let mut segment = Vec::with_capacity(2 + segment_len as usize);
    segment.extend_from_slice(&[0xFF, 0xE1]);
    segment.extend_from_slice(&segment_len.to_be_bytes());
    segment.extend_from_slice(XMP_NAMESPACE);
    segment.extend_from_slice(packet.as_bytes());
    image_bytes.splice(insert_at..insert_at, segment);
}

fn embed_png_xmp(image_bytes: &mut Vec<u8>, packet: &str) {
    let mut data = b"XML:com.adobe.xmp\0\0\0\0\0".to_vec();
    data.extend_from_slice(packet.as_bytes());
    insert_png_chunk(image_bytes, b"iTXt", &data);
}

pub fn insert_png_chunk(image_bytes: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    const IHDR_END: usize = 33;

    if image_bytes.len() < IHDR_END || &image_bytes[12..16] != b"IHDR" {
        return;
    }

    let mut crc = crc32fast::Hasher::new();
    crc.update(chunk_type);
    crc.update(data);

    let mut chunk = Vec::with_capacity(data.len() + 12);
    chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
    chunk.extend_from_slice(chunk_type);
    chunk.extend_from_slice(data);
    chunk.extend_from_slice(&crc.finalize().to_be_bytes());
    image_bytes.splice(IHDR_END..IHDR_END, chunk);
}

fn embed_webp_xmp(image_bytes: &mut Vec<u8>, packet: &str) {
    const VP8X_FLAGS: usize = 20;
    const XMP_FLAG: u8 = 0x04;

    if image_bytes.len() <= VP8X_FLAGS
        || &image_bytes[0..4] != b"RIFF"
        || &image_bytes[12..16] != b"VP8X"
    {
        return;
    }

    image_bytes[VP8X_FLAGS] |= XMP_FLAG;
    image_bytes.extend_from_slice(b"XMP ");
    image_bytes.extend_from_slice(&(packet.len() as u32).to_le_bytes());
    image_bytes.extend_from_slice(packet.as_bytes());
    if !packet.len().is_multiple_of(2) {
        image_bytes.push(0);
    }
    update_riff_size(image_bytes);
}

fn convert_webp_to_extended(image_bytes: &mut Vec<u8>) {
    const VP8_FRAME: usize = 20;

    if image_bytes.len() < VP8_FRAME + 10
        || &image_bytes[0..4] != b"RIFF"
        || &image_bytes[12..16] != b"VP8 "
        || image_bytes[VP8_FRAME + 3..VP8_FRAME + 6] != [0x9D, 0x01, 0x2A]
    {
        return;
    }

    let dimension = |offset: usize| {
        u32::from(u16::from_le_bytes([image_bytes[offset], image_bytes[offset + 1]]) & 0x3FFF)
            .saturating_sub(1)
    };
    let width = dimension(VP8_FRAME + 6);
    let height = dimension(VP8_FRAME + 8);

    let mut vp8x = Vec::with_capacity(18);
    vp8x.extend_from_slice(b"VP8X");
    vp8x.extend_from_slice(&10u32.to_le_bytes());
    vp8x.extend_from_slice(&[0; 4]);
    vp8x.extend_from_slice(&width.to_le_bytes()[..3]);
    vp8x.extend_from_slice(&height.to_le_bytes()[..3]);
    image_bytes.splice(12..12, vp8x);
    update_riff_size(image_bytes);
}

fn update_riff_size(image_bytes: &mut [u8]) {
    let riff_size = (image_bytes.len() - 8) as u32;
    image_bytes[4..8].copy_from_slice(&riff_size.to_le_bytes());
}

pub fn get_primary_sidecar_path(image_path: &Path) -> PathBuf {
    let mut filename = image_path.file_name().unwrap_or_default().to_os_string();
    filename.push(".rrdata");
    let parent = image_path.parent().unwrap_or_else(|| Path::new(""));
    let subfolder_sidecar = parent.join(".rapidraw").join(&filename);
    let legacy_sidecar = image_path.with_file_name(&filename);

    if subfolder_sidecar.exists() {
        subfolder_sidecar
    } else if legacy_sidecar.exists() {
        legacy_sidecar
    } else {
        subfolder_sidecar
    }
}

pub fn get_rrexif_path(image_path: &Path) -> PathBuf {
    let mut filename = image_path.file_name().unwrap_or_default().to_os_string();
    filename.push(".rrexif");
    let parent = image_path.parent().unwrap_or_else(|| Path::new(""));
    let subfolder_rrexif = parent.join(".rapidraw").join(&filename);
    let legacy_rrexif = image_path.with_file_name(&filename);

    if subfolder_rrexif.exists() {
        subfolder_rrexif
    } else if legacy_rrexif.exists() {
        legacy_rrexif
    } else {
        subfolder_rrexif
    }
}

pub fn load_primary_metadata(image_path: &Path) -> ImageMetadata {
    let primary = get_primary_sidecar_path(image_path);
    load_sidecar(&primary)
}

fn save_primary_metadata(image_path: &Path, metadata: &ImageMetadata) -> std::io::Result<()> {
    let primary = get_primary_sidecar_path(image_path);
    if let Some(parent) = primary.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(metadata).map_err(std::io::Error::other)?;
    fs::write(&primary, json)
}

pub fn read_rrexif_sidecar(image_path: &Path) -> Option<HashMap<String, String>> {
    let metadata = load_primary_metadata(image_path);
    if let Some(exif) = metadata.exif {
        return Some(exif);
    }

    let legacy = get_rrexif_path(image_path);
    if legacy.exists()
        && let Ok(content) = fs::read_to_string(&legacy)
        && let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&content)
    {
        let mut migrated = load_primary_metadata(image_path);
        migrated.exif = Some(map.clone());
        if save_primary_metadata(image_path, &migrated).is_ok() {
            let _ = fs::remove_file(&legacy);
        }
        return Some(map);
    }

    None
}

pub fn read_exif_data_from_bytes(path: &str, file_bytes: &[u8]) -> HashMap<String, String> {
    if is_raw_file(path)
        && let Some(map) = extract_metadata(file_bytes)
    {
        return map;
    }

    let mut exif_data = HashMap::new();
    if let Some(exif) = read_exif(file_bytes) {
        for field in exif.fields() {
            let raw_val = match &field.value {
                exif::Value::Ascii(_) => match clean_ascii_value(&field.value) {
                    Some(v) => v,
                    None => continue,
                },
                exif::Value::Rational(v) if field.tag == exif::Tag::LensSpecification => {
                    match format_lens_specification(v) {
                        Some(s) => s,
                        None => continue,
                    }
                }
                _ => field.display_value().with_unit(&exif).to_string(),
            };
            exif_data.insert(field.tag.to_string(), truncate_large_exif(&raw_val));
        }
    }
    exif_data
}

static CENTRAL_EXIF_CACHE_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn init_central_exif_cache_dir(dir: PathBuf) {
    let _ = fs::create_dir_all(&dir);
    let _ = CENTRAL_EXIF_CACHE_DIR.set(dir);
}

pub fn get_central_exif_cache_dir() -> PathBuf {
    if let Some(dir) = CENTRAL_EXIF_CACHE_DIR.get() {
        return dir.clone();
    }
    let fallback = std::env::temp_dir().join("rapidraw").join("exif");
    let _ = fs::create_dir_all(&fallback);
    fallback
}

pub fn compute_exif_cache_key(source_path: &Path) -> Option<String> {
    let canonical = source_path.canonicalize().unwrap_or_else(|_| source_path.to_path_buf());
    let path_str = canonical.to_string_lossy();
    let mod_time = fs::metadata(source_path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();

    let mut hasher = blake3::Hasher::new();
    hasher.update(path_str.as_bytes());
    hasher.update(&mod_time.to_le_bytes());
    Some(hasher.finalize().to_hex().to_string())
}

pub fn read_central_cached_exif(source_path: &Path) -> Option<HashMap<String, String>> {
    let key = compute_exif_cache_key(source_path)?;
    let cache_dir = get_central_exif_cache_dir();
    let cache_file = cache_dir.join(format!("{}.json", key));
    if cache_file.exists() {
        if let Ok(content) = fs::read_to_string(&cache_file) {
            if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&content) {
                return Some(map);
            }
        }
    }
    None
}

pub fn write_central_cached_exif(source_path: &Path, exif_map: &HashMap<String, String>) {
    if exif_map.is_empty() {
        return;
    }
    if let Some(key) = compute_exif_cache_key(source_path) {
        let cache_dir = get_central_exif_cache_dir();
        let _ = fs::create_dir_all(&cache_dir);
        let cache_file = cache_dir.join(format!("{}.json", key));
        if let Ok(json) = serde_json::to_string(exif_map) {
            let _ = fs::write(&cache_file, json);
        }
    }
}

pub fn read_exif_data(path: &str, file_bytes: &[u8]) -> HashMap<String, String> {
    let source_path = Path::new(path);
    if let Some(sidecar_exif) = read_rrexif_sidecar(source_path) {
        return sidecar_exif;
    }

    if let Some(cached_exif) = read_central_cached_exif(source_path) {
        return cached_exif;
    }

    let exif_map = read_exif_data_from_bytes(path, file_bytes);
    if !exif_map.is_empty() {
        write_central_cached_exif(source_path, &exif_map);
        let mut metadata = load_primary_metadata(source_path);
        metadata.exif = Some(exif_map.clone());
        let _ = save_primary_metadata(source_path, &metadata);
    }
    exif_map
}

pub fn persist_exif_if_missing(source_path: &Path, source_path_str: &str, file_bytes: &[u8]) {
    {
        let metadata = load_primary_metadata(source_path);
        if metadata.exif.is_some() {
            return;
        }
    }

    let legacy = get_rrexif_path(source_path);
    if legacy.exists()
        && let Ok(content) = fs::read_to_string(&legacy)
        && let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&content)
    {
        let mut metadata = load_primary_metadata(source_path);
        metadata.exif = Some(map);
        if save_primary_metadata(source_path, &metadata).is_ok() {
            let _ = fs::remove_file(&legacy);
        }
        return;
    }

    let exif_map = read_exif_data_from_bytes(source_path_str, file_bytes);
    if exif_map.is_empty() {
        return;
    }

    let mut metadata = load_primary_metadata(source_path);

    if metadata.exif.is_none() {
        metadata.exif = Some(exif_map);
        let _ = save_primary_metadata(source_path, &metadata);
    }
}

pub fn write_rrexif_sidecar(source_path_str: &str, target_image_path: &Path) -> Result<(), String> {
    let source_path = Path::new(source_path_str);

    let exif_data = if let Some(existing) = read_rrexif_sidecar(source_path) {
        existing
    } else if let Ok(bytes) = fs::read(source_path) {
        read_exif_data_from_bytes(source_path_str, &bytes)
    } else {
        return Ok(());
    };

    if exif_data.is_empty() {
        return Ok(());
    }

    let mut metadata = load_primary_metadata(target_image_path);
    metadata.exif = Some(exif_data);
    save_primary_metadata(target_image_path, &metadata)
        .map_err(|e| format!("Failed to write sidecar: {}", e))
}

/// Generates standard Google Photosphere GPano XMP metadata payload for 360° VR and photosphere viewers.
pub fn generate_gpano_xmp(full_w: u32, full_h: u32, is_spherical: bool) -> String {
    let proj_type = if is_spherical { "equirectangular" } else { "cylindrical" };
    format!(
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="Adobe XMP Core 5.1.0-jc003">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about=""
        xmlns:GPano="http://ns.google.com/photos/1.0/panorama/">
      <GPano:UsePanoramaViewer>True</GPano:UsePanoramaViewer>
      <GPano:ProjectionType>{}</GPano:ProjectionType>
      <GPano:FullPanoWidthPixels>{}</GPano:FullPanoWidthPixels>
      <GPano:FullPanoHeightPixels>{}</GPano:FullPanoHeightPixels>
      <GPano:CroppedAreaImageWidthPixels>{}</GPano:CroppedAreaImageWidthPixels>
      <GPano:CroppedAreaImageHeightPixels>{}</GPano:CroppedAreaImageHeightPixels>
      <GPano:CroppedAreaLeftPixels>0</GPano:CroppedAreaLeftPixels>
      <GPano:CroppedAreaTopPixels>0</GPano:CroppedAreaTopPixels>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>"#,
        proj_type, full_w, full_h, full_w, full_h
    )
}
