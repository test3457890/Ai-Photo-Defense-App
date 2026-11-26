# AI Photo Defense development handoff

Updated 2026-11-26 by Claude (Opus 5.5). Start here. This is a working development increment, not completion of the full brief in [docs/MASTER_SPECIFICATION.md](docs/MASTER_SPECIFICATION.md). Requirement-by-requirement status is in [docs/IMPLEMENTATION_MATRIX.md](docs/IMPLEMENTATION_MATRIX.md).

## Starting point and audit

- Starting commit: `499728455a726db95da3cd901e4db2b37ca0ee25` (main, "Merge PR #1"). Branches at start: `main`, `development/foundation` (already merged), five open Dependabot branches (rfd 0.18, safetensors 0.8, sha2 0.11.1, checkout 7.0.1, upload-artifact 7.0.2) left untouched.
- `gh` was not authenticated on the working machine, so open PR metadata could not be read through the API; branches were inspected with git.
- Baseline on Windows 10 / Rust 1.100.0: `cargo check`, 13 tests, clippy `-D warnings` and fmt all passed.

### Problems found in the existing (ChatGPT) implementation

1. **Part 2 absent.** No cropping, rotation, resize, selection, masks, blur, pixelation or redaction existed anywhere; no Privacy Scan, no profiles, no batch folder handling, no final verification.
2. **Re-opened exports decoded without limits.** `photo_core::decode_export` used `image::load_from_memory` with default limits; it now uses the same bounded decoder as imports (`decode_bounded`).
3. **Export did not verify its own output.** The desktop wrote a JSON claiming metadata removal without re-inspecting the bytes. Exports now pass through `photo_pipeline::verify`, which reopens the encoded file and inspects container metadata, codes and redaction pixels.
4. **Hidden colour under transparency.** Fully transparent pixels kept their original RGB in PNG/WebP/TIFF exports. `photo_edit::flatten` now zeroes them and verification checks it.
5. **Name collisions failed batches.** Two inputs with the same stem made the second export fail; `photo_core::save_unique` now picks `-2`, `-3`… atomically without ever overwriting.
6. **Unused dependency** (`photo-engine` in the CLI without the experimental feature) produced a manifest warning; it is now optional behind the feature.
7. **Workspace crates publishable by default** (cargo-deny wildcard failures); now `publish = false`.
8. Model manifest still conflates local-use approval with redistribution (`redistribution_approved`). **Not fixed**; recorded as a blocker in docs/research/MODEL_LICENSES.md.

No fabricated results were found in the existing work; its documentation correctly labelled protection as unvalidated.

## What this increment added

| Crate | Purpose |
|---|---|
| `photo-edit` | Non-destructive `EditStack` (undo/redo, serializable recipes), crop/aspect crop/resize/fit/rotate/straighten/flip, binary masks (rect, ellipse, polygon/lasso, brush, union, grow/shrink), blur/pixelate/mosaic visual concealment, opaque secure redaction, flattening |
| `photo-privacy` | Container metadata inspector (JPEG/PNG/WebP/TIFF + EXIF fields via kamadak-exif), QR/barcode detection with tiling (rxing), 45+ sensitive-text and credential rules with checksum validation, 12 privacy profiles with JSON import/export, honest detector-availability reporting |
| `photo-pipeline` | Secure export pipeline, independent re-open verification, publish-or-block, batch collection (no symlinks, containment, depth/count limits), duplicate detection, review queue vs explicit auto-redact, cancel/pause, polling folder monitor |
| `photo-cli` | New `scan`, `process`, `batch` (`--recursive`, `--auto-redact`, `--watch`, `--summary`), `profiles` |
| `photo-desktop` | Rewritten: 8 workspaces across 3 modes, drag-to-select redaction, scan overlays, click-to-locate, dismiss, conceal located findings, undo/redo, crop/rotate/straighten/flip/resize presets, verified export with explicit "export anyway" for reviewed findings |

## Verified locally (Windows 10 Home 19045, x86-64, Rust 1.100.0, MSVC 14.52, CPU only)

- `cargo fmt --all --check`: pass.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: pass.
- `cargo test --locked --workspace`: 46 passed, 0 failed (core 6, edit 10, privacy 16, pipeline 6, cli integration 3, engine 2, models 2, lab 1).
- `cargo audit`: 0 vulnerabilities; informational warnings only. New since start: RUSTSEC-2026-0543 (chrono unmaintained, via rxing). Pre-existing: lru unsound (via iced text stack), several unmaintained crates.
- `cargo deny check` (new `deny.toml`): advisories ok, bans ok, licenses ok, sources ok.
- GUI runtime on Windows (synthetic CC0 fixture with a fake Wi-Fi QR): import, Ctrl+P scan found and boxed the QR (Confirmed, Wi-Fi credentials), Conceal applied opaque redaction, drag selection + Enter redacted a region, Ctrl+Z/redo, Ctrl+E export reported "verification passed", independent CLI re-scan of the exported file found 0 findings, original file unchanged. Screenshots in docs/screenshots (privacy-scan, edit-redact, export-verified, technical-ai-defense) are captures of this run.
- Two GUI defects were found by that runtime test and fixed: a profile that wanted QR codes ignored codes classified as Wi-Fi credentials; edit-canvas overlays and pointer mapping were wrong when the window was smaller than the preview.

See the final section of this file for results of the all-features and release builds.

## Not done / blocked (do not present as working)

- **Adversarial protection**: still no validated objective; presets are shown as unavailable. Needs licensed diffusion/face models and a real attack harness (Blocked).
- **OCR, face, person, object, document, plate, screen, reflection and open-vocabulary detection**: no verified models; the scan explicitly lists them as NOT examined (Blocked on model licensing; see docs/research/MODEL_LICENSES.md).
- Polygon, lasso and brush selection exist in the engine but cannot yet be drawn in the GUI. No rule-of-thirds overlay, no layer panel, no edit-canvas zoom.
- Watermarking, TrustMark, C2PA, SQLite history, settings persistence, docking, GPU, model downloads, updates, fuzz harness, sandboxed decoding, ICC colour management, lossy WebP, HEIC/AVIF/RAW.
- macOS and Linux runtime tests of this increment; Windows ARM64/Linux ARM64 CI targets.
- Signed installers: no signing credentials; CI artifacts remain unsigned development builds.

## Continue in this order

1. Read CI results for the PR that carries this increment; fix any platform failures before merging.
2. Split model manifest local-use vs redistribution; pick and licence-review an OCR model (ocrs/rten) and a face detector; integrate behind the existing `ScanInput.ocr_text` and a new detector trait, keeping the "not examined" reporting.
3. Add GUI drawing for polygon/lasso/brush selections (engine already supports them) and edit-canvas zoom.
4. Begin the matched-budget defense evaluation protocol in docs/TESTING.md before enabling any protection preset.
5. Add cargo-fuzz targets for `metadata::inspect`, `decode_bounded` and text rules.

## Collaboration rules

Record the starting commit, files changed, checks and blockers. Preserve others' work; never force-push. Keep this file, ROADMAP.md and IMPLEMENTATION_MATRIX.md accurate. Never fabricate benchmarks or screenshots. No private photos, credentials or restricted weights in the repository; all fixtures here are synthetic.
