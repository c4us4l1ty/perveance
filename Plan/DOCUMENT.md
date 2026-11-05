# Bounded data-only extensible documents (v2 foundation)

**Status:** implemented for review; new Rust/CLI/packaged-binary results must be
recorded after CI. This is an offline storage/native-edit increment, not a
capability registry, executable plugin ABI, vector feature or self-extension demo.
No provider/model is invoked and no generated code is loaded or executed.

## Separation from the existing editor

`src/document.rs` defines a separate strict **version 2** document. The existing
version-1 `Project`/`Layer`, GUI, Python bridge, provider prompt adapter and both
job backends stay v1-only. They must reject v2, including raster-only v2, rather
than reinterpret or drop fields. Use the new `document-*` commands for v2. There
is no implicit conversion, downgrade or automatic migration on open.

Explicit `document-import-raster` imports a captured valid v1 raster project into
v2 **as a new file**, preserving canvas, layer order, names, visibility and every
RGBA byte (including hidden pixels and RGB under zero alpha). This leaves the
original file intact. Native rendering and pixel editing use the existing Rust
`Project` engine: there is no second composition or drawing implementation.

## Data contract

The top level is `{version:2,id,width,height,layers}`. Layers are bottom to top,
with `{id,name,visible,content}`. A content value is exactly one of:

```json
{"kind":"raster","pixels":[[0,0,0,0]]}
```

```json
{
  "kind":"capability",
  "capability":{
    "id":"example.unavailable",
    "version":"1.2.3",
    "artifact_sha256":"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    "abi_version":99,
    "payload_schema":77
  },
  "objects":[{
    "id":"40000000000000000000000000000000",
    "payload":{"encoding":"hex","data":"00ff7b6e6f74204a534f4e7d"}
  }]
}
```

This fake digest/reference illustrates **unavailable data**, not an installed or
trusted module. `examples/opaque-document-v2.json` is a preservation/refusal
fixture, not working vector art or a capability shipped with the bundle.

- Document, layer and object IDs are exactly 32 lowercase hexadecimal digits,
  unique in a single document namespace. They are identifiers, **not authority**.
  Raster import derives deterministic IDs from a domain-separated v1 content
  fingerprint and layer ordinal. Identical imports share IDs; these are not
  globally unique random UUIDs. Copies and native edits retain IDs unchanged.
- The canvas retains the v1 1–256 limit per axis; there are 1–32 ordered layers.
  A raster layer has exactly `width*height` straight-alpha RGBA pixels.
- Layer names are at most 256 UTF-8 bytes. Import preserves names accepted by
  v1, including empty names; a rename rejects blank or overlong names.
- There are at most 1,024 capability objects **across all layers**. Each opaque
  payload is 0–16 KiB, total at most 1 MiB. Hex is mandatory, even-length and
  lowercase. It can represent any byte sequence, including invalid UTF-8,
  malformed JSON, executable-looking text or arbitrary binary. Those bytes are
  never treated as JSON, code, a path, a URL, an archive or a dependency request.
- References contain a 1–64 byte ASCII ID (alphanumeric first, then
  alphanumeric/dot/underscore/hyphen), exact three-component unsigned-u32
  `major.minor.patch` without leading zeros, lowercase SHA-256, positive ABI
  version and positive payload-schema version. No `latest`, ranges, commands,
  paths, URLs, dependencies, permissions or activation instructions exist here.
  Different digests/schema/ABI values are different references; none is trusted.
- Serialized input/output is bounded at 44,302,336 bytes (42.25 MiB). Bounded
  typed sequence deserializers reject excessive layers, per-layer native pixels
  and objects during parsing. A custom typed content visitor avoids serde's
  arbitrary internally-tagged-content buffering, even when `kind` appears last.
  Aggregate object/payload limits are checked after parse. File-byte and sequence
  limits do **not** establish hard process CPU/memory quotas.
- Unknown fields/variants, duplicate JSON fields, malformed hex, invalid sizes,
  future root versions and duplicate/malformed identities are errors. Payload
  schemas/ABIs may be unknown: the opaque bytes can still be preserved, **not
  semantically validated, trusted, rendered or edited**.

A copy/save canonically serializes the known v2 envelope. Payload **decoded bytes
and canonical hex content** remain exact; original JSON whitespace, escapes and
field order are not preserved. There are no object deletion, payload mutation,
flattening, garbage collection or schema-migration operations in this increment.

## Revision-bound native editing

Revision is `sha256-document-v2:` plus SHA-256 of canonical validated v2 bytes.
This includes all identities, layer order/visibility, hidden raster RGBA,
references and opaque payload bytes. It is a content fingerprint, **not a lock,
signature, authorization token, anti-rollback guarantee or compare-and-swap on
another process's file/unsaved state**. Each command operates on its captured
snapshot and writes an independently named new file, never updates in place.

A separate document proposal has `{version:1,base_revision,operations}`. It is
not a v1 raster `EditProposal`. Input is at most 512 KiB and 1–1,024 operations;
total edit units (one per wrapper operation, one per native pixel) are 1–1,024.
All targets are stable **layer IDs**, never unstable vector indices:

```json
{
  "version":1,
  "base_revision":"sha256-document-v2:<64 lowercase hex from document-inspect>",
  "operations":[
    {"op":"rename_layer","layer_id":"<32 lowercase hex>","name":"Foreground"},
    {"op":"set_visibility","layer_id":"<32 lowercase hex>","visible":true},
    {"op":"move_layer","layer_id":"<32 lowercase hex>","before":null},
    {"op":"raster_pixels","layer_id":"<native layer ID>","edits":[
      {"x":0,"y":0,"color":[12,34,56,0]}
    ]}
  ]
}
```

`move_layer.before` is a destination layer ID; null/omitted moves the layer to
the top. Moving before itself is a no-op. All operations apply to a clone and
commit to the in-memory document only if the entire batch succeeds. Later
operations resolve IDs after earlier moves. Stale/wrong-kind/out-of-bounds edits
leave the original in-memory document and source file unchanged. Wrapper edits
may rename/reorder/toggle an unavailable layer without touching its reference or
objects. `raster_pixels` refuses capability layers. No arbitrary JSON patch,
opaque mutation/deletion or candidate-supplied operation interpreter is allowed.

## Explicit rendering refusal

Every capability reference is **unavailable** in this implementation. Loading a
document never searches an installed directory, fetches an artifact, invokes a
provider or activates a module. Inspection reports layer IDs, kind, visibility,
counts, reference metadata and revision; it does not return names, colors or
payload bytes. `opaque_payloads_validated`, `code_loaded`, `registry_consulted`
and `autonomous_execution` are all false.

PNG export succeeds only for an entirely native raster document and uses the
existing v1 PNG/composition engine. **Any** capability layer blocks rendering,
even when hidden or empty. Refusal happens before creating any output/stage.
There is no placeholder image returned as a successful export, silent missing
content, partial composite, automatic rasterization or capability-data loss.
A future renderer must be independently verified, schema-compatible, confined,
resource-bounded and host-mediated; a reference or hash is not execution consent.

## Descriptor-scoped Linux CLI and publication

Core validation/editing is Rust library code. Disk-facing `document-*` CLI is
Linux-only and reuses `worker_io::PrivateDirectory`: owner-only 0700 roots,
owner-only single-link regular inputs (use mode 0600), mandatory `openat2` resolution,
no traversal/symlink/magic-link/mount crossing and captured-source inode/version
checks. A document never supplies an authority-bearing filesystem location.
Explicit root arguments are supplied by the trusted caller. The GUI and bridge
have **not** gained this CLI or new-format support.

```text
document-import-raster PRIVATE_INPUT_ROOT RELATIVE_RASTER PRIVATE_OUTPUT_ROOT NEW_NAME
document-inspect PRIVATE_INPUT_ROOT RELATIVE_DOCUMENT
document-copy PRIVATE_INPUT_ROOT RELATIVE_DOCUMENT PRIVATE_OUTPUT_ROOT NEW_NAME
document-apply PRIVATE_INPUT_ROOT RELATIVE_DOCUMENT PRIVATE_OUTPUT_ROOT NEW_NAME PRIVATE_PROPOSAL_ROOT RELATIVE_PROPOSAL
document-export-png PRIVATE_INPUT_ROOT RELATIVE_DOCUMENT PRIVATE_OUTPUT_ROOT NEW_NAME
```

`NEW_NAME` is a single 1–64 byte basename, alphanumeric first and then
alphanumeric/dot/underscore/hyphen. It cannot begin with dot. For example:

```sh
mkdir -m 700 input output proposals
# Place a v1 project in input/raster.json and chmod 600 that file first.
perveance-cli document-import-raster "$PWD/input" raster.json "$PWD/output" native.json
perveance-cli document-inspect "$PWD/output" native.json
perveance-cli document-export-png "$PWD/output" native.json "$PWD/output" native.png
```

Publication stages a random private file through the **held destination directory
FD**, syncs its contents, verifies the stage inode and commits with mandatory
`renameat2(RENAME_NOREPLACE)`, then syncs the parent directory. Existing files,
symlinks or directories are never replaced. Normal precommit failure removes
only our stage inode; a replaced stage is not deleted as our artwork. Tests
include real concurrent publication, held-root rename and failure injection.
There is no fallback to path-based publication or weaker no-clobber behavior.

The receipt contains basename, byte length, SHA-256, `committed` and
`parent_directory_synced`. A false final sync flag means the output is **already
visible but crash durability is uncertain**: do not delete or blindly retry it.
Crash/SIGKILL may leave an uncommitted `.perveance-file-*` private stage, or a
committed file without its receipt. There is no automatic replay, repair or
orphan sweep. Inspect and retain uncertain artwork; clean up only after explicit
operator review. This is not a new durable job/registry journal.

Host parsing, hashing, cloning, native rendering and I/O are outside worker
quotas. Permission/descriptor checks do not defend against a malicious same-UID
host process, privileged host, debugger or filesystem rollback. Ordinary host
code is trusted; workers never receive these root descriptors. Errors are
redacted to fixed schema/validation messages, not arbitrary serde contents.

## Verification and remaining gates

New coverage lives in `tests/document.rs`, single-file publisher tests in
`src/worker_io/linux/tests.rs`, and `tools/test_document_lifecycle.py`. The latter
runs against both the actual debug CLI and the **uploaded then downloaded
release-bundle CLI**, using the bundled unavailable-data fixture. It checks
native PNG parity, stable-ID edits, hidden transparent data, exact opaque-byte
preservation/save/reopen, hidden-layer render refusal, malformed/over-limit
inputs, stale/rollback/no-replace/path denials, and rejection by v1 authoring,
bridge and both task backends before job/output creation.

**New Rust/CLI CI evidence pending.** Existing fourteen required delegated worker
kernel cases remain required; these storage tests do not replace them.

Still absent: available capability discovery, immutable registry/install/verify/
stage/activation, code/dependency fetch/build, protected extension-specific
validators/typed editing/render ABI, migrations/backups/rollback, runtime
extension confinement, GUI/provider document integration and a genuine missing
vector build-and-reuse demonstration. The generic executable extension contract
in COMPLETION.md remains open. Passing data-only preservation tests must not be
reported as the original self-extension acceptance.
