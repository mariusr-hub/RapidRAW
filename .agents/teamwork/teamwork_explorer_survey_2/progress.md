# Progress Log - teamwork_explorer_survey_2

Last visited: 2026-10-29T15:31:00Z
Status: In progress

- [x] Initialized workspace and tracking files (DISPATCH.md, BRIEFING.md, progress.md)
- [x] Read ORIGINAL_REQUEST.md and reference plan
- [x] Inspect Display & Stability Fixes (R1): tauri.conf.json, gpu_processing.rs, Tauri lifecycle
  - Found `"transparent": true` in `tauri.conf.json:21` (causes DWM flicker)
  - Found `wgpu::PresentMode::Fifo` in `gpu_processing.rs:313`
  - Found `lib.rs:2255` & `lib.rs:2547` lack `WindowEvent::CloseRequested` handling
- [x] Shader comparison (R5):
  - Both `src-tauri/crates/rapidraw-gpu/src/shader.wgsl` and `src-tauri/src/shaders/shader.wgsl` have SHA256 `B8B2B841D756BECE1A287C21674DAE359552ABD92BE4C43EBACB66D756E47EEE` (byte-for-byte identical)
- [/] Inspect Image Pipeline & Shader (R5): IQbQr luma, blue-noise shadow dither, ai_denoise.rs
  - `ai_denoise.rs:293` currently uses Rec.709 `0.2126*r + 0.7152*g + 0.0722*b` in `apply_ai_denoise` instead of IQbQr
  - `shader.wgsl` defines `rgb_to_iqbqr` and `iqbqr_to_rgb` at line 1694/1701 but never calls them
  - `shader.wgsl:488` uses pseudo-random white noise hash instead of blue-noise shadow dither
- [/] Inspect Sensor conditioning routines & CFA checks (Fuji X-Trans, Linear DNG)
  - `condition_raw_bayer_sensor` in `raw_processing.rs:377` called unconditionally without `is_bayer()` or checking linear DNG
- [ ] Check Upstream v1.6.7 parity (dual illuminants, ACES gamut compression, dark channel dehaze, PR #2004/#2005)
- [ ] Competitor brand name & disassembly audit
- [ ] Synthesize findings, update BRIEFING.md, and write handoff.md
