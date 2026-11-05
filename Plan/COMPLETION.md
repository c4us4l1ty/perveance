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
| Durable task orchestration | Implemented and CI-tested Linux raster-proposal CLI/library backend | Integrate the GUI/provider and later verified worker pipeline. Existing offline tasks do not authorize code, make model calls or enforce OS quotas. |
| Foundation decision | Working composed Rust raster engine; scoped Graphite research/headless-check evidence | Record the reversible foundation decision and remaining Graphite rendering/integration tradeoffs. A successful headless check is not a desktop integration spike. |
| Linux isolation | Fixed-code bubblewrap/staging/FD/lifecycle diagnostics | Enforce and adversarially verify CPU, memory, processes, total disk, wall time, descendant cleanup, trusted input brokerage and denial of secret/network/registry access on the actual supported backend. |
| Model broker and coding harness | Missing | Broker owns credentials and durable reservations; worker has scoped calls but no provider key or general network. Prove one real harness/provider pair. |
| Extensible document/render contract | Missing; v1 is a strict raster format | Stable IDs, bounded versioned capability payloads, typed editing/render mediation, preservation of unavailable payloads and explicit failure when rendering cannot be correct. |
| Capability registry/lifecycle | Missing | Immutable versioned artifacts, compatibility/provenance, independent verification, install/stage/activate, crash-safe rollback and recoverable project migrations. Candidate code must not bless itself. |
| Autonomous build/reuse | Missing and disabled | Genuine vector-absent baseline, independently verified build/activation, editable saved/reopened vector artwork plus PNG, then a distinct task reusing the same version without another build. |
| Distribution | Unsigned x86-64 Linux evaluation artifact, tested after download | Runtime/desktop compatibility, full dependency/asset/license review, owner license choice, signing/provenance/update policy and release approval. |

## Critical path, in build order

### 1. Durable data-only job lifecycle — current implementation increment

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
- [ ] Implement actual quotas and descendant cancellation/cleanup; test each
  denial/failure case independently, including interrupted supervision.
- [ ] Add race-resistant, minimal input/output mediation and isolated roles for
  fetch, build, runtime and provider calls. No host sockets or inherited secrets.
- [ ] Build the credential broker with durable call/cost reservations, bounded
  requests and conservative billing behavior. No keys in worker files/env/args.
- [ ] Test a real compatible harness through that broker. If its auth/network
  needs cannot meet the contract, reject it rather than weakening isolation.

### 3. Document/capability contract, independent verification and activation

- [ ] Implement the generic bounded extension payload/edit/render contract
  before attempting the vector experiment; do not preinstall vectors and call
  their use self-extension.
- [ ] Implement immutable capability manifests/artifacts, version lookup and
  compatibility checks, trusted independent tests and provenance.
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
