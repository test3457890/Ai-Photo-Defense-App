//! Manual research-only smoke test. No weights are distributed with this project.
//! Independently verify a local safetensors file before passing it to this harness.
#[cfg(feature = "clip")]
fn main() -> anyhow::Result<()> {
    use photo_engine::{Objective, Settings, WeightedObjective};
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("Pass a local CLIP safetensors file"))?;
    let bytes = std::fs::read(path)?;
    // This harness tests architecture compatibility, not license approval or efficacy.
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
    println!(
        "after_two_steps={}",
        model.feature_energy(&optimized.pixels)?
    );
    Ok(())
}
#[cfg(not(feature = "clip"))]
fn main() {
    eprintln!("Build with --features clip");
}
