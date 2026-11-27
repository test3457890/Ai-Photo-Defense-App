# Part 5 validation report (2026-11-26)

Scope: the twelve `ag/*` branches produced by the Google Antigravity agent on the owner's machine (Assignments 3 to 9 plus four extra Part 5 branches), reviewed, merged where they hold up, and checked by Claude. Requirement status for Parts 1 to 5: [IMPLEMENTATION_MATRIX.md](IMPLEMENTATION_MATRIX.md#part-5).

## Merge decisions

| Branch | Decision | Reason |
|---|---|---|
| `ag/voice-protect` | Merged (experimental, off by default, not in installers) | Code reviewed; test-only `unwrap`s; refuses pickle; no effect claimed |
| `ag/face-detect` | Merged; included in installers | Real-model check passed (below) |
| `ag/c2pa` | Merged; included in installers | Unit tests with generated chain; advisory ignore justified (ES256 only) |
| `ag/installers` | Merged with fixes | Pinned `cargo-wix 0.3.9`, `cargo-deb 3.8.0`; `appimagetool` 1.9.1 verified by SHA-256 instead of the moving "continuous" build; release builds include `faces` and `c2pa` |
| `ag/gui-smoke` | Merged | CI job; first results in the integration PR's checks |
| `ag/branding-logo` | Merged | Original artwork; window icon checked in the Windows app |
| `ag/voice-safe-playback` | Merged (CLI, needs `stt`) | Report marks it untested on devices; no effectiveness claim |
| `ag/ask-ai-accessibility` | Merged with a fix | Loopback enforced by parsed IP (no DNS); added a 4 MiB response cap |
| `ag/video-frames`, `ag/video-tracking-verify`, `ag/opus-wma` | **Not merged** | They depend on `filmcraft-h264`, `filmcraft-h264enc`, `filmcraft-opus` (crates.io). These crates have no repository link and are owned by `aphrody-dev` (account created 2026-09), which has no visible relation to the FilmCraft project. FilmCraft's own codecs live in `storytold/craft-libs` (Apache-2.0). The crate sources were inspected (no `unsafe`, no build scripts, no network or process use outside `#[cfg(test)]`), but a decoder for untrusted files must come from a verifiable source. Next step: port the branches to `craft-libs` pinned by commit |
| `ag/handoff-part5` | Not merged | Superseded by this report and the updated HANDOFF.md |

## Checks run by Claude on the integrated tree (Windows 10)

- `cargo fmt --all --check`: pass.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: pass (also default features for the desktop).
- `cargo test --workspace --all-features`: 110 passed, 0 failed, 4 ignored (need model files). Default features: 94 passed.
- `cargo deny --all-features check`: advisories, bans, licences, sources ok.
- `cargo audit`: no vulnerabilities after documenting `RUSTSEC-2023-0071` in `.cargo/audit.toml`; 17 informational warnings (unmaintained crates).

## Real-model and real-media checks

| Check | Result |
|---|---|
| Face detection, UltraFace `version-RFB-320.onnx` (MIT, git blob `a5e2309…` matches the official repository) | Neil Armstrong portrait (NASA, public domain, 3300x4125): 1 face, 216 ms. Apollo 11 crew (NASA, public domain, 4200x3300): 3 faces (3 people in the photo), 110 ms. Synthetic landscape: 0. Ignored test `detects_faces_with_ultraface_model` passes with the model. Two photos are not a benchmark: no precision/recall, skin-tone or lighting coverage is claimed |
| ECAPA conversion and parity (`tools/convert_ecapa.py`) | **Not run**: PyTorch and SpeechBrain are not installed and the machine has Python 3.15. Parity, Resemblyzer transfer and purification are unmeasured |
| Whisper transcription, voice-assistant safe playback | **Not run**: no Whisper weights on the machine; no assistant devices |
| Video redaction / tracking on a real phone clip, Opus on a real file | Not run: branches not merged (provenance) and no real phone video available |
| C2PA desktop signing with a CA-issued chain | Not run: only the unit test's generated chain |
| Installers and Linux/macOS GUI smoke | Run by CI after merge; see the PR checks |

## What this means

New in the app and verified: face detection on real photographs, C2PA inspection/signing (unit level), installers and GUI smoke CI (pending first run), branding. Present but unverified, and labelled so: voice protection, speech-to-text, wake-word suppression, Ask AI. Still not available in `main`: video frames and Opus (port pending), WMA/AC-3/AMR, person/object/document/plate detection and OCR, signed releases.

## Follow-up: video and Opus ported to FilmCraft's own libraries (2026-11-27)

`ag/video-frames`, `ag/video-tracking-verify` and `ag/opus-wma` were merged after replacing `filmcraft-h264`, `filmcraft-h264enc`, `filmcraft-opus` and the test-only `effectcraft-opusenc` (all owned by `aphrody-dev` on crates.io) with `craft-video` and `craft-audio` from `storytold/craft-libs` at commit `a4db799`, the revision FilmCraft's own workspace pins. The public API was identical (only import paths changed; craft's Opus encoder returns `Result`). `deny.toml` allows exactly that git source.

Checks: all media tests pass with `video,opus` (20); workspace 115 passed with all features, 94 with defaults; clippy, deny, audit clean. Real runs on a synthetic H.264 clip (320x240, 30 fps, 90 frames, generated with `cargo run -p photo-media --features video --example synth_h264`): `video-redact` 90 frames in and out, duration 3.0 s, A/V difference 0, metadata removed; `video-track` per-frame verification 90 checked, 0 failed. Desktop: frame preview, Play/Pause and scrub work (screenshot `docs/screenshots/part5-video-panel.png`).

Not tested: a real phone recording with AAC audio, HEVC/AV1 inputs (refused), real `.opus`/`.webm` files. Both features stay out of the installers until a real-file test passes.

