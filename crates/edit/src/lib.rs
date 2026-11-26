//! Non-destructive photo editing: an ordered, serializable operation stack rendered from the
//! untouched original. Geometry, selections/masks, visual concealment and secure redaction.
//!
//! Visual concealment (blur, pixelation, mosaic) can leave recoverable or inferable detail.
//! Secure redaction (opaque fills) replaces every selected pixel, forces it opaque and the
//! rendered output is always a single flattened raster.
use anyhow::{Result, bail, ensure};
use image::{DynamicImage, Rgba, RgbaImage, imageops::FilterType};
use serde::{Deserialize, Serialize};

pub mod color;
pub mod layers;
pub mod mask;
pub mod proxy;
pub use color::Adjustments;
pub use layers::{BlendMode, LayerMask};
pub use mask::{Mask, Selection};

/// Upper bound on stack length so a crafted recipe cannot cause unbounded work.
pub const MAX_OPERATIONS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl Rect {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
    /// Intersects with an image of the given size; None when empty.
    pub fn clamp_to(self, w: u32, h: u32) -> Option<Self> {
        let x2 = (u64::from(self.x) + u64::from(self.width)).min(u64::from(w)) as u32;
        let y2 = (u64::from(self.y) + u64::from(self.height)).min(u64::from(h)) as u32;
        (self.x < x2 && self.y < y2).then(|| Self::new(self.x, self.y, x2 - self.x, y2 - self.y))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AspectRatio {
    Original,
    Square,
    R4x3,
    R3x2,
    R16x9,
    R9x16,
    R4x5,
    Custom { width: u32, height: u32 },
}
impl AspectRatio {
    pub const PRESETS: [Self; 7] = [
        Self::Original,
        Self::Square,
        Self::R4x3,
        Self::R3x2,
        Self::R16x9,
        Self::R9x16,
        Self::R4x5,
    ];
    pub fn ratio(self, w: u32, h: u32) -> Result<(u32, u32)> {
        Ok(match self {
            Self::Original => (w, h),
            Self::Square => (1, 1),
            Self::R4x3 => (4, 3),
            Self::R3x2 => (3, 2),
            Self::R16x9 => (16, 9),
            Self::R9x16 => (9, 16),
            Self::R4x5 => (4, 5),
            Self::Custom { width, height } => {
                ensure!(
                    width > 0 && height > 0,
                    "Aspect ratio terms must be positive"
                );
                (width, height)
            }
        })
    }
    /// Largest centered rectangle of this ratio inside a w x h image.
    pub fn centered_crop(self, w: u32, h: u32) -> Result<Rect> {
        let (rw, rh) = self.ratio(w, h)?;
        let (w64, h64, rw, rh) = (u64::from(w), u64::from(h), u64::from(rw), u64::from(rh));
        let (cw, ch) = if w64 * rh > h64 * rw {
            ((h64 * rw / rh).max(1), h64)
        } else {
            (w64, (w64 * rh / rw).max(1))
        };
        Ok(Rect::new(
            ((w64 - cw) / 2) as u32,
            ((h64 - ch) / 2) as u32,
            cw as u32,
            ch as u32,
        ))
    }
}
impl std::fmt::Display for AspectRatio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Original => write!(f, "Original"),
            Self::Square => write!(f, "1:1"),
            Self::R4x3 => write!(f, "4:3"),
            Self::R3x2 => write!(f, "3:2"),
            Self::R16x9 => write!(f, "16:9"),
            Self::R9x16 => write!(f, "9:16"),
            Self::R4x5 => write!(f, "4:5"),
            Self::Custom { width, height } => write!(f, "{width}:{height}"),
        }
    }
}

/// How selected pixels are hidden.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Concealment {
    /// Visual concealment only. Strong blur can still leak shapes, colors and large text.
    GaussianBlur { sigma: f32 },
    /// Visual concealment. Each block takes its top-left sample.
    Pixelate { block: u32 },
    /// Visual concealment. Each block takes the mean of its selected pixels.
    Mosaic { block: u32 },
    /// Secure redaction: opaque replacement of every selected pixel.
    Solid { rgb: [u8; 3] },
}
impl Concealment {
    pub const BLACK: Self = Self::Solid { rgb: [0, 0, 0] };
    pub const WHITE: Self = Self::Solid {
        rgb: [255, 255, 255],
    };
    pub fn is_secure_redaction(self) -> bool {
        matches!(self, Self::Solid { .. })
    }
    pub fn validate(self) -> Result<()> {
        match self {
            Self::GaussianBlur { sigma } => ensure!(
                sigma.is_finite() && (0.5..=200.).contains(&sigma),
                "Blur sigma must be 0.5..200"
            ),
            Self::Pixelate { block } | Self::Mosaic { block } => {
                ensure!((2..=512).contains(&block), "Block size must be 2..512")
            }
            Self::Solid { .. } => {}
        }
        Ok(())
    }
    pub fn label(self) -> String {
        match self {
            Self::GaussianBlur { sigma } => format!("Gaussian blur (sigma {sigma})"),
            Self::Pixelate { block } => format!("Pixelate ({block} px)"),
            Self::Mosaic { block } => format!("Mosaic ({block} px)"),
            Self::Solid { rgb } => format!(
                "Opaque redaction #{:02x}{:02x}{:02x}",
                rgb[0], rgb[1], rgb[2]
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Operation {
    Crop(Rect),
    CropAspect(AspectRatio),
    Resize {
        width: u32,
        height: u32,
    },
    /// Fit inside a box while keeping aspect ratio (never upscales).
    FitWithin {
        max_width: u32,
        max_height: u32,
    },
    Rotate90,
    Rotate180,
    Rotate270,
    /// Straighten by a small angle and crop to the largest inscribed rectangle.
    Straighten {
        degrees: f32,
    },
    FlipHorizontal,
    FlipVertical,
    Conceal {
        selection: Selection,
        method: Concealment,
        /// Conceal everything except the selection (background blur / concealment).
        #[serde(default)]
        invert: bool,
    },
    /// Adjustment layer: global when `mask` is None.
    Adjust {
        #[serde(default)]
        name: String,
        adjustments: Box<Adjustments>,
        #[serde(default)]
        mask: Option<LayerMask>,
        #[serde(default)]
        invert: bool,
        #[serde(default = "one")]
        opacity: f32,
        #[serde(default)]
        blend: BlendMode,
        #[serde(default = "yes")]
        visible: bool,
    },
    /// Shape layer: colour fill of a selection. Opaque fills are not treated as redaction
    /// unless created as a Conceal operation.
    Fill { selection: Selection, rgba: [u8; 4] },
    /// Maps the quadrilateral TL, TR, BR, BL onto an upright rectangle.
    Perspective { corners: [(f32, f32); 4], width: u32, height: u32 },
    /// One-parameter radial distortion correction (not a lens profile).
    LensDistortion { k1: f32 },
    /// Centred canvas resize with an opaque fill colour.
    Canvas { width: u32, height: u32, fill: [u8; 3] },
}
fn one() -> f32 {
    1.
}
fn yes() -> bool {
    true
}
impl Operation {
    pub fn label(&self) -> String {
        match self {
            Self::Crop(r) => format!("Crop {}x{} at {},{}", r.width, r.height, r.x, r.y),
            Self::CropAspect(a) => format!("Crop to {a}"),
            Self::Resize { width, height } => format!("Resize to {width}x{height}"),
            Self::FitWithin {
                max_width,
                max_height,
            } => format!("Fit within {max_width}x{max_height}"),
            Self::Rotate90 => "Rotate 90 clockwise".into(),
            Self::Rotate180 => "Rotate 180".into(),
            Self::Rotate270 => "Rotate 90 counter-clockwise".into(),
            Self::Straighten { degrees } => format!("Straighten {degrees:.2} degrees"),
            Self::FlipHorizontal => "Flip horizontal".into(),
            Self::FlipVertical => "Flip vertical".into(),
            Self::Conceal {
                selection,
                method,
                invert,
            } => format!(
                "{} {}{}",
                method.label(),
                selection.label(),
                if *invert {
                    " (inverted: background)"
                } else {
                    ""
                }
            ),
            Self::Adjust { name, mask, visible, opacity, blend, .. } => format!(
                "{}{}{}{}{}",
                if name.is_empty() { "Adjustment layer" } else { name },
                mask.as_ref().map(|m| format!(" ({})", m.label())).unwrap_or_default(),
                if *opacity < 1. { format!(" {:.0}%", opacity * 100.) } else { String::new() },
                if *blend != BlendMode::Normal { format!(" {blend}") } else { String::new() },
                if *visible { "" } else { " [hidden]" }
            ),
            Self::Fill { selection, rgba } => format!("Shape fill #{:02x}{:02x}{:02x} {}", rgba[0], rgba[1], rgba[2], selection.label()),
            Self::Perspective { width, height, .. } => format!("Perspective correction to {width}x{height}"),
            Self::LensDistortion { k1 } => format!("Lens distortion {k1:+.3}"),
            Self::Canvas { width, height, .. } => format!("Canvas {width}x{height}"),
        }
    }
    /// Geometry operations change pixel coordinates; redaction regions cannot be carried across them.
    pub fn is_geometry(&self) -> bool {
        !matches!(self, Self::Conceal { .. } | Self::Adjust { .. } | Self::Fill { .. })
    }
    pub fn is_secure_redaction(&self) -> bool {
        matches!(self, Self::Conceal { method: Concealment::Solid { .. }, .. })
    }
    fn apply(&self, img: RgbaImage) -> Result<RgbaImage> {
        let (w, h) = img.dimensions();
        Ok(match self {
            Self::Crop(r) => {
                let r = r
                    .clamp_to(w, h)
                    .ok_or_else(|| anyhow::anyhow!("Crop outside image"))?;
                image::imageops::crop_imm(&img, r.x, r.y, r.width, r.height).to_image()
            }
            Self::CropAspect(a) => {
                let r = a.centered_crop(w, h)?;
                image::imageops::crop_imm(&img, r.x, r.y, r.width, r.height).to_image()
            }
            Self::Resize { width, height } => {
                check_dims(*width, *height)?;
                image::imageops::resize(&img, *width, *height, FilterType::Lanczos3)
            }
            Self::FitWithin {
                max_width,
                max_height,
            } => {
                check_dims(*max_width, *max_height)?;
                if w <= *max_width && h <= *max_height {
                    img
                } else {
                    let s = (f64::from(*max_width) / f64::from(w))
                        .min(f64::from(*max_height) / f64::from(h));
                    let nw = ((f64::from(w) * s).round() as u32).max(1);
                    let nh = ((f64::from(h) * s).round() as u32).max(1);
                    image::imageops::resize(&img, nw, nh, FilterType::Lanczos3)
                }
            }
            Self::Rotate90 => image::imageops::rotate90(&img),
            Self::Rotate180 => image::imageops::rotate180(&img),
            Self::Rotate270 => image::imageops::rotate270(&img),
            Self::Straighten { degrees } => straighten(&img, *degrees)?,
            Self::FlipHorizontal => image::imageops::flip_horizontal(&img),
            Self::FlipVertical => image::imageops::flip_vertical(&img),
            Self::Conceal {
                selection,
                method,
                invert,
            } => {
                method.validate()?;
                let mut m = selection.rasterize(w, h)?;
                if *invert {
                    m.invert();
                }
                let mut img = img;
                conceal(&mut img, &m, *method);
                img
            }
            Self::Adjust { adjustments, mask, invert, opacity, blend, visible, .. } => {
                if !*visible || adjustments.is_identity() {
                    img
                } else {
                    layers::apply_layer(img, adjustments, mask.as_ref(), *invert, *opacity, *blend)?
                }
            }
            Self::Fill { selection, rgba } => {
                let mut img = img;
                layers::fill(&mut img, selection, *rgba)?;
                img
            }
            Self::Perspective { corners, width, height } => layers::perspective(&img, *corners, *width, *height)?,
            Self::LensDistortion { k1 } => layers::lens_correct(&img, *k1)?,
            Self::Canvas { width, height, fill } => layers::canvas(&img, *width, *height, *fill)?,
        })
    }
}
fn check_dims(w: u32, h: u32) -> Result<()> {
    ensure!(
        w > 0 && h > 0 && u64::from(w) * u64::from(h) <= photo_core::MAX_PIXELS,
        "Target dimensions must be non-zero and at most 24 megapixels"
    );
    Ok(())
}

/// Rotates about the centre with bilinear sampling, then crops to the largest axis-aligned
/// rectangle that contains no undefined (outside-the-photo) pixels.
fn straighten(img: &RgbaImage, degrees: f32) -> Result<RgbaImage> {
    ensure!(
        degrees.is_finite() && degrees.abs() <= 45.,
        "Straighten angle must be within +/-45 degrees"
    );
    if degrees == 0. {
        return Ok(img.clone());
    }
    let (w, h) = img.dimensions();
    let a = f64::from(degrees).to_radians();
    let (cw, ch) = inscribed(f64::from(w), f64::from(h), a.abs());
    let (ow, oh) = ((cw.floor() as u32).max(1), (ch.floor() as u32).max(1));
    let (cos, sin) = (a.cos(), a.sin());
    let (cx, cy) = (f64::from(w) / 2., f64::from(h) / 2.);
    let mut out = RgbaImage::new(ow, oh);
    for (x, y, px) in out.enumerate_pixels_mut() {
        let dx = f64::from(x) + 0.5 - f64::from(ow) / 2.;
        let dy = f64::from(y) + 0.5 - f64::from(oh) / 2.;
        // Inverse rotation maps an output pixel back into the source.
        let sx = cos * dx + sin * dy + cx - 0.5;
        let sy = -sin * dx + cos * dy + cy - 0.5;
        *px = bilinear(img, sx, sy);
    }
    Ok(out)
}
fn inscribed(w: f64, h: f64, a: f64) -> (f64, f64) {
    let (long, short) = if w >= h { (w, h) } else { (h, w) };
    let (s, c) = (a.sin().abs(), a.cos().abs());
    let (wr, hr) = if short <= 2. * s * c * long || (s - c).abs() < 1e-10 {
        let x = 0.5 * short;
        if w >= h {
            (x / s, x / c)
        } else {
            (x / c, x / s)
        }
    } else {
        let cos2 = c * c - s * s;
        ((w * c - h * s) / cos2, (h * c - w * s) / cos2)
    };
    (wr.min(w), hr.min(h))
}
fn bilinear(img: &RgbaImage, x: f64, y: f64) -> Rgba<u8> {
    let (w, h) = img.dimensions();
    let x = x.clamp(0., f64::from(w - 1));
    let y = y.clamp(0., f64::from(h - 1));
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - f64::from(x0), y - f64::from(y0));
    let mut out = [0u8; 4];
    for (c, o) in out.iter_mut().enumerate() {
        let p = |xx, yy| f64::from(img.get_pixel(xx, yy)[c]);
        let top = p(x0, y0) * (1. - fx) + p(x1, y0) * fx;
        let bottom = p(x0, y1) * (1. - fx) + p(x1, y1) * fx;
        *o = (top * (1. - fy) + bottom * fy).round().clamp(0., 255.) as u8;
    }
    Rgba(out)
}

/// Applies a concealment method to every pixel where the mask is set.
pub fn conceal(img: &mut RgbaImage, mask: &Mask, method: Concealment) {
    let Some(bb) = mask.bounds() else { return };
    match method {
        Concealment::Solid { rgb } => {
            for (x, y) in mask.iter_set() {
                img.put_pixel(x, y, Rgba([rgb[0], rgb[1], rgb[2], 255]));
            }
        }
        Concealment::GaussianBlur { sigma } => {
            // Blur a margin around the selection so edges sample real neighbours.
            let margin = (sigma * 3.).ceil() as u32 + 1;
            let x0 = bb.x.saturating_sub(margin);
            let y0 = bb.y.saturating_sub(margin);
            let r = Rect::new(x0, y0, bb.width + 2 * margin, bb.height + 2 * margin)
                .clamp_to(img.width(), img.height())
                .expect("bounds inside image");
            let region = image::imageops::crop_imm(img, r.x, r.y, r.width, r.height).to_image();
            let blurred = image::imageops::blur(&region, sigma);
            for (x, y) in mask.iter_set() {
                img.put_pixel(x, y, *blurred.get_pixel(x - r.x, y - r.y));
            }
        }
        Concealment::Pixelate { block } | Concealment::Mosaic { block } => {
            let mean = matches!(method, Concealment::Mosaic { .. });
            let (w, h) = img.dimensions();
            let mut by = bb.y / block * block;
            while by < bb.y + bb.height {
                let mut bx = bb.x / block * block;
                while bx < bb.x + bb.width {
                    let x2 = (bx + block).min(w);
                    let y2 = (by + block).min(h);
                    let mut sum = [0u64; 4];
                    let mut n = 0u64;
                    let mut first = None;
                    for y in by..y2 {
                        for x in bx..x2 {
                            if mask.get(x, y) {
                                let p = img.get_pixel(x, y).0;
                                first.get_or_insert(p);
                                for c in 0..4 {
                                    sum[c] += u64::from(p[c]);
                                }
                                n += 1;
                            }
                        }
                    }
                    if n > 0 {
                        let fill = if mean {
                            sum.map(|s| (s / n) as u8)
                        } else {
                            first.expect("n>0")
                        };
                        for y in by..y2 {
                            for x in bx..x2 {
                                if mask.get(x, y) {
                                    img.put_pixel(x, y, Rgba(fill));
                                }
                            }
                        }
                    }
                    bx += block;
                }
                by += block;
            }
        }
    }
}

/// Final single-layer raster. Fully transparent pixels have their color channels zeroed so an
/// exported alpha channel cannot carry hidden original color data. Opaque results become RGB.
pub fn flatten(img: RgbaImage) -> DynamicImage {
    let mut img = img;
    let mut opaque = true;
    for p in img.pixels_mut() {
        if p[3] == 0 {
            *p = Rgba([0, 0, 0, 0]);
        }
        opaque &= p[3] == 255;
    }
    let d = DynamicImage::ImageRgba8(img);
    if opaque {
        DynamicImage::ImageRgb8(d.to_rgb8())
    } else {
        d
    }
}

fn validate_op(op: &Operation) -> Result<()> {
    match op {
        Operation::Conceal { method, .. } => method.validate(),
        Operation::Adjust { adjustments, opacity, .. } => {
            ensure!(opacity.is_finite() && (0.0..=1.0).contains(opacity), "Opacity must be 0..1");
            adjustments.validate()
        }
        _ => Ok(()),
    }
}

/// A secure-redaction mask and its fill color in final output coordinates.
pub type Redaction = (Mask, [u8; 3]);

/// Ordered non-destructive edits with undo/redo. The original is never modified.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EditStack {
    operations: Vec<Operation>,
    #[serde(default)]
    cursor: Option<usize>,
}
impl EditStack {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn from_operations(operations: Vec<Operation>) -> Result<Self> {
        ensure!(operations.len() <= MAX_OPERATIONS, "Too many operations");
        Ok(Self {
            cursor: Some(operations.len()),
            operations,
        })
    }
    fn active_len(&self) -> usize {
        self.cursor
            .unwrap_or(self.operations.len())
            .min(self.operations.len())
    }
    /// Pushing after an undo discards the redo branch, as in conventional editors.
    pub fn push(&mut self, op: Operation) -> Result<()> {
        let n = self.active_len();
        self.operations.truncate(n);
        ensure!(
            self.operations.len() < MAX_OPERATIONS,
            "Edit history is full"
        );
        validate_op(&op)?;
        self.operations.push(op);
        self.cursor = Some(self.operations.len());
        Ok(())
    }
    pub fn undo(&mut self) -> bool {
        let n = self.active_len();
        if n == 0 {
            return false;
        }
        self.cursor = Some(n - 1);
        true
    }
    pub fn redo(&mut self) -> bool {
        let n = self.active_len();
        if n >= self.operations.len() {
            return false;
        }
        self.cursor = Some(n + 1);
        true
    }
    pub fn can_undo(&self) -> bool {
        self.active_len() > 0
    }
    pub fn can_redo(&self) -> bool {
        self.active_len() < self.operations.len()
    }
    pub fn active(&self) -> &[Operation] {
        &self.operations[..self.active_len()]
    }
    pub fn history(&self) -> Vec<(String, bool)> {
        let n = self.active_len();
        self.operations
            .iter()
            .enumerate()
            .map(|(i, o)| (o.label(), i < n))
            .collect()
    }
    pub fn is_empty(&self) -> bool {
        self.active_len() == 0
    }
    pub fn has_visual_only_concealment(&self) -> bool {
        self.active().iter().any(
            |o| matches!(o, Operation::Conceal { method, .. } if !method.is_secure_redaction()),
        )
    }
    /// Render order: within each run of operations between geometry edits, adjustment and
    /// shape layers render first and secure redactions last, so a redaction layer always sits
    /// on top and no later adjustment can alter its fill.
    pub fn ordered(&self) -> Vec<&Operation> {
        let mut out = Vec::with_capacity(self.active().len());
        let mut pending = vec![];
        for op in self.active() {
            if op.is_geometry() {
                out.append(&mut pending);
                out.push(op);
            } else if op.is_secure_redaction() {
                pending.push(op);
            } else {
                out.push(op);
            }
        }
        out.append(&mut pending);
        out
    }
    /// Layer editing: these modify the active stack and discard any redo branch.
    pub fn remove(&mut self, index: usize) -> bool {
        let n = self.active_len();
        if index >= n {
            return false;
        }
        self.operations.truncate(n);
        self.operations.remove(index);
        self.cursor = Some(self.operations.len());
        true
    }
    pub fn move_layer(&mut self, index: usize, up: bool) -> bool {
        let n = self.active_len();
        let j = if up { index.checked_sub(1) } else { Some(index + 1) };
        match j {
            Some(j) if index < n && j < n => {
                self.operations.truncate(n);
                self.operations.swap(index, j);
                self.cursor = Some(n);
                true
            }
            _ => false,
        }
    }
    pub fn toggle_visible(&mut self, index: usize) -> bool {
        let n = self.active_len();
        if let Some(Operation::Adjust { visible, .. }) = self.operations[..n].get_mut(index) {
            *visible = !*visible;
            self.operations.truncate(n);
            return true;
        }
        false
    }
    pub fn replace(&mut self, index: usize, op: Operation) -> Result<()> {
        let n = self.active_len();
        ensure!(index < n, "No such layer");
        validate_op(&op)?;
        self.operations.truncate(n);
        self.operations[index] = op;
        Ok(())
    }
    /// Renders active operations from the original and flattens the result.
    pub fn render(&self, original: &DynamicImage) -> Result<DynamicImage> {
        let mut img = original.to_rgba8();
        for op in self.ordered() {
            img = op.apply(img)?;
            if img.width() == 0 || img.height() == 0 {
                bail!("Edit produced an empty image");
            }
        }
        Ok(flatten(img))
    }
    /// Secure redaction regions in final output coordinates, for export verification.
    /// Only valid when no geometry operation follows a redaction (otherwise None).
    pub fn final_redactions(&self, original: &DynamicImage) -> Result<Option<Vec<Redaction>>> {
        let mut out = vec![];
        let mut img = original.to_rgba8();
        for op in self.ordered() {
            let (w, h) = img.dimensions();
            match op {
                Operation::Conceal {
                    selection,
                    method: Concealment::Solid { rgb },
                    invert,
                } => {
                    let mut m = selection.rasterize(w, h)?;
                    if *invert {
                        m.invert();
                    }
                    out.push((m, *rgb));
                }
                Operation::Conceal { .. } | Operation::Adjust { .. } | Operation::Fill { .. } => {}
                _ if !out.is_empty() => return Ok(None),
                _ => {}
            }
            img = op.apply(img)?;
        }
        Ok(Some(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn gradient(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([
                (x * 7 % 256) as u8,
                (y * 11 % 256) as u8,
                ((x + y) % 256) as u8,
            ])
        }))
    }
    #[test]
    fn redaction_layers_render_above_later_adjustments() {
        let img = gradient(32, 32);
        let mut s = EditStack::new();
        s.push(Operation::Conceal { selection: Selection::Rect(Rect::new(4, 4, 8, 8)), method: Concealment::BLACK, invert: false }).unwrap();
        s.push(Operation::Adjust { name: "lift".into(), adjustments: Box::new(Adjustments { exposure: 2., brightness: 0.8, ..Default::default() }), mask: None, invert: false, opacity: 1., blend: BlendMode::Normal, visible: true }).unwrap();
        let out = s.render(&img).unwrap().to_rgb8();
        assert_eq!(out.get_pixel(6, 6), &image::Rgb([0, 0, 0]), "redaction stays opaque black");
        assert_ne!(out.get_pixel(20, 20), img.to_rgb8().get_pixel(20, 20));
        assert_eq!(s.final_redactions(&img).unwrap().unwrap().len(), 1);
        assert!(s.toggle_visible(1));
        assert_eq!(s.render(&img).unwrap().to_rgb8().get_pixel(20, 20), img.to_rgb8().get_pixel(20, 20));
        assert!(s.move_layer(1, true) && s.remove(0));
        assert_eq!(s.active().len(), 1);
        assert!(s.push(Operation::Adjust { name: String::new(), adjustments: Box::default(), mask: None, invert: false, opacity: 3., blend: BlendMode::Normal, visible: true }).is_err());
    }
    #[test]
    fn aspect_crops() {
        assert_eq!(
            AspectRatio::Square.centered_crop(400, 300).unwrap(),
            Rect::new(50, 0, 300, 300)
        );
        assert_eq!(
            AspectRatio::R16x9.centered_crop(1600, 1600).unwrap(),
            Rect::new(0, 350, 1600, 900)
        );
        assert_eq!(
            AspectRatio::R9x16.centered_crop(900, 900).unwrap().width,
            506
        );
        assert_eq!(
            AspectRatio::Original.centered_crop(30, 20).unwrap(),
            Rect::new(0, 0, 30, 20)
        );
        assert!(
            AspectRatio::Custom {
                width: 0,
                height: 1
            }
            .centered_crop(10, 10)
            .is_err()
        );
    }
    #[test]
    fn geometry_pipeline_and_undo() {
        let img = gradient(40, 20);
        let mut s = EditStack::new();
        s.push(Operation::Rotate90).unwrap();
        assert_eq!(s.render(&img).unwrap().width(), 20);
        s.push(Operation::Crop(Rect::new(0, 0, 10, 10))).unwrap();
        assert_eq!(s.render(&img).unwrap().height(), 10);
        assert!(s.undo());
        assert_eq!(s.render(&img).unwrap().height(), 40);
        assert!(s.redo());
        assert!(!s.redo());
        s.undo();
        s.push(Operation::FlipHorizontal).unwrap();
        assert!(!s.can_redo(), "new edit discards redo branch");
        let flipped = s.render(&img).unwrap().to_rgb8();
        let rotated = image::imageops::rotate90(&img.to_rgb8());
        assert_eq!(flipped.get_pixel(0, 0), rotated.get_pixel(19, 0));
        assert_eq!(
            img.to_rgb8().get_pixel(0, 0),
            &image::Rgb([0, 0, 0]),
            "original untouched"
        );
    }
    #[test]
    fn straighten_has_no_undefined_corners() {
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            200,
            100,
            image::Rgb([200, 10, 10]),
        ));
        let mut s = EditStack::new();
        s.push(Operation::Straighten { degrees: 5. }).unwrap();
        let out = s.render(&img).unwrap().to_rgb8();
        assert!(out.width() < 200 && out.width() > 150);
        assert!(out.pixels().all(|p| p == &image::Rgb([200, 10, 10])));
        assert!(
            Operation::Straighten { degrees: 90. }
                .apply(img.to_rgba8())
                .is_err()
        );
    }
    #[test]
    fn secure_redaction_replaces_every_selected_pixel() {
        let img = gradient(64, 64);
        for sel in [
            Selection::Rect(Rect::new(5, 5, 20, 10)),
            Selection::Ellipse(Rect::new(10, 10, 30, 20)),
            Selection::Polygon(vec![(2., 2.), (60., 5.), (30., 50.)]),
            Selection::Brush {
                points: vec![(5., 40.), (50., 45.)],
                radius: 4.,
            },
        ] {
            let mut s = EditStack::new();
            s.push(Operation::Conceal {
                selection: sel.clone(),
                method: Concealment::BLACK,
                invert: false,
            })
            .unwrap();
            let out = s.render(&img).unwrap().to_rgba8();
            let m = sel.rasterize(64, 64).unwrap();
            assert!(m.count() > 0);
            for (x, y) in m.iter_set() {
                assert_eq!(out.get_pixel(x, y), &Rgba([0, 0, 0, 255]));
            }
            // Outside the mask is untouched.
            let orig = img.to_rgba8();
            for (x, y, p) in out.enumerate_pixels() {
                if !m.get(x, y) {
                    assert_eq!(p, orig.get_pixel(x, y));
                }
            }
            let regions = s.final_redactions(&img).unwrap().unwrap();
            assert_eq!(regions.len(), 1);
        }
    }
    #[test]
    fn visual_concealment_changes_pixels_and_is_flagged() {
        let img = gradient(64, 64);
        for method in [
            Concealment::GaussianBlur { sigma: 4. },
            Concealment::Pixelate { block: 8 },
            Concealment::Mosaic { block: 8 },
        ] {
            assert!(!method.is_secure_redaction());
            let mut s = EditStack::new();
            s.push(Operation::Conceal {
                selection: Selection::Rect(Rect::new(8, 8, 32, 32)),
                method,
                invert: false,
            })
            .unwrap();
            assert!(s.has_visual_only_concealment());
            let out = s.render(&img).unwrap().to_rgb8();
            assert_ne!(out.get_pixel(20, 20), img.to_rgb8().get_pixel(20, 20));
            assert_eq!(out.get_pixel(60, 60), img.to_rgb8().get_pixel(60, 60));
        }
        assert!(Concealment::Pixelate { block: 1 }.validate().is_err());
        assert!(
            Concealment::GaussianBlur { sigma: f32::NAN }
                .validate()
                .is_err()
        );
    }
    #[test]
    fn inverted_background_concealment() {
        let img = gradient(32, 32);
        let mut s = EditStack::new();
        s.push(Operation::Conceal {
            selection: Selection::Rect(Rect::new(8, 8, 8, 8)),
            method: Concealment::WHITE,
            invert: true,
        })
        .unwrap();
        let out = s.render(&img).unwrap().to_rgb8();
        assert_eq!(out.get_pixel(0, 0), &image::Rgb([255; 3]));
        assert_eq!(out.get_pixel(10, 10), img.to_rgb8().get_pixel(10, 10));
    }
    #[test]
    fn flatten_clears_hidden_color_under_transparency() {
        let mut img = RgbaImage::from_pixel(4, 4, Rgba([9, 8, 7, 255]));
        img.put_pixel(1, 1, Rgba([200, 100, 50, 0]));
        let out = flatten(img).to_rgba8();
        assert_eq!(out.get_pixel(1, 1), &Rgba([0, 0, 0, 0]));
        assert!(matches!(
            flatten(RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]))),
            DynamicImage::ImageRgb8(_)
        ));
    }
    #[test]
    fn recipe_roundtrip_and_limits() {
        let s = EditStack::from_operations(vec![
            Operation::CropAspect(AspectRatio::R4x5),
            Operation::FitWithin {
                max_width: 1080,
                max_height: 1350,
            },
        ])
        .unwrap();
        let json = serde_json::to_string(&s).unwrap();
        let back: EditStack = serde_json::from_str(&json).unwrap();
        assert_eq!(back.active().len(), 2);
        assert!(EditStack::from_operations(vec![Operation::Rotate90; MAX_OPERATIONS + 1]).is_err());
        assert!(
            Operation::Resize {
                width: 100_000,
                height: 100_000
            }
            .apply(RgbaImage::new(2, 2))
            .is_err()
        );
        let fit = s.render(&gradient(4000, 3000)).unwrap();
        assert!(fit.width() <= 1080 && fit.height() <= 1350);
    }
}
