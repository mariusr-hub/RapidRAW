# Progress Report — teamwork_explorer_survey_v28_3

Last visited: 2026-10-30T22:58:00Z
Status: COMPLETED

## Completed
- Initialized DISPATCH.md and BRIEFING.md
- Reviewed `ORIGINAL_REQUEST.md` and reference plan `radiant_photo_instant_enhancement_ui_plan.md`
- Surveyed R4 (Window Transparency & Background Stability):
  - `src-tauri/tauri.conf.json`: `transparent` is already `false`.
  - `index.html`: missing background styling on `html, body, #root`.
  - `src/styles.css`: `html` and `body` explicitly set to `transparent`, causing desktop flash.
  - `src/App.tsx`: line 871 uses `isWgpuActive ? 'bg-transparent' : 'bg-bg-primary'`.
  - `src/components/panel/Editor.tsx`: line 2291 leaves viewport background blank when wgpu is active.
  - Teardown: `WindowEvent::CloseRequested` missing in `src-tauri/src/lib.rs`.
- Surveyed R5 (Left Sidebar Dual-Tab Switcher):
  - `src/App.tsx:717-720`: in editor mode, `Panel.FolderTree` is completely hijacked and replaced by `TonesPanel`.
  - Designed dual-tab switcher: `[Folders & Sources]` and `[HDR & Looks]` in left sidebar navigation.
- Surveyed R6 (White Balance Dropper & Auto-Crop Pixel Scaling):
  - `src/components/panel/editor/ImageCanvas.tsx:2376`: `applyWbPick` ignores picks if `asShotWhiteBalance` is undefined. Needs neutral daylight 5500K fallback.
  - `src/components/panel/right/CropPanel.tsx:263-273`: `handleCompositionAi` sets percentage coordinates into `adjustments.crop`, which `Editor.tsx:360` interprets as raw pixels. Must scale by `cropBaseWidth` and `cropBaseHeight`.
- Surveyed R7 (Brand Scrubbing & Shader Sync):
  - Found all competitor trademarks (SNS-HDR, Radiant Photo, Luminar Neo, DxO, AKVIS) in UI strings, tooltips, identifiers, comments, and unit tests.
  - Located both copies of `shader.wgsl` (`src-tauri/crates/rapidraw-gpu/src/shader.wgsl` and `src-tauri/src/shaders/shader.wgsl`). Confirmed identical SHA256 (`B8B2B841D756BECE1A287C21674DAE359552ABD92BE4C43EBACB66D756E47EEE`).
  - Proposed automated regression test `test_shader_wgsl_byte_identity`.
- Surveyed R8 (Verification and Release Packaging):
  - Identified `scripts/with-stay-awake.ps1` for power management during builds.
  - Validated `npm run typecheck` passes with 0 errors.
  - Detailed release packaging prerequisites.
- Generated comprehensive handoff report at `d:\RapidRAW_V.03\.agents\teamwork\teamwork_explorer_survey_v28_3\handoff.md`.
