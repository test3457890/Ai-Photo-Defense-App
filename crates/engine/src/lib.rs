//! Coordinated projected-gradient optimization. No bundled defense models or efficacy claims.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Settings {
    pub epsilon: f32,
    pub step_size: f32,
    pub steps: u32,
    pub fidelity_penalty: f32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            epsilon: 4. / 255.,
            step_size: 1. / 255.,
            steps: 40,
            fidelity_penalty: 0.05,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.epsilon.is_finite() && (0.0..=32. / 255.).contains(&self.epsilon),
            "Epsilon must be finite, 0..32/255"
        );
        ensure!(
            self.step_size.is_finite() && self.step_size > 0. && self.step_size <= self.epsilon,
            "Invalid step size"
        );
        ensure!((1..=1000).contains(&self.steps), "Steps must be 1..1000");
        ensure!(
            self.fidelity_penalty.is_finite() && (0.0..=1000.0).contains(&self.fidelity_penalty),
            "Invalid fidelity penalty"
        );
        Ok(())
    }
}
/// Implement an audited differentiable model objective. Loss is maximized.
/// Gradients must include preprocessing Jacobians and use the input's [0,1] coordinates.
pub trait Objective: Send + Sync {
    fn name(&self) -> &str;
    fn loss_gradient(&self, input: &[f32]) -> Result<(f32, Vec<f32>)>;
}
pub struct WeightedObjective<'a> {
    pub objective: &'a dyn Objective,
    pub weight: f32,
}
#[derive(Debug, Serialize)]
pub struct Optimization {
    pub pixels: Vec<f32>,
    pub trace: Vec<f32>,
    pub objective_names: Vec<String>,
    pub efficacy_verified: bool,
}
/// Each iteration combines normalized model gradients under ONE shared L-infinity budget.
/// This is not sequential noise stacking. The caller owns model suitability and validation.
pub fn optimize(
    original: &[f32],
    objectives: &[WeightedObjective<'_>],
    settings: Settings,
    cancel: &AtomicBool,
) -> Result<Optimization> {
    settings.validate()?;
    ensure!(
        !original.is_empty() && original.len() <= 24_000_000 * 4,
        "Invalid input length"
    );
    ensure!(
        original
            .iter()
            .all(|x| x.is_finite() && (0.0..=1.0).contains(x)),
        "Pixels must be finite [0,1]"
    );
    ensure!(
        !objectives.is_empty(),
        "No defense model installed; no protection was performed"
    );
    ensure!(
        objectives
            .iter()
            .all(|o| o.weight.is_finite() && o.weight > 0.),
        "Weights must be finite and positive"
    );
    let weight_sum: f32 = objectives.iter().map(|o| o.weight).sum();
    ensure!(weight_sum.is_finite(), "Weight sum overflow");
    let mut pixels = original.to_vec();
    let mut trace = Vec::new();
    for _ in 0..settings.steps {
        if cancel.load(Ordering::Relaxed) {
            bail!("Processing cancelled")
        }
        let mut gradient = vec![0.; pixels.len()];
        let mut loss = 0.;
        for o in objectives {
            if cancel.load(Ordering::Relaxed) {
                bail!("Processing cancelled")
            }
            let (value, g) = o.objective.loss_gradient(&pixels)?;
            ensure!(
                value.is_finite() && g.len() == pixels.len() && g.iter().all(|x| x.is_finite()),
                "Model returned invalid loss or gradient"
            );
            let norm = g.iter().map(|x| x.abs()).sum::<f32>() / g.len() as f32;
            ensure!(norm.is_finite(), "Gradient norm overflow");
            let weight = o.weight / weight_sum;
            if norm > 0. {
                for (dst, src) in gradient.iter_mut().zip(g) {
                    *dst += weight * src / norm;
                }
            }
            loss += weight * value;
        }
        for ((x, base), g) in pixels.iter_mut().zip(original).zip(gradient) {
            let g = g - settings.fidelity_penalty * 2. * (*x - base);
            let direction = if g > 0. {
                1.
            } else if g < 0. {
                -1.
            } else {
                0.
            };
            *x = (*x + settings.step_size * direction).clamp(
                (base - settings.epsilon).max(0.),
                (base + settings.epsilon).min(1.),
            );
        }
        trace.push(loss);
    }
    Ok(Optimization {
        pixels,
        trace,
        objective_names: objectives
            .iter()
            .map(|o| o.objective.name().to_string())
            .collect(),
        efficacy_verified: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    // Analytic objectives are TEST FIXTURES, never marketed as AI protection.
    struct Linear(f32);
    impl Objective for Linear {
        fn name(&self) -> &str {
            "test fixture"
        }
        fn loss_gradient(&self, x: &[f32]) -> Result<(f32, Vec<f32>)> {
            Ok((x.iter().sum::<f32>() * self.0, vec![self.0; x.len()]))
        }
    }
    #[test]
    fn joint_budget_and_conflict() {
        let p = Linear(1.);
        let n = Linear(-1.);
        let s = Settings::default();
        let x = vec![0.5, 0., 1.];
        let c = AtomicBool::new(false);
        let out = optimize(
            &x,
            &[WeightedObjective {
                objective: &p,
                weight: 1.,
            }],
            s,
            &c,
        )
        .unwrap();
        assert!(out.pixels[0] > 0.5);
        assert!(
            out.pixels
                .iter()
                .zip(&x)
                .all(|(a, b)| (a - b).abs() <= s.epsilon + 1e-7)
        );
        let out = optimize(
            &x,
            &[
                WeightedObjective {
                    objective: &p,
                    weight: 1.,
                },
                WeightedObjective {
                    objective: &n,
                    weight: 1.,
                },
            ],
            s,
            &c,
        )
        .unwrap();
        assert_eq!(out.pixels, x);
    }
    #[test]
    fn refuses_missing_model_cancel_and_nan() {
        let c = AtomicBool::new(false);
        assert!(optimize(&[0.5], &[], Settings::default(), &c).is_err());
        let p = Linear(1.);
        c.store(true, Ordering::Relaxed);
        assert!(
            optimize(
                &[0.5],
                &[WeightedObjective {
                    objective: &p,
                    weight: 1.
                }],
                Settings::default(),
                &c
            )
            .is_err()
        );
        assert!(
            Settings {
                epsilon: f32::NAN,
                ..Settings::default()
            }
            .validate()
            .is_err()
        );
    }
}

#[cfg(feature = "clip")]
pub mod clip;
