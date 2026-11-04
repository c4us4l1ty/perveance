# Security and trust model

## Security goal and limits

Enable autonomous creative capability development while minimizing risk to the user's files, secrets, artwork, budget and machine. A language model is not a security boundary. Tests and signatures do not prove arbitrary generated code safe. No claims of complete isolation, zero data leakage, secure deletion, protection against a compromised OS, or guaranteed prevention of model/provider abuse.

**Threats:** malicious candidate code or dependency/build script; prompt injection in artwork, file names, SVGs, web content or tool outputs; unauthorized project access; model-suggested exfiltration; overspend/denial of service; forged manifests/test results; poisoning of cached capabilities; corruption during upgrades or project migrations. A malicious or compromised local OS/user account, physical attack, model-provider compromise and GPU/OS sandbox escapes are outside the initial guarantee. Independent review is required before describing a boundary as strong.

## Authority separation

- The trusted supervisor owns policy, sandbox configuration, credential mediation, independent tests and activation. Candidate builders and applications have no write access to supervisor binaries/configuration, policy, trust roots, test fixtures or registry state. Logs/results from the sandbox are untrusted; supervisor determines exit and verification status itself.
- Workspace copy-on-write: task input is an explicit minimal snapshot, not the user's home directory; outputs are staged and validated before being committed. Read and write access are scoped by the supervisor to selected project resources. Do not follow attacker-controlled symlinks, archive paths or hard links outside workspace boundaries.
- Model calls use a separate broker with the smallest possible per-task scope; API keys are held in OS credential storage when supported and never provided as environment variables, command arguments, prompt text, logs or files visible to agents. Scrub errors. Host OS and authorized broker can still access credentials. If a third-party harness requires direct key access or unrestricted network, it cannot run autonomously under this contract until a verified safe integration exists.
- Separate builders, runtime extensions and the model broker into distinct roles. A running extension is untrusted even after it passes tests. For core changes, only the application components that can be confined and mediated are eligible. Supervisor and policy changes require the separate trusted update mechanism; therefore the product is self-extending, not unrestricted self-rewriting.
- Enforce budgets (CPU, memory, process count, wall time, file/output size, model calls, token/cost where provider metering permits). If a hard monetary cap cannot be guaranteed due to billing delay or lack of provider enforcement, say so and stop conservatively; do not claim a guaranteed cap.

## Isolation and networking

Initial supported security backend targets Linux. The feasibility spike must choose and document a concrete OS sandbox/VM technology, version and required kernel features; a subprocess, Rust memory safety, a container with host mounts, or a prompt is not sufficient. Verify namespace/filesystem/process isolation, syscall exposure, no privileged escalation, network egress and kill/cleanup semantics against the threat suite. Use the strongest practical tested boundary, ideally a VM for native builds, considering local prerequisites and portability. WASM extensions must also be resource-limited and mediated. Untrusted compilation includes cargo build scripts, procedural macros, tests and linked tools, not just the final program.

Default worker egress is denied. A separately controlled provider broker permits only configured model endpoints and scoped data; inputs may still be disclosed to that provider, including anything placed in a model request. Dependency acquisition uses a separate policy-controlled fetcher and pinned/allowlisted, integrity-checked packages; builds run offline against staged dependencies. Package pinning and audits reduce risk, but malicious packages and vulnerabilities remain possible. If sandbox prerequisites or policy cannot be enforced, fail closed with an actionable explanation—never silently run on the host.

Windows/macOS sandbox implementations need their own threat analysis and adversarial tests. Do not claim Linux guarantees transfer automatically. Preview, headless, CI and packaged-app environments each require verification. Running Perveance inside another sandbox, such as Arena, does not prove nested isolation is available.

## Activation, provenance and recovery

- Validate manifest, ABI/API, integrity, dependency policy and required platform; run supervisor-owned baseline and candidate-specific tests. Schema migrations have pre/post fixtures and backup/restore checks.
- Use staged artifact hashes and atomic registry/version transitions. Keep prior working version, independent rollback metadata and project backups. Crash/restart tests cover each transition. Rollback restores compatible versions; it does not magically reverse arbitrary document migrations—restore backups when needed.
- No candidate is allowed to bless itself by editing the registry, test suite, sandbox policy or version checks. The trusted supervisor signs activation records only after checks; signature indicates provenance/integrity, not a security audit.
- Separate trusted software update channel for supervisor upgrades. Never automatically merge or push upstream repository changes as part of creating an asset.

## Holonomy audit: lessons, not inherited guarantees

Reviewed [Holonomy README](https://github.com/c4us4l1ty/holonomy/blob/382b901c268a13e4a350ad878d69f519f340e34f/README.md), [PRD audit](https://github.com/c4us4l1ty/holonomy/blob/382b901c268a13e4a350ad878d69f519f340e34f/PRD_AUDIT.md), workspace manifest and [audit workflow](https://github.com/c4us4l1ty/holonomy/blob/382b901c268a13e4a350ad878d69f519f340e34f/.github/workflows/audit.yml); selected security-related source files were sampled, not independently verified or executed.

Useful practices: explicitly state what a protection does and does not cover; make budgets measurable; test fail-closed transitions; distinguish OS-specific capabilities and honest fallbacks; isolate long-running or blocking operations from critical watchdog decisions; use established cryptographic libraries and review provenance. Holonomy's audit explicitly rejects several earlier unrealistic security claims—this is a reason to test assumptions, not to repeat them.

Do **not** transplant its specialized USB-pad/container, duress, bespoke cryptographic protocol, timing guarantees, or its security claims into Perveance by default. These solve different problems and add complexity. Optional encrypted projects, if later requested, need a separate threat model and independently reviewed standard construction. Do not claim memory wiping, screenshot exclusion, or disk erasure works universally. Do not copy the workflow's automatic format-and-push behavior into untrusted autonomous feature builds; use least-privilege CI and protected review/update flows instead. No Holonomy code is selected for reuse yet.

## Security verification gates

Adversarial tests must cover: attempts to read a planted host secret or credential, write outside project scratch (including traversal/symlinks), reach an unapproved endpoint, spawn excessive processes, exhaust memory/storage/time, tamper with trusted tests/registry/supervisor, leak secrets via logs, access a previous task's data, and persist after cancellation. Inject instructions into imported assets and fake tool logs and confirm they cannot alter enforced policy. Exercise crashes at activation/migration boundaries and recovery from bad capability versions. Run each supported OS/backend independently. A failed or unavailable required test blocks autonomous execution on that backend.

## Experimental project-only staging (not a boundary)

The prototype can stage one already-validated raster project into a private Linux temporary directory under a caller-supplied trusted parent, verify the regular file and round-trip revision, and remove it. It does not traverse or copy an arbitrary workspace, socket, symlink or attachment, and it does not start an untrusted worker. The input project itself—including hidden pixels and names—is copied. This path-based implementation assumes a trusted parent and no malicious same-user process, and cleanup after a crash is not guaranteed. Source project loading follows symlinks. It is not the race-resistant staging, sandbox, authorization, quota or isolation required by the supervisor gate; do not use it as proof that arbitrary worker inputs are safe.

## Experimental task intake (not a gate implementation)

The data-only intake inspection checks a versioned request against budget ceilings provided by the trusted caller and a project snapshot fingerprint. It unconditionally returns blocked for all valid requests: user-provided JSON cannot enable execution or assert that a gate passed. Ceilings are declarations, not resource limits; the standalone OpenRouter prototype is not subject to them. There is no job, supervisor process, enforced isolation, resource meter, broker or verifier. Normal path loading assumes trusted regular files and follows symlinks; this interface is not safe input brokerage.

An experimental in-memory model-call reservation counter rejects unknown/zero cost bounds, over-budget calls and integer overflow, and does not refund reservations. It is not wired to the provider, survives no restart, depends on a trustworthy bound supplied by trusted code, and cannot guarantee provider charges. It is not an authorization or a credential broker. A future broker must own synchronization, durable accounting, provider-specific billing controls and all other security gates; otherwise calls remain disabled in the autonomous pipeline.
