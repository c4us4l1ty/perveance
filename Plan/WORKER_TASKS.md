# Durable fixed pixel-worker jobs

**2026-11-05 — fixed, offline, CLI/library-only continuation.** This is a separate
`worker_tasks` backend, not a silent upgrade of legacy `tasks`, the GUI, provider
prototype or autonomy intake. It does not add a coding harness, new creative
capability, real provider, generated code, capability installation or autonomy.

## Actual work and independent authority

A job captures an entire validated raster project and a revision-bound proposal
containing **1–1024 Pixel operations only**. Unsupported operations, stale inputs
and invalid persisted limits fail closed. The host independently computes the
expected project with the compiled Rust raster model. It never accepts a
candidate's claimed expected revision, success flag or self-written test.

The same required cgroup/bubblewrap backend executes an embedded fixed Python
peer. stdin is a kernel-sealed, read-only `{version,project,proposal}` payload,
not arguments, environment, a source mount or directory FD. The peer performs
five offline broker policy/dedup/exhaustion exchanges using one irreversible
synthetic reservation, applies the captured pixels itself, and returns exactly
one length-prefixed candidate project followed by EOF on the connected socket.
The host does **not** send the expected edited result to be echoed back. The
constant broker response is not parsed as a proposal or executed.

A candidate length over 40 MiB is rejected before allocation; partial frames,
trailing bytes, cancellation and absolute scope expiry fail. Unlike admission,
bounded artifact bytes already queued at normal peer exit may be read after EOF:
this is data consumption, not renewal of dispatch authority. All worker
confinement/credential/environment/host-listener checks remain enabled. No
relaxation of syscall policy, host mounts or quotas is introduced.

After normal worker exit and whole-cgroup removal, Rust requires full project
equality with the independently computed expected result, including hidden
layers, names and transparent RGB. It then renders the PNG in the trusted host,
seals canonical artifacts and publishes both through the existing no-replace
atomic bundle publisher. This is **worker-edited project / host-rendered PNG**,
not a claim of a worker renderer or arbitrary-code verifier. Host parsing,
verification/rendering, filesystem sync and broker threads remain outside worker
quotas; input sizes are bounded, but whole-host resource/time hardening is open.

## Storage and lifecycle

All subsequent task I/O is through held `PrivateDirectory` capabilities and
mandatory `openat2` constraints. No original source/proposal pathname is saved.
The job has an owned 0700 directory, private single-link files, captured inputs,
an immutable one-call offline ledger, a separate cooperative flock, and one
bounded append-only journal (16 KiB, at most five 3072-byte JSON lines).

```
queued -> running -> prepared -> succeeded
   |         |           |
 cancelled   +-> failed/cancelled/interrupted
                         +-> failed/cancelled/interrupted
```

`prepared` is synced **only after** successful worker/quota/broker evidence and
independent output verification. Publication is the no-replace rename of the
fixed `result` directory. `succeeded` follows parent sync and a durable final
event. Reports never advertise prepared/failed/interrupted results as completed
outputs. Reporting success re-syncs the journal and directory, rechecks inputs,
re-reads/verifies both actual artifacts and checks canonical byte hashes and the
actual completed ledger. This is a snapshot check, not protection from later
trusted-host edits or an authenticated anti-rollback store.

The exact backend device/inode identity is recorded at running and checked again
against the actual held backend before worker launch. Recovery requires the same
identity. A pathname cannot silently redirect the recorded execution backend.
Cooperating task runners and recovery serialize on the task flock; the existing
backend root lock also excludes active backend supervisors.

Cancellation is a private synced marker. A host watcher observes it during the
worker/broker lifetime, revokes admission and kills the entire worker cgroup.
Invalid markers also stop the scope. The marker is checked again before verified
publication. A request racing the commit can lose to success; cancellation is
not rollback, a refund or retroactive proof of zero dispatch/billing.

Terminal jobs are never replayed. Succeeded reruns only re-verify existing
artifacts. A running job whose owner died requires explicit whole-backend orphan
recovery; it becomes interrupted, never retried. A prepared job with an **already
visible, fully verified** fixed result may be finalized by recovery without
another worker/provider call. If no committed result exists it becomes
interrupted, without republishing or rerunning. Occupied/unverified results are
never overwritten/deleted. Uncertain worker/cleanup errors leave the task
running for explicit recovery, not a misleading terminal cleanup claim.

The broker ledger is separate from the job journal: recovery never deletes,
resets, refunds or retries it. Cancellation in flight retains the entire call/
cost reservation and halted unknown outcome. A truncated/malformed journal fails
closed; there is no automatic truncate/repair. Host rollback/copy/tail removal,
malicious same-user supervisors, OS/filesystem compromise and cryptographic
provenance are not solved by this store.

## CLI

Create private roots as the normal user; the source/proposal files must be
private, owned and single-link. On Linux:

```
worker-task-create PRIVATE_INPUT_ROOT RELATIVE_PROJECT PRIVATE_PROPOSAL_ROOT RELATIVE_PROPOSAL PRIVATE_TASK_ROOT
worker-task-inspect PRIVATE_TASK_ROOT ID
worker-task-run PRIVATE_TASK_ROOT ID DELEGATED_CGROUP_ROOT
worker-task-cancel PRIVATE_TASK_ROOT ID
worker-task-recover PRIVATE_TASK_ROOT ID RECORDED_DELEGATED_CGROUP_ROOT
```

Creation uses default validated worker limits and a fixed 1-call/10-synthetic-unit
offline policy. Trusted Rust callers may select validated limits **at creation**;
run cannot increase them. Run/recovery require an already provisioned compatible
backend, never sudo or an unconfined fallback. A successful CLI invocation can
report a failed/cancelled job; clients must inspect `task.state`, not just `ok`.
An active broker may hold the ledger lock: running inspection can explicitly
report unknown accounting, never spare call authority. No automatic scheduler,
GUI/progress UI, user-retention controls or real credential input is added.

Interrupted create/publication can leave private job/stage directories. Existing
synthetic broker probe directories can remain after owner SIGKILL. There is no
automatic sweep; operators must establish abandonment before manual retention
cleanup. Never sweep original projects, active work or accounting ledgers.

## Verification status and remaining gates

Local: **50 Python tests pass**, probe compilation, shell syntax and diff checks
pass. No local Rust toolchain is installed. New Rust/kernel validation is pending.
CI now requires **12 worker integration kernel tests and two library kernel
cases**, including full >8-KiB input/output, hidden-data preservation, downloaded
source capture/CLI reuse, occupied-output preservation, active broker cancellation,
flock exclusion, and real owner SIGKILL before/after atomic publication. Protocol
units also cover oversized/partial/trailing candidate frames and independent
rejection of changed hidden data. Downloaded binaries exercise create/inspect/
unsupported-backend refusal/cancel without claiming kernel support on that host.

Provider pricing/token/deadline and response recovery, protected fetch/build/
runtime roles, real harness compatibility, general document/capability contracts,
immutable registry/independent verification/activation, genuine missing-feature
build-and-reuse acceptance, GUI task integration and independent review remain
open. Every report keeps real-provider use and production autonomy disabled.
