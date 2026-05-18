# Starfall I - Improvement Notes

Open issues and design follow-ups found during the May 2026 documentation/code review.

## Fixed In This Pass

- `SaveState` now defaults to a 30 second autosave interval instead of autosaving every frame.
- `PerkTree` is initialized, awarded on level-up, saved/loaded, spendable from chapter select, and wired into HP, HP regen, beam damage, ammo/charge caps, dodge cost, and parry timing.
- Controller special tools are documented and wired through Select + D-pad.
- Castle/domain bosses in chapters 6, 7, 8, 10, and 11 now escape to faction-colored airship decks before their rematch.
- Relic-fragment sub puzzles now support five collectible pieces that assemble into one scientist relic, save partial progress, and spawn moving obstacle courses around the fragments.
- City generation now mixes glass panels, glowing window grids, brushed-metal cladding, factory ribbon windows, and stone-brick/mortar facade variants.
- Each chapter now has a secret cave system with walkable tunnel geometry, ancient/new cave dressing, moving platforms, and a save-backed discovery beacon.
- Chapter director no longer fails outright when multiple players exist; it still uses the first active player as the shared encounter anchor.
- Save/load now stores `players[]` records keyed by `PlayerIndex`, while legacy top-level stat fields remain only for old-save compatibility.
- HUD setup now spawns separate stat/weapon panels per active player instead of mirroring P1.
- Companions now carry an owner index, spawn defaults per active player, follow/heal that owner, and assign recruited allies to the player who collected the beacon.
- Crafting queues now carry an owner index, chests reward the nearest player, and vehicle buffs apply only to the activating player.
- Kill rewards are shared across active players.
- The stale Rapier compatibility comment in `Cargo.toml` now matches the pinned dependency.
- The legacy `heavy_water_save.json` root save artifact was removed; the active runtime save remains `starfall_i_save.json` and is ignored.
- Chapter definitions are cached through a `OnceLock` catalog, so chapter lookup no longer rebuilds the 14-chapter vector every time the director or UI asks for data.
- `Esc` / controller Start now transitions between `Playing` and `Paused` with a lightweight overlay, and HUD setup is idempotent when resuming.
- Character customization now writes editable `CharacterBlueprint` data with body proportions, procedural part/material/socket/rig/animation/movement recipe sections, body-shape steppers in the designer, save/load support, and gameplay-linked movement/stat tuning.
- Pause now freezes Rapier physics, exposes save and save-and-title actions, shows control hints, and cleans up preserved play-session entities when returning to the title.
- Chapter 1 now has a north-coast ocean route, an island behind the mountain range, dock markers, a visible wake lane, and a boardable boat vehicle.
- The Chapter 1 starter/lab area now reserves a clearer ground zone so opaque or translucent world props are less likely to block early walking routes.

## High Priority

### 1. Finish per-player multiplayer ownership
**Files:** `src/plugins/*`

The player-select, input, movement, camera, combat, save, HUD, companion, crafting, chest, vehicle-buff, and death paths support multiple players, but a few support systems are still first-player or shared-state oriented.

Known remaining areas:

- Some enemy/world reward pickup paths still assume a single nearby player or shared reward target.
- `armor_plugin.rs` still has single-player debug element cycling.
- Vehicle mode is owner-keyed, but only one active vehicle mode exists at a time for the party.
- Camera shake and damage vignette are still global feedback pools.

**Design direction:** add explicit owner/player index fields to interaction systems, then decide which resources are campaign-shared and which are per-player.

### 2. Add a real perk training screen
**Files:** `src/plugins/ui_plugin.rs`, `src/perks.rs`

The current chapter-select perk UI is intentionally small: keyboard shortcuts spend points and the text panel updates ranks. This proves the system, but it is not the final UX.

**Design direction:** make a dedicated perk screen with branch tabs, rank pips, disabled states, previews of stat changes, and controller navigation.

### 3. Deepen airship levels
**Files:** `src/chapters/mod.rs`, `src/plugins/chapter_plugin.rs`

Airship levels currently spawn a solid deck, rail blockers, engine visuals, a guard wave, and a boss rematch. That supports the requested chapter loop, but the levels could become richer platforming spaces.

**Design direction:** add moving deck hazards, side-scrolling sky debris, engine weak points, airship-specific loot, and boarding/extraction transitions.

### 4. Clarify save-game scope
**File:** `src/plugins/save_plugin.rs`

Save data now persists chapter progress, perks, player-slot character blueprints, and per-player runtime stats. The schema still carries legacy top-level stat fields for old-save compatibility and still mixes shared `wave_number` with newer campaign/chapter data.

**Design direction:** split shared campaign data from per-player character data into separate schema sections or files. Treat max health/stamina as derived values when possible.

### 5. Deepen relic-fragment challenge design
**Files:** `src/chapters/mod.rs`, `src/plugins/chapter_plugin.rs`, `src/plugins/discoverable_plugin.rs`

Fragment puzzles now prove the five-pieces-make-a-relic loop with moving bars, lift platforms, saving, and auto-assembly. The next design step is making each fragment course feel more authored and biome-specific.

**Design direction:** add hazard damage, timed doors, rotating platform chains, airship-style variants, and optional bonus fragments that reward careful platforming without blocking chapter completion.

## Medium Priority

### 6. Add cave-specific rewards and secrets
**Files:** `src/plugins/world_plugin.rs`, `src/plugins/chapter_plugin.rs`, `src/plugins/discoverable_plugin.rs`

Secret caves now exist as discoverable places, but they currently reward charting/progression rather than unique cave loot.

**Design direction:** add cave-only chests, lore tablets, biome hazards, hidden relic-fragment shortcuts, and a cave completion count on the map/HUD.

### 7. Dual armor tracking can drift
**Files:** `src/components/player.rs`, `src/components/armor.rs`, `src/plugins/armor_plugin.rs`

`PlayerStats` tracks numeric `armor` / `max_armor`, while `ArmorSet` tracks equipped armor pieces and damage reduction. Incoming damage uses both concepts. This should be named and documented as two distinct mechanics, or consolidated.

**Design direction:** either rename `PlayerStats.armor` to `current_armor_points`, or move armor durability into `ArmorSet`.

### 8. Health maximum has multiple writers
**Files:** `src/plugins/player_plugin.rs`, `src/plugins/armor_plugin.rs`, `src/plugins/save_plugin.rs`

Level-up, armor bonuses, perk bonuses, and loading all write max health. The armor/perk sync now recalculates from stable sources, but the data model would be cleaner with one source of truth.

**Design direction:** make `Health.max` the canonical runtime value and derive it from level + equipment + perks.

## Lower Priority

### 8. Move away from deprecated Bevy bundle APIs
**Files:** many rendering systems

`cargo check` succeeds, but Bevy emits many deprecation warnings for `PbrBundle`, `Camera3dBundle`, `PointLightBundle`, and related APIs.

**Fix:** migrate gradually to direct `Mesh3d`, `MeshMaterial3d`, `Camera3d`, `PointLight`, and related component insertion.

### 9. Second Wind needs an out-of-combat timer
**Files:** `src/plugins/player_plugin.rs`, `src/perks.rs`

`Second Wind` currently regenerates HP while the player is alive and below max health. The design text says "out of combat," but there is no combat timer yet.

**Fix:** track recent damage/dealt-damage timestamps and enable regen only after a short quiet window.

### 10. Input conflicts need a final control pass
**Files:** `src/plugins/input_plugin.rs`, `src/plugins/armor_plugin.rs`

Bracket keys currently overlap weapon cycling and developer elemental armor cycling. This is fine for a prototype, but final controls should separate player-facing actions from debug actions.

**Fix:** move element cycling behind a debug flag, menu, or dedicated dev-only chord.
