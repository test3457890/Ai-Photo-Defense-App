# Security review status

2026-11-26: source review, focused tests and a RustSec dependency scan were completed for this development increment. This is not an independent penetration test or a release approval.

## Remediated findings

- Limited decoder entry points to supported image formats and a single bounded byte snapshot; enforce dimensions, pixel count and decoder allocations.
- Rejected high bit depth inputs explicitly, preserved orientation, refused JPEG alpha loss and retained originals.
- Used private same-directory temporary output and no-clobber publication; tested existing destinations and Unix symlinks.
- Bounded model and manifest reads, required safetensors and exact SHA-256/size, rejected path separators and missing license approval, rechecked the snapshot passed to the ML backend.
- Validated optimization settings, gradients, dimensions, finite values and cancellation.
- Removed Iced's default broad image codecs from user-file handling with image-without-codecs; previews use decoded RGBA handles.
- Updated Candle 0.9.2 to 0.11.1, moving ZIP from 7.4.0 to 9.8.2 and eliminating the eight reported ZIP vulnerabilities.

## Dependency scan

The updated cargo-audit scan returned zero known vulnerabilities. Informational warnings remain for legacy transitive instant, paste, pin-utils, rustybuzz, ttf-parser and D-Bus serialization/name packages. They need upgrade/replacement review, not a blanket ignore list.

Two unsoundness warnings name lru 0.12.5 in the lockfile's optional Iced/wgpu graph. `cargo tree --workspace --all-features --target all -i lru` reported no enabled reverse dependencies. The app does not enable Iced's wgpu renderer; it uses tiny-skia. Do not enable that old optional graph until it is upgraded to patched dependencies. Adding GPU inference with another framework still requires a fresh target-specific graph and audit. No advisory has been suppressed in this repository.

An SBOM generator and CI audit workflow are included. The SBOM is an inventory, not a legal compliance attestation or proof that a package is reachable in a particular binary. Final target artifacts need exact component/license collection. Scan results are time-sensitive and must be refreshed for releases.

## Open release blockers

- Model/data redistribution approval and independent validation of actual defense efficacy.
- Newer Iced and native dialog dependency migration; full screen reader and platform keyboard tests.
- Decoder fuzzing, hard CPU/RSS limits and helper-process isolation for hostile inputs.
- ICC color management and high-quality photo fidelity validation.
- Persistent-history privacy schema, safe model catalog/download signatures and trusted update channel.
- Windows 10/11, macOS Intel/Apple Silicon runtime verification and signing/notarization.
- Certificate/key lifecycle before optional C2PA signing; no authenticity indicator without successful signature verification.

CLI export image and sidecar publication are separate; an image can be saved when report publication fails. The error is reported and the copy is retained. This preserves user data but is not an atomic two-file transaction. Existing names remain untouched. Same-user adversaries controlling the selected output directory are outside the current filesystem boundary.
