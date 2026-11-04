# Agent and headless interface

## Goal

External agents, including one working in an Arena sandbox, can use Perveance to create and inspect art without controlling the GUI. Both GUI and headless interface share the same operations and project persistence. A direct Arena connector/API is not assumed to exist; MCP or another protocol may be explored later, but must not be advertised as supported until implemented and the host's support is verified.

## Proposed CLI contract (interface design, not implemented commands)

- Discover: machine-readable product/API version, installed capabilities with version, signatures/operations, required permissions and current support status.
- Projects: create/open, inspect canvas/layers/objects/revision, save/checkpoint, validate and close. Explicit workspace root and input/output paths, never implicit unrestricted home-directory access.
- Edit: submit versioned, typed operations in an atomic transaction, including raster writes, layer editing and capability-defined payload operations. Return stable IDs, revision and undo/checkpoint reference. Preconditions reject stale revision; do not overwrite an active GUI session's edits.
- Preview and export: render a bounded preview and PNG at explicit size/color/alpha; later support additional formats only when actually installed. Return file paths relative to the authorized workspace, dimensions and hashes.
- Tasks: start/cancel/query creative tasks, monitor capability-build progress, inspect cost/permission scope, logs and final status. Headless creative tasks need explicitly configured supported provider and sandbox; basic manual operations need neither.
- Capability lifecycle: discover/read status and logs, list installed/staged versions, request rollback via the supervisor under policy. No direct registry writes from an untrusted process.

Use a stable documented JSON schema or equivalent machine-readable protocol, predictable exit codes, structured errors, explicit API compatibility and bounded output/log sizes. Long-running tasks expose job IDs and durable events; a synchronous convenience mode may wait. Document schema evolution and error taxonomy. Do not claim a protocol is available before it is implemented.

## Arena workflow

1. The Arena session must have a usable Perveance build (or supported installation), allowed execution environment and access to the artwork/project. Dependency installation, connectivity, GPU and sandbox availability vary by session.
2. Ask the agent in Arena to make an asset. It queries supported operations and works in the workspace; it inspects generated previews with whatever image-viewing tools that session supplies, then saves/exports the deliverables.
3. The agent can present generated files from that workspace. An editable project can later be opened in the GUI when transferred to a compatible Perveance installation.
4. Missing capabilities may be built only if the Perveance supervisor's verified isolation and resource policy can operate in that environment. If nested sandboxing or provider access is unavailable, fail closed or do the feature-development work through the normal supervised repository workflow; do not silently execute unrestricted model code.

This does not confer access to the user's local desktop, installed software, keys or network. Arena cannot be assumed callable from Perveance. A future local service/API bridge would need explicit discovery, authentication, consent and verified Arena support.

## Concurrent editing and file safety

Use project revisions and explicit transaction preconditions for headless access; detect conflicts with a GUI or other agents, reject stale writes and offer reload/rebase where safe. No blanket destructive command. Outputs use explicit paths and safe atomic replacement. A sandboxed external agent may only access the files it is given; Perveance must not escalate that access.

## Prototype implementation: data-only proposal handoff

`inspect` exposes a content fingerprint; `apply-proposal` and the GUI can check a bounded version-1 proposal against the loaded/in-memory snapshot and apply existing typed raster operations transactionally. GUI application is one undoable edit. The CLI still writes a new output file only. The hash is not a lock, an authorization token, a capability registry, or a way to see another process's unsaved edits. No provider, harness, agent execution or sandbox is implemented by this handoff; arbitrary code and commands are not proposal operations.

## Experimental headless prompt-to-pixels

The opt-in `prompt-openrouter` CLI command reads a UTF-8 prompt file and a key from non-terminal stdin, asks the same fixed OpenRouter adapter for restricted typed edits, checks and applies them to one loaded snapshot, and writes a create-new project. It is not an agent harness, credential broker, sandbox, code execution system or live-validated provider integration. Network requests can incur charges and no spend cap exists; output preflight is not a reservation, and input changes after loading are not detected. See README for confidentiality and operational limitations.

An explicit `--include-composite` option supplies at most 1,024 visible composite pixels as RGBA hex text to the provider; hidden layer pixels, layer names, paths and the project revision are not sent; dimensions and layer count are still sent. It is off by default and can increase token costs. The GUI offers a separate one-request opt-in. This is not vision-model integration or live-validated visual understanding.

## Experimental fail-closed intake inspection

`inspect-autonomy-request` validates a strict bounded task request with prompt, budget declarations, and project revision against trusted caller-provided or default intake ceilings. A valid response is always `blocked`; `ok: true` means only that validation succeeded. There is no job ID, execution permit, persistence, cancellation, measured resource usage, or enforcement. The prompt is omitted from the report; request files and paths remain the caller's responsibility. This inspection does not control the separate experimental `prompt-openrouter` command. See README for schema, limits and path/revision caveats.

The supervisor module also contains an isolated in-memory model-call reservation counter for future trusted broker work. Nothing currently uses it to make provider calls, it is not persistent, and it cannot guarantee billing; it does not change either the blocked autonomy result or the standalone provider's behavior.

## Arena offline project bridge

`tools/arena_art.py` is a limited Python standard-library, data-only path for an external agent to create, inspect, sample/read bounded pixel regions, edit into a new file and export editable raster projects and nearest-neighbor scaled PNGs without a Rust toolchain. It implements the current typed raster batch operations with stricter project limits, but not all Rust CLI commands. CI cross-checks representative bridge projects, layer/canvas edits, pixel regions, brushes, shapes and PNG pixels against the Rust model. A native X11 test covers a bridge-created project edited in the GUI and then read and revised by the bridge; this remains file handoff, not a live connector. This is independent code, not a running Rust app or an Arena invocation integration. It supports revision-bound v1 proposals against the loaded snapshot, but has no file locking, sandbox or autonomous feature development; callers must have the checkout and trusted local files. See README for usage and constraints.

## Prototype raw-region edits

`copy_region` and `clear_region` are typed batch/proposal operations in the
Rust model/CLI and limited Python bridge. They operate on raw layer bytes,
including hidden and zero-alpha RGB data; paste replaces rather than blends.
Source and destination must fit completely, with no implicit clipping. A copy
snapshots its source, including for same-layer overlaps. Ordered copy followed
by clear is not an atomic overlapping move: the clear can erase the new copy.
Use revision-bound proposals when editing a known snapshot; the existing
create-new and file-locking limitations still apply. The GUI's selection and
internal copy buffer are session-local and are not exposed to headless agents.
The OpenRouter adapter does not permit these new operations.
