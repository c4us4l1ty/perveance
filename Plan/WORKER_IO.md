# Descriptor-scoped raster I/O increment

**2026-11-05 — Linux building blocks, not autonomous execution authority.**
This extends the fixed worker and adds offline bundle publication. Provider,
fetch/build/runtime roles, credential brokerage, activation and general
untrusted-code execution are still missing/disabled.

## Capabilities instead of repeatedly checked pathnames

`worker_io::PrivateDirectory::open` captures an owned mode-0700 directory
object. Its absolute bootstrap path must have no symlink components. The caller
must authorize that object and filesystem; an arbitrary model-supplied root is
not a permission grant. Subsequent reads/writes use the held directory FD, not
the original pathname. Renaming/replacing an ancestor cannot redirect them.

Relative reads require Linux `openat2` with `RESOLVE_BENEATH`,
`RESOLVE_NO_SYMLINKS`, `RESOLVE_NO_MAGICLINKS` and `RESOLVE_NO_XDEV`. Absolute,
traversal, ambiguous separator/dot, control-character, overlong and excessively
nested paths are rejected. There is **no canonicalize-then-open or ordinary-open
fallback** on an unsupported kernel or denied syscall.

An `O_PATH` descriptor is opened first; regular-file type, current-user ownership,
private permissions, one-link count and byte length are checked **before** a
readable open. FIFOs, devices, sockets, symlinks and directories cannot be
activated as input. Only the already-held regular inode is reopened through a
supervisor-generated numeric `/proc/self/fd` path. This internal trusted-procfs
conversion is not a user path resolver. Identity, size and modification metadata
are checked again after a bounded read. Other files in the directory are not
traversed/copied, and no source FD or directory FD goes to a worker.

These checks reduce concurrent-file races; they are not an atomic filesystem
snapshot, authenticated storage or cross-process CAS. A malicious same-user host
process, compromised OS/procfs/filesystem, persistent protection after publication
and arbitrary root selection are outside this component's guarantees.

## Frozen project snapshots and the worker boundary

`ProjectSnapshot::capture` parses and validates the full strict v1 raster project
(maximum 40 MiB), serializes it canonically and creates a private anonymous memfd.
`F_SEAL_WRITE`, `F_SEAL_GROW`, `F_SEAL_SHRINK` and `F_SEAL_SEAL` are applied and
verified; additional restrictive seals are allowed. Failure blocks the operation.
Hidden pixels, transparent RGB and names are retained and disclosed to the worker.

`bounded_worker::run_snapshot` passes a fresh read-only description of that sealed
file as **stdin**, with an independent offset on every run. It mounts no source
file/directory, creates no artwork staging file and captures logs in anonymous
memfds. The inherited 8-KiB per-file write rlimit does not limit input reads or
require bubblewrap to copy larger inputs into a file. The worker still gets only
standard descriptors; its cgroup quotas/watchdog and empty-root/read-only-system
filesystem restrictions remain in force. `/input` is empty.

The preferred fixed-code diagnostic is:

```sh
perveance-cli verify-mediated-worker PRIVATE_INPUT_ROOT RELATIVE_PROJECT DELEGATED_CGROUP_ROOT [FIXED_MODE]
```

`verify-bounded-worker PROJECT PRIVATE_PARENT CGROUP_ROOT` remains compatible:
it loads its source through the old **trusted-path** loader, validates the legacy
parent, then uses an anonymous sealed snapshot. Its initial path loading has not
become safe for untrusted source paths. The older `verify-fixed-worker`, general
project loaders, data-only raster task store, Python bridge and GUI are unchanged.

## Independent artifact validation and publication

A trusted supervisor can capture a stopped candidate's fixed
`output.perveance.json` and `output.png` through a `PrivateDirectory` capability.
`VerifiedBundle::capture` requires the complete project to equal a **trusted
expected Project**, and decoded static PNG dimensions/RGBA pixels to equal that
project's composite. A candidate-provided expected revision, success claim or
self-written test result is not authority. JSON/PNG input sizes and decoding are
bounded; special files/links are rejected before consumption.

After comparison, artifacts are re-serialized/re-encoded from the trusted model
and sealed. Candidate PNG text/metadata, unknown neighboring files, whitespace and
mutable source files are not published. Later mutation of the candidate files
cannot affect publication. This library validation is not yet wired to a general
worker output transport or the capability installer.

`VerifiedBundle::publish` writes exactly the two canonical artifacts into a new
private staging directory relative to an authorized destination FD. Files and
the staging directory are synced, then one `renameat2(RENAME_NOREPLACE)` installs
the whole bundle. Readers do not see a one-file half-bundle under the final name.
Existing files, directories, symlinks and FIFOs are never replaced. Unsupported
rename semantics fail; there is no overwrite fallback. Normal pre-commit errors
clean only this operation's fixed files/private stage, never arbitrary trees.

**A post-rename parent-sync failure is different:** the bundle is already
committed. Return `committed:true, parent_directory_synced:false`, keep the
artwork and tell callers to inspect that same name instead of blindly retrying
or deleting it. Sync success is syscall evidence, not a power-loss guarantee.
A killed publisher can leave an unpublished private stage; automatic crash
retention/recovery is not implemented. Published files remain editable by their
owner; hashes are metadata, not signatures or perpetual immutability.

## Useful offline command (no cgroup or model required)

```sh
mkdir -m 700 inbox bundles
# Place an owned, private (0600) project in inbox; do not put credentials there.
perveance-cli publish-raster-bundle /absolute/inbox project.json /absolute/bundles artwork-01
```

This captures the project, uses the trusted Rust renderer, and publishes one
editable-project/PNG bundle. It is a data-only **in-process** operation, not a
worker job, model call, capability build or quota-enforced task. Metadata includes
the bundle name, project revision, fixed filenames, sizes, hashes and parent-sync
status; raw artwork, layer names and input paths are not echoed in errors.
`ok` means committed publication, not guaranteed crash durability: check the
parent-sync field. Neither command grants autonomy.

## Prerequisites, verification and remaining gates

Linux `openat2`, trusted procfs, memfd creation/seals and a filesystem honoring
`renameat2(RENAME_NOREPLACE)` and directory/file sync are required. Root-user
execution is refused. Local Linux filesystems are the development target;
network/unusual filesystems and Windows/macOS are not validated. Worker runs
add the existing delegated-cgroup/bubblewrap prerequisites in LINUX_WORKER.md.

New Rust tests exercise real path/mount/magic-link denial, special files,
hardlinks, source/root replacement, seals, offset independence, oversized inputs,
malformed/mismatching candidate files, PNG metadata stripping, concurrent
no-clobber publication and failures before/after commit. The delegated CI suite
adds repeated sealed input larger than the write rlimit and the mediated CLI;
the downloaded Linux executable also performs an offline publication round trip.
At implementation commit `36963cc`, [Rust run 42006495225](https://github.com/c4us4l1ty/perveance/actions/runs/42006495225)
passed all 12 new unit and six new integration tests, the existing full Rust,
native-GUI/bridge/recovery regression and downloaded-binary publication,
revision/hash and occupied-name checks. [Feasibility run 42006494788](https://github.com/c4us4l1ty/perveance/actions/runs/42006494788)
passed all seven real cgroup-worker tests on the delegated Ubuntu 26.04 service:
kernel `7.0.0-1017-azure`, bubblewrap `0.11.1`, AppArmor user-namespace restriction
still enabled. This includes repeated sealed input larger than 8 KiB, source
mutation after capture, mediated CLI and absence of disk artwork/log staging.
All 41 local Python tests also pass. No local Rust toolchain was available;
Rust/kernel evidence comes from CI, not an untested local claim. These selected
runner results are not approval of another host or of arbitrary generated code.

This is a scoped raster component, not the entire trusted I/O/role separation
requirement. Dependency acquisition, protected fetch/build/runtime namespaces,
output transport from an untrusted worker, durable task integration, bounded
credential/model brokerage, schema migration, independent capability verification
and crash-safe activation remain required before generated execution can open.


## Single-file data-only document publication

The v2 [document foundation](DOCUMENT.md) reuses held-directory capture and adds
private single-file publication: bounded canonical host bytes, random 0600 stage,
file sync, stage inode check, mandatory `renameat2(RENAME_NOREPLACE)`, parent sync
and a basename/length/SHA-256 receipt. Existing names are never replaced; cleanup
unlinks only our stage inode. A postcommit sync failure reports visible output
with `parent_directory_synced=false`, not a precommit failure or deletion.
Concurrent and injected pre/postcommit failure cases supplement existing bundle
tests. Crash stages may remain private; no automatic repair/sweep is provided.
This does not authorize plugin code or alter worker/broker/job contracts. V2
unknown capability bytes are preserved, never semantically approved or rendered.
At `8593268`, all-target Rust and real debug/downloaded-CLI document lifecycle
passed ([42030894898](https://github.com/c4us4l1ty/perveance/actions/runs/42030894898)),
including injected failures/concurrency/held-root cases. All fourteen mandatory
worker kernel cases separately passed ([42030894888](https://github.com/c4us4l1ty/perveance/actions/runs/42030894888));
this is not production certification or generated-code authority.
