# AI Photo Defense development handoff

Updated 2026-11-26. Start here before changing the repository. This is a working development increment, not completion of the full project brief.

## Where work stands

Six Rust crates implement real image processing, three shared-state native workspaces, CLI, transformation measurements, local model verification and an experimental CLIP encoder objective. README describes exactly what runs. ROADMAP.md maps remaining work to the ten requested phases. docs/research/AUDIT.md identifies all eighteen requested research candidates and additional counterexamples. No upstream defense code or pretrained weights are bundled.

The desktop export is metadata removal only. Do not rename it Protect, enable protection presets or advertise deepfake/training resistance without real model evaluation. The optional CLI feature experimental-clip computes real adversarial input optimization but only against its own feature-energy objective. Current efficacy status is always unverified. Prototype computations and synthetic gradient fixtures must stay clearly distinguished from validated protection.

## Verified locally

- Rust 1.100.0 Linux builds, 15 tests, fmt and clippy with warnings denied.
- Real native image preview, direct-path export, source preservation, before/after metrics, workspace switching, theme changes and transformation lab.
- Screenshots of the working app under docs/screenshots, using the original CC0 synthetic calibration fixture.
- A real CLIP safetensors checkpoint loaded and produced gradients; a two-step synthetic-input smoke test changed feature energy. See docs/EXPERIMENTAL_CLIP.md for exact scope and checkpoint hash.
- Eight ML-stack ZIP dependency vulnerabilities removed by upgrading Candle. Updated RustSec scan: zero known vulnerabilities, informational warnings documented in docs/SECURITY_REVIEW.md.

Native file dialogs did not open in the minimal Linux desktop. Import and output path fields plus startup arguments work around that. Report-save dialog fallback is still missing; CLI lab writes reports directly. Full Windows/Mac runtime testing is not complete. Build CI must be read, not assumed successful.

## Continue in this order

1. Inspect current GitHub CI and PR status. Fix any target/toolchain/workflow failures before merging or releasing. Refresh this file with observed results and commit IDs.
2. Finish model licensing and conversion provenance. Separate local-use approval from redistribution in the manifest. Do not set approval fields just to make tests run.
3. Verify full pretrained CLIP inference and gradients against a primary reference, with exact preprocessing. The current bilinear stretch intentionally differs from standard CLIP preprocessing and may have poor transfer.
4. Add a licensed diffusion-encoder defense and real attack evaluation adapter. Begin matched-budget comparisons before adding more model names. No random-noise fallback.
5. Add EOT, conflict diagnostics, perceptual fidelity and adaptive objective weights only when measured ablations justify them. Retain one shared perturbation budget.
6. Improve image fidelity: ICC conversion, high bit depth, multipage/animation awareness. Add constrained decoder workers and fuzzing.
7. Upgrade desktop dependencies, test dialogs/accessibility on each OS, add proper docking/persistence and a privacy-reviewed SQLite history schema.
8. Add TrustMark and C2PA as separate optional provenance features after their runtime, weight, certificate and key handling are reviewed.
9. Package signed/notarized releases only after runtime and security gates pass. CI debug artifacts are not production installers.

## Collaboration rules

Record the starting commit, concrete files changed, checks and blockers. Preserve user changes; do not reset branches or overwrite pending work. Keep this file and ROADMAP.md accurate after each increment. Do not claim the project is finished because documentation, a UI or a model loss exists. Never fabricate benchmark data or screenshots. No private photos or unsafe executable models in the repository.

Suggested About description, if repository metadata editing is unavailable: "Offline Rust photo privacy utility and experimental adversarial-defense workbench with three desktop workspaces, CLI and reproducible measurements."
