# Remaining Tasks

**Date:** 2026-11-27  
**Branch:** `gemini/protection-validation`

## High Priority
1. **Real-Model Benchmarks (CLIP + UltraFace):** Run `photo-defense experimental-clip --hardened` and `photo-defense validate` on real consented portraits and public-domain photos with local CLIP ViT-B/32 `.safetensors` and UltraFace `.onnx` weights to record empirical cosine-similarity and face-detection survival tables.
2. **Face Recognition & Diffusion VAE Objectives:** Evaluate MIT/Apache-2.0-compatible face recognition (e.g., FaceNet/InceptionResnetV1 or commercial-safe ONNX) and a differentiable VAE encoder surrogate (PhotoGuard-style) for `photo-engine` and `photo-lab::validation`.
3. **Side-by-Side Comparison Controls in Validation Center:** Desktop CLIP model/manifest selection and Batch Validation are now wired (`experimental-clip` feature); add dedicated side-by-side / difference image preview directly inside the Validation Center tab.

## Medium Priority
4. **GPU Acceleration (Optional):** Evaluate optional `candle-core` CUDA / `ort` DirectML feature for the Quadro RTX 5000 while preserving CPU fallback.
5. **Video/Audio Manual Correction UI:** Expose `apply_manual_box_override` and `summarize_track` warnings interactively in the `photo-desktop` video timeline panel.
6. **Cross-Platform Runtime Testing:** Run desktop GUI and media/validation smoke tests on Linux and macOS (currently verified on Windows 10 x64).

## Blocked / External
- **Physical Voice-Assistant Testing:** Requires physical Siri, Google Assistant, Alexa, and Meta AI devices.
- **Voice-Cloning Resistance:** Requires a licensed local voice-cloning synthesis model + purification benchmark.
- **CA-Issued C2PA Signing:** Requires a production CA-issued signing certificate.
