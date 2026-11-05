# Perveance — planning index

**State:** Implementation authorized and in progress (see [STATUS.md](STATUS.md)). These are evolving product and engineering plans, not a claim that every feature exists. [COMPLETION.md](COMPLETION.md) separates implemented work from the remaining acceptance gates.

Perveance is an AI-first creative studio for pixel artists, game developers, illustrators, and graphic designers. Its differentiator is an autonomous, bounded ability to build and retain missing creative capabilities, then complete the original artwork. Humans and external agents should both be able to use it.

## Reading order

1. [PRD.md](PRD.md) — user experience, requirements, release boundaries and acceptance.
2. [PROJECT.md](PROJECT.md) — technical choices, system map, storage and non-goals.
3. [AUTONOMY.md](AUTONOMY.md) — capability discovery, build, verification, activation and reuse.
4. [SECURITY.md](SECURITY.md) — threats, trust boundaries, isolation and Holonomy audit.
5. [AGENT_INTERFACE.md](AGENT_INTERFACE.md) — GUI-independent operations and Arena workflow.
6. [GUI.md](GUI.md) — visual direction from the supplied reference and editor UX.
7. [RESEARCH_AND_LICENSES.md](RESEARCH_AND_LICENSES.md) — audited references, licenses and foundation decision gate.
8. [ROADMAP.md](ROADMAP.md) — implementation phases, tests, gates and risks.
9. [STATUS.md](STATUS.md) — current facts and the approval/decision log.
10. [PHASE0_FINDINGS.md](PHASE0_FINDINGS.md) — feasibility observations and unresolved gates.
11. [COMPLETION.md](COMPLETION.md) — remaining work, critical path and completion criteria.
12. [LINUX_WORKER.md](LINUX_WORKER.md) — fixed-code cgroup-v2 resource/watchdog backend and its open security gates.
13. [WORKER_IO.md](WORKER_IO.md) — descriptor-scoped raster inputs, sealed worker snapshots and verified atomic bundle publication.

These documents describe a proposed design, not a guarantee that every implementation detail is achievable. If they conflict, the security boundaries in SECURITY.md and the acceptance gates in ROADMAP.md take precedence. Explicit owner instructions supersede proposed defaults, except where a constraint cannot safely be met without stating the limitation.
