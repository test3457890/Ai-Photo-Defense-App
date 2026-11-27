//! H.264 MP4/MOV video frame decode, preview, time-ranged region redaction, and clean re-encode
//! using FilmCraft's `craft-video` H.264 decoder and encoder (storytold/craft-libs, MIT OR Apache-2.0, pure Rust).
//!
//! Supported: non-fragmented MP4/MOV with H.264 (`avc1`/`avc3`) video and optional passthrough audio.
//! Fragmented MP4, HEVC, AV1, VP9, and MKV/WebM are refused with a clear message.

use crate::mp4::{self, BoxRef, children, find, kids, u16_at, u32_at, u64_at};
use crate::{MAX_INSPECT_BYTES, MAX_NODES, MediaKind, detect};
use anyhow::{Context, Result, bail, ensure};
use craft_video::h264::decode::{Decoder, Picture};
use craft_video::h264::{Encoder, EncoderConfig, PacketFormat, Profile, RateControl, YuvFrame};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RedactRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub from_s: f64,
    pub to_s: f64,
    pub blur: bool,
}

#[derive(Debug, Clone)]
pub struct PreviewFrame {
    pub width: u32,
    pub height: u32,
    pub pts_s: f64,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoSummary {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration_s: f64,
    pub frame_count: usize,
    pub has_audio: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct VideoRedactReport {
    pub width: u32,
    pub height: u32,
    pub frame_count: usize,
    pub duration_s: f64,
    pub audio_samples: usize,
    pub av_sync_diff_s: f64,
    pub verified: bool,
    pub metadata_removed: bool,
}

#[derive(Debug, Clone)]
struct SampleSpan {
    offset: usize,
    size: usize,
    pts_ticks: u64,
}

type SttsEntries = Vec<(u32, u32)>;

#[derive(Debug, Clone)]
struct AudioTrackData {
    timescale: u32,
    duration_ticks: u64,
    stsd_box: Vec<u8>,
    stts_entries: Vec<(u32, u32)>,
    samples: Vec<SampleSpan>,
}

struct DemuxedMp4 {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
    video_timescale: u32,
    video_duration_ticks: u64,
    nal_len_size: usize,
    sps_pps_annexb: Vec<u8>,
    video_samples: Vec<SampleSpan>,
    audio: Option<AudioTrackData>,
}

fn parse_stts(b: &[u8], stts: &BoxRef) -> Result<Vec<(u32, u32)>> {
    let p = stts.payload();
    let count = u32_at(b, p + 4).context("stts count")? as usize;
    ensure!(p + 8 + count * 8 <= stts.end, "stts truncated");
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let c = u32_at(b, p + 8 + i * 8).context("stts sample_count")?;
        let d = u32_at(b, p + 12 + i * 8).context("stts sample_delta")?;
        out.push((c, d));
    }
    Ok(out)
}

fn parse_sample_table(
    b: &[u8],
    stbl: &[BoxRef],
    timescale: u32,
) -> Result<(Vec<SampleSpan>, SttsEntries)> {
    let stsz = find(stbl, b"stsz").context("Missing stsz")?;
    let p = stsz.payload();
    let uniform_size = u32_at(b, p + 4).context("stsz sample_size")?;
    let sample_count = u32_at(b, p + 8).context("stsz sample_count")? as usize;
    ensure!(sample_count <= 2_000_000, "Too many samples in track");

    let mut sizes = Vec::with_capacity(sample_count);
    if uniform_size != 0 {
        sizes.resize(sample_count, uniform_size as usize);
    } else {
        ensure!(p + 12 + sample_count * 4 <= stsz.end, "stsz truncated");
        for i in 0..sample_count {
            sizes.push(u32_at(b, p + 12 + i * 4).context("stsz entry")? as usize);
        }
    }

    let chunk_offsets: Vec<usize> = if let Some(stco) = find(stbl, b"stco") {
        let cp = stco.payload();
        let n = u32_at(b, cp + 4).context("stco count")? as usize;
        ensure!(cp + 8 + n * 4 <= stco.end, "stco truncated");
        (0..n)
            .map(|i| Ok(u32_at(b, cp + 8 + i * 4).context("stco entry")? as usize))
            .collect::<Result<Vec<_>>>()?
    } else if let Some(co64) = find(stbl, b"co64") {
        let cp = co64.payload();
        let n = u32_at(b, cp + 4).context("co64 count")? as usize;
        ensure!(cp + 8 + n * 8 <= co64.end, "co64 truncated");
        (0..n)
            .map(|i| {
                Ok(usize::try_from(
                    u64_at(b, cp + 8 + i * 8).context("co64 entry")?,
                )?)
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        bail!("Missing stco/co64");
    };

    let stsc = find(stbl, b"stsc").context("Missing stsc")?;
    let sp = stsc.payload();
    let stsc_n = u32_at(b, sp + 4).context("stsc count")? as usize;
    ensure!(sp + 8 + stsc_n * 12 <= stsc.end, "stsc truncated");
    let mut runs = Vec::with_capacity(stsc_n);
    for i in 0..stsc_n {
        let first_chunk = u32_at(b, sp + 8 + i * 12).context("stsc first_chunk")? as usize;
        let per_chunk = u32_at(b, sp + 12 + i * 12).context("stsc samples_per_chunk")? as usize;
        runs.push((first_chunk, per_chunk));
    }

    let stts_entries = if let Some(stts) = find(stbl, b"stts") {
        parse_stts(b, stts)?
    } else {
        vec![(sample_count as u32, timescale / 30)]
    };

    let mut durs = Vec::with_capacity(sample_count);
    for &(cnt, dur) in &stts_entries {
        for _ in 0..cnt {
            if durs.len() >= sample_count {
                break;
            }
            durs.push(dur);
        }
    }
    while durs.len() < sample_count {
        durs.push(durs.last().copied().unwrap_or(1));
    }

    let mut spans = Vec::with_capacity(sample_count);
    let mut sample_idx = 0usize;
    let mut pts = 0u64;
    for (chunk_idx, &chunk_off) in chunk_offsets.iter().enumerate() {
        let chunk_num = chunk_idx + 1;
        let per_chunk = runs
            .iter()
            .rev()
            .find(|(fc, _)| chunk_num >= *fc)
            .map(|(_, pc)| *pc)
            .unwrap_or(1);
        let mut off = chunk_off;
        for _ in 0..per_chunk {
            if sample_idx >= sample_count {
                break;
            }
            let size = sizes[sample_idx];
            ensure!(
                off.checked_add(size).is_some_and(|e| e <= b.len()),
                "Sample offset out of bounds"
            );
            let dur = durs[sample_idx];
            spans.push(SampleSpan {
                offset: off,
                size,
                pts_ticks: pts,
            });
            off += size;
            pts += u64::from(dur);
            sample_idx += 1;
        }
    }
    ensure!(spans.len() == sample_count, "Sample table count mismatch");
    Ok((spans, stts_entries))
}

fn parse_avcc(b: &[u8], avcc: &BoxRef) -> Result<(usize, Vec<u8>)> {
    let p = avcc.payload();
    ensure!(p + 7 <= avcc.end, "avcC box truncated");
    let nal_len_size = ((b[p + 4] & 0x03) + 1) as usize;
    let num_sps = (b[p + 5] & 0x1F) as usize;
    let mut pos = p + 6;
    let mut annexb = Vec::new();
    for _ in 0..num_sps {
        let len = u16_at(b, pos).context("avcC SPS len")? as usize;
        pos += 2;
        ensure!(pos + len <= avcc.end, "avcC SPS truncated");
        annexb.extend_from_slice(&[0, 0, 0, 1]);
        annexb.extend_from_slice(&b[pos..pos + len]);
        pos += len;
    }
    ensure!(pos < avcc.end, "avcC PPS count missing");
    let num_pps = b[pos] as usize;
    pos += 1;
    for _ in 0..num_pps {
        let len = u16_at(b, pos).context("avcC PPS len")? as usize;
        pos += 2;
        ensure!(pos + len <= avcc.end, "avcC PPS truncated");
        annexb.extend_from_slice(&[0, 0, 0, 1]);
        annexb.extend_from_slice(&b[pos..pos + len]);
        pos += len;
    }
    Ok((nal_len_size, annexb))
}

fn avcc_sample_to_annexb(sample: &[u8], nal_len_size: usize, out: &mut Vec<u8>) -> Result<()> {
    let mut pos = 0;
    while pos + nal_len_size <= sample.len() {
        let mut nal_len = 0usize;
        for i in 0..nal_len_size {
            nal_len = (nal_len << 8) | usize::from(sample[pos + i]);
        }
        pos += nal_len_size;
        ensure!(
            pos + nal_len <= sample.len(),
            "Truncated H.264 NAL unit in MP4 sample"
        );
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&sample[pos..pos + nal_len]);
        pos += nal_len;
    }
    Ok(())
}

fn demux_mp4(path: &Path) -> Result<DemuxedMp4> {
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink(),
        "Video input must be a regular file"
    );
    ensure!(
        meta.len() <= MAX_INSPECT_BYTES,
        "Video file exceeds size limit"
    );
    let bytes = std::fs::read(path)?;
    let det = detect(&bytes);
    if det.kind != MediaKind::Video {
        bail!("Not a video file (detected {})", det.container);
    }
    if !matches!(det.container, "MP4" | "MOV") {
        bail!(
            "Not supported: {} video is not supported (H.264 MP4/MOV only)",
            det.container
        );
    }

    let mut budget = MAX_NODES;
    let top = children(&bytes, 0, bytes.len(), &mut budget)?;
    if find(&top, b"moof").is_some() {
        bail!("Not supported: fragmented MP4 is not supported");
    }
    let moov = find(&top, b"moov").context("Missing moov box")?;
    let mk = kids(&bytes, moov, &mut budget)?;
    if find(&mk, b"mvex").is_some() {
        bail!("Not supported: fragmented MP4 (mvex) is not supported");
    }

    let mut video_info = None;
    let mut audio_info = None;

    for trak in mk.iter().filter(|b| b.is(b"trak")) {
        let tk = kids(&bytes, trak, &mut budget)?;
        let Some(mdia) = find(&tk, b"mdia") else {
            continue;
        };
        let mdia_k = kids(&bytes, mdia, &mut budget)?;
        let Some(hdlr) = find(&mdia_k, b"hdlr") else {
            continue;
        };
        let hp = hdlr.payload();
        let handler = bytes.get(hp + 8..hp + 12);
        let Some(mdhd) = find(&mdia_k, b"mdhd") else {
            continue;
        };
        let mp = mdhd.payload();
        let ver = bytes[mp];
        let (timescale, duration_ticks) = if ver == 1 {
            (
                u32_at(&bytes, mp + 20).unwrap_or(1),
                u64_at(&bytes, mp + 24).unwrap_or(0),
            )
        } else {
            (
                u32_at(&bytes, mp + 12).unwrap_or(1),
                u64::from(u32_at(&bytes, mp + 16).unwrap_or(0)),
            )
        };
        let Some(minf) = find(&mdia_k, b"minf") else {
            continue;
        };
        let minf_k = kids(&bytes, minf, &mut budget)?;
        let Some(stbl) = find(&minf_k, b"stbl") else {
            continue;
        };
        let stbl_k = kids(&bytes, stbl, &mut budget)?;
        let Some(stsd) = find(&stbl_k, b"stsd") else {
            continue;
        };
        let entries = kids(&bytes, stsd, &mut budget)?;

        if handler == Some(b"vide") && video_info.is_none() {
            let entry = entries.first().context("Empty video stsd")?;
            match &entry.kind {
                b"avc1" | b"avc3" => {}
                b"hvc1" | b"hev1" => bail!("Not supported: HEVC video is not supported"),
                b"av01" => bail!("Not supported: AV1 video is not supported"),
                b"vp09" | b"vp08" => bail!("Not supported: VP9/VP8 video is not supported"),
                other => bail!(
                    "Not supported: video codec {} is not supported",
                    String::from_utf8_lossy(other)
                ),
            }
            let ep = entry.payload();
            let width = u32::from(u16_at(&bytes, ep + 24).context("video width")?);
            let height = u32::from(u16_at(&bytes, ep + 26).context("video height")?);
            // avc1 has 78 bytes of VisualSampleEntry fields before child boxes
            let entry_kids = children(&bytes, ep + 78, entry.end, &mut budget)?;
            let avcc = find(&entry_kids, b"avcC").context("Missing avcC box in H.264 track")?;
            let (nal_len_size, sps_pps_annexb) = parse_avcc(&bytes, avcc)?;
            let (samples, _) = parse_sample_table(&bytes, &stbl_k, timescale)?;
            video_info = Some((
                width,
                height,
                timescale,
                duration_ticks,
                nal_len_size,
                sps_pps_annexb,
                samples,
            ));
        } else if handler == Some(b"soun") && audio_info.is_none() {
            let (samples, stts_entries) = parse_sample_table(&bytes, &stbl_k, timescale)?;
            audio_info = Some(AudioTrackData {
                timescale,
                duration_ticks,
                stsd_box: bytes[stsd.start..stsd.end].to_vec(),
                stts_entries,
                samples,
            });
        }
    }

    let (
        width,
        height,
        video_timescale,
        video_duration_ticks,
        nal_len_size,
        sps_pps_annexb,
        video_samples,
    ) = video_info.context("No H.264 video track found")?;
    ensure!(!video_samples.is_empty(), "Video track has 0 frames");

    Ok(DemuxedMp4 {
        bytes,
        width,
        height,
        video_timescale,
        video_duration_ticks,
        nal_len_size,
        sps_pps_annexb,
        video_samples,
        audio: audio_info,
    })
}

/// Converts 8-bit planar YUV 4:2:0 (`Picture`) to tight RGBA8.
pub fn yuv420_to_rgba(pic: &Picture) -> Vec<u8> {
    let w = pic.width as usize;
    let h = pic.height as usize;
    let mut rgba = vec![255u8; w * h * 4];
    for row in 0..h {
        let y_row = &pic.y[row * pic.y_stride..];
        let uv_row = (row / 2) * pic.uv_stride;
        for col in 0..w {
            let y = f32::from(y_row[col]) - 16.0;
            let u = f32::from(pic.u[uv_row + col / 2]) - 128.0;
            let v = f32::from(pic.v[uv_row + col / 2]) - 128.0;
            let r = (1.164 * y + 1.596 * v).round().clamp(0.0, 255.0) as u8;
            let g = (1.164 * y - 0.392 * u - 0.813 * v)
                .round()
                .clamp(0.0, 255.0) as u8;
            let b = (1.164 * y + 2.017 * u).round().clamp(0.0, 255.0) as u8;
            let idx = (row * w + col) * 4;
            rgba[idx] = r;
            rgba[idx + 1] = g;
            rgba[idx + 2] = b;
        }
    }
    rgba
}

fn box_blur_plane(
    plane: &mut [u8],
    stride: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    radius: usize,
) {
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let w = x1 - x0;
    let h = y1 - y0;
    let mut temp = vec![0u8; w * h];
    for r in 0..h {
        for c in 0..w {
            let mut sum = 0u32;
            let mut cnt = 0u32;
            let c_lo = c.saturating_sub(radius);
            let c_hi = (c + radius + 1).min(w);
            for cc in c_lo..c_hi {
                sum += u32::from(plane[(y0 + r) * stride + x0 + cc]);
                cnt += 1;
            }
            temp[r * w + c] = (sum / cnt.max(1)) as u8;
        }
    }
    for r in 0..h {
        for c in 0..w {
            let mut sum = 0u32;
            let mut cnt = 0u32;
            let r_lo = r.saturating_sub(radius);
            let r_hi = (r + radius + 1).min(h);
            for rr in r_lo..r_hi {
                sum += u32::from(temp[rr * w + c]);
                cnt += 1;
            }
            plane[(y0 + r) * stride + x0 + c] = (sum / cnt.max(1)) as u8;
        }
    }
}

fn apply_redactions_yuv(pic: &mut Picture, t_s: f64, rects: &[RedactRect]) {
    let width = pic.width as usize;
    let height = pic.height as usize;
    let y_stride = pic.y_stride;
    let uv_stride = pic.uv_stride;
    for r in rects {
        if t_s < r.from_s || t_s > r.to_s {
            continue;
        }
        let x0 = (r.x as usize).min(width);
        let y0 = (r.y as usize).min(height);
        let x1 = (r.x.saturating_add(r.width) as usize).min(width);
        let y1 = (r.y.saturating_add(r.height) as usize).min(height);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        let cx0 = x0 / 2;
        let cy0 = y0 / 2;
        let cx1 = x1.div_ceil(2).min(width.div_ceil(2));
        let cy1 = y1.div_ceil(2).min(height.div_ceil(2));
        if r.blur {
            for _ in 0..2 {
                box_blur_plane(&mut pic.y, y_stride, x0, y0, x1, y1, 12);
                box_blur_plane(&mut pic.u, uv_stride, cx0, cy0, cx1, cy1, 6);
                box_blur_plane(&mut pic.v, uv_stride, cx0, cy0, cx1, cy1, 6);
            }
        } else {
            for row in y0..y1 {
                pic.y[row * y_stride + x0..row * y_stride + x1].fill(16);
            }
            for row in cy0..cy1 {
                pic.u[row * uv_stride + cx0..row * uv_stride + cx1].fill(128);
                pic.v[row * uv_stride + cx0..row * uv_stride + cx1].fill(128);
            }
        }
    }
}

pub fn probe_video(path: &Path) -> Result<VideoSummary> {
    let d = demux_mp4(path)?;
    let duration_s = if d.video_timescale > 0 {
        d.video_duration_ticks as f64 / f64::from(d.video_timescale)
    } else {
        0.0
    };
    let fps = if duration_s > 0.0 {
        d.video_samples.len() as f64 / duration_s
    } else {
        30.0
    };
    Ok(VideoSummary {
        width: d.width,
        height: d.height,
        fps,
        duration_s,
        frame_count: d.video_samples.len(),
        has_audio: d.audio.is_some(),
    })
}

/// Decodes frames up to `target_s` in bounded memory and returns the closest frame as RGBA8.
pub fn decode_frame_at(path: &Path, target_s: f64) -> Result<PreviewFrame> {
    let d = demux_mp4(path)?;
    let mut dec = Decoder::new();
    let mut annexb = Vec::new();
    let mut best: Option<Picture> = None;
    let target_ticks = (target_s.max(0.0) * f64::from(d.video_timescale)).round() as u64;

    for (idx, s) in d.video_samples.iter().enumerate() {
        annexb.clear();
        if idx == 0 {
            annexb.extend_from_slice(&d.sps_pps_annexb);
        }
        avcc_sample_to_annexb(
            &d.bytes[s.offset..s.offset + s.size],
            d.nal_len_size,
            &mut annexb,
        )?;
        for pic in dec.decode(&annexb, s.pts_ticks as i64)? {
            let done = pic.pts as u64 >= target_ticks;
            best = Some(pic);
            if done {
                let p = best.take().expect("just set");
                let pts_s = p.pts as f64 / f64::from(d.video_timescale);
                return Ok(PreviewFrame {
                    width: p.width,
                    height: p.height,
                    pts_s,
                    rgba: yuv420_to_rgba(&p),
                });
            }
        }
    }
    for pic in dec.flush() {
        let done = pic.pts as u64 >= target_ticks;
        best = Some(pic);
        if done {
            break;
        }
    }
    let p = best.context("Failed to decode any video frame")?;
    let pts_s = p.pts as f64 / f64::from(d.video_timescale);
    Ok(PreviewFrame {
        width: p.width,
        height: p.height,
        pts_s,
        rgba: yuv420_to_rgba(&p),
    })
}

fn write_box(out: &mut Vec<u8>, kind: &[u8; 4], payload: &[u8]) {
    let total = 8 + payload.len();
    out.extend_from_slice(&(total as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(payload);
}

fn write_full_box(out: &mut Vec<u8>, kind: &[u8; 4], version: u8, flags: u32, payload: &[u8]) {
    let mut body = Vec::with_capacity(4 + payload.len());
    body.push(version);
    body.extend_from_slice(&flags.to_be_bytes()[1..4]);
    body.extend_from_slice(payload);
    write_box(out, kind, &body);
}

struct VideoTrakParams<'a> {
    width: u32,
    height: u32,
    timescale: u32,
    duration_ticks: u64,
    avcc: &'a [u8],
    sizes: &'a [u32],
    offsets: &'a [u32],
    stts: &'a [(u32, u32)],
}

fn build_video_trak(p: &VideoTrakParams<'_>) -> Vec<u8> {
    let (width, height, timescale, duration_ticks, avcc, sizes, offsets, stts) = (
        p.width,
        p.height,
        p.timescale,
        p.duration_ticks,
        p.avcc,
        p.sizes,
        p.offsets,
        p.stts,
    );
    let mut tkhd = Vec::new();
    tkhd.extend_from_slice(&0u32.to_be_bytes()); // ctime
    tkhd.extend_from_slice(&0u32.to_be_bytes()); // mtime
    tkhd.extend_from_slice(&1u32.to_be_bytes()); // track_id
    tkhd.extend_from_slice(&0u32.to_be_bytes()); // reserved
    tkhd.extend_from_slice(&(duration_ticks as u32).to_be_bytes());
    tkhd.extend_from_slice(&[0u8; 8]); // reserved
    tkhd.extend_from_slice(&0u16.to_be_bytes()); // layer
    tkhd.extend_from_slice(&0u16.to_be_bytes()); // alt_group
    tkhd.extend_from_slice(&0u16.to_be_bytes()); // volume
    tkhd.extend_from_slice(&0u16.to_be_bytes()); // reserved
    // unity matrix
    tkhd.extend_from_slice(&[
        0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0x40, 0x00, 0, 0,
    ]);
    tkhd.extend_from_slice(&(width << 16).to_be_bytes());
    tkhd.extend_from_slice(&(height << 16).to_be_bytes());

    let mut mdhd = Vec::new();
    mdhd.extend_from_slice(&0u32.to_be_bytes());
    mdhd.extend_from_slice(&0u32.to_be_bytes());
    mdhd.extend_from_slice(&timescale.to_be_bytes());
    mdhd.extend_from_slice(&(duration_ticks as u32).to_be_bytes());
    mdhd.extend_from_slice(&0x55C4u16.to_be_bytes()); // und
    mdhd.extend_from_slice(&0u16.to_be_bytes());

    let mut hdlr = Vec::new();
    hdlr.extend_from_slice(&0u32.to_be_bytes());
    hdlr.extend_from_slice(b"vide");
    hdlr.extend_from_slice(&[0u8; 12]);
    hdlr.extend_from_slice(b"VideoHandler\0");

    let mut vmhd = Vec::new();
    vmhd.extend_from_slice(&[0u8; 8]);

    let mut url = Vec::new();
    write_full_box(&mut url, b"url ", 0, 1, &[]);
    let mut dref_body = 1u32.to_be_bytes().to_vec();
    dref_body.extend_from_slice(&url);
    let mut dref = Vec::new();
    write_full_box(&mut dref, b"dref", 0, 0, &dref_body);
    let mut dinf = Vec::new();
    write_box(&mut dinf, b"dinf", &dref);

    let mut avc1_body = Vec::new();
    avc1_body.extend_from_slice(&[0u8; 6]);
    avc1_body.extend_from_slice(&1u16.to_be_bytes()); // data_ref_index
    avc1_body.extend_from_slice(&[0u8; 16]);
    avc1_body.extend_from_slice(&(width as u16).to_be_bytes());
    avc1_body.extend_from_slice(&(height as u16).to_be_bytes());
    avc1_body.extend_from_slice(&0x0048_0000u32.to_be_bytes());
    avc1_body.extend_from_slice(&0x0048_0000u32.to_be_bytes());
    avc1_body.extend_from_slice(&0u32.to_be_bytes());
    avc1_body.extend_from_slice(&1u16.to_be_bytes());
    avc1_body.extend_from_slice(&[0u8; 32]);
    avc1_body.extend_from_slice(&0x0018u16.to_be_bytes());
    avc1_body.extend_from_slice(&0xFFFFu16.to_be_bytes());
    write_box(&mut avc1_body, b"avcC", avcc);
    let mut stsd_body = 1u32.to_be_bytes().to_vec();
    write_box(&mut stsd_body, b"avc1", &avc1_body);
    let mut stsd = Vec::new();
    write_full_box(&mut stsd, b"stsd", 0, 0, &stsd_body);

    let mut stts_body = (stts.len() as u32).to_be_bytes().to_vec();
    for &(c, d) in stts {
        stts_body.extend_from_slice(&c.to_be_bytes());
        stts_body.extend_from_slice(&d.to_be_bytes());
    }
    let mut stts_box = Vec::new();
    write_full_box(&mut stts_box, b"stts", 0, 0, &stts_body);

    // 1 sample per chunk
    let mut stsc_body = 1u32.to_be_bytes().to_vec();
    stsc_body.extend_from_slice(&1u32.to_be_bytes());
    stsc_body.extend_from_slice(&1u32.to_be_bytes());
    stsc_body.extend_from_slice(&1u32.to_be_bytes());
    let mut stsc = Vec::new();
    write_full_box(&mut stsc, b"stsc", 0, 0, &stsc_body);

    let mut stsz_body = 0u32.to_be_bytes().to_vec();
    stsz_body.extend_from_slice(&(sizes.len() as u32).to_be_bytes());
    for &sz in sizes {
        stsz_body.extend_from_slice(&sz.to_be_bytes());
    }
    let mut stsz = Vec::new();
    write_full_box(&mut stsz, b"stsz", 0, 0, &stsz_body);

    let mut stco_body = (offsets.len() as u32).to_be_bytes().to_vec();
    for &off in offsets {
        stco_body.extend_from_slice(&off.to_be_bytes());
    }
    let mut stco = Vec::new();
    write_full_box(&mut stco, b"stco", 0, 0, &stco_body);

    let mut stbl_body = Vec::new();
    stbl_body.extend_from_slice(&stsd);
    stbl_body.extend_from_slice(&stts_box);
    stbl_body.extend_from_slice(&stsc);
    stbl_body.extend_from_slice(&stsz);
    stbl_body.extend_from_slice(&stco);
    let mut stbl = Vec::new();
    write_box(&mut stbl, b"stbl", &stbl_body);

    let mut minf_body = Vec::new();
    write_full_box(&mut minf_body, b"vmhd", 0, 1, &vmhd);
    minf_body.extend_from_slice(&dinf);
    minf_body.extend_from_slice(&stbl);
    let mut minf = Vec::new();
    write_box(&mut minf, b"minf", &minf_body);

    let mut mdia_body = Vec::new();
    write_full_box(&mut mdia_body, b"mdhd", 0, 0, &mdhd);
    write_full_box(&mut mdia_body, b"hdlr", 0, 0, &hdlr);
    mdia_body.extend_from_slice(&minf);
    let mut mdia = Vec::new();
    write_box(&mut mdia, b"mdia", &mdia_body);

    let mut trak_body = Vec::new();
    write_full_box(&mut trak_body, b"tkhd", 0, 3, &tkhd);
    trak_body.extend_from_slice(&mdia);
    let mut trak = Vec::new();
    write_box(&mut trak, b"trak", &trak_body);
    trak
}

fn build_audio_trak(audio: &AudioTrackData, mvhd_timescale: u32, offsets: &[u32]) -> Vec<u8> {
    let mvhd_dur = if audio.timescale > 0 {
        ((audio.duration_ticks as f64 / f64::from(audio.timescale)) * f64::from(mvhd_timescale))
            .round() as u32
    } else {
        0
    };
    let mut tkhd = Vec::new();
    tkhd.extend_from_slice(&0u32.to_be_bytes());
    tkhd.extend_from_slice(&0u32.to_be_bytes());
    tkhd.extend_from_slice(&2u32.to_be_bytes());
    tkhd.extend_from_slice(&0u32.to_be_bytes());
    tkhd.extend_from_slice(&mvhd_dur.to_be_bytes());
    tkhd.extend_from_slice(&[0u8; 8]);
    tkhd.extend_from_slice(&0u16.to_be_bytes());
    tkhd.extend_from_slice(&0u16.to_be_bytes());
    tkhd.extend_from_slice(&0x0100u16.to_be_bytes()); // volume 1.0
    tkhd.extend_from_slice(&0u16.to_be_bytes());
    tkhd.extend_from_slice(&[
        0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0x40, 0x00, 0, 0,
    ]);
    tkhd.extend_from_slice(&0u64.to_be_bytes());

    let mut mdhd = Vec::new();
    mdhd.extend_from_slice(&0u32.to_be_bytes());
    mdhd.extend_from_slice(&0u32.to_be_bytes());
    mdhd.extend_from_slice(&audio.timescale.to_be_bytes());
    mdhd.extend_from_slice(&(audio.duration_ticks as u32).to_be_bytes());
    mdhd.extend_from_slice(&0x55C4u16.to_be_bytes());
    mdhd.extend_from_slice(&0u16.to_be_bytes());

    let mut hdlr = Vec::new();
    hdlr.extend_from_slice(&0u32.to_be_bytes());
    hdlr.extend_from_slice(b"soun");
    hdlr.extend_from_slice(&[0u8; 12]);
    hdlr.extend_from_slice(b"SoundHandler\0");

    let mut url = Vec::new();
    write_full_box(&mut url, b"url ", 0, 1, &[]);
    let mut dref_body = 1u32.to_be_bytes().to_vec();
    dref_body.extend_from_slice(&url);
    let mut dref = Vec::new();
    write_full_box(&mut dref, b"dref", 0, 0, &dref_body);
    let mut dinf = Vec::new();
    write_box(&mut dinf, b"dinf", &dref);

    let mut stts_body = (audio.stts_entries.len() as u32).to_be_bytes().to_vec();
    for &(c, d) in &audio.stts_entries {
        stts_body.extend_from_slice(&c.to_be_bytes());
        stts_body.extend_from_slice(&d.to_be_bytes());
    }
    let mut stts_box = Vec::new();
    write_full_box(&mut stts_box, b"stts", 0, 0, &stts_body);

    let mut stsc_body = 1u32.to_be_bytes().to_vec();
    stsc_body.extend_from_slice(&1u32.to_be_bytes());
    stsc_body.extend_from_slice(&1u32.to_be_bytes());
    stsc_body.extend_from_slice(&1u32.to_be_bytes());
    let mut stsc = Vec::new();
    write_full_box(&mut stsc, b"stsc", 0, 0, &stsc_body);

    let mut stsz_body = 0u32.to_be_bytes().to_vec();
    stsz_body.extend_from_slice(&(audio.samples.len() as u32).to_be_bytes());
    for s in &audio.samples {
        stsz_body.extend_from_slice(&(s.size as u32).to_be_bytes());
    }
    let mut stsz = Vec::new();
    write_full_box(&mut stsz, b"stsz", 0, 0, &stsz_body);

    let mut stco_body = (offsets.len() as u32).to_be_bytes().to_vec();
    for &off in offsets {
        stco_body.extend_from_slice(&off.to_be_bytes());
    }
    let mut stco = Vec::new();
    write_full_box(&mut stco, b"stco", 0, 0, &stco_body);

    let mut stbl_body = Vec::new();
    stbl_body.extend_from_slice(&audio.stsd_box);
    stbl_body.extend_from_slice(&stts_box);
    stbl_body.extend_from_slice(&stsc);
    stbl_body.extend_from_slice(&stsz);
    stbl_body.extend_from_slice(&stco);
    let mut stbl = Vec::new();
    write_box(&mut stbl, b"stbl", &stbl_body);

    let mut minf_body = Vec::new();
    write_full_box(&mut minf_body, b"smhd", 0, 0, &[0u8; 4]);
    minf_body.extend_from_slice(&dinf);
    minf_body.extend_from_slice(&stbl);
    let mut minf = Vec::new();
    write_box(&mut minf, b"minf", &minf_body);

    let mut mdia_body = Vec::new();
    write_full_box(&mut mdia_body, b"mdhd", 0, 0, &mdhd);
    write_full_box(&mut mdia_body, b"hdlr", 0, 0, &hdlr);
    mdia_body.extend_from_slice(&minf);
    let mut mdia = Vec::new();
    write_box(&mut mdia, b"mdia", &mdia_body);

    let mut trak_body = Vec::new();
    write_full_box(&mut trak_body, b"tkhd", 0, 3, &tkhd);
    trak_body.extend_from_slice(&mdia);
    let mut trak = Vec::new();
    write_box(&mut trak, b"trak", &trak_body);
    trak
}

/// Decodes an H.264 MP4/MOV frame-by-frame, burns redactions, re-encodes with `craft-video`,
/// copies original audio samples unchanged into a metadata-free MP4, and verifies the output.
pub fn redact_mp4(input: &Path, output: &Path, rects: &[RedactRect]) -> Result<VideoRedactReport> {
    ensure!(
        output
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("mp4") || e.eq_ignore_ascii_case("mov")),
        "Output must be .mp4 or .mov"
    );
    let d = demux_mp4(input)?;
    let fps_num = if d.video_duration_ticks > 0 {
        ((d.video_samples.len() as f64 * f64::from(d.video_timescale))
            / d.video_duration_ticks as f64)
            .round()
            .clamp(1.0, 240.0) as u32
    } else {
        30
    };

    let mut cfg = EncoderConfig::new(d.width, d.height, fps_num, 1);
    cfg.profile = Profile::Main;
    cfg.bframes = 0;
    cfg.rate = RateControl::Crf(18.0);
    cfg.format = PacketFormat::LengthPrefixed;
    let mut enc = Encoder::new(cfg).map_err(|e| anyhow::anyhow!("H.264 encoder init: {e}"))?;
    let avcc = enc.avcc();

    let mut dec = Decoder::new();
    let mut annexb = Vec::new();
    let mut mdat_payload = Vec::new();
    let mut video_sizes = Vec::with_capacity(d.video_samples.len());
    let mut video_offsets = Vec::with_capacity(d.video_samples.len());

    let ftyp = {
        let mut b = Vec::new();
        let mut body = Vec::new();
        body.extend_from_slice(b"isom");
        body.extend_from_slice(&512u32.to_be_bytes());
        body.extend_from_slice(b"isomiso2avc1mp41");
        write_box(&mut b, b"ftyp", &body);
        b
    };
    let mdat_data_start = ftyp.len() + 8;

    let mut frame_idx = 0i64;
    let mut process_pic = |mut pic: Picture| -> Result<()> {
        let t_s = pic.pts as f64 / f64::from(d.video_timescale);
        apply_redactions_yuv(&mut pic, t_s, rects);
        let frame = YuvFrame {
            y: &pic.y,
            u: &pic.u,
            v: &pic.v,
            y_stride: pic.y_stride,
            uv_stride: pic.uv_stride,
        };
        let pkts = enc
            .encode(&frame, frame_idx)
            .map_err(|e| anyhow::anyhow!("H.264 encode: {e}"))?;
        frame_idx += 1;
        for pkt in pkts {
            video_offsets.push((mdat_data_start + mdat_payload.len()) as u32);
            video_sizes.push(pkt.data.len() as u32);
            mdat_payload.extend_from_slice(&pkt.data);
        }
        Ok(())
    };

    for (idx, s) in d.video_samples.iter().enumerate() {
        annexb.clear();
        if idx == 0 {
            annexb.extend_from_slice(&d.sps_pps_annexb);
        }
        avcc_sample_to_annexb(
            &d.bytes[s.offset..s.offset + s.size],
            d.nal_len_size,
            &mut annexb,
        )?;
        for pic in dec.decode(&annexb, s.pts_ticks as i64)? {
            process_pic(pic)?;
        }
    }
    for pic in dec.flush() {
        process_pic(pic)?;
    }
    for pkt in enc.flush() {
        video_offsets.push((mdat_data_start + mdat_payload.len()) as u32);
        video_sizes.push(pkt.data.len() as u32);
        mdat_payload.extend_from_slice(&pkt.data);
    }

    let frame_count = video_sizes.len();
    ensure!(frame_count > 0, "Encoder produced 0 frames");

    let mut audio_offsets = Vec::new();
    let mut audio_samples = 0usize;
    if let Some(a) = &d.audio {
        audio_samples = a.samples.len();
        for s in &a.samples {
            audio_offsets.push((mdat_data_start + mdat_payload.len()) as u32);
            mdat_payload.extend_from_slice(&d.bytes[s.offset..s.offset + s.size]);
        }
    }

    let out_timescale = fps_num;
    let out_duration = frame_count as u64;
    let video_trak = build_video_trak(&VideoTrakParams {
        width: d.width,
        height: d.height,
        timescale: out_timescale,
        duration_ticks: out_duration,
        avcc: &avcc,
        sizes: &video_sizes,
        offsets: &video_offsets,
        stts: &[(frame_count as u32, 1)],
    });

    let next_track_id = if d.audio.is_some() { 3u32 } else { 2u32 };
    let mut mvhd = Vec::new();
    mvhd.extend_from_slice(&0u32.to_be_bytes());
    mvhd.extend_from_slice(&0u32.to_be_bytes());
    mvhd.extend_from_slice(&out_timescale.to_be_bytes());
    mvhd.extend_from_slice(&(out_duration as u32).to_be_bytes());
    mvhd.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    mvhd.extend_from_slice(&0x0100u16.to_be_bytes());
    mvhd.extend_from_slice(&[0u8; 10]);
    mvhd.extend_from_slice(&[
        0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0x40, 0x00, 0, 0,
    ]);
    mvhd.extend_from_slice(&[0u8; 24]);
    mvhd.extend_from_slice(&next_track_id.to_be_bytes());

    let mut moov_body = Vec::new();
    write_full_box(&mut moov_body, b"mvhd", 0, 0, &mvhd);
    moov_body.extend_from_slice(&video_trak);
    if let Some(a) = &d.audio {
        moov_body.extend_from_slice(&build_audio_trak(a, out_timescale, &audio_offsets));
    }

    let mut out_bytes =
        Vec::with_capacity(ftyp.len() + 8 + mdat_payload.len() + moov_body.len() + 16);
    out_bytes.extend_from_slice(&ftyp);
    write_box(&mut out_bytes, b"mdat", &mdat_payload);
    write_box(&mut out_bytes, b"moov", &moov_body);

    let dir = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    ensure!(dir.is_dir(), "Output folder must exist");
    let tmp = tempfile::NamedTempFile::new_in(dir)?;
    std::fs::write(tmp.path(), &out_bytes)?;
    tmp.persist(output)
        .map_err(|e| anyhow::anyhow!("Failed to save MP4: {}", e.error))?;

    // Reopen and verify
    let reopened = std::fs::read(output)?;
    let info = mp4::probe(&reopened)?;
    let v_track = info
        .tracks
        .iter()
        .find(|t| t.kind == "video")
        .context("Reopened MP4 missing video track")?;
    let a_track = info.tracks.iter().find(|t| t.kind == "audio");
    let duration_s = v_track.duration_s.unwrap_or(0.0);
    let av_sync_diff_s = if let Some(at) = a_track {
        (duration_s - at.duration_s.unwrap_or(duration_s)).abs()
    } else {
        0.0
    };
    let one_frame_s = 1.0 / f64::from(fps_num);
    ensure!(
        v_track.samples == Some(frame_count as u64),
        "Reopened frame count mismatch"
    );
    ensure!(
        info.findings.is_empty(),
        "Reopened MP4 still contains metadata findings"
    );
    if a_track.is_some() {
        ensure!(
            a_track.and_then(|t| t.samples) == Some(audio_samples as u64),
            "Reopened audio sample count mismatch"
        );
        ensure!(
            av_sync_diff_s < one_frame_s,
            "A/V duration difference {av_sync_diff_s:.4}s exceeds 1 frame ({one_frame_s:.4}s)"
        );
    }

    let frame_verify = crate::video_track::verify_redacted_mp4(output, rects)?;
    ensure!(
        frame_verify.passed,
        "Independent frame-level export verification failed ({} of {} redacted frames failed)",
        frame_verify.failed_frames,
        frame_verify.redacted_frames_checked
    );

    Ok(VideoRedactReport {
        width: d.width,
        height: d.height,
        frame_count,
        duration_s,
        audio_samples,
        av_sync_diff_s,
        verified: frame_verify.passed,
        metadata_removed: frame_verify.metadata_clean,
    })
}

pub(crate) struct LumaFrame {
    pub pts_s: f64,
    pub width: u32,
    pub height: u32,
    pub y: Vec<u8>,
}

pub(crate) fn decode_all_luma(path: &Path) -> Result<Vec<LumaFrame>> {
    let d = demux_mp4(path)?;
    let mut dec = Decoder::new();
    let mut annexb = Vec::new();
    let mut out = Vec::with_capacity(d.video_samples.len());
    let mut push_pic = |pic: Picture| {
        let mut y = Vec::with_capacity((d.width * d.height) as usize);
        for row in 0..d.height as usize {
            let start = row * pic.y_stride;
            y.extend_from_slice(&pic.y[start..start + d.width as usize]);
        }
        out.push(LumaFrame {
            pts_s: pic.pts as f64 / f64::from(d.video_timescale),
            width: d.width,
            height: d.height,
            y,
        });
    };
    for (idx, s) in d.video_samples.iter().enumerate() {
        annexb.clear();
        if idx == 0 {
            annexb.extend_from_slice(&d.sps_pps_annexb);
        }
        avcc_sample_to_annexb(
            &d.bytes[s.offset..s.offset + s.size],
            d.nal_len_size,
            &mut annexb,
        )?;
        for pic in dec.decode(&annexb, s.pts_ticks as i64)? {
            push_pic(pic);
        }
    }
    for pic in dec.flush() {
        push_pic(pic);
    }
    Ok(out)
}

/// Generates a minimal valid H.264 MP4 in memory for testing.
pub fn write_synthetic_h264_mp4(
    output: &Path,
    width: u32,
    height: u32,
    fps: u32,
    frames_rgb: &[(u8, u8, u8)],
) -> Result<()> {
    let mut cfg = EncoderConfig::new(width, height, fps, 1);
    cfg.profile = Profile::Baseline;
    cfg.bframes = 0;
    cfg.rate = RateControl::Qp(20);
    cfg.format = PacketFormat::LengthPrefixed;
    let mut enc = Encoder::new(cfg).map_err(|e| anyhow::anyhow!("H.264 encoder: {e}"))?;
    let avcc = enc.avcc();

    let w = width as usize;
    let h = height as usize;
    let cw = w / 2;
    let ch = h / 2;

    let ftyp = {
        let mut b = Vec::new();
        let mut body = Vec::new();
        body.extend_from_slice(b"isom");
        body.extend_from_slice(&512u32.to_be_bytes());
        body.extend_from_slice(b"isomiso2avc1mp41");
        write_box(&mut b, b"ftyp", &body);
        b
    };
    let mdat_start = ftyp.len() + 8;
    let mut mdat_payload = Vec::new();
    let mut sizes = Vec::new();
    let mut offsets = Vec::new();

    for (idx, &(r, g, b)) in frames_rgb.iter().enumerate() {
        let y_val = (0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b))
            .round()
            .clamp(16.0, 235.0) as u8;
        let u_val = (-0.169 * f32::from(r) - 0.331 * f32::from(g) + 0.5 * f32::from(b) + 128.0)
            .round()
            .clamp(16.0, 240.0) as u8;
        let v_val = (0.5 * f32::from(r) - 0.419 * f32::from(g) - 0.081 * f32::from(b) + 128.0)
            .round()
            .clamp(16.0, 240.0) as u8;
        let y = vec![y_val; w * h];
        let u = vec![u_val; cw * ch];
        let v = vec![v_val; cw * ch];
        let frame = YuvFrame {
            y: &y,
            u: &u,
            v: &v,
            y_stride: w,
            uv_stride: cw,
        };
        for pkt in enc
            .encode(&frame, idx as i64)
            .map_err(|e| anyhow::anyhow!("encode: {e}"))?
        {
            offsets.push((mdat_start + mdat_payload.len()) as u32);
            sizes.push(pkt.data.len() as u32);
            mdat_payload.extend_from_slice(&pkt.data);
        }
    }
    for pkt in enc.flush() {
        offsets.push((mdat_start + mdat_payload.len()) as u32);
        sizes.push(pkt.data.len() as u32);
        mdat_payload.extend_from_slice(&pkt.data);
    }

    let trak = build_video_trak(&VideoTrakParams {
        width,
        height,
        timescale: fps,
        duration_ticks: sizes.len() as u64,
        avcc: &avcc,
        sizes: &sizes,
        offsets: &offsets,
        stts: &[(sizes.len() as u32, 1)],
    });
    let mut mvhd = Vec::new();
    mvhd.extend_from_slice(&0u32.to_be_bytes());
    mvhd.extend_from_slice(&0u32.to_be_bytes());
    mvhd.extend_from_slice(&fps.to_be_bytes());
    mvhd.extend_from_slice(&(sizes.len() as u32).to_be_bytes());
    mvhd.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    mvhd.extend_from_slice(&0x0100u16.to_be_bytes());
    mvhd.extend_from_slice(&[0u8; 10]);
    mvhd.extend_from_slice(&[
        0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0x40, 0x00, 0, 0,
    ]);
    mvhd.extend_from_slice(&[0u8; 24]);
    mvhd.extend_from_slice(&2u32.to_be_bytes());

    let mut moov_body = Vec::new();
    write_full_box(&mut moov_body, b"mvhd", 0, 0, &mvhd);
    moov_body.extend_from_slice(&trak);

    let mut out = Vec::new();
    out.extend_from_slice(&ftyp);
    write_box(&mut out, b"mdat", &mdat_payload);
    write_box(&mut out, b"moov", &moov_body);
    std::fs::write(output, out)?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn write_raw_h264_mp4(
    output: &Path,
    width: u32,
    height: u32,
    fps: u32,
    avcc: &[u8],
    sizes: &[u32],
    mdat_payload: &[u8],
) -> Result<()> {
    let ftyp = {
        let mut b = Vec::new();
        let mut body = Vec::new();
        body.extend_from_slice(b"isom");
        body.extend_from_slice(&512u32.to_be_bytes());
        body.extend_from_slice(b"isomiso2avc1mp41");
        write_box(&mut b, b"ftyp", &body);
        b
    };
    let mdat_start = ftyp.len() + 8;
    let mut offsets = Vec::with_capacity(sizes.len());
    let mut cur = 0usize;
    for &sz in sizes {
        offsets.push((mdat_start + cur) as u32);
        cur += sz as usize;
    }
    let trak = build_video_trak(&VideoTrakParams {
        width,
        height,
        timescale: fps,
        duration_ticks: sizes.len() as u64,
        avcc,
        sizes,
        offsets: &offsets,
        stts: &[(sizes.len() as u32, 1)],
    });
    let mut mvhd = Vec::new();
    mvhd.extend_from_slice(&0u32.to_be_bytes());
    mvhd.extend_from_slice(&0u32.to_be_bytes());
    mvhd.extend_from_slice(&fps.to_be_bytes());
    mvhd.extend_from_slice(&(sizes.len() as u32).to_be_bytes());
    mvhd.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    mvhd.extend_from_slice(&0x0100u16.to_be_bytes());
    mvhd.extend_from_slice(&[0u8; 10]);
    mvhd.extend_from_slice(&[
        0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0x40, 0x00, 0, 0,
    ]);
    mvhd.extend_from_slice(&[0u8; 24]);
    mvhd.extend_from_slice(&2u32.to_be_bytes());
    let mut moov_body = Vec::new();
    write_full_box(&mut moov_body, b"mvhd", 0, 0, &mvhd);
    moov_body.extend_from_slice(&trak);
    let mut out = Vec::new();
    out.extend_from_slice(&ftyp);
    write_box(&mut out, b"mdat", mdat_payload);
    write_box(&mut out, b"moov", &moov_body);
    std::fs::write(output, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_redact_decode_and_verify_region() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("in.mp4");
        let dst = dir.path().join("out.mp4");
        // 6 white-ish frames at 10 fps -> 0.6s total
        let frames = vec![(220u8, 220u8, 220u8); 6];
        write_synthetic_h264_mp4(&src, 64, 64, 10, &frames).unwrap();

        let rect = RedactRect {
            x: 16,
            y: 16,
            width: 32,
            height: 32,
            from_s: 0.15,
            to_s: 0.45,
            blur: false,
        };
        let report = redact_mp4(&src, &dst, &[rect]).unwrap();
        assert!(report.verified);
        assert_eq!(report.frame_count, 6);

        // Check affected frame at t = 0.2s (inside [0.15, 0.45]): center pixel is black (redacted),
        // corner pixel is still bright (untouched).
        let f_mid = decode_frame_at(&dst, 0.2).unwrap();
        let center_idx = (32 * 64 + 32) * 4;
        assert!(
            f_mid.rgba[center_idx] < 40,
            "center R={}",
            f_mid.rgba[center_idx]
        );
        let corner_idx = (2 * 64 + 2) * 4;
        assert!(
            f_mid.rgba[corner_idx] > 180,
            "corner R={}",
            f_mid.rgba[corner_idx]
        );

        // Check unaffected frame at t = 0.0s (before 0.15s): center pixel is still bright.
        let f_start = decode_frame_at(&dst, 0.0).unwrap();
        assert!(
            f_start.rgba[center_idx] > 180,
            "start center R={}",
            f_start.rgba[center_idx]
        );
    }
}
