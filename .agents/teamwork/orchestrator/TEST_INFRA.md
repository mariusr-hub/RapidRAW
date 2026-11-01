# E2E Test Infra: RapidRAW Master Plan v2.8

## Test Philosophy
- Opaque-box, requirement-driven.
- Methodology: Category-Partition + Boundary Value Analysis + Combinatorial + Real-World Workload Testing.

## Feature Inventory
| # | Feature | Source | Tier 1 | Tier 2 | Tier 3 |
|---|---------|--------|:------:|:------:|:------:|
| 1 | Dark TIFF Export Tone-Mapping Parity | R1 | 5 | 5 | ✓ |
| 2 | Safe Buffered I/O on Removable Media | R2 | 5 | 5 | ✓ |
| 3 | Linear DNG Direct TIFF Decoding | R2 | 5 | 5 | ✓ |
| 4 | Large Folder EXIF Batch Store | R2 | 5 | 5 | ✓ |
| 5 | Real-Time Synthesis Progress Broadcasting | R3 | 5 | 5 | ✓ |
| 6 | Window Non-Transparency & Solid Background | R4 | 5 | 5 | ✓ |
| 7 | Left Sidebar Navigation Dual-Tab Switcher | R5 | 5 | 5 | ✓ |
| 8 | WB Dropper 5500K Neutral Fallback | R6 | 5 | 5 | ✓ |
| 9 | Auto-Crop Pixel Coordinate Scaling | R6 | 5 | 5 | ✓ |
| 10 | Competitor Brand Sanitization | R7 | 5 | 5 | ✓ |
| 11 | WGSL Shader Byte-for-Byte Synchronization | R7 | 5 | 5 | ✓ |
| 12 | Release Installer Packaging | R8 | 5 | 5 | ✓ |

## Test Architecture
- Test Runner:
  - `cargo test --workspace`: Comprehensive Rust backend tests across unit and integration targets.
  - `npm run typecheck`: Strict TypeScript compile verification.
  - `npm run tauri build`: Final production release packaging.
- Directory Layout:
  - `src-tauri/tests/`: Integration tests (`test_save_formats.rs`, `test_linear_dng.rs`, `test_shader_parity.rs`).

## Coverage Thresholds
- Tier 1: Feature Coverage (>=5 test cases per feature area)
- Tier 2: Boundary & Corner Cases (>=5 test cases per feature area)
- Tier 3: Cross-Feature Combinations (Pairwise coverage)
- Tier 4: Real-World Application Workloads
