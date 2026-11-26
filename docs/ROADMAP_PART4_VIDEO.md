# Part 4 roadmap: video and multimedia (not started)

Video editing, audio editing and timelines are deliberately out of scope for Part 3. This file keeps the research so Part 4 does not need a rewrite.

Architecture already prepared:

- `EditStack` operations are frame-independent and serializable, so a per-clip or per-frame stack can reuse them.
- Redaction masks are geometric `Selection`s that can later be keyframed or tracked.
- The pipeline separates render, encode and independent verification; a video pipeline needs the same stages per frame plus container-level metadata inspection.
- The structured logger, project journal and crash recovery are not image-specific.

Research to evaluate in Part 4:

- FilmCraft (https://github.com/storytold/filmcraft): native Rust UI, docking, command architecture, GPU rendering, autosave and project organisation. Its video editor must not be integrated before Part 4.
- Decoding/encoding: FFmpeg via a separate process versus Rust-native demuxers; licensing (LGPL/GPL builds) needs review.
- Face and plate tracking across frames requires the same licence-reviewed detection models listed in `docs/research/MODEL_LICENSES.md`.
- Audio: speech in recordings can disclose names and locations; transcription models would be needed and must run locally.
