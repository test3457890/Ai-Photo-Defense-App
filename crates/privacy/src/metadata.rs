//! Container-level metadata inspection over untrusted bytes. Every parser is bounds-checked
//! and iteration-limited; nothing here decodes pixels or follows references outside the buffer.
use crate::{Category, Evidence, Finding, Source};
use serde::Serialize;

const MAX_SEGMENTS: usize = 4096;

#[derive(Debug, Clone, Default, Serialize)]
pub struct MetadataReport {
    pub container: String,
    pub findings: Vec<Finding>,
    /// Free-text metadata values, kept in memory only for sensitive-text scanning.
    #[serde(skip)]
    pub text_values: Vec<String>,
}
impl MetadataReport {
    fn add(&mut self, category: Category, evidence: Evidence, detail: impl Into<String>) {
        self.findings.push(Finding::new(
            category,
            evidence,
            Source::Metadata,
            None,
            detail.into(),
            None,
        ));
    }
}

pub fn inspect(bytes: &[u8]) -> MetadataReport {
    let mut r = MetadataReport::default();
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        r.container = "JPEG".into();
        jpeg(bytes, &mut r);
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        r.container = "PNG".into();
        png(bytes, &mut r);
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        r.container = "WebP".into();
        webp(bytes, &mut r);
    } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        r.container = "TIFF".into();
        tiff(bytes, &mut r);
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        r.container = "GIF".into();
        gif(bytes, &mut r);
    } else if bytes.starts_with(b"BM") && bytes.len() >= 14 {
        r.container = "BMP".into();
        // BMP has no metadata blocks; only trailing bytes beyond the declared size matter.
        let declared = u32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]) as usize;
        trailing(bytes, Some(declared.min(bytes.len())), &mut r);
    } else {
        r.container = "unknown".into();
        r.add(
            Category::UnexpectedData,
            Evidence::Pattern,
            "Unrecognised container; metadata could not be inspected",
        );
    }
    exif_fields(bytes, &mut r);
    r
}

/// EXIF field-level inspection via kamadak-exif (BSD-2-Clause), which supports all four
/// containers. Reports presence and kind, never coordinate values.
fn exif_fields(bytes: &[u8], r: &mut MetadataReport) {
    let Ok(exif) = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(bytes)) else {
        return;
    };
    use exif::{In, Tag};
    let mut gps = false;
    let mut device = vec![];
    let mut time = false;
    let mut thumb = false;
    let mut serial = false;
    for f in exif.fields() {
        match f.tag {
            Tag::GPSLatitude | Tag::GPSLongitude | Tag::GPSAltitude | Tag::GPSDestLatitude => {
                gps = true
            }
            Tag::Make | Tag::Model | Tag::LensModel | Tag::LensMake | Tag::Software => {
                device.push(f.tag.to_string())
            }
            Tag::BodySerialNumber
            | Tag::LensSerialNumber
            | Tag::ImageUniqueID
            | Tag::CameraOwnerName => serial = true,
            Tag::DateTimeOriginal | Tag::DateTime | Tag::DateTimeDigitized => time = true,
            Tag::JPEGInterchangeFormat if f.ifd_num == In::THUMBNAIL => thumb = true,
            Tag::ImageDescription | Tag::UserComment | Tag::Artist | Tag::Copyright => {
                let v = f.display_value().to_string();
                if !v.trim().is_empty() {
                    r.text_values.push(v.chars().take(4096).collect());
                }
            }
            _ => {}
        }
    }
    if gps {
        r.add(
            Category::GpsLocation,
            Evidence::Confirmed,
            "EXIF GPS position tags present",
        );
    }
    if !device.is_empty() {
        device.dedup();
        r.add(
            Category::DeviceInformation,
            Evidence::Confirmed,
            format!("EXIF device tags present: {}", device.join(", ")),
        );
    }
    if serial {
        r.add(
            Category::DeviceInformation,
            Evidence::Confirmed,
            "EXIF serial number, unique ID or owner name present",
        );
    }
    if time {
        r.add(
            Category::Timestamp,
            Evidence::Confirmed,
            "EXIF capture timestamps present",
        );
    }
    if thumb {
        r.add(
            Category::EmbeddedThumbnail,
            Evidence::Confirmed,
            "EXIF embedded thumbnail present (may show pre-edit content)",
        );
    }
}

fn be16(b: &[u8], i: usize) -> Option<usize> {
    Some(u16::from_be_bytes(b.get(i..i + 2)?.try_into().ok()?) as usize)
}
fn be32(b: &[u8], i: usize) -> Option<usize> {
    Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?) as usize)
}
fn le32(b: &[u8], i: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?) as usize)
}

fn jpeg(b: &[u8], r: &mut MetadataReport) {
    let mut i = 2;
    let mut segments = 0;
    let mut eoi = None;
    while i + 2 <= b.len() && segments < MAX_SEGMENTS {
        segments += 1;
        if b[i] != 0xff {
            r.add(
                Category::UnexpectedData,
                Evidence::Pattern,
                "Malformed JPEG segment structure",
            );
            return;
        }
        let marker = b[i + 1];
        if marker == 0xff {
            i += 1;
            continue;
        }
        if marker == 0xd9 {
            eoi = Some(i + 2);
            break;
        }
        if (0xd0..=0xd7).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        let Some(len) = be16(b, i + 2) else { break };
        if len < 2 || i + 2 + len > b.len() {
            r.add(
                Category::UnexpectedData,
                Evidence::Pattern,
                "Truncated JPEG segment",
            );
            return;
        }
        let payload = &b[i + 4..i + 2 + len];
        match marker {
            0xe1 if payload.starts_with(b"Exif\0") => r.add(
                Category::ExifMetadata,
                Evidence::Confirmed,
                "APP1 EXIF block",
            ),
            0xe1 if payload.starts_with(b"http://ns.adobe.com/xap/1.0/") => {
                r.add(
                    Category::XmpMetadata,
                    Evidence::Confirmed,
                    "APP1 XMP packet",
                );
                r.text_values.push(
                    String::from_utf8_lossy(payload)
                        .chars()
                        .take(65536)
                        .collect(),
                );
            }
            0xe1 => r.add(
                Category::XmpMetadata,
                Evidence::Pattern,
                "APP1 extension block",
            ),
            0xe2 if payload.starts_with(b"ICC_PROFILE") => r.add(
                Category::IccProfile,
                Evidence::Confirmed,
                "APP2 ICC color profile",
            ),
            0xe2 if payload.starts_with(b"MPF") => r.add(
                Category::EmbeddedImage,
                Evidence::Confirmed,
                "APP2 Multi-Picture Format: additional embedded images",
            ),
            0xed => r.add(
                Category::IptcMetadata,
                Evidence::Confirmed,
                "APP13 Photoshop/IPTC block",
            ),
            0xfe => {
                r.add(
                    Category::Comment,
                    Evidence::Confirmed,
                    "JPEG comment segment",
                );
                r.text_values.push(
                    String::from_utf8_lossy(payload)
                        .chars()
                        .take(4096)
                        .collect(),
                );
            }
            0xe0 if payload.starts_with(b"JFIF") || payload.starts_with(b"JFXX") => {
                if payload.len() > 16 {
                    r.add(
                        Category::EmbeddedThumbnail,
                        Evidence::Pattern,
                        "JFIF thumbnail data",
                    );
                }
            }
            0xe3..=0xef | 0xe0 => r.add(
                Category::OtherMetadata,
                Evidence::Confirmed,
                format!("Application segment APP{}", marker - 0xe0),
            ),
            _ => {}
        }
        i += 2 + len;
        if marker == 0xda {
            // Skip entropy-coded data; stuffed 0xFF00 and restart markers are not boundaries.
            while i + 1 < b.len() {
                if b[i] == 0xff && b[i + 1] != 0 && !(0xd0..=0xd7).contains(&b[i + 1]) {
                    break;
                }
                i += 1;
            }
        }
    }
    trailing(b, eoi, r);
}

fn trailing(b: &[u8], end: Option<usize>, r: &mut MetadataReport) {
    let Some(end) = end else {
        r.add(
            Category::UnexpectedData,
            Evidence::Pattern,
            "No end-of-image marker found",
        );
        return;
    };
    let rest = &b[end.min(b.len())..];
    if rest.iter().any(|x| *x != 0) {
        let embedded = rest.windows(3).any(|w| w == [0xff, 0xd8, 0xff])
            || rest.windows(8).any(|w| w == b"\x89PNG\r\n\x1a\n");
        r.add(
            if embedded {
                Category::EmbeddedImage
            } else {
                Category::UnexpectedData
            },
            Evidence::Confirmed,
            format!(
                "{} bytes after end of image{}",
                rest.len(),
                if embedded {
                    " containing another image"
                } else {
                    ""
                }
            ),
        );
    }
}

fn png(b: &[u8], r: &mut MetadataReport) {
    let mut i = 8;
    let mut n = 0;
    let mut end = None;
    while i + 12 <= b.len() && n < MAX_SEGMENTS {
        n += 1;
        let Some(len) = be32(b, i) else { break };
        let Some(data_end) = (i + 8).checked_add(len) else {
            break;
        };
        if data_end + 4 > b.len() {
            r.add(
                Category::UnexpectedData,
                Evidence::Pattern,
                "Truncated PNG chunk",
            );
            return;
        }
        let kind = &b[i + 4..i + 8];
        let data = &b[i + 8..data_end];
        match kind {
            b"tEXt" | b"zTXt" | b"iTXt" => {
                let key: String = data
                    .iter()
                    .take_while(|c| **c != 0)
                    .take(79)
                    .map(|c| *c as char)
                    .collect();
                let cat = if key.contains("XML:com.adobe.xmp") {
                    Category::XmpMetadata
                } else {
                    Category::TextMetadata
                };
                r.add(
                    cat,
                    Evidence::Confirmed,
                    format!("PNG text chunk \"{key}\""),
                );
                if kind == b"tEXt" {
                    r.text_values
                        .push(String::from_utf8_lossy(data).chars().take(65536).collect());
                }
            }
            b"eXIf" => r.add(
                Category::ExifMetadata,
                Evidence::Confirmed,
                "PNG eXIf chunk",
            ),
            b"iCCP" => r.add(
                Category::IccProfile,
                Evidence::Confirmed,
                "PNG embedded ICC profile",
            ),
            b"tIME" => r.add(
                Category::Timestamp,
                Evidence::Confirmed,
                "PNG modification time",
            ),
            b"acTL" => r.add(
                Category::HiddenLayers,
                Evidence::Confirmed,
                "Animated PNG: additional frames",
            ),
            b"IEND" => {
                end = Some(data_end + 4);
                break;
            }
            b"IHDR" | b"PLTE" | b"IDAT" | b"tRNS" | b"gAMA" | b"cHRM" | b"sRGB" | b"pHYs"
            | b"bKGD" | b"sBIT" => {}
            _ => r.add(
                Category::OtherMetadata,
                Evidence::Confirmed,
                format!("PNG chunk {}", String::from_utf8_lossy(kind)),
            ),
        }
        i = data_end + 4;
    }
    trailing(b, end, r);
}

fn webp(b: &[u8], r: &mut MetadataReport) {
    let Some(riff) = le32(b, 4) else { return };
    let end = riff.saturating_add(8).min(b.len());
    let mut i = 12;
    let mut n = 0;
    while i + 8 <= end && n < MAX_SEGMENTS {
        n += 1;
        let kind = &b[i..i + 4];
        let Some(len) = le32(b, i + 4) else { break };
        match kind {
            b"EXIF" => r.add(
                Category::ExifMetadata,
                Evidence::Confirmed,
                "WebP EXIF chunk",
            ),
            b"XMP " => {
                r.add(Category::XmpMetadata, Evidence::Confirmed, "WebP XMP chunk");
                if let Some(d) = b.get(i + 8..(i + 8).saturating_add(len).min(end)) {
                    r.text_values
                        .push(String::from_utf8_lossy(d).chars().take(65536).collect());
                }
            }
            b"ICCP" => r.add(
                Category::IccProfile,
                Evidence::Confirmed,
                "WebP ICC profile",
            ),
            b"ANIM" | b"ANMF" => r.add(
                Category::HiddenLayers,
                Evidence::Confirmed,
                "Animated WebP frames",
            ),
            b"VP8 " | b"VP8L" | b"VP8X" | b"ALPH" => {}
            _ => r.add(
                Category::OtherMetadata,
                Evidence::Confirmed,
                format!("WebP chunk {}", String::from_utf8_lossy(kind)),
            ),
        }
        i = i
            .saturating_add(8)
            .saturating_add(len)
            .saturating_add(len & 1);
    }
    trailing(b, Some(end), r);
}

/// GIF: walks blocks to count frames and find comment, XMP and other application extensions.
fn gif(b: &[u8], r: &mut MetadataReport) {
    if b.len() < 13 {
        r.add(
            Category::UnexpectedData,
            Evidence::Pattern,
            "Truncated GIF header",
        );
        return;
    }
    let mut i = 13;
    if b[10] & 0x80 != 0 {
        i += 3 * (1usize << ((b[10] & 7) + 1));
    }
    // Skips a chain of data sub-blocks; returns the index after the terminator.
    let skip = |mut i: usize| -> Option<usize> {
        loop {
            let n = *b.get(i)? as usize;
            i += 1;
            if n == 0 {
                return Some(i);
            }
            i = i.checked_add(n)?;
        }
    };
    let (mut frames, mut guard) = (0usize, 0usize);
    let mut end = None;
    while i < b.len() && guard < 100_000 {
        guard += 1;
        match b[i] {
            0x3b => {
                end = Some(i + 1);
                break;
            }
            0x2c => {
                frames += 1;
                let Some(flags) = b.get(i + 9) else { break };
                i += 10;
                if flags & 0x80 != 0 {
                    i += 3 * (1usize << ((flags & 7) + 1));
                }
                i += 1; // LZW minimum code size
                let Some(n) = skip(i) else { break };
                i = n;
            }
            0x21 => {
                let label = b.get(i + 1).copied().unwrap_or(0);
                if label == 0xfe {
                    r.add(
                        Category::Comment,
                        Evidence::Confirmed,
                        "GIF comment extension",
                    );
                    if let Some(len) = b.get(i + 2) {
                        let s = i + 3;
                        r.text_values.push(
                            String::from_utf8_lossy(
                                &b[s.min(b.len())..(s + *len as usize).min(b.len())],
                            )
                            .into_owned(),
                        );
                    }
                } else if label == 0xff {
                    let id = b.get(i + 3..i + 14).unwrap_or_default();
                    if id.starts_with(b"XMP Data") {
                        r.add(
                            Category::XmpMetadata,
                            Evidence::Confirmed,
                            "GIF XMP application block",
                        );
                    } else if !id.starts_with(b"NETSCAPE") && !id.starts_with(b"ANIMEXTS") {
                        r.add(
                            Category::OtherMetadata,
                            Evidence::Confirmed,
                            "GIF application extension",
                        );
                    }
                }
                let Some(n) = skip(i + 2) else { break };
                i = n;
            }
            _ => {
                r.add(
                    Category::UnexpectedData,
                    Evidence::Pattern,
                    "Malformed GIF block structure",
                );
                return;
            }
        }
    }
    if frames > 1 {
        r.add(
            Category::HiddenLayers,
            Evidence::Confirmed,
            format!("Animated GIF: {frames} frames (only the first is processed as a photograph)"),
        );
    }
    trailing(b, end, r);
}

fn tiff(b: &[u8], r: &mut MetadataReport) {
    let le = b[0] == b'I';
    let u16_at = |i: usize| -> Option<usize> {
        let s: [u8; 2] = b.get(i..i + 2)?.try_into().ok()?;
        Some(if le {
            u16::from_le_bytes(s)
        } else {
            u16::from_be_bytes(s)
        } as usize)
    };
    let u32_at = |i: usize| -> Option<usize> {
        let s: [u8; 4] = b.get(i..i + 4)?.try_into().ok()?;
        Some(if le {
            u32::from_le_bytes(s)
        } else {
            u32::from_be_bytes(s)
        } as usize)
    };
    let mut ifd = u32_at(4).unwrap_or(0);
    let mut pages = 0;
    let mut seen = std::collections::HashSet::new();
    while ifd != 0 && pages < 64 && seen.insert(ifd) {
        let Some(count) = u16_at(ifd) else { break };
        pages += 1;
        for e in 0..count.min(1024) {
            let Some(tag) = u16_at(ifd + 2 + e * 12) else {
                break;
            };
            if pages > 1 {
                continue;
            }
            match tag {
                34853 => r.add(Category::GpsLocation, Evidence::Confirmed, "TIFF GPS IFD"),
                34665 => r.add(Category::ExifMetadata, Evidence::Confirmed, "TIFF EXIF IFD"),
                700 => r.add(Category::XmpMetadata, Evidence::Confirmed, "TIFF XMP tag"),
                33723 => r.add(Category::IptcMetadata, Evidence::Confirmed, "TIFF IPTC tag"),
                34377 => r.add(
                    Category::IptcMetadata,
                    Evidence::Confirmed,
                    "TIFF Photoshop resources",
                ),
                34675 => r.add(
                    Category::IccProfile,
                    Evidence::Confirmed,
                    "TIFF ICC profile",
                ),
                270 | 315 | 33432 => r.add(
                    Category::TextMetadata,
                    Evidence::Confirmed,
                    format!("TIFF text tag {tag}"),
                ),
                271 | 272 | 305 => r.add(
                    Category::DeviceInformation,
                    Evidence::Confirmed,
                    format!("TIFF device tag {tag}"),
                ),
                306 => r.add(Category::Timestamp, Evidence::Confirmed, "TIFF DateTime"),
                _ => {}
            }
        }
        ifd = u32_at(ifd + 2 + count * 12).unwrap_or(0);
    }
    if pages > 1 {
        r.add(
            Category::HiddenLayers,
            Evidence::Confirmed,
            format!("Multi-page TIFF: {pages} images (only the first is displayed)"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cats(r: &MetadataReport) -> Vec<Category> {
        r.findings.iter().map(|f| f.category).collect()
    }
    fn tiny_png() -> Vec<u8> {
        let mut c = std::io::Cursor::new(vec![]);
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut c, image::ImageFormat::Png)
            .unwrap();
        c.into_inner()
    }
    fn chunk(kind: &[u8], data: &[u8]) -> Vec<u8> {
        let mut v = (data.len() as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(data);
        v.extend_from_slice(&[0; 4]); // CRC is not validated by the inspector.
        v
    }
    #[test]
    fn clean_encoder_output_has_no_findings() {
        assert!(inspect(&tiny_png()).findings.is_empty());
        let mut c = std::io::Cursor::new(vec![]);
        image::DynamicImage::new_rgb8(8, 8)
            .write_to(&mut c, image::ImageFormat::Jpeg)
            .unwrap();
        let r = inspect(&c.into_inner());
        assert!(r.findings.is_empty(), "{:?}", r.findings);
        let mut c = std::io::Cursor::new(vec![]);
        image::DynamicImage::new_rgb8(8, 8)
            .write_to(&mut c, image::ImageFormat::WebP)
            .unwrap();
        assert!(inspect(&c.into_inner()).findings.is_empty());
        let mut c = std::io::Cursor::new(vec![]);
        image::DynamicImage::new_rgb8(8, 8)
            .write_to(&mut c, image::ImageFormat::Tiff)
            .unwrap();
        let r = inspect(&c.into_inner());
        assert!(r.findings.is_empty(), "{:?}", r.findings);
    }
    #[test]
    fn png_text_icc_and_trailing_image() {
        let png = tiny_png();
        let iend = png.len() - 12;
        let mut v = png[..iend].to_vec();
        v.extend(chunk(b"tEXt", b"Comment\0password=hunter22"));
        v.extend(chunk(b"iCCP", b"x\0\0"));
        v.extend_from_slice(&png[iend..]);
        v.extend_from_slice(&[0xff, 0xd8, 0xff, 0xe0, 1, 2, 3]);
        let r = inspect(&v);
        let c = cats(&r);
        assert!(c.contains(&Category::TextMetadata));
        assert!(c.contains(&Category::IccProfile));
        assert!(c.contains(&Category::EmbeddedImage));
        assert!(r.text_values.iter().any(|t| t.contains("hunter22")));
        // Values never appear in serialized findings.
        assert!(!serde_json::to_string(&r).unwrap().contains("hunter22"));
    }
    #[test]
    fn jpeg_segments() {
        let mut c = std::io::Cursor::new(vec![]);
        image::DynamicImage::new_rgb8(8, 8)
            .write_to(&mut c, image::ImageFormat::Jpeg)
            .unwrap();
        let base = c.into_inner();
        let mut v = base[..2].to_vec();
        for (m, p) in [
            (0xe1u8, &b"http://ns.adobe.com/xap/1.0/\0<x/>"[..]),
            (0xfe, b"secret note"),
            (0xed, b"Photoshop 3.0\0"),
        ] {
            v.extend_from_slice(&[0xff, m]);
            v.extend_from_slice(&((p.len() + 2) as u16).to_be_bytes());
            v.extend_from_slice(p);
        }
        v.extend_from_slice(&base[2..]);
        let c = cats(&inspect(&v));
        assert!(
            c.contains(&Category::XmpMetadata)
                && c.contains(&Category::Comment)
                && c.contains(&Category::IptcMetadata)
        );
    }
    #[test]
    fn hostile_inputs_do_not_panic() {
        for v in [
            b"\xff\xd8\xff\xe1\xff\xff".to_vec(),
            b"\x89PNG\r\n\x1a\n\xff\xff\xff\xffIHDR".to_vec(),
            b"RIFF\xff\xff\xff\xffWEBPEXIF\xff\xff\xff\xff".to_vec(),
            b"II*\0\x08\0\0\0\xff\xff".to_vec(),
            b"MM\0*\0\0\0\x08\0\x01\0\0\0\0\0\0\0\0\0\0\0\x08".to_vec(),
            vec![0; 3],
        ] {
            let _ = inspect(&v);
        }
    }
}

#[cfg(test)]
mod format_tests {
    use super::*;
    fn encode(f: image::ImageFormat) -> Vec<u8> {
        let mut c = std::io::Cursor::new(vec![]);
        image::DynamicImage::new_rgb8(6, 4)
            .write_to(&mut c, f)
            .unwrap();
        c.into_inner()
    }
    #[test]
    fn gif_and_bmp_clean_and_animated() {
        assert!(
            inspect(&encode(image::ImageFormat::Bmp))
                .findings
                .is_empty()
        );
        let gif = encode(image::ImageFormat::Gif);
        assert!(
            inspect(&gif).findings.is_empty(),
            "{:?}",
            inspect(&gif).findings
        );
        // Duplicate the single frame (between header/palette and trailer) to make an animation.
        let start = gif.windows(1).position(|w| w[0] == 0x2c).unwrap();
        let mut anim = gif[..gif.len() - 1].to_vec();
        anim.extend_from_slice(&gif[start..gif.len() - 1]);
        anim.extend_from_slice(&[0x21, 0xfe, 4, b'n', b'o', b't', b'e', 0, 0x3b]);
        let c: Vec<_> = inspect(&anim).findings.iter().map(|f| f.category).collect();
        assert!(
            c.contains(&Category::HiddenLayers) && c.contains(&Category::Comment),
            "{c:?}"
        );
    }
    /// Deterministic mutation testing: thousands of corrupted variants must never panic.
    #[test]
    fn mutated_inputs_never_panic() {
        let seeds = [
            encode(image::ImageFormat::Png),
            encode(image::ImageFormat::Jpeg),
            encode(image::ImageFormat::Gif),
            encode(image::ImageFormat::WebP),
            encode(image::ImageFormat::Tiff),
        ];
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for seed in &seeds {
            for _ in 0..400 {
                let mut v = seed.clone();
                for _ in 0..(next() % 8 + 1) {
                    let i = (next() as usize) % v.len();
                    v[i] = next() as u8;
                }
                if next() % 4 == 0 {
                    v.truncate((next() as usize) % v.len());
                }
                let _ = inspect(&v);
                let _ = photo_core_decode(&v);
            }
        }
    }
    fn photo_core_decode(v: &[u8]) -> bool {
        image::load_from_memory(v).is_ok()
    }
}
