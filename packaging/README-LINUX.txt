Perveance experimental Linux build
===================================

This is an unsigned test build of the early Perveance pixel-art prototype,
not a stable release. Back up important work. It does not run generated code
or enable autonomous capability development.

Requirements: x86-64 Linux with a compatible glibc and X11/GL runtime. The
build environment's glibc version and source revision are in BUILD-INFO.txt.
It was smoke-tested in a virtual X11 session on the build runner, not on all
Linux distributions, graphics drivers, or desktop environments. Newer library
versions are often compatible, but compatibility is not guaranteed. Windows,
macOS, ARM and native Wayland are not supported by this bundle.

To launch from a terminal in a writable directory on an X11 desktop:
  /path/to/perveance-linux-x86_64/bin/perveance

To open an existing UTF-8 project path directly:
  /path/to/perveance-linux-x86_64/bin/perveance /path/to/project.json

Use Open or Ctrl+O to choose and open a project; unsaved artwork requires
confirmation before replacement. Escape cancels the chooser or confirmation.
Use Browse beside a file or directory field to select a path. Use path only
fills the field; opening, saving, importing and exporting need separate actions.
This is an in-app browser, not a native OS dialog or security boundary. Large
directories may require manual path entry. Non-UTF-8 paths are not supported;
special-file entries are omitted and opening special files may block.

Tool shortcuts: B Pencil, E Eraser, G Fill, R Rectangle, O Ellipse, L Line,
I Eyedropper, and M Select region. In Select mode, C copies, X cuts, V pastes,
Delete clears and Escape deselects. This uses an internal session buffer, not
the OS clipboard; paste replaces raw RGBA without blending or clipping.
The Tools panel has a shortcut reference. Shortcuts are
ignored while typing, in dialogs and during active drawing gestures.

Use Save or Ctrl+S to create the first file (using the GUI's File path field),
or a numbered version after edits. Existing files are never overwritten.
Ctrl+S is ignored in text fields, dialogs and active drawing gestures.

Optional recovery checkpoints are off by default. To enable them, select an
existing trusted directory in the GUI's Recovery checkpoints section, or start:
  /path/to/perveance-linux-x86_64/bin/perveance --recovery-dir /path/to/directory
While enabled, use the top-bar Checkpoint button or F9 for an immediate
snapshot (ignored while typing, in dialogs, or during a drawing gesture).
Manual and automatic snapshots share the eight-file limit, skip identical
project snapshots, include hidden layers, and remain until manually removed.
Deduplication does not check that the old file still exists. Checkpoints do not
mark the project saved and are not a replacement for saving or backups; see the
main repository README for limits and recovery instructions.

The GUI uses the current working directory for its default project and PNG
paths. Save projects and exports in a directory you can write to. Project
and PNG saves never overwrite existing files. The project format is experimental;
version saves are not locking or automatic backup. Recovery snapshots are opt-in and best-effort.

To try the command-line editor from a writable directory:
  /path/to/perveance-linux-x86_64/bin/perveance-cli new 16 16 potion-base.json
  /path/to/perveance-linux-x86_64/bin/perveance-cli apply potion-base.json potion.json /path/to/perveance-linux-x86_64/examples/potion-operations.json
  /path/to/perveance-linux-x86_64/bin/perveance-cli export-png potion.json potion.png

Use new output filenames. Open potion.json in the GUI to edit the layers.
Run perveance-cli capabilities for supported commands. The independent Python
Arena bridge and its stricter limits are described in the repository README;
it is not included in this binary bundle.

Persistent raster jobs are available through the CLI on Linux:
  mkdir -m 700 raster-tasks
  perveance-cli task-create project.json proposal.json raster-tasks
  perveance-cli task-run raster-tasks TASK_ID
  perveance-cli task-inspect raster-tasks TASK_ID
Use task.id returned by task-create in place of TASK_ID. The proposal must be
version-1 and bound to the project's inspected revision. These jobs only run
compiled typed raster edits; no model or generated code runs. task-cancel is
cooperative between stages, not a process kill. task-recover marks an abandoned
running job interrupted without replaying it. Successful jobs retain verified
editable project/PNG outputs in raster-tasks/TASK_ID. Full snapshots, including
hidden artwork and partial outputs, remain until you remove them. There is no
global retention/disk quota, OS resource enforcement, or GUI/provider wiring.
Use trusted private local paths and see the repository README for full limits.

OpenRouter integration is experimental and not live-validated. Paid requests
can incur charges, including when a request fails, and there is no spend cap.
Do not put credentials in chat, project files, or command-line arguments.

SHA256SUMS accompanies the archive, not this extracted directory. Verify it
before extraction using sha256sum -c SHA256SUMS in the downloaded artifact
directory. A checksum detects corruption, not the identity of the publisher;
this artifact is not signed. Download only from a GitHub Actions run you trust.
CI artifacts include a separate LICENSE-INVENTORY.json and checksum based on
Cargo metadata, outside this archive. It is an unverified review aid, not a
complete inventory of linked code, system libraries or assets, nor a license
or redistribution review. Local packaging does not generate the sidecar.
This artifact is provided for evaluation, not as a formal release.
