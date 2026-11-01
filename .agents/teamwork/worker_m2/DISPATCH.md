## 2026-10-31T00:01:45Z
[Message] timestamp=2026-10-31T00:01:45Z sender=5a72144f-8be1-4295-9cb2-a3907b4e00a6 priority=MESSAGE_PRIORITY_HIGH content=You are worker_m2.
Your working directory is: d:\RapidRAW_V.03\.agents\teamwork\worker_m2\

MANDATORY FIRST STEP:
Read d:\RapidRAW_V.03\.agents\teamwork\ORIGINAL_REQUEST.md completely.
Also read d:\RapidRAW_V.03\.agents\teamwork\orchestrator\PROJECT.md.
Read Explorer 2's detailed handoff report in:
d:\RapidRAW_V.03\.agents\teamwork\teamwork_explorer_survey_v28_2\handoff.md

MANDATORY INTEGRITY WARNING:
DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A teamwork_auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.

Scope & Write Ownership for Milestone 2:
You own exclusively:
- `src-tauri/src/file_management.rs`
- `src-tauri/src/image_loader.rs`
- `src-tauri/src/raw_processing.rs`
- `src/hooks/useAppNavigation.ts`
- `src/store/useLibraryStore.ts`
- `src-tauri/tests/test_save_formats.rs` (or new test file `src-tauri/tests/test_linear_dng.rs`)

Tasks to Implement:
1. Safe File Buffer & Removable Drive Detection (`src-tauri/src/file_management.rs`):
   - Define `SafeFileBuffer` (`Mapped(memmap2::Mmap)`, `Buffered(Vec<u8>)`) implementing `std::ops::Deref<Target = [u8]>` and `AsRef<[u8]>`.
   - Implement `is_removable_or_remote_drive(path: &Path) -> bool` using Win32 `GetDriveTypeW` on Windows (detecting `DRIVE_REMOVABLE`, `DRIVE_REMOTE`, and UNC `\\` prefixes).
   - In `read_file_mapped` (or `read_file_safe`), check if path is on a removable or network drive: if true, use `fs::read(path)` returning `SafeFileBuffer::Buffered(vec)`; if false, use `memmap2::MmapOptions` returning `SafeFileBuffer::Mapped(mmap)`. Ensure transparent drop-in compatibility for all existing call sites.
   - Eliminate redundant whole-file prefetch (`prefetch_source_file` in line 1935).
2. Linear DNG 3-Channel Direct TIFF Loader (`src-tauri/src/image_loader.rs` & `src-tauri/src/raw_processing.rs`):
   - Implement zero-allocation `is_linear_dng_probe(bytes: &[u8]) -> bool` inspecting TIFF header and IFD tags: returns true if `PhotometricInterpretation == 34892` (LinearRaw) or `2` (RGB) and `SamplesPerPixel >= 3`.
   - In `image_loader.rs` (`load_image_with_orientation` / `develop_raw_image`) and `raw_processing.rs`: when `is_linear_dng_probe` is true, route directly to standard TIFF reader (`image::load_from_memory` or `tiff::decoder::Decoder`), completely bypassing `rawler` demosaicing to avoid `left == right: 1 == 3` panics.
   - Guard `condition_raw_bayer_sensor` in `raw_processing.rs`: `if !raw_image.cfa.is_bayer() { return; }`.
3. Frontend Large Folder EXIF Store Optimization:
   - In `src/store/useLibraryStore.ts`, add `exifMap: Record<string, any>` to store state, and `updateExifMap(chunk: Record<string, any>)`.
   - In `src/hooks/useAppNavigation.ts` (`fetchExifInChunks` lines 384–405): eliminate the 50-item array re-cloning loop. Update `exifMap` via `useLibraryStore.getState().updateExifMap(chunkExif)` and mutate `image.exif` in-place on existing array elements, avoiding V8 GC spikes and UI freezes.
4. Unit Tests & Verification:
   - In `src-tauri/tests/test_save_formats.rs`: add `test_linear_dng_probe_and_load_synthetic` generating a synthetic 3-channel Linear DNG using `rapidraw_lib::dng_encoder::encode_linear_dng` and verifying `is_linear_dng_probe` returns true and decodes without rawler demosaicing panic.
   - Verify `cargo test --manifest-path src-tauri/Cargo.toml --test test_save_formats -- --nocapture` passes 100%.
   - Verify `npm.cmd run typecheck` passes with 0 errors.

Deliverable:
Write a comprehensive completion report to `d:\RapidRAW_V.03\.agents\teamwork\worker_m2\handoff.md`.
Include test outputs, files modified, and rationale.
When done, notify the orchestrator via `send_message`.
