Perveance experimental Linux build
===================================

This is an unsigned test build of the early Perveance pixel-art prototype,
not a stable release. Back up important work. It does not run generated code
or enable autonomous capability development.

Requirements: x86-64 Linux with a compatible glibc and X11/GL runtime. The
build environment's glibc version and source revision are in BUILD-INFO.txt.
It was smoke-tested in a virtual X11 session on the build runner, not on all
Linux distributions, graphics drivers, or desktop environments. Newer library
versions are often compatible, but compatibility is not guaranteed. Windows,
macOS, ARM and native Wayland are not supported by this bundle.

To launch from a terminal in a writable directory on an X11 desktop:
  /path/to/perveance-linux-x86_64/bin/perveance

To open an existing UTF-8 project path directly:
  /path/to/perveance-linux-x86_64/bin/perveance /path/to/project.json

Use Open or Ctrl+O to choose and open a project; unsaved artwork requires
confirmation before replacement. Escape cancels the chooser or confirmation.
Use Browse beside a file or directory field to select a path. Use path only
fills the field; opening, saving, importing and exporting need separate actions.
This is an in-app browser, not a native OS dialog or security boundary. Large
directories may require manual path entry. Non-UTF-8 paths are not supported;
special-file entries are omitted and opening special files may block.

Tool shortcuts: B Pencil, E Eraser, G Fill, R Rectangle, O Ellipse, L Line,
I Eyedropper, and M Select region. In Select mode, C copies, X cuts, V pastes,
Delete clears and Escape deselects. This uses an internal session buffer, not
the OS clipboard; paste replaces raw RGBA without blending or clipping.
The Tools panel has a shortcut reference. Shortcuts are
ignored while typing, in dialogs and during active drawing gestures.

Use Save or Ctrl+S to create the first file (using the GUI's File path field),
or a numbered version after edits. Existing files are never overwritten.
Ctrl+S is ignored in text fields, dialogs and active drawing gestures.

Optional recovery checkpoints are off by default. To enable them, select an
existing trusted directory in the GUI's Recovery checkpoints section, or start:
  /path/to/perveance-linux-x86_64/bin/perveance --recovery-dir /path/to/directory
While enabled, use the top-bar Checkpoint button or F9 for an immediate
snapshot (ignored while typing, in dialogs, or during a drawing gesture).
Manual and automatic snapshots share the eight-file limit, skip identical
project snapshots, include hidden layers, and remain until manually removed.
Deduplication does not check that the old file still exists. Checkpoints do not
mark the project saved and are not a replacement for saving or backups; see the
main repository README for limits and recovery instructions.

The GUI uses the current working directory for its default project and PNG
paths. Save projects and exports in a directory you can write to. Project
and PNG saves never overwrite existing files. The project format is experimental;
version saves are not locking or automatic backup. Recovery snapshots are opt-in and best-effort.

To try the command-line editor from a writable directory:
  /path/to/perveance-linux-x86_64/bin/perveance-cli new 16 16 potion-base.json
  /path/to/perveance-linux-x86_64/bin/perveance-cli apply potion-base.json potion.json /path/to/perveance-linux-x86_64/examples/potion-operations.json
  /path/to/perveance-linux-x86_64/bin/perveance-cli export-png potion.json potion.png

Use new output filenames. Open potion.json in the GUI to edit the layers.
Run perveance-cli capabilities for supported commands. The independent Python
Arena bridge and its stricter limits are described in the repository README;
it is not included in this binary bundle.

Persistent raster jobs are available through the CLI on Linux:
  mkdir -m 700 raster-tasks
  perveance-cli task-create project.json proposal.json raster-tasks
  perveance-cli task-run raster-tasks TASK_ID
  perveance-cli task-inspect raster-tasks TASK_ID
Use task.id returned by task-create in place of TASK_ID. The proposal must be
version-1 and bound to the project's inspected revision. These jobs only run
compiled typed raster edits; no model or generated code runs. task-cancel is
cooperative between stages, not a process kill. task-recover marks an abandoned
running job interrupted without replaying it. Successful jobs retain verified
editable project/PNG outputs in raster-tasks/TASK_ID. Full snapshots, including
hidden artwork and partial outputs, remain until you remove them. There is no
global retention/disk quota, OS resource enforcement, or GUI/provider wiring.
Use trusted private local paths and see the repository README for full limits.

Separate developer-only verify-bounded-worker/recover-bounded-workers commands
require an explicitly provisioned private delegated cgroup-v2 root, compatible
unprivileged bubblewrap and system Python. Never run the application with sudo.
These run fixed diagnostics only, not generated code; they do not apply quotas
to raster jobs or OpenRouter. See Plan/LINUX_WORKER.md in the source repository.
Autonomous execution stays disabled.

OpenRouter integration is experimental and not live-validated. Paid requests
can incur charges, including when a request fails, and there is no spend cap.
Do not put credentials in chat, project files, or command-line arguments.

SHA256SUMS accompanies the archive, not this extracted directory. Verify it
before extraction using sha256sum -c SHA256SUMS in the downloaded artifact
directory. A checksum detects corruption, not the identity of the publisher;
this artifact is not signed. Download only from a GitHub Actions run you trust.
CI artifacts include a separate LICENSE-INVENTORY.json and checksum based on
Cargo metadata, outside this archive. It is an unverified review aid, not a
complete inventory of linked code, system libraries or assets, nor a license
or redistribution review. Local packaging does not generate the sidecar.
This artifact is provided for evaluation, not as a formal release.


Linux descriptor I/O adds publish-raster-bundle PRIVATE_INPUT_ROOT
RELATIVE_PROJECT PRIVATE_OUTPUT_ROOT BUNDLE_NAME for offline editable-project/PNG
publication in one new directory. Roots must be absolute, owned, 0700 and have no
symlink components; source files must be private and single-link. This runs the
trusted renderer in-process, without worker quotas or a provider. If committed
is true but parent_directory_synced is false, inspect that installed name; do not
blindly retry or delete it. Existing names are never replaced. The preferred
fixed-code diagnostic is verify-mediated-worker PRIVATE_INPUT_ROOT
RELATIVE_PROJECT DELEGATED_CGROUP_ROOT; it supplies sealed stdin, not host source
paths. Required Linux syscalls have no unsafe fallback. See Plan/WORKER_IO.md in
the source repository. Autonomy remains disabled.


Linux offline broker fixtures add broker-ledger-init PRIVATE_ROOT MAX_CALLS
MAX_COST_MICROUSD PER_CALL_UPPER_MICROUSD, broker-ledger-inspect PRIVATE_ROOT and
broker-fixture-call PRIVATE_LEDGER_ROOT PRIVATE_REQUEST_ROOT RELATIVE_REQUEST.
Roots must be owned absolute private 0700 directories, and request files private
single-link regular JSON files. These use a synthetic server-side key and no
provider network. Full reservations persist across calls/restarts, never refund,
and never redispatch the same ID. Check fixture.reply.status, not just CLI ok;
already_finished has no cached text, and uncertain outcomes halt new calls.
No real key is accepted by this fixture CLI. This ledger does not cover the
manual OpenRouter prototype or existing tasks/GUI/workers, and is not a real
provider spending guarantee. See Plan/MODEL_BROKER.md in the source repository.
Autonomy remains disabled.


verify-broker-worker PRIVATE_INPUT_ROOT RELATIVE_PROJECT DELEGATED_CGROUP_ROOT
PRIVATE_ONE_CALL_LEDGER_ROOT is a separate confined offline integration probe.
It requires an already provisioned supported cgroup/bubblewrap backend and a
healthy one-call fixture ledger. It accepts no real key and makes no provider
call. Credentials/ledger mounts are absent; stdin is sealed and stdout is scoped
IPC. Synthetic private probe files may remain after SIGKILL; cgroup recovery
never resets accounting. See Plan/BROKER_WORKER.md. This is not a production
broker, coding harness, raster task or autonomy permit.


worker-task-create/inspect/run/cancel/recover are separate fixed Pixel-only
worker jobs, not the old task-* store or an autonomous agent. Capture private
revision-bound proposals, use an already-provisioned delegated backend at run,
and inspect task.state for outcome. Worker edits are independently compared to
trusted Rust edits; host PNG and editable JSON publish atomically. Recovery never
replays a worker/provider or resets accounting. No real key/model code is accepted.
See Plan/WORKER_TASKS.md; downloaded offline checks do not attest kernel support.


Data-only v2 documents (Linux CLI/library only)
----------------------------------------------
document-import-raster/inspect/copy/apply/export-png take private descriptor-root
arguments; run an operation without arguments for exact usage. Roots must be
owned mode 0700; use mode 0600 single-link regular input files. New output names
are non-hidden ASCII basenames; outputs are mode 0600 and never replace existing
names. A publication with parent_directory_synced=false is already visible but
crash durability is uncertain; retain it and inspect, do not blindly overwrite.

V2 preserves stable IDs and bounded arbitrary opaque bytes, permits native pixel
and layer-wrapper edits, and reuses the existing raster renderer. Any unavailable
capability layer blocks export, even hidden/empty. Nothing is fetched, installed,
loaded as code or activated on open. examples/opaque-document-v2.json is only an
UNAVAILABLE-data preservation/refusal fixture; it is not a vector plugin. GUI,
bridge, provider and both job formats remain v1-only and reject v2. No registry,
extension execution or autonomy is supplied. See Plan/DOCUMENT.md in the source
repository for schema/limits and verification status.


Quarantine registry (storage only, not a plugin installation)
------------------------------------------------------------
registry-init/inspect/quarantine/resolve-document use private Linux descriptor
roots. Run an operation without arguments for exact usage. Entries have strict
bounded declarations and captured artifact hashes under immutable ID/version
slots. A wasm header/hash match is NOT a section/import/export/ABI/behavior or
provenance test. Every entry stays unverified, uninstalled, inactive, unavailable;
no verify/install/activate/run command exists. Document export still refuses any
capability layer. Interrupted stages block admission and require retained-data
operator review, never automatic reset/sweep. Parent-sync=false means an already
visible package with uncertain crash durability. See Plan/REGISTRY.md in the
source repository for limits, failure behavior and verification status. No code,
provider or autonomy is enabled by quarantine storage.
