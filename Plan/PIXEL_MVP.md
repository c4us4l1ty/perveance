# Dependable pixel-art MVP milestone

This is a milestone toward a usable artist tool, not completion of the broader
AI-first autonomous vision. The Rust app, GUI, CLI and separate limited Python
Arena bridge remain prototypes; arbitrary-code autonomy stays disabled.

## Exit criteria

1. **Obtain and run:** provide an evaluation bundle that can be downloaded,
   checked, extracted and launched without building Rust. Record source and
   build environment; verify the downloaded bundle's CLI and GUI in CI. Before
   calling it a release, validate on a real supported desktop, document exact
   runtime dependencies, review third-party licenses, and address signing,
   provenance, update and compatibility policies.
2. **Artist workflow:** on the supported desktop, create/edit a layered project,
   save, close, reopen, inspect/edit again, undo/redo and export a pixel-correct
   PNG. Test common controls through native input rather than simulated UI only;
   improve paths, errors, accessibility and discoverability based on actual use.
3. **Protect work:** define a stable format/migration policy, provide meaningful
   backups or recovery, and test failed writes, interruptions and conflicting
   edits. The current numbered-copy saves never overwrite files and perform a
   best-effort source-content check, but have no atomic cross-process CAS, file
   lock or durability guarantee after power loss. Opt-in recovery checkpoints provide limited, user-managed snapshots but not guaranteed crash recovery.
4. **Agent handoff:** verify an Arena agent can create art with the bridge, a
   person can transfer/open/edit it in the Rust app, and the agent can inspect
   the resulting saved project. Document unsupported operations, limits and
   trust assumptions. The bridge is not a live GUI connector.
5. **Product checks:** keep CI green with shared-model, PNG, agent-bridge and
   native X11 tests; add targeted fault, parser and interaction coverage. Record
   known limitations, supported environments and steps to report issues.

## Current increment

CI is configured to produce an unsigned x86-64 Linux evaluation bundle with
GUI, CLI, examples, build metadata and a checksum, then download and exercise
the artifact. Actual status depends on the corresponding CI run. It is not a
formal release, does not prove compatibility outside the runner, and does not
complete the other criteria above. Autonomous execution is still disabled.

## Next increment: agent/editor handoff

The Python bridge now supports the current Rust typed raster layer and canvas
operations under its stricter bounds, and CI is configured to exercise a
bridge-created project through a GUI edit and back to a revision-bound bridge
edit and PNG comparison. Check the relevant CI run for the actual result. This
is an automated X11/file-transfer test, not direct Arena integration or
validation on a user's desktop. Native installation, real model/provider
validation, usability and security/reliability work remain open.
