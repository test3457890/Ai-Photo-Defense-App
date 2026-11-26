# Format and codec compatibility (Part 4, version 0.4.0)

What this build can actually do with each format. A format is **not supported** unless a column
says otherwise. Every file is identified from its content (`photo_media::detect`, magic bytes and
container structure), never from its extension; a mismatched extension is reported and the real
format is used.

Legend: **Yes** = implemented and tested on Windows 10; **Detect** = recognised and named, nothing
else; **No** = not available. "Protection" means adversarial AI protection, which is **No** for
every format: no validated image, voice or video protection model is installed, and the engines
refuse to run without one (see [guides/AUDIO_PROTECTION.md](guides/AUDIO_PROTECTION.md)).

GPU: no format uses a GPU path. All work is CPU-only.

## Images

| Format | Import / preview | Privacy Scan | Metadata removal on export | Export | Notes |
|---|---|---|---|---|---|
| JPEG, PNG, WebP, TIFF | Yes | Yes | Yes (re-encode) | JPEG, PNG, WebP, TIFF | Parts 1 to 3 |
| BMP | Yes (new in 0.4) | Yes | Yes (re-encode) | as above | trailing data after the pixel array is reported |
| GIF | Yes, first frame (new in 0.4) | Yes | Yes (re-encode, first frame) | as above | animated GIFs are flagged (HiddenLayers) because export keeps only the first frame; comment and XMP extension blocks are reported |
| Camera RAW (DNG, CR2, CR3, NEF, ARW, RAF, ORF, RW2, PEF, ...) | Embedded preview only | Yes | Yes | as above | no RAW development (Part 3) |
| APNG, animated WebP | Detect | No | No | No | animation is not decoded |
| AVIF, HEIC / HEIF | Detect | No | No | No | needs a licence-reviewed decoder (HEVC patents for HEIC) |
| JPEG XL, JPEG 2000, OpenEXR, PSD, XCF, SVG | Detect | No | No | No | not implemented |

Converting a layered or vector source into a protected raster export never protects the editable
original; the original file is never modified.

## Video

Video **decoding, frame preview, re-encoding and frame-level protection are not implemented.**
This build works on the container only.

| Container | Detect | Structure inspection (tracks, codecs, size, fps, duration, language) | Privacy metadata inspection | Metadata removal | Notes |
|---|---|---|---|---|---|
| MP4, M4V, MOV, 3GP, M4A | Yes | Yes | Yes: GPS (`©xyz`, ISO 6709 keys, `loci`), make/model, software, dates, text tags, cover art, XMP `uuid`, timed-metadata tracks, creation times | **Yes, lossless (no re-encode)** | drops `udta`/`meta`/`uuid`/XMP and timed-metadata tracks, zeroes creation/modification times, rewrites `stco`/`co64` chunk offsets and re-verifies track parameters and the first 64 bytes of every chunk. Fragmented MP4 is refused |
| Matroska (MKV), WebM | Yes | Yes | Yes: title, date, writing/muxing application, tags, attachments, chapters | No | inspection only; removal is not implemented and the app says so |
| AVI, MPEG-PS (MPG/MPEG), MPEG-TS, M2TS / MTS, MXF, FLV, ASF / WMV, Ogg (Theora) | Detect | No | No | No | named only |

Codecs are identified from the sample description (`stsd`) or Matroska `CodecID`: H.264/AVC,
H.265/HEVC, AV1, VP8, VP9, MPEG-4 Part 2, MPEG-2, ProRes variants, DNxHD/HR, Motion JPEG, FFV1,
Theora, AAC, Opus, MP3, FLAC, ALAC, AC-3, E-AC-3, PCM. **Identification only**: none of them is
decoded or encoded by this build.

Resolution classes (SD to 8K), vertical/square video, VFR, HDR and bit depth are reported where
the container states them; nothing is processed.

## Audio

Decoding uses Symphonia 0.5 (pure Rust, MPL-2.0, used unmodified as a library). Decoding is
streamed, so long files do not need to fit in memory.

| Format | Detect | Decode, waveform, spectrogram, loudness | Metadata inspection | Clean export (mute / bleep redaction) | Notes |
|---|---|---|---|---|---|
| WAV (PCM 8/16/24/32-bit, float), BWF | Yes | Yes | Yes (RIFF INFO tags exposed by Symphonia; BWF originator fields flagged) | Yes, to WAV | |
| FLAC | Yes | Yes | Yes (Vorbis comments, pictures) | Yes, to WAV | |
| MP3 | Yes | Yes | Yes (ID3) | Yes, to WAV | |
| AAC (ADTS), M4A / MP4 audio | Yes | Yes | Yes | Yes, to WAV | |
| ALAC (in M4A) | Yes | Yes | Yes | Yes, to WAV | |
| Ogg Vorbis | Yes | Yes | Yes | Yes, to WAV | |
| W64, CAF | Detect | No | No | No | |
| AIFF / AIF | Detect | **No** | No | No | Symphonia 0.5 has no AIFF demuxer |
| Opus | Detect | **No** | No | No | no Opus decoder in Symphonia 0.5 |
| WMA, AC-3, E-AC-3, AMR | Detect | No | No | No | |

Export writes a new WAV: 16-bit PCM when the source is 16-bit integer, otherwise 32-bit float.
Channels and sample rate are preserved; stereo is never folded to mono. No encoded (MP3, AAC,
FLAC, Opus) export exists yet. The export is reopened and checked: frame count, channel count,
sample rate, and that every sample outside the redacted ranges is unchanged.

Limits: sample rate 1 Hz to 768 kHz, at most 32 channels, at most 12 hours. Panics inside the
third-party decoder are contained (see [SECURITY_REVIEW.md](SECURITY_REVIEW.md)).

## Subtitles

| Format | Detect | Parse | Privacy Scan | Redacted export |
|---|---|---|---|---|
| SubRip (SRT) | Yes | Yes | Yes (same text rules as photographs: e-mail, phone, payment cards with checksum, financial accounts, national IDs and passports for US/GB/CA, passwords, API keys, private keys, Wi-Fi credentials, IP/MAC addresses, URLs, dates of birth) | Yes (flagged cues replaced by `[redacted]`, timing kept) |
| WebVTT | Yes | Yes | Yes | Yes |
| Subtitle tracks inside MP4/MKV | Listed as tracks | No | No | Removed with metadata only if they are timed-metadata tracks |

Findings never contain the matched text; they record category, cue number and time.

## How to check a file

```
photo-defense media-inspect FILE       # format, tracks, codecs, privacy metadata (JSON)
photo-defense audio-analyze FILE       # loudness, true peak, clipping, silence
photo-defense subtitles FILE           # subtitle Privacy Scan
```

In the desktop app: Technical mode, **Format Inspection**.
