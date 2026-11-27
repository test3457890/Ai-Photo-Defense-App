//! Intelligent video object tracking and independent frame-level export verification.
//!
//! - `track_object_in_mp4`: forward/backward NCC template tracking from one or more manual
//!   keyframes, with scene-cut detection, constant-velocity rapid-motion search, occlusion
//!   detection & automatic re-acquisition, and motion-adaptive mask padding.
//! - `verify_redacted_mp4`: independently reopens an exported MP4, decodes every frame, and
//!   verifies that redacted regions actually removed original pixel structure (opaque black
//!   luma check or blur edge-energy check).

use crate::mp4;
use crate::video::{LumaFrame, RedactRect, decode_all_luma};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub time_s: f64,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackStatus {
    Seed,
    Tracked,
    Occluded,
    Reacquired,
    SceneCut,
    OutOfFrame,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrackedBox {
    pub frame_idx: usize,
    pub pts_s: f64,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub confidence: f32,
    pub status: TrackStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct FrameVerifyReport {
    pub frames_checked: usize,
    pub redacted_frames_checked: usize,
    pub failed_frames: usize,
    pub weak_blur_frames: usize,
    pub metadata_clean: bool,
    pub passed: bool,
}

fn mean_abs_diff(a: &[u8], b: &[u8]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let sum: u64 = a
        .iter()
        .zip(b.iter())
        .map(|(&x, &y)| u64::from(x.abs_diff(y)))
        .sum();
    sum as f32 / a.len() as f32
}

/// Returns indices `i` where frame `i` begins a new scene relative to frame `i - 1`.
pub(crate) fn detect_scene_cuts(frames: &[LumaFrame]) -> Vec<usize> {
    let mut cuts = Vec::new();
    for i in 1..frames.len() {
        if mean_abs_diff(&frames[i - 1].y, &frames[i].y) > 28.0 {
            cuts.push(i);
        }
    }
    cuts
}

fn extract_patch(frame: &LumaFrame, x: u32, y: u32, w: u32, h: u32) -> Vec<f32> {
    let mut out = Vec::with_capacity((w * h) as usize);
    for dy in 0..h {
        let yy = (y + dy).min(frame.height - 1) as usize;
        for dx in 0..w {
            let xx = (x + dx).min(frame.width - 1) as usize;
            out.push(f32::from(frame.y[yy * frame.width as usize + xx]));
        }
    }
    out
}

fn ncc_score(frame: &LumaFrame, x: u32, y: u32, w: u32, h: u32, tpl: &[f32]) -> f32 {
    if x + w > frame.width || y + h > frame.height || tpl.len() != (w * h) as usize {
        return -1.0;
    }
    let n = tpl.len() as f32;
    let mut sum_t = 0.0f32;
    let mut sum_f = 0.0f32;
    for dy in 0..h {
        let row = (y + dy) as usize * frame.width as usize + x as usize;
        let t_row = dy as usize * w as usize;
        for dx in 0..w as usize {
            sum_t += tpl[t_row + dx];
            sum_f += f32::from(frame.y[row + dx]);
        }
    }
    let mean_t = sum_t / n;
    let mean_f = sum_f / n;
    let mut num = 0.0f32;
    let mut den_t = 0.0f32;
    let mut den_f = 0.0f32;
    for dy in 0..h {
        let row = (y + dy) as usize * frame.width as usize + x as usize;
        let t_row = dy as usize * w as usize;
        for dx in 0..w as usize {
            let dt = tpl[t_row + dx] - mean_t;
            let df = f32::from(frame.y[row + dx]) - mean_f;
            num += dt * df;
            den_t += dt * dt;
            den_f += df * df;
        }
    }
    let den = (den_t * den_f).sqrt();
    if den < 1e-3 {
        // Uniform patch: fall back to negative normalized SAD so flat black/white boxes match.
        let mut sad = 0.0f32;
        for dy in 0..h {
            let row = (y + dy) as usize * frame.width as usize + x as usize;
            let t_row = dy as usize * w as usize;
            for dx in 0..w as usize {
                sad += (tpl[t_row + dx] - f32::from(frame.y[row + dx])).abs();
            }
        }
        return 1.0 - (sad / (n * 255.0));
    }
    num / den
}

fn search_patch(
    frame: &LumaFrame,
    pred_x: i32,
    pred_y: i32,
    w: u32,
    h: u32,
    radius: i32,
    tpl: &[f32],
) -> (u32, u32, f32) {
    let max_x = frame.width.saturating_sub(w) as i32;
    let max_y = frame.height.saturating_sub(h) as i32;
    let x0 = (pred_x - radius).clamp(0, max_x);
    let x1 = (pred_x + radius).clamp(0, max_x);
    let y0 = (pred_y - radius).clamp(0, max_y);
    let y1 = (pred_y + radius).clamp(0, max_y);

    let mut best_x = pred_x.clamp(0, max_x) as u32;
    let mut best_y = pred_y.clamp(0, max_y) as u32;
    let mut best = f32::NEG_INFINITY;

    // Coarse pass (stride 2) for rapid motion, then fine 3x3 refinement.
    for yy in (y0..=y1).step_by(2) {
        for xx in (x0..=x1).step_by(2) {
            let s = ncc_score(frame, xx as u32, yy as u32, w, h, tpl);
            if s > best {
                best = s;
                best_x = xx as u32;
                best_y = yy as u32;
            }
        }
    }
    let fx0 = (best_x as i32 - 2).clamp(0, max_x);
    let fx1 = (best_x as i32 + 2).clamp(0, max_x);
    let fy0 = (best_y as i32 - 2).clamp(0, max_y);
    let fy1 = (best_y as i32 + 2).clamp(0, max_y);
    for yy in fy0..=fy1 {
        for xx in fx0..=fx1 {
            let s = ncc_score(frame, xx as u32, yy as u32, w, h, tpl);
            if s > best {
                best = s;
                best_x = xx as u32;
                best_y = yy as u32;
            }
        }
    }
    (best_x, best_y, best)
}

fn nearest_frame_idx(frames: &[LumaFrame], time_s: f64) -> usize {
    frames
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (a.pts_s - time_s)
                .abs()
                .total_cmp(&(b.pts_s - time_s).abs())
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Tracks an object forward and backward from one or more manual keyframes with scene-cut
/// awareness, occlusion recovery, and motion-adaptive mask padding.
pub fn track_object_in_mp4(
    path: &Path,
    keyframes: &[Keyframe],
    blur: bool,
) -> Result<(Vec<TrackedBox>, Vec<RedactRect>)> {
    ensure!(!keyframes.is_empty(), "At least one keyframe is required");
    let frames = decode_all_luma(path)?;
    ensure!(!frames.is_empty(), "Video has no decodable frames");

    let cuts = detect_scene_cuts(&frames);
    let mut sorted_kf = keyframes.to_vec();
    sorted_kf.sort_by(|a, b| a.time_s.total_cmp(&b.time_s));

    let seed_kf = sorted_kf[0];
    let seed_idx = nearest_frame_idx(&frames, seed_kf.time_s);
    let fw = frames[seed_idx].width;
    let fh = frames[seed_idx].height;
    let init_w = seed_kf.width.max(4).min(fw);
    let init_h = seed_kf.height.max(4).min(fh);
    let init_x = seed_kf.x.min(fw - init_w);
    let init_y = seed_kf.y.min(fh - init_h);

    let mut boxes: Vec<Option<TrackedBox>> = vec![None; frames.len()];
    boxes[seed_idx] = Some(TrackedBox {
        frame_idx: seed_idx,
        pts_s: frames[seed_idx].pts_s,
        x: init_x,
        y: init_y,
        width: init_w,
        height: init_h,
        confidence: 1.0,
        status: TrackStatus::Seed,
    });

    let seed_tpl = extract_patch(&frames[seed_idx], init_x, init_y, init_w, init_h);

    // Track forward and backward from the seed keyframe.
    for dir in [1isize, -1isize] {
        let mut tpl = seed_tpl.clone();
        let mut cur_x = init_x as i32;
        let mut cur_y = init_y as i32;
        let mut cur_w = init_w;
        let mut cur_h = init_h;
        let mut vx = 0i32;
        let mut vy = 0i32;
        let mut occluded = false;

        let mut i = seed_idx as isize + dir;
        while i >= 0 && (i as usize) < frames.len() {
            let ui = i as usize;
            let cut_crossed = if dir > 0 {
                cuts.contains(&ui)
            } else {
                cuts.contains(&(ui + 1))
            };
            if cut_crossed {
                boxes[ui] = Some(TrackedBox {
                    frame_idx: ui,
                    pts_s: frames[ui].pts_s,
                    x: cur_x.max(0) as u32,
                    y: cur_y.max(0) as u32,
                    width: cur_w,
                    height: cur_h,
                    confidence: 0.0,
                    status: TrackStatus::SceneCut,
                });
                break;
            }

            // Check if a manual keyframe overrides this frame.
            if let Some(kf) = sorted_kf
                .iter()
                .find(|k| nearest_frame_idx(&frames, k.time_s) == ui)
            {
                let kw = kf.width.max(4).min(frames[ui].width);
                let kh = kf.height.max(4).min(frames[ui].height);
                let kx = kf.x.min(frames[ui].width - kw);
                let ky = kf.y.min(frames[ui].height - kh);
                tpl = extract_patch(&frames[ui], kx, ky, kw, kh);
                vx = kx as i32 - cur_x;
                vy = ky as i32 - cur_y;
                cur_x = kx as i32;
                cur_y = ky as i32;
                cur_w = kw;
                cur_h = kh;
                occluded = false;
                boxes[ui] = Some(TrackedBox {
                    frame_idx: ui,
                    pts_s: frames[ui].pts_s,
                    x: kx,
                    y: ky,
                    width: kw,
                    height: kh,
                    confidence: 1.0,
                    status: TrackStatus::Seed,
                });
                i += dir;
                continue;
            }

            let base_radius = (cur_w.max(cur_h) as i32).max(24);
            let radius = if occluded {
                base_radius * 2
            } else {
                base_radius
            };
            let (bx, by, conf) = search_patch(
                &frames[ui],
                cur_x + vx,
                cur_y + vy,
                cur_w,
                cur_h,
                radius,
                &tpl,
            );

            let status = if !occluded && conf < 0.45 {
                occluded = true;
                TrackStatus::Occluded
            } else if occluded && conf >= 0.55 {
                occluded = false;
                TrackStatus::Reacquired
            } else if occluded {
                TrackStatus::Occluded
            } else {
                TrackStatus::Tracked
            };

            if !occluded {
                vx = bx as i32 - cur_x;
                vy = by as i32 - cur_y;
                cur_x = bx as i32;
                cur_y = by as i32;
            }

            // Motion-adaptive padding to absorb rapid-motion blur.
            let speed = vx.abs().max(vy.abs()) as u32;
            let pad = (speed / 4).min(8);
            let ax = (cur_x.max(0) as u32).saturating_sub(pad);
            let ay = (cur_y.max(0) as u32).saturating_sub(pad);
            let aw = (cur_w + pad * 2).min(frames[ui].width - ax);
            let ah = (cur_h + pad * 2).min(frames[ui].height - ay);

            boxes[ui] = Some(TrackedBox {
                frame_idx: ui,
                pts_s: frames[ui].pts_s,
                x: ax,
                y: ay,
                width: aw,
                height: ah,
                confidence: conf,
                status,
            });
            i += dir;
        }
    }

    let tracked: Vec<TrackedBox> = boxes.into_iter().flatten().collect();
    let half_dt = if frames.len() > 1 {
        ((frames.last().map(|f| f.pts_s).unwrap_or(0.0) - frames[0].pts_s)
            / (frames.len() - 1) as f64)
            * 0.55
    } else {
        0.05
    };
    let rects: Vec<RedactRect> = tracked
        .iter()
        .filter(|b| !matches!(b.status, TrackStatus::SceneCut | TrackStatus::OutOfFrame))
        .map(|b| RedactRect {
            x: b.x,
            y: b.y,
            width: b.width,
            height: b.height,
            from_s: (b.pts_s - half_dt).max(0.0),
            to_s: b.pts_s + half_dt,
            blur,
        })
        .collect();

    Ok((tracked, rects))
}

/// Independently reopens an exported MP4, decodes every video frame, and verifies that every
/// active redaction region meets an opaque-luma or heuristic blur check. A blur check
/// cannot establish privacy, and this does not establish tracking coverage of a subject.
pub fn verify_redacted_mp4(output: &Path, rects: &[RedactRect]) -> Result<FrameVerifyReport> {
    ensure!(!rects.is_empty(), "At least one redaction rectangle is required");
    let raw = std::fs::read(output)?;
    let probe = mp4::probe(&raw)?;
    let metadata_clean = probe.findings.is_empty();
    let frames = decode_all_luma(output)?;
    let first = frames.first().ok_or_else(|| anyhow::anyhow!("No video frames to verify"))?;
    for rect in rects {
        rect.validate(first.width, first.height)?;
    }
    let mut checked_rects = vec![false; rects.len()];

    let mut redacted_frames_checked = 0usize;
    let mut failed_frames = 0usize;
    let mut weak_blur_frames = 0usize;

    for frame in &frames {
        let mut frame_had_rect = false;
        let mut frame_failed = false;
        let mut frame_weak_blur = false;

        for (index, r) in rects.iter().enumerate() {
            if frame.pts_s < r.from_s || frame.pts_s > r.to_s {
                continue;
            }
            let x0 = r.x.min(frame.width);
            let y0 = r.y.min(frame.height);
            let x1 = r.x.saturating_add(r.width).min(frame.width);
            let y1 = r.y.saturating_add(r.height).min(frame.height);
            if x1 <= x0 || y1 <= y0 {
                frame_failed = true;
                continue;
            }
            frame_had_rect = true;
            checked_rects[index] = true;

            if !r.blur {
                // Trim 1px border when region is >= 4x4 to tolerate H.264 4:2:0 / deblock ringing.
                let ix0 = if x1 - x0 >= 4 { x0 + 1 } else { x0 };
                let iy0 = if y1 - y0 >= 4 { y0 + 1 } else { y0 };
                let ix1 = if x1 - x0 >= 4 { x1 - 1 } else { x1 };
                let iy1 = if y1 - y0 >= 4 { y1 - 1 } else { y1 };
                let mut max_y = 0u8;
                for yy in iy0..iy1 {
                    let row = yy as usize * frame.width as usize;
                    for xx in ix0..ix1 {
                        max_y = max_y.max(frame.y[row + xx as usize]);
                    }
                }
                if max_y > 56 {
                    frame_failed = true;
                }
            } else {
                // Measure mean horizontal+vertical adjacent pixel gradient inside blurred rect.
                let mut grad_sum = 0u64;
                let mut count = 0u64;
                for yy in y0..y1.saturating_sub(1) {
                    let row = yy as usize * frame.width as usize;
                    let next_row = (yy + 1) as usize * frame.width as usize;
                    for xx in x0..x1.saturating_sub(1) {
                        let c = frame.y[row + xx as usize];
                        let rx = frame.y[row + xx as usize + 1];
                        let dy = frame.y[next_row + xx as usize];
                        grad_sum += u64::from(c.abs_diff(rx)) + u64::from(c.abs_diff(dy));
                        count += 2;
                    }
                }
                if count > 0 && (grad_sum as f64 / count as f64) > 35.0 {
                    frame_weak_blur = true;
                }
            }
        }

        if frame_had_rect {
            redacted_frames_checked += 1;
            if frame_failed {
                failed_frames += 1;
            }
            if frame_weak_blur {
                weak_blur_frames += 1;
            }
        }
    }

    Ok(FrameVerifyReport {
        frames_checked: frames.len(),
        redacted_frames_checked,
        failed_frames,
        weak_blur_frames,
        metadata_clean,
        passed: metadata_clean
            && failed_frames == 0
            && weak_blur_frames == 0
            && checked_rects.iter().all(|checked| *checked),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::{redact_mp4, write_synthetic_h264_mp4};
    use craft_video::h264::{Encoder, EncoderConfig, PacketFormat, Profile, RateControl, YuvFrame};

    fn write_moving_box_mp4(
        path: &Path,
        positions: &[Option<(u32, u32)>],
        cut_at: Option<usize>,
    ) -> Result<()> {
        let w = 64u32;
        let h = 64u32;
        let mut cfg = EncoderConfig::new(w, h, 10, 1);
        cfg.profile = Profile::Main;
        cfg.bframes = 0;
        cfg.rate = RateControl::Crf(18.0);
        cfg.format = PacketFormat::LengthPrefixed;
        let mut enc = Encoder::new(cfg).map_err(|e| anyhow::anyhow!("{e}"))?;
        let avcc = enc.avcc();
        let mut mdat_payload = Vec::new();
        let mut sizes = Vec::new();
        let mut offsets = Vec::new();

        for (idx, pos) in positions.iter().enumerate() {
            let bg_y = if cut_at.is_some_and(|c| idx >= c) {
                210u8
            } else {
                24u8
            };
            let mut y = vec![bg_y; (w * h) as usize];
            let u = vec![128u8; ((w / 2) * (h / 2)) as usize];
            let v = vec![128u8; ((w / 2) * (h / 2)) as usize];
            if let Some((bx, by)) = pos {
                for dy in 0..16 {
                    for dx in 0..16 {
                        let px = (bx + dx).min(w - 1) as usize;
                        let py = (by + dy).min(h - 1) as usize;
                        y[py * w as usize + px] = 230;
                    }
                }
            }
            let frame = YuvFrame {
                y: &y,
                u: &u,
                v: &v,
                y_stride: w as usize,
                uv_stride: (w / 2) as usize,
            };
            for pkt in enc
                .encode(&frame, idx as i64)
                .map_err(|e| anyhow::anyhow!("{e}"))?
            {
                offsets.push(pkt.data.len());
                sizes.push(pkt.data.len() as u32);
                mdat_payload.extend_from_slice(&pkt.data);
            }
        }
        for pkt in enc.flush() {
            offsets.push(pkt.data.len());
            sizes.push(pkt.data.len() as u32);
            mdat_payload.extend_from_slice(&pkt.data);
        }
        // Reuse write_synthetic_h264_mp4? No, use a tiny inline MP4 muxer via crate::video?
        // Actually write_synthetic_h264_mp4 takes solid frames, so let's build a minimal MP4 here
        // using crate::video::mux_h264_only!
        crate::video::write_raw_h264_mp4(path, w, h, 10, &avcc, &sizes, &mdat_payload)
    }

    #[test]
    fn tracks_moving_box_forward_backward_with_occlusion_and_verifies_export() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("moving.mp4");
        let out = dir.path().join("redacted.mp4");
        // Frames 0..8: bright 16x16 box moves rapidly right, occluded on frames 4..5, reappears at 6..7.
        let positions = vec![
            Some((6, 20)),
            Some((12, 20)),
            Some((18, 20)), // seed at t=0.2s
            Some((24, 20)),
            None, // occluded
            None, // occluded
            Some((36, 20)),
            Some((42, 20)),
        ];
        write_moving_box_mp4(&src, &positions, None).unwrap();

        let kf = Keyframe {
            time_s: 0.2,
            x: 18,
            y: 20,
            width: 16,
            height: 16,
        };
        let (tracked, rects) = track_object_in_mp4(&src, &[kf], false).unwrap();
        assert_eq!(tracked.len(), 8);
        assert!(tracked.iter().any(|b| b.status == TrackStatus::Occluded));
        assert!(tracked.iter().any(|b| b.status == TrackStatus::Reacquired));

        let rep = redact_mp4(&src, &out, &rects).unwrap();
        assert!(rep.verified);
        let verify = verify_redacted_mp4(&out, &rects).unwrap();
        assert!(verify.passed);
        assert_eq!(verify.failed_frames, 0);
        assert!(verify.redacted_frames_checked > 0);
    }

    #[test]
    fn scene_cut_stops_tracking_across_unrelated_scene() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("cut.mp4");
        let positions = vec![
            Some((10, 10)),
            Some((14, 10)),
            Some((18, 10)),
            Some((18, 10)),
            Some((18, 10)),
        ];
        write_moving_box_mp4(&src, &positions, Some(3)).unwrap();
        let kf = Keyframe {
            time_s: 0.0,
            x: 10,
            y: 10,
            width: 16,
            height: 16,
        };
        let (tracked, _) = track_object_in_mp4(&src, &[kf], false).unwrap();
        assert!(tracked.iter().any(|b| b.status == TrackStatus::SceneCut));
    }

    #[test]
    fn independent_frame_verifier_catches_unredacted_region() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("bright.mp4");
        write_synthetic_h264_mp4(&src, 64, 64, 10, &[(240, 240, 240); 4]).unwrap();
        let rect = RedactRect {
            x: 16,
            y: 16,
            width: 32,
            height: 32,
            from_s: 0.0,
            to_s: 0.5,
            blur: false,
        };
        let verify = verify_redacted_mp4(&src, &[rect]).unwrap();
        assert!(!verify.passed);
        assert!(verify.failed_frames > 0);
    }
}

