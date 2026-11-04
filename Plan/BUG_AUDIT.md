# Focused bug audit

Scope: review of the current raster editor, history, project persistence and headless interface. This is not a complete security audit or proof of correctness. Autonomous code execution and AI providers remain unimplemented.

## Findings addressed

1. **Valid projects could not be saved.** The 16 MiB JSON limit was lower than the worst-case 256×256 project with 32 layers and 8-bit RGBA values. The limit is now 40 MiB, with a regression test that saves and reopens a worst-case project including escaped layer names.
2. **Failed and no-op actions could discard redo.** The GUI previously checkpointed before knowing whether an action changed anything, including when adding beyond the layer limit, painting an unchanged pixel or renaming to the same name. Discrete edits now use a transactional history operation; strokes checkpoint on their first actual change. Failure/no-op history behavior is regression-tested.
3. **Undo/redo after a layer move could select a different layer.** GUI history snapshots now record the selected layer, and restore the selection alongside the project. Reorder undo/redo has a regression test.
4. **A failed write could leave a partial destination file.** New project and PNG writes are staged and file-synced in the destination directory, then installed without replacing an existing path. A concurrent-create test verifies one winner and intact output. Guarantees depend on platform/filesystem, and power loss may still leave a temporary file or lose the directory entry; no directory-durability claim is made. Use a trusted destination directory; staging does not defend against a malicious process with control over that directory.
5. **Layer controls became inaccessible for many layers.** The layer panel can now scroll.

## Continued editor work

Added Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y shortcuts when no text input or pointer gesture is active. Text fields retain their own shortcuts. CI compiles the Rust code and exercises the GUI startup, but full interaction, accessibility and multi-platform testing remain open.

Verification: the initial [CI run](https://github.com/c4us4l1ty/perveance/actions/runs/41805622949) passed Rust unit/integration tests and X11 startup using the newly resolved dependencies. The committed lockfile and final workflow need their own `--locked` check.
