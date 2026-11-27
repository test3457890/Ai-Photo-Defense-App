//! Pure-Rust local face detection using UltraFace RFB-320 (`Linzaer`, MIT) via `tract-onnx`.
//!
//! Security & licensing invariants:
//! - Only `.onnx` files from a user-selected local path are accepted; `.pt`/`.pth`/`.bin`/`.pkl`
//!   are refused.
//! - If a sibling `manifest.json` (or `<stem>.manifest.json`) is present, it is verified via
//!   `photo_models::verify_onnx` (schema 2, SHA-256, `local_use_approved`).
//! - Zero network access; nothing is ever downloaded at runtime.

use crate::Rect;
use anyhow::{Context, Result, bail, ensure};
use image::imageops::FilterType;
use std::path::Path;
use tract_onnx::prelude::*;

pub const INPUT_W: usize = 320;
pub const INPUT_H: usize = 240;

#[derive(Debug, Clone)]
pub struct FaceDetector {
    model: std::sync::Arc<TypedRunnableModel>,
}

impl FaceDetector {
    /// Loads a local UltraFace RFB-320 `.onnx` model (1x3x240x320 NCHW RGB input).
    pub fn load(onnx_path: &Path) -> Result<Self> {
        let meta = std::fs::symlink_metadata(onnx_path)?;
        ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "Face model must be a regular non-symlink file"
        );
        let ext = onnx_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ["pt", "pth", "bin", "pkl", "pickle"]
            .iter()
            .any(|f| ext.eq_ignore_ascii_case(f))
        {
            bail!("Refusing unsafe pickle/binary weights; only .onnx is permitted");
        }
        ensure!(
            ext.eq_ignore_ascii_case("onnx"),
            "Face model must be an .onnx file"
        );

        let manifest_path = onnx_path.with_file_name("manifest.json");
        let alt_manifest = onnx_path.with_extension("manifest.json");
        let chosen_manifest = if manifest_path.is_file() {
            Some(manifest_path)
        } else if alt_manifest.is_file() {
            Some(alt_manifest)
        } else {
            None
        };
        if let Some(mp) = chosen_manifest {
            let bytes = std::fs::read(&mp).context("Failed to read face model manifest")?;
            let manifest: photo_models::Manifest =
                serde_json::from_slice(&bytes).context("Invalid face model manifest.json")?;
            photo_models::verify_onnx(onnx_path, &manifest)?;
        }

        let model = tract_onnx::onnx()
            .model_for_path(onnx_path)
            .context("Failed to parse ONNX face model")?
            .with_input_fact(0, f32::fact([1, 3, INPUT_H, INPUT_W]).into())
            .context("Face model input shape mismatch (expected 1x3x240x320)")?
            .into_optimized()
            .context("Failed to optimize ONNX face model")?
            .into_runnable()
            .context("Failed to prepare runnable ONNX face model")?;

        Ok(Self { model })
    }

    /// Detects faces in `img`, returning bounding boxes in original image pixel coordinates.
    pub fn detect(
        &self,
        img: &image::DynamicImage,
        score_threshold: f32,
        iou_threshold: f32,
    ) -> Result<Vec<Rect>> {
        let orig_w = img.width().max(1);
        let orig_h = img.height().max(1);
        let resized = img
            .resize_exact(INPUT_W as u32, INPUT_H as u32, FilterType::Triangle)
            .to_rgb8();

        let plane = INPUT_W * INPUT_H;
        let mut nchw = vec![0.0_f32; 3 * plane];
        for (i, px) in resized.pixels().enumerate() {
            nchw[i] = (f32::from(px[0]) - 127.0) / 128.0;
            nchw[plane + i] = (f32::from(px[1]) - 127.0) / 128.0;
            nchw[2 * plane + i] = (f32::from(px[2]) - 127.0) / 128.0;
        }
        let tensor: Tensor = tract_ndarray::Array4::from_shape_vec((1, 3, INPUT_H, INPUT_W), nchw)
            .context("Failed to build input tensor")?
            .into();

        let outputs = self
            .model
            .run(tvec!(tensor.into()))
            .context("ONNX face inference failed")?;
        ensure!(
            outputs.len() >= 2,
            "Expected 2 output tensors (scores, boxes)"
        );

        let scores = outputs[0]
            .to_plain_array_view::<f32>()
            .context("Invalid scores tensor")?;
        let boxes = outputs[1]
            .to_plain_array_view::<f32>()
            .context("Invalid boxes tensor")?;

        let n_boxes = scores.shape().get(1).copied().unwrap_or(0);
        let mut cands = Vec::new();
        for i in 0..n_boxes {
            let conf = scores[[0, i, 1]];
            if conf >= score_threshold {
                let x1 = boxes[[0, i, 0]].clamp(0.0, 1.0);
                let y1 = boxes[[0, i, 1]].clamp(0.0, 1.0);
                let x2 = boxes[[0, i, 2]].clamp(0.0, 1.0);
                let y2 = boxes[[0, i, 3]].clamp(0.0, 1.0);
                if x2 > x1 && y2 > y1 {
                    cands.push((conf, [x1, y1, x2, y2]));
                }
            }
        }

        let kept = nms(&mut cands, iou_threshold);
        Ok(kept
            .into_iter()
            .filter_map(|(_, [x1, y1, x2, y2])| {
                let px1 = (x1 * orig_w as f32).floor().max(0.0) as u32;
                let py1 = (y1 * orig_h as f32).floor().max(0.0) as u32;
                let px2 = (x2 * orig_w as f32).ceil().min(orig_w as f32) as u32;
                let py2 = (y2 * orig_h as f32).ceil().min(orig_h as f32) as u32;
                let w = px2.saturating_sub(px1);
                let h = py2.saturating_sub(py1);
                (w > 0 && h > 0).then_some(Rect {
                    x: px1,
                    y: py1,
                    width: w,
                    height: h,
                })
            })
            .collect())
    }
}

fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let x1 = a[0].max(b[0]);
    let y1 = a[1].max(b[1]);
    let x2 = a[2].min(b[2]);
    let y2 = a[3].min(b[3]);
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let area_a = (a[2] - a[0]).max(0.0) * (a[3] - a[1]).max(0.0);
    let area_b = (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0);
    let denom = area_a + area_b - inter;
    if denom > 0.0 { inter / denom } else { 0.0 }
}

/// Greedy non-maximum suppression over `(score, [x1, y1, x2, y2])`.
pub fn nms(cands: &mut [(f32, [f32; 4])], iou_threshold: f32) -> Vec<(f32, [f32; 4])> {
    cands.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut kept = Vec::new();
    for &(score, bx) in cands.iter() {
        if !kept.iter().any(|(_, kb)| iou(&bx, kb) > iou_threshold) {
            kept.push((score, bx));
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nms_suppresses_overlapping_boxes() {
        let mut cands = vec![
            (0.95, [0.1, 0.1, 0.4, 0.4]),
            (0.80, [0.12, 0.12, 0.41, 0.41]),
            (0.85, [0.6, 0.6, 0.9, 0.9]),
        ];
        let kept = nms(&mut cands, 0.35);
        assert_eq!(kept.len(), 2);
        assert!((kept[0].0 - 0.95).abs() < 1e-6);
        assert!((kept[1].0 - 0.85).abs() < 1e-6);
    }

    #[test]
    fn refuses_pickle_extension() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("ultraface.pt");
        std::fs::write(&bad, b"not-onnx").unwrap();
        let err = FaceDetector::load(&bad).unwrap_err();
        assert!(format!("{err:#}").contains("Refusing unsafe pickle/binary weights"));
    }

    #[test]
    fn scan_removes_face_from_not_examined_only_when_model_ran() {
        let img = image::DynamicImage::new_rgb8(64, 64);
        let profile = crate::Profile::builtin("Maximum Privacy").unwrap();

        let r_without = crate::scan_with_faces(
            crate::ScanInput {
                bytes: &[],
                pixels: &img,
                ocr_text: None,
                extra_text: vec![],
            },
            &profile,
            None,
        );
        assert!(r_without.not_examined.contains(&crate::Category::Face));

        let face_box = Rect {
            x: 10,
            y: 10,
            width: 20,
            height: 20,
        };
        let r_with = crate::scan_with_faces(
            crate::ScanInput {
                bytes: &[],
                pixels: &img,
                ocr_text: None,
                extra_text: vec![],
            },
            &profile,
            Some(&[face_box]),
        );
        assert!(!r_with.not_examined.contains(&crate::Category::Face));
        assert!(
            r_with
                .findings
                .iter()
                .any(|f| f.category == crate::Category::Face && f.region == Some(face_box))
        );
    }

    #[test]
    #[ignore = "Requires UltraFace RFB-320 .onnx in APD_FACE_ONNX (and optional APD_FACE_IMAGE)"]
    fn detects_faces_with_ultraface_model() {
        let onnx = std::env::var("APD_FACE_ONNX").expect("APD_FACE_ONNX must be set");
        let det = FaceDetector::load(Path::new(&onnx)).unwrap();
        let img = if let Ok(p) = std::env::var("APD_FACE_IMAGE") {
            image::open(p).unwrap()
        } else {
            image::DynamicImage::new_rgb8(320, 240)
        };
        let boxes = det.detect(&img, 0.7, 0.35).unwrap();
        for b in &boxes {
            assert!(b.width > 0 && b.height > 0);
        }
    }
}
