# Changelog

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
