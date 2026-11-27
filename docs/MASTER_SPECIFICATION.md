# AI Photo Defense: master development, security, implementation and quality assurance instructions

A complete, condensed restatement of the owner's instructions (section numbers and every listed item match the original; prose is shortened) so ChatGPT, Claude and Codex work from identical requirements (§34). If the owner supplies the original text, it supersedes this file. Track progress in [IMPLEMENTATION_MATRIX.md](IMPLEMENTATION_MATRIX.md).

## 1. GitHub repository

https://github.com/test3457890/Ai-Photo-Defense-App

## 2. Assignment

ChatGPT Work was assigned Part 1. Claude's responsibility: independently audit ChatGPT's work; fix incomplete, insecure or incorrectly implemented functionality; complete unfinished Part 1 requirements; implement all Part 2 requirements; integrate everything into one Rust-native desktop application; perform comprehensive security and functionality testing; push to GitHub; create, review and merge validated pull requests; prepare verified cross-platform releases; produce a development handoff for ChatGPT and Codex to audit.

Do not assume previous AI-generated code is correct because it was marked complete. Real people will use this application. Prioritize security, privacy, reliability and genuine functionality over the appearance of completion.

## 3. Project objective

A comprehensive open-source, privacy-first photo protection application combining adversarial AI image protection, anti-deepfake technology, facial identity cloning resistance, AI training resistance, professional photo editing, automatic sensitive information detection, permanent redaction, privacy verification, image authenticity verification, batch processing, and AI protection research and benchmarking. Preserve the original visual appearance as closely as possible while making unauthorized AI manipulation more difficult. Never claim any photograph is completely immune to AI manipulation.

## 4. Mandatory Rust development requirements

Rust-first native desktop application. Production components in Rust: desktop GUI, adversarial protection engine, photo editing, cropping/resizing, blur/pixelation, permanent redaction, Privacy Scan, object/face detection integration, OCR integration, sensitive information and credential detection, metadata removal, batch processing, model management, Defense Lab, benchmarking, secure export, local database, CLI, developer SDK, application security, update management. Modular Cargo workspace with a shared processing engine.

Do not build the primary application with Python, JavaScript, TypeScript, React, Electron, Node.js or HTML/CSS desktop interfaces, and do not wrap a Python application in Rust. Python is allowed for development-time research, training, conversion and reference comparison. End users must not need Python, Node.js or other programming environments. External native libraries and ML runtimes are permitted when technically necessary, documented, justified and securely packaged. Do not sacrifice protection effectiveness merely to avoid a necessary dependency.

Technology candidates to evaluate (not all required): Iced, Burn, Candle, ORT, wgpu, image-rs, imageproc, Photon, OCRS, RTen, RXing, Rayon, Tokio, Rusqlite, Serde, Clap, C2PA Rust, BLAKE3, SHA-256.

## 5. Supported platforms

One codebase for Windows 10/11 (x86-64, ARM64 where feasible), macOS Intel and Apple Silicon, Linux x86-64 (ARM64 where feasible). CPU-only operation, optional GPU (CUDA, Metal, Vulkan where supported), automatic hardware detection, configurable resource limits, graceful CPU fallback. Must work without a dedicated GPU.

## 6. Application privacy requirements

Fully local processing; offline after models are installed; no mandatory accounts or subscriptions; no hidden telemetry, advertising or sale of data; no unauthorized uploads or silent cloud processing; user-controlled model downloads and updates; transparent network activity; original preservation. Never transmit photographs, credentials, facial information or other sensitive data without explicit authorization.

## 7. Desktop design requirements

Must resemble professional photography/engineering software (inspiration: Capture One, Lightroom Classic, Affinity Photo, ImageOptim, GIMP, Darktable) without copying proprietary assets. Use traditional menus, compact toolbars, native typography, rectangular controls, minimal rounding, restrained accents, charcoal and light themes, thin borders, dockable/resizable panels, keyboard shortcuts, context menus, accessible controls, large photographic workspaces, minimal animation. Avoid marketing headings, dashboard cards, excessive rounding, shield branding, neon themes, gradients, fake protection scores, empty space and web-like layouts. Functionality first.

## 8. Three interchangeable interface modes

All share one engine, settings and project data; switching never loses work.

- **Essential** (ImageOptim-like): minimal UI, drag and drop, simple protection presets, automatic privacy scanning, basic crop, basic redaction, batch, quick export, simple warnings.
- **Professional** (Capture One / Affinity-like): large preview, original/protected comparison, adjustable panels, collections, detailed editing, privacy tools, layered masks, advanced protection settings, batch export, quality measurements.
- **Technical** (GIMP / Darktable / engineering-like): Defense Lab, model management, individual defense configuration, benchmarking, perturbation visualization, processing logs, technical validation reports, robustness testing, CPU/GPU monitoring, advanced detection configuration.

## 9. Shared workspaces

AI Defense, Privacy Scan, Edit and Redact, Defense Lab, Batch Processing, Model Library, Protection History, Settings.

## 10. Part 1: AI protection engine

Develop and evaluate protection against unauthorized AI editing, inpainting, image-to-image, identity cloning, face swapping, deepfakes, unauthorized training, facial-recognition training, style imitation; cross-model transferability; resilience to compression, cropping, resizing, screenshots, denoising and purification. Investigate adaptive image-specific protection and multi-model joint optimization; do not simply stack noise; determine interference; balance effectiveness, visual quality, speed, memory, hardware and social-platform compatibility. Presets Quick, Balanced, Maximum, Custom must correspond to real configurations. No invented effectiveness percentages.

## 11. Research repositories

Investigate PhotoGuard, FaceShield, FaceOff, FaceLock, SGD-Guard, Diffusion Immunization, DiffusionGuard, DiffVax, Fawkes, DCT-Shield, BlurGuard, VETO, Glaze, Nightshade, Adobe TrustMark, C2PA Rust, ShieldShot, DeepSafe (initial URLs: MadryLab/photoguard, Shawn-Shan/fawkes, huzimun/FaceOff, taco-group/FaceLock, kuai-lab/iccv25_faceshield, choi403/DiffusionGuard, adobe/trustmark, contentauth/c2pa-rs) and newer work. For each: official repository, publication, code availability, language, licence, weight availability and redistribution rights, commercial restrictions, hardware, weaknesses, maintenance, Rust feasibility, capabilities. Reimplement compatible techniques in Rust with attribution; never incorporate unlicensed or incompatible code or weights.

## 12. Part 2: Professional photo editing

Non-destructive engine: freeform/fixed-aspect/custom-pixel cropping, resizing, rotation, straightening, horizontal/vertical flip, rule-of-thirds guides, aspect lock, zoom, pan, brush/rectangle/ellipse/polygon/lasso/object selection, layered masks, mask refinement, undo/redo, history, original vs edited comparison. Export aspect ratios Original, 1:1, 4:3, 3:2, 16:9, 9:16, 4:5, Custom; configurable social presets. Never overwrite the original without explicit authorization.

## 13. Blur and permanent redaction

Gaussian blur (adjustable), pixelation, mosaic, solid black/white/custom opaque redaction, full-object masking, selective face blur, background blur and concealment, full-person, screen-content, document and manual brush concealment. Clearly distinguish visual concealment from secure redaction; recommend opaque redaction for sensitive data. Secure redaction replaces selected pixels and flattens the export; no original pixels may remain beneath masks.

## 14. Advanced Privacy Scan

Local, multi-detector: computer vision, object detection, segmentation, face detection, OCR, NER, pattern matching, context analysis, document recognition, barcode and QR recognition, open-vocabulary detection, tiled high-resolution scanning, multi-scale detection, perspective correction. Categories:

- **14.1 Electronics:** computers, laptops, monitors, phones, tablets, smartwatches, TVs, terminals, CCTV monitors and cameras, server racks, network equipment, routers, switches, radios, communication devices, USB and smart-home devices, equipment labels and serials, notifications. Conceal a whole device, its screen, or individual on-screen information.
- **14.2 Documents:** passports, licences, government/student/employee/military IDs, access cards, medical documents, prescriptions, financial statements, cheques, credit/debit cards, legal, contracts, tax, school records, handwritten notes, notebooks, mail, envelopes, shipping labels, boarding passes, itineraries, tickets, receipts, business cards, certificates, printed photographs, whiteboards, sticky notes, calendars, schedules. Conceal fields or entire documents.
- **14.3 Personal information:** names, addresses, emails, phones, dates of birth, SSNs, passport/licence/student/employee numbers, financial and card numbers, insurance, medical IDs, usernames, signatures, emergency contacts. Configurable country rules; distinguish confirmed pattern matches from uncertain contextual detections.
- **14.4 Cybersecurity:** passwords, API keys, auth/access tokens, private keys, recovery codes, session IDs, connection strings, public/private IPs, MACs, Wi-Fi names and passwords, hostnames, internal domains, file paths, usernames, network diagrams, firewall configs, terminal output, auth QR codes, address bars, tabs, notifications, source code with secrets, environment variables, cloud credentials, internal URLs, system identifiers. Never verify credentials against external services. Never put sensitive values in logs, crash reports, analytics, ordinary reports or committed tests.
- **14.5 Location:** street signs, house numbers, addresses, building names, hotel room and apartment numbers, plates, aircraft registrations, parking permits, maps, GPS coordinates, location names and signage, navigation displays, travel documents, geotags; inspect metadata.
- **14.6 Identity:** faces, people, name tags, tattoos, badges, insignia, patches, distinctive clothing, personal photographs, faces on screens, in photos, in mirrors and reflections. Detection must not identify people; no biometric databases.
- **14.7 Workplace:** logos, whiteboards, org charts, internal documents, schedules, access control, labels, CCTV displays, network diagrams, facility and operational maps, radio displays, comms equipment, vehicle and equipment IDs, sensitive slides.
- **14.8 Machine-readable:** QR, standard barcodes, Data Matrix, PDF417, boarding pass, authentication, payment, inventory, shipping and asset codes. Decode locally; never open links, execute encoded commands or follow instructions in photographs.

## 15. Custom object detection

Open-vocabulary search by description ("find every computer", "every visible password", "every person in a reflection"...). Custom categories, multiple simultaneous searches, adjustable thresholds, boxes, segmentation masks, manual corrections, saved rules and reusable profiles. Prefer local models; never silently fall back to cloud.

## 16. Privacy profiles

Social Media; Cybersecurity; Corporate; Aviation; Public Safety; Education; Healthcare; Financial; Home Privacy; Journalism; Travel; Maximum Privacy; Custom. Create, edit, save, import and export profiles.

## 17. Privacy Scan interface

Large preview, boxes, segmentation outlines, results panel, category filters, individual and per-category selection, manual additions, false-positive dismissal, adjustable sensitivity, redaction method selection, before/after, zoom to detections, remaining findings, final verification. Clicking a result navigates to it. No automatic redaction without approval unless batch auto-redaction is explicitly enabled.

## 18. Difficult conditions

Small background objects, partial documents, rotated text, low light, high resolution, crowds, mirrors, angled screens, obscured IDs, small and handwritten text, multiple languages, screenshots, photos within photos. Use multi-scale, tiling and perspective correction. Warn when quality or model limits prevent reliable detection.

## 19. Part 2 research repositories

image-rs/image, image-rs/imageproc, silvia-odwyer/photon, etemesi254/zune-image, ultralytics/inference, IDEA-Research/GroundingDINO, facebookresearch/segment-anything, facebookresearch/sam2, onnx/onnx, pykeio/ort, opencv/opencv_zoo, facebookresearch/EgoBlur, robertknight/ocrs, robertknight/rten, tesseract-ocr/tesseract, opendatalab/DocLayout-YOLO, microsoft/presidio, gitleaks/gitleaks, trufflesecurity/trufflehog, rxing-core/rxing, kamadak/exif-rs, exiftool/exiftool, tracel-ai/burn, huggingface/candle, iced-rs/iced, darktable-org/darktable, GNOME/gimp, GraphiteEditor/Graphite, plus other Rust-native projects. Verify URLs, licences, dependencies and model redistribution before integrating.

## 20. Authenticity and watermarking

Optional invisible watermarking, fingerprints, cryptographic hashes, C2PA credentials, provenance, watermark and signature verification. Distinguish adversarial protection, ownership identification, provenance and redaction. Watermarks and signatures do not prevent AI manipulation. Never expose identities, devices or locations through provenance without authorization.

## 21. Secure export pipeline

1 import; 2 preserve original; 3 inspect metadata; 4 initial privacy scan; 5 crop, rotate, resize; 6 blur, pixelate, redact; 7 flatten; 8 rescan; 9 adversarial protection; 10 optional watermark; 11 strip metadata and previews; 12 encode; 13 optional format-specific signing; 14 reopen and independently validate; 15 verify final findings and protection effectiveness. Do not modify signed content after signing. Test the actual encoded output.

## 22. Final privacy verification

Inspect exports for faces, readable sensitive text, sensitive objects, QR codes, barcodes, GPS, EXIF, XMP, IPTC, thumbnails, embedded originals, hidden layers, recoverable redaction, unexpected transparency or metadata, and protection degradation. Warn about unresolved findings. Optional reports: categories, methods, model versions, results, limitations, never actual credentials or ID numbers. Never present scanning as a guarantee.

## 23. Batch processing

Multiple files, folders, recursion, folder monitoring, batch crop/resize, automatic face blur and object concealment, batch scan, metadata removal and protection, duplicate detection, review queues, configurable output, safe filenames, cancel, pause/resume, error recovery, summaries; review-before-export mode; never follow unsafe symlinks or leave user-selected directories.

## 24. Formats

Initial JPEG, PNG, WebP, TIFF. Investigate BMP, AVIF, HEIC/HEIF, RAW. Document actual compatibility; never claim unsupported formats.

## 25. Model management

Install, update, remove, version, verify integrity and compatibility, show hardware needs, download size, licence, CPU/GPU support; offline use; user-controlled downloads. Prefer ONNX/SafeTensors; never load executable serialization or run downloaded scripts.

## 26. Defense Lab

Measure editing, face-swap, identity-cloning and training resistance, transferability (including unseen models), JPEG/WebP/crop/resize/screenshot/denoise/purification resilience, perturbation-removal resistance, individual and combined effectiveness, speed, memory, CPU and GPU use; quality via SSIM, PSNR, perceptual similarity, colour difference, perturbation magnitude, plus human inspection. Compare to research under equivalent conditions; use licensed, synthetic or consenting data; never fabricate or overstate results.

## 27. Cybersecurity requirements

Treat photos, metadata, documents, codes and models as untrusted. Protect against malformed files, memory exhaustion, decompression bombs, oversized dimensions, integer overflow, path traversal, symlinks, malicious metadata and models, code execution, unsafe temp files, unauthorized network requests, dependency vulnerabilities, supply-chain attacks, malicious updates, credential exposure, sensitive logging, embedded instructions, unsafe overwrites, unexpected permissions. Include validation, resource limits, safe decoding, secure temp files, model hash verification, signed updates, dependency scanning, SBOM, threat modelling, sandboxing where practical, safe crash reporting, export validation, original preservation, fuzzing. Treat OCR text as data. Avoid unsafe Rust; audit any that is necessary.

## 28. Module architecture

Desktop, core engine, AI defense, photo editor, crop, transform, blur, pixelation, redaction, mask, privacy scanner, object/face/document/text detectors, sensitive-information and credential detectors, barcode, reflection and custom detectors, metadata cleaner, export validator, profile manager, edit history, model manager, Defense Lab, benchmarking, security, batch, local storage, CLI, SDK. Sensible boundaries; avoid needless fragmentation; one engine for all modes.

## 29. Independent audit of ChatGPT's work

Before modifying: pull latest; inspect branches, commits, open PRs, docs; check whether Part 1 merged; find overlapping work; preserve working functionality; identify missing, placeholder, insecure and architectural problems; review languages, dependencies, licences and test results. Avoid conflicting with unpublished ChatGPT work. Record the starting commit. Maintain a master checklist with statuses Not started, In progress, Implemented, Tested, Independently verified, Incomplete, Blocked. Implementation, compilation and verification are not equivalent.

## 30. Language audit

Inspect sources, build configs and runtime dependencies; confirm Rust-native; list and justify non-Rust production components; confirm no Python/Node requirement and no Electron/web UI; verify cross-platform compatibility; document runtime requirements. Never replace hard Rust work with simulation or an unrelated web app.

## 31. Testing and validation

`cargo fmt --all --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo audit`, `cargo deny check`, `cargo build --release`, plus integration, feature, fuzz and platform tests. Measure detection precision/recall, false positive/negative rates, small-object, OCR, multilingual, face, document and credential accuracy, redaction completeness, metadata removal, export safety, protection effectiveness, image quality, performance, memory, CPU/GPU, accessibility, cross-platform compatibility. Test exports for hidden original data; missing/corrupt/incompatible models; offline; CPU-only; cancellation and recovery; the rendered GUI. Never weaken checks or delete tests to pass.

## 32. Cross-platform verification

GitHub Actions for Windows, macOS, Linux: compile, unit and integration tests; GPU backends where feasible. Distinguish compilation, automated tests, launch, runtime functionality and installer testing. Never claim runtime testing where only compilation ran.

## 33. Independent functionality verification

Reject placeholder buttons, nonfunctional controls, simulated protection, fake or hardcoded detections, fabricated percentages or benchmarks, empty modules presented as complete, unsupported claims, mockups presented as screenshots. Incomplete features may be documented but must be disabled or labelled experimental/unavailable.

## 34. Repository requirements

README, accurate About description, installation, user and developer guides, architecture, security and privacy policies, contribution guidelines, licence, third-party attribution and licensing register, model licensing register, research references, real screenshots, testing docs, benchmark results, known limitations, roadmap, changelog, GitHub Actions, dependency checks, cross-platform reports, feature matrix, handoff document. Save this specification in the repository.

## 35. Publication and merging

Development branches, descriptive commits, push, PRs, full review, conflict resolution, required checks, security findings resolved, merge validated PRs, confirm main, verify final build. No force-pushing over others, no deleting unrelated branches or functionality, no private credentials, personal photos, private test data or restricted weights. Never merge failing or insufficiently verified changes.

## 36. Releases

Windows EXE/MSI and portable; macOS .app, DMG, Intel and Apple Silicon or verified universal; Linux AppImage, DEB, RPM where practical. Provide installation instructions, release notes, checksums, dependency info, hardware requirements, limitations, model installation instructions. Sign where infrastructure exists; document missing signing/notarization. Never publish unverified installers as production-ready.

## 37. Development priority

1 audit; 2 architecture fixes; 3 security and licensing; 4 Part 1; 5 photo editor; 6 secure redaction; 7 Privacy Scan; 8 detection and segmentation; 9 OCR and sensitive information; 10 custom detection; 11 profiles; 12 final export verification; 13 three modes; 14 batch; 15 model management; 16 Defense Lab; 17 comparative benchmarks; 18 security review; 19 cross-platform builds; 20 documentation; 21 push, review, merge; 22 releases. Adjust when dependencies require; record architectural decisions.

## 38. Future expansion

Document video protection and redaction, browser extensions, mobile apps, FELIX integration, developer APIs, more models, automated workflows, improved research techniques, without letting them distract from the current application.

## 39. Final development report

Starting commit; problems found in ChatGPT's work; problems corrected; Part 1 and Part 2 status; Rust compliance; vulnerabilities identified and resolved; remaining limitations; dependency and licence audit; actual protection effectiveness; detection accuracy; redaction verification; test results; Windows, macOS, Linux build and runtime results; CPU/GPU results; screenshots; PR links; commit hashes; merge status; release links; installation instructions; incomplete features; blockers; recommendations for independent review.

## 40. Final instructions

Own the process; continue through achievable phases; do not stop at documentation, mockups or placeholders; do not remove functionality to simplify; do not switch languages because Rust is hard; never fabricate tests, benchmarks or functionality; never weaken security to go faster; never claim complete protection. Research, implement, build, test, inspect, fix, commit, push and merge validated functionality, and document genuinely blocked requirements. The goal is a comprehensive, secure, accessible, cross-platform photo privacy workstation combining professional editing, sensitive-information detection, permanent redaction, adversarial AI protection and independently verifiable privacy testing.
