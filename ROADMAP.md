# Development roadmap

The project brief defines the destination. This file distinguishes implemented work from remaining work. A checked task is not a protection efficacy claim.

## Phase 1: research and licensing

- [x] Identify intended primary sources and separate name collisions.
- [x] Record first-pass code-license, model and maintenance findings for the requested projects.
- [ ] Pin and inspect all upstream license files, transitive code, weights and dataset rights.
- [ ] Reproduce selected methods under equivalent test conditions.
- [ ] Approve a distributable model catalog and independently authenticated hashes.

## Phase 2: interface concepts

- [x] Produce and review three distinct concept layouts before implementation.
- [x] Implement all three as switchable native workspaces with shared state.
- [x] Capture actual working Linux screenshots.
- [ ] Finish compact Essential sizing, filmstrip, splitters, docking and full-resolution preview.
- [ ] Complete accessibility and native OS menu integration; confirm keyboard behavior on each OS.

## Phases 3 and 4: architecture and image workflow

- [x] Modular Rust core, engine, models, lab, CLI and desktop.
- [x] Safe local import, orientation, preview, no-clobber batch export, hashes and reports.
- [x] JPEG, PNG, lossless WebP, TIFF, comparisons and difference visualization.
- [x] Cancellation between exported files, error recovery and bounded queue.
- [x] Direct path fields as a fallback for missing native dialog services.
- [ ] Color-managed ICC conversion with 16-bit preservation where supported.
- [ ] Animated/multipage awareness, folder monitoring, persistent SQLite history and collections.
- [ ] Social-media output profiles with real recompression validation.

## Phases 5 and 6: adversarial protection

- [x] Shared-budget multi-objective PGD engine with finite-gradient checks.
- [x] Experimental Candle CLIP encoder and exact preprocessing gradient path.
- [x] Tiny-model finite-difference tests and real-checkpoint objective smoke test.
- [x] Pretrained-checkpoint output/gradient parity against the primary PyTorch framework on one synthetic input.
- [ ] Broader parity coverage, trained model acceptance and downstream editor validation.
- [ ] Actual verified implementations targeting diffusion editing, face swapping and identity customization.
- [ ] Adaptive selection, EOT, perceptual penalties, joint model tuning and interference diagnostics.
- [ ] Quick/Balanced/Maximum/Custom protection presets based on measured tradeoffs.
- [ ] Burn/wgpu GPU compute, CUDA/Metal/Vulkan options with bounded CPU fallback.
- [ ] TrustMark watermark integration and C2PA signing/verification with trustworthy keys/certificates.

## Phase 7: Defense Lab

- [x] Deterministic paired JPEG/WebP/resize/crop/blur measurements with named metric definitions.
- [ ] Editor, identity, face-swap, generative-training and recognition-training evaluation adapters.
- [ ] Rights-cleared datasets with synthetic/consenting identities and subject-disjoint splits.
- [ ] Holdout models, actual screenshot capture, adaptive purification, denoising and removal tests.
- [ ] Hardware/time/RSS monitoring, LPIPS or a justified alternative, repeated trials and uncertainty.
- [ ] Matched-budget comparisons with research baselines and joint-defense ablations.

## Phases 8 through 10: security, platforms and release

- [x] Threat model, local privacy policy, malformed input and export safety tests.
- [x] Dependency scan; upgrade ML stack to remove eight ZIP advisories.
- [x] CI definitions for Linux, Windows, macOS Intel and Apple Silicon builds; automated dependency checks and SBOM generation.
- [x] Initial hosted CI passed on all four targets and dependency security. New commits still require green checks.
- [ ] Complete runtime platform verification beyond compilation and automated tests.
- [ ] Triage/remove unmaintained transitive dependencies; keep disabled GPU cache advisories outside enabled graphs.
- [ ] Decoder fuzzing, subprocess resource isolation, model-schema hardening and independent review.
- [ ] Windows 10/11 and both Mac architectures runtime tests, installers and accessibility checks.
- [ ] Signed/notarized binaries, full third-party notices, signed updates and release-candidate gate.
- [ ] Downloadable public releases only after the above verification succeeds.

Future: video, browser extension, mobile, FELIX integration and developer APIs. Each needs its own privacy, performance and validation plan. None is included in current capability claims.
