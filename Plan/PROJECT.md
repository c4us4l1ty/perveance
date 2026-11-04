# Project and technical architecture

## Decisions and defaults

- **Core language:** Rust. The GUI or an external harness may bring other technologies where justified; do not force a rewrite of an otherwise suitable upstream component.
- **Deployment:** desktop GUI plus the same engine available headlessly. Prototype Linux first; define other OS targets explicitly as supported backends become testable.
- **Foundation:** conduct a short Graphite-versus-composed-Rust-engine spike before committing to a full fork. Graphite already includes vector/raster infrastructure, so a vector-absent experiment must use a controlled minimal configuration, not pretend Graphite lacks vectors. Pick integration, selected component reuse, fork, or new engine using the gate in RESEARCH_AND_LICENSES.md.
- **GUI candidate:** egui for a small Rust-native shell, subject to canvas, input, accessibility, OS integration and upstream-composition spike. Graphite's existing frontend is Svelte; assess actual integration costs rather than asserting Graphite can be embedded as an egui crate.
- **Sandbox:** Linux-first independently enforced isolation, with a practical backend selected and tested in the spike. Unsupported/unavailable isolation means autonomous execution is disabled, not run unsandboxed.
- **Product license:** undecided. Do not copy GPL code into the product pending a deliberate decision. Preserve required upstream attribution.

## Logical components and trust

1. **Studio GUI** — human editing, chat, task status, capability management and policy controls.
2. **Document engine** — versioned projects, operations, undo/redo, raster/object compositing, rendering and export. No agent provider dependency.
3. **Headless gateway/CLI** — stable machine-readable operations against the same engine; no mandatory daemon or cloud service.
4. **Coordinator** — task planning, capability discovery, checkpoints, budgets, structured events and provider/harness adapters.
5. **Trusted supervisor** — policy enforcement, secret broker, isolation, validation, activation and recovery. This small boundary is not rewritten or overridden by the autonomous agent.
6. **Sandboxed workers** — untrusted creative scripts, extension candidates, coding agents and build/test processes; separate execution contexts where their permissions differ.
7. **Capability registry** — immutable versioned artifacts and manifests, compatibility/test status, exact provenance and dependencies; separate installed and staged states.

Architecture describes responsibility, not a mandatory initial crate layout or an assertion that an existing upstream exposes this API. Minimize duplicated engines and incompatible document models.

## Document and artifact model

A project holds metadata, canvas size/color settings, stable object and layer IDs, ordered layers, editable raster data and capability-defined payloads, plus asset references. Use a documented versioned format with explicit limits and migration tests. PNG is an export, not the editable project. A command is an atomic transaction with validation, revision number and undo information where possible. Checkpoints and write-to-temp + durable atomic replacement protect against partial saves; validate behavior on each filesystem/platform.

Unknown or disabled capability payloads must survive open/save byte-for-byte where practical. Mark them unavailable, do not silently rasterize or claim editability, and fail exports that cannot be rendered correctly. Migration preserves a recoverable pre-migration copy and verifies reopen; reverse migration is not presumed. Project data cannot execute code on open. Imports/images and archives have size, nesting, decompression and parser limits.

Stable capability ID + version + schema version identifies a layer/operation. Upgrades declare compatibility/migrations; downgrades are supported only when explicitly verified. Per-task candidate artifacts and prior installed versions stay separate; failed updates cannot replace a working feature.

## Renderer and reproducibility

Use a deterministic software/headless path for baseline fixtures where feasible; GPU rendering is an optimization, with documented differences. Specify blend modes, alpha, color space, pixel grid rules and export dimensions. Test exact pixel art output and tolerance-based vector rendering where appropriate. GUI previews and exports must share rendering semantics. Seed procedural work where applicable, record generator inputs/versions, and do not claim fully deterministic behavior for remote models or nondeterministic hardware.

## Resource and data lifecycle

Project workspaces are chosen by the user. Task scratch and build caches have size/time limits and retention controls; discard candidate outputs on failed activation. Never claim secure deletion on SSD, snapshots or copy-on-write storage. Protect credentials separately from projects; no remote data transfer without the user enabling a provider and understanding the scope. Backup/restore and project export are visible features, not an assumption about Git.

## Delivery shape

Distribute a signed/versioned app and separately versioned compatible capabilities once packaging is designed. The signature authenticates the trusted supervisor's activation record, not the trustworthiness of AI-generated code. Updates use staged, integrity-checked A/B versions and health checks with rollback. Agent-produced core changes cannot change the supervisor, policy, sandbox, key broker, verification machinery or their trust roots. Supervisor updates are a separate user/developer-controlled software-update channel, not autonomous task output.
