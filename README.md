# Perveance

An early Rust prototype for an AI-first art studio. The long-term vision and security model are documented in [Plan/README.md](Plan/README.md).

## Current prototype

- A native Linux desktop window with a pixel canvas, pencil, eraser, fill, zoom/grid, layer management, bounded undo, and a limited save/open and PNG export flow.
- A headless CLI backed by the same simple project model. Run `cargo run --bin perveance-cli -- capabilities` for the current operations and `cargo run --bin perveance` for the GUI.
- A new-file-only save/export policy: outputs cannot overwrite an existing file. The simple JSON format is experimental and not yet a stable interchange format.

This is **not** the finished product: AI providers, autonomous development, sandbox enforcement, vector artwork, a full editor, atomic/revision-aware project updates, and broad OS support are not implemented. Do not rely on this prototype for important artwork. It does not execute generated code.

## Development and verification

Use a current stable Rust toolchain and the Linux X11 development dependencies, including the `libxkbcommon-x11` runtime library. `cargo test --locked --all-targets` runs the Rust tests. GitHub Actions compiles and runs these tests and a CLI round trip. The Python diagnostic `python3 tools/phase0_probe.py` reports a few local development prerequisites; it is **not** a sandbox certification. Run its tests with `python3 -m unittest discover -s tests -v`.

### Headless batch editing

The prototype CLI can apply a JSON array of up to 1,024 typed operations in one all-or-nothing in-memory transaction. Pass it as `perveance-cli apply INPUT OUTPUT OPERATIONS_JSON`; `OUTPUT` must not exist. Supported operations are `add_layer`, `set_visibility`, `rename_layer`, `move_layer`, `remove_layer`, `pixel`, `line` (inclusive integer endpoints), `rectangle` (filled; width and height) and `fill` (four-connected, on the selected layer). Layers are numbered from zero, bottom to top. For example:

```json
[
  {"op":"add_layer"},
  {"op":"fill","layer":1,"x":0,"y":0,"color":[30,40,50,255]},
  {"op":"line","layer":1,"from":[0,0],"to":[7,7],"color":[255,200,60,255]}
]
```

All colors are unpremultiplied RGBA bytes. A failed operation leaves the source project unchanged and does not create the requested output. These are editable raster operations, not vectors or generated executable code. Project saves use exclusive creation but are not yet crash-atomic; an incomplete output may remain after an I/O failure.

The GUI can create a new canvas up to 256 × 256, zoom and scroll, and rename, reorder or remove layers. Layer edits are undoable. Opening or creating a canvas replaces the current in-memory artwork only after enabling the replacement checkbox. Unsupported project fields are rejected rather than silently discarded.

### Example layered asset

The repository includes [`examples/potion-operations.json`](examples/potion-operations.json), a small editable pixel-art potion. From the repository root, with Rust installed:

```sh
cargo run --locked --bin perveance-cli -- new 16 16 potion-base.json
cargo run --locked --bin perveance-cli -- apply potion-base.json potion.json examples/potion-operations.json
cargo run --locked --bin perveance-cli -- export-png potion.json potion.png
```

These commands require paths that do not already exist. Open `potion.json` in the GUI to edit its layers. `perveance-cli capabilities` reports example operation objects and their limits in JSON.
