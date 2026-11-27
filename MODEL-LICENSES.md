# Model Licenses & Redistribution Status

**Date:** 2026-11-27  
**Detailed Register:** See [docs/research/MODEL_LICENSES.md](docs/research/MODEL_LICENSES.md).

No model weights are bundled or auto-downloaded by AI Photo Defense. Every model requires a schema-2 manifest (`local_use_approved`, `redistribution_approved`, `commercial_use_permitted`, SHA-256, HTTPS source URL).

| Model / Family | Purpose | Format | License Status | Integration Status |
|---|---|---|---|---|
| **UltraFace RFB-320** (`Linzaer`) | Face detection | `.onnx` | MIT (code & weights) | Integrated (`faces` feature, user-supplied `.onnx`) |
| **CLIP ViT-B/32** (`openai/clip-vit-base-patch32`) | Feature / cosine divergence | `.safetensors` | Independent Rust code (MIT); weights per checkpoint | Integrated (`experimental-clip` / `clip` feature, user-supplied) |
| **Whisper** (`tiny` / `base`) | Local speech-to-text | `.safetensors` | MIT / Apache-2.0 | Integrated (`stt` feature, user-supplied) |
| **SpeechBrain ECAPA-TDNN** | Experimental speaker embedding | `.safetensors` | Apache-2.0 | Integrated (`voice-protect`, proven ineffective across models) |
| **Ollama Local LLMs** (`qwen2.5:3b`, `phi3.5:mini`, `mistral:7b-instruct`, `llama3.2:3b`) | Optional local advisor | GGUF (via localhost Ollama) | Apache-2.0 / MIT / Llama-3.2-Community | Integrated (`ask-ai` feature, disabled by default) |
| **Glaze / Nightshade** | Anti-training cloaking | Closed source | Closed / non-redistributable | **Not integrated** (incompatible license) |
| **FaceShield / SimSwap / DeepFaceLab** | Deepfake / face-swap | Various | Research-only / non-commercial / unverified | **Not integrated** |
| **ArcFace / FaceNet / RetinaFace / SD / FLUX** | Recognition & diffusion validation | `.onnx` / `.safetensors` | Requires per-checkpoint review (e.g. InsightFace non-commercial weights vs Apache-2.0/MIT alternatives) | **Not integrated** (marked `Not tested` / `Unsupported` in Validation Suite) |
