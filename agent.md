# Starfall I Agent Guide

This file is the durable handoff for future Codex/agent work on Starfall I.
It is based on a repository review on 2026-05-18 and should be updated whenever
the architecture, production priorities, or team guidelines change.

## Project North Star

Starfall I is a family-friendly, cartoon action platformer RPG built in Rust
with Bevy 0.15 and Rapier 3D. The fantasy is 1-4 local players controlling a
family of star-powered heroes through open 3D chapters, secret caves, dragon
domains, airship rematches, RPG loot, crafting, companions, and expressive
star-beam combat.

The AAA target is not just more features. It means:

- Movement and camera feel polished under keyboard, mouse, and controller.
- Combat reads clearly, gives strong feedback, and supports cooperative play.
- Levels feel authored, paced, replayable, and visually memorable.
- Save data, progression, UI, and multiplayer ownership are reliable.
- Performance, accessibility, audio, art direction, tooling, and QA are treated
  as core systems instead of late polish.

## Current Software Review

### Verified Baseline

- `cargo check` passes in the current workspace.
- The codebase is organized as a Bevy plugin-per-feature prototype.
- Main runtime stack: Rust 2021, Bevy 0.15, `bevy_rapier3d` 0.28, `serde`,
  `serde_json`, `rand`, and `noise`.
- Core app states are `MainMenu`, `PlayerSelect`, `CharacterDesign`,
  `ChapterSelect`, `ChassisEditor`, `Playing`, `Paused`, and `GameOver`.
- The shipped docs in `README.md`, `docs/architecture.md`, `docs/systems.md`,
  and `docs/improvements.md` are broadly aligned with the code.

### Strengths

- Broad prototype surface: platforming, combat, RPG stats, perks, chests,
  crafting, companions, vehicles, saves, chapter progression, caves, airships,
  relic puzzles, character authoring, and local split-screen all exist.
- Good Bevy shape: systems are split into feature plugins under `src/plugins/`.
- `PlayerInput` abstracts keyboard/gamepad input before movement and combat.
- `PlayerIndex` is already the intended owner key for local multiplayer state.
- `CharacterBlueprint` preserves editable character recipes rather than baking
  customization into one-off meshes.
- Default heroes now spawn through the same upgraded blueprint path as custom
  characters, including body-linked movement, collider proportions, animation
  stride/agility, and visual foot grounding.
- Chapter scripting is data-oriented through `EncounterStep` definitions.
- Hidden reward rooms and companion rescue rewards now have a save-backed
  reward path for credits, XP, armor, power-up ids, and special ability upgrades.
- Save data preserves newer per-player records while keeping old-save fields.
- The project already has useful design documentation and improvement notes.

### Prototype Risks Blocking AAA Quality

- Multiplayer ownership is not complete. `enemy_plugin.rs` still has a loot
  pickup path that uses `get_single()` for player transform and inventory,
  `armor_plugin.rs` debug element cycling assumes one player, chapter encounter
  anchoring uses the first active player, and camera shake is global.
- Presentation is still prototype-heavy. Much of the world, characters, VFX,
  and encounters are procedural primitives or simple spawned shapes rather than
  production assets, animation, audio, authored lighting, cinematics, and tuned
  set pieces.
- UX is functional but not final. Perk training is keyboard-shortcut based,
  character/chassis editing is simple, inventory/crafting/map/menu flows need
  controller-first polish, and accessibility settings are not yet a system.
- Combat and enemy behavior need depth. The current state-machine enemies prove
  the loop, but AAA combat needs readable telegraphs, hit reactions, encounter
  roles, difficulty tuning, camera feedback, audio layering, and boss scripting.
- Save/progression scope still mixes old and new concepts. `WaveInfo` remains
  for compatibility, max health has several writers, and campaign-shared data
  should be separated clearly from per-player character data.
- Testing and production tooling are thin. `cargo check` is the current health
  gate; there are no visible gameplay regression tests, CI checks, profiling
  budgets, asset validation, or save compatibility tests.

## Agent Working Guidelines

Follow these rules when changing this repo.

1. Keep the current engine stack pinned unless the task is explicitly an engine
   migration: Bevy 0.15 and `bevy_rapier3d` 0.28.
2. Run `cargo check` after Rust changes. Prefer adding focused tests or smoke
   checks when touching save data, progression, damage, input, or ownership.
3. Preserve save compatibility. Add defaulted fields with `serde(default)` and
   keep legacy readers until a deliberate save migration is designed.
4. Treat `PlayerIndex` as the ownership key for local multiplayer. Do not guess
   ownership from query order when an event/component/resource can carry it.
5. Keep `CharacterBlueprint` as editable recipe data. Do not collapse it into
   baked-only meshes or throw away unused recipe fields just because the current
   renderer does not consume them yet.
6. Keep campaign-shared systems explicit. `ChapterProgress`, `PerkTree`, and
   chapter unlocks may remain shared, but inventory, rewards, HUD, companions,
   feedback, interaction prompts, and save snapshots should be per player unless
   a design note says otherwise.
7. Prefer data-driven chapter, item, enemy, and encounter definitions over
   hard-coded one-off branches.
8. Keep feature work scoped to the relevant plugin/module. Avoid broad rewrites
   unless they remove a known production blocker.
9. Preserve the family-friendly star-beam tone. Starfall uses cartoon energy
   tools and magical sci-fi beams, not realistic firearms.
10. Update this file and the docs when changing architecture, controls, content
    flow, save scope, or production priorities.

## Production Quality Bars

Use these bars when deciding whether a feature is prototype, vertical-slice
ready, or AAA-ready.

### Movement And Camera

- Platforming supports coyote time, jump buffering, wall interaction, ledges,
  dodge, parry, jetpack, moving platforms, and camera framing without surprising
  the player.
- Each traversal action has animation, VFX/audio cues, controller rumble, and
  readable failure states.
- Split-screen cameras do not fight over global effects and remain playable in
  boss arenas, caves, interiors, vertical climbs, and airship decks.

### Combat

- Every enemy role has a readable purpose, range, telegraph, hit reaction,
  counterplay window, and reward profile.
- Star beams, special tools, melee, parry, dodge, armor, perks, companions, and
  bosses are balanced together instead of tuned in isolation.
- Damage numbers and UI feedback are useful but not required for understanding
  what happened.

### World And Levels

- Procedural generation provides breadth, but each chapter needs authored
  landmarks, traversal routes, encounter beats, secrets, pacing, and visual
  identity.
- Caves, relic-fragment courses, castles, and airships should have biome-specific
  mechanics, hazards, rewards, and shortcuts.
- Chapter completion should feel like a finished episode, not a chain of spawned
  test encounters.

### UX, Accessibility, And Onboarding

- All primary flows work with keyboard/mouse and controller.
- Player select, character design, chapter select, perks, crafting, inventory,
  map, pause, settings, save/load, and game over have clear focus states and
  predictable back/confirm behavior.
- Include remapping, captions/subtitles, colorblind-safe combat cues, camera
  shake intensity, damage number toggles, text scale, and assist options.

### Technical Quality

- No new global single-player assumptions in multiplayer systems.
- Save/load round trips should be tested for current and legacy schemas.
- Performance budgets should be visible: frame time, entity counts, draw calls,
  physics cost, asset memory, save size, and load time.
- Crashes should produce useful logs without corrupting save data.

## AAA Roadmap

### Phase 0: Stabilize The Prototype

Goal: make the current game safe to iterate on every day.

- Fix remaining per-player ownership gaps:
  - Convert world loot pickup to choose the nearest eligible player and add the
    item to that player's inventory.
  - Move armor element cycling behind a debug/dev mode or route it through a
    specific `PlayerIndex`.
  - Split camera shake and damage feedback per player camera.
  - Decide whether vehicles are party-shared or per-player, then encode that
    decision in components/resources.
- Add a basic automated quality gate:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features`
  - `cargo check`
  - save/load compatibility smoke test
- Keep `agent.md` as the canonical handoff file and update it whenever current
  gameplay systems or production priorities change.
- Document campaign-shared versus per-player data in `docs/architecture.md`.

### Phase 1: Build A True Vertical Slice

Goal: one polished, replayable chapter that defines the final game feel.

Recommended slice: Chapter 1, Starfall Lab.

- Lock final movement numbers for running, jumping, wall jumps, ledges, dodge,
  parry, jetpack, and moving-platform behavior.
- Replace placeholder combat presentation with final-style star-beam VFX, hit
  sparks, impact pause, camera/rumble feedback, audio cues, and enemy reactions.
- Turn the chapter into a 10-15 minute authored mission with a readable opening,
  traversal tutorial, secret cave, hidden reward rooms, relic puzzle, enemy
  escalation, mini climax, rewards, and chapter-complete beat.
- Build final-style UI for HUD, objective tracker, pause, chapter rewards, and
  perk training.
- Establish art direction with production character proportions, material
  palettes, lighting, sky, props, and environment landmarks.

### Phase 2: Production Systems

Goal: replace prototype workflows with scalable tools.

- Create content authoring formats for chapters, encounters, loot tables, NPC
  dialogue, enemy stats, boss phases, cave layouts, and relic puzzles.
- Introduce asset pipeline rules for models, materials, animations, audio, VFX,
  naming, scale, collision, LOD, and import validation.
- Add debug overlays for player ownership, collision, enemy state, active chapter
  step, save data, performance, and interaction targets.
- Build deterministic replay or scripted smoke scenes for movement, combat,
  progression, save/load, and local multiplayer.
- Start tracking performance budgets per chapter and per split-screen count.

### Phase 3: Core Game Depth

Goal: make the whole game design worthy of the feature list.

- Expand enemy families into roles: scouts, brutes, shielders, snipers, casters,
  swarmers, aerial threats, support units, elite variants, and bosses.
- Give each dragon/domain boss unique mechanics, phases, arena hazards, music,
  cinematics, and rematch changes.
- Deepen RPG progression with clear build choices across beams, armor, perks,
  companions, crafting, and Star Sabre upgrades.
- Make caves, airships, relic-fragment courses, and castles biome-specific
  rather than reskinned templates.
- Add meaningful cooperative design: revive rules, combo assists, shared puzzles,
  player-specific rewards, and camera-safe encounter layouts.

### Phase 4: Presentation Pass

Goal: move from functional prototype to memorable game.

- Replace primitive-only character/world presentation with production assets,
  rigs, animation sets, facial/pose language, readable silhouettes, and optimized
  materials.
- Add music direction, adaptive combat layers, ambience, UI sound, traversal
  sounds, enemy vocalizations, boss stingers, and mix settings.
- Build cinematic chapter intros/outros, boss entrances, airship escapes, relic
  reveals, and family character moments.
- Add post-processing, lighting passes, weather/time-of-day moments, VFX quality
  tiers, and split-screen-safe composition.

### Phase 5: Ship Readiness

Goal: make the game reliable, accessible, and maintainable.

- Add full settings: controls, remap, audio, display, graphics, accessibility,
  language/captions, save slots, and difficulty.
- Build QA matrices for 1-4 players, keyboard/mouse, multiple gamepads, all
  chapters, old saves, deaths, pausing, disconnects, and long sessions.
- Add crash-safe save writes, save backups, explicit versioning, and migration
  tests.
- Profile and optimize CPU, GPU, physics, assets, memory, startup, save/load,
  split-screen, and large encounters.
- Prepare release packaging, credits, telemetry/privacy decisions, localization
  hooks, platform compliance, and build reproducibility.

## Next 30 Days

Prioritize these before adding broad new content:

1. Finish per-player ownership in loot, armor debug, feedback, and interaction
   prompts.
2. Build the Chapter 1 vertical-slice plan into actionable tasks and acceptance
   criteria.
3. Add final-style perk training UI with controller navigation.
4. Create smoke tests for save/load and chapter progression.
5. Establish art/audio/content pipeline docs so new assets land consistently.
6. Add a profiling/debug overlay for entity counts, frame time, current chapter
   step, active players, and ownership.

## Definition Of Done For Future Agent Changes

Before closing a task, an agent should be able to answer:

- What player-facing behavior changed?
- Which files/modules own that behavior?
- Is the behavior campaign-shared or per-player?
- Did save compatibility change?
- Did controls, docs, UI, or accessibility need updates?
- What was verified locally?
- What risks remain?

At minimum, Rust gameplay changes should end with `cargo check` passing unless
the task is explicitly exploratory or blocked.
