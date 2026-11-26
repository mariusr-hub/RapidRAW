use crate::adjustments::lens_correction::LensProfile;
use crate::library::file_ops::read_file_mapped;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::UNIX_EPOCH;

const TIFF_TAG_SUBIFDS: u16 = 330;
const TIFF_TAG_EXIF_IFD: u16 = 34665;
const TIFF_TAG_OPCODE_LIST3: u16 = 51009;
const OPCODE_WARP_RECTILINEAR: u32 = 1;
const OPCODE_FIX_VIGNETTE_RADIAL: u32 = 3;
const MAX_IFDS_VISITED: usize = 64;
const MAX_CACHE_ENTRIES: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct CacheKey {
    path: String,
    mtime_ms: u128,
    file_size: u64,
}

static OPCODE_LENS_CACHE: LazyLock<Mutex<HashMap<CacheKey, Option<LensProfile>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Copy)]
enum Endian {
    Little,
    Big,
}

impl Endian {
    #[inline]
    fn u16(self, bytes: &[u8]) -> u16 {
        let arr: [u8; 2] = [bytes[0], bytes[1]];
        match self {
            Self::Little => u16::from_le_bytes(arr),
            Self::Big => u16::from_be_bytes(arr),
        }
    }

    #[inline]
    fn u32(self, bytes: &[u8]) -> u32 {
        let arr: [u8; 4] = [bytes[0], bytes[1], bytes[2], bytes[3]];
        match self {
            Self::Little => u32::from_le_bytes(arr),
            Self::Big => u32::from_be_bytes(arr),
        }
    }

    #[inline]
    fn u64(self, bytes: &[u8]) -> u64 {
        let arr: [u8; 8] = [
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ];
        match self {
            Self::Little => u64::from_le_bytes(arr),
            Self::Big => u64::from_be_bytes(arr),
        }
    }
}

#[inline]
fn be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

#[inline]
fn be_f64(bytes: &[u8], offset: usize) -> Option<f64> {
    let slice = bytes.get(offset..offset + 8)?;
    let val = f64::from_be_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]);
    val.is_finite().then_some(val)
}

fn tiff_type_size(ty: u16) -> Option<u64> {
    match ty {
        1 | 2 | 6 | 7 => Some(1),      // BYTE, ASCII, SBYTE, UNDEFINED
        3 | 8 | 13 => Some(2),         // SHORT, SSHORT
        4 | 9 | 11 | 18 => Some(4),    // LONG, SLONG, FLOAT, IFD8/IFD? (classic IFD is 13, 18 is IFD8)
        5 | 10 | 12 | 16 | 17 => Some(8), // RATIONAL, SRATIONAL, DOUBLE, LONG8, SLONG8
        _ => None,
    }
}

fn read_ifd_offsets(data: &[u8], endian: Endian, bigtiff: bool, ty: u16, count: u64, val_bytes: &[u8]) -> Vec<u64> {
    let mut out = Vec::new();
    let elem_size: u64 = match ty {
        4 | 13 => 4,
        16 | 18 => 8,
        _ => return out,
    };
    let inline_limit: usize = if bigtiff { 8 } else { 4 };
    let total_bytes = (count as usize).saturating_mul(elem_size as usize);
    let raw: &[u8] = if total_bytes <= inline_limit {
        val_bytes.get(..total_bytes).unwrap_or(&[])
    } else {
        let offset = if bigtiff {
            endian.u64(val_bytes) as usize
        } else {
            endian.u32(val_bytes) as usize
        };
        data.get(offset..offset.saturating_add(total_bytes))
            .unwrap_or(&[])
    };
    for chunk in raw.chunks_exact(elem_size as usize) {
        let off = if elem_size == 8 {
            endian.u64(chunk)
        } else {
            endian.u32(chunk) as u64
        };
        if off > 0 {
            out.push(off);
        }
    }
    out
}

fn extract_opcode_list3_bytes(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 8 {
        return None;
    }
    let endian = match &data[0..2] {
        b"II" => Endian::Little,
        b"MM" => Endian::Big,
        _ => return None,
    };
    let version = endian.u16(&data[2..4]);
    let bigtiff = match version {
        42 => false,
        43 => true,
        _ => return None,
    };

    let first_ifd = if bigtiff {
        if data.len() < 16 {
            return None;
        }
        endian.u64(&data[8..16])
    } else {
        endian.u32(&data[4..8]) as u64
    };
    if first_ifd == 0 {
        return None;
    }

    let mut queue = vec![first_ifd];
    let mut visited = Vec::with_capacity(16);

    while let Some(ifd_off) = queue.pop() {
        if ifd_off == 0 || visited.contains(&ifd_off) || visited.len() >= MAX_IFDS_VISITED {
            continue;
        }
        visited.push(ifd_off);
        let pos = usize::try_from(ifd_off).ok()?;

        let (entry_count, entries_start, entry_size, inline_limit) = if bigtiff {
            let count = endian.u64(data.get(pos..pos.checked_add(8)?)?) as usize;
            (count.min(4096), pos.checked_add(8)?, 20usize, 8usize)
        } else {
            let count = endian.u16(data.get(pos..pos.checked_add(2)?)?) as usize;
            (count.min(4096), pos.checked_add(2)?, 12usize, 4usize)
        };

        for idx in 0..entry_count {
            let e = entries_start.checked_add(idx.checked_mul(entry_size)?)?;
            let entry = data.get(e..e.checked_add(entry_size)?)?;
            let tag = endian.u16(&entry[0..2]);
            let ty = endian.u16(&entry[2..4]);
            let (count, val_bytes) = if bigtiff {
                (endian.u64(&entry[4..12]), &entry[12..20])
            } else {
                (endian.u32(&entry[4..8]) as u64, &entry[8..12])
            };

            if tag == TIFF_TAG_OPCODE_LIST3 {
                let unit = tiff_type_size(ty).unwrap_or(1);
                let byte_len = usize::try_from(count.checked_mul(unit)?).ok()?;
                if byte_len < 4 {
                    continue;
                }
                let payload = if byte_len <= inline_limit {
                    val_bytes.get(..byte_len)?
                } else {
                    let off = if bigtiff {
                        endian.u64(val_bytes) as usize
                    } else {
                        endian.u32(val_bytes) as usize
                    };
                    data.get(off..off.checked_add(byte_len)?)?
                };
                return Some(payload.to_vec());
            } else if tag == TIFF_TAG_SUBIFDS || tag == TIFF_TAG_EXIF_IFD {
                for child in read_ifd_offsets(data, endian, bigtiff, ty, count, val_bytes) {
                    if !visited.contains(&child) {
                        queue.push(child);
                    }
                }
            }
        }

        let next_pos = entries_start.checked_add(entry_count.checked_mul(entry_size)?)?;
        let next_ifd = if bigtiff {
            data.get(next_pos..next_pos + 8)
                .map(|b| endian.u64(b))
                .unwrap_or(0)
        } else {
            data.get(next_pos..next_pos + 4)
                .map(|b| endian.u32(b) as u64)
                .unwrap_or(0)
        };
        if next_ifd != 0 && !visited.contains(&next_ifd) {
            queue.push(next_ifd);
        }
    }

    None
}

#[derive(Clone, Copy)]
struct WarpPlane {
    kr: [f64; 4],
}

fn eval_warp_radius_ratio(p: &WarpPlane, r: f64) -> f64 {
    let r2 = r * r;
    p.kr[0] + r2 * (p.kr[1] + r2 * (p.kr[2] + r2 * p.kr[3]))
}

fn fit_vignette_divider(k: &[f64; 5]) -> Option<[f64; 3]> {
    if k.iter().all(|v| v.abs() <= 1e-8) {
        return None;
    }
    let eval_f = |u: f64| -> Option<f64> {
        let g = 1.0 + u * (k[0] + u * (k[1] + u * (k[2] + u * (k[3] + u * k[4]))));
        if !g.is_finite() || g <= 1e-4 {
            return None;
        }
        let target = (1.0 / g) - 1.0;
        Some(target / u)
    };

    // Quadratic Lagrange interpolation of ((1 / gain(r)) - 1) / u = v1 + v2*u + v3*u^2 at u = r^2.
    const U: [f64; 3] = [0.25, 0.65, 1.0];
    let y = [eval_f(U[0])?, eval_f(U[1])?, eval_f(U[2])?];

    let mut coeffs = [0.0f64; 3]; // [v1, v2, v3]
    for i in 0..3 {
        let j = (i + 1) % 3;
        let m = (i + 2) % 3;
        let denom = (U[i] - U[j]) * (U[i] - U[m]);
        let weight = y[i] / denom;
        coeffs[0] += weight * (U[j] * U[m]);
        coeffs[1] += weight * (-(U[j] + U[m]));
        coeffs[2] += weight * 1.0;
    }

    if coeffs.iter().any(|c| !c.is_finite()) {
        return None;
    }
    Some(coeffs)
}

pub fn parse_opcode_list3(bytes: &[u8]) -> Option<LensProfile> {
    let count = be_u32(bytes, 0)? as usize;
    let mut cursor = 4usize;

    let mut distortion: Option<[f64; 5]> = None;
    let mut tca: Option<[f64; 6]> = None;
    let mut vignetting: Option<[f64; 3]> = None;

    for _ in 0..count.min(64) {
        let id = be_u32(bytes, cursor)?;
        let _version = be_u32(bytes, cursor + 4)?;
        let _flags = be_u32(bytes, cursor + 8)?;
        let param_bytes = be_u32(bytes, cursor + 12)? as usize;
        cursor = cursor.checked_add(16)?;
        let payload = bytes.get(cursor..cursor.checked_add(param_bytes)?)?;
        cursor += param_bytes;

        match id {
            OPCODE_WARP_RECTILINEAR => {
                let num_planes = be_u32(payload, 0)? as usize;
                if num_planes != 1 && num_planes != 3 {
                    continue;
                }
                let needed = 4 + num_planes * 48;
                if payload.len() < needed {
                    continue;
                }
                let mut planes = Vec::with_capacity(num_planes);
                for p in 0..num_planes {
                    let base = 4 + p * 48;
                    planes.push(WarpPlane {
                        kr: [
                            be_f64(payload, base)?,
                            be_f64(payload, base + 8)?,
                            be_f64(payload, base + 16)?,
                            be_f64(payload, base + 24)?,
                        ],
                    });
                }

                let g_idx = if num_planes == 3 { 1 } else { 0 };
                let g = &planes[g_idx];
                if g.kr[0].abs() > 1e-6 {
                    let kr0 = g.kr[0];
                    // Exact match at r^2 = 0.5 and r^2 = 1.0 when kr3 != 0; exact identity when kr3 == 0.
                    let a2 = (g.kr[1] - 0.5 * g.kr[3]) / kr0;
                    let a4 = (g.kr[2] + 1.5 * g.kr[3]) / kr0;
                    if a2.abs() > 1e-8 || a4.abs() > 1e-8 || (kr0 - 1.0).abs() > 1e-6 {
                        distortion = Some([1.0, 0.0, a2, 0.0, a4]);
                    }

                    if num_planes == 3 {
                        let ratio = |plane: &WarpPlane, r: f64| -> f64 {
                            let denom = eval_warp_radius_ratio(g, r);
                            if denom.abs() > 1e-6 {
                                eval_warp_radius_ratio(plane, r) / denom
                            } else {
                                1.0
                            }
                        };
                        let fit_plane = |plane: &WarpPlane| -> [f64; 3] {
                            let f0 = ratio(plane, 0.0);
                            let f5 = ratio(plane, 0.5);
                            let f1 = ratio(plane, 1.0);
                            let c = 4.0 * f5 - f1 - 3.0 * f0;
                            let b = 2.0 * f1 - 4.0 * f5 + 2.0 * f0;
                            [f0, c, b]
                        };
                        let [vr, cr, br] = fit_plane(&planes[0]);
                        let [vb, cb, bb] = fit_plane(&planes[2]);
                        let has_tca = [vr - 1.0, vb - 1.0, cr, cb, br, bb]
                            .iter()
                            .any(|x| x.abs() > 1e-6);
                        if has_tca {
                            tca = Some([vr, vb, cr, cb, br, bb]);
                        }
                    }
                }
            }
            OPCODE_FIX_VIGNETTE_RADIAL => {
                if payload.len() < 40 {
                    continue;
                }
                let k = [
                    be_f64(payload, 0)?,
                    be_f64(payload, 8)?,
                    be_f64(payload, 16)?,
                    be_f64(payload, 24)?,
                    be_f64(payload, 32)?,
                ];
                if let Some(v) = fit_vignette_divider(&k) {
                    vignetting = Some(v);
                }
            }
            _ => {}
        }
    }

    if distortion.is_none() && tca.is_none() && vignetting.is_none() {
        return None;
    }

    Some(LensProfile {
        distortion,
        tca,
        vignetting,
        distortion_scale: 1.0,
        vignetting_scale: 1.0,
    })
}

pub(crate) fn resolve_dng_opcode_lens(source_path: &str) -> Option<(String, String, LensProfile)> {
    let path = Path::new(source_path);
    let meta = std::fs::metadata(path).ok()?;
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let key = CacheKey {
        path: source_path.to_string(),
        mtime_ms,
        file_size: meta.len(),
    };

    if let Some(cached) = OPCODE_LENS_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .cloned()
    {
        return cached.map(|profile| ("DNG Embedded".to_string(), "OpcodeList3".to_string(), profile));
    }

    let bytes = read_file_mapped(path).ok()?;
    let profile = extract_opcode_list3_bytes(&bytes).and_then(|op| parse_opcode_list3(&op));

    let mut cache = OPCODE_LENS_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if cache.len() >= MAX_CACHE_ENTRIES {
        cache.clear();
    }
    cache.insert(key, profile.clone());

    profile.map(|p| ("DNG Embedded".to_string(), "OpcodeList3".to_string(), p))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_synthetic_dng(little_endian: bool, opcode_payload: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        let push_u16 = |b: &mut Vec<u8>, v: u16| {
            b.extend_from_slice(&if little_endian {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            });
        };
        let push_u32 = |b: &mut Vec<u8>, v: u32| {
            b.extend_from_slice(&if little_endian {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            });
        };

        buf.extend_from_slice(if little_endian { b"II" } else { b"MM" });
        push_u16(&mut buf, 42);
        push_u32(&mut buf, 8); // IFD0 offset

        let payload_offset = 8u32 + 2 + 12 + 4;
        push_u16(&mut buf, 1); // 1 entry
        push_u16(&mut buf, TIFF_TAG_OPCODE_LIST3);
        push_u16(&mut buf, 7); // UNDEFINED
        push_u32(&mut buf, opcode_payload.len() as u32);
        push_u32(&mut buf, payload_offset);
        push_u32(&mut buf, 0); // next IFD = 0
        buf.extend_from_slice(opcode_payload);
        buf
    }

    fn build_warp_and_vignette_opcodes(num_planes: u32) -> Vec<u8> {
        let mut op = Vec::new();
        op.extend_from_slice(&2u32.to_be_bytes()); // 2 opcodes

        // Opcode 1: WarpRectilinear
        let warp_params_len = 4u32 + num_planes * 48 + 16;
        op.extend_from_slice(&OPCODE_WARP_RECTILINEAR.to_be_bytes());
        op.extend_from_slice(&0x01030000u32.to_be_bytes());
        op.extend_from_slice(&1u32.to_be_bytes()); // optional bit
        op.extend_from_slice(&warp_params_len.to_be_bytes());
        op.extend_from_slice(&num_planes.to_be_bytes());
        for plane in 0..num_planes {
            let kr0 = match (num_planes, plane) {
                (3, 0) => 1.002_f64,
                (3, 2) => 0.998_f64,
                _ => 1.0_f64,
            };
            let kr1 = -0.12_f64;
            let kr2 = 0.03_f64;
            let kr3 = 0.0_f64;
            let kt0 = 0.0_f64;
            let kt1 = 0.0_f64;
            for v in [kr0, kr1, kr2, kr3, kt0, kt1] {
                op.extend_from_slice(&v.to_be_bytes());
            }
        }
        op.extend_from_slice(&0.5_f64.to_be_bytes());
        op.extend_from_slice(&0.5_f64.to_be_bytes());

        // Opcode 2: FixVignetteRadial
        op.extend_from_slice(&OPCODE_FIX_VIGNETTE_RADIAL.to_be_bytes());
        op.extend_from_slice(&0x01030000u32.to_be_bytes());
        op.extend_from_slice(&0u32.to_be_bytes());
        op.extend_from_slice(&56u32.to_be_bytes());
        for v in [0.45_f64, 0.12_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.5_f64, 0.5_f64] {
            op.extend_from_slice(&v.to_be_bytes());
        }
        op
    }

    #[test]
    fn parses_little_endian_dng_with_3_plane_warp_and_vignette() {
        let opcodes = build_warp_and_vignette_opcodes(3);
        let dng = build_synthetic_dng(true, &opcodes);
        let raw_opcodes = extract_opcode_list3_bytes(&dng).expect("OpcodeList3 tag present");
        let profile = parse_opcode_list3(&raw_opcodes).expect("parsed lens profile");

        let dist = profile.distortion.expect("distortion");
        assert!((dist[0] - 1.0).abs() < 1e-9);
        assert!((dist[2] - (-0.12)).abs() < 1e-9);
        assert!((dist[4] - 0.03).abs() < 1e-9);

        let tca = profile.tca.expect("tca");
        assert!((tca[0] - 1.002).abs() < 1e-6);
        assert!((tca[1] - 0.998).abs() < 1e-6);

        let vig = profile.vignetting.expect("vignetting");
        // Verify 1 / (1 + v1*u + v2*u^2 + v3*u^3) matches gain(u) = 1 + 0.45*u + 0.12*u^2 at corners
        let u = 1.0_f64;
        let expected_gain = 1.0 + 0.45 * u + 0.12 * u * u;
        let eval_gain = 1.0 / (1.0 + u * (vig[0] + u * (vig[1] + u * vig[2])));
        assert!((eval_gain - expected_gain).abs() < 1e-3);
    }

    #[test]
    fn parses_big_endian_dng_with_1_plane_warp() {
        let opcodes = build_warp_and_vignette_opcodes(1);
        let dng = build_synthetic_dng(false, &opcodes);
        let raw_opcodes = extract_opcode_list3_bytes(&dng).expect("OpcodeList3 tag present");
        let profile = parse_opcode_list3(&raw_opcodes).expect("parsed lens profile");

        assert!(profile.distortion.is_some());
        assert!(profile.tca.is_none());
        assert!(profile.vignetting.is_some());
    }

    #[test]
    fn returns_none_for_invalid_bytes() {
        assert!(extract_opcode_list3_bytes(b"NOT_A_DNG").is_none());
        assert!(parse_opcode_list3(&[0, 0, 0, 1, 0, 0]).is_none());
    }
}
