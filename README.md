# Perveance

An early Rust prototype for an AI-first art studio. The long-term vision and security model are documented in [Plan/README.md](Plan/README.md).

## Current prototype

- A native Linux desktop window with a pixel canvas, pencil, eraser, fill, line, rectangle, ellipse and composite-eyedropper tools, zoom/grid, layer management, bounded undo/redo, and a limited save/open and PNG export flow.
- A headless CLI backed by the same simple project model. Run `cargo run --locked --bin perveance-cli -- capabilities` for the current operations and `cargo run --locked --bin perveance` for the GUI.
- A new-file-only save/export policy: outputs cannot overwrite an existing file. Writes are staged in the target directory before installation. The simple JSON format is experimental and not yet a stable interchange format.

This is **not** the finished product: AI providers, autonomous development, sandbox enforcement, vector artwork, a full editor, atomic/revision-aware project updates, and broad OS support are not implemented. Do not rely on this prototype for important artwork. It does not execute generated code.

## Development and verification

Use a current stable Rust toolchain and the Linux X11 development dependencies, including the `libxkbcommon-x11` runtime library. `cargo test --locked --all-targets` runs the Rust tests. GitHub Actions compiles and runs these tests and a CLI round trip. The Python diagnostic `python3 tools/phase0_probe.py` reports a few local development prerequisites; it is **not** a sandbox certification. Run its tests with `python3 -m unittest discover -s tests -v`.

### Headless batch editing

The prototype CLI can apply a JSON array of up to 1,024 typed operations in one all-or-nothing in-memory transaction. Pass it as `perveance-cli apply INPUT OUTPUT OPERATIONS_JSON`; `OUTPUT` must not exist. Supported operations are `add_layer`, `duplicate_layer`, `clear_layer`, `flip_layer`, `set_visibility`, `rename_layer`, `move_layer`, `remove_layer`, `pixel`, `line` (inclusive integer endpoints), `rectangle` (filled; width and height), `rectangle_outline`, `ellipse` (filled or outlined) and `fill` (four-connected, on the selected layer). Layers are numbered from zero, bottom to top. For example:

```json
[
  {"op":"add_layer"},
  {"op":"fill","layer":1,"x":0,"y":0,"color":[30,40,50,255]},
  {"op":"line","layer":1,"from":[0,0],"to":[7,7],"color":[255,200,60,255]}
]
```

Layer duplicates are inserted immediately above their source and retain its visibility and exact pixel bytes. `flip_layer` mirrors a full layer across the canvas (`"axis":"horizontal"` or `"axis":"vertical"`); `clear_layer` resets all its pixels to transparent black. These edits are undoable in the GUI. Layer indices can change after a duplicate or reorder, so later batch operations use the resulting ordering.

All colors are unpremultiplied RGBA bytes. A failed operation leaves the source project unchanged and does not create the requested output. These are editable raster operations, not vectors or generated executable code. Project saves and PNG exports stage and sync a temporary file before a no-clobber installation. Guarantees depend on the filesystem and platform; directory durability across power loss is not guaranteed, and a crash can leave a temporary file.

The GUI can create a new canvas up to 256 × 256, zoom and scroll, and rename, reorder, duplicate, flip, clear or remove layers. Layer edits are undoable and redoable within a bounded local history (Ctrl+Z, Ctrl+Shift+Z, or Ctrl+Y). Opening or creating a canvas replaces the current in-memory artwork only after enabling the replacement checkbox. Unsupported project fields are rejected rather than silently discarded.

### Example layered asset

The repository includes [`examples/potion-operations.json`](examples/potion-operations.json), a small editable pixel-art potion. From the repository root, with Rust installed:

```sh
cargo run --locked --bin perveance-cli -- new 16 16 potion-base.json
cargo run --locked --bin perveance-cli -- apply potion-base.json potion.json examples/potion-operations.json
cargo run --locked --bin perveance-cli -- export-png potion.json potion.png
```

These commands require paths that do not already exist. Open `potion.json` in the GUI to edit its layers. `perveance-cli capabilities` reports example operation objects and their limits in JSON.

### Importing PNG artwork

The studio can open a static PNG as a new canvas, or import one as an editable top layer when its dimensions match the current canvas. Enter the PNG path in the left panel; replacing the canvas requires the existing confirmation checkbox. Layer import can be undone.

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
