## 2026-10-30T22:43:54Z
Sender: 5a72144f-8be1-4295-9cb2-a3907b4e00a6
Priority: MESSAGE_PRIORITY_HIGH

You are teamwork_explorer_survey_v28_2.
Your working directory is: d:\RapidRAW_V.03\.agents\teamwork\teamwork_explorer_survey_v28_2\

MANDATORY FIRST STEP:
Read d:\RapidRAW_V.03\.agents\teamwork\ORIGINAL_REQUEST.md completely.
Also refer to reference plan at: C:\Users\Kibernetinė Jungtis\.gemini\antigravity\brain\9e9e86f5-f613-480e-9135-284f0b725438\radiant_photo_instant_enhancement_ui_plan.md

Objective:
Investigate requirement R2 (Large Folder Crash Prevention & Linear DNG Support):
1. In `src-tauri/src/file_management.rs`:
   - Inspect `read_file_mapped` and usages of `mmap`.
   - Analyze how Windows drive types (`DRIVE_REMOVABLE`, `DRIVE_REMOTE`) can be detected or how safe buffered `fs::read` fallback should be structured to prevent `0xc0000006` STATUS_IN_PAGE_ERROR.
   - Inspect `prefetch_source_file` and why it should be eliminated.
2. In `src-tauri/src/image_loader.rs` and `src-tauri/src/raw_processing.rs`:
   - Inspect `rawler` decoding path and where `cpp == 1` vs `cpp == 3` panics happen with Linear DNGs.
   - Trace how to probe/detect 3-channel Linear DNGs before demosaicing and route them to standard TIFF reader (`tiff::decoder` / `image::load_from_memory`).
   - Check how thumbnails and full images are loaded for Linear DNG.
3. In `src/hooks/useAppNavigation.ts`:
   - Inspect `fetchExifInChunks` and identify array re-cloning loops on large folders.
   - Design the batched key-value metadata store pattern to eliminate V8 GC thrashing.

Deliverable:
Write a comprehensive, structured handoff report to `d:\RapidRAW_V.03\.agents\teamwork\teamwork_explorer_survey_v28_2\handoff.md` with:
- Exact file paths and line numbers
- Root cause diagnosis and precise code diff suggestions
- Step-by-step implementation plan for worker
- Test fixtures and verification strategy (including synthetic Linear DNG unit test)

Update your `progress.md` regularly while working.
When done, notify the orchestrator with `send_message`.
