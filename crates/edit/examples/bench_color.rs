//! Times colour adjustments on synthetic images (release build recommended). Prints raw timings.
use photo_edit::color::Adjustments;
use std::time::Instant;
fn main() {
    for (w, h) in [(1600u32, 1067u32), (4000, 3000), (6000, 4000)] {
        let img = image::RgbaImage::from_fn(w, h, |x, y| {
            image::Rgba([(x % 256) as u8, (y % 256) as u8, ((x ^ y) % 256) as u8, 255])
        });
        let global = Adjustments {
            exposure: 0.5,
            contrast: 0.2,
            vibrance: 0.3,
            hsl_hue: [0.2; 8],
            temperature: 0.1,
            ..Default::default()
        };
        let detail = Adjustments {
            clarity: 0.4,
            sharpness: 0.5,
            noise_reduction: 0.3,
            ..global.clone()
        };
        for (name, a) in [
            ("per-pixel (exposure, contrast, HSL, WB)", &global),
            ("with clarity + sharpen + NR", &detail),
        ] {
            let mut times = vec![];
            for _ in 0..3 {
                let t = Instant::now();
                let _ = a.apply(&img).unwrap();
                times.push(t.elapsed().as_secs_f64() * 1000.);
            }
            times.sort_by(f64::total_cmp);
            println!(
                "{w}x{h} {name}: median {:.0} ms (runs {:?})",
                times[1],
                times.iter().map(|t| t.round()).collect::<Vec<_>>()
            );
        }
    }
}
