//! ISO base media files (MP4, MOV, M4A, 3GP): bounded box parsing, track and codec
//! inspection, privacy metadata detection and lossless metadata removal.
//!
//! Removal never re-encodes. It drops metadata boxes (udta, meta, uuid/XMP, loci), timed
//! metadata tracks (Apple location/sensor tracks), zeroes creation/modification times, then
//! patches every chunk offset (stco/co64) so the untouched media data stays addressable. The
//! result is reopened and every chunk's bytes are compared with the original.
use crate::{MAX_NODES, MediaInfo, Track};
use anyhow::{Context, Result, bail, ensure};
use photo_privacy::{Category, Evidence, Finding, Source};
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
struct BoxRef {
    kind: [u8; 4],
    start: usize,
    header: usize,
    end: usize,
}
impl BoxRef {
    fn payload(&self) -> usize {
        self.start + self.header
    }
    fn is(&self, k: &[u8; 4]) -> bool {
        &self.kind == k
    }
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        b.get(i..i.checked_add(2)?)?.try_into().ok()?,
    ))
}
fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        b.get(i..i.checked_add(4)?)?.try_into().ok()?,
    ))
}
fn u64_at(b: &[u8], i: usize) -> Option<u64> {
    Some(u64::from_be_bytes(
        b.get(i..i.checked_add(8)?)?.try_into().ok()?,
    ))
}

fn children(b: &[u8], start: usize, end: usize, budget: &mut usize) -> Result<Vec<BoxRef>> {
    let mut out = vec![];
    let mut i = start;
    while i + 8 <= end {
        ensure!(*budget > 0, "Too many boxes");
        *budget -= 1;
        let size32 = u32_at(b, i).context("box size")? as u64;
        let kind: [u8; 4] = b[i + 4..i + 8].try_into().expect("4 bytes");
        let (size, header) = match size32 {
            1 => (u64_at(b, i + 8).context("large size")?, 16usize),
            0 => ((end - i) as u64, 8),
            s => (s, 8),
        };
        let header = if &kind == b"uuid" {
            header + 16
        } else {
            header
        };
        ensure!(size >= header as u64, "Box smaller than its header");
        let box_end = i
            .checked_add(usize::try_from(size)?)
            .context("box overflow")?;
        ensure!(
            box_end <= end,
            "Box extends past its parent ({} bytes over)",
            box_end - end
        );
        out.push(BoxRef {
            kind,
            start: i,
            header,
            end: box_end,
        });
        i = box_end;
    }
    Ok(out)
}
/// Child list of a container; `meta` is a full box in ISO files but not in QuickTime.
fn kids(b: &[u8], p: &BoxRef, budget: &mut usize) -> Result<Vec<BoxRef>> {
    if p.is(b"meta") {
        let iso = b.get(p.payload() + 8..p.payload() + 12) == Some(b"hdlr");
        let off = if iso { 4 } else { 0 };
        return children(b, p.payload() + off, p.end, budget);
    }
    if p.is(b"stsd") {
        return children(b, p.payload() + 8, p.end, budget);
    }
    children(b, p.payload(), p.end, budget)
}
fn find<'a>(list: &'a [BoxRef], k: &[u8; 4]) -> Option<&'a BoxRef> {
    list.iter().find(|x| x.is(k))
}

fn finding(cat: Category, d: impl Into<String>) -> Finding {
    Finding::new(
        cat,
        Evidence::Confirmed,
        Source::Metadata,
        None,
        d.into(),
        None,
    )
}

const XMP_UUID: [u8; 16] = [
    0xbe, 0x7a, 0xcf, 0xcb, 0x97, 0xa9, 0x42, 0xe8, 0x9c, 0x71, 0x99, 0x94, 0x91, 0xe3, 0xaf, 0xac,
];

fn scan_metadata(
    b: &[u8],
    container: &BoxRef,
    findings: &mut Vec<Finding>,
    budget: &mut usize,
    depth: usize,
) -> Result<()> {
    if depth > 12 {
        return Ok(());
    }
    let payload = &b[container.payload().min(container.end)..container.end];
    let has = |s: &[u8]| payload.windows(s.len()).any(|w| w == s);
    if has(b"location.ISO6709") || has(b"\xa9xyz") || has(b"loci") {
        findings.push(finding(
            Category::GpsLocation,
            "Location metadata (QuickTime/3GPP)",
        ));
    }
    if has(b"\xa9mak") || has(b"\xa9mod") || has(b"quicktime.make") || has(b"quicktime.model") {
        findings.push(finding(Category::DeviceInformation, "Camera make/model"));
    }
    if has(b"\xa9swr") || has(b"\xa9too") || has(b"quicktime.software") {
        findings.push(finding(Category::DeviceInformation, "Recording software"));
    }
    if has(b"\xa9day") || has(b"creationdate") {
        findings.push(finding(Category::Timestamp, "Recording date"));
    }
    if has(b"\xa9cmt")
        || has(b"\xa9des")
        || has(b"desc")
        || has(b"\xa9nam")
        || has(b"\xa9ART")
        || has(b"\xa9aut")
    {
        findings.push(finding(
            Category::TextMetadata,
            "Text tags (title, author, comment or description)",
        ));
    }
    if has(b"covr") {
        findings.push(finding(Category::EmbeddedImage, "Embedded cover image"));
    }
    let _ = budget;
    Ok(())
}

pub fn probe(b: &[u8]) -> Result<MediaInfo> {
    let det = crate::detect(b);
    let mut budget = MAX_NODES;
    let top = children(b, 0, b.len(), &mut budget)?;
    let mut info = MediaInfo {
        kind: det.kind,
        container: det.container.into(),
        duration_s: None,
        tracks: vec![],
        findings: vec![],
        warnings: vec![],
    };
    if find(&top, b"moof").is_some() {
        info.warnings
            .push("Fragmented MP4 (moof): per-fragment data was not inspected".into());
    }
    for t in &top {
        match &t.kind {
            b"udta" | b"meta" => scan_metadata(b, t, &mut info.findings, &mut budget, 0)?,
            b"uuid" if b.get(t.start + 8..t.start + 24) == Some(&XMP_UUID) => info
                .findings
                .push(finding(Category::XmpMetadata, "XMP packet (uuid box)")),
            b"uuid" => info
                .findings
                .push(finding(Category::OtherMetadata, "Vendor uuid box")),
            b"XMP_" => info
                .findings
                .push(finding(Category::XmpMetadata, "XMP packet")),
            _ => {}
        }
    }
    let Some(moov) = find(&top, b"moov").copied() else {
        info.warnings
            .push("No movie header (moov) found: file is incomplete or not a media file".into());
        return Ok(info);
    };
    let mk = kids(b, &moov, &mut budget)?;
    if let Some(m) = find(&mk, b"mvhd") {
        let p = m.payload();
        let v = b.get(p).copied().unwrap_or(0);
        let (ctime, ts, dur) = if v == 1 {
            (
                u64_at(b, p + 4).unwrap_or(0),
                u32_at(b, p + 20).unwrap_or(0),
                u64_at(b, p + 24).unwrap_or(0),
            )
        } else {
            (
                u64::from(u32_at(b, p + 4).unwrap_or(0)),
                u32_at(b, p + 12).unwrap_or(0),
                u64::from(u32_at(b, p + 16).unwrap_or(0)),
            )
        };
        if ts > 0 {
            info.duration_s = Some(dur as f64 / f64::from(ts));
        }
        if ctime != 0 {
            info.findings
                .push(finding(Category::Timestamp, "Movie creation time (mvhd)"));
        }
    }
    for m in mk.iter().filter(|x| x.is(b"udta") || x.is(b"meta")) {
        scan_metadata(b, m, &mut info.findings, &mut budget, 0)?;
    }
    for trak in mk.iter().filter(|x| x.is(b"trak")) {
        let mut t = probe_track(b, trak, &mut budget, &mut info.findings)?;
        // Presentation size in tkhd is only meaningful for visual tracks.
        if t.kind != "video" {
            t.width = None;
            t.height = None;
        }
        info.tracks.push(t);
    }
    // One finding per category/description is enough for reporting.
    let mut seen = std::collections::HashSet::new();
    info.findings
        .retain(|f| seen.insert((f.category, f.description.clone())));
    if info.tracks.iter().any(|t| t.kind == "subtitle") {
        info.warnings.push(
            "Subtitle or text track present: its text may contain private information".into(),
        );
    }
    Ok(info)
}

fn probe_track(
    b: &[u8],
    trak: &BoxRef,
    budget: &mut usize,
    findings: &mut Vec<Finding>,
) -> Result<Track> {
    let tk = kids(b, trak, budget)?;
    let mut t = Track {
        id: 0,
        kind: "unknown".into(),
        codec: "unknown".into(),
        width: None,
        height: None,
        frame_rate: None,
        sample_rate: None,
        channels: None,
        duration_s: None,
        language: None,
        samples: None,
    };
    if let Some(h) = find(&tk, b"tkhd") {
        let p = h.payload();
        let v = b.get(p).copied().unwrap_or(0);
        t.id = if v == 1 {
            u32_at(b, p + 20)
        } else {
            u32_at(b, p + 12)
        }
        .unwrap_or(0);
        let (wo, ho) = if v == 1 {
            (p + 88, p + 92)
        } else {
            (p + 76, p + 80)
        };
        let w = u32_at(b, wo).unwrap_or(0) >> 16;
        let hh = u32_at(b, ho).unwrap_or(0) >> 16;
        if w > 0 && hh > 0 {
            t.width = Some(w);
            t.height = Some(hh);
        }
    }
    for m in tk.iter().filter(|x| x.is(b"udta") || x.is(b"meta")) {
        scan_metadata(b, m, findings, budget, 1)?;
    }
    let Some(mdia) = find(&tk, b"mdia") else {
        return Ok(t);
    };
    let md = kids(b, mdia, budget)?;
    if let Some(h) = find(&md, b"mdhd") {
        let p = h.payload();
        let v = b.get(p).copied().unwrap_or(0);
        let (ts, dur, lang) = if v == 1 {
            (u32_at(b, p + 20), u64_at(b, p + 24), u16_at(b, p + 32))
        } else {
            (
                u32_at(b, p + 12),
                u32_at(b, p + 16).map(u64::from),
                u16_at(b, p + 20),
            )
        };
        if let (Some(ts), Some(dur)) = (ts, dur)
            && ts > 0
        {
            t.duration_s = Some(dur as f64 / f64::from(ts));
        }
        if let Some(l) = lang
            && l != 0
            && l != 0x7fff
        {
            let c = |s: u16| char::from(((s & 0x1f) as u8) + 0x60);
            t.language = Some([c(l >> 10), c(l >> 5), c(l)].iter().collect());
        }
    }
    if let Some(h) = find(&md, b"hdlr") {
        t.kind = match b.get(h.payload() + 8..h.payload() + 12).unwrap_or_default() {
            b"vide" => "video",
            b"soun" => "audio",
            b"subt" | b"sbtl" | b"text" | b"clcp" => "subtitle",
            b"meta" => "timed metadata",
            b"hint" => "hint",
            _ => "other",
        }
        .into();
    }
    if t.kind == "timed metadata" {
        findings.push(finding(
            Category::HiddenLayers,
            "Timed metadata track (may carry location or sensor data)",
        ));
    }
    let stbl = find(&md, b"minf")
        .and_then(|m| kids(b, m, budget).ok())
        .and_then(|mi| find(&mi, b"stbl").copied());
    if let Some(stbl) = stbl {
        let sk = kids(b, &stbl, budget)?;
        if let Some(stsd) = find(&sk, b"stsd")
            && let Some(e) = kids(b, stsd, budget)?.first()
        {
            let code = String::from_utf8_lossy(&e.kind).trim().to_string();
            t.codec = match code.as_str() {
                "avc1" | "avc3" => "H.264/AVC".into(),
                "hvc1" | "hev1" => "H.265/HEVC".into(),
                "av01" => "AV1".into(),
                "vp08" => "VP8".into(),
                "vp09" => "VP9".into(),
                "mp4v" => "MPEG-4 Part 2".into(),
                "apch" | "apcn" | "apcs" | "apco" | "ap4h" | "ap4x" => "Apple ProRes".into(),
                "AVdn" | "AVdh" => "DNxHD/DNxHR".into(),
                "jpeg" | "mjpa" | "mjpb" => "Motion JPEG".into(),
                "mp4a" => "AAC".into(),
                "Opus" => "Opus".into(),
                "ac-3" => "AC-3".into(),
                "ec-3" => "E-AC-3".into(),
                "alac" => "ALAC".into(),
                "fLaC" => "FLAC".into(),
                "lpcm" | "sowt" | "twos" | "in24" | "fl32" => "PCM".into(),
                "tx3g" | "wvtt" | "stpp" | "c608" => format!("timed text ({code})"),
                other => format!("unrecognised ({other})"),
            };
            let p = e.payload();
            if t.kind == "video" {
                t.width = t.width.or(u16_at(b, p + 24).map(u32::from));
                t.height = t.height.or(u16_at(b, p + 26).map(u32::from));
            } else if t.kind == "audio" {
                t.channels = u16_at(b, p + 16);
                t.sample_rate = u32_at(b, p + 24).map(|r| r >> 16).filter(|r| *r > 0);
            }
        }
        if let Some(stsz) = find(&sk, b"stsz") {
            t.samples = u32_at(b, stsz.payload() + 8).map(u64::from);
        }
    }
    if t.kind == "video"
        && let (Some(n), Some(d)) = (t.samples, t.duration_s)
        && d > 0.
    {
        t.frame_rate = Some((n as f64 / d * 1000.).round() / 1000.);
    }
    Ok(t)
}

// ---------------------------------------------------------------- lossless metadata removal

#[derive(Debug, Clone, Serialize)]
pub struct StripReport {
    pub removed: Vec<String>,
    pub zeroed_timestamps: usize,
    pub chunks_verified: usize,
    pub bytes_before: usize,
    pub bytes_after: usize,
    pub remaining_findings: Vec<Finding>,
    pub warnings: Vec<String>,
}

struct Writer {
    out: Vec<u8>,
    /// (position in `out` of the offset table entries, entry width 4 or 8, count)
    tables: Vec<(usize, usize, usize)>,
    removed: Vec<String>,
    zeroed: usize,
}

fn header(out: &mut Vec<u8>, kind: &[u8; 4], payload_len: usize, extra: &[u8]) {
    let total = 8 + extra.len() + payload_len;
    if total <= u32::MAX as usize {
        out.extend_from_slice(&(total as u32).to_be_bytes());
        out.extend_from_slice(kind);
    } else {
        out.extend_from_slice(&1u32.to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(&((total + 8) as u64).to_be_bytes());
    }
    out.extend_from_slice(extra);
}

const DROP: [&[u8; 4]; 5] = [b"udta", b"meta", b"uuid", b"XMP_", b"loci"];

impl Writer {
    /// Copies a container recursively, dropping metadata and recording offset tables.
    fn copy(
        &mut self,
        b: &[u8],
        bx: &BoxRef,
        budget: &mut usize,
        dropped_tracks: &[u32],
        depth: usize,
    ) -> Result<()> {
        ensure!(depth < 16, "Box nesting too deep");
        let container = matches!(
            &bx.kind,
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"edts" | b"dinf"
        );
        if !container {
            let start = self.out.len();
            self.out.extend_from_slice(&b[bx.start..bx.end]);
            let p = start + bx.header;
            match &bx.kind {
                b"mvhd" | b"tkhd" | b"mdhd" => {
                    let v = self.out[p];
                    let w = if v == 1 { 8 } else { 4 };
                    let span = p + 4..p + 4 + 2 * w;
                    if span.end <= self.out.len() && self.out[span.clone()].iter().any(|x| *x != 0)
                    {
                        self.out[span].iter_mut().for_each(|x| *x = 0);
                        self.zeroed += 1;
                    }
                }
                b"stco" | b"co64" => {
                    let n = u32_at(&self.out, p + 4).context("offset count")? as usize;
                    let w = if bx.is(b"stco") { 4 } else { 8 };
                    ensure!(
                        p + 8 + n.checked_mul(w).context("overflow")? <= self.out.len(),
                        "Offset table truncated"
                    );
                    self.tables.push((p + 8, w, n));
                }
                b"tref" if !dropped_tracks.is_empty() => {
                    self.out.truncate(start);
                    self.removed
                        .push("track reference to removed metadata track".into());
                }
                _ => {}
            }
            return Ok(());
        }
        let start = self.out.len();
        header(&mut self.out, &bx.kind, 0, &[]);
        for c in kids(b, bx, budget)? {
            if DROP.contains(&&c.kind) {
                self.removed
                    .push(format!("{} box", String::from_utf8_lossy(&c.kind)));
                continue;
            }
            if c.is(b"trak") && is_metadata_track(b, &c, budget)? {
                self.removed.push("timed metadata track".into());
                continue;
            }
            self.copy(b, &c, budget, dropped_tracks, depth + 1)?;
        }
        let total = self.out.len() - start;
        ensure!(total <= u32::MAX as usize, "Container too large to rewrite");
        self.out[start..start + 4].copy_from_slice(&(total as u32).to_be_bytes());
        Ok(())
    }
}

fn is_metadata_track(b: &[u8], trak: &BoxRef, budget: &mut usize) -> Result<bool> {
    let tk = kids(b, trak, budget)?;
    let Some(mdia) = find(&tk, b"mdia") else {
        return Ok(false);
    };
    let md = kids(b, mdia, budget)?;
    Ok(find(&md, b"hdlr")
        .is_some_and(|h| b.get(h.payload() + 8..h.payload() + 12) == Some(b"meta")))
}

fn offsets(b: &[u8], moov: &BoxRef, budget: &mut usize) -> Result<Vec<(u32, Vec<u64>)>> {
    let mut out = vec![];
    for trak in kids(b, moov, budget)?.iter().filter(|x| x.is(b"trak")) {
        if is_metadata_track(b, trak, budget)? {
            continue;
        }
        let tk = kids(b, trak, budget)?;
        let id = find(&tk, b"tkhd")
            .and_then(|h| u32_at(b, h.payload() + 12))
            .unwrap_or(0);
        let stbl = find(&tk, b"mdia")
            .and_then(|m| kids(b, m, budget).ok())
            .and_then(|md| find(&md, b"minf").and_then(|m| kids(b, m, budget).ok()))
            .and_then(|mi| find(&mi, b"stbl").copied());
        let Some(stbl) = stbl else { continue };
        let mut v = vec![];
        for t in kids(b, &stbl, budget)? {
            let w = if t.is(b"stco") {
                4
            } else if t.is(b"co64") {
                8
            } else {
                continue;
            };
            let n = u32_at(b, t.payload() + 4).context("count")? as usize;
            for k in 0..n.min(10_000_000) {
                let o = t.payload() + 8 + k * w;
                v.push(if w == 4 {
                    u64::from(u32_at(b, o).context("stco")?)
                } else {
                    u64_at(b, o).context("co64")?
                });
            }
        }
        out.push((id, v));
    }
    Ok(out)
}

/// Rewrites an MP4/MOV/M4A/3GP file without privacy metadata. Media samples are not touched.
pub fn strip(b: &[u8]) -> Result<(Vec<u8>, StripReport)> {
    let det = crate::detect(b);
    ensure!(
        matches!(det.container, "MP4" | "MOV" | "M4A" | "3GP"),
        "Lossless metadata removal supports MP4, MOV, M4A and 3GP (found {})",
        det.container
    );
    let mut budget = MAX_NODES;
    let top = children(b, 0, b.len(), &mut budget)?;
    if find(&top, b"moof").is_some() {
        bail!("Fragmented MP4 is not supported for lossless metadata removal");
    }
    let moov = *find(&top, b"moov").context("No moov box")?;
    let mut w = Writer {
        out: Vec::with_capacity(b.len()),
        tables: vec![],
        removed: vec![],
        zeroed: 0,
    };
    // Layout: (old_start, old_end, new_start) for retained top-level boxes.
    let mut layout: Vec<(usize, usize, usize)> = vec![];
    let mut moov_range = (0, 0);
    for t in &top {
        if DROP.contains(&&t.kind) {
            w.removed.push(format!(
                "top-level {} box",
                String::from_utf8_lossy(&t.kind)
            ));
            continue;
        }
        let new_start = w.out.len();
        if t.is(b"moov") {
            w.copy(b, &moov, &mut budget, &[], 0)?;
            moov_range = (new_start, w.out.len());
        } else {
            w.out.extend_from_slice(&b[t.start..t.end]);
        }
        layout.push((t.start, t.end, new_start));
    }
    // Patch chunk offsets so they point to the same media bytes in the new layout.
    for &(pos, width, n) in &w.tables {
        ensure!(
            pos >= moov_range.0 && pos < moov_range.1,
            "Offset table outside moov"
        );
        for k in 0..n {
            let at = pos + k * width;
            let old = if width == 4 {
                u64::from(u32_at(&w.out, at).context("stco")?)
            } else {
                u64_at(&w.out, at).context("co64")?
            };
            let old_us = usize::try_from(old)?;
            let &(os, _, ns) = layout
                .iter()
                .find(|(s, e, _)| old_us >= *s && old_us < *e)
                .context("A chunk offset points outside retained data; refusing to rewrite")?;
            ensure!(
                os != moov.start,
                "Chunk offset points into the movie header"
            );
            let new = (old_us - os + ns) as u64;
            if width == 4 {
                let v = u32::try_from(new).context("Chunk offset exceeds 32 bits after rewrite")?;
                w.out[at..at + 4].copy_from_slice(&v.to_be_bytes());
            } else {
                w.out[at..at + 8].copy_from_slice(&new.to_be_bytes());
            }
        }
    }
    // Independent verification of the rewritten file.
    let before = probe(b)?;
    let after = probe(&w.out).context("Rewritten file could not be re-parsed")?;
    let kept: Vec<&Track> = before
        .tracks
        .iter()
        .filter(|t| t.kind != "timed metadata")
        .collect();
    ensure!(
        kept.len() == after.tracks.len(),
        "Track count changed unexpectedly"
    );
    for (a, c) in kept.iter().zip(&after.tracks) {
        ensure!(
            a.codec == c.codec
                && a.samples == c.samples
                && a.width == c.width
                && a.sample_rate == c.sample_rate,
            "Track {} parameters changed after rewrite",
            a.id
        );
    }
    let mut checked = 0;
    let old_off = offsets(b, &moov, &mut budget)?;
    let new_top = children(&w.out, 0, w.out.len(), &mut budget)?;
    let new_moov = *find(&new_top, b"moov").context("moov missing after rewrite")?;
    let new_off = offsets(&w.out, &new_moov, &mut budget)?;
    ensure!(old_off.len() == new_off.len(), "Offset tables differ");
    for ((_, o), (_, n)) in old_off.iter().zip(&new_off) {
        ensure!(o.len() == n.len(), "Chunk counts differ");
        for (x, y) in o.iter().zip(n) {
            let (x, y) = (*x as usize, *y as usize);
            let len = 64
                .min(b.len() - x.min(b.len()))
                .min(w.out.len() - y.min(w.out.len()));
            ensure!(
                len > 0 && b[x..x + len] == w.out[y..y + len],
                "Chunk data mismatch after rewrite"
            );
            checked += 1;
        }
    }
    let remaining: Vec<Finding> = after
        .findings
        .into_iter()
        .filter(|f| f.category != Category::Timestamp || !f.description.contains("mvhd"))
        .collect();
    let mut warnings = after.warnings;
    if after.tracks.iter().any(|t| t.kind == "subtitle") {
        warnings.push("Subtitle tracks were kept; review their text before publishing".into());
    }
    photo_log::info(
        photo_log::Component::Export,
        photo_log::Code::ExportVerified,
        &format!("video metadata removed, {checked} chunks verified"),
    );
    Ok((
        w.out.clone(),
        StripReport {
            removed: w.removed,
            zeroed_timestamps: w.zeroed,
            chunks_verified: checked,
            bytes_before: b.len(),
            bytes_after: w.out.len(),
            remaining_findings: remaining,
            warnings,
        },
    ))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(body);
        v
    }
    fn full(kind: &[u8; 4], ver: u8, body: &[u8]) -> Vec<u8> {
        let mut b = vec![ver, 0, 0, 0];
        b.extend_from_slice(body);
        bx(kind, &b)
    }
    fn track(
        id: u32,
        handler: &[u8; 4],
        entry: &[u8; 4],
        chunk_offsets: &[u32],
        with_udta: bool,
    ) -> Vec<u8> {
        let mut tkhd = vec![0u8; 80];
        tkhd[0..4].copy_from_slice(&1_700_000_000u32.to_be_bytes()); // creation time
        tkhd[8..12].copy_from_slice(&id.to_be_bytes());
        tkhd[72..76].copy_from_slice(&(640u32 << 16).to_be_bytes());
        tkhd[76..80].copy_from_slice(&(360u32 << 16).to_be_bytes());
        let mut mdhd = vec![0u8; 20];
        mdhd[8..12].copy_from_slice(&1000u32.to_be_bytes());
        mdhd[12..16].copy_from_slice(&2000u32.to_be_bytes());
        mdhd[16..18].copy_from_slice(&0x15c7u16.to_be_bytes()); // "eng"
        let mut hdlr = vec![0u8; 4];
        hdlr.extend_from_slice(handler);
        hdlr.extend_from_slice(&[0u8; 13]);
        let mut sample_entry = vec![0u8; 28];
        sample_entry[24..28].copy_from_slice(&(48_000u32 << 16).to_be_bytes());
        sample_entry[16..18].copy_from_slice(&2u16.to_be_bytes());
        let mut stsd = 1u32.to_be_bytes().to_vec();
        stsd.extend(bx(entry, &sample_entry));
        let mut stsz = vec![0u8; 4];
        stsz.extend_from_slice(&(chunk_offsets.len() as u32 * 25).to_be_bytes());
        let mut stco = (chunk_offsets.len() as u32).to_be_bytes().to_vec();
        for o in chunk_offsets {
            stco.extend_from_slice(&o.to_be_bytes());
        }
        let stbl = [
            full(b"stsd", 0, &stsd),
            full(b"stsz", 0, &stsz),
            full(b"stco", 0, &stco),
        ]
        .concat();
        let minf = bx(b"minf", &bx(b"stbl", &stbl));
        let mdia = bx(
            b"mdia",
            &[full(b"mdhd", 0, &mdhd), full(b"hdlr", 0, &hdlr), minf].concat(),
        );
        let mut t = [full(b"tkhd", 0, &tkhd), mdia].concat();
        if with_udta {
            t.extend(bx(
                b"udta",
                &bx(b"\xa9xyz", b"\0\x12\x15\xc7+40.7128-074.0060/"),
            ));
        }
        bx(b"trak", &t)
    }
    /// Synthetic MP4: video + audio + Apple-style location metadata track, GPS and device tags.
    pub fn synthetic(moov_first: bool) -> Vec<u8> {
        let ftyp = bx(b"ftyp", b"isom\0\0\x02\0isomiso2avc1mp41");
        let payload: Vec<u8> = (0..400u32).map(|i| (i * 7 % 251) as u8).collect();
        let build = |mdat_start: u32| {
            let o = |k: u32| mdat_start + 8 + k * 50;
            let mut mvhd = vec![0u8; 96];
            mvhd[0..4].copy_from_slice(&1_700_000_000u32.to_be_bytes());
            mvhd[8..12].copy_from_slice(&1000u32.to_be_bytes());
            mvhd[12..16].copy_from_slice(&2000u32.to_be_bytes());
            let udta = bx(
                b"udta",
                &[
                    bx(b"\xa9xyz", b"\0\x12\x15\xc7+40.7128-074.0060/"),
                    bx(b"\xa9mak", b"\0\x05\x15\xc7Phone"),
                    bx(b"\xa9swr", b"\0\x03\x15\xc71.0"),
                ]
                .concat(),
            );
            let meta = bx(
                b"meta",
                &[
                    full(b"hdlr", 0, b"\0\0\0\0mdta\0\0\0\0\0\0\0\0\0\0\0\0\0"),
                    bx(b"keys", b"com.apple.quicktime.location.ISO6709"),
                ]
                .concat(),
            );
            bx(
                b"moov",
                &[
                    full(b"mvhd", 0, &mvhd),
                    track(1, b"vide", b"avc1", &[o(0), o(2), o(4)], true),
                    track(2, b"soun", b"mp4a", &[o(1), o(3)], false),
                    track(3, b"meta", b"mebx", &[o(5)], false),
                    udta,
                    meta,
                ]
                .concat(),
            )
        };
        let body = b"<x:xmpmeta/>\0\0";
        let xmp = [
            ((8 + 16 + body.len()) as u32).to_be_bytes().to_vec(),
            b"uuid".to_vec(),
            XMP_UUID.to_vec(),
            body.to_vec(),
        ]
        .concat();
        if moov_first {
            let moov_len = build(0).len() as u32;
            let mdat_start = (ftyp.len() + xmp.len()) as u32 + moov_len;
            [ftyp.clone(), xmp, build(mdat_start), bx(b"mdat", &payload)].concat()
        } else {
            let mdat_start = (ftyp.len() + xmp.len()) as u32;
            [ftyp.clone(), xmp, bx(b"mdat", &payload), build(mdat_start)].concat()
        }
    }
    /// Writes the synthetic sample to `$APD_DEMO_MP4` for manual GUI testing.
    #[test]
    #[ignore]
    fn write_demo_sample() {
        let path = std::env::var("APD_DEMO_MP4").expect("set APD_DEMO_MP4");
        std::fs::write(path, synthetic(true)).unwrap();
    }
    #[test]
    fn probe_reports_tracks_and_privacy_metadata() {
        let f = synthetic(true);
        let i = probe(&f).unwrap();
        assert_eq!(i.container, "MP4");
        assert_eq!(i.duration_s, Some(2.0));
        let v = &i.tracks[0];
        assert_eq!(
            (
                v.kind.as_str(),
                v.codec.as_str(),
                v.width,
                v.height,
                v.samples
            ),
            ("video", "H.264/AVC", Some(640), Some(360), Some(75))
        );
        assert_eq!(v.language.as_deref(), Some("eng"));
        assert_eq!(i.tracks[1].sample_rate, Some(48_000));
        let cats: Vec<_> = i.findings.iter().map(|f| f.category).collect();
        for c in [
            Category::GpsLocation,
            Category::DeviceInformation,
            Category::XmpMetadata,
            Category::Timestamp,
            Category::HiddenLayers,
        ] {
            assert!(cats.contains(&c), "{c:?} missing from {cats:?}");
        }
    }
    #[test]
    fn strip_is_lossless_for_both_layouts() {
        for moov_first in [true, false] {
            let f = synthetic(moov_first);
            let (out, r) = strip(&f).unwrap();
            assert!(out.len() < f.len());
            assert_eq!(r.chunks_verified, 5, "video 3 + audio 2 chunks");
            assert!(r.zeroed_timestamps >= 3);
            let after = probe(&out).unwrap();
            assert_eq!(after.tracks.len(), 2, "location track removed");
            assert!(
                !after.findings.iter().any(|f| matches!(
                    f.category,
                    Category::GpsLocation
                        | Category::DeviceInformation
                        | Category::XmpMetadata
                        | Category::HiddenLayers
                )),
                "{:?}",
                after.findings
            );
            assert!(!out.windows(8).any(|w| w == b"+40.7128"));
            assert!(
                r.remaining_findings.is_empty(),
                "{:?}",
                r.remaining_findings
            );
        }
    }
    #[test]
    fn hostile_mp4_inputs() {
        let f = synthetic(true);
        let mut state = 0x1234_5678_9abc_def0u64;
        for _ in 0..1500 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let mut v = f.clone();
            let i = (state as usize) % v.len();
            v[i] = (state >> 32) as u8;
            if state.is_multiple_of(5) {
                v.truncate((state as usize >> 8) % v.len());
            }
            let _ = probe(&v);
            if let Ok((out, _)) = strip(&v) {
                // Whatever is produced must itself parse.
                probe(&out).unwrap();
            }
        }
        assert!(strip(b"\0\0\0\x08ftyp").is_err());
        let mut lie = bx(b"ftyp", b"isom\0\0\0\0");
        lie.extend_from_slice(&[0xff, 0xff, 0xff, 0xff, b'm', b'o', b'o', b'v']);
        assert!(probe(&lie).is_err());
    }
}
