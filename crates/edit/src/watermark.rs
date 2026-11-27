//! Invisible, key-based ownership watermark for photographs.
//!
//! A 48-bit payload plus a 16-bit checksum is spread over the whole image: every 8 x 8 block
//! carries one payload bit in four mid-frequency DCT coefficients of luminance, modulated by a
//! key-derived pseudo-random +-1 sequence. Detection correlates the same coefficients with the
//! same sequence and accepts the result only if the checksum matches, so a wrong key or an
//! unmarked image reports "not found" rather than a random payload.
//!
//! What it is: a provenance marker that survives the re-compression it is tested against
//! (JPEG quality 70 and above, lossless WebP/PNG) at about 50 dB PSNR.
//! What it is not: protection against AI editing or cloning, and not robust to resizing,
//! cropping, rotation or deliberate removal. Anyone with the key can detect or erase it.
use anyhow::{Result, ensure};
use image::RgbImage;

const COEFFS: [(usize, usize); 4] = [(1, 2), (2, 1), (2, 2), (1, 3)];
const BITS: usize = 64;
/// Minimum blocks per bit for a usable result (about 128 x 128 px for 64 bits).
const MIN_BLOCKS: usize = BITS * 4;

/// Result of a detection attempt.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Detection {
    /// Payload if the checksum matched.
    pub payload: Option<u64>,
    /// Mean absolute normalised correlation over all bits (0 = none, higher = stronger).
    pub confidence: f64,
}

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// Derives a 64-bit key from a passphrase (FNV-1a then mixing; not a password hash: the key
/// only selects the spreading sequence).
pub fn key_from_passphrase(p: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in p.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    splitmix(h)
}

fn crc16(v: u64) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for b in v.to_le_bytes().iter().take(6) {
        crc ^= (*b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn word(payload: u64) -> u64 {
    let p = payload & 0xFFFF_FFFF_FFFF;
    p | ((crc16(p) as u64) << 48)
}

/// 8-point orthonormal DCT-II basis value for frequency `u` at sample `x`.
fn basis(u: usize, x: usize) -> f32 {
    let c = if u == 0 {
        (1.0f32 / 8.0).sqrt()
    } else {
        (2.0f32 / 8.0).sqrt()
    };
    c * ((std::f32::consts::PI * (2 * x + 1) as f32 * u as f32) / 16.0).cos()
}

struct Plan {
    /// For each block: (bit index, chip per coefficient).
    blocks: Vec<(usize, [f32; 4])>,
    bx: usize,
}

fn plan(w: u32, h: u32, key: u64) -> Result<Plan> {
    let (bx, by) = (w as usize / 8, h as usize / 8);
    ensure!(
        bx * by >= MIN_BLOCKS,
        "Image too small for a watermark (needs about 128 x 128 pixels)"
    );
    let blocks = (0..bx * by)
        .map(|i| {
            let r = splitmix(key ^ (i as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93));
            let bit = (r % BITS as u64) as usize;
            let mut chips = [0f32; 4];
            for (k, c) in chips.iter_mut().enumerate() {
                *c = if (r >> (8 + k)) & 1 == 1 { 1.0 } else { -1.0 };
            }
            (bit, chips)
        })
        .collect();
    Ok(Plan { blocks, bx })
}

fn luma(p: &image::Rgb<u8>) -> f32 {
    0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
}

/// Embeds `payload` (lower 48 bits) with the given key. `strength` is the coefficient change in
/// 8-bit luminance units; 4.0 is the default (about 50 dB PSNR).
pub fn embed(img: &mut RgbImage, key: u64, payload: u64, strength: f32) -> Result<()> {
    ensure!(
        (0.5..=20.0).contains(&strength),
        "Watermark strength must be 0.5 to 20"
    );
    let pl = plan(img.width(), img.height(), key)?;
    let w = word(payload);
    let mut basis_tab = [[0f32; 8]; 8];
    for (u, row) in basis_tab.iter_mut().enumerate() {
        for (x, v) in row.iter_mut().enumerate() {
            *v = basis(u, x);
        }
    }
    for (i, (bit, chips)) in pl.blocks.iter().enumerate() {
        let sign = if (w >> bit) & 1 == 1 { 1.0 } else { -1.0 };
        let (ox, oy) = ((i % pl.bx) * 8, (i / pl.bx) * 8);
        for y in 0..8 {
            for x in 0..8 {
                let mut d = 0f32;
                for (k, &(u, v)) in COEFFS.iter().enumerate() {
                    d += sign * strength * chips[k] * basis_tab[u][x] * basis_tab[v][y];
                }
                let p = img.get_pixel_mut((ox + x) as u32, (oy + y) as u32);
                for c in p.0.iter_mut() {
                    *c = (*c as f32 + d).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    Ok(())
}

/// Looks for a watermark made with `key`.
pub fn detect(img: &RgbImage, key: u64) -> Result<Detection> {
    let pl = plan(img.width(), img.height(), key)?;
    let mut acc = [0f64; BITS];
    let mut energy = [0f64; BITS];
    for (i, (bit, chips)) in pl.blocks.iter().enumerate() {
        let (ox, oy) = ((i % pl.bx) * 8, (i / pl.bx) * 8);
        for (k, &(u, v)) in COEFFS.iter().enumerate() {
            let mut c = 0f32;
            for y in 0..8 {
                for x in 0..8 {
                    c += luma(img.get_pixel((ox + x) as u32, (oy + y) as u32))
                        * basis(u, x)
                        * basis(v, y);
                }
            }
            acc[*bit] += (c * chips[k]) as f64;
            energy[*bit] += (c as f64).powi(2);
        }
    }
    let mut word = 0u64;
    let mut conf = 0f64;
    for b in 0..BITS {
        if acc[b] > 0.0 {
            word |= 1 << b;
        }
        conf += acc[b].abs() / energy[b].max(1e-9).sqrt();
    }
    conf /= BITS as f64;
    let payload = word & 0xFFFF_FFFF_FFFF;
    let ok = (word >> 48) as u16 == crc16(payload);
    Ok(Detection {
        payload: ok.then_some(payload),
        confidence: conf,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo_like(w: u32, h: u32) -> RgbImage {
        let mut s = 7u64;
        RgbImage::from_fn(w, h, |x, y| {
            s = splitmix(s);
            let n = (s % 9) as f32 - 4.0;
            let base = 120.0 + 60.0 * ((x as f32 / 37.0).sin() * (y as f32 / 23.0).cos()) + n;
            let v = base.clamp(0.0, 255.0) as u8;
            image::Rgb([v, v.saturating_add(20), v.saturating_sub(15)])
        })
    }

    fn psnr(a: &RgbImage, b: &RgbImage) -> f64 {
        let mse: f64 = a
            .as_raw()
            .iter()
            .zip(b.as_raw())
            .map(|(x, y)| (*x as f64 - *y as f64).powi(2))
            .sum::<f64>()
            / a.as_raw().len() as f64;
        10.0 * (255f64.powi(2) / mse.max(1e-12)).log10()
    }

    fn jpeg(img: &RgbImage, q: u8) -> RgbImage {
        let mut buf = vec![];
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, q)
            .encode_image(img)
            .unwrap();
        image::load_from_memory(&buf).unwrap().to_rgb8()
    }

    #[test]
    fn embeds_detects_and_survives_jpeg() {
        let orig = photo_like(640, 480);
        let key = key_from_passphrase("owner secret");
        let mut wm = orig.clone();
        embed(&mut wm, key, 0xABCD_1234_5678, 4.0).unwrap();
        let q = psnr(&orig, &wm);
        assert!(q > 45.0, "invisible: PSNR {q:.1} dB");
        assert_eq!(detect(&wm, key).unwrap().payload, Some(0xABCD_1234_5678));
        for quality in [90, 80, 70] {
            let d = detect(&jpeg(&wm, quality), key).unwrap();
            assert_eq!(d.payload, Some(0xABCD_1234_5678), "JPEG q{quality}: {d:?}");
        }
        // No false positives: unmarked image and wrong key.
        assert_eq!(detect(&orig, key).unwrap().payload, None);
        assert_eq!(
            detect(&wm, key_from_passphrase("other")).unwrap().payload,
            None
        );
    }

    #[test]
    fn does_not_survive_resize_and_says_so() {
        let mut wm = photo_like(512, 512);
        let key = key_from_passphrase("k");
        embed(&mut wm, key, 42, 4.0).unwrap();
        let small = image::imageops::resize(&wm, 400, 400, image::imageops::FilterType::Triangle);
        assert_eq!(detect(&small, key).unwrap().payload, None);
        assert!(embed(&mut RgbImage::new(64, 64), key, 1, 4.0).is_err());
    }
}
