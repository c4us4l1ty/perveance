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

Use Browse beside a file or directory field to select a path. Use path only
fills the field; opening, saving, importing and exporting need separate actions.
This is an in-app browser, not a native OS dialog or security boundary. Large
directories and non-UTF-8 or special-file names may require manual path entry.

Use Save or Ctrl+S to create the first file (using the GUI's File path field),
or a numbered version after edits. Existing files are never overwritten.
Ctrl+S is ignored in text fields, dialogs and active drawing gestures.

Optional recovery checkpoints are off by default. To enable them, select an
existing trusted directory in the GUI's Recovery checkpoints section, or start:
  /path/to/perveance-linux-x86_64/bin/perveance --recovery-dir /path/to/directory
They include hidden layers and remain on disk until manually removed. They are
best-effort snapshots, not a replacement for saving or backups; see the main
repository README for limits and recovery instructions.

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

OpenRouter integration is experimental and not live-validated. Paid requests
can incur charges, including when a request fails, and there is no spend cap.
Do not put credentials in chat, project files, or command-line arguments.

SHA256SUMS accompanies the archive, not this extracted directory. Verify it
before extraction using sha256sum -c SHA256SUMS in the downloaded artifact
directory. A checksum detects corruption, not the identity of the publisher;
this artifact is not signed. Download only from a GitHub Actions run you trust.
The complete third-party license inventory and redistribution review remain
outstanding; this artifact is provided for evaluation, not as a formal release.
