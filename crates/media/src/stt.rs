//! Local speech-to-text transcription and private-content detection using OpenAI Whisper
//! via `candle-transformers` (optional `stt` feature).
//!
//! Security and privacy invariants:
//! - Weights are loaded strictly from a local directory (`model.safetensors`, `config.json`,
//!   `tokenizer.json`). Nothing is ever downloaded at runtime; no network code exists.
//! - Pickle / `.bin` / `.pt` / `.pth` weights are explicitly refused.
//! - Transcript text is never logged.

use crate::audio::{self, Decoder, RedactKind};
use anyhow::{Context, Result, bail, ensure};
use candle_core::{DType, Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{
    self as m, Config, EOT_TOKEN, N_FFT, N_FRAMES, N_SAMPLES, NO_TIMESTAMPS_TOKEN, SAMPLE_RATE,
    SOT_TOKEN, TRANSCRIBE_TOKEN, model::Whisper,
};
use photo_privacy::{Country, Source, TextSpan};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokenizers::Tokenizer;

/// Padding added around each flagged transcript segment when generating bleep ranges.
pub const REDACT_PAD_S: f64 = 0.2;

/// A transcribed speech segment with start/end timestamps in seconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub start_s: f64,
    pub end_s: f64,
    pub text: String,
}

/// Validates that `model_dir` contains local safetensors Whisper weights and refuses pickle/bin files.
fn validate_model_dir(model_dir: &Path) -> Result<()> {
    ensure!(
        model_dir.is_dir(),
        "Whisper model path must be a local directory: {}",
        model_dir.display()
    );

    // Refuse pickle / PyTorch .bin weights if present without safetensors, or if any forbidden
    // weight format is explicitly targeted.
    let forbidden_exts = ["bin", "pt", "pth", "pkl", "pickle"];
    if let Ok(entries) = std::fs::read_dir(model_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && let Some(ext) = path.extension().and_then(|e| e.to_str())
                && forbidden_exts.iter().any(|f| ext.eq_ignore_ascii_case(f))
            {
                bail!(
                    "Refusing unsafe pickle/binary weights file ({}); only model.safetensors is permitted",
                    path.display()
                );
            }
        }
    }

    for required in ["model.safetensors", "config.json", "tokenizer.json"] {
        let p = model_dir.join(required);
        ensure!(
            p.is_file(),
            "Missing required Whisper model file: {}",
            p.display()
        );
    }
    Ok(())
}

/// Downmixes interleaved multi-channel PCM to mono by averaging channels.
pub fn downmix_mono(interleaved: &[f32], channels: usize) -> Result<Vec<f32>> {
    ensure!(channels >= 1, "Channel count must be >= 1");
    if channels == 1 {
        return Ok(interleaved.to_vec());
    }
    let frames = interleaved.len() / channels;
    let mut mono = Vec::with_capacity(frames);
    let inv = 1.0 / channels as f32;
    for chunk in interleaved.chunks_exact(channels) {
        let sum: f32 = chunk.iter().copied().sum();
        mono.push(sum * inv);
    }
    Ok(mono)
}

/// Resamples a mono f32 signal from `src_rate` to `dst_rate` using linear interpolation.
pub fn resample_mono(samples: &[f32], src_rate: u32, dst_rate: u32) -> Result<Vec<f32>> {
    ensure!(src_rate > 0 && dst_rate > 0, "Sample rates must be > 0");
    if samples.is_empty() || src_rate == dst_rate {
        return Ok(samples.to_vec());
    }
    let ratio = src_rate as f64 / dst_rate as f64;
    let out_len = ((samples.len() as f64) / ratio).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(out_len);
    let last = samples.len() - 1;
    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = (src_pos.floor() as usize).min(last);
        let frac = (src_pos - idx as f64) as f32;
        let s0 = samples[idx];
        let s1 = samples[(idx + 1).min(last)];
        out.push(s0 + (s1 - s0) * frac);
    }
    Ok(out)
}

/// Computes Slaney-scale mel filterbank weights matching OpenAI Whisper (`librosa.filters.mel`),
/// returning a row-major buffer of length `n_mels * (n_fft / 2 + 1)`.
pub fn whisper_mel_filters(sample_rate: usize, n_fft: usize, n_mels: usize) -> Vec<f32> {
    let n_freqs = n_fft / 2 + 1;
    let f_min = 0.0_f64;
    let f_max = sample_rate as f64 / 2.0;

    let f_sp = 200.0 / 3.0;
    let min_log_hz = 1000.0;
    let min_log_mel = min_log_hz / f_sp;
    let logstep = 6.4_f64.ln() / 27.0;

    let hz_to_mel = |f: f64| -> f64 {
        if f >= min_log_hz {
            min_log_mel + (f / min_log_hz).ln() / logstep
        } else {
            f / f_sp
        }
    };
    let mel_to_hz = |m: f64| -> f64 {
        if m >= min_log_mel {
            min_log_hz * ((m - min_log_mel) * logstep).exp()
        } else {
            m * f_sp
        }
    };

    let mel_min = hz_to_mel(f_min);
    let mel_max = hz_to_mel(f_max);
    let n_pts = n_mels + 2;
    let mel_freqs: Vec<f64> = (0..n_pts)
        .map(|i| {
            let m = mel_min + (mel_max - mel_min) * (i as f64) / ((n_pts - 1) as f64);
            mel_to_hz(m)
        })
        .collect();

    let fft_freqs: Vec<f64> = (0..n_freqs)
        .map(|i| (i as f64) * (sample_rate as f64) / (n_fft as f64))
        .collect();

    let mut weights = vec![0.0_f32; n_mels * n_freqs];
    for m in 0..n_mels {
        let f_left = mel_freqs[m];
        let f_center = mel_freqs[m + 1];
        let f_right = mel_freqs[m + 2];
        let enorm = 2.0 / (f_right - f_left);
        let lower_slope = f_center - f_left;
        let upper_slope = f_right - f_center;

        for (k, &freq) in fft_freqs.iter().enumerate() {
            let lower = if lower_slope > 0.0 {
                (freq - f_left) / lower_slope
            } else {
                0.0
            };
            let upper = if upper_slope > 0.0 {
                (f_right - freq) / upper_slope
            } else {
                0.0
            };
            let w = lower.min(upper).max(0.0) * enorm;
            weights[m * n_freqs + k] = w as f32;
        }
    }
    weights
}

/// Computes a Whisper log-mel spectrogram tensor of shape `[1, num_mel_bins, 3000]`
/// for a 16 kHz mono audio chunk (zero-padded or truncated to 30 seconds).
pub fn log_mel_tensor(
    samples_16k: &[f32],
    cfg: &Config,
    filters: &[f32],
    device: &Device,
) -> Result<Tensor> {
    let mut padded = vec![0.0_f32; N_SAMPLES];
    let copy_len = samples_16k.len().min(N_SAMPLES);
    padded[..copy_len].copy_from_slice(&samples_16k[..copy_len]);
    let mel = m::audio::pcm_to_mel(cfg, &padded, filters);
    let total_frames = mel.len() / cfg.num_mel_bins;
    ensure!(
        total_frames >= N_FRAMES,
        "Unexpected mel frame count {total_frames} < {N_FRAMES}"
    );
    let full = Tensor::from_vec(mel, (1, cfg.num_mel_bins, total_frames), device)
        .context("Failed to construct log-mel tensor")?;
    full.narrow(2, 0, N_FRAMES)
        .context("Failed to slice 3000-frame log-mel tensor")
}

/// Greedy-decodes one 30-second chunk with timestamp tokens and returns timestamped `Segment`s.
fn decode_chunk(
    model: &mut Whisper,
    tokenizer: &Tokenizer,
    mel: &Tensor,
    chunk_offset_s: f64,
    chunk_duration_s: f64,
    device: &Device,
) -> Result<Vec<Segment>> {
    let token_id = |tok: &str| -> Result<u32> {
        tokenizer
            .token_to_id(tok)
            .with_context(|| format!("Tokenizer missing required token {tok}"))
    };

    let sot = token_id(SOT_TOKEN)?;
    let transcribe_tok = token_id(TRANSCRIBE_TOKEN)?;
    let eot = token_id(EOT_TOKEN)?;
    let no_timestamps = token_id(NO_TIMESTAMPS_TOKEN)?;
    let timestamp_begin = no_timestamps + 1;

    let mut prompt = vec![sot];
    if let Some(en) = tokenizer.token_to_id("<|en|>")
        && model.config.vocab_size >= 51865
    {
        prompt.push(en);
    }
    prompt.push(transcribe_tok);

    model.reset_kv_cache();
    let audio_features = model.encoder.forward(mel, true)?;

    let max_decode = model.config.max_target_positions / 2;
    let mut tokens = prompt.clone();
    let mut generated = Vec::new();

    for step in 0..max_decode {
        // candle's Whisper decoder caches only the audio cross-attention and derives token
        // positions from the input length, so the full sequence is passed on every step.
        let x = Tensor::new(tokens.as_slice(), device)?.unsqueeze(0)?;
        let ys = model.decoder.forward(&x, &audio_features, step == 0)?;
        let (_, seq_len, _) = ys.dims3()?;
        let logits = model.decoder.final_linear(&ys.i((..1, seq_len - 1..))?)?;
        let mut logits_vec = logits.squeeze(0)?.squeeze(0)?.to_vec1::<f32>()?;

        for &sup in &model.config.suppress_tokens {
            if let Some(v) = logits_vec.get_mut(sup as usize) {
                *v = f32::NEG_INFINITY;
            }
        }
        if let Some(v) = logits_vec.get_mut(no_timestamps as usize) {
            *v = f32::NEG_INFINITY;
        }
        apply_timestamp_rules(&mut logits_vec, &generated, timestamp_begin, eot);

        let next_token = logits_vec
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(idx, _)| idx as u32)
            .context("Empty logits vector")?;

        if next_token == eot {
            break;
        }
        tokens.push(next_token);
        generated.push(next_token);
        if let Some(cut) = repetition_cut(&generated, timestamp_begin) {
            generated.truncate(cut);
            break;
        }
    }

    Ok(parse_timestamped_tokens(
        &generated,
        timestamp_begin,
        tokenizer,
        chunk_offset_s,
        chunk_duration_s,
    ))
}

/// Converts generated Whisper token IDs (including timestamp tokens) into `Segment`s.
/// Whisper's timestamp rules (as in the reference decoder): the output starts with a timestamp,
/// timestamps come in pairs and never decrease, and a timestamp is chosen whenever timestamps are
/// collectively more likely than any single text token. Without them the model rarely emits
/// timestamps and segments span whole 30-second windows.
fn apply_timestamp_rules(logits: &mut [f32], generated: &[u32], ts_begin: u32, eot: u32) {
    let tb = ts_begin as usize;
    if tb >= logits.len() {
        return;
    }
    let is_ts = |t: u32| t >= ts_begin;
    let last = generated.last().copied();
    let penult = generated.len().checked_sub(2).map(|i| generated[i]);
    if generated.is_empty() {
        // First token must be a timestamp (and no later than 1 s).
        for v in logits[..tb].iter_mut() {
            *v = f32::NEG_INFINITY;
        }
        for v in logits.iter_mut().skip(tb + 50) {
            *v = f32::NEG_INFINITY;
        }
        return;
    }
    if let Some(l) = last
        && is_ts(l)
    {
        // As in the reference decoder, "no penultimate token" counts as a timestamp, so the
        // opening timestamp is followed by text.
        if penult.is_none_or(is_ts) {
            // A pair just closed: the next token must be text (or end).
            for v in logits[tb..].iter_mut() {
                *v = f32::NEG_INFINITY;
            }
        } else {
            // An open timestamp: the next token must be a timestamp or end of text.
            for (i, v) in logits[..tb].iter_mut().enumerate() {
                if i as u32 != eot {
                    *v = f32::NEG_INFINITY;
                }
            }
        }
    }
    // Timestamps never go backwards.
    if let Some(&max_ts) = generated.iter().filter(|&&t| is_ts(t)).max() {
        let upto = (max_ts as usize).min(logits.len());
        for v in logits[tb..upto].iter_mut() {
            *v = f32::NEG_INFINITY;
        }
    }
    // Prefer a timestamp when the timestamp mass beats the best text token (log-space).
    let max_all = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if !max_all.is_finite() {
        return;
    }
    let lse = |xs: &[f32]| -> f32 {
        let s: f32 = xs.iter().map(|x| (x - max_all).exp()).sum();
        s.ln() + max_all
    };
    let ts_lse = lse(&logits[tb..]);
    let text_max = logits[..tb]
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    if ts_lse > text_max {
        for v in logits[..tb].iter_mut() {
            *v = f32::NEG_INFINITY;
        }
    }
}

/// Detects a decoding loop (the same run of 3-12 text tokens repeated three times in a row at the
/// end) and returns where the first repeat starts, so the repeats can be dropped.
fn repetition_cut(generated: &[u32], ts_begin: u32) -> Option<usize> {
    for n in 3..=12usize {
        if generated.len() < 3 * n {
            continue;
        }
        let end = generated.len();
        let a = &generated[end - n..];
        let b = &generated[end - 2 * n..end - n];
        let c = &generated[end - 3 * n..end - 2 * n];
        if a == b && b == c && a.iter().any(|&t| t < ts_begin) {
            return Some(end - 2 * n);
        }
    }
    None
}

fn parse_timestamped_tokens(
    tokens: &[u32],
    timestamp_begin: u32,
    tokenizer: &Tokenizer,
    chunk_offset_s: f64,
    chunk_duration_s: f64,
) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut current_start: Option<f64> = None;
    let mut text_tokens: Vec<u32> = Vec::new();

    let flush_segment = |start: f64, end: f64, toks: &mut Vec<u32>, out: &mut Vec<Segment>| {
        if toks.is_empty() {
            return;
        }
        if let Ok(decoded) = tokenizer.decode(toks, true) {
            let trimmed = decoded.trim().to_string();
            if !trimmed.is_empty() && end > start {
                out.push(Segment {
                    start_s: chunk_offset_s + start,
                    end_s: chunk_offset_s + end,
                    text: trimmed,
                });
            }
        }
        toks.clear();
    };

    for &tok in tokens {
        if tok >= timestamp_begin {
            let t = ((tok - timestamp_begin) as f64 * 0.02).min(chunk_duration_s);
            if let Some(start) = current_start {
                if !text_tokens.is_empty() {
                    flush_segment(start, t.max(start + 0.02), &mut text_tokens, &mut segments);
                    current_start = None;
                } else {
                    current_start = Some(t);
                }
            } else {
                current_start = Some(t);
            }
        } else {
            if current_start.is_none() {
                current_start = Some(0.0);
            }
            text_tokens.push(tok);
        }
    }

    if let Some(start) = current_start {
        flush_segment(
            start,
            chunk_duration_s.max(start + 0.02),
            &mut text_tokens,
            &mut segments,
        );
    }

    segments
}

/// Transcribes `audio` using local OpenAI Whisper weights in `model_dir`.
///
/// Decodes audio via `photo_media::audio::Decoder`, downmixes and resamples to 16 kHz mono,
/// computes log-mel spectrograms, and performs greedy decoding with timestamps.
/// Never downloads weights or accesses the network; refuses `.bin`/pickle weights.
pub fn transcribe(audio_path: &Path, model_dir: &Path) -> Result<Vec<Segment>> {
    validate_model_dir(model_dir)?;

    let config_bytes =
        std::fs::read(model_dir.join("config.json")).context("Failed to read config.json")?;
    let config: Config =
        serde_json::from_slice(&config_bytes).context("Failed to parse Whisper config.json")?;

    let tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json"))
        .map_err(|e| anyhow::anyhow!("Failed to load tokenizer.json: {e}"))?;

    let device = Device::Cpu;
    let tensors = candle_core::safetensors::load(model_dir.join("model.safetensors"), &device)
        .context("Failed to load model.safetensors")?;
    let vb = VarBuilder::from_tensors(tensors, DType::F32, &device);
    let mut model = Whisper::load(&vb, config.clone()).context("Failed to load Whisper model")?;

    let mut decoder = Decoder::open(audio_path)?;
    let src_rate = decoder.info.sample_rate;
    let channels = decoder.info.channels;
    let mut mono_samples = Vec::new();
    while let Some(block) = decoder.next_block()? {
        mono_samples.extend(downmix_mono(&block, channels)?);
    }

    let samples_16k = resample_mono(&mono_samples, src_rate, SAMPLE_RATE as u32)?;
    if samples_16k.is_empty() {
        return Ok(Vec::new());
    }

    let filters = whisper_mel_filters(SAMPLE_RATE, N_FFT, config.num_mel_bins);
    let mut all_segments = Vec::new();

    for (chunk_idx, chunk) in samples_16k.chunks(N_SAMPLES).enumerate() {
        let chunk_offset_s = (chunk_idx * N_SAMPLES) as f64 / SAMPLE_RATE as f64;
        let chunk_duration_s = chunk.len() as f64 / SAMPLE_RATE as f64;
        let mel = log_mel_tensor(chunk, &config, &filters, &device)?;
        let segs = decode_chunk(
            &mut model,
            &tokenizer,
            &mel,
            chunk_offset_s,
            chunk_duration_s,
            &device,
        )?;
        all_segments.extend(segs);
    }

    Ok(all_segments)
}

/// Scans transcript segments with `photo_privacy` text rules and returns padded `Bleep` ranges.
///
/// Each flagged segment is padded by `0.2` seconds on both sides (clamped at `0.0`), and
/// overlapping or adjacent ranges are merged. Never logs transcript text.
pub fn find_private(segments: &[Segment], countries: &[Country]) -> Vec<audio::Segment> {
    let mut raw_ranges: Vec<audio::Segment> = Vec::new();
    for seg in segments {
        if !seg.start_s.is_finite() || !seg.end_s.is_finite() || seg.end_s <= seg.start_s {
            continue;
        }
        let findings = photo_privacy::text::scan(
            &[TextSpan {
                text: seg.text.clone(),
                region: None,
                source: Source::Text,
            }],
            countries,
        );
        if !findings.is_empty() {
            raw_ranges.push(audio::Segment {
                start_s: (seg.start_s - REDACT_PAD_S).max(0.0),
                end_s: seg.end_s + REDACT_PAD_S,
                kind: RedactKind::Bleep,
            });
        }
    }

    raw_ranges.sort_by(|a, b| a.start_s.total_cmp(&b.start_s));
    let mut merged: Vec<audio::Segment> = Vec::with_capacity(raw_ranges.len());
    for seg in raw_ranges {
        if let Some(last) = merged.last_mut()
            && seg.start_s <= last.end_s
        {
            last.end_s = last.end_s.max(seg.end_s);
        } else {
            merged.push(seg);
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_rules_follow_reference_decoder() {
        let (tb, eot) = (100u32, 50u32);
        // First token: only early timestamps.
        let mut l = vec![0f32; 300];
        apply_timestamp_rules(&mut l, &[], tb, eot);
        assert!(
            l[..100].iter().all(|v| v.is_infinite()) && l[100].is_finite() && l[160].is_infinite()
        );
        // After the opening timestamp: text, not another timestamp.
        let mut l = vec![0f32; 300];
        apply_timestamp_rules(&mut l, &[110], tb, eot);
        assert!(l[10].is_finite() && l[120].is_infinite());
        // Open timestamp after text: timestamp or end of text only, never earlier than 110.
        let mut l = vec![0f32; 300];
        apply_timestamp_rules(&mut l, &[110, 5, 6, 120], tb, eot);
        assert!(l[5].is_infinite() && l[105].is_infinite());
        assert!(l[125].is_finite());
    }

    #[test]
    fn repetition_guard_cuts_loops() {
        let mut g = vec![100, 1, 2];
        for _ in 0..3 {
            g.extend([7, 8, 9, 10]);
        }
        assert_eq!(repetition_cut(&g, 100), Some(7));
        assert_eq!(repetition_cut(&[100, 1, 2, 3, 4, 5, 6], 100), None);
    }

    fn tiny_test_config() -> Config {
        Config {
            num_mel_bins: 80,
            max_source_positions: 1500,
            d_model: 384,
            encoder_attention_heads: 6,
            encoder_layers: 4,
            vocab_size: 51865,
            max_target_positions: 448,
            decoder_attention_heads: 6,
            decoder_layers: 4,
            suppress_tokens: vec![],
        }
    }

    #[test]
    fn mel_spectrogram_shape_is_1_80_3000() {
        let cfg = tiny_test_config();
        let filters = whisper_mel_filters(SAMPLE_RATE, N_FFT, cfg.num_mel_bins);
        assert_eq!(filters.len(), 80 * (N_FFT / 2 + 1));

        // 1 second of 440 Hz sine wave at 16 kHz.
        let samples: Vec<f32> = (0..SAMPLE_RATE)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * (i as f32) / SAMPLE_RATE as f32).sin())
            .collect();
        let tensor = log_mel_tensor(&samples, &cfg, &filters, &Device::Cpu).unwrap();
        assert_eq!(tensor.dims(), &[1, 80, N_FRAMES]);
        assert_eq!(N_FRAMES, 3000);
    }

    #[test]
    fn resampler_preserves_duration_and_dc() {
        // 48 kHz -> 16 kHz (3:1 downsampling)
        let src_rate = 48_000;
        let dst_rate = 16_000;
        let input = vec![0.42_f32; 48_000];
        let out = resample_mono(&input, src_rate, dst_rate).unwrap();
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|&x| (x - 0.42).abs() < 1e-5));

        // 44.1 kHz -> 16 kHz
        let input_44k = vec![0.25_f32; 44_100];
        let out_16k = resample_mono(&input_44k, 44_100, 16_000).unwrap();
        assert_eq!(out_16k.len(), 16_000);
    }

    #[test]
    fn find_private_flags_pii_and_pads_0_2s() {
        let segs = vec![
            Segment {
                start_s: 0.1,
                end_s: 1.5,
                text: "My email is alice@example.com".into(),
            },
            Segment {
                start_s: 2.0,
                end_s: 3.0,
                text: "The weather is sunny today".into(),
            },
            Segment {
                start_s: 4.0,
                end_s: 5.2,
                text: "Call me at 415-555-0199".into(),
            },
        ];

        let bleeps = find_private(&segs, &Country::ALL);
        assert_eq!(bleeps.len(), 2);
        assert_eq!(bleeps[0].kind, RedactKind::Bleep);
        assert!((bleeps[0].start_s - 0.0).abs() < 1e-9);
        assert!((bleeps[0].end_s - 1.7).abs() < 1e-9);

        assert_eq!(bleeps[1].kind, RedactKind::Bleep);
        assert!((bleeps[1].start_s - 3.8).abs() < 1e-9);
        assert!((bleeps[1].end_s - 5.4).abs() < 1e-9);
    }

    #[test]
    fn find_private_merges_overlapping_bleeps() {
        let segs = vec![
            Segment {
                start_s: 1.0,
                end_s: 2.0,
                text: "Email bob@example.org".into(),
            },
            Segment {
                start_s: 2.1,
                end_s: 3.0,
                text: "Name: Bob Smith".into(),
            },
        ];
        let bleeps = find_private(&segs, &Country::ALL);
        assert_eq!(bleeps.len(), 1);
        assert!((bleeps[0].start_s - 0.8).abs() < 1e-9);
        assert!((bleeps[0].end_s - 3.2).abs() < 1e-9);
    }

    #[test]
    fn refuses_pickle_bin_weights() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pytorch_model.bin"), b"not-a-real-pickle").unwrap();
        std::fs::write(dir.path().join("config.json"), b"{}").unwrap();
        std::fs::write(dir.path().join("tokenizer.json"), b"{}").unwrap();

        let wav = dir.path().join("dummy.wav");
        let err = transcribe(&wav, dir.path()).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("Refusing unsafe pickle/binary weights"));
    }

    #[test]
    #[ignore = "Requires local Whisper safetensors model directory in APD_WHISPER_DIR"]
    fn transcribe_with_local_whisper_model() {
        let model_dir =
            std::env::var("APD_WHISPER_DIR").expect("APD_WHISPER_DIR must be set for this test");
        let dir = tempfile::tempdir().unwrap();
        let wav_path = dir.path().join("silence.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        {
            let mut w = hound::WavWriter::create(&wav_path, spec).unwrap();
            for _ in 0..16_000 {
                w.write_sample(0_i16).unwrap();
            }
            w.finalize().unwrap();
        }
        let segments = transcribe(&wav_path, Path::new(&model_dir)).unwrap();
        // Verify timestamps are ordered and finite.
        for s in &segments {
            assert!(s.start_s.is_finite() && s.end_s.is_finite() && s.end_s >= s.start_s);
        }
    }
}
