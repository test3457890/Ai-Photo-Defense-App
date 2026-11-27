//! Offline converter for `speechbrain/spkrec-ecapa-voxceleb` (Apache-2.0) to SafeTensors
//! plus a schema-2 `photo-models` manifest and parity reference embeddings.
//!
//! Usage (run once offline in an environment with torch, speechbrain, and safetensors):
//!   python tools/convert_ecapa.py --src /path/to/spkrec-ecapa-voxceleb --out /path/to/ecapa_out
//!
//! Never shipped or invoked by the Rust application.

import argparse
import hashlib
import json
import math
from pathlib import Path

import torch
from safetensors.torch import save_file


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def synthetic_waveforms(sample_rate: int = 16000, seconds: float = 1.5) -> dict[str, torch.Tensor]:
    n = int(sample_rate * seconds)
    t = torch.arange(n, dtype=torch.float32) / sample_rate
    return {
        "sine_220_440": 0.3 * torch.sin(2 * math.pi * 220 * t) + 0.2 * torch.sin(2 * math.pi * 440 * t),
        "chirp_150_600": 0.35 * torch.sin(2 * math.pi * (150 + 300 * t / seconds) * t),
        "harmonic_pulse": sum((0.25 / k) * torch.sin(2 * math.pi * 130 * k * t) for k in range(1, 6)),
    }


def main() -> None:
    ap = argparse.ArgumentParser(description="Convert SpeechBrain ECAPA-TDNN to SafeTensors")
    ap.add_argument("--src", required=True, type=Path, help="Local folder with embedding_model.ckpt")
    ap.add_argument("--out", required=True, type=Path, help="Output directory")
    args = ap.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    ckpt_path = args.src / "embedding_model.ckpt"
    if not ckpt_path.is_file():
        raise FileNotFoundError(f"Missing {ckpt_path}")

    state = torch.load(ckpt_path, map_location="cpu", weights_only=True)
    tensors: dict[str, torch.Tensor] = {}
    for k, v in state.items():
        if isinstance(v, torch.Tensor):
            tensors[k] = v.contiguous().to(torch.float32)

    norm_path = args.src / "mean_var_norm_emb.ckpt"
    if norm_path.is_file():
        norm_state = torch.load(norm_path, map_location="cpu", weights_only=True)
        if "glob_mean" in norm_state and isinstance(norm_state["glob_mean"], torch.Tensor):
            tensors["emb_norm.glob_mean"] = norm_state["glob_mean"].contiguous().to(torch.float32)

    out_weights = args.out / "model.safetensors"
    save_file(tensors, str(out_weights))

    manifest = {
        "schema_version": 2,
        "id": "speechbrain-ecapa-voxceleb",
        "filename": "model.safetensors",
        "sha256": sha256_file(out_weights),
        "size_bytes": out_weights.stat().st_size,
        "source_url": "https://huggingface.co/speechbrain/spkrec-ecapa-voxceleb",
        "code_license": "Apache-2.0",
        "weights_license": "Apache-2.0",
        "local_use_approved": True,
        "redistribution_approved": True,
        "commercial_use_permitted": True,
    }
    (args.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

    try:
        from speechbrain.inference.speaker import EncoderClassifier

        classifier = EncoderClassifier.from_hparams(source=str(args.src), run_opts={"device": "cpu"})
        parity = {}
        with torch.no_grad():
            for name, wav in synthetic_waveforms().items():
                emb = classifier.encode_batch(wav.unsqueeze(0)).squeeze().cpu().tolist()
                parity[name] = emb
        (args.out / "parity_refs.json").write_text(json.dumps(parity, indent=2) + "\n", encoding="utf-8")
        print(f"Wrote {out_weights}, manifest.json, and parity_refs.json")
    except ImportError:
        print(f"Wrote {out_weights} and manifest.json (speechbrain not installed; skipped parity_refs.json)")


if __name__ == "__main__":
    main()
