//! Binary selection masks. Hard edges are deliberate: secure redaction must cover every
//! selected pixel completely, never partially through anti-aliased coverage.
use crate::Rect;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_POLYGON_POINTS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask {
    width: u32,
    height: u32,
    bits: Vec<bool>,
}
impl Mask {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            bits: vec![false; width as usize * height as usize],
        }
    }
    pub fn full(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            bits: vec![true; width as usize * height as usize],
        }
    }
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub fn get(&self, x: u32, y: u32) -> bool {
        x < self.width && y < self.height && self.bits[(y * self.width + x) as usize]
    }
    pub fn set(&mut self, x: u32, y: u32, v: bool) {
        if x < self.width && y < self.height {
            self.bits[(y * self.width + x) as usize] = v;
        }
    }
    pub fn count(&self) -> usize {
        self.bits.iter().filter(|b| **b).count()
    }
    pub fn invert(&mut self) {
        self.bits.iter_mut().for_each(|b| *b = !*b);
    }
    pub fn union(&mut self, other: &Mask) {
        if other.dimensions() == self.dimensions() {
            self.bits
                .iter_mut()
                .zip(&other.bits)
                .for_each(|(a, b)| *a |= *b);
        }
    }
    pub fn subtract(&mut self, other: &Mask) {
        if other.dimensions() == self.dimensions() {
            self.bits
                .iter_mut()
                .zip(&other.bits)
                .for_each(|(a, b)| *a &= !*b);
        }
    }
    pub fn intersect(&mut self, other: &Mask) {
        if other.dimensions() == self.dimensions() {
            self.bits
                .iter_mut()
                .zip(&other.bits)
                .for_each(|(a, b)| *a &= *b);
        }
    }
    pub fn iter_set(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        let w = self.width;
        self.bits
            .iter()
            .enumerate()
            .filter(|(_, b)| **b)
            .map(move |(i, _)| ((i as u32) % w, (i as u32) / w))
    }
    pub fn bounds(&self) -> Option<Rect> {
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for (x, y) in self.iter_set() {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        (x0 != u32::MAX).then(|| Rect::new(x0, y0, x1 - x0 + 1, y1 - y0 + 1))
    }
    /// Mask refinement: grow (positive) or shrink (negative) by a square radius.
    /// Growing is recommended before secure redaction of detected text to absorb box error.
    pub fn refine(&self, radius: i32) -> Mask {
        if radius == 0 {
            return self.clone();
        }
        let grow = radius > 0;
        let r = radius.unsigned_abs().min(256);
        // Separable max/min filter: horizontal then vertical pass.
        let pass = |src: &Mask, horizontal: bool| {
            let mut out = Mask::new(src.width, src.height);
            for y in 0..src.height {
                for x in 0..src.width {
                    let mut v = !grow;
                    for d in 0..=2 * r {
                        let o = i64::from(d) - i64::from(r);
                        let (sx, sy) = if horizontal {
                            (i64::from(x) + o, i64::from(y))
                        } else {
                            (i64::from(x), i64::from(y) + o)
                        };
                        let inside = sx >= 0
                            && sy >= 0
                            && sx < i64::from(src.width)
                            && sy < i64::from(src.height);
                        let b = inside && src.get(sx as u32, sy as u32);
                        if grow && b {
                            v = true;
                            break;
                        }
                        if !grow && !b {
                            v = false;
                            break;
                        }
                    }
                    out.set(x, y, v);
                }
            }
            out
        };
        pass(&pass(self, true), false)
    }
}

/// A serializable selection in image coordinates, rasterized at render time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Selection {
    All,
    Rect(Rect),
    Ellipse(Rect),
    /// Closed polygon or freeform lasso (even-odd fill).
    Polygon(Vec<(f32, f32)>),
    /// Brush stroke: discs of `radius` swept along the polyline.
    Brush {
        points: Vec<(f32, f32)>,
        radius: f32,
    },
    /// Union of selections, e.g. all detections of a category.
    Union(Vec<Selection>),
    /// A selection grown or shrunk by `radius` pixels.
    Refined {
        inner: Box<Selection>,
        radius: i32,
    },
}
impl Selection {
    pub fn label(&self) -> String {
        match self {
            Self::All => "whole image".into(),
            Self::Rect(r) => format!("rectangle {}x{} at {},{}", r.width, r.height, r.x, r.y),
            Self::Ellipse(r) => format!("ellipse {}x{} at {},{}", r.width, r.height, r.x, r.y),
            Self::Polygon(p) => format!("polygon ({} points)", p.len()),
            Self::Brush { points, radius } => {
                format!("brush ({} points, radius {radius})", points.len())
            }
            Self::Union(v) => format!("{} regions", v.len()),
            Self::Refined { inner, radius } => format!("{} refined {radius:+} px", inner.label()),
        }
    }
    fn point_count(&self) -> usize {
        match self {
            Self::Polygon(p) | Self::Brush { points: p, .. } => p.len(),
            Self::Union(v) => v.iter().map(Self::point_count).sum::<usize>() + v.len(),
            Self::Refined { inner, .. } => inner.point_count() + 1,
            _ => 1,
        }
    }
    pub fn rasterize(&self, w: u32, h: u32) -> Result<Mask> {
        ensure!(
            self.point_count() <= MAX_POLYGON_POINTS,
            "Selection is too complex"
        );
        let mut m = Mask::new(w, h);
        match self {
            Self::All => m = Mask::full(w, h),
            Self::Rect(r) => {
                if let Some(r) = r.clamp_to(w, h) {
                    for y in r.y..r.y + r.height {
                        for x in r.x..r.x + r.width {
                            m.set(x, y, true);
                        }
                    }
                }
            }
            Self::Ellipse(r) => {
                let (cx, cy) = (
                    f64::from(r.x) + f64::from(r.width) / 2.,
                    f64::from(r.y) + f64::from(r.height) / 2.,
                );
                let (rx, ry) = (f64::from(r.width) / 2., f64::from(r.height) / 2.);
                if let Some(b) = r.clamp_to(w, h)
                    && rx > 0.
                    && ry > 0.
                {
                    for y in b.y..b.y + b.height {
                        for x in b.x..b.x + b.width {
                            let dx = (f64::from(x) + 0.5 - cx) / rx;
                            let dy = (f64::from(y) + 0.5 - cy) / ry;
                            if dx * dx + dy * dy <= 1. {
                                m.set(x, y, true);
                            }
                        }
                    }
                }
            }
            Self::Polygon(pts) => {
                ensure!(pts.len() >= 3, "Polygon needs at least three points");
                ensure!(
                    pts.iter().all(|(x, y)| x.is_finite() && y.is_finite()),
                    "Invalid polygon point"
                );
                for y in 0..h {
                    let sy = f64::from(y) + 0.5;
                    let mut xs = vec![];
                    for i in 0..pts.len() {
                        let (x1, y1) = (f64::from(pts[i].0), f64::from(pts[i].1));
                        let j = (i + 1) % pts.len();
                        let (x2, y2) = (f64::from(pts[j].0), f64::from(pts[j].1));
                        if (y1 <= sy) != (y2 <= sy) {
                            xs.push(x1 + (sy - y1) / (y2 - y1) * (x2 - x1));
                        }
                    }
                    xs.sort_by(|a, b| a.total_cmp(b));
                    for pair in xs.chunks(2) {
                        if let [a, b] = pair {
                            let start = (a - 0.5).ceil().max(0.) as u32;
                            let end = ((b - 0.5).floor().min(f64::from(w) - 1.)).max(-1.);
                            if end >= 0. {
                                for x in start..=(end as u32) {
                                    m.set(x, y, true);
                                }
                            }
                        }
                    }
                }
            }
            Self::Brush { points, radius } => {
                ensure!(
                    radius.is_finite() && (0.5..=2048.).contains(radius),
                    "Brush radius must be 0.5..2048"
                );
                ensure!(!points.is_empty(), "Brush stroke has no points");
                ensure!(
                    points.iter().all(|(x, y)| x.is_finite() && y.is_finite()),
                    "Invalid brush point"
                );
                let r = f64::from(*radius);
                let segments: Vec<_> = if points.len() == 1 {
                    vec![(points[0], points[0])]
                } else {
                    points.windows(2).map(|p| (p[0], p[1])).collect()
                };
                for (a, b) in segments {
                    let (ax, ay, bx, by) = (
                        f64::from(a.0),
                        f64::from(a.1),
                        f64::from(b.0),
                        f64::from(b.1),
                    );
                    let x0 = (ax.min(bx) - r).floor().max(0.) as u32;
                    let y0 = (ay.min(by) - r).floor().max(0.) as u32;
                    let x1 = ((ax.max(bx) + r).ceil().min(f64::from(w))) as u32;
                    let y1 = ((ay.max(by) + r).ceil().min(f64::from(h))) as u32;
                    let (vx, vy) = (bx - ax, by - ay);
                    let len2 = vx * vx + vy * vy;
                    for y in y0..y1 {
                        for x in x0..x1 {
                            let (px, py) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
                            let t = if len2 > 0. {
                                (((px - ax) * vx + (py - ay) * vy) / len2).clamp(0., 1.)
                            } else {
                                0.
                            };
                            let (dx, dy) = (px - (ax + t * vx), py - (ay + t * vy));
                            if dx * dx + dy * dy <= r * r {
                                m.set(x, y, true);
                            }
                        }
                    }
                }
            }
            Self::Union(items) => {
                for s in items {
                    m.union(&s.rasterize(w, h)?);
                }
            }
            Self::Refined { inner, radius } => {
                ensure!(
                    radius.abs() <= 256,
                    "Refinement radius must be within 256 px"
                );
                m = inner.rasterize(w, h)?.refine(*radius);
            }
        }
        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shapes_and_set_operations() {
        let r = Selection::Rect(Rect::new(2, 2, 4, 4))
            .rasterize(10, 10)
            .unwrap();
        assert_eq!(r.count(), 16);
        assert_eq!(r.bounds(), Some(Rect::new(2, 2, 4, 4)));
        let grown = r.refine(1);
        assert_eq!(grown.count(), 36);
        assert_eq!(grown.refine(-1), r);
        let e = Selection::Ellipse(Rect::new(0, 0, 10, 10))
            .rasterize(10, 10)
            .unwrap();
        assert!(e.get(5, 5) && !e.get(0, 0));
        let tri = Selection::Polygon(vec![(0., 0.), (10., 0.), (0., 10.)])
            .rasterize(10, 10)
            .unwrap();
        assert!(tri.get(1, 1) && !tri.get(9, 9));
        let mut u = r.clone();
        u.subtract(&r);
        assert_eq!(u.count(), 0);
        let out = Selection::Rect(Rect::new(50, 50, 4, 4))
            .rasterize(10, 10)
            .unwrap();
        assert_eq!(out.count(), 0);
        let mut i = Mask::full(10, 10);
        i.intersect(&tri);
        assert_eq!(i, tri);
    }
    #[test]
    fn rejects_hostile_selections() {
        assert!(
            Selection::Polygon(vec![(0., 0.); 2])
                .rasterize(5, 5)
                .is_err()
        );
        assert!(
            Selection::Polygon(vec![(f32::NAN, 0.); 3])
                .rasterize(5, 5)
                .is_err()
        );
        assert!(
            Selection::Brush {
                points: vec![(0., 0.)],
                radius: f32::INFINITY
            }
            .rasterize(5, 5)
            .is_err()
        );
        assert!(
            Selection::Polygon(vec![(0., 0.); MAX_POLYGON_POINTS + 1])
                .rasterize(5, 5)
                .is_err()
        );
    }
}
