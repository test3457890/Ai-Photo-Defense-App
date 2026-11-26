//! Temporal consistency measurements for video protection research (Defense Lab).
//!
//! Given original and processed frame sequences, measures how much the *perturbation* changes
//! from frame to frame. Independent per-frame perturbations produce high values (visible
//! flicker); temporally consistent perturbations produce low values. These are quality
//! measurements only and say nothing about protection effectiveness.
use anyhow::{Result, ensure};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Temporal {
    pub frames: usize,
    /// Mean absolute perturbation per frame (0..255 scale).
    pub mean_perturbation: f64,
    /// Mean absolute change of the perturbation between consecutive frames.
    pub perturbation_flicker: f64,
    /// Mean absolute change in global luminance between consecutive frames introduced by
    /// processing (processed delta minus original delta).
    pub luminance_flicker: f64,
}

/// Frames are RGB8 buffers of equal size.
pub fn measure(original: &[Vec<u8>], processed: &[Vec<u8>]) -> Result<Temporal> {
    ensure!(
        original.len() == processed.len() && !original.is_empty(),
        "Sequences must have the same, non-zero length"
    );
    let n = original[0].len();
    ensure!(
        n > 0 && original.iter().chain(processed).all(|f| f.len() == n),
        "All frames must have the same size"
    );
    let pert: Vec<Vec<i16>> = original
        .iter()
        .zip(processed)
        .map(|(a, b)| {
            a.iter()
                .zip(b)
                .map(|(x, y)| i16::from(*y) - i16::from(*x))
                .collect()
        })
        .collect();
    let mean_perturbation = pert
        .iter()
        .map(|p| p.iter().map(|v| f64::from(v.unsigned_abs())).sum::<f64>() / n as f64)
        .sum::<f64>()
        / pert.len() as f64;
    let luma = |f: &[u8]| f.iter().map(|v| f64::from(*v)).sum::<f64>() / n as f64;
    let (mut flick, mut lum) = (0., 0.);
    for k in 1..pert.len() {
        flick += pert[k]
            .iter()
            .zip(&pert[k - 1])
            .map(|(a, b)| f64::from((a - b).unsigned_abs()))
            .sum::<f64>()
            / n as f64;
        lum += ((luma(&processed[k]) - luma(&processed[k - 1]))
            - (luma(&original[k]) - luma(&original[k - 1])))
        .abs();
    }
    let d = (pert.len() - 1).max(1) as f64;
    Ok(Temporal {
        frames: pert.len(),
        mean_perturbation,
        perturbation_flicker: flick / d,
        luminance_flicker: lum / d,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn consistent_perturbation_has_no_flicker() {
        let frames: Vec<Vec<u8>> = (0..5).map(|k| vec![100 + k as u8; 300]).collect();
        let consistent: Vec<Vec<u8>> = frames
            .iter()
            .map(|f| {
                f.iter()
                    .enumerate()
                    .map(|(i, v)| v + (i % 3) as u8)
                    .collect()
            })
            .collect();
        let independent: Vec<Vec<u8>> = frames
            .iter()
            .enumerate()
            .map(|(k, f)| {
                f.iter()
                    .enumerate()
                    .map(|(i, v)| v + ((i + k) % 3) as u8)
                    .collect()
            })
            .collect();
        let c = measure(&frames, &consistent).unwrap();
        let i = measure(&frames, &independent).unwrap();
        assert_eq!(c.perturbation_flicker, 0.);
        assert!(i.perturbation_flicker > 0.5);
        assert!(
            (c.mean_perturbation - i.mean_perturbation).abs() < 1e-9,
            "same budget, different stability"
        );
        assert!(measure(&frames, &frames[..2]).is_err());
    }
}
