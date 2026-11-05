# Bounded Linux worker increment

**2026-11-05 — development backend, fixed code only.** This implements resource
mechanisms, not the complete autonomous-execution security gate. It neither
accepts generated programs nor connects workers to raster tasks, providers,
registries or capability activation. `inspect-autonomy-request` stays blocked.

## Concrete validation target and prerequisites

The development validation target is **x86-64 Ubuntu 26.04**, unified cgroup v2,
a non-setuid/non-file-capability `/usr/bin/bwrap`, and trusted system
`/usr/bin/python3`. The CI fixture uses systemd's `Delegate=yes` and
`DelegateSubgroup=supervisor` (systemd 254+) to run the test executable as the
ordinary runner user. Privilege provisions that disposable service only. No
AppArmor/sysctl relaxation, shared host networking, root application execution or
unconfined fallback is permitted.

The implementation checks actual features rather than treating an OS label or
previous CI result as authorization. Required kernel interfaces include
`close_range(CLOEXEC)`, cgroup-v2 `cgroup.kill`, `memory.peak`, memory/swap and pids
controllers, and `cpu.max`. Bubblewrap must support the requested namespaces,
`--disable-userns`, `--size` and read-only mounts. A failed namespace setup is a
failed diagnostic, not permission to omit it. These requirements do **not**
establish a production-supported Linux distribution/version matrix.

A trusted caller supplies an **absolute, owned, mode-0700 delegated root** with
no processes directly in it and `cpu memory pids` already enabled in
`cgroup.subtree_control`. Put the supervisor in a sibling leaf beneath that
root. Delegation must permit migration from the supervisor leaf to worker
leaves. The backend never discovers, mounts, enables or weakens host cgroups
itself. It refuses ordinary directories, final symlinks, threaded cgroups,
missing controls, root-user execution and cooperating concurrent supervisors.
Directory ancestors and system-tool installation remain trusted.

## What is enforced

| Mechanism | Behavior and limitation |
| --- | --- |
| Memory | `memory.max`, `memory.swap.max=0`, `memory.oom.group=1` apply to bubblewrap and all worker descendants. Peak and OOM-kill counters are reported. This is cgroup accounting, not a cap on the supervisor/watchdog or all machine memory. |
| Processes/threads | `pids.max` covers helpers and descendants; fork denials are detected through kernel counters. No nested worker cgroups are allowed. |
| CPU rate | `cpu.max` enforces the configured percentage of one CPU over a 100-ms period. This is a rate limit, **not** a total-CPU-seconds limit. |
| CPU budget | A separate watchdog samples aggregate `usage_usec` and kills the cgroup at the configured budget. Sampling/scheduling cause overshoot; there is no exact hard accumulated-CPU cap. |
| Writable scratch | One size-limited tmpfs is reachable through `/work`, `/tmp` and `/dev/shm`. Input is read-only; root/system files are read-only. Minimal device nodes replace bubblewrap's writable `/dev` tmpfs/pty setup. This bounds this scratch filesystem, not global host disk or abandoned snapshots. |
| Captured output | Each of two private anonymous regular files has an 8-KiB file-size rlimit; the supervisor rejects output over 4 KiB. Captured bytes are never returned or echoed as errors. The file limit is not an aggregate scratch quota. |
| Wall time / loss of supervisor | An independently scheduled fixed Python watchdog lives outside the worker cgroup. It kills the **entire** cgroup on the wall deadline or lease-pipe EOF. Cancellation, output overflow and watchdog failure also trigger whole-cgroup cleanup while the supervisor is alive. |

Limits are deliberately small for diagnostics: wall and sampled CPU budgets
1–30 s, CPU rate 1–100%, memory 32–512 MiB, scratch 1–64 MiB, and 8–32 processes.
Defaults are 10 s wall, 2 s CPU, 50% CPU, 64 MiB RAM, 4 MiB scratch and 12 tasks.
The supervisor/watchdog, host-staged input and retained crash files are outside
these worker quotas. The data-only raster task backend is still separate and
has **not** acquired these limits.

## Launch, cleanup and recovery protocol

1. Hold an exclusive advisory lock on the delegated root. Refuse a new run if a
   strictly named abandoned worker cgroup exists; recovery is explicit.
2. Create an unpredictable `perveance-worker-` + 32-lowercase-hex leaf. Write and
   read back its kernel limits **before** launch. Open required counters/kill
   control and verify that the new group is empty.
3. Stage one validated project under a separate private trusted directory. Its
   full contents, including hidden pixels/names, are disclosed to the worker.
4. Start the embedded watchdog with only its lease pipe, protocol pipe and one
   explicit cgroup-directory descriptor. Wait for its readiness byte. The
   watchdog gets no model/provider credentials or worker-supplied code.
5. In the worker child's async-signal-safe `pre_exec`, set parent-death handling,
   no-new-privileges, descriptor hygiene and rlimits, then write `0` through a
   held `cgroup.procs` descriptor. Only then exec bubblewrap. No candidate can
   fork before attachment; no cgroup/control descriptor survives worker exec.
6. Bubblewrap denies networking, drops capabilities and further user namespaces,
   stages read-only input, and exposes only the bounded scratch for file writes.
   The diagnostic is an embedded fixed script, never an arbitrary command.
7. Cleanup uses `cgroup.kill`, not a process-group signal or a list of sampled
   PIDs. Detached/session-changing descendants are included. After lease
   revocation the watchdog confirms `populated=0`; the supervisor independently
   confirms emptiness before removing the cgroup and normal staging.
8. If the supervisor dies, the watchdog kills descendants but deliberately
   leaves the cgroup and any abandoned staging for explicit recovery/retention.
   `recover-bounded-workers` kills/removes only exactly named owned leaf cgroups.
   It never recursively cleans foreign groups or deletes artwork files.

The watchdog keeps issuing kills after a budget/deadline until the lease closes;
a single kill of an initially empty cgroup would have a late-launch race. A
worker fork still holding the close-on-exec lease write end cannot cause an
early EOF before its attachment/exec. The supervisor holds no arbitrary-code
hook in its pre-exec path.

No real-time or uninterruptible-kernel-I/O guarantee is made. Failed emptiness,
watchdog or removal checks are errors, not successful cleanup. Kernel limits
remain on an abandoned group, but simultaneous loss of both supervisor and
watchdog can lose deadline enforcement; explicit recovery and an independently
managed production watchdog policy still need design/review. Cooperative locks
and private paths do not protect against a malicious same-user host process.

## CLI (developer diagnostics, not creative tasks)

Given an **already provisioned** delegated root and a separate private staging
parent, run the compiled CLI **without sudo**:

```sh
perveance-cli verify-bounded-worker INPUT PRIVATE_PARENT DELEGATED_CGROUP_ROOT
perveance-cli recover-bounded-workers DELEGATED_CGROUP_ROOT
```

An optional final fixed mode is `inspect`, `wait`, `fork-tree`, `cpu`, `memory`,
`processes`, `scratch`, `output` or `fail`. These are deliberate failure/limit
fixtures. CLI `ok` means the requested diagnostic behaved as expected (including
an expected quota failure), **not** that artwork completed or autonomy is safe.
Reports include limits, observed usage, outcome and verified cgroup removal;
they explicitly report `autonomous_execution:false` and
`security_boundary_verified:false`.

## Verification and open gates

- Local Python tests exercise the watchdog's actual descriptor/pipe protocol,
  malformed counters, EOF, repeated kills and deadline behavior against ordinary
  fixture files. **Mock files are not evidence of kernel enforcement.**
- Normal Rust/CLI tests reject forged ordinary-file cgroup attestations, bad
  limits, unknown modes and unsupported roots without staging/launch fallback.
- Six opt-in Rust integration tests on the delegated CI service require actual
  memory, process, scratch and CPU-budget limits; read back kernel rate/other
  limits; verify descriptor hygiene/read-only input and project preservation;
  cancel detached descendants; kill/freeze the supervisor; kill the watchdog;
  and recover only owned managed groups. Missing prerequisites fail the selected
  CI job instead of passing/skipping the requirement.

The new Rust/kernel checks are **pending first CI validation** at implementation
time. Even a pass does not close trusted race-resistant input/output brokerage,
broader malicious-code/syscall/secret/network threats, durable task integration,
credential brokerage, real harness compatibility, independent verification,
activation/rollback or production security review. Arbitrary code stays disabled.
