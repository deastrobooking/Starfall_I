# Starfall I — Documentation Map

Documentation has accumulated across several workstreams. This index says what
each document is FOR and whether it's **living** (kept current, edit it) or a **snapshot**
(point-in-time review/triage — read for history, don't update).

## Start here

| Doc | Status | What it is |
|---|---|---|
| [current_state.md](current_state.md) | living | **Canonical current handoff**: verified baseline, delivered foundations, next production order, authority map |
| [guides/](guides/README.md) | living | **Process how-tos**: verification gates, fixed-tick motor, combat feel, character-studio pipeline |
| [architecture.md](architecture.md) | living | Module map, state flow, ownership policy, key design choices |
| [systems.md](systems.md) | living | Gameplay systems reference (movement, combat, world, saves…) — the encyclopedia |

## Roadmaps (milestone prefixes never mix)

| Doc | Prefix | Scope |
|---|---|---|
| [engine_roadmap.md](engine_roadmap.md) | `EC#` | Engine core: fixed tick, frame-data combat, profiling, extraction |
| [engine_upgrade_milestones.md](engine_upgrade_milestones.md) | `M#` | Campaign/engine strategy milestones + upgrade procedure + naming table |
| [playerengine.md](playerengine.md) | `MM#` | Motion mechanics / traversal / skeletal-animation roadmap |
| [agent_next_steps.md](agent_next_steps.md) | — | Near-term priorities + work rules for agents |
| [player_creator_experience_plan.md](player_creator_experience_plan.md) | `PX#` / `PM#` | Aiming, weapon presentation, shop UX, editor launcher, projects, presets, and multi-level support |
| [controller_player_ownership.md](controller_player_ownership.md) | `PO#` | Start-to-join controller assignment and per-player persistence ownership |

## Subsystem references (living)

| Doc | Covers |
|---|---|
| [character_studio.md](character_studio.md) | Character Studio feature reference (pipeline how-to: [guides/character-studio-pipeline.md](guides/character-studio-pipeline.md)) |
| [audio_modding.md](audio_modding.md) | Player music and modular action-SFX workflow |
| [engine_tools_multistage_pass.md](engine_tools_multistage_pass.md) | Starfall Forge in-game editor architecture (ET# passes) |
| [EDITOR_DESIGNER.MD](EDITOR_DESIGNER.MD) / [game_maker_toolchain.md](game_maker_toolchain.md) | Forge/editor design vision + toolchain plan |
| [rendering_shader_pass.md](rendering_shader_pass.md) | Rendering R# shader-material suite (toon/grass/water/energy/shield/ice/lava) |
| [ui_architecture.md](ui_architecture.md) | UI lifecycle per AppState, HUD, overlays |
| [naming.md](naming.md) | Canonical names: cast, factions, chapters, enums |

Repository-level `README.md` is the player/developer overview. `agent.md` is
the compact operating contract and must point here rather than maintain a
competing priority list. Asset-folder READMEs under `assets/` are user-facing
drop-in instructions.

## Snapshots (historical — do not update)

| Doc | When | What it captured |
|---|---|---|
| [game_review_2026-06.md](game_review_2026-06.md) | Jun 2026 | Full-game triage: city physics, dead logic, gameplay top-10 (most items since fixed — see roadmap statuses) |
| [game_review_2026-07.md](game_review_2026-07.md) | Jul 2026 | Follow-up review |
| [parallel_review_triage_2026-07.md](parallel_review_triage_2026-07.md) | Jul 2026 | Parallel review triage |
| [codebase_alignment_2026-07.md](codebase_alignment_2026-07.md) | Jul 2026 | Code-review reconciliation: accepted fixes, stale findings, and staged follow-up |
| [software_audit_triage_2026-07-17.md](software_audit_triage_2026-07-17.md) | Jul 2026 | Follow-up architecture audit disposition: retained winners and rejected stale sequencing |
| [improvements.md](improvements.md) | May 2026 | Early backlog (superseded by roadmaps) |
