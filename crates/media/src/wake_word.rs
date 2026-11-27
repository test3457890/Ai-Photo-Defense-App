//! Voice Assistant Safe Playback: detects wake phrases for Siri, Google, Alexa, Meta,
//! Bixby, and custom assistants from local speech transcripts, and applies either reliable
//! muting/bleeping or experimental speech-band suppression.
//!
//! IMPORTANT: Experimental band suppression has NOT been tested on physical voice-assistant
//! devices; use `SuppressionMode::Mute` or `SuppressionMode::Bleep` for reliable removal.

use crate::audio::{self, Decoder, RedactKind, Segment};
use anyhow::{Result, ensure};
use rustfft::{FftPlanner, num_complex::Complex};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssistantKind {
    Siri,
    Google,
    Alexa,
    Meta,
    Bixby,
    Custom(String),
}

impl AssistantKind {
    pub const BUILTINS: [AssistantKind; 5] = [
        AssistantKind::Siri,
        AssistantKind::Google,
        AssistantKind::Alexa,
        AssistantKind::Meta,
        AssistantKind::Bixby,
    ];

    fn phrases(&self) -> Vec<&str> {
        match self {
            Self::Siri => vec!["hey siri", "siri"],
            Self::Google => vec!["ok google", "okay google", "hey google"],
            Self::Alexa => vec!["alexa"],
            Self::Meta => vec!["hey meta"],
            Self::Bixby => vec!["hi bixby", "hey bixby", "bixby"],
            Self::Custom(p) => vec![p.as_str()],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuppressionMode {
    /// Reliable zeroing of the detected wake-phrase window (with crossfade).
    Mute,
    /// Reliable replacement of the detected wake-phrase window with a 1 kHz tone.
    Bleep,
    /// Experimental FFT formant-band (300–3400 Hz) attenuation inside the wake-phrase window.
    /// Not tested on physical devices; no effectiveness claim is made.
    ExperimentalBandSuppress,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptSpan {
    pub start_s: f64,
    pub end_s: f64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WakeHit {
    pub assistant: AssistantKind,
    pub start_s: f64,
    pub end_s: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SafePlaybackReport {
    pub hits_detected: usize,
    pub mode: SuppressionMode,
    pub sample_rate: u32,
    pub frames: usize,
    pub tested_on_physical_devices: bool,
    pub disclaimer: &'static str,
}

fn normalize_tokens(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_phrase(haystack_norm: &str, phrase_norm: &str) -> bool {
    if phrase_norm.is_empty() {
        return false;
    }
    let padded_h = format!(" {haystack_norm} ");
    let padded_p = format!(" {phrase_norm} ");
    padded_h.contains(&padded_p)
}

/// Scans transcript spans for enabled assistant wake phrases and custom phrases, padding
/// each hit by 0.15 s on both sides and merging overlaps per assistant. Never logs text.
pub fn find_wake_phrases(
    spans: &[TranscriptSpan],
    assistants: &[AssistantKind],
    custom_phrases: &[String],
) -> Vec<WakeHit> {
    let mut hits = Vec::new();
    let pad = 0.15;

    let mut targets: Vec<AssistantKind> = assistants.to_vec();
    for c in custom_phrases {
        let norm = normalize_tokens(c);
        if !norm.is_empty() {
            targets.push(AssistantKind::Custom(norm));
        }
    }

    for span in spans {
        let norm_text = normalize_tokens(&span.text);
        if norm_text.is_empty() {
            continue;
        }
        for target in &targets {
            let matched = target
                .phrases()
                .iter()
                .any(|p| contains_phrase(&norm_text, &normalize_tokens(p)));
            if matched {
                hits.push(WakeHit {
                    assistant: target.clone(),
                    start_s: (span.start_s - pad).max(0.0),
                    end_s: (span.end_s + pad).max(span.start_s),
                });
            }
        }
    }
    hits.sort_by(|a, b| a.start_s.total_cmp(&b.start_s));
    hits
}

/// Transcribes `audio` locally with Whisper (`stt` feature) and detects wake phrases.
/// Never downloads models at runtime.
#[cfg(feature = "stt")]
pub fn detect_wake_phrases_with_whisper(
    audio_path: &Path,
    whisper_model_dir: &Path,
    assistants: &[AssistantKind],
    custom_phrases: &[String],
) -> Result<Vec<WakeHit>> {
    let segs = crate::stt::transcribe(audio_path, whisper_model_dir)?;
    let spans: Vec<TranscriptSpan> = segs
        .into_iter()
        .map(|s| TranscriptSpan {
            start_s: s.start_s,
            end_s: s.end_s,
            text: s.text,
        })
        .collect();
    Ok(find_wake_phrases(&spans, assistants, custom_phrases))
}

fn suppress_speech_band_in_window(samples: &mut [f32], sample_rate: u32) {
    let n = samples.len();
    if n < 32 {
        return;
    }
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let ifft = planner.plan_fft_inverse(n);
    let mut buf: Vec<Complex<f32>> = samples.iter().map(|&s| Complex::new(s, 0.0)).collect();
    fft.process(&mut buf);

    let sr = sample_rate as f32;
    let nf = n as f32;
    for (k, bin) in buf.iter_mut().enumerate() {
        let freq = if k <= n / 2 {
            (k as f32) * sr / nf
        } else {
            ((n - k) as f32) * sr / nf
        };
        // Attenuate 300–3400 Hz speech formant band inside the wake-phrase window.
        if (300.0..=3400.0).contains(&freq) {
            *bin *= 0.10;
        }
    }
    ifft.process(&mut buf);
    let inv = 1.0 / nf;
    for (dst, c) in samples.iter_mut().zip(buf.iter()) {
        *dst = (c.re * inv).clamp(-1.0, 1.0);
    }
}

/// Applies Voice Assistant Safe Playback suppression to `input` and writes a verified WAV.
pub fn apply_safe_playback(
    input: &Path,
    output: &Path,
    hits: &[WakeHit],
    mode: SuppressionMode,
) -> Result<SafePlaybackReport> {
    const DISCLAIMER: &str = "Experimental suppression has not been tested on physical Siri/Google/Alexa/Meta/Bixby devices; use Mute or Bleep for reliable removal.";

    match mode {
        SuppressionMode::Mute | SuppressionMode::Bleep => {
            let kind = if mode == SuppressionMode::Mute {
                RedactKind::Mute
            } else {
                RedactKind::Bleep
            };
            let segs: Vec<Segment> = hits
                .iter()
                .map(|h| Segment {
                    start_s: h.start_s,
                    end_s: h.end_s,
                    kind,
                })
                .collect();
            let rep = audio::export_wav(input, output, &segs)?;
            Ok(SafePlaybackReport {
                hits_detected: hits.len(),
                mode,
                sample_rate: rep.sample_rate,
                frames: rep.frames_out as usize,
                tested_on_physical_devices: false,
                disclaimer: DISCLAIMER,
            })
        }
        SuppressionMode::ExperimentalBandSuppress => {
            ensure!(
                output
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("wav")),
                "Output must be .wav"
            );
            let mut dec = Decoder::open(input)?;
            let sr = dec.info.sample_rate;
            let ch = dec.info.channels;
            let mut all_frames: Vec<Vec<f32>> = vec![Vec::new(); ch];
            while let Some(interleaved) = dec.next_block()? {
                for frame in interleaved.chunks_exact(ch) {
                    for (c, &sample) in frame.iter().enumerate() {
                        all_frames[c].push(sample);
                    }
                }
            }
            let total_frames = all_frames.first().map(|v| v.len()).unwrap_or(0);
            for h in hits {
                let s0 = ((h.start_s * f64::from(sr)).floor() as usize).min(total_frames);
                let s1 = ((h.end_s * f64::from(sr)).ceil() as usize).min(total_frames);
                if s1 > s0 {
                    for plane in &mut all_frames {
                        suppress_speech_band_in_window(&mut plane[s0..s1], sr);
                    }
                }
            }
            let spec = hound::WavSpec {
                channels: dec.info.channels as u16,
                sample_rate: sr,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer = hound::WavWriter::create(output, spec)?;
            for i in 0..total_frames {
                for plane in &all_frames {
                    let s = (plane[i].clamp(-1.0, 1.0) * 32767.0).round() as i16;
                    writer.write_sample(s)?;
                }
            }
            writer.finalize()?;
            Ok(SafePlaybackReport {
                hits_detected: hits.len(),
                mode,
                sample_rate: sr,
                frames: total_frames,
                tested_on_physical_devices: false,
                disclaimer: DISCLAIMER,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_all_built_in_and_custom_assistants_without_false_substrings() {
        let spans = vec![
            TranscriptSpan {
                start_s: 0.5,
                end_s: 1.2,
                text: "Hey Siri, what is the weather?".into(),
            },
            TranscriptSpan {
                start_s: 2.0,
                end_s: 2.8,
                text: "OK Google set a timer".into(),
            },
            TranscriptSpan {
                start_s: 3.5,
                end_s: 4.1,
                text: "Alexa play music".into(),
            },
            TranscriptSpan {
                start_s: 5.0,
                end_s: 5.6,
                text: "Hey Meta take a photo".into(),
            },
            TranscriptSpan {
                start_s: 6.0,
                end_s: 6.7,
                text: "Hi Bixby open notes".into(),
            },
            TranscriptSpan {
                start_s: 7.5,
                end_s: 8.2,
                text: "Computer wake up now".into(),
            },
            TranscriptSpan {
                start_s: 9.0,
                end_s: 9.5,
                text: "metadata is clean".into(), // must NOT match "hey meta"
            },
        ];
        let hits = find_wake_phrases(&spans, &AssistantKind::BUILTINS, &["computer".into()]);
        assert_eq!(hits.len(), 6);
        assert_eq!(hits[0].assistant, AssistantKind::Siri);
        assert_eq!(hits[1].assistant, AssistantKind::Google);
        assert_eq!(hits[2].assistant, AssistantKind::Alexa);
        assert_eq!(hits[3].assistant, AssistantKind::Meta);
        assert_eq!(hits[4].assistant, AssistantKind::Bixby);
        assert_eq!(hits[5].assistant, AssistantKind::Custom("computer".into()));
    }

    #[test]
    fn mute_and_experimental_band_suppress_both_reduce_wake_window_energy() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("speech.wav");
        let out_mute = dir.path().join("muted.wav");
        let out_band = dir.path().join("band.wav");

        let sr = 16_000u32;
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: sr,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&src, spec).unwrap();
        for i in 0..sr * 2 {
            let t = i as f32 / sr as f32;
            let sample = (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 0.5;
            w.write_sample((sample * 32767.0) as i16).unwrap();
        }
        w.finalize().unwrap();

        let hits = vec![WakeHit {
            assistant: AssistantKind::Alexa,
            start_s: 0.5,
            end_s: 1.0,
        }];

        let rep_m = apply_safe_playback(&src, &out_mute, &hits, SuppressionMode::Mute).unwrap();
        assert!(!rep_m.tested_on_physical_devices);
        assert_eq!(rep_m.frames, 32_000);

        let rep_b = apply_safe_playback(
            &src,
            &out_band,
            &hits,
            SuppressionMode::ExperimentalBandSuppress,
        )
        .unwrap();
        assert!(!rep_b.tested_on_physical_devices);
        assert_eq!(rep_b.frames, 32_000);

        let r_band = hound::WavReader::open(&out_band).unwrap();
        let samples: Vec<i16> = r_band.into_samples::<i16>().map(|s| s.unwrap()).collect();
        let rms_window: f64 = (samples[9_000..15_000]
            .iter()
            .map(|&s| f64::from(s) * f64::from(s))
            .sum::<f64>()
            / 6_000.0)
            .sqrt();
        let rms_outside: f64 = (samples[1_000..6_000]
            .iter()
            .map(|&s| f64::from(s) * f64::from(s))
            .sum::<f64>()
            / 5_000.0)
            .sqrt();
        assert!(rms_window < rms_outside * 0.25);
    }
}
