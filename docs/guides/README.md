# User guides

## Modes

- **Essential** (Ctrl+1): drop photos or folders, pick a privacy profile, optional crop and "Redact QR/barcodes", choose an output folder, press **Process all**. Every photo is scanned on import. Blocked photos are listed with a warning; open them in Professional mode or explicitly export anyway.
- **Professional** (Ctrl+2): Library (left), Viewer (centre), Filmstrip (bottom), Inspector (right). Drag pane borders to resize, drag title bars to rearrange, **Maximize** in a title bar to focus one pane, View > Reset panel layout to restore.
- **Technical** (Ctrl+3): Diagnostics, Performance, Export Validation, Forensic Inspection, Defense Lab, Model Library, Processing History.

All modes share the same photos, edits and scans.

## Photo editing

Tools (key): Inspect I, Crop C, Rect M, Ellipse E, Polygon P (click points, Enter closes), Lasso L (drag), Brush B (drag; radius in Inspector > Redaction), Gradient G, Radial R. Enter applies the current tool's action, Esc cancels.

Geometry (Inspector > Geometry): aspect crops (1:1, 4:3, 3:2, 16:9, 9:16, 4:5), rotate, flip, straighten (crops to the largest clean rectangle), one-parameter lens distortion, platform resize presets, and perspective correction from a 4-point polygon drawn clockwise from the top-left corner of the plane.

Everything is a reversible layer until export. Undo/redo (Ctrl+Z / Ctrl+Y) covers every edit; each slider drag is one undo step.

## Colour correction

Inspector groups: Histogram (with clipping markers, **Auto tone**, clipping view J), Basic (temperature, tint, exposure in EV, contrast, highlights, shadows, whites, blacks, brightness), Presence (clarity, texture, dehaze, vibrance, saturation), Tone Curve (parametric), HSL (8 bands), Color Grading (split toning), Detail (sharpening, noise reduction), Effects (vignette, grain), Levels (black/white point, gamma), LUT (.cube import/export), Presets.

White balance and exposure are computed in linear light; tone and colour controls work on display-referred values; everything is 32-bit float until the layer is written. Dehaze and noise reduction are simple global/local approximations, not model-based.

Controls edit the selected adjustment layer. **+ Adjustment layer** adds a global layer, or a masked one when a selection, gradient or radial is active. Layers have visibility, opacity, blend mode (Normal, Multiply, Screen, Overlay, Soft light, Luminosity, Color), order and delete.

## RAW photographs

RAW files open through the camera's embedded JPEG preview. Full RAW development is not available (see `docs/CAMERA_COMPATIBILITY.md`). The Info group shows when a photo is a RAW preview.

## Privacy Scan and redaction

Ctrl+P scans locally: container metadata, QR/barcodes (decoded, never opened) and credential/personal-data rules over decoded text. Findings are split into **Confirmed / Pattern** and **Uncertain**; click one to locate it, tick to dismiss a false positive. **Conceal located** redacts all located, undismissed findings with the chosen method. Faces, objects, documents and text in the pixels are **not examined** until licensed models are installed; the scan lists them.

Opaque black/white is secure redaction: pixels are replaced and flattened. Blur, pixelate and mosaic are visual concealment only. Redaction layers always render above adjustments.

## Export (high assurance)

Choose an output folder, format and quality. With high-assurance review on, export is blocked when: no scan was run, located findings are neither redacted nor dismissed, uncertain findings were not reviewed, or only visual concealment was used. After encoding, the file is reopened and checked for metadata, remaining credential codes, redaction completeness and hidden colour under transparency. "I reviewed these: export anyway" is an explicit override. Originals are never overwritten; names get `-2`, `-3` if needed.

## AI protection

Unavailable. No validated adversarial defence model is installed and no preset has measured effectiveness. Enhancements must be applied before any future protection stage; the pipeline order already places protection after edits.

## Diagnostics

Technical > Diagnostics Center: live sanitized log with level/component filters and search, code lookup (e.g. APD-3002), verbosity and retention, **Copy report**, **Preview report**, **Save report**, **Clear logs**. Logs are local JSON lines in the application data folder (`%APPDATA%\AI Photo Defense\logs` on Windows), rotated at 1 MiB with an 8 MiB cap. Nothing is ever uploaded; to file a GitHub issue, preview the report and paste it yourself.

## Crash recovery

The open session is autosaved every 60 seconds to a journal in the application data folder. If the app did not exit normally, the next start shows a banner offering to restore the autosaved work. Saved projects (`.apdproj`) are written atomically; the previous valid version becomes `.bak` plus up to five checkpoints, and a corrupt project opens from the last valid backup. Start with `--safe-mode` to skip autosave restore, the catalog and caches.
