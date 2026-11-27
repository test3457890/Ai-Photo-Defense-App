# Changelog

## 1.0.1 (2026-11-27)

- **Desktop Shortcut Option:** Added **Create Desktop Shortcut** in the Startup Wizard and Main Menu (creates a native `.lnk` shortcut on Windows or `.desktop` entry on Linux).
- **Automatic Update & Relaunch:** **Download & Install Update** now verifies SHA-256 against `SHA256SUMS.txt`, closes the running app cleanly, runs the installer (`msiexec /passive` on Windows), and automatically relaunches the updated app.

## 1.0.0 (2026-11-27)

Official 1.0.0 release candidate: Startup Wizard, Main Menu, Interactive Tutorial Mode, Opt-In GitHub Release Updater (tag-gated `v*` releases only), Sanitized Error Log export, Validation Center, 23-transform Real-World Durability Suite, and hardened adversarial engine (`photo-engine`).

### Protection Hardening & Validation Suite (`gemini/protection-validation`, 2026-11-27)

- **Hardened Adversarial Engine (`photo-engine`):** Added `optimize_hardened` with MI-FGSM momentum, TI-FGSM gradient smoothing, Sobel adaptive texture masking (`compute_adaptive_texture_mask`), soft-feathered regional masking (`build_regional_mask`), 2x2-block PGD initial offset, and pure-Rust 8x8 2D DCT mid-frequency projection & `FrequencyBandObjective` (`crates/engine/src/frequency.rs`).
- **Fixed CLIP Objective (`photo-engine::clip`):** Added `ClipCosineObjective` (minimizes cosine similarity to original L2-normalized embedding with differentiable EoT scale/bias jitter) and `cosine_similarity()`. Fixed Candle `clamp()` autograd detachment bug. Added `--hardened` flag to CLI `experimental-clip`.
- **Extended Image Quality Metrics (`photo-core`):** Added `ExtendedQuality` and `detailed_quality()` computing CIELAB $\Delta E_{76}$ color accuracy, multi-scale gradient perceptual distance proxy, Laplacian artifact score, resolution, and file sizes.
- **23-Transform Real-World Durability Suite (`photo-lab`):** Added `evaluate_durability()` covering JPEG 95/85/70/50, WebP lossless, PNG conversion, simulated Instagram/Facebook/screenshot/AI-enhance transforms (`is_simulated = true`), crops, resizes, rotation, color/tone shifts, Gaussian blurs, 3x3 median filter, and 3x JPEG re-export chain.
- **Integrated Validation Engine & UI (`photo-lab::validation`, `photo-cli`, `photo-desktop`):** Added `validate` and `durability` CLI commands and integrated the Validation Center into Desktop Technical > Defense Lab with Quick/Comprehensive/Durability modes, honest classifications (`Passed`, `Partially effective`, `Failed`, `Inconclusive`, `Not tested`, `Unsupported`), no invented overall percentage, and `.md`/`.json` report export.
- **Video Tracking Safety & Offline AI Advisor (`photo-media`, `photo-models`):** Added fail-safe occlusion mask expansion (`+12` px), `summarize_track()` warnings, and `apply_manual_box_override()` in `video_track.rs`; added deterministic offline `suggest_protection_settings()` and optional localhost-Ollama `explain_validation_with_ollama()` in `ai_assistant.rs`.
- **Windows CLI Fix:** Wrapped `photo-cli` `main()` in an 8 MiB stack thread to prevent Windows 1 MiB default-stack overflow in `clap::Parser::parse()`.

### Part 5 Integration & Real-World Validation (Claude & Antigravity)

Integration of Antigravity's Part 5 branches, reviewed and verified by Claude. Heavy additions are optional cargo features; release installers include `faces` and `c2pa`.

- Face detection (`faces`): pure-Rust `tract-onnx` running UltraFace RFB-320 (MIT), chosen by the user as a local `.onnx` file. Privacy Scan reports face regions; Professional mode "Select all faces". Checked on public-domain photographs: 1 of 1 and 3 of 3 faces found, none on a synthetic landscape. No precision/recall figure: no labelled dataset was used.
- C2PA content credentials (`c2pa`): inspection in Forensic Inspection and optional ES256 signing of exports with a certificate you provide. `RUSTSEC-2023-0071` (rsa, via c2pa) is ignored in deny.toml and .cargo/audit.toml because only ECDSA is used.
- Voice protection (`voice-protect`, experimental, not in installers): Rust ECAPA-TDNN speaker encoder for the audio protection engine, an offline SafeTensors converter and a "Protect voice" control. Parity with SpeechBrain and any protection effect are **not measured**.
- Voice-assistant safe playback (`voice-safe-playback` CLI, needs `stt`): finds wake words in a transcript and mutes or bleeps them. Not tested against real assistant devices.
- Optional Ask AI (`ask-ai`, models crate only): licence-approved model list and a loopback-only local Ollama client, refused unless enabled; responses capped at 4 MiB. View > Scale 100/125/150 %.
- Unsigned installers workflow (MSI, .deb, AppImage, .dmg, checksums, SBOM) with pinned tool versions and a checksum-verified appimagetool; GUI smoke tests on Linux and macOS in CI; original logo and app icons.
- Desktop shows optional-feature controls only when the feature is built in.
- Real-media validation fixed nine bugs (see docs/PART5_VALIDATION_REPORT.md): safe video export (verify before saving), A/V drift measured against the source, AAC channel discovery, HE-AAC timing, Whisper decoding and timestamps, ECAPA front end now identical to SpeechBrain, converter script, MSI and AppImage packaging. Installers now include H.264 video and Opus.

- H.264 video (`video`, experimental, not in installers): MP4/MOV frame decode, scrub/play preview, time-ranged rectangle redaction (opaque or blur), forward/backward object tracking with scene-cut stop, re-encode with original audio copied, and an independent per-frame verification of the export. CLI `video-redact`, `video-track`.
- Opus decoding (`opus`, experimental, not in installers) in Ogg/WebM through the same analysis and WAV export path. WMA, AC-3 and AMR remain unsupported.
- Video and Opus use FilmCraft's own codec libraries (`storytold/craft-libs`, MIT OR Apache-2.0) pinned to the commit FilmCraft itself uses. Antigravity's first versions used look-alike `filmcraft-*`/`effectcraft-*` crates from an unrelated crates.io publisher; those were replaced before merging.

## 0.5.0 release candidate (Part 5), 2026-11-26

Refinement and validation pass over Parts 1 to 4. Fixed: text fields painted their text outside the box (a clip widget now confines every field). Added: live and export-time warnings for weak blur or pixelation, backed by a test showing small radii and blocks leave text structure; AIFF, CAF, MP1/MP2 and ADPCM audio decoding; an invisible key-based ownership watermark with CLI `watermark-embed` / `watermark-detect` (survives JPEG quality 70+, not resizing; provenance only, not AI protection); unsigned cross-platform installer workflow (`.github/workflows/release.yml`: Windows MSI, Linux `.deb` + AppImage, macOS `.dmg`, SHA-256 checksums, CycloneDX SBOM, no release published). Removed an unused dependency. Research register extended with a licence check of 25 projects: SpeechBrain ECAPA and Resemblyzer (Apache-2.0) and Whisper (MIT) make voice protection and spoken-content detection licence-feasible; FaceShield is research-only and Glaze/Nightshade are closed. Master requirements checklist for Parts 1 to 5 in IMPLEMENTATION_MATRIX.md.

## 0.4.0 development preview (Part 4), 2026-11-26

Multimedia privacy foundation, not a video editor. New `photo-media` crate: content-based format detection (40+ signatures, extension-mismatch warnings); MP4/MOV/3GP/M4A and Matroska/WebM structure and privacy-metadata inspection; lossless MP4-family metadata removal that rewrites chunk offsets and verifies every chunk; streamed audio decoding through Symphonia with loudness, true peak, clipping, silence, waveform and spectrogram; mute/bleep redaction with verified WAV export; original-vs-export comparison; SRT/WebVTT Privacy Scan and redaction; temporal flicker metrics; a psychoacoustically bounded audio protection engine that refuses to run without a validated model. Desktop: media in all three modes, loudness-matched A/B playback, Format Inspection page. CLI: `media-inspect`, `strip-video`, `audio-analyze`, `audio-compare`, `audio-export`, `audio-protect` (refuses), `subtitles`. Images: BMP and GIF import. Model manifests move to schema 2 (local use separate from redistribution).

Fixes found in the Parts 1 to 3 audit: release desktop build opened a console window; long text-field values painted over neighbouring widgets; folder import ignored media. Security: contained a Symphonia panic on a crafted WAV. Video decoding/encoding, voice and video protection, speech-to-text, watermarking and GPU acceleration remain unavailable.

## 0.3.0 development preview (Part 3), 2026-11-26

Redesigned native interface with three distinct modes; Color Studio and adjustment layers with soft masks and blend modes; perspective, lens and canvas corrections; RAW embedded-preview support; SQLite Media Manager; structured privacy-sanitised logging (`photo-log`); projects with atomic saves, autosave journal and crash recovery (`photo-project`); Diagnostics, Performance and Forensic workspaces; high-assurance export review; CLI `forensics` and `raw-preview`. Security: replaced a vulnerable bundled SQLite (RUSTSEC-2026-0459) before release by moving to rusqlite 0.43. RAW development, adversarial protection, model-based detection, accessibility and GPU acceleration remain unavailable.

## 0.2.0 development preview, 2026-11-26

Part 2 foundation, audited against the 0.1 work. Added `photo-edit` (non-destructive edit stack, crop/aspect/resize/rotate/straighten/flip, masks, blur/pixelate/mosaic, opaque secure redaction, flattening), `photo-privacy` (container metadata inspector, local QR/barcode detection with tiling, credential and personal-data rules with checksum validation, 12 privacy profiles with JSON import/export), and `photo-pipeline` (secure export with re-open verification, publish-or-block, safe batch collection, duplicates, review queue, folder monitor). CLI gains `scan`, `process`, `batch` and `profiles`. The desktop app was rebuilt around eight workspaces with drag-to-select redaction, scan overlays and verified export.

Fixes: re-opened exports now decode under the same limits as imports; transparent pixels no longer carry hidden original colour; same-name inputs no longer fail exports; optional engine dependency; crates marked unpublished; cargo-deny policy added. OCR, face/object detection and adversarial protection remain unavailable and are reported as such.

## 0.1.0 development preview, 2026-11-26

Initial Rust workspace with functional native Essential, Professional and Technical layouts, bounded image I/O, orientation-aware metadata removal, no-clobber batch copies, quality measurements, difference previews, transformation lab, CLI and local safetensors verification.

Added a coordinated projected-gradient SDK and optional experimental CLIP ViT-B/32 feature suppression implementation, with gradient tests, a real-checkpoint smoke test and a PyTorch reference-parity check on one synthetic input. These establish computation only; AI protection efficacy is not validated.

Added research/license inventory, design review, actual Linux screenshots, security/privacy/threat-model documents, CI/build definitions, dependency alerts and SBOM generation. Updated Candle from 0.9.2 to 0.11.1 to remove ZIP dependency vulnerabilities. Production releases remain blocked on model validation, licensing, security and platform runtime testing.
