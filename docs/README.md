# Starfall I — Documentation

Six active documents. Everything else is either a task-focused guide or a
preserved historical record.

| Document | Read it when you want… |
|---|---|
| **[PROJECT_PLAN.md](PROJECT_PLAN.md)** | …the **two-edition plan**: what ships to players (Game) versus what ships to creators (Designer), the boundary between them, and the roadmap to walking away with each. |
| **[FEATURES.md](FEATURES.md)** | …to know **what the game does**. The master feature reference: every system, from movement and combat to settlements, raids, creation tools, and audio. |
| **[DEVELOPMENT.md](DEVELOPMENT.md)** | …to **work on the code**. Build and run, repository layout, architectural rules, the simulation frame, data-driven combat, verification, conventions. |
| **[engine_roadmap.md](engine_roadmap.md)** | …to see **where the engine is going**. The EC# track: phases, acceptance criteria, and what has landed. |
| **[naming.md](naming.md)** | …the canonical cast, place, and terminology list for player-facing text. |
| **[guides/](guides/README.md)** | …a focused how-to: fixed-tick motor, combat feel, spatial LOD, Character Studio pipeline, verification procedure. |

The repository-level `README.md` is the player/developer overview; `agent.md` is
the compact operating contract and points here rather than maintaining a
competing list. Asset-folder READMEs under `assets/` are user-facing drop-in
instructions.

## Archive

[archive/](archive/README.md) preserves dated reviews, audits, triage passes,
milestone logs (`M#`, `MM#`, `ET#`, `PX#`), and superseded plans. They are
evidence of how the project got here, not current instruction — **prefer the
five documents above** when they disagree.

## Keeping docs honest

- A feature is not done until FEATURES.md describes it.
- If a doc and the code disagree, the code wins — fix the doc in the same change
  that caused the drift.
- Point-in-time documents (reviews, audits, dated plans) belong in `archive/`
  from the day they are written; they should never accumulate in the root.
