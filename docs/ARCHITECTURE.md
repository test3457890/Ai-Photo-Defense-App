# Architecture

A Cargo workspace shares code between the desktop interface and CLI. There is no web server, account system, image upload endpoint or network client in the application.

| Crate | Responsibility | Status |
|---|---|---|
| photo-core | Bounded decoding, orientation, encoding, no-clobber exports, hashes, metrics | Implemented |
| photo-engine | Objective trait and coordinated projected-gradient optimization | Implemented, no downstream efficacy validation |
| photo-engine/clip | Candle CLIP ViT-B/32 vision transformer and image gradients | Experimental opt-in feature |
| photo-models | License manifest and local safetensors verification | Implemented; downloads/catalog pending |
| photo-lab | Paired deterministic transformations and quality reports | Implemented; AI evaluation adapters pending |
| photo-edit | Non-destructive edit stack, geometry, masks, concealment, secure redaction, flattening | Implemented and tested |
| photo-privacy | Metadata inspector, QR/barcode detection, sensitive-text rules, privacy profiles | Implemented and tested; model-based detectors unavailable |
| photo-pipeline | Secure export pipeline, re-open verification, batch collection and processing, folder monitor | Implemented and tested |
| photo-cli | Clap automation interface | Implemented |
| photo-desktop | Iced workspaces and shared document collection | Functional; tested on Windows 10 |

## Edit and export model

Each photograph owns an `EditStack` of serializable operations applied to the untouched original at render time; undo moves a cursor and a new edit discards the redo branch. Selections are stored geometrically (rect, ellipse, polygon, brush, union, refined) and rasterized to hard-edged binary masks so secure redaction covers whole pixels. `flatten` produces one raster and clears colour under fully transparent pixels.

`photo_pipeline::prepare` runs: initial scan of the original bytes, render, metadata-free encode, then `verify` reopens the encoded bytes under the import limits, inspects container metadata, re-runs code detection, checks every redacted pixel (exactly for lossless formats, interior with tolerance for JPEG), and checks transparency. `publish` writes the image and report with no-clobber unique names, and refuses when verification failed unless the caller explicitly accepts. Redaction regions are re-located for verification only when no geometry edit follows a redaction; otherwise the report says so.

## Privacy Scan model

Detectors report availability. Findings carry category, evidence (Confirmed, Pattern, Contextual), source, optional region and value length, never the value. Text rules run over decoded code payloads, metadata text and any OCR text an installed engine supplies through `ScanInput.ocr_text`. Categories whose detector is unavailable are listed in `not_examined` so the UI and reports never imply they were checked.

## Desktop document lifecycle

One state object owns file entries, thumbnails, selected item, export results, lab report and settings. Workspace changes replace only the layout. Images are decoded in blocking workers, not on the UI thread. Only thumbnails remain in the collection; originals are re-read on export. A changed original can therefore produce a different export than its old thumbnail. Source and output hashes in the report identify the bytes actually processed. Refresh/reimport after external edits. The collection is session-only and paths are not persisted.

The queue is capped at 64 entries. Batch export processes one image at a time. Cancellation is cooperative between files, not during decoding or encoding. Each individual export is published with a same-directory private tempfile and a no-clobber persist. Image and report publication are separate operations: a report failure can leave a successful image export. The CLI surfaces this condition; the desktop log retains the error. Existing output files are never deleted for rollback.

## Joint optimization

`Objective::loss_gradient` returns a scalar to maximize and its derivative with respect to original-resolution RGB floats in [0,1]. Adapters must include their preprocessing Jacobian. Each iteration normalizes each objective's gradient by its mean absolute magnitude, combines positive user weights, subtracts a squared-fidelity penalty gradient, takes a signed step, and projects onto the intersection of [0,1] and one shared L-infinity box around the original. Equal and opposite gradients cancel. Noise layers are not applied sequentially.

This algorithm is a starting point, not the requested fully adaptive engine. Per-image method selection, learned weights, expectation over transformations, perceptual fidelity losses, gradient conflict diagnostics, holdout-model validation and robust multi-model tuning remain pending. Conflicting objectives can still reduce effectiveness; the optimizer does not promise otherwise.

A true `efficacy_verified` flag is never emitted by the present implementation. Encoder loss is a proxy, not an editing/identity success metric. All fixture objectives are confined to tests.

## Technology decisions

Iced 0.13 was selected for a compact native Rust interface with a software renderer, avoiding a GPU requirement for normal desktop use. Its menu strip is drawn by Iced; it is not yet an OS-native menu integration. `image-without-codecs` ensures user-controlled files go through photo-core rather than Iced's broader decoder defaults. Native file dialogs use rfd and the platform dialog service.

Candle supplies tensor operations and autodifferentiation for an independent CLIP architecture implementation. No Python runtime is needed by the app. Burn remains a candidate for wgpu training/inference after parity and security work. Adding both ML frameworks before a validated use case would increase the build and audit surface. CUDA, Metal, Vulkan and wgpu compute adapters are not implemented. Rayon is present transitively in tensor dependencies; CPU batch jobs intentionally remain sequential to bound memory.

SQLite history and collection persistence are deferred until a storage/privacy schema and deletion policy are designed. SHA-256 identifies model and image bytes; BLAKE3 supports local image integrity comparison. Hashes are not content-authenticity signatures. C2PA and TrustMark are integration candidates, not current dependencies or enabled capabilities.

## Developer SDK

External Rust clients can depend on the workspace crates by path or pinned git revision. The API is pre-1.0 and may change. Implement `photo_engine::Objective`, validate exact gradients against finite differences, then pass references through `WeightedObjective` to `optimize`. Save only after quantization, decoding and quality validation. Use `photo_core::save_new` for publication. Do not implement adapters that execute arbitrary imported model code or deserialize Python pickle.

The CLI's experimental CLIP command provides an end-to-end reference. Integration must document model hash, code and weights license, preprocessing, objective direction, budget, numerical precision, hardware and downstream evaluation before any efficacy claim.
