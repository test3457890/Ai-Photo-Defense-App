# Changelog

## 0.2.0 development preview, 2026-11-26

Part 2 foundation, audited against the 0.1 work. Added `photo-edit` (non-destructive edit stack, crop/aspect/resize/rotate/straighten/flip, masks, blur/pixelate/mosaic, opaque secure redaction, flattening), `photo-privacy` (container metadata inspector, local QR/barcode detection with tiling, credential and personal-data rules with checksum validation, 12 privacy profiles with JSON import/export), and `photo-pipeline` (secure export with re-open verification, publish-or-block, safe batch collection, duplicates, review queue, folder monitor). CLI gains `scan`, `process`, `batch` and `profiles`. The desktop app was rebuilt around eight workspaces with drag-to-select redaction, scan overlays and verified export.

Fixes: re-opened exports now decode under the same limits as imports; transparent pixels no longer carry hidden original colour; same-name inputs no longer fail exports; optional engine dependency; crates marked unpublished; cargo-deny policy added. OCR, face/object detection and adversarial protection remain unavailable and are reported as such.

## 0.1.0 development preview, 2026-11-26

Initial Rust workspace with functional native Essential, Professional and Technical layouts, bounded image I/O, orientation-aware metadata removal, no-clobber batch copies, quality measurements, difference previews, transformation lab, CLI and local safetensors verification.

Added a coordinated projected-gradient SDK and optional experimental CLIP ViT-B/32 feature suppression implementation, with gradient tests, a real-checkpoint smoke test and a PyTorch reference-parity check on one synthetic input. These establish computation only; AI protection efficacy is not validated.

Added research/license inventory, design review, actual Linux screenshots, security/privacy/threat-model documents, CI/build definitions, dependency alerts and SBOM generation. Updated Candle from 0.9.2 to 0.11.1 to remove ZIP dependency vulnerabilities. Production releases remain blocked on model validation, licensing, security and platform runtime testing.
