# Review & Adversarial Critic Report: Milestone 1 (R1 & R3)

**Verdict**: `APPROVE`  
**Reviewer**: `reviewer_m1_2`  
**Role**: Reviewer & Adversarial Critic  
**Date**: 2026-10-31T02:00:00Z  
**Target**: Orchestrator (`5a72144f-8be1-4295-9cb2-a3907b4e00a6`)  

---

## Executive Summary

Worker `worker_m1`'s modifications for Milestone 1 directly and rigorously resolve:
1. **R1 (Dark TIFF Export)**: Replaces the scene-linear radiance export bypass in `save_hdr` (`src-tauri/src/lib.rs:1792`) and `save_panorama` (`src-tauri/src/panorama_stitching.rs:324`) with `hdr_preview.to_rgb32f()` / `panorama_image.to_rgb32f()`. This delivers tone-mapped display-referred sRGB floats into `save_tiff_compressed`, resulting in a decoded 16-bit midtone of ~32768 (matching the on-screen preview 1:1), while properly retaining 32-bit linear radiance for `.dng` raw workflows.
2. **R3 (Granular Synthesis Progress Broadcasting)**: Injects monotonically increasing percentage tags matching `/(\d+)%/` across all stages of bracket loading (5%–25%), handheld alignment (25%–50%), linear radiance synthesis (52%–65%), Burt-Adelson multi-scale Laplacian pyramid tone mapping (68%–94%), and preview completion (98%–100%) in `hdr_handheld.rs`, `hdr_fusion.rs`, and `lib.rs`. This directly activates the percentage progress bar in `HdrModal.tsx`.
3. **Integrity & Verification**: No integrity violations detected. No dummy facades or hardcoded bypasses. All tests (`test_save_formats` 10/10 passed, `cargo test --lib` 148/148 passed, and `npm run typecheck` 0 errors) were independently executed and passed.

---

## Review Findings & Quality Assessment

### Verified Claims
- **Claim 1**: `save_hdr` and `save_panorama` 16-bit TIFF export now uses tone-mapped preview rather than scene-linear radiance.  
  *Verification*: Inspected `src-tauri/src/lib.rs:1792` (`let tm_img = hdr_preview.to_rgb32f(); crate::export_processing::save_tiff_compressed(&output_path, &tm_img)?;`) and `src-tauri/src/panorama_stitching.rs:324` (`let tm_img = panorama_image.to_rgb32f(); crate::export_processing::save_tiff_compressed(&output_path, &tm_img)?;`). Verified both call `save_tiff_compressed` with display-referred tone-mapped floats.
- **Claim 2**: Tone-mapping math eliminates dark TIFF export completely.  
  *Verification*: `save_tiff_compressed` tags TIFF files with the standard sRGB ICC profile (`Tag 34675`). Under sRGB encoding, an 18% gray midtone corresponds to ~0.50 in display sRGB floats, which `save_tiff_compressed` maps to $0.50 \times 65535 \approx 32768 \pm 2$ (via TPDF dither). Previously, scene-linear radiance (where 18% gray is 0.18) resulted in $(0.18)^{2.2} \approx 0.0229$ (~1500 / 65535, or ~3 stops underexposed) in sRGB-aware viewers. Test `test_save_hdr_tiff_tone_mapping_parity` wrote a synthetic 0.5 midtone image and decoded it with `tiff::decoder::Decoder`, verifying the decoded pixel is $32768 \pm 2$ (> 30000).
- **Claim 3**: All progress emissions match `/(\d+)%/` across stages without regressions.  
  *Verification*: Verified regex match for every emitted string in `hdr_handheld.rs`, `hdr_fusion.rs`, and `lib.rs` via `test_hdr_progress_format_contains_percentage`. Verified frontend regex in `HdrModal.tsx:238` matches all emissions.
- **Claim 4**: Test suite passes cleanly.  
  *Verification*:
  - `cargo test --manifest-path src-tauri/Cargo.toml --test test_save_formats -- --nocapture`: 10 passed, 0 failed, 1 ignored.
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib`: 148 passed, 0 failed.
  - `npm.cmd run typecheck`: 0 errors (exit code 0).

---

## Adversarial Challenges & Stress Testing

### Challenge 1: Zero or Degenerate Bracket Counts
- **Assumption Challenged**: Stage progress formulas `5 + (idx * 20 / len)` and `25 + (k * 25 / n)` assume non-zero slice lengths.
- **Attack Scenario**: Calling `load_handheld_brackets_with_mode` or `merge_handheld_hdr_linear_panel` with an empty or single-element slice could trigger integer division by zero (`SIGFPE`).
- **Inspection & Stress Test**:
  - `src-tauri/src/lib.rs:1604` guards entry with `if paths.len() < 2 { return Err("Please select at least two images to merge.".to_string()); }`.
  - In `src-tauri/src/hdr_handheld.rs:187`, the divisor is `coherent_paths.len().max(1)`.
  - In `src-tauri/src/hdr_handheld.rs:825`, the divisor is `n.max(1)`.
- **Verdict**: PASS. Division by zero is mathematically impossible.

### Challenge 2: Progress Monotonicity Under Variable Exposure Counts
- **Assumption Challenged**: High bracket counts (e.g. 5, 7, 9 frames) might cause progress percentages to exceed 100% or jump non-monotonically.
- **Stress Test Trace**:
  - Stage 1 (Load): $5 + \frac{\text{idx} \times 20}{\text{len}} \in [5, 25]\%$
  - Stage 2 (Align): $25 + \frac{k \times 25}{n} \in [25, 50]\%$
  - Stage 3 (Radiance): 52% $\to$ 60% $\to$ 65%
  - Stage 4 (Laplacian Pyramid): 68% $\to$ 72% $\to$ 76% (Ch 0) $\to$ 82% (Ch 1) $\to$ 88% (Ch 2) $\to$ 94% (Oklab)
  - Stage 5 (Preview & Complete): 98% $\to$ 100%
- **Verdict**: PASS. Sequence is strictly monotonic, non-decreasing, and strictly upper-bounded by 100%.

### Challenge 3: Backward Compatibility for Non-UI Callers of `tone_map_virtual_exposure_pyramid`
- **Assumption Challenged**: Adding `app_handle: Option<&AppHandle<R>>` to `tone_map_virtual_exposure_pyramid` could break callers in `hdr_panorama.rs` or external tests that do not pass an `AppHandle`.
- **Inspection**:
  - `worker_m1` preserved `pub fn tone_map_virtual_exposure_pyramid(radiance: &Rgb32FImage, options: &HdrMergeOptions) -> Rgb32FImage` as a public wrapper delegating to `tone_map_virtual_exposure_pyramid_with_progress::<tauri::Wry>(radiance, options, None)`.
  - All 148 library unit tests compile and run with zero signature errors.
- **Verdict**: PASS. Non-UI consumers function without breakage.

### Coverage Gap (Minor, Non-Blocking)
- In `hdr2.rs:248, 414` and `hdr2_ai.rs:83`, progress emissions remain untagged with `%` (e.g., `"HDR2: Normalizing white balance & applying flat-field devignetting..."`).
- **Risk Assessment**: LOW / Acceptable. `Hdr2` and `Hdr2Ai` are non-default research experimental models invoked only in specialized offline benchmark scripts (`render_all.rs`). The production synthesis pipeline in RapidRAW exclusively routes through `merge_handheld_hdr_pipeline`, which is 100% tagged.

---

## 5-Component Handoff Report

### 1. Observation
- `src-tauri/src/lib.rs` (lines 1789–1795):
  ```rust
  _ => {
      // High-bit-depth 16-bit TIFF receives the approved tone-mapped sRGB image
      // with full 16-bit dynamic gradation, matching on-screen preview 100%
      let tm_img = hdr_preview.to_rgb32f();
      crate::export_processing::save_tiff_compressed(&output_path, &tm_img)?;
  }
  ```
- `src-tauri/src/panorama_stitching.rs` (lines 321–326):
  ```rust
  _ => {
      let tm_img = panorama_image.to_rgb32f();
      crate::export_processing::save_tiff_compressed(&output_path, &tm_img)?;
  }
  ```
- `src-tauri/src/hdr_handheld.rs`:
  - Line 187: `let pct = 5 + (idx * 20 / coherent_paths.len().max(1));`
  - Line 825: `let pct = 25 + (k * 25 / n.max(1));`
  - Lines 886, 924, 1082: emits 52%, 60%, 65%.
  - Lines 1119, 1324, 1334, 1427: emits 68%, 72%, 76%/82%/88%, 94%.
- `src-tauri/src/hdr_fusion.rs`:
  - Lines 870, 889, 898, 923, 931, 994, 1051, 1071, 1077, 1309, 1556 emit percentage-tagged strings across all stages.
- `src-tauri/tests/test_save_formats.rs`:
  - `test_save_hdr_tiff_tone_mapping_parity`: asserts `(sample as i32 - 32768).abs() <= 2` and `sample > 30000`.
  - `test_hdr_progress_format_contains_percentage`: asserts 24 stage messages match `/(\d+)%/` and `pct_val <= 100`.

### 2. Logic Chain
1. When exporting a 16-bit TIFF, `save_tiff_compressed` encodes floating point input via $((v + d) \cdot 65535)$ and writes an sRGB ICC profile tag.
2. Previously, passing un-tone-mapped scene-linear radiance caused sRGB decoding applications to render 18% gray at $(0.18)^{2.2} \approx 0.0229$ (~1500 / 65535), appearing almost entirely black.
3. Passing tone-mapped sRGB floats from `hdr_preview.to_rgb32f()` / `panorama_image.to_rgb32f()` ensures 18% gray is mapped to ~0.50 (32768 / 65535), achieving parity with the on-screen preview.
4. `HdrModal.tsx` parses progress via `progressMessage?.match(/(\d+)%/)`. Without `%`, `progressPct` remained `null`, locking the UI into an indeterminate shimmer bar.
5. Ingesting `/(\d+)%/` across stages 1 through 5 provides monotonically increasing percentages (5% $\to$ 100%), allowing the frontend progress bar to fill smoothly and display `{progressPct}% completed`.

### 3. Caveats
- DirectML acceleration for experimental `hdr2_ai` was not run under hardware test since it is outside the M1 scope.
- No caveats regarding regressions in the main pipeline.

### 4. Conclusion
Milestone 1 satisfies all requirements for R1 and R3. The tone-mapping export logic is sound and verified by round-trip decode assertions. Granular progress broadcasting complies with the frontend regex contract without gaps or division-by-zero risks. Integrity checks passed with zero violations.

### 5. Verification Method
To independently reproduce and verify this assessment:
```powershell
# 1. Run save formats unit tests
cargo test --manifest-path src-tauri/Cargo.toml --test test_save_formats -- --nocapture

# 2. Run full Rust library test suite
cargo test --manifest-path src-tauri/Cargo.toml --lib

# 3. Run TypeScript typecheck
npm.cmd run typecheck
```
Invalidation conditions:
- Any test failure in `test_save_hdr_tiff_tone_mapping_parity` or `test_hdr_progress_format_contains_percentage`.
- Any TypeScript typecheck errors.
- Any regression in `cargo test --lib`.
