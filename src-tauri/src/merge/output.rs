use crate::app::settings::AppSettings;
use crate::decode::baseline_exposure::BaselineOptions;
use crate::decode::formats::is_raw_file;
use crate::library::sidecars::parse_virtual_path;
use crate::merge::dng::{DngDescription, encode_linear_dng};
use crate::merge::panorama::projection::Projection;
use crate::merge::raster::Raster;
use crate::metadata::exif_processing::{read_exif_data_from_bytes, read_rrexif_sidecar};
use crate::render::agx::agx;
use crate::render::color_space::{OutputColorSpace, OutputTransform, srgb_transfer, working_to_srgb};
use crate::render::hdr::basic_raw_display;
use crate::render::image_processing::working_display_to_srgb8;
use base64::{Engine as _, engine::general_purpose};
use glam::Vec3;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{DynamicImage, ImageBuffer, ImageEncoder, Rgb, Rgb32FImage, RgbImage};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

const PREVIEW_JPEG_QUALITY: u8 = 90;
const LENS_KEYS: [&str; 6] = [
    "LensModel",
    "LensMake",
    "LensSpecification",
    "LensSerialNumber",
    "FocalLength",
    "FocalLengthIn35mmFilm",
];

fn tone_mapper_override(settings: &AppSettings) -> Option<bool> {
    crate::render::options::resolve_tonemapper_override(settings, true).map(|mode| mode == 1)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rendition {
    Display,
    SrgbDisplay,
    Scene { exposure: f32, agx: bool },
}

impl Rendition {
    pub fn for_sources(scene_referred: bool, reference: &str, settings: &AppSettings) -> Self {
        if !scene_referred {
            return Rendition::Display;
        }
        let exposure = if is_raw_file(reference) {
            crate::decode::as_shot::as_shot(reference)
                .exposure
                .compensation(BaselineOptions::from_settings(settings))
        } else {
            0.0
        };
        let default_agx = is_raw_file(reference)
            && crate::adjustments::develop_defaults::overlay(Path::new(reference))
                .get("toneMapper")
                .and_then(|v| v.as_str())
                == Some("agx");
        Rendition::Scene {
            exposure,
            agx: tone_mapper_override(settings).unwrap_or(default_agx),
        }
    }

    pub fn with_agx(self, settings: &AppSettings) -> Self {
        match self {
            Rendition::Scene { exposure, .. } => Rendition::Scene {
                exposure,
                agx: tone_mapper_override(settings).unwrap_or(true),
            },
            display => display,
        }
    }

    pub fn scene_exposure(self) -> f32 {
        match self {
            Rendition::Scene { exposure, .. } => exposure,
            _ => 0.0,
        }
    }

    fn display_pixel(self, linear: [f32; 3]) -> [f32; 3] {
        let transfer = srgb_transfer();
        match self {
            Rendition::SrgbDisplay => linear.map(|v| v.clamp(0.0, 1.0)),
            Rendition::Display => linear.map(|v| transfer.encode(v.max(0.0))),
            Rendition::Scene {
                exposure,
                agx: false,
            } => {
                let gain = exposure.exp2();
                linear.map(|v| transfer.encode(basic_raw_display((v * gain).max(0.0))))
            }
            Rendition::Scene {
                exposure,
                agx: true,
            } => {
                let gain = exposure.exp2();
                agx().tonemap(Vec3::from_array(linear) * gain).to_array()
            }
        }
    }

    pub fn to_srgb8(self, image: &Raster) -> RgbImage {
        if matches!(self, Rendition::SrgbDisplay) {
            let encoded: Vec<u8> = image
                .data
                .par_chunks_exact(3)
                .flat_map_iter(|p| [p[0], p[1], p[2]].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
                .collect();
            return RgbImage::from_raw(image.width as u32, image.height as u32, encoded)
                .expect("preview buffer matches its dimensions");
        }
        let encoded: Vec<f32> = image
            .data
            .par_chunks_exact(3)
            .flat_map_iter(|p| self.display_pixel([p[0], p[1], p[2]]))
            .collect();
        RgbImage::from_raw(
            image.width as u32,
            image.height as u32,
            working_display_to_srgb8(&encoded),
        )
        .expect("preview buffer matches its dimensions")
    }

    pub fn to_srgb16(self, image: &Raster) -> ImageBuffer<Rgb<u16>, Vec<u16>> {
        let encoded: Vec<u16> = if matches!(self, Rendition::SrgbDisplay) {
            image
                .data
                .par_chunks_exact(3)
                .flat_map_iter(|p| {
                    [p[0], p[1], p[2]].map(|c| (c.clamp(0.0, 1.0) * 65535.0).round() as u16)
                })
                .collect()
        } else {
            let output = OutputTransform::srgb(false);
            image
                .data
                .par_chunks_exact(3)
                .flat_map_iter(|p| {
                    output
                        .apply_srgb_encoded(self.display_pixel([p[0], p[1], p[2]]))
                        .map(|c| (c.clamp(0.0, 1.0) * 65535.0).round() as u16)
                })
                .collect()
        };
        ImageBuffer::<Rgb<u16>, Vec<u16>>::from_raw(
            image.width as u32,
            image.height as u32,
            encoded,
        )
        .expect("output buffer matches its dimensions")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MergeOutputFormat {
    #[default]
    Tiff,
    Jpeg,
    UltraHdr,
    Png,
    Dng,
}

impl MergeOutputFormat {
    pub fn from_settings(settings: &Value) -> Self {
        match settings
            .get("outputFormat")
            .and_then(|v| v.as_str())
            .map(|s| s.to_lowercase())
            .as_deref()
        {
            Some("jpeg") | Some("jpg") => MergeOutputFormat::Jpeg,
            Some("ultrahdr") | Some("ultra_hdr") => MergeOutputFormat::UltraHdr,
            Some("png") => MergeOutputFormat::Png,
            Some("dng") => MergeOutputFormat::Dng,
            _ => MergeOutputFormat::Tiff,
        }
    }
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct MergePreview {
    pub base64: String,
    pub overlay_base64: Option<String>,
    pub reference_base64: Option<String>,
    pub hdr_preview_base64: Option<String>,
    pub headroom_map_base64: Option<String>,
    pub hdr_peak_nits: Option<f32>,
    pub is_scene_referred: bool,
    pub width: usize,
    pub height: usize,
    pub image_count: usize,
    pub excluded_count: usize,
    pub unaligned_count: usize,
    pub projection: Option<Projection>,
    pub fov_deg: Option<f32>,
    pub projection_note: Option<String>,
    pub wraps: bool,
}

fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Inserts a standard PNG `cICP` chunk (`[9, 16, 0, 1]` = BT.2020 + SMPTE ST 2084 PQ + Full Range)
/// immediately after the `IHDR` chunk (byte offset 33).
fn inject_png_cicp_chunk(png_bytes: &mut Vec<u8>, cicp: [u8; 4]) {
    if png_bytes.len() < 33 || !png_bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return;
    }
    let mut chunk = Vec::with_capacity(16);
    chunk.extend_from_slice(&4u32.to_be_bytes());
    chunk.extend_from_slice(b"cICP");
    chunk.extend_from_slice(&cicp);
    let mut crc_input = Vec::with_capacity(8);
    crc_input.extend_from_slice(b"cICP");
    crc_input.extend_from_slice(&cicp);
    chunk.extend_from_slice(&crc32_ieee(&crc_input).to_be_bytes());
    png_bytes.splice(33..33, chunk);
}

/// Computes the 99.9th-percentile HDR peak luminance in nits cleanly without encoding PNG bytes.
pub fn compute_hdr_peak_nits(linear_working: &Raster, exposure: f32) -> f32 {
    let gain = exposure.exp2();
    let mut lumas: Vec<f32> = Vec::with_capacity(linear_working.width * linear_working.height);
    for p in linear_working.data.chunks_exact(3) {
        let r = if p[0].is_finite() { (p[0] * gain).max(0.0) } else { 0.0 };
        let g = if p[1].is_finite() { (p[1] * gain).max(0.0) } else { 0.0 };
        let b = if p[2].is_finite() { (p[2] * gain).max(0.0) } else { 0.0 };
        lumas.push(0.2627 * r + 0.6780 * g + 0.0593 * b);
    }
    let peak_scene = if lumas.is_empty() {
        1.0
    } else {
        let idx = ((lumas.len() as f32) * 0.999) as usize;
        let idx = idx.min(lumas.len() - 1);
        *lumas
            .select_nth_unstable_by(idx, |a, b| a.total_cmp(b))
            .1
    };
    (peak_scene * crate::enhanced::hdr_color_science::REFERENCE_WHITE_NITS)
        .clamp(crate::enhanced::hdr_color_science::REFERENCE_WHITE_NITS, 4000.0)
}

/// Encodes linear Rec.2020 scene-referred raster into a 16-bit HDR10 PQ PNG (`cICP [9,16,0,1]`)
/// base64 data URI and returns `(data_uri, peak_nits)`.
pub fn encode_hdr10_pq_png_base64(
    linear_working: &Raster,
    exposure: f32,
) -> Result<(String, f32), String> {
    let gain = exposure.exp2();
    let mut pq_pixels: Vec<u16> = Vec::with_capacity(linear_working.width * linear_working.height * 3);

    for p in linear_working.data.chunks_exact(3) {
        let r = if p[0].is_finite() { (p[0] * gain).max(0.0) } else { 0.0 };
        let g = if p[1].is_finite() { (p[1] * gain).max(0.0) } else { 0.0 };
        let b = if p[2].is_finite() { (p[2] * gain).max(0.0) } else { 0.0 };
        pq_pixels.push(crate::enhanced::hdr_color_science::pq_normalized_to_u16(
            crate::enhanced::hdr_color_science::linear_scene_to_pq(r),
        ));
        pq_pixels.push(crate::enhanced::hdr_color_science::pq_normalized_to_u16(
            crate::enhanced::hdr_color_science::linear_scene_to_pq(g),
        ));
        pq_pixels.push(crate::enhanced::hdr_color_science::pq_normalized_to_u16(
            crate::enhanced::hdr_color_science::linear_scene_to_pq(b),
        ));
    }

    let peak_nits = compute_hdr_peak_nits(linear_working, exposure);

    let img16 = ImageBuffer::<Rgb<u16>, Vec<u16>>::from_raw(
        linear_working.width as u32,
        linear_working.height as u32,
        pq_pixels,
    )
    .ok_or_else(|| "Failed to allocate HDR10 PQ buffer".to_string())?;

    let mut bytes = Vec::new();
    let encoder = PngEncoder::new_with_quality(
        &mut bytes,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::NoFilter,
    );
    DynamicImage::ImageRgb16(img16)
        .write_with_encoder(encoder)
        .map_err(|e| format!("Failed encoding HDR10 PQ PNG: {}", e))?;
    inject_png_cicp_chunk(&mut bytes, [9, 16, 0, 1]);

    Ok((
        format!("data:image/png;base64,{}", general_purpose::STANDARD.encode(&bytes)),
        peak_nits,
    ))
}

/// Encodes a false-color highlight headroom heatmap (SDR <= 203 nits in mono,
/// +1/+2/+3 EV HDR highlight stops highlighted in cyan/amber/magenta).
pub fn encode_hdr_headroom_heatmap_base64(
    linear_working: &Raster,
    exposure: f32,
) -> Result<String, String> {
    let gain = exposure.exp2();
    let transfer = srgb_transfer();
    let mut rgb8 = vec![0u8; linear_working.width * linear_working.height * 3];
    for (p, dst) in linear_working
        .data
        .chunks_exact(3)
        .zip(rgb8.chunks_exact_mut(3))
    {
        let lum = crate::render::color_space::luminance(p).max(0.0) * gain;
        if lum <= 1.0 {
            let y = (transfer.encode(lum) * 200.0).round() as u8;
            dst.copy_from_slice(&[y, y, y]);
        } else {
            let stops = lum.log2();
            let color = if stops < 1.0 {
                [56u8, 189, 248] // 0..+1 EV Cyan
            } else if stops < 2.0 {
                [251, 191, 36] // +1..+2 EV Amber
            } else if stops < 3.0 {
                [249, 115, 22] // +2..+3 EV Orange
            } else {
                [236, 72, 153] // +3 EV+ Magenta
            };
            dst.copy_from_slice(&color);
        }
    }
    let img = RgbImage::from_raw(
        linear_working.width as u32,
        linear_working.height as u32,
        rgb8,
    )
    .ok_or_else(|| "Failed to allocate HDR headroom map".to_string())?;
    encode_jpeg_base64(&img)
}

pub fn encode_jpeg_base64(image: &RgbImage) -> Result<String, String> {
    let mut buf = Cursor::new(Vec::new());
    JpegEncoder::new_with_quality(&mut buf, PREVIEW_JPEG_QUALITY)
        .encode_image(image)
        .map_err(|e| format!("Failed to encode the preview: {}", e))?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        general_purpose::STANDARD.encode(buf.get_ref())
    ))
}

pub struct MergeOutput {
    pub image: Raster,
    pub linear_scene: Option<Raster>,
    pub scene_exposure: f32,
    pub hdr_exposure_bias: f32,
    pub rendition: Rendition,
    pub suffix: &'static str,
    pub name_source: String,
    pub metadata_source: String,
    pub keeps_lens_geometry: bool,
    pub is_scene_referred: bool,
    pub is_360_equirectangular: bool,
    pub tone_mapper: Option<&'static str>,
}

fn inject_gpano_xmp_jpeg(bytes: &mut Vec<u8>, width: usize, height: usize) {
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return;
    }
    let xmp = format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
         <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
         <rdf:Description rdf:about=\"\" xmlns:GPano=\"http://ns.google.com/photos/1.0/panorama/\">\n\
         <GPano:UsePanoramaViewer>True</GPano:UsePanoramaViewer>\n\
         <GPano:ProjectionType>equirectangular</GPano:ProjectionType>\n\
         <GPano:CroppedAreaImageWidthPixels>{w}</GPano:CroppedAreaImageWidthPixels>\n\
         <GPano:CroppedAreaImageHeightPixels>{h}</GPano:CroppedAreaImageHeightPixels>\n\
         <GPano:FullPanoWidthPixels>{w}</GPano:FullPanoWidthPixels>\n\
         <GPano:FullPanoHeightPixels>{h}</GPano:FullPanoHeightPixels>\n\
         <GPano:CroppedAreaLeftPixels>0</GPano:CroppedAreaLeftPixels>\n\
         <GPano:CroppedAreaTopPixels>0</GPano:CroppedAreaTopPixels>\n\
         </rdf:Description>\n\
         </rdf:RDF>\n\
         </x:xmpmeta>\n\
         <?xpacket end=\"w\"?>",
        w = width,
        h = height,
    );
    const HEADER: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
    let payload_len = HEADER.len() + xmp.len();
    let total_len = 2 + payload_len;
    if total_len > u16::MAX as usize {
        return;
    }
    let mut segment = Vec::with_capacity(2 + total_len);
    segment.push(0xFF);
    segment.push(0xE1);
    segment.extend_from_slice(&(total_len as u16).to_be_bytes());
    segment.extend_from_slice(HEADER);
    segment.extend_from_slice(xmp.as_bytes());
    bytes.splice(2..2, segment);
}

fn source_exif(path: &str) -> HashMap<String, String> {
    read_rrexif_sidecar(Path::new(path))
        .or_else(|| {
            std::fs::read(path)
                .ok()
                .map(|bytes| read_exif_data_from_bytes(path, &bytes))
        })
        .unwrap_or_default()
}

pub fn write_output(
    name_source: &str,
    suffix: &str,
    extension: &str,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    let (first_path, _) = parse_virtual_path(name_source);
    let parent_dir = first_path
        .parent()
        .ok_or_else(|| "Could not determine the folder of the first image.".to_string())?;
    let stem = first_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| suffix.to_lowercase());
    crate::library::file_ops::write_unique_output(
        parent_dir,
        &format!("{}_{}", stem, suffix),
        extension,
        bytes,
    )
    .map_err(|e| format!("Failed to save the merged image: {}", e))
}

fn encode_display_tiff(
    image: &Raster,
    rendition: Rendition,
    metadata_source: &str,
) -> Result<Vec<u8>, String> {
    let rgb = rendition.to_srgb16(image);
    let dyn_img = DynamicImage::ImageRgb16(rgb);
    let dims = (image.width as u32, image.height as u32);
    let metadata = crate::metadata::export_metadata::collect_metadata_from_source(
        metadata_source,
        false,
        Some(dims),
        true,
    );
    crate::metadata::tiff_metadata::encode_compressed_tiff_with_metadata(
        &dyn_img,
        crate::metadata::tiff_metadata::TiffSamples::Sixteen,
        metadata.as_ref(),
        Some(OutputColorSpace::Srgb.icc_profile()),
        None,
    )
}

fn encode_rgb8_jpeg(
    rgb8: &RgbImage,
    metadata_source: &str,
) -> Result<Vec<u8>, String> {
    let dims = (rgb8.width(), rgb8.height());
    let mut bytes = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut bytes, 95);
    let _ = encoder.set_icc_profile(OutputColorSpace::Srgb.icc_profile().to_vec());
    encoder
        .encode_image(rgb8)
        .map_err(|e| format!("Failed to encode merged JPEG: {}", e))?;
    if !metadata_source.is_empty()
        && let Some(metadata) = crate::metadata::export_metadata::collect_metadata_from_source(
            metadata_source,
            false,
            Some(dims),
            true,
        )
    {
        let _ = metadata.write_to_vec(&mut bytes, little_exif::filetype::FileExtension::JPEG);
    }
    Ok(bytes)
}

fn encode_display_jpeg(
    image: &Raster,
    rendition: Rendition,
    metadata_source: &str,
) -> Result<Vec<u8>, String> {
    let rgb8 = rendition.to_srgb8(image);
    encode_rgb8_jpeg(&rgb8, metadata_source)
}

fn encode_display_png(
    image: &Raster,
    rendition: Rendition,
    metadata_source: &str,
) -> Result<Vec<u8>, String> {
    let rgb16 = rendition.to_srgb16(image);
    let mut bytes = Vec::new();
    let mut encoder = PngEncoder::new(&mut bytes);
    let _ = encoder.set_icc_profile(OutputColorSpace::Srgb.icc_profile().to_vec());
    DynamicImage::ImageRgb16(rgb16)
        .write_with_encoder(encoder)
        .map_err(|e| format!("Failed to encode merged PNG: {}", e))?;
    if !metadata_source.is_empty() && Path::new(metadata_source).exists() {
        let empty_xmp = crate::metadata::export_metadata::ExportXmp::from_metadata(
            &crate::metadata::sidecar::ImageMetadata::default(),
        );
        let _ = crate::metadata::export_metadata::write_image_with_metadata(
            &mut bytes,
            metadata_source,
            "png",
            false,
            &empty_xmp,
            None,
            true,
        );
    }
    Ok(bytes)
}

fn encode_ultrahdr_jpeg(
    image: &Raster,
    linear_scene: Option<&Raster>,
    scene_exposure: f32,
    hdr_exposure_bias: f32,
    rendition: Rendition,
    metadata_source: &str,
) -> Result<Vec<u8>, String> {
    let sdr_rgb8 = rendition.to_srgb8(image);
    let primary_bytes = encode_rgb8_jpeg(&sdr_rgb8, metadata_source)?;
    let Some(linear) = linear_scene else {
        return Ok(primary_bytes);
    };
    if linear.width != image.width || linear.height != image.height {
        return Ok(primary_bytes);
    }
    let sdr_dyn = DynamicImage::ImageRgb8(sdr_rgb8);
    let gain = (scene_exposure + hdr_exposure_bias).exp2();
    let to_srgb = working_to_srgb();
    let curve = OutputColorSpace::Srgb.curve();
    let hdr_raw: Vec<f32> = linear
        .data
        .par_chunks_exact(3)
        .flat_map_iter(|p| {
            let clean = Vec3::from_array([
                if p[0].is_finite() { p[0].max(0.0) } else { 0.0 },
                if p[1].is_finite() { p[1].max(0.0) } else { 0.0 },
                if p[2].is_finite() { p[2].max(0.0) } else { 0.0 },
            ]);
            let srgb_lin = to_srgb * (clean * gain);
            [
                curve.encode(srgb_lin.x.clamp(0.0, 64.0)),
                curve.encode(srgb_lin.y.clamp(0.0, 64.0)),
                curve.encode(srgb_lin.z.clamp(0.0, 64.0)),
            ]
        })
        .collect();
    let hdr_buf = Rgb32FImage::from_raw(linear.width as u32, linear.height as u32, hdr_raw)
        .ok_or_else(|| "Failed to allocate HDR buffer for UltraHDR gain map".to_string())?;
    let hdr_dyn = DynamicImage::ImageRgb32F(hdr_buf);
    crate::export::gain_map::attach(&primary_bytes, &sdr_dyn, &hdr_dyn, OutputColorSpace::Srgb, 92)
}

pub fn save_output(output: MergeOutput, format: MergeOutputFormat) -> Result<String, String> {
    let mut exif = source_exif(&output.metadata_source);
    if !output.keeps_lens_geometry {
        for key in LENS_KEYS {
            exif.remove(key);
        }
    }

    let (bytes, extension, suffix_override, is_scene_dng) = match format {
        MergeOutputFormat::Jpeg => {
            let mut jpeg =
                encode_display_jpeg(&output.image, output.rendition, &output.metadata_source)?;
            if output.is_360_equirectangular {
                inject_gpano_xmp_jpeg(&mut jpeg, output.image.width, output.image.height);
            }
            (jpeg, "jpg", None, false)
        }
        MergeOutputFormat::UltraHdr => {
            let mut jpeg = encode_ultrahdr_jpeg(
                &output.image,
                output.linear_scene.as_ref(),
                output.scene_exposure,
                output.hdr_exposure_bias,
                output.rendition,
                &output.metadata_source,
            )?;
            if output.is_360_equirectangular {
                inject_gpano_xmp_jpeg(&mut jpeg, output.image.width, output.image.height);
            }
            (jpeg, "jpg", Some(format!("{}_UltraHDR", output.suffix)), false)
        }
        MergeOutputFormat::Png => (
            encode_display_png(&output.image, output.rendition, &output.metadata_source)?,
            "png",
            None,
            false,
        ),
        MergeOutputFormat::Tiff => (
            encode_display_tiff(&output.image, output.rendition, &output.metadata_source)?,
            "tiff",
            None,
            false,
        ),
        MergeOutputFormat::Dng => {
            let scene_raster = output.linear_scene.as_ref().unwrap_or(&output.image);
            let is_scene = output.is_scene_referred
                && (output.linear_scene.is_some()
                    || matches!(output.rendition, Rendition::Scene { .. }));
            if is_scene {
                let text = |key: &str| {
                    exif.get(key)
                        .map(|v| v.trim().trim_matches('"').to_string())
                };
                let make = text("Make").unwrap_or_default();
                let model = text("Model").unwrap_or_default();
                let description = DngDescription {
                    make: &make,
                    model: &model,
                    baseline_exposure: output.scene_exposure,
                };
                (encode_linear_dng(scene_raster, &description)?, "dng", None, true)
            } else {
                (
                    encode_display_tiff(&output.image, output.rendition, &output.metadata_source)?,
                    "tiff",
                    None,
                    false,
                )
            }
        }
    };
    drop(output.image);
    drop(output.linear_scene);

    let effective_suffix = suffix_override.as_deref().unwrap_or(output.suffix);
    let path = write_output(&output.name_source, effective_suffix, extension, &bytes)?;
    if !exif.is_empty() {
        let _ = crate::metadata::sidecar::update_primary_metadata(&path, true, |metadata| {
            metadata.exif = Some(exif);
        });
    }
    if is_scene_dng
        && let Some(tone_mapper) = output.tone_mapper
    {
        let mut adjustments = crate::render::adjustment_utils::default_adjustments(&path);
        adjustments["toneMapper"] = Value::from(tone_mapper);
        let _ = crate::metadata::sidecar::update_primary_metadata(&path, true, |metadata| {
            metadata.adjustments = adjustments;
        });
    }
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_output_format_from_settings() {
        assert_eq!(
            MergeOutputFormat::from_settings(&serde_json::json!({})),
            MergeOutputFormat::Tiff
        );
        assert_eq!(
            MergeOutputFormat::from_settings(&serde_json::json!({ "outputFormat": "jpeg" })),
            MergeOutputFormat::Jpeg
        );
        assert_eq!(
            MergeOutputFormat::from_settings(&serde_json::json!({ "outputFormat": "dng" })),
            MergeOutputFormat::Dng
        );
        assert_eq!(
            MergeOutputFormat::from_settings(&serde_json::json!({ "outputFormat": "tiff" })),
            MergeOutputFormat::Tiff
        );
        assert_eq!(
            MergeOutputFormat::from_settings(&serde_json::json!({ "outputFormat": "ultrahdr" })),
            MergeOutputFormat::UltraHdr
        );
        assert_eq!(
            MergeOutputFormat::from_settings(&serde_json::json!({ "outputFormat": "png" })),
            MergeOutputFormat::Png
        );
    }

    #[test]
    fn test_srgb8_and_srgb16_agree_for_display_and_srgb_display() {
        let raster = Raster::from_data(
            3,
            1,
            3,
            vec![
                0.5, 0.5, 0.5,
                0.8, 0.2, 0.1,
                0.1, 0.7, 0.4,
            ],
        );
        for rendition in [Rendition::Display, Rendition::SrgbDisplay] {
            let srgb8 = rendition.to_srgb8(&raster);
            let srgb16 = rendition.to_srgb16(&raster);
            for (p8, p16) in srgb8.pixels().zip(srgb16.pixels()) {
                for c in 0..3 {
                    let from_16 = ((p16[c] as f32 / 65535.0) * 255.0).round() as i32;
                    assert!(
                        (p8[c] as i32 - from_16).abs() <= 1,
                        "mismatch for {:?}: 8-bit={} vs 16-bit->8={}",
                        rendition,
                        p8[c],
                        from_16
                    );
                }
            }
        }
    }

    #[test]
    fn test_encode_display_png_embeds_iccp_and_decodes() {
        let raster = Raster::from_data(2, 2, 3, vec![0.5; 12]);
        let bytes = encode_display_png(&raster, Rendition::SrgbDisplay, "").unwrap();
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(bytes.windows(4).any(|w| w == b"iCCP"), "PNG must embed sRGB iCCP chunk");
    }

    #[test]
    fn test_ultrahdr_sanitizes_nan_and_applies_exposure_bias() {
        let sdr = Raster::from_data(4, 4, 3, vec![0.5; 48]);
        let mut linear = Raster::from_data(4, 4, 3, vec![0.5; 48]);
        linear.data[0] = f32::NAN;
        linear.data[1] = f32::INFINITY;
        linear.data[2] = 10000.0;
        let bytes = encode_ultrahdr_jpeg(&sdr, Some(&linear), 0.0, 1.0, Rendition::SrgbDisplay, "").unwrap();
        assert!(bytes.starts_with(&[0xFF, 0xD8]));
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("GainMapMax=\"NaN\""), "UltraHDR XMP must never contain NaN");
    }

    #[test]
    fn test_inject_gpano_xmp_jpeg_inserts_valid_app1_segment() {
        let raster = Raster::from_data(4, 2, 3, vec![0.5; 24]);
        let mut jpeg = encode_display_jpeg(&raster, Rendition::SrgbDisplay, "").unwrap();
        inject_gpano_xmp_jpeg(&mut jpeg, 4096, 2048);
        assert_eq!(&jpeg[0..4], &[0xFF, 0xD8, 0xFF, 0xE1]);
        let text = String::from_utf8_lossy(&jpeg);
        assert!(text.contains("<GPano:UsePanoramaViewer>True</GPano:UsePanoramaViewer>"));
        assert!(text.contains("<GPano:ProjectionType>equirectangular</GPano:ProjectionType>"));
        assert!(text.contains("<GPano:FullPanoWidthPixels>4096</GPano:FullPanoWidthPixels>"));
    }

    #[test]
    fn test_encode_hdr10_pq_png_emits_valid_cicp_bt2020_pq_chunk() {
        // Scene radiance up to 4.0 (2 stops above diffuse 203-nit white -> ~812 nits)
        let raster = Raster::from_data(2, 2, 3, vec![0.25, 0.25, 0.25, 1.0, 1.0, 1.0, 4.0, 4.0, 4.0, 2.0, 2.0, 2.0]);
        let (uri, peak_nits) = encode_hdr10_pq_png_base64(&raster, 0.0).unwrap();
        assert!(uri.starts_with("data:image/png;base64,"));
        assert!(peak_nits >= 800.0, "Expected peak_nits >= 800 nits, got {}", peak_nits);
        let b64 = uri.trim_start_matches("data:image/png;base64,");
        let raw = general_purpose::STANDARD.decode(b64).unwrap();
        assert!(raw.starts_with(b"\x89PNG\r\n\x1a\n"));
        // Locate cICP chunk immediately at offset 33: [0,0,0,4, 'c','I','C','P', 9, 16, 0, 1]
        assert_eq!(&raw[37..41], b"cICP");
        assert_eq!(&raw[41..45], &[9, 16, 0, 1]);
    }
}
