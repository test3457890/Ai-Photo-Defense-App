# Model licensing register

No model weights are bundled, downloaded or loaded by default. Every row below is a *candidate* that must pass the gate before installation support is enabled. "Unverified" means the upstream terms have not yet been reviewed against redistribution and commercial use; it is not a claim about what the terms say.

| Purpose | Candidate | Format needed | Code licence | Weights licence | Redistribution | Status |
|---|---|---|---|---|---|---|
| Face detection | OpenCV Zoo YuNet | ONNX | Unverified | Unverified | Unverified | Not integrated |
| Face/plate concealment | EgoBlur (Meta) | TorchScript upstream; would need conversion | Unverified | Unverified | Unverified | Not integrated |
| OCR | ocrs detection + recognition (rten) | .rten | Unverified | Unverified | Unverified | Not integrated |
| Open-vocabulary detection | Grounding DINO | ONNX conversion | Unverified | Unverified | Unverified | Not integrated |
| Segmentation | SAM / SAM 2 | ONNX conversion | Unverified | Unverified | Unverified | Not integrated |
| Document layout | DocLayout-YOLO | ONNX conversion | Unverified | Unverified | Unverified | Not integrated |
| Experimental adversarial objective | CLIP ViT-B/32 vision tower | SafeTensors | Independent Rust implementation (MIT) | Per checkpoint | Per checkpoint | Opt-in CLI feature; see EXPERIMENTAL_CLIP.md |

## Gate before any model is enabled

1. Record official source URL, exact file SHA-256 and size in a manifest reviewed independently of the distributor.
2. Record code licence and weights licence separately, plus redistribution and commercial-use terms. The current manifest field `redistribution_approved` conflates local use and redistribution; split it before adding a downloader.
3. Accept only SafeTensors, ONNX or RTen. Never pickle, TorchScript or anything that executes code on load. Reject bundled scripts.
4. Run the detector against a licensed or synthetic labelled set and publish precision/recall with the exact dataset manifest. Until then the detector is reported to users as uncertain.
5. Keep a CPU path. GPU execution providers are optional.
