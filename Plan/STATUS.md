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
- No Graphite or Holonomy build, formal security audit, license audit of all dependencies, agent compatibility test or sandbox escape test has been performed. Do not treat plan proposals as implemented features.

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
