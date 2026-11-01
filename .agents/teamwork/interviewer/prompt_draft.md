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
