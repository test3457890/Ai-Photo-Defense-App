//! Read-only forensic inspection. Values shown here are for the local user only; this report is
//! marked sensitive and must never be logged.
//!
//! Distinguishes: (a) structural facts read from the file, (b) cryptographic provenance, which
//! this build can only detect, not verify, and (c) heuristic indicators, which never prove
//! manipulation or AI generation.
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct JpegInfo {
    pub progressive: bool,
    pub quant_tables: usize,
    /// Estimated encoder quality from the luminance table (approximate, IJG scaling).
    pub estimated_quality: Option<u8>,
    pub chroma_subsampling: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForensicReport {
    pub sensitive: bool,
    pub container: String,
    pub size_bytes: usize,
    pub sha256: String,
    pub blake3: String,
    pub exif: Vec<(String, String)>,
    pub gps_present: bool,
    pub embedded_thumbnail_bytes: Option<usize>,
    pub jpeg: Option<JpegInfo>,
    pub icc_profile_present: bool,
    pub c2pa: String,
    pub structure: Vec<String>,
    pub indicators: Vec<String>,
    pub disclaimer: String,
}

const STD_LUMA: [u16; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113,
    92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];

/// DQT tables are stored in zig-zag order; maps zig-zag position to natural (row-major) index.
const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

fn jpeg_info(b: &[u8]) -> Option<JpegInfo> {
    if !b.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    let (mut i, mut progressive, mut tables, mut q, mut sub) = (2, false, 0, None, None);
    let mut guard = 0;
    while i + 4 <= b.len() && guard < 4096 {
        guard += 1;
        if b[i] != 0xff {
            break;
        }
        let m = b[i + 1];
        if m == 0xd9 || m == 0xda {
            break;
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        let Some(seg) = b.get(i + 4..i + 2 + len) else {
            break;
        };
        match m {
            0xc2 => progressive = true,
            0xdb => {
                let mut k = 0;
                while k < seg.len() {
                    let pq = seg[k] >> 4;
                    let id = seg[k] & 15;
                    let n = if pq == 0 { 64 } else { 128 };
                    let Some(t) = seg.get(k + 1..k + 1 + n) else {
                        break;
                    };
                    tables += 1;
                    if id == 0 && pq == 0 && q.is_none() {
                        let ratio: f64 = t
                            .iter()
                            .zip(ZIGZAG)
                            .map(|(&v, z)| f64::from(v) * 100. / f64::from(STD_LUMA[z]))
                            .sum::<f64>()
                            / 64.;
                        let quality = if ratio <= 100. {
                            (200. - ratio) / 2.
                        } else {
                            5000. / ratio
                        };
                        q = Some(quality.clamp(1., 100.).round() as u8);
                    }
                    k += 1 + n;
                }
            }
            0xc0 | 0xc1 if seg.len() >= 9 && seg[5] >= 3 => {
                let h = seg[7] >> 4;
                let v = seg[7] & 15;
                sub = Some(match (h, v) {
                    (1, 1) => "4:4:4".into(),
                    (2, 1) => "4:2:2".into(),
                    (2, 2) => "4:2:0".into(),
                    _ => format!("{h}x{v}"),
                });
            }
            _ => {}
        }
        if m == 0xc2 && seg.len() >= 9 && seg[5] >= 3 {
            let (h, v) = (seg[7] >> 4, seg[7] & 15);
            sub = Some(format!("{h}x{v}"));
        }
        i += 2 + len;
    }
    Some(JpegInfo {
        progressive,
        quant_tables: tables,
        estimated_quality: q,
        chroma_subsampling: sub,
    })
}

fn c2pa_status(b: &[u8]) -> String {
    #[cfg(feature = "c2pa")]
    {
        let s = crate::c2pa_cred::inspect_status(b);
        if s != "No C2PA manifest found" {
            return s;
        }
    }
    let jumbf = b.windows(4).any(|w| w == b"jumb") && b.windows(4).any(|w| w == b"c2pa");
    let png_chunk = b.windows(4).any(|w| w == b"caBX");
    if jumbf || png_chunk {
        "C2PA manifest data present: NOT cryptographically verified (verification is not implemented in this build)".into()
    } else {
        "No C2PA manifest found".into()
    }
}

pub fn inspect(bytes: &[u8]) -> ForensicReport {
    let meta = crate::metadata::inspect(bytes);
    let mut exif_fields = vec![];
    let (mut gps, mut thumb, mut software) = (false, None, None);
    if let Ok(e) = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(bytes)) {
        for f in e.fields().take(500) {
            let v: String = f
                .display_value()
                .with_unit(&e)
                .to_string()
                .chars()
                .filter(|c| !c.is_control())
                .take(120)
                .collect();
            if f.tag.to_string().starts_with("GPS") {
                gps = true;
            }
            if f.tag == exif::Tag::Software {
                software = Some(v.clone());
            }
            if f.tag == exif::Tag::JPEGInterchangeFormatLength
                && let Some(n) = f.value.get_uint(0)
            {
                thumb = Some(n as usize);
            }
            exif_fields.push((format!("{} ({})", f.tag, f.ifd_num), v));
        }
    }
    let icc = meta
        .findings
        .iter()
        .any(|f| f.category == crate::Category::IccProfile);
    let mut indicators = vec![];
    if let Some(s) = &software {
        indicators.push(format!(
            "Software tag names an application: \"{s}\" (files are often re-saved legitimately)"
        ));
    }
    let jpeg = jpeg_info(bytes);
    if let Some(j) = &jpeg
        && j.quant_tables > 4
    {
        indicators.push("Unusual number of quantization tables".into());
    }
    if exif_fields.is_empty() && jpeg.is_some() {
        indicators.push("No EXIF data (common after export, screenshots or social media; not evidence of manipulation)".into());
    }
    let fp = |b: &[u8]| {
        use sha2::Digest;
        (
            format!("{:x}", sha2::Sha256::digest(b)),
            blake3::hash(b).to_hex().to_string(),
        )
    };
    let (sha256, b3) = fp(bytes);
    ForensicReport {
        sensitive: true,
        container: meta.container.clone(),
        size_bytes: bytes.len(),
        sha256,
        blake3: b3,
        exif: exif_fields,
        gps_present: gps || meta.findings.iter().any(|f| f.category == crate::Category::GpsLocation),
        embedded_thumbnail_bytes: thumb,
        jpeg,
        icc_profile_present: icc,
        c2pa: c2pa_status(bytes),
        structure: meta.findings.iter().map(|f| f.description.clone()).collect(),
        indicators,
        disclaimer: "Structural facts are read from the file. Heuristic indicators are not proof of editing or AI generation. Provenance is only trustworthy after cryptographic verification.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jpeg_quality_estimate_and_structure() {
        for q in [50u8, 75, 90] {
            let mut c = std::io::Cursor::new(vec![]);
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut c, q)
                .encode_image(&image::DynamicImage::new_rgb8(32, 32))
                .unwrap();
            let r = inspect(&c.into_inner());
            let est = r.jpeg.unwrap().estimated_quality.unwrap();
            assert!(est.abs_diff(q) <= 3, "q {q} est {est}");
            assert_eq!(r.c2pa, "No C2PA manifest found");
            assert!(r.sensitive && !r.gps_present);
        }
    }
    #[test]
    fn hostile_and_c2pa_detection() {
        let _ = inspect(b"\xff\xd8\xff\xdb\xff\xff");
        let _ = inspect(&[]);
        let mut fake = b"\x89PNG\r\n\x1a\n".to_vec();
        fake.extend_from_slice(b"....caBX....");
        assert!(
            inspect(&fake)
                .c2pa
                .contains("NOT cryptographically verified")
        );
    }
}
