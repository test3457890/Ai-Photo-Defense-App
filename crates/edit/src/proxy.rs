//! Proxy rendering for interactive previews: every coordinate-bearing operation is rescaled so
//! the same edit stack can render a downsized copy quickly. Exports always render at full size.
use crate::{AspectRatio, Concealment, EditStack, LayerMask, Operation, Rect, Selection};

fn r(rect: Rect, s: f32) -> Rect {
    let f = |v: u32| (v as f32 * s).round() as u32;
    Rect::new(f(rect.x), f(rect.y), f(rect.width).max(1), f(rect.height).max(1))
}
fn p(pt: (f32, f32), s: f32) -> (f32, f32) {
    (pt.0 * s, pt.1 * s)
}
pub fn scale_selection(sel: &Selection, s: f32) -> Selection {
    match sel {
        Selection::All => Selection::All,
        Selection::Rect(x) => Selection::Rect(r(*x, s)),
        Selection::Ellipse(x) => Selection::Ellipse(r(*x, s)),
        Selection::Polygon(v) => Selection::Polygon(v.iter().map(|q| p(*q, s)).collect()),
        Selection::Brush { points, radius } => Selection::Brush { points: points.iter().map(|q| p(*q, s)).collect(), radius: (radius * s).max(0.5) },
        Selection::Union(v) => Selection::Union(v.iter().map(|x| scale_selection(x, s)).collect()),
        Selection::Refined { inner, radius } => Selection::Refined { inner: Box::new(scale_selection(inner, s)), radius: ((*radius as f32) * s).round() as i32 },
    }
}
fn scale_mask(m: &LayerMask, s: f32) -> LayerMask {
    match m {
        LayerMask::Selection(x) => LayerMask::Selection(scale_selection(x, s)),
        LayerMask::Linear { x0, y0, x1, y1 } => LayerMask::Linear { x0: x0 * s, y0: y0 * s, x1: x1 * s, y1: y1 * s },
        LayerMask::Radial { cx, cy, rx, ry, feather } => LayerMask::Radial { cx: cx * s, cy: cy * s, rx: (rx * s).max(0.5), ry: (ry * s).max(0.5), feather: *feather },
        other => other.clone(),
    }
}
impl Operation {
    pub fn scaled(&self, s: f32) -> Operation {
        let d = |v: u32| ((v as f32 * s).round() as u32).max(1);
        match self {
            Self::Crop(x) => Self::Crop(r(*x, s)),
            Self::CropAspect(AspectRatio::Original) => Self::CropAspect(AspectRatio::Original),
            Self::Resize { width, height } => Self::Resize { width: d(*width), height: d(*height) },
            Self::FitWithin { max_width, max_height } => Self::FitWithin { max_width: d(*max_width), max_height: d(*max_height) },
            Self::Conceal { selection, method, invert } => Self::Conceal {
                selection: scale_selection(selection, s),
                method: match *method {
                    Concealment::GaussianBlur { sigma } => Concealment::GaussianBlur { sigma: (sigma * s).max(0.5) },
                    Concealment::Pixelate { block } => Concealment::Pixelate { block: d(block).max(2) },
                    Concealment::Mosaic { block } => Concealment::Mosaic { block: d(block).max(2) },
                    m => m,
                },
                invert: *invert,
            },
            Self::Adjust { name, adjustments, mask, invert, opacity, blend, visible } => Self::Adjust {
                name: name.clone(),
                adjustments: adjustments.clone(),
                mask: mask.as_ref().map(|m| scale_mask(m, s)),
                invert: *invert,
                opacity: *opacity,
                blend: *blend,
                visible: *visible,
            },
            Self::Fill { selection, rgba } => Self::Fill { selection: scale_selection(selection, s), rgba: *rgba },
            Self::Perspective { corners, width, height } => Self::Perspective { corners: corners.map(|q| p(q, s)), width: d(*width), height: d(*height) },
            Self::Canvas { width, height, fill } => Self::Canvas { width: d(*width), height: d(*height), fill: *fill },
            other => other.clone(),
        }
    }
}
impl EditStack {
    /// The active operations rescaled by `s` (proxy = full x s), ready to render a preview.
    pub fn scaled(&self, s: f32) -> EditStack {
        EditStack::from_operations(self.active().iter().map(|o| o.scaled(s)).collect()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proxy_render_matches_downscaled_full_render() {
        let full = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(400, 300, |x, y| image::Rgb([(x / 2) as u8, (y / 2) as u8, 90])));
        let mut s = EditStack::new();
        s.push(Operation::Crop(Rect::new(40, 30, 320, 240))).unwrap();
        s.push(Operation::Conceal { selection: Selection::Rect(Rect::new(20, 20, 100, 60)), method: Concealment::BLACK, invert: false }).unwrap();
        let big = s.render(&full).unwrap();
        let small_src = full.resize_exact(100, 75, image::imageops::FilterType::Triangle);
        let small = s.scaled(0.25).render(&small_src).unwrap();
        assert_eq!((small.width(), small.height()), (80, 60));
        assert_eq!((big.width() / 4, big.height() / 4), (80, 60));
        assert_eq!(small.to_rgb8().get_pixel(10, 10), &image::Rgb([0, 0, 0]));
    }
}
