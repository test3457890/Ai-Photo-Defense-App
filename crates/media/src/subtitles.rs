//! SRT and WebVTT subtitles: bounded parsing, Privacy Scan of cue text, cue redaction.
//! Cue text is data only: markup is never interpreted, links are never opened.
use anyhow::{Result, ensure};
use photo_privacy::{Country, Finding, Source, TextSpan};
use serde::Serialize;

pub const MAX_SUBTITLE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CUES: usize = 200_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Cue {
    pub start_ms: u64,
    pub end_ms: u64,
    #[serde(skip)]
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CueFinding {
    pub cue: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub finding: Finding,
}

pub fn looks_like_srt(text: &str) -> bool {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    matches!((lines.next(), lines.next()), (Some(a), Some(b)) if a.trim().chars().all(|c| c.is_ascii_digit()) && b.contains("-->"))
}

fn time(s: &str) -> Option<u64> {
    let s = s.split_whitespace().next()?.replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    let (h, m, rest) = match parts.as_slice() {
        [h, m, r] => (h.parse::<u64>().ok()?, m.parse::<u64>().ok()?, *r),
        [m, r] => (0, m.parse::<u64>().ok()?, *r),
        _ => return None,
    };
    let (sec, ms) = rest.split_once('.').unwrap_or((rest, "0"));
    let ms: String = ms.chars().chain("000".chars()).take(3).collect();
    let total = ((h.checked_mul(60)?.checked_add(m)?)
        .checked_mul(60)?
        .checked_add(sec.parse::<u64>().ok()?)?)
    .checked_mul(1000)?
    .checked_add(ms.parse::<u64>().ok()?)?;
    (m < 60 || h == 0).then_some(total)
}

pub fn parse(text: &str) -> Result<Vec<Cue>> {
    ensure!(text.len() <= MAX_SUBTITLE_BYTES, "Subtitle file too large");
    let mut cues = vec![];
    let normalized = text.replace("\r\n", "\n");
    for block in normalized.split("\n\n") {
        let lines: Vec<&str> = block.lines().collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else {
            continue;
        };
        let (a, b) = lines[ti].split_once("-->").expect("checked");
        let (Some(start_ms), Some(end_ms)) = (time(a), time(b)) else {
            continue;
        };
        ensure!(cues.len() < MAX_CUES, "Too many subtitle cues");
        cues.push(Cue {
            start_ms,
            end_ms: end_ms.max(start_ms),
            text: lines[ti + 1..].join("\n").chars().take(4096).collect(),
        });
    }
    Ok(cues)
}

pub fn scan(cues: &[Cue], countries: &[Country]) -> Vec<CueFinding> {
    let mut out = vec![];
    for (i, c) in cues.iter().enumerate() {
        for f in photo_privacy::text::scan(
            &[TextSpan {
                text: c.text.clone(),
                region: None,
                source: Source::Text,
            }],
            countries,
        ) {
            out.push(CueFinding {
                cue: i,
                start_ms: c.start_ms,
                end_ms: c.end_ms,
                finding: f,
            });
        }
    }
    out
}

fn stamp(ms: u64, vtt: bool) -> String {
    let (h, m, s, f) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    if vtt {
        format!("{h:02}:{m:02}:{s:02}.{f:03}")
    } else {
        format!("{h:02}:{m:02}:{s:02},{f:03}")
    }
}

/// Writes cues back out, replacing the text of the listed cues with "[redacted]".
/// Structure and Privacy Scan summary for the media inspector. Findings never contain the
/// matched text, only its category and the cue position.
pub fn inspect(bytes: &[u8], container: &str) -> Result<crate::MediaInfo> {
    let text = String::from_utf8_lossy(bytes);
    let cues = parse(&text)?;
    let findings = scan(&cues, &Country::ALL)
        .into_iter()
        .map(|f| {
            let mut x = f.finding;
            x.description = format!(
                "{} (cue {}, {:.1} s)",
                x.description,
                f.cue + 1,
                f.start_ms as f64 / 1000.
            );
            x
        })
        .collect();
    Ok(crate::MediaInfo {
        kind: crate::MediaKind::Subtitle,
        container: container.into(),
        duration_s: cues
            .iter()
            .map(|c| c.end_ms)
            .max()
            .map(|m| m as f64 / 1000.),
        tracks: vec![crate::Track {
            id: 1,
            kind: "subtitle".into(),
            codec: format!("{container} ({} cues)", cues.len()),
            width: None,
            height: None,
            frame_rate: None,
            sample_rate: None,
            channels: None,
            duration_s: None,
            language: None,
            samples: Some(cues.len() as u64),
        }],
        findings,
        warnings: vec![],
    })
}

pub fn write(cues: &[Cue], redact: &[usize], vtt: bool) -> String {
    let mut s = if vtt {
        "WEBVTT\n\n".to_string()
    } else {
        String::new()
    };
    for (i, c) in cues.iter().enumerate() {
        if !vtt {
            s.push_str(&format!("{}\n", i + 1));
        }
        let text = if redact.contains(&i) {
            "[redacted]"
        } else {
            c.text.as_str()
        };
        s.push_str(&format!(
            "{} --> {}\n{}\n\n",
            stamp(c.start_ms, vtt),
            stamp(c.end_ms, vtt),
            text
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_scan_and_redact() {
        let srt = "1\r\n00:00:01,000 --> 00:00:02,500\r\nHello there\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nThe wifi password: example-pass-123\r\n\r\n3\r\n00:00:05,000 --> 00:00:06,000\r\nCall (555) 123-4567\r\n";
        assert!(looks_like_srt(srt));
        let cues = parse(srt).unwrap();
        assert_eq!(cues.len(), 3);
        assert_eq!((cues[0].start_ms, cues[0].end_ms), (1000, 2500));
        let found = scan(&cues, &Country::ALL);
        let flagged: Vec<usize> = found.iter().map(|f| f.cue).collect();
        assert!(flagged.contains(&1) && flagged.contains(&2) && !flagged.contains(&0));
        assert!(
            !serde_json::to_string(&found)
                .unwrap()
                .contains("example-pass")
        );
        let out = write(&cues, &flagged, false);
        assert!(
            out.contains("[redacted]")
                && !out.contains("example-pass")
                && out.contains("Hello there")
        );
        let vtt = write(&cues, &[], true);
        assert_eq!(parse(&vtt).unwrap(), cues);
        assert!(parse(&"x".repeat(MAX_SUBTITLE_BYTES + 1)).is_err());
        assert!(
            parse("1\n99:99:99,999 --> 00:00:01,000\nx\n")
                .unwrap()
                .len()
                <= 1
        );
    }
}
