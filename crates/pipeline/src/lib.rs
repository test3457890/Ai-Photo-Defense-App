//! Coordinated secure export: preserve original -> inspect metadata -> initial scan -> edits
//! and redaction -> flatten -> post-edit scan -> (protection: not available) -> metadata-free
//! encode -> reopen and independently verify the encoded bytes -> publish without overwriting.
use anyhow::{Context, Result, ensure};
use image::DynamicImage;
use photo_core::{ExportFormat, Photo, Quality};
use photo_edit::{Concealment, EditStack, Mask, Operation, Selection};
use photo_privacy::{
    Category, Finding, Profile, ScanInput, ScanReport, profiles::RedactionPreference,
};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub mod batch;

#[derive(Debug, Clone, Serialize)]
pub struct RedactionCheck {
    pub bounds: Option<photo_edit::Rect>,
    pub pixels_checked: usize,
    pub mismatched: usize,
    pub passed: bool,
}

/// Results of re-opening and inspecting the encoded file, not the in-memory render.
#[derive(Debug, Clone, Serialize)]
pub struct Verification {
    pub reopened: bool,
    pub metadata_findings: Vec<Finding>,
    pub remaining_findings: Vec<Finding>,
    pub redactions: Vec<RedactionCheck>,
    pub redaction_geometry_verified: bool,
    pub transparency: Option<String>,
    pub visual_only_concealment: bool,
    pub encoded_vs_render: Quality,
    pub passed: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportReport {
    pub schema_version: u32,
    pub pipeline: Vec<String>,
    pub profile: String,
    pub source_hash: photo_core::Fingerprint,
    pub output_hash: photo_core::Fingerprint,
    pub output_dimensions: (u32, u32),
    pub edits: Vec<String>,
    pub initial_scan: ScanReport,
    pub verification: Verification,
    pub adversarial_protection: String,
    pub watermark: String,
    pub limitations: Vec<String>,
}

pub struct Prepared {
    pub bytes: Vec<u8>,
    pub rendered: DynamicImage,
    pub report: ExportReport,
}

pub fn scan_photo(photo: &Photo, profile: &Profile) -> ScanReport {
    photo_privacy::scan(
        ScanInput {
            bytes: &photo.bytes,
            pixels: &photo.pixels,
            ocr_text: None,
            extra_text: vec![],
        },
        profile,
    )
}

/// Builds secure-redaction operations for every located finding (e.g. decoded codes).
pub fn conceal_findings(
    findings: &[Finding],
    preference: RedactionPreference,
    grow: i32,
) -> Option<Operation> {
    let regions: Vec<Selection> = findings
        .iter()
        .filter_map(|f| f.region)
        .map(|r| Selection::Rect(photo_edit::Rect::new(r.x, r.y, r.width, r.height)))
        .collect();
    if regions.is_empty() {
        return None;
    }
    let method = match preference {
        RedactionPreference::OpaqueBlack => Concealment::BLACK,
        RedactionPreference::OpaqueWhite => Concealment::WHITE,
        RedactionPreference::Blur => Concealment::GaussianBlur { sigma: 12. },
        RedactionPreference::Pixelate => Concealment::Pixelate { block: 16 },
    };
    Some(Operation::Conceal {
        selection: Selection::Refined {
            inner: Box::new(Selection::Union(regions)),
            radius: grow,
        },
        method,
        invert: false,
    })
}

/// High-assurance pre-export review. Returns blocking risks; empty means none were found by
/// the available checks (which never guarantees that no private information remains).
pub fn pre_export_review(
    edits: &EditStack,
    scan: Option<(&ScanReport, &[bool])>,
    auto_redacted: bool,
) -> Vec<String> {
    let mut risks = vec![];
    match scan {
        None if !auto_redacted => {
            risks.push("No privacy scan has been run on this photograph.".into())
        }
        None => {}
        Some((report, dismissed)) => {
            let unresolved = report
                .findings
                .iter()
                .zip(dismissed)
                .filter(|(f, d)| !**d && f.region.is_some())
                .count();
            let redactions = edits
                .active()
                .iter()
                .filter(|o| o.is_secure_redaction())
                .count();
            if unresolved > 0 && redactions == 0 && !auto_redacted {
                risks.push(format!(
                    "{unresolved} located finding(s) are neither redacted nor dismissed."
                ));
            }
            let uncertain = report
                .findings
                .iter()
                .zip(dismissed)
                .filter(|(f, d)| !**d && f.evidence == photo_privacy::Evidence::Contextual)
                .count();
            if uncertain > 0 {
                risks.push(format!(
                    "{uncertain} uncertain (contextual) finding(s) need review or dismissal."
                ));
            }
        }
    }
    if edits.has_visual_only_concealment() {
        risks.push("Blur or pixelation is visual concealment only; use opaque redaction for sensitive content.".into());
    }
    risks
}

/// Runs every pipeline stage in memory. Nothing is written to disk.
pub fn prepare(
    photo: &Photo,
    edits: &EditStack,
    format: ExportFormat,
    quality: u8,
    profile: &Profile,
) -> Result<Prepared> {
    let initial_scan = scan_photo(photo, profile);
    let rendered = edits.render(&photo.pixels).context("Apply edits")?;
    let bytes = photo_core::encode(&rendered, format, quality)?;
    let verification = verify(&bytes, &rendered, edits, &photo.pixels, format, profile)?;
    let mut limitations = initial_scan.limitations.clone();
    limitations.push(
        "Source ICC profiles are not converted; colors of wide-gamut images can shift.".into(),
    );
    let report = ExportReport {
        schema_version: 1,
        pipeline: [
            "original preserved (read-only)",
            "metadata inspected",
            "initial privacy scan",
            "edits and redaction applied",
            "flattened to one layer; hidden color under full transparency cleared",
            "post-edit verification scan",
            "adversarial protection: not applied",
            "watermark: not applied",
            "encoded from pixels only (no source metadata, thumbnails or previews)",
            "encoded file reopened and verified",
        ]
        .map(String::from)
        .to_vec(),
        profile: profile.name.clone(),
        source_hash: photo.fingerprint.clone(),
        output_hash: photo_core::fingerprint(&bytes),
        output_dimensions: (rendered.width(), rendered.height()),
        edits: edits.active().iter().map(Operation::label).collect(),
        initial_scan,
        verification,
        adversarial_protection: "Not applied: no validated defense model is installed. This file has no demonstrated resistance to AI editing, face swapping or training.".into(),
        watermark: "Not applied".into(),
        limitations,
    };
    Ok(Prepared {
        bytes,
        rendered,
        report,
    })
}

/// Reopens encoded bytes and checks them independently of the render that produced them.
pub fn verify(
    bytes: &[u8],
    rendered: &DynamicImage,
    edits: &EditStack,
    original: &DynamicImage,
    format: ExportFormat,
    profile: &Profile,
) -> Result<Verification> {
    let decoded =
        photo_core::decode_export(bytes).context("Exported file could not be reopened")?;
    ensure!(
        decoded.width() == rendered.width() && decoded.height() == rendered.height(),
        "Exported dimensions differ from the edited image"
    );
    let encoded_vs_render = photo_core::quality(rendered, &decoded)?;
    let meta = photo_privacy::metadata::inspect(bytes);
    let metadata_findings = meta.findings;
    let post = photo_privacy::scan(
        ScanInput {
            bytes,
            pixels: &decoded,
            ocr_text: None,
            extra_text: vec![],
        },
        profile,
    );
    let remaining_findings: Vec<Finding> = post
        .findings
        .into_iter()
        .filter(|f| f.category.detector() != photo_privacy::Detector::Metadata)
        .collect();
    let lossy = format == ExportFormat::Jpeg;
    let mut redactions = vec![];
    let regions = edits.final_redactions(original)?;
    let redaction_geometry_verified = regions.is_some();
    let rgba = decoded.to_rgba8();
    for (mask, rgb) in regions.unwrap_or_default() {
        // JPEG blocks blur redaction edges; check the interior with a tolerance instead.
        let check: Mask = if lossy { mask.refine(-3) } else { mask.clone() };
        let tolerance = if lossy { 24 } else { 0 };
        let mut n = 0;
        let mut bad = 0;
        for (x, y) in check.iter_set() {
            n += 1;
            let p = rgba.get_pixel(x, y);
            if p[3] != 255 || (0..3).any(|c| p[c].abs_diff(rgb[c]) > tolerance) {
                bad += 1;
            }
        }
        redactions.push(RedactionCheck {
            bounds: mask.bounds(),
            pixels_checked: n,
            mismatched: bad,
            passed: bad == 0,
        });
    }
    let hidden_color = rgba
        .pixels()
        .filter(|p| p[3] == 0 && (p[0] | p[1] | p[2]) != 0)
        .count();
    let partial = rgba.pixels().filter(|p| p[3] != 255).count();
    let transparency = (partial > 0).then(|| {
        format!(
            "{partial} non-opaque pixels; {hidden_color} fully transparent pixels retain color data"
        )
    });
    let mut warnings = vec![];
    let visual_only_concealment = edits.has_visual_only_concealment();
    if visual_only_concealment {
        warnings.push("Blur or pixelation was used. These are visual concealment and can leave recoverable or inferable detail; use opaque redaction for sensitive information.".into());
    }
    if !redaction_geometry_verified {
        warnings.push("Geometry edits follow a redaction, so redaction regions could not be re-located for verification.".into());
    }
    if !remaining_findings.is_empty() {
        warnings.push(format!(
            "{} privacy finding(s) remain visible in the exported pixels.",
            remaining_findings.len()
        ));
    }
    if transparency.is_some() {
        warnings.push("The export contains transparency; check that no content is hidden in transparent areas.".into());
    }
    let passed = metadata_findings.is_empty()
        && remaining_findings
            .iter()
            .all(|f| f.category != Category::AuthenticationCode && !f.category.is_credential())
        && redactions.iter().all(|r| r.passed)
        && hidden_color == 0;
    Ok(Verification {
        reopened: true,
        metadata_findings,
        remaining_findings,
        redactions,
        redaction_geometry_verified,
        transparency,
        visual_only_concealment,
        encoded_vs_render,
        passed,
        warnings,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Saved {
    pub image: PathBuf,
    pub report: PathBuf,
    pub verification_passed: bool,
}

/// Writes the verified image and its report as new files. Refuses to publish a file whose
/// verification failed unless the caller explicitly accepts unresolved findings.
pub fn publish(
    prepared: &Prepared,
    source: &Path,
    out_dir: &Path,
    format: ExportFormat,
    accept_unresolved: bool,
) -> Result<Saved> {
    ensure!(
        prepared.report.verification.passed || accept_unresolved,
        "Export blocked: final verification failed ({}). Review the findings or explicitly accept them.",
        prepared.report.verification.warnings.join(" ")
    );
    let stem = format!("{}-private", photo_core::safe_stem(source)?);
    let image = photo_core::save_unique(out_dir, &stem, format.extension(), &prepared.bytes)?;
    let report_stem = image
        .file_stem()
        .and_then(|s| s.to_str())
        .context("Report name")?
        .to_string();
    let report = photo_core::save_unique(
        out_dir,
        &report_stem,
        &format!("{}.json", format.extension()),
        &serde_json::to_vec_pretty(&prepared.report)?,
    )
    .with_context(|| format!("Image saved at {}, but report failed", image.display()))?;
    Ok(Saved {
        image,
        report,
        verification_passed: prepared.report.verification.passed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use photo_edit::Rect;
    use rxing::Writer;
    pub(crate) fn qr_photo(payload: &str) -> Photo {
        let m = rxing::MultiFormatWriter
            .encode(payload, &rxing::BarcodeFormat::QR_CODE, 200, 200)
            .unwrap();
        let mut img = image::RgbImage::from_pixel(400, 300, image::Rgb([200, 190, 180]));
        for y in 0..m.getHeight() {
            for x in 0..m.getWidth() {
                img.put_pixel(
                    x + 100,
                    y + 50,
                    if m.get(x, y) {
                        image::Rgb([0; 3])
                    } else {
                        image::Rgb([255; 3])
                    },
                );
            }
        }
        let pixels = DynamicImage::ImageRgb8(img);
        let bytes = photo_core::encode(&pixels, ExportFormat::Png, 95).unwrap();
        Photo {
            pixels,
            fingerprint: photo_core::fingerprint(&bytes),
            format: image::ImageFormat::Png,
            bytes,
            raw: None,
        }
    }
    #[test]
    fn unredacted_credential_code_blocks_export_and_redaction_fixes_it() {
        let photo = qr_photo("otpauth://totp/Example:x?secret=JBSWY3DPEHPK3PXP");
        let profile = Profile::builtin("Cybersecurity").unwrap();
        let p = prepare(&photo, &EditStack::new(), ExportFormat::Png, 95, &profile).unwrap();
        assert!(!p.report.verification.passed);
        let d = tempfile::tempdir().unwrap();
        assert!(publish(&p, Path::new("a.png"), d.path(), ExportFormat::Png, false).is_err());
        assert_eq!(
            std::fs::read_dir(d.path()).unwrap().count(),
            0,
            "nothing written when blocked"
        );

        let mut edits = EditStack::new();
        edits
            .push(
                conceal_findings(
                    &p.report.initial_scan.findings,
                    RedactionPreference::OpaqueBlack,
                    4,
                )
                .unwrap(),
            )
            .unwrap();
        let p = prepare(&photo, &edits, ExportFormat::Png, 95, &profile).unwrap();
        let v = &p.report.verification;
        assert!(v.passed, "{:?}", v.warnings);
        assert!(v.metadata_findings.is_empty() && v.remaining_findings.is_empty());
        assert!(
            v.redactions
                .iter()
                .all(|r| r.passed && r.pixels_checked > 1000)
        );
        let saved = publish(&p, Path::new("a.png"), d.path(), ExportFormat::Png, false).unwrap();
        let report = std::fs::read_to_string(&saved.report).unwrap();
        assert!(
            !report.contains("JBSWY3DPEHPK3PXP"),
            "secret never reaches the report"
        );
        // The exported bytes carry no original pixels under the redaction.
        let out = photo_core::load(&saved.image).unwrap().pixels.to_rgb8();
        assert_eq!(out.get_pixel(200, 150), &image::Rgb([0, 0, 0]));
    }
    #[test]
    fn jpeg_redaction_checked_with_tolerance() {
        let photo = qr_photo("plain text");
        let mut edits = EditStack::new();
        edits
            .push(Operation::Conceal {
                selection: Selection::Rect(Rect::new(10, 10, 80, 60)),
                method: Concealment::BLACK,
                invert: false,
            })
            .unwrap();
        let p = prepare(
            &photo,
            &edits,
            ExportFormat::Jpeg,
            85,
            &Profile::builtin("Maximum Privacy").unwrap(),
        )
        .unwrap();
        assert!(p.report.verification.redactions[0].passed);
        assert!(p.report.verification.metadata_findings.is_empty());
    }
    #[test]
    fn visual_concealment_warns() {
        let photo = qr_photo("plain text");
        let mut edits = EditStack::new();
        edits
            .push(Operation::Conceal {
                selection: Selection::Rect(Rect::new(10, 10, 80, 60)),
                method: Concealment::GaussianBlur { sigma: 6. },
                invert: false,
            })
            .unwrap();
        let p = prepare(
            &photo,
            &edits,
            ExportFormat::Png,
            95,
            &Profile::builtin("Social Media").unwrap(),
        )
        .unwrap();
        assert!(p.report.verification.visual_only_concealment);
        assert!(
            p.report
                .verification
                .warnings
                .iter()
                .any(|w| w.contains("visual concealment"))
        );
    }
}
