# Starfall I - Improvement Notes

Open issues and design follow-ups found during the May 2026 documentation/code review.

## Fixed In This Pass

- `SaveState` now defaults to a 30 second autosave interval instead of autosaving every frame.
- `PerkTree` is initialized, awarded on level-up, saved/loaded, spendable from chapter select, and wired into HP, HP regen, beam damage, ammo/charge caps, dodge cost, and parry timing.
- Controller special tools are documented and wired through Select + D-pad.
- Castle/domain bosses in chapters 6, 7, 8, 10, and 11 now escape to faction-colored airship decks before their rematch.
- Chapter director and HUD no longer fail outright when multiple players exist; they use the first active player as the shared chapter/HUD anchor.
- Kill rewards are shared across active players.
- The stale Rapier compatibility comment in `Cargo.toml` now matches the pinned dependency.

## High Priority

### 1. Finish per-player multiplayer ownership
**Files:** `src/plugins/*`

The player-select, input, movement, camera, combat, and death paths support multiple players, but several support systems are still first-player or shared-state oriented.

Known remaining areas:

- `crafting_plugin.rs` and the crafting panel use one player inventory.
- `chest_plugin.rs` grants rewards through a single player interaction path.
- `companion_plugin.rs` generally follows/heals/assists one player.
- `vehicle_plugin.rs` uses a shared vehicle state and P1-style control model.
- `save_plugin.rs` snapshots the first active player's stats.
- The primary HUD displays the first active player rather than per-player panels.

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

Save data now persists chapter progress and perks, but still mixes legacy `wave_number`, derived max stats, and first-player stats. That is serviceable for the current prototype, but the schema should be cleaned up before serious campaign progression.

**Design direction:** save shared campaign data separately from per-player character data. Treat max health/stamina as derived values when possible.

## Medium Priority

### 5. Remove legacy root save data
**File:** `heavy_water_save.json`

The repo still contains a tracked save file from the previous project identity. The active save path is `starfall_i_save.json`, so this file is dead data.

**Fix:** delete the file. The active runtime save, `starfall_i_save.json`, is now ignored.

### 6. Dual armor tracking can drift
**Files:** `src/components/player.rs`, `src/components/armor.rs`, `src/plugins/armor_plugin.rs`

`PlayerStats` tracks numeric `armor` / `max_armor`, while `ArmorSet` tracks equipped armor pieces and damage reduction. Incoming damage uses both concepts. This should be named and documented as two distinct mechanics, or consolidated.

**Design direction:** either rename `PlayerStats.armor` to `current_armor_points`, or move armor durability into `ArmorSet`.

### 7. Health maximum has multiple writers
**Files:** `src/plugins/player_plugin.rs`, `src/plugins/armor_plugin.rs`, `src/plugins/save_plugin.rs`

Level-up, armor bonuses, perk bonuses, and loading all write max health. The armor/perk sync now recalculates from stable sources, but the data model would be cleaner with one source of truth.

**Design direction:** make `Health.max` the canonical runtime value and derive it from level + equipment + perks.

### 8. Chapter lookup allocates
**File:** `src/chapters/mod.rs`

`get_chapter(id)` calls `all_chapters()`, which builds the 14-chapter Vec on each lookup.

**Fix:** cache the chapter list in a `OnceLock<Vec<ChapterDef>>`, initialize it as a resource, or index by `ChapterId::index()`.

## Lower Priority

### 9. Move away from deprecated Bevy bundle APIs
**Files:** many rendering systems

`cargo check` succeeds, but Bevy emits many deprecation warnings for `PbrBundle`, `Camera3dBundle`, `PointLightBundle`, and related APIs.

**Fix:** migrate gradually to direct `Mesh3d`, `MeshMaterial3d`, `Camera3d`, `PointLight`, and related component insertion.

### 10. Second Wind needs an out-of-combat timer
**Files:** `src/plugins/player_plugin.rs`, `src/perks.rs`

`Second Wind` currently regenerates HP while the player is alive and below max health. The design text says "out of combat," but there is no combat timer yet.

**Fix:** track recent damage/dealt-damage timestamps and enable regen only after a short quiet window.

### 11. Input conflicts need a final control pass
**Files:** `src/plugins/input_plugin.rs`, `src/plugins/armor_plugin.rs`

Bracket keys currently overlap weapon cycling and developer elemental armor cycling. This is fine for a prototype, but final controls should separate player-facing actions from debug actions.

**Fix:** move element cycling behind a debug flag, menu, or dedicated dev-only chord.
