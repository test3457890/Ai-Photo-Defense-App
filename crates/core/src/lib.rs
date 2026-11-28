//! Bounded image I/O, metadata-free copies, hashes, and decoded-pixel quality metrics.
use anyhow::{Context, Result, bail, ensure};
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, RgbImage};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Cursor, Read, Write},
    path::Path,
};

pub mod raw;

pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 24_000_000;
pub const MAX_SIDE: u32 = 16_384;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fingerprint {
    pub sha256: String,
    pub blake3: String,
}
/// Formats raw digest or binary bytes as lowercase hexadecimal.
pub fn encode_lower_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub fn fingerprint(bytes: &[u8]) -> Fingerprint {
    Fingerprint {
        sha256: encode_lower_hex(&Sha256::digest(bytes)),
        blake3: blake3::hash(bytes).to_hex().to_string(),
    }
}

pub struct Photo {
    pub pixels: DynamicImage,
    pub fingerprint: Fingerprint,
    pub format: ImageFormat,
    /// The exact bytes that were read, for metadata inspection without a second read.
    pub bytes: Vec<u8>,
    /// Set when the pixels come from a RAW file's embedded camera preview.
    pub raw: Option<raw::RawKind>,
}
impl Photo {
    /// Same source (hash, bytes, RAW status) with different pixels, e.g. after edits.
    pub fn with_pixels(&self, pixels: DynamicImage) -> Photo {
        Photo {
            pixels,
            fingerprint: self.fingerprint.clone(),
            format: self.format,
            bytes: self.bytes.clone(),
            raw: self.raw,
        }
    }
}
/// Uses one bounded byte snapshot, avoiding a dimensions/decode path replacement race.
pub fn load(path: &Path) -> Result<Photo> {
    let file = File::open(path).context("Open image")?;
    ensure!(file.metadata()?.is_file(), "Input must be a regular file");
    ensure!(
        file.metadata()?.len() <= raw::MAX_RAW_BYTES,
        "Input exceeds 256 MiB"
    );
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES.max(raw::MAX_RAW_BYTES) + 1)
        .read_to_end(&mut bytes)?;
    let ext = path.extension().and_then(|e| e.to_str());
    let (pixels, format, raw) = match raw::identify(&bytes, ext) {
        Some(_) => {
            ensure!(
                bytes.len() as u64 <= raw::MAX_RAW_BYTES,
                "RAW file exceeds 256 MiB"
            );
            let preview = raw::extract_preview(&bytes, ext)?;
            let (pixels, format) = decode_bounded(&preview.jpeg)?;
            (pixels, format, Some(preview.kind))
        }
        None => {
            let (pixels, format) = decode_bounded(&bytes)?;
            (pixels, format, None)
        }
    };
    Ok(Photo {
        pixels,
        fingerprint: fingerprint(&bytes),
        format,
        bytes,
        raw,
    })
}

/// Decodes untrusted bytes with format allow-listing, dimension, pixel-count, allocation and
/// bit-depth limits. Used for imports and for re-opening exported files during verification.
pub fn decode_bounded(bytes: &[u8]) -> Result<(DynamicImage, ImageFormat)> {
    ensure!(bytes.len() as u64 <= MAX_FILE_BYTES, "Input exceeds 64 MiB");
    let format = image::guess_format(bytes).context("Unknown image format")?;
    ensure!(
        matches!(
            format,
            ImageFormat::Jpeg
                | ImageFormat::Png
                | ImageFormat::WebP
                | ImageFormat::Tiff
                | ImageFormat::Bmp
                | ImageFormat::Gif
        ),
        "Only JPEG, PNG, WebP, TIFF, BMP and GIF are supported"
    );
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().context("Read image header")?;
    let (w, h) = decoder.dimensions();
    ensure!(
        w > 0 && h > 0 && u64::from(w) * u64::from(h) <= MAX_PIXELS,
        "Image exceeds 24 megapixels or is empty"
    );
    ensure!(
        matches!(
            decoder.color_type(),
            image::ColorType::L8
                | image::ColorType::La8
                | image::ColorType::Rgb8
                | image::ColorType::Rgba8
        ),
        "High bit depth inputs are not supported yet; convert to 8-bit first"
    );
    let orientation = decoder.orientation()?;
    let mut pixels = DynamicImage::from_decoder(decoder).context("Decode image")?;
    pixels.apply_orientation(orientation);
    Ok((pixels, format))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ExportFormat {
    #[default]
    Png,
    Jpeg,
    WebP,
    Tiff,
}
impl ExportFormat {
    pub const ALL: [Self; 4] = [Self::Png, Self::Jpeg, Self::WebP, Self::Tiff];
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
            Self::Tiff => "tiff",
        }
    }
    pub fn image_format(self) -> ImageFormat {
        match self {
            Self::Png => ImageFormat::Png,
            Self::Jpeg => ImageFormat::Jpeg,
            Self::WebP => ImageFormat::WebP,
            Self::Tiff => ImageFormat::Tiff,
        }
    }
}
impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.extension())
    }
}

/// Encodes decoded pixels only. Source EXIF/GPS, XMP and ICC are not propagated.
/// JPEG refuses alpha rather than silently compositing against an unexpected background.
pub fn encode(pixels: &DynamicImage, format: ExportFormat, quality: u8) -> Result<Vec<u8>> {
    ensure!((1..=100).contains(&quality), "JPEG quality must be 1..100");
    ensure!(
        pixels.width() > 0
            && pixels.height() > 0
            && u64::from(pixels.width()) * u64::from(pixels.height()) <= MAX_PIXELS,
        "Invalid export dimensions"
    );
    let mut writer = Cursor::new(Vec::new());
    if format == ExportFormat::Jpeg {
        ensure!(
            !pixels.to_rgba8().pixels().any(|p| p[3] != 255),
            "JPEG cannot preserve transparency; choose PNG, WebP or TIFF"
        );
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, quality)
            .encode_image(&DynamicImage::ImageRgb8(pixels.to_rgb8()))?;
    } else {
        pixels.write_to(&mut writer, format.image_format())?;
    }
    Ok(writer.into_inner())
}

/// Same-directory private temporary file and atomic no-clobber publication.
/// Existing destinations, including symlinks and source files, are never replaced.
pub fn save_new(path: &Path, bytes: &[u8]) -> Result<()> {
    ensure!(path.file_name().is_some(), "Missing output filename");
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    ensure!(parent.is_dir(), "Output directory must already exist");
    let mut temp =
        tempfile::NamedTempFile::new_in(parent).context("Create private temporary file")?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(path)
        .map_err(|e| anyhow::anyhow!("Export refused (destination must be new): {}", e.error))?;
    Ok(())
}

/// Publishes `bytes` as `<stem><suffix>.<ext>` in `dir`, adding `-2`, `-3`... when a name is
/// taken. Never replaces an existing file; the name is chosen atomically by no-clobber rename.
pub fn save_unique(dir: &Path, stem: &str, ext: &str, bytes: &[u8]) -> Result<std::path::PathBuf> {
    ensure!(dir.is_dir(), "Output directory must already exist");
    ensure!(
        !stem.is_empty()
            && stem.len() <= 120
            && stem
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            && ext.chars().all(|c| c.is_ascii_alphanumeric() || c == '.'),
        "Unsafe output filename"
    );
    let mut temp = tempfile::NamedTempFile::new_in(dir).context("Create private temporary file")?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    for n in 1..=1000u32 {
        let name = if n == 1 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem}-{n}.{ext}")
        };
        let path = dir.join(name);
        match temp.persist_noclobber(&path) {
            Ok(_) => return Ok(path),
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => temp = e.file,
            Err(e) => bail!("Export failed: {}", e.error),
        }
    }
    bail!("No free output filename after 1000 attempts")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quality {
    pub mse_rgb: f64,
    /// None means mathematically infinite when MSE is zero.
    pub psnr_db: Option<f64>,
    /// Non-overlapping 8x8 luminance windows, population covariance, constants 0.01/0.03.
    pub ssim_luma_8x8: f64,
    pub max_channel_delta: u8,
}
pub fn quality(a: &DynamicImage, b: &DynamicImage) -> Result<Quality> {
    ensure!(
        a.width() == b.width() && a.height() == b.height(),
        "Quality metrics require equal dimensions"
    );
    let a = a.to_rgb8();
    let b = b.to_rgb8();
    ensure!(!a.is_empty(), "Empty image");
    let mut sum = 0f64;
    let mut max_delta = 0;
    for (x, y) in a.as_raw().iter().zip(b.as_raw()) {
        let d = x.abs_diff(*y);
        sum += f64::from(d).powi(2);
        max_delta = max_delta.max(d);
    }
    let mse = sum / a.as_raw().len() as f64;
    let luma = |p: &image::Rgb<u8>| {
        0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2])
    };
    let mut ssim = 0.;
    let mut count = 0;
    for top in (0..a.height()).step_by(8) {
        for left in (0..a.width()).step_by(8) {
            let mut xs = Vec::with_capacity(64);
            let mut ys = Vec::with_capacity(64);
            for y in top..(top + 8).min(a.height()) {
                for x in left..(left + 8).min(a.width()) {
                    xs.push(luma(a.get_pixel(x, y)));
                    ys.push(luma(b.get_pixel(x, y)));
                }
            }
            let n = xs.len() as f64;
            let mx = xs.iter().sum::<f64>() / n;
            let my = ys.iter().sum::<f64>() / n;
            let vx = xs.iter().map(|x| (x - mx).powi(2)).sum::<f64>() / n;
            let vy = ys.iter().map(|y| (y - my).powi(2)).sum::<f64>() / n;
            let cov = xs
                .iter()
                .zip(&ys)
                .map(|(x, y)| (x - mx) * (y - my))
                .sum::<f64>()
                / n;
            ssim += ((2. * mx * my + 6.5025) * (2. * cov + 58.5225))
                / ((mx * mx + my * my + 6.5025) * (vx + vy + 58.5225));
            count += 1;
        }
    }
    Ok(Quality {
        mse_rgb: mse,
        psnr_db: if mse == 0. {
            None
        } else {
            Some(10. * (65025. / mse).log10())
        },
        ssim_luma_8x8: ssim / f64::from(count),
        max_channel_delta: max_delta,
    })
}

/// Extended image quality metrics for Validation Center (PSNR, SSIM, CIELAB Delta-E,
/// multi-scale gradient perceptual proxy, artifact score, resolution, and file sizes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedQuality {
    pub base: Quality,
    /// Mean CIELAB Delta-E76 color difference between original and candidate.
    pub delta_e_mean: f64,
    /// Maximum CIELAB Delta-E76 color difference.
    pub delta_e_max: f64,
    /// Multi-scale structural/gradient perceptual distance proxy in [0, 1] (0 = identical).
    /// Note: this is a deterministic structural proxy, not neural LPIPS weights.
    pub perceptual_distance_proxy: f64,
    /// High-frequency residual / blocking artifact indicator (0 = no added artifacts).
    pub artifact_score: f64,
    pub resolution: [u32; 2],
    pub original_bytes: Option<u64>,
    pub candidate_bytes: Option<u64>,
}

fn srgb_to_lab(rgb: [u8; 3]) -> [f64; 3] {
    let lin = |c: u8| {
        let v = f64::from(c) / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let r = lin(rgb[0]);
    let g = lin(rgb[1]);
    let b = lin(rgb[2]);
    // sRGB D65 -> XYZ
    let x = (0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047;
    let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
    let z = (0.0193339 * r + 0.1191920 * g + 0.9503041 * b) / 1.08883;
    let f = |t: f64| {
        if t > 0.008856 {
            t.cbrt()
        } else {
            7.787 * t + 16.0 / 116.0
        }
    };
    let fx = f(x);
    let fy = f(y);
    let fz = f(z);
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

pub fn detailed_quality(
    a: &DynamicImage,
    b: &DynamicImage,
    original_bytes: Option<u64>,
    candidate_bytes: Option<u64>,
) -> Result<ExtendedQuality> {
    let base = quality(a, b)?;
    let a_rgb = a.to_rgb8();
    let b_rgb = b.to_rgb8();
    let w = a_rgb.width() as usize;
    let h = a_rgb.height() as usize;
    let n = (w * h).max(1) as f64;

    let mut de_sum = 0.0f64;
    let mut de_max = 0.0f64;
    for (pa, pb) in a_rgb.pixels().zip(b_rgb.pixels()) {
        let la = srgb_to_lab(pa.0);
        let lb = srgb_to_lab(pb.0);
        let de =
            ((la[0] - lb[0]).powi(2) + (la[1] - lb[1]).powi(2) + (la[2] - lb[2]).powi(2)).sqrt();
        de_sum += de;
        de_max = de_max.max(de);
    }

    // Gradient difference proxy for perceptual distance & high-freq artifact score
    let luma = |img: &RgbImage, x: usize, y: usize| -> f64 {
        let p = img.get_pixel(x as u32, y as u32).0;
        0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2])
    };
    let mut grad_diff = 0.0f64;
    let mut lap_diff = 0.0f64;
    if w >= 3 && h >= 3 {
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let gax = luma(&a_rgb, x + 1, y) - luma(&a_rgb, x - 1, y);
                let gay = luma(&a_rgb, x, y + 1) - luma(&a_rgb, x, y - 1);
                let gbx = luma(&b_rgb, x + 1, y) - luma(&b_rgb, x - 1, y);
                let gby = luma(&b_rgb, x, y + 1) - luma(&b_rgb, x, y - 1);
                grad_diff += ((gax - gbx).powi(2) + (gay - gby).powi(2)).sqrt() / 255.0;

                let la = 4.0 * luma(&a_rgb, x, y)
                    - luma(&a_rgb, x - 1, y)
                    - luma(&a_rgb, x + 1, y)
                    - luma(&a_rgb, x, y - 1)
                    - luma(&a_rgb, x, y + 1);
                let lb = 4.0 * luma(&b_rgb, x, y)
                    - luma(&b_rgb, x - 1, y)
                    - luma(&b_rgb, x + 1, y)
                    - luma(&b_rgb, x, y - 1)
                    - luma(&b_rgb, x, y + 1);
                lap_diff += (la - lb).abs() / 255.0;
            }
        }
    }
    let interior = ((w.saturating_sub(2)) * (h.saturating_sub(2))).max(1) as f64;
    let ssim_dist = (1.0 - base.ssim_luma_8x8).clamp(0.0, 1.0);
    let grad_dist = (grad_diff / interior).clamp(0.0, 1.0);
    let perceptual_distance_proxy = (0.6 * ssim_dist + 0.4 * grad_dist).clamp(0.0, 1.0);
    let artifact_score = (lap_diff / interior).clamp(0.0, 1.0);

    Ok(ExtendedQuality {
        base,
        delta_e_mean: de_sum / n,
        delta_e_max: de_max,
        perceptual_distance_proxy,
        artifact_score,
        resolution: [a.width(), a.height()],
        original_bytes,
        candidate_bytes,
    })
}

pub fn difference(a: &DynamicImage, b: &DynamicImage, gain: u8) -> Result<DynamicImage> {
    ensure!(
        a.width() == b.width() && a.height() == b.height(),
        "Difference requires equal dimensions"
    );
    let a = a.to_rgb8();
    let b = b.to_rgb8();
    let mut out = RgbImage::new(a.width(), a.height());
    for ((o, x), y) in out.pixels_mut().zip(a.pixels()).zip(b.pixels()) {
        for c in 0..3 {
            o[c] = x[c].abs_diff(y[c]).saturating_mul(gain);
        }
    }
    Ok(DynamicImage::ImageRgb8(out))
}

/// Re-opens an encoded export under the same limits as untrusted input.
pub fn decode_export(bytes: &[u8]) -> Result<DynamicImage> {
    Ok(decode_bounded(bytes)?.0)
}

pub fn safe_stem(path: &Path) -> Result<String> {
    let stem = path
        .file_stem()
        .context("No image filename")?
        .to_string_lossy();
    let clean: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    if clean.is_empty() {
        bail!("Empty filename")
    };
    Ok(clean)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn orientation_is_applied_and_exif_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rotated.jpg");
        let base = encode(&DynamicImage::new_rgb8(2, 3), ExportFormat::Jpeg, 95).unwrap();
        let mut exif =
            b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0".to_vec();
        let mut bytes = vec![0xff, 0xd8, 0xff, 0xe1];
        bytes.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        bytes.append(&mut exif);
        bytes.extend_from_slice(&base[2..]);
        std::fs::write(&path, &bytes).unwrap();
        let photo = load(&path).unwrap();
        assert_eq!((photo.pixels.width(), photo.pixels.height()), (3, 2));
        let out = encode(&photo.pixels, ExportFormat::Jpeg, 95).unwrap();
        assert!(!out.windows(4).any(|w| w == b"Exif"));
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    #[test]
    fn rejects_high_bit_depth_and_oversized_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.png");
        let high = DynamicImage::ImageLuma16(image::ImageBuffer::from_pixel(
            2,
            2,
            image::Luma([65535u16]),
        ));
        let mut bytes = Cursor::new(Vec::new());
        high.write_to(&mut bytes, ImageFormat::Png).unwrap();
        std::fs::write(&path, bytes.into_inner()).unwrap();
        assert!(
            load(&path)
                .err()
                .unwrap()
                .to_string()
                .contains("High bit depth")
        );
        let mut bytes = encode(&DynamicImage::new_rgb8(2, 2), ExportFormat::Png, 95).unwrap();
        bytes[16..20].copy_from_slice(&20_000u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&20_000u32.to_be_bytes());
        let mut crc = 0xffff_ffffu32;
        for b in &bytes[12..29] {
            crc ^= u32::from(*b);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xedb8_8320
                } else {
                    crc >> 1
                };
            }
        }
        bytes[29..33].copy_from_slice(&(!crc).to_be_bytes());
        std::fs::write(&path, bytes).unwrap();
        assert!(load(&path).is_err());
    }
    #[test]
    fn known_metrics() {
        let a = DynamicImage::ImageRgb8(RgbImage::from_pixel(9, 9, image::Rgb([100; 3])));
        let b = DynamicImage::ImageRgb8(RgbImage::from_pixel(9, 9, image::Rgb([101; 3])));
        let same = quality(&a, &a).unwrap();
        assert_eq!(same.mse_rgb, 0.);
        assert!(same.psnr_db.is_none());
        assert!((same.ssim_luma_8x8 - 1.).abs() < 1e-12);
        let q = quality(&a, &b).unwrap();
        assert_eq!(q.mse_rgb, 1.);
        assert!((q.psnr_db.unwrap() - 48.1308036087).abs() < 1e-8);
    }
    #[test]
    fn no_clobber_and_roundtrip() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("out.png");
        let img = DynamicImage::new_rgb8(16, 16);
        let bytes = encode(&img, ExportFormat::Png, 90).unwrap();
        save_new(&p, &bytes).unwrap();
        assert!(save_new(&p, b"bad").is_err());
        assert_eq!(load(&p).unwrap().pixels.width(), 16);
        assert_eq!(std::fs::read(p).unwrap(), bytes);
        let a = save_unique(d.path(), "out", "png", b"one").unwrap();
        let b = save_unique(d.path(), "out", "png", b"two").unwrap();
        assert!(a.ends_with("out-2.png") && b.ends_with("out-3.png"));
        assert_eq!(std::fs::read(d.path().join("out.png")).unwrap(), bytes);
        assert!(save_unique(d.path(), "../x", "png", b"x").is_err());
    }
    #[test]
    fn rejects_malformed() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("bad.jpg");
        std::fs::write(&p, b"not an image").unwrap();
        assert!(load(&p).is_err());
    }
    #[test]
    fn all_formats_roundtrip_and_alpha() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(16, 16, image::Rgb([34, 67, 120])));
        for f in ExportFormat::ALL {
            let b = encode(&img, f, 95).unwrap();
            let out = decode_export(&b).unwrap();
            assert_eq!(out.width(), 16);
        }
        assert!(encode(&DynamicImage::new_rgba8(2, 2), ExportFormat::Jpeg, 90).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn no_symlink_overwrite() {
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("original");
        let dst = d.path().join("link");
        std::fs::write(&src, b"original").unwrap();
        symlink(&src, &dst).unwrap();
        assert!(save_new(&dst, b"bad").is_err());
        assert_eq!(std::fs::read(src).unwrap(), b"original");
    }
}
