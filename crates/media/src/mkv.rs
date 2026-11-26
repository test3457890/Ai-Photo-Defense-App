//! Matroska / WebM inspection (EBML). Reads segment info, tracks, tags and attachments with
//! bounded recursion. Clusters (media data) are skipped. Rewriting is not implemented.
use crate::{MAX_NODES, MediaInfo, MediaKind, Track};
use anyhow::{Context, Result, ensure};
use photo_privacy::{Category, Evidence, Finding, Source};

fn vint(b: &[u8], i: usize, keep_marker: bool) -> Option<(u64, usize)> {
    let first = *b.get(i)?;
    let len = first.leading_zeros() as usize + 1;
    if len > 8 {
        return None;
    }
    let mut v = if keep_marker {
        u64::from(first)
    } else {
        u64::from(first) & ((1u64 << (8 - len)) - 1)
    };
    for k in 1..len {
        v = (v << 8) | u64::from(*b.get(i + k)?);
    }
    Some((v, len))
}

struct El {
    id: u64,
    data: usize,
    end: usize,
}

fn elements(b: &[u8], start: usize, end: usize, budget: &mut usize) -> Result<Vec<El>> {
    let mut out = vec![];
    let mut i = start;
    while i < end {
        ensure!(*budget > 0, "Too many elements");
        *budget -= 1;
        let (id, il) = vint(b, i, true).context("element id")?;
        let (size, sl) = vint(b, i + il, false).context("element size")?;
        let data = i + il + sl;
        let unknown = size == (1u64 << (7 * sl)) - 1;
        let el_end = if unknown {
            end
        } else {
            data.checked_add(usize::try_from(size)?)
                .context("overflow")?
        };
        ensure!(el_end <= end, "Element extends past its parent");
        out.push(El {
            id,
            data,
            end: el_end,
        });
        if unknown && id == 0x1F43_B675 {
            break; // unknown-size cluster: media data until end; stop walking.
        }
        i = el_end;
    }
    Ok(out)
}
fn uint(b: &[u8], e: &El) -> u64 {
    b[e.data..e.end]
        .iter()
        .take(8)
        .fold(0, |a, x| (a << 8) | u64::from(*x))
}
fn float(b: &[u8], e: &El) -> Option<f64> {
    match e.end - e.data {
        4 => Some(f64::from(f32::from_be_bytes(
            b[e.data..e.end].try_into().ok()?,
        ))),
        8 => Some(f64::from_be_bytes(b[e.data..e.end].try_into().ok()?)),
        _ => None,
    }
}
fn string(b: &[u8], e: &El) -> String {
    String::from_utf8_lossy(&b[e.data..e.end])
        .trim_end_matches('\0')
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect()
}

pub fn probe(b: &[u8]) -> Result<MediaInfo> {
    let det = crate::detect(b);
    let mut budget = MAX_NODES;
    let mut info = MediaInfo {
        kind: MediaKind::Video,
        container: det.container.into(),
        duration_s: None,
        tracks: vec![],
        findings: vec![],
        warnings: vec![],
    };
    let top = elements(b, 0, b.len(), &mut budget)?;
    let seg = top
        .iter()
        .find(|e| e.id == 0x1853_8067)
        .context("No Matroska segment")?;
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
    let mut scale = 1_000_000f64;
    for e in elements(b, seg.data, seg.end, &mut budget)? {
        match e.id {
            0x1549_A966 => {
                let kids = elements(b, e.data, e.end, &mut budget)?;
                if let Some(s) = kids.iter().find(|k| k.id == 0x2A_D7B1) {
                    scale = uint(b, s) as f64;
                }
                for k in &kids {
                    match k.id {
                        0x4489 => info.duration_s = float(b, k).map(|d| d * scale / 1e9),
                        0x4461 => info
                            .findings
                            .push(f(Category::Timestamp, "Recording date (DateUTC)")),
                        0x7BA9 => info.findings.push(f(Category::TextMetadata, "Title")),
                        0x5741 | 0x4D80 => info.findings.push(f(
                            Category::DeviceInformation,
                            "Writing or muxing application",
                        )),
                        _ => {}
                    }
                }
            }
            0x1654_AE6B => {
                for te in elements(b, e.data, e.end, &mut budget)?
                    .iter()
                    .filter(|x| x.id == 0xAE)
                {
                    let mut t = Track {
                        id: 0,
                        kind: "other".into(),
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
                    for k in elements(b, te.data, te.end, &mut budget)? {
                        match k.id {
                            0xD7 => t.id = uint(b, &k) as u32,
                            0x83 => {
                                t.kind = match uint(b, &k) {
                                    1 => "video",
                                    2 => "audio",
                                    17 => "subtitle",
                                    _ => "other",
                                }
                                .into()
                            }
                            0x86 => t.codec = string(b, &k),
                            0x22_B59C => t.language = Some(string(b, &k)),
                            0x23_E383 => {
                                t.frame_rate =
                                    Some((1e9 / uint(b, &k).max(1) as f64 * 1000.).round() / 1000.)
                            }
                            0xE0 => {
                                for v in elements(b, k.data, k.end, &mut budget)? {
                                    match v.id {
                                        0xB0 => t.width = Some(uint(b, &v) as u32),
                                        0xBA => t.height = Some(uint(b, &v) as u32),
                                        _ => {}
                                    }
                                }
                            }
                            0xE1 => {
                                for a in elements(b, k.data, k.end, &mut budget)? {
                                    match a.id {
                                        0xB5 => t.sample_rate = float(b, &a).map(|r| r as u32),
                                        0x9F => t.channels = Some(uint(b, &a) as u16),
                                        _ => {}
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    info.tracks.push(t);
                }
            }
            0x1254_C367 => info
                .findings
                .push(f(Category::TextMetadata, "Matroska tags")),
            0x1941_A469 => info.findings.push(f(
                Category::EmbeddedImage,
                "Attached files (fonts, cover art or other attachments)",
            )),
            0x1043_A770 => info
                .findings
                .push(f(Category::TextMetadata, "Chapter titles")),
            _ => {}
        }
    }
    if info.tracks.iter().any(|t| t.kind == "subtitle") {
        info.warnings
            .push("Subtitle track present: its text may contain private information".into());
    }
    info.warnings
        .push("Matroska/WebM metadata removal is not implemented; use inspection only".into());
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn el(id: &[u8], body: &[u8]) -> Vec<u8> {
        let mut v = id.to_vec();
        v.push(0x01);
        v.extend_from_slice(&(body.len() as u64 | (1 << 56)).to_be_bytes()[1..]);
        v.extend_from_slice(body);
        v
    }
    #[test]
    fn probes_tracks_tags_and_attachments() {
        let header = el(&[0x1a, 0x45, 0xdf, 0xa3], &el(&[0x42, 0x82], b"webm"));
        let info = el(
            &[0x15, 0x49, 0xa9, 0x66],
            &[
                el(&[0x2a, 0xd7, 0xb1], &[0x0f, 0x42, 0x40]),
                el(&[0x44, 0x89], &2500f64.to_be_bytes()),
                el(&[0x57, 0x41], b"phone-app"),
            ]
            .concat(),
        );
        let video = el(
            &[0xae],
            &[
                el(&[0xd7], &[1]),
                el(&[0x83], &[1]),
                el(&[0x86], b"V_VP9"),
                el(
                    &[0xe0],
                    &[el(&[0xb0], &[0x07, 0x80]), el(&[0xba], &[0x04, 0x38])].concat(),
                ),
            ]
            .concat(),
        );
        let audio = el(
            &[0xae],
            &[
                el(&[0xd7], &[2]),
                el(&[0x83], &[2]),
                el(&[0x86], b"A_OPUS"),
                el(
                    &[0xe1],
                    &[el(&[0xb5], &48000f64.to_be_bytes()), el(&[0x9f], &[2])].concat(),
                ),
            ]
            .concat(),
        );
        let tracks = el(&[0x16, 0x54, 0xae, 0x6b], &[video, audio].concat());
        let tags = el(&[0x12, 0x54, 0xc3, 0x67], b"x");
        let att = el(&[0x19, 0x41, 0xa4, 0x69], b"x");
        let seg = el(
            &[0x18, 0x53, 0x80, 0x67],
            &[info, tracks, tags, att].concat(),
        );
        let f = [header, seg].concat();
        let i = probe(&f).unwrap();
        assert_eq!(i.container, "WebM");
        assert_eq!(i.duration_s, Some(2.5));
        assert_eq!(
            (
                i.tracks[0].codec.as_str(),
                i.tracks[0].width,
                i.tracks[0].height
            ),
            ("V_VP9", Some(1920), Some(1080))
        );
        assert_eq!(
            (i.tracks[1].sample_rate, i.tracks[1].channels),
            (Some(48000), Some(2))
        );
        let c: Vec<_> = i.findings.iter().map(|f| f.category).collect();
        assert!(
            c.contains(&Category::EmbeddedImage)
                && c.contains(&Category::TextMetadata)
                && c.contains(&Category::DeviceInformation)
        );
        for n in 0..f.len() {
            let _ = probe(&f[..n]);
        }
    }
}
