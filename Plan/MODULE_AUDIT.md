# Static WASM diagnostic — not capability verification or activation

**2026-11-05 — verified diagnostic source at `3f6639bce992fa0deaf7adf523ed1436b3d5e397`.**
This continues [immutable quarantine storage](REGISTRY.md), not a new runtime.
`registry-audit-module PRIVATE_REGISTRY_ROOT EXACT_SLOT` inspects the **actual
frozen stored bytes**, not a candidate's success claims or a version fallback.
No module is instantiated, compiled, called or executed. A static pass grants
**no** installation/activation permission, behavior/provenance trust, payload
schema support or document rendering access. Autonomy remains disabled.

## Two separate stages, no cached approval

1. The Linux registry opens its mandatory private descriptor backend and holds
   the existing exclusive lifetime flock. The slot must be exactly 64 lowercase
   hex digits, not an ID, path, `latest` or version range. A complete inventory
   revalidates every package's members, canonical manifest/receipt, actual byte
   lengths/digests, exact slot and registry scope, file/FD rules and storage caps.
   Unknown/corrupt storage or a missing slot fails closed. Interrupted stages
   also block module audit; `registry-inspect` can still report their retained
   state. Nothing is cleaned, repaired, rewritten, resynced or replayed.
2. That same walk retains **only the selected frozen candidate** for audit,
   rather than all stored artifacts or a second unchecked path read. The
   portable `CapturedCandidate::audit_static()` API uses the same inspector on
   immutable already-bound captured bytes. Capture/admission itself still checks
   only the 8-byte WASM-v1 envelope, size and SHA-256; invalid module bodies can
   still be retained as unavailable quarantine data.

The diagnostic does not add files, bless receipts, update entry state or persist
its result. Repeated calls recheck storage and bytes. An externally saved report
is not authenticated, anti-rollback storage or an execution permit. Private roots
and flock do not stop malicious same-UID/privileged host forgery or rollback.
This is **not** protected independent verifier/supervisor role separation.

## Pinned parser and explicit feature policy

`src/module_audit.rs` uses the established `wasmparser =0.240.0`, with default
crate features disabled and only `std`, `validate`, `features`. No interpreter,
JIT, WASI, component-model backend or extension executor is added. Its sole new
required normal dependency is the already-locked bitflags 2.x. Cargo resolution
also requires a lockfile edge to the already-locked indexmap for the weak
`std` feature relationship. No other package/version is added or upgraded.
The diagnostic resolution confirmed enabled wasmparser features exactly
`features`, `std`, `validate`; it did not relax or pass the mandatory locked test.
That temporary diagnostic step has been removed. `Cargo.lock` records the
official crates.io-index checksum:

```text
b722dcf61e0ea47440b53ff83ccb5df8efec57a69d150e4f24882e4eba7e24a4
```

Sources: [upstream manifest at v1.240.0](https://github.com/bytecodealliance/wasm-tools/blob/v1.240.0/crates/wasmparser/Cargo.toml)
and [registry index](https://github.com/rust-lang/crates.io-index/blob/master/wa/sm/wasmparser).
This is an exact dependency pin, **not** a claim of latest version, independent
security certification or completed license/release review. Cargo's locked CI
build and package metadata inventory must still pass for the actual revision.

A bounded prepass is followed by the library's full `Validator::validate_all`,
including function-body stack/type/index validation. Merely parsing sections or
reading the header is not enough. The validator feature set is explicitly the
union of mutable globals, saturating float-to-int, sign extension, multi-value,
bulk memory and floats, **not** evolving default features. Memory64, shared
memory/threads, multiple memories, reference/GC types, SIMD, tail calls,
exceptions, extended constant expressions and components are not enabled.
No imports, start function, tables, elements or non-profile exports are allowed.
Float allowance does **not** prove cross-host determinism or NaN behavior.

## Prospective static ABI signature skeleton, policy v1

The versioned diagnostic policy is `perveance-static-abi-v1`, for declared ABI 1
and document version 2 only. Other positive declarations remain retainable in
quarantine but produce `unsupported_manifest_contract` here.

Exactly these **five export names** are required:

| Export | Required kind/signature |
| --- | --- |
| `memory` | Memory index 0 |
| `perveance_alloc_v1` | Function `(i32) -> i32` |
| manifest `entrypoints.validate_payload` | Function `(i32, i32) -> i64` |
| manifest `entrypoints.apply_edits` | Function `(i32, i32) -> i64` |
| manifest `entrypoints.render_layer` | Function `(i32, i32) -> i64` |

The three manifest names cannot collide with the two reserved names. Their
function indices may alias if signatures match; names/types do not establish
separate behavior or protected roles. Internal helper functions/globals are
allowed within limits. No extra exported names are allowed. There must be one
non-shared 32-bit, standard-64-KiB-page memory, initial size at least one page,
and an explicit maximum at least its initial size and no greater than the
manifest's bounded memory declaration.

These are **only numeric signature checks**. Pointer/length interpretation,
returned i64 packing, allocator correctness, memory ownership/lifetime, input/
output wire framing, errors, typed edits, render buffers, schema semantics,
determinism and bounds at each call remain unspecified/unimplemented runtime
contract gates. Do not implement a loader or treat this skeleton as a completed
extension ABI. Even `return 0` stubs and an infinite loop can pass this policy.
The tests deliberately demonstrate that a static pass proves no useful behavior
or termination. Payload/operation schema versions and provenance hashes remain
candidate claims, never behavior-tested by this diagnostic.

## Input and work-shape caps

The existing capture cap is 4 MiB per artifact and 16 KiB per manifest. Before
full validation, the inspector imposes these additional compiled ceilings:

| Item | Ceiling |
| --- | --- |
| Parser events (includes header/end and individual code bodies) | 2,048 |
| Function types / defined functions / numeric globals | 128 / 1,024 / 64 |
| Export entries / parameters per type / results per type | 5 / 16 / 4 |
| Local declaration groups or expanded values per function | 1,024 each |
| Expanded local values across all bodies | 16,384 |
| Encoded body bytes per function | 256 KiB |
| Body operator count across all functions | 200,000 |
| Body control depth (including function frame) | 128 |
| `br_table` non-default labels | 4,096 |
| Global initializer bytes | 32 each |
| Data segments / total data bytes | 32 / 2 MiB |
| Total custom-section content bytes (including name encoding) | 64 KiB |

Values are counted before local expansion/full body validation. Type/global/
function/export/segment counts are checked before iterating those collections.
Only numeric i32/i64/f32/f64 types are accepted for functions/locals/globals.
Passive data segments are allowed. Active data offsets must be a single
nonnegative `i32.const` followed by `end`, for memory 0, with the entire segment
inside the declared **initial** memory. No initializer is executed. Custom
sections are bounded opaque metadata, not source/dependencies to fetch or code.

These input/count/depth caps constrain accepted work shape; **they are not
process CPU/RAM/wall quotas or a parser sandbox**. Some parsing necessarily
happens before a count check. The inspector runs on the host; no worker fuel,
process confinement or independently supervised verifier resource enforcement
is claimed. Manifest fuel/output bounds are still declarations, not enforced
limits. A future confined execution/verification design remains mandatory.

## JSON result and failure semantics

On a completed audit the CLI returns `ok:true` and `module_audit`, containing
`registry_scope`, `slot`, `storage_integrity_checked:true` and `audit`:

- `version`, `policy`, pinned `inspector`, exact `reference`, canonical
  `manifest_sha256`, actual `artifact_sha256` and `artifact_bytes` bind the
  observation. They are not authentication or an activation capability.
- `static_profile_passed` is true only after bounded prepass, complete structural
  validation and all profile/signature checks pass.
- `structural_validation` is `not_run`, `passed` or `failed`. A prepass rejection
  leaves it `not_run`, even if that rejection found malformed encoding. It must
  never be interpreted as a claim that rejected bytes are structurally valid.
- `abi_signatures_match` requires the entire signature profile. `counts` are
  partial on early rejection; `counts_complete` only means the prepass finished,
  **not** that structural/profile validation passed.
- `rejection` is null on a pass or **one fixed enum** on rejection. Hostile parser
  errors, export/import/custom names, paths, source text and code excerpts are
  not echoed. The bounded exact capability reference is intentional metadata.
- `module_instantiated`, `code_executed`, `runtime_limits_enforced`,
  `behavior_verified`, `payload_schemas_verified`, `provenance_verified`,
  `capability_verified`, `activation_permitted`, `registry_modified`, `installed`,
  `active`, `available`, `autonomous_execution` are **always false**.

`ok:true` can therefore contain `static_profile_passed:false`; it means the
requested diagnostic completed, not that the candidate passed. Storage/slot/
path/corruption/scope/orphan errors instead return `ok:false` on stderr and a
nonzero exit, without a module audit. Registry inspection/resolution remains
unchanged after even a passing audit: `extension_abi_supported=false`,
`quarantined_unverified`, unavailable, and the document binding still reports
`quarantined_no_runtime` because a diagnostic call binds and activates nothing. Every capability layer still
blocks PNG export, including hidden or empty ones.

## Tests and remaining gates

`tests/module_audit.rs` builds byte fixtures without a WAT compiler/dependency:
positive stubs/infinite-loop/nonexecuted data/custom metadata; malformed and
truncated bytes; stack/index/count/type/signature errors; imports/start/exports/
reserved-name/features/memory denials; local/body/operator/depth/branch/data/
custom/count bounds; mutation corpus; frozen capture; exact read-only registry
binding, full-inventory corruption/scope/extra-member/orphan refusal; continued
unavailability and rendering refusal. `tools/test_module_audit_lifecycle.py`
runs a separate real CLI lifecycle, required by CI for both the debug binary
and the actual uploaded-then-downloaded release executable. No mock CLI,
interpreter, dynamic plugin or candidate-supplied success test is substituted.

At exact source `3f6639b`, [Rust CI 42060281542](https://github.com/c4us4l1ty/perveance/actions/runs/42060281542)
passed in **6m18s** (job `127359693355`): mandatory `cargo test --locked --all-targets`
including **twenty new inspector cases** (18 portable, 2 Linux), real debug CLI
static-audit/document/registry lifecycles, native X11/bridge/recovery regressions,
release build/metadata/packaging/upload/download and all three lifecycles against
the **actual downloaded release executable**. The dependency lock was resolved
with Cargo (including the weak indexmap edge) and the temporary failing-build
diagnostic removed; all required final builds still use `--locked`. A negative
tag fixture's section ordering was corrected before this green run.

[Matrix 42060281533](https://github.com/c4us4l1ty/perveance/actions/runs/42060281533)
passed all three jobs. Delegated Ubuntu26 job `127359692952` passed the exact
**twelve worker + two durable-task kernel cases** in **1m16s**, kernel
`7.0.0-1019-azure`, bubblewrap `0.11.1`, AppArmor userns restriction `1` unchanged.
The successful required step and its notice are guarded by distinct complete
`test result: ok. 12 passed; 0 failed` and `test result: ok. 2 passed; 0 failed`
prefixes in the unchanged workflow; the two-case check cannot match twelve.
The ordinary runner's optional namespace probe still could not start. These
are fixed offline CI observations, not end-user host/desktop approval, independent
behavior verification, production role certification or real-provider evidence.

Local 50 Python tests passed again (1.783s), Python compile/shell syntax/diff checks
passed. There is **no local Rust toolchain**; Rust and real-binary execution
claims above are actual exact-source CI, not a claimed local run. Prior storage
evidence remains in REGISTRY.md. The subsequent evidence-only documentation
head is also checked by the same required workflows; see the PR's latest status.

Still missing: protected independent behavior/schema/provenance verification,
a complete typed-edit/render wire ABI, confined fuel/memory/output/runtime
execution, trusted immutable installation/activation records and protected
production roles, migration/backup/rollback/recovery, and the genuine missing
creative-capability build/install/edit/render/save/reopen/distinct-reuse test.
This component does not satisfy original capability-growth acceptance and
activates no autonomous work or paid provider request.

## Runtime follow-up without relaxing the audit

The static profile is now optionally followed by a metered wasmi call for the
same frozen bytes (`registry-invoke-capability`, see
[EXTENSION_PROTOCOL.md](EXTENSION_PROTOCOL.md#metered-interpreter-diagnostic-nonauthorizing)).
The audit is unchanged and still gates it: a module that fails the profile is
refused before instantiation, so execution cannot be used to bypass validation.
The four encoding faults exposed by the runtime fixtures (data section before
code, a four-byte data flag, a loop body missing the function `end`, and a
wrong reply kind byte) were corrected in the fixtures themselves. Every limit
and rejection rule, including import/start/section/signature/data checks, is
still applied exactly as before, and a passing audit still enables no
installation, activation or availability.
