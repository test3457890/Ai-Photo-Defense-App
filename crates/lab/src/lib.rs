//! Reproducible image transformation measurements. These are NOT AI attack success metrics.
use anyhow::{Result, ensure};
use image::DynamicImage;
use photo_core::{ExportFormat, Quality, quality};
use serde::Serialize;
use std::time::Instant;
#[derive(Debug, Serialize, Clone)]
pub struct Measurement {
    pub transformation: String,
    pub quality: Quality,
    pub elapsed_ms: f64,
}
#[derive(Debug, Serialize, Clone)]
pub struct Report {
    pub schema_version: u32,
    pub scope: String,
    pub ai_efficacy: String,
    pub measurements: Vec<Measurement>,
}
/// Applies the same transformation to source and candidate so metrics measure their
/// remaining differences, not displacement introduced by resize/crop themselves.
pub fn evaluate(source: &DynamicImage, candidate: &DynamicImage) -> Result<Report> {
    ensure!(
        source.width() == candidate.width() && source.height() == candidate.height(),
        "Lab inputs must have equal dimensions"
    );
    let mut measurements = vec![];
    for name in [
        "identity",
        "jpeg-95",
        "jpeg-85",
        "jpeg-70",
        "webp-lossless",
        "resize-half",
        "crop-center-80",
        "gaussian-blur-1",
    ] {
        let start = Instant::now();
        let a = transform(source, name)?;
        let b = transform(candidate, name)?;
        measurements.push(Measurement {
            transformation: name.into(),
            quality: quality(&a, &b)?,
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.,
        });
    }
    Ok(Report {
        schema_version: 1,
        scope: "Decoded-pixel perturbation survival only; not model resistance".into(),
        ai_efficacy: "not evaluated".into(),
        measurements,
    })
}
fn transform(x: &DynamicImage, name: &str) -> Result<DynamicImage> {
    Ok(match name {
        "identity" => x.clone(),
        "jpeg-95" | "jpeg-85" | "jpeg-70" => {
            let q = name[5..].parse()?;
            photo_core::decode_export(&photo_core::encode(x, ExportFormat::Jpeg, q)?)?
        }
        "webp-lossless" => {
            photo_core::decode_export(&photo_core::encode(x, ExportFormat::WebP, 90)?)?
        }
        "resize-half" => x.resize_exact(
            (x.width() / 2).max(1),
            (x.height() / 2).max(1),
            image::imageops::FilterType::Lanczos3,
        ),
        "crop-center-80" => {
            let w = (x.width() * 4 / 5).max(1);
            let h = (x.height() * 4 / 5).max(1);
            x.crop_imm((x.width() - w) / 2, (x.height() - h) / 2, w, h)
        }
        "gaussian-blur-1" => x.blur(1.),
        _ => anyhow::bail!("Unknown transformation"),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identical_has_zero_difference_for_all_transforms() {
        let x = DynamicImage::new_rgb8(16, 16);
        let r = evaluate(&x, &x).unwrap();
        assert_eq!(r.measurements.len(), 8);
        assert!(r.measurements.iter().all(|m| m.quality.mse_rgb == 0.));
        assert_eq!(r.ai_efficacy, "not evaluated");
    }
}
