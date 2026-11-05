# What remains to complete Perveance

**Assessment date:** 2026-11-05. This is the current execution checklist, not a
claim of completion. The owner has authorized development. Read this alongside
[PRD.md](PRD.md), [ROADMAP.md](ROADMAP.md) and the security gates in
[SECURITY.md](SECURITY.md).

## Two different finish lines

1. **Dependable pixel editor:** the GUI/shared Rust raster engine, editable
   layers, basic tools, palette, undo/redo, PNG import/export, numbered saves,
   limited recovery and offline agent handoff are implemented and exercised in
   CI. This is a useful prototype, not yet a supported stable release.
2. **Original AI-first product:** an agent can safely build a missing persistent
   creative capability, activate it, finish the original artwork, and reuse
   that exact capability on another request. This is **not implemented**. More
   pixel tools alone will not complete it.

[Rust CI run 41986111833](https://github.com/c4us4l1ty/perveance/actions/runs/41986111833)
passed the complete Rust suite, native X11 workflows, bridge parity and downloaded
Linux bundle verification for task-backend commit `3dd9ae0`. This includes
cooperative cancellation, simulated stage interruptions, forced CLI termination
and a persistent task round trip using the downloaded executable. The
[feasibility matrix](https://github.com/c4us4l1ty/perveance/actions/runs/41986111831)
also passed its configured checks. These results are not real-desktop or
production-sandbox certification, and do not authorize generated-code execution.

## Inventory and remaining acceptance

| Area | Current state | What still closes the requirement |
| --- | --- | --- |
| Manual pixel art | Implemented, with automated native X11 coverage | Real supported-desktop use, accessibility and usability checks; address findings rather than claim full artist-tool parity. |
| Headless/Arena handoff | Rust CLI plus separate limited Python bridge; file handoff tested | Keep compatibility and limits explicit. A live Arena/local-GUI connector is not a prerequisite and is not available. |
| Work protection | No-clobber version copies and opt-in recovery snapshots | Stable project compatibility/migration policy, meaningful conflict/recovery tests, backup/retention UX and documented durability limits. No current atomic cross-process CAS. |
| Prompt-to-pixels | Opt-in typed OpenRouter prototype | Real model-route compatibility, cancellation/progress, secure credential handling and explicit billing controls. It is not a coding harness. |
| Durable task orchestration | Legacy data-only jobs plus [fixed offline pixel-worker jobs](WORKER_TASKS.md), independently verified publication and crash recovery | Integrate GUI/provider/harness tasks and general output contracts. Legacy tasks still have no quotas; fixed worker jobs do not authorize generated code or real providers. |
| Foundation decision | Working composed Rust raster engine; scoped Graphite research/headless-check evidence | Record the reversible foundation decision and remaining Graphite rendering/integration tradeoffs. A successful headless check is not a desktop integration spike. |
| Linux isolation | Fixed-code bubblewrap/staging diagnostics plus a new [delegated cgroup-v2 quota/watchdog backend](LINUX_WORKER.md) with twelve passing worker integration + two task kernel cases and [descriptor I/O](WORKER_IO.md) | Integrate and adversarially verify quotas/descendant cleanup in the actual supported build/runtime pipeline, trusted input brokerage, secret/network/registry denial and crash/retention policies. |
| Model broker and coding harness | New [offline broker/ledger component](MODEL_BROKER.md); real harness/route missing | Isolate the actual broker/worker roles and integrate durable calls into tasks; retain response recovery and conservative uncertainty behavior. Verify provider billing/token bounds and one real compatible harness; no paid autonomous route is enabled. |
| Extensible document/render contract | Verified [data-only v2 foundation](DOCUMENT.md); legacy v1 stays strict raster | Complete the protected extension-specific validation, typed editing/render ABI, confined resource-bounded dispatch and migration/rollback contract. Opaque preservation/native edits are not a working extension. |
| Capability registry/lifecycle | Verified [quarantine-only immutable storage](REGISTRY.md); [static WASM diagnostic](MODULE_AUDIT.md) implemented, exact-head CI pending; no active/installed capability | Protected independent module/ABI/behavior verification and provenance, confined execution, trusted install/stage/activate records, crash-safe rollback and recoverable project migrations. Candidate code must not bless itself. |
| Autonomous build/reuse | Missing and disabled | Genuine vector-absent baseline, independently verified build/activation, editable saved/reopened vector artwork plus PNG, then a distinct task reusing the same version without another build. |
| Distribution | Unsigned x86-64 Linux evaluation artifact, tested after download | Runtime/desktop compatibility, full dependency/asset/license review, owner license choice, signing/provenance/update policy and release approval. |

## Critical path, in build order

### 1. Durable data-only job lifecycle — legacy backend

- [x] Capture a validated project and revision-bound typed proposal into a new
  private Linux job directory; never reread the original source paths at run.
- [x] Persist bounded, versioned, append-only state events and expose job IDs,
  status and artifact metadata through the Rust CLI.
- [x] Serialize cooperating runners; record cancellation intent; check it
  between fixed raster stages without pretending to kill a process.
- [x] Publish success only after editable-project/PNG verification. Keep failed
  or interrupted outputs unpublished and never replay a terminal task.
- [x] Provide explicit interrupted-run recovery and test simulated interruptions,
  forced CLI termination, concurrent runners, malformed records, occupied
  outputs and tampered files.
- [x] Verify the task backend in GitHub CI, including the downloaded binary
  ([run 41986111833](https://github.com/c4us4l1ty/perveance/actions/runs/41986111833)).
- [ ] Connect task status/cancellation to the GUI and provider adapter. This
  increment is CLI/library-only and accepts proposals, not natural-language
  creative tasks. The old autonomy intake inspection remains always blocked.

### 2. Verified worker boundary and broker

- [ ] Choose the concrete supported Linux backend/version/prerequisites; a
  developer's CI probe must never automatically approve an end user's host.
- [x] Implement a fixed-code cgroup-v2 quota/independent-watchdog component,
  descriptor-based attachment, bounded scratch and explicit cgroup recovery.
- [x] Validate its six real-kernel tests on the delegated CI service
  ([run 41993520272](https://github.com/c4us4l1ty/perveance/actions/runs/41993520272),
  `b7be7f2`); see [LINUX_WORKER.md](LINUX_WORKER.md). Protocol mocks are not proof.
- [ ] Integrate and adversarially verify quotas/descendant cleanup in the actual
  builder/runtime pipeline, including interruption, watchdog loss and retention.
- [x] Implement [descriptor-scoped raster I/O](WORKER_IO.md): confined reads,
  sealed stdin snapshots, trusted expected-artifact comparison and no-replace
  project/PNG bundle publication. No worker receives a source directory FD.
- [x] Validate this I/O increment at `36963cc`: 18 new Rust I/O tests/full
  regression/downloaded Linux bundle ([run 42006495225](https://github.com/c4us4l1ty/perveance/actions/runs/42006495225))
  and seven real worker-kernel tests on the delegated service
  ([run 42006494788](https://github.com/c4us4l1ty/perveance/actions/runs/42006494788)).
- [ ] Complete role-separated input/output transport and minimal permissions for
  fetch, build, runtime and provider calls. Existing task/GUI loaders are not
  retroactively brokered. No host sockets or inherited secrets.
- [x] Implement the scoped [broker/ledger component](MODEL_BROKER.md): host-owned
  credentials, bounded connected-socket requests, immutable route/budget policy,
  exclusive durable reservations before dispatch, no refunds/retries and halted
  uncertain outcomes. Only an offline fixture transport is shipped.
- [x] Validate the broker's 14 unit/eight integration cases and downloaded CLI
  at `258cc80` ([Rust run 42011716176](https://github.com/c4us4l1ty/perveance/actions/runs/42011716176));
  the seven fixed-worker tests also passed regression
  ([run 42011716112](https://github.com/c4us4l1ty/perveance/actions/runs/42011716112)).
  Offline fixtures are not paid-provider or protected broker-isolation proof.
- [x] Connect the host offline broker to a confined fixed-code peer through
  sealed stdin/duplex stdout only, with leased admission and no secret/ledger
  mounts. See [BROKER_WORKER.md](BROKER_WORKER.md); no real provider is enabled.
- [x] Validate the four new broker/kernel cases (eleven total), revocation tests
  and CLI integration at `cf1fc2c`: [Rust/full bundle 42017625949](https://github.com/c4us4l1ty/perveance/actions/runs/42017625949)
  and [delegated matrix 42017625633](https://github.com/c4us4l1ty/perveance/actions/runs/42017625633).
  This verifies fixed offline code only, not the production role/task gates below.
- [x] Implement a separate [fixed pixel worker-task backend](WORKER_TASKS.md)
  connecting captured typed proposals, persisted limits, the offline broker,
  bounded candidate output and independently verified atomic publication. Legacy
  tasks/GUI/intake are unchanged; this is not generated-code or real AI work.
- [x] Validate twelve mandatory worker integration plus two task kernel cases,
  including active cancellation and owner death before/after publication at
  `61a0f20`: [matrix 42022046128](https://github.com/c4us4l1ty/perveance/actions/runs/42022046128)
  and [Rust/native/downloaded bundle 42022046445](https://github.com/c4us4l1ty/perveance/actions/runs/42022046445).
- [ ] Complete protected production broker/worker roles, natural-language/provider/
  harness task integration, response recovery and verified billing/token/deadline controls. No
  keys in worker files/env/args; do not treat a declared bound as billing proof.
- [ ] Test a real compatible harness through that broker. If its auth/network
  needs cannot meet the contract, reject it rather than weakening isolation.

### 3. Document/capability contract, independent verification and activation

- [x] Verify the [data-only v2 document foundation](DOCUMENT.md): stable
  document/layer/object IDs, bounded opaque byte preservation, revision-bound
  native edits, atomic private file publication and explicit unavailable-content
  rendering refusal. At `8593268`, all-target/debug-CLI/native/downloaded-bundle
  checks passed ([42030894898](https://github.com/c4us4l1ty/perveance/actions/runs/42030894898));
  fourteen mandatory kernel cases also passed ([42030894888](https://github.com/c4us4l1ty/perveance/actions/runs/42030894888)).
- [ ] Complete the generic protected extension validation/typed-edit/render
  contract before attempting the vector experiment. V2 does not yet execute or
  semantically validate any capability. Do not preinstall vectors and call
  their use self-extension.
- [x] Verify [immutable quarantine storage](REGISTRY.md): bounded manifests and
  captured artifacts, exact ID/version/pin lookup, duplicate/conflict handling,
  private fixed-member atomic publication, storage budgets and interruption
  refusal. At `9955839`, all-target/debug/downloaded-CLI/native regression passed
  ([42048502060](https://github.com/c4us4l1ty/perveance/actions/runs/42048502060));
  fourteen mandatory kernel cases also passed ([42048502059](https://github.com/c4us4l1ty/perveance/actions/runs/42048502059)).
- [ ] Validate the new [static WASM diagnostic](MODULE_AUDIT.md) on the exact head:
  full structural/body validation, bounded memory/export/signature profile,
  frozen read-only exact-slot binding, negative fixtures and debug/downloaded
  executable lifecycles. This is not independent capability verification or
  activation authority; even useless stubs/infinite loops can pass.
- [ ] Complete capability verification/provenance and lifecycle beyond quarantine.
  A captured SHA-256/header/storage receipt is not a wasm/ABI/behavior test or
  an install/activation permit. All registry entries remain unavailable.
- [ ] Stage and activate under protected supervisor control, preserving prior
  versions; test failure/crash points and rollback plus project backup/reopen.
- [ ] Keep runtime candidate code confined even after tests pass. A signature
  or passing candidate tests is not permission for unrestricted host execution.

### 4. Original end-to-end acceptance and release

- [ ] First request genuinely adds a missing persistent vector capability,
  produces editable objects, saves/reopens them and exports a verified PNG.
- [ ] A second distinct request discovers and reuses the exact installed
  capability without another build. Record actual logs, duration and spend.
- [ ] Malicious candidate, cancellation, exhausted budgets, incompatible schema
  and interrupted activation leave previous versions and projects usable.
- [ ] Validate GUI/headless output parity and the actual supported desktop.
- [ ] Close licensing, runtime dependency, signing, update and release-review
  decisions before calling the evaluation bundle a supported release.

## External decisions and validation that cannot be fabricated

- A real OpenRouter/provider test needs credentials configured through a secure
  local mechanism and a permitted endpoint on the target environment. Never
  request keys in chat. The development sandbox's network policy is not proof
  that a user's provider/harness route works.
- Product licensing, distribution/signing policy and supported-platform
  promises need explicit owner/reviewer decisions.
- Real-desktop interaction and independent security review are distinct from
  virtual-X11 tests and fixed diagnostics; report them separately.

Windows/macOS security backends, full animation/spritesheet workflows, image
model integrations, a marketplace and broad 2.5D support remain deferred. They
are not prerequisites for the scoped first autonomous-growth demonstration.
