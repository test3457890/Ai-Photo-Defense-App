# Master implementation checklist

Tracks every requirement in [MASTER_SPECIFICATION.md](MASTER_SPECIFICATION.md). Updated 2026-11-26 by Claude at the end of the increment that started from commit `499728455a726db95da3cd901e4db2b37ca0ee25`.

Status meanings: **Not started**, **In progress**, **Implemented** (code exists, compiles), **Tested** (automated tests and/or observed runtime on the stated platform), **Independently verified** (confirmed by a reviewer other than the author: nothing is marked this yet), **Incomplete** (partial, gaps listed), **Blocked** (cannot proceed without an external decision, licence, model or credential).

Implementation, compilation and verification are different things. "Tested" names where it was tested.

## Platform and architecture (§4, §5, §28, §30)

| Requirement | Status | Evidence / gap |
|---|---|---|
| Rust-native desktop app (Iced), no web UI, no Python/Node at runtime | Tested (Windows 10 runtime, Linux runtime in earlier increment) | `crates/desktop`; language audit in HANDOFF.md |
| Modular Cargo workspace with shared engine | Implemented | 9 crates: core, edit, privacy, pipeline, engine, models, lab, cli, desktop |
| CLI | Tested | `photo-defense` commands inspect, scan, process, batch, profiles, export, compare, lab, verify-model, protect (refuses) |
| Developer SDK | Incomplete | Public Rust crate APIs, documented in ARCHITECTURE.md; no stable versioned SDK |
| Windows x64 | Tested (local build + GUI runtime on Windows 10) | Screenshots in docs/screenshots |
| Windows ARM64, Linux ARM64 | Not started | No CI target yet |
| macOS Intel / Apple Silicon | Incomplete | CI compile + unit tests at earlier commit; no runtime test |
| Linux x64 | Incomplete | CI compile/test; GUI runtime tested in earlier increment only |
| CPU-only operation | Tested | All features run on CPU |
| GPU acceleration (CUDA/Metal/Vulkan), hardware detection, resource limits UI | Not started | Fixed pixel/file/allocation limits only |
| Local database (rusqlite) | Not started | History is in-memory by design until a privacy-reviewed schema exists |
| Update management, signed updates | Not started | App makes no network requests |

## Part 1 – AI protection engine (§10, §11)

| Requirement | Status | Evidence / gap |
|---|---|---|
| Joint multi-objective projected-gradient optimizer with shared budget | Tested (unit tests) | `photo-engine`; no validated objectives |
| Experimental CLIP encoder objective | Tested (gradient + PyTorch parity on one input, earlier increment) | Opt-in CLI feature; not a protection claim |
| Protection against editing / inpainting / img2img | Blocked | Needs licensed diffusion encoder + attack harness |
| Face swap / identity cloning / recognition training resistance | Blocked | Needs licensed face models, consenting dataset |
| Training resistance / style imitation | Blocked | Needs training-based evaluation protocol |
| Compression/crop/resize/screenshot/denoise/purification resilience | Incomplete | Pixel-survival lab only (8 transforms); no model-level measurement |
| Quick / Balanced / Maximum / Custom presets | Blocked | Shown in UI as "unavailable (not measured)"; no fake percentages |
| Research repository audit | Incomplete | docs/research/AUDIT.md (earlier increment); licences of model weights still unverified |

## Part 2 – Photo editing (§12)

| Requirement | Status | Evidence / gap |
|---|---|---|
| Non-destructive edit stack rendered from untouched original | Tested | `photo_edit::EditStack`; unit tests confirm original untouched |
| Freeform crop, fixed aspect crop (Original, 1:1, 4:3, 3:2, 16:9, 9:16, 4:5, custom), custom pixel crop | Tested | Unit tests; GUI pick list + crop-to-selection; CLI `--aspect`, `--crop` |
| Resize, fit-within social presets | Tested | `Resize`, `FitWithin`; GUI "Resize for platform"; CLI `--fit` |
| Rotate 90/180/270, straighten with inscribed-rectangle crop | Tested | Unit tests; GUI buttons and slider |
| Flip horizontal / vertical | Tested | Unit tests, GUI |
| Undo / redo / editing history | Tested | Unit tests; GUI Ctrl+Z / Ctrl+Y observed on Windows |
| Zoom and pan | Incomplete | Comparison viewers zoom/pan; edit canvas is fit-to-window only |
| Rectangle / ellipse selection | Tested | Mouse drag on Windows GUI, numeric fields, CLI |
| Polygon / freeform lasso / brush selection | Incomplete | Engine + tests (`Selection::Polygon`, `Brush`); not yet drawable in GUI |
| Object selection | Blocked | Needs segmentation model |
| Layered masks, mask refinement | Incomplete | Union/subtract/intersect/grow/shrink in engine; no layer panel |
| Rule-of-thirds guides, aspect lock while dragging | Not started | |
| Original vs edited comparison | Tested | Batch Export workspace (original vs reopened export, difference x8) |
| Never overwrite original | Tested | No-clobber publish; tests compare source bytes |

## Blur and secure redaction (§13)

| Requirement | Status | Evidence / gap |
|---|---|---|
| Gaussian blur (adjustable), pixelation, mosaic | Tested | Unit tests; GUI strength slider |
| Solid black / white / custom-colour opaque redaction | Tested | Unit tests; CLI `--redact-color` |
| Visual concealment vs secure redaction clearly distinguished | Tested | Method labels, warnings in UI and export report |
| Exported files keep no original pixels under redaction; flattening | Tested | Pipeline test reopens exported bytes and checks every masked pixel; transparent pixels' colour zeroed |
| Background blur / concealment | Tested | `invert` selection |
| Selective face blur, full person, screen, document concealment | Blocked | Needs detection models |
| Manual brush concealment | Incomplete | Engine supports brush; GUI drawing pending |

## Privacy Scan (§14–§18)

| Requirement | Status | Evidence / gap |
|---|---|---|
| Metadata inspection: EXIF, GPS, device/serial, timestamps, XMP, IPTC, ICC, comments, PNG/WebP chunks, thumbnails, MPF, trailing data/embedded images, multi-page TIFF, animation | Tested | `photo_privacy::metadata`; fuzz-style malformed-input test |
| QR, Data Matrix, PDF417, Aztec, 1-D barcode detection, local decode, never opened | Tested | rxing; tiled scan finds small codes in large images |
| Classification of auth (otpauth), Wi-Fi, payment, AAMVA licence, boarding pass codes | Tested (otpauth, Wi-Fi); Implemented (others) | |
| Sensitive-text and credential rules (45+ rules), evidence levels Confirmed/Pattern/Contextual, checksum validation (Luhn, IBAN, ABA, ICAO MRZ, SIN) | Tested | `photo_privacy::text`; applied to decoded codes, metadata text and supplied text |
| Country-specific rules (US, GB, CA) | Tested | |
| Findings never contain the value | Tested | Serialization tests assert secrets absent |
| OCR of text in pixels | Blocked | No licensed OCR model installed; UI and reports state text was NOT read |
| Face / person / object / document / screen / plate / reflection detection | Blocked | No verified models; reported as not examined |
| Open-vocabulary custom object detection | Blocked | Needs Grounding-DINO-class model |
| Tiling for small objects | Incomplete | Implemented for codes only |
| Privacy profiles (12 built-in + custom JSON import/export) | Tested | `photo_privacy::profiles`, CLI `profiles`, `--profile-file` |
| Scan UI: boxes, results list, click to locate, dismiss false positives, conceal located, not-examined list | Tested (Windows GUI) | screenshots/privacy-scan.png |
| No auto-redaction without approval | Tested | GUI requires click; batch requires `--auto-redact` |
| Detection accuracy metrics | Not started | No labelled dataset yet |

## Authenticity (§20)

| Requirement | Status |
|---|---|
| Cryptographic file hashes (SHA-256, BLAKE3) in reports | Tested |
| Invisible watermarking, TrustMark, C2PA signing/verification | Not started (reports state "Watermark: not applied") |

## Secure export pipeline and final verification (§21, §22)

| Requirement | Status | Evidence / gap |
|---|---|---|
| Ordered pipeline: preserve, inspect, scan, edit, redact, flatten, re-scan, encode metadata-free, reopen and verify | Tested | `photo_pipeline::prepare/verify/publish` |
| Adversarial protection step | Blocked | Recorded as "not applied" |
| Watermark / signing steps | Not started | |
| Verification: metadata, remaining codes/credentials, redaction completeness, transparency, decoded-vs-render quality | Tested | Export blocked when verification fails unless explicitly accepted |
| Verification of faces / readable text | Blocked | Needs models |
| Reports exclude sensitive values | Tested | |

## Batch processing (§23)

| Requirement | Status | Evidence / gap |
|---|---|---|
| Files, folders, recursion, extension filter, output dir excluded | Tested | |
| Symlink/junction not followed; resolved-path containment | Tested (Unix symlink test; Windows uses same `symlink_metadata` + canonical check) | Windows junction test pending |
| Duplicate detection (BLAKE3) | Tested | |
| Review queue (default) vs explicit auto-redact | Tested | |
| Shared crop/resize in batch | Implemented | CLI edit flags apply to every file |
| Cancel, pause/resume (in-process), error recovery, summary | Tested (cancel, errors) / Implemented (pause) | |
| Folder monitoring | Tested | Polling monitor; CLI `--watch` |
| Safe unique output names | Tested | `save_unique` |
| Desktop batch | Implemented | Collection limit 64; folders expanded with safe walker |

## Formats (§24)

| Format | Status |
|---|---|
| JPEG, PNG, WebP, TIFF (8-bit) import/export | Tested |
| WebP export | Lossless only (image crate encoder) |
| BMP, AVIF, HEIC/HEIF, RAW, 16-bit | Not supported (documented) |
| ICC colour management | Not started (profile dropped; warned) |

## Models, Defense Lab, security (§25–§27)

| Requirement | Status |
|---|---|
| SafeTensors verification against reviewed manifest; pickle refused | Tested |
| Model library UI | Incomplete (honest status page; no install/remove/download) |
| Defense Lab transformation measurements, report saving | Tested |
| AI attack harness, benchmarks vs research projects | Blocked |
| Bounded decoding for imports and re-opened exports | Tested |
| No-clobber atomic writes, private temp files | Tested |
| Path traversal / unsafe names | Tested (`save_unique` rejects unsafe stems) |
| Dependency scanning (cargo audit, cargo deny) | Tested locally; CI workflow updated |
| SBOM | Implemented (scripts/sbom.py, CI) |
| Threat model | Incomplete (docs/THREAT_MODEL.md, earlier increment) |
| Fuzzing harness | Not started (hostile-input unit tests only) |
| Sandboxed decoding | Not started |
| `unsafe` in project code | None (`grep unsafe crates` returns nothing) |

## Interface (§7–§9)

| Requirement | Status |
|---|---|
| Essential / Professional / Technical modes sharing one collection and edit stacks | Tested (Windows GUI) |
| Workspaces: Batch Export, Privacy Scan, Edit and Redact, AI Defense, Protection History, Defense Lab, Model Library, Settings | Tested (Windows GUI) |
| Menus, keyboard shortcuts (Ctrl+O/E/P/Z/Y/1/2/3, Enter, Esc, Tab) | Tested (Ctrl+P, Ctrl+E, Ctrl+Z, Ctrl+3, Enter observed) |
| Dockable panels, context menus, persisted settings | Not started |
| Screen-reader accessibility | Not tested |

## Releases (§36)

| Requirement | Status |
|---|---|
| Signed installers (MSI/EXE, DMG, AppImage/DEB/RPM) | Blocked: no signing credentials, runtime testing incomplete. CI artifacts are unsigned development builds. |
