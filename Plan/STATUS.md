# Status and decision log

**Date:** 2026-11-04  
**State:** Owner said START. Phase 0 investigation and an initial experimental Rust GUI/headless prototype have begun. See PHASE0_FINDINGS.md for open gates and current limitations.

## Confirmed owner direction

- Name: **Perveance**. Rust-based, AI-first art studio, with an actual GUI.
- Main audiences: pixel artists/game developers, illustrators and graphic designers.
- AI may write code to generate assets and export PNG; editable tools/layers and optional image-generation integration are also desirable.
- The AI should autonomously build missing features, persist them for reuse, and complete the requested artwork without repeated approval prompts.
- Headless/CLI usability by an external agent in Arena matters; no automatic access to the user's computer is implied.
- Strong security; consider lessons from Holonomy. Use and evaluate open source where appropriate.
- Visual reference is an inspiration for dark surfaces, icons, padding and green accents, not a requirement to clone the screenshot.
- The owner subsequently said START and requested GitHub CI be used for Rust compilation in this sandbox.

## Proposed defaults for owner review

1. Linux-first GUI and headless baseline; other systems after separate security/platform validation. Platform priorities remain changeable.
2. Graphite is the first foundation candidate, but choose fork/integration/composition only after the Phase 0 spike.
3. Autonomous execution within configured bounds; fail closed if isolation or policy requirements cannot be met. The trusted supervisor cannot be rewritten by the art agent.
4. First live provider/harness pair chosen based on actual OpenRouter/broker/sandbox compatibility; no promised support for every named harness in MVP.
5. Editable project + PNG; scoped vector capability proof before broad feature expansion.
6. Product license undecided; do not incorporate GPL code pending decision and review.

## Evidence collected

- Perveance repository contained only README.md before the planning files; a small Rust prototype and CI workflow have since been added.
- Inspected Graphite's README, root layout and selected project metadata, relevant library manifests, license information and Holonomy's README, PRD audit, workspace, workflow and selected security-related files. See RESEARCH_AND_LICENSES.md and SECURITY.md for scoped findings and pinned links.
- No Graphite desktop or Holonomy build, formal security audit, license audit of all dependencies, agent compatibility test or sandbox escape test has been performed. The Graphite headless CLI passed a default-feature cargo check in CI. Do not treat plan proposals as implemented features.

## Still to validate during Phase 0

- Actual target OS priorities and Linux sandbox prerequisites; provider/harness endpoint and authentication support; ability to run in nested sandbox environments such as Arena.
- Graphite build/architecture, editable document operations, pixel workflows and headless exposure; license/asset/dependency inventory.
- Project license/distribution intent, sandbox backend and acceptable operating-resource defaults. Record owner decisions or documented, reversible defaults before release; do not request secrets in chat.
- Feasibility of editing/rendering arbitrary extension-defined object types without granting unsafe host execution; feasibility of autonomously updated core components under confinement.

## Work log

- Planning: researched upstream repositories and created the planning documents.
- Implementation started: preliminary feasibility observations, a shared pixel model, minimal desktop GUI and headless CLI, and a GitHub CI workflow. Autonomous execution is not implemented.
- Next: verify the actual CI run; complete the Graphite, sandbox and harness feasibility gates. Do not claim production readiness.
- Follow-up: added shared raster line/flood-fill operations and a bounded, all-or-nothing headless operation batch, plus GUI fill and continuous strokes. Rust CI passed on commit `7b36396` ([run 41800848834](https://github.com/c4us4l1ty/perveance/actions/runs/41800848834)). The native GUI was compiled, not interactively validated. Autonomous code execution, AI integration, vector support, crash-atomic saves and a finalized foundation decision remain outstanding.

- Current branch: added zoom/grid, variable-size canvases, GUI layer rename/reorder/removal, headless rectangle and layer commands, strict unknown-field rejection and a committed Cargo.lock. The locked Rust tests, CLI smoke and X11 desktop-startup smoke passed in [CI run 41802110846](https://github.com/c4us4l1ty/perveance/actions/runs/41802110846). Window startup is not a visual or interaction audit. Projects with unknown fields are rejected; preserving opaque extension data remains future work.

- Added CLI operation examples and a tested layered potion asset. Inspected a CI-generated X11 screenshot of the default window and refined centering, spacing and original tool icons. This is only a single startup screenshot, not a complete visual, input, accessibility or platform test.
- Added bounded static-PNG import into a new canvas or matching-size raster layer in the GUI and CLI. The initial [PNG import CI run](https://github.com/c4us4l1ty/perveance/actions/runs/41804157603) passed tests covering RGBA, RGB, grayscale, indexed and 16-bit conversion, invalid/truncated data, APNG rejection, size limits and no-overwrite behavior. PNG metadata and color management are not preserved.
- Added bounded undo/redo history for the GUI; its [initial CI run](https://github.com/c4us4l1ty/perveance/actions/runs/41804474941) passed. Subsequent changes still require the current branch's final CI result. Autonomous execution and AI providers remain unimplemented.
- PR #2 was merged to `main`. Subsequent work was carried onto a new Arena branch: pixel-aligned rectangle/ellipse tools and matching typed CLI operations; a straight-line tool; GUI composite eyedropper; and headless sampling of a composite or raw layer pixel. Shape previews are guides, and all shapes remain raster pixels rather than editable vector objects.
- The [editor/CLI CI run](https://github.com/c4us4l1ty/perveance/actions/runs/41808571823) passed Rust tests, headless tests, and fixed-layout X11 interaction checks for drawing and sampling. This is not full GUI, accessibility, or cross-platform validation. Autonomous execution, an AI provider, sandbox enforcement, and vectors are still absent.
- Added undoable layer duplication, horizontal/vertical reflection, and clearing in the shared raster model, GUI, and typed headless batches. Duplication preserves visibility and raw RGBA bytes, inserts immediately above the original, and truncates long names at UTF-8 boundaries. The [layer-model CI run](https://github.com/c4us4l1ty/perveance/actions/runs/41809378964) passed the Rust/CLI tests, PNG export and existing X11 interaction checks. The [X11 interaction run](https://github.com/c4us4l1ty/perveance/actions/runs/41810432763) also exercised the duplicate, horizontal flip, clear and undo controls. These are fixed-layout smoke tests, not a complete GUI audit.
- Added a normal-window-close warning for unsaved artwork, comparing the current project with the last successful project save/open (or the initial blank canvas). A new or PNG-imported canvas requires a project save. The warning is not crash recovery or protection against force-quit/external changes. [CI run 41811187520](https://github.com/c4us4l1ty/perveance/actions/runs/41811187520) passed a native X11 close request and cancel interaction plus dirty-state tests; broader GUI testing remains needed.

- Added versioned, revision-checked, data-only edit proposals for the CLI and GUI, using SHA-256 fingerprints of the v1 project serialization. The [locked CI run](https://github.com/c4us4l1ty/perveance/actions/runs/41812937470) passed library/CLI tests, a simulated egui proposal-button interaction, and existing native X11 checks. The new handoff itself has not been exercised in a native GUI session. Fingerprints are not cross-process locks or authorization; untrusted proposals can still clear artwork. No provider, agent execution, sandbox or extension registry was added.
- Added an experimental, explicitly opt-in OpenRouter GUI adapter that requests typed pixel edits and permits the model only to add and paint new raster layers. A locally generated revision binds responses to the initiating project; stale, disallowed and invalid edits are rejected atomically. [Initial CI](https://github.com/c4us4l1ty/perveance/actions/runs/41814609447) passed mock HTTP, response-validation, GUI state, Rust and X11 tests. No real OpenRouter call, model compatibility, spend-limit enforcement, credential broker or sandbox has been validated. Requests can incur charges and transmit the prompt and canvas dimensions/layer count; see README.

- PR #4 merged the revision-bound handoff, experimental OpenRouter adapter and fixed isolation diagnostics. [CI run 41816532726](https://github.com/c4us4l1ty/perveance/actions/runs/41816532726) passed, but its bubblewrap diagnostic failed during network namespace setup; it did not certify isolation. Autonomous execution remains disabled.

- Added a 16-slot editable session palette to the GUI; [CI run 41819935147](https://github.com/c4us4l1ty/perveance/actions/runs/41819935147) passed the Rust tests and existing X11 checks. Palette interaction was simulated in egui tests; it was not separately exercised in native X11. The bubblewrap diagnostic still failed on network namespace setup, so autonomy remains disabled. A follow-up adds explicit separate palette-file save/load and headless inspection. [CI run 41820353435](https://github.com/c4us4l1ty/perveance/actions/runs/41820353435) passed Rust tests, simulated palette-file controls and existing X11 checks; the bubblewrap diagnostic again failed network namespace setup.
