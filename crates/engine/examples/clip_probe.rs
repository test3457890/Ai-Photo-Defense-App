//! Manual research smoke test for a checksum-pinned CLIP checkpoint.
//! No weights are distributed. See docs/EXPERIMENTAL_CLIP.md for provenance and scope.
#[cfg(feature = "clip")]
fn main() -> anyhow::Result<()> {
    use photo_engine::{Objective, Settings, WeightedObjective};
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or_else(|| {
        anyhow::anyhow!("Pass the pinned CLIP safetensors file, then an optional new report path")
    })?;
    let report = args.next();
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(605157884 + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() == 605157884, "Unexpected checkpoint size");
    use std::fmt::Write as _;
    let mut hash = String::with_capacity(64);
    for byte in Sha256::digest(&bytes) {
        let _ = write!(hash, "{byte:02x}");
    }
    anyhow::ensure!(
        hash == "99d28a652e6ec46629ab7047a0ac82c69b1fe11e0ce672c43af65d3a9a3fc05d",
        "Unexpected checkpoint hash"
    );
    let model = photo_engine::clip::ClipObjective::from_bytes(bytes, 32, 32)?;
    let x: Vec<f32> = (0..32 * 32 * 3)
        .map(|i| 0.2 + (i % 255) as f32 / 500.)
        .collect();
    let before = model.feature_energy(&x)?;
    let (loss, g) = model.loss_gradient(&x)?;
    println!(
        "feature_energy={before}, loss={loss}, max_gradient={}",
        g.iter().map(|x| x.abs()).fold(0f32, f32::max)
    );
    let optimized = photo_engine::optimize(
        &x,
        &[WeightedObjective {
            objective: &model,
            weight: 1.,
        }],
        Settings {
            steps: 2,
            ..Settings::default()
        },
        &std::sync::atomic::AtomicBool::new(false),
    )?;
    let after = model.feature_energy(&optimized.pixels)?;
    println!("after_two_steps={after}");
    if let Some(path) = report {
        let result = serde_json::json!({"schema_version":1,"model_sha256":hash,"energy_before":before,"energy_after_two_steps":after,"loss":loss,"input_gradient":g,"optimized_pixels":optimized.pixels,"ai_efficacy":"not evaluated"});
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        out.write_all(&serde_json::to_vec_pretty(&result)?)?;
        out.sync_all()?;
    }
    Ok(())
}
#[cfg(not(feature = "clip"))]
fn main() {
    eprintln!("Build with --features clip");
}
