//! Adjustment layers with soft masks and blend modes, plus geometry corrections.
use crate::color::{Adjustments, rgb_to_hsl};
use crate::{Rect, Selection};
use anyhow::{Result, ensure};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    SoftLight,
    Luminosity,
    Color,
}
impl BlendMode {
    pub const ALL: [Self; 7] = [Self::Normal, Self::Multiply, Self::Screen, Self::Overlay, Self::SoftLight, Self::Luminosity, Self::Color];
    fn blend(self, base: [f32; 3], top: [f32; 3]) -> [f32; 3] {
        let per = |f: fn(f32, f32) -> f32| [f(base[0], top[0]), f(base[1], top[1]), f(base[2], top[2])];
        match self {
            Self::Normal => top,
            Self::Multiply => per(|b, t| b * t),
            Self::Screen => per(|b, t| 1. - (1. - b) * (1. - t)),
            Self::Overlay => per(|b, t| if b < 0.5 { 2. * b * t } else { 1. - 2. * (1. - b) * (1. - t) }),
            Self::SoftLight => per(|b, t| (1. - 2. * t) * b * b + 2. * t * b),
            Self::Luminosity | Self::Color => {
                let lb = 0.2126 * base[0] + 0.7152 * base[1] + 0.0722 * base[2];
                let lt = 0.2126 * top[0] + 0.7152 * top[1] + 0.0722 * top[2];
                let (src, l) = if self == Self::Luminosity { (base, lt - lb) } else { (top, lb - lt) };
                [src[0] + l, src[1] + l, src[2] + l]
            }
        }
    }
}
impl std::fmt::Display for BlendMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Soft masks for adjustment layers. Weights are 0..1 per pixel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LayerMask {
    Selection(Selection),
    /// Full effect at (x0,y0), fading to none at (x1,y1).
    Linear { x0: f32, y0: f32, x1: f32, y1: f32 },
    /// Full effect inside the ellipse, fading over `feather` (fraction of radius).
    Radial { cx: f32, cy: f32, rx: f32, ry: f32, feather: f32 },
    /// Pixels whose hue is within `tolerance` degrees of `hue` and saturation above `min_saturation`.
    ColorRange { hue: f32, tolerance: f32, min_saturation: f32 },
    /// Pixels with luminance in [min,max] with soft edges.
    Luminance { min: f32, max: f32, softness: f32 },
}
impl LayerMask {
    pub fn label(&self) -> String {
        match self {
            Self::Selection(s) => s.label(),
            Self::Linear { .. } => "linear gradient".into(),
            Self::Radial { .. } => "radial gradient".into(),
            Self::ColorRange { hue, .. } => format!("color range around {hue:.0} degrees"),
            Self::Luminance { min, max, .. } => format!("luminance {min:.2}-{max:.2}"),
        }
    }
    pub fn weights(&self, img: &RgbaImage) -> Result<Vec<f32>> {
        let (w, h) = img.dimensions();
        let n = w as usize * h as usize;
        let finite = |v: &[f32]| v.iter().all(|x| x.is_finite());
        Ok(match self {
            Self::Selection(s) => {
                let m = s.rasterize(w, h)?;
                let mut v = vec![0.; n];
                for (x, y) in m.iter_set() {
                    v[y as usize * w as usize + x as usize] = 1.;
                }
                v
            }
            Self::Linear { x0, y0, x1, y1 } => {
                ensure!(finite(&[*x0, *y0, *x1, *y1]), "Invalid gradient");
                let (dx, dy) = (x1 - x0, y1 - y0);
                let len2 = (dx * dx + dy * dy).max(1e-6);
                (0..n).map(|i| {
                    let (x, y) = ((i % w as usize) as f32 + 0.5, (i / w as usize) as f32 + 0.5);
                    1. - (((x - x0) * dx + (y - y0) * dy) / len2).clamp(0., 1.)
                }).collect()
            }
            Self::Radial { cx, cy, rx, ry, feather } => {
                ensure!(finite(&[*cx, *cy, *rx, *ry, *feather]) && *rx > 0. && *ry > 0. && (0.0..=2.0).contains(feather), "Invalid radial mask");
                (0..n).map(|i| {
                    let (x, y) = ((i % w as usize) as f32 + 0.5, (i / w as usize) as f32 + 0.5);
                    let d = (((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2)).sqrt();
                    if d <= 1. { 1. } else if *feather <= 0. { 0. } else { (1. - (d - 1.) / feather).clamp(0., 1.) }
                }).collect()
            }
            Self::ColorRange { hue, tolerance, min_saturation } => {
                ensure!(finite(&[*hue, *tolerance, *min_saturation]) && (1.0..=180.0).contains(tolerance), "Invalid color range");
                img.pixels().map(|p| {
                    let hsl = rgb_to_hsl([f32::from(p[0]) / 255., f32::from(p[1]) / 255., f32::from(p[2]) / 255.]);
                    let d = ((hsl[0] - hue + 180.).rem_euclid(360.) - 180.).abs();
                    if hsl[1] < *min_saturation { 0. } else { (1. - d / tolerance).clamp(0., 1.) }
                }).collect()
            }
            Self::Luminance { min, max, softness } => {
                ensure!(finite(&[*min, *max, *softness]) && min < max && *softness >= 0., "Invalid luminance range");
                img.pixels().map(|p| {
                    let l = (0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2])) / 255.;
                    let s = softness.max(1e-3);
                    ((l - min + s) / s).clamp(0., 1.).min(((max + s - l) / s).clamp(0., 1.))
                }).collect()
            }
        })
    }
}

/// Applies an adjustment layer: full adjustment, then blended into the base by mask x opacity.
pub fn apply_layer(img: RgbaImage, adj: &Adjustments, mask: Option<&LayerMask>, invert: bool, opacity: f32, blend: BlendMode) -> Result<RgbaImage> {
    ensure!(opacity.is_finite() && (0.0..=1.0).contains(&opacity), "Opacity must be 0..1");
    let top = adj.apply(&img)?;
    if mask.is_none() && opacity >= 1. && blend == BlendMode::Normal {
        return Ok(top);
    }
    let weights = match mask {
        Some(m) => m.weights(&img)?,
        None => vec![1.; img.width() as usize * img.height() as usize],
    };
    let mut out = img;
    for ((p, t), w) in out.pixels_mut().zip(top.pixels()).zip(weights) {
        let w = if invert { 1. - w } else { w } * opacity;
        if w <= 0. {
            continue;
        }
        let b = [f32::from(p[0]) / 255., f32::from(p[1]) / 255., f32::from(p[2]) / 255.];
        let c = blend.blend(b, [f32::from(t[0]) / 255., f32::from(t[1]) / 255., f32::from(t[2]) / 255.]);
        for i in 0..3 {
            p[i] = ((b[i] + (c[i].clamp(0., 1.) - b[i]) * w) * 255.).round().clamp(0., 255.) as u8;
        }
    }
    Ok(out)
}

/// Perspective correction: maps the quadrilateral `corners` (TL, TR, BR, BL) to a rectangle
/// of the given output size using a homography with bilinear sampling.
pub fn perspective(img: &RgbaImage, corners: [(f32, f32); 4], out_w: u32, out_h: u32) -> Result<RgbaImage> {
    ensure!(corners.iter().all(|(x, y)| x.is_finite() && y.is_finite()), "Invalid corners");
    ensure!(out_w > 0 && out_h > 0 && u64::from(out_w) * u64::from(out_h) <= photo_core::MAX_PIXELS, "Invalid output size");
    let dst = [(0., 0.), (out_w as f64, 0.), (out_w as f64, out_h as f64), (0., out_h as f64)];
    let src: Vec<(f64, f64)> = corners.iter().map(|(x, y)| (f64::from(*x), f64::from(*y))).collect();
    // Homography from destination to source (inverse mapping).
    let h = homography(&dst, &[src[0], src[1], src[2], src[3]]).ok_or_else(|| anyhow::anyhow!("Corners are degenerate"))?;
    let mut out = RgbaImage::new(out_w, out_h);
    for (x, y, px) in out.enumerate_pixels_mut() {
        let (u, v) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
        let d = h[6] * u + h[7] * v + 1.;
        let sx = (h[0] * u + h[1] * v + h[2]) / d - 0.5;
        let sy = (h[3] * u + h[4] * v + h[5]) / d - 0.5;
        *px = sample(img, sx, sy);
    }
    Ok(out)
}
fn homography(src: &[(f64, f64); 4], dst: &[(f64, f64); 4]) -> Option<[f64; 8]> {
    let mut a = [[0f64; 9]; 8];
    for i in 0..4 {
        let ((x, y), (u, v)) = (src[i], dst[i]);
        a[2 * i] = [x, y, 1., 0., 0., 0., -u * x, -u * y, u];
        a[2 * i + 1] = [0., 0., 0., x, y, 1., -v * x, -v * y, v];
    }
    for c in 0..8 {
        let p = (c..8).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[p][c].abs() < 1e-9 {
            return None;
        }
        a.swap(c, p);
        for r in 0..8 {
            if r != c {
                let f = a[r][c] / a[c][c];
                for k in c..9 {
                    a[r][k] -= f * a[c][k];
                }
            }
        }
    }
    let mut h = [0.; 8];
    for i in 0..8 {
        h[i] = a[i][8] / a[i][i];
    }
    Some(h)
}
fn sample(img: &RgbaImage, x: f64, y: f64) -> Rgba<u8> {
    let (w, h) = img.dimensions();
    if x < -0.5 || y < -0.5 || x > f64::from(w) - 0.5 || y > f64::from(h) - 0.5 {
        return Rgba([0, 0, 0, 0]);
    }
    let x = x.clamp(0., f64::from(w - 1));
    let y = y.clamp(0., f64::from(h - 1));
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - f64::from(x0), y - f64::from(y0));
    let mut o = [0u8; 4];
    for (c, v) in o.iter_mut().enumerate() {
        let p = |a, b| f64::from(img.get_pixel(a, b)[c]);
        *v = ((p(x0, y0) * (1. - fx) + p(x1, y0) * fx) * (1. - fy) + (p(x0, y1) * (1. - fx) + p(x1, y1) * fx) * fy).round() as u8;
    }
    Rgba(o)
}

/// Simple radial lens distortion correction (k1 > 0 corrects barrel, < 0 pincushion).
/// This is a one-parameter model, not a camera lens profile.
pub fn lens_correct(img: &RgbaImage, k1: f32) -> Result<RgbaImage> {
    ensure!(k1.is_finite() && (-0.5..=0.5).contains(&k1), "Distortion must be -0.5..0.5");
    let (w, h) = img.dimensions();
    let (cx, cy) = (f64::from(w) / 2., f64::from(h) / 2.);
    let norm = (cx * cx + cy * cy).sqrt();
    let k = f64::from(k1);
    // Scale so the corners stay inside the source after correction.
    let scale = 1. / (1. + k.max(0.));
    let mut out = RgbaImage::new(w, h);
    for (x, y, px) in out.enumerate_pixels_mut() {
        let (dx, dy) = ((f64::from(x) + 0.5 - cx) / norm * scale, (f64::from(y) + 0.5 - cy) / norm * scale);
        let r2 = dx * dx + dy * dy;
        let f = 1. + k * r2;
        *px = sample(img, cx + dx * f * norm - 0.5, cy + dy * f * norm - 0.5);
    }
    Ok(out)
}

/// Canvas resize centred on the image, padding with an opaque colour.
pub fn canvas(img: &RgbaImage, width: u32, height: u32, fill: [u8; 3]) -> Result<RgbaImage> {
    ensure!(width > 0 && height > 0 && u64::from(width) * u64::from(height) <= photo_core::MAX_PIXELS, "Invalid canvas size");
    let mut out = RgbaImage::from_pixel(width, height, Rgba([fill[0], fill[1], fill[2], 255]));
    let ox = i64::from(width) / 2 - i64::from(img.width()) / 2;
    let oy = i64::from(height) / 2 - i64::from(img.height()) / 2;
    image::imageops::replace(&mut out, img, ox, oy);
    Ok(out)
}

/// Opaque or translucent fill of a selection (shape layer). Only opacity 255 counts as redaction.
pub fn fill(img: &mut RgbaImage, sel: &Selection, rgba: [u8; 4]) -> Result<()> {
    let m = sel.rasterize(img.width(), img.height())?;
    let a = f32::from(rgba[3]) / 255.;
    for (x, y) in m.iter_set() {
        let p = img.get_pixel_mut(x, y);
        for i in 0..3 {
            p[i] = (f32::from(p[i]) * (1. - a) + f32::from(rgba[i]) * a).round() as u8;
        }
        if rgba[3] == 255 {
            p[3] = 255;
        }
    }
    Ok(())
}

pub fn rect_corners(r: Rect) -> [(f32, f32); 4] {
    let (x, y, w, h) = (r.x as f32, r.y as f32, r.width as f32, r.height as f32);
    [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn masks_blend_and_opacity() {
        let img = RgbaImage::from_pixel(20, 10, Rgba([100, 100, 100, 255]));
        let adj = Adjustments { exposure: 1., ..Default::default() };
        let half = apply_layer(img.clone(), &adj, None, false, 0.5, BlendMode::Normal).unwrap();
        let full = apply_layer(img.clone(), &adj, None, false, 1., BlendMode::Normal).unwrap();
        assert!(half.get_pixel(0, 0)[0] > 100 && half.get_pixel(0, 0)[0] < full.get_pixel(0, 0)[0]);
        let grad = LayerMask::Linear { x0: 0., y0: 0., x1: 20., y1: 0. };
        let g = apply_layer(img.clone(), &adj, Some(&grad), false, 1., BlendMode::Normal).unwrap();
        assert!(g.get_pixel(0, 5)[0] > g.get_pixel(19, 5)[0]);
        let inv = apply_layer(img.clone(), &adj, Some(&grad), true, 1., BlendMode::Normal).unwrap();
        assert!(inv.get_pixel(0, 5)[0] < inv.get_pixel(19, 5)[0]);
        let radial = LayerMask::Radial { cx: 10., cy: 5., rx: 3., ry: 3., feather: 0. };
        let w = radial.weights(&img).unwrap();
        assert_eq!(w[5 * 20 + 10], 1.);
        assert_eq!(w[0], 0.);
        let m = apply_layer(img.clone(), &Adjustments { exposure: -1., ..Default::default() }, None, false, 1., BlendMode::Screen).unwrap();
        assert!(m.get_pixel(0, 0)[0] >= 100, "screen never darkens");
        assert!(apply_layer(img, &adj, None, false, 2., BlendMode::Normal).is_err());
    }
    #[test]
    fn color_and_luminance_ranges() {
        let mut img = RgbaImage::from_pixel(2, 1, Rgba([20, 20, 220, 255]));
        img.put_pixel(1, 0, Rgba([220, 20, 20, 255]));
        let w = LayerMask::ColorRange { hue: 240., tolerance: 30., min_saturation: 0.2 }.weights(&img).unwrap();
        assert!(w[0] > 0.9 && w[1] == 0.);
        let l = LayerMask::Luminance { min: 0.0, max: 0.2, softness: 0.01 }.weights(&img).unwrap();
        assert!(l[0] > 0.9);
    }
    #[test]
    fn geometry_corrections() {
        let img = RgbaImage::from_fn(40, 30, |x, y| Rgba([x as u8 * 6, y as u8 * 8, 0, 255]));
        let same = perspective(&img, rect_corners(Rect::new(0, 0, 40, 30)), 40, 30).unwrap();
        let diff = same.pixels().zip(img.pixels()).map(|(a, b)| a[0].abs_diff(b[0])).max().unwrap();
        assert!(diff <= 1, "identity homography");
        assert!(perspective(&img, [(0., 0.); 4], 10, 10).is_err());
        let skew = perspective(&img, [(5., 0.), (35., 0.), (40., 30.), (0., 30.)], 40, 30).unwrap();
        assert_eq!(skew.dimensions(), (40, 30));
        assert_eq!(lens_correct(&img, 0.).unwrap(), img);
        assert!(lens_correct(&img, 0.2).is_ok() && lens_correct(&img, 3.).is_err());
        let c = canvas(&img, 60, 50, [255, 255, 255]).unwrap();
        assert_eq!(c.get_pixel(0, 0), &Rgba([255, 255, 255, 255]));
        assert_eq!(c.get_pixel(10, 10), img.get_pixel(0, 0));
    }
}
