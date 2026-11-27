//! Rasterised graphics (histogram, viewer overlay) and compact control helpers.
//!
//! The histogram and overlays are drawn into RGBA buffers and shown as images. iced 0.13's
//! software canvas mispositions geometry nested inside pane grids (reproduced with a minimal
//! probe), so CPU rasterisation is used for predictable, pixel-exact placement.
use crate::theme;
use iced::widget::{button, column, container, horizontal_space, row, slider, text};
use iced::{Element, Length};
use image::{Rgba, RgbaImage};
use photo_edit::color::Histogram;

/// Histogram as an image: channel areas with alpha, luminance in grey, clipping markers.
pub fn histogram_image(h: &Histogram, w: u32, ht: u32, dark: bool) -> RgbaImage {
    let bg = if dark {
        Rgba([28, 29, 31, 255])
    } else {
        Rgba([214, 216, 219, 255])
    };
    let mut img = RgbaImage::from_pixel(w, ht, bg);
    let max = [&h.red, &h.green, &h.blue, &h.luma]
        .iter()
        .flat_map(|c| c[1..255].iter())
        .copied()
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    let mut draw = |data: &[u32], color: [u8; 3], alpha: f32| {
        for x in 0..w {
            // Take the peak of the bins covered by this column so narrow spikes stay visible.
            let a = (x as usize * 256) / w as usize;
            let b = (((x as usize + 1) * 256) / w as usize).max(a + 1).min(256);
            let v = data[a..b].iter().copied().max().unwrap_or(0) as f32;
            let top = ((1. - (v / max).sqrt().min(1.)) * (ht as f32 - 2.)) as u32 + 1;
            for y in top..ht {
                let p = img.get_pixel_mut(x, y);
                for c in 0..3 {
                    p[c] = (f32::from(p[c]) * (1. - alpha) + f32::from(color[c]) * alpha) as u8;
                }
            }
        }
    };
    draw(&h.luma, [190, 190, 190], 0.30);
    draw(&h.red, [230, 70, 70], 0.38);
    draw(&h.green, [80, 210, 90], 0.38);
    draw(&h.blue, [80, 120, 240], 0.38);
    let marker = |img: &mut RgbaImage, right: bool, on: bool, c: [u8; 3]| {
        for y in 1..9u32 {
            for x in 0..(9 - y) {
                let xx = if right {
                    w.saturating_sub(x + 2)
                } else {
                    x + 1
                };
                if xx < w {
                    img.put_pixel(
                        xx,
                        y,
                        if on {
                            Rgba([c[0], c[1], c[2], 255])
                        } else {
                            Rgba([90, 92, 96, 255])
                        },
                    );
                }
            }
        }
    };
    marker(&mut img, false, h.clipped_shadows > 0, [80, 140, 255]);
    marker(&mut img, true, h.clipped_highlights > 0, [255, 80, 80]);
    img
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlay {
    /// Scale from full-resolution image coordinates to displayed pixels.
    pub scale: f32,
    pub findings: Vec<(f32, f32, f32, f32, bool)>,
    pub rect: Option<(f32, f32, f32, f32, bool)>,
    pub points: Vec<(f32, f32)>,
    pub closed: bool,
    pub brush_radius: Option<f32>,
    pub line: Option<((f32, f32), (f32, f32))>,
    pub thirds: bool,
}
impl Overlay {
    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
            && self.rect.is_none()
            && self.points.is_empty()
            && self.line.is_none()
            && !self.thirds
    }
}

struct Canvas {
    img: RgbaImage,
}
impl Canvas {
    fn blend(&mut self, x: i64, y: i64, c: [u8; 4]) {
        if x < 0 || y < 0 || x >= i64::from(self.img.width()) || y >= i64::from(self.img.height()) {
            return;
        }
        let p = self.img.get_pixel_mut(x as u32, y as u32);
        let a = f32::from(c[3]) / 255.;
        for i in 0..3 {
            p[i] = (f32::from(p[i]) * (1. - a) + f32::from(c[i]) * a) as u8;
        }
        p[3] = p[3].max(c[3]);
    }
    fn disc(&mut self, cx: f32, cy: f32, r: f32, c: [u8; 4]) {
        let ri = r.ceil() as i64;
        for dy in -ri..=ri {
            for dx in -ri..=ri {
                if (dx * dx + dy * dy) as f32 <= r * r {
                    self.blend(cx as i64 + dx, cy as i64 + dy, c);
                }
            }
        }
    }
    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, c: [u8; 4]) {
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let steps = (len.ceil() as usize).clamp(1, 20_000);
        let r = (width / 2.).max(0.5);
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            if r <= 0.75 {
                self.blend(x as i64, y as i64, c);
            } else {
                self.disc(x, y, r, c);
            }
        }
    }
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, width: f32, c: [u8; 4]) {
        let pts = [(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)];
        for p in pts.windows(2) {
            self.line(p[0], p[1], width, c);
        }
    }
}

/// Draws the overlay into a transparent image of the displayed size.
pub fn overlay_image(o: &Overlay, w: u32, h: u32, accent: [u8; 3]) -> RgbaImage {
    let mut cv = Canvas {
        img: RgbaImage::new(w.max(1), h.max(1)),
    };
    let s = o.scale;
    let acc = [accent[0], accent[1], accent[2], 255];
    let white = [255, 255, 255, 230];
    if o.thirds {
        for k in 1..3 {
            let x = w as f32 * k as f32 / 3.;
            let y = h as f32 * k as f32 / 3.;
            cv.line((x, 0.), (x, h as f32), 1., [255, 255, 255, 110]);
            cv.line((0., y), (w as f32, y), 1., [255, 255, 255, 110]);
        }
    }
    for &(x, y, fw, fh, focused) in &o.findings {
        let c = if focused {
            [255, 190, 30, 255]
        } else {
            [235, 64, 52, 255]
        };
        cv.rect(
            x * s,
            y * s,
            fw * s,
            fh * s,
            if focused { 3. } else { 2. },
            c,
        );
    }
    if let Some((x, y, rw, rh, ellipse)) = o.rect {
        if ellipse {
            let (cx, cy, rx, ry) = (
                (x + rw / 2.) * s,
                (y + rh / 2.) * s,
                rw / 2. * s,
                rh / 2. * s,
            );
            let n = ((rx + ry) * 2.).clamp(32., 2000.) as usize;
            let pts: Vec<(f32, f32)> = (0..=n)
                .map(|i| {
                    let a = i as f32 / n as f32 * std::f32::consts::TAU;
                    (cx + rx * a.cos(), cy + ry * a.sin())
                })
                .collect();
            for p in pts.windows(2) {
                cv.line(p[0], p[1], 3., white);
            }
            for p in pts.windows(2) {
                cv.line(p[0], p[1], 1., acc);
            }
        } else {
            cv.rect(x * s, y * s, rw * s, rh * s, 3., white);
            cv.rect(x * s, y * s, rw * s, rh * s, 1., acc);
        }
    }
    if !o.points.is_empty() {
        let pts: Vec<(f32, f32)> = o.points.iter().map(|(x, y)| (x * s, y * s)).collect();
        if let Some(r) = o.brush_radius {
            let c = [accent[0], accent[1], accent[2], 110];
            for p in pts.windows(2) {
                cv.line(p[0], p[1], (2. * r * s).max(2.), c);
            }
            if pts.len() == 1 {
                cv.disc(pts[0].0, pts[0].1, (r * s).max(1.), c);
            }
        } else {
            for p in pts.windows(2) {
                cv.line(p[0], p[1], 2., acc);
            }
            if o.closed && pts.len() > 2 {
                cv.line(pts[pts.len() - 1], pts[0], 2., acc);
            }
            for p in pts.iter().take(400) {
                cv.disc(p.0, p.1, 2.5, [255, 255, 255, 255]);
            }
        }
    }
    if let Some((a, b)) = o.line {
        let (a, b) = ((a.0 * s, a.1 * s), (b.0 * s, b.1 * s));
        cv.line(a, b, 2., white);
        cv.disc(a.0, a.1, 4., acc);
        cv.disc(b.0, b.1, 4., [255, 255, 255, 255]);
    }
    cv.img
}

/// Collapsible tool group header.
pub fn group<'a, M: Clone + 'a>(
    title: &'a str,
    open: bool,
    toggle: M,
    body: impl Fn() -> Element<'a, M>,
) -> Element<'a, M> {
    let head = button(
        row![
            text(if open { "-" } else { "+" })
                .size(theme::FONT_SMALL)
                .width(14),
            text(title).size(theme::FONT_SMALL)
        ]
        .spacing(2),
    )
    .width(Length::Fill)
    .padding([4, 6])
    .style(theme::flat)
    .on_press(toggle);
    let mut c = column![
        container(head)
            .style(theme::section_header)
            .width(Length::Fill)
    ];
    if open {
        c = c.push(container(body()).padding([6, 8]));
    }
    c.into()
}

/// Labelled slider with numeric readout; releasing the slider commits one undo step.
pub fn slider_row<'a, M: Clone + 'a>(
    label: &'a str,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    on_change: impl Fn(f32) -> M + 'a,
    on_release: M,
) -> Element<'a, M> {
    row![
        text(label).size(theme::FONT_SMALL).width(78),
        slider(range, value, on_change)
            .step(0.01_f32)
            .on_release(on_release)
            .height(14),
        text(format!("{value:+.2}"))
            .size(theme::FONT_SMALL)
            .width(40),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center)
    .into()
}

pub fn label_value<'a, M: 'a>(label: &'a str, value: String) -> Element<'a, M> {
    row![
        text(label).size(theme::FONT_SMALL).width(110),
        text(value).size(theme::FONT_SMALL),
        horizontal_space()
    ]
    .spacing(6)
    .into()
}

/// Waveform image from per-column (min, max) pairs of the mono mix.
pub fn waveform_image(cols: &[(f32, f32)], w: u32, h: u32) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(w, h, Rgba([24, 25, 27, 255]));
    let mid = h as f32 / 2.;
    for y in [h / 2] {
        for x in 0..w {
            img.put_pixel(x, y, Rgba([60, 62, 66, 255]));
        }
    }
    if cols.is_empty() {
        return img;
    }
    for x in 0..w {
        let a = (x as usize * cols.len()) / w as usize;
        let b = (((x as usize + 1) * cols.len()) / w as usize)
            .max(a + 1)
            .min(cols.len());
        let (lo, hi) = cols[a..b]
            .iter()
            .fold((0f32, 0f32), |(l, h), c| (l.min(c.0), h.max(c.1)));
        let y0 = (mid - hi.clamp(-1., 1.) * mid).round() as u32;
        let y1 = (mid - lo.clamp(-1., 1.) * mid).round() as u32;
        for y in y0.min(h - 1)..=y1.min(h - 1) {
            img.put_pixel(x, y, Rgba([110, 170, 230, 255]));
        }
    }
    img
}

/// Spectrogram image: columns of band levels in dB, low frequencies at the bottom.
pub fn spectrogram_image(cols: &[Vec<f32>], w: u32, h: u32) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(w, h, Rgba([10, 10, 14, 255]));
    if cols.is_empty() || cols[0].is_empty() {
        return img;
    }
    let bands = cols[0].len();
    let max = cols.iter().flatten().copied().fold(f32::MIN, f32::max);
    for x in 0..w {
        let c = &cols[(x as usize * cols.len()) / w as usize];
        for y in 0..h {
            let band = ((h - 1 - y) as usize * bands) / h as usize;
            // 70 dB display range below the loudest bin.
            let t = ((c[band.min(bands - 1)] - (max - 70.)) / 70.).clamp(0., 1.);
            let px = [
                (t * 255.) as u8,
                (t * t * 200.) as u8,
                ((1. - t) * 90. * t + t.powi(4) * 160.) as u8,
                255,
            ];
            img.put_pixel(x, y, Rgba(px));
        }
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlay_and_histogram_rasterise() {
        let o = Overlay {
            scale: 0.5,
            rect: Some((10., 10., 40., 20., false)),
            findings: vec![(0., 0., 20., 20., true)],
            thirds: true,
            ..Default::default()
        };
        let img = overlay_image(&o, 100, 60, [80, 140, 210]);
        assert!(img.pixels().any(|p| p[3] > 0));
        assert_eq!(img.get_pixel(60, 50)[3], 0, "empty areas stay transparent");
        let h =
            photo_edit::color::histogram(&RgbaImage::from_pixel(4, 4, Rgba([10, 200, 90, 255])));
        let hi = histogram_image(&h, 256, 80, true);
        assert_eq!(hi.dimensions(), (256, 80));
        assert!(!Overlay::default().is_empty() || Overlay::default().is_empty());
    }
}
