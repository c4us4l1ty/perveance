# Product requirements — Perveance

**Status:** Proposed, owner review. **Product:** an AI-first, desktop-oriented art studio whose agents can extend its creative abilities. **Initial operating-system target:** Linux desktop and Linux headless environments; Windows/macOS are subsequent platform milestones, not assumed to have equivalent sandboxing. This is a planning default, not a promise to drop other platforms.

## 1. Vision and audiences

Create editable art with conversation, direct manipulation, code-driven generation, or a combination. Primary audiences are pixel artists and game developers, illustrators, and graphic designers. Long-term ambition is a sophisticated 2D/2.5D editor, not a promise of near-term Krita or Aseprite parity.

The novel behavior: when a requested reusable capability is genuinely absent, Perveance tries to implement, verify, activate, catalog and use it without interrupting the user for routine approval. It remembers successful capabilities. The system must admit failure rather than claim an unimplemented feature exists.

## 2. Product principles

- Human-editable output first: preserve layers/objects where supported; PNG alone is a delivery artifact, not always the source.
- The GUI is a real art editor, not merely a chatbot or automation dashboard. Agents use the same document engine via a headless interface.
- BYOK, explicit provider selection, and no assumption that all coding harnesses use all providers.
- Autonomy is on inside a clearly configured policy. Do not prompt for every generated feature. On policy violation or untrustworthy environment, stop, report and offer a way to change settings later.
- Never silently downgrade isolation or fabricate successful validation. Preserve the user's art and make rollback visible.
- Reuse established open source where appropriate, respecting code, asset, trademark and dependency licenses.

## 3. Example user journeys

**Pixel asset:** Ask for a 32×32 potion sprite with three editable layers. The agent uses existing tools or generates asset code in isolation, previews, checks dimensions and transparency, saves an editable project and exports PNG. The user can edit the result manually.

**Missing feature:** Ask for a vector temple with editable shapes and PNG export. If native vector capability is absent, the agent attempts a scoped persistent vector capability. It adds explicit tests, activates within policy, creates artwork, and exports it. On a second request, it discovers and reuses the capability. If it cannot add it safely, it explains why and offers a clearly labeled alternative; a raster-only output is not claimed to be editable vector art.

**External agent:** In Arena Agent Mode, a supported build and project are available in the session. The agent discovers the headless interface, generates and visually inspects an asset, exports files and presents them. No assumption that it can control an app open on the user's machine.

## 4. Functional requirements

### R1 — Creative editor
- GUI: canvas, zoom/pan, pixel grid at appropriate zoom, layers, selection, basic pixel pencil/eraser/fill, undo/redo, color/palette controls, save/open and PNG export. Fully usable without an AI key.
- A versioned, editable project format retains layer order, dimensions, color/alpha, and capability-specific data. Loss of unsupported capability data must be avoided; see PROJECT.md.
- Human and agent actions use the same transaction/document model. External changes are not silently overwritten by an open GUI.
- Long-term: vector objects, richer brushes, animation/sprite sheets, asset pipelines, 2.5D workflows and more sophisticated design tools, prioritized after the core loop.

### R2 — Creative agent
- Accept a prompt, optionally references attached by explicit user choice, an output target and constraints. Display progress, choices and failures. Offer cancel.
- Prefer existing tools, reusable workflows and installed capabilities. Distinguish a one-off asset-generation script from a persistent feature.
- Provide preview and deliver an editable source plus requested PNG when the task requires editable output. Record what was actually produced.
- Image-generation model integrations are a later optional adapter; disclose what data leaves the machine and what is and is not editable. No model-provider capability is assumed.

### R3 — Autonomous growth
- Inventory capabilities, identify a gap, select a scoped extension or candidate core change, build and verify in isolation, activate atomically, retry the original task, and register provenance for future reuse.
- Support core evolution in principle, not just plugins, subject to the immutable security boundary and the stronger core-update gate described in SECURITY.md. This does not grant unrestricted host-code modification.
- Cancellation, timeout, quota, failed tests, sandbox loss or incompatible projects lead to no partial activation and a recoverable report.

### R4 — Agent interface and Arena
- Provide a documented, versioned headless CLI with machine-readable discovery, structured operations, results, progress and failures. It must run without the desktop GUI.
- Arena access is via an available sandbox installation/project and its permitted command tools. Direct invocation of Arena Agent Mode from Perveance and direct access to a local desktop are unverified and not prerequisites.
- File access, networking and credentials depend on the specific session; never promise local-machine access or credential availability.

### R5 — Providers and harnesses
- BYOK with OS credential storage or a clearly disclosed alternative. No API keys in source, project files, prompts, process arguments or logs.
- First prove one OpenRouter model route and one compatible coding harness after a compatibility spike; do not promise that Claude Code, Codex, DeepSeek, Pi, Hermes, Oh My Pi and OpenCode all accept the same endpoint or auth.
- Reserve/request budget and enforce time, process and storage limits; report uncertainty in cost estimation. Provider-side charging/limits may differ.

### R6 — Safety and transparency
- Enforce isolation at the OS/VM boundary, not through prompts. Default-deny capabilities and untrusted execution; fixed trusted supervisor. Test isolation adversarially per platform.
- Version and roll back features and app updates. Expose task history, data-sharing scope, provenance, errors and security mode. Clean up temporary data under a documented retention policy.
- Accessibility: keyboard operations, visible focus, readable contrast, scalable UI; do not rely on color alone.

## 5. Release acceptance

**First usable vertical slice (MVP):** Linux GUI and headless engine; basic editable pixel art, PNG export, local projects, discoverable CLI, an isolated autonomous builder, bounded execution, registry and rollback. Demonstrate the missing-feature scenario in ROADMAP.md twice. For this experiment vector must really be absent initially; a preinstalled or mocked feature is not evidence of autonomous growth. Include a CPU/software rendering path for headless testing or explicitly gate environments without one.

**Not MVP:** Krita parity, production-grade illustration brushes, real-time collaboration, a public extension marketplace, all listed harnesses, image diffusion, Windows/macOS sandbox parity, full 2.5D, or a direct Arena connector. These are future directions, not quietly promised MVP features.

## 6. Success measures

- A second compatible request invokes the previously installed feature without a second build; check registry and build logs.
- After save/reopen, the test illustration retains editable vector objects and exports a valid PNG with expected dimensions and nonempty pixels.
- Failed or malicious candidate changes do not activate or gain a host secret in the adversarial test suite.
- GUI and CLI display/render the same saved project within documented renderer tolerance.
- Track build success, time, spend and manual intervention over a documented test set; no invented target percentage before baseline measurement.
