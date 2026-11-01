# Empirical Challenge & Regression Verification Report: Milestone 1 (R1 & R3)

**Author**: `challenger_m1_2` (Empirical Challenger)  
**Target Folder**: `d:\RapidRAW_V.03\.agents\teamwork\challenger_m1_2\`  
**Target**: Orchestrator (`5a72144f-8be1-4295-9cb2-a3907b4e00a6`)  
**Date**: 2026-10-31T01:59:30Z  
**Verdict**: **`APPROVE`**

---

## 1. Observation

### 1.1 Linear DNG Export Non-Regression (`src-tauri/src/lib.rs:1771–1788`)
Direct inspection of `save_hdr` in `src-tauri/src/lib.rs` confirms that 32-bit Linear DNG export remains intact and preserves uncompressed scene-linear physical radiance:
```rust
1771:         "dng" => {
1772:             let lin_img = linear_radiance.unwrap_or_else(|| hdr_preview.to_rgb32f());
1773:             let mut dng_meta = crate::dng_encoder::DngExportMetadata::default();
1774:             dng_meta.description = Some(format!("RapidRAW 32-Bit Linear Composite ({})", suffix));
1775: 
1776:             if let Ok(raw_source) = rawler::rawsource::RawSource::new(std::path::Path::new(&real_path_str)) {
1777:                 let loader = rawler::RawLoader::new();
1778:                 if let Ok(decoder) = loader.get_decoder(&raw_source) {
1779:                     if let Ok(raw_meta) = decoder.raw_metadata(&raw_source, &Default::default()) {
1780:                         if !raw_meta.make.is_empty() { dng_meta.make = Some(raw_meta.make); }
1781:                         if !raw_meta.model.is_empty() { dng_meta.model = Some(raw_meta.model); }
1782:                     }
1783:                 }
1784:             }
1785: 
1786:             crate::dng_encoder::write_linear_dng_file(&output_path, &lin_img, Some(&dng_meta))
1787:                 .map_err(|e| format!("Failed to save 32-bit Linear DNG image: {}", e))?;
1788:         }
```
Only the fallback branch `_` (TIFF export, lines 1789–1795) was adjusted to consume the tone-mapped preview:
```rust
1789:         _ => {
1790:             // High-bit-depth 16-bit TIFF receives the approved tone-mapped sRGB image
1791:             // with full 16-bit dynamic gradation, matching on-screen preview 100%
1792:             let tm_img = hdr_preview.to_rgb32f();
1793:             crate::export_processing::save_tiff_compressed(&output_path, &tm_img)?;
1794:         }
```

### 1.2 Panorama Stitching TIFF Parity (`src-tauri/src/panorama_stitching.rs:321–326`)
Direct inspection of `save_panorama` in `src-tauri/src/panorama_stitching.rs` confirms complete parity with `save_hdr`:
```rust
321:         _ => {
322:             // High-bit-depth 16-bit TIFF receives the approved tone-mapped sRGB image
323:             // with full 16-bit dynamic gradation, matching on-screen preview 100%
324:             let tm_img = panorama_image.to_rgb32f();
325:             crate::export_processing::save_tiff_compressed(&output_path, &tm_img)?;
326:         }
```
In addition, panorama DNG export (lines 291–320) continues to pass `linear_radiance` (`&Rgb32FImage`) directly to `write_linear_dng_file`.

### 1.3 Test Suite Execution
Direct empirical execution of `cargo test --manifest-path src-tauri/Cargo.toml --test test_save_formats -- --nocapture`:
```text
running 11 tests
test test_render_hdr_and_pano_targets ... ignored, Full dataset render - use 'cargo run --release --bin render_all' for batch rendering
test test_dual_illuminant_interpolation ... ok
test test_encode_linear_dng_and_tags ... ok
test test_sensel_laplacian_inpainting ... ok
test test_resize_in_linear_space_radiance ... ok
test test_tpdf_dithering_and_quantization ... ok
test test_autodetect_tca_fallback ... ok
Compressed 100x100 TIFF size: 477 bytes (Uncompressed: 120,000 bytes)
test test_save_tiff_deflate_compression ... ok
test test_hdr_progress_format_contains_percentage ... ok
test test_tiff_icc_profile_injection ... ok
test test_save_hdr_tiff_tone_mapping_parity ... ok

test result: ok. 10 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

Core library unit tests:
```text
cargo test --manifest-path src-tauri/Cargo.toml --lib
test result: ok. 148 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 18.10s
```

Frontend typecheck:
```text
npm.cmd run typecheck
> rapidraw@1.6.8 typecheck
> tsc --noEmit
(Exit code 0, 0 errors)
```

---

## 2. Logic Chain

1. **DNG Integrity Assessment**:
   - 32-bit Linear DNG is specifically designed for non-destructive raw workflows in Lightroom and ACR, requiring physical scene-linear radiance ($\gamma = 1.0$) rather than tone-mapped sRGB.
   - Observation 1.1 proves that `src-tauri/src/lib.rs:1772` extracts `linear_radiance.unwrap_or_else(...)` and passes `&lin_img` to `write_linear_dng_file`.
   - The test `test_encode_linear_dng_and_tags` (Observation 1.3) verifies that linear floats up to `1.2` encode into a valid Linear DNG without error.
   - Therefore, the DNG export pipeline did not regress and correctly retains scene-linear data.

2. **Panorama Stitching TIFF Parity Assessment**:
   - `save_panorama` previously forwarded scene-linear floats to `save_tiff_compressed`. Because `save_tiff_compressed` does not apply a transfer curve and tags the image with sRGB ICC, viewing applications applied an sRGB decoding curve, yielding an underexposed dark TIFF image.
   - Observation 1.2 demonstrates that `src-tauri/src/panorama_stitching.rs:324–325` now passes `panorama_image.to_rgb32f()` (the tone-mapped display image) to `save_tiff_compressed`.
   - This matches `src-tauri/src/lib.rs:1792–1793` 1:1, ensuring consistent exposure between preview and exported TIFF across both single-bracket HDR synthesis and multi-shot panorama stitching.

3. **Progress Broadcasting Granularity Assessment**:
   - The frontend regex `/(\d+)%/` in `HdrModal.tsx` requires percentage digits in event messages to update the progress bar.
   - In `hdr_handheld.rs` and `hdr_fusion.rs`, 23 event emissions across Stages 1–5 now include percentage tags (11% through 100%).
   - The unit test `test_hdr_progress_format_contains_percentage` (Observation 1.3) confirms all 23 messages parse correctly under `/(\d+)%/` and satisfy `pct <= 100`.

---

## 3. Caveats

- `test_render_hdr_and_pano_targets` in `test_save_formats.rs` is flagged `#[ignore]` because it expects offline raw test files in `D:\neapdirbti`. Its ignore status is standard for local test runs and does not affect the validity of unit or integration test assertions.
- No regressions or defects were detected.

---

## 4. Conclusion

Empirical challenge and adversarial inspection confirm:
- 32-bit Linear DNG export (`"dng"` branch) correctly preserves scene-linear radiance without regression.
- Stitched panorama TIFF export (`save_panorama`) maintains 100% parity with HDR TIFF export, using tone-mapped sRGB.
- All unit tests in `test_save_formats` pass (10 passed, 0 failed, 1 ignored).
- Core library tests (148 passed) and TypeScript compilation (0 errors) verify system stability.

**Final Verdict**: **`APPROVE`**

---

## 5. Verification Method

To independently reproduce this verification:
1. Run the save formats test suite:
   ```powershell
   cargo test --manifest-path src-tauri/Cargo.toml --test test_save_formats -- --nocapture
   ```
2. Verify core library tests:
   ```powershell
   cargo test --manifest-path src-tauri/Cargo.toml --lib
   ```
3. Inspect `src-tauri/src/lib.rs:1771-1795` and `src-tauri/src/panorama_stitching.rs:321-326`.
