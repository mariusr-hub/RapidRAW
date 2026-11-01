# Project: RapidRAW Architectural Parity & Stability Master Plan (v2.8)

## Architecture
RapidRAW is a high-performance cross-platform RAW image processor built with Tauri v2 (Rust backend) and React/TypeScript (frontend with WebGPU canvas).
- **Backend Core**: Tauri commands in `src-tauri/src/lib.rs`, image decoding via `rawler` and `image`/`tiff`, HDR synthesis in `hdr_handheld.rs` and `hdr_fusion.rs`, file operations in `file_management.rs`.
- **Compute Pipeline**: WebGPU compute shaders in `src-tauri/crates/rapidraw-gpu/src/shader.wgsl` and `src-tauri/src/shaders/shader.wgsl`.
- **Frontend Architecture**: React 18, TailwindCSS, Zustand stores (`useLibraryStore`, `useUIStore`), Tauri listener hooks (`useTauriListeners.ts`), Editor canvas (`ImageCanvas.tsx`, `Editor.tsx`).

## Feature Inventory
Every feature from the Survey phase appears here with its assigned milestone:
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Dark TIFF Export Fix | Export `hdr_preview.to_rgb32f()` instead of `linear_radiance` in `save_hdr` & `save_panorama` | M1 | Survey R1 |
| 2 | TIFF Tone-Mapping Parity Unit Test | Verify 16-bit TIFF export matches tone-mapped preview midtones (~32768) | M1 | Survey R1 |
| 3 | Real-Time Synthesis Progress Broadcasting | Broadcast granular 0%–100% stage percentages in `hdr_handheld.rs` & `hdr_fusion.rs` | M1 | Survey R3 |
| 4 | Safe Buffered I/O on Removable Media | Detect `DRIVE_REMOVABLE`/`DRIVE_REMOTE` via Win32 `GetDriveTypeW`, fallback from `mmap` to `fs::read` | M2 | Survey R2 |
| 5 | Eliminate Redundant Prefetch | Remove `prefetch_source_file` in `file_management.rs:1935` | M2 | Survey R2 |
| 6 | 3-Channel Linear DNG Direct TIFF Loader | Fast IFD probe `is_linear_dng_probe` and route to standard TIFF decoder before `rawler` | M2 | Survey R2 |
| 7 | Bayer Sensor Conditioning CFA Guard | Guard `condition_raw_bayer_sensor` with `raw_image.cfa.is_bayer()` | M2 | Survey R2 |
| 8 | Large Folder EXIF Store Optimization | Use key-value `exifMap` in `useLibraryStore` to eliminate V8 GC re-cloning loops | M2 | Survey R2 |
| 9 | Window Transparency & Solid Background | Set `#121212` solid background in `index.html`, `styles.css`, `App.tsx`, and clean close teardown | M3 | Survey R4 |
| 10 | Left Sidebar Dual-Tab Switcher | Add `[Folders & Sources]` vs `[HDR & Looks]` dual tabs in left dock in `App.tsx` and `TonesPanel.tsx` | M3 | Survey R5 |
| 11 | WB Dropper Neutral 5500K Fallback | Fallback to 5500K daylight neutral in `ImageCanvas.tsx` when EXIF as-shot is missing | M4 | Survey R6 |
| 12 | Auto-Crop Pixel Coordinate Scaling | Scale normalized 0–100% coordinates by `cropBaseWidth`/`cropBaseHeight` in `CropPanel.tsx` | M4 | Survey R6 |
| 13 | Competitor Brand Sanitization | Purge SNS-HDR, Radiant Photo, Luminar Neo, DxO, AKVIS across UI, code variables, comments | M5 | Survey R7 |
| 14 | WGSL Shader Synchronization & Test | Enforce byte-for-byte SHA256 identity between rapidraw-gpu and src-tauri shader copies | M5 | Survey R7 |
| 15 | Workspace Verification & Packaging | Run `cargo test --workspace`, `npm run typecheck`, build `RapidRAW_1.6.7-enhanced.1_x64-setup.exe` | M6 | Survey R8 |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| 1 | M1: HDR Tone-Mapping Parity & Progress Broadcasting | R1, R3: Fix dark TIFF export in `lib.rs` & `panorama_stitching.rs`; granular 0%–100% progress in `hdr_handheld.rs` & `hdr_fusion.rs`; unit tests | none | DONE |
| 2 | M2: Safe File I/O, Linear DNG & Folder Navigation | R2: `SafeFileBuffer` + Win32 drive detection; Linear DNG probe + TIFF routing; `exifMap` batch store | none | IN_PROGRESS |
| 3 | M3: Window Stability, Background & Left Dual-Tab Navigation | R4, R5: Solid `#121212` backgrounds; close teardown; Left Sidebar dual tabs `[Folders & Sources]` vs `[HDR & Looks]` | none | PLANNED |
| 4 | M4: White Balance Dropper Fallback & Auto-Crop Scaling | R6: 5500K neutral daylight fallback in `ImageCanvas.tsx`; pixel scaling in `CropPanel.tsx` | none | PLANNED |
| 5 | M5: Brand Scrubbing & Shader Synchronization | R7: Trademark purge across UI strings, tooltips, identifiers, comments; automated shader hash identity test | none | PLANNED |
| 6 | M6: Verification, Test Suite & Release Build Packaging | R8: Full workspace cargo tests, TypeScript typecheck, release installer build | M1, M2, M3, M4, M5 | PLANNED |

## Interface Contracts

### 1. `save_hdr` & `save_panorama` ↔ `export_processing::save_tiff_compressed`
- **Caller**: `src-tauri/src/lib.rs:1789`, `src-tauri/src/panorama_stitching.rs:321`
- **Argument**: `&DynamicImage::to_rgb32f(&hdr_preview)` (`image::Rgb32FImage` with sRGB display-referred pixel values in $[0.0, 1.0]$)
- **Output**: 16-bit integer TIFF with Deflate compression and standard sRGB ICC profile tag (34675).

### 2. `hdr-progress` Event Payload Contract
- **Emitter**: `src-tauri/src/hdr_handheld.rs`, `src-tauri/src/hdr_fusion.rs`, `src-tauri/src/lib.rs`
- **Listener**: `src/hooks/useTauriListeners.ts:316`, `src/components/modals/HdrModal.tsx:238`
- **Payload Format**: String containing `(\d+)%` (e.g., `"Decoding raw exposure 1/3 (IMG_001.CR2)... 11%"`, `"HDR synthesis complete... 100%"`)
- **Range**: Monotonically increasing across stages 0%–100%.

### 3. Safe File Buffer Contract
- **Type**: `pub enum SafeFileBuffer { Mapped(memmap2::Mmap), Buffered(Vec<u8>) }`
- **Trait**: `impl std::ops::Deref<Target = [u8]> for SafeFileBuffer`
- **Function**: `pub fn read_file_safe(path: &Path) -> Result<SafeFileBuffer, ReadFileError>`
- **Behavior**: On Windows, checks `is_removable_or_remote_drive(path)`. If removable or network drive, uses `std::fs::read`. On fixed SSD/HDD, uses `Mmap`. Transparent drop-in replacement for all `read_file_mapped` callers.

### 4. Linear DNG Decoder Route Contract
- **Function**: `pub fn is_linear_dng_probe(bytes: &[u8]) -> bool`
- **Contract**: Parses TIFF header magic (42) and IFD tags without allocating. Returns `true` if `PhotometricInterpretation == 34892` (LinearRaw) or `2` (RGB) and `SamplesPerPixel >= 3`.
- **Router**: In `image_loader.rs` and `raw_processing.rs`, if `is_linear_dng_probe(&bytes)` is `true`, route to standard TIFF decoder `load_linear_dng_buffer`, completely bypassing `rawler` demosaicing.

### 5. Crop Recommendation Contract
- **Function**: `handleCompositionAi` in `CropPanel.tsx`
- **Backend output**: `{ x: number, y: number, width: number, height: number }` in normalized percentages $[0, 100]$
- **Frontend calculation**:
  - `cropX = (res.x * cropBaseWidth) / 100`
  - `cropY = (res.y * cropBaseHeight) / 100`
  - `cropW = (res.width * cropBaseWidth) / 100`
  - `cropH = (res.height * cropBaseHeight) / 100`
- **Commit**: `setAdjustments({ ...prev, crop: { unit: 'px', x: cropX, y: cropY, width: cropW, height: cropH } })`

## Code Layout
- `src-tauri/src/lib.rs`: Tauri command handlers, event registration, window close teardown.
- `src-tauri/src/hdr_handheld.rs`: Handheld bracket alignment, deghosting, Laplacian pyramid tone mapping.
- `src-tauri/src/hdr_fusion.rs`: Exposure fusion engine and fallback pipelines.
- `src-tauri/src/file_management.rs`: Safe buffered/mapped file reading, drive detection.
- `src-tauri/src/image_loader.rs`: Image format routing, thumbnail generation, Linear DNG handling.
- `src-tauri/src/raw_processing.rs`: RAW demosaicing and sensor conditioning.
- `src-tauri/src/export_processing.rs`: JPEG, PNG, TIFF, and UltraHDR export pipelines.
- `src-tauri/crates/rapidraw-gpu/src/shader.wgsl` & `src-tauri/src/shaders/shader.wgsl`: Compute shaders.
- `src/App.tsx`: Top application frame, left sidebar panels, layout containers.
- `src/components/panel/left/TonesPanel.tsx`: Dynamic HDR profiles and styles.
- `src/components/panel/left/FolderTree.tsx`: Library folder tree navigation.
- `src/components/panel/editor/ImageCanvas.tsx`: WebGPU canvas, eyedropper, interactive tools.
- `src/components/panel/right/CropPanel.tsx`: Aspect ratio, leveling, composition AI crop.
- `src/hooks/useAppNavigation.ts` & `src/store/useLibraryStore.ts`: Folder navigation and EXIF cache store.
