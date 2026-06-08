# Starfall I Agent Guide

This file is the concise handoff for future Codex/agent work on Starfall I.
For the detailed engine upgrade and milestone process, use
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
  gates. The garage UI and runtime mech/ship controllers are future work.
- Tech upgrade foundation lives in `src/upgrades.rs`: shared saved beam,
  missile, turret, health, rejuvenation, and mech-link ranks. Chapter select
  spends robot salvage on ranks; rejuvenation healing consumes saved reserve.
- Core app states are `MainMenu`, `PlayerSelect`, `CharacterDesign`,
  `ChapterSelect`, `ChassisEditor`, `Playing`, `Paused`, and `GameOver`.
- Active docs are `README.md`, `docs/architecture.md`, `docs/systems.md`,
  `docs/improvements.md`, and `docs/engine_upgrade_milestones.md`.
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
8. Prefer data-driven chapter, item, enemy, and encounter definitions over
   hard-coded one-off branches.
9. Keep feature work scoped to the relevant plugin/module. Avoid broad rewrites
   unless they remove a known production blocker.
10. Preserve the family-friendly star-beam tone. Starfall uses cartoon energy
    tools and magical sci-fi beams, not realistic firearms.
11. Update docs when changing architecture, controls, content flow, save scope,
    engine policy, or production priorities.

## Milestone Priority

1. Finish multiplayer ownership and save foundation.
2. Keep local and CI quality gates green.
3. Pay down engine migration debt while using Bevy 0.18-native APIs for new
   work.
4. Build the Chapter 1 vertical slice after ownership/save gates are reliable.
5. Build the robot pet garage loop: rescue/store UI, pet assignment, and runtime
   adapters for vehicles, mechs, ships, and megaships.
6. Build the full tech upgrade screen: weapons, missiles, turrets, health,
   rejuvenation reserve, robot mechs, vehicles, ships, and megaships.
7. Then proceed through production systems, core game depth, presentation, and
   ship readiness.

## Definition Of Done

Before closing a task, an agent should be able to answer:

- What player-facing behavior changed?
- Which files/modules own that behavior?
- Is the behavior campaign-shared or per-player?
- Did save compatibility change?
- Did controls, docs, UI, CI, or accessibility need updates?
- What was verified locally?
- What risks remain?
