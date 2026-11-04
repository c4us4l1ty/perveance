# Autonomous capability growth

## Contract

The user asks for an outcome once. Perveance attempts it autonomously within preset policy, including adding a missing reusable feature. It does not request approval for each ordinary build. Bounds, unavailable isolation, safety gates and declared dependencies can stop a task; tell the user why, without presenting an incomplete result as success. The supervisor itself is outside agent write authority.

## State machine

1. **Intake:** capture prompt, project revision, desired editable artifacts, input permissions, budgets and cancellation token.
2. **Discover:** search a locally installed capability registry by versioned operation, data type, scope and test evidence. Separate installed, staged, disabled, incompatible and missing states.
3. **Plan gap:** decide existing operations vs one-off sandboxed generation vs persistent extension vs candidate editable-core update. Specify limited semantics, required tests, permitted dependencies and migration impacts. Prevent duplicate builds through per-capability locks.
4. **Prepare:** snapshot project and current active versions; allocate isolated scratch, immutable source snapshot, restricted dependencies and sandbox. No inherited host secrets.
5. **Build:** harness edits only its assigned candidate workspace. Compilation, build scripts, tests and code-generated tooling are untrusted processes. Capture bounded logs; deny unapproved network and host writes.
6. **Verify:** trusted supervisor runs independent baseline tests and candidate tests, checks manifest/contract, performs malicious-behavior and migration checks as applicable, and renders fixture outputs. Candidate tests alone cannot validate themselves. A test pass is evidence, not a proof of safety or artistic quality.
7. **Stage and activate:** validate immutable content digests and compatibility, stage new version, then atomically commit registry/artifact pointer and, for core updates, restart into a staged application and health-check. Roll back on any failure or crash. Never replace trusted supervisor, sandbox policy or trust roots.
8. **Fulfill:** reload registry; run the actual creative task with the feature. Save editable source and PNG, inspect result when tools permit, and report capability version, limitations and output paths.
9. **Reuse:** future tasks search the registry and use a compatible installed feature rather than build it again; check ownership, version and actual test status. If it is inadequate, propose and build a scoped upgrade.

Cancel/timeout/crash at any point leaves previous active versions and saved project usable. Every transition has a durable event and idempotent recovery; no half-installed feature.

## Capability package

A manifest describes ID, version, host API compatibility, operations, input/output schemas, document payload schema, supported platforms, requested permissions, resource ceilings, dependencies, tests, known limits, provenance and content hash. It is a declaration, not permission to access a resource: the supervisor grants only what policy allows.

First-class extensions should use a sandboxable portable execution model (evaluate WASM + a constrained host API). They return validated document operations rather than unrestricted mutation of host memory or paths. Runtime isolation and resource limits must be tested; WASM is not a magical guarantee. An extension can produce an editable vector payload and render operations only if the host's versioned extension contract explicitly supports such typed payloads, editing operations and rendering. Native binaries are not accepted as plugins simply because they passed tests.

Some features require changing core document/renderer code. A separate candidate-core pipeline may rebuild agent-writable components while the independently installed supervisor holds activation authority. Protected components and tests are read-only to the candidate and cannot be replaced by it. Candidate code is still untrusted: run it under an enforced runtime boundary with mediated document/filesystem access; if that cannot be supported for a particular update, refuse autonomous activation. Code signing/health checks do not make it safe. The future of this pipeline is gated on proving practical isolation and recovery in ROADMAP.md.

## Honest vector demonstration

Use a deliberately vector-free baseline with generic extension-payload, edit and render contracts already present. Ask for editable filled/stroked paths and PNG export. The agent creates and installs a persistent scoped vector capability, supplies tests, makes an illustration, saves/reopens it and exports it; a second task reuses the exact registered version. If the baseline does not support those contracts, first implement and validate the generic contract as foundational work; do not simulate extensibility. If the agent cannot implement the real capability, report failure. Passing a mock or returning a prewritten vector implementation is not proof of the end-to-end behavior.

## Boundaries and limitations

- Different model/harness/provider combinations have varying performance and connectivity. Use a controlled compatibility matrix, not claims of interchangeable auth.
- Version-aware operations and stable IDs permit agents to query what is available; do not rely only on prose descriptions or assume vector means full SVG support.
- Spend and time estimates are best effort. Enforce preconfigured hard local quotas where technically possible and show actual provider usage when available.
- Artistic quality remains subjective; validate structure and rendering mechanically, then use visual review and user feedback, not invented success guarantees.
