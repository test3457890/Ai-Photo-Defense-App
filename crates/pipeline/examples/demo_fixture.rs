//! Writes a synthetic demonstration photograph: the CC0 calibration fixture with a QR code
//! carrying a fake Wi-Fi credential. Used for screenshots and manual testing only.
use rxing::Writer;
fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let fixture = args
        .next()
        .unwrap_or("tests/fixtures/calibration.png".into());
    let out = args.next().unwrap_or("demo-with-qr.png".into());
    let mut img = photo_core::load(std::path::Path::new(&fixture))?
        .pixels
        .to_rgb8();
    let m = rxing::MultiFormatWriter.encode(
        "WIFI:T:WPA;S:ExampleNetwork;P:not-a-real-password;;",
        &rxing::BarcodeFormat::QR_CODE,
        220,
        220,
    )?;
    let (x0, y0) = (
        img.width().saturating_sub(260),
        img.height().saturating_sub(260),
    );
    for y in 0..m.getHeight().min(img.height()) {
        for x in 0..m.getWidth().min(img.width()) {
            img.put_pixel(
                x + x0,
                y + y0,
                if m.get(x, y) {
                    image::Rgb([0; 3])
                } else {
                    image::Rgb([255; 3])
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
