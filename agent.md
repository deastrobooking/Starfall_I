# Starfall I Agent Guide

This file is the concise handoff for future Codex/agent work on Starfall I.
For the production next-steps guide, use `docs/agent_next_steps.md`. For the
detailed engine upgrade and milestone process, use
`docs/engine_upgrade_milestones.md`.

## Project North Star

Starfall I is a family-friendly cartoon action platformer RPG built in Rust with
Bevy `0.18.1` and Rapier 3D through `bevy_rapier3d` `0.34`. The fantasy is 1-4
local players controlling a family of star-powered heroes through open 3D
chapters, secret caves, dragon domains, airship rematches, RPG loot, crafting,
companions, vehicles, and expressive star-beam combat.

The AAA target is not just more features. It means movement, camera, combat,
local multiplayer, save data, progression, UI, performance, accessibility,
audio, art direction, tooling, and QA all move toward production quality
together.

## Current Baseline

- Current pushed baseline: Bevy `0.18.1`, `bevy_rapier3d` `0.34`, Rust 2021.
- Gameplay communication uses Bevy messages: `Message`, `MessageReader`,
  `MessageWriter`, and `App::add_message`.
- Local Bevy render bundle compatibility helpers live in `src/rendering.rs`.
- Robot pet foundation lives in `src/robot_pets.rs`: shared saved pet records,
  enemy salvage parts, store-build recipes, and combined vehicle/mech/ship form
  gates. Chapter scripts already use robot rescue pods; the garage UI and
  runtime mech/ship controllers are future work.
- Tech upgrade foundation lives in `src/upgrades.rs`: shared saved beam,
  missile, turret, health, rejuvenation, and mech-link ranks. Chapter select
  spends robot salvage on ranks; chapter tech caches grant parts/route hints;
  rejuvenation healing consumes saved reserve.
- Airship boss rematches now spawn turret-guarded decks with moving cover.
  Keep airship additions temporary and chapter-owned so cleanup stays simple.
- Boss mode lives in `src/plugins/player_plugin.rs`: 2-4 local players switch
  from split-screen to one full-screen party camera during `BossEnemy` fights
  or nearby flying-drone wings, with distant players pulled toward the battle
  anchor and split-screen restored afterward.
- Dragon lair dungeons live in `src/plugins/world_plugin.rs` for Chapters
  6-11. They are physical room/corridor layouts with hoard beacons,
  interactable `DungeonCrawlGate`s, sliding `DungeonGateDoor`s, and
  `dragon_dungeon_chXX` anchors for future boss/key/switch staging. Dungeon
  gates activate `DungeonCrawlState`, which switches players to one top-down
  screen and makes melee/Star Sabre use wider dungeon arcs.
- Great Scientist temple subquests also live in `src/plugins/world_plugin.rs`.
  They reuse dungeon gates/top-down mode and `HiddenReward` caches to grant
  persistent mechanics upgrades: flight core, solar sabre/laser, nova missile,
  and Aegis armor traversal. `player_plugin::apply_scientist_temple_progress`
  reapplies these unlocks from `ChapterProgress` whenever players spawn.
- Human hero combat identity lives in `src/hero_roster.rs`: all eight siblings
  have unique signature weapons/specials plus shared speed, strength, flight,
  and magic axes. Rescued robot pets amplify those axes by role.
- Terrain uses deterministic waves plus `assets/terrain/everest.png` mapped
  across the full 200 x 200 mile Everest Range (`20_000` world units at
  `100` units per mile). `chapter_map_locations()` in `src/chapters/mod.rs`
  is the campaign fast-travel source of truth, and `map_settlements()` owns
  optional cities/villages/harbors/outposts for side-quest staging.
  `WorldPlugin` spawns matching heightmap beacons/anchors, settlements, range
  biomes, glacier streams, snowfields, outposts, and lair silhouettes, and
  chapter/player startup uses chapter anchors for fast travel. Keep heightmap
  changes tied to the shared height function so visuals, anchors, and
  collision agree.
- Settlement conversations are data-driven in `src/discussion.rs`.
  `DiscussionNpc` entities open the HUD discussion panel from `E` / D-pad
  Down, and each line can spawn an MP3 voice file from `assets/voice/...`.
  Cloudrail City and Switchwork Borough are peaceful mega-city hubs with
  Free Peoples guardian ship patrols. They also spawn non-combat
  `CitySpyDrone`s that players can shoot down; each drops a save-backed
  `SpyData` beacon reward.
- Player input is controller-first local multiplayer: preserve analog movement
  magnitude, keep circular deadzone remapping, support LT/RT button and axis
  paths, and treat controller smoke testing as required for movement changes.
- Runtime player guidance uses the `PlayerGuidance` resource and HUD panel in
  `src/plugins/ui_plugin.rs`. Feed new interactables into that panel when they
  need immediate player-facing prompts.
- Core app states are `MainMenu`, `PlayerSelect`, `CharacterDesign`,
  `ChapterSelect`, `ChassisEditor`, `Playing`, `Paused`, and `GameOver`.
- Active docs are `README.md`, `docs/architecture.md`, `docs/systems.md`,
  `docs/improvements.md`, `docs/naming.md`, and
  `docs/agent_next_steps.md`, plus `docs/engine_upgrade_milestones.md`.
- Current gates for Rust/engine work are `cargo fmt --check`, `cargo check`,
  `cargo clippy --all-targets -- -D warnings`, and `cargo test`.

## Agent Rules

1. Keep the engine stack pinned unless the task is explicitly an engine
   migration. Future engine upgrades must follow
   `docs/engine_upgrade_milestones.md`.
2. Run the relevant local gates after Rust changes. At minimum, run
   `cargo check`; for engine, CI, ownership, save, input, or broad gameplay
   changes, run the full gate set.
3. Preserve save compatibility. Add defaulted fields with `serde(default)` and
   keep legacy readers until a deliberate save migration is designed.
4. Treat `PlayerIndex` as the ownership key for local multiplayer. Do not guess
   ownership from query order when an event/component/resource can carry it.
5. Keep campaign-shared systems explicit. `ChapterProgress`, chapter
   objectives, kill gates, boss phases, unlocks, `PerkTree`,
   `RobotPetCollection`, and `UpgradeLedger` are shared by default;
   inventories, rewards, HUD, feedback, companions, crafting, runtime stats,
   character blueprints, and save `players[]` records are per-player by
   default.
6. Keep the current vehicle policy unless explicitly changed: one party-shared
   active vehicle/driver mode with passengers keyed by `PlayerIndex`.
7. Keep `CharacterBlueprint` as editable recipe data. Do not collapse it into
   baked-only meshes or remove unused recipe fields just because the current
   renderer does not consume them yet.
8. For player mechanics, keep `PlayerMovement` as the explicit home for
   movement/physics tuning. Prefer small tested changes to acceleration,
   step/snap, analog strength, wall interaction, and controller mapping over
   hidden constants in systems.
9. Prefer data-driven chapter, item, enemy, and encounter definitions over
   hard-coded one-off branches.
10. Keep feature work scoped to the relevant plugin/module. Avoid broad rewrites
   unless they remove a known production blocker.
11. Preserve the family-friendly star-beam tone. Starfall uses cartoon energy
    tools and magical sci-fi beams, not realistic firearms.
12. Keep player-facing names aligned with `docs/naming.md`. The alien invaders
    are Scallarians, even though the stable internal faction enum is still
    `DimensionalAlien`.
13. Update docs when changing architecture, controls, content flow, save scope,
    engine policy, or production priorities.

## Milestone Priority

1. Stabilize the 200-mile Everest Range: marker readability, route signs,
   settlements, grounded anchors, and far-zone smoke tests.
2. Build controller-first upgrade and robot garage screens.
3. Finish multiplayer ownership UI and save reliability.
4. Deepen dragon lairs and Great Scientist temples with keys, locks, room
   objectives, miniboss staging, mechanics trials, and unique hazards.
5. Tune player movement, physics, hand combat, beam saber, and controller
   diagnostics.
6. Add app/plugin smoke tests, debug overlays, profiling notes, and repeatable
   manual macOS validation.
7. Then proceed through Chapter 1 vertical slice polish, full production
   systems, presentation, accessibility, and ship readiness.

## Definition Of Done

Before closing a task, an agent should be able to answer:

- What player-facing behavior changed?
- Which files/modules own that behavior?
- Is the behavior campaign-shared or per-player?
- Did save compatibility change?
- Did controls, docs, UI, CI, or accessibility need updates?
- What was verified locally?
- What risks remain?
