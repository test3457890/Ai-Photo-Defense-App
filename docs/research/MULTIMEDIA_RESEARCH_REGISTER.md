# Multimedia research register (Part 4)

Status of every project named in [PART4_SPECIFICATION.md](../PART4_SPECIFICATION.md) sections 10
to 13. **Verified** means the facts in that row were read from the project's own page on
2026-11-26 (README, licence field, project site). **Not verified** means nothing was checked in
this increment: the row records only what the specification asks us to investigate, and no
technical or licence claim is made. Nothing here is integrated unless the "Used" column says so.

No research result is assumed to generalise to commercial cloning or editing systems. No
effectiveness number from any paper is reported as a property of this application.

## Multimedia engineering libraries

| Project | Licence | What it is | Used | Decision |
|---|---|---|---|---|
| Symphonia (pdeljanov/Symphonia) | MPL-2.0 (verified via crate metadata, cargo-deny) | Pure-Rust audio demuxers and decoders | **Yes**, unmodified library, features `aac, alac, flac, isomp4, mkv, mp3, ogg, pcm, vorbis, wav` | Audio decode path. MPL-2.0 is file-level copyleft; we do not modify its files |
| rodio + cpal (RustAudio) | rodio MIT OR Apache-2.0, cpal Apache-2.0 (crate metadata, cargo-deny) | Audio output | **Yes**, rodio 0.19 with default features off (no decoders), local A/B preview only | Needs ALSA headers on Linux builds |
| hound | Apache-2.0 | WAV writer | **Yes** | Export container |
| ebur128 | MIT | EBU R128 loudness, true peak, LRA | **Yes** | Loudness matching and analysis |
| rustfft | MIT OR Apache-2.0 | FFT | **Yes** | Spectrogram and protection engine STFT |
| FilmCraft (storytold/filmcraft) | MIT OR Apache-2.0 (**verified**, README badge and licence files) | Open-source non-linear video editor in pure Rust. README states its video codecs, AAC, Opus and containers are its own Rust code (FFmpeg only as a test oracle). Codec table: H.264 decode and encode (Baseline/Main/High); HEVC decode (Main, Main 10), HEVC export only through hardware encoders; AV1 decode only | No | **Leading candidate for Part 4C video decode/encode** because it is Rust-native and permissively licensed. Not integrated: its codecs must first be reviewed for security (untrusted bitstreams), correctness, performance and patent exposure (H.264/HEVC are patent-encumbered regardless of code licence). Its editor, timeline and UI must not be imported (spec section 10) |
| FFmpeg | LGPL-2.1+ or GPL depending on build | C media framework | No | Would add a non-Rust runtime dependency and licence/patent obligations; kept as a possible external test oracle only |
| gstreamer-rs | MIT/Apache bindings over LGPL GStreamer | Bindings | No | Same concern as FFmpeg: large native runtime |
| openh264-rs | BSD-2 bindings; Cisco OpenH264 binary licence | H.264 | No (not verified) | Cisco's binary-download patent arrangement needs review |
| rav1e (xiph) | BSD-2 (not verified in this pass) | AV1 encoder in Rust | No | Candidate encoder for protected AV1 output |
| wgpu | MIT OR Apache-2.0 (not verified in this pass) | GPU API | No | GPU work is out of scope until a validated model exists |
| avio, lossless-cut, MediaInfoLib | Not verified | Media I/O, lossless cutting UI, media inspector | No | MediaInfoLib (BSD-2 per its repo, not verified) is a reference for the inspection fields |

## Voice protection research (spec section 12)

| Project | Verified facts | Used |
|---|---|---|
| **AntiFake** (WUSTL-CSPL/AntiFake), ACM CCS 2023 | **Verified:** Python 3.7, PyTorch 1.13; evaluated against SV2TTS (Real-Time Voice Cloning), Adaptive Voice Conversion, Coqui TTS and Tortoise TTS; ensemble of speaker encoders; model weights are not in the repository (external download, no redistribution terms stated); about 16 GB memory recommended. **No licence is stated on the repository page**, so its code cannot be reused | No. The Rust engine follows the same general idea (bounded perturbation that moves speaker embeddings) but contains no AntiFake code |
| **De-AntiFake**, ICML 2025 (arXiv 2507.02606) | **Verified:** reports that purification (their two-stage "PhonePuRe" method) neutralises a large part of protective perturbations; the hardest case they report is AntiFake, where their attack success rises from 45.1% to 76.2%. Code is linked from the project page; licence not checked | Used as a design constraint: any future audio protection must be evaluated **after** purification, and no resistance claim is made without that test |
| Voice Guard, Enkidu, Good Noise, SceneGuard, VoiceCloak, SafeSpeech (USENIX Security 2025), E2E-VGuard, CloneShield (arXiv 2505.19119) | Not verified | No |

## Image and video immunisation research (spec section 13)

| Project | Verified facts | Used |
|---|---|---|
| **PhotoGuard** (MadryLab/photoguard), arXiv 2302.06588 | **Verified:** MIT licence; targets Stable Diffusion img2img and inpainting through an encoder attack (PGD on the VAE encoder) and an end-to-end diffusion attack | The Part 1 projected-gradient SDK implements the same optimisation family; no PhotoGuard code is included and no Stable Diffusion model is shipped |
| DiffVax, TC-UAP, Anti-I2V, FaceShield, DiffusionGuard, FaceOff, FaceLock, SGD-Guard, Diffusion Immunization, Fawkes, DCT-Shield, BlurGuard, VETO, Glaze, Nightshade | Not verified in this increment | No |

## What this means for Part 4 (honest status)

- **Audio protection:** a psychoacoustically bounded projected-gradient engine exists in Rust
  (`photo_media::protect`). It needs a differentiable speaker-embedding or synthesis objective. None
  is installed, because every candidate model found so far either has no stated licence, unclear
  weight redistribution terms, or has not been reviewed. Without one the engine refuses to run.
- **Video protection:** temporal-consistency metrics exist (`photo_media::temporal`) so a future
  per-frame method (TC-UAP style) can be measured for flicker, but no frames are decoded yet.
- **Next verification work:** read the licences of the unverified rows, obtain a speaker encoder
  whose code and weights may be redistributed, and reproduce one published result (including a
  De-AntiFake-style purification test) before any resistance claim.

## Licence check of the remaining projects (2026-11-26)

Read from each project's official page, repository, model card or paper by the Google Antigravity agent on the owner's machine, at Claude's request, and reviewed by Claude. Only facts stated on those pages are kept; Claude did not re-open every page. "-" means not stated.

| Project | Code licence | Weights | Notes relevant to this application |
|---|---|---|---|
| Voice Guard (IJCAI 2023) | - | - | Built on AttackVC; cannot be reused without a licence |
| Enkidu (ACM MM 2025) | - | uses SpeechBrain ECAPA | PyTorch 2.4; real-time claim |
| Good Noise | - (core optimiser is a closed API) | SpeechBrain, Whisper | about 37 MB GPU memory per audio second |
| SceneGuard | MIT | user-downloaded upstream models | adds audible scene noise (SNR 10 to 20 dB) |
| VoiceCloak (2025) | - | - | demo page only |
| SafeSpeech (USENIX Security 2025) | MIT | Zenodo 14736906 | paper reports DEMUCS denoising and MP3 compression partly undo it; needs a GPU |
| E2E-VGuard (arXiv 2511.07099) | - | - | |
| CloneShield (arXiv 2505.19119) | - | - | no repository linked |
| De-AntiFake | none on GitHub | MIT (Hugging Face model card) | purification attack: the evaluation tool for any future voice protection |
| DiffVax (ICLR 2026) | MIT | - | |
| TC-UAP (NeurIPS 2026) | starting kit Apache-2.0; full code not linked | - | temporally consistent video protection |
| Anti-I2V (CVPR 2026) | code "coming soon" | - | |
| FaceShield (ICCV 2025) | KUAICV research-only, non-commercial | research-only | **cannot be shipped** in a product for real users |
| DiffusionGuard (ICLR 2025) | - | uses Stable Diffusion inpainting | |
| Fawkes (USENIX Security 2020) | BSD-3-Clause ("personal privacy or academic research") | - | TensorFlow |
| Glaze, Nightshade | closed source | not distributed | cannot be integrated |
| **SpeechBrain ECAPA-TDNN** (spkrec-ecapa-voxceleb) | **Apache-2.0** | **Apache-2.0** (Hugging Face) | **Implemented behind `voice-protect`** (`crates/media/src/ecapa.rs`, offline converter `tools/convert_ecapa.py`); parity and external cloner transfer remain unverified until run against converted weights |
| WavLM base-plus-sv | CC-BY-SA-3.0 | CC-BY-SA-3.0 | share-alike obligations |
| Resemblyzer (GE2E) | Apache-2.0 | Apache-2.0 (in repo) | second licence-compatible speaker encoder |
| **OpenAI Whisper** | **MIT** | **MIT** | **licence-compatible speech-to-text** for detecting spoken names and numbers; candle (already a dependency) has a Rust Whisper implementation |
| rav1e | BSD-2-Clause | - | AV1 encoder |
| openh264-rs | BSD-2-Clause | - | wraps Cisco OpenH264; H.264 patents apply |
| MediaInfoLib | BSD-2-Clause | - | C++ |
| avio | MIT OR Apache-2.0 | - | wraps FFmpeg (LGPL/GPL inherited) |
| Symphonia AIFF | MPL-2.0 | - | `aiff` feature since 0.5.4: **now enabled** |

Consequence: voice-cloning protection and spoken-content detection are no longer blocked by licensing. They are blocked by implementation and validation work: porting ECAPA-TDNN (or GE2E) and Whisper to candle, converting weights, and measuring against real cloning systems with a De-AntiFake purification test. Antigravity's raw notes are not committed.

