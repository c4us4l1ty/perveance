# Build roadmap and acceptance gates

**Planning only.** Start implementation only after the owner says START. Stages are gates, not delivery-date promises; report blockers instead of hiding them.

## Phase 0 — foundation and feasibility spike

- Evaluate Graphite integration/fork/components against a composed Rust engine using the documented research gate. Record chosen revision, build requirements, legal obligations, UI integration and tradeoffs.
- Select and verify an enforceable Linux sandbox backend, local prerequisites, offline build/dependency acquisition design and nested-sandbox limitations. Test a deliberate read/write/network escape attempt. If unavailable, disable autonomous execution and report it.
- Check actual compatibility of one coding harness with the controlled model broker and OpenRouter; build a provider/harness matrix. If it requires key disclosure or broad network, do not claim compatibility. Evaluate software/headless rendering and secure persistent local storage.
- Resolve document model, extension payload/edit/render contract and minimum viable GUI toolkit from evidence. Produce a decision record and update this plan with results.

**Exit:** reproducible evidence for the chosen foundation, sandbox and agent integration; explicit fail-closed fallback. Unknowns are listed, not declared solved.

## Phase 1 — editable GUI + headless baseline

- Functional Linux desktop editor with the GUI structure in GUI.md, canvas, nearest-neighbor zoom, layer operations, basic pixel tools, undo/redo, palette, project save/open and PNG export.
- Shared engine and versioned project transactions; headless discovery/edit/inspect/render/export with structured errors. Concurrent revision detection and safe saves.
- Test GUI/headless save/reopen/render equivalence, malformed input, dimensions/alpha and transparent pixel export. Basic editing works without network or AI.

**Exit:** a human and an external headless agent can independently create, edit, save, reopen and export a layered pixel-art fixture. Not yet an autonomous feature-building claim.

## Phase 2 — supervisor and sandbox

- Trusted policy supervisor, per-task workspace isolation, credential broker, approved dependency fetch, offline builds, resource quotas, cancellation, bounded logs, and clear status.
- Capability contracts for versioned payloads, editing, rendering, discovery, tested persistence and compatibility. Registry lifecycle with provenance, staged/active versions and rollback; candidate code cannot alter trusted boundaries.
- Adversarial isolation, secret, network, quota, cancellation, malformed manifest, crash recovery and migration tests from SECURITY.md. Independent tests are outside candidate control.

**Exit:** supported backend passes the required threat suite; failed or unsupported isolation disables autonomy. A hand-authored test fixture can be installed, used and rolled back safely; this alone is not self-development.

## Phase 3 — real autonomous feature-growth experiment

- Connect one actually compatible provider/harness; exercise the broker without exposing provider credentials to worker processes. Add bounded, durable task-state orchestration.
- Start from a baseline that demonstrably lacks editable vectors but has generic extensible payload/edit/render contracts. Ask for editable filled/stroked vector paths and PNG. Agent writes a persistent capability, builds, passes supervisor-owned checks, activates, creates artwork, saves, reopens and exports.
- Verify against filesystem and registry evidence: first task creates a new capability, actual objects remain editable, PNG is valid and nonempty; second distinct request uses the same installed version with no second build. A failure is documented, not papered over with a mock or prebuilt implementation.
- Exercise failure: invalid/malicious candidate, budget exhaustion, cancelled build, incompatible manifest and activation crash leave previous version/projects intact. Validate visual output; record measured duration and spend where available.

**Exit:** successful reproducible end-to-end demonstration plus threat gates, or documented failure and revised design. This proves a scoped vector subset only, not full vector parity or generalized autonomy.

## Phase 4 — broaden product and controlled core evolution

- Improve pixel tools, palettes, layers, workflow usability, provenance and artist-facing documentation from real usage. Add more formats and tested harness adapters individually.
- Research and prototype candidate core updates inside an independently protected supervisor and enforce runtime confinement. Prove update/rollback and project migration; enable only on environments passing the security gate. If it cannot be isolated, core self-updates remain unavailable rather than unsafe.
- Expand creative coverage in user-prioritized increments: vector editing, animation/spritesheets, procedural assets, non-destructive workflows, optional image-model integrations and 2.5D.
- Add Windows/macOS only after each platform's GUI, credential storage, sandbox, recovery and adversarial tests pass. Packaging, signing, update and backup practices require separate release validation.

## Verification strategy across phases

Unit tests for document math and operations; property/fuzz tests for project and import parsers; integration tests for CLI/GUI/project parity, migrations and rollback; golden PNG tests for pixel output and tolerance-based vector fixtures; interaction/accessibility tests for editor states; sandbox adversarial and fault-injection tests; performance baselines for time, memory and output sizes. Test model outputs with fixed fixtures for deterministic infrastructure tests; separately report non-deterministic live-model experiments and costs.

CI should be least privilege, pin trusted dependencies/actions, check licenses and advisories where feasible, and never expose signing secrets to untrusted build candidates. No candidate-generated test can bypass supervisor-owned checks. Human security review before production claims or broader deployment.

## Key risks and mitigations

| Risk | Consequence | Mitigation |
| --- | --- | --- |
| Graphite too coupled or lacks useful headless API | Integration consumes schedule | Phase 0 spike; select components/composed engine if warranted. |
| Isolation unavailable or nesting unsupported | Autonomous code cannot safely run | Fail closed; verified alternate backend or manual development, no silent host execution. |
| Generic extension contract too weak for real vectors | Demo becomes a mock or forces unsafe host changes | Test payload, editing, rendering and save/reopen before the autonomous experiment. |
| Candidate core code can attack host | Rewriting compromises trust | Confine candidate runtime and deny edits to supervisor; defer core autonomy if not provable. |
| Cost, dependency availability and live model failures | Unpredictable tasks | Quotas, controlled fetching, clear accounting, checkpoints and retry limits. |
| Raster-only output for requested editable art | Product misleads the user | Explicit object-structure and reopen tests; accurately label limitations. |
| Project incompatibility during updates | Data loss | Backups, opaque unknown data handling, compatibility checks, migration tests and restore. |
