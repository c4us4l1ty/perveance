# Phase 0 — preliminary findings

**Date:** 2026-11-04. This is a feasibility record, not a completed security review. The owner requested using GitHub CI as the Rust compiler because the Arena sandbox has no Rust toolchain.

## Environment observations

- `rustc` and `cargo` are not installed in the Arena environment, and requests to `static.rust-lang.org`, `crates.io` and `index.crates.io` failed there. GitHub Actions is configured to compile/test Rust; CI outcomes must be read from the actual run, not inferred from the workflow file. CI does not prove that the GUI works on a user's desktop or that runtime sandboxing is safe.
- `unshare -Urn` worked during the probe, and the process saw the temporary host sentinel. The isolated network namespace showed only loopback. Namespaces on their own do not isolate the filesystem or establish a sandbox. `bwrap`, `podman`, and `docker` were not found in this environment. `tools/phase0_probe.py` repeats a limited, non-destructive version of these checks; it is not an adversarial suite or certification.
- The current prototype cannot run agent-authored code. Autonomous execution remains unimplemented and disabled; do not treat CI runners as a user-facing sandbox.

## Graphite investigation

At the reviewed [Graphite revision](https://github.com/GraphiteEditor/Graphite/tree/708cca339de55b6bbb3f6257769c45dffa492c3e), its workspace requires Rust 1.98, and `graphene-cli` exposes document compilation and export commands, including PNG and SVG. The desktop UI crate lists a Chromium Embedded Framework dependency, while the editor depends on numerous workspace-internal graph/document crates. This shows an actual headless export path and substantial integration surface, not a verdict on suitability or an indication of a stable external editing API. Graphite has not been built or interactively evaluated in this environment.

A small independent Rust GUI/headless pixel prototype is being used to test product interaction and shared document operations, **not** to declare that Graphite has been rejected or the Phase 0 foundation gate has passed. Compare with a real upstream build and editor/CLI exercise before choosing the final engine. Do not copy Graphite code without the license inventory.

## Model/harness investigation

The reviewed OpenCode source includes an OpenRouter provider implementation. That is evidence of upstream provider support, not evidence that it works through a Perveance credential broker, honors its network restrictions, or meets its security model. No provider credentials, live model call or agent harness were tested.

## Remaining gate

Build and inspect Graphite on CI or a suitable development environment; evaluate document editing and pixel workflows. Select and adversarially test a sandbox backend and broker-compatible harness; test controlled dependencies, resource limits and failure recovery. These tasks remain open. The initial prototype does not enable or claim them.

## First GitHub Actions result

[Rust CI run 41799173401](https://github.com/c4us4l1ty/perveance/actions/runs/41799173401) completed successfully on the initial prototype commit: the Rust all-target tests, Python probe tests and CLI round-trip steps passed. This does not visually verify the GUI, certify a sandbox, or test the subsequent changes. The workflow uses read-only permissions. Full build logs may not be retrievable from environments that cannot access GitHub's Actions log host; check results in the GitHub UI when necessary.

A separate branch-triggered workflow checks the reviewed Graphite revision's CLI; its actual result must be checked. A successful `cargo check` would establish compilation on that runner, not suitability, visual behavior, a verified license inventory, or a safe plugin architecture.

The first Graphite CI check ([run 41799441325](https://github.com/c4us4l1ty/perveance/actions/runs/41799441325)) failed with exit code 101. Its detailed log was not accessible from this sandbox. A rerun is configured to surface the final diagnostics as check annotations; the first failure is not evidence that Graphite is unsuitable.

The diagnostic rerun ([41799783606](https://github.com/c4us4l1ty/perveance/actions/runs/41799783606)) found that `cargo check --locked -p graphene-cli --no-default-features` fails at this revision with an unresolved `wgpu_executor` reference in `export.rs` on Rust 1.99.1. This is a concrete limitation of that configuration, not proof that the default Graphite build fails. The next check uses default features; even if that passes, GPU-independent rendering remains unverified.

The default-feature `cargo check --locked -p graphene-cli` succeeded in [run 41800196385](https://github.com/c4us4l1ty/perveance/actions/runs/41800196385) on the pinned upstream revision. This checks compilation, not a runnable export, graphical usability, headless GPU availability, or integration. The feature-disabled failure remains relevant. The foundation choice is still open.

Perveance now has a committed Cargo.lock, and CI tests with `--locked`. A full license inventory remains outstanding before release. CI launches the desktop window under X11, but full desktop interaction and visual testing remain outstanding.

## Bubblewrap fixed-test feasibility probe

The fixed-code, non-destructive `tools/isolation_probe.py` reports narrow observations and always leaves autonomous execution disabled. It exercises a synthetic host sentinel, workspace, symlink, `/proc` root paths, host IPv4 loopback, and a synthetic environment variable. It is not a production sandbox profile or a security certification. Automated tests also run the fixed worker without isolation against disposable fixtures and verify it detects the missing boundary.

In [CI run 41816238642](https://github.com/c4us4l1ty/perveance/actions/runs/41816238642), bubblewrap was installed but the fixed test did not start: `bwrap: loopback: Failed RTM_NEWADDR: Operation not permitted`. The CI workflow passed because it verifies that this failure is reported and execution remains disabled, **not** because isolation passed. In the Arena development sandbox bubblewrap was not found. These observations do not establish whether another host/configuration can run bubblewrap safely; do not elevate privileges or silently omit network isolation to bypass this failure. No candidate code was executed.
