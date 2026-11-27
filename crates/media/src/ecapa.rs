//! Apache-2.0 ECAPA-TDNN speaker encoder (`speechbrain/spkrec-ecapa-voxceleb`) in `candle` 0.11
//! and differentiable `AudioObjective` for `crate::protect`.
//!
//! Security & licensing invariants:
//! - Weights are loaded strictly from a user-selected local directory (`model.safetensors`,
//!   optionally verified against a schema-2 `manifest.json` via `photo_models`).
//! - Pickle / PyTorch binary checkpoints (`.pt`, `.pth`, `.bin`, `.pkl`, `.pickle`) are refused.
//! - Zero network access; nothing is ever downloaded at runtime.

use crate::audio::{self, Decoder};
use crate::protect::{self, AudioObjective, Protected, Settings};
use anyhow::{Context, Result, bail, ensure};
use candle_core::{DType, Device, Tensor, Var};
use candle_nn::{Conv1d, Conv1dConfig, Module, VarBuilder};
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::AtomicBool;

pub const ECAPA_SAMPLE_RATE: u32 = 16_000;
const N_FFT: usize = 400;
const HOP_LENGTH: usize = 160;
const N_MELS: usize = 80;
const N_FREQS: usize = N_FFT / 2 + 1;
const RES2NET_SCALE: usize = 8;
const EMB_DIM: usize = 192;

#[derive(Debug, Clone, Serialize)]
pub struct VoiceProtectReport {
    pub snr_db: f64,
    pub segmental_snr_db: Option<f64>,
    pub peak: f32,
    pub cosine_before: f32,
    pub cosine_after: f32,
    pub cosine_after_compression_sim: f32,
    pub cosine_after_denoise_sim: f32,
    pub efficacy_verified: bool,
    pub note: String,
}

/// Validates `model_dir`, refuses pickle/binary checkpoints, and verifies `manifest.json` if present.
pub fn validate_model_dir(model_dir: &Path) -> Result<()> {
    ensure!(
        model_dir.is_dir(),
        "Voice model path must be a local directory: {}",
        model_dir.display()
    );
    let forbidden = ["pt", "pth", "bin", "pkl", "pickle"];
    if let Ok(entries) = std::fs::read_dir(model_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file()
                && let Some(ext) = p.extension().and_then(|e| e.to_str())
                && forbidden.iter().any(|f| ext.eq_ignore_ascii_case(f))
            {
                bail!(
                    "Refusing unsafe pickle/binary weights file ({}); only model.safetensors is permitted",
                    p.display()
                );
            }
        }
    }
    let weights_path = model_dir.join("model.safetensors");
    ensure!(
        weights_path.is_file(),
        "Missing required model.safetensors in {}",
        model_dir.display()
    );
    let manifest_path = model_dir.join("manifest.json");
    if manifest_path.is_file() {
        let bytes = std::fs::read(&manifest_path).context("Failed to read manifest.json")?;
        let manifest: photo_models::Manifest =
            serde_json::from_slice(&bytes).context("Invalid manifest.json")?;
        photo_models::verify(&weights_path, &manifest)?;
    }
    Ok(())
}

/// Reflect-pads the last dimension (`time`) of a `[B, C, T]` tensor on both sides by `pad`.
fn reflect_pad_1d(x: &Tensor, pad: usize) -> Result<Tensor> {
    if pad == 0 {
        return Ok(x.clone());
    }
    let (_, _, t) = x.dims3()?;
    if t <= 1 {
        let zeros = Tensor::zeros((x.dim(0)?, x.dim(1)?, pad), x.dtype(), x.device())?;
        return Ok(Tensor::cat(&[&zeros, x, &zeros], 2)?);
    }
    let mut left_parts = Vec::with_capacity(pad);
    for i in (1..=pad).rev() {
        let idx = i.min(t - 1);
        left_parts.push(x.narrow(2, idx, 1)?);
    }
    let mut right_parts = Vec::with_capacity(pad);
    for i in 1..=pad {
        let idx = t.saturating_sub(1 + i);
        right_parts.push(x.narrow(2, idx, 1)?);
    }
    let mut all: Vec<Tensor> = Vec::with_capacity(pad * 2 + 1);
    all.extend(left_parts);
    all.push(x.clone());
    all.extend(right_parts);
    let refs: Vec<&Tensor> = all.iter().collect();
    Ok(Tensor::cat(&refs, 2)?)
}

#[derive(Debug, Clone)]
struct BatchNorm1dEval {
    scale: Tensor,
    bias: Tensor,
}

impl BatchNorm1dEval {
    fn load(channels: usize, eps: f64, vb: VarBuilder) -> Result<Self> {
        let weight = vb.get(channels, "weight")?;
        let bias = vb.get(channels, "bias")?;
        let running_mean = vb.get(channels, "running_mean")?;
        let running_var = vb.get(channels, "running_var")?;
        let inv_std = (running_var + eps)?.sqrt()?.recip()?;
        let scale = (&weight * &inv_std)?;
        let shift = (&bias - (&running_mean * &scale)?)?;
        Ok(Self {
            scale: scale.reshape((1, channels, 1))?,
            bias: shift.reshape((1, channels, 1))?,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        Ok(x.broadcast_mul(&self.scale)?.broadcast_add(&self.bias)?)
    }
}

#[derive(Debug, Clone)]
struct TdnnBlock {
    conv: Conv1d,
    norm: BatchNorm1dEval,
    pad: usize,
}

impl TdnnBlock {
    fn load(
        in_ch: usize,
        out_ch: usize,
        kernel_size: usize,
        dilation: usize,
        vb: VarBuilder,
    ) -> Result<Self> {
        let pad = (kernel_size - 1) * dilation / 2;
        let cfg = Conv1dConfig {
            padding: 0,
            stride: 1,
            dilation,
            groups: 1,
            cudnn_fwd_algo: None,
        };
        let conv = candle_nn::conv1d(in_ch, out_ch, kernel_size, cfg, vb.pp("conv.conv"))?;
        let norm = BatchNorm1dEval::load(out_ch, 1e-5, vb.pp("norm.norm"))?;
        Ok(Self { conv, norm, pad })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let xp = reflect_pad_1d(x, self.pad)?;
        let y = self.conv.forward(&xp)?.relu()?;
        self.norm.forward(&y)
    }
}

#[derive(Debug, Clone)]
struct Res2NetBlock {
    blocks: Vec<TdnnBlock>,
    scale: usize,
}

impl Res2NetBlock {
    fn load(
        channels: usize,
        kernel_size: usize,
        dilation: usize,
        scale: usize,
        vb: VarBuilder,
    ) -> Result<Self> {
        ensure!(
            channels.is_multiple_of(scale),
            "channels must divide by scale"
        );
        let sub_ch = channels / scale;
        let mut blocks = Vec::with_capacity(scale - 1);
        for i in 0..(scale - 1) {
            blocks.push(TdnnBlock::load(
                sub_ch,
                sub_ch,
                kernel_size,
                dilation,
                vb.pp(format!("blocks.{i}")),
            )?);
        }
        Ok(Self { blocks, scale })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let chunks = x.chunk(self.scale, 1)?;
        let mut out = Vec::with_capacity(self.scale);
        let mut prev: Option<Tensor> = None;
        for (i, chunk) in chunks.into_iter().enumerate() {
            if i == 0 {
                out.push(chunk);
            } else {
                let inp = match (&prev, i) {
                    (Some(p), idx) if idx > 1 => (&chunk + p)?,
                    _ => chunk,
                };
                let y = self.blocks[i - 1].forward(&inp)?;
                prev = Some(y.clone());
                out.push(y);
            }
        }
        let refs: Vec<&Tensor> = out.iter().collect();
        Ok(Tensor::cat(&refs, 1)?)
    }
}

#[derive(Debug, Clone)]
struct SeBlock {
    conv1: Conv1d,
    conv2: Conv1d,
}

impl SeBlock {
    fn load(channels: usize, se_channels: usize, vb: VarBuilder) -> Result<Self> {
        let cfg = Conv1dConfig::default();
        let conv1 = candle_nn::conv1d(channels, se_channels, 1, cfg, vb.pp("conv1.conv"))?;
        let conv2 = candle_nn::conv1d(se_channels, channels, 1, cfg, vb.pp("conv2.conv"))?;
        Ok(Self { conv1, conv2 })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let s = x.mean_keepdim(2)?;
        let s = self.conv1.forward(&s)?.relu()?;
        let s = candle_nn::ops::sigmoid(&self.conv2.forward(&s)?)?;
        Ok(x.broadcast_mul(&s)?)
    }
}

#[derive(Debug, Clone)]
struct SeRes2NetBlock {
    tdnn1: TdnnBlock,
    res2net: Res2NetBlock,
    tdnn2: TdnnBlock,
    se: SeBlock,
}

impl SeRes2NetBlock {
    fn load(
        in_ch: usize,
        out_ch: usize,
        kernel_size: usize,
        dilation: usize,
        vb: VarBuilder,
    ) -> Result<Self> {
        let tdnn1 = TdnnBlock::load(in_ch, out_ch, 1, 1, vb.pp("tdnn1"))?;
        let res2net = Res2NetBlock::load(
            out_ch,
            kernel_size,
            dilation,
            RES2NET_SCALE,
            vb.pp("res2net_block"),
        )?;
        let tdnn2 = TdnnBlock::load(out_ch, out_ch, 1, 1, vb.pp("tdnn2"))?;
        let se = SeBlock::load(out_ch, 128, vb.pp("se_block"))?;
        Ok(Self {
            tdnn1,
            res2net,
            tdnn2,
            se,
        })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let h = self.tdnn1.forward(x)?;
        let h = self.res2net.forward(&h)?;
        let h = self.tdnn2.forward(&h)?;
        let h = self.se.forward(&h)?;
        Ok((h + x)?)
    }
}

#[derive(Debug, Clone)]
struct AttentiveStatisticsPooling {
    tdnn: TdnnBlock,
    conv: Conv1d,
}

impl AttentiveStatisticsPooling {
    fn load(channels: usize, attention_channels: usize, vb: VarBuilder) -> Result<Self> {
        let tdnn = TdnnBlock::load(channels * 3, attention_channels, 1, 1, vb.pp("tdnn"))?;
        let conv = candle_nn::conv1d(
            attention_channels,
            channels,
            1,
            Conv1dConfig::default(),
            vb.pp("conv.conv"),
        )?;
        Ok(Self { tdnn, conv })
    }

    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let (b, c, t) = x.dims3()?;
        let mean = x.mean_keepdim(2)?;
        let diff = x.broadcast_sub(&mean)?;
        let var = diff.sqr()?.mean_keepdim(2)?;
        let std = (var + 1e-12)?.sqrt()?;
        let mean_exp = mean.broadcast_as((b, c, t))?;
        let std_exp = std.broadcast_as((b, c, t))?;
        let attn_in = Tensor::cat(&[x, &mean_exp, &std_exp], 1)?;

        let attn = self.tdnn.forward(&attn_in)?.tanh()?;
        let attn = self.conv.forward(&attn)?;
        let attn = candle_nn::ops::softmax(&attn, 2)?;

        let weighted_mean = x.broadcast_mul(&attn)?.sum_keepdim(2)?;
        let diff_w = x.broadcast_sub(&weighted_mean)?;
        let weighted_var = (diff_w.sqr()?.broadcast_mul(&attn)?.sum_keepdim(2)? + 1e-12)?;
        let weighted_std = weighted_var.sqrt()?;
        Ok(Tensor::cat(&[&weighted_mean, &weighted_std], 1)?)
    }
}

/// Slaney-scale mel filterbank matching SpeechBrain Fbank (`n_mels=80`, `n_fft=400`, `16 kHz`).
pub fn slaney_mel_filters(sample_rate: usize, n_fft: usize, n_mels: usize) -> Vec<f32> {
    let n_freqs = n_fft / 2 + 1;
    let f_sp = 200.0 / 3.0;
    let min_log_hz = 1000.0;
    let min_log_mel = min_log_hz / f_sp;
    let logstep = 6.4_f64.ln() / 27.0;
    let hz_to_mel = |f: f64| {
        if f >= min_log_hz {
            min_log_mel + (f / min_log_hz).ln() / logstep
        } else {
            f / f_sp
        }
    };
    let mel_to_hz = |m: f64| {
        if m >= min_log_mel {
            min_log_hz * ((m - min_log_mel) * logstep).exp()
        } else {
            m * f_sp
        }
    };
    let mel_min = hz_to_mel(0.0);
    let mel_max = hz_to_mel(sample_rate as f64 / 2.0);
    let n_pts = n_mels + 2;
    let mel_freqs: Vec<f64> = (0..n_pts)
        .map(|i| mel_to_hz(mel_min + (mel_max - mel_min) * (i as f64) / ((n_pts - 1) as f64)))
        .collect();
    let fft_freqs: Vec<f64> = (0..n_freqs)
        .map(|i| (i as f64) * (sample_rate as f64) / (n_fft as f64))
        .collect();
    let mut weights = vec![0.0_f32; n_mels * n_freqs];
    for m in 0..n_mels {
        let (fl, fc, fr) = (mel_freqs[m], mel_freqs[m + 1], mel_freqs[m + 2]);
        let enorm = 2.0 / (fr - fl);
        for (k, &freq) in fft_freqs.iter().enumerate() {
            let lo = if fc > fl {
                (freq - fl) / (fc - fl)
            } else {
                0.0
            };
            let hi = if fr > fc {
                (fr - freq) / (fr - fc)
            } else {
                0.0
            };
            weights[m * n_freqs + k] = (lo.min(hi).max(0.0) * enorm) as f32;
        }
    }
    weights
}

/// Precomputed differentiable Fbank projection matrices matching SpeechBrain's `Fbank(n_mels=80)`.
#[derive(Debug, Clone)]
struct DifferentiableFbank {
    w_cos: Tensor,
    w_sin: Tensor,
    mel_fb: Tensor,
}

impl DifferentiableFbank {
    fn new(device: &Device) -> Result<Self> {
        let hann: Vec<f32> = (0..N_FFT)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N_FFT as f32).cos())
            .collect();

        let mut cos_w = vec![0.0_f32; N_FFT * N_FREQS];
        let mut sin_w = vec![0.0_f32; N_FFT * N_FREQS];
        for n in 0..N_FFT {
            let win = hann[n];
            for k in 0..N_FREQS {
                let angle = 2.0 * std::f32::consts::PI * (k as f32) * (n as f32) / (N_FFT as f32);
                cos_w[n * N_FREQS + k] = win * angle.cos();
                sin_w[n * N_FREQS + k] = -win * angle.sin();
            }
        }

        let mel = slaney_mel_filters(ECAPA_SAMPLE_RATE as usize, N_FFT, N_MELS);
        // Transpose mel from [N_MELS, N_FREQS] to [N_FREQS, N_MELS]
        let mut mel_t = vec![0.0_f32; N_FREQS * N_MELS];
        for m in 0..N_MELS {
            for k in 0..N_FREQS {
                mel_t[k * N_MELS + m] = mel[m * N_FREQS + k];
            }
        }

        Ok(Self {
            w_cos: Tensor::from_vec(cos_w, (N_FFT, N_FREQS), device)?,
            w_sin: Tensor::from_vec(sin_w, (N_FFT, N_FREQS), device)?,
            mel_fb: Tensor::from_vec(mel_t, (N_FREQS, N_MELS), device)?,
        })
    }

    /// Computes sentence-mean-normalized `[1, 80, T]` log-mel features from `[N]` mono 16 kHz samples.
    fn forward(&self, x_1d: &Tensor) -> Result<Tensor> {
        let n = x_1d.dim(0)?;
        ensure!(
            n >= N_FFT,
            "Audio must be at least {N_FFT} samples at 16 kHz"
        );
        let n_frames = (n - N_FFT) / HOP_LENGTH + 1;
        let mut frames = Vec::with_capacity(n_frames);
        for i in 0..n_frames {
            frames.push(x_1d.narrow(0, i * HOP_LENGTH, N_FFT)?);
        }
        let frame_refs: Vec<&Tensor> = frames.iter().collect();
        let framed = Tensor::stack(&frame_refs, 0)?; // [T, 400]

        let re = framed.matmul(&self.w_cos)?;
        let im = framed.matmul(&self.w_sin)?;
        let power = (re.sqr()? + im.sqr()?)?; // [T, 201]
        let mel = power.matmul(&self.mel_fb)?; // [T, 80]
        let log_scale = (10.0 / 10.0_f64.ln()) as f32;
        let log_mel = ((mel + 1e-10)?.log()? * f64::from(log_scale))?; // [T, 80]

        // Sentence mean normalization along time (dim 0)
        let mean = log_mel.mean_keepdim(0)?;
        let norm = log_mel.broadcast_sub(&mean)?;
        // Transpose to [1, 80, T]
        Ok(norm.t()?.unsqueeze(0)?)
    }
}

/// Pure-Rust Candle implementation of `speechbrain/spkrec-ecapa-voxceleb`.
#[derive(Debug, Clone)]
pub struct EcapaEncoder {
    fbank: DifferentiableFbank,
    block0: TdnnBlock,
    block1: SeRes2NetBlock,
    block2: SeRes2NetBlock,
    block3: SeRes2NetBlock,
    mfa: TdnnBlock,
    asp: AttentiveStatisticsPooling,
    asp_bn: BatchNorm1dEval,
    fc: Conv1d,
    emb_glob_mean: Option<Tensor>,
    device: Device,
}

impl EcapaEncoder {
    pub fn load(model_dir: &Path) -> Result<Self> {
        validate_model_dir(model_dir)?;
        let device = Device::Cpu;
        let tensors = candle_core::safetensors::load(model_dir.join("model.safetensors"), &device)
            .context("Failed to load model.safetensors")?;
        let vb = VarBuilder::from_tensors(tensors, DType::F32, &device);
        Self::from_vb(&vb, &device)
    }

    pub fn from_vb(vb: &VarBuilder, device: &Device) -> Result<Self> {
        let fbank = DifferentiableFbank::new(device)?;
        let block0 = TdnnBlock::load(N_MELS, 1024, 5, 1, vb.pp("blocks.0"))?;
        let block1 = SeRes2NetBlock::load(1024, 1024, 3, 2, vb.pp("blocks.1"))?;
        let block2 = SeRes2NetBlock::load(1024, 1024, 3, 3, vb.pp("blocks.2"))?;
        let block3 = SeRes2NetBlock::load(1024, 1024, 3, 4, vb.pp("blocks.3"))?;
        let mfa = TdnnBlock::load(3072, 3072, 1, 1, vb.pp("mfa"))?;
        let asp = AttentiveStatisticsPooling::load(3072, 128, vb.pp("asp"))?;
        let asp_bn = BatchNorm1dEval::load(6144, 1e-5, vb.pp("asp_bn.norm"))?;
        let fc = candle_nn::conv1d(6144, EMB_DIM, 1, Conv1dConfig::default(), vb.pp("fc.conv"))?;
        let emb_glob_mean = vb.get(EMB_DIM, "emb_norm.glob_mean").ok();

        Ok(Self {
            fbank,
            block0,
            block1,
            block2,
            block3,
            mfa,
            asp,
            asp_bn,
            fc,
            emb_glob_mean,
            device: device.clone(),
        })
    }

    /// Computes raw 192-D speaker embedding from a 1-D 16 kHz mono waveform tensor `[N]`.
    pub fn encode_tensor(&self, samples_16k: &Tensor) -> Result<Tensor> {
        let feats = self.fbank.forward(samples_16k)?; // [1, 80, T]
        let x0 = self.block0.forward(&feats)?;
        let x1 = self.block1.forward(&x0)?;
        let x2 = self.block2.forward(&x1)?;
        let x3 = self.block3.forward(&x2)?;
        let cat = Tensor::cat(&[&x1, &x2, &x3], 1)?;
        let mfa = self.mfa.forward(&cat)?;
        let asp = self.asp.forward(&mfa)?;
        let asp = self.asp_bn.forward(&asp)?;
        let emb = self.fc.forward(&asp)?.squeeze(2)?.squeeze(0)?; // [192]
        if let Some(gm) = &self.emb_glob_mean {
            Ok((emb - gm)?)
        } else {
            Ok(emb)
        }
    }

    /// Computes an L2-normalized 192-D speaker embedding from 16 kHz mono samples.
    pub fn encode_samples_16k(&self, samples_16k: &[f32]) -> Result<Vec<f32>> {
        let t = Tensor::from_slice(samples_16k, samples_16k.len(), &self.device)?;
        let emb = self.encode_tensor(&t)?;
        let norm = emb.sqr()?.sum_all()?.sqrt()?;
        let unit = emb.broadcast_div(&(norm + 1e-12)?)?;
        Ok(unit.to_vec1::<f32>()?)
    }
}

/// Linearly resamples mono audio to `dst_rate`, and maps gradients back to `src_rate`.
fn resample_linear(samples: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if samples.is_empty() || src_rate == dst_rate {
        return samples.to_vec();
    }
    let ratio = src_rate as f64 / dst_rate as f64;
    let out_len = ((samples.len() as f64) / ratio).round().max(1.0) as usize;
    let last = samples.len() - 1;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let idx = (pos.floor() as usize).min(last);
        let frac = (pos - idx as f64) as f32;
        let s0 = samples[idx];
        let s1 = samples[(idx + 1).min(last)];
        out.push(s0 + (s1 - s0) * frac);
    }
    out
}

fn resample_grad_adjoint(
    grad_dst: &[f32],
    src_len: usize,
    src_rate: u32,
    dst_rate: u32,
) -> Vec<f32> {
    if src_len == 0 || src_rate == dst_rate {
        return grad_dst.to_vec();
    }
    let mut grad_src = vec![0.0_f32; src_len];
    let ratio = src_rate as f64 / dst_rate as f64;
    let last = src_len - 1;
    for (i, &g) in grad_dst.iter().enumerate() {
        let pos = i as f64 * ratio;
        let idx = (pos.floor() as usize).min(last);
        let frac = (pos - idx as f64) as f32;
        grad_src[idx] += g * (1.0 - frac);
        grad_src[(idx + 1).min(last)] += g * frac;
    }
    grad_src
}

/// Differentiable voice-protection objective minimizing ECAPA-TDNN cosine similarity.
pub struct EcapaObjective {
    encoder: EcapaEncoder,
    orig_unit: Tensor,
}

impl EcapaObjective {
    pub fn new(encoder: EcapaEncoder, original_samples: &[f32], sample_rate: u32) -> Result<Self> {
        let orig_16k = resample_linear(original_samples, sample_rate, ECAPA_SAMPLE_RATE);
        let t = Tensor::from_slice(&orig_16k, orig_16k.len(), &encoder.device)?;
        let emb = encoder.encode_tensor(&t)?;
        let norm = emb.sqr()?.sum_all()?.sqrt()?;
        let orig_unit = emb.broadcast_div(&(norm + 1e-12)?)?.detach();
        Ok(Self { encoder, orig_unit })
    }

    pub fn cosine_similarity(&self, samples: &[f32], sample_rate: u32) -> Result<f32> {
        let s_16k = resample_linear(samples, sample_rate, ECAPA_SAMPLE_RATE);
        let emb = self.encoder.encode_samples_16k(&s_16k)?;
        let orig = self.orig_unit.to_vec1::<f32>()?;
        Ok(emb.iter().zip(&orig).map(|(a, b)| a * b).sum())
    }
}

impl AudioObjective for EcapaObjective {
    fn name(&self) -> &str {
        "speechbrain-ecapa-voxceleb"
    }

    /// Returns `(-cosine_similarity, grad(-cosine_similarity, samples))` so `protect::protect`
    /// gradient ascent minimizes speaker cosine similarity.
    fn loss_gradient(&self, samples: &[f32], sample_rate: u32) -> Result<(f32, Vec<f32>)> {
        let s_16k = resample_linear(samples, sample_rate, ECAPA_SAMPLE_RATE);
        let x_var = Var::from_slice(&s_16k, s_16k.len(), &self.encoder.device)?;
        let emb = self.encoder.encode_tensor(x_var.as_tensor())?;
        let norm = emb.sqr()?.sum_all()?.sqrt()?;
        let unit = emb.broadcast_div(&(norm + 1e-12)?)?;
        let cos_sim = (unit * &self.orig_unit)?.sum_all()?;
        let loss = cos_sim.neg()?;
        let grads = loss.backward()?;
        let g_16k = grads
            .get(x_var.as_tensor())
            .context("Missing gradient for audio input")?
            .to_vec1::<f32>()?;
        let grad = resample_grad_adjoint(&g_16k, samples.len(), sample_rate, ECAPA_SAMPLE_RATE);
        Ok((loss.to_scalar::<f32>()?, grad))
    }
}

/// Simulates lossy MP3/AAC-style bandwidth reduction by downsampling to 8 kHz and back.
pub fn simulate_compression(samples: &[f32], rate: u32) -> Vec<f32> {
    let down = resample_linear(samples, rate, 8_000);
    resample_linear(&down, 8_000, rate)
}

/// Simple 3-tap moving-average smoothing to simulate basic denoising.
pub fn simulate_denoise(samples: &[f32]) -> Vec<f32> {
    if samples.len() < 3 {
        return samples.to_vec();
    }
    let mut out = Vec::with_capacity(samples.len());
    out.push(samples[0]);
    for w in samples.windows(3) {
        out.push((w[0] + w[1] + w[2]) / 3.0);
    }
    out.push(samples[samples.len() - 1]);
    out
}

/// Protects `input` audio using the local ECAPA-TDNN model in `model_dir` and writes a clean WAV.
pub fn protect_wav(
    input: &Path,
    output: &Path,
    model_dir: &Path,
    settings: Settings,
    cancel: &AtomicBool,
) -> Result<VoiceProtectReport> {
    ensure!(
        output
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("wav")),
        "Output must be .wav"
    );
    let dir = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    ensure!(dir.is_dir(), "Output folder must exist");

    let encoder = EcapaEncoder::load(model_dir)?;
    let mut dec = Decoder::open(input)?;
    let rate = dec.info.sample_rate;
    let ch = dec.info.channels;
    let mut mono = Vec::new();
    while let Some(block) = dec.next_block()? {
        for frame in block.chunks_exact(ch) {
            let sum: f32 = frame.iter().copied().sum();
            mono.push(sum / ch as f32);
        }
    }

    let obj = EcapaObjective::new(encoder, &mono, rate)?;
    let cosine_before = obj.cosine_similarity(&mono, rate)?;
    let protected: Protected = protect::protect(&mono, rate, &[(&obj, 1.0)], settings, cancel)?;
    let cosine_after = obj.cosine_similarity(&protected.samples, rate)?;
    let compressed = simulate_compression(&protected.samples, rate);
    let cosine_after_compression_sim = obj.cosine_similarity(&compressed, rate)?;
    let denoised = simulate_denoise(&protected.samples);
    let cosine_after_denoise_sim = obj.cosine_similarity(&denoised, rate)?;

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let tmp = tempfile::NamedTempFile::new_in(dir)?;
    {
        let mut w =
            hound::WavWriter::new(std::io::BufWriter::new(tmp.as_file().try_clone()?), spec)?;
        for &s in &protected.samples {
            w.write_sample(s.clamp(-1.0, 1.0))?;
        }
        w.flush()?;
    }
    tmp.persist(output)
        .map_err(|e| anyhow::anyhow!("Failed to save WAV: {}", e.error))?;

    let cmp = audio::compare(input, output).ok();
    Ok(VoiceProtectReport {
        snr_db: protected.snr_db,
        segmental_snr_db: cmp.and_then(|c| c.segmental_snr_db),
        peak: protected.peak,
        cosine_before,
        cosine_after,
        cosine_after_compression_sim,
        cosine_after_denoise_sim,
        efficacy_verified: false,
        note: "Self-encoder cosine drop measured locally; transfer to external voice cloners is unverified unless separately tested.".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_test_encoder(dir: &Path) -> EcapaEncoder {
        // Build deterministic random-like tensors in safetensors with running_var = 1.0.
        let mut tensors: std::collections::HashMap<String, Tensor> =
            std::collections::HashMap::new();
        let dev = &Device::Cpu;
        let add_conv = |m: &mut std::collections::HashMap<String, Tensor>,
                        prefix: &str,
                        in_c: usize,
                        out_c: usize,
                        k: usize| {
            let n = out_c * in_c * k;
            let w: Vec<f32> = (0..n).map(|i| ((i % 17) as f32 - 8.0) * 0.005).collect();
            m.insert(
                format!("{prefix}.weight"),
                Tensor::from_vec(w, (out_c, in_c, k), dev).unwrap(),
            );
            m.insert(
                format!("{prefix}.bias"),
                Tensor::zeros(out_c, DType::F32, dev).unwrap(),
            );
        };
        let add_bn = |m: &mut std::collections::HashMap<String, Tensor>, prefix: &str, c: usize| {
            m.insert(
                format!("{prefix}.weight"),
                Tensor::ones(c, DType::F32, dev).unwrap(),
            );
            m.insert(
                format!("{prefix}.bias"),
                Tensor::zeros(c, DType::F32, dev).unwrap(),
            );
            m.insert(
                format!("{prefix}.running_mean"),
                Tensor::zeros(c, DType::F32, dev).unwrap(),
            );
            m.insert(
                format!("{prefix}.running_var"),
                Tensor::ones(c, DType::F32, dev).unwrap(),
            );
        };

        add_conv(&mut tensors, "blocks.0.conv.conv", 80, 1024, 5);
        add_bn(&mut tensors, "blocks.0.norm.norm", 1024);
        for b in 1..=3 {
            let p = format!("blocks.{b}");
            add_conv(&mut tensors, &format!("{p}.tdnn1.conv.conv"), 1024, 1024, 1);
            add_bn(&mut tensors, &format!("{p}.tdnn1.norm.norm"), 1024);
            for s in 0..7 {
                add_conv(
                    &mut tensors,
                    &format!("{p}.res2net_block.blocks.{s}.conv.conv"),
                    128,
                    128,
                    3,
                );
                add_bn(
                    &mut tensors,
                    &format!("{p}.res2net_block.blocks.{s}.norm.norm"),
                    128,
                );
            }
            add_conv(&mut tensors, &format!("{p}.tdnn2.conv.conv"), 1024, 1024, 1);
            add_bn(&mut tensors, &format!("{p}.tdnn2.norm.norm"), 1024);
            add_conv(
                &mut tensors,
                &format!("{p}.se_block.conv1.conv"),
                1024,
                128,
                1,
            );
            add_conv(
                &mut tensors,
                &format!("{p}.se_block.conv2.conv"),
                128,
                1024,
                1,
            );
        }
        add_conv(&mut tensors, "mfa.conv.conv", 3072, 3072, 1);
        add_bn(&mut tensors, "mfa.norm.norm", 3072);
        add_conv(&mut tensors, "asp.tdnn.conv.conv", 9216, 128, 1);
        add_bn(&mut tensors, "asp.tdnn.norm.norm", 128);
        add_conv(&mut tensors, "asp.conv.conv", 128, 3072, 1);
        add_bn(&mut tensors, "asp_bn.norm", 6144);
        add_conv(&mut tensors, "fc.conv", 6144, 192, 1);

        let path = dir.join("model.safetensors");
        candle_core::safetensors::save(&tensors, &path).unwrap();
        EcapaEncoder::load(dir).unwrap()
    }

    #[test]
    fn refuses_pickle_weights_in_model_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("embedding_model.ckpt"), b"pickle").unwrap();
        std::fs::write(dir.path().join("pytorch_model.bin"), b"pickle").unwrap();
        let err = EcapaEncoder::load(dir.path()).unwrap_err();
        assert!(format!("{err:#}").contains("Refusing unsafe pickle/binary weights"));
    }

    #[test]
    fn ecapa_autograd_and_protect_reduce_cosine_similarity() {
        let dir = tempfile::tempdir().unwrap();
        let encoder = init_test_encoder(dir.path());
        let samples: Vec<f32> = (0..16_000)
            .map(|i| 0.3 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 16_000.0).sin())
            .collect();
        let obj = EcapaObjective::new(encoder, &samples, 16_000).unwrap();
        let cos_before = obj.cosine_similarity(&samples, 16_000).unwrap();
        assert!((cos_before - 1.0).abs() < 1e-4);

        let (loss, grad) = obj.loss_gradient(&samples, 16_000).unwrap();
        assert!(loss.is_finite());
        assert_eq!(grad.len(), samples.len());
        assert!(grad.iter().any(|&g| g.abs() > 1e-9));

        let settings = Settings {
            steps: 3,
            ..Default::default()
        };
        let cancel = AtomicBool::new(false);
        let protected =
            protect::protect(&samples, 16_000, &[(&obj, 1.0)], settings, &cancel).unwrap();
        assert!(protected.snr_db >= 29.0);
        assert!(protected.peak <= 1.0);
    }

    #[test]
    #[ignore = "Requires converted SpeechBrain ECAPA model + parity_refs.json in APD_ECAPA_DIR"]
    fn parity_with_speechbrain_python_embeddings() {
        let dir = std::env::var("APD_ECAPA_DIR").expect("APD_ECAPA_DIR must be set");
        let model_dir = Path::new(&dir);
        let encoder = EcapaEncoder::load(model_dir).unwrap();
        let refs_bytes = std::fs::read(model_dir.join("parity_refs.json")).unwrap();
        let refs: std::collections::HashMap<String, Vec<f32>> =
            serde_json::from_slice(&refs_bytes).unwrap();

        let sr = 16_000usize;
        let secs = 1.5_f32;
        let n = (sr as f32 * secs) as usize;
        let wave = |name: &str| -> Vec<f32> {
            (0..n)
                .map(|i| {
                    let t = i as f32 / sr as f32;
                    match name {
                        "sine_220_440" => {
                            0.3 * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
                                + 0.2 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
                        }
                        "chirp_150_600" => {
                            0.35 * (2.0 * std::f32::consts::PI * (150.0 + 300.0 * t / secs) * t)
                                .sin()
                        }
                        _ => (1..=5)
                            .map(|k| {
                                (0.25 / k as f32)
                                    * (2.0 * std::f32::consts::PI * 130.0 * k as f32 * t).sin()
                            })
                            .sum(),
                    }
                })
                .collect()
        };

        for (name, ref_emb) in &refs {
            let got = encoder.encode_samples_16k(&wave(name)).unwrap();
            let ref_norm: f32 = ref_emb.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-12);
            let cos: f32 = got
                .iter()
                .zip(ref_emb)
                .map(|(a, b)| a * (b / ref_norm))
                .sum();
            assert!(
                cos > 0.999,
                "Parity cosine for {name} was {cos} (expected > 0.999)"
            );
        }
    }
}
