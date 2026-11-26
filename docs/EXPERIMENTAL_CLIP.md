# Experimental CLIP objective

This opt-in CLI implementation suppresses the squared feature energy of a CLIP ViT-B/32 vision encoder. It is a real differentiable model objective implemented in Rust with Candle. It is not a reproduction of PhotoGuard, FaceOff, FaceShield or another audited defense and has not demonstrated downstream editing, face-swapping or training resistance.

Inputs are opaque 8-bit images of at most 1,048,576 pixels and 4096 pixels per side. Source dimensions are retained. A differentiable bilinear resize produces 224x224 RGB samples, followed by CLIP channel normalization, patch projection, positional and class embeddings, 12 transformer blocks, final layer norm and visual projection. The bilinear stretch differs from the canonical CLIP bicubic resize and center crop. This distinction must be included in evaluations.

The optional `clip` feature is not built into the default CLI or desktop. No weights are redistributed. Before using a checkpoint, independently review its publisher, license, exact artifact hash and conversion provenance. A license manifest is an audit record, not legal permission granted by this app.

```sh
cargo build --locked -p photo-cli --features experimental-clip
photo-defense verify-model /path/to/model.safetensors /path/to/reviewed-manifest.json
photo-defense experimental-clip photo.png /path/to/model.safetensors /path/to/reviewed-manifest.json --output experiment.png --epsilon 4 --steps 10
```

Manifest schema 2 separates `local_use_approved` (required to load the weights locally) from `redistribution_approved` and `commercial_use_permitted` (recorded for future installers; not needed locally). Do not set it true merely because an artifact is downloadable. The shipped [manifest example](research/model-manifest.example.json) is deliberately not approved and cannot be used to claim successful verification.

Output is PNG with a JSON sidecar. Quantized samples are checked against the requested integer pixel budget. Reports include model and image hashes, loss trace, feature energy before and after, pixel quality and `ai_efficacy: not evaluated`. A model loss improvement is not a protection score. Runtime and memory requirements can be substantial on CPU. Cancellation is exposed through the engine SDK but not yet wired to CLI Ctrl+C; interrupting before publication produces no final image.

## Reproducibility smoke test

The local research session downloaded the safetensors artifact from `sentence-transformers/clip-ViT-B-32`, path `0_CLIPModel/model.safetensors`. Its SHA-256 matched the publisher's Hugging Face LFS metadata:

`99d28a652e6ec46629ab7047a0ac82c69b1fe11e0ce672c43af65d3a9a3fc05d`

Size: 605157884 bytes. This conversion artifact is not approved for bundling by this repository. Code license references do not settle every conversion or training-data redistribution question. The test used a synthetic 32x32 RGB array defined in `clip_probe.rs`, CPU f32, epsilon 4/255, step 1/255 and two steps. Observed feature energy was 0.31836617 before and 0.2386863 after, with a nonzero input gradient. This checks model loading, autodiff and objective behavior only. It is not an efficacy benchmark or a speed claim. Numerical values may differ slightly across hardware or dependency versions.

The manual probe accepts a local file after the operator independently verifies its hash. It performs no download, writes no image, and does not run an editor. The small test architecture uses deterministic synthetic weights solely to compare analytical gradients with finite differences. Neither test fixture is offered as a usable defense.

Before enabling a desktop protection preset: verify inference parity with a primary implementation, check gradients on the full pretrained model, benchmark quantized outputs against real supported editor/identity systems, test purification and holdout models, establish licensing and publish uncertainty. A CLIP-only surrogate may transfer poorly or fail entirely.

## Primary-framework comparison

A second check used the same pinned checkpoint in Hugging Face Transformers CLIPVisionModelWithProjection with eager attention and CPU f32, using the identical bilinear preprocessing and objective. The Rust implementation and PyTorch reference agreed on this synthetic input: maximum absolute input-gradient difference 0.00000262633 and maximum optimized-pixel difference 0.0000000596046. The full machine-readable result and exact package versions are in [clip-reference-parity.json](validation/clip-reference-parity.json). This checks one input and one objective; it does not validate downstream protection or every numerical/backend path.

To reproduce, install the optional reference-only Python tools in an isolated environment. The observed versions were torch 2.15.0+cpu, transformers 5.24.1 and safetensors 0.8.0. These are developer validation dependencies and are not required by the Rust desktop or CLI. The original checkpoint must be prepared manually and match its pinned hash. Neither script downloads models or uploads inputs.

```sh
cargo run --locked -p photo-engine --features clip --example clip_probe -- /path/to/model.safetensors /path/to/new-rust-probe.json
python scripts/clip_reference_check.py /path/to/model.safetensors /path/to/new-rust-probe.json /path/to/new-reference-report.json
```

The probe refuses a checkpoint with a different size/hash and refuses to overwrite its report. Its synthetic weights test remains distinct from this pretrained-checkpoint comparison. Expand parity tests to varied resolutions and photographs before using this adapter as a supported defense backend.
