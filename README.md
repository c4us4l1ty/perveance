# Perveance

An early Rust prototype for an AI-first art studio. The long-term vision and security model are documented in [Plan/README.md](Plan/README.md).

## Current prototype

- A native Linux desktop window with a pixel canvas, pencil, eraser, layer visibility, bounded undo, and a limited save/open and PNG export flow.
- A headless CLI backed by the same simple project model. Run `cargo run --bin perveance-cli -- capabilities` for the current operations and `cargo run --bin perveance` for the GUI.
- A new-file-only save/export policy: outputs cannot overwrite an existing file. The simple JSON format is experimental and not yet a stable interchange format.

This is **not** the finished product: AI providers, autonomous development, sandbox enforcement, vector artwork, a full editor, atomic/revision-aware project updates, and broad OS support are not implemented. Do not rely on this prototype for important artwork. It does not execute generated code.

## Development and verification

Use a current stable Rust toolchain and the Linux X11 development dependencies. `cargo test --all-targets` runs the Rust tests. GitHub Actions compiles and runs these tests and a CLI round trip. The Python diagnostic `python3 tools/phase0_probe.py` reports a few local development prerequisites; it is **not** a sandbox certification. Run its tests with `python3 -m unittest discover -s tests -v`.
