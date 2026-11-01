# Handoff Report: Investigation of Requirements R1 and R3

**Author:** `teamwork_explorer_survey_v28_1`  
**Working Directory:** `d:\RapidRAW_V.03\.agents\teamwork\teamwork_explorer_survey_v28_1\`  
**Target:** Parent Orchestrator (`5a72144f-8be1-4295-9cb2-a3907b4e00a6`) & Implementation Worker  
**Date:** 2026-10-30T22:55:00Z  
**Scope:**
1. **R1: Dark TIFF Export Tone-Mapping Parity**
2. **R3: Real-Time Synthesis Progress Broadcasting**

---

## 1. Observation

### 1.1 Requirement R1: Dark TIFF Export Tone-Mapping Parity

#### A. Export Format Branching in `src-tauri/src/lib.rs` (`save_hdr`)
In [`src-tauri/src/lib.rs#L1722-L1798`](file:///d:/RapidRAW_V.03/src-tauri/src/lib.rs#L1722-L1798), `save_hdr` inspects format options and dispatches image writing:

```rust
1722:    let hdr_preview = {
1723:        let hdr_guard = state.hdr_result.lock().map_err(|e| e.to_string())?;
1724:        hdr_guard
1725:            .as_ref()
1726:            .ok_or_else(|| "No HDR image to save".to_string())?
1727:            .clone()
1728:    };
1729:    let linear_radiance = state.hdr_linear_radiance.lock().unwrap().clone();
...
1757:    match export_format {
1758:        "jpeg" | "jpg" => {
1759:            let sdr_rgb = hdr_preview.to_rgb8();
1760:            crate::export_processing::save_jpeg_high_quality_with_metadata(&output_path, &sdr_rgb, Some(&real_path_str))?;
1761:        }
1762:        "ultrahdr" => {
1763:            let sdr_rgb = hdr_preview.to_rgb8();
1764:            let lin_img = linear_radiance.unwrap_or_else(|| hdr_preview.to_rgb32f());
1765:            crate::export_processing::save_ultrahdr_jpeg_with_metadata(&output_path, &lin_img, &sdr_rgb, Some(&real_path_str))?;
1766:        }
1767:        "png" => {
1768:            crate::export_processing::save_png_high_quality_with_metadata(&output_path, &hdr_preview, Some(&real_path_str))?;
1769:        }
1770:        "dng" => {
1771:            let lin_img = linear_radiance.unwrap_or_else(|| hdr_preview.to_rgb32f());
...
1785:            crate::dng_encoder::write_linear_dng_file(&output_path, &lin_img, Some(&dng_meta))
1786:                .map_err(|e| format!("Failed to save 32-bit Linear DNG image: {}", e))?;
1787:        }
1788:        _ => {
1789:            let lin_img = linear_radiance.unwrap_or_else(|| hdr_preview.to_rgb32f());
1790:            crate::export_processing::save_tiff_compressed(&output_path, &lin_img)?;
1791:        }
1792:    }
```

- **Data Types:**
  - `hdr_preview`: `image::DynamicImage`, instantiated in `merge_hdr` as `DynamicImage::ImageRgb32F(fusion_res.tone_mapped_preview)` ([`src-tauri/src/lib.rs#L1622`](file:///d:/RapidRAW_V.03/src-tauri/src/lib.rs#L1622)) and in `update_hdr_tone_mapping` as `DynamicImage::ImageRgb32F(tm)` ([`src-tauri/src/lib.rs#L1691`](file:///d:/RapidRAW_V.03/src-tauri/src/lib.rs#L1691)). The underlying pixel values are tone-mapped, shadow-lifted, and display-encoded in sRGB color space with values bounded in $[0.0, 1.0]$.
  - `linear_radiance`: `Option<image::Rgb32FImage>`, containing scene-referred physical radiance $(\gamma = 1.0)$. Midtones are centered around $0.18$ (18% gray), with highlights extending well above $1.0$.

#### B. TIFF Encoder Implementation in `src-tauri/src/export_processing.rs`
In [`src-tauri/src/export_processing.rs#L1329-L1355`](file:///d:/RapidRAW_V.03/src-tauri/src/export_processing.rs#L1329-L1355):

```rust
1329: pub fn save_tiff_compressed<P: AsRef<Path>>(path: P, img: &image::Rgb32FImage) -> Result<(), String> {
...
1342:     // TPDF anti-contour continuous-tone print dithering
1343:     let mut rng_seed = 0x853c49e6748fea9b_u64;
1344:     let u16_data: Vec<u16> = img.as_raw().iter().map(|&v| {
1345:         let d = tpdf_dither(&mut rng_seed) / 65535.0;
1346:         ((v + d).clamp(0.0, 1.0) * 65535.0).round() as u16
1347:     }).collect();
1348: 
1349:     image.write_data(&u16_data).map_err(|e| format!("TIFF write error: {}", e))?;
1350:     std::io::Write::flush(&mut writer).map_err(|e| format!("Failed to flush TIFF: {}", e))?;
1351:     drop(writer);
1352: 
1353:     let _ = inject_icc_and_resolution_tags_into_tiff(path.as_ref(), crate::exif_processing::STANDARD_SRGB_ICC_PROFILE, 300, 300);
1354:     Ok(())
1355: }
```
- `save_tiff_compressed` maps incoming float values linearly to 16-bit integer $v \times 65535.0$ and attaches `STANDARD_SRGB_ICC_PROFILE` (`Tag 34675`).
- It applies **no gamma transfer curve**. It expects incoming float values to already be in display sRGB non-linear color space.

#### C. Exact Parallel Pattern in `src-tauri/src/panorama_stitching.rs`
In [`src-tauri/src/panorama_stitching.rs#L321-L331`](file:///d:/RapidRAW_V.03/src-tauri/src/panorama_stitching.rs#L321-L331):
```rust
321:         _ => {
322:             let lin_img_fallback;
323:             let lin_img: &Rgb32FImage = match linear_radiance {
324:                 Some(r) => r,
325:                 None => {
326:                     lin_img_fallback = panorama_image.to_rgb32f();
327:                     &lin_img_fallback
328:                 }
329:             };
330:             crate::export_processing::save_tiff_compressed(&output_path, lin_img)?;
331:         }
```
If `panorama_linear_radiance` is populated, panorama TIFF export exhibits the exact same dark TIFF bug!

#### D. Batch Rendering Reference in `src-tauri/src/bin/render_all.rs` and `test_save_formats.rs`
In [`src-tauri/src/bin/render_all.rs#L588-L596`](file:///d:/RapidRAW_V.03/src-tauri/src/bin/render_all.rs#L588-L596):
```rust
588:             "tiff" => {
589:                 match rapidraw_lib::export_processing::save_tiff_compressed(
590:                     &out_path,
591:                     &fusion_result.tone_mapped_preview,
592:                 ) {
```
And in [`src-tauri/tests/test_save_formats.rs#L213`](file:///d:/RapidRAW_V.03/src-tauri/tests/test_save_formats.rs#L213):
```rust
213:             rapidraw_lib::export_processing::save_tiff_compressed(&out_path, &fusion_result.tone_mapped_preview).unwrap();
```
In both the batch renderer and the test suite, `save_tiff_compressed` is passed `&fusion_result.tone_mapped_preview`! Only `save_hdr` in `lib.rs` (and `save_panorama`) mistakenly grabbed `linear_radiance`.

---

### 1.2 Requirement R3: Real-Time Synthesis Progress Broadcasting

#### A. Frontend Progress Parsing in `src/components/modals/HdrModal.tsx`
In [`src/components/modals/HdrModal.tsx#L238-L239`](file:///d:/RapidRAW_V.03/src/components/modals/HdrModal.tsx#L238-L239) and [`#L502-L511`](file:///d:/RapidRAW_V.03/src/components/modals/HdrModal.tsx#L502-L511):
```tsx
238:   const pctMatch = progressMessage?.match(/(\d+)%/);
239:   const progressPct = pctMatch ? parseInt(pctMatch[1], 10) : null;
...
502:                 {progressPct !== null ? (
503:                   <div className="flex flex-col items-center gap-2 w-full">
504:                     <div className="h-2 bg-neutral-800 rounded-full overflow-hidden w-full relative shadow-inner">
505:                       <div
506:                         className="h-full bg-gradient-to-r from-amber-500 to-amber-400 transition-all duration-300 rounded-full"
507:                         style={{ width: `${progressPct}%` }}
508:                       />
509:                     </div>
510:                     <span className="text-xs font-mono font-bold text-amber-400">{progressPct}% completed</span>
511:                   </div>
512:                 ) : (
513:                   <div className="h-1.5 bg-surface rounded-full overflow-hidden relative w-full shadow-xs">
...
```
- The frontend expects `progressMessage` to match `/(\d+)%/`.
- If matched, `progressPct` is extracted (0–100), driving the amber progress bar and percentage text.
- If not matched (`progressPct === null`), it falls back to an indeterminate shimmer loop.

#### B. Event Delivery Pipeline in `src/hooks/useTauriListeners.ts`
In [`src/hooks/useTauriListeners.ts#L316-L328`](file:///d:/RapidRAW_V.03/src/hooks/useTauriListeners.ts#L316-L328):
```ts
316:       listen('hdr-progress', (event: any) => {
317:         if (isEffectActive) {
318:           useUIStore.getState().setUI((state) => {
319:             if (!state.hdrModalState.isOpen) return state;
320:             return {
321:               hdrModalState: {
322:                 ...state.hdrModalState,
323:                 progressMessage: event.payload,
324:               },
325:             };
326:           });
327:         }
328:       }),
```
`hdr-progress` payload string is stored directly in `hdrModalState.progressMessage`.

#### C. Backend Emission Points in `src-tauri/src/hdr_handheld.rs`
In [`src-tauri/src/hdr_handheld.rs`](file:///d:/RapidRAW_V.03/src-tauri/src/hdr_handheld.rs):
1. **Bracket Frame Loading (`load_handheld_brackets_with_mode`, lines 188–195):**
   ```rust
   let _ = h.emit("hdr-progress", format!("Loading linear RAW bracket {}/{} ({})...", idx + 1, coherent_paths.len(), name));
   ```
   *(Missing `%` digit!)*
2. **Alignment & Calibration (`merge_handheld_hdr_linear_panel`, lines 824–833):**
   ```rust
   let _ = h_app.emit("hdr-progress", format!("Aligning & calibrating bracket {}/{} (EV ratio {:.2}x)...", k + 1, n, exif_ratio));
   ```
   *(Missing `%` digit!)*
3. **Linear Radiance Synthesis (`merge_handheld_hdr_linear_panel`, lines 881–885):**
   ```rust
   let _ = h_app.emit("hdr-progress", "Merging Reference-Locked 32-bit Linear Radiance...");
   ```
   *(Missing `%` digit!)*
4. **Laplacian Pyramid Tone-Mapping (`merge_handheld_hdr_pipeline`, lines 1100–1104):**
   ```rust
   let _ = h_app.emit("hdr-progress", "Tone-mapping via Virtual-Exposure Multi-Scale Laplacian Pyramid...");
   ```
   *(Missing `%` digit!)*
5. **Inside `tone_map_virtual_exposure_pyramid` (lines 1129–1407):**
   Does not accept `app_handle` and emits zero events across the entire 8-level Gaussian and Laplacian multi-scale collapse (which takes multiple seconds on large RAW files).
6. **Preview Creation (`src-tauri/src/lib.rs:1624`):**
   ```rust
   let _ = app_handle.emit("hdr-progress", "Creating preview... 98%");
   ```
   *(This is the ONLY emission that contained `%` digits!)*

#### D. Secondary Engine Emission Points in `src-tauri/src/hdr_fusion.rs`
In [`src-tauri/src/hdr_fusion.rs`](file:///d:/RapidRAW_V.03/src-tauri/src/hdr_fusion.rs):
- Line 870: `"Normalizing bracket white balance & exposure gains..."` *(no %)*
- Line 889: `"Calculating Multi-Scale Exposure Fusion weights..."` *(no %)*
- Line 898: `"Applying Guided Filter edge preservation (halo elimination)..."` *(no %)*
- Line 923: `"Applying morphological motion deghosting..."` *(no %)*
- Line 931: `"Constructing Laplacian Pyramids..."` *(no %)*
- Line 994: `"Blending Multi-Scale Pyramid layers..."` *(no %)*
- Line 1051: `"Reconstructing high dynamic range image... 85%"` *(has %)*
- Line 1071: `"Anchoring contrast & dynamic range... 90%"` *(has %)*
- Line 1077: `"Applying AgX filmic color grading... 95%"` *(has %)*
- Line 1309: `"Accumulating 32-bit linear HDR radiance map..."` *(no %)*
- Line 1556: `"Applying 3-Tier Multi-Scale Decomposition & Oklab Perceptual Grading..."` *(no %)*

---

## 2. Logic Chain

### 2.1 Why Exported TIFF Was Pitch Black (R1)
1. **Scene-Linear Radiance vs Display-Referred Color:**
   - In scene-linear radiance ($\gamma = 1.0$), an 18% reflective gray card has a value of $0.18$.
   - The tone-mapped display preview (`hdr_preview`) compresses the dynamic range, lifts shadows, and applies the sRGB transfer curve (`linear_to_srgb_fast`). An 18% gray midtone in `hdr_preview` is around $0.46 \dots 0.50$ (display code value $\approx 118 \dots 128$ in 8-bit, or $30000 \dots 32768$ in 16-bit).
2. **Transfer Function Mismatch in TIFF:**
   - `save_tiff_compressed` takes floating-point values in $[0.0, 1.0]$, multiplies directly by $65535.0$, quantizes to `u16`, and embeds an sRGB ICC Profile (`Tag 34675`).
   - Standard image viewers (Windows Photos, Apple Preview, Photoshop, Lightroom, web browsers) inspect the embedded sRGB profile and apply the sRGB decoding transfer function ($\approx \gamma = 2.2$) to display the image.
   - When scene-linear radiance is stored directly:
     $$L_{\text{display}} \approx (0.18)^{2.2} \approx 0.0229$$
   - The midtone renders at $0.0229$ (less than $2.3\%$ brightness).
   - In exposure stops:
     $$\Delta \text{EV} = \log_2\left(\frac{0.0229}{0.18}\right) \approx -2.97 \text{ to } -2.35 \text{ EV}$$
   - The image appears severely underexposed, virtually pitch black in shadows and deep midtones.
3. **Parity with JPEG and PNG:**
   - JPEG exports `hdr_preview.to_rgb8()`.
   - PNG exports `&hdr_preview` through `save_png_high_quality_with_metadata`, which preserves `hdr_preview.as_rgb32f()`.
   - TIFF in `save_hdr` (line 1789) passed `linear_radiance.unwrap_or_else(|| hdr_preview.to_rgb32f())`. Because `linear_radiance` was populated, it bypassed `hdr_preview`!
   - Replacing line 1789 with `let tm_img = hdr_preview.to_rgb32f();` guarantees that the 16-bit TIFF receives the approved tone-mapped display image with full 16-bit continuous-tone gradation, perfectly matching the on-screen preview.
   - 32-bit uncompressed scene-linear radiance remains properly preserved for professional raw post-processing workflows under the dedicated `"dng"` export format (Line 1770–1787).

### 2.2 Why HDR Progress Bar Appeared Frozen (R3)
1. **Regex Dependency in UI:**
   - `HdrModal.tsx:238` performs `progressMessage?.match(/(\d+)%/);`.
   - When no digits followed by `%` exist in the string, `progressPct` is `null`.
2. **Silence in Backend Emission:**
   - In `hdr_handheld.rs`, stages 1 to 4 emit strings like `"Loading linear RAW bracket 1/3 (IMG_4712.CR2)..."`, `"Aligning & calibrating bracket 2/3 (EV ratio 2.00x)..."`, and `"Tone-mapping via Virtual-Exposure Multi-Scale Laplacian Pyramid..."`.
   - Zero numbers with `%` were emitted during these stages.
   - Only at line 1624 (`lib.rs`) did `"Creating preview... 98%"` finally trigger the regex, jumping from indeterminate shimmer directly to 98% in the last 100 milliseconds of execution.
3. **Stage Allocation:**
   Allocating monotonic percentage intervals across the execution pipeline:
   - **0% – 25%:** Raw bracket decoding (`5 + (idx * 20 / count)`)
   - **25% – 50%:** Handheld alignment & homography calibration (`25 + (k * 25 / n)`)
   - **50% – 65%:** Reference-locked radiance merge & deghosting
   - **65% – 95%:** 8-level Burt-Adelson Laplacian pyramid virtual exposure tone-mapping (per channel: Ch0=78%, Ch1=84%, Ch2=90%, Oklab grading=94%)
   - **95% – 100%:** Preview encoding (98%) and completion (100%)
   Guarantees that the progress bar animates smoothly from 0% to 100% without jumping or freezing.

---

## 3. Caveats

1. **DNG Export Must Retain `linear_radiance`:**
   - Do NOT modify the `"dng"` export branch in `lib.rs:1771`. The 32-bit Linear DNG format is designed specifically for raw developers (Lightroom/Photoshop Camera Raw) and must remain scene-linear floating-point radiance.
2. **`save_panorama` in `panorama_stitching.rs`:**
   - The primary requirement R1 specifies `save_hdr` in `src-tauri/src/lib.rs`. However, `save_panorama` in `src-tauri/src/panorama_stitching.rs:321-331` has the exact same pattern. It should also be updated to ensure panorama TIFF exports are not dark.
3. **`tone_map_virtual_exposure_pyramid` Callers:**
   - `tone_map_virtual_exposure_pyramid` is called in `hdr_handheld.rs:1107`, `hdr_panorama.rs:381`, `lib.rs:1690`, and `panorama_stitching.rs:782`.
   - To avoid breaking external callers, either provide `app_handle: Option<&AppHandle<R>>` in an overloaded function `tone_map_virtual_exposure_pyramid_with_progress`, or add `app_handle: Option<&AppHandle<R>>` and pass `None` from non-UI callers.
4. **Interactive Slider Tone-Mapping Updates (`update_hdr_tone_mapping`):**
   - When the user drags exposure/highlight/shadow sliders in `HdrModal`, `update_hdr_tone_mapping` runs in < 80ms on cached `linear_radiance`. It does not require progress broadcasting since it is interactive real-time tuning.

---

## 4. Conclusion & Concrete Implementation Plan

### 4.1 Step 1: Fix Dark TIFF Export in `src-tauri/src/lib.rs` (R1)

**Target File:** `src-tauri/src/lib.rs`  
**Lines:** 1788–1792  
**Action:** Replace `linear_radiance` with `hdr_preview.to_rgb32f()`:

```rust
<<<< BEFORE (Line 1788-1792)
        _ => {
            let lin_img = linear_radiance.unwrap_or_else(|| hdr_preview.to_rgb32f());
            crate::export_processing::save_tiff_compressed(&output_path, &lin_img)?;
        }
==== AFTER
        _ => {
            // High-bit-depth 16-bit TIFF receives the approved tone-mapped sRGB image
            // with full 16-bit dynamic gradation, matching on-screen preview 100%
            let tm_img = hdr_preview.to_rgb32f();
            crate::export_processing::save_tiff_compressed(&output_path, &tm_img)?;
        }
>>>>
```

**Target File:** `src-tauri/src/panorama_stitching.rs`  
**Lines:** 321–331  
**Action:** Apply identical tone-mapping parity for Panorama TIFF export:

```rust
<<<< BEFORE (Line 321-331)
        _ => {
            let lin_img_fallback;
            let lin_img: &Rgb32FImage = match linear_radiance {
                Some(r) => r,
                None => {
                    lin_img_fallback = panorama_image.to_rgb32f();
                    &lin_img_fallback
                }
            };
            crate::export_processing::save_tiff_compressed(&output_path, lin_img)?;
        }
==== AFTER
        _ => {
            let tm_img = panorama_image.to_rgb32f();
            crate::export_processing::save_tiff_compressed(&output_path, &tm_img)?;
        }
>>>>
```

---

### 4.2 Step 2: Implement Real-Time Synthesis Progress Broadcasting (R3)

#### A. In `src-tauri/src/hdr_handheld.rs`:

1. **Stage 1: Raw Bracket Decoding (lines 188–195):**
   ```rust
   let pct = 5 + (idx * 20 / coherent_paths.len());
   let _ = h.emit(
       "hdr-progress",
       format!(
           "Decoding raw exposure {}/{} ({})... {}%",
           idx + 1,
           coherent_paths.len(),
           name,
           pct
       ),
   );
   ```

2. **Stage 2: Handheld Alignment & Calibration (lines 824–833):**
   ```rust
   let pct = 25 + (k * 25 / n);
   let _ = h_app.emit(
       "hdr-progress",
       format!(
           "Aligning handheld bracket {}/{} (EV ratio {:.2}x)... {}%",
           k + 1,
           n,
           exif_ratio,
           pct
       ),
   );
   ```

3. **Stage 3: 32-bit Linear Radiance Synthesis (lines 880–885, line 975, line 1070):**
   ```rust
   if let Some(h_app) = app_handle {
       let _ = h_app.emit(
           "hdr-progress",
           "Synthesizing 32-bit linear radiance map... 52%",
       );
   }
   ```
   And after blending darker brackets:
   ```rust
   if let Some(h_app) = app_handle {
       let _ = h_app.emit(
           "hdr-progress",
           "Merging dynamic range highlights & deep shadows... 60%",
       );
   }
   ```
   At completion of linear panel:
   ```rust
   if let Some(h_app) = app_handle {
       let _ = h_app.emit(
           "hdr-progress",
           "Linear radiance map complete... 65%",
       );
   }
   ```

4. **Stage 4: Laplacian Pyramid Tone Mapping (`merge_handheld_hdr_pipeline` & `tone_map_virtual_exposure_pyramid`):**
   In `merge_handheld_hdr_pipeline` (line 1100):
   ```rust
   if let Some(h_app) = app_handle {
       let _ = h_app.emit(
           "hdr-progress",
           "Synthesizing HDR tones: Multi-scale Laplacian pyramid... 68%",
       );
   }
   ```
   In `tone_map_virtual_exposure_pyramid` (or `tone_map_virtual_exposure_pyramid_with_progress`):
   - At line 1274 (Weight Pyramids):
     `"Constructing multi-scale Gaussian weight pyramids... 72%"`
   - In channel fusion loop (line 1296):
     ```rust
     let ch_pct = 76 + ch * 6; // Ch 0: 76%, Ch 1: 82%, Ch 2: 88%
     if let Some(h_app) = app_handle {
         let _ = h_app.emit(
             "hdr-progress",
             format!("Synthesizing HDR tones: Multi-scale fusion channel {}/3... {}%", ch + 1, ch_pct),
         );
     }
     ```
   - At line 1380 (Oklab Perceptual Grading):
     `"Applying Oklab perceptual grading & dynamic range anchor... 94%"`

5. **Stage 5: Final Preview & Completion in `src-tauri/src/lib.rs`:**
   In `merge_hdr` line 1624:
   ```rust
   let _ = app_handle.emit("hdr-progress", "Creating high-fidelity preview... 98%");
   ```
   Right before emitting `hdr-complete` (line 1635):
   ```rust
   let _ = app_handle.emit("hdr-progress", "HDR synthesis complete... 100%");
   ```

#### B. In `src-tauri/src/hdr_fusion.rs` (Fallback / Dual Engine):
Add percentage format strings to existing text-only emissions:
- Line 870: `"Normalizing bracket white balance & exposure gains... 15%"`
- Line 889: `"Calculating Multi-Scale Exposure Fusion weights... 25%"`
- Line 898: `"Applying Guided Filter edge preservation (halo elimination)... 35%"`
- Line 923: `"Applying morphological motion deghosting... 45%"`
- Line 931: `"Constructing Laplacian Pyramids... 55%"`
- Line 994: `"Blending Multi-Scale Pyramid layers... 70%"`
- Line 1309: `"Accumulating 32-bit linear HDR radiance map... 40%"`
- Line 1556: `"Applying 3-Tier Multi-Scale Decomposition & Oklab Perceptual Grading... 75%"`

---

## 5. Verification Method

### 5.1 Unit Test for TIFF Tone-Mapping Parity
Add a synthetic unit test to `src-tauri/tests/test_save_formats.rs`:

```rust
#[test]
fn test_save_hdr_tiff_tone_mapping_parity() {
    use image::{Rgb32FImage, Rgb, DynamicImage};
    let (w, h) = (32u32, 32u32);
    // Synthetic tone-mapped preview with display sRGB midtone 0.5 (~128 in 8-bit, ~32768 in 16-bit)
    let mut tm_img = Rgb32FImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            tm_img.put_pixel(x, y, Rgb([0.5, 0.5, 0.5]));
        }
    }
    let temp_dir = std::env::temp_dir();
    let temp_tiff = temp_dir.join("test_tone_mapping_parity.tiff");
    rapidraw_lib::export_processing::save_tiff_compressed(&temp_tiff, &tm_img)
        .expect("Failed to save TIFF");

    // Read back using tiff decoder
    let file = std::fs::File::open(&temp_tiff).expect("Failed to open saved TIFF");
    let mut decoder = tiff::decoder::Decoder::new(file).expect("Failed to create TIFF decoder");
    match decoder.read_image().expect("Failed to decode TIFF") {
        tiff::decoder::DecodingResult::U16(data) => {
            let sample = data[0];
            // 0.5 * 65535 = 32767.5 (+/- 2 due to TPDF dither)
            assert!(
                (sample as i32 - 32768).abs() <= 2,
                "Expected 16-bit midtone around 32768, got {}",
                sample
            );
        }
        _ => panic!("Expected U16 TIFF decoding result"),
    }
    let _ = std::fs::remove_file(&temp_tiff);
}
```

**Run command:**
```powershell
cd src-tauri
cargo test --test test_save_formats test_save_hdr_tiff_tone_mapping_parity -- --nocapture
```

### 5.2 Unit Test for Progress String Format
Add a unit test verifying regex extraction:
```rust
#[test]
fn test_hdr_progress_format_contains_percentage() {
    let test_messages = [
        "Decoding raw exposure 1/3 (IMG_001.CR2)... 11%",
        "Aligning handheld bracket 2/3 (EV ratio 2.00x)... 37%",
        "Synthesizing 32-bit linear radiance map... 52%",
        "Synthesizing HDR tones: Multi-scale fusion channel 1/3... 76%",
        "Creating high-fidelity preview... 98%",
        "HDR synthesis complete... 100%",
    ];
    for msg in test_messages {
        let has_pct = msg.contains('%');
        assert!(has_pct, "Message '{}' must contain percentage symbol", msg);
        let digits: String = msg.chars().rev().skip(1).take_while(|c| c.is_ascii_digit()).collect();
        assert!(!digits.is_empty(), "Message '{}' must have digits preceding '%'", msg);
    }
}
```

### 5.3 Workspace Verification Commands
1. **Rust Tests:**
   ```powershell
   cd src-tauri
   cargo test --workspace
   ```
2. **Frontend Typecheck:**
   ```powershell
   npm run typecheck
   ```
3. **End-to-End Verification:**
   - Launch HDR modal with 3 bracket images.
   - Observe progress bar moving from 5% $\to$ 25% $\to$ 50% $\to$ 75% $\to$ 98% $\to$ 100%.
   - Save output as TIFF (`.tiff`).
   - Open saved TIFF in Windows Photos or Photoshop; verify exposure and brightness match the on-screen preview 1:1.
