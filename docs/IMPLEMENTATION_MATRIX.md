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

## Part 3

Increment started from `7b0e8b22b11b7191632974552298ec539e5ac51a` (Part 2 branch, PR #7 open, not merged). Specification: [PART3_SPECIFICATION.md](PART3_SPECIFICATION.md). Same status vocabulary as above; nothing is "Independently verified" yet.

| Requirement (section) | Status | Evidence / gap |
|---|---|---|
| Review of Parts 1-2, launch and screenshot old UI (3) | Tested | HANDOFF.md, docs/design/PART3_DESIGN.md |
| GUI redesign, three distinct modes (5-7) | Tested (Windows 10) | screenshots/part3-*.png; design doc |
| Menus, toolbar, collapsible groups, dockable/resizable panes, status bar, shortcuts, fullscreen (6) | Tested (Windows 10) | pane_grid resize/drag/maximise; F11 |
| Context menus | Incomplete | Right-click on filmstrip toggles flag only |
| Accessibility (screen readers), multi-monitor | Blocked / not tested | Iced 0.13 has no AccessKit |
| Light/dark themes, high-DPI (125%), window sizes (9) | Tested (Windows 10) | |
| Framework evaluation (8) | Implemented | Iced kept; egui/Slint compared |
| Crop, resize, rotate, straighten, flip, perspective (4-point), lens (1-parameter), canvas resize, custom ratios (10) | Tested | edit unit tests; GUI Geometry group |
| Image alignment, automatic object/subject/background selection | Blocked | Needs segmentation model |
| Rect, ellipse, polygon, lasso, brush, gradient, radial masks drawable in GUI (10) | Tested (Windows 10, rect) / Implemented (others) | Overlay rasterised; masks unit-tested |
| Mask refinement, layer masks, colour-range and luminance masks (10, 12, 14) | Tested (engine) | GUI exposes selection/gradient/radial masks; colour range/luminance in engine and recipes only |
| Undo/redo, history, before/after, split view (10) | Tested | |
| All §11 adjustments, global or masked, float precision, linear-light WB/exposure | Tested | color.rs tests; GUI sliders |
| Curves (master parametric in GUI; master + R/G/B in engine), HSL 8 bands, split toning/grading, colour balance, LUT import/export, presets (12) | Tested | .cube parser hostile-input tests |
| Colour wheels, selective colour UI, profile conversion, HDR, scopes beyond histogram | Not started | |
| Histogram, clipping warnings/overlay, pixel inspection (12) | Tested (Windows 10) | |
| RAW: identification + embedded preview for DNG/CR2/CR3/NEF/ARW/RAF/ORF/RW2/PEF (13) | Tested (synthetic files only) | CAMERA_COMPATIBILITY.md |
| RAW decoding/demosaic/camera and lens profiles | Blocked | LGPL decoders need owner licensing decision |
| Layers: adjustment, redaction, shape, visibility, opacity, blend, ordering, delete, flatten on export (14) | Tested | Redaction layers forced above adjustments |
| Text layers, layer groups, locking, duplication, merging | Not started | Fonts are an untrusted-input surface; deferred |
| Intelligent editing (15) | Incomplete | Auto exposure/WB (deterministic) only; others blocked on models |
| Privacy Scan: uncertain vs confirmed separated (16) | Tested (Windows 10) | Detection model improvements blocked |
| High-assurance export pre-review + post-verify (17) | Tested | pre_export_review; verify; GUI blocks |
| C2PA / authenticity verification | Not started | Detected only, reported unverified |
| Media Manager: SQLite catalog, albums, tags, ratings, search, filters, duplicates, missing files, optional cache with secure clear (18) | Tested (engine), Implemented (GUI subset) | GUI: search, rating filter, tag, open from catalog |
| Portable projects / archiving | Not started | |
| Batch: existing Part 2 batch + colour/crop recipes via CLI (19) | Implemented | Batch watermarking/AI protection not available |
| Structured logger, allowlisted fields, sanitisation, rotation, retention, caps, clear, report (21-22) | Tested | log tests use synthetic secrets |
| Diagnostics Center (23) | Tested (Windows 10) | |
| Autosave, atomic saves, backups, checkpoints, unclean-shutdown detection, restore banner, safe mode, temp cleanup (24) | Tested | project tests; forced kill observed in GUI |
| Disk-exhaustion, GPU and model failure recovery tests | Not started | |
| Performance Center with real sysinfo data (25) | Tested (Windows 10) | GPU shown as not used |
| Optimisation: parallel colour stage, proxy rendering, bounded proxy cache (26) | Tested | BENCHMARKS.md |
| GPU acceleration, SIMD intrinsics, 8 GB test | Not started | |
| Forensic inspection (27) | Tested | JPEG quality estimate tests |
| Security hardening: hostile RAW/LUT/project/catalog inputs, SQL parameterisation, log injection (28) | Tested | Fuzz harness still not started |
| Competitive benchmarking (29) | Blocked | Competing apps not available; no claim made |
| Cross-platform runtime (30) | Incomplete | Windows only |
| Signed releases | Blocked | No signing credentials |


## Part 4

Specification: [PART4_SPECIFICATION.md](PART4_SPECIFICATION.md) (verbatim). Status words as above; "Tested" means automated tests plus a manual run of the real Windows 10 app unless noted. Formats in detail: [FORMAT_COMPATIBILITY.md](FORMAT_COMPATIBILITY.md).

| Requirement (spec section) | Status | Evidence / blocker |
|---|---|---|
| Audit of Parts 1 to 3 before expanding (3, 37A) | Done | Fixed: release desktop opened a console window; long text-field values painted over neighbouring widgets (iced 0.13 tiny-skia); folder import ignored media; manifest schema 2 separates local use from redistribution |
| Content-based format detection, extension mismatch warning (9) | Tested | `photo_media::detect`, 40+ signatures; unit tests; GUI and CLI warn on mismatch |
| Format compatibility matrix (9, 38) | Done | FORMAT_COMPATIBILITY.md |
| Images: BMP and GIF import (6) | Tested | animated GIF flagged, first frame only |
| Images: AVIF, HEIC/HEIF, JPEG XL, JPEG 2000, APNG, animated WebP, EXR, PSD, XCF, SVG (6) | Detect only | decoders not added (licence/patent review, untrusted-input surface) |
| Video container inspection: MP4/MOV/3GP/M4A, MKV/WebM (7, 9) | Tested | tracks, codecs, size, fps, duration, language, metadata |
| Video decode / preview / encode, codecs (7) | Blocked | no reviewed decoder; FilmCraft identified as candidate (research register) |
| Audio decode, analysis, waveform, spectrogram (8, 15) | Tested | Symphonia; WAV, FLAC, MP3, AAC/M4A, ALAC, Ogg Vorbis |
| AIFF, Opus, WMA, AC-3, AMR decode (8) | Blocked | not in Symphonia 0.5; detect only |
| Large-file streaming (31) | Implemented | audio streamed (10 min 48 kHz stereo analysed in 3.4 s); MP4 rewrite holds the file in memory (1 GiB GUI limit, 2 GiB CLI) |
| FilmCraft evaluation (10) | Done (desk review) | MIT OR Apache-2.0, own H.264 codec; not integrated (security/patent review pending) |
| Repository and research investigation (11 to 14) | Partial | research/MULTIMEDIA_RESEARCH_REGISTER.md: 4 projects verified, the rest listed as not verified |
| Audio protection engine (15) | Implemented, not usable | masked PGD engine with tests; refuses without a validated objective; no model installed |
| Music and artist protection (16) | Blocked | same engine and profile; no model |
| Video protection engine, temporal consistency (17) | Blocked | flicker metrics implemented (`temporal`); no frames, no model |
| Combined audiovisual protection, A/V sync (18) | Blocked | depends on video decode/encode |
| Protection profiles (19) | Implemented (audio engine) | Original Quality, Balanced/Voice, Maximum, Music |
| Social media profiles (20) | Not started for media | photo profiles from Part 2 unchanged |
| Quality preservation controls (21) | Partial | SNR floor, masking offset, no clipping in engine; loudness/peak analysis |
| Comparison workspace: waveform, spectrogram, loudness-matched A/B (22) | Tested | desktop A/B at -23 LUFS; SNR, segmental SNR, length checks |
| Multimedia Privacy Scan: metadata, subtitles (23) | Tested | MP4/MKV metadata; SRT/WebVTT text rules with cue times |
| Spoken information redaction (23) | Partial | manual mute/bleep ranges, verified; automatic detection blocked (no local speech-to-text model) |
| Moving-object redaction (23) | Blocked | needs decode, tracking and detection models |
| Metadata removal (24) | Tested | audio via clean WAV export; MP4 family lossless; MKV not implemented |
| Watermarking and provenance (25) | Not started | no C2PA signing credentials; no watermark |
| Defense Lab for media, compression resilience (26, 27) | Not started | needs protection model and encoders |
| Batch multimedia (28) | Partial | desktop imports folders of media; CLI commands per file; no batch media export yet |
| GUI in all three modes (29) | Tested (Windows 10) | Essential list + side panel, Professional MEDIA library + viewer, Technical Format Inspection; screenshots `part4-*.png` |
| 21-step pipeline (30) | Partial | steps 1-8, 11, 14, 17, 18, 21 for supported formats; protection, encode, watermark, credentials and A/V sync steps blocked |
| Logging and diagnostics (32) | Tested | media events logged with codes; no file names or values |
| Multimedia cybersecurity (33) | Tested | bounded parsers, node/depth caps, mutation tests, decoder panic containment (SECURITY_REVIEW.md) |
| Cross-platform testing (34) | Incomplete | Windows 10 runtime; Linux/macOS compile in CI only |
| Competitive benchmarking (35) | Blocked | no competing tools installed; no comparison claimed |
| Releases (39) | Not done | PR opened for review; owner asked not to merge before review |

## Part 5

Specification: [PART5_SPECIFICATION.md](PART5_SPECIFICATION.md) (received cut off in section 8). Classification uses the Part 5 vocabulary. "Independently tested" means automated tests **and** a manual check in the real Windows 10 app or an independent CLI check of the output.

### Master checklist, Parts 1 to 5 (summary by capability)

| Capability | Classification | Evidence |
|---|---|---|
| Bounded image import (JPEG, PNG, WebP, TIFF, BMP, GIF), RAW embedded preview | Implemented and independently tested | core tests, mutation fuzz, GUI imports |
| Metadata removal on export with re-open verification | Implemented and independently tested | pipeline tests, Part 2/3 GUI exports |
| Non-destructive editing, colour, layers, masks, geometry | Implemented and independently tested | edit tests, Part 3 screenshots |
| Opaque redaction | Implemented and independently tested | render-order test, export verification |
| Blur / pixelation / mosaic | Implemented and independently tested | new recoverability test; weak settings now warned live and at export |
| Privacy Scan: metadata, QR/barcodes, text rules | Implemented and independently tested | privacy tests, GUI scans |
| Automatic face detection | Implemented and independently tested (optional `faces`, in installers) | UltraFace RFB-320 (MIT); public-domain photos: 1/1 and 3/3 faces, 0 on a landscape; no labelled precision/recall set |
| Person / object / document / plate detection, OCR | Blocked by licensing | no licence-reviewed local model; scans list them as not examined |
| Photo adversarial protection (CLIP PGD experiment) | Experimental | CLI only, computation verified, efficacy not measured; never shown as protection in the GUI |
| Batch processing, review queue, duplicates | Implemented and independently tested | batch tests, CLI runs |
| Media Manager catalog, projects, autosave, crash recovery, logging | Implemented and independently tested | catalog/project/log tests; forced-kill restart observed |
| Diagnostics, Performance, Forensics, Format Inspection pages | Implemented and independently tested | Windows GUI screenshots |
| Audio decode/analysis/redaction/export, A/B playback | Implemented and independently tested | media tests, GUI run, CLI check of outputs |
| AIFF, CAF, MP1/MP2, ADPCM decode | Implemented but insufficiently tested | AIFF tested; CAF/MP2/ADPCM not tested with real files |
| Opus decode | Partially implemented (not merged) | branch `ag/opus-wma`, same provenance issue as video |
| WMA, AC-3, AMR decode | Blocked by technical limitations | no licence-clean Rust decoder found |
| MP4-family metadata removal | Implemented and independently tested | 1,500 mutations, GUI + CLI verification (synthetic files only) |
| Matroska metadata removal | Not implemented | inspection only |
| Subtitle Privacy Scan and redaction | Implemented and independently tested | tests, GUI export, CLI check |
| Video frame decode, preview, redaction, re-encode, tracking | Partially implemented (not merged) | working branches `ag/video-frames`, `ag/video-tracking-verify` use `filmcraft-*` crates of unverified provenance; to be ported to official `storytold/craft-libs` |
| Video adversarial protection | Not supported by sufficient research | no licence-compatible validated method with weights; FaceShield is research-only |
| Voice-cloning protection | Experimental (optional `voice-protect`, not in installers) | Rust ECAPA-TDNN objective; parity with SpeechBrain and protection effect not measured (PyTorch unavailable for conversion on the test machine) |
| Automatic spoken-content detection | Implemented but insufficiently tested (optional `stt`) | Whisper via candle; accuracy not measured (no weights on the test machine); text rules match written forms only |
| Voice-assistant safe playback | Experimental (CLI, needs `stt`) | not tested on physical assistant devices |
| Invisible ownership watermark | Implemented and independently tested (CLI) | tests incl. JPEG 70-90 survival; real photo check; not in GUI yet |
| C2PA content credentials | Implemented but insufficiently tested (optional `c2pa`, in installers) | inspection and ES256 signing unit-tested with a generated test chain; not tested with a CA-issued certificate |
| Linux / macOS | Implemented but insufficiently tested | CI builds and tests on Linux, macOS Intel and Apple Silicon; new `gui-smoke` CI job launches the app and captures screenshots (results in the PR checks) |
| Installers | Implemented but insufficiently tested | `release.yml` builds unsigned MSI/.deb/AppImage/.dmg with checksums and SBOM; first run pending |
| Signed releases, updater | Not implemented | no signing credentials; no updater (the app makes no network connections) |
| Competitive benchmarks | Blocked by technical limitations | comparison tools not installed |

### Section 4 sweep (incomplete functionality)

| Search | Result |
|---|---|
| TODO / FIXME / `todo!` / `unimplemented!` in workspace code | none |
| Placeholder / mock / simulated data outside tests | none: demo fixtures live in `examples/` and are labelled as fixtures |
| Messages without a handler, buttons without an effect | none: every `Message` variant is handled; disabled buttons are disabled, not no-ops |
| Unused dependencies | removed `serde_json` from `photo-models` |
| Incorrect success messages | none found: exports report "verified" only after re-open checks; audio protection refuses with an error |
| Fabricated protection scores | none: no protection percentage is computed or displayed |
| Unsupported format claims | corrected: README and docs list exactly what decodes |

### Bugs fixed in this part

1. Text fields drew their text outside the box (owner's screenshots). Root cause: iced 0.13 only clips text-input text while a selection is active, and containers/scrollables without overflow create no clip layer. Fix: a small `Clip` widget that always draws its child in a clipped layer; every text field now uses it with the frame drawn outside the clip. Verified while typing and after leaving the field.
2. Weak blur and pixelation were accepted without warning. Now flagged in the inspector, the pre-export review and the export report.
3. AIFF was advertised as "detect only" although the decoder supports it once its feature is enabled; enabled with a test.
