# Data-only extension call/reply protocol — no execution or activation

**2026-11-05 — verified at exact source `b1febc0`; see evidence below.**
This continues the [static WASM signature diagnostic](MODULE_AUDIT.md) with a
concrete version-1 byte contract and host-side **hypothetical reply** inspection.
No module is instantiated/called, no opaque document is edited, and no candidate
pixels are displayed/composited/exported. It is not the complete protected
extension runtime or the original creative capability-growth acceptance.

## Boundary and scope

`CapturedCandidate::prepare_call(document, intent)` first requires the existing
static profile to pass, then constructs an immutable `PreparedCall`. The exact
manifest reference must equal the selected capability layer's full reference
(ID, exact version, artifact hash, ABI, payload schema). ABI 1/document 2 only.
The intent's base revision must equal the actual captured document revision.
Only the selected layer's ordered object IDs/raw opaque bytes are in the frame;
other layers, native pixels, names, visibility and source paths are not sent.
The canvas size and whole-document revision hash are included. Selecting a
hidden layer explicitly includes its opaque data; this is not DLP or permission
to send that data to a provider.

`registry-inspect-protocol` uses the same complete locked inventory/frozen-byte
selection as module audit. All package members, canonical/hash/scope/slot/quota
bindings are revalidated before preparation. Corrupt/missing/wrong-scope storage
and retained interrupted stages block it, without cleanup/replay/repair. Registry
calls bind the registry's nonsecret 128-bit scope into the request. The portable
captured-byte API uses an all-zero diagnostic scope. Scope/nonce/hash binding is
**not** authentication, replay prevention, a signature or protected production
role separation; a hostile same-UID/privileged process can forge/roll back files.

No registry/source FD is passed to code, and there is no loader or invocation.
The prepared object owns only scoped bytes/metadata and aggregate foreign-layer
counts for enforcing document-wide limits; it does not clone/edit the entire
raster document. Its public byte getter contains private selected-layer data:
never log it or treat it as provider-safe. CLI reports include hashes/counts,
not request/reply payloads, private names, arguments or candidate error strings.

## Strict intent JSON

The private input file is at most 64 KiB, with unknown/duplicate fields rejected:

```json
{
  "version": 1,
  "call_id": "00000000000000000000000000000001",
  "base_revision": "sha256-document-v2:<64 lowercase hex digits>",
  "layer_id": "<32 lowercase hex digits>",
  "kind": "apply_edits",
  "edit": {
    "operation": "example_edit",
    "schema_version": 1,
    "arguments": {"encoding":"hex", "data":""},
    "new_object_ids": ["50000000000000000000000000000000"]
  }
}
```

Kinds are `validate_payload`, `apply_edits`, `render_layer`. `edit` must be null/
absent for validation/rendering, and present for edits. The operation ID and
schema version must exactly match a manifest declaration. Arguments are opaque,
not JSON/commands to interpret, at most 16 KiB decoded. At most 32 offered new
IDs are allowed, all unique and absent from **every** document/layer/object ID.
They are caller-selected diagnostic IDs, not a persisted reservation allocator.
The call ID is a caller-supplied 128-bit correlation value, not proof of entropy,
authority, uniqueness or replay protection. Repeated inspection is allowed.
Unknown positive payload/operation schema versions remain opaque claims; exact
matching does not mean semantic support or behavior verification.

## Canonical request frame

All integers are fixed-width **unsigned little-endian u32** unless stated.
`blob` is a u32 length followed by exactly that many bytes. IDs/scopes are 16 raw
bytes, hashes 32 raw bytes (hex is only for JSON display). No varints, padding
other than specified zeros, compression, paths, optional unknown fields or
trailing bytes are accepted. `decode_request` returns a bounded borrowed view;
a decoded request alone cannot construct an authoritative host snapshot/permit.

| In order | Encoding |
| --- | --- |
| Magic/version | 8 bytes `PVEXRQ01` |
| Kind | u8: 1 validation, 2 edits, 3 render |
| Reserved | 3 zero bytes |
| Call ID, context scope | 16 bytes each |
| Canonical manifest SHA-256 | 32 bytes |
| Exact captured artifact SHA-256 | 32 bytes |
| Document revision hash (without textual prefix) | 32 bytes |
| Document ID, selected layer ID | 16 bytes each |
| Canvas width, height, payload schema | u32 each |
| Object count | u32, at most 1,024 |
| Ordered objects | each: ID (16 bytes), payload blob (at most 16 KiB) |
| Edits only: operation ID | ASCII symbol blob, 1–64 bytes |
| Edits only: operation schema | positive u32 |
| Edits only: opaque arguments | blob, at most 16 KiB |
| Edits only: offered ID count/list | u32 at most 32, then 16 bytes each |

Object count/total payload obey the existing **whole-document** ceilings of
1,024 objects and 1 MiB decoded payload, with per-object 16 KiB. The frame contains
only the selected subset. Total frame size is at most 2 MiB and must be strictly
smaller than the manifest memory declaration (pointer zero is reserved).
Preparation does not prove the allocator or actual module memory can hold it.

The request binding is SHA-256 of:

```text
perveance-extension-request-v1\0 || entire canonical request frame
```

Changing scope, nonce, manifest/pin/schema, document revision/canvas/layer/object
bytes/order, edit operation/arguments or offered ID order changes the binding.
The SHA is not secret: any candidate can forge a matching reply. It prevents
accidental cross-context acceptance, not malicious behavior or authorization.

## Reply frame and status

| In order | Encoding |
| --- | --- |
| Magic/version | 8 bytes `PVEXRP01` |
| Kind | same u8 kind as request |
| Candidate status | u8: 0 success, 1 invalid payload, 2 unsupported operation, 3 resource declined, 4 failed |
| Reserved | 2 zero bytes |
| Request binding | exactly the expected 32-byte request digest |
| Body | blob; no trailing bytes |

The fixed header is 48 bytes. The **entire frame**, not only the body, must fit
both the manifest output declaration and the 1 MiB host ceiling. Preparation
requires the declared output ceiling to accommodate at least the header. Unknown
status/kind/flags, mismatched digest, truncated/overlong frames and trailing data
are rejected with fixed enum findings. Non-success statuses require an empty
body: no free-form error text can leak via diagnostics. Success means a candidate
claim, never trusted payload validity or useful capability behavior.

### Validation success

Body is empty. Wire acceptance only records the candidate's claim; it does not
validate any opaque schema, approve an edit or enable rendering.

### Edit success: bounded proposal, not a document commit

Body begins with a u32 count (0–128), then typed records:

| Tag | Fields |
| --- | --- |
| u8 `1`: insert | new ID (16); before marker u8 0=end or 1=following anchor ID (16); payload blob |
| u8 `2`: replace | existing object ID (16); expected old raw-payload SHA-256 (32); new payload blob |
| u8 `3`: remove | existing object ID (16); expected old raw-payload SHA-256 (32) |

Only offered new IDs can be inserted. Replace/remove targets must belong to the
captured selected layer and match their old payload hash. Each target may be
touched **once** per proposal; no insert-then-replace/delete or duplicate targets.
An insertion anchor must exist at that point in the ordered projection (including
an earlier inserted object). No layer/document/reference/schema/native pixels,
foreign objects, host path or command can be edited. Every intermediate prefix
and final projection must respect per-object and whole-document count/byte caps,
including **unselected** layers. Conservative prefix limits may reject an
insert-before-remove sequence that would fit only after completion.

Inspection works on a private selected-layer copy and discards the entire
projection on any failure. A successful report contains counts and a projected
layer SHA-256 over `perveance-extension-layer-v1\0` followed by the same count/
ID/blob object encoding. **No projected payloads/document are returned, applied,
saved or installed**. Old-payload hashes detect stale data, not schema semantics.
A no-op proposal is allowed and proves no useful edit capability.

### Render success: raw RGBA shape, not an image publication

Body: width u32, height u32, format u8 `1`, three zero reserved bytes, then pixel
blob. Width/height must exactly match the captured canvas, 1–256 each, and the
blob must be exactly `width * height * 4` bytes. Format 1 denotes top-left,
row-major tightly packed RGBA8, straight/unpremultiplied alpha, sRGB byte encoding.
No stride, crops, scaling, floats, PNG/compression or external resources.

Only byte shape/length and raw pixel SHA-256 are checked. This does not prove
color interpretation, correct geometry, deterministic rendering or fidelity.
Pixels are not displayed, composited or exported, and **every capability layer
still blocks document PNG export even after an accepted hypothetical reply**.

## Prospective memory call convention, without a runtime

The static allocator signature stays `(i32 length) -> i32 pointer`; three
manifest entrypoints stay `(i32 request_pointer, i32 request_length) -> i64`.
Integers are interpreted as unsigned bit fields, not signed addresses:

- Pointer zero is reserved for failure; zero-length replies are invalid.
- Returned i64: **high 32 bits pointer, low 32 bits reply length**.
- `request_window(memory_bytes, pointer)` and `inspect_memory_reply(memory,
  packed)` require standard nonzero 64-KiB page multiples, no greater than the
  manifest memory ceiling; checked pointer+length must fit the actual supplied
  memory slice. No wrap/truncation/signed arithmetic or pointer dereference.
- The reply range must also fit output limits before inspection/copying. Ranges
  may alias request storage; immutable host request/context bytes stay outside
  module memory. Future host code must not re-trust mutated request bytes.
- A future executor must use a **fresh confined instance per call**, meter fuel/
  memory/output/CPU/wall/process cleanup and copy from stable/quiescent unshared
  memory before dropping the instance. There is no deallocation/reuse API here.
  Those requirements are **not implemented or authorized** by these helpers.

Memory helpers only inspect caller-supplied slices/ranges; no engine exists.
Input/count caps and host projections are not a supervised process CPU/RAM/wall
sandbox. Host JSON/revision/static parsing and I/O remain outside worker quotas.

## CLI and report semantics

```text
registry-inspect-protocol PRIVATE_REGISTRY_ROOT EXACT_SLOT PRIVATE_DOCUMENT_ROOT DOCUMENT PRIVATE_INTENT_ROOT INTENT [PRIVATE_REPLY_ROOT REPLY]
```

All roots/files use the existing mandatory private Linux descriptor backend.
Without a reply, report `call` is a redacted preparation summary (including
scope/ref/revision/request digest/counts), not raw request bytes or an invocation.
With a reply, `reply.wire_accepted` says only whether the hypothetical frame,
binding, limits and typed shape are acceptable. `ok:true` may therefore include
`wire_accepted:false`; fixed `issue` identifies rejection. Invalid storage/scope/
intent/snapshot preparation or file capture instead returns `ok:false`/nonzero.
An accepted non-success candidate status is not a successful edit/render result.
Partial projections/hashes/statuses are cleared on rejection. No result is cached.

All `gates` fields remain false: module_instantiated, code_executed,
behavior_verified, payload_schemas_verified, provenance_verified,
capability_verified, runtime_limits_enforced, activation_permitted, installed,
active, available, document_modified, registry_modified, png_published,
provider_network_used, autonomous_execution, authenticated,
replay_protection_enforced. There is no verify/install/activate/run command,
permitted-call token, migration, automatic replay, provider or autonomy route.

## Verification and remaining gates

Twenty-one new Rust cases (twenty portable, one Linux-only isolated registry case)
cover all kinds, canonical/digest/context binding, opaque scoping, intent/pin/
schema/ID preconditions, truncation/mutation/length/flag/status/count attacks,
transactional projection/global frame caps, exact RGBA shape, packed-memory
boundaries, redaction, readonly registry selection and interrupted-store refusal.
The existing document, registry and static-audit cases also continue to pass;
the locked all-target step still fails on any single case failure.

The real debug and uploaded-then-downloaded release executables also run
`tools/test_extension_protocol_lifecycle.py`, which quarantines the hand-built
stub, binds the selected layer, and inspects **hand-authored** reply fixtures for
all three kinds plus stale revision, unknown intent field, scope/format attacks,
wrong kind/digest/status, malformed and truncated frames, precondition/duplicate
target/limit failures, exact-canvas RGBA, repeated inspection, source changes,
and interrupted stages. Those fixtures are not executed module behavior, useful
edits, semantic rendering or original vector acceptance.

Exact source `b1febc0` passed:
[Rust CI 42067792375](https://github.com/c4us4l1ty/perveance/actions/runs/42067792375),
job `127386732699` succeeded in **6m12s**: locked `--all-targets` tests including
these 21 new cases, the new real debug and downloaded-release protocol lifecycle,
and all existing document/registry/static/native/package/downloaded regressions.
[Matrix 42067792369](https://github.com/c4us4l1ty/perveance/actions/runs/42067792369)
passed all three jobs; delegated26 job `127386702285` passed exact **twelve + two**
kernel cases, with kernel `7.0.0-1019-azure`, bwrap `0.11.1` and AppArmor
restriction `1` unchanged; the ordinary optional namespace probe remained
unavailable. Two earlier commits `a0bb48a`/`a99dacb` exposed a binary-private read
and a serde test typing error; both were fixed without relaxing `--locked` or any
authority gate. There is no local Rust toolchain; local 50 Python tests,
compile/shell/diff checks and fixture encoding checks passed. This evidence does
not extend to any future runtime, semantic or activation work.

Still required: confined engine/worker integration under protected production
roles, independent semantic/behavior/provenance verification, authenticated
trusted install/activation state, durable replay/cancellation/recovery semantics,
actual editable capability document commits/composition, migrations/backup/
rollback, and genuine missing-capability build/edit/render/save/reopen plus a
distinct request reusing the exact installed capability. No vector functionality
or completion of the protected extension contract is claimed by this increment.
