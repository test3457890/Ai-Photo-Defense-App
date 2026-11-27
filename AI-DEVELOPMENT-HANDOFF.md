# AI Photo Defense — Development Handoff (`gemini/protection-validation`)

**Date:** 2026-11-27  
**Branch:** `gemini/protection-validation` (branched from `main` at `a8b6163`)  
**Previous Work Preserved:** All ChatGPT Work and Claude (`0.1.0`–`0.6.0` / Parts 1–5 + real-world validation) code, crates, tests, and documentation preserved intact.

---

## 1. What Existed Before This Session

- Workspace of 11 Rust crates (`photo-core`, `photo-engine`, `photo-models`, `photo-lab`, `photo-edit`, `photo-privacy`, `photo-pipeline`, `photo-log`, `photo-project`, `photo-catalog`, `photo-media`, `photo-cli`, `photo-desktop`).
- `photo-engine`: basic sign-SGD PGD optimizer (`optimize`) and optional `ClipObjective` (`clip` feature, Candle CLIP ViT-B/32) minimizing `||f(x)||_2^2` on a single fixed `224x224` bilinear grid. **Not wired into desktop export.**
- `photo-lab`: 8-transform pixel perturbation survival measurement (`evaluate`) explicitly marked `ai_efficacy: "not evaluated"`.
- `photo-privacy`: metadata inspection, QR/barcode detection (`rxing`), 45+ text/credential rules, UltraFace ONNX face detection (`faces`), C2PA (`c2pa`), forensics.
- `photo-media`: MP4/MKV inspection, MP4 lossless metadata strip, audio decode/analyze/export, Whisper STT (`stt`), ECAPA voice protect (`voice-protect`, already proven ineffective across models/denoising), wake-word safe playback (`wake_word.rs`), H.264 video decode/redact/track/verify (`video`, `video_track.rs`), Opus (`opus`).
- `photo-models`: schema-2 manifest verification + optional localhost-only Ollama assistant (`ask-ai`).

---

## 2. Protection Weaknesses Identified (Phase 2)

1. **Desktop Export had zero adversarial protection:** GUI exports only stripped metadata and applied edits/redactions.
2. **CLIP Objective Loss Flaw:** `ClipObjective::loss_gradient` minimized `||f(x)||_2^2` (unnormalized L2 norm). Because CLIP embeddings are L2-normalized before cosine similarity / image matching / diffusion conditioning, shrinking the norm does not rotate the embedding direction (`cos_sim` remained ~`1.0`).
3. **Stationary-Point Gradient Trap:** Naive cosine-distance loss `1 - cos_sim(f(x), f(x_0))` has **zero gradient** at `x = x_0` (`theta = 0`), requiring a PGD initial offset.
4. **No Expectation over Transformation (EoT) or Frequency Shaping:** Single deterministic `224x224` bilinear resize + high-frequency `4/255` noise overfit to exact pixels and collapsed under JPEG quantization, resizing, and denoising.
5. **Candle `clamp` Autograd Detachment Bug (caught and fixed):** `Tensor::clamp` in `candle-core` 0.11 detaches gradients from `Var`; EoT transforms must remain differentiable without `clamp` inside the autograd graph.
6. **Windows CLI Stack Overflow (caught and fixed):** Adding new subcommands (`validate`, `durability`) pushed `clap::Parser::parse()` over Windows' 1 MiB default main-thread stack in debug builds; fixed by spawning `run_cli()` on an 8 MiB stack thread.

---

## 3. What Was Implemented & Modified

### `crates/engine/`
- **`crates/engine/src/frequency.rs` (new):** Pure-Rust 8x8 2D DCT/IDCT (`dct2_8x8`, `idct2_8x8`), `project_mid_frequency_delta` (damps DC `0.90` and fragile high-frequency `u+v > max_band` `0.65`), and `FrequencyBandObjective` (rewards mid-frequency `2..=5` DCT energy while penalizing DC drift and high-frequency waste).
- **`crates/engine/src/lib.rs` (modified):** Added `HardenedSettings`, `compute_adaptive_texture_mask` (Sobel edge-magnitude perceptual masking `[0.35, 1.0]`), `build_regional_mask` (soft-feathered `[0.25, 1.0]` mask from bounding boxes), `smooth_gradient_ti` (TI-FGSM separable smoothing), and `optimize_hardened` (MI-FGSM momentum + TI-FGSM + adaptive/regional masking + mid-frequency DCT projection + 2x2-block PGD initial offset). Preserved legacy `optimize()` intact.
- **`crates/engine/src/clip.rs` (modified):** Added `ClipObjective::normalized_embedding`, `ClipObjective::cosine_similarity`, and `ClipCosineObjective` (minimizes cosine similarity to original L2-normalized embedding with differentiable EoT scale/bias jitter). Added `cosine_divergence_beats_feature_energy_suppression` unit test.

### `crates/core/`
- **`crates/core/src/lib.rs` (modified):** Added `ExtendedQuality` and `detailed_quality()` computing CIELAB $\Delta E_{76}$ (`delta_e_mean`, `delta_e_max`), `perceptual_distance_proxy` (multi-scale SSIM + gradient distance), `artifact_score` (Laplacian residual), `resolution`, and file sizes. Preserved `Quality` and `quality()` intact.

### `crates/lab/`
- **`crates/lab/Cargo.toml` (modified):** Added `photo-privacy`, `photo-models`, `serde_json`, and optional `photo-engine` behind `faces` and `clip` features.
- **`crates/lab/src/lib.rs` (modified):** Added 23-transform Real-World Durability Suite (`evaluate_durability`, `DurabilityReport`, `DURABILITY_TRANSFORMS`) covering JPEG 95/85/70/50, WebP lossless, PNG conversion, `instagram-sim`, `facebook-sim`, `screenshot-sim`, crops, resizes, rotation, warmth/brightness/contrast, Gaussian blur 1/2, 3x3 median filter, `ai-enhance-sim`, and `multi-export-3x` — explicitly flagging `is_simulated: bool`.
- **`crates/lab/src/validation.rs` (new):** Integrated Validation Engine (`run_validation`, `ValidationMode`, `ResultClassification`, `AttackCategory`, `ValidationSuiteReport`, `.to_markdown()`, `.to_json_pretty()`). Runs real UltraFace ONNX (`faces`) and CLIP ViT-B/32 (`clip`) when local models are provided, and honestly classifies unbundled models (`ArcFace`, `FaceNet`, `Stable Diffusion`, `FLUX`, `SimSwap`, `DeepFaceLab`, `DreamBooth`, `LoRA`) as `NotTested` or `Unsupported`. Never invents an overall protection percentage.

### `crates/media/` & `crates/models/`
- **`crates/media/src/video_track.rs` (modified):** Added fail-safe occlusion mask expansion (`+12` px up to `24` px when `occluded == true`), `TrackSummary` + `summarize_track()` (emits warnings for occlusion, scene cuts, and low confidence `<0.50`), and `apply_manual_box_override()` for manual frame-by-frame correction.
- **`crates/models/src/ai_assistant.rs` (modified):** Added deterministic offline `suggest_protection_settings()` (`ProtectionAdvice`) and optional localhost-Ollama `explain_validation_with_ollama()`.

### `crates/cli/` & `crates/desktop/`
- **`crates/cli/Cargo.toml` & `src/main.rs` (modified):** Wired `faces` and `clip` features through `photo-lab`; added `durability` and `validate` subcommands; added `--hardened` flag to `experimental-clip`; wrapped `main()` in an 8 MiB stack thread for Windows compatibility.
- **`crates/desktop/Cargo.toml`, `src/app.rs`, `src/views.rs` (modified):** Integrated Validation Center into Technical > Defense Lab (`Quick Test`, `Comprehensive Test`, `Social Media Durability`, `ExtendedQuality` summary, honest classification table, durability table with `SIMULATED`/`STANDARD` tags, `.md`/`.json` report export).

---

## 4. Verification & Test Results (2026-11-27, Windows 10 x64, Rust 1.100.0)

**Note on Windows build:** Due to Windows `MAX_PATH` on `link.exe`, set `CARGO_TARGET_DIR=C:\Users\brett\apd-target` before running Cargo commands.

- `cargo fmt --all --check`: **PASS**
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`: **PASS** (0 warnings, 0 errors)
- `cargo test --locked --workspace --all-features`: **PASS** (exit code 0 across all 13 workspace crates; 4 pre-existing ignored tests requiring external model files)
- `cargo audit`: **PASS** (0 vulnerabilities; 17 pre-existing informational warnings for transitive unmaintained/unsound-optional crates documented in `SECURITY-REVIEW.md`)
- `cargo deny --all-features check`: **PASS** (`advisories ok, bans ok, licenses ok, sources ok`)
- Live CLI smoke test: `cargo run --locked -p photo-cli --all-features -- validate tests/fixtures/calibration.png tests/fixtures/calibration.png --mode comprehensive`: **PASS** (verified Markdown report output with `Not tested` / `Unsupported` classifications and zero fabricated metrics).

---

## 5. Instructions for Continuing Development

1. **Always use short `CARGO_TARGET_DIR`** on Windows (e.g. `$env:CARGO_TARGET_DIR = "C:\Users\brett\apd-target"`).
2. **Real-model validation next:** Run `photo-defense validate` and `photo-defense experimental-clip --hardened` against real local CLIP ViT-B/32 SafeTensors and UltraFace RFB-320 ONNX weights (and ArcFace/FaceNet ONNX once licensed and added).
3. **See `REMAINING-TASKS.md`** for prioritized next engineering items (ArcFace/RetinaFace ONNX integration, Diffusion VAE objective, GPU/CUDA execution provider, GUI Batch Validation & Comparison view, Linux/macOS runtime testing).
