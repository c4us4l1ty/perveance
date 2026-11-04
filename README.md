# Perveance

An early Rust prototype for an AI-first art studio. The long-term vision and security model are documented in [Plan/README.md](Plan/README.md).

## Current prototype

- A native Linux desktop window with a pixel canvas, pencil, eraser, fill, line, rectangle, ellipse and composite-eyedropper tools, zoom/grid, layer management, bounded undo/redo, and a limited save/open and PNG export flow.
- An experimental, opt-in OpenRouter prompt-to-pixels adapter in the GUI and CLI, restricted to creating and painting new raster layers. Live provider/model compatibility has not yet been verified.
- A headless CLI backed by the same simple project model. Run `cargo run --locked --bin perveance-cli -- capabilities` for the current operations and `cargo run --locked --bin perveance` for the GUI.
- A new-file-only save/export policy: outputs cannot overwrite an existing file. Writes are staged in the target directory before installation. The simple JSON format is experimental and not yet a stable interchange format.

This is **not** the finished product: autonomous development, sandbox enforcement, vector artwork, a full editor, atomic/revision-aware project updates, and broad OS support are not implemented. Do not rely on this prototype for important artwork. It does not execute generated code.

## Development and verification

Use a current stable Rust toolchain and the Linux X11 development dependencies, including the `libxkbcommon-x11` runtime library. `cargo test --locked --all-targets` runs the Rust tests. GitHub Actions compiles and runs these tests and a CLI round trip. The Python diagnostic `python3 tools/phase0_probe.py` reports a few local development prerequisites; it is **not** a sandbox certification. Run its tests with `python3 -m unittest discover -s tests -v`. A separate fixed-test diagnostic, `python3 tools/isolation_probe.py`, checks a few bubblewrap behaviors on Linux and exits nonzero if unavailable or failed. It never runs model code or enables autonomy, and passing is **not** sandbox certification.

### Headless batch editing

The prototype CLI can apply a JSON array of up to 1,024 typed operations in one all-or-nothing in-memory transaction. Pass it as `perveance-cli apply INPUT OUTPUT OPERATIONS_JSON`; `OUTPUT` must not exist. Supported operations are `add_layer`, `duplicate_layer`, `clear_layer`, `replace_color`, `flip_layer`, `translate_layer`, `resize_canvas`, `set_visibility`, `rename_layer`, `move_layer`, `remove_layer`, `pixel`, `line` (inclusive integer endpoints), `brush_line`, `rectangle` (filled; width and height), `rectangle_outline`, `ellipse` (filled or outlined) and `fill` (four-connected, on the selected layer). Layers are numbered from zero, bottom to top. For example:

```json
[
  {"op":"add_layer"},
  {"op":"fill","layer":1,"x":0,"y":0,"color":[30,40,50,255]},
  {"op":"line","layer":1,"from":[0,0],"to":[7,7],"color":[255,200,60,255]}
]
```

Layer duplicates are inserted immediately above their source and retain its visibility and exact pixel bytes. `flip_layer` mirrors a full layer across the canvas (`"axis":"horizontal"` or `"axis":"vertical"`); `clear_layer` resets all its pixels to transparent black. These edits are undoable in the GUI. Layer indices can change after a duplicate or reorder, so later batch operations use the resulting ordering.

### Pencil and eraser brush size

Select Pencil or Eraser and adjust **Brush size** from 1 to 16 pixels. Strokes stamp a solid square along an inclusive pixel line; stamps are clipped at the canvas edges. Even sizes are biased one pixel toward the top and left of the pointer. The pencil replaces raw RGBA rather than blending; the eraser writes transparent black. A continuous stroke is one undoable edit. Brush size is session-only and does not change the separate line or shape tools.

Agents can use `{"op":"brush_line","layer":0,"from":[1,1],"to":[8,8],"size":3,"color":[255,0,0,255]}` in a batch or revision-bound proposal. Size 1 matches `line`. The experimental OpenRouter adapter does not accept `brush_line`.

### Replace a layer color

In the Layers panel, expand **Replace layer color**. Set the exact match RGBA bytes manually, or pick a color and click **Use current color as match**. Then choose the replacement as the current drawing color and click **Replace matches**. The operation affects every matching raw pixel on the selected layer, including hidden layers and fully transparent pixels. It is undoable; undo history is bounded, so keep backups.

For agents, `{"op":"replace_color","layer":0,"from":[1,2,3,255],"to":[4,5,6,128]}` works in batches and revision-bound proposals. Matching is exact in all four straight-alpha bytes, not a tolerance or visible-composite match. It can turn transparent pixels opaque or vice versa. The experimental OpenRouter adapter does not accept this operation.

### Moving a raster layer

Use the four **Nudge selected layer** buttons in the Layers panel to move its pixels one canvas pixel at a time. This is undoable and works on hidden layers as well. By default a move is rejected if it would discard any nonzero RGBA data, including RGB bytes of transparent pixels. To permit loss at the canvas edge, enable **Allow clipping** first. Vacated pixels are transparent black; pixels do not wrap. The clipping checkbox is session-only.

For headless batches or revision-checked proposals, use an explicit signed offset and clipping choice:

```json
{"op":"translate_layer","layer":0,"dx":1,"dy":-2,"clip":false}
```

Offsets range from -256 to 256 per axis. `clip` is required: `false` rejects data loss atomically; `true` discards out-of-bounds pixels and can clear an entire layer. Back up artwork and review untrusted proposals. This translates raster pixels, not a non-destructive object transform. The experimental OpenRouter adapter does not permit this operation.

All colors are unpremultiplied RGBA bytes. A failed operation leaves the source project unchanged and does not create the requested output. These are editable raster operations, not vectors or generated executable code. Project saves and PNG exports stage and sync a temporary file before a no-clobber installation. Guarantees depend on the filesystem and platform; directory durability across power loss is not guaranteed, and a crash can leave a temporary file.

The GUI can create a new canvas up to 256 × 256, zoom and scroll, and rename, reorder, duplicate, flip, clear or remove layers. Layer edits are undoable and redoable within a bounded local history (Ctrl+Z, Ctrl+Shift+Z, or Ctrl+Y). Opening or creating a canvas prompts immediately before replacing unsaved artwork; cancel to keep editing or discard and replace. Replacement clears undo history and cannot itself be undone. The editor also warns before a normal window close if the project differs from the last successful project save or open. PNG exports do not mark a project saved. This is not crash recovery and cannot prevent data loss from force-quitting, system failure or external file changes. Unsupported project fields are rejected rather than silently discarded.

### Resizing a canvas

In the GUI, expand **Resize existing canvas** in the Project section and enter the new width and height (1–256). Resize is undoable and affects every raster layer, including hidden layers. It is anchored at the top-left and does not scale or resample; added pixels are transparent black. Shrinking is rejected if it would discard nonzero RGBA data, including hidden RGB in fully transparent pixels, unless **Allow clipping** is explicitly enabled. Clipping can permanently discard artwork; undo history is bounded, so keep backups. After a successful resize the clipping option resets.

Agents can use `{"op":"resize_canvas","width":64,"height":64,"clip":false}` in a batch or revision-bound proposal. The `clip` field is required and `true` permits destructive cropping. Changes are transactional; the experimental OpenRouter adapter cannot resize the canvas. This is a raster canvas resize, not image scaling or a non-destructive transform.

### Palette

Below the GUI color picker, select one of 16 numbered swatches to set the drawing color. Choose a slot, adjust the color picker (or use the eyedropper), then press **Store current color** to replace that slot, including alpha. **Reset palette** restores the default swatches and selects slot 1 without changing the current drawing color. Swatch previews blend transparency over gray; hover a slot to see its exact RGBA values.

Swatches are separate from the project and undo history. To keep them, expand **Palette files**, enter a path, and save as a new file; existing files are never overwritten. Loading a palette explicitly replaces the 16 swatches and selects slot 1, but leaves the current drawing color unchanged. An invalid or unreadable file leaves the palette unchanged. Palette settings are otherwise session-only. Choosing, saving or loading swatches does not change artwork; drawing with them is undoable as usual.

Palette files are version-1 JSON with exactly 16 straight-alpha RGBA colors, for example `{"version":1,"colors":[[0,0,0,255], ...]}` (the ellipsis is illustrative, not valid JSON). Files are limited to 8 KiB and unknown fields are rejected. Agents can inspect one using `perveance-cli inspect-palette palette.perveance.json`. Palette saves use the same staged create-new mechanism and filesystem caveats as projects.

### Scaled PNG export

The GUI Project section has a separate **PNG export path** and an integer **Scale** control from 1 to 8. Export uses nearest-neighbor pixel enlargement of the visible composite, without changing the editable project or marking it saved. It creates a new file and refuses to overwrite an existing one.

The headless equivalent is:

```sh
perveance-cli export-png-scaled potion.json potion-4x.png 4
```

For example, a 16 × 16 canvas at 4× becomes a 64 × 64 PNG. Export is bounded to 2,048 × 2,048 pixels at the maximum project size and scale. Alpha and visible-layer compositing match regular PNG export; no interpolation or color management is applied. `export-png` remains the 1× command. The project/PNG import limit remains 256 × 256, so a larger exported PNG cannot be imported back into this prototype; keep the editable project.

### Example layered asset

The repository includes [`examples/potion-operations.json`](examples/potion-operations.json), a small editable pixel-art potion. From the repository root, with Rust installed:

```sh
cargo run --locked --bin perveance-cli -- new 16 16 potion-base.json
cargo run --locked --bin perveance-cli -- apply potion-base.json potion.json examples/potion-operations.json
cargo run --locked --bin perveance-cli -- export-png potion.json potion.png
```

These commands require paths that do not already exist. Open `potion.json` in the GUI to edit its layers. `perveance-cli capabilities` reports example operation objects and their limits in JSON.

### Importing PNG artwork

The studio can open a static PNG as a new canvas, or import one as an editable top layer when its dimensions match the current canvas. Enter the PNG path in the left panel; replacing unsaved artwork requires a discard confirmation. Layer import can be undone.

The headless equivalents also produce new files without overwriting:

```sh
cargo run --locked --bin perveance-cli -- new-from-png image.png project.json
cargo run --locked --bin perveance-cli -- import-png project.json another-image.png layered-project.json
```

PNG input is limited to 8 MiB and dimensions of 1–256 pixels on each side. Static RGB, RGBA, grayscale, grayscale-alpha and indexed images are decoded to editable 8-bit RGBA; 16-bit samples are reduced to 8 bits. Animated PNGs are rejected. Metadata, ICC profiles and color management are not preserved or applied, so colors may differ from color-managed applications. Importing does not make a generated image's original objects or layers editable: the PNG is one raster layer. There is no resizing or alignment on layer import.

The line, rectangle and ellipse tools write raster pixels to the selected layer; they are not editable vector objects. Drag in either direction; rectangles and ellipses can be filled or outlined. Ellipse edges use a deterministic pixel-center rasterization, and outline pixels are the four-connected edge.

### Sampling colors

The GUI eyedropper samples the visible composited pixel, including alpha, without the checkerboard. It does not change the artwork or undo history. For agents, the CLI can inspect either the visible composite or a raw layer pixel (including hidden layers):

```sh
perveance-cli sample project.json 2 3 composite
perveance-cli sample project.json 2 3 layer 0
```

Both commands return straight-alpha RGBA bytes as JSON. Raw pixels preserve their RGB bytes even when alpha is zero; the visible composite uses the same blending as PNG export.

### Reading artwork regions headlessly

Agents can inspect a rectangular area without invoking the CLI once per pixel:

```sh
perveance-cli read-region project.json 0 0 16 16 composite
perveance-cli read-region project.json 0 0 16 16 layer 0
```

The JSON response contains `revision`, `x`, `y`, `width`, `height`, `source`, `layer` (null for composite), and `pixels`. Pixels are rows from top to bottom, each containing left-to-right straight-alpha RGBA byte arrays. Composite matches PNG export; raw layers include hidden pixels and preserve RGB even when alpha is zero. Regions must fit the canvas, have positive dimensions, and contain at most 4,096 pixels. Larger canvases can be read in tiles. The revision identifies the loaded snapshot, not a file lock: compare revisions across tile reads, and retry if they differ. As with `inspect`, this cannot see unsaved GUI edits or prevent external changes.

### Revision-checked agent handoff

External tools or agents can prepare a **data-only** edit proposal for a particular project snapshot. First obtain `revision` from `perveance-cli inspect project.json`, then write a JSON file such as:

```json
{
  "version": 1,
  "base_revision": "<revision returned by inspect>",
  "operations": [
    {"op":"add_layer"},
    {"op":"pixel","layer":1,"x":0,"y":0,"color":[255,0,0,255]}
  ]
}
```

Run `perveance-cli apply-proposal project.json edited.json proposal.json`; the output must not exist. Alternatively, in the GUI's **Agent handoff** section, copy the current in-memory revision and apply a proposal file. GUI application is one undoable edit. Proposals are limited to 256 KiB and 1,024 existing typed operations; unknown fields, stale revisions and invalid operations fail without applying any edits. The proposal format cannot run code or invoke commands, and an agent is not automatically connected to the GUI. A valid proposal can still clear pixels or delete layers; review untrusted proposals and keep backups. The size and operation-count limits are not CPU or execution-time quotas.

The revision is `sha256-json-v1:` followed by the lowercase SHA-256 of the compact v1 project serialization (including hidden layer pixels and metadata). It fingerprints the document, not its file formatting. **It is not a lock, permission token, or atomic compare-and-swap:** the CLI checks the snapshot it loads, cannot see unsaved GUI edits, and another process may change a file after it is read. It never replaces the input file. Cross-process coordination and conflict-safe in-place updates remain unimplemented.

### Experimental prompt-to-pixels

In the GUI, scroll to **Prompt to pixels**, enter a model ID and an OpenRouter API key in the app itself, then enter a prompt and click **Send to OpenRouter**. Do not send API keys in chat. The key is not stored in the project or passed to a subprocess; the input field clears when a request starts. It remains in process/HTTP memory temporarily, and secure memory erasure is not guaranteed.

By default this sends your prompt, canvas dimensions, layer count, and selected model ID to OpenRouter and the selected model provider; existing pixels, layer names, filenames and the project revision are not included. You can explicitly choose **Send visible artwork for this request** to include the visible composite; the checkbox resets after a request starts. The request is opt-in and may incur charges even when the result is rejected or the app closes. Output is capped at 2,048 tokens, but **there is no dollar spend cap**. Review the provider's terms and billing settings. Redirects and environment-configured proxies are disabled. Requests have a 60-second timeout, although DNS can exceed it; cancelling a pending request is not implemented.

When sharing is enabled, the current visible composite is sent as text rows of straight-alpha RGBA hex values, not as an image attachment. This transmits artwork that may be sensitive and can increase token usage and charges. Hidden layer pixels, raw pixels obscured by the composite, layer names, paths and revision are not included. A canvas may contain at most 1,024 pixels for this option; larger canvases fail before a request starts. Composite pixels with zero alpha are transparent black. Models may not understand this representation; visual understanding and live compatibility have not been verified.

The model is asked for JSON containing existing typed raster operations. The app only permits it to add and paint new layers, validates the complete batch, and applies it as one undoable edit if the project revision has not changed while the request was in flight. Invalid, stale, oversized, truncated, or disallowed results are rejected. A new top layer can still obscure existing artwork. This does **not** run generated code, build features, call an agent harness, or provide a sandbox. An adversarial provider/model remains untrusted.

#### Headless OpenRouter request

To opt in from the CLI, put the art prompt in a UTF-8 file and supply the key through redirected stdin (or a trusted secret manager pipe):

```sh
perveance-cli prompt-openrouter input.json output.json MODEL_ID prompt.txt < /path/to/private-key-file
perveance-cli export-png output.json output.png
```

Do not put a key in chat, the command line, shell history, project files or an environment variable for this command. If using a key file, secure its permissions and handle its lifecycle yourself. The CLI reads at most 512 key bytes plus an optional newline from stdin, waits for EOF, and refuses a terminal as stdin to avoid echoing a key. The key is held in CLI and HTTP process memory; secure erasure is not guaranteed. Other local processes, the invoking tool, a pipe or key file may expose it depending on the environment. There is no credential broker.

To include visible composite pixels in one CLI request, add `--include-composite` after `prompt.txt`. It is off by default and subject to the same 1,024-pixel limit. Successful output reports `shared_composite` as `true` or `false`. The prompt is still sent in both modes; do not put secrets in it.

The command makes a **paid network request** to the same fixed OpenRouter endpoint and uses the same restricted new-layer-only adapter described above. There is no dollar spend cap or cancellation; a request may be charged even if the response is rejected, the output already appears, or the save fails. The initial output-existence check is not a reservation. Input is loaded as a snapshot; edits to the input or unsaved GUI changes during the request are not detected. Output is a new project file only, with no overwrite. A successful response can also contain no effective edits. The CLI returns revisions and operation count, not a PNG; export separately. The prompt file is limited to 4,000 bytes. Use trusted paths and verify the resulting artwork.

Only local mock HTTP and simulated CLI pipeline tests have run; a real OpenRouter request and any specific model's JSON compatibility have **not** been tested. Some models may not support the requested JSON response mode. The provider error body is not displayed, so status errors can be generic. Do not rely on this prototype for important work.

### Experimental autonomy intake inspection

`perveance-cli inspect-autonomy-request PROJECT REQUEST_JSON [TRUSTED_POLICY_JSON]` validates a proposed task against a loaded project revision and budget ceilings. It is a **data-only inspection**: a valid request returns exit code 0 and `ok: true`, but `intake.status` is always `blocked`. No task is saved, no agent or worker runs, no network request is made, and the declared budgets are **not enforced or tracked**. A valid inspection is not an authorization to run. Existing `prompt-openrouter` is separate and is not governed by this inspection or its budgets.

Get `base_revision` from `perveance-cli inspect PROJECT`. Example request JSON:

```json
{
  "version": 1,
  "base_revision": "replace with revision from inspect",
  "prompt": "Make a tiny spaceship",
  "budgets": {
    "wall_seconds": 120, "cpu_seconds": 120, "memory_mib": 256,
    "scratch_mib": 128, "processes": 8, "model_calls": 4,
    "model_cost_microusd": 500000
  }
}
```

The optional trusted policy JSON has `version: 1` and a `maximum` object with exactly the same seven budget fields. Omitting it uses the ceilings in `capabilities.autonomy_intake.default_maximum`. Time values are seconds, storage/memory values are MiB, and cost is millionths of a US dollar. Wall/CPU/memory/scratch/process requests must be positive; model calls and cost may be zero. A zero maximum prohibits any positive request for that resource. These ceilings are prototype intake configuration, not an OS resource policy or guaranteed spend cap. Unknown/missing/duplicate fields, unsupported versions, over-budget requests, empty or over-4,000-byte UTF-8 prompts and stale revisions are rejected. The whole request is limited to 32 KiB and policy to 4 KiB.

The report omits the prompt, but the input file contains it; protect that file accordingly. Pass only trusted regular local files: path-based loading follows symlinks and special files can block. Revision checking compares a loaded snapshot; it is not a lock against later disk changes or unsaved GUI edits. It is not a durable job system, supervisor isolation, a credential broker or an activation mechanism.

### Experimental project staging diagnostic

On Linux, `perveance-cli verify-project-staging project.json TRUSTED_PARENT` stages a validated project in a private temporary directory, verifies that the only staged entry is a regular project file with matching content and revision, then removes the temporary directory. `TRUSTED_PARENT` must be an existing directory controlled by the user and not writable by untrusted processes. The source project path must also be trusted; normal project loading follows symlinks. The staged file includes **all** project data, including hidden pixels and layer names. The command does not copy neighboring files, directories, sockets or symlinks and does not start a worker.

This is a diagnostic and preliminary data-only staging primitive—not a sandbox, an atomic filesystem snapshot, or a defense against a malicious same-user process or a tampered parent directory. Normal cleanup is checked, but a crash or forced termination can leave temporary files. It does not validate future arbitrary workspace copying, attachments, quotas, isolation, credentials or output activation, and it cannot enable autonomous execution. Use only trusted local paths.

### Fixed-worker lifecycle diagnostic

On a compatible Linux host, `perveance-cli verify-fixed-worker PROJECT TRUSTED_PARENT` stages a project and invokes only a built-in diagnostic under `/usr/bin/bwrap`, then checks its summary and removes the staging directory. `TRUSTED_PARENT`, the source path, installed bubblewrap, Python and system libraries must be trusted; the project snapshot includes hidden pixels and names. It does not accept commands or model code. It fails if sandbox startup fails, rather than running unsandboxed.

The experimental runner limits each child output file to 8 KiB, treats more than 4 KiB on stdout or stderr as failure, and uses a best-effort 10-second timeout. Its library also exposes fixed test modes for cancellation, timeout and flooding; none are arbitrary executors. Before exec it marks descriptors above stderr close-on-exec using Linux `close_range`; if that syscall is unavailable or blocked, startup fails. A CI test uses one deliberately inheritable synthetic host-file descriptor; this is not a complete descriptor audit. It signals and reaps the immediate bubblewrap process, but does **not** provide a proven descendant-kill guarantee, a cgroup, total-disk/CPU/memory/process quotas or crash recovery. It binds system directories read-only and assumes path-based trusted staging. A diagnostic pass is not authorization, a production sandbox, or an autonomy gate. Outputs and error details are not returned; temporary cleanup can fail or be interrupted by a crash. The separate OpenRouter command is unaffected.

### Isolation feasibility diagnostic

The optional bubblewrap diagnostic creates its own temporary sentinel, two workspaces, local TCP listener and Unix socket, and runs only fixed test code. The first workspace contains a fixed input file and no host socket; it checks workspace access, synthetic host sentinel access (including through a symlink and `/proc` root links), the host socket's original path, host IPv4 loopback and removal of a synthetic environment variable. It then runs a negative control with a deliberately exposed host Unix socket in the second workspace and expects the worker to reach it. The clean socket check demonstrates that the fixture omitted the socket, **not** that bubblewrap blocks sockets in arbitrary workspaces. It prints JSON with `passed`, `failed`, or `unavailable`; all outcomes leave autonomous execution disabled. The probe does not copy real project inputs or validate the Rust staging implementation. A separate opt-in fixed-worker integration test exercises an actual staged synthetic project on the selected CI runner; it is not a production worker launcher.

A separate GitHub Actions matrix compares fixed probes on explicitly selected Ubuntu runner images without changing host AppArmor settings, using setuid or dropping network isolation. The Ubuntu 26.04 diagnostic job requires the narrow probe to pass; the other images report their results. Image labels and packages can change, so this is evidence about those CI runs, not automatic approval of a user's host. See `Plan/PHASE0_FINDINGS.md` for observed results and limitations.

A separate fixed-code test observed that an unprivileged user/network namespace blocked the local IPv4 listener but could still connect to the planted Unix socket on the shared filesystem in the development environment. This test is **not** a bubblewrap test and does not establish a complete sandbox. Future worker staging must address special files and host sockets rather than assuming network namespaces isolate them.

These are narrow observations, not a safety guarantee. In particular, the test binds host system directories read-only and does not test kernel escapes, all network paths, resource quotas, credential brokering or malicious build scripts. Do not use the diagnostic command as a sandbox for untrusted code. The tested build-and-activation supervisor still needs to be designed and independently validated.
