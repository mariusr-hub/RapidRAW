# BRIEFING — 2026-10-30T22:54:00Z

## Mission
Investigate R2 (Large Folder Crash Prevention & Linear DNG Support) across Rust backend and React frontend, providing root cause analysis, architecture design, and diff specifications.

## 🔒 My Identity
- Archetype: explorer
- Roles: read-only investigation, analyze problems, synthesize findings, produce structured reports
- Working directory: d:\RapidRAW_V.03\.agents\teamwork\teamwork_explorer_survey_v28_2\
- Original parent: 5a72144f-8be1-4295-9cb2-a3907b4e00a6
- Milestone: Survey & Investigation v28 (Requirement R2)

## 🔒 Key Constraints
- Read-only investigation — do NOT implement
- Inspect file_management.rs, image_loader.rs, raw_processing.rs, useAppNavigation.ts
- Provide exact paths, line numbers, root cause, code diffs, verification tests

## Current Parent
- Conversation ID: 5a72144f-8be1-4295-9cb2-a3907b4e00a6
- Updated: 2026-10-30T22:54:00Z

## Investigation State
- **Explored paths**: `src-tauri/src/file_management.rs`, `src-tauri/src/image_loader.rs`, `src-tauri/src/raw_processing.rs`, `src-tauri/src/dng_encoder.rs`, `src-tauri/src/formats.rs`, `src/hooks/useAppNavigation.ts`, `src/store/useLibraryStore.ts`
- **Key findings**:
  1. `read_file_mapped` (file_management.rs:1449) crashes on removable/network drives with uncatchable SEH `0xc0000006 STATUS_IN_PAGE_ERROR`. Can be fixed with `is_removable_or_remote_drive` via `GetDriveTypeW` and `SafeFileBuffer` (Mapped vs Buffered `Deref<Target = [u8]>`).
  2. `prefetch_source_file` (file_management.rs:1935) performs redundant 50MB `fs::read` per thumbnail, saturating USB I/O. Must be eliminated.
  3. `rawler` panics on 3-channel Linear DNGs with `cpp == 1` assertion failure (raw_processing.rs:356). Can be resolved via fast IFD probe `is_linear_dng_probe` and routing to TIFF reader `load_linear_dng`.
  4. `condition_raw_bayer_sensor` (raw_processing.rs:377) must be guarded with `raw_image.cfa.is_bayer()`.
  5. `fetchExifInChunks` (useAppNavigation.ts:384) clones the full 5,000-image array 100 times (500,000 objects), thrashing V8 GC. Resolved via key-value store `exifMap`, in-place mutation, and single commit.
- **Unexplored areas**: None for requirement R2. Complete root cause, diffs, and verification tests delivered.

## Key Decisions Made
- `SafeFileBuffer` implements `Deref<Target = [u8]>`, making it a transparent drop-in replacement across all 14 call sites in 12 files.
- `is_linear_dng_probe` uses zero heap allocations by parsing TIFF IFD tags directly from header bytes.
- Handoff report finalized in `handoff.md`.

## Artifact Index
- d:\RapidRAW_V.03\.agents\teamwork\teamwork_explorer_survey_v28_2\DISPATCH.md — Dispatch instructions
- d:\RapidRAW_V.03\.agents\teamwork\teamwork_explorer_survey_v28_2\progress.md — Liveness & progress heartbeat
- d:\RapidRAW_V.03\.agents\teamwork\teamwork_explorer_survey_v28_2\handoff.md — Final handoff report
