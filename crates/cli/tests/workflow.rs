use image::{DynamicImage, Rgb, RgbImage};
use std::{path::Path, process::Command};
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_photo-defense"))
        .args(args)
        .output()
        .unwrap()
}
fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}
#[test]
fn export_report_original_preservation_and_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("source.png");
    let dest = dir.path().join("out");
    std::fs::create_dir(&dest).unwrap();
    let x = DynamicImage::ImageRgb8(RgbImage::from_fn(32, 32, |x, y| {
        Rgb([x as u8, y as u8, 128])
    }));
    let bytes = photo_core::encode(&x, photo_core::ExportFormat::Png, 95).unwrap();
    std::fs::write(&src, &bytes).unwrap();
    let args = ["export", path(&src), "--output-dir", path(&dest)];
    let result = cli(&args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(std::fs::read(&src).unwrap(), bytes);
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dest.join("source-private.png.json")).unwrap())
            .unwrap();
    assert_eq!(report["ai_efficacy"], "not evaluated");
    assert_eq!(report["quality"]["mse_rgb"], 0.0);
    let exported = std::fs::read(dest.join("source-private.png")).unwrap();
    assert!(!cli(&args).status.success());
    assert_eq!(
        std::fs::read(dest.join("source-private.png")).unwrap(),
        exported
    );
    assert!(!cli(&["protect", path(&src)]).status.success());
    let inspect = cli(&["inspect", path(&src)]);
    assert!(inspect.status.success());
    let j: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(j["width"], 32);
    let lab = dir.path().join("lab.json");
    assert!(
        cli(&[
            "lab",
            path(&src),
            path(&dest.join("source-private.png")),
            "--report",
            path(&lab)
        ])
        .status
        .success()
    );
    let j: serde_json::Value = serde_json::from_slice(&std::fs::read(lab).unwrap()).unwrap();
    assert_eq!(j["measurements"].as_array().unwrap().len(), 8);
    assert_eq!(j["ai_efficacy"], "not evaluated");
}
#[test]
fn batch_continues_after_bad_input() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("ok.png");
    let bad = dir.path().join("bad.jpg");
    let dest = dir.path().join("out");
    std::fs::create_dir(&dest).unwrap();
    std::fs::write(
        &src,
        photo_core::encode(
            &DynamicImage::new_rgb8(2, 2),
            photo_core::ExportFormat::Png,
            90,
        )
        .unwrap(),
    )
    .unwrap();
    std::fs::write(&bad, b"bad").unwrap();
    assert!(
        !cli(&[
            "export",
            path(&bad),
            path(&src),
            "--output-dir",
            path(&dest)
        ])
        .status
        .success()
    );
    assert!(dest.join("ok-private.png").exists());
}
