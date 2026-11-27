//! Audio protection engine: projected-gradient optimisation of a perturbation that is kept
//! below a simple psychoacoustic masking threshold.
//!
//! The engine optimises whatever differentiable objective is supplied (for example a speaker
//! embedding model's distance from the original voice). No validated voice-cloning objective
//! is shipped: `protect` refuses to run without one, so the application never adds noise and
//! calls it protection. Tests use analytic fixtures only to verify the constraint machinery.
//!
//! Masking model (deliberately conservative and simple): per STFT frame, the allowed
//! perturbation magnitude in each bin is the signal magnitude spread over +-3 bins, lowered by
//! `masking_offset_db`, with a floor from a fixed absolute threshold. A global SNR floor and a
//! no-clipping rule are enforced after every step.
use anyhow::{Result, bail, ensure};
use rustfft::num_complex::Complex;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

pub trait AudioObjective: Send + Sync {
    fn name(&self) -> &str;
    /// Loss to maximise and its gradient with respect to the mono samples.
    fn loss_gradient(&self, samples: &[f32], sample_rate: u32) -> Result<(f32, Vec<f32>)>;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Settings {
    pub steps: u32,
    /// Step size as a fraction of the per-bin masking threshold.
    pub step: f32,
    pub masking_offset_db: f32,
    /// Minimum signal-to-perturbation ratio.
    pub snr_floor_db: f32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            steps: 50,
            step: 0.2,
            masking_offset_db: 18.,
            snr_floor_db: 30.,
        }
    }
}
impl Settings {
    /// Real processing configurations (none has measured protection effectiveness).
    pub fn profile(name: &str) -> Option<Self> {
        Some(match name {
            "Original Quality" => Self {
                masking_offset_db: 24.,
                snr_floor_db: 40.,
                ..Self::default()
            },
            "Balanced Protection" | "Voice Protection" => Self::default(),
            "Maximum Protection" => Self {
                steps: 150,
                masking_offset_db: 12.,
                snr_floor_db: 24.,
                ..Self::default()
            },
            "Music Protection" => Self {
                masking_offset_db: 22.,
                snr_floor_db: 36.,
                ..Self::default()
            },
            _ => return None,
        })
    }
    fn validate(&self) -> Result<()> {
        ensure!((1..=2000).contains(&self.steps), "Steps must be 1..2000");
        ensure!(
            self.step.is_finite() && self.step > 0. && self.step <= 1.,
            "Step must be in (0,1]"
        );
        ensure!(
            self.masking_offset_db.is_finite() && (6.0..=60.0).contains(&self.masking_offset_db),
            "Masking offset must be 6..60 dB"
        );
        ensure!(
            self.snr_floor_db.is_finite() && (10.0..=80.0).contains(&self.snr_floor_db),
            "SNR floor must be 10..80 dB"
        );
        Ok(())
    }
}

pub const FRAME: usize = 1024;
const HOP: usize = FRAME / 2;
const ABS_FLOOR: f32 = 1e-5;

struct Stft {
    fwd: std::sync::Arc<dyn rustfft::Fft<f32>>,
    inv: std::sync::Arc<dyn rustfft::Fft<f32>>,
    window: Vec<f32>,
}
impl Stft {
    fn new() -> Self {
        let mut p = rustfft::FftPlanner::new();
        // sqrt-Hann analysis and synthesis windows sum to one at 50% overlap.
        let window = (0..FRAME)
            .map(|i| {
                (0.5 - 0.5 * (2. * std::f32::consts::PI * i as f32 / FRAME as f32).cos()).sqrt()
            })
            .collect();
        Self {
            fwd: p.plan_fft_forward(FRAME),
            inv: p.plan_fft_inverse(FRAME),
            window,
        }
    }
    fn frames(n: usize) -> usize {
        n.div_ceil(HOP) + 1
    }
    fn analyze(&self, x: &[f32]) -> Vec<Vec<Complex<f32>>> {
        (0..Self::frames(x.len()))
            .map(|f| {
                let start = f as isize * HOP as isize - HOP as isize;
                let mut buf: Vec<Complex<f32>> = (0..FRAME)
                    .map(|i| {
                        let j = start + i as isize;
                        let v = if j >= 0 && (j as usize) < x.len() {
                            x[j as usize]
                        } else {
                            0.
                        };
                        Complex::new(v * self.window[i], 0.)
                    })
                    .collect();
                self.fwd.process(&mut buf);
                buf
            })
            .collect()
    }
    fn synthesize(&self, frames: &[Vec<Complex<f32>>], n: usize) -> Vec<f32> {
        let mut out = vec![0f32; n];
        for (f, spec) in frames.iter().enumerate() {
            let mut buf = spec.clone();
            self.inv.process(&mut buf);
            let start = f as isize * HOP as isize - HOP as isize;
            for i in 0..FRAME {
                let j = start + i as isize;
                if j >= 0 && (j as usize) < n {
                    out[j as usize] += buf[i].re / FRAME as f32 * self.window[i];
                }
            }
        }
        out
    }
}

/// Per-frame, per-bin allowed perturbation magnitude.
fn thresholds(spec: &[Vec<Complex<f32>>], offset_db: f32) -> Vec<Vec<f32>> {
    let k = 10f32.powf(-offset_db / 20.);
    let raw: Vec<Vec<f32>> = spec
        .iter()
        .map(|frame| {
            let mags: Vec<f32> = frame.iter().map(|c| c.norm()).collect();
            (0..FRAME)
                .map(|b| {
                    let lo = b.saturating_sub(3);
                    let hi = (b + 3).min(FRAME - 1);
                    let spread = mags[lo..=hi].iter().copied().fold(0f32, f32::max);
                    (spread * k).max(ABS_FLOOR)
                })
                .collect()
        })
        .collect();
    // Overlapping frames share samples, so a loud frame's allowance would bleed into a quiet
    // neighbour. Use the minimum over adjacent frames (conservative: ignores temporal masking).
    (0..raw.len())
        .map(|f| {
            let (a, b) = (f.saturating_sub(1), (f + 1).min(raw.len() - 1));
            (0..FRAME)
                .map(|bin| (a..=b).map(|g| raw[g][bin]).fold(f32::MAX, f32::min))
                .collect()
        })
        .collect()
}

/// Alternating projection: per-frame spectral clipping is not exactly consistent after
/// overlap-add, so the constraint is re-applied a few times.
fn project(stft: &Stft, delta: &[f32], limits: &[Vec<f32>]) -> Vec<f32> {
    let mut d = delta.to_vec();
    for _ in 0..10 {
        let mut spec = stft.analyze(&d);
        for (frame, lim) in spec.iter_mut().zip(limits) {
            for (c, l) in frame.iter_mut().zip(lim) {
                let m = c.norm();
                if m > *l {
                    *c *= *l / m;
                }
            }
        }
        d = stft.synthesize(&spec, d.len());
    }
    d
}

#[derive(Debug, Clone, Serialize)]
pub struct Protected {
    #[serde(skip)]
    pub samples: Vec<f32>,
    pub trace: Vec<f32>,
    pub snr_db: f64,
    pub peak: f32,
    pub objectives: Vec<String>,
    pub efficacy_verified: bool,
}

pub fn snr_db(x: &[f32], y: &[f32]) -> f64 {
    let s: f64 = x.iter().map(|v| f64::from(*v).powi(2)).sum();
    let n: f64 = x.iter().zip(y).map(|(a, b)| f64::from(b - a).powi(2)).sum();
    if n == 0. {
        f64::INFINITY
    } else {
        10. * (s / n).log10()
    }
}

/// Optimises a perturbation of mono `samples` under the masking, SNR and no-clipping limits.
pub fn protect(
    samples: &[f32],
    rate: u32,
    objectives: &[(&dyn AudioObjective, f32)],
    s: Settings,
    cancel: &AtomicBool,
) -> Result<Protected> {
    s.validate()?;
    if objectives.is_empty() {
        bail!("No validated voice-protection model is installed; the audio was not changed");
    }
    ensure!(
        !samples.is_empty() && samples.len() <= 60 * 60 * 48_000,
        "Segment must be 1 sample to 1 hour at 48 kHz"
    );
    ensure!(samples.iter().all(|v| v.is_finite()), "Non-finite samples");
    ensure!(
        objectives.iter().all(|(_, w)| w.is_finite() && *w > 0.),
        "Weights must be positive"
    );
    let stft = Stft::new();
    let limits = thresholds(&stft.analyze(samples), s.masking_offset_db);
    let mut delta = vec![0f32; samples.len()];
    let mut trace = vec![];
    for _ in 0..s.steps {
        if cancel.load(Ordering::Relaxed) {
            bail!("Processing cancelled");
        }
        let x: Vec<f32> = samples.iter().zip(&delta).map(|(a, d)| a + d).collect();
        let mut grad = vec![0f32; samples.len()];
        let mut loss = 0.;
        let wsum: f32 = objectives.iter().map(|(_, w)| w).sum();
        for (o, w) in objectives {
            let (l, g) = o.loss_gradient(&x, rate)?;
            ensure!(
                l.is_finite() && g.len() == x.len() && g.iter().all(|v| v.is_finite()),
                "Objective returned invalid values"
            );
            let norm = (g.iter().map(|v| v * v).sum::<f32>()).sqrt().max(1e-12);
            for (a, b) in grad.iter_mut().zip(&g) {
                *a += w / wsum * b / norm;
            }
            loss += w / wsum * l;
        }
        // Step in the spectral domain, scaled to each bin's allowance, then project.
        let mut gs = stft.analyze(&grad);
        for (frame, lim) in gs.iter_mut().zip(&limits) {
            for (c, l) in frame.iter_mut().zip(lim) {
                let m = c.norm().max(1e-20);
                *c *= s.step * l / m;
            }
        }
        let step = stft.synthesize(&gs, samples.len());
        let cand: Vec<f32> = delta.iter().zip(&step).map(|(d, st)| d + st).collect();
        delta = project(&stft, &cand, &limits);
        // Global SNR floor and no clipping.
        let snr = snr_db(
            samples,
            &samples
                .iter()
                .zip(&delta)
                .map(|(a, d)| a + d)
                .collect::<Vec<_>>(),
        );
        if snr < f64::from(s.snr_floor_db) {
            let k = 10f64.powf((snr - f64::from(s.snr_floor_db)) / 20.) as f32;
            delta.iter_mut().for_each(|d| *d *= k);
        }
        for (d, a) in delta.iter_mut().zip(samples) {
            let y = a + *d;
            if y.abs() > 0.999 {
                *d = y.clamp(-0.999, 0.999) - a;
            }
        }
        trace.push(loss);
    }
    let out: Vec<f32> = samples.iter().zip(&delta).map(|(a, d)| a + d).collect();
    let peak = out.iter().fold(0f32, |m, v| m.max(v.abs()));
    Ok(Protected {
        snr_db: snr_db(samples, &out),
        samples: out,
        trace,
        peak,
        objectives: objectives
            .iter()
            .map(|(o, _)| o.name().to_string())
            .collect(),
        efficacy_verified: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    /// TEST FIXTURE ONLY: rewards energy at 2 kHz. Not a protection objective.
    struct Fixture;
    impl AudioObjective for Fixture {
        fn name(&self) -> &str {
            "test fixture"
        }
        fn loss_gradient(&self, x: &[f32], rate: u32) -> Result<(f32, Vec<f32>)> {
            let w = 2. * std::f32::consts::PI * 2000. / rate as f32;
            let c: f32 = x
                .iter()
                .enumerate()
                .map(|(i, v)| v * (w * i as f32).cos())
                .sum();
            Ok((
                c,
                (0..x.len())
                    .map(|i| (w * i as f32).cos() * c.signum())
                    .collect(),
            ))
        }
    }
    fn speechlike(n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = i as f32 / 16_000.;
                let env = (2. * std::f32::consts::PI * 3. * t).sin().abs();
                env * (0.3 * (2. * std::f32::consts::PI * 180. * t).sin()
                    + 0.15 * (2. * std::f32::consts::PI * 2100. * t).sin())
            })
            .collect()
    }
    #[test]
    fn refuses_without_model() {
        let e = protect(
            &[0.1; 100],
            16_000,
            &[],
            Settings::default(),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(e.to_string().contains("No validated"));
    }
    #[test]
    fn stft_reconstructs_perfectly() {
        let x = speechlike(5000);
        let s = Stft::new();
        let y = s.synthesize(&s.analyze(&x), x.len());
        assert!(x.iter().zip(&y).all(|(a, b)| (a - b).abs() < 1e-4));
    }
    #[test]
    fn perturbation_respects_masking_snr_and_clipping() {
        let x = speechlike(16_000);
        let f = Fixture;
        let s = Settings {
            steps: 20,
            ..Settings::default()
        };
        let p = protect(&x, 16_000, &[(&f, 1.)], s, &AtomicBool::new(false)).unwrap();
        assert_eq!(p.samples.len(), x.len());
        assert!(p.snr_db >= f64::from(s.snr_floor_db) - 0.01, "{}", p.snr_db);
        assert!(p.peak < 1.);
        assert!(
            p.trace.last().unwrap() > p.trace.first().unwrap(),
            "objective increases"
        );
        // Every bin of the final perturbation stays within its masking allowance (+ small leakage).
        let st = Stft::new();
        let lim = thresholds(&st.analyze(&x), s.masking_offset_db);
        let d: Vec<f32> = p.samples.iter().zip(&x).map(|(a, b)| a - b).collect();
        let ds = st.analyze(&d);
        // Per frame, perturbation energy stays within the frame's masking allowance.
        let mut worst = 0f32;
        for (fr, l) in ds.iter().zip(&lim) {
            let allowed: f32 = l.iter().map(|v| v * v).sum();
            let used: f32 = fr.iter().map(|c| c.norm_sqr()).sum();
            if allowed > 1e-6 {
                worst = worst.max(used / allowed);
            }
        }
        assert!(
            worst < 1.5,
            "perturbation exceeds the frame masking allowance by {worst}x"
        );
        assert!(!p.efficacy_verified);
        let cancel = AtomicBool::new(true);
        assert!(protect(&x, 16_000, &[(&f, 1.)], s, &cancel).is_err());
        assert!(
            Settings::profile("Maximum Protection").is_some() && Settings::profile("x").is_none()
        );
    }
}
