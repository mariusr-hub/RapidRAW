use crate::adjustments::lut_processing::Lut;
use crate::app::settings::resolved_app_data_dir;
use crate::decode::white_balance::{self, WhiteBalance};
use crate::render::color_space::{prophoto_to_working, working_profile, working_to_prophoto};
use glam::Vec3;
use image::{DynamicImage, Rgb32FImage};
use moxcms::{ColorProfile, DataColorSpace, Layout, RenderingIntent, TransformOptions};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use tauri::AppHandle;

const TAG_UNIQUE_CAMERA_MODEL: u16 = 50708;
const TAG_COLOR_MATRIX_1: u16 = 50721;
const TAG_COLOR_MATRIX_2: u16 = 50722;
const TAG_CALIBRATION_ILLUMINANT_1: u16 = 50778;
const TAG_CALIBRATION_ILLUMINANT_2: u16 = 50779;
const TAG_PROFILE_NAME: u16 = 50936;
const TAG_PROFILE_HUE_SAT_MAP_DIMS: u16 = 50937;
const TAG_PROFILE_HUE_SAT_MAP_DATA_1: u16 = 50938;
const TAG_PROFILE_HUE_SAT_MAP_DATA_2: u16 = 50939;
const TAG_PROFILE_TONE_CURVE: u16 = 50940;
const TAG_FORWARD_MATRIX_1: u16 = 50964;
const TAG_FORWARD_MATRIX_2: u16 = 50965;
const TAG_PROFILE_LOOK_TABLE_DIMS: u16 = 50981;
const TAG_PROFILE_LOOK_TABLE_DATA: u16 = 50982;
const TAG_PROFILE_LOOK_TABLE_ENCODING: u16 = 51108;
const TAG_BASELINE_EXPOSURE_OFFSET: u16 = 51109;

const DCP_LUT_SIZE: u32 = 33;
const DCP_SHAPER_MAX: f32 = 4.0;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CameraProfileInfo {
    pub id: String,
    pub name: String,
    pub camera_model: String,
    pub format: String,
    pub path: String,
    pub dual_illuminant: bool,
    pub has_hue_sat_map: bool,
    pub has_tone_curve: bool,
}

#[derive(Clone, Debug)]
pub struct DcpProfile {
    pub name: String,
    pub camera_model: String,
    pub illuminant1_kelvin: f64,
    pub illuminant2_kelvin: Option<f64>,
    pub color_matrix_1: Option<[[f32; 3]; 3]>,
    pub color_matrix_2: Option<[[f32; 3]; 3]>,
    pub forward_matrix_1: Option<[[f32; 3]; 3]>,
    pub forward_matrix_2: Option<[[f32; 3]; 3]>,
    pub hue_sat_dims: Option<[usize; 3]>, // [hue_divs, sat_divs, val_divs]
    pub hue_sat_data_1: Option<Vec<[f32; 3]>>,
    pub hue_sat_data_2: Option<Vec<[f32; 3]>>,
    pub look_table_dims: Option<[usize; 3]>,
    pub look_table_data: Option<Vec<[f32; 3]>>,
    pub look_table_encoding: u32,
    pub baseline_exposure_offset: f32,
    pub tone_curve: Option<Vec<[f32; 2]>>,
}

#[derive(Clone)]
enum CachedCameraProfile {
    Dcp(DcpProfile),
    Icc(Vec<u8>),
}

static CAMERA_PROFILE_CACHE: LazyLock<Mutex<HashMap<String, CachedCameraProfile>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static DCP_GPU_LUT_CACHE: LazyLock<Mutex<HashMap<(String, u32), Arc<Lut>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static CAMERA_PROFILES_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn dng_illuminant_kelvin(code: u16) -> f64 {
    match code {
        17 => 2856.0, // Standard light A
        18 => 4874.0, // Standard light B
        19 => 6774.0, // Standard light C
        20 => 5503.0, // D55
        21 => 6504.0, // D65
        22 => 7504.0, // D75
        23 => 5003.0, // D50
        3 => 3200.0,  // Tungsten
        2 => 4200.0,  // Fluorescent
        _ => 6504.0,
    }
}

/// Computes normalized mired interpolation factor `w in [0.0, 1.0]` between
/// `ill1_kelvin` (`w = 0.0`) and `ill2_kelvin` (`w = 1.0`).
pub fn interpolate_mired_weight(temp_kelvin: f64, ill1_kelvin: f64, ill2_kelvin: f64) -> f64 {
    let m_target = 1_000_000.0 / temp_kelvin.max(1000.0);
    let m1 = 1_000_000.0 / ill1_kelvin.max(1000.0);
    let m2 = 1_000_000.0 / ill2_kelvin.max(1000.0);
    let span = m2 - m1;
    if span.abs() < 1e-6 {
        return 0.5;
    }
    ((m_target - m1) / span).clamp(0.0, 1.0)
}

pub fn interpolate_matrix3(m1: &[[f32; 3]; 3], m2: &[[f32; 3]; 3], t: f32) -> [[f32; 3]; 3] {
    let w = t.clamp(0.0, 1.0);
    let inv = 1.0 - w;
    let mut out = [[0.0f32; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            out[r][c] = m1[r][c] * inv + m2[r][c] * w;
        }
    }
    out
}

pub fn interpolate_huesatmap(d1: &[[f32; 3]], d2: &[[f32; 3]], t: f32) -> Vec<[f32; 3]> {
    let w = t.clamp(0.0, 1.0);
    let inv = 1.0 - w;
    d1.iter()
        .zip(d2.iter())
        .map(|(a, b)| {
            [
                a[0] * inv + b[0] * w,
                a[1] * inv + b[1] * w,
                a[2] * inv + b[2] * w,
            ]
        })
        .collect()
}

fn read_u16(bytes: &[u8], off: usize, le: bool) -> Result<u16, String> {
    let slice = bytes
        .get(off..off + 2)
        .ok_or_else(|| "Unexpected EOF reading u16".to_string())?;
    Ok(if le {
        u16::from_le_bytes([slice[0], slice[1]])
    } else {
        u16::from_be_bytes([slice[0], slice[1]])
    })
}

fn read_u32(bytes: &[u8], off: usize, le: bool) -> Result<u32, String> {
    let slice = bytes
        .get(off..off + 4)
        .ok_or_else(|| "Unexpected EOF reading u32".to_string())?;
    Ok(if le {
        u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]])
    } else {
        u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]])
    })
}

fn read_i32(bytes: &[u8], off: usize, le: bool) -> Result<i32, String> {
    let slice = bytes
        .get(off..off + 4)
        .ok_or_else(|| "Unexpected EOF reading i32".to_string())?;
    Ok(if le {
        i32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]])
    } else {
        i32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]])
    })
}

fn read_f32(bytes: &[u8], off: usize, le: bool) -> Result<f32, String> {
    let bits = read_u32(bytes, off, le)?;
    Ok(f32::from_bits(bits))
}

fn type_unit_size(field_type: u16) -> usize {
    match field_type {
        1 | 2 | 6 | 7 => 1, // BYTE, ASCII, SBYTE, UNDEFINED
        3 | 8 => 2,         // SHORT, SSHORT
        4 | 9 | 11 => 4,    // LONG, SLONG, FLOAT
        5 | 10 | 12 => 8,   // RATIONAL, SRATIONAL, DOUBLE
        _ => 0,
    }
}

fn read_tag_payload(
    bytes: &[u8],
    entry_off: usize,
    field_type: u16,
    count: u32,
    le: bool,
) -> Result<Vec<u8>, String> {
    let unit = type_unit_size(field_type);
    let total = unit
        .checked_mul(count as usize)
        .ok_or_else(|| "Tag byte size overflow".to_string())?;
    if total == 0 {
        return Ok(Vec::new());
    }
    let data_off = if total <= 4 {
        entry_off + 8
    } else {
        read_u32(bytes, entry_off + 8, le)? as usize
    };
    bytes
        .get(data_off..data_off + total)
        .map(|s| s.to_vec())
        .ok_or_else(|| "DCP tag payload out of bounds".to_string())
}

fn decode_srational_matrix3(payload: &[u8], le: bool) -> Option<[[f32; 3]; 3]> {
    if payload.len() < 9 * 8 {
        return None;
    }
    let mut vals = [0.0f32; 9];
    for (i, dst) in vals.iter_mut().enumerate() {
        let num = read_i32(payload, i * 8, le).ok()? as f64;
        let den = read_i32(payload, i * 8 + 4, le).ok()? as f64;
        *dst = if den.abs() > 1e-9 {
            (num / den) as f32
        } else {
            0.0
        };
    }
    Some([
        [vals[0], vals[1], vals[2]],
        [vals[3], vals[4], vals[5]],
        [vals[6], vals[7], vals[8]],
    ])
}

fn decode_f32_triplets(payload: &[u8], le: bool) -> Vec<[f32; 3]> {
    let count = payload.len() / 12;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let base = i * 12;
        if let (Ok(a), Ok(b), Ok(c)) = (
            read_f32(payload, base, le),
            read_f32(payload, base + 4, le),
            read_f32(payload, base + 8, le),
        ) {
            out.push([a, b, c]);
        }
    }
    out
}

/// Parses an Adobe DNG Camera Profile (`.dcp`) binary buffer.
pub fn parse_dcp_bytes(bytes: &[u8]) -> Result<DcpProfile, String> {
    if bytes.len() < 8 {
        return Err("DCP file too short".to_string());
    }
    let le = match &bytes[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return Err("Invalid DCP TIFF byte order marker".to_string()),
    };
    let magic = read_u16(bytes, 2, le)?;
    // Adobe DCP uses standard TIFF 42 or DCP extended magic 0x4352 ("CR")
    if magic != 42 && magic != 0x4352 {
        return Err(format!("Unsupported DCP magic number: {}", magic));
    }

    let ifd_off = read_u32(bytes, 4, le)? as usize;
    let num_entries = read_u16(bytes, ifd_off, le)? as usize;

    let mut profile = DcpProfile {
        name: String::new(),
        camera_model: String::new(),
        illuminant1_kelvin: 6504.0,
        illuminant2_kelvin: None,
        color_matrix_1: None,
        color_matrix_2: None,
        forward_matrix_1: None,
        forward_matrix_2: None,
        hue_sat_dims: None,
        hue_sat_data_1: None,
        hue_sat_data_2: None,
        look_table_dims: None,
        look_table_data: None,
        look_table_encoding: 0,
        baseline_exposure_offset: 0.0,
        tone_curve: None,
    };

    for idx in 0..num_entries {
        let entry_off = ifd_off + 2 + idx * 12;
        if entry_off + 12 > bytes.len() {
            break;
        }
        let tag = read_u16(bytes, entry_off, le)?;
        let field_type = read_u16(bytes, entry_off + 2, le)?;
        let count = read_u32(bytes, entry_off + 4, le)?;
        let Ok(payload) = read_tag_payload(bytes, entry_off, field_type, count, le) else {
            continue;
        };

        match tag {
            TAG_UNIQUE_CAMERA_MODEL => {
                profile.camera_model = String::from_utf8_lossy(&payload)
                    .trim_end_matches('\0')
                    .to_string();
            }
            TAG_PROFILE_NAME => {
                profile.name = String::from_utf8_lossy(&payload)
                    .trim_end_matches('\0')
                    .to_string();
            }
            TAG_CALIBRATION_ILLUMINANT_1 => {
                if payload.len() >= 2 {
                    profile.illuminant1_kelvin = dng_illuminant_kelvin(read_u16(&payload, 0, le)?);
                }
            }
            TAG_CALIBRATION_ILLUMINANT_2 => {
                if payload.len() >= 2 {
                    let code = read_u16(&payload, 0, le)?;
                    if code != 0 {
                        profile.illuminant2_kelvin = Some(dng_illuminant_kelvin(code));
                    }
                }
            }
            TAG_COLOR_MATRIX_1 => {
                profile.color_matrix_1 = decode_srational_matrix3(&payload, le);
            }
            TAG_COLOR_MATRIX_2 => {
                profile.color_matrix_2 = decode_srational_matrix3(&payload, le);
            }
            TAG_FORWARD_MATRIX_1 => {
                profile.forward_matrix_1 = decode_srational_matrix3(&payload, le);
            }
            TAG_FORWARD_MATRIX_2 => {
                profile.forward_matrix_2 = decode_srational_matrix3(&payload, le);
            }
            TAG_PROFILE_HUE_SAT_MAP_DIMS => {
                if payload.len() >= 12 {
                    let h = read_u32(&payload, 0, le)? as usize;
                    let s = read_u32(&payload, 4, le)? as usize;
                    let v = read_u32(&payload, 8, le)? as usize;
                    if h > 0 && s > 0 && v > 0 {
                        profile.hue_sat_dims = Some([h, s, v]);
                    }
                }
            }
            TAG_PROFILE_HUE_SAT_MAP_DATA_1 => {
                profile.hue_sat_data_1 = Some(decode_f32_triplets(&payload, le));
            }
            TAG_PROFILE_HUE_SAT_MAP_DATA_2 => {
                profile.hue_sat_data_2 = Some(decode_f32_triplets(&payload, le));
            }
            TAG_PROFILE_LOOK_TABLE_DIMS => {
                if payload.len() >= 12 {
                    let h = read_u32(&payload, 0, le)? as usize;
                    let s = read_u32(&payload, 4, le)? as usize;
                    let v = read_u32(&payload, 8, le)? as usize;
                    if h > 0 && s > 0 && v > 0 {
                        profile.look_table_dims = Some([h, s, v]);
                    }
                }
            }
            TAG_PROFILE_LOOK_TABLE_DATA => {
                profile.look_table_data = Some(decode_f32_triplets(&payload, le));
            }
            TAG_PROFILE_LOOK_TABLE_ENCODING => {
                if payload.len() >= 4 {
                    profile.look_table_encoding = read_u32(&payload, 0, le).unwrap_or(0);
                } else if payload.len() >= 2 {
                    profile.look_table_encoding = read_u16(&payload, 0, le).unwrap_or(0) as u32;
                }
            }
            TAG_BASELINE_EXPOSURE_OFFSET => {
                if payload.len() >= 8 {
                    let num = read_i32(&payload, 0, le).unwrap_or(0) as f64;
                    let den = read_i32(&payload, 4, le).unwrap_or(1) as f64;
                    if den.abs() > 1e-9 {
                        profile.baseline_exposure_offset = ((num / den) as f32).clamp(-6.0, 6.0);
                    }
                }
            }
            TAG_PROFILE_TONE_CURVE => {
                let pairs = payload.len() / 8;
                let mut curve = Vec::with_capacity(pairs);
                for i in 0..pairs {
                    if let (Ok(x), Ok(y)) = (
                        read_f32(&payload, i * 8, le),
                        read_f32(&payload, i * 8 + 4, le),
                    ) {
                        curve.push([x, y]);
                    }
                }
                if curve.len() >= 2 {
                    curve.sort_by(|a, b| a[0].total_cmp(&b[0]));
                    profile.tone_curve = Some(curve);
                }
            }
            _ => {}
        }
    }

    if profile.name.is_empty() {
        profile.name = if !profile.camera_model.is_empty() {
            format!("{} Profile", profile.camera_model)
        } else {
            "Custom DCP Profile".to_string()
        };
    }
    Ok(profile)
}

#[inline]
fn rgb_to_hsv_norm(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max_c = r.max(g).max(b);
    let min_c = r.min(g).min(b);
    let delta = max_c - min_c;
    let h = if delta < 1e-6 {
        0.0
    } else if (max_c - r).abs() < 1e-6 {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if (max_c - g).abs() < 1e-6 {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };
    let s = if max_c > 1e-6 { delta / max_c } else { 0.0 };
    (h, s, max_c)
}

#[inline]
fn hsv_to_rgb_norm(h_deg: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let s = s.clamp(0.0, 1.0);
    let h = h_deg.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r1 + m, g1 + m, b1 + m)
}

fn sample_hsm_trilinear(dims: [usize; 3], table: &[[f32; 3]], h_deg: f32, s: f32, v: f32) -> [f32; 3] {
    let [h_divs, s_divs, v_divs] = dims;
    let hf = (h_deg.rem_euclid(360.0) / 360.0) * h_divs as f32;
    let sf = s.clamp(0.0, 1.0) * (s_divs.saturating_sub(1)) as f32;
    let vf = v.clamp(0.0, 1.0) * (v_divs.saturating_sub(1)) as f32;

    let h0 = (hf.floor() as usize) % h_divs;
    let h1 = (h0 + 1) % h_divs;
    let s0 = (sf.floor() as usize).min(s_divs.saturating_sub(1));
    let s1 = (s0 + 1).min(s_divs.saturating_sub(1));
    let v0 = (vf.floor() as usize).min(v_divs.saturating_sub(1));
    let v1 = (v0 + 1).min(v_divs.saturating_sub(1));

    let fh = hf - hf.floor();
    let fs = sf - sf.floor();
    let fv = vf - vf.floor();

    let get = |vi: usize, si: usize, hi: usize| -> [f32; 3] {
        let idx = (vi * s_divs + si) * h_divs + hi;
        table.get(idx).copied().unwrap_or([0.0, 1.0, 1.0])
    };

    let lerp3 = |a: [f32; 3], b: [f32; 3], t: f32| -> [f32; 3] {
        [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
        ]
    };

    let c00 = lerp3(get(v0, s0, h0), get(v0, s0, h1), fh);
    let c01 = lerp3(get(v0, s1, h0), get(v0, s1, h1), fh);
    let c10 = lerp3(get(v1, s0, h0), get(v1, s0, h1), fh);
    let c11 = lerp3(get(v1, s1, h0), get(v1, s1, h1), fh);

    let c0 = lerp3(c00, c01, fs);
    let c1 = lerp3(c10, c11, fs);
    lerp3(c0, c1, fv)
}

fn linear_to_srgb_scalar(c: f32) -> f32 {
    let x = c.max(0.0);
    if x <= 0.003_130_8 {
        x * 12.92
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

fn srgb_to_linear_scalar(c: f32) -> f32 {
    let x = c.max(0.0);
    if x <= 0.040_45 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

fn eval_tone_curve(curve: &[[f32; 2]], x: f32) -> f32 {
    if curve.len() < 2 {
        return x;
    }
    let clamped = x.max(0.0);
    if clamped <= curve[0][0] {
        return curve[0][1];
    }
    for win in curve.windows(2) {
        let [x0, y0] = win[0];
        let [x1, y1] = win[1];
        if clamped <= x1 {
            let t = if (x1 - x0).abs() > 1e-6 {
                (clamped - x0) / (x1 - x0)
            } else {
                0.0
            };
            return y0 + (y1 - y0) * t;
        }
    }
    let prev = curve[curve.len() - 2];
    let last = curve[curve.len() - 1];
    let dx = (last[0] - prev[0]).max(1e-4);
    let end_slope = ((last[1] - prev[1]) / dx).clamp(0.1, 4.0);
    last[1] + (clamped - last[0]).max(0.0) * end_slope
}

fn apply_hsv_table(rgb: Vec3, dims: [usize; 3], table: &[[f32; 3]], srgb_encode: bool) -> Vec3 {
    let mut p = if srgb_encode {
        Vec3::new(
            linear_to_srgb_scalar(rgb.x),
            linear_to_srgb_scalar(rgb.y),
            linear_to_srgb_scalar(rgb.z),
        )
    } else {
        rgb.max(Vec3::ZERO)
    };
    let (mut hh, mut ss, vv) = rgb_to_hsv_norm(p.x, p.y, p.z);
    let [dh, ms, mv] = sample_hsm_trilinear(dims, table, hh, ss, vv.min(1.0));
    hh = (hh + dh).rem_euclid(360.0);
    ss = (ss * ms).clamp(0.0, 1.0);
    let new_vv = (vv * mv).max(0.0);
    let (nr, ng, nb) = hsv_to_rgb_norm(hh, ss, new_vv);
    p = Vec3::new(nr, ng, nb);
    if srgb_encode {
        // Preserve HDR headroom smoothly above 1.0 when LookTable is sRGB-encoded.
        let hdr_scale = if rgb.max_element() > 1.0 {
            let ref_max = p.max_element().max(1e-5);
            (rgb.max_element() / ref_max).max(1.0)
        } else {
            1.0
        };
        Vec3::new(
            srgb_to_linear_scalar(p.x),
            srgb_to_linear_scalar(p.y),
            srgb_to_linear_scalar(p.z),
        ) * hdr_scale
    } else {
        p
    }
}

struct PreparedDcpEval<'a> {
    exposure_scale: f32,
    hsm_table: Option<([usize; 3], Vec<[f32; 3]>)>,
    look_table: Option<([usize; 3], &'a [[f32; 3]], bool)>,
    tone_curve: Option<&'a [[f32; 2]]>,
    to_prophoto: glam::Mat3,
    from_prophoto: glam::Mat3,
}

impl<'a> PreparedDcpEval<'a> {
    fn new(dcp: &'a DcpProfile, temp_kelvin: f64) -> Option<Self> {
        let t_interp = if let Some(ill2) = dcp.illuminant2_kelvin {
            interpolate_mired_weight(temp_kelvin, dcp.illuminant1_kelvin, ill2) as f32
        } else {
            0.0
        };

        let hsm_table = dcp.hue_sat_dims.and_then(|dims| {
            match (&dcp.hue_sat_data_1, &dcp.hue_sat_data_2) {
                (Some(d1), Some(d2)) if d1.len() == d2.len() => {
                    Some((dims, interpolate_huesatmap(d1, d2, t_interp)))
                }
                (Some(d1), _) => Some((dims, d1.clone())),
                _ => None,
            }
        });

        let look_table = dcp.look_table_dims.and_then(|dims| {
            dcp.look_table_data
                .as_deref()
                .map(|d| (dims, d, dcp.look_table_encoding != 0))
        });

        let exposure_scale = if dcp.baseline_exposure_offset.abs() > 1e-4 {
            2.0_f32.powf(dcp.baseline_exposure_offset)
        } else {
            1.0
        };

        let tone_curve = dcp.tone_curve.as_deref();
        if hsm_table.is_none()
            && look_table.is_none()
            && tone_curve.is_none()
            && (exposure_scale - 1.0).abs() <= 1e-4
        {
            return None;
        }

        Some(Self {
            exposure_scale,
            hsm_table,
            look_table,
            tone_curve,
            to_prophoto: working_to_prophoto(),
            from_prophoto: prophoto_to_working(),
        })
    }

    #[inline]
    fn eval_working(&self, working_rgb: Vec3) -> Vec3 {
        let mut p = (self.to_prophoto * working_rgb).max(Vec3::ZERO) * self.exposure_scale;
        if let Some((dims, ref table)) = self.hsm_table {
            p = apply_hsv_table(p, dims, table, false);
        }
        if let Some((dims, table, srgb_encode)) = self.look_table {
            p = apply_hsv_table(p, dims, table, srgb_encode);
        }
        if let Some(c) = self.tone_curve {
            p = Vec3::new(
                eval_tone_curve(c, p.x),
                eval_tone_curve(c, p.y),
                eval_tone_curve(c, p.z),
            );
        }
        (self.from_prophoto * p).max(Vec3::ZERO)
    }
}

fn apply_dcp_profile(rgb: &mut Rgb32FImage, dcp: &DcpProfile) {
    let Some(eval) = PreparedDcpEval::new(dcp, 6504.0) else {
        return;
    };

    let (w, _h) = rgb.dimensions();
    let row_stride = (w * 3) as usize;
    rgb.as_mut()
        .par_chunks_mut(row_stride)
        .for_each(|row| {
            for x in 0..w as usize {
                let idx = x * 3;
                let out = eval.eval_working(Vec3::new(
                    row[idx].max(0.0),
                    row[idx + 1].max(0.0),
                    row[idx + 2].max(0.0),
                ));
                row[idx] = out.x;
                row[idx + 1] = out.y;
                row[idx + 2] = out.z;
            }
        });
}

fn apply_icc_camera_profile(rgb: &mut Rgb32FImage, icc_bytes: &[u8]) {
    let Ok(in_prof) = ColorProfile::new_from_slice(icc_bytes) else {
        return;
    };
    if in_prof.color_space != DataColorSpace::Rgb {
        return;
    }
    let out_prof = working_profile();
    let Ok(xform) = in_prof.create_transform_f32(
        Layout::Rgb,
        &out_prof,
        Layout::Rgb,
        TransformOptions {
            rendering_intent: RenderingIntent::RelativeColorimetric,
            ..TransformOptions::default()
        },
    ) else {
        return;
    };
    let mut dst = vec![0.0f32; rgb.as_raw().len()];
    if xform.transform(rgb.as_raw(), &mut dst).is_ok() {
        rgb.as_mut().copy_from_slice(&dst);
    }
}

fn bake_dcp_lut(dcp: &DcpProfile, temp_kelvin: f64) -> Option<Arc<Lut>> {
    let eval = PreparedDcpEval::new(dcp, temp_kelvin)?;
    let size = DCP_LUT_SIZE as usize;
    let denom = (size - 1) as f32;
    let mut data = vec![0.0f32; size * size * size * 3];

    data.par_chunks_mut(size * size * 3)
        .enumerate()
        .for_each(|(b_idx, plane)| {
            let ub = b_idx as f32 / denom;
            let lb = DCP_SHAPER_MAX * ub * ub;
            for g_idx in 0..size {
                let ug = g_idx as f32 / denom;
                let lg = DCP_SHAPER_MAX * ug * ug;
                let row_off = g_idx * size * 3;
                for r_idx in 0..size {
                    let ur = r_idx as f32 / denom;
                    let lr = DCP_SHAPER_MAX * ur * ur;
                    let out = eval.eval_working(Vec3::new(lr, lg, lb));
                    let i = row_off + r_idx * 3;
                    plane[i] = out.x;
                    plane[i + 1] = out.y;
                    plane[i + 2] = out.z;
                }
            }
        });

    Some(Arc::new(Lut {
        size: DCP_LUT_SIZE,
        data,
    }))
}

fn bake_icc_lut(icc_bytes: &[u8]) -> Option<Arc<Lut>> {
    let in_prof = ColorProfile::new_from_slice(icc_bytes).ok()?;
    if in_prof.color_space != DataColorSpace::Rgb {
        return None;
    }
    let out_prof = working_profile();
    let xform = in_prof
        .create_transform_f32(
            Layout::Rgb,
            &out_prof,
            Layout::Rgb,
            TransformOptions {
                rendering_intent: RenderingIntent::RelativeColorimetric,
                ..TransformOptions::default()
            },
        )
        .ok()?;

    let size = DCP_LUT_SIZE as usize;
    let denom = (size - 1) as f32;
    let count = size * size * size;
    let mut normalized_in = vec![0.0f32; count * 3];
    let mut scales = vec![1.0f32; count];

    for b in 0..size {
        let ub = b as f32 / denom;
        let lb = DCP_SHAPER_MAX * ub * ub;
        for g in 0..size {
            let ug = g as f32 / denom;
            let lg = DCP_SHAPER_MAX * ug * ug;
            for r in 0..size {
                let ur = r as f32 / denom;
                let lr = DCP_SHAPER_MAX * ur * ur;
                let idx = (b * size + g) * size + r;
                let peak = lr.max(lg).max(lb);
                let scale = if peak > 1.0 { peak } else { 1.0 };
                scales[idx] = scale;
                normalized_in[idx * 3] = (lr / scale).clamp(0.0, 1.0);
                normalized_in[idx * 3 + 1] = (lg / scale).clamp(0.0, 1.0);
                normalized_in[idx * 3 + 2] = (lb / scale).clamp(0.0, 1.0);
            }
        }
    }

    let mut data = vec![0.0f32; count * 3];
    xform.transform(&normalized_in, &mut data).ok()?;
    for (idx, chunk) in data.as_chunks_mut::<3>().0.iter_mut().enumerate() {
        let s = scales[idx];
        chunk[0] = (chunk[0] * s).max(0.0);
        chunk[1] = (chunk[1] * s).max(0.0);
        chunk[2] = (chunk[2] * s).max(0.0);
    }

    Some(Arc::new(Lut {
        size: DCP_LUT_SIZE,
        data,
    }))
}

fn load_cached_profile_by_id(profile_id: &str) -> Option<CachedCameraProfile> {
    if let Some(existing) = CAMERA_PROFILE_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(profile_id)
        .cloned()
    {
        return Some(existing);
    }

    let dir = CAMERA_PROFILES_DIR.get()?;
    let path = dir.join(profile_id);
    if !path.is_file() {
        return None;
    }
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let bytes = fs::read(&path).ok()?;
    let item = if ext == "dcp" {
        CachedCameraProfile::Dcp(parse_dcp_bytes(&bytes).ok()?)
    } else if ext == "icc" || ext == "icm" {
        let icc = ColorProfile::new_from_slice(&bytes).ok()?;
        if icc.color_space != DataColorSpace::Rgb {
            return None;
        }
        CachedCameraProfile::Icc(bytes)
    } else {
        return None;
    };

    CAMERA_PROFILE_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(profile_id.to_string(), item.clone());
    Some(item)
}

/// Resolves and caches a GPU-resident 33³ linear-shaper Rec.2020 LUT for the selected camera profile.
pub fn resolve_dcp_gpu_lut(
    adjustments: &Value,
    as_shot_wb: Option<WhiteBalance>,
) -> Option<Arc<Lut>> {
    let profile_id = adjustments["cameraProfileId"]
        .as_str()
        .unwrap_or("")
        .trim();
    if profile_id.is_empty() {
        return None;
    }

    let cached = load_cached_profile_by_id(profile_id)?;
    let (temp_kelvin, kelvin_bucket) = match &cached {
        CachedCameraProfile::Dcp(dcp) if dcp.illuminant2_kelvin.is_some() => {
            let wb = white_balance::from_adjustments(
                adjustments,
                as_shot_wb.unwrap_or_else(WhiteBalance::reference),
            );
            let bucket = ((wb.temperature / 100.0).round() as u32).clamp(15, 500) * 100;
            (bucket as f64, bucket)
        }
        _ => (6504.0, 0),
    };

    let cache_key = (profile_id.to_string(), kelvin_bucket);
    if let Some(lut) = DCP_GPU_LUT_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&cache_key)
        .cloned()
    {
        return Some(lut);
    }

    let lut = match &cached {
        CachedCameraProfile::Dcp(dcp) => bake_dcp_lut(dcp, temp_kelvin),
        CachedCameraProfile::Icc(icc) => bake_icc_lut(icc),
    }?;

    let mut lut_cache = DCP_GPU_LUT_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if lut_cache.len() >= 32 {
        lut_cache.clear();
    }
    lut_cache.insert(cache_key, Arc::clone(&lut));
    Some(lut)
}

/// Applies the user-selected DCP/ICC camera profile non-destructively when `cameraProfileId` is set.
pub fn apply_camera_profile_if_enabled<'a>(
    image: Cow<'a, DynamicImage>,
    adjustments: &Value,
) -> Cow<'a, DynamicImage> {
    let profile_id = adjustments["cameraProfileId"]
        .as_str()
        .unwrap_or("")
        .trim();
    if profile_id.is_empty() {
        return image;
    }

    let Some(profile) = load_cached_profile_by_id(profile_id) else {
        return image;
    };

    let mut rgb32f = image.to_rgb32f();
    match profile {
        CachedCameraProfile::Dcp(ref dcp) => apply_dcp_profile(&mut rgb32f, dcp),
        CachedCameraProfile::Icc(ref icc_bytes) => apply_icc_camera_profile(&mut rgb32f, icc_bytes),
    }
    Cow::Owned(DynamicImage::ImageRgb32F(rgb32f))
}

fn camera_profiles_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let dir = resolved_app_data_dir(app_handle)?.join("camera_profiles");
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    let _ = CAMERA_PROFILES_DIR.set(dir.clone());
    Ok(dir)
}

pub fn init_camera_profiles_dir(app_handle: &AppHandle) {
    let _ = camera_profiles_dir(app_handle);
}

fn scan_profiles_in_dir(dir: &Path) -> Vec<CameraProfileInfo> {
    let mut list = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return list;
    };
    let mut cache_updates = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        if ext != "dcp" && ext != "icc" && ext != "icm" {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let id = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        if id.is_empty() {
            continue;
        }
        if ext == "dcp" {
            if let Ok(dcp) = parse_dcp_bytes(&bytes) {
                list.push(CameraProfileInfo {
                    id: id.clone(),
                    name: dcp.name.clone(),
                    camera_model: dcp.camera_model.clone(),
                    format: "DCP".to_string(),
                    path: path.to_string_lossy().to_string(),
                    dual_illuminant: dcp.illuminant2_kelvin.is_some(),
                    has_hue_sat_map: dcp.hue_sat_dims.is_some(),
                    has_tone_curve: dcp.tone_curve.is_some(),
                });
                cache_updates.push((id, CachedCameraProfile::Dcp(dcp)));
            }
        } else if let Ok(icc) = ColorProfile::new_from_slice(&bytes) {
            if icc.color_space == DataColorSpace::Rgb {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("ICC Profile")
                    .to_string();
                list.push(CameraProfileInfo {
                    id: id.clone(),
                    name: stem,
                    camera_model: "ICC Input".to_string(),
                    format: "ICC".to_string(),
                    path: path.to_string_lossy().to_string(),
                    dual_illuminant: false,
                    has_hue_sat_map: false,
                    has_tone_curve: false,
                });
                cache_updates.push((id, CachedCameraProfile::Icc(bytes)));
            }
        }
    }

    if !cache_updates.is_empty() {
        let mut cache = CAMERA_PROFILE_CACHE
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for (id, item) in cache_updates {
            cache.insert(id, item);
        }
        DCP_GPU_LUT_CACHE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    list
}

#[tauri::command]
pub async fn list_camera_profiles(app_handle: AppHandle) -> Result<Vec<CameraProfileInfo>, String> {
    let dir = camera_profiles_dir(&app_handle)?;
    tokio::task::spawn_blocking(move || scan_profiles_in_dir(&dir))
        .await
        .map_err(|e| format!("Camera profile scan join error: {}", e))
}

#[tauri::command]
pub async fn import_camera_profile(
    source_path: String,
    app_handle: AppHandle,
) -> Result<CameraProfileInfo, String> {
    let dir = camera_profiles_dir(&app_handle)?;
    tokio::task::spawn_blocking(move || {
        let src = Path::new(&source_path);
        let ext = src
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        if ext != "dcp" && ext != "icc" && ext != "icm" {
            return Err("Only .dcp, .icc, and .icm camera profiles are supported".to_string());
        }
        let bytes = fs::read(src).map_err(|e| format!("Failed reading profile: {}", e))?;
        if ext == "dcp" {
            parse_dcp_bytes(&bytes)?;
        } else {
            let icc = ColorProfile::new_from_slice(&bytes)
                .map_err(|e| format!("Invalid ICC profile: {}", e))?;
            if icc.color_space != DataColorSpace::Rgb {
                return Err("Camera input ICC profile must be RGB colorspace".to_string());
            }
        }
        let fname = src
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| "Invalid profile file name".to_string())?;
        let dest = dir.join(fname);
        fs::write(&dest, &bytes).map_err(|e| format!("Failed saving profile: {}", e))?;
        let scanned = scan_profiles_in_dir(&dir);
        scanned
            .into_iter()
            .find(|p| p.id == fname)
            .ok_or_else(|| "Imported profile could not be indexed".to_string())
    })
    .await
    .map_err(|e| format!("Camera profile import join error: {}", e))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_le_u16(buf: &mut Vec<u8>, v: u16) {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    fn push_le_u32(buf: &mut Vec<u8>, v: u32) {
        buf.extend_from_slice(&v.to_le_bytes());
    }

    #[test]
    fn test_dcp_parser_reads_dual_illuminant_matrices_and_huesatmap() {
        // Construct a minimal valid little-endian DCP binary in RAM with:
        // UniqueCameraModel, CalibrationIlluminant1=17(StdA), CalibrationIlluminant2=21(D65),
        // ColorMatrix1, ColorMatrix2, ProfileHueSatMapDims=[2,2,1], ProfileHueSatMapData1 (4 entries)
        let mut buf = Vec::new();
        buf.extend_from_slice(b"II");
        push_le_u16(&mut buf, 42);
        push_le_u32(&mut buf, 8); // IFD0 offset

        let num_entries: u16 = 7;
        push_le_u16(&mut buf, num_entries);
        let entries_start = buf.len();
        buf.resize(entries_start + (num_entries as usize) * 12 + 4, 0);

        let write_entry = |idx: usize, tag: u16, ftype: u16, count: u32, payload: &[u8], buf: &mut Vec<u8>| {
            let off = entries_start + idx * 12;
            buf[off..off + 2].copy_from_slice(&tag.to_le_bytes());
            buf[off + 2..off + 4].copy_from_slice(&ftype.to_le_bytes());
            buf[off + 4..off + 8].copy_from_slice(&count.to_le_bytes());
            if payload.len() <= 4 {
                buf[off + 8..off + 8 + payload.len()].copy_from_slice(payload);
            } else {
                let data_off = buf.len() as u32;
                buf[off + 8..off + 12].copy_from_slice(&data_off.to_le_bytes());
                buf.extend_from_slice(payload);
            }
        };

        let make_srational_identity = |diag: i32| -> Vec<u8> {
            let mut p = Vec::new();
            for r in 0..3 {
                for c in 0..3 {
                    let num: i32 = if r == c { diag } else { 0 };
                    p.extend_from_slice(&num.to_le_bytes());
                    p.extend_from_slice(&1000i32.to_le_bytes());
                }
            }
            p
        };

        write_entry(0, TAG_UNIQUE_CAMERA_MODEL, 2, 9, b"TestCam1\0", &mut buf);
        write_entry(1, TAG_CALIBRATION_ILLUMINANT_1, 3, 1, &17u16.to_le_bytes(), &mut buf);
        write_entry(2, TAG_CALIBRATION_ILLUMINANT_2, 3, 1, &21u16.to_le_bytes(), &mut buf);
        write_entry(3, TAG_COLOR_MATRIX_1, 10, 9, &make_srational_identity(1200), &mut buf);
        write_entry(4, TAG_COLOR_MATRIX_2, 10, 9, &make_srational_identity(1000), &mut buf);

        let mut dims_payload = Vec::new();
        for v in [2u32, 2u32, 1u32] {
            dims_payload.extend_from_slice(&v.to_le_bytes());
        }
        write_entry(5, TAG_PROFILE_HUE_SAT_MAP_DIMS, 4, 3, &dims_payload, &mut buf);

        let mut hsm_payload = Vec::new();
        for i in 0..4 {
            for val in [i as f32 * 5.0, 1.15f32, 1.0f32] {
                hsm_payload.extend_from_slice(&val.to_le_bytes());
            }
        }
        write_entry(6, TAG_PROFILE_HUE_SAT_MAP_DATA_1, 11, 12, &hsm_payload, &mut buf);

        let dcp = parse_dcp_bytes(&buf).expect("Valid synthetic DCP parses");
        assert_eq!(dcp.camera_model, "TestCam1");
        assert!((dcp.illuminant1_kelvin - 2856.0).abs() < 1.0);
        assert_eq!(dcp.illuminant2_kelvin, Some(6504.0));
        assert!((dcp.color_matrix_1.unwrap()[0][0] - 1.2).abs() < 1e-4);
        assert!((dcp.color_matrix_2.unwrap()[0][0] - 1.0).abs() < 1e-4);
        assert_eq!(dcp.hue_sat_dims, Some([2, 2, 1]));
        let table = dcp.hue_sat_data_1.unwrap();
        assert_eq!(table.len(), 4);
        assert!((table[2][0] - 10.0).abs() < 1e-4);
        assert!((table[2][1] - 1.15).abs() < 1e-4);
    }

    #[test]
    fn test_dual_illuminant_mired_interpolation_endpoints_and_midpoint() {
        let w_a = interpolate_mired_weight(2856.0, 2856.0, 6504.0);
        let w_d65 = interpolate_mired_weight(6504.0, 2856.0, 6504.0);
        assert!((w_a - 0.0).abs() < 1e-6);
        assert!((w_d65 - 1.0).abs() < 1e-6);

        // Exact mired midpoint between 2856K (~350.14 MK^-1) and 6504K (~153.75 MK^-1) is ~251.95 MK^-1 (~3969 K)
        let mid_temp = 2_000_000.0 / (1_000_000.0 / 2856.0 + 1_000_000.0 / 6504.0);
        let w_mid = interpolate_mired_weight(mid_temp, 2856.0, 6504.0);
        assert!((w_mid - 0.5).abs() < 1e-5, "Expected 0.5 at mired midpoint, got {}", w_mid);

        let m1 = [[2.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let m2 = [[1.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 1.0]];
        let m_mid = interpolate_matrix3(&m1, &m2, w_mid as f32);
        assert!((m_mid[0][0] - 1.5).abs() < 1e-4);
        assert!((m_mid[1][1] - 1.5).abs() < 1e-4);
    }

    #[test]
    fn test_dcp_look_table_and_baseline_offset_bake_into_33_cube() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"II");
        push_le_u16(&mut buf, 42);
        push_le_u32(&mut buf, 8);

        let num_entries: u16 = 5;
        push_le_u16(&mut buf, num_entries);
        let entries_start = buf.len();
        buf.resize(entries_start + (num_entries as usize) * 12 + 4, 0);

        let write_entry = |idx: usize, tag: u16, ftype: u16, count: u32, payload: &[u8], buf: &mut Vec<u8>| {
            let off = entries_start + idx * 12;
            buf[off..off + 2].copy_from_slice(&tag.to_le_bytes());
            buf[off + 2..off + 4].copy_from_slice(&ftype.to_le_bytes());
            buf[off + 4..off + 8].copy_from_slice(&count.to_le_bytes());
            if payload.len() <= 4 {
                buf[off + 8..off + 8 + payload.len()].copy_from_slice(payload);
            } else {
                let data_off = buf.len() as u32;
                buf[off + 8..off + 12].copy_from_slice(&data_off.to_le_bytes());
                buf.extend_from_slice(payload);
            }
        };

        let mut look_dims = Vec::new();
        for v in [2u32, 2u32, 1u32] {
            look_dims.extend_from_slice(&v.to_le_bytes());
        }
        write_entry(0, TAG_PROFILE_LOOK_TABLE_DIMS, 4, 3, &look_dims, &mut buf);

        let mut look_data = Vec::new();
        for _ in 0..4 {
            for v in [8.0f32, 1.25f32, 1.0f32] {
                look_data.extend_from_slice(&v.to_le_bytes());
            }
        }
        write_entry(1, TAG_PROFILE_LOOK_TABLE_DATA, 11, 12, &look_data, &mut buf);
        write_entry(2, TAG_PROFILE_LOOK_TABLE_ENCODING, 4, 1, &1u32.to_le_bytes(), &mut buf);

        let mut baseline_payload = Vec::new();
        baseline_payload.extend_from_slice(&1i32.to_le_bytes());
        baseline_payload.extend_from_slice(&2i32.to_le_bytes()); // +0.5 EV
        write_entry(3, TAG_BASELINE_EXPOSURE_OFFSET, 10, 1, &baseline_payload, &mut buf);
        write_entry(4, TAG_UNIQUE_CAMERA_MODEL, 2, 7, b"CamX1\0\0", &mut buf);

        let dcp = parse_dcp_bytes(&buf).expect("DCP with LookTable & BaselineExposureOffset");
        assert_eq!(dcp.look_table_dims, Some([2, 2, 1]));
        assert_eq!(dcp.look_table_encoding, 1);
        assert!((dcp.baseline_exposure_offset - 0.5).abs() < 1e-5);

        let lut = bake_dcp_lut(&dcp, 5500.0).expect("33^3 LUT baked");
        assert_eq!(lut.size, 33);
        assert_eq!(lut.data.len(), 33 * 33 * 33 * 3);
        assert!(lut.data.iter().all(|v| v.is_finite() && *v >= 0.0));
        // Mid-gray index (16, 16, 16) corresponds to u=0.5 => linear 1.0, scaled by 2^0.5 (~1.414)
        let idx = ((16 * 33 + 16) * 33 + 16) * 3;
        assert!(lut.data[idx] > 1.2, "Expected +0.5 EV boost, got {}", lut.data[idx]);
    }
}
