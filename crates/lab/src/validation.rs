//! Integrated AI Photo Defense Validation Engine.
//!
//! Runs real local models when installed (UltraFace ONNX, CLIP ViT-B/32 SafeTensors)
//! and classifies every attack category honestly using Phase-8 classifications:
//! Passed, PartiallyEffective, Failed, Inconclusive, NotTested, Unsupported.
//! Never invents an overall protection percentage and never fabricates results.

use crate::{DurabilityReport, evaluate_durability, transform};
use anyhow::Result;
use image::DynamicImage;
use photo_core::{ExtendedQuality, detailed_quality};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationMode {
    QuickProtectionTest,
    ComprehensiveProtectionTest,
    CustomModelTest,
    SocialMediaDurabilityTest,
    BatchValidation,
}

impl ValidationMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::QuickProtectionTest => "Quick Protection Test",
            Self::ComprehensiveProtectionTest => "Comprehensive Protection Test",
            Self::CustomModelTest => "Custom Model Testing",
            Self::SocialMediaDurabilityTest => "Social Media Durability Test",
            Self::BatchValidation => "Batch Validation",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResultClassification {
    Passed,
    PartiallyEffective,
    Failed,
    Inconclusive,
    NotTested,
    Unsupported,
}

impl ResultClassification {
    pub fn label(self) -> &'static str {
        match self {
            Self::Passed => "Passed",
            Self::PartiallyEffective => "Partially effective",
            Self::Failed => "Failed",
            Self::Inconclusive => "Inconclusive",
            Self::NotTested => "Not tested",
            Self::Unsupported => "Unsupported",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttackCategory {
    FacialDetection,
    FacialRecognition,
    FaceSwapping,
    Deepfakes,
    IdentityCloning,
    AiImageManipulation,
    AiImageReconstruction,
    ReverseImageMatching,
    UnauthorizedAiTraining,
    AiUpscaling,
    Denoising,
    AdversarialProtectionRemoval,
}

impl AttackCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::FacialDetection => "Facial detection",
            Self::FacialRecognition => "Facial recognition",
            Self::FaceSwapping => "Face swapping",
            Self::Deepfakes => "Deepfakes",
            Self::IdentityCloning => "Identity cloning",
            Self::AiImageManipulation => "AI image manipulation",
            Self::AiImageReconstruction => "AI image reconstruction",
            Self::ReverseImageMatching => "Reverse image matching",
            Self::UnauthorizedAiTraining => "Unauthorized AI training",
            Self::AiUpscaling => "AI upscaling",
            Self::Denoising => "Denoising",
            Self::AdversarialProtectionRemoval => "Adversarial protection removal",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackTestResult {
    pub category: AttackCategory,
    pub model_name: String,
    pub classification: ResultClassification,
    pub original_metric: Option<String>,
    pub protected_metric: Option<String>,
    pub survived_durability: Option<bool>,
    pub details: String,
}

#[derive(Debug, Clone, Default)]
pub struct ValidationConfig {
    pub mode: Option<ValidationMode>,
    pub face_model_path: Option<PathBuf>,
    pub clip_model_path: Option<PathBuf>,
    pub clip_manifest_path: Option<PathBuf>,
    pub original_bytes: Option<u64>,
    pub candidate_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationSuiteReport {
    pub schema_version: u32,
    pub mode: ValidationMode,
    pub quality: ExtendedQuality,
    pub tests: Vec<AttackTestResult>,
    pub durability: Option<DurabilityReport>,
    pub models_used: Vec<String>,
    pub limitations: Vec<String>,
}

impl ValidationSuiteReport {
    pub fn to_json_pretty(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("# AI Photo Defense - Validation Report\n\n");
        md.push_str(&format!("- **Mode:** {}\n", self.mode.label()));
        md.push_str(&format!(
            "- **Resolution:** {}x{}\n",
            self.quality.resolution[0], self.quality.resolution[1]
        ));
        md.push_str(&format!(
            "- **PSNR:** {} dB | **SSIM (8x8):** {:.6} | **Max delta:** {}\n",
            self.quality
                .base
                .psnr_db
                .map(|p| format!("{p:.2}"))
                .unwrap_or_else(|| "inf".into()),
            self.quality.base.ssim_luma_8x8,
            self.quality.base.max_channel_delta
        ));
        md.push_str(&format!(
            "- **Color accuracy (CIELAB Delta-E mean/max):** {:.3} / {:.3}\n",
            self.quality.delta_e_mean, self.quality.delta_e_max
        ));
        md.push_str(&format!(
            "- **Perceptual distance proxy:** {:.5} | **Artifact score:** {:.5}\n\n",
            self.quality.perceptual_distance_proxy, self.quality.artifact_score
        ));
        md.push_str("## Protection & Attack Resistance Results\n\n");
        md.push_str("| Category | Model | Classification | Original | Protected | Details |\n");
        md.push_str("|---|---|---|---|---|---|\n");
        for t in &self.tests {
            md.push_str(&format!(
                "| {} | {} | **{}** | {} | {} | {} |\n",
                t.category.label(),
                t.model_name,
                t.classification.label(),
                t.original_metric.as_deref().unwrap_or("-"),
                t.protected_metric.as_deref().unwrap_or("-"),
                t.details
            ));
        }
        if !self.limitations.is_empty() {
            md.push_str("\n## Limitations & Unverified Scope\n\n");
            for lim in &self.limitations {
                md.push_str(&format!("- {lim}\n"));
            }
        }
        md
    }
}

pub fn run_validation(
    original: &DynamicImage,
    protected: &DynamicImage,
    cfg: &ValidationConfig,
) -> Result<ValidationSuiteReport> {
    let mode = cfg.mode.unwrap_or(ValidationMode::QuickProtectionTest);
    let quality = detailed_quality(original, protected, cfg.original_bytes, cfg.candidate_bytes)?;
    let mut tests = Vec::new();
    let mut models_used = Vec::new();
    let mut limitations = vec![
        "No overall protection percentage is reported; each attack category is classified independently.".into(),
        "Models not installed locally are marked Not tested or Unsupported and never fabricated.".into(),
    ];

    // 1. Facial detection (UltraFace ONNX when `faces` feature and model path are provided)
    tests.push(run_face_detection_test(
        original,
        protected,
        cfg.face_model_path.as_deref(),
        &mut models_used,
    ));

    // 2. CLIP reverse image matching + denoising/purification survival (when `clip` feature + weights provided)
    let (clip_match, clip_purify) = run_clip_tests(
        original,
        protected,
        cfg.clip_model_path.as_deref(),
        cfg.clip_manifest_path.as_deref(),
        &mut models_used,
    );
    tests.push(clip_match);
    tests.push(clip_purify);

    // 3. Honest status for external / unbundled model categories
    let external_categories = [
        (
            AttackCategory::FacialRecognition,
            "ArcFace / FaceNet",
            "Not tested: no local ArcFace or FaceNet weights installed.",
        ),
        (
            AttackCategory::FaceSwapping,
            "SimSwap / DeepFaceLab",
            "Unsupported locally: no face-swap model integrated.",
        ),
        (
            AttackCategory::Deepfakes,
            "DeepFaceLab / diffusion inpaint",
            "Unsupported locally: requires external deepfake pipeline.",
        ),
        (
            AttackCategory::IdentityCloning,
            "DreamBooth / LoRA",
            "Unsupported locally: requires GPU fine-tuning harness.",
        ),
        (
            AttackCategory::AiImageManipulation,
            "Stable Diffusion / FLUX",
            "Not tested: no local diffusion img2img/inpaint model loaded.",
        ),
        (
            AttackCategory::AiImageReconstruction,
            "Diffusion VAE / Img2Img",
            "Not tested: no local VAE/diffusion reconstruction model loaded.",
        ),
        (
            AttackCategory::UnauthorizedAiTraining,
            "LoRA / DreamBooth",
            "Not tested: requires multi-epoch training harness.",
        ),
        (
            AttackCategory::AiUpscaling,
            "ESRGAN / SwinIR",
            "Not tested: no neural upscaler loaded (see ai-enhance-sim in durability).",
        ),
        (
            AttackCategory::Denoising,
            "3x3 Median + Gaussian Blur",
            "Tested via pixel/CLIP purification when CLIP weights are loaded; standalone neural denoiser not installed.",
        ),
    ];

    if mode != ValidationMode::QuickProtectionTest {
        for (cat, model, msg) in external_categories {
            let class = if msg.starts_with("Unsupported") {
                ResultClassification::Unsupported
            } else {
                ResultClassification::NotTested
            };
            tests.push(AttackTestResult {
                category: cat,
                model_name: model.into(),
                classification: class,
                original_metric: None,
                protected_metric: None,
                survived_durability: None,
                details: msg.into(),
            });
        }
    }

    let durability = match mode {
        ValidationMode::ComprehensiveProtectionTest | ValidationMode::SocialMediaDurabilityTest => {
            Some(evaluate_durability(original, protected)?)
        }
        _ => None,
    };

    if models_used.is_empty() {
        limitations.push("No local AI models were configured for this run; only image quality and durability transforms were measured.".into());
    }

    Ok(ValidationSuiteReport {
        schema_version: 1,
        mode,
        quality,
        tests,
        durability,
        models_used,
        limitations,
    })
}

fn run_face_detection_test(
    original: &DynamicImage,
    protected: &DynamicImage,
    model_path: Option<&std::path::Path>,
    models_used: &mut Vec<String>,
) -> AttackTestResult {
    #[cfg(feature = "faces")]
    {
        if let Some(path) = model_path {
            match photo_privacy::faces::FaceDetector::load(path) {
                Ok(det) => {
                    models_used.push(format!("UltraFace ONNX ({})", path.display()));
                    let orig_faces = det.detect(original, 0.7, 0.35).unwrap_or_default();
                    let prot_faces = det.detect(protected, 0.7, 0.35).unwrap_or_default();
                    let jpeg_prot = transform(protected, "jpeg-85")
                        .ok()
                        .and_then(|img| det.detect(&img, 0.7, 0.35).ok())
                        .unwrap_or_default();
                    let survived = jpeg_prot.len() < orig_faces.len();
                    let classification = if orig_faces.is_empty() {
                        ResultClassification::Inconclusive
                    } else if prot_faces.is_empty() {
                        ResultClassification::Passed
                    } else if prot_faces.len() < orig_faces.len() {
                        ResultClassification::PartiallyEffective
                    } else {
                        ResultClassification::Failed
                    };
                    return AttackTestResult {
                        category: AttackCategory::FacialDetection,
                        model_name: "UltraFace RFB-320 (ONNX)".into(),
                        classification,
                        original_metric: Some(format!("{} face(s)", orig_faces.len())),
                        protected_metric: Some(format!(
                            "{} face(s) (after JPEG-85: {})",
                            prot_faces.len(),
                            jpeg_prot.len()
                        )),
                        survived_durability: Some(survived),
                        details: if orig_faces.is_empty() {
                            "No faces detected in original image.".into()
                        } else {
                            format!(
                                "Original {} -> Protected {} -> JPEG-85 {}",
                                orig_faces.len(),
                                prot_faces.len(),
                                jpeg_prot.len()
                            )
                        },
                    };
                }
                Err(e) => {
                    return AttackTestResult {
                        category: AttackCategory::FacialDetection,
                        model_name: "UltraFace RFB-320 (ONNX)".into(),
                        classification: ResultClassification::Inconclusive,
                        original_metric: None,
                        protected_metric: None,
                        survived_durability: None,
                        details: format!("Failed to load face model: {e}"),
                    };
                }
            }
        }
    }
    let _ = (original, protected, model_path, models_used);
    AttackTestResult {
        category: AttackCategory::FacialDetection,
        model_name: "UltraFace RFB-320 (ONNX)".into(),
        classification: ResultClassification::NotTested,
        original_metric: None,
        protected_metric: None,
        survived_durability: None,
        details: "No local UltraFace .onnx model selected (enable `faces` feature and select model in Model Library).".into(),
    }
}

fn run_clip_tests(
    original: &DynamicImage,
    protected: &DynamicImage,
    model_path: Option<&std::path::Path>,
    manifest_path: Option<&std::path::Path>,
    models_used: &mut Vec<String>,
) -> (AttackTestResult, AttackTestResult) {
    #[cfg(feature = "clip")]
    {
        if let (Some(mp), Some(man_p)) = (model_path, manifest_path) {
            let load_res = (|| -> Result<photo_engine::clip::ClipObjective> {
                let man_bytes = std::fs::read(man_p)?;
                let manifest: photo_models::Manifest = serde_json::from_slice(&man_bytes)?;
                photo_models::verify(mp, &manifest)?;
                let weights = std::fs::read(mp)?;
                photo_engine::clip::ClipObjective::from_bytes(
                    weights,
                    original.width() as usize,
                    original.height() as usize,
                )
            })();
            match load_res {
                Ok(clip) => {
                    models_used.push("CLIP ViT-B/32 (SafeTensors)".into());
                    let to_f32 = |img: &DynamicImage| -> Vec<f32> {
                        img.to_rgb8()
                            .into_raw()
                            .into_iter()
                            .map(|b| f32::from(b) / 255.0)
                            .collect()
                    };
                    let orig_v = to_f32(original);
                    let prot_v = to_f32(protected);
                    let cos_direct = clip.cosine_similarity(&orig_v, &prot_v).unwrap_or(1.0);
                    let cos_jpeg = transform(protected, "jpeg-85")
                        .ok()
                        .and_then(|img| clip.cosine_similarity(&orig_v, &to_f32(&img)).ok())
                        .unwrap_or(1.0);
                    let cos_median = transform(protected, "median-filter-3x3")
                        .ok()
                        .and_then(|img| clip.cosine_similarity(&orig_v, &to_f32(&img)).ok())
                        .unwrap_or(1.0);

                    let match_class = if cos_direct < 0.70 {
                        ResultClassification::Passed
                    } else if cos_direct < 0.88 {
                        ResultClassification::PartiallyEffective
                    } else {
                        ResultClassification::Failed
                    };
                    let purify_class = if cos_median < 0.75 && cos_jpeg < 0.75 {
                        ResultClassification::Passed
                    } else if cos_median < 0.90 || cos_jpeg < 0.90 {
                        ResultClassification::PartiallyEffective
                    } else {
                        ResultClassification::Failed
                    };

                    return (
                        AttackTestResult {
                            category: AttackCategory::ReverseImageMatching,
                            model_name: "CLIP ViT-B/32".into(),
                            classification: match_class,
                            original_metric: Some("cos_sim = 1.000".into()),
                            protected_metric: Some(format!("cos_sim = {cos_direct:.4}")),
                            survived_durability: Some(cos_jpeg < 0.85),
                            details: format!("Direct CLIP cosine similarity: {cos_direct:.4}"),
                        },
                        AttackTestResult {
                            category: AttackCategory::AdversarialProtectionRemoval,
                            model_name: "CLIP ViT-B/32 + JPEG-85 / Median-3x3".into(),
                            classification: purify_class,
                            original_metric: Some("cos_sim = 1.000".into()),
                            protected_metric: Some(format!(
                                "JPEG-85: {cos_jpeg:.4}, Median-3x3: {cos_median:.4}"
                            )),
                            survived_durability: Some(cos_median < 0.85 && cos_jpeg < 0.85),
                            details: format!(
                                "After JPEG-85 ({cos_jpeg:.4}) and 3x3 median purification ({cos_median:.4})"
                            ),
                        },
                    );
                }
                Err(e) => {
                    let err_msg = format!("Failed to load CLIP model: {e}");
                    return (
                        AttackTestResult {
                            category: AttackCategory::ReverseImageMatching,
                            model_name: "CLIP ViT-B/32".into(),
                            classification: ResultClassification::Inconclusive,
                            original_metric: None,
                            protected_metric: None,
                            survived_durability: None,
                            details: err_msg.clone(),
                        },
                        AttackTestResult {
                            category: AttackCategory::AdversarialProtectionRemoval,
                            model_name: "CLIP ViT-B/32".into(),
                            classification: ResultClassification::Inconclusive,
                            original_metric: None,
                            protected_metric: None,
                            survived_durability: None,
                            details: err_msg,
                        },
                    );
                }
            }
        }
    }
    let _ = (original, protected, model_path, manifest_path, models_used);
    (
        AttackTestResult {
            category: AttackCategory::ReverseImageMatching,
            model_name: "CLIP ViT-B/32".into(),
            classification: ResultClassification::NotTested,
            original_metric: None,
            protected_metric: None,
            survived_durability: None,
            details: "No local CLIP ViT-B/32 SafeTensors model configured.".into(),
        },
        AttackTestResult {
            category: AttackCategory::AdversarialProtectionRemoval,
            model_name: "CLIP ViT-B/32 + Purification".into(),
            classification: ResultClassification::NotTested,
            original_metric: None,
            protected_metric: None,
            survived_durability: None,
            details: "No local CLIP ViT-B/32 SafeTensors model configured.".into(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_suite_produces_honest_classifications_without_models() {
        let orig = DynamicImage::new_rgb8(32, 32);
        let rep = run_validation(
            &orig,
            &orig,
            &ValidationConfig {
                mode: Some(ValidationMode::ComprehensiveProtectionTest),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(rep.durability.is_some());
        assert!(rep.tests.iter().all(|t| matches!(
            t.classification,
            ResultClassification::NotTested | ResultClassification::Unsupported
        )));
        let md = rep.to_markdown();
        assert!(md.contains("Validation Report"));
        assert!(md.contains("Not tested") || md.contains("Unsupported"));
    }
}
