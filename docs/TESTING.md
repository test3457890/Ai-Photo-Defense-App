# Testing and evaluation

## Part 3 (Windows 10, 2026-11-26)

73 tests pass (`cargo test --locked --workspace`), 74 with `--all-features`. New coverage: colour pipeline (identity is lossless, linear-light exposure maths, control directions, monotone curves, HSL round-trip, band weights, LUT bake/parse round-trip and hostile .cube files, histogram/clipping, auto tone, presets), layers (opacity, gradient/radial/colour/luminance masks, blend modes, identity homography, degenerate corners, lens, canvas), redaction-above-adjustment ordering, proxy rendering equivalence, RAW previews (DNG, NEF, RAF, CR3 synthetic files, IFD loops, truncation), forensic JPEG quality estimates and C2PA detection, catalog filters/duplicates/missing files/SQL metacharacters/cache clearing, project save/backup/checkpoints/corruption fallback/autosave/unclean shutdown/temp cleanup, logger sanitisation/rotation/caps/report/clear with synthetic secrets, overlay and histogram rasterisation.

Real-application testing is recorded in `docs/design/PART3_DESIGN.md` with screenshots. Not tested: macOS, Linux, 8 GB systems, disk exhaustion, GPU failures, screen readers, multiple monitors.


## 0.2 increment (Windows 10, 2026-11-26)

46 automated tests pass with default features (`cargo test --locked --workspace`) and 47 with `--all-features` (adds the CLIP gradient test): edit geometry, undo/redo, straighten bounds, every selection shape, secure redaction pixel replacement, visual concealment, inverted selections, transparency flattening, recipe limits; metadata inspection of clean encoder output and injected EXIF/XMP/IPTC/comments/PNG text/ICC/trailing images; hostile container inputs; QR detection, classification and tiling for small codes; checksum-validated identifiers (Luhn, IBAN, ABA, ICAO MRZ, SIN); credential and network rules; country rules; values never serialized; profile round-trips and evidence thresholds; pipeline blocking of unredacted credential codes, verified redaction in the reopened PNG and JPEG; batch collection, recursion, exclusion, limits, Unix symlink refusal, review queue, duplicates, errors, cancellation, folder monitor; CLI scan/process/batch/profiles.

Runtime GUI test on Windows 10 with a synthetic CC0 fixture plus a fake Wi-Fi QR code: scan, conceal, drag-select redaction, undo, export and verification all observed; the exported file was independently re-scanned with the CLI (0 findings). Detection accuracy (precision/recall) has not been measured: no labelled dataset exists yet.

## 0.1 increment

Local checks on 2026-11-26 used Rust 1.100.0, Linux x86_64 and CPU processing. Re-run the README commands for the current commit. CI results must be inspected separately; workflow definitions alone do not establish success.

## Automated tests

The current suite contains 15 tests covering:

- Known MSE/PSNR/SSIM values and identical-image handling.
- No-clobber export and original-file preservation, including existing symlink destinations on Unix.
- JPEG/PNG/WebP/TIFF roundtrips and transparent JPEG refusal.
- Malformed input, high bit depth refusal, oversized PNG headers, EXIF orientation and metadata removal.
- Joint optimizer budget, opposing-objective cancellation, missing model refusal, cancellation and invalid settings.
- CLIP input gradients against finite differences on a small deterministic architecture.
- All eight transformation comparisons on identical source/candidate samples.
- Model path/license rejection and checksum failure before model parsing.
- CLI end-to-end export, reports, comparison metadata, inspection, lab and batch error continuation.

`cargo test --locked --workspace --all-features` passed locally. `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` passed. `cargo fmt --all --check` and workspace builds are required alongside the tests.

## Native Linux smoke test

A compiled native build was opened in the desktop session with tests/fixtures/calibration.png, an original synthetic CC0 illustration. The following were observed directly:

1. Image import through startup paths, native preview, and dimensions.
2. Ctrl+1/2/3 changed workspace while preserving the selected image and completed export.
3. Direct output-directory field exported a PNG and JSON sidecar without touching the source.
4. Original/export previews and measured zero RGB error for that lossless copy.
5. Eight real transformation comparisons completed in Technical mode; reports remain explicit that AI efficacy is not evaluated.
6. Light/Dark theme selection worked.

The minimal desktop initially had no functioning window manager focus and lacked a usable native file-dialog service. After restoring desktop window focus, keyboard and mouse tests ran. Native dialogs still did not appear in that environment; direct path inputs and startup/drag import are the fallback. Native dialog testing on ordinary Windows, macOS and supported Linux desktops remains pending. Lab reports use the configured output directory when present, avoiding the unavailable dialog service.

Actual screenshots: [Essential](screenshots/essential.jpg), [Professional](screenshots/professional.jpg), [Technical](screenshots/technical.jpg). They show this development build, not a finished protection product. The conceptual mockups are not software screenshots. Full keyboard traversal, drag/drop across different desktop environments, zoom/pan precision, cancellation during large batches and screen readers require further testing.

## Model computation smoke

See EXPERIMENTAL_CLIP.md for the checkpoint hash, synthetic input, observed feature energies and limitations. A full pretrained checkpoint also passed a Hugging Face Transformers/PyTorch reference comparison on the same synthetic input. The recorded maximum gradient error was 0.00000262633. This is not an attack benchmark. No unsupported efficacy percentage is inferred from these results.

## Defense efficacy protocol, pending implementation

Maintain a dataset manifest with source, license, consent where applicable, subject ID, image hash and split. Use synthetic/consenting identities with subject-disjoint training, reference and evaluation partitions. A public research face dataset alone is not permission to redistribute or use identities for a new purpose.

For each defender, record pinned source/model hashes, license, preprocessing, image resolution, epsilon, precision, iterations, seeds, time, peak RSS and hardware. Match budgets and conditions across baselines. Retain a clean baseline and undefended attack outputs. Compare per-method, joint and ablation configurations, including gradient interference.

Use fixed prompt/mask sets, multiple attack seeds and models that were not used for optimization. Measure editing instruction success and identity transfer with justified metrics and human review, not only embedding distance. Recognition-training and generative-training protection require actual retraining/fine-tuning protocols. Use confidence intervals and disclose failures. A transformed image's surviving pixel difference does not show surviving protection.

Transformation tests must cover platform JPEG/WebP recompression, resizing, crops, actual screenshots, denoising, adaptive purification and combinations. Current Gaussian blur is neither a realistic learned denoiser nor an adaptive purifier. Current WebP is lossless; lossy WebP testing is pending. Current timestamps measure transform plus quality computation, not CPU utilization or memory peaks.

Do not claim complete immunity, commercially unrestricted upstream rights, cross-model transfer or superiority until independently reviewable evidence supports the exact statement.

## Hosted platform verification

The initial foundation commit `124a906130166578d985e9f445034ed139e1fc16` passed all jobs in [Rust verification run 45868746998](https://github.com/test3457890/Ai-Photo-Defense-App/actions/runs/45868746998): Clippy, native-host tests, and target builds for Linux x64, Windows MSVC x64, macOS Intel and macOS Apple Silicon. [Dependency security run 45868746947](https://github.com/test3457890/Ai-Photo-Defense-App/actions/runs/45868746947) also passed. These verify compilation and automated tests, not Windows/macOS GUI runtime behavior or installers. New commits require fresh CI checks.


## Part 4 (2026-11-26)

Automated (`cargo test --workspace`, 88 tests; 89 with `--all-features`), including the `photo-media` crate (12 tests plus one ignored helper that writes the synthetic MP4 used for manual testing): format detection signatures and extension mismatch; MP4 probe, lossless metadata removal with `moov` first and last, chunk verification and 1,500 mutations; Matroska probe; audio decode, analysis, mute/bleep export with sample-exact verification and the crafted-WAV panic case; protection engine constraints with a fixture objective (masking ratio below 1.5, SNR floor, no clipping, refusal without objective); subtitle parse, scan and redaction; temporal flicker metrics; GIF/BMP metadata and image mutation fuzzing; media collection only when requested.

Manual, real Windows 10 desktop (release build), screenshots in `docs/screenshots/part4-*.png`: imported a folder with a WAV, an MP4 carrying GPS/device/XMP/timed-metadata, and an SRT containing an e-mail address and phone number; checked Essential, Professional and Technical views; added a bleep at 1.5 to 2.5 s and exported; played A and B loudness-matched; removed MP4 metadata; exported redacted subtitles. Independent CLI checks of the outputs: the MP4 had no remaining privacy findings and kept its video and audio tracks; the WAV had 122,868 frames in and out (length difference 0) with segmental SNR 64.1 dB; the SRT's second cue read `[redacted]`.

Not tested: real camera or phone videos (none available on the test machine; synthetic MP4 only), Matroska files from real muxers, Linux/macOS runtime, audio devices other than the default output, voice-cloning resistance (no model).
