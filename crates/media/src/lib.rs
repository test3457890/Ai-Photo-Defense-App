//! Multimedia foundation (Part 4).
//!
//! - `detect`: identify the actual format from content, never from the file extension.
//! - `mp4`: ISO-BMFF (MP4, MOV, M4A, 3GP) structure, tracks, codecs and privacy metadata, plus
//!   lossless metadata removal that rewrites boxes and patches chunk offsets without re-encoding.
//! - `mkv`: Matroska/WebM tracks, tags and attachments (inspection only).
//! - `audio`: streaming decode (Symphonia), analysis (peak, true-peak estimate, loudness, clipping,
//!   waveform, spectrogram), comparison metrics, mute/bleep privacy redaction and WAV export.
//! - `protect`: psychoacoustically bounded audio perturbation engine. It needs a validated model
//!   objective; without one it refuses to run, so no unvalidated "protection" is ever applied.
//! - `subtitles`: SRT/WebVTT parsing, Privacy Scan of cue text, cue redaction.
//! - `temporal`: frame-sequence consistency and flicker metrics for video protection research.
use serde::Serialize;

pub mod audio;
pub mod detect;
pub mod mkv;
pub mod mp4;
pub mod protect;
#[cfg(feature = "stt")]
pub mod stt;
pub mod subtitles;
pub mod temporal;
pub mod wake_word;

pub use detect::{MediaKind, detect};

/// Upper bound on any media file read fully into memory for inspection or rewriting.
pub const MAX_INSPECT_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// Upper bound on box / element counts walked in a container.
pub const MAX_NODES: usize = 200_000;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Track {
    pub id: u32,
    pub kind: String,
    pub codec: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_rate: Option<f64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub duration_s: Option<f64>,
    pub language: Option<String>,
    pub samples: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaInfo {
    pub kind: MediaKind,
    pub container: String,
    pub duration_s: Option<f64>,
    pub tracks: Vec<Track>,
    pub findings: Vec<photo_privacy::Finding>,
    pub warnings: Vec<String>,
}

/// Inspects any supported media file from its bytes.
pub fn inspect(bytes: &[u8]) -> anyhow::Result<MediaInfo> {
    let d = detect(bytes);
    match d.container {
        "MP4" | "MOV" | "M4A" | "3GP" | "HEIF" | "AVIF" => mp4::probe(bytes),
        "Matroska" | "WebM" => mkv::probe(bytes),
        _ if d.kind == MediaKind::Audio => audio::probe_bytes(bytes, d.container),
        _ if d.kind == MediaKind::Subtitle => subtitles::inspect(bytes, d.container),
        _ => Ok(MediaInfo {
            kind: d.kind,
            container: d.container.into(),
            duration_s: None,
            tracks: vec![],
            findings: vec![],
            warnings: vec![format!(
                "{}: structural inspection is not implemented for this container",
                d.container
            )],
        }),
    }
}
