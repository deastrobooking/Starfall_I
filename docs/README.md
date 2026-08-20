# Starfall I — Documentation

This repository has a single active source of truth per concern. The top-level docs are intentionally narrow and specific so the project can stay understandable without reading every old milestone log.

| Document | Purpose |
|---|---|
| **[MASTER_FEATURES.md](MASTER_FEATURES.md)** | The master feature catalog: what the game does, what is stable, and what is still in progress. This is the main reference for game scope, systems, and feature maturity. |
| **[PROJECT_PLAN.md](PROJECT_PLAN.md)** | The current product plan and next-step priorities for the next milestone cycle. |
| **[DEVELOPMENT.md](DEVELOPMENT.md)** | Build, run, architecture, conventions, verification, and implementation rules. |
| **[engine_roadmap.md](engine_roadmap.md)** | The engine-core roadmap, covering fixed-tick simulation, combat substrate, and technical engine milestones. |
| **[editor_roadmap.md](editor_roadmap.md)** | The Forge/editor roadmap and dependency-separation strategy. |
| **[naming.md](naming.md)** | Canonical names for game-facing cast, places, and terminology. |
| **[HEAVY_WATER_PORT.md](HEAVY_WATER_PORT.md)** | The evidence-backed continuity ledger for Heavy Water and what remains staged. |
| **[guides/README.md](guides/README.md)** | Focused how-to guides for major systems and workflows. |

The repository-level [README.md](../README.md) remains the high-level project overview. The docs under [archive/](archive/README.md) preserve historical reviews, audits, and milestone logs; they are evidence, not active instruction.

## Documentation rule

- If a feature is major enough to matter, it should be described in [MASTER_FEATURES.md](MASTER_FEATURES.md).
- If a roadmap item is next in priority, it should live in [PROJECT_PLAN.md](PROJECT_PLAN.md).
- If a doc and the code disagree, the code wins; update the doc in the same work that caused the drift.
- Old dated plans stay in [archive/](archive/README.md), not in the active root docs.
