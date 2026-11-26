//! Local QR/barcode detection with rxing (Apache-2.0, port of ZXing). Payloads are classified
//! and returned only for in-memory text scanning. Links are never opened, nothing encoded is
//! executed, and payload text is never written to findings.
use crate::{Category, Evidence, Finding, Rect, Source};
use anyhow::Result;
use image::DynamicImage;

/// Tile size and overlap for high-resolution scanning of small background codes.
const TILE: u32 = 1024;
const OVERLAP: u32 = 256;
const MAX_TILES: usize = 64;

pub struct DetectedCode {
    pub finding: Finding,
    pub payload: String,
    pub region: Option<Rect>,
}
pub struct Detection {
    pub codes: Vec<DetectedCode>,
    pub tiles: usize,
}

pub fn detect(img: &DynamicImage) -> Result<Detection> {
    let luma = img.to_luma8();
    let (w, h) = luma.dimensions();
    let mut found: Vec<(String, String, Rect)> = vec![];
    let mut tiles = 0;
    let run = |x0: u32, y0: u32, tw: u32, th: u32, found: &mut Vec<(String, String, Rect)>| {
        let tile = image::imageops::crop_imm(&luma, x0, y0, tw, th).to_image();
        // rxing reports "not found" as an error; that is simply an empty result.
        let results =
            rxing::helpers::detect_multiple_in_luma(tile.into_raw(), tw, th).unwrap_or_default();
        for r in results {
            let pts = r.getRXingResultPoints();
            let region = bbox(pts, x0, y0, w, h);
            let format = format!("{:?}", r.getBarcodeFormat());
            let text = r.getText().to_string();
            if !found.iter().any(|(f, t, _)| *f == format && *t == text) {
                found.push((format, text, region));
            }
        }
    };
    run(0, 0, w, h, &mut found);
    tiles += 1;
    if w > TILE || h > TILE {
        let step = TILE - OVERLAP;
        'outer: for y0 in (0..h).step_by(step as usize) {
            for x0 in (0..w).step_by(step as usize) {
                if tiles >= MAX_TILES {
                    break 'outer;
                }
                run(x0, y0, TILE.min(w - x0), TILE.min(h - y0), &mut found);
                tiles += 1;
            }
        }
    }
    let codes = found
        .into_iter()
        .map(|(format, payload, region)| {
            let (category, evidence, description) = classify(&format, &payload);
            DetectedCode {
                finding: Finding::new(
                    category,
                    evidence,
                    Source::Barcode,
                    Some(region),
                    description,
                    Some(payload.chars().count()),
                ),
                payload,
                region: Some(region),
            }
        })
        .collect();
    Ok(Detection { codes, tiles })
}

fn bbox(points: &[rxing::Point], x0: u32, y0: u32, w: u32, h: u32) -> Rect {
    if points.is_empty() {
        return Rect {
            x: x0,
            y: y0,
            width: 1,
            height: 1,
        };
    }
    let (mut a, mut b, mut c, mut d) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in points {
        a = a.min(p.x);
        b = b.min(p.y);
        c = c.max(p.x);
        d = d.max(p.y);
    }
    // Finder-pattern centres sit inside the symbol; pad so concealment covers the whole code.
    let pad = ((c - a).max(d - b) * 0.25).max(8.);
    let x = (a - pad).max(0.) as u32 + x0;
    let y = (b - pad).max(0.) as u32 + y0;
    let x2 = ((c + pad) as u32 + x0).min(w);
    let y2 = ((d + pad) as u32 + y0).min(h);
    Rect {
        x,
        y,
        width: x2.saturating_sub(x).max(1),
        height: y2.saturating_sub(y).max(1),
    }
}

fn classify(format: &str, payload: &str) -> (Category, Evidence, String) {
    let p = payload.trim_start();
    let lower = p.to_ascii_lowercase();
    let kind = if format.contains("QR") {
        Category::QrCode
    } else {
        Category::Barcode
    };
    if lower.starts_with("otpauth") {
        return (
            Category::AuthenticationCode,
            Evidence::Confirmed,
            format!("{format}: authenticator enrollment secret"),
        );
    }
    if lower.starts_with("wifi:") {
        return (
            Category::WifiCredentials,
            Evidence::Confirmed,
            format!("{format}: Wi-Fi network credentials"),
        );
    }
    if [
        "bitcoin:",
        "ethereum:",
        "lightning:",
        "upi://",
        "bank:",
        "bcd\n",
    ]
    .iter()
    .any(|s| lower.starts_with(s))
        || p.starts_with("000201")
    {
        return (
            Category::PaymentCode,
            Evidence::Confirmed,
            format!("{format}: payment request code"),
        );
    }
    if p.starts_with("@\n") && p.contains("ANSI ") {
        return (
            Category::DriversLicense,
            Evidence::Confirmed,
            format!("{format}: AAMVA identity card barcode"),
        );
    }
    if p.starts_with("M1") && p.len() >= 58 {
        return (
            Category::IdentityDocument,
            Evidence::Pattern,
            format!("{format}: boarding pass barcode (BCBP)"),
        );
    }
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return (
            kind,
            Evidence::Confirmed,
            format!("{format}: link (not opened)"),
        );
    }
    if lower.starts_with("begin:vcard") || lower.starts_with("mecard:") {
        return (kind, Evidence::Confirmed, format!("{format}: contact card"));
    }
    (
        kind,
        Evidence::Confirmed,
        format!("{format}: decoded locally"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rxing::Writer;
    fn qr(payload: &str, size: i32) -> DynamicImage {
        let m = rxing::MultiFormatWriter
            .encode(payload, &rxing::BarcodeFormat::QR_CODE, size, size)
            .unwrap();
        let (w, h) = (m.getWidth(), m.getHeight());
        DynamicImage::ImageLuma8(image::GrayImage::from_fn(w, h, |x, y| {
            image::Luma([if m.get(x, y) { 0 } else { 255 }])
        }))
    }
    #[test]
    fn detects_and_classifies_without_exposing_payload() {
        let img = qr("WIFI:T:WPA;S:ExampleNet;P:example-pass;;", 240);
        let d = detect(&img).unwrap();
        assert_eq!(d.codes.len(), 1);
        let f = &d.codes[0].finding;
        assert_eq!(f.category, Category::WifiCredentials);
        let r = f.region.unwrap();
        assert!(r.width > 50 && r.x + r.width <= 240);
        assert!(!serde_json::to_string(f).unwrap().contains("example-pass"));
    }
    #[test]
    fn small_code_in_large_image_found_by_tiling() {
        let code = qr("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP", 150);
        let mut big = image::GrayImage::from_pixel(3000, 2000, image::Luma([255]));
        image::imageops::overlay(&mut big, &code.to_luma8(), 2600, 1700);
        let d = detect(&DynamicImage::ImageLuma8(big)).unwrap();
        assert!(d.tiles > 1);
        let f = d
            .codes
            .iter()
            .find(|c| c.finding.category == Category::AuthenticationCode)
            .expect("found");
        assert!(f.region.unwrap().x > 2400);
    }
    #[test]
    fn blank_image_has_no_codes() {
        assert!(
            detect(&DynamicImage::new_rgb8(64, 64))
                .unwrap()
                .codes
                .is_empty()
        );
    }
}
