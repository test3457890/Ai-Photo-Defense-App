//! Audio: streaming decode, analysis, comparison, privacy redaction and WAV export.
//!
//! Decoding streams packet by packet; nothing holds a whole recording in memory except the
//! compact waveform/spectrogram summaries. Exports keep the source sample rate and channel
//! count (never down-mixed) and write 32-bit float PCM unless the source is 16/24-bit PCM.
use crate::{MediaInfo, MediaKind, Track};
use anyhow::{Context, Result, bail, ensure};
use photo_privacy::{Category, Evidence, Finding, Source};
use serde::{Deserialize, Serialize};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTagKey};
use symphonia::core::probe::Hint;

pub const MAX_SAMPLE_RATE: u32 = 768_000;
pub const MAX_CHANNELS: usize = 32;
/// Upper bound on decoded duration (frames) to stop maliciously long streams: 12 h at 192 kHz.
pub const MAX_FRAMES: u64 = 12 * 3600 * 192_000;

#[derive(Debug, Clone, Serialize)]
pub struct StreamInfo {
    pub container: String,
    pub codec: String,
    pub sample_rate: u32,
    pub channels: usize,
    pub bits_per_sample: Option<u32>,
    pub frames: Option<u64>,
    pub findings: Vec<Finding>,
    /// Decoding caveats the user should know about (for example HE-AAC core-only decoding).
    pub notes: Vec<String>,
}

enum CodecDecoder {
    Symphonia(Box<dyn symphonia::core::codecs::Decoder>),
    #[cfg(feature = "opus")]
    Opus(craft_audio::opus::Decoder),
}

pub struct Decoder {
    format: Box<dyn FormatReader>,
    decoder: CodecDecoder,
    track: u32,
    pub info: StreamInfo,
    pub decoded_frames: u64,
    pub decode_errors: u64,
    /// First decoded block, kept when it had to be decoded early to learn the channel count.
    pending: Option<Vec<f32>>,
}

fn tag_findings(rev: &MetadataRevision, out: &mut Vec<Finding>) {
    let f = |c: Category, d: &str| {
        Finding::new(
            c,
            Evidence::Confirmed,
            Source::Metadata,
            None,
            d.into(),
            None,
        )
    };
    for t in rev.tags() {
        let key = t.key.to_ascii_lowercase();
        let cat = match t.std_key {
            Some(
                StandardTagKey::Comment | StandardTagKey::Description | StandardTagKey::Lyrics,
            ) => (Category::Comment, "Comment, description or lyrics tag"),
            Some(
                StandardTagKey::Date | StandardTagKey::OriginalDate | StandardTagKey::ReleaseDate,
            ) => (Category::Timestamp, "Date tag"),
            Some(
                StandardTagKey::Encoder
                | StandardTagKey::EncoderSettings
                | StandardTagKey::EncodedBy,
            ) => (Category::DeviceInformation, "Encoder or software tag"),
            Some(
                StandardTagKey::Artist
                | StandardTagKey::Composer
                | StandardTagKey::Performer
                | StandardTagKey::Writer
                | StandardTagKey::Conductor,
            ) => (Category::TextMetadata, "Artist or contributor tags"),
            _ if key.contains("gps") || key.contains("location") || key.contains("geo") => {
                (Category::GpsLocation, "Location tag")
            }
            _ if key.contains("bext") || key.contains("originator") => {
                (Category::DeviceInformation, "Broadcast WAV originator")
            }
            _ => (Category::TextMetadata, "Other text tags"),
        };
        out.push(f(cat.0, cat.1));
    }
    if !rev.visuals().is_empty() {
        out.push(f(Category::EmbeddedImage, "Embedded cover art"));
    }
}

/// Third-party decoders can panic on crafted input (observed: Symphonia 0.5.5 asserts on a
/// zero sample rate). Contain such panics at this boundary and report them as errors.
fn guard<T>(what: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => {
            photo_log::warn(
                photo_log::Component::Decode,
                photo_log::Code::DecodeFailed,
                &format!("decoder panic contained during {what}"),
            );
            bail!("The audio decoder rejected malformed input ({what})")
        }
    }
}

impl Decoder {
    pub fn open_source(src: Box<dyn MediaSource>, ext: Option<&str>) -> Result<Self> {
        let ext = ext.map(str::to_string);
        guard("open", move || Self::open_inner(src, ext.as_deref()))
    }
    fn open_inner(src: Box<dyn MediaSource>, ext: Option<&str>) -> Result<Self> {
        let mss = MediaSourceStream::new(src, Default::default());
        let mut hint = Hint::new();
        if let Some(e) = ext {
            hint.with_extension(e);
        }
        let mut probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .context("Unsupported or corrupted audio container")?;
        let mut findings = vec![];
        if let Some(m) = probed.metadata.get()
            && let Some(rev) = m.current()
        {
            tag_findings(rev, &mut findings);
        }
        if let Some(rev) = probed.format.metadata().current() {
            tag_findings(rev, &mut findings);
        }
        let mut seen = std::collections::HashSet::new();
        findings.retain(|f| seen.insert(f.description.clone()));
        let format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .context("No decodable audio track")?;
        let p = &track.codec_params;
        let sample_rate = p.sample_rate.context("Unknown sample rate")?;
        ensure!(
            (1..=MAX_SAMPLE_RATE).contains(&sample_rate),
            "Invalid sample rate {sample_rate}"
        );
        // Some containers (for example AAC in MP4) only reveal the channel count once a packet is
        // decoded; 0 means "learn it from the first decoded packet".
        let channels = p.channels.map(|c| c.count()).unwrap_or(0);
        ensure!(channels <= MAX_CHANNELS, "Invalid channel count {channels}");
        if let Some(n) = p.n_frames {
            ensure!(
                n <= MAX_FRAMES,
                "Declared duration exceeds the 12-hour limit"
            );
        }
        let codec = symphonia::default::get_codecs()
            .get_codec(p.codec)
            .map(|d| d.short_name.to_string())
            .unwrap_or_else(|| {
                if p.codec == symphonia::core::codecs::CODEC_TYPE_OPUS {
                    "opus".into()
                } else {
                    "unknown".into()
                }
            });
        #[cfg(feature = "opus")]
        let (decoder, sample_rate) = if p.codec == symphonia::core::codecs::CODEC_TYPE_OPUS {
            let mut dec = if let Some(extra) = p.extra_data.as_deref()
                && extra.starts_with(b"OpusHead")
            {
                craft_audio::opus::Decoder::new(extra)
                    .map_err(|e| anyhow::anyhow!("Invalid OpusHead: {e}"))?
            } else {
                craft_audio::opus::Decoder::simple(48_000, channels.max(1))
                    .map_err(|e| anyhow::anyhow!("Invalid Opus stream: {e}"))?
            };
            dec.set_trim_pre_skip(true);
            (CodecDecoder::Opus(dec), 48_000)
        } else {
            let dec = symphonia::default::get_codecs()
                .make(p, &DecoderOptions::default())
                .context("No decoder for this audio codec")?;
            (CodecDecoder::Symphonia(dec), sample_rate)
        };
        #[cfg(not(feature = "opus"))]
        let decoder = CodecDecoder::Symphonia(
            symphonia::default::get_codecs()
                .make(p, &DecoderOptions::default())
                .context("No decoder for this audio codec")?,
        );
        let info = StreamInfo {
            container: String::new(),
            codec,
            sample_rate,
            channels,
            bits_per_sample: p.bits_per_sample,
            frames: p.n_frames,
            findings,
            notes: vec![],
        };
        let id = track.id;
        let mut s = Self {
            format,
            decoder,
            track: id,
            info,
            decoded_frames: 0,
            decode_errors: 0,
            pending: None,
        };
        // Decode the first packet now: it reveals the channel count when the container does not
        // state it, and the true sample rate of HE-AAC streams (see next_inner).
        s.pending = s.next_inner()?;
        ensure!(
            s.info.channels > 0,
            "Unknown channel layout: no audio could be decoded"
        );
        Ok(s)
    }
    pub fn open(path: &Path) -> Result<Self> {
        let meta = std::fs::symlink_metadata(path)?;
        ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "Audio input must be a regular file"
        );
        let mut head = [0u8; 64];
        let n = std::io::Read::read(&mut std::fs::File::open(path)?, &mut head)?;
        let det = crate::detect(&head[..n]);
        ensure!(
            det.kind == MediaKind::Audio
                || det.container == "MP4"
                || det.container == "MOV"
                || det.container == "WebM"
                || det.container == "Matroska",
            "Not an audio file (detected {})",
            det.container
        );
        let mut d = Self::open_source(
            Box::new(std::fs::File::open(path)?),
            path.extension().and_then(|e| e.to_str()),
        )?;
        d.info.container = det.container.into();
        Ok(d)
    }
    /// Next block of interleaved samples, or None at end of stream.
    pub fn next_block(&mut self) -> Result<Option<Vec<f32>>> {
        guard("decode", || self.next_inner())
    }
    fn next_inner(&mut self) -> Result<Option<Vec<f32>>> {
        if let Some(b) = self.pending.take() {
            return Ok(Some(b));
        }
        loop {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(None);
                }
                Err(SymError::ResetRequired) => return Ok(None),
                Err(e) => return Err(e.into()),
            };
            if packet.track_id() != self.track {
                continue;
            }
            match &mut self.decoder {
                CodecDecoder::Symphonia(dec) => match dec.decode(&packet) {
                    Ok(buf) => {
                        let spec = *buf.spec();
                        if self.info.channels == 0 {
                            let n = spec.channels.count();
                            ensure!((1..=MAX_CHANNELS).contains(&n), "Invalid channel count {n}");
                            self.info.channels = n;
                        }
                        ensure!(
                            spec.channels.count() == self.info.channels,
                            "Channel layout changed mid-stream"
                        );
                        // HE-AAC (SBR/PS): Symphonia decodes only the AAC core, which runs at half
                        // the signalled rate. The container's packet duration is then twice the
                        // decoded frame count; use the core rate so timing stays correct.
                        if self.decoded_frames == 0
                            && buf.frames() > 0
                            && packet.dur == 2 * buf.frames() as u64
                            && self.info.codec.to_ascii_lowercase().contains("aac")
                        {
                            self.info.sample_rate /= 2;
                            self.info.notes.push(format!(
                                "HE-AAC stream: only the AAC core is decoded (spectral band replication and parametric stereo are not supported), so the audio is {} Hz, {}-channel with reduced high-frequency detail. Timing is preserved.",
                                self.info.sample_rate, self.info.channels
                            ));
                        }
                        let mut sb = SampleBuffer::<f32>::new(buf.capacity() as u64, spec);
                        sb.copy_interleaved_ref(buf);
                        let s = sb.samples().to_vec();
                        self.decoded_frames += (s.len() / self.info.channels) as u64;
                        ensure!(
                            self.decoded_frames <= MAX_FRAMES,
                            "Decoded duration exceeds the 12-hour limit"
                        );
                        if s.is_empty() {
                            continue;
                        }
                        return Ok(Some(s));
                    }
                    Err(SymError::DecodeError(_)) => {
                        self.decode_errors += 1;
                        ensure!(self.decode_errors < 1000, "Too many corrupt audio packets");
                    }
                    Err(e) => return Err(e.into()),
                },
                #[cfg(feature = "opus")]
                CodecDecoder::Opus(dec) => match dec.decode(Some(&packet.data)) {
                    Ok(planar) => {
                        let ch = self.info.channels;
                        ensure!(planar.len() == ch, "Opus channel count changed mid-stream");
                        let frames = planar.first().map_or(0, Vec::len);
                        if frames == 0 {
                            continue;
                        }
                        self.decoded_frames += frames as u64;
                        ensure!(
                            self.decoded_frames <= MAX_FRAMES,
                            "Decoded duration exceeds the 12-hour limit"
                        );
                        let mut s = Vec::with_capacity(frames * ch);
                        for f in 0..frames {
                            for c in &planar {
                                s.push(c[f]);
                            }
                        }
                        return Ok(Some(s));
                    }
                    Err(_) => {
                        self.decode_errors += 1;
                        ensure!(self.decode_errors < 1000, "Too many corrupt Opus packets");
                    }
                },
            }
        }
    }
}

pub fn probe_bytes(bytes: &[u8], container: &str) -> Result<MediaInfo> {
    let d = Decoder::open_source(Box::new(std::io::Cursor::new(bytes.to_vec())), None)?;
    let i = &d.info;
    Ok(MediaInfo {
        kind: MediaKind::Audio,
        container: container.into(),
        duration_s: i.frames.map(|f| f as f64 / f64::from(i.sample_rate)),
        tracks: vec![Track {
            id: 1,
            kind: "audio".into(),
            codec: i.codec.clone(),
            width: None,
            height: None,
            frame_rate: None,
            sample_rate: Some(i.sample_rate),
            channels: Some(i.channels as u16),
            duration_s: i.frames.map(|f| f as f64 / f64::from(i.sample_rate)),
            language: None,
            samples: i.frames,
        }],
        findings: i.findings.clone(),
        warnings: vec![],
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Analysis {
    pub sample_rate: u32,
    pub channels: usize,
    pub frames: u64,
    pub duration_s: f64,
    pub peak_dbfs: f64,
    pub true_peak_dbtp: Option<f64>,
    pub integrated_lufs: Option<f64>,
    pub loudness_range_lu: Option<f64>,
    pub clipped_samples: u64,
    pub dc_offset: f64,
    pub silent_fraction: f64,
    /// Per-column (min, max) of the mono mix for drawing.
    pub waveform: Vec<(f32, f32)>,
    /// Columns of 64 log-spaced band magnitudes in dB (mono mix).
    pub spectrogram: Vec<Vec<f32>>,
    pub decode_errors: u64,
    /// Decoding caveats (copied from the decoder).
    pub notes: Vec<String>,
}

const COLUMNS: usize = 1200;
const BANDS: usize = 64;
const FFT: usize = 2048;

fn db(x: f64) -> f64 {
    20. * x.max(1e-12).log10()
}

/// Streams a file once and computes loudness, peaks, clipping, waveform and spectrogram.
pub fn analyze(path: &Path) -> Result<Analysis> {
    let mut d = Decoder::open(path)?;
    let (rate, ch) = (d.info.sample_rate, d.info.channels);
    let mut r128 = ebur128::EbuR128::new(
        ch as u32,
        rate,
        ebur128::Mode::I | ebur128::Mode::LRA | ebur128::Mode::TRUE_PEAK,
    )
    .ok();
    let expected = d.info.frames.unwrap_or(u64::from(rate) * 600).max(1);
    let per_col = (expected / COLUMNS as u64).max(1);
    let hop = (expected / COLUMNS as u64).clamp(256, 1 << 22) as usize;
    let mut planner = rustfft::FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT);
    let window: Vec<f32> = (0..FFT)
        .map(|i| 0.5 - 0.5 * (2. * std::f32::consts::PI * i as f32 / FFT as f32).cos())
        .collect();
    let mut ring: Vec<f32> = Vec::with_capacity(FFT);
    let mut since_hop = 0usize;
    let (mut peak, mut clipped, mut sum, mut silent, mut frames) = (0f32, 0u64, 0f64, 0u64, 0u64);
    let (mut wave, mut col_min, mut col_max, mut col_n) = (vec![], f32::MAX, f32::MIN, 0u64);
    let mut spec: Vec<Vec<f32>> = vec![];
    let nyquist = rate as f32 / 2.;
    let bin_hz = rate as f32 / FFT as f32;
    let band_edges: Vec<usize> = (0..=BANDS)
        .map(|k| {
            (20. * (nyquist / 20.).max(1.).powf(k as f32 / BANDS as f32) / bin_hz)
                .clamp(1., (FFT / 2) as f32) as usize
        })
        .collect();
    while let Some(block) = d.next_block()? {
        if let Some(r) = r128.as_mut() {
            let _ = r.add_frames_f32(&block);
        }
        for f in block.chunks_exact(ch) {
            let mut m = 0f32;
            for s in f {
                let a = s.abs();
                peak = peak.max(a);
                if a >= 0.999 {
                    clipped += 1;
                }
                m += s;
            }
            m /= ch as f32;
            sum += f64::from(m);
            if m.abs() < 1e-4 {
                silent += 1;
            }
            frames += 1;
            col_min = col_min.min(m);
            col_max = col_max.max(m);
            col_n += 1;
            if col_n >= per_col {
                wave.push((col_min, col_max));
                (col_min, col_max, col_n) = (f32::MAX, f32::MIN, 0);
            }
            if ring.len() == FFT {
                ring.remove(0);
            }
            ring.push(m);
            since_hop += 1;
            if since_hop >= hop && ring.len() == FFT && spec.len() < COLUMNS * 2 {
                since_hop = 0;
                let mut buf: Vec<rustfft::num_complex::Complex<f32>> = ring
                    .iter()
                    .zip(&window)
                    .map(|(x, w)| rustfft::num_complex::Complex::new(x * w, 0.))
                    .collect();
                fft.process(&mut buf);
                let col: Vec<f32> = band_edges
                    .windows(2)
                    .map(|e| {
                        let (a, b) = (e[0], e[1].max(e[0] + 1).min(FFT / 2));
                        let p: f32 =
                            buf[a..b].iter().map(|c| c.norm_sqr()).sum::<f32>() / (b - a) as f32;
                        10. * (p.max(1e-12)).log10()
                    })
                    .collect();
                spec.push(col);
            }
        }
    }
    if col_n > 0 {
        wave.push((col_min, col_max));
    }
    ensure!(frames > 0, "No audio samples decoded");
    let tp = r128
        .as_ref()
        .and_then(|r| {
            (0..ch as u32)
                .map(|c| r.true_peak(c).ok())
                .collect::<Option<Vec<f64>>>()
        })
        .map(|v| db(v.into_iter().fold(0., f64::max)));
    Ok(Analysis {
        sample_rate: rate,
        channels: ch,
        frames,
        duration_s: frames as f64 / f64::from(rate),
        peak_dbfs: db(f64::from(peak)),
        true_peak_dbtp: tp,
        integrated_lufs: r128
            .as_ref()
            .and_then(|r| r.loudness_global().ok())
            .filter(|v| v.is_finite()),
        loudness_range_lu: r128
            .as_ref()
            .and_then(|r| r.loudness_range().ok())
            .filter(|v| v.is_finite()),
        clipped_samples: clipped,
        dc_offset: sum / frames as f64,
        silent_fraction: silent as f64 / frames as f64,
        waveform: wave,
        spectrogram: spec,
        decode_errors: d.decode_errors,
        notes: d.info.notes.clone(),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Comparison {
    pub frames_compared: u64,
    pub length_difference_frames: i64,
    pub snr_db: Option<f64>,
    pub segmental_snr_db: Option<f64>,
    pub max_abs_difference: f64,
    pub note: String,
}

/// Lockstep streaming comparison of two recordings with the same rate and channel count.
pub fn compare(original: &Path, processed: &Path) -> Result<Comparison> {
    let mut a = Decoder::open(original)?;
    let mut b = Decoder::open(processed)?;
    ensure!(
        a.info.sample_rate == b.info.sample_rate && a.info.channels == b.info.channels,
        "Sample rate or channel count differs"
    );
    let ch = a.info.channels;
    let seg = (a.info.sample_rate as usize / 50).max(1) * ch; // 20 ms segments
    let (mut qa, mut qb) = (
        std::collections::VecDeque::new(),
        std::collections::VecDeque::new(),
    );
    let (mut sig, mut noise, mut maxd, mut n) = (0f64, 0f64, 0f64, 0u64);
    let (mut seg_sum, mut seg_count, mut ss, mut sn, mut sc) = (0f64, 0u64, 0f64, 0f64, 0usize);
    let (mut ea, mut eb) = (false, false);
    let (mut fa, mut fb) = (0u64, 0u64);
    loop {
        while qa.len() < 65536 && !ea {
            match a.next_block()? {
                Some(v) => {
                    fa += (v.len() / ch) as u64;
                    qa.extend(v)
                }
                None => ea = true,
            }
        }
        while qb.len() < 65536 && !eb {
            match b.next_block()? {
                Some(v) => {
                    fb += (v.len() / ch) as u64;
                    qb.extend(v)
                }
                None => eb = true,
            }
        }
        let k = qa.len().min(qb.len());
        if k == 0 {
            break;
        }
        for _ in 0..k {
            let (x, y) = (
                f64::from(qa.pop_front().expect("k")),
                f64::from(qb.pop_front().expect("k")),
            );
            let d = y - x;
            sig += x * x;
            noise += d * d;
            maxd = maxd.max(d.abs());
            n += 1;
            ss += x * x;
            sn += d * d;
            sc += 1;
            if sc == seg {
                if ss > 1e-8 {
                    seg_sum += (10. * (ss / sn.max(1e-20)).log10()).clamp(-10., 80.);
                    seg_count += 1;
                }
                (ss, sn, sc) = (0., 0., 0);
            }
        }
    }
    ensure!(n > 0, "Nothing to compare");
    Ok(Comparison {
        frames_compared: n / ch as u64,
        length_difference_frames: fb as i64 - fa as i64,
        snr_db: (noise > 0.).then(|| 10. * (sig / noise).log10()),
        segmental_snr_db: (seg_count > 0).then(|| seg_sum / seg_count as f64),
        max_abs_difference: maxd,
        note: "Signal-quality metrics only; they do not measure resistance to voice cloning."
            .into(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RedactKind {
    Mute,
    Bleep,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub start_s: f64,
    pub end_s: f64,
    pub kind: RedactKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportReport {
    pub sample_rate: u32,
    pub channels: usize,
    pub frames_in: u64,
    pub frames_out: u64,
    pub format: String,
    pub segments: Vec<Segment>,
    pub source_metadata_removed: Vec<String>,
    pub verified: bool,
    pub max_unredacted_difference: f64,
    pub notes: Vec<String>,
}

const FADE_S: f64 = 0.005;

fn gain_and_tone(t: f64, segs: &[Segment]) -> (f32, f32) {
    for s in segs {
        if t >= s.start_s - FADE_S && t <= s.end_s + FADE_S {
            let edge = ((t - (s.start_s - FADE_S)) / FADE_S)
                .min(((s.end_s + FADE_S) - t) / FADE_S)
                .clamp(0., 1.);
            let g = 1. - edge as f32;
            let tone = if s.kind == RedactKind::Bleep {
                edge as f32 * 0.1 * (2. * std::f64::consts::PI * 1000. * t).sin() as f32
            } else {
                0.
            };
            return (g, tone);
        }
    }
    (1., 0.)
}

/// Decodes, applies mute/bleep segments with 5 ms fades, writes a metadata-free WAV that keeps
/// the source rate and channel count, then reopens and verifies it.
pub fn export_wav(input: &Path, output: &Path, segments: &[Segment]) -> Result<ExportReport> {
    ensure!(
        output
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("wav")),
        "Output must be .wav"
    );
    ensure!(
        segments.len() <= 10_000
            && segments.iter().all(|s| s.start_s.is_finite()
                && s.end_s.is_finite()
                && s.start_s >= 0.
                && s.end_s > s.start_s),
        "Invalid redaction segments"
    );
    let dir = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    ensure!(dir.is_dir(), "Output folder must exist");
    let mut d = Decoder::open(input)?;
    let (rate, ch) = (d.info.sample_rate, d.info.channels);
    let int16 = d.info.bits_per_sample == Some(16) && d.info.codec.starts_with("pcm_s16");
    let spec = if int16 {
        hound::WavSpec {
            channels: ch as u16,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        }
    } else {
        hound::WavSpec {
            channels: ch as u16,
            sample_rate: rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        }
    };
    let tmp = tempfile::NamedTempFile::new_in(dir)?;
    let mut frames = 0u64;
    {
        let mut w =
            hound::WavWriter::new(std::io::BufWriter::new(tmp.as_file().try_clone()?), spec)?;
        while let Some(block) = d.next_block()? {
            for f in block.chunks_exact(ch) {
                let t = frames as f64 / f64::from(rate);
                let (g, tone) = gain_and_tone(t, segments);
                for s in f {
                    let v = (s * g + tone).clamp(-1., 1.);
                    if int16 {
                        w.write_sample((v * 32767.).round() as i16)?;
                    } else {
                        w.write_sample(v)?;
                    }
                }
                frames += 1;
            }
        }
        w.finalize()?;
    }
    tmp.as_file().sync_all()?;
    tmp.persist_noclobber(output)
        .map_err(|e| anyhow::anyhow!("Export refused (destination must be new): {}", e.error))?;
    // Verification: same layout and length; unredacted audio unchanged; redactions applied.
    let mut a = Decoder::open(input)?;
    let mut b = Decoder::open(output)?;
    ensure!(
        b.info.sample_rate == rate && b.info.channels == ch,
        "Exported layout changed"
    );
    let (mut i, mut maxd, mut red_energy, mut red_n) = (0u64, 0f64, 0f64, 0u64);
    let (mut qa, mut qb) = (
        std::collections::VecDeque::new(),
        std::collections::VecDeque::new(),
    );
    loop {
        if qa.len() < ch * 4096
            && let Some(v) = a.next_block()?
        {
            qa.extend(v);
            continue;
        }
        if qb.len() < ch * 4096
            && let Some(v) = b.next_block()?
        {
            qb.extend(v);
            continue;
        }
        let k = (qa.len().min(qb.len()) / ch) * ch;
        if k == 0 {
            break;
        }
        for _ in 0..k / ch {
            let t = i as f64 / f64::from(rate);
            let (g, _) = gain_and_tone(t, segments);
            for _ in 0..ch {
                let (x, y) = (
                    f64::from(qa.pop_front().expect("k")),
                    f64::from(qb.pop_front().expect("k")),
                );
                if g >= 1. {
                    maxd = maxd.max((x - y).abs());
                } else if g <= 0. {
                    red_energy += y * y;
                    red_n += 1;
                }
            }
            i += 1;
        }
    }
    let tol = if int16 { 1.5 / 32767. } else { 1e-6 };
    let red_rms = if red_n > 0 {
        (red_energy / red_n as f64).sqrt()
    } else {
        0.
    };
    let mute_only = segments.iter().all(|s| s.kind == RedactKind::Mute);
    let verified = b.decoded_frames == frames && maxd <= tol && (!mute_only || red_rms < 1e-6);
    if !verified {
        bail!(
            "Export verification failed (length {} vs {}, max difference {maxd:.2e}, redacted RMS {red_rms:.2e}); the file was written but must not be trusted",
            b.decoded_frames,
            frames
        );
    }
    photo_log::info(
        photo_log::Component::Export,
        photo_log::Code::ExportVerified,
        &format!("audio export verified, {} segments", segments.len()),
    );
    Ok(ExportReport {
        sample_rate: rate,
        channels: ch,
        frames_in: frames,
        frames_out: b.decoded_frames,
        format: if int16 { "WAV PCM 16-bit".into() } else { "WAV 32-bit float".into() },
        segments: segments.to_vec(),
        source_metadata_removed: d.info.findings.iter().map(|f| f.description.clone()).collect(),
        verified,
        max_unredacted_difference: maxd,
        notes: [
            "Lossy sources (MP3, AAC, Vorbis) are written as lossless WAV of the decoded audio; re-encoding for publication is not implemented.".to_string(),
            "Bleeps are a 1 kHz tone at -20 dBFS with 5 ms fades. Muting removes the audio; the words are not recoverable from the export.".to_string(),
        ]
        .into_iter()
        .chain(d.info.notes.iter().cloned())
        .collect(),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    /// Minimal big-endian 16-bit AIFF at 44.1 kHz (sample rate as 80-bit extended float).
    fn write_test_aiff(path: &Path, ch: u16, frames: u32) {
        let mut ssnd = vec![0u8; 8];
        for i in 0..frames {
            let v = (0.4 * (2. * std::f32::consts::PI * 440. * i as f32 / 44_100.).sin() * 32767.)
                as i16;
            for _ in 0..ch {
                ssnd.extend_from_slice(&v.to_be_bytes());
            }
        }
        let mut comm = ch.to_be_bytes().to_vec();
        comm.extend_from_slice(&frames.to_be_bytes());
        comm.extend_from_slice(&16u16.to_be_bytes());
        comm.extend_from_slice(&[0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]);
        let mut body = b"AIFF".to_vec();
        for (id, data) in [(b"COMM", &comm), (b"SSND", &ssnd)] {
            body.extend_from_slice(id);
            body.extend_from_slice(&(data.len() as u32).to_be_bytes());
            body.extend_from_slice(data);
        }
        let mut f = b"FORM".to_vec();
        f.extend_from_slice(&(body.len() as u32).to_be_bytes());
        f.extend_from_slice(&body);
        std::fs::write(path, f).unwrap();
    }
    #[test]
    fn aiff_decodes_and_exports() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("a.aiff");
        write_test_aiff(&src, 2, 22_050);
        assert_eq!(
            crate::detect(&std::fs::read(&src).unwrap()).container,
            "AIFF"
        );
        let a = analyze(&src).unwrap();
        assert_eq!((a.sample_rate, a.channels, a.frames), (44_100, 2, 22_050));
        let out = d.path().join("a.wav");
        let r = export_wav(&src, &out, &[]).unwrap();
        assert!(r.verified);
        assert_eq!(r.frames_out, 22_050);
    }
    pub fn write_test_wav(path: &Path, rate: u32, ch: u16, secs: f32, int16: bool) {
        let spec = hound::WavSpec {
            channels: ch,
            sample_rate: rate,
            bits_per_sample: if int16 { 16 } else { 32 },
            sample_format: if int16 {
                hound::SampleFormat::Int
            } else {
                hound::SampleFormat::Float
            },
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..(rate as f32 * secs) as u32 {
            let t = i as f32 / rate as f32;
            for c in 0..ch {
                let v = 0.4 * (2. * std::f32::consts::PI * (220. + 110. * c as f32) * t).sin()
                    + 0.05 * (2. * std::f32::consts::PI * 3000. * t).sin();
                if int16 {
                    w.write_sample((v * 32767.) as i16).unwrap()
                } else {
                    w.write_sample(v).unwrap()
                }
            }
        }
        w.finalize().unwrap();
    }
    #[test]
    fn analyze_compare_and_redact_stereo() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("in.wav");
        write_test_wav(&src, 48_000, 2, 2.0, true);
        let a = analyze(&src).unwrap();
        assert_eq!((a.channels, a.sample_rate, a.frames), (2, 48_000, 96_000));
        assert!(a.peak_dbfs < -5. && a.peak_dbfs > -9., "{}", a.peak_dbfs);
        assert!(a.integrated_lufs.unwrap() < -5. && a.true_peak_dbtp.is_some());
        assert_eq!(a.clipped_samples, 0);
        assert!(a.waveform.len() > 100 && !a.spectrogram.is_empty());
        let out = d.path().join("out.wav");
        let segs = [Segment {
            start_s: 0.5,
            end_s: 1.0,
            kind: RedactKind::Mute,
        }];
        let r = export_wav(&src, &out, &segs).unwrap();
        assert!(
            r.verified && r.frames_out == 96_000 && r.channels == 2 && r.format.contains("16-bit")
        );
        assert!(export_wav(&src, &out, &segs).is_err(), "never overwrites");
        let c = compare(&src, &out).unwrap();
        assert_eq!(c.length_difference_frames, 0);
        assert!(c.snr_db.unwrap() < 20., "muted quarter lowers SNR");
        let same = compare(&src, &src).unwrap();
        assert!(same.snr_db.is_none() && same.max_abs_difference == 0.);
        let bleep = d.path().join("bleep.wav");
        let r = export_wav(
            &src,
            &bleep,
            &[Segment {
                start_s: 0.2,
                end_s: 0.4,
                kind: RedactKind::Bleep,
            }],
        )
        .unwrap();
        assert!(r.verified);
    }
    #[test]
    fn float_and_mono_preserved_and_hostile_inputs() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("f.wav");
        write_test_wav(&src, 44_100, 1, 0.5, false);
        let r = export_wav(&src, &d.path().join("o.wav"), &[]).unwrap();
        assert!(r.format.contains("float") && r.channels == 1 && r.max_unredacted_difference == 0.);
        let bad = d.path().join("bad.wav");
        let mut bytes = std::fs::read(&src).unwrap();
        bytes[24..28].copy_from_slice(&0u32.to_le_bytes()); // sample rate 0
        std::fs::write(&bad, &bytes).unwrap();
        assert!(analyze(&bad).is_err());
        std::fs::write(&bad, b"RIFF\0\0\0\0WAVEjunk").unwrap();
        assert!(analyze(&bad).is_err());
        let good = std::fs::read(&src).unwrap();
        for cut in [10, 44, 100, 1000] {
            std::fs::write(&bad, &good[..cut]).unwrap();
            let _ = analyze(&bad);
        }
        assert!(export_wav(&src, &d.path().join("x.mp3"), &[]).is_err());
        assert!(
            export_wav(
                &src,
                &d.path().join("y.wav"),
                &[Segment {
                    start_s: 2.,
                    end_s: 1.,
                    kind: RedactKind::Mute
                }]
            )
            .is_err()
        );
    }

    #[cfg(feature = "opus")]
    fn ogg_crc(page: &[u8]) -> u32 {
        let mut crc = 0u32;
        for &b in page {
            crc ^= u32::from(b) << 24;
            for _ in 0..8 {
                crc = if (crc & 0x8000_0000) != 0 {
                    (crc << 1) ^ 0x04C1_1DB7
                } else {
                    crc << 1
                };
            }
        }
        crc
    }

    #[cfg(feature = "opus")]
    fn ogg_page(
        header_type: u8,
        granule: u64,
        serial: u32,
        seq: u32,
        packets: &[&[u8]],
    ) -> Vec<u8> {
        let mut segs = Vec::new();
        let mut body = Vec::new();
        for p in packets {
            let mut rem = p.len();
            while rem >= 255 {
                segs.push(255u8);
                rem -= 255;
            }
            segs.push(rem as u8);
            body.extend_from_slice(p);
        }
        let mut page = Vec::with_capacity(27 + segs.len() + body.len());
        page.extend_from_slice(b"OggS");
        page.push(0);
        page.push(header_type);
        page.extend_from_slice(&granule.to_le_bytes());
        page.extend_from_slice(&serial.to_le_bytes());
        page.extend_from_slice(&seq.to_le_bytes());
        page.extend_from_slice(&0u32.to_le_bytes());
        page.push(segs.len() as u8);
        page.extend_from_slice(&segs);
        page.extend_from_slice(&body);
        let crc = ogg_crc(&page);
        page[22..26].copy_from_slice(&crc.to_le_bytes());
        page
    }

    #[cfg(feature = "opus")]
    #[test]
    fn opus_in_ogg_decodes_analyzes_and_exports_wav() {
        let mut enc = craft_audio::opus::Encoder::new(1, 64_000).unwrap();
        let head = enc.opus_head();
        let mut tags = b"OpusTags".to_vec();
        tags.extend_from_slice(&4u32.to_le_bytes());
        tags.extend_from_slice(b"test");
        tags.extend_from_slice(&0u32.to_le_bytes());

        let mut packets = Vec::new();
        let frame_n = craft_audio::opus::Encoder::FRAME_SIZE;
        for f in 0..15 {
            let pcm: Vec<f32> = (0..frame_n)
                .map(|i| {
                    let t = (f * frame_n + i) as f32 / 48_000.0;
                    0.25 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
                })
                .collect();
            packets.push(enc.encode_float(&pcm).unwrap());
        }

        let mut ogg = Vec::new();
        ogg.extend_from_slice(&ogg_page(0x02, 0, 1, 0, &[&head]));
        ogg.extend_from_slice(&ogg_page(0x00, 0, 1, 1, &[&tags]));
        let pkt_refs: Vec<&[u8]> = packets.iter().map(Vec::as_slice).collect();
        ogg.extend_from_slice(&ogg_page(0x04, (15 * frame_n) as u64, 1, 2, &pkt_refs));

        let dir = tempfile::tempdir().unwrap();
        let in_ogg = dir.path().join("tone.ogg");
        let out_wav = dir.path().join("tone.wav");
        std::fs::write(&in_ogg, &ogg).unwrap();

        let analysis = analyze(&in_ogg).unwrap();
        assert_eq!(analysis.sample_rate, 48_000);
        assert_eq!(analysis.channels, 1);
        assert!(analysis.frames > 0);

        let exp = export_wav(&in_ogg, &out_wav, &[]).unwrap();
        assert!(exp.verified);
        let cmp = compare(&in_ogg, &out_wav).unwrap();
        assert_eq!(cmp.length_difference_frames, 0);
    }
}
