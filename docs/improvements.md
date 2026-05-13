# Starfall I — Improvement Notes

Issues and suggestions found during review, grouped by severity.

---

## Bugs

### 1. Save data doesn't persist chapter progress or perks
**File:** `src/plugins/save_plugin.rs`, `SaveData` struct

`SaveData` saves only `level`, `experience`, `credits`, `max_health`, `max_stamina`, `max_armor`, and `wave_number`. `ChapterProgress` (completed chapters, discoverables, companion recruits) and `PerkTree` are never written to disk. Restarting the game loses all chapter and perk state.

**Fix:** Add `ChapterProgress` and `PerkTree` serialization to `SaveData` and restore them in `load_save_on_enter`.

---

### 2. Leftover dev save file
**File:** `heavy_water_save.json` (project root)

A save file from an earlier game name sits in the repo root. The current constant (`SAVE_FILE = "starfall_i_save.json"`) is different, so this file is never read or written — it's just dead data.

**Fix:** Delete the file and add `*.json` (or specifically `heavy_water_save.json`) to `.gitignore`.

---

### 3. Several resources not initialized in `main.rs`
**File:** `src/main.rs`, `src/resources.rs`

The following resources are defined and used in plugins but never `init_resource`'d at app startup:
- `CurrentChapter` (chapter_plugin presumably adds it, verify)
- `BiomePalette`
- `PlayerChassis`
- `ChapterProgress`
- `RadioChatter`
- `UiMessage`
- `PerkTree` (defined in `perks.rs`)

If any system runs before the owning plugin inserts the resource, Bevy will panic.

**Fix:** Either `init_resource` each in `main.rs` or verify each plugin inserts it in `OnEnter(Playing)` before any `Update` system that reads it.

---

### 4. ~~Dodge direction ignores move input~~ ✓ Fixed
**File:** `src/plugins/player_plugin.rs` — fixed during multiplayer refactor

```rust
dodge.dodge_direction = -fwd;  // always dodges straight backward
```

The dodge direction is hardcoded to the backward facing vector regardless of left-stick / WASD input. Players expecting directional dodge (e.g. side-roll) won't get it.

**Fix:**
```rust
let input_dir = (fwd * gi.move_axis.y + right * gi.move_axis.x).normalize_or_zero();
dodge.dodge_direction = if input_dir.length_squared() > 0.01 { -input_dir } else { -fwd };
```

---

### 5. `manual_save_system` bypasses `InputPlugin`
**File:** `src/plugins/save_plugin.rs:153`

```rust
if !keyboard.just_pressed(KeyCode::F5) { ... }
```

Every other input reads from `PlayerInput` (the component written by `input_plugin.rs`), but the manual save reads `ButtonInput<KeyCode>` directly. Controller users can't trigger a manual save.

**Fix:** Add a `save` bool field to `PlayerInput` (mapped to F5 / Start+Select for P1) and read `pi.save` in `manual_save_system`.

---

## Code Quality

### 6. `EdgeGrabState::new()` is redundant
**File:** `src/components/player.rs:108`

```rust
pub fn new() -> Self { Self::default() }
```

This is identical to `Default::default()`. Callers in `player_plugin.rs` already use `EdgeGrabState::new()`. Either remove `new()` and update callers to `EdgeGrabState::default()`, or keep `new()` as a named constructor alias and remove the manual `Default` impl to let `#[derive(Default)]` handle it.

---

### 7. `all_chapters()` allocates on every call; `get_chapter()` does linear search
**File:** `src/chapters/mod.rs:268,528`

`get_chapter(id)` calls `all_chapters()` which builds a 14-element Vec each call, then iterates it to find one entry. With 14 chapters this is negligible, but it creates unnecessary allocations if called frequently (e.g., per-frame from chapter_plugin).

**Fix:** Cache the result in a `OnceLock<Vec<ChapterDef>>` or initialize once as a resource. At minimum, add a chapter lookup by `id.index()` since `ChapterId` is 1-based with a stable `index()` method.

---

### 8. Dual armor tracking
**File:** `src/components/player.rs:16-18`, `src/components/armor.rs`

`PlayerStats` has both `armor: f32` and `max_armor: f32`. There is also an `ArmorSet` component. `damage_player()` reads `armor_set.calculate_damage_reduction()` AND deducts from `stats.armor`. Two components track the same concept in different forms, which will diverge if one is updated without the other.

**Fix:** Remove `armor`/`max_armor` from `PlayerStats` and read all armor values exclusively from `ArmorSet`. Or rename `PlayerStats.armor` to `current_armor_points` and document that it is the numeric pool, not the ArmorSet tier.

---

### 9. `PlayerStats.max_health` and `Health.max` must be kept in sync manually
**File:** `src/plugins/player_plugin.rs:578-581`

Level-up updates both `stats.max_health` and `health.max`:
```rust
stats.max_health += 10.0;
health.max = stats.max_health;
```
This dual write must happen together every time max health changes. If one is missed, the values drift.

**Fix:** Make `Health.max` the single source of truth. Remove `PlayerStats.max_health` or make it a derived getter that reads `Health`.

---

### 10. `bevy_rapier3d` version comment is stale
**File:** `Cargo.toml:8`

```toml
# bevy_rapier3d compatibility: 0.27 targets bevy 0.15; update if needed
bevy_rapier3d = { version = "0.28", ... }
```

The comment says 0.27 but the version is 0.28.

**Fix:** Update the comment to match the pinned version, or remove it.

---

## Suggestions

### 11. `wall_normal_from_controller_output` can be simplified
**File:** `src/plugins/player_plugin.rs:414`

The inner match for tracking the best-strength collision is hard to follow:
```rust
match best {
    Some((_, current_strength)) if current_strength >= strength => {}
    _ => best = Some((normal, strength)),
}
```
More readable as:
```rust
if best.map_or(true, |(_, s)| s < strength) {
    best = Some((normal, strength));
}
```

---

### 12. `SpecialSlot` naming is unintuitive
**File:** `src/components/weapon.rs:177`

`SpecialSlot::Slot0` is the last slot (Sprite Turret, keyboard key `0`) but its name suggests "first slot." Consider `Slot7`, `Slot8`, `Slot9`, `SlotZero` or use descriptive names (`HomingStar`, `TriStarBurst`, `MoonBubble`, `SpriteTurret`) directly as the enum variant names.

---

### 13. `SaveData.wave_number` vs chapter system
**File:** `src/plugins/save_plugin.rs:22`

`wave_number` in `SaveData` maps to the legacy `WaveInfo`, which the code comments describe as superseded by the chapter director. Saving and restoring `wave_number` only partially restores game state — the current chapter, step index, and completion list are what actually matter now.

This ties back to **Bug #1** but also suggests `wave_number` can be removed from `SaveData` once chapter progress is properly persisted.

---

### 14. `PerkTree` is never applied to player stats
**File:** `src/perks.rs`

`PerkTree` has methods like `damage_mult()`, `hp_bonus()`, `dodge_cost_mult()`, etc. but there are no calls to these methods in any plugin (grep finds no usages outside perks.rs itself). The perk tree is fully defined but currently a no-op.

**Fix:** In `player_plugin.rs`, read `PerkTree` resource and apply bonuses at the appropriate points (level-up, dodge init, etc.).
