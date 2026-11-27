# Invisible ownership watermark

`photo_edit::watermark` hides a 48-bit number (an owner, licence or batch ID) in a photograph so
you can later show that a copy came from you. It is **provenance, not protection**: it does not
stop AI editing, face swapping or cloning, and anyone who has your passphrase can find or erase it.

## How it works

Every 8 x 8 block of the image carries one bit of a 64-bit word (48-bit payload + 16-bit
checksum) in four mid-frequency DCT coefficients of brightness, multiplied by a pseudo-random
+1/-1 sequence derived from your passphrase. Detection repeats the sequence and correlates. A
result is reported only when the checksum matches, so an unmarked photo or the wrong passphrase
reports "not found" instead of a random number.

## Measured (unit tests and a real 1600 x 1067 photo)

| Condition | Result |
|---|---|
| Visibility | PSNR above 45 dB at the default strength 4 (test asserts it) |
| Lossless PNG / TIFF / WebP | detected |
| JPEG quality 90, 80, 70 | detected (tests); quality 80 on the real photo: confidence 11.7 |
| Wrong passphrase, unmarked photo | not detected (confidence below 1) |
| Resize (512 to 400 px) | **not detected**; cropping and rotation also break it |
| Images smaller than about 128 x 128 px | refused |

Not tested: social-network recompression pipelines, screenshots, printing and rescanning.

## Command line

The passphrase is read from an environment variable so it does not end up in shell history
(PowerShell: `$env:APD_WATERMARK_KEY = "..."`). Use at least 8 characters.

```
photo-defense watermark-embed photo.png -o marked.jpg --payload 123456 --format jpeg --quality 85
photo-defense watermark-detect marked.jpg
```

`watermark-embed` encodes the output, decodes it again and refuses to save if the watermark did not
survive at the chosen quality and strength. The desktop app does not yet offer the watermark;
it is command-line only in this version.
