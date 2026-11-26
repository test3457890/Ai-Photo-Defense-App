//! RAW camera files: format identification and embedded JPEG preview extraction.
//!
//! This is NOT RAW development. No sensor data is demosaiced. Cameras embed a full- or
//! reduced-size JPEG rendering; this module locates it with bounds-checked parsing and decodes
//! it with the same limits as any other JPEG. Full RAW decoding is blocked: the mature Rust
//! decoders (rawloader, rawler) are LGPL-2.1 and LibRaw is LGPL/CDDL, which needs a licensing
//! decision before they can be linked into this MIT application.
use anyhow::{Result, bail, ensure};
use serde::Serialize;

pub const EXTENSIONS: [&str; 10] = ["dng", "cr2", "cr3", "nef", "arw", "raf", "orf", "rw2", "pef", "srw"];
const MAX_IFDS: usize = 64;
/// RAW files are larger than processed images; previews inside are still bounded by decode limits.
pub const MAX_RAW_BYTES: u64 = 256 * 1024 * 1024;
const MAX_CANDIDATES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RawKind {
    Dng,
    Cr2,
    Cr3,
    Nef,
    Arw,
    Raf,
    Orf,
    Rw2,
    Pef,
    TiffRaw,
}

#[derive(Debug, Clone, Serialize)]
pub struct RawPreview {
    pub kind: RawKind,
    pub jpeg: Vec<u8>,
    pub candidates: usize,
}

pub fn is_raw_extension(ext: &str) -> bool {
    EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
}

/// Identifies a RAW container from its bytes (extension is only a hint).
pub fn identify(b: &[u8], ext_hint: Option<&str>) -> Option<RawKind> {
    if b.len() < 16 {
        return None;
    }
    if b.starts_with(b"FUJIFILMCCD-RAW") {
        return Some(RawKind::Raf);
    }
    if &b[4..8] == b"ftyp" && &b[8..12] == b"crx " {
        return Some(RawKind::Cr3);
    }
    if b.starts_with(b"IIU\0") {
        return Some(RawKind::Rw2);
    }
    if b.starts_with(b"IIRO") || b.starts_with(b"IIRS") || b.starts_with(b"MMOR") {
        return Some(RawKind::Orf);
    }
    let tiff = b.starts_with(b"II*\0") || b.starts_with(b"MM\0*");
    if !tiff {
        return None;
    }
    if &b[8..10] == b"CR" {
        return Some(RawKind::Cr2);
    }
    if Tiff::new(b).is_some_and(|t| t.has_tag(50706)) {
        return Some(RawKind::Dng);
    }
    match ext_hint.map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("nef") => Some(RawKind::Nef),
        Some("arw") | Some("srw") => Some(RawKind::Arw),
        Some("pef") => Some(RawKind::Pef),
        Some("dng") => Some(RawKind::Dng),
        Some(e) if is_raw_extension(e) => Some(RawKind::TiffRaw),
        _ => None,
    }
}

struct Tiff<'a> {
    b: &'a [u8],
    le: bool,
}
impl<'a> Tiff<'a> {
    fn new(b: &'a [u8]) -> Option<Self> {
        let le = match b.get(..2)? {
            b"II" => true,
            b"MM" => false,
            _ => return None,
        };
        Some(Self { b, le })
    }
    fn u16(&self, i: usize) -> Option<usize> {
        let s: [u8; 2] = self.b.get(i..i.checked_add(2)?)?.try_into().ok()?;
        Some(if self.le { u16::from_le_bytes(s) } else { u16::from_be_bytes(s) } as usize)
    }
    fn u32(&self, i: usize) -> Option<usize> {
        let s: [u8; 4] = self.b.get(i..i.checked_add(4)?)?.try_into().ok()?;
        Some(if self.le { u32::from_le_bytes(s) } else { u32::from_be_bytes(s) } as usize)
    }
    /// Walks IFD0 chain and SubIFDs breadth-first; yields (tag, type, count, value-or-offset).
    fn ifds(&self) -> Vec<Vec<(usize, usize, usize, usize)>> {
        let mut out = vec![];
        let mut queue = vec![self.u32(4).unwrap_or(0)];
        let mut seen = std::collections::HashSet::new();
        while let Some(off) = queue.pop() {
            if off == 0 || out.len() >= MAX_IFDS || !seen.insert(off) {
                continue;
            }
            let Some(n) = self.u16(off) else { continue };
            let mut entries = vec![];
            for e in 0..n.min(512) {
                let at = off + 2 + e * 12;
                let (Some(tag), Some(ty), Some(count)) = (self.u16(at), self.u16(at + 2), self.u32(at + 4)) else { break };
                let value = if ty == 3 && count == 1 { self.u16(at + 8).unwrap_or(0) } else { self.u32(at + 8).unwrap_or(0) };
                if tag == 330 {
                    // SubIFDs: one inline offset or an array of offsets.
                    if count == 1 {
                        queue.push(value);
                    } else {
                        for k in 0..count.min(16) {
                            if let Some(o) = self.u32(value + k * 4) {
                                queue.push(o);
                            }
                        }
                    }
                }
                entries.push((tag, ty, count, value));
            }
            if let Some(next) = self.u32(off + 2 + n * 12) {
                queue.push(next);
            }
            out.push(entries);
        }
        out
    }
    fn has_tag(&self, tag: usize) -> bool {
        self.ifds().iter().any(|i| i.iter().any(|e| e.0 == tag))
    }
}

fn is_jpeg_at(b: &[u8], off: usize, len: usize) -> bool {
    len >= 4 && off.checked_add(len).is_some_and(|end| end <= b.len()) && b[off..].starts_with(&[0xff, 0xd8, 0xff])
}

/// Returns embedded JPEG previews, largest first.
pub fn preview_candidates(b: &[u8]) -> Vec<(usize, usize)> {
    let mut c: Vec<(usize, usize)> = vec![];
    if b.starts_with(b"FUJIFILMCCD-RAW") && b.len() > 92 {
        let off = u32::from_be_bytes(b[84..88].try_into().expect("len")) as usize;
        let len = u32::from_be_bytes(b[88..92].try_into().expect("len")) as usize;
        if is_jpeg_at(b, off, len) {
            c.push((off, len));
        }
    }
    let tiff_start = if b.starts_with(b"IIU\0") || b.starts_with(b"IIRO") || b.starts_with(b"IIRS") { Some(b"II*\0".as_slice()) } else { None };
    let view: Option<Tiff> = if tiff_start.is_some() { Tiff::new(b).map(|t| Tiff { b: t.b, le: true }) } else { Tiff::new(b) };
    if let Some(t) = view {
        for ifd in t.ifds() {
            let get = |tag| ifd.iter().find(|e| e.0 == tag).map(|e| e.3);
            if let (Some(off), Some(len)) = (get(513), get(514))
                && is_jpeg_at(b, off, len)
            {
                c.push((off, len));
            }
            let compression = get(259);
            let single_strip = ifd.iter().find(|e| e.0 == 273).is_some_and(|e| e.2 == 1);
            if matches!(compression, Some(6) | Some(7))
                && single_strip
                && let (Some(off), Some(len)) = (get(273), get(279))
                && is_jpeg_at(b, off, len)
                && get(262) != Some(32803)
            {
                c.push((off, len));
            }
        }
    }
    // Generic fallback (CR3 and unknown layouts): SOI..EOI scan, bounded.
    if c.is_empty() {
        let mut i = 0;
        while i + 3 < b.len() && c.len() < MAX_CANDIDATES {
            if b[i] == 0xff && b[i + 1] == 0xd8 && b[i + 2] == 0xff {
                if let Some(end) = b[i + 2..].windows(2).position(|w| w == [0xff, 0xd9]) {
                    let len = end + 4;
                    if len > 128 {
                        c.push((i, len));
                    }
                    i += len;
                    continue;
                }
                break;
            }
            i += 1;
        }
    }
    c.sort_by_key(|x| std::cmp::Reverse(x.1));
    c.dedup();
    c.truncate(MAX_CANDIDATES);
    c
}

/// Extracts the largest embedded preview that decodes under the standard limits.
pub fn extract_preview(b: &[u8], ext_hint: Option<&str>) -> Result<RawPreview> {
    let Some(kind) = identify(b, ext_hint) else { bail!("Not a recognised RAW container") };
    let cands = preview_candidates(b);
    ensure!(!cands.is_empty(), "No embedded preview found; full RAW decoding is not available");
    for &(off, len) in &cands {
        let jpeg = &b[off..off + len];
        if crate::decode_bounded(jpeg).is_ok() {
            return Ok(RawPreview { kind, jpeg: jpeg.to_vec(), candidates: cands.len() });
        }
    }
    bail!("Embedded previews could not be decoded")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub fn jpeg(w: u32, h: u32) -> Vec<u8> {
        crate::encode(&image::DynamicImage::new_rgb8(w, h), crate::ExportFormat::Jpeg, 90).unwrap()
    }
    /// Minimal little-endian TIFF with JPEGInterchangeFormat pointing at an embedded JPEG.
    pub fn synthetic_tiff_raw(dng: bool, preview: &[u8]) -> Vec<u8> {
        let mut v = b"II*\0\x08\0\0\0".to_vec();
        let tags: Vec<(u16, u16, u32, u32)> = {
            let mut t = vec![(256, 4, 1, 16), (257, 4, 1, 16)];
            let data_off = 8 + 2 + 12 * (if dng { 5 } else { 4 }) + 4;
            t.push((513, 4, 1, data_off as u32));
            t.push((514, 4, 1, preview.len() as u32));
            if dng {
                t.push((50706, 1, 4, 0x0001_0401));
            }
            t
        };
        v.extend_from_slice(&(tags.len() as u16).to_le_bytes());
        for (tag, ty, count, value) in tags {
            v.extend_from_slice(&tag.to_le_bytes());
            v.extend_from_slice(&ty.to_le_bytes());
            v.extend_from_slice(&count.to_le_bytes());
            v.extend_from_slice(&value.to_le_bytes());
        }
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(preview);
        v
    }
    #[test]
    fn dng_and_tiff_raw_previews() {
        let p = jpeg(64, 48);
        let dng = synthetic_tiff_raw(true, &p);
        assert_eq!(identify(&dng, None), Some(RawKind::Dng));
        let r = extract_preview(&dng, None).unwrap();
        assert_eq!(r.jpeg, p);
        let nef = synthetic_tiff_raw(false, &p);
        assert_eq!(identify(&nef, Some("NEF")), Some(RawKind::Nef));
        assert_eq!(identify(&nef, None), None, "plain TIFF without RAW hint is not RAW");
        assert!(extract_preview(&nef, Some("nef")).is_ok());
    }
    #[test]
    fn raf_and_generic_scan() {
        let p = jpeg(80, 60);
        let mut raf = b"FUJIFILMCCD-RAW 0201FF383501".to_vec();
        raf.resize(100, 0);
        raf[84..88].copy_from_slice(&100u32.to_be_bytes());
        raf[88..92].copy_from_slice(&(p.len() as u32).to_be_bytes());
        raf.extend_from_slice(&p);
        assert_eq!(extract_preview(&raf, None).unwrap().kind, RawKind::Raf);
        let mut cr3 = vec![0, 0, 0, 24];
        cr3.extend_from_slice(b"ftypcrx \0\0\0\x01crx isom");
        cr3.extend_from_slice(&[0; 64]);
        cr3.extend_from_slice(&p);
        cr3.extend_from_slice(&[0; 32]);
        let r = extract_preview(&cr3, None).unwrap();
        assert_eq!(r.kind, RawKind::Cr3);
    }
    #[test]
    fn hostile_raw_inputs() {
        let mut loops = b"II*\0\x08\0\0\0\x01\0\x4a\x01\x04\0\x01\0\0\0\x08\0\0\0\x08\0\0\0".to_vec();
        loops.extend_from_slice(&[0; 16]);
        assert!(extract_preview(&loops, Some("dng")).is_err());
        let mut lying = synthetic_tiff_raw(true, &jpeg(8, 8));
        let n = lying.len();
        lying.truncate(n - 20);
        let _ = extract_preview(&lying, None);
        assert!(extract_preview(b"FUJIFILMCCD-RAW short", None).is_err());
        assert!(identify(&[0; 4], Some("dng")).is_none());
    }
}
