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

pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 24_000_000;
pub const MAX_SIDE: u32 = 16_384;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fingerprint {
    pub sha256: String,
    pub blake3: String,
}
pub fn fingerprint(bytes: &[u8]) -> Fingerprint {
    Fingerprint {
        sha256: format!("{:x}", Sha256::digest(bytes)),
        blake3: blake3::hash(bytes).to_hex().to_string(),
    }
}

pub struct Photo {
    pub pixels: DynamicImage,
    pub fingerprint: Fingerprint,
    pub format: ImageFormat,
    /// The exact bytes that were decoded, for metadata inspection without a second read.
    pub bytes: Vec<u8>,
}
/// Uses one bounded byte snapshot, avoiding a dimensions/decode path replacement race.
pub fn load(path: &Path) -> Result<Photo> {
    let file = File::open(path).context("Open image")?;
    ensure!(file.metadata()?.is_file(), "Input must be a regular file");
    ensure!(
        file.metadata()?.len() <= MAX_FILE_BYTES,
        "Input exceeds 64 MiB"
    );
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    let (pixels, format) = decode_bounded(&bytes)?;
    Ok(Photo {
        pixels,
        fingerprint: fingerprint(&bytes),
        format,
        bytes,
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
            ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP | ImageFormat::Tiff
        ),
        "Only JPEG, PNG, WebP and TIFF supported"
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
