# BRIEFING — 2026-10-30T22:56:00Z

## Mission
Investigate R4 (Window Transparency & Background Stability), R5 (Left Sidebar Dual-Tab Switcher), R6 (White Balance Dropper & Auto-Crop Pixel Scaling), R7 (Brand Scrubbing & Shader Sync), and R8 (Verification and Release Packaging) for RapidRAW v0.3.

## 🔒 My Identity
- Archetype: explorer
- Roles: investigation, synthesis
- Working directory: d:\RapidRAW_V.03\.agents\teamwork\teamwork_explorer_survey_v28_3\
- Original parent: 5a72144f-8be1-4295-9cb2-a3907b4e00a6
- Milestone: Survey & Investigation (R4–R8)

## 🔒 Key Constraints
- Read-only investigation — do NOT implement
- Deliver structured handoff report in handoff.md
- Maintain liveness heartbeat in progress.md

## Current Parent
- Conversation ID: 5a72144f-8be1-4295-9cb2-a3907b4e00a6
- Updated: not yet

## Investigation State
- **Explored paths**:
  - R4: `src-tauri/tauri.conf.json`, `index.html`, `src/styles.css`, `src/App.tsx`, `src/components/panel/Editor.tsx`, `src-tauri/src/lib.rs`, `src-tauri/src/gpu_processing.rs`
  - R5: `src/App.tsx`, `src/components/panel/left/TonesPanel.tsx`, `src/components/panel/right/FolderTree.tsx`, `src/components/ui/AppProperties.tsx`, `src/store/useUIStore.ts`
  - R6: `src/components/panel/editor/ImageCanvas.tsx`, `src/components/panel/right/CropPanel.tsx`, `src/utils/whiteBalance.ts`, `src-tauri/src/defect_repair.rs`
  - R7: Full trademark scan (SNS-HDR, Radiant Photo, Luminar Neo, DxO, AKVIS) across TS/TSX, Rust files, WGSL shaders, tests. Both copies of `shader.wgsl` SHA256 verified.
  - R8: `package.json`, `tsconfig.json`, `scripts/with-stay-awake.ps1`, `src-tauri/Cargo.toml`, `npm run typecheck` (passed with 0 errors).
- **Key findings**:
  - R4: `tauri.conf.json` has `transparent: false`, but `styles.css:105, 115` has `html, body { background-color: transparent; }`, and `App.tsx:871` has `isWgpuActive ? 'bg-transparent' : 'bg-bg-primary'`. `index.html` lacks dark background. `WindowEvent::CloseRequested` missing in `lib.rs:2547`.
  - R5: `App.tsx:717-720` unconditionally replaces `FolderTree` with `<TonesPanel />` when `activeView === 'editor'`, locking users out of folder navigation. Solution: dual-tab switcher `[Folders & Sources]` and `[HDR & Looks]`.
  - R6: `ImageCanvas.tsx:2376` only applies WB if `asShotWhiteBalance` exists; missing fallback to neutral daylight 5500K (`{ temperature: 5500, tint: 0 }`). `CropPanel.tsx:263` puts raw percentage (0-100%) into `adjustments.crop`, which `Editor.tsx:360` treats as raw pixel dimensions (shrinking crop by 60x).
  - R7: Mapped all trademark locations in UI, comments, and identifiers. Both copies of `shader.wgsl` currently match SHA256 `B8B2B841D756BECE1A287C21674DAE359552ABD92BE4C43EBACB66D756E47EEE`. Add automated unit test `test_shader_wgsl_byte_identity`.
  - R8: `npm run typecheck` passed with 0 errors. Machine safety power guard in `scripts/with-stay-awake.ps1`.
- **Unexplored areas**: None. R4 through R8 fully analyzed.

## Key Decisions Made
- Design dual-tab switcher for Left Dock in Editor mode that toggles between `FolderTree` and `TonesPanel`.
- Default WB to neutral 5500K daylight when EXIF as-shot metadata is absent.
- Scale saliency crop percentage recommendations to actual pixel dimensions using `cropBaseWidth` and `cropBaseHeight`.
- Formulate comprehensive replacement table for trademarks across code, shaders, and tests.

## Artifact Index
- DISPATCH.md — Initial dispatch message
- progress.md — Liveness heartbeat
- BRIEFING.md — Working memory
- handoff.md — Comprehensive handoff report
