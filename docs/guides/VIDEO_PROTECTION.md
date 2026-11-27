# Video protection guide

## What works today

| Capability | Status |
|---|---|
| Identify containers from content (MP4/MOV/3GP/M4A, Matroska/WebM, AVI, MPEG-PS/TS, MXF, FLV, ASF, Ogg) | Working |
| List tracks: codec, resolution, frame rate, sample rate, channels, language, duration | Working for MP4 family and Matroska/WebM |
| Privacy metadata inspection (GPS, device, software, dates, XMP, cover art, timed-metadata tracks; Matroska tags, attachments, chapters) | Working |
| Lossless metadata removal for MP4 / MOV / 3GP / M4A | Working, verified |
| Metadata removal for Matroska / WebM | Not implemented (inspection only, stated in the app) |
| Frame decoding, preview, playback | **Not implemented** |
| Moving-object / face / plate redaction | **Not implemented** (needs decoding, tracking and licence-reviewed detection models) |
| Temporally consistent adversarial protection | **Not implemented**: no frames decoded and no validated model. Flicker metrics exist for future evaluation |
| Re-encoding, audio-video sync verification after encode | Not implemented |

## Lossless MP4 metadata removal

The MP4 family stores media in `mdat` and describes it in `moov`. Location and device data live in
`udta`, `meta`, XMP `uuid` boxes and separate timed-metadata tracks (Apple's location / sensor
tracks). Removal:

1. Parses the box tree with bounded depth and node count; fragmented MP4 (`moof`) is refused
   rather than half-processed.
2. Drops `udta`, `meta`, `uuid`/XMP, `loci` and every timed-metadata track (and `tref`
   references to dropped tracks); zeroes creation and modification times.
3. Recomputes where each kept chunk now sits and rewrites every `stco` / `co64` offset table.
4. Re-parses the output, checks that every kept track has the same codec, width, sample rate and
   sample count, and that the first 64 bytes of every chunk are identical to the
   original. Any mismatch fails the export; nothing is written.

The picture and sound are never re-encoded, so quality is unchanged. Tested on synthetic files
with `moov` before and after `mdat`, and with 1,500 random mutations (no panics).

## Path to real video protection (Part 4C)

1. A decoder for untrusted bitstreams. Leading candidate: FilmCraft's pure-Rust codecs (MIT OR
   Apache-2.0; H.264 decode/encode, HEVC and AV1 decode per its README), subject to a security,
   correctness and patent review. FFmpeg remains a test oracle only.
2. A validated per-frame objective (an image-to-video or face-swap model whose licence allows
   local use), optimised with the existing projected-gradient SDK.
3. Temporal consistency: perturbations shared or smoothed across frames so they do not flicker.
   `photo_media::temporal::measure` already reports mean perturbation, frame-to-frame perturbation
   change and luminance flicker for this.
4. Re-encode, reopen, and re-measure after social-media style compression before any claim.

Until all four exist the application does not claim any video manipulation resistance.

## Command line

```
photo-defense media-inspect clip.mov
photo-defense strip-video clip.mov -o clip-clean.mov
```
