# Immutable capability quarantine registry — storage, NOT activation

**2026-11-05 — verified storage foundation at `9955839b7bd73b36c33e88a35662da0b9e415b17`.**
This continues the verified [v2 document foundation](DOCUMENT.md) on the same
fixed session branch and open PR25. PR24/main are not merged or modified by this
increment. It is a host-owned offline storage/diagnostic component, not a plugin
loader, independent verifier, installer, execution permit or vector capability.

The next [static WASM diagnostic increment](MODULE_AUDIT.md) adds a separate
read-only optional audit, not an admission/runtime change or persisted blessing.
Its exact source `3f6639b` passed all-target/debug/downloaded/native regression CI
([42060281542](https://github.com/c4us4l1ty/perveance/actions/runs/42060281542)) and
all fourteen mandatory kernel cases ([42060281533](https://github.com/c4us4l1ty/perveance/actions/runs/42060281533));
see MODULE_AUDIT.md. The storage evidence below covers the prior foundation.

## Explicitly closed execution gates

Every entry is `quarantined_unverified`, `installed=false`, `active=false`,
`available=false`, `capability_verified=false`, `provenance_verified=false`,
`extension_abi_supported=false` and `runtime_limits_enforced=false`. There are
no verify/install/activate/run/rollback/evict/reset commands. Nothing is fetched,
unpacked, compiled or executed. No provider/network route or credential is used.

Candidate declarations cannot supply a success bit, verification receipt,
signature/trust decision, permission grant, command, path, dependency or URL.
Unknown fields are rejected. The host capture receipt deliberately says
`captured_quarantine_only_v1`; it must never be accepted as independent behavior
verification by a future activation pipeline. The receipt is not signed/MACed,
authenticated storage, anti-rollback protection or an OS key-storage mechanism.

The old document loading/rendering behavior is unchanged: it never consults this
registry, and any capability layer still blocks PNG export, even hidden/empty.
`registry-resolve-document` is **diagnostic only**, not a rendering dispatch path.
Matching staged bytes do not validate payload schemas or implement an edit/render
ABI. A fake/malformed module can match its declared hash and still be unsafe or
nonfunctional; this registry never calls it.

## Bounded strict manifest and captured bytes

`src/registry.rs` defines a version-1 manifest with exactly:

- `reference`: the v2 document's exact ID/version/artifact-SHA256/ABI/payload-schema
  reference. The existing ASCII ID, exact three-u32 version and lowercase digest
  rules apply. Unknown positive ABI/schema values can be retained, never used.
- `artifact`: `{format:"wasm_v1",bytes:N}`. Artifact input is 8 bytes–4 MiB.
- `document_version`: positive declared root version. Version 2 is the known
  document envelope; other positive values are stored as incompatible declarations.
- `entrypoints`: distinct bounded ASCII export symbols `validate_payload`,
  `apply_edits`, `render_layer`. These are **claimed names**, not exports checked at admission.
  The separate static diagnostic can inspect them without granting authority. No executable command or entrypoint path is accepted.
- `operations`: 1–16 unique ASCII symbols and positive schema versions. These are
  declarations only, not callable registered operations or schema validators.
- `runtime`: positive requested fuel (≤10,000,000), memory (64-KiB multiples,
  ≤16 MiB), output (≤1 MiB). These are bounded **requests, NOT runtime quotas**;
  no runtime exists, no fuel is metered, and nothing is admitted for execution.
- `provenance`: lowercase 64-hex source/recipe hashes, both candidate claims.
  No source/recipe is fetched, built, compared, signed or attested.

Export/operation symbols are 1–64 ASCII bytes, alphabetic/underscore first and
then alphanumeric/underscore. The manifest is at most 16 KiB and uses strict
serde structs: unknown/duplicate fields, unsupported format/root version,
overlong declarations and bad limits are errors. JSON errors are redacted; raw
code, arbitrary field names, source paths and parser excerpts are not returned.

The caller explicitly chooses a private input root and separate relative
manifest/artifact names. Both are captured through existing descriptor-mediated
single-link regular-file reads with size/inode/version checks. The host freezes
owned bytes, canonicalizes the manifest, independently computes SHA-256 over the
**actual captured artifact**, and checks declared byte length, pinned digest and
the eight-byte wasm-v1 header `00 61 73 6d 01 00 00 00`.

**At capture/admission, only the envelope header and storage digest/size are
checked.** No wasm section, import, export, function body, ABI, permission,
determinism, payload validity or malicious behavior is validated by admission.
The separate [static audit](MODULE_AUDIT.md) has its own diagnostic-only findings,
never changes this receipt/state and never verifies behavior/payload/provenance. Native executables, scripts, archives and other
headers are rejected, but a wasm header followed by arbitrary invalid data can
still be quarantined. Tests deliberately include such data and require every
availability/verification flag to remain false. This is not a working plugin.

## Immutable version identity and exact diagnostics

One slot binds one case-sensitive `(capability ID, exact version)` forever under
this API. Its safe 64-lowercase-hex basename is a domain-separated SHA-256 of the
canonical ID/version tuple. It is not derived from a candidate path. A different
artifact digest, ABI, payload schema, provenance, runtime request, operation list
or other manifest field cannot silently rebind the same version.

Re-admitting the exact same canonical manifest and pinned artifact is
`already_quarantined`: revalidate stored bytes/receipt, resync the existing
package/parent, and return existing metadata without rewriting files or consuming
a second slot. This is **storage deduplication**, not capability execution/reuse,
a rebuild demonstration, a provider replay or proof of the original acceptance.
Distinct versions coexist; no `latest`, version range, fallback or implicit update
lookup exists. There is no delete/evict API.

Document diagnostics validate the v2 snapshot and return its ID/revision plus
layer IDs and exact references, never names/pixels/payload/code. States are:

- `missing`: no slot for that exact ID/version;
- `pinned_reference_mismatch`: a slot exists, but artifact/ABI/schema differs;
- `incompatible_document_version`: exact reference exists, declared root version
  does not match the document;
- `quarantined_no_runtime`: exact pin/declaration is present; execution still absent.

All states return `available=false`. Any corrupt/incomplete package causes a
fixed error rather than trusting other packages or weakening validation. A source
file or declaration cannot override rendering refusal or inject activation policy.

## Private storage, ownership and storage budgets

Linux `Registry` owns an existing authorized mode-0700 directory and lifetime
exclusive nonblocking `flock` on an owner-only single-link zero-length
`registry.lock`. A second object/process cannot open the same store concurrently.
Mutating admission requires `&mut Registry`, preventing concurrent calls sharing
one lock object. Immutable readers can share one object; directory enumeration
opens an independent directory file description rather than sharing seek offsets
with `try_clone`.

`registry-init` requires an **empty** private root, exclusively creates/syncs the
lock, assigns a random 32-hex nonsecret scope and atomically publishes immutable
canonical `registry.v1.json`. Header policy is fixed by this build, not a caller
or candidate: **32 entries, 4 MiB/artifact, 32 MiB total committed artifact bytes,
16 KiB/manifest**. Each package also has a ≤4 KiB host receipt. Metadata overhead
is bounded separately by entry count/member/file limits. One bounded in-flight
stage can temporarily add artifact/metadata bytes. No whole-process, source-root,
whole-filesystem, CPU or memory quota is claimed.

An incomplete initialization is retained and refused, never auto-reset/repaired.
Changing header limits, purpose, scope format or canonical bytes is an error.
The fixed limits cannot be raised by editing an apparently descriptive policy.
No budgets are refunded/reset by this API. Storage counters are recomputed from
all committed validated immutable packages under the lock, not trusted from a
candidate-maintained index. This is not the provider's spending ledger.

A package contains **exactly** three fixed 0600 files in a 0700 directory:

```text
<64-hex-ID/version-slot>/
  manifest.v1.json   # canonical bounded candidate declarations
  artifact.wasm     # exact captured unvalidated artifact bytes
  capture.v1.json   # host scope/slot/length/hashes; quarantine purpose only
```

Every inspect/admit/resolve rechecks package membership, canonical manifest,
actual artifact hash/size/header, slot identity and canonical capture receipt
bound to this registry scope. Copying a package from another scope without fresh
host capture fails. Tampered code, forged behavior receipts, extra/missing files,
symlinks/hardlinks and unsupported entry names fail closed. This detects common
corruption; a malicious same-UID host/privileged process can forge or roll back
all files. No production role-separation or anti-tamper guarantee follows.

## Atomic publication and interruption

`worker_io` now supports bounded fixed-member directory publication. All names
are selected by trusted host code, not a candidate manifest. It creates a random
private stage through the held destination FD, syncs each file and the staged
directory, checks stage-directory identity, then performs mandatory
`renameat2(RENAME_NOREPLACE)` and parent fsync. Existing names are never replaced.
Normal precommit failures remove only fixed files from the held private stage;
replacement host data is not published or recursively swept. There is no path-
based, unconfined or weaker no-clobber fallback.

Publication receipts include actual file lengths/hashes, `committed` and
`parent_directory_synced`. A false final sync flag means the package is **already
visible but crash durability is uncertain**. Retain it, inspect and resync the
same immutable package; never delete/blindly replace it. Validation/report
preparation occurs before commit; postcommit readback errors cannot be mistaken
for an absent publication. The next operation independently rechecks storage.

Crash/SIGKILL can leave `.perveance-publish-*` or `.perveance-file-*` private
stages. Inspection preserves committed metadata, reports `interrupted_stages`
and `mutations_blocked=true`; all admissions (including duplicates) then stop.
Diagnostics still report unavailable references. No stage is opened as code,
accepted as a committed package, replayed, auto-repaired or swept. Unknown
storage also fails closed. Operator-reviewed retention/recovery UX is still
missing. Fault-injection/orphan fixtures are not actual power-loss certification.

## CLI and verification

```text
registry-init PRIVATE_EMPTY_ROOT
registry-inspect PRIVATE_ROOT
registry-quarantine PRIVATE_REGISTRY_ROOT PRIVATE_INPUT_ROOT RELATIVE_MANIFEST RELATIVE_ARTIFACT
registry-resolve-document PRIVATE_REGISTRY_ROOT PRIVATE_DOCUMENT_ROOT RELATIVE_DOCUMENT
registry-audit-module PRIVATE_REGISTRY_ROOT EXACT_SLOT
```

These commands require the existing mandatory Linux `openat2` descriptor backend
and an unprivileged owner. No registry/source FD is exported to a worker. The
registry is not wired to GUI, job creation, provider requests or document runtime.
`ok` means the storage/diagnostic command completed, **not verified/installed/
active/available**; also check mutation/publication durability fields.
For module audits `ok:true` can include `static_profile_passed:false`. A positive
static result also leaves every registry state/authority flag unchanged. See
[static report semantics and failure policy](MODULE_AUDIT.md).

Coverage: fourteen new core/registry tests in `tests/registry.rs`, three real
fixed-member publisher/enumeration tests, and `tools/test_registry_lifecycle.py`
against both the real debug CLI and uploaded-then-downloaded release binary.
Cases include immutable versions, exact pins/deduplication, frozen capture,
limits across reopen, real lock contention/readers, malformed/self-approval
inputs, path/link/extra-member/corruption/scope denial, orphan retention and
continued document rendering refusal. Existing fourteen mandatory worker kernel
cases remain required.

At `9955839`, [Rust CI 42048502060](https://github.com/c4us4l1ty/perveance/actions/runs/42048502060)
passed in **4m57s** (job `127317915507`): all-target Rust including the seventeen
new registry/publisher cases, real debug-CLI registry and document lifecycles,
native X11/bridge/recovery regressions, packaging/upload/download and the actual
downloaded-release CLI registry/document lifecycles. All 50 local Python tests,
Python compile, shell syntax and diff checks passed. No local Rust toolchain was
available; Rust execution evidence is actual CI, not a claimed local run.

[Matrix 42048502059](https://github.com/c4us4l1ty/perveance/actions/runs/42048502059)
passed all three jobs. Delegated Ubuntu26 job `127317915167` passed the exact
**twelve worker + two durable-task kernel cases** in 1m34s, kernel
`7.0.0-1017-azure`, bubblewrap `0.11.1`, AppArmor userns restriction `1` unchanged.
The ordinary runner's optional namespace probe still could not start. This is
fixed offline CI evidence, not independent capability verification, production
role certification, portable end-user host approval or real-desktop validation.

Still required: protected independent wasm/ABI/import/behavior/payload tests and
attested build provenance; a confined resource-enforced extension validate/edit/
render contract; immutable trusted verification records; staging/activation state
transitions under protected production roles; migration/backup/rollback/crash
recovery; genuine missing-vector build/install/render/edit/save/reopen followed
by a distinct task reusing the exact installed capability. Quarantine storage is
not completion of any of those gates, and enables no autonomy.
