# Codex Working Notes

Use this as a lightweight handoff scratchpad for future passes.

## Current Ownership Model

- `PlayerIndex` is the owner key for player runtime state.
- `save_plugin.rs` writes authoritative per-player state into `SaveData.players[]`.
- `ui_plugin.rs` spawns one player HUD panel per active local player.
- `companion_plugin.rs` stores `Companion.owner` and resolves follow/heal/combat around that player.
- `crafting_plugin.rs` queues craft completions back to the owning player inventory.
- `chest_plugin.rs` rewards the nearest opener.
- `vehicle_plugin.rs` applies active vehicle buffs only to the activating player.
- `ChapterProgress`, `PerkTree`, and `WaveInfo` remain shared campaign/session resources.

## Still Worth Fixing Next

- `enemy_plugin.rs` / loot drops: replace remaining `get_single()` inventory pickup paths.
- `armor_plugin.rs`: remove single-player debug cycling or route it through an owner.
- Feedback: camera shake and damage vignette are still global.
- Vehicle design: decide whether each player can have an independent active vehicle mode, or whether party-shared mode switching is intentional.

## Guardrails

- Keep Bevy/Rapier pinned to the current `Cargo.toml` stack unless doing a deliberate engine migration.
- Do not collapse editable `CharacterBlueprint` data into baked meshes.
- Prefer adding owner/index fields to runtime components and events over guessing from query order.
