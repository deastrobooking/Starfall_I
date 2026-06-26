# Starfall I — UI Architecture Reference

> This document describes the production architecture of `src/plugins/ui_plugin.rs`
> and related UI systems. Keep it up to date as milestones add or change panels.

---

## 1. UI Lifecycle — AppState Anchoring

UI setup and teardown are driven entirely by Bevy's `OnEnter` / `OnExit`
transition hooks, ensuring no stray node hierarchies survive state changes.

| State | Setup system | Teardown system |
|---|---|---|
| `AppState::MainMenu` | `setup_main_menu` | `despawn_menu` (on enter PlayerSelect) |
| `AppState::PlayerSelect` | `setup_player_select` | `despawn_player_select` |
| `AppState::ChapterSelect` | `setup_chapter_select` | `despawn_chapter_select` |
| `AppState::Playing` | `setup_hud`, `setup_controller_diag`, `setup_command_overlay` | `cleanup_play_ui_for_menu` (on enter MainMenu) |
| `AppState::Paused` | `setup_pause_menu` | `despawn_pause_menu` |
| `AppState::GameOver` | `setup_game_over` | — |

A persistent `MenuCamera` (order −100) is spawned once on `Startup` and
never despawned, providing a fallback render surface for menu states.

---

## 2. HUD Architecture — Per-Player Split-Screen

`setup_hud` reads `LocalPlayerConfig.active` (1–4) and spawns one HUD
slot per active player. Bars are marked with `PlayerHudBar { player_index, kind }`
and text nodes with `PlayerHudText { player_index, kind }`.

**Bar kinds**: `Health`, `Armor`, `Stamina`, `Jetpack`

**Text kinds**: `Header`, `Credits`, `Level`, `Element`, `WeaponName`, `Ammo`, `SpecialAmmo`

`hud_update_system` runs every frame in `Playing | GameOver`. It queries
`Health`, `PlayerStats`, `JetpackState`, `WeaponInventory`, `SpecialWeaponInventory`,
`ArmorSet`, and `BeamSabre` per player, then writes `node.width` and `Text`
values directly.

### M14+ optimization target

`hud_update_system` currently polls every frame. Switching to Bevy change
detection (`Changed<Health>`, `Changed<PlayerStats>`, etc.) would eliminate
layout recalculations on frames where nothing changed. The wave/enemy-count
texts can short-circuit on `wave.is_changed()`.

---

## 3. Chapter Select — Preparation Panels

`setup_chapter_select` builds the full prep screen when the player enters
`ChapterSelect`. The screen is a Column root (`ChapterSelectRoot`) containing:

1. **Fast-travel map** — `spawn_fast_travel_map`: absolute-positioned chapter
   buttons (`ChapterFastTravelButton`), temple markers, settlement markers,
   and world-site badges (✓ / !). Colored map region bands behind them.
2. **Chapter list** — scrollable column of chapter rows; unlocked chapters
   shown brighter.
3. **Perk Training panel** — blue border; rows marked `PerkRowText(perk_id)`,
   header `PerkPointsHeader`. Keys A–H spend points.
4. **Tech Upgrades panel** — green border; rows marked `UpgradeRowText(id)`,
   header `UpgradeReserveHeader`. Keys Z–N purchase upgrades.
5. **Weapon Rank panel** — purple border; rows marked `WeaponRankRowText(slot)`,
   header `WeaponRankHeader`. Keys Y–P / J upgrade weapon slots 0–5.
6. **Settlement Economy panel** *(added M12)* — teal border; header
   `EconomyPanelHeader`, one row per settlement `EconomyPanelSiteRow(idx)`.
   Shows stockpile summary, site state badge (`[FREE]`/`[HELD]`/`[RAID]`),
   build count, and next recommended build. Key `M` toggles focus (hint only).

Each panel has a matching update system that returns early if its backing
resource has not changed:

| Panel | Update system | Change guard |
|---|---|---|
| Perk | `chapter_select_perk_panel_update` | `perks.is_changed()` |
| Upgrades | `chapter_select_upgrade_panel_update` | `upgrades.is_changed() \|\| robot_pets.is_changed()` |
| Weapon ranks | `chapter_select_weapon_rank_panel_update` | `weapon_ranks.is_changed() \|\| robot_pets.is_changed()` |
| Economy | `chapter_select_economy_panel_update` | `economy.is_changed() \|\| world_site_registry.is_changed()` |

Hint bar format:
```
1-9 0 Q W R T / click = travel  |  A-H Perks  |  Z-N Tech  |  Y-P/J Weapons  |  M Economy  |  E Character Editor  |  G Garage  |  Esc Back
```

### Colorblind-friendly shape layer (recommended next step)

The map currently relies entirely on color to convey node state (green =
complete, blue = unlocked, grey = locked). Adding shape outlines — circle for
chapters, triangle for temples, diamond for sites — would improve accessibility
without changing the color logic.

---

## 4. Pause Menu — Physics Freeze And Input Guard

`setup_pause_menu` spawns `PauseRoot` with two pages: `PausePage::Main` and
`PausePage::Controls`.

`freeze_physics_on_pause` pauses Avian's `Time<Physics>` clock on enter,
halting physics advancement. `resume_physics_after_pause` unpauses it on exit.

`PauseMenuState` tracks:
- `page: PausePage` — current visible page
- `resume_lockout: f32` — countdown timer (0.3 s) blocking instant resume
- `resume_armed: bool` — false while Start/Escape is held, preventing double-toggle

`pause_menu_action_system` handles button clicks and keyboard/controller input,
calling `save_current_session` inline when the Save action fires.

### Layout cleanup (recommended next step)

Panel colors, font sizes, and padding are currently hardcoded inline. Extracting
them to a `UiTheme` resource (or a module-level constant block) would make
visual updates a single-file change instead of a grep across setup functions.

---

## 5. Live-Play Overlays

### Player Guidance (`GuidancePanelRoot`)
`PlayerGuidance` resource drives a contextual bottom-of-screen prompt showing
nearby interactions (NPCs, terminals, chests, drones). `player_guidance_system`
polls it every frame. New world interactions should write to `PlayerGuidance`
rather than spawning one-off message texts.

### Discussion Panel (`DiscussionPanelRoot`)
`DiscussionState` feeds active dialogue lines, speaker names, and body text to
`discussion_panel_system`. Hides automatically when `DiscussionState` is idle.

### Controller Diagnostics (`ControllerDiagRoot`)
`F8` toggles `ControllerDiagState.visible`. The overlay shows per-player analog
deflection, raw gilrs stick values, gamepad index, and native controller name.
Runs in `Playing | Paused`.

### Command Overlay (`F7`)
`CommandOverlayState` drives a bottom-left overlay showing all command assets
(scout/worker drones, fighters, mechs, ships) by site assignment. Runs in
`Playing | Paused`. Added in M9.

---

## 6. Recommended Optimization Directions

These are tracked as implementation targets, not architectural debt:

1. **HUD change detection** — transition `hud_update_system` to gate on
   `Changed<Health>` / `Changed<PlayerStats>` etc. to skip bar recalculations
   on static frames. Medium impact on CPU budget in dense 4-player sessions.

2. **Colorblind shapes on map** — supplement chapter-select map nodes with shape
   outlines (circle for chapters, triangle for temples, diamond for sites)
   alongside the existing colored icons.

3. **UiTheme constant block** — extract panel background colors, border colors,
   and font sizes from inline spawn code to a shared `const` or `Resource` so
   visual theme changes are a single edit.

---

## Change Log

| Milestone | Change |
|---|---|
| M5 | World-site badges added to fast-travel map |
| M9 | Command overlay (F7) added |
| M11 | Controller diagnostics overlay (F8) added |
| M12 | Settlement Economy panel added to chapter select |
| M14 | Hero class differentiation wired; HUD change-detection optimization applied |
