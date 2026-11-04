# Research, reuse and licensing

**Checked:** 2026-11-04 via upstream GitHub repository metadata, README and selected manifests. This is a preliminary engineering review, not legal advice or a full dependency, trademark, asset or security audit. Upstreams evolve; recheck at the actual pinned revision before incorporating or distributing anything. A repository license does not necessarily cover every file or bundled asset.

## Art/editor candidates

| Project / reviewed reference | What was checked | Use and cautions |
| --- | --- | --- |
| [Graphite](https://github.com/GraphiteEditor/Graphite/tree/708cca339de55b6bbb3f6257769c45dffa492c3e) | [README](https://github.com/GraphiteEditor/Graphite/blob/708cca339de55b6bbb3f6257769c45dffa492c3e/README.md): source MIT **or** Apache-2.0, unless otherwise noted; it describes a vector/raster engine and alpha editor. Repository has Rust engine/editor, a desktop directory and Svelte frontend. | Best first foundation candidate to investigate. Could reuse components, integrate, fork or use as architectural reference. Whether stable APIs, editable pixel workflows, headless control and the desired extension boundary are practical is unverified. Its existing vector capability must not be represented as absent. |
| [Vello](https://github.com/linebender/vello/tree/5f61a03541a11e6405272f1f37ff596974245f22) | Workspace manifest: Apache-2.0 **or** MIT. | Candidate GPU 2D renderer, not a complete art editor. Validate CPU/headless fallback and platform fit. |
| [Kurbo](https://github.com/linebender/kurbo/tree/a5f97ff4070773fe01be92c6a607120e49da4e6f), [Peniko](https://github.com/linebender/peniko/tree/a8ad89c73ff985a58d70e2581d34557b66ba6cf1) | Workspace manifests: Apache-2.0 **or** MIT. | Curve/geometry and styling building blocks; check actual API/version compatibility. |
| [resvg](https://github.com/linebender/resvg/tree/cd64e5155706cada17316fc2ed413e673d5de04b) | Workspace manifest: Apache-2.0 **or** MIT. | SVG rendering/import candidate, not by itself an editable-vector object model. Audit supported SVG subset and external-resource handling. |
| [egui](https://github.com/emilk/egui/tree/fc9d66dda742d722eedaa8684d23f86369cf8e5f) | Workspace manifest: MIT **or** Apache-2.0. | Candidate Rust GUI; evaluate art canvas input, accessibility, platform integration and performance first. |
| [LibreSprite](https://github.com/LibreSprite/LibreSprite/tree/98f74431f5f523c747d278e3486625ccc7fc52cc) | [README](https://github.com/LibreSprite/LibreSprite/blob/98f74431f5f523c747d278e3486625ccc7fc52cc/README.md) says GPL version 2. | Strong pixel-art workflow reference. Copying/linking or distributing derivative code may invoke GPL obligations; do not incorporate pending project-license decision and review. |
| [Aseprite](https://github.com/aseprite/aseprite/tree/3669f4774d181d77deb3e6db95fe6df8ca347410) | [README](https://github.com/aseprite/aseprite/blob/3669f4774d181d77deb3e6db95fe6df8ca347410/README.md) says main source/official releases are under its EULA; some components have other licenses. | Do not treat the main application as freely forkable open-source code. Review individual components and terms before use. |

MIT/Apache generally permit modification, redistribution and commercial use subject to conditions, including retaining applicable copyright/license notices; Apache also has conditions around notices and changes and contains a patent license/termination terms. Dual-license means choose one offered license for the relevant material, not that obligations disappear. Review transitive dependencies, fonts, icons, artwork, model assets, trademarks and package-specific overrides.

## Agent candidates

[OpenCode](https://github.com/anomalyco/opencode/tree/ffe6fed58334d921f64a86a8d86128bff5a4b566), [Pi](https://github.com/earendil-works/pi/tree/2205a824bba414b9814ec3d2283952d581b19b60), [Hermes](https://github.com/NousResearch/hermes-agent/tree/71736c9f32597bbca9d89cede5aa136e55946b2f), and [Oh My Pi](https://github.com/can1357/oh-my-pi/tree/33ea58aa7dfd696c1ac419ba51fb915660740ab2) show MIT repository licenses at the reviewed revisions. That alone does **not** establish compatibility with our broker, sandbox, APIs, packaging, provider terms or dependency licensing. Claude Code and Codex require their own current distribution, provider and integration terms assessment; no source/license claim is made here. DeepSeek and OpenRouter also need provider/model/API and billing verification; distinguish provider from harness.

## Holonomy

[Holonomy](https://github.com/c4us4l1ty/holonomy/tree/382b901c268a13e4a350ad878d69f519f340e34f) workspace manifest declares MIT OR Apache-2.0. The repository includes older archived material, so evaluate files individually before any reuse. SECURITY.md records observed lessons and explicitly avoids adopting its specialized security construction or assuming its claims are verified. No code is currently copied.

## Foundation spike — decision gate before a big commitment

Perform small, time-boxed experiments after START; do not count them as the final autonomous-feature proof:

1. Build/launch the chosen Graphite revision on the Linux target; measure prerequisites and inspect build/license requirements.
2. Test whether existing document operations, layer objects, rendering, save/open and headless use are accessible without forking most of the product. Verify actual APIs, not assumed ones.
3. Evaluate pixel precision, layer interoperability, extensible editable payloads, CLI integration, Svelte/desktop integration and security isolation around candidate features.
4. Compare with composing a small Rust document engine + vetted libraries, factoring time, maintenance, portability, license, extensibility and ability to honor agent and GUI requirements.
5. Record a decision with evidence: **integrate**, **reuse selected modules**, **fork**, or **compose**. Choose the smallest approach that passes gates; revisit if the spike fails. Record actual license notices and transitive dependencies before distribution.

Avoid cloning or committing an upstream project wholesale until the decision and license review are complete. No code has been copied at planning time.

## Experimental provider transport

The prototype OpenRouter adapter uses [`ureq` 2.12.1](https://github.com/algesten/ureq/tree/2.12.1), whose manifest declares MIT OR Apache-2.0. TLS is enabled, while compression, cookies and environment proxy detection are disabled. This is a preliminary package check, not a complete transitive license or security audit. No live provider or harness compatibility test has been performed.
