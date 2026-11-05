# Confined offline broker/worker integration

**2026-11-05 — fixed synthetic diagnostics, not a production autonomy permit.**
This connects the [model broker](MODEL_BROKER.md) to the existing
[cgroup/bubblewrap worker](LINUX_WORKER.md). It does not add a real provider,
generated-code runner, coding harness, capability installer or raster-task/GUI
integration. The parent supervisor/broker remains trusted, outside the worker's
namespaces and cgroup. A separately hardened broker daemon is not introduced.

## Roles and descriptors

`bounded_worker::run_broker_fixture` captures an existing healthy offline ledger
through an authorized private root. This diagnostic requires a one-call policy
and a sufficient positive upper-cost budget; it never changes that policy. It
creates a fresh unpredictable **synthetic** credential, private host sentinel,
Unix socket and IPv4 loopback listener. Host-side positive controls open/connect
the actual targets before the peer's negative tests. Only synthetic fixture
paths and the ledger pathname are intentionally disclosed for access-denial
probes; no credential value goes into args, environment, prompt or replies.

The fixed Python peer uses the same required user/PID/network/IPC/UTS/cgroup
namespaces, pre-exec attachment, quota/watchdog, empty root, read-only system
mounts and bounded scratch as previous worker diagnostics. It gets only:

- **stdin**: the full canonical sealed project snapshot, including hidden data;
- **stdout**: one already connected duplex Unix socket to its scoped broker;
- **stderr**: a bounded anonymous diagnostic-summary memfd.

No additional descriptors, ledger/source/credential directory mount or host
socket pathname is made accessible. stdout is intentionally a socket, not a log.
It is the sole declared IPC exception to omitted host sockets, not general
network access. The launcher drops its `Command`/worker-end Stdio handles after
spawn: retaining a parent copy would conceal peer EOF and invalidate revocation.
The broker waits for actual child spawn before servicing a request.

The fixed peer checks descriptor/environment hygiene, read-only sealed input,
empty `/input`, denial of real fixture/ledger paths, an escaping scratch symlink,
`/proc/1/root` and host-PID proc routes, the omitted host Unix socket and host
IPv4 loopback listener. The Unix-socket check proves omission, **not** that a
network namespace blocks an exposed pathname socket. These selected-path tests
are not an exhaustive syscall, secret, IPv6/network or sandbox-escape audit.

## Scoped protocol and independent evidence

The fixed inspect workload performs at most five framed exchanges:

1. Unknown endpoint override is rejected without a reservation.
2. A valid, scope-bound stable ID completes once (or is already finished).
3. Identical replay returns `already_finished`, never another dispatch/text.
4. Same-ID changed prompt is a conflict.
5. Another ID is denied by the immutable one-call ceiling.

Only the constant offline transport runs, using synthetic usage. No real key
can be supplied to this fixture API/CLI. The host separately checks service
statuses, adapter-entry count and final ledger state against the peer summary;
peer claims alone cannot substitute for that evidence. Successful reports
require normal worker completion and verified whole-cgroup removal. Reports
explicitly keep `synthetic_fixture_only:true`, `provider_network_used:false`,
`autonomous_execution:false` and `security_boundary_verified:false`.

## Cancellation, lost peers and interrupted supervision

The broker's new trusted `serve_guarded` entry checks admission **before
reservation**, **after reservation sync before adapter entry**, and **after
settlement before text release**. The fixture's lease combines a monotonic
scope deadline, cancellation, terminal/error revocation and actual socket
liveness. EOF/half-close revokes even if a complete request is buffered; extra
queued bytes cannot renew an expired or dead scope. Read/write retain their
absolute bounded protocol deadlines.

Normal/error/unwind paths revoke before joining the broker service. Main-loop
cancellation or quota/watchdog failure revokes and kills the entire worker
cgroup. Independent watchdog lease EOF covers supervisor SIGKILL; its monotonic
wall timer still kills the peer while the supervisor/broker is SIGSTOPped. On
resume, expired scope/peer EOF denies a stale reserved request instead of
starting a delayed adapter call. Cgroup recovery remains explicit and separate
from accounting: it never deletes or resets the ledger.

Fixed library-only hold modes stop at synced reservation or inside the fake
adapter. They support tests, not worker-selected delays/commands. Diagnostic
`BrokerProgress` is an in-memory observation, not a durable task state or permit.
Revocation after reserve retains the **entire** reservation and records an
unknown outcome when possible. A killed broker may leave `reserved_uncertain`.
Neither state allows a new dispatch, refund or automatic retry. Revocation after
a completed settlement withholds text but cannot make the bill/reservation free;
a same-ID retry still has no cached text.

These are checkpoints, not an atomic cancellation guarantee with a remote
provider. Work already entered may be billable, and the real adapter still needs
its own bounded network/read/cancellation contract. Host filesystem I/O, broker
parsing/threads and the supervisor are outside worker quotas; uninterruptible I/O,
sampling overshoot and simultaneous supervisor/watchdog loss remain limitations.

## CLI, retention and prerequisites

On an **already provisioned** compatible delegated host, run without sudo:

```sh
mkdir -m 700 broker-ledger
perveance-cli broker-ledger-init /absolute/broker-ledger 1 10 10
perveance-cli verify-broker-worker PRIVATE_INPUT_ROOT RELATIVE_PROJECT DELEGATED_CGROUP_ROOT /absolute/broker-ledger
```

The input root/project obey WORKER_IO.md; the ledger obeys MODEL_BROKER.md. Reusing
the healthy completed ledger tests deduplication without another adapter entry.
Missing prerequisites or unexpected probe results fail closed, never run the
peer on the host or enable an execution gate. The packaged CLI exposes the
command, but a downloaded-bundle offline check cannot attest kernel support.

Unlike anonymous artwork/log transport, this diagnostic deliberately creates a
private temporary directory of **synthetic credential/sentinel/socket fixtures**.
Ordinary return removes it. SIGKILL can leave `perveance-broker-probe-*` directories
under the trusted temporary parent. There is no automatic sweep/retention policy;
operators must establish that a fixture is abandoned before manually removing
it. Never sweep ledger roots, original projects or arbitrary matching trees.
No real credentials or artwork are copied into those fixture directories.

## Verification and open gates

New unit tests cover the three admission checkpoints, buffered EOF/half-close,
cancellation/deadline revocation. Python protocol/negative controls verify that
exposed host files/listeners really fail the denial probe and that framing/errors
remain bounded/redacted. Four new delegated tests require actual confined
exchanges plus CLI/reuse, cancellation before/in dispatch, owner SIGKILL and owner
SIGSTOP → independent watchdog kill → SIGCONT with **zero stale dispatches**.
The mandatory CI suite now requires eleven real kernel tests, not seven.

These new Rust/kernel checks are pending CI. All 47 local Python tests pass;
there is no local Rust toolchain. Historical seven-test passes do not validate
this new integration.

Actual task/output transport, durable call-response recovery, protected fetch/
build/runtime roles, hardened broker lifecycle/credential stores, one verified
real route/harness/billing contract and independent adversarial review remain
open. Fixed code passing here is not authority to run an untrusted extension.
