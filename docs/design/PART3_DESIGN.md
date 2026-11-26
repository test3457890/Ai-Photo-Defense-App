# Part 3 interface redesign: evaluation, proposals and validation

## 1. Evaluation of the Part 2 interface

Screenshots of the previous build are kept in `docs/screenshots/privacy-scan.png`, `edit-redact.png`, `export-verified.png` and `technical-ai-defense.png`.

Problems found by launching it on Windows 10 (1920x1080 at 125% scaling):

- Workspaces were a row of large tab buttons over one fixed three-column layout. The three modes changed which tabs appeared, not how the work was done.
- Controls were full-size Iced defaults with rounded, saturated blue buttons: it read as a web dashboard, not photo software.
- No menu bar with menus, no histogram, no filmstrip, no resizable panels, no before/after, no adjustments.
- Explanatory paragraphs competed with the photograph for space.
- The window opened larger than the work area at 125% scaling, hiding the status bar.

## 2. Three proposals considered

| | A. "Darkroom" | B. "Inspector" | C. "Utility + Studio + Bench" (selected) |
|---|---|---|---|
| Idea | One Lightroom-like layout for everyone; modes only hide panels | Single-window document inspector (Affinity-like) with modal tool palettes | Three task-shaped layouts over one shared state |
| Essential | Same layout, fewer panels | Same as Professional | ImageOptim-like processing list with a single action bar |
| Professional | Library / viewer / inspector / filmstrip | Canvas with floating palettes | Dockable pane grid: Library, Viewer, Inspector, Filmstrip |
| Technical | Extra panels | Separate window | Tool navigator with full-page Diagnostics, Performance, Validation, Forensics, Lab, Models, History |
| Risk | Beginners still see a pro editor | Floating palettes are fragile in Iced 0.13 | More views to maintain |

C was chosen because the brief requires meaningfully different workflows. Principles taken from each reference without copying assets: ImageOptim (Essential: drop files, one list, one button, clear warnings), Capture One / Lightroom Classic (Professional: neutral grey viewer, right-hand collapsible tool groups, histogram at the top, filmstrip, keyboard tool keys, \\ for before/after), Darktable / GIMP (Technical: dense tables, logs and measurements instead of decoration).

Visual identity: "Graphite" dark and "Paper" light themes, one blue accent, 1 px separators, 2 px radius, 11 to 13 px system UI font, no gradients, glows, shadows, shield icons or scores. The viewer background is a neutral grey so colour judgement is not biased.

## 3. Framework evaluation

| Criterion | Iced 0.13 (kept) | egui | Slint |
|---|---|---|---|
| Native rendering without GPU | Yes (tiny-skia) | Needs a GPU backend or software fallback | Yes (software renderer) |
| Docking / resizable panes | `pane_grid` (resize, drag to rearrange, maximise) | egui_dock (third party) | Manual |
| Custom styling | Full style functions | Immediate-mode styling | Strong (.slint DSL) |
| Accessibility | Limited (no AccessKit in 0.13) | AccessKit | AccessKit |
| Licence | MIT | MIT/Apache-2.0 | GPLv3 / royalty-free / commercial |
| Migration cost | none | full rewrite | full rewrite + DSL |

Decision: keep Iced. Its gaps (accessibility, native menus) are real and documented below, but a rewrite would have consumed Part 3 without fixing the photographic functionality. Slint's licensing needs an owner decision. egui would trade one set of gaps for another. Revisit when Iced 0.14 (AccessKit) is evaluated.

Defect found during validation: Iced 0.13's software canvas draws geometry at the wrong position when nested inside pane grids (reproduced with a minimal probe). The histogram and viewer overlays are therefore rasterised on the CPU and shown as images.

## 4. Validation performed on the real application (Windows 10, 125% scaling)

| Check | Result |
|---|---|
| Launch, maximised, status bar visible | Pass (after fixing the oversized default window) |
| Essential, Professional, Technical layouts | Pass, screenshots `docs/screenshots/part3-*.png` |
| Mode switching keeps collection, edits, scan results | Pass |
| Dark and light themes | Pass (`part3-light-photo-menu.png`) |
| Restored (smaller) window | Pass, panes reflow (`part3-small-window.png`) |
| Menus open, close on outside click / Esc | Pass after fixing full-width popup background |
| Rect selection drag, overlay alignment | Pass after replacing canvas overlays |
| Exposure slider drag creates "Basic" layer, live proxy render, histogram updates | Pass |
| Ctrl+P scan, finding box drawn, uncertain/confirmed grouping | Pass |
| Autosave recovery after forced termination | Pass: banner offered restore of the autosaved session |
| Keyboard tool keys, \\, J, O, F11, arrows, Ctrl shortcuts | Implemented; Ctrl+P, Esc, Enter observed |
| Screen reader | Not supported (Iced 0.13 has no AccessKit) |
| Multi-monitor | Not tested |
| macOS / Linux rendering | Not tested in this increment |

Known UI limitations: no OS-native menu bar or right-click context menus (right-click on a filmstrip item toggles the flag), no edit-canvas zoom beyond fit, pane layout is not persisted, settings are not saved between sessions.
