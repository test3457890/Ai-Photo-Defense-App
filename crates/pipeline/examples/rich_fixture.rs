//! Generates a synthetic, photo-like test image (smooth gradients, texture and colour, plus a
//! QR code carrying a fake Wi-Fi credential). Original work, CC0. For screenshots and tests only.
use rxing::Writer;
fn main() -> anyhow::Result<()> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or("rich-synthetic.png".into());
    let (w, h) = (1600u32, 1067u32);
    let mut img = image::RgbImage::from_fn(w, h, |x, y| {
        let fx = x as f32 / w as f32;
        let fy = y as f32 / h as f32;
        // Deterministic hash noise for texture.
        let n = (((x.wrapping_mul(2654435761) ^ y.wrapping_mul(40503)) >> 7) % 23) as f32 - 11.;
        let (r, g, b) = if fy < 0.55 {
            // Sky: warm horizon to cool zenith.
            (90. + 140. * fy + 30. * fx, 130. + 90. * fy, 200. - 40. * fy)
        } else {
            // Field: green to olive with rows.
            let row = ((fy * 90.).sin() * 0.5 + 0.5) * 25.;
            (
                70. + 60. * fx + row,
                110. + 40. * (1. - fy) + row,
                40. + 20. * fx,
            )
        };
        let sun = (-(((fx - 0.72).powi(2) + (fy - 0.28).powi(2)) * 60.)).exp() * 120.;
        let c = |v: f32| (v + sun + n * 0.6).clamp(0., 255.) as u8;
        image::Rgb([c(r), c(g), c(b * 0.9)])
    });
    // A dark tree silhouette.
    for y in 380..800u32 {
        for x in 200..520u32 {
            let dx = (x as f32 - 360.) / 160.;
            let dy = (y as f32 - 520.) / 150.;
            if dx * dx + dy * dy < 1. || (y > 600 && (x as i32 - 360).abs() < 18) {
                img.put_pixel(x, y, image::Rgb([30, 52 + (y % 7) as u8, 28]));
            }
        }
    }
    let m = rxing::MultiFormatWriter.encode(
        "WIFI:T:WPA;S:ExampleNetwork;P:not-a-real-password;;",
        &rxing::BarcodeFormat::QR_CODE,
        260,
        260,
    )?;
    for y in 0..m.getHeight() {
        for x in 0..m.getWidth() {
            img.put_pixel(
                x + 1180,
                y + 700,
                if m.get(x, y) {
                    image::Rgb([10; 3])
                } else {
                    image::Rgb([245; 3])
                },
            );
        }
    }
    let bytes = photo_core::encode(
        &image::DynamicImage::ImageRgb8(img),
        photo_core::ExportFormat::Png,
        95,
    )?;
    photo_core::save_new(std::path::Path::new(&out), &bytes)?;
    println!("wrote {out}");
    Ok(())
}
