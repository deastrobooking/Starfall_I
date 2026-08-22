# Starfall I Agent Guide

This file is the concise handoff for future Codex/agent work on Starfall I.
Read `docs/README.md` first for the documentation authority map. The current
game-facing baseline is `docs/MASTER_FEATURES.md`, the active production order
is `docs/PROJECT_PLAN.md`, and the Forge architecture plan is
`docs/editor_roadmap.md`. Files under `docs/archive/` are dated evidence only;
consult them for history, not current priorities.

## Project North Star

Starfall I is a family-friendly cartoon action platformer RPG built in Rust with
Bevy `0.19.1` and Avian `0.7`. The fantasy is 1-4 local players controlling a
family of star-powered heroes through open 3D
chapters, secret caves, dragon domains, airship rematches, RPG loot, crafting,
companions, vehicles, and expressive star-beam combat.

The AAA target is not just more features. It means movement, camera, combat,
local multiplayer, save data, progression, UI, performance, accessibility,
audio, art direction, tooling, and QA all move toward production quality
together.

## Current Baseline

- Current local engine baseline: Bevy `0.19.1`, Avian `0.7`, Rust 2021.
- Starfall Forge ET1–ET5d first-slice work lives in `src/engine_tools/mod.rs` and
  `src/engine_tools/persistence.rs`: independent safe
  tool mode, stable IDs, selection, transactional undo/redo, controller UI,
  viewport picking, fly/orbit cameras, transform handles, snapping, primitive
  drafts, guarded world adapters, versioned manifests, atomic draft save/load,
  rotating recovery, validation, publishing, atomic per-record source files,
  cross-file generation recovery, and a controller-focusable registry browser,
  search/rename/create/duplicate/delete/source-move flow, typed payload boundary,
  and live disk diagnostics are implemented. Material records seed the upgraded
  `toon_v1` shader contract and Forge renders a live preview. ET5d now provides
  snapped controller selection/movement of road spline points and topology
  nodes as undoable recipe transactions plus derived sandbox viewport markers.
  Typed Door/Stair/Tunnel/Portal edge authoring marks two stable node IDs and
  connects/removes exact edges with a live guide and last-valid-root retention.
  Direct red/green/blue viewport handles project pointer movement into snapped
  recipe-space XYZ and commit one snapshot transaction on release. Spline
  points now carry backward-compatible automatic/explicit Hermite derivatives;
  sampled curves and typed Merge/Split/Cross/Loop-Link connectors compile under
  the sandbox budget. Building/cave/city/biome nodes now own typed Doorway,
  Stair Landing, Encounter, Item, and Portal sockets with stable IDs, bounded
  local transforms, controller editing, viewport facing guides, and optional
  edge endpoint bindings that preserve legacy center-to-center recipes. Terrain
  projection persists a bounded world origin/clearance and projects selected or
  complete topology against the exact shipped terrain-collider surface through
  controller-driven undoable edits and green viewport targets. Structured
  validation highlighting and published-layer promotion remain; ET4 production
  workspace integration also remains.
- Forge M0 correctness hardening now centralizes controller focus in one stable
  action registry, validates and hashes the complete level set, detects stale
  project sessions under an OS-backed writer lock, preserves duplicate names/modifier stacks,
  drops invalid history entries, caches unchanged disk diagnostics, assigns
  unique new-project identities, and atomically selects immutable published
  generations through one `current.json` snapshot. This is a contract pass, not the final
  editor architecture: `docs/editor_roadmap.md` owns the M1–M5 extraction,
  document/command, asset/cook, Edit/Simulate/PIE, and extensibility sequence.
- Gameplay communication uses Bevy messages: `Message`, `MessageReader`,
  `MessageWriter`, and `App::add_message`.
- Local Bevy render bundle compatibility helpers live in `src/engine/rendering.rs`.
- Rendering R2 lives in `src/engine/rendering.rs` and `assets/shaders/`: Toon, Grass,
  Water, Energy, Shield, Ice/Snow, and Lava use Bevy 0.19 material contracts.
  Weapon VFX reuse five bounded Energy handles; each local-player slot owns one
  persistent damage/parry Shield handle. Forge compiles all material families
  and exposes typed family/preset/bounded-parameter authoring with live previews.
  Published materials enter `PublishedMaterialCatalog` by stable content ID;
  Forge scene primitives persist optional validated IDs rather than Bevy handles.
  ET3f also publishes deterministic Biome/Road/Building/Cave/City records into
  `PublishedProceduralRecipeCatalog`; generated world geometry requests named
  material slots and retains a reversible Standard-material fallback.
  ET5a exposes controller buttons for recipe type/create, published-material
  copy, named-slot selection/bind/clear, and bounded preview regeneration.
  Recipe snapshots share the editor undo stack, and panel focus automatically
  scrolls the active control into view.
  ET5b adds bounded typed generator parameters, seed/reset/validation controls,
  parameter-driven four-object previews, and road/building/cave budget gates.
  ET5c implements that boundary inside a protected editor sandbox: stable road
  spline points, typed room/zone nodes and portal edges, collision/navigation
  preflight, cached part assets, and atomic owned-root replacement. Shipped world
  promotion still requires explicit published-layer scope. ET5d selection,
  snapped topology edits, typed endpoint edge connect/remove, and direct
  viewport handles are delivered. Direct topology dragging may mutate the draft
  for live preview only;
  restore the before snapshot and commit one command on release. Never convert
  derived sandbox parts into independently authored state.
  F11 reports camera/projectile/VFX/material counts. World Kit regeneration UI,
  clustered-light evaluation, and four-camera GPU timing work remain in
  `docs/rendering_shader_pass.md`; do not copy legacy fixed-group WGSL examples
  or claim unmeasured millisecond budgets.
- Robot pet foundation lives in `src/world/robot_pets.rs`: shared saved pet records,
  enemy salvage parts, store-build recipes, and combined vehicle/mech/ship form
  gates. Chapter scripts already use robot rescue pods. The Robot Garage screen
  (`src/plugins/robot_garage_plugin.rs`, `AppState::RobotGarage`) lets players
  assemble forms from collected pets and parts. Assembled forms drive vehicle
  modes at runtime: Motorcycle/Car assembly enables Motorcycle mode from Enter Vehicle,
  the two-pet JetBike assembly spawns a transforming party-owned
  Motorcycle/Jet visual and cycles ground/flight/off from Enter Vehicle, Tank
  enables Tank mode, GiantMech enables Mech mode, SpaceJet enables Jet mode
  (J key), SpaceShip/MegaShip enables Ship mode. `MechCommandLink` upgrade
  rank gates advanced forms. Runtime mech/ship 3D controllers are still future
  work.
- Tech upgrade foundation lives in `src/combat/upgrades.rs`: shared saved beam,
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
- Shared-screen dungeons now carry a stable active `gate_id` and spawn a
  `DungeonExitPortal`. A second intentional interact returns every local player
  to safe exterior slots, clears movement/boat state, and releases the linked
  camera; do not reintroduce distance-only dungeon exit logic.
- Mountain cave floors use adaptive continuous profiles sampled from the final
  terrain trimesh. Cave insets run after road terracing, and tests enforce seam,
  grade, minimum-clearance, maximum-float, and chamber-connection contracts for
  all fourteen authored caves.
- Mountain caves generate stable three-node `DungeonRoomZone` graphs connected
  by `DungeonRoomPortal` edges. `DungeonRoomState` tracks active/visited/cleared
  identities, and party-centroid adjacency plus hysteresis drives the shared
  camera focus without room skipping or boundary bounce.
- Every mountain cave has a physical 13.5-meter `AncientCaveGate`; discovery
  fast travel is optional. Star Chamber spawners tag `DungeonEncounterEnemy`
  ownership, so their collision-backed seals close during combat and reopen
  with a visible reward only when that chamber's enemies are gone.
- Great Scientist temple subquests also live in `src/plugins/world_plugin.rs`.
  They reuse dungeon gates/top-down mode and `HiddenReward` caches to grant
  persistent mechanics upgrades: flight core, solar sabre/laser, nova missile,
  and Aegis armor traversal. `player_plugin::apply_scientist_temple_progress`
  reapplies these unlocks from `ChapterProgress` whenever players spawn.
- Human hero combat identity lives in `src/character/hero_roster.rs`: all eight siblings
  have unique signature weapons/specials plus shared speed, strength, flight,
  and magic axes. Rescued robot pets amplify those axes by role.
- Terrain uses deterministic waves plus `assets/terrain/everest.png` mapped
  across the full 200 x 200 mile Everest Range (`20_000` world units at
  `100` units per mile). `chapter_map_locations()` in `src/chapters/mod.rs`
  is the campaign fast-travel source of truth, and `map_settlements()` owns
  optional cities/villages/harbors/outposts for side-quest staging.
  `WorldPlugin` spawns matching heightmap beacons/anchors, settlements, range
  biomes, glacier streams, snowfields, outposts, mountain-route path slabs,
  cyan guide studs, and lair silhouettes, and chapter/player startup uses
  chapter anchors for fast travel. The terrain mesh uses height/slope/route
  vertex colors for grass, alpine rock, ice, snow, and trails. Keep heightmap
  changes tied to the shared height function and `mountain_routes()` so
  visuals, anchors, route markers, and collision agree.
- Settlement layout is terrain-aware in `world_plugin`: each settlement samples
  local relief and uses grounded, terraced, or sky-district floors. Mega cities
  float on large walkable platforms with giant ramps/bridges and guide lights;
  rough settlement edges can spawn mountain-inset gates/thresholds.
- Settlement builder/economy vertical slice lives in
  `src/world/settlement_economy.rs`, `src/plugins/world_plugin.rs`, and
  `src/plugins/save_plugin.rs`. `SettlementEconomy` is campaign-shared and
  save-backed. Settlement terminals unlock after cache recovery or matching M5
  site liberation, spend shared stockpile plus robot parts, spawn visible farm,
  factory, spaceport, power plant, research lab, defense outpost, and bridge hub
  props, and tick bounded passive outputs. Route unlocks are Engine M7,
  assigned units/strategy overlays are Engine M9, and confirmation UX is still
  future polish.
- Engine M8 raid counteroffensives live in `src/world/raids.rs`,
  `src/plugins/world_plugin.rs`, and `src/plugins/save_plugin.rs`.
  `RaidRegistry` is save-backed through `SaveData.raids`. The first playable
  slice is a recoverable Cloudrail City `DroneSwarm`: warning sets the site to
  `UnderAttack`, active phase spawns a visible UFO marker plus tagged drones,
  player victory returns the site to `Liberated`, static defenses can resolve it
  as `Shielded`, and failure marks the site `Damaged` without deleting progress.
- Engine M10 tech hacking lives in `src/world/hacking.rs`,
  `src/plugins/hacking_plugin.rs`, `src/plugins/enemy_plugin.rs`,
  `src/plugins/weapon_plugin.rs`, and `src/plugins/save_plugin.rs`.
  `HackingRegistry` is save-backed through
  `SaveData.hacking`. The first slice makes small Scallarian drones hackable:
  interact starts a short range-held hack, completion saves
  `blueprint_scallarian_drone_core`, adds a `ScoutDrone` command asset, converts
  the drone into a temporary friendly `HackedUnit`, removes raid threat markers,
  and lets the owner fire a basic pulse through the linked drone.
- Settlement conversations are data-driven in `src/world/discussion.rs`.
  `DiscussionNpc` entities open the HUD discussion panel from `E` / D-pad
  Down, and each line can spawn an MP3 voice file from `assets/voice/...`.
  Cloudrail City and Switchwork Borough are peaceful mega-city hubs with
  Free Peoples guardian ship patrols. They also spawn non-combat
  `CitySpyDrone`s that players can shoot down; each drops a save-backed
  `SpyData` beacon reward.
- Player input is controller-first local multiplayer: preserve analog movement
  magnitude, keep circular deadzone remapping, support LT/RT button and axis
  paths, and treat controller smoke testing as required for movement changes.
  `GamepadAssignments` is the controller ownership authority: the first
  connected controller automatically claims P1, Start in Player Select claims
  P2–P4, disconnect releases only the device binding, and runtime input/F8
  diagnostics must not return to query-order mapping. Save schema v4 stores
  per-player `PlayerProgression` alongside the
  already-individual stats, inventory, shop ownership, loadout, and traversal
  selection. Legacy top-level progression fields remain compatibility mirrors;
  do not restore them as the runtime ownership authority.
- Roadmap labels are intentionally namespaced: Engine M# refers to
  `docs/archive/engine_upgrade_milestones.md`; Motion MM# refers to
  `docs/archive/playerengine.md`; future enemy behavior planning should use Enemy AI
  AI# labels.
- Humanoid traversal planning lives in `docs/archive/playerengine.md`.
  `GrappleHookState` is the single star-tech grappling source of truth;
  targeting, route/mountain zip, swing cable motion, enemy/boss pull behavior,
  procedural cable visuals, and pose/IK integration are present. Bespoke
  utility profiles, authored effects, and broad course tuning remain. Keep
  future parkour/flight/combat traversal work
  routed through `PlayerMovement`, `EdgeGrabState`, `GrappleHookState`, and
  `PlayerStateMachine` before adding new standalone movement state.
- Star Sabre combo work lives in `src/plugins/weapon_plugin.rs` and
  `src/components/weapon.rs`. Beam Capacitors raise effective saber wave tier
  for combo waves without mutating `BeamSabre.level`; keep future saber upgrades
  covered by pure helper tests before adding heavier Bevy app tests.
- Runtime player guidance uses the `PlayerGuidance` resource and HUD panel in
  `src/plugins/ui_plugin.rs`. Feed new interactables into that panel when they
  need immediate player-facing prompts.
- Core app states are `MainMenu`, `ProjectHub`, `CreatureForge`, `PlayerSelect`,
  `CharacterDesign`, `CharacterStudio`, `ChapterSelect`, `RobotGarage`,
  `Playing`, `Paused`, `GameOver`, and `Victory`.
  `CharacterDesign` and `RobotGarage` are both entered from `ChapterSelect`
  via [E] and [G] and return to `ChapterSelect`.
- The documentation authority map is `docs/README.md`;
  `docs/MASTER_FEATURES.md` owns the current feature baseline and
  `docs/PROJECT_PLAN.md` owns production priorities. Archived PX#/PM# plans and
  `docs/archive/current_state.md` remain useful delivery evidence, but they are
  not active backlogs. PX1 authoritative per-player aiming and the PM1 Start
  Editor / Project Hub shell plus the PM1 multi-project registry, PX3 shop
  transaction flow, PM2 lock and preset records, and PM3 multi-level/playtest
  slices are delivered. Manual four-pad acceptance remains an open verification
  item in the active project plan's controller and validation workstreams.
  The evidence-based memory for the external 156-item
  review is `docs/parallel_review_triage_2026-07.md`; do not treat its raw idea
  inventory as verified defects or implicit product commitments. Its July 15
  follow-up reconciliation also supersedes the later six-item deficiency memo:
  companion/chest/aim/fixed-loop claims were stale. Per-player inventory and
  active loadout persistence plus the Star Loadout button GUI are now delivered;
  four-pad acceptance, broader vehicle bodies, and missile obstacle casts remain.
- Current gates for Rust/engine work are `cargo fmt --all -- --check`, locked
  all-target check, strict Clippy, and tests for both the default Designer and
  `--no-default-features` Game editions. See `docs/guides/verification.md` for
  the exact commands mirrored by CI.
- The July 2026 audit is `docs/archive/game_review_2026-07.md`. Shared menu navigation
  now includes spatial D-pad/stick focus, repeat, Confirm, Back, and disabled
  skipping/styles. Speed roads include checkpoints, recovery, and guided
  hoverboard loop traversal plus 18 per-player bidirectional grind rails,
  automatic four-player Sonic springs, jump-off rail transfers, and strongly
  banked sweeper curves. Every settlement ring now has an ordered three-lap
  four-gate race and two NPC rivals. `StuntRunState` banks per-player rail/air/
  spin combos on landing and drives board rotation, stunt FOV, UI score, and
  owner-mapped Bevy rumble. `Homing Star` now tracks/reacquires hostile targets
  with per-player SEEK/LOCK HUD feedback. `BeamSabre` has a persistent blade
  visual; authored animation/audio and hardware polish remain.
- Audio has three preserved layers: `MusicPlayerPlugin` scans player MP3s from
  `assets/user_music`/`STARFALL_MUSIC_DIR`; `ActionSfxRegistry` maps safe modular
  IDs from `assets/user_sfx/actions.json`/`STARFALL_SFX_DIR`; and `SfxLibrary`
  retains every synthesized arcade fallback. `F6` opens the music/import deck
  and `R` hot reloads both custom libraries. Never replace the fallback bus.

## Agent Rules

1. Keep the engine stack pinned unless the task is explicitly an engine
   migration. Future engine upgrades must follow
   `docs/archive/engine_upgrade_milestones.md`.
2. Run the relevant local gates after Rust changes. At minimum, run
   `cargo check`; for engine, CI, ownership, save, input, or broad gameplay
   changes, run the full gate set.
3. Preserve save compatibility. Add defaulted fields with `serde(default)` and
   keep legacy readers until a deliberate save migration is designed.
4. Treat `PlayerIndex` as the ownership key for local multiplayer. Do not guess
   ownership from query order when an event/component/resource can carry it.
5. Keep ownership explicit. Chapters, objectives, world state, settlements,
   raids, robot pets, commands, hacking, and final-war state are shared.
   `PlayerProgression` owns perks, upgrades, weapon ranks, and shop state per
   player; inventories, rewards, HUD, feedback, companions, crafting, runtime
   stats, character blueprints, and save `players[]` records are also
   per-player.
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
14. Minimize the use of `unwrap()` and `expect()` for system inputs, procedural generation outputs (e.g. generated physics colliders in world generation), and file loading. Handle edge cases with clean defaults, error logging, and explicit failure paths.

## Completed Feature Milestones (recent)

- **Swappable parts system** — `CharacterLoadout`, `PartSlotTag`, `CharacterVisualConfig`,
  `PlayerPartLoadout`; `swap_character_parts` system; robot spawn functions.
- **Rounded robot factory** — `src/robots/factory.rs` fully rewritten with
  Capsule3d/Sphere/Cylinder geometry.
- **Character Design editor** (`AppState::CharacterDesign`) — single playable
  character editor with GLB-inspired base models, live preview, part presets,
  color controls, and saved loadouts.
- **Perk Tree UI + Upgrade Shop UI** — per-branch/per-track colored rows with rank bars
  in chapter select; live update systems; A–H spend perks, Z–N purchase upgrades.
- **Robot Garage** (`AppState::RobotGarage`) — assembly form browser, auto pet selection,
  MechCommandLink gating.
- **HP bonus application** — `perks.hp_bonus()` + `upgrades.armor_health_bonus()` applied
  at player spawn.
- **Chassis persistence** — `CharacterPartStyle` is serde-able; `PlayerPartLoadout` saved
  and hydrated across all save paths.
- **Assembly-driven vehicle modes** — `VehicleState` uses `GroundMode`/`AirMode` enums;
  assembled forms unlock Motorcycle/Tank/Mech/Jet/Ship modes at runtime.

## Milestone Priority

1. Maintain the delivered reusable app-construction seam and headless
   construction/Startup/steady-frame plugin smoke tests.
2. Maintain the save-compatible `PlayerBaseStats`/derived-cap contract; new
   permanent health or armor-cap rewards must update bases, not effective caches.
3. Execute Forge M1 from `docs/editor_roadmap.md`: establish the library and
   neutral schema/runtime seams, then split editor ownership without behavior
   changes.
4. Complete recorded four-controller hardware acceptance for ownership,
   controller-only menus, aiming, shops, editor launch, and save/reload.
5. Measure large-world simulation and one-to-four-camera costs before expanding
   density or streaming.
6. Tune the playable MVP: camera/animation feel, road traversal, weapons, shop,
   loot attraction, and mountain collision.
7. Continue behavior-preserving world/UI hotspot extraction and finish the
   remaining PM1–PM3 project/preset/level workflow gaps behind the new seams.

## Definition Of Done

Before closing a task, an agent should be able to answer:

- What player-facing behavior changed?
- Which files/modules own that behavior?
- Is the behavior campaign-shared or per-player?
- Did save compatibility change?
- Did controls, docs, UI, CI, or accessibility need updates?
- What was verified locally?
- What risks remain?
