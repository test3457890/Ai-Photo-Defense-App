# Interface review

A three-workspace concept sheet was generated and visually reviewed on 2026-11-26 before implementing the desktop layouts. It is a design mockup, not software evidence. Generated concepts are available in the project conversation; actual screenshots belong in docs/screenshots and must be labeled with their tested version and fixture provenance.

| Workspace | Concept | Implementation direction | Remaining work |
|---|---|---|---|
| Essential | Compact light ImageOptim-style file list and simple export controls | Shared queue, add/drop, export controls, no invented protection scores | Reduce blank space at large sizes; validated presets and compact window defaults |
| Professional | Charcoal photographic canvas with browser and inspector | Original/export viewers, difference, adjustable browser width, collapsible inspector | Filmstrip, draggable splitters, dockable panels, full-resolution/color-managed canvas |
| Technical | Tabular engineering utility with tests, model state and logs | Real transformation table, processing log, honest model/compute status, reports | Model catalog, hardware monitoring, attack adapters, reproducible benchmark tabs |

The mockup labels were reviewed for misleading claims. Controls that depend on unimplemented model defenses are not shown as operational. The Professional workspace can use the Dark theme, while Light remains available in every mode. Native font families, thin separators, a traditional menu strip, minimal motion and ordinary rectangular controls establish a desktop utility appearance. Current Iced control styling has small default rounding; further platform-specific visual review remains necessary. There are no gradients, marketing cards, shield badges or fabricated protection percentages.

The three modes share application state, preventing a mode switch from discarding queued files. The initial UI is not final. Menu integration is drawn in Iced, and full OS menu semantics, screen readers, accessibility testing, focus visibility and keyboard workflows remain release gates.

## Screenshot rules

Only capture a running native build. Do not present the generated concept sheet as a screenshot. Use tests/fixtures/calibration.png, an original synthetic CC0 test illustration created for this repository, or a rights-cleared photograph with its source recorded. A screenshot with a displayed photograph is evidence of preview rendering, not evidence of AI protection. A table of quality metrics is evidence of measurements only.
