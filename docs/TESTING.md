# Testing and evaluation

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

The minimal desktop initially had no functioning window manager focus and lacked a usable native file-dialog service. After restoring desktop window focus, keyboard and mouse tests ran. Native dialogs still did not appear in that environment; direct path inputs and startup/drag import are the fallback. Native dialog testing on ordinary Windows, macOS and supported Linux desktops remains pending. Save-report dialog fallback is not yet implemented; use the CLI for report files when the desktop dialog service is unavailable.

Actual screenshots: [Essential](screenshots/essential.jpg), [Professional](screenshots/professional.jpg), [Technical](screenshots/technical.jpg). They show this development build, not a finished protection product. The conceptual mockups are not software screenshots. Full keyboard traversal, drag/drop across different desktop environments, zoom/pan precision, cancellation during large batches and screen readers require further testing.

## Model computation smoke

See EXPERIMENTAL_CLIP.md for the checkpoint hash, synthetic input, observed feature energies and limitations. This is not an attack benchmark. No unsupported efficacy percentage is inferred from these results.

## Defense efficacy protocol, pending implementation

Maintain a dataset manifest with source, license, consent where applicable, subject ID, image hash and split. Use synthetic/consenting identities with subject-disjoint training, reference and evaluation partitions. A public research face dataset alone is not permission to redistribute or use identities for a new purpose.

For each defender, record pinned source/model hashes, license, preprocessing, image resolution, epsilon, precision, iterations, seeds, time, peak RSS and hardware. Match budgets and conditions across baselines. Retain a clean baseline and undefended attack outputs. Compare per-method, joint and ablation configurations, including gradient interference.

Use fixed prompt/mask sets, multiple attack seeds and models that were not used for optimization. Measure editing instruction success and identity transfer with justified metrics and human review, not only embedding distance. Recognition-training and generative-training protection require actual retraining/fine-tuning protocols. Use confidence intervals and disclose failures. A transformed image's surviving pixel difference does not show surviving protection.

Transformation tests must cover platform JPEG/WebP recompression, resizing, crops, actual screenshots, denoising, adaptive purification and combinations. Current Gaussian blur is neither a realistic learned denoiser nor an adaptive purifier. Current WebP is lossless; lossy WebP testing is pending. Current timestamps measure transform plus quality computation, not CPU utilization or memory peaks.

Do not claim complete immunity, commercially unrestricted upstream rights, cross-model transfer or superiority until independently reviewable evidence supports the exact statement.
