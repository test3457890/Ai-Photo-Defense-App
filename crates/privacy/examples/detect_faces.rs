//! Prints face detections for each image: `cargo run -p photo-privacy --features faces --example
//! detect_faces -- MODEL.onnx IMAGE...`. Used to check the detector on real photographs.
fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let model = args
        .next()
        .expect("usage: detect_faces MODEL.onnx IMAGE...");
    let det = photo_privacy::faces::FaceDetector::load(std::path::Path::new(&model))?;
    for p in args {
        let img = image::open(&p)?;
        let t = std::time::Instant::now();
        let boxes = det.detect(&img, 0.7, 0.35)?;
        println!(
            "{p}: {}x{} -> {} face(s) in {} ms",
            img.width(),
            img.height(),
            boxes.len(),
            t.elapsed().as_millis()
        );
        for b in boxes {
            println!("  {b:?}");
        }
    }
    Ok(())
}
