# Original User Request

## Approved Task Prompt — 2026-10-29T15:18:51Z

Execute the Master Architectural Gap Matrix & Comprehensive Engineering Plan (v2.5) for RapidRAW to resolve display flicker, restore dual-dock left navigation, fix white balance eyedropper and auto-crop coordinate scaling, consolidate UI controls, sanitize competitor trademarks, implement image pipeline enhancements with upstream v1.6.7 parity, and build the verified release installer.

Working directory: d:\RapidRAW_V.03
Integrity mode: development

Reference Plan: `C:\Users\Kibernetinė Jungtis\.gemini\antigravity\brain\9e9e86f5-f613-480e-9135-284f0b725438\radiant_photo_instant_enhancement_ui_plan.md`

## Requirements

### R1. Display & Stability Fixes
- Disable window transparency in `src-tauri/tauri.conf.json` (`"transparent": false`) to eliminate Windows DWM desktop compositing screen flicker.
- Ensure `wgpu::PresentMode::Fifo` (V-Sync) is locked in `gpu_processing.rs`.
- Register clean window teardown on `WindowEvent::CloseRequested` so no background GPU threads linger after shutdown.

### R2. Navigation & Dual-Dock Restoration
- In `src/App.tsx` and `src/components/panel/left/TonesPanel.tsx`, restore full `FolderTree` access by implementing a top dual-tab bar in the Left Sidebar: `[ 📁 Folders & Sources ]` vs `[ ✨ HDR & Styles ]`. Prevent users from being trapped in TonesPanel without folder tree access.

### R3. White Balance & Auto-Crop Coordinate Fixes
- In `src/components/panel/editor/ImageCanvas.tsx`, decouple `applyWbPick` from camera EXIF metadata presence, falling back to 5500K daylight neutral if `asShotWhiteBalance` is undefined. Ensure pointer events on the canvas stage are not intercepted by SVG overlays so picking WB works 100% of the time.
- In `src/components/panel/right/CropPanel.tsx`, fix `handleCompositionAi` by scaling normalized percentage coordinates (0–100%) into image pixel dimensions before updating `adjustments.crop`, immediately synchronizing canvas state.

### R4. UI/UX Feature Consolidation & Brand Sanitization
- Remove `ReferencePresetsRibbon` from `ControlsPanel.tsx`.
- Consolidate competing "Auto" buttons into one primary "Smart Auto Enhance" button on the top toolbar (with subtle/medium/vivid options) and a baseline "Auto Tone" in `Basic.tsx`.
- Remove duplicate White Balance sliders from `Color.tsx`, keeping WB exclusively in `Basic.tsx`.
- Unify Noise Reduction in `Details.tsx` with a `[ Standard / BM3D ]` vs `[ Neural AI Denoise ]` segmented mode switch, retiring the detached `DenoiseModal.tsx`.
- Remove competitor trademarks ("SNS-HDR", "Radiant Photo", "Radiant Vivid", "Luminar Neo Composition AI", "DxO", "AKVIS") across UI strings, tooltips, variable names (`SNS_HDR_PROFILES`, `RADIANT_CREATIVE_LOOKS`), struct fields (`_pad_dxo_end`), and code comments, replacing them with photography terminology ("Dynamic HDR Profiles", "Smart Auto-Crop & Level", "Edge Flow Guides", "Protected Areas").

### R5. Image Pipeline, Sensor Guards & Upstream Parity
- Implement IQbQr luma isolation and blue-noise shadow dither in `ai_denoise.rs` and `shader.wgsl`.
- Guard Bayer-specific sensor routines (`condition_raw_bayer_sensor`, Gr/Gb Nyquist cross-talk) with `if raw_image.cfa.is_bayer()` to protect Fuji X-Trans RAF and Linear DNG files.
- Maintain full upstream official v1.6.7 parity (DNG dual illuminants, ACES gamut compression, dark channel dehaze, PR #2004/#2005 safe file operations).
- Ensure `src-tauri/crates/rapidraw-gpu/src/shader.wgsl` and `src-tauri/src/shaders/shader.wgsl` remain 100% byte-for-byte identical.

### R6. Verification & Release Build
- Rely on existing repository test fixtures and synthetic/mock inputs for validation.
- Verify `cargo test --workspace` passes 100%.
- Verify `npm run typecheck` passes with 0 errors.
- Confirm byte-for-byte identity of shader files.
- Build verified release installer: `RapidRAW_1.6.7-enhanced.1_x64-setup.exe`.

## Acceptance Criteria

### Display & Stability
- [ ] `src-tauri/tauri.conf.json` has `"transparent": false`.
- [ ] `gpu_processing.rs` enforces `wgpu::PresentMode::Fifo`.
- [ ] Window close teardown cleanly stops GPU threads without lingering processes.

### Navigation & UI
- [ ] Left sidebar contains functional dual-tab navigation switching between Folders & Sources and HDR & Styles.
- [ ] Top toolbar has single "Smart Auto Enhance" control; duplicate Auto buttons and duplicate WB sliders in `Color.tsx` are removed.
- [ ] Details panel contains segmented noise reduction mode switch; `DenoiseModal.tsx` is removed.

### Image Processing & Coordinate Handling
- [ ] WB eyedropper functions reliably without EXIF metadata, defaulting to 5500K neutral daylight.
- [ ] Auto-crop correctly converts percentage bounding boxes to pixel values matching image dimensions.
- [ ] Non-Bayer raw images (Fuji X-Trans RAF, Linear DNG) bypass Bayer-specific sensor conditioning.
- [ ] IQbQr luma isolation and blue-noise dither are active in denoising pipeline.
- [ ] Both `shader.wgsl` copies match byte-for-byte (identical cryptographic hash).

### Brand Scrubbing
- [ ] Zero occurrences of competitor brand names ("SNS-HDR", "Radiant Photo", "Radiant Vivid", "Luminar Neo", "DxO", "AKVIS") in UI text, code comments, or identifiers.

### Verification & Deliverables
- [ ] `cargo test --workspace` passes with 0 failures.
- [ ] `npm run typecheck` passes with 0 errors.
- [ ] Installer artifact `RapidRAW_1.6.7-enhanced.1_x64-setup.exe` is built successfully.


## Approved Task Prompt — 2026-10-30T22:40:21Z

Execute the approved Master Architectural Gap Matrix & Comprehensive Engineering Plan (v2.8) for RapidRAW to resolve stability issues, fix dark TIFF exports and large folder crashes, restore navigation, and build the release installer.

Working directory: d:\RapidRAW_V.03
Integrity mode: development

Reference plan: C:\Users\Kibernetinė Jungtis\.gemini\antigravity\brain\9e9e86f5-f613-480e-9135-284f0b725438\radiant_photo_instant_enhancement_ui_plan.md

## Requirements

### R1. Dark TIFF Export Tone-Mapping Parity
In `src-tauri/src/lib.rs` (`save_hdr`), export `hdr_preview.to_rgb32f()` (the approved tone-mapped sRGB image) instead of scene-linear radiance so exported 16-bit TIFF matches the on-screen preview 100% in brightness/exposure.

### R2. Large Folder Crash Prevention & Linear DNG Support
In `file_management.rs`, `image_loader.rs`, and `useAppNavigation.ts`:
- Replace memory-mapping (`mmap` / `read_file_mapped`) on removable USB/network drives with safe buffered `fs::read` and eliminate redundant whole-file prefetch to prevent fatal OS in-page errors (`0xc0000006`).
- Detect 3-channel Linear DNGs (such as `IMG_5138_Hdr.dng`) before `rawler` decoding and route them to the TIFF reader to avoid demosaicing assertion panics.
- Eliminate frontend array re-cloning loops on large image lists in `useAppNavigation.ts` (e.g., using key-value batched metadata storage).

### R3. Real-Time Synthesis Progress Broadcasting
In `hdr_handheld.rs` and `hdr_fusion.rs`, broadcast granular 0%–100% real-time progress percentages across all processing stages to activate the percentage progress bar in `HdrModal.tsx`.

### R4. Window Transparency & Background Stability
Enforce `"transparent": false` in `src-tauri/tauri.conf.json` and permanent solid background colors (`#121212` / `#18181b`) on `html, body, #root` in `index.html` and `styles.css`.

### R5. Left Sidebar Navigation Dual-Tab Switcher
In `App.tsx` and `TonesPanel.tsx`, restore Left Sidebar navigation by providing a dual-tab switcher: `[Folders & Sources]` and `[HDR & Looks]`.

### R6. White Balance Dropper Fallback & Auto-Crop Pixel Scaling
In `ImageCanvas.tsx`, implement a neutral (5500K) fallback for the WB dropper so it functions reliably when camera EXIF as-shot metadata is absent. In `CropPanel.tsx`, scale auto-crop saliency percentage coordinates (0–100%) to actual pixel dimensions.

### R7. Brand Scrubbing & Shader Synchronization
Purge competitor trademarks (SNS-HDR, Radiant Photo, Luminar Neo, DxO) across UI strings and identifiers. Ensure `crates/rapidraw-gpu/src/shader.wgsl` and `src-tauri/src/shaders/shader.wgsl` remain byte-for-byte identical.

### R8. Verification and Release Packaging
Verify all changes by running `cargo test --workspace` and `npm run typecheck`, generating any required synthetic test fixtures or unit tests for Linear DNG and TIFF tone mapping verification. Build the signed release installer `RapidRAW_1.6.7-enhanced.1_x64-setup.exe` via `npm run tauri build`.

## Acceptance Criteria

### Export & Stability
- [ ] Exported 16-bit TIFF from HDR synthesis matches the on-screen tone-mapped preview in exposure and brightness.
- [ ] Loading folders with 3-channel Linear DNG files decodes without `rawler` panics.
- [ ] Removable drive file access uses buffered I/O without fatal memory-mapping crashes (`0xc0000006`).
- [ ] Large photo folders do not cause UI freeze or garbage collection loops.

### UI & Navigation
- [ ] HDR synthesis broadcasts granular percentage progress (0%–100%) displayed in `HdrModal.tsx`.
- [ ] Application window is non-transparent with solid background `#121212` / `#18181b`.
- [ ] Left Sidebar includes functional tabs for `[Folders & Sources]` and `[HDR & Looks]`.
- [ ] WB Dropper functions when as-shot EXIF is missing.
- [ ] Auto-crop applies correct pixel boundaries rather than raw percentage offsets.

### Code Quality & Release
- [ ] No competitor brand names remain in user-facing UI strings.
- [ ] `crates/rapidraw-gpu/src/shader.wgsl` and `src-tauri/src/shaders/shader.wgsl` are byte-for-byte identical.
- [ ] `cargo test --workspace` passes with zero failures.
- [ ] `npm run typecheck` passes with zero TypeScript errors.
- [ ] `npm run tauri build` successfully generates `RapidRAW_1.6.7-enhanced.1_x64-setup.exe`.
