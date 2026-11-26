//! Experimental CLIP ViT-B/32 feature suppression objective.
//! Independent implementation of the published architecture using Candle autodiff.
//! This is NOT a port or a reproduction of PhotoGuard, FaceOff or FaceShield.
use crate::Objective;
use anyhow::{Result, ensure};
use candle_core::{D, DType, Device, Tensor, Var};
use candle_nn::VarBuilder;

#[derive(Clone, Copy)]
struct Config {
    side: usize,
    patch: usize,
    hidden: usize,
    heads: usize,
    layers: usize,
    intermediate: usize,
    projection: usize,
}
impl Config {
    const BASE: Self = Self {
        side: 224,
        patch: 32,
        hidden: 768,
        heads: 12,
        layers: 12,
        intermediate: 3072,
        projection: 512,
    };
}

pub struct ClipObjective {
    vb: VarBuilder<'static>,
    config: Config,
    width: usize,
    height: usize,
    indices: [Tensor; 4],
    coefficients: [Tensor; 4],
}
impl ClipObjective {
    /// Call only with a checksum-verified safetensors snapshot and reviewed license.
    /// Resizes with differentiable bilinear sampling, preserving source dimensions on output.
    pub fn from_bytes(bytes: Vec<u8>, width: usize, height: usize) -> Result<Self> {
        let vb = VarBuilder::from_buffered_safetensors(bytes, DType::F32, &Device::Cpu)?;
        Self::new(vb, Config::BASE, width, height)
    }
    fn new(vb: VarBuilder<'static>, config: Config, width: usize, height: usize) -> Result<Self> {
        ensure!(
            width > 0
                && height > 0
                && width <= 4096
                && height <= 4096
                && width * height <= 1_048_576,
            "Experimental CLIP input limit: 1 megapixel, 4096 per side"
        );
        let mut indices: [Vec<u32>; 4] = std::array::from_fn(|_| Vec::new());
        let mut coefficients: [Vec<f32>; 4] = std::array::from_fn(|_| Vec::new());
        for y in 0..config.side {
            for x in 0..config.side {
                let fx = ((x as f32 + 0.5) * width as f32 / config.side as f32 - 0.5)
                    .clamp(0., (width - 1) as f32);
                let fy = ((y as f32 + 0.5) * height as f32 / config.side as f32 - 0.5)
                    .clamp(0., (height - 1) as f32);
                let x0 = fx.floor() as usize;
                let y0 = fy.floor() as usize;
                let x1 = (x0 + 1).min(width - 1);
                let y1 = (y0 + 1).min(height - 1);
                let dx = fx - x0 as f32;
                let dy = fy - y0 as f32;
                for c in 0..3 {
                    for (i, (xx, yy, w)) in [
                        (x0, y0, (1. - dx) * (1. - dy)),
                        (x1, y0, dx * (1. - dy)),
                        (x0, y1, (1. - dx) * dy),
                        (x1, y1, dx * dy),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        indices[i].push(((yy * width + xx) * 3 + c) as u32);
                        coefficients[i].push(w);
                    }
                }
            }
        }
        let indices = indices
            .map(|x| Tensor::from_vec(x, config.side * config.side * 3, &Device::Cpu))
            .into_iter()
            .collect::<candle_core::Result<Vec<_>>>()?
            .try_into()
            .unwrap();
        let coefficients = coefficients
            .map(|x| Tensor::from_vec(x, config.side * config.side * 3, &Device::Cpu))
            .into_iter()
            .collect::<candle_core::Result<Vec<_>>>()?
            .try_into()
            .unwrap();
        Ok(Self {
            vb,
            config,
            width,
            height,
            indices,
            coefficients,
        })
    }
    fn norm(&self, x: &Tensor, prefix: &str) -> candle_core::Result<Tensor> {
        let mean = x.mean_keepdim(D::Minus1)?;
        let centered = x.broadcast_sub(&mean)?;
        let variance = centered.sqr()?.mean_keepdim(D::Minus1)?;
        let x = centered.broadcast_div(&(variance + 1e-5)?.sqrt()?)?;
        x.broadcast_mul(
            &self
                .vb
                .get(self.config.hidden, &format!("{prefix}.weight"))?,
        )?
        .broadcast_add(&self.vb.get(self.config.hidden, &format!("{prefix}.bias"))?)
    }
    fn linear(
        &self,
        x: &Tensor,
        prefix: &str,
        output: usize,
        input: usize,
        bias: bool,
    ) -> candle_core::Result<Tensor> {
        let y = x.matmul(
            &self
                .vb
                .get((output, input), &format!("{prefix}.weight"))?
                .t()?,
        )?;
        if bias {
            y.broadcast_add(&self.vb.get(output, &format!("{prefix}.bias"))?)
        } else {
            Ok(y)
        }
    }
    fn features(&self, input: &Tensor) -> candle_core::Result<Tensor> {
        let c = self.config;
        let g = c.side / c.patch;
        let tokens = g * g + 1;
        let mut sampled = input
            .index_select(&self.indices[0], 0)?
            .mul(&self.coefficients[0])?;
        for i in 1..4 {
            sampled = sampled.add(
                &input
                    .index_select(&self.indices[i], 0)?
                    .mul(&self.coefficients[i])?,
            )?;
        }
        let x = sampled.reshape((c.side, c.side, 3))?;
        let mean = Tensor::new(&[0.48145466f32, 0.4578275, 0.40821073], &Device::Cpu)?;
        let std = Tensor::new(&[0.26862954f32, 0.2613026, 0.2757771], &Device::Cpu)?;
        let x = x
            .broadcast_sub(&mean)?
            .broadcast_div(&std)?
            .permute((2, 0, 1))?
            .reshape((3, g, c.patch, g, c.patch))?
            .permute((1, 3, 0, 2, 4))?
            .contiguous()?
            .reshape((g * g, 3 * c.patch * c.patch))?;
        let kernel = self
            .vb
            .get(
                (c.hidden, 3, c.patch, c.patch),
                "vision_model.embeddings.patch_embedding.weight",
            )?
            .reshape((c.hidden, 3 * c.patch * c.patch))?;
        let patches = x.matmul(&kernel.t()?)?;
        let cls = self
            .vb
            .get(c.hidden, "vision_model.embeddings.class_embedding")?
            .unsqueeze(0)?;
        let x = Tensor::cat(&[&cls, &patches], 0)?.add(&self.vb.get(
            (tokens, c.hidden),
            "vision_model.embeddings.position_embedding.weight",
        )?)?;
        let mut x = self.norm(&x, "vision_model.pre_layrnorm")?;
        for layer in 0..c.layers {
            let p = format!("vision_model.encoder.layers.{layer}");
            let n = self.norm(&x, &format!("{p}.layer_norm1"))?;
            let q = self
                .linear(
                    &n,
                    &format!("{p}.self_attn.q_proj"),
                    c.hidden,
                    c.hidden,
                    true,
                )?
                .reshape((tokens, c.heads, c.hidden / c.heads))?
                .transpose(0, 1)?
                .contiguous()?;
            let k = self
                .linear(
                    &n,
                    &format!("{p}.self_attn.k_proj"),
                    c.hidden,
                    c.hidden,
                    true,
                )?
                .reshape((tokens, c.heads, c.hidden / c.heads))?
                .transpose(0, 1)?
                .contiguous()?;
            let v = self
                .linear(
                    &n,
                    &format!("{p}.self_attn.v_proj"),
                    c.hidden,
                    c.hidden,
                    true,
                )?
                .reshape((tokens, c.heads, c.hidden / c.heads))?
                .transpose(0, 1)?
                .contiguous()?;
            let scores = (q.matmul(&k.transpose(1, 2)?.contiguous()?)?
                / (c.hidden as f64 / c.heads as f64).sqrt())?;
            let attended = candle_nn::ops::softmax(&scores, D::Minus1)?
                .matmul(&v)?
                .transpose(0, 1)?
                .contiguous()?
                .reshape((tokens, c.hidden))?;
            x = x.add(&self.linear(
                &attended,
                &format!("{p}.self_attn.out_proj"),
                c.hidden,
                c.hidden,
                true,
            )?)?;
            let n = self.norm(&x, &format!("{p}.layer_norm2"))?;
            let hidden =
                self.linear(&n, &format!("{p}.mlp.fc1"), c.intermediate, c.hidden, true)?;
            // QuickGELU with primitive operations so gradients remain available.
            let gate = (((&hidden * (-1.702))?.exp()? + 1.)?.recip())?;
            let activated = hidden.mul(&gate)?;
            x = x.add(&self.linear(
                &activated,
                &format!("{p}.mlp.fc2"),
                c.hidden,
                c.intermediate,
                true,
            )?)?;
        }
        let pooled = self.norm(&x.narrow(0, 0, 1)?, "vision_model.post_layernorm")?;
        self.linear(&pooled, "visual_projection", c.projection, c.hidden, false)
    }
    pub fn feature_energy(&self, input: &[f32]) -> Result<f32> {
        ensure!(
            input.len() == self.width * self.height * 3,
            "Model input shape mismatch"
        );
        let x = Tensor::from_slice(input, input.len(), &Device::Cpu)?;
        Ok(self.features(&x)?.sqr()?.mean_all()?.to_scalar()?)
    }
}
impl Objective for ClipObjective {
    fn name(&self) -> &str {
        "experimental-clip-vit-b32-feature-suppression"
    }
    fn loss_gradient(&self, input: &[f32]) -> Result<(f32, Vec<f32>)> {
        ensure!(
            input.len() == self.width * self.height * 3,
            "Model input shape mismatch"
        );
        let x = Var::from_slice(input, input.len(), &Device::Cpu)?;
        let loss = self.features(&x)?.sqr()?.mean_all()?.neg()?;
        let gradient = loss
            .backward()?
            .remove(&x)
            .ok_or_else(|| anyhow::anyhow!("Model produced no input gradient"))?;
        Ok((loss.to_scalar()?, gradient.to_vec1()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    fn tiny() -> ClipObjective {
        let c = Config {
            side: 4,
            patch: 2,
            hidden: 8,
            heads: 2,
            layers: 1,
            intermediate: 12,
            projection: 4,
        };
        let mut weights = HashMap::new();
        let mut insert = |name: String, dims: Vec<usize>, norm: bool| {
            let n = dims.iter().product::<usize>();
            let data: Vec<f32> = (0..n)
                .map(|i| {
                    if norm {
                        1.
                    } else {
                        ((i * 17 % 101) as f32 - 50.) * 0.002
                    }
                })
                .collect();
            weights.insert(name, Tensor::from_vec(data, dims, &Device::Cpu).unwrap());
        };
        insert(
            "vision_model.embeddings.patch_embedding.weight".into(),
            vec![8, 3, 2, 2],
            false,
        );
        insert(
            "vision_model.embeddings.class_embedding".into(),
            vec![8],
            false,
        );
        insert(
            "vision_model.embeddings.position_embedding.weight".into(),
            vec![5, 8],
            false,
        );
        for p in [
            "vision_model.pre_layrnorm",
            "vision_model.post_layernorm",
            "vision_model.encoder.layers.0.layer_norm1",
            "vision_model.encoder.layers.0.layer_norm2",
        ] {
            insert(format!("{p}.weight"), vec![8], true);
            insert(format!("{p}.bias"), vec![8], false);
        }
        for name in ["q_proj", "k_proj", "v_proj", "out_proj"] {
            let p = format!("vision_model.encoder.layers.0.self_attn.{name}");
            insert(format!("{p}.weight"), vec![8, 8], false);
            insert(format!("{p}.bias"), vec![8], false);
        }
        for (name, out, input) in [("fc1", 12, 8), ("fc2", 8, 12)] {
            let p = format!("vision_model.encoder.layers.0.mlp.{name}");
            insert(format!("{p}.weight"), vec![out, input], false);
            insert(format!("{p}.bias"), vec![out], false);
        }
        insert("visual_projection.weight".into(), vec![4, 8], false);
        ClipObjective::new(
            VarBuilder::from_tensors(weights, DType::F32, &Device::Cpu),
            c,
            6,
            5,
        )
        .unwrap()
    }
    #[test]
    fn input_gradient_matches_finite_difference() {
        let m = tiny();
        let mut x: Vec<f32> = (0..90).map(|i| 0.3 + (i % 7) as f32 * 0.03).collect();
        let (_, g) = m.loss_gradient(&x).unwrap();
        assert!(g.iter().any(|g| g.abs() > 1e-8));
        for i in [0, 31, 76] {
            let base = x[i];
            x[i] = base + 0.001;
            let plus = -m.feature_energy(&x).unwrap();
            x[i] = base - 0.001;
            let minus = -m.feature_energy(&x).unwrap();
            x[i] = base;
            let numeric = (plus - minus) / 0.002;
            assert!((g[i] - numeric).abs() < 0.0001, "{} != {}", g[i], numeric);
        }
    }
}
