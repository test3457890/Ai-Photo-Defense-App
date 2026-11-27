//! Content-based format identification. Extensions are compared only to flag mislabelling.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MediaKind {
    Image,
    AnimatedImage,
    RawPhoto,
    Video,
    Audio,
    Subtitle,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Detected {
    pub kind: MediaKind,
    pub container: &'static str,
}

fn at(b: &[u8], i: usize, s: &[u8]) -> bool {
    b.get(i..i + s.len()) == Some(s)
}

pub fn detect(b: &[u8]) -> Detected {
    let d = |kind, container| Detected { kind, container };
    use MediaKind::*;
    if at(b, 4, b"ftyp") {
        let brand = b.get(8..12).unwrap_or_default();
        let brands = b.get(8..64.min(b.len())).unwrap_or_default();
        let has = |x: &[u8]| brands.windows(4).any(|w| w == x);
        return match brand {
            b"qt  " => d(Video, "MOV"),
            b"M4A " | b"M4B " | b"M4P " => d(Audio, "M4A"),
            b"3gp4" | b"3gp5" | b"3gp6" | b"3g2a" => d(Video, "3GP"),
            b"crx " => d(RawPhoto, "CR3"),
            b"avif" | b"avis" => d(
                if brand == b"avis" {
                    AnimatedImage
                } else {
                    Image
                },
                "AVIF",
            ),
            b"heic" | b"heix" | b"mif1" | b"msf1" | b"hevc" => d(Image, "HEIF"),
            _ if has(b"M4A ") => d(Audio, "M4A"),
            _ => d(Video, "MP4"),
        };
    }
    if b.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        let webm = b
            .get(..64.min(b.len()))
            .unwrap_or_default()
            .windows(4)
            .any(|w| w == b"webm");
        return d(Video, if webm { "WebM" } else { "Matroska" });
    }
    if at(b, 0, b"RIFF") {
        return match b.get(8..12).unwrap_or_default() {
            b"WAVE" => d(Audio, "WAV"),
            b"AVI " => d(Video, "AVI"),
            b"WEBP" => d(Image, "WebP"),
            _ => d(Unknown, "RIFF"),
        };
    }
    if at(b, 0, b"riff") && at(b, 24, b"wave") {
        return d(Audio, "W64");
    }
    if at(b, 0, b"FORM") && (at(b, 8, b"AIFF") || at(b, 8, b"AIFC")) {
        return d(Audio, "AIFF");
    }
    if at(b, 0, b"caff") {
        return d(Audio, "CAF");
    }
    if at(b, 0, b"fLaC") {
        return d(Audio, "FLAC");
    }
    if at(b, 0, b"OggS") {
        let head = b.get(..200.min(b.len())).unwrap_or_default();
        let has = |x: &[u8]| head.windows(x.len()).any(|w| w == x);
        return if has(b"theora") {
            d(Video, "Ogg (Theora)")
        } else if has(b"OpusHead") {
            d(Audio, "Ogg (Opus)")
        } else {
            d(Audio, "Ogg (Vorbis)")
        };
    }
    if at(b, 0, b"ID3") || (b.len() > 2 && b[0] == 0xff && (b[1] & 0xe6) == 0xe2) {
        return d(Audio, "MP3");
    }
    if b.len() > 2 && b[0] == 0xff && (b[1] & 0xf6) == 0xf0 {
        return d(Audio, "AAC (ADTS)");
    }
    if at(b, 0, &[0x0b, 0x77]) {
        return d(Audio, "AC-3");
    }
    if at(b, 0, b"#!AMR") {
        return d(Audio, "AMR");
    }
    if at(b, 0, &[0x30, 0x26, 0xb2, 0x75, 0x8e, 0x66, 0xcf, 0x11]) {
        return d(Video, "ASF/WMV/WMA");
    }
    if at(b, 0, b"FLV\x01") {
        return d(Video, "FLV");
    }
    if at(b, 0, &[0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01]) {
        return d(Video, "MXF");
    }
    if at(b, 0, &[0x00, 0x00, 0x01, 0xba]) || at(b, 0, &[0x00, 0x00, 0x01, 0xb3]) {
        return d(Video, "MPEG-PS");
    }
    if b.len() >= 377 && b[0] == 0x47 && b[188] == 0x47 && b[376] == 0x47 {
        return d(Video, "MPEG-TS");
    }
    if b.len() >= 196 && b[4] == 0x47 && b[196] == 0x47 {
        return d(Video, "M2TS");
    }
    if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        return d(AnimatedImage, "GIF");
    }
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        let apng = b
            .get(..4096.min(b.len()))
            .unwrap_or_default()
            .windows(4)
            .any(|w| w == b"acTL");
        return d(
            if apng { AnimatedImage } else { Image },
            if apng { "APNG" } else { "PNG" },
        );
    }
    if b.starts_with(&[0xff, 0xd8, 0xff]) {
        return d(Image, "JPEG");
    }
    if b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") {
        return d(Image, "TIFF");
    }
    if b.starts_with(b"BM") {
        return d(Image, "BMP");
    }
    if b.starts_with(&[0xff, 0x0a]) || at(b, 4, b"JXL ") {
        return d(Image, "JPEG XL");
    }
    if at(b, 4, b"jP  ") || b.starts_with(&[0xff, 0x4f, 0xff, 0x51]) {
        return d(Image, "JPEG 2000");
    }
    if b.starts_with(&[0x76, 0x2f, 0x31, 0x01]) {
        return d(Image, "OpenEXR");
    }
    if b.starts_with(b"8BPS") {
        return d(Image, "PSD");
    }
    if b.starts_with(b"gimp xcf") {
        return d(Image, "XCF");
    }
    let text = String::from_utf8_lossy(b.get(..512.min(b.len())).unwrap_or_default())
        .trim_start_matches('\u{feff}')
        .trim_start()
        .to_string();
    if text.starts_with("WEBVTT") {
        return d(Subtitle, "WebVTT");
    }
    if text.starts_with("<svg") || (text.starts_with("<?xml") && text.contains("<svg")) {
        return d(Image, "SVG");
    }
    if crate::subtitles::looks_like_srt(&text) {
        return d(Subtitle, "SRT");
    }
    d(Unknown, "unknown")
}

/// True when the extension contradicts the detected format (a possible mislabelled file).
pub fn extension_mismatch(det: &Detected, ext: &str) -> bool {
    let e = ext.to_ascii_lowercase();
    let expected: &[&str] = match det.container {
        "MP4" => &["mp4", "m4v", "mp4v"],
        "MOV" => &["mov", "qt"],
        "M4A" => &["m4a", "m4b", "mp4", "aac"],
        "3GP" => &["3gp", "3g2"],
        "Matroska" => &["mkv", "mka", "mk3d"],
        "WebM" => &["webm", "mkv"],
        "WAV" => &["wav", "bwf", "wave"],
        "AVI" => &["avi"],
        "FLAC" => &["flac"],
        "MP3" => &["mp3"],
        "AIFF" => &["aif", "aiff", "aifc"],
        "JPEG" => &["jpg", "jpeg", "jpe"],
        "PNG" | "APNG" => &["png", "apng"],
        "GIF" => &["gif"],
        "SRT" => &["srt"],
        "WebVTT" => &["vtt"],
        _ => return false,
    };
    !expected.contains(&e.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifies_by_content_not_extension() {
        let cases: &[(&[u8], &str)] = &[
            (b"\0\0\0\x18ftypisom\0\0\0\0isomavc1", "MP4"),
            (b"\0\0\0\x14ftypqt  \0\0\0\0qt  ", "MOV"),
            (b"\0\0\0\x18ftypM4A \0\0\0\0M4A mp42", "M4A"),
            (b"\x1a\x45\xdf\xa3\x9f\x42\x86\x81\x01webm", "WebM"),
            (b"RIFF\0\0\0\0WAVEfmt ", "WAV"),
            (b"RIFF\0\0\0\0AVI LIST", "AVI"),
            (b"fLaC\0\0\0\x22", "FLAC"),
            (b"ID3\x04\0\0\0\0\0\0", "MP3"),
            (b"FORM\0\0\0\0AIFFCOMM", "AIFF"),
            (b"WEBVTT\n\n00:00.000 --> 00:01.000\nHi", "WebVTT"),
            (b"1\n00:00:01,000 --> 00:00:02,000\nHello\n", "SRT"),
            (b"GIF89a\x01\0\x01\0\0\0\0", "GIF"),
        ];
        for (b, c) in cases {
            assert_eq!(detect(b).container, *c, "{:?}", String::from_utf8_lossy(b));
        }
        let wav = detect(b"RIFF\0\0\0\0WAVEfmt ");
        assert!(extension_mismatch(&wav, "mp3"));
        assert!(!extension_mismatch(&wav, "WAV"));
        assert_eq!(detect(b"random bytes").kind, MediaKind::Unknown);
        assert_eq!(detect(&[]).kind, MediaKind::Unknown);
    }
}
