# Protection Validation Report

**Date:** 2026-11-27  
**Branch:** `gemini/protection-validation`  
**Scope:** Independent investigation of existing protection weaknesses, unit-level verification of hardened adversarial algorithms, and live CLI validation suite smoke testing.

---

## 1. Phase 2 Investigation: Why Preliminary Protection Failed

1. **GUI Export Pipeline:** `photo-desktop` export applies metadata stripping, edits, and redactions only; it does not run `photo-engine`.
2. **CLIP Loss Function:** The legacy `ClipObjective` (`crates/engine/src/clip.rs`) minimized `||f(x)||_2^2` (unnormalized L2 energy). Because downstream CLIP matching and diffusion conditioning normalize embeddings to unit length (`f(x) / ||f(x)||_2`), shrinking magnitude leaves cosine similarity near `1.0`.
3. **Stationary Point at `x = x_0`:** Direct cosine-similarity loss `1 - cos_sim(f(x), f(x_0))` has zero first-order gradient at `x = x_0`, requiring a PGD random/seeded start offset.
4. **Lack of EoT and Frequency Shaping:** Single-grid `224x224` bilinear optimization without Expectation over Transformation (EoT) or mid-frequency DCT projection collapses under JPEG compression, resizing, and denoising.

---

## 2. Measured Results (This Session)

| Check | Target | Result | Classification |
|---|---|---|---|
| `cosine_divergence_beats_feature_energy_suppression` | `photo-engine` (`tiny()` ViT) | `ClipCosineObjective` + `optimize_hardened` (MI-FGSM + TI-FGSM + EoT + DCT) achieved strictly lower cosine similarity than legacy `ClipObjective` | **Passed** (unit-level; synthetic weights) |
| `hardened_optimizer_respects_regional_and_texture_masks` | `photo-engine` | Perturbation bounded within local `epsilon * mask`, flat regions attenuated, regional mask prioritized | **Passed** |
| `dct_idct_roundtrip_is_exact` & `frequency_objective_gradient_matches_finite_difference` | `photo-engine::frequency` | Exact 8x8 DCT/IDCT roundtrip (`<1e-5`) and finite-difference gradient match (`<1e-3`) | **Passed** |
| `durability_suite_runs_all_transforms_and_marks_simulations` | `photo-lab` | All 23 transforms executed; `instagram-sim`, `facebook-sim`, `screenshot-sim`, `ai-enhance-sim` flagged `is_simulated = true` | **Passed** |
| `photo-defense validate ... --mode comprehensive` | `photo-cli` on `tests/fixtures/calibration.png` | Quality metrics computed (`PSNR=inf`, `SSIM=1.000000`, `Delta-E=0.000`); all 12 attack categories reported honestly as `Not tested` or `Unsupported` without local weights | **Passed** |

---

## 3. Unverified / Blocked Categories (Honest Status)

No local weights for the following models are bundled or installed on this machine in this session, so they remain **Not tested** or **Unsupported**:
- **Facial recognition:** ArcFace, FaceNet (**Not tested** — no local weights)
- **Face swapping / Deepfakes:** SimSwap, DeepFaceLab (**Unsupported** locally)
- **Diffusion reconstruction / AI training:** Stable Diffusion, FLUX, DreamBooth, LoRA (**Not tested** / **Unsupported** locally)
- **Voice-cloning & physical wake-word devices:** No cloning systems or physical Siri/Google/Alexa/Meta devices tested (prior `PART5_VALIDATION_REPORT.md` already proved ECAPA-only voice perturbation does not transfer to x-vector).
