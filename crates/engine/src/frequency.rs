//! Frequency-domain (8x8 2D DCT) analysis and compression-resistant perturbation utilities.
//!
//! Standard JPEG compression (quantization tables) aggressively discards high-frequency
//! pixel-level perturbations (DCT indices u+v >= 7). Concentrating adversarial energy in
//! mid-frequency bands (2 <= u+v <= 6) improves survival through JPEG, WebP, and resizing
//! while minimizing visible blocking or color banding in flat areas.

use crate::Objective;
use anyhow::{Result, ensure};
use std::f32::consts::PI;

const N: usize = 8;

/// Precomputed 8x8 orthonormal DCT-II basis matrix `C[k][n]`.
fn dct_matrix() -> [[f32; N]; N] {
    let mut c = [[0.0f32; N]; N];
    for (k, row) in c.iter_mut().enumerate() {
        let alpha = if k == 0 {
            (1.0 / N as f32).sqrt()
        } else {
            (2.0 / N as f32).sqrt()
        };
        for (n, val) in row.iter_mut().enumerate() {
            *val = alpha * ((PI * (2 * n + 1) as f32 * k as f32) / (2.0 * N as f32)).cos();
        }
    }
    c
}

/// Forward 8x8 2D DCT-II on a row-major 8x8 block.
pub fn dct2_8x8(block: &[f32; 64]) -> [f32; 64] {
    let c = dct_matrix();
    let mut temp = [0.0f32; 64];
    for r in 0..N {
        for k in 0..N {
            let mut sum = 0.0f32;
            for n in 0..N {
                sum += block[r * N + n] * c[k][n];
            }
            temp[r * N + k] = sum;
        }
    }
    let mut out = [0.0f32; 64];
    for col in 0..N {
        for k in 0..N {
            let mut sum = 0.0f32;
            for r in 0..N {
                sum += c[k][r] * temp[r * N + col];
            }
            out[k * N + col] = sum;
        }
    }
    out
}

/// Inverse 8x8 2D DCT-II (orthonormal transpose) on a row-major 8x8 block.
pub fn idct2_8x8(coeffs: &[f32; 64]) -> [f32; 64] {
    let c = dct_matrix();
    let mut temp = [0.0f32; 64];
    for col in 0..N {
        for n in 0..N {
            let mut sum = 0.0f32;
            for k in 0..N {
                sum += c[k][n] * coeffs[k * N + col];
            }
            temp[n * N + col] = sum;
        }
    }
    let mut out = [0.0f32; 64];
    for r in 0..N {
        for n in 0..N {
            let mut sum = 0.0f32;
            for k in 0..N {
                sum += temp[r * N + k] * c[k][n];
            }
            out[r * N + n] = sum;
        }
    }
    out
}

/// Filters a perturbation delta image (`width * height * 3` RGB f32) in 8x8 DCT blocks
/// to suppress fragile high-frequency checkerboard noise (`u + v > max_band`) and DC drift
/// (`u + v == 0`), concentrating perturbation energy in compression-resistant mid-frequency bands.
pub fn project_mid_frequency_delta(
    delta: &mut [f32],
    width: usize,
    height: usize,
    max_band: usize,
) {
    if width < N || height < N || delta.len() != width * height * 3 {
        return;
    }
    let bx_count = width / N;
    let by_count = height / N;
    for by in 0..by_count {
        for bx in 0..bx_count {
            for ch in 0..3 {
                let mut block = [0.0f32; 64];
                for dy in 0..N {
                    let y = by * N + dy;
                    for dx in 0..N {
                        let x = bx * N + dx;
                        block[dy * N + dx] = delta[(y * width + x) * 3 + ch];
                    }
                }
                let mut coeffs = dct2_8x8(&block);
                for u in 0..N {
                    for v in 0..N {
                        let band = u + v;
                        if band == 0 {
                            // Gently damp DC shift per step to preserve local brightness/color accuracy.
                            coeffs[u * N + v] *= 0.90;
                        } else if band > max_band {
                            // Gently damp fragile high frequencies that JPEG/VAE discard.
                            coeffs[u * N + v] *= 0.65;
                        }
                    }
                }
                let filtered = idct2_8x8(&coeffs);
                for dy in 0..N {
                    let y = by * N + dy;
                    for dx in 0..N {
                        let x = bx * N + dx;
                        delta[(y * width + x) * 3 + ch] = filtered[dy * N + dx];
                    }
                }
            }
        }
    }
}

/// Differentiable mid-frequency structural perturbation objective.
///
/// Encourages mid-frequency DCT divergence from the original image while penalizing
/// DC color shifts and fragile high-frequency noise. Designed to be combined with neural
/// objectives (e.g., CLIP, face embeddings) in `optimize_hardened`.
pub struct FrequencyBandObjective {
    original: Vec<f32>,
    width: usize,
    height: usize,
}

impl FrequencyBandObjective {
    pub fn new(original: Vec<f32>, width: usize, height: usize) -> Result<Self> {
        ensure!(
            width >= N && height >= N && original.len() == width * height * 3,
            "FrequencyBandObjective requires at least 8x8 RGB input"
        );
        Ok(Self {
            original,
            width,
            height,
        })
    }
}

impl Objective for FrequencyBandObjective {
    fn name(&self) -> &str {
        "mid-frequency-dct-resilience"
    }

    fn loss_gradient(&self, input: &[f32]) -> Result<(f32, Vec<f32>)> {
        ensure!(
            input.len() == self.original.len(),
            "FrequencyBandObjective input length mismatch"
        );
        let mut grad = vec![0.0f32; input.len()];
        let mut total_loss = 0.0f32;
        let bx_count = self.width / N;
        let by_count = self.height / N;

        for by in 0..by_count {
            for bx in 0..bx_count {
                for ch in 0..3 {
                    let mut diff_block = [0.0f32; 64];
                    for dy in 0..N {
                        let y = by * N + dy;
                        for dx in 0..N {
                            let x = bx * N + dx;
                            let idx = (y * self.width + x) * 3 + ch;
                            diff_block[dy * N + dx] = input[idx] - self.original[idx];
                        }
                    }
                    let coeffs = dct2_8x8(&diff_block);
                    let mut g_coeffs = [0.0f32; 64];
                    for u in 0..N {
                        for v in 0..N {
                            let band = u + v;
                            let c = coeffs[u * N + v];
                            // Weight > 0 rewards energy in mid-freq (2..=5),
                            // weight < 0 penalizes DC (0) and fragile high-freq (>= 7).
                            let w = match band {
                                0 => -2.0,
                                1 => 0.2,
                                2..=5 => 1.0,
                                6 => 0.1,
                                _ => -1.5,
                            };
                            total_loss += 0.5 * w * c * c;
                            g_coeffs[u * N + v] = w * c;
                        }
                    }
                    let g_spatial = idct2_8x8(&g_coeffs);
                    for dy in 0..N {
                        let y = by * N + dy;
                        for dx in 0..N {
                            let x = bx * N + dx;
                            let idx = (y * self.width + x) * 3 + ch;
                            grad[idx] = g_spatial[dy * N + dx];
                        }
                    }
                }
            }
        }
        let norm_blocks = (bx_count * by_count * 3).max(1) as f32;
        Ok((total_loss / norm_blocks, grad))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dct_idct_roundtrip_is_exact() {
        let mut block = [0.0f32; 64];
        for (i, v) in block.iter_mut().enumerate() {
            *v = ((i * 13 % 29) as f32 - 14.0) * 0.05;
        }
        let coeffs = dct2_8x8(&block);
        let recon = idct2_8x8(&coeffs);
        for (a, b) in block.iter().zip(recon.iter()) {
            assert!((a - b).abs() < 1e-5, "DCT roundtrip mismatch: {a} vs {b}");
        }
    }

    #[test]
    fn frequency_objective_gradient_matches_finite_difference() {
        let orig = vec![0.5f32; 8 * 8 * 3];
        let mut x: Vec<f32> = (0..8 * 8 * 3)
            .map(|i| 0.48 + ((i * 7) % 11) as f32 * 0.005)
            .collect();
        let obj = FrequencyBandObjective::new(orig, 8, 8).unwrap();
        let (_, g) = obj.loss_gradient(&x).unwrap();
        // Loss is normalized by norm_blocks = 3 in return value, whereas grad is unnormalized
        // per-block derivative; check sign and proportionality on a sample index.
        let idx = 17;
        let eps = 1e-3f32;
        x[idx] += eps;
        let (lp, _) = obj.loss_gradient(&x).unwrap();
        x[idx] -= 2.0 * eps;
        let (lm, _) = obj.loss_gradient(&x).unwrap();
        let numeric = (lp - lm) / (2.0 * eps) * 3.0;
        assert!(
            (g[idx] - numeric).abs() < 1e-3,
            "grad {} != numeric {}",
            g[idx],
            numeric
        );
    }
}
