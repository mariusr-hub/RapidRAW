## 2026-10-29T15:21:53Z
You are teamwork_explorer_survey_1.
Your working directory is: d:\RapidRAW_V.03\.agents\teamwork\teamwork_explorer_survey_1\
The authoritative request is located at: d:\RapidRAW_V.03\.agents\teamwork\ORIGINAL_REQUEST.md
Also see reference plan: C:\Users\Kibernetinė Jungtis\.gemini\antigravity\brain\9e9e86f5-f613-480e-9135-284f0b725438\radiant_photo_instant_enhancement_ui_plan.md

MANDATORY: Read d:\RapidRAW_V.03\.agents\teamwork\ORIGINAL_REQUEST.md first.

Your mission is: Conduct a deep, read-only architectural and code survey of the frontend requirements (R2, R3, R4) in d:\RapidRAW_V.03:
1. Navigation & Dual-Dock Restoration (R2):
   - Inspect `src/App.tsx`, `src/components/panel/left/TonesPanel.tsx`, and `FolderTree` usage.
   - Analyze how the Left Sidebar is currently structured, how TonesPanel replaces or coexists with FolderTree, and design the dual-tab bar `[ 📁 Folders & Sources ]` vs `[ ✨ HDR & Styles ]`.
2. White Balance & Auto-Crop Coordinate Fixes (R3):
   - Inspect `src/components/panel/editor/ImageCanvas.tsx`: analyze `applyWbPick`, EXIF metadata handling (`asShotWhiteBalance`), and pointer event handling (SVG overlay interception issues).
   - Inspect `src/components/panel/right/CropPanel.tsx`: analyze `handleCompositionAi`, how crop coordinates are handled (normalized percentage 0-100% vs image pixel dimensions), and canvas state synchronization.
3. UI/UX Feature Consolidation & Brand Sanitization (R4):
   - Inspect `ControlsPanel.tsx`: locate `ReferencePresetsRibbon` and see how to remove it.
   - Inspect Toolbar and `Basic.tsx`: locate competing "Auto" buttons, how to consolidate into single primary "Smart Auto Enhance" (subtle/medium/vivid) and baseline "Auto Tone" in `Basic.tsx`.
   - Inspect `Color.tsx`: locate duplicate White Balance sliders and see how to cleanly remove them so WB is exclusively in `Basic.tsx`.
   - Inspect `Details.tsx` and `DenoiseModal.tsx`: analyze how to unify Noise Reduction into a segmented switch `[ Standard / BM3D ]` vs `[ Neural AI Denoise ]` and retire `DenoiseModal.tsx`.
   - Search frontend files for competitor trademarks: "SNS-HDR", "Radiant Photo", "Radiant Vivid", "Luminar Neo", "DxO", "AKVIS", `SNS_HDR_PROFILES`, `RADIANT_CREATIVE_LOOKS`, etc.

Do NOT modify any code. You are read-only.
Write a comprehensive, structured handoff report to:
`d:\RapidRAW_V.03\.agents\teamwork\teamwork_explorer_survey_1\handoff.md`
including concrete file paths, exact lines of interest, potential pitfalls, and implementation recommendations.
When finished, send a brief message to your caller (orchestrator) reporting completion and the path to your handoff.md.
