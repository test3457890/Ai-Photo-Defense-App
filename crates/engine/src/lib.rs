//! Coordinated projected-gradient optimization. No bundled defense models or efficacy claims.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

pub mod frequency;

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

/// Configuration for hardened multi-layer adversarial optimization (MI-FGSM + TI-FGSM +
/// adaptive perceptual masking + mid-frequency DCT projection).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HardenedSettings {
    pub base: Settings,
    /// Momentum decay factor `mu` in `[0.0, 0.99]` (MI-FGSM). 0.9 improves black-box transfer.
    pub momentum: f32,
    /// Translation-Invariant Gaussian kernel radius in pixels (`0` disables, `1` = 3x3, `2` = 5x5).
    pub ti_radius: u32,
    /// Weight perturbation budget by local edge/texture energy so flat backgrounds stay clean.
    pub adaptive_texture_mask: bool,
    /// Project perturbation delta onto compression-resistant mid-frequency DCT bands each step.
    pub frequency_projection: bool,
    /// Maximum DCT band `u + v` kept at full strength during frequency projection (typically 6).
    pub max_dct_band: usize,
}

impl Default for HardenedSettings {
    fn default() -> Self {
        Self {
            base: Settings {
                epsilon: 8. / 255.,
                step_size: 1.5 / 255.,
                steps: 40,
                fidelity_penalty: 0.03,
            },
            momentum: 0.9,
            ti_radius: 1,
            adaptive_texture_mask: true,
            frequency_projection: true,
            max_dct_band: 6,
        }
    }
}

impl HardenedSettings {
    pub fn validate(&self) -> Result<()> {
        self.base.validate()?;
        ensure!(
            self.momentum.is_finite() && (0.0..=0.99).contains(&self.momentum),
            "Momentum must be finite in [0.0, 0.99]"
        );
        ensure!(self.ti_radius <= 4, "TI kernel radius must be <= 4");
        ensure!(
            (2..=14).contains(&self.max_dct_band),
            "max_dct_band must be 2..=14"
        );
        Ok(())
    }
}

/// Builds a per-pixel `[0.35, 1.0]` perceptual masking map from local Sobel gradient magnitude.
/// Textured/edge regions receive full budget (`1.0`); smooth flat regions (sky, walls) receive
/// reduced budget (`0.35`) to preserve photographic quality.
pub fn compute_adaptive_texture_mask(rgb: &[f32], width: usize, height: usize) -> Vec<f32> {
    let n_px = width * height;
    if n_px == 0 || rgb.len() != n_px * 3 {
        return vec![1.0; n_px];
    }
    let mut luma = vec![0.0f32; n_px];
    for i in 0..n_px {
        luma[i] = 0.299 * rgb[i * 3] + 0.587 * rgb[i * 3 + 1] + 0.114 * rgb[i * 3 + 2];
    }
    let mut mask = vec![0.35f32; n_px];
    if width < 3 || height < 3 {
        return vec![1.0; n_px];
    }
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let idx = |yy: usize, xx: usize| luma[yy * width + xx];
            let gx = -idx(y - 1, x - 1) + idx(y - 1, x + 1) - 2.0 * idx(y, x - 1)
                + 2.0 * idx(y, x + 1)
                - idx(y + 1, x - 1)
                + idx(y + 1, x + 1);
            let gy = -idx(y - 1, x - 1) - 2.0 * idx(y - 1, x) - idx(y - 1, x + 1)
                + idx(y + 1, x - 1)
                + 2.0 * idx(y + 1, x)
                + idx(y + 1, x + 1);
            let mag = (gx * gx + gy * gy).sqrt();
            // Map edge magnitude to [0.35, 1.0]
            mask[y * width + x] = (0.35 + (mag * 3.0).min(0.65)).clamp(0.35, 1.0);
        }
    }
    mask
}

/// Builds a soft-feathered regional weight mask `[0.25, 1.0]` from `[x, y, w, h]` rectangles
/// (e.g., detected faces or license plates). Inside boxes = `1.0`, outside = `0.25`.
pub fn build_regional_mask(
    width: usize,
    height: usize,
    regions: &[[u32; 4]],
    feather_px: u32,
) -> Vec<f32> {
    let n_px = width * height;
    if regions.is_empty() || n_px == 0 {
        return vec![1.0; n_px];
    }
    let mut mask = vec![0.25f32; n_px];
    let feather = feather_px.max(1) as f32;
    for &[rx, ry, rw, rh] in regions {
        let x0 = rx.saturating_sub(feather_px) as usize;
        let y0 = ry.saturating_sub(feather_px) as usize;
        let x1 = ((rx + rw + feather_px) as usize).min(width);
        let y1 = ((ry + rh + feather_px) as usize).min(height);
        for y in y0..y1 {
            for x in x0..x1 {
                let dx = if (x as u32) < rx {
                    (rx - x as u32) as f32
                } else if (x as u32) >= rx + rw {
                    (x as u32 - (rx + rw) + 1) as f32
                } else {
                    0.0
                };
                let dy = if (y as u32) < ry {
                    (ry - y as u32) as f32
                } else if (y as u32) >= ry + rh {
                    (y as u32 - (ry + rh) + 1) as f32
                } else {
                    0.0
                };
                let dist = (dx * dx + dy * dy).sqrt();
                let w = if dist == 0.0 {
                    1.0
                } else {
                    (1.0 - dist / feather).clamp(0.25, 1.0)
                };
                let idx = y * width + x;
                mask[idx] = mask[idx].max(w);
            }
        }
    }
    mask
}

/// Applies separable box/Gaussian-approximated translation-invariant smoothing to a gradient.
fn smooth_gradient_ti(grad: &mut [f32], width: usize, height: usize, radius: u32) {
    if radius == 0 || width < 3 || height < 3 || grad.len() != width * height * 3 {
        return;
    }
    let r = radius as isize;
    let mut tmp = vec![0.0f32; grad.len()];
    // Horizontal pass with triangular weights
    for y in 0..height {
        for x in 0..width {
            for c in 0..3 {
                let mut sum = 0.0f32;
                let mut wsum = 0.0f32;
                for dx in -r..=r {
                    let xx = (x as isize + dx).clamp(0, width as isize - 1) as usize;
                    let w = (r + 1 - dx.abs()) as f32;
                    sum += grad[(y * width + xx) * 3 + c] * w;
                    wsum += w;
                }
                tmp[(y * width + x) * 3 + c] = sum / wsum;
            }
        }
    }
    // Vertical pass
    for y in 0..height {
        for x in 0..width {
            for c in 0..3 {
                let mut sum = 0.0f32;
                let mut wsum = 0.0f32;
                for dy in -r..=r {
                    let yy = (y as isize + dy).clamp(0, height as isize - 1) as usize;
                    let w = (r + 1 - dy.abs()) as f32;
                    sum += tmp[(yy * width + x) * 3 + c] * w;
                    wsum += w;
                }
                grad[(y * width + x) * 3 + c] = sum / wsum;
            }
        }
    }
}

/// Hardened multi-layer optimizer using MI-FGSM momentum, TI-FGSM gradient smoothing,
/// adaptive texture/regional masking, and optional mid-frequency DCT projection.
pub fn optimize_hardened(
    original: &[f32],
    width: usize,
    height: usize,
    objectives: &[WeightedObjective<'_>],
    settings: HardenedSettings,
    regional_mask: Option<&[f32]>,
    cancel: &AtomicBool,
) -> Result<Optimization> {
    settings.validate()?;
    ensure!(
        width > 0 && height > 0 && original.len() == width * height * 3,
        "Hardened optimizer requires RGB f32 input matching width * height * 3"
    );
    ensure!(
        original.len() <= 24_000_000 * 3,
        "Input exceeds 24 megapixels"
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
    if let Some(rm) = regional_mask {
        ensure!(
            rm.len() == width * height
                && rm.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
            "Regional mask length or values invalid"
        );
    }

    let tex_mask = if settings.adaptive_texture_mask {
        compute_adaptive_texture_mask(original, width, height)
    } else {
        vec![1.0f32; width * height]
    };
    let combined_mask: Vec<f32> = match regional_mask {
        Some(rm) => tex_mask
            .iter()
            .zip(rm.iter())
            .map(|(&t, &r)| (t * 0.4 + r * 0.6).clamp(0.15, 1.0))
            .collect(),
        None => tex_mask,
    };

    let weight_sum: f32 = objectives.iter().map(|o| o.weight).sum();
    ensure!(weight_sum.is_finite(), "Weight sum overflow");

    let mut pixels = original.to_vec();
    // Standard PGD initial offset breaks stationary points at x == original
    // (e.g. cosine-distance or L2-distance objectives where grad == 0 at theta == 0).
    // Use 2x2 spatial blocks so mid-frequency DCT projection preserves the initial seed.
    let init_delta = (settings.base.step_size * 0.5).min(settings.base.epsilon * 0.25);
    for (i, p) in pixels.iter_mut().enumerate() {
        let px_idx = i / 3;
        let px_x = px_idx % width;
        let px_y = px_idx / width;
        let sign = if ((px_x / 2) ^ (px_y / 2)) & 1 == 0 {
            1.0
        } else {
            -1.0
        };
        let local_eps = settings.base.epsilon * combined_mask[px_idx];
        *p = (*p + sign * init_delta * combined_mask[px_idx]).clamp(
            (original[i] - local_eps).max(0.0),
            (original[i] + local_eps).min(1.0),
        );
    }
    let mut momentum_buf = vec![0.0f32; pixels.len()];
    let mut trace = Vec::new();

    for _ in 0..settings.base.steps {
        if cancel.load(Ordering::Relaxed) {
            bail!("Processing cancelled")
        }
        let mut gradient = vec![0.0f32; pixels.len()];
        let mut loss = 0.0f32;
        for o in objectives {
            if cancel.load(Ordering::Relaxed) {
                bail!("Processing cancelled")
            }
            let (value, mut g) = o.objective.loss_gradient(&pixels)?;
            ensure!(
                value.is_finite() && g.len() == pixels.len() && g.iter().all(|x| x.is_finite()),
                "Model returned invalid loss or gradient"
            );
            smooth_gradient_ti(&mut g, width, height, settings.ti_radius);
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

        // MI-FGSM momentum update
        let g_norm = gradient.iter().map(|x| x.abs()).sum::<f32>() / gradient.len() as f32;
        for (m, &g) in momentum_buf.iter_mut().zip(gradient.iter()) {
            let normalized_g = if g_norm > 0. { g / g_norm } else { 0.0 };
            *m = settings.momentum * *m + normalized_g;
        }

        for i in 0..pixels.len() {
            let base = original[i];
            let local_eps = settings.base.epsilon * combined_mask[i / 3];
            let g = momentum_buf[i] - settings.base.fidelity_penalty * 2.0 * (pixels[i] - base);
            let direction = if g > 0. {
                1.0
            } else if g < 0. {
                -1.0
            } else {
                0.0
            };
            pixels[i] = (pixels[i] + settings.base.step_size * direction)
                .clamp((base - local_eps).max(0.0), (base + local_eps).min(1.0));
        }

        if settings.frequency_projection {
            let mut delta: Vec<f32> = pixels.iter().zip(original).map(|(p, o)| p - o).collect();
            frequency::project_mid_frequency_delta(
                &mut delta,
                width,
                height,
                settings.max_dct_band,
            );
            for i in 0..pixels.len() {
                let base = original[i];
                let local_eps = settings.base.epsilon * combined_mask[i / 3];
                pixels[i] = (base + delta[i])
                    .clamp((base - local_eps).max(0.0), (base + local_eps).min(1.0));
            }
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

    #[test]
    fn hardened_optimizer_respects_regional_and_texture_masks() {
        let w = 16usize;
        let h = 16usize;
        let mut orig = vec![0.5f32; w * h * 3];
        // Add an edge/texture in bottom-right quadrant
        for y in 8..16 {
            for x in 8..16 {
                let v = if (x + y) % 2 == 0 { 0.8 } else { 0.2 };
                let idx = (y * w + x) * 3;
                orig[idx] = v;
                orig[idx + 1] = v;
                orig[idx + 2] = v;
            }
        }
        let tex = compute_adaptive_texture_mask(&orig, w, h);
        assert!(tex[2 * w + 2] < tex[12 * w + 12]);

        let reg = build_regional_mask(w, h, &[[2, 2, 4, 4]], 2);
        assert!((reg[3 * w + 3] - 1.0).abs() < 1e-6);
        assert!(reg[14 * w + 14] < 0.3);

        let p = Linear(1.0);
        let c = AtomicBool::new(false);
        let s = HardenedSettings::default();
        let out = optimize_hardened(
            &orig,
            w,
            h,
            &[WeightedObjective {
                objective: &p,
                weight: 1.0,
            }],
            s,
            Some(&reg),
            &c,
        )
        .unwrap();
        assert_eq!(out.pixels.len(), orig.len());
        assert!(
            out.pixels
                .iter()
                .zip(&orig)
                .all(|(a, b)| (a - b).abs() <= s.base.epsilon + 1e-6)
        );
    }
}

#[cfg(feature = "clip")]
pub mod clip;
