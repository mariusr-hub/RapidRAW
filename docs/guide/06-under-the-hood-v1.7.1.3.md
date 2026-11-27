# 06. Under-the-Hood, Performance & Reliability (`v1.7.1` → `v1.7.1.3`)

[← Export, Delivery & Integrations](05-export-delivery-integrations.md) · [Back to Overview →](../README.md)

---

Beyond visible panels and modals, **RapidRAW `v1.7.1.3`** includes major color-science, GPU, memory-management, and reliability upgrades over official **`v1.7.1`**:

## 1. Color Science & Optical Pipeline

| Upgrade | What Changed Since Official `v1.7.1` | End-User Benefit |
| --- | --- | --- |
| **Neutral Single-Photo Color Fidelity** | Restored neutral default rendering parity across CPU/GPU adjustment uniforms and shaders (`shader_check.rs`, `uniforms.rs`, `shader.wgsl`). | Single RAW photos open with faithful, neutral color—no unintended cast or contrast shift before you start editing. |
| **DNG `OpcodeList3` Optics** | Full support for embedded DNG `OpcodeList3` geometric/vignette/warp corrections. | Smartphones, drones, and Leica/Pentax DNGs render with accurate lens geometry automatically. |
| **GPU DCP `@binding(7)`** | Hardware-accelerated `.dcp` camera profile lookup tables directly in the `wgpu` shader pipeline. | Switch camera color profiles in real time with zero CPU bottleneck. |
| **Half-Float DNG & UltraHDR Safety** | Clamps linear radiance to `f16::MAX` before encoding float DNGs and sanitizes `NaN`/`Inf` pixels before `gain_map::attach`. | Prevents corrupted pixels or crashes when saving extreme-DR HDR merges and Ultra HDR JPEGs. |
| **Color Parity (`to_srgb8` ↔ `to_srgb16`)** | Unified `OutputTransform::srgb(false)` and `OutputColorSpace::Srgb` ICC embedding across 8-bit and 16-bit export paths. | 8-bit web JPEGs and 16-bit print TIFFs match in tone and color. |

---

## 2. AI Memory, GPU Stability & Catalog Speed

| Upgrade | What Changed Since Official `v1.7.1` | End-User Benefit |
| --- | --- | --- |
| **ONNX LRU Session Reaper** | Automatically evicts idle AI ONNX Runtime sessions under an LRU policy. | Prevents VRAM/RAM exhaustion when switching between Denoise, Subject/Sky masks, Depth, and Upscale. |
| **DirectML Stall Guard & Cancel** | Guards Windows DirectML execution against driver timeouts and supports clean mid-task cancellation. | Keeps Windows systems responsive during heavy AI denoise runs and lets you cancel instantly. |
| **SQLite WAL Catalog Engine** | Concurrent Write-Ahead Logging SQLite database (`catalog_db.rs`) for metadata, ratings, EXIF, and Smart Collections. | Instant folder switching, sorting, and filtering—even on libraries with thousands of RAWs. |
| **Auto-Paused CLIP Indexing** | Background semantic indexing automatically yields when foreground editing or culling is active. | Smooth 60 FPS slider dragging and zooming even while new folders are being indexed. |

---

## 3. Workflow Safety & Contract Guards

| Upgrade | What Changed Since Official `v1.7.1` | End-User Benefit |
| --- | --- | --- |
| **Atomic File Output & Sleep Lock** | Writes exports/merges atomically (`atomic_output.rs`) and holds an OS sleep lock (`sleep_lock.rs`) during long batches. | Your PC won't sleep mid-export or leave half-written corrupted files on disk. |
| **Photo Context Menu & Merge Parity** | Restored Panorama/HDR context-menu visibility and unified `MergeOutput.scene_exposure` from `Rendition`. | Right-click multi-selection merges are always discoverable and expose accurately. |
| **20 Automated Contract Guards** | `scripts/verify-contracts.mjs` enforces adjustment-key parity, neutral color defaults, modal cancel domains, and React lifecycle hygiene on every build. | Eliminates regressions across panels, undo history, and Rust/TypeScript IPC boundaries. |
