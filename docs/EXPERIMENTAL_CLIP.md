# Experimental CLIP objective

This opt-in CLI implementation suppresses the squared feature energy of a CLIP ViT-B/32 vision encoder. It is a real differentiable model objective implemented in Rust with Candle. It is not a reproduction of PhotoGuard, FaceOff, FaceShield or another audited defense and has not demonstrated downstream editing, face-swapping or training resistance.

Inputs are opaque 8-bit images of at most 1,048,576 pixels and 4096 pixels per side. Source dimensions are retained. A differentiable bilinear resize produces 224x224 RGB samples, followed by CLIP channel normalization, patch projection, positional and class embeddings, 12 transformer blocks, final layer norm and visual projection. The bilinear stretch differs from the canonical CLIP bicubic resize and center crop. This distinction must be included in evaluations.

The optional `clip` feature is not built into the default CLI or desktop. No weights are redistributed. Before using a checkpoint, independently review its publisher, license, exact artifact hash and conversion provenance. A license manifest is an audit record, not legal permission granted by this app.

```sh
cargo build --locked -p photo-cli --features experimental-clip
photo-defense verify-model /path/to/model.safetensors /path/to/reviewed-manifest.json
photo-defense experimental-clip photo.png /path/to/model.safetensors /path/to/reviewed-manifest.json --output experiment.png --epsilon 4 --steps 10
```

`redistribution_approved` is currently a conservative gate required for use of the manifest. It does not cause redistribution. A future catalog must distinguish local-use approval from redistribution rights. Do not set it true merely because an artifact is downloadable. The shipped [manifest example](research/model-manifest.example.json) is deliberately not approved and cannot be used to claim successful verification.

Output is PNG with a JSON sidecar. Quantized samples are checked against the requested integer pixel budget. Reports include model and image hashes, loss trace, feature energy before and after, pixel quality and `ai_efficacy: not evaluated`. A model loss improvement is not a protection score. Runtime and memory requirements can be substantial on CPU. Cancellation is exposed through the engine SDK but not yet wired to CLI Ctrl+C; interrupting before publication produces no final image.

## Reproducibility smoke test

The local research session downloaded the safetensors artifact from `sentence-transformers/clip-ViT-B-32`, path `0_CLIPModel/model.safetensors`. Its SHA-256 matched the publisher's Hugging Face LFS metadata:

`99d28a652e6ec46629ab7047a0ac82c69b1fe11e0ce672c43af65d3a9a3fc05d`

Size: 605157884 bytes. This conversion artifact is not approved for bundling by this repository. Code license references do not settle every conversion or training-data redistribution question. The test used a synthetic 32x32 RGB array defined in `clip_probe.rs`, CPU f32, epsilon 4/255, step 1/255 and two steps. Observed feature energy was 0.31836617 before and 0.2386863 after, with a nonzero input gradient. This checks model loading, autodiff and objective behavior only. It is not an efficacy benchmark or a speed claim. Numerical values may differ slightly across hardware or dependency versions.

The manual probe accepts a local file after the operator independently verifies its hash. It performs no download, writes no image, and does not run an editor. The small test architecture uses deterministic synthetic weights solely to compare analytical gradients with finite differences. Neither test fixture is offered as a usable defense.

Before enabling a desktop protection preset: verify inference parity with a primary implementation, check gradients on the full pretrained model, benchmark quantized outputs against real supported editor/identity systems, test purification and holdout models, establish licensing and publish uncertainty. A CLIP-only surrogate may transfer poorly or fail entirely.
