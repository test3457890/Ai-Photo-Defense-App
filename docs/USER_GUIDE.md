# User guide

Start with copies of 8-bit JPEG, PNG, WebP or TIFF images. Preserve the source files separately. The program never replaces an existing output, even when the operating system save dialog offers overwrite confirmation.

## Essential

Add photographs or drop files onto the window. Choose output format and JPEG quality, then Export copies. Select an existing output folder, or enter its path in Output directory. The Image path field can import a file when the native file dialog service is unavailable. Each file receives a `-private` suffix and a JSON sidecar. The name refers to metadata removal only. It does not mean that identity or AI training is blocked.

A bad file is reported and other batch exports continue. Filenames are sanitized for cross-platform output. Files with identical sanitized stems can collide and produce an error; choose another output folder or rename the sources. The queue allows up to 64 images. Clear removes the in-memory collection, not the files on disk.

## Professional

Select a collection item to see the original preview. After export, the decoded exported copy appears beside it with measured quality. The difference checkbox displays absolute RGB differences at 8x gain. Image viewers support zoom and pan on the preview, not a full-resolution color-managed editing canvas. The browser width slider adjusts the left panel; Hide inspector collapses the export controls. Full drag docking is not implemented.

Changing workspace keeps the same collection, selection and completed exports. Restarting the program clears the collection. Startup can choose a workspace and import files:

```sh
cargo run -p photo-desktop -- --workspace professional first.png second.jpg
```

## Technical

Export the selected image, then run the transformation lab. Eight deterministic comparisons apply the same transform to original and exported images. The log shows errors and completed operations. Save report writes JSON with the metric definition and an explicit `not evaluated` AI efficacy field. A JPEG test cannot run on a transparent source; use an opaque image for this lab.

The model line truthfully shows no installed desktop defenses. Detailed model catalog controls and hardware monitoring are not yet implemented. GPU inference is not available.

## Keyboard and themes

| Shortcut | Action |
|---|---|
| Ctrl+O, Command+O on macOS | Import |
| Ctrl+E, Command+E on macOS | Export |
| Ctrl+1 / 2 / 3, Command on macOS | Workspace |
| Tab / Shift+Tab | Traverse focusable controls |
| Escape | Close open menu strip |

Light and Dark are explicit themes. System samples the operating system appearance at startup; live theme-change subscriptions are pending. The UI uses Segoe UI on Windows, Helvetica Neue on macOS and DejaVu Sans on Linux, falling back if unavailable. Screen reader integration and full keyboard-only usability still need platform testing.

## Quality and privacy limits

PNG, WebP and TIFF export preserve decoded 8-bit samples losslessly; JPEG is lossy. A PSNR value of null with MSE zero means infinite PSNR, not a missing measurement. SSIM uses non-overlapping 8x8 luminance windows and is named explicitly to distinguish it from Gaussian-window implementations. RGB metrics exclude alpha. Neither high SSIM nor low perturbation proves invisibility or AI resistance.

GPS/EXIF/XMP/ICC are discarded by re-encoding decoded pixels. The program cannot remove location clues or identifying information visible inside a photograph. Your operating system, cloud-synced destination folder and backup software may copy outputs independently. The program does not upload them.

Source color profiles are not converted before removal. Wide-gamut colors can change in other viewers. High bit depth input is rejected rather than silently downconverted. Animated and multi-page files export only the first frame/image. Originals and outputs may remain in OS caches and backups; clearing the collection is not secure erasure.
