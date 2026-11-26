# Contributing

Read HANDOFF.md before changing code so work is not duplicated. Pick a scoped roadmap task, record the starting commit, and keep new features behind an honest availability state until they work. Never call random noise a defense, confuse metadata removal with AI protection or invent benchmark scores.

Run fmt, tests, clippy, build and dependency audit as documented in the README. Include a reproducible case for security/engine changes. For UI changes, inspect a running native build and capture rights-cleared screenshots. Test workspaces against the same document state. Do not upload personal images, model weights or credentials to issues or pull requests.

Research integration requires a pinned upstream source, license/weight review, attribution, exact preprocessing and objective documentation, numerical parity/gradient tests, and a reproducible benchmark with rights-cleared data. Permissive code licenses do not override model or dataset terms. Keep research-only dependencies outside distributable builds unless separately authorized for unrestricted redistribution.

Changes to authentication, telemetry, image uploads, automatic updates or key storage require a threat-model and privacy-policy update. Use a pull request with the concrete behavior, actual validation and remaining limitations. Passing compilation does not establish runtime platform support or protection efficacy.
