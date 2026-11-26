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


## 0.2 increment review (2026-11-26)

- Re-opened exports are decoded with `decode_bounded` (format allow-list, 16,384 px side, 24 MP, 256 MiB allocation, 8-bit only), the same limits as imports.
- Metadata parsers operate only on the in-memory byte snapshot, with bounds-checked reads, segment/chunk iteration caps (4,096) and TIFF IFD loop detection; malformed inputs are tested not to panic.
- QR/barcode payloads and OCR text are data only: links are never opened, nothing is executed, and payloads are never logged or written to reports (tests assert secret strings are absent from serialized findings and saved reports).
- Text scanning is capped at 1 MiB per span and 256 matches per rule; the `regex` crate guarantees linear-time matching.
- Edit recipes, profiles and manifests are size-limited and use `deny_unknown_fields` where applicable; selection complexity, brush radius, blur sigma, block size and output dimensions are bounded.
- Batch collection never follows symbolic links or junctions, requires every canonical path to stay inside the selected root, excludes the output directory and caps depth (16) and file count (10,000).
- Output files are written via private temporary files and atomic no-clobber renames; names are sanitized and uniquely suffixed.
- No `unsafe` code in workspace crates.
- `cargo audit`: no vulnerabilities. Informational: RUSTSEC-2026-0543 chrono unmaintained (new, via rxing; no fixed release), lru unsound advisories via the Iced text stack (pre-existing), several unmaintained transitive crates (pre-existing). `cargo deny check` passes with the new deny.toml.
- Open items: fuzzing harness, decoder sandboxing/process isolation, Windows junction test, split model local-use vs redistribution approval.


## Part 3 review (2026-11-26)

- New untrusted inputs: RAW containers (bounded IFD walk with loop detection, offset/length checks, candidate cap, previews decoded under the standard limits), .cube LUTs (16 MiB cap, size 2..65, finite values, exact entry count), project files (64 MiB cap, schema and field validation, symlink refusal), catalog queries (parameterised SQL with escaped LIKE patterns; column names allow-listed).
- Logging: closed schema, sanitiser for paths/numbers/tokens/URLs/e-mails/credentials, control and bidi characters stripped, local only, rotation and caps, no memory dumps. Tests use synthetic secrets.
- Crash recovery never replaces valid data with invalid data: backups and checkpoints are only taken from files that parse and validate.
- Thumbnail cache is off by default; clearing overwrites with zeros before deletion (SSD and snapshot limitations documented).
- Vulnerability found and fixed: rusqlite 0.38's bundled SQLite (RUSTSEC-2026-0459, medium) replaced by rusqlite 0.43 before any release. `cargo audit` and `cargo deny check` pass.
- Still open: fuzzing, decoder process isolation, signed updates (no updater exists), font parsing (no text layers yet).
