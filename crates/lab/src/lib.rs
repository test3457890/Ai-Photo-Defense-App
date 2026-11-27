//! Reproducible image transformation measurements and Real-World Durability Suite.
//! Distinguishes simulated platform transformations from actual platform tests.
//! Pixel differences alone do not demonstrate that AI protection remains effective.

use anyhow::{Result, ensure};
use image::{DynamicImage, ImageBuffer, Rgb};
use photo_core::{ExportFormat, Quality, quality};
use serde::{Deserialize, Serialize};
use std::time::Instant;

pub mod validation;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Measurement {
    pub transformation: String,
    pub quality: Quality,
    pub elapsed_ms: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Report {
    pub schema_version: u32,
    pub scope: String,
    pub ai_efficacy: String,
    pub measurements: Vec<Measurement>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DurabilityMeasurement {
    pub transformation: String,
    pub category: String,
    /// True when the transform is a local simulation (e.g., Instagram/Facebook/AI enhance),
    /// false when it is a standard codec/geometric operation.
    pub is_simulated: bool,
    pub note: String,
    pub quality: Quality,
    pub elapsed_ms: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DurabilityReport {
    pub schema_version: u32,
    pub scope: String,
    pub measurements: Vec<DurabilityMeasurement>,
}

pub const DURABILITY_TRANSFORMS: &[(&str, &str, bool, &str)] = &[
    ("identity", "Baseline", false, "Unmodified decoded pixels"),
    ("jpeg-95", "Compression", false, "Standard JPEG quality 95"),
    ("jpeg-85", "Compression", false, "Standard JPEG quality 85"),
    ("jpeg-70", "Compression", false, "Standard JPEG quality 70"),
    (
        "jpeg-50",
        "Compression",
        false,
        "Aggressive JPEG quality 50",
    ),
    (
        "webp-lossless",
        "Compression",
        false,
        "Lossless WebP roundtrip",
    ),
    (
        "png-conversion",
        "Compression",
        false,
        "Lossless PNG re-encode",
    ),
    (
        "instagram-sim",
        "Social Platform (Simulated)",
        true,
        "Simulated: max side 1080px + JPEG q=82 (not an actual Instagram upload)",
    ),
    (
        "facebook-sim",
        "Social Platform (Simulated)",
        true,
        "Simulated: max side 2048px + JPEG q=78 (not an actual Facebook upload)",
    ),
    (
        "screenshot-sim",
        "Capture (Simulated)",
        true,
        "Simulated: 0.98x->1.0x bilinear resample + PNG",
    ),
    ("crop-center-80", "Geometric", false, "Center 80% crop"),
    ("crop-center-60", "Geometric", false, "Center 60% crop"),
    ("resize-half", "Geometric", false, "50% Lanczos3 downscale"),
    (
        "resize-1080",
        "Geometric",
        false,
        "Fit within 1080px max side",
    ),
    (
        "rotate-5deg",
        "Geometric",
        false,
        "5-degree rotation proxy (90% center crop + resample)",
    ),
    (
        "color-warmth",
        "Color & Tone",
        false,
        "Warmth channel shift (+6 R, -6 B)",
    ),
    ("brightness-plus15", "Color & Tone", false, "Brightness +15"),
    ("contrast-plus20", "Color & Tone", false, "Contrast +20%"),
    (
        "gaussian-blur-1",
        "Filtering",
        false,
        "Gaussian blur sigma=1.0",
    ),
    (
        "gaussian-blur-2",
        "Filtering",
        false,
        "Gaussian blur sigma=2.0",
    ),
    (
        "median-filter-3x3",
        "Filtering",
        false,
        "3x3 median filter (adversarial purifier test)",
    ),
    (
        "ai-enhance-sim",
        "Enhancement (Simulated)",
        true,
        "Simulated: blur(0.6) + unsharp sharpen proxy (not neural super-resolution)",
    ),
    (
        "multi-export-3x",
        "Re-export Chain",
        false,
        "3x sequential JPEG q=85 exports",
    ),
];

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

/// Evaluates all 23 Phase-5 durability transformations, explicitly marking simulated platform
/// transforms (`is_simulated = true`).
pub fn evaluate_durability(
    source: &DynamicImage,
    candidate: &DynamicImage,
) -> Result<DurabilityReport> {
    ensure!(
        source.width() == candidate.width() && source.height() == candidate.height(),
        "Durability inputs must have equal dimensions"
    );
    let mut measurements = Vec::with_capacity(DURABILITY_TRANSFORMS.len());
    for &(name, category, is_simulated, note) in DURABILITY_TRANSFORMS {
        let start = Instant::now();
        let a = transform(source, name)?;
        let b = transform(candidate, name)?;
        measurements.push(DurabilityMeasurement {
            transformation: name.into(),
            category: category.into(),
            is_simulated,
            note: note.into(),
            quality: quality(&a, &b)?,
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.,
        });
    }
    Ok(DurabilityReport {
        schema_version: 2,
        scope: "Pixel perturbation survival across codecs, geometric, color, filtering, and simulated platform transforms. Pixel survival alone does not prove AI model resistance.".into(),
        measurements,
    })
}

pub fn transform(x: &DynamicImage, name: &str) -> Result<DynamicImage> {
    Ok(match name {
        "identity" => x.clone(),
        "jpeg-95" | "jpeg-85" | "jpeg-70" | "jpeg-50" => {
            let q = name[5..].parse()?;
            photo_core::decode_export(&photo_core::encode(x, ExportFormat::Jpeg, q)?)?
        }
        "webp-lossless" => {
            photo_core::decode_export(&photo_core::encode(x, ExportFormat::WebP, 90)?)?
        }
        "png-conversion" => {
            photo_core::decode_export(&photo_core::encode(x, ExportFormat::Png, 95)?)?
        }
        "instagram-sim" => {
            let resized = resize_max_side(x, 1080);
            photo_core::decode_export(&photo_core::encode(&resized, ExportFormat::Jpeg, 82)?)?
        }
        "facebook-sim" => {
            let resized = resize_max_side(x, 2048);
            photo_core::decode_export(&photo_core::encode(&resized, ExportFormat::Jpeg, 78)?)?
        }
        "screenshot-sim" => {
            let w = x.width().max(2);
            let h = x.height().max(2);
            let down = x.resize_exact(
                ((w as f32 * 0.98) as u32).max(1),
                ((h as f32 * 0.98) as u32).max(1),
                image::imageops::FilterType::Triangle,
            );
            let up = down.resize_exact(w, h, image::imageops::FilterType::Triangle);
            photo_core::decode_export(&photo_core::encode(&up, ExportFormat::Png, 95)?)?
        }
        "resize-half" => x.resize_exact(
            (x.width() / 2).max(1),
            (x.height() / 2).max(1),
            image::imageops::FilterType::Lanczos3,
        ),
        "resize-1080" => resize_max_side(x, 1080),
        "crop-center-80" => crop_center(x, 4, 5),
        "crop-center-60" => crop_center(x, 3, 5),
        "rotate-5deg" => rotate_small_proxy(x, 5.0f32.to_radians()),
        "color-warmth" => adjust_rgb(x, 6, 0, -6),
        "brightness-plus15" => x.brighten(15),
        "contrast-plus20" => x.adjust_contrast(20.0),
        "gaussian-blur-1" => x.blur(1.0),
        "gaussian-blur-2" => x.blur(2.0),
        "median-filter-3x3" => median_3x3(x),
        "ai-enhance-sim" => x.blur(0.6).unsharpen(1.2, 2),
        "multi-export-3x" => {
            let mut cur = x.clone();
            for _ in 0..3 {
                cur =
                    photo_core::decode_export(&photo_core::encode(&cur, ExportFormat::Jpeg, 85)?)?;
            }
            cur
        }
        _ => anyhow::bail!("Unknown transformation: {name}"),
    })
}

fn resize_max_side(x: &DynamicImage, max_side: u32) -> DynamicImage {
    let w = x.width();
    let h = x.height();
    if w <= max_side && h <= max_side {
        return x.clone();
    }
    let scale = max_side as f32 / w.max(h) as f32;
    let nw = ((w as f32 * scale).round() as u32).max(1);
    let nh = ((h as f32 * scale).round() as u32).max(1);
    x.resize_exact(nw, nh, image::imageops::FilterType::Lanczos3)
}

fn crop_center(x: &DynamicImage, num: u32, den: u32) -> DynamicImage {
    let w = (x.width() * num / den).max(1);
    let h = (x.height() * num / den).max(1);
    x.crop_imm((x.width() - w) / 2, (x.height() - h) / 2, w, h)
}

fn adjust_rgb(x: &DynamicImage, dr: i16, dg: i16, db: i16) -> DynamicImage {
    let mut rgb = x.to_rgb8();
    for p in rgb.pixels_mut() {
        p[0] = (i16::from(p[0]) + dr).clamp(0, 255) as u8;
        p[1] = (i16::from(p[1]) + dg).clamp(0, 255) as u8;
        p[2] = (i16::from(p[2]) + db).clamp(0, 255) as u8;
    }
    DynamicImage::ImageRgb8(rgb)
}

fn rotate_small_proxy(x: &DynamicImage, angle_rad: f32) -> DynamicImage {
    let rgb = x.to_rgb8();
    let w = rgb.width() as i32;
    let h = rgb.height() as i32;
    let cx = w as f32 * 0.5;
    let cy = h as f32 * 0.5;
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();
    let mut out = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(rgb.width(), rgb.height());
    for y in 0..h {
        for x_px in 0..w {
            let dx = x_px as f32 - cx;
            let dy = y as f32 - cy;
            let sx = (dx * cos_a + dy * sin_a + cx).round() as i32;
            let sy = (-dx * sin_a + dy * cos_a + cy).round() as i32;
            let px = if sx >= 0 && sx < w && sy >= 0 && sy < h {
                *rgb.get_pixel(sx as u32, sy as u32)
            } else {
                *rgb.get_pixel(x_px as u32, y as u32)
            };
            out.put_pixel(x_px as u32, y as u32, px);
        }
    }
    crop_center(&DynamicImage::ImageRgb8(out), 9, 10)
}

fn median_3x3(x: &DynamicImage) -> DynamicImage {
    let rgb = x.to_rgb8();
    let (w, h) = rgb.dimensions();
    let mut out = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(w, h);
    for y in 0..h {
        for px in 0..w {
            let mut ch_vals = [[0u8; 9]; 3];
            let mut idx = 0;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let sx = (px as i32 + dx).clamp(0, w as i32 - 1) as u32;
                    let sy = (y as i32 + dy).clamp(0, h as i32 - 1) as u32;
                    let p = rgb.get_pixel(sx, sy).0;
                    ch_vals[0][idx] = p[0];
                    ch_vals[1][idx] = p[1];
                    ch_vals[2][idx] = p[2];
                    idx += 1;
                }
            }
            for ch in &mut ch_vals {
                ch.sort_unstable();
            }
            out.put_pixel(px, y, Rgb([ch_vals[0][4], ch_vals[1][4], ch_vals[2][4]]));
        }
    }
    DynamicImage::ImageRgb8(out)
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

    #[test]
    fn durability_suite_runs_all_transforms_and_marks_simulations() {
        let x = DynamicImage::new_rgb8(32, 32);
        let rep = evaluate_durability(&x, &x).unwrap();
        assert_eq!(rep.measurements.len(), DURABILITY_TRANSFORMS.len());
        assert!(
            rep.measurements
                .iter()
                .any(|m| m.is_simulated && m.transformation == "instagram-sim")
        );
        assert!(
            rep.measurements
                .iter()
                .any(|m| !m.is_simulated && m.transformation == "jpeg-85")
        );
    }
}
