#!/usr/bin/env python3
"""Optional developer validation against Hugging Face Transformers/PyTorch.

Requires torch, transformers and safetensors, separate from the Rust application.
No downloads, training, real faces, network calls or image edits are performed.
Usage: python scripts/clip_reference_check.py CHECKPOINT RUST_PROBE_JSON NEW_REPORT_JSON
"""
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import sys
os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"
import torch
from torch.nn import functional as F
from safetensors import safe_open
from transformers import CLIPVisionConfig, CLIPVisionModelWithProjection

checkpoint, rust_path, output_path = map(Path, sys.argv[1:])
expected = "99d28a652e6ec46629ab7047a0ac82c69b1fe11e0ce672c43af65d3a9a3fc05d"
with checkpoint.open("rb") as stream:
    actual = hashlib.file_digest(stream, "sha256").hexdigest()
if actual != expected:
    raise ValueError("Checkpoint does not match the pinned research artifact")
rust = json.loads(rust_path.read_text())
if rust["model_sha256"] != actual:
    raise ValueError("Rust probe used a different checkpoint")
torch.set_num_threads(4)
config = CLIPVisionConfig(hidden_size=768, intermediate_size=3072, projection_dim=512,
                         num_hidden_layers=12, num_attention_heads=12, image_size=224,
                         patch_size=32, hidden_act="quick_gelu", layer_norm_eps=1e-5)
config._attn_implementation = "eager"
model = CLIPVisionModelWithProjection(config).eval()
with safe_open(checkpoint, framework="pt", device="cpu") as source:
    available = set(source.keys())
    state = {key: source.get_tensor(key) for key in model.state_dict() if key in available}
model.load_state_dict(state, strict=True)
for parameter in model.parameters():
    parameter.requires_grad_(False)
original = torch.tensor([0.2+(i % 255)/500 for i in range(32*32*3)], dtype=torch.float32)
mean = torch.tensor([0.48145466, 0.4578275, 0.40821073]).view(1,3,1,1)
std = torch.tensor([0.26862954, 0.2613026, 0.2757771]).view(1,3,1,1)

def energy(flat):
    pixels = flat.view(32,32,3).permute(2,0,1).unsqueeze(0)
    pixels = F.interpolate(pixels, size=(224,224), mode="bilinear", align_corners=False)
    return model(pixel_values=(pixels-mean)/std).image_embeds.square().mean()

x = original.clone().requires_grad_(True)
before = energy(x)
loss = -before
loss.backward()
gradient = x.grad.detach().clone()
rust_gradient = torch.tensor(rust["input_gradient"])
gradient_error = (gradient-rust_gradient).abs().max().item()
energy_error = abs(before.item()-rust["energy_before"])
current = original.clone()
for _ in range(2):
    current = current.detach().requires_grad_(True)
    (-energy(current)).backward()
    grad = current.grad
    norm = grad.abs().mean()
    if norm > 0:
        grad = grad/norm
    grad = grad-0.05*2*(current-original)
    current = current+grad.sign()/255
    current = torch.max(torch.min(current, (original+4/255).clamp(max=1)),
                        (original-4/255).clamp(min=0))
with torch.no_grad():
    after = energy(current).item()
pixel_error = (current.detach()-torch.tensor(rust["optimized_pixels"])).abs().max().item()
passed = energy_error < 1e-5 and gradient_error < 1e-4 and abs(after-rust["energy_after_two_steps"]) < 1e-5 and pixel_error < 1e-6
report = {"schema_version":1, "checkpoint_sha256":actual,
          "reference":"Hugging Face CLIPVisionModelWithProjection, eager attention, CPU f32",
          "versions": {p:importlib.metadata.version(p) for p in ["torch","transformers","safetensors"]},
          "preprocessing":"Bilinear 32x32 to 224x224 stretch, CLIP mean/std; canonical bicubic center crop is intentionally not used",
          "energy_before_reference":before.item(), "energy_before_rust":rust["energy_before"],
          "max_absolute_input_gradient_difference":gradient_error,
          "energy_after_two_steps_reference":after,
          "energy_after_two_steps_rust":rust["energy_after_two_steps"],
          "max_absolute_optimized_pixel_difference":pixel_error,
          "passed":passed, "scope":"Architecture and gradient parity on one synthetic input only",
          "ai_efficacy":"not evaluated"}
with output_path.open("x") as stream:
    json.dump(report, stream, indent=2)
    stream.write("\n")
print(json.dumps(report, indent=2))
if not passed:
    raise SystemExit("Reference parity failed")
