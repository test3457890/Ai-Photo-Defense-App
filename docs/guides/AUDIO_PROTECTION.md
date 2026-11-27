# Audio protection guide

## What works today

| Capability | Status |
|---|---|
| Decode and analyse WAV, FLAC, MP3, AAC/M4A, ALAC, Ogg Vorbis | Working (streamed, CPU) |
| Loudness (EBU R128 integrated, LRA), true peak, sample peak, clipping, DC offset, silence | Working |
| Waveform and spectrogram | Working |
| Metadata inspection (ID3, Vorbis comments, MP4 tags, RIFF INFO, embedded pictures) | Working |
| Clean WAV export without any source metadata | Working, verified on reopen |
| Mute / bleep redaction of time ranges | Working, verified (samples outside ranges unchanged) |
| Original vs export comparison (SNR, segmental SNR, length, max difference) | Working |
| Loudness-matched A/B listening | Working (first 2 minutes, both at -23 LUFS) |
| Automatic detection of spoken names, numbers, addresses | **Not available**: needs a local speech-to-text model |
| Adversarial voice-cloning protection | **Not available**: engine present, no validated model; refuses to run |
| Encoded export (MP3, AAC, FLAC, Opus) | Not available; WAV only |
| Compression-resilience testing | Not available (needs encoders and a protection model) |

## The protection engine and why it refuses to run

`photo_media::protect` is a projected-gradient optimiser that adds a perturbation in the
short-time Fourier domain (sqrt-Hann window, 1024-sample frames, 512 hop) and keeps it below a
simple psychoacoustic masking threshold: per-bin signal energy spread over neighbouring bins,
lowered by a configurable offset (dB), and taken as the minimum over neighbouring frames so the
perturbation cannot ride on short transients. After every step the perturbation is projected back
under the threshold. It also enforces an SNR floor and never clips. Profiles: Original Quality,
Balanced / Voice, Maximum, Music.

What it optimises is supplied by an `AudioObjective`: something that can score how strongly a
recording identifies its speaker (for example the distance between speaker embeddings) and return
a gradient. Published methods (AntiFake, VoiceCloak, SafeSpeech and others) use neural speaker
encoders and synthesis models for this. **None is installed**, because none has yet passed the
licence and redistribution review (see
[../research/MULTIMEDIA_RESEARCH_REGISTER.md](../research/MULTIMEDIA_RESEARCH_REGISTER.md)).

Without an objective, `protect` returns an error and the file is unchanged:

```
$ photo-defense audio-protect talk.wav
Error: No validated voice-protection model is installed. The audio protection engine refuses to
run without one; no file was changed.
```

Adding random noise would be easy and would be dishonest: noise without an objective gives no
measurable resistance, and published work (De-AntiFake, ICML 2025) shows that purification removes
much of even optimised perturbations. Any future resistance claim must therefore be measured
against real cloning systems **and** after purification, and reported with the method.

The engine's mechanics are unit-tested with a fixture objective: the perturbation stays under the
masking threshold (per-frame energy ratio below 1.5), respects the SNR floor and never clips. These
tests show that the constraint machinery works; they say nothing about cloning resistance.

## Quality controls

The comparison panel reports frame count, length difference (must be 0), SNR and segmental SNR.
When ranges are muted or bleeped the overall SNR is low by design; segmental SNR reflects the
untouched audio. Signal metrics do not measure resistance to cloning and are labelled so.

## Command line

```
photo-defense audio-analyze in.wav
photo-defense audio-export in.mp3 -o clean.wav --mute 12.5-14 --bleep 30-31.2
photo-defense audio-compare in.mp3 clean.wav
photo-defense audio-protect in.wav        # refuses without a validated model
```
