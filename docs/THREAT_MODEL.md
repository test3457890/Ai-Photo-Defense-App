# Threat model

Assets: original photographs, exported copies, location metadata, source/weight integrity, signing keys if later added, local resource availability and truthful efficacy reports.

Adversaries: malicious image providers, malicious model distributors, compromised dependency publishers and people attempting to remove perturbations or edit/train on publicly shared images. A compromised OS or another process with the same user privileges is outside the present boundary. Output directories are user-selected and trusted; directory replacement by a local same-user adversary is not prevented by path-based publication.

| Entry point | Implemented control | Residual risk / next control |
|---|---|---|
| Image files | One bounded regular-file snapshot; supported formats only; dimension, pixel and decoder allocation limits | Codec CPU time is not a hard limit; move decoding to constrained helper processes and fuzz codecs |
| Compressed images | 64 MiB file, 24 MP total pixels, 16384 per side, decoder allocation limit | Codec scratch memory can exceed a nominal allocation limit; measure peak process RSS, not only buffer size |
| High bit depth / alpha | Reject unsupported bit depths; JPEG refuses nonopaque input | ICC color conversion, animation awareness and multi-page selection pending |
| Export paths | Sanitized derived stems; private same-directory tempfiles; no-clobber publication | Trusted parent-directory assumption; image/report are not one atomic transaction |
| Models | Bounded safetensors only, expected size, SHA-256, tensor structure, license gate; no Python execution | A manifest can be forged; verify publisher independently. Tensor schema/shape constraints belong to each adapter. Imported tensors may exhaust memory or contain NaNs. |
| Dependencies | Cargo.lock, RustSec audit, planned CI and SBOM | Dependency compromise and unmaintained transitive libraries remain release risks |
| Application network | No built-in network client or uploads | OS/cloud sync/backups are outside the app's control |
| Shared protected image | No universal protection claim; objective and output measurements separated | Compression, screenshots, crops, denoising, adaptive purification and unknown models can defeat protection |
| Future provenance | No fake watermark or C2PA indicators | Keys, certificates, trust lists, timestamping, revocation and metadata privacy must be designed before signing |
| Future updates | No updater | Signed update verification, rollback resistance and publisher code signing required before adding one |

The engine rejects missing objectives, invalid numerical settings, mismatched gradients, nonfinite model outputs and cancellation. Gradient projection bounds samples but cannot prove imperceptibility. Models should be loaded from a verified immutable snapshot; the experimental CLI rechecks the exact second read before constructing its adapter.

No retaliatory network actions, tracking pixels or covert identification of people who download photographs are part of the project.
