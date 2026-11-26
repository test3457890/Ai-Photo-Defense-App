# Hardware and platform compatibility

| Platform | Compiles | Automated tests | Launch | Runtime functionality | Installer |
|---|---|---|---|---|---|
| Windows 10 x64 (19045) | Yes (local) | Yes, 73 pass (local) | Yes | Import, edit, colour, layers, scan, redact, export, logging, crash recovery observed | None |
| Windows 11 x64 | CI (pending for this branch) | CI | Not tested | Not tested | None |
| macOS Intel / Apple Silicon | CI build at earlier commit | CI | Not tested | Not tested | None |
| Linux x64 | CI | CI | Earlier increment only | Not tested in Part 3 | None |
| ARM64 (Windows/Linux) | Not configured | No | No | No | None |

**CPU:** all processing is CPU-only. Colour adjustments run on scoped threads (one band of rows per logical core, up to 16). The interface renders with tiny-skia on the CPU.

**GPU:** not used. No CUDA, Metal, Vulkan or wgpu compute path exists; the Performance Center says so instead of showing a figure.

**Memory:** imports are limited to 24 megapixels after decode (RAW previews included), 64 MiB for ordinary files and 256 MiB for RAW containers. Interactive editing works on a proxy of at most 1600 px; at most four proxies are kept in memory. An 8 GB system was not available for testing; the limits are sized so a 500-photo collection holds thumbnails only.

**Measured on the test machine** (16 logical cores, 32 GB RAM, debug build): proxy render of a 1600x1067 image with no edits about 290 ms; Privacy Scan of the same image about 2.8 s (debug). Release-build figures are in `docs/BENCHMARKS.md`.
