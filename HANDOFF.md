# AI Photo Defense development handoff

## Part 3 increment (2026-11-26, Claude Opus 5.5)

Branch `claude/part3-studio`, stacked on `claude/part2-privacy-editing` (PR #7, open and not yet merged). Starting commit `7b0e8b22b11b7191632974552298ec539e5ac51a`. Specification: [docs/PART3_SPECIFICATION.md](docs/PART3_SPECIFICATION.md); status: [docs/IMPLEMENTATION_MATRIX.md](docs/IMPLEMENTATION_MATRIX.md#part-3).

### Problems found in Parts 1-2 during this review

1. The Part 2 desktop interface was a tabbed dashboard: the three modes only changed which tabs were visible, controls were oversized and rounded, there was no menu bar, histogram, filmstrip, adjustments or resizable panels, and at 125% scaling the window exceeded the work area (status bar hidden). Evidence: earlier screenshots `docs/screenshots/privacy-scan.png` etc.
2. No structured logging, no crash recovery, no projects: a crash lost all work.
3. No colour correction, layers, RAW handling, catalog or forensic tools.
4. Iced 0.13's canvas mispositions geometry inside pane grids (found while validating the redesign); overlays now rasterise.
5. Security finding introduced and fixed during this increment: the first catalog build used rusqlite 0.38 whose bundled SQLite had RUSTSEC-2026-0459 (CVE-2026-119375, medium). Upgraded to rusqlite 0.43 (libsqlite3-sys 0.40); `cargo audit` reports no vulnerabilities.

### What was added

| Crate / area | Content |
|---|---|
| `photo-log` (new) | One allowlisted structured logger: stable APD codes, session/job ids, durations, recovery status; sanitiser masks paths, numbers, tokens, URLs, e-mails, credentials and strips control/bidi characters; JSON lines, rotation, retention, size cap, clear, sanitized report, panic hook without memory dumps |
| `photo-project` (new) | Projects: atomic saves, `.bak` + checkpoints, autosave journal, corrupt-file fallback, session lock for unclean-shutdown detection, temp cleanup |
| `photo-catalog` (new) | SQLite Media Manager: photos, tags, ratings, flags, albums, parameterised filters, duplicates, missing files; optional thumbnail cache with overwrite-then-delete clearing |
| `photo-edit` | `color` (linear-light WB/exposure, tone, curves, HSL, grading, LUT .cube import/export, presets, histogram, clipping, auto tone), `layers` (adjustment layers with soft masks, blend modes, opacity, visibility, ordering; perspective, lens, canvas; shape fills), `proxy` (scaled preview rendering); redaction layers always render on top |
| `photo-core` | `raw`: RAW identification and embedded-preview extraction (DNG, CR2, CR3, NEF, ARW, RAF, ORF, RW2, PEF) |
| `photo-privacy` | `forensics`: read-only inspection (hashes, EXIF listing, JPEG tables/quality estimate, C2PA presence reported as unverified, heuristic indicators labelled as not proof) |
| `photo-pipeline` | `pre_export_review` for high-assurance export; RAW extensions in safe batch collection |
| `photo-desktop` | Complete redesign: menus, Essential processing list, Professional dockable panes (Library, Viewer, Inspector, Filmstrip), Technical tool pages (Diagnostics, Performance, Validation, Forensics, Lab, Models, History), autosave/restore banner, safe mode |
| CLI | `forensics`, `raw-preview` |

### Verified on Windows 10 x64 (Rust 1.100.0)

- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings` (and `--all-features`): pass.
- `cargo test --locked --workspace`: 73 passed (74 with `--all-features`).
- `cargo audit`: no vulnerabilities (informational: chrono unmaintained via rxing; lru unsound and unmaintained crates via Iced). `cargo deny check`: pass.
- `cargo build --release --workspace`: pass.
- Real GUI runs with screenshots in `docs/screenshots/part3-*.png`; defects found and fixed are listed in `docs/design/PART3_DESIGN.md`. Forced termination then restart showed the autosave restore banner.
- Logging privacy: unit tests feed synthetic paths, card numbers, tokens, Wi-Fi and OTP payloads and injection sequences; none reach files or reports.
- Benchmarks: `docs/BENCHMARKS.md` (measured, release build).

### Not done or blocked (never present as working)

RAW development (LGPL licensing decision), adversarial protection (no validated model), model-based detection/segmentation/OCR, intelligent selection, text layers, colour wheels/profile conversion/HDR, C2PA verification, GPU acceleration, accessibility (no AccessKit in Iced 0.13), OS-native menus and full context menus, persisted settings/pane layout, portable project archives, disk-exhaustion and GPU-failure recovery tests, fuzz harness, competitive benchmarks, macOS/Linux runtime testing, signed installers.

### Suggested next steps

1. Merge PR #7 after CI, then retarget the Part 3 PR to `main`.
2. Owner decision on RAW decoder licensing (rawler/rawloader LGPL in a separate helper process is one option).
3. Evaluate Iced 0.14 for AccessKit and fix the canvas issue upstream; then restore vector overlays.
4. Add cargo-fuzz targets for RAW previews, metadata, LUT and project parsing.

---


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
