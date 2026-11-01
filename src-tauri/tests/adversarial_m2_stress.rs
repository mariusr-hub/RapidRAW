//! Adversarial empirical challenge and stress harness for Milestone 2 (R2).
//! Tests:
//! 1. `is_linear_dng_probe` robustness on corrupt buffers, boundaries, loops, non-TIFF, Bayer vs Linear DNG.
//! 2. Big-Endian (MM) and Little-Endian (II) Linear DNG detection.
//! 3. CFA sensor conditioning guards.
//! 4. End-to-end synthetic 32-bit float Linear DNG encoding and decoding fidelity.
//! 5. `SafeFileBuffer` dereference, slicing, and drive detection logic.

use std::io::Write;
use std::path::Path;
use image::{Rgb, Rgb32FImage};

use rapidraw_lib::app_settings::AppSettings;
use rapidraw_lib::dng_encoder::{encode_linear_dng, DngExportMetadata};
use rapidraw_lib::image_loader::{
    is_linear_dng_probe, is_removable_or_remote_drive, load_base_image_from_bytes,
    load_linear_dng, read_file_mapped, read_file_safe,
};
use rapidraw_lib::raw_processing::{develop_raw_image, CfaExt};

// Helper: build a minimal valid TIFF/DNG structure in memory
fn build_synthetic_tiff(
    little_endian: bool,
    ifds: &[(u32, Vec<(u16, u16, u32, u32)>, u32)], // (ifd_offset, [(tag, type, count, value)], next_ifd_offset)
    total_size: usize,
) -> Vec<u8> {
    let mut buf = vec![0u8; total_size.max(64)];

    // Header
    if little_endian {
        buf[0..4].copy_from_slice(&[0x49, 0x49, 0x2A, 0x00]);
        let ifd0 = ifds.first().map(|x| x.0).unwrap_or(8);
        buf[4..8].copy_from_slice(&(ifd0 as u32).to_le_bytes());
    } else {
        buf[0..4].copy_from_slice(&[0x4D, 0x4D, 0x00, 0x2A]);
        let ifd0 = ifds.first().map(|x| x.0).unwrap_or(8);
        buf[4..8].copy_from_slice(&(ifd0 as u32).to_be_bytes());
    }

    for (ifd_off, entries, next_off) in ifds {
        let off = *ifd_off as usize;
        if off + 2 > buf.len() {
            continue;
        }

        let count = entries.len() as u16;
        if little_endian {
            buf[off..off + 2].copy_from_slice(&count.to_le_bytes());
        } else {
            buf[off..off + 2].copy_from_slice(&count.to_be_bytes());
        }

        for (i, &(tag, typ, cnt, val)) in entries.iter().enumerate() {
            let e_off = off + 2 + i * 12;
            if e_off + 12 > buf.len() {
                break;
            }
            if little_endian {
                buf[e_off..e_off + 2].copy_from_slice(&tag.to_le_bytes());
                buf[e_off + 2..e_off + 4].copy_from_slice(&typ.to_le_bytes());
                buf[e_off + 4..e_off + 8].copy_from_slice(&cnt.to_le_bytes());
                buf[e_off + 8..e_off + 12].copy_from_slice(&val.to_le_bytes());
            } else {
                buf[e_off..e_off + 2].copy_from_slice(&tag.to_be_bytes());
                buf[e_off + 2..e_off + 4].copy_from_slice(&typ.to_be_bytes());
                buf[e_off + 4..e_off + 8].copy_from_slice(&cnt.to_be_bytes());
                if typ == 3 && cnt == 1 {
                    buf[e_off + 8..e_off + 10].copy_from_slice(&(val as u16).to_be_bytes());
                } else {
                    buf[e_off + 8..e_off + 12].copy_from_slice(&val.to_be_bytes());
                }
            }
        }

        let next_pos = off + 2 + entries.len() * 12;
        if next_pos + 4 <= buf.len() {
            if little_endian {
                buf[next_pos..next_pos + 4].copy_from_slice(&(*next_off).to_le_bytes());
            } else {
                buf[next_pos..next_pos + 4].copy_from_slice(&(*next_off).to_be_bytes());
            }
        }
    }

    buf
}

#[test]
fn test_corrupted_buffers_do_not_panic() {
    // 1. Empty buffer
    assert!(!is_linear_dng_probe(&[]));

    // 2. Truncated buffers (< 16 bytes)
    for len in 1..16 {
        let truncated = vec![0x49; len];
        assert!(!is_linear_dng_probe(&truncated), "Failed on len {}", len);
    }

    // 3. Random noise / non-TIFF formats
    let jpeg_header = b"\xFF\xD8\xFF\xE1\x00\x18Exif\x00\x00II*\x00\x08\x00\x00\x00";
    assert!(!is_linear_dng_probe(jpeg_header));

    let png_header = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x10";
    assert!(!is_linear_dng_probe(png_header));

    let zeroes = vec![0u8; 1024];
    assert!(!is_linear_dng_probe(&zeroes));

    let ffs = vec![0xFFu8; 1024];
    assert!(!is_linear_dng_probe(&ffs));

    // 4. Valid TIFF header, but IFD offset points outside buffer
    let mut bad_ifd_off = vec![0x49, 0x49, 0x2A, 0x00, 0x00, 0x10, 0x00, 0x00]; // offset 4096
    bad_ifd_off.resize(64, 0);
    assert!(!is_linear_dng_probe(&bad_ifd_off));

    // 5. Valid TIFF header, IFD offset inside, but truncated before entry count
    let cut_before_count = vec![0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, 0x05]; // len 9, needs at least 10 for u16 count
    assert!(!is_linear_dng_probe(&cut_before_count));

    // 6. Valid header, entry count = 100, but buffer truncated mid-entry
    let mut cut_mid_entry = vec![0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, 0x64, 0x00]; // count = 100
    cut_mid_entry.resize(20, 0); // only 10 bytes for entries, each is 12 bytes
    assert!(!is_linear_dng_probe(&cut_mid_entry));
}

#[test]
fn test_ifd_cycle_and_chain_termination() {
    // 1. Direct 2-IFD cycle: IFD0 at 8 points to IFD1 at 32; IFD1 at 32 points back to IFD0 at 8!
    // Must terminate cleanly via visited array without infinite loop!
    let cyclic_dng = build_synthetic_tiff(
        true,
        &[
            (8, vec![], 32), // IFD0 -> IFD1
            (32, vec![], 8), // IFD1 -> IFD0 (cycle!)
        ],
        64,
    );
    assert!(!is_linear_dng_probe(&cyclic_dng), "Cyclic IFDs must terminate cleanly");

    // 2. SubIFD self-loop: IFD0 points to itself via SubIFD (tag 0x014A)
    let subifd_self_loop = build_synthetic_tiff(
        true,
        &[
            (8, vec![(0x014A, 4, 1, 8)], 0), // SubIFD points to 8 (self)
        ],
        64,
    );
    assert!(!is_linear_dng_probe(&subifd_self_loop), "Self-loop SubIFD must terminate");

    // 3. Deep IFD chain exceeding queue capacity (16 IFDs)
    // Create chain of 20 IFDs: 8 -> 24 -> 40 -> ... -> 312
    let mut ifd_chain = Vec::new();
    for i in 0..20 {
        let cur = 8 + i * 24;
        let next = if i < 19 { 8 + (i + 1) * 24 } else { 0 };
        ifd_chain.push((cur as u32, vec![], next as u32));
    }
    let chain_buf = build_synthetic_tiff(true, &ifd_chain, 600);
    assert!(!is_linear_dng_probe(&chain_buf), "Long IFD chain must terminate safely without overflow");
}

#[test]
fn test_bayer_raw_vs_linear_dng_classification() {
    // Case 1: Standard Bayer Raw DNG
    // PhotometricInterpretation = 32803 (CFA), SamplesPerPixel = 1
    let bayer_dng = build_synthetic_tiff(
        true,
        &[(
            8,
            vec![
                (0x0106, 3, 1, 32803), // PhotometricInterpretation = 32803 (CFA)
                (0x0115, 3, 1, 1),     // SamplesPerPixel = 1
            ],
            0,
        )],
        128,
    );
    assert!(
        !is_linear_dng_probe(&bayer_dng),
        "Standard Bayer CFA DNG must return FALSE"
    );

    // Case 2: Multi-IFD DNG with RGB thumbnail and CFA raw image
    // IFD0: SubfileType = 1 (thumbnail), Photometric = 2 (RGB), Samples = 3
    // SubIFD0 (at 60): SubfileType = 0 (main), Photometric = 32803 (CFA), Samples = 1
    let camera_raw_dng = build_synthetic_tiff(
        true,
        &[
            (
                8,
                vec![
                    (0x00FE, 4, 1, 1),     // NewSubfileType = 1 (reduced resolution thumbnail)
                    (0x0106, 3, 1, 2),     // Photometric = 2 (RGB)
                    (0x0115, 3, 1, 3),     // Samples = 3
                    (0x014A, 4, 1, 60),    // SubIFDs -> offset 60
                ],
                0,
            ),
            (
                60,
                vec![
                    (0x00FE, 4, 1, 0),     // NewSubfileType = 0 (main raw)
                    (0x0106, 3, 1, 32803), // Photometric = 32803 (CFA)
                    (0x0115, 3, 1, 1),     // Samples = 1
                ],
                0,
            ),
        ],
        256,
    );
    assert!(
        !is_linear_dng_probe(&camera_raw_dng),
        "Camera RAW DNG with RGB thumb and CFA main sensor MUST return FALSE"
    );

    // Case 3: 3-channel Linear DNG (Little Endian, DNGVersion + Photometric = 34892 LinearRaw)
    let linear_dng_le = build_synthetic_tiff(
        true,
        &[(
            8,
            vec![
                (0x0106, 3, 1, 34892),      // LinearRaw
                (0x0115, 3, 1, 3),          // 3 channels
                (0xC612, 1, 4, 0x00000401), // DNGVersion 1.4.0.0
            ],
            0,
        )],
        128,
    );
    assert!(
        is_linear_dng_probe(&linear_dng_le),
        "3-channel Linear DNG (LE) must return TRUE"
    );

    // Case 4: 3-channel Linear DNG (Big Endian, DNGVersion + Photometric = 34892 LinearRaw)
    let linear_dng_be = build_synthetic_tiff(
        false,
        &[(
            8,
            vec![
                (0x0106, 3, 1, 34892),      // LinearRaw
                (0x0115, 3, 1, 3),          // 3 channels
                (0xC612, 1, 4, 0x01040000), // DNGVersion 1.4.0.0
            ],
            0,
        )],
        128,
    );
    assert!(
        is_linear_dng_probe(&linear_dng_be),
        "3-channel Linear DNG (BE) must return TRUE"
    );

    // Case 5: Linear DNG stored in SubIFD
    // IFD0: Reduced-resolution thumbnail (SubfileType = 1)
    // SubIFD (offset 60): Main Linear DNG (SubfileType = 0, Photometric = 34892, Samples = 3)
    let linear_subifd_dng = build_synthetic_tiff(
        true,
        &[
            (
                8,
                vec![
                    (0x00FE, 4, 1, 1),          // thumbnail
                    (0x014A, 4, 1, 60),         // SubIFD at 60
                    (0xC612, 1, 4, 0x00000401), // DNGVersion 1.4.0.0
                ],
                0,
            ),
            (
                60,
                vec![
                    (0x00FE, 4, 1, 0),     // main
                    (0x0106, 3, 1, 34892), // LinearRaw
                    (0x0115, 3, 1, 3),     // 3 channels
                ],
                0,
            ),
        ],
        256,
    );
    assert!(
        is_linear_dng_probe(&linear_subifd_dng),
        "Linear DNG inside SubIFD must return TRUE"
    );

    // Case 6: Photometric = 2 (RGB) with Samples = 3 and SubfileType = 0 (main)
    let linear_rgb_main = build_synthetic_tiff(
        true,
        &[(
            8,
            vec![
                (0x00FE, 4, 1, 0), // Main
                (0x0106, 3, 1, 2), // RGB
                (0x0115, 3, 1, 3), // Samples = 3
            ],
            0,
        )],
        128,
    );
    assert!(
        !is_linear_dng_probe(&linear_rgb_main),
        "RGB Photometric=2 (e.g. Canon CR2 IFD0 JPEG preview) must NEVER be misclassified as LinearRaw DNG"
    );

    // Case 7: Photometric = 2 (RGB), but Samples = 1 (illegal / grayscale)
    let rgb_one_sample = build_synthetic_tiff(
        true,
        &[(
            8,
            vec![
                (0x0106, 3, 1, 2),
                (0x0115, 3, 1, 1),
            ],
            0,
        )],
        128,
    );
    assert!(
        !is_linear_dng_probe(&rgb_one_sample),
        "RGB with 1 sample must return FALSE"
    );
}

#[test]
fn test_adversarial_linear_dng_e2e_decode_and_fidelity() {
    let (w, h) = (48u32, 48u32);
    let mut orig_img = Rgb32FImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let r = (x as f32) / (w as f32);
            let g = (y as f32) / (h as f32);
            let b = 0.42f32;
            orig_img.put_pixel(x, y, Rgb([r, g, b]));
        }
    }

    let meta = DngExportMetadata::default();
    let dng_bytes = encode_linear_dng(&orig_img, Some(&meta)).expect("Encode linear DNG");
    assert!(!dng_bytes.is_empty());

    // 1. Probe
    assert!(
        is_linear_dng_probe(&dng_bytes),
        "Probe must return true for synthetic Linear DNG"
    );

    // 2. Load via load_linear_dng
    let loaded = load_linear_dng(&dng_bytes, None).expect("load_linear_dng failed");
    assert_eq!(loaded.width(), w);
    assert_eq!(loaded.height(), h);

    // Check pixel precision
    let loaded_f32 = loaded.to_rgb32f();
    for y in [0, h / 2, h - 1] {
        for x in [0, w / 2, w - 1] {
            let orig = orig_img.get_pixel(x, y);
            let dec = loaded_f32.get_pixel(x, y);
            for c in 0..3 {
                assert!(
                    (orig[c] - dec[c]).abs() < 1e-4,
                    "Pixel mismatch at ({},{}) ch {}: orig {} vs dec {}",
                    x, y, c, orig[c], dec[c]
                );
            }
        }
    }

    // 3. Load via develop_raw_image (which routes through Linear DNG probe)
    let dev_img = develop_raw_image(&dng_bytes, false, 2.5, "calibrated".to_string(), None)
        .expect("develop_raw_image must route through TIFF reader without panic");
    assert_eq!(dev_img.width(), w);
    assert_eq!(dev_img.height(), h);

    // 4. Load via load_base_image_from_bytes
    let settings = AppSettings::default();
    let base_img = load_base_image_from_bytes(&dng_bytes, "test_linear.dng", false, &settings, None)
        .expect("load_base_image_from_bytes must succeed");
    assert_eq!(base_img.width(), w);
    assert_eq!(base_img.height(), h);

    // 5. Truncated DNG returns error gracefully, never panics
    let truncated = &dng_bytes[..dng_bytes.len() / 3];
    assert!(load_linear_dng(truncated, None).is_err());
}

#[test]
fn test_cfa_ext_and_sensor_conditioning_guard() {
    let bayer_cfa = rawler::cfa::CFA::new("GRBG");
    assert!(bayer_cfa.is_bayer(), "2x2 CFA must be recognized as Bayer");
    assert_eq!(bayer_cfa.width, 2);
    assert_eq!(bayer_cfa.height, 2);

    let mut xtrans_cfa = rawler::cfa::CFA::new("GRBG");
    xtrans_cfa.width = 6;
    xtrans_cfa.height = 6;
    assert!(!xtrans_cfa.is_bayer(), "6x6 Fuji X-Trans CFA must NOT be recognized as Bayer");

    let mut single_cfa = rawler::cfa::CFA::new("GRBG");
    single_cfa.width = 1;
    single_cfa.height = 1;
    assert!(!single_cfa.is_bayer(), "1x1 CFA must NOT be recognized as Bayer");
}

#[test]
fn test_safe_file_buffer_and_drive_type_handling() {
    // 1. Temp file on local fixed disk
    let mut temp = tempfile::NamedTempFile::new().expect("temp file");
    let test_data = b"RAPIDRAW_SAFE_FILE_BUFFER_STRESS_PAYLOAD_ABC123";
    temp.write_all(test_data).expect("write");

    let safe_buf = read_file_safe(temp.path()).expect("read_file_safe must succeed");
    assert_eq!(safe_buf.len(), test_data.len());
    assert_eq!(&safe_buf[..], test_data);
    assert_eq!(safe_buf.as_ref(), test_data);
    assert_eq!(&safe_buf[0..8], b"RAPIDRAW");

    let mapped_buf = read_file_mapped(temp.path()).expect("read_file_mapped must succeed");
    assert_eq!(mapped_buf.len(), test_data.len());
    assert_eq!(&mapped_buf[..], test_data);

    // 2. Drive type probe on local temp path should be false (fixed drive, not removable)
    let is_removable = is_removable_or_remote_drive(temp.path());
    println!("Local path {:?} is_removable_or_remote_drive: {}", temp.path(), is_removable);
    assert!(!is_removable, "C: or D: fixed drive must not be marked removable/remote");

    // 3. UNC network paths must be detected as remote
    assert!(is_removable_or_remote_drive(Path::new(r"\\192.168.1.100\photos\raw.dng")));
    assert!(is_removable_or_remote_drive(Path::new(r"\\nas\share\IMG_001.CR2")));
    assert!(is_removable_or_remote_drive(Path::new("//nas/share/IMG_001.CR2")));
    assert!(is_removable_or_remote_drive(Path::new(r"\\?\UNC\nas\share\IMG_001.CR2")));

    // 4. Non-existent file error handling
    let non_existent = Path::new("d:\\non_existent_drive_or_path_rapidraw_12345.raw");
    let err_res = read_file_safe(non_existent);
    assert!(err_res.is_err(), "Non-existent file must return Err");

    // 5. Empty file error handling
    let empty_file = tempfile::NamedTempFile::new().expect("empty temp file");
    let empty_res = read_file_safe(empty_file.path());
    assert!(empty_res.is_err(), "Expected Empty error on 0-byte file");
}
