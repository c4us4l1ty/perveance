# Perveance

An early Rust prototype for an AI-first art studio. The long-term vision and security model are documented in [Plan/README.md](Plan/README.md); the current usable-editor milestone is in [Plan/PIXEL_MVP.md](Plan/PIXEL_MVP.md).

## Current prototype

- A native Linux desktop window with a pixel canvas, pencil, eraser, fill, line, rectangle, ellipse, composite-eyedropper and rectangular region-editing tools, zoom/grid, layer management, bounded undo/redo, and a limited save/open and PNG export flow.
- An experimental, opt-in OpenRouter prompt-to-pixels adapter in the GUI and CLI, restricted to creating and painting new raster layers. Live provider/model compatibility has not yet been verified.
- A headless CLI backed by the same simple project model. Run `cargo run --locked --bin perveance-cli -- capabilities` for the current operations and `cargo run --locked --bin perveance` for the GUI.
- A new-file-only save/export policy: outputs cannot overwrite an existing file. Writes are staged in the target directory before installation. The simple JSON format is experimental and not yet a stable interchange format.

This is **not** the finished product: autonomous development, sandbox enforcement, vector artwork, a full editor, atomic/revision-aware project updates, and broad OS support are not implemented. Do not rely on this prototype for important artwork. It does not execute generated code.

## Development and verification

Use a current stable Rust toolchain and the Linux X11 development dependencies, including the `libxkbcommon-x11` runtime library. `cargo test --locked --all-targets` runs the Rust tests. GitHub Actions compiles and runs these tests and a CLI round trip. The Python diagnostic `python3 tools/phase0_probe.py` reports a few local development prerequisites; it is **not** a sandbox certification. Run its tests with `python3 -m unittest discover -s tests -v`. A separate fixed-test diagnostic, `python3 tools/isolation_probe.py`, checks a few bubblewrap behaviors on Linux and exits nonzero if unavailable or failed. It never runs model code or enables autonomy, and passing is **not** sandbox certification.

CI runs on pull requests and pushes to `main` (or by manual workflow dispatch), not on a feature-branch push without a pull request. Superseded pull-request runs may be canceled to reduce duplicate work. A queued or canceled run is not a passing test.

### Experimental Linux evaluation bundle

Successful **Rust CI** runs upload an unsigned `perveance-linux-x86_64` Actions artifact containing a compressed archive, checksums, a Cargo license-inventory sidecar, desktop and CLI executables, instructions and example operations. This is an evaluation build, not a formal release. From a successful trusted run, download the artifact from its GitHub Actions page (GitHub sign-in may be required), or with GitHub CLI:

```sh
gh run download RUN_ID --name perveance-linux-x86_64 --dir perveance-download
cd perveance-download
sha256sum -c SHA256SUMS
tar -xzf perveance-linux-x86_64.tar.gz
./perveance-linux-x86_64/bin/perveance
```

Run the GUI from a writable working directory on an x86-64 Linux desktop with compatible glibc and X11/GL runtime libraries. The bundle records its build revision and glibc version in `BUILD-INFO.txt`; it is not a self-contained package and compatibility with other hosts is not established. CI downloads the uploaded artifact, checks the checksum, exercises its CLI save/reopen/export flow and launches its GUI in virtual X11. This is not a real-desktop compatibility test. Artifacts expire after 14 days, require a trusted successful workflow, and are not signed: the checksum detects corruption, not authenticity. The sidecar `LICENSE-INVENTORY.json` summarizes Cargo metadata for the target-resolved graph; it may include build/dev dependencies and is not an exact inventory of linked code. License strings and license-file flags are unverified metadata. System libraries, assets, license texts and redistribution obligations are not covered; complete license and legal review remains outstanding. The sidecar is outside the archive and covered by `SHA256SUMS`; local `tools/package_linux.sh` runs do not generate it. Windows, macOS, ARM and native Wayland are not supported by this bundle. No autonomous execution is enabled.

### Arena agent: offline data-only bridge

An external agent with this checkout and Python 3 can create Perveance-compatible editable raster projects and PNGs without Rust, a provider key, bubblewrap, a display, or network access:

```sh
python3 tools/arena_art.py capabilities
python3 tools/arena_art.py create 16 16 examples/potion-operations.json potion.perveance.json
python3 tools/arena_art.py inspect potion.perveance.json
python3 tools/arena_art.py sample potion.perveance.json 6 7 composite
# Return a revision-bound grid of composite or raw layer RGBA pixels (up to 4096).
python3 tools/arena_art.py read-region potion.perveance.json 0 0 16 16 composite
python3 tools/arena_art.py export-png potion.perveance.json potion.png
# Optional larger nearest-neighbor preview (integer scale 1–8):
python3 tools/arena_art.py export-png-scaled potion.perveance.json potion-preview.png 8
# Further edits, written to a separate new project:
python3 tools/arena_art.py apply potion.perveance.json edited.perveance.json more-operations.json
# Or check a v1 proposal's base_revision from inspect before applying:
python3 tools/arena_art.py apply-proposal potion.perveance.json edited.perveance.json proposal.json
```

The operations file is a JSON array in the Rust CLI's format. The bridge supports the current typed raster batch operations: `add_layer`, `duplicate_layer`, `clear_layer`, `clear_region`, `copy_region`, `replace_color`, `flip_layer`, `translate_layer`, `resize_canvas`, `rename_layer`, `move_layer`, `remove_layer`, `set_visibility`, `pixel`, `line`, `brush_line`, `rectangle`, `rectangle_outline`, `ellipse`, and `fill`. Its project and file limits remain stricter than Rust's; CLI commands and PNG import are not fully supported. Use `sample ... layer LAYER` or `read-region ... layer LAYER` for raw layer pixels (including hidden pixels). Region results are row-major, retain raw RGB under zero alpha, and report the snapshot revision. A brush is a clipped 1–16 pixel square, and even sizes are biased toward the top left; ellipses follow the Rust pixel-center rasterization. Straight-alpha RGBA compositing follows the Rust model; CI compares batch results, representative translucent composites, and bridge PNGs against the Rust implementation. A native X11 test exercises a file handoff from the bridge to a GUI edit and back to a revision-bound bridge edit; it is not a live connector or real-desktop validation. This is a separately implemented, limited compatibility bridge—not the Rust app running inside Arena, an Arena connector, or full CLI parity. Projects can be opened in Perveance when transferred to an available build, but the project format is experimental.

Bridge limits are 128×128, 16 layers, 1,024 operations, 256 KiB batch input and 8 MiB project input/output; scaled PNGs may be up to 1024×1024. The Rust app has different limits, and scaled PNGs larger than 256×256 cannot be reimported into it. Inputs are loaded snapshots. `inspect`, `sample` and `read-region` report the Rust-compatible content revision, and `apply-proposal` rejects a mismatched v1 base revision; plain `apply` does not check one. Revision checks are not locks or atomic compare-and-swap and cannot see unsaved GUI changes or later disk edits. Output files are create-new and installed through a same-directory temporary file and hard link; existing paths are not overwritten. Directories and inputs must be trusted; symlinks are followed on input, hard-link support is required, and a crash or cleanup error may leave temporary or completed files. There is no precise CPU/memory quota. The bridge runs no generated code and supplies no sandbox, supervisor, model, or automatic self-extension; an Arena agent must itself choose the operations and have access to the checkout and output directory. Do not put secrets in artwork or paths.

### Headless batch editing

The prototype CLI can apply a JSON array of up to 1,024 typed operations in one all-or-nothing in-memory transaction. Pass it as `perveance-cli apply INPUT OUTPUT OPERATIONS_JSON`; `OUTPUT` must not exist. Supported operations are `add_layer`, `duplicate_layer`, `clear_layer`, `clear_region`, `copy_region`, `replace_color`, `flip_layer`, `translate_layer`, `resize_canvas`, `set_visibility`, `rename_layer`, `move_layer`, `remove_layer`, `pixel`, `line` (inclusive integer endpoints), `brush_line`, `rectangle` (filled; width and height), `rectangle_outline`, `ellipse` (filled or outlined) and `fill` (four-connected, on the selected layer). Layers are numbered from zero, bottom to top. For example:

```json
[
  {"op":"add_layer"},
  {"op":"fill","layer":1,"x":0,"y":0,"color":[30,40,50,255]},
  {"op":"line","layer":1,"from":[0,0],"to":[7,7],"color":[255,200,60,255]}
]
```

Layer duplicates are inserted immediately above their source and retain its visibility and exact pixel bytes. `flip_layer` mirrors a full layer across the canvas (`"axis":"horizontal"` or `"axis":"vertical"`); `clear_layer` resets all its pixels to transparent black. These edits are undoable in the GUI. Layer indices can change after a duplicate or reorder, so later batch operations use the resulting ordering.

### Tool shortcuts

When the canvas or editor has focus, use **B** for Pencil, **E** Eraser, **G** Fill, **R** Rectangle, **O** Ellipse, **L** Line, **I** Eyedropper, and **M** Select region. The Tools panel has a collapsible shortcut reference and each tool's tooltip shows its key. These unmodified-key shortcuts do not change artwork by themselves, and are ignored while typing, a dialog is open, or a drawing gesture is active. **Ctrl+O** still opens a project; **Ctrl+S** saves; **Ctrl+Z** undoes; **Ctrl+Shift+Z** or **Ctrl+Y** redoes.

### Rectangular region editing

Choose **Select region** (**M**) and drag in either direction; endpoints are inclusive. A click selects one pixel. In this tool, use the buttons or unmodified **C** to copy, **X** to cut, **V** to paste, **Delete** to clear, **Escape** to deselect, and **Ctrl+A** to select the whole canvas. These actions are ignored while typing, in dialogs, or during a drawing gesture. C/X/V deliberately do not use Ctrl: this is an **internal session buffer**, not the operating system clipboard. No image or text is read from or sent to the OS clipboard.

Copy/cut reads the **selected raw layer**, even if hidden, preserving straight-alpha RGBA bytes including RGB under zero alpha. Copy and selection do not modify artwork or undo history. Cut, clear and paste are each one undoable edit; failed or no-op edits preserve redo. Undo history is bounded, so save important work first. Clear and cut write transparent black; they do not affect other layers.

To paste elsewhere, click or drag a new destination, optionally choose another layer, then press V. Only the destination's top-left is used; the copied rectangle keeps its size. Without a selection, paste uses the original copied coordinates. The entire rectangle must fit; an out-of-bounds paste fails without clipping or changing artwork. **Paste replaces raw pixels, including transparent ones; it does not alpha-blend.** A paste may erase existing destination artwork.

The buffer survives layer changes, new/open canvases and undo during the session, but is not saved to disk and is lost on exit. Selection bounds are session-only, separate from undo, and cleared when switching to another tool, replacing the canvas or using Resize. This is a region-editing aid, **not a painting mask**, floating selection, resizable transform or vector object.

Headless batches and revision-bound proposals support the equivalent data-only operations:

```json
[
  {"op":"copy_region","layer":0,"x":1,"y":2,"width":3,"height":2,"to_layer":1,"to_x":5,"to_y":6},
  {"op":"clear_region","layer":0,"x":1,"y":2,"width":3,"height":2}
]
```

Both layers must already exist and both rectangles must fit. `copy_region` snapshots the source before writing, so overlapping same-layer copies do not smear. It does not clear the source. Operations run in order: a later `clear_region` can erase an overlapping destination, so it is not a general overlapping move command. Both operations are also supported by the limited Python bridge, under its stricter bounds; the experimental OpenRouter adapter rejects them. Edits can cover the entire bounded canvas (256×256 in Rust, 128×128 in the bridge); the separate `read-region` JSON inspection remains limited to 4,096 pixels.

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

### Choosing files and directories

Use **Open…** in the top bar or Ctrl+O to choose and open an existing project. The file name starts focused; type a name and press Enter, or select a listed file and choose **Open**. Pressing Enter in the directory field navigates there instead. Escape cancels. If artwork is modified, opening asks for confirmation before discarding it; a failed open retains the current artwork.

Use **Browse…** beside the project, PNG, palette, proposal or recovery-directory path to navigate without typing the whole path. Pick a directory or file name and select **Use path**; this only fills the associated path field. Opening, saving, importing and exporting still require their separate buttons and retain their existing confirmation and no-overwrite behavior. Cancel or Escape leaves the field unchanged. File names for new outputs can be typed in the browser; select **Go** after editing the directory.

This is a simple in-app browser, not the operating system's file dialog. It lists directories, regular files and links without filtering extensions; hidden entries are optional. Listings are limited to 512 entries including hidden entries, and non-UTF-8 or special-file entries are omitted. For larger directories, enter a trusted path manually. Non-UTF-8 paths are not supported by the GUI; special-file entries are omitted and opening special files may block. Links are not followed while listing, but the existing open/import operations follow input symlinks. The browser is not a sandbox, path authorization mechanism, file lock or reservation, and trusted directories are required.

### Saving project versions

The top-bar **Save** button and Ctrl+S use the same no-overwrite workflow. For a project without a saved path, they create the file named in the Project section's **File path** field (initially `art.perveance.json`, relative to the working directory). If that path exists, saving fails; choose a new path. Once a project is open or saved, edits are saved to the next numbered copy beside that project. If there are no edits, Save checks the source and does not create another copy. The top-bar **Not saved**, **Modified**, and **Saved** indicators describe the editor's in-memory snapshot; they do not monitor external changes. Ctrl+S is ignored while a text field, dialog or drawing gesture has focus; click the canvas or use the button.

You can also open a project directly when starting the built desktop app: `perveance /path/to/project.json` (or `cargo run --locked --bin perveance -- /path/to/project.json`). Paths must be valid UTF-8. If the file cannot be opened or validated, the program reports an error and exits instead of silently opening a blank document.

After opening a project or saving it as a new file, **Save next version** creates a numbered copy alongside the last opened/saved project: for example, `art.perveance.json` becomes `art.perveance.v001.json`, then `art.perveance.v002.json`. Existing names, directories and symlinks are skipped; a concurrent claimant causes the save to fail instead of being overwritten. The editor switches its current project path to the successful copy and keeps undo history. The path field does not select the source for version saves; use **Save as new file** to choose a different destination. New canvases and PNG imports must first be saved as a new project.

Before creating a version, the editor reloads the last opened/saved file and compares its project content to the saved snapshot. If it changed, was removed, or cannot be read, the version save is refused and the current artwork and undo history are kept. Save your work under a new name or reopen the other file after handling unsaved changes. Formatting-only changes that decode to the same project are not conflicts. This comparison is a best-effort check, **not a lock or atomic compare-and-swap**: changes after the check may go undetected. Version saving never overwrites the source or any other existing path. The usual trusted-path, filesystem, crash and durability caveats apply; it is not automatic backup or crash recovery.

### Opt-in recovery checkpoints

Recovery is **off by default**. In the Project panel, expand **Recovery checkpoints**, enter a trusted existing directory and enable checkpoints. Alternatively start the desktop app with `perveance --recovery-dir /path/to/directory [project.json]`. The directory must already exist; no recovery setting is persisted between runs. All file and directory paths must be trusted, and input symlinks are followed.

While enabled, the editor checks roughly every 15 seconds and writes a separate, no-overwrite project file if the artwork has unsaved changes since the last checkpoint. Use the top-bar **Checkpoint** button or **F9** to request one immediately, including for a clean project. F9 is ignored while typing, in dialogs, or during drawing gestures. Manual and automatic checkpoints share the eight-file limit and never mark the project saved. An identical project is not checkpointed again; this deduplication does not check whether the earlier file was moved or deleted. A successful manual checkpoint restarts the automatic interval. It waits until drawing and replacement/close dialogs finish. It writes at most **eight** checkpoints per enablement, then stops and reports that limit. Re-enabling starts a new session; there is no global disk quota. Large projects can make checkpointing slow and use substantial disk space. A write failure stops checkpointing and reports an error without marking the artwork saved.

Checkpoints contain the **entire project, including hidden layers and names**. They remain after saving, discarding, exiting or crashing and are never automatically deleted. Store them in a private location and remove them yourself when no longer needed. To recover, open a `perveance-recovery-*.perveance.json` file in the GUI and save your work under a separate project name. This is a best-effort snapshot, not a journal or guarantee: edits made after the latest checkpoint, during an unfinished gesture, before the first interval, or after a write failure may be lost. It does not protect against all crashes, disk failure, or power loss; directory syncing is not performed. Checkpoints do not change the saved/modified state or replace normal saves and backups.

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
