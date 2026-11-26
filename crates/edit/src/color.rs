//! Color Studio: photographic adjustments computed in 32-bit float.
//!
//! White balance and exposure are applied in linear light (sRGB transfer removed); tone, curve,
//! HSL and grading controls operate on display-referred (sRGB-encoded) values, which is how
//! photographers expect those controls to respond. Results are quantized to 8 bits once, at the
//! end of an adjustment layer. Neighbourhood operations (clarity, texture, sharpening, noise
//! reduction) run after the per-pixel stage. Work is split across CPU threads by rows.
use anyhow::{Result, bail, ensure};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

pub const HSL_BANDS: [&str; 8] = ["Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple", "Magenta"];
const BAND_CENTERS: [f32; 8] = [0., 30., 60., 120., 180., 225., 270., 315.];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    /// Control points in [0,1]x[0,1], sorted by x. Identity when empty.
    pub points: Vec<(f32, f32)>,
}
impl Default for Curve {
    fn default() -> Self {
        Self { points: vec![] }
    }
}
impl Curve {
    pub fn is_identity(&self) -> bool {
        self.points.is_empty() || self.points.iter().all(|(x, y)| (x - y).abs() < 1e-6)
    }
    /// Parametric tone curve: shadows, darks, lights, highlights offsets in [-1,1].
    pub fn parametric(shadows: f32, darks: f32, lights: f32, highlights: f32) -> Self {
        let k = 0.25;
        Self {
            points: vec![
                (0., 0.),
                (0.125, (0.125 + k * 0.5 * shadows).clamp(0., 1.)),
                (0.375, (0.375 + k * darks).clamp(0., 1.)),
                (0.625, (0.625 + k * lights).clamp(0., 1.)),
                (0.875, (0.875 + k * 0.5 * highlights).clamp(0., 1.)),
                (1., 1.),
            ],
        }
    }
    fn validate(&self) -> Result<()> {
        ensure!(self.points.len() <= 32, "Too many curve points");
        ensure!(self.points.iter().all(|(x, y)| x.is_finite() && y.is_finite() && (0.0..=1.0).contains(x) && (0.0..=1.0).contains(y)), "Curve points must be in [0,1]");
        ensure!(self.points.windows(2).all(|w| w[0].0 < w[1].0), "Curve x values must increase");
        Ok(())
    }
    /// Monotone cubic (Fritsch-Carlson) lookup table with 1024 entries.
    pub fn lut(&self) -> Vec<f32> {
        let n = 1024;
        if self.is_identity() || self.points.len() < 2 {
            return (0..n).map(|i| i as f32 / (n - 1) as f32).collect();
        }
        let p = &self.points;
        let m = p.len();
        let d: Vec<f32> = p.windows(2).map(|w| (w[1].1 - w[0].1) / (w[1].0 - w[0].0)).collect();
        let mut t = vec![0f32; m];
        t[0] = d[0];
        t[m - 1] = d[m - 2];
        for i in 1..m - 1 {
            t[i] = if d[i - 1] * d[i] <= 0. { 0. } else { (d[i - 1] + d[i]) / 2. };
        }
        for i in 0..m - 1 {
            if d[i] == 0. {
                t[i] = 0.;
                t[i + 1] = 0.;
            } else {
                let (a, b) = (t[i] / d[i], t[i + 1] / d[i]);
                let s = a * a + b * b;
                if s > 9. {
                    let tau = 3. / s.sqrt();
                    t[i] = tau * a * d[i];
                    t[i + 1] = tau * b * d[i];
                }
            }
        }
        (0..n)
            .map(|i| {
                let x = i as f32 / (n - 1) as f32;
                if x <= p[0].0 {
                    return p[0].1;
                }
                if x >= p[m - 1].0 {
                    return p[m - 1].1;
                }
                let k = p.windows(2).position(|w| x >= w[0].0 && x <= w[1].0).unwrap_or(0);
                let h = p[k + 1].0 - p[k].0;
                let s = (x - p[k].0) / h;
                let (h00, h10, h01, h11) = (2. * s.powi(3) - 3. * s * s + 1., s.powi(3) - 2. * s * s + s, -2. * s.powi(3) + 3. * s * s, s.powi(3) - s * s);
                (h00 * p[k].1 + h10 * h * t[k] + h01 * p[k + 1].1 + h11 * h * t[k + 1]).clamp(0., 1.)
            })
            .collect()
    }
}

/// A 3D lookup table parsed from an Adobe/IRIDAS .cube file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lut3d {
    pub size: usize,
    pub data: Vec<[f32; 3]>,
    pub domain_min: [f32; 3],
    pub domain_max: [f32; 3],
}
pub const MAX_CUBE_BYTES: usize = 16 * 1024 * 1024;
impl Lut3d {
    /// Parses untrusted .cube text with strict bounds (size 2..=65, finite values).
    pub fn parse_cube(text: &str) -> Result<Self> {
        ensure!(text.len() <= MAX_CUBE_BYTES, "LUT file too large");
        let (mut size, mut data) = (0usize, Vec::new());
        let (mut dmin, mut dmax) = ([0f32; 3], [1f32; 3]);
        for line in text.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with('#') || l.starts_with("TITLE") {
                continue;
            }
            let mut it = l.split_whitespace();
            match it.next() {
                Some("LUT_3D_SIZE") => {
                    size = it.next().unwrap_or("").parse()?;
                    ensure!((2..=65).contains(&size), "LUT size must be 2..65");
                    data.reserve(size * size * size);
                }
                Some("LUT_1D_SIZE") => bail!("1D LUTs are not supported"),
                Some("DOMAIN_MIN") | Some("DOMAIN_MAX") => {
                    let v: Vec<f32> = it.map(str::parse).collect::<Result<_, _>>()?;
                    ensure!(v.len() == 3 && v.iter().all(|x| x.is_finite()), "Invalid LUT domain");
                    if l.starts_with("DOMAIN_MIN") { dmin = [v[0], v[1], v[2]] } else { dmax = [v[0], v[1], v[2]] }
                }
                Some(first) => {
                    ensure!(size > 0, "LUT data before LUT_3D_SIZE");
                    let v: Vec<f32> = std::iter::once(first).chain(it).map(str::parse).collect::<Result<_, _>>()?;
                    ensure!(v.len() == 3 && v.iter().all(|x| x.is_finite() && (-4.0..=4.0).contains(x)), "Invalid LUT entry");
                    ensure!(data.len() < size * size * size, "Too many LUT entries");
                    data.push([v[0], v[1], v[2]]);
                }
                None => {}
            }
        }
        ensure!(size > 0 && data.len() == size * size * size, "LUT has {} entries, expected {}", data.len(), size * size * size);
        ensure!((0..3).all(|c| dmax[c] > dmin[c]), "Invalid LUT domain");
        Ok(Self { size, data, domain_min: dmin, domain_max: dmax })
    }
    pub fn to_cube(&self, title: &str) -> String {
        let mut s = format!("TITLE \"{}\"\nLUT_3D_SIZE {}\n", title.replace(['"', '\n', '\r'], ""), self.size);
        for v in &self.data {
            s.push_str(&format!("{:.6} {:.6} {:.6}\n", v[0], v[1], v[2]));
        }
        s
    }
    /// Trilinear sample. Red varies fastest, per the .cube specification.
    pub fn sample(&self, rgb: [f32; 3]) -> [f32; 3] {
        let n = self.size;
        let f = |c: usize| ((rgb[c] - self.domain_min[c]) / (self.domain_max[c] - self.domain_min[c])).clamp(0., 1.) * (n - 1) as f32;
        let (r, g, b) = (f(0), f(1), f(2));
        let (r0, g0, b0) = (r.floor() as usize, g.floor() as usize, b.floor() as usize);
        let (r1, g1, b1) = ((r0 + 1).min(n - 1), (g0 + 1).min(n - 1), (b0 + 1).min(n - 1));
        let (fr, fg, fb) = (r - r0 as f32, g - g0 as f32, b - b0 as f32);
        let at = |r: usize, g: usize, b: usize| self.data[r + g * n + b * n * n];
        let mut out = [0.; 3];
        for (c, o) in out.iter_mut().enumerate() {
            let c00 = at(r0, g0, b0)[c] * (1. - fr) + at(r1, g0, b0)[c] * fr;
            let c10 = at(r0, g1, b0)[c] * (1. - fr) + at(r1, g1, b0)[c] * fr;
            let c01 = at(r0, g0, b1)[c] * (1. - fr) + at(r1, g0, b1)[c] * fr;
            let c11 = at(r0, g1, b1)[c] * (1. - fr) + at(r1, g1, b1)[c] * fr;
            *o = (c00 * (1. - fg) + c10 * fg) * (1. - fb) + (c01 * (1. - fg) + c11 * fg) * fb;
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Adjustments {
    pub exposure: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub saturation: f32,
    pub vibrance: f32,
    pub temperature: f32,
    pub tint: f32,
    pub clarity: f32,
    pub texture: f32,
    pub dehaze: f32,
    pub sharpness: f32,
    pub noise_reduction: f32,
    pub vignette: f32,
    pub grain: f32,
    pub gamma: f32,
    pub black_point: f32,
    pub white_point: f32,
    /// Color balance offsets for shadows, midtones and highlights (RGB each, -0.25..0.25).
    pub balance_shadows: [f32; 3],
    pub balance_midtones: [f32; 3],
    pub balance_highlights: [f32; 3],
    /// Split toning: hue in degrees, strength 0..1.
    pub split_shadow_hue: f32,
    pub split_shadow_strength: f32,
    pub split_highlight_hue: f32,
    pub split_highlight_strength: f32,
    /// Per-band hue shift (-1..1 = +-30 degrees), saturation and luminance (-1..1).
    pub hsl_hue: [f32; 8],
    pub hsl_saturation: [f32; 8],
    pub hsl_luminance: [f32; 8],
    /// Luminance vs saturation for shadows, midtones, highlights (-1..1).
    pub lum_saturation: [f32; 3],
    pub curve_master: Curve,
    pub curve_red: Curve,
    pub curve_green: Curve,
    pub curve_blue: Curve,
    pub lut: Option<Lut3d>,
    pub lut_strength: f32,
}
impl Default for Adjustments {
    fn default() -> Self {
        Self {
            exposure: 0., brightness: 0., contrast: 0., highlights: 0., shadows: 0., whites: 0., blacks: 0.,
            saturation: 0., vibrance: 0., temperature: 0., tint: 0., clarity: 0., texture: 0., dehaze: 0.,
            sharpness: 0., noise_reduction: 0., vignette: 0., grain: 0., gamma: 1., black_point: 0., white_point: 1.,
            balance_shadows: [0.; 3], balance_midtones: [0.; 3], balance_highlights: [0.; 3],
            split_shadow_hue: 220., split_shadow_strength: 0., split_highlight_hue: 40., split_highlight_strength: 0.,
            hsl_hue: [0.; 8], hsl_saturation: [0.; 8], hsl_luminance: [0.; 8], lum_saturation: [0.; 3],
            curve_master: Curve::default(), curve_red: Curve::default(), curve_green: Curve::default(), curve_blue: Curve::default(),
            lut: None, lut_strength: 1.,
        }
    }
}

/// Built-in creative presets (original parameter sets, not copies of any product's presets).
pub fn presets() -> Vec<(&'static str, Adjustments)> {
    let base = Adjustments::default;
    vec![
        ("Neutral", base()),
        ("Clean contrast", Adjustments { contrast: 0.2, clarity: 0.15, vibrance: 0.1, ..base() }),
        ("Soft matte", Adjustments { contrast: -0.15, black_point: 0., curve_master: Curve { points: vec![(0., 0.06), (0.5, 0.5), (1., 0.96)] }, saturation: -0.1, ..base() }),
        ("Warm evening", Adjustments { temperature: 0.25, tint: 0.05, split_highlight_strength: 0.2, vibrance: 0.1, ..base() }),
        ("Cool overcast", Adjustments { temperature: -0.2, contrast: 0.1, saturation: -0.1, ..base() }),
        ("Monochrome", Adjustments { saturation: -1., contrast: 0.15, clarity: 0.1, ..base() }),
        ("Teal and orange", Adjustments { split_shadow_hue: 190., split_shadow_strength: 0.25, split_highlight_hue: 30., split_highlight_strength: 0.25, contrast: 0.1, ..base() }),
    ]
}

impl Adjustments {
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }
    pub fn validate(&self) -> Result<()> {
        let unit = [
            self.brightness, self.contrast, self.highlights, self.shadows, self.whites, self.blacks, self.saturation,
            self.vibrance, self.temperature, self.tint, self.clarity, self.texture, self.dehaze, self.vignette,
        ];
        ensure!(unit.iter().all(|v| v.is_finite() && (-1.0..=1.0).contains(v)), "Adjustment out of range (-1..1)");
        ensure!(self.exposure.is_finite() && (-5.0..=5.0).contains(&self.exposure), "Exposure must be -5..5 EV");
        ensure!([self.sharpness, self.noise_reduction, self.grain, self.split_shadow_strength, self.split_highlight_strength, self.lut_strength].iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "Strength must be 0..1");
        ensure!(self.gamma.is_finite() && (0.2..=5.0).contains(&self.gamma), "Gamma must be 0.2..5");
        ensure!(self.black_point.is_finite() && self.white_point.is_finite() && self.black_point >= 0. && self.white_point <= 1. && self.white_point - self.black_point >= 0.05, "Invalid black/white point");
        let arrays = self.hsl_hue.iter().chain(&self.hsl_saturation).chain(&self.hsl_luminance).chain(&self.lum_saturation);
        ensure!(arrays.clone().all(|v| v.is_finite() && (-1.0..=1.0).contains(v)), "HSL value out of range");
        let bal = self.balance_shadows.iter().chain(&self.balance_midtones).chain(&self.balance_highlights);
        ensure!(bal.clone().all(|v| v.is_finite() && (-0.25..=0.25).contains(v)), "Color balance out of range");
        ensure!(self.split_shadow_hue.is_finite() && self.split_highlight_hue.is_finite(), "Invalid hue");
        for c in [&self.curve_master, &self.curve_red, &self.curve_green, &self.curve_blue] {
            c.validate()?;
        }
        if let Some(l) = &self.lut {
            ensure!((2..=65).contains(&l.size) && l.data.len() == l.size.pow(3), "Invalid LUT");
        }
        Ok(())
    }
}

fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}
fn linear_to_srgb(v: f32) -> f32 {
    let v = v.max(0.);
    if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1. / 2.4) - 0.055 }
}
fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}
pub fn rgb_to_hsl(c: [f32; 3]) -> [f32; 3] {
    let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
    let l = (mx + mn) / 2.;
    if mx - mn < 1e-6 {
        return [0., 0., l];
    }
    let d = mx - mn;
    let s = if l > 0.5 { d / (2. - mx - mn) } else { d / (mx + mn) };
    let h = if mx == c[0] {
        ((c[1] - c[2]) / d).rem_euclid(6.)
    } else if mx == c[1] {
        (c[2] - c[0]) / d + 2.
    } else {
        (c[0] - c[1]) / d + 4.
    } * 60.;
    [h, s, l]
}
pub fn hsl_to_rgb(h: [f32; 3]) -> [f32; 3] {
    let (hh, s, l) = (h[0].rem_euclid(360.), h[1].clamp(0., 1.), h[2].clamp(0., 1.));
    let c = (1. - (2. * l - 1.).abs()) * s;
    let x = c * (1. - ((hh / 60.).rem_euclid(2.) - 1.).abs());
    let m = l - c / 2.;
    let (r, g, b) = match (hh / 60.) as u32 {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    [r + m, g + m, b + m]
}
fn band_weights(h: f32) -> [f32; 8] {
    let mut w = [0f32; 8];
    for (i, c) in BAND_CENTERS.iter().enumerate() {
        let next = BAND_CENTERS[(i + 1) % 8] + if i == 7 { 360. } else { 0. };
        let prev = if i == 0 { BAND_CENTERS[7] - 360. } else { BAND_CENTERS[i - 1] };
        let d = (h - c + 180.).rem_euclid(360.) - 180.;
        w[i] = if d >= 0. { (1. - d / (next - c)).max(0.) } else { (1. + d / (c - prev)).max(0.) };
    }
    w
}

struct Prepared {
    to_linear: Vec<f32>,
    master: Vec<f32>,
    red: Vec<f32>,
    green: Vec<f32>,
    blue: Vec<f32>,
    wb: [f32; 3],
    gain: f32,
}
fn lookup(lut: &[f32], v: f32) -> f32 {
    let x = v.clamp(0., 1.) * (lut.len() - 1) as f32;
    let i = x.floor() as usize;
    let j = (i + 1).min(lut.len() - 1);
    lut[i] + (lut[j] - lut[i]) * (x - i as f32)
}

impl Adjustments {
    fn prepare(&self) -> Prepared {
        let t = self.temperature;
        let g = self.tint;
        Prepared {
            to_linear: (0..256).map(|i| srgb_to_linear(i as f32 / 255.)).collect(),
            master: self.curve_master.lut(),
            red: self.curve_red.lut(),
            green: self.curve_green.lut(),
            blue: self.curve_blue.lut(),
            wb: [1. + 0.35 * t + 0.1 * g, 1. - 0.25 * g, 1. - 0.35 * t + 0.1 * g],
            gain: 2f32.powf(self.exposure),
        }
    }
    /// Per-pixel stage on display-referred input in [0,1]; returns display-referred output.
    fn pixel(&self, p: &Prepared, px: [u8; 3]) -> [f32; 3] {
        let mut c = [0f32; 3];
        for i in 0..3 {
            c[i] = linear_to_srgb(p.to_linear[px[i] as usize] * p.wb[i] * p.gain).min(1.5);
        }
        // Levels and gamma.
        let (bp, wp) = (self.black_point, self.white_point);
        for v in c.iter_mut() {
            *v = ((*v - bp) / (wp - bp)).max(0.);
            if self.gamma != 1. {
                *v = v.powf(1. / self.gamma);
            }
        }
        // Tone: highlights/shadows/whites/blacks with smooth luminance masks.
        let l = luma(c).clamp(0., 1.);
        let delta = 0.35 * self.shadows * (1. - l).powi(2) * l.sqrt()
            + 0.35 * self.highlights * l.powi(2) * (1. - l).sqrt()
            + 0.2 * self.whites * l.powi(4)
            + 0.2 * self.blacks * (1. - l).powi(4);
        for v in c.iter_mut() {
            *v += delta;
        }
        // Brightness (midtone lift) and contrast around middle grey.
        for v in c.iter_mut() {
            let x = v.clamp(0., 1.5);
            let x = x + self.brightness * 0.5 * x.min(1.) * (1. - x.min(1.)) * 2.;
            *v = (x - 0.5) * (1. + self.contrast) + 0.5;
        }
        if self.dehaze != 0. {
            // Approximate global dehaze: subtract a uniform veil and restore range.
            let a = 0.12 * self.dehaze;
            for v in c.iter_mut() {
                *v = (*v - a) / (1. - a);
            }
        }
        // Curves.
        c = [
            lookup(&p.red, lookup(&p.master, c[0])),
            lookup(&p.green, lookup(&p.master, c[1])),
            lookup(&p.blue, lookup(&p.master, c[2])),
        ];
        // HSL bands, saturation, vibrance, luminance-vs-saturation.
        let mut hsl = rgb_to_hsl(c);
        let w = band_weights(hsl[0]);
        let (mut dh, mut ds, mut dl) = (0., 0., 0.);
        for i in 0..8 {
            dh += w[i] * self.hsl_hue[i] * 30.;
            ds += w[i] * self.hsl_saturation[i];
            dl += w[i] * self.hsl_luminance[i];
        }
        hsl[0] += dh;
        hsl[1] *= 1. + ds;
        let lw = [(1. - hsl[2]).powi(2), 1. - (2. * hsl[2] - 1.).powi(2), hsl[2].powi(2)];
        hsl[1] *= 1. + lw[0] * self.lum_saturation[0] + lw[1] * self.lum_saturation[1] + lw[2] * self.lum_saturation[2];
        hsl[1] *= 1. + self.saturation;
        hsl[1] *= 1. + self.vibrance * (1. - hsl[1].clamp(0., 1.));
        hsl[1] = hsl[1].clamp(0., 1.);
        hsl[2] = (hsl[2] + dl * 0.2 * hsl[1]).clamp(0., 1.);
        c = hsl_to_rgb(hsl);
        // Color balance and split toning.
        let l = luma(c).clamp(0., 1.);
        let (ws, wm, wh) = ((1. - l).powi(2), 1. - (2. * l - 1.).powi(2), l.powi(2));
        let st = hsl_to_rgb([self.split_shadow_hue, 1., 0.5]);
        let ht = hsl_to_rgb([self.split_highlight_hue, 1., 0.5]);
        for i in 0..3 {
            c[i] += ws * self.balance_shadows[i] + wm * self.balance_midtones[i] + wh * self.balance_highlights[i];
            c[i] += ws * self.split_shadow_strength * 0.25 * (st[i] - 0.5) + wh * self.split_highlight_strength * 0.25 * (ht[i] - 0.5);
        }
        if let Some(lut) = &self.lut {
            let o = lut.sample([c[0].clamp(0., 1.), c[1].clamp(0., 1.), c[2].clamp(0., 1.)]);
            for i in 0..3 {
                c[i] += (o[i] - c[i]) * self.lut_strength;
            }
        }
        c
    }

    /// Applies the adjustments to an image. Alpha is preserved.
    pub fn apply(&self, img: &RgbaImage) -> Result<RgbaImage> {
        self.validate()?;
        let (w, h) = img.dimensions();
        let p = self.prepare();
        let mut out = RgbaImage::new(w, h);
        parallel_rows(img, &mut out, |_, src, dst| {
            for (s, d) in src.chunks_exact(4).zip(dst.chunks_exact_mut(4)) {
                let c = self.pixel(&p, [s[0], s[1], s[2]]);
                for i in 0..3 {
                    d[i] = (c[i].clamp(0., 1.) * 255.).round() as u8;
                }
                d[3] = s[3];
            }
        });
        // Neighbourhood detail operations.
        if self.clarity != 0. {
            local_contrast(&mut out, 18., self.clarity * 0.6, true);
        }
        if self.texture != 0. {
            local_contrast(&mut out, 3., self.texture * 0.5, false);
        }
        if self.noise_reduction > 0. {
            let blurred = image::imageops::blur(&out, 0.6 + 1.4 * self.noise_reduction);
            for (o, b) in out.pixels_mut().zip(blurred.pixels()) {
                for i in 0..3 {
                    o[i] = (f32::from(o[i]) + (f32::from(b[i]) - f32::from(o[i])) * self.noise_reduction * 0.8).round() as u8;
                }
            }
        }
        if self.sharpness > 0. {
            local_contrast(&mut out, 1., self.sharpness * 1.2, false);
        }
        if self.vignette != 0. || self.grain > 0. {
            let (cx, cy) = (w as f32 / 2., h as f32 / 2.);
            let r2 = cx * cx + cy * cy;
            for (x, y, px) in out.enumerate_pixels_mut() {
                let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)) / r2;
                let v = 1. - self.vignette * d.powi(2) * 0.8;
                // Deterministic per-pixel noise so renders are reproducible.
                let n = (((x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663)) % 1000) as f32 / 1000. - 0.5) * self.grain * 40.;
                for i in 0..3 {
                    px[i] = (f32::from(px[i]) * v + n).clamp(0., 255.).round() as u8;
                }
            }
        }
        Ok(out)
    }

    /// Bakes the per-pixel stage into a 3D LUT (neighbourhood effects cannot be represented).
    pub fn bake_lut(&self, size: usize) -> Result<Lut3d> {
        self.validate()?;
        ensure!((2..=65).contains(&size), "LUT size must be 2..65");
        let p = self.prepare();
        let mut data = Vec::with_capacity(size.pow(3));
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    let q = |v: usize| (v as f32 / (size - 1) as f32 * 255.).round() as u8;
                    let c = self.pixel(&p, [q(r), q(g), q(b)]);
                    data.push([c[0].clamp(0., 1.), c[1].clamp(0., 1.), c[2].clamp(0., 1.)]);
                }
            }
        }
        Ok(Lut3d { size, data, domain_min: [0.; 3], domain_max: [1.; 3] })
    }
}

fn local_contrast(img: &mut RgbaImage, sigma: f32, amount: f32, midtones_only: bool) {
    let blurred = image::imageops::blur(img, sigma);
    for (o, b) in img.pixels_mut().zip(blurred.pixels()) {
        let l = (0.2126 * f32::from(o[0]) + 0.7152 * f32::from(o[1]) + 0.0722 * f32::from(o[2])) / 255.;
        let weight = if midtones_only { 1. - (2. * l - 1.).powi(2) } else { 1. };
        for i in 0..3 {
            let v = f32::from(o[i]) + (f32::from(o[i]) - f32::from(b[i])) * amount * weight;
            o[i] = v.clamp(0., 255.).round() as u8;
        }
    }
}

/// Runs `f(row_index, src_row, dst_row)` on disjoint row bands using scoped threads.
pub fn parallel_rows(src: &RgbaImage, dst: &mut RgbaImage, f: impl Fn(u32, &[u8], &mut [u8]) + Sync) {
    let w = src.width() as usize * 4;
    let rows = src.height() as usize;
    if w == 0 || rows == 0 {
        return;
    }
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(16).min(rows);
    let band = rows.div_ceil(threads);
    let s = src.as_raw();
    let d: &mut [u8] = dst;
    std::thread::scope(|scope| {
        for (k, chunk) in d.chunks_mut(band * w).enumerate() {
            let f = &f;
            let start = k * band;
            scope.spawn(move || {
                for (j, row) in chunk.chunks_mut(w).enumerate() {
                    let y = start + j;
                    f(y as u32, &s[y * w..(y + 1) * w], row);
                }
            });
        }
    });
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Histogram {
    pub red: Vec<u32>,
    pub green: Vec<u32>,
    pub blue: Vec<u32>,
    pub luma: Vec<u32>,
    pub pixels: u64,
    /// Pixels with any channel at 0 / at 255.
    pub clipped_shadows: u64,
    pub clipped_highlights: u64,
}
pub fn histogram(img: &RgbaImage) -> Histogram {
    let mut h = Histogram { red: vec![0; 256], green: vec![0; 256], blue: vec![0; 256], luma: vec![0; 256], pixels: 0, clipped_shadows: 0, clipped_highlights: 0 };
    for p in img.pixels() {
        if p[3] == 0 {
            continue;
        }
        h.red[p[0] as usize] += 1;
        h.green[p[1] as usize] += 1;
        h.blue[p[2] as usize] += 1;
        let l = (0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2])).round() as usize;
        h.luma[l.min(255)] += 1;
        h.pixels += 1;
        if p[0] == 0 || p[1] == 0 || p[2] == 0 {
            h.clipped_shadows += 1;
        }
        if p[0] == 255 || p[1] == 255 || p[2] == 255 {
            h.clipped_highlights += 1;
        }
    }
    h
}

/// Clipping overlay: blue where shadows clip, red where highlights clip.
pub fn clipping_overlay(img: &RgbaImage) -> RgbaImage {
    let mut o = img.clone();
    for p in o.pixels_mut() {
        if p[0] == 255 || p[1] == 255 || p[2] == 255 {
            *p = Rgba([255, 0, 0, 255]);
        } else if p[0] == 0 && p[1] == 0 && p[2] == 0 {
            *p = Rgba([0, 80, 255, 255]);
        }
    }
    o
}

/// Automatic exposure and white balance estimate (gray-world, median-exposure). Deterministic,
/// local, no model. Returns suggested adjustments the user can accept or refine.
pub fn auto_adjust(img: &RgbaImage) -> Adjustments {
    let h = histogram(img);
    let total: u64 = h.luma.iter().map(|&v| u64::from(v)).sum::<u64>().max(1);
    let mut acc = 0u64;
    let median = h.luma.iter().position(|&v| { acc += u64::from(v); acc * 2 >= total }).unwrap_or(128) as f32 / 255.;
    let ev = if median > 0.01 { (srgb_to_linear(0.46) / srgb_to_linear(median)).log2().clamp(-2., 2.) } else { 0. };
    let mean = |c: &Vec<u32>| c.iter().enumerate().map(|(i, &n)| i as f64 * f64::from(n)).sum::<f64>() / total as f64;
    let (r, g, b) = (mean(&h.red), mean(&h.green), mean(&h.blue));
    let temperature = (((b - r) / 255.) * 1.5).clamp(-0.6, 0.6) as f32;
    let tint = (((r + b) / 2. - g) / 255. * 1.5).clamp(-0.6, 0.6) as f32;
    Adjustments { exposure: (ev * 100.).round() / 100., temperature, tint, ..Adjustments::default() }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ramp() -> RgbaImage {
        RgbaImage::from_fn(64, 32, |x, y| Rgba([(x * 4) as u8, (y * 8) as u8, ((x + y) * 3 % 256) as u8, 255]))
    }
    #[test]
    fn identity_is_lossless() {
        let img = ramp();
        assert_eq!(Adjustments::default().apply(&img).unwrap(), img);
        assert!(Adjustments::default().is_identity());
    }
    #[test]
    fn exposure_is_linear_light_and_monotone() {
        let img = RgbaImage::from_pixel(4, 4, Rgba([100, 100, 100, 255]));
        let up = Adjustments { exposure: 1., ..Default::default() }.apply(&img).unwrap();
        // +1 EV doubles linear light: sRGB 100 (lin 0.127) -> lin 0.254 -> sRGB ~138.
        assert!((i32::from(up.get_pixel(0, 0)[0]) - 138).abs() <= 1, "{}", up.get_pixel(0, 0)[0]);
        let down = Adjustments { exposure: -1., ..Default::default() }.apply(&img).unwrap();
        assert!(down.get_pixel(0, 0)[0] < 100);
    }
    #[test]
    fn controls_move_in_expected_direction() {
        let img = ramp();
        let mean = |i: &RgbaImage, c: usize| i.pixels().map(|p| f64::from(p[c])).sum::<f64>() / i.pixels().len() as f64;
        let warm = Adjustments { temperature: 0.5, ..Default::default() }.apply(&img).unwrap();
        assert!(mean(&warm, 0) > mean(&img, 0) && mean(&warm, 2) < mean(&img, 2));
        let mono = Adjustments { saturation: -1., ..Default::default() }.apply(&img).unwrap();
        assert!(mono.pixels().all(|p| p[0].abs_diff(p[1]) <= 1 && p[1].abs_diff(p[2]) <= 1));
        let bright = Adjustments { shadows: 1., ..Default::default() }.apply(&img).unwrap();
        assert!(mean(&bright, 1) > mean(&img, 1));
        let c = Adjustments { contrast: 0.5, ..Default::default() }.apply(&img).unwrap();
        assert!(c.get_pixel(0, 0)[1] <= img.get_pixel(0, 0)[1]);
    }
    #[test]
    fn curves_are_monotone_and_validated() {
        let lut = Curve::parametric(0.5, 0.2, -0.3, -0.5).lut();
        assert!(lut.windows(2).all(|w| w[1] >= w[0] - 1e-6));
        assert!((lut[0]).abs() < 1e-6 && (lut[1023] - 1.).abs() < 1e-6);
        let bad = Adjustments { curve_master: Curve { points: vec![(0.5, 0.), (0.2, 1.)] }, ..Default::default() };
        assert!(bad.apply(&ramp()).is_err());
        assert!(Adjustments { exposure: f32::NAN, ..Default::default() }.validate().is_err());
    }
    #[test]
    fn hsl_roundtrip_and_band_weights() {
        for c in [[0.2, 0.4, 0.9], [1., 0., 0.], [0.5, 0.5, 0.5]] {
            let back = hsl_to_rgb(rgb_to_hsl(c));
            assert!((0..3).all(|i| (back[i] - c[i]).abs() < 1e-4));
        }
        for h in [0., 15., 100., 200., 300., 359.] {
            let s: f32 = band_weights(h).iter().sum();
            assert!((s - 1.).abs() < 1e-4, "{h}: {s}");
        }
    }
    #[test]
    fn cube_parse_bake_roundtrip_and_hostile_files() {
        let adj = Adjustments { temperature: 0.3, contrast: 0.2, ..Default::default() };
        let lut = adj.bake_lut(17).unwrap();
        let parsed = Lut3d::parse_cube(&lut.to_cube("test")).unwrap();
        assert_eq!(parsed.size, 17);
        let img = ramp();
        let direct = adj.apply(&img).unwrap();
        let via = Adjustments { lut: Some(parsed), ..Default::default() }.apply(&img).unwrap();
        let max = direct.pixels().zip(via.pixels()).map(|(a, b)| (0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap()).max().unwrap();
        assert!(max <= 6, "baked LUT within interpolation error, got {max}");
        for bad in ["LUT_3D_SIZE 1000\n", "LUT_3D_SIZE 2\n0 0 0\n", "0 0 0\n", "LUT_3D_SIZE 2\n0 0 nan\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n0 0 0\n", "LUT_1D_SIZE 4\n"] {
            assert!(Lut3d::parse_cube(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn histogram_clipping_and_auto() {
        let mut img = RgbaImage::from_pixel(10, 10, Rgba([40, 40, 40, 255]));
        img.put_pixel(0, 0, Rgba([255, 255, 255, 255]));
        img.put_pixel(1, 0, Rgba([0, 0, 0, 255]));
        let h = histogram(&img);
        assert_eq!((h.pixels, h.clipped_highlights, h.clipped_shadows), (100, 1, 1));
        assert_eq!(clipping_overlay(&img).get_pixel(0, 0), &Rgba([255, 0, 0, 255]));
        let a = auto_adjust(&img);
        assert!(a.exposure > 0.5, "dark image gets positive exposure: {}", a.exposure);
        let blue = RgbaImage::from_pixel(8, 8, Rgba([90, 110, 170, 255]));
        assert!(auto_adjust(&blue).temperature > 0.);
    }
    #[test]
    fn presets_are_valid_and_parallel_matches_serial() {
        for (_, p) in presets() {
            p.validate().unwrap();
        }
        let img = ramp();
        let adj = Adjustments { vibrance: 0.4, hsl_hue: [0.5; 8], ..Default::default() };
        let a = adj.apply(&img).unwrap();
        let b = adj.apply(&img).unwrap();
        assert_eq!(a, b);
    }
}
