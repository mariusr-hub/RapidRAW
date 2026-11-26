# RapidRAW Workspace Agent Rules

These invariants are mandatory for every agent, planner, executor, and subagent operating in this repository.

## 1. Frontend & React Lifecycle Invariants
- **No Unmounted-Panel `CustomEvent` Dispatch:** Right-hand panels (`Panel.Adjustments`, `Panel.Ai`, `Panel.Masks`, `Panel.Crop`, `Panel.Presets`) unmount when inactive in `src/App.tsx`. Never pair `setPanel(...)` with synchronous `window.dispatchEvent(...)`. Route cross-panel actions through `useUIStore.triggerQuickAction(...)` / `pendingQuickAction`.
- **`isForMask` Isolation:** Any component mounted both globally in Editor and inside `MasksPanel` (such as `src/components/adjustments/Basic.tsx`) must guard global store actions and `window` listeners with `if (isForMask) return;`.
- **`useEffect` Listener Hygiene:** Never call `window.addEventListener` or `document.addEventListener` inside a `useEffect` without an explicit dependency array.
- **Single History Push:** `setAdjustments(value, label)` in `useEditorActions` already pushes a labelled Undo entry. Never call both `setAdjustments(...)` and `useEditorStore.getState().pushHistory(...)` for the same user action.
- **Keybind Registry:** Register all user-facing shortcuts in `KEYBIND_DEFINITIONS` (`src/utils/keyboardUtils.ts`) with a `fallbackDescription` and handle them via `comboMap` in `src/hooks/useKeyboardShortcuts.ts`. Never hardcode bare `KeyA`–`KeyZ` checks inside `builtinShortcuts`.
- **Modal Cancel Domains:** Every modal cancel handler must invoke its own domain backend cancel command (e.g., `MergeModal.tsx` -> `Invokes.DiscardMerge`, never `Invokes.CancelAiDenoise`).

## 2. Rust Backend, Async I/O & Color Science Invariants
- **No Blocking Full-File Reads on IPC / Async Threads:** Never call `fs::read` / `std::fs::read` / `image::open` directly inside a sync `#[tauri::command] pub fn` or `pub async fn` outside `tokio::task::spawn_blocking`. For EXIF/header reads on RAW files, check `read_rrexif_sidecar` first and fall back to `crate::library::file_ops::read_file_mapped`.
- **Color Parity (`to_srgb8` ↔ `to_srgb16`):** `load_base_image_from_bytes` outputs linear Rec.2020 (`WORKING`). Display-encoded sRGB paths (`to_srgb8` / `working_display_to_srgb8` and `to_srgb16`) must both use `OutputTransform::srgb(false)` and embed `OutputColorSpace::Srgb.icc_profile()`.
- **`MergeOutput.scene_exposure`:** Never hardcode `scene_exposure: 0.0` in a `MergeOutput` literal when a `Rendition` is in scope; always pass `self.rendition.scene_exposure()`.
- **Half-Float DNG & UltraHDR Safety:** Clamp linear radiance to `half::f16::MAX.to_f32()` before `f16::from_f32` in `dng.rs`, and sanitize non-finite pixels (`NaN`/`Inf`) before `gain_map::attach`.

## 3. Versioning, Build & Windows Shell Rules
- **Version Parity:** Keep 3-part SemVer (`1.7.1`) in `src-tauri/Cargo.toml` and top-level `src-tauri/tauri.conf.json`. Keep 4-part display version (`1.7.1.1`) synchronized across `package.json`, `APP_DISPLAY_VERSION` (`src/components/panel/MainLibrary.tsx`), `bundle.windows.wix.version` (`src-tauri/tauri.conf.json`), and `src-tauri/nsis-hooks.nsh`.
- **Node Child Processes:** Never pass `shell: true` alongside an args array to `spawn`/`spawnSync` (`DEP0190`). Invoke JS CLIs via `process.execPath` with `shell: false`.
- **Windows PowerShell 5.1:** Do not chain commands with `&&` (use `; if ($LASTEXITCODE -eq 0) { ... }`). Always invoke `npm.cmd` (not bare `npm`). Run `cargo` and `npm.cmd run tauri:build` with `BypassSandbox: true`.
- **Mandatory Gate:** Always ensure `node scripts/verify-contracts.mjs` passes after edits and `cargo check --manifest-path src-tauri/Cargo.toml --bins` + `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --skip shaders_compile_on_gpu_backends` pass after any `.rs` edit.
