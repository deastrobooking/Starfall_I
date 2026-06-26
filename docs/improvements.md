# Starfall I - Improvement Notes

Open issues and design follow-ups found during the May 2026 documentation/code review.
For the current agent-facing execution order, use `docs/agent_next_steps.md`.

## Fixed In This Pass

- `SaveState` now defaults to a 30 second autosave interval instead of autosaving every frame.
- `PerkTree` is initialized, awarded on level-up, saved/loaded, spendable from chapter select, and wired into HP, HP regen, beam damage, ammo/charge caps, dodge cost, and parry timing.
- Controller special tools are documented and wired through Select + D-pad.
- Castle/domain bosses in chapters 6, 7, 8, 10, and 11 now escape to faction-colored airship decks before their rematch.
- Relic-fragment sub puzzles now support five collectible pieces that assemble into one scientist relic, save partial progress, and spawn moving obstacle courses around the fragments.
- City generation now mixes glass panels, glowing window grids, brushed-metal cladding, factory ribbon windows, and stone-brick/mortar facade variants.
- Aurora Castle now has brick/mortar materials, ornate framed glowing windows, wind-animated flags, an open giant gate, a front bridge, and a hollow keep with explorable room geometry.
- Each chapter now has a secret cave system with walkable tunnel geometry, ancient/new cave dressing, moving platforms, and a save-backed discovery beacon.
- Chapter director no longer fails outright when multiple players exist; it still uses the first active player as the shared encounter anchor.
- Save/load now stores `players[]` records keyed by `PlayerIndex`, while legacy top-level stat fields remain only for old-save compatibility.
- HUD setup now spawns separate stat/weapon panels per active player instead of mirroring P1.
- Companions now carry an owner index, spawn defaults per active player, follow/heal that owner, and assign recruited allies to the player who collected the beacon.
- Crafting queues now carry an owner index, chests reward the nearest player, and vehicle buffs apply only to the activating player.
- Kill rewards are shared across active players.
- The stale physics compatibility comment in `Cargo.toml` was removed when the
  engine branch moved to Avian.
- The legacy `heavy_water_save.json` root save artifact was removed; the active runtime save remains `starfall_i_save.json` and is ignored.
- Chapter definitions are cached through a `OnceLock` catalog, so chapter lookup no longer rebuilds the 14-chapter vector every time the director or UI asks for data.
- `Esc` / controller Start now transitions between `Playing` and `Paused` with a lightweight overlay, and HUD setup is idempotent when resuming.
- Character customization now writes editable `CharacterBlueprint` data with body proportions, procedural part/material/socket/rig/animation/movement recipe sections, body-shape steppers in the designer, save/load support, and gameplay-linked movement/stat tuning.
- Pause now pauses the Avian physics clock, exposes save and save-and-title actions, shows control hints, and cleans up preserved play-session entities when returning to the title.
- Chapter 1 now has a north-coast ocean route, an island behind the mountain range, dock markers, a visible wake lane, and a boardable boat vehicle.
- The Chapter 1 starter/lab area now reserves a clearer ground zone so opaque or translucent world props are less likely to block early walking routes.
- Default heroes now spawn through upgraded runtime blueprints, body-derived stride/agility animation tuning, and a visual foot-grounding lift so the live player path matches the newer character design system more closely.
- Player grounding now uses a slightly larger controller skin/snap tolerance and higher visual foot lift to reduce terrain lips and feet sinking.
- F9 toggles a temporary collider debug overlay during play, including the invisible ground safety plane, to help track bad blockers in Level 1 playtests.
- The pause menu now has a main page and a controls/tips page, with party-wide save/title actions labeled clearly.
- The Chapter 1 boat now boards nearby passengers, locks one driver, steers along the wake lane, clamps to the water route, and requires docking before disembark.
- Hidden city reward rooms now exist as visible walkable spaces, and `HiddenReward` caches can grant credits, XP, armor, power-up ids, and special-ability upgrades/refills.
- Freeing companions now grants first-time rescue supplies to the collecting player.
- Wall slide is now the default when pushing into walls while falling, hanging is intentional through interact, and wall jumps have stronger launch, short steering lockout, and two refreshed wall-jump charges for chained climbs.
- The city now includes Sling Tower and Ramp Brick Tower traversal courses with slingshot pads, ramp obstacles, moving brick chains, rotating elevators, wall-jump shafts, reset platforms, return slingshots, marker posts, and reward caches.
- Hidden rooms now have pressure-plate puzzle dressing, visible gate pieces, and clearer reward staging.
- Enemy world loot now goes to the nearest eligible player instead of assuming one player/inventory.
- Damage events now carry player ownership, so split-screen camera shake and damage vignette feedback target the damaged player.
- Chapter encounter placement now uses the party center, and airship deck placement uses `PlayerIndex` slots instead of query order.
- Dev armor element cycling no longer calls `get_single_mut`; keyboard cycling targets P1 only.
- Engine dependencies are upgraded through Bevy 0.19.0 with Avian 0.7 on the current engine branch, with buffered gameplay events migrated to Bevy messages and the changed hierarchy, camera, cursor, render-import, ambient-light, text, and physics APIs handled.
- Engine milestone guidance now lives in `docs/engine_upgrade_milestones.md`, `agent.md` points future agents there, and GitHub Actions mirrors the local format/check/clippy/test gates.
- The multiplayer ownership policy is now documented in architecture/systems docs, and the first save tests cover per-player records, legacy hydration, `player_index` lookup, sorted save output, and clamped runtime application.
- Default player visuals now move away from tiny voxel/chibi blocks toward taller Dreamcast-anime sci-fantasy heroes with capsule limbs, layered armor, expressive eyes/hair, and glow accents.
- Robot pets now have a campaign-shared saved data model, enemy-salvage part rewards, named robot-part materials, store-build recipes, and combination gates for cars, motorcycles, tanks, boats, submarines, space jets, giant mechs, spaceships, and megaships.
- Tech upgrades now have a saved campaign ledger, chapter-select UI, robot-salvage costs, beam/missile/Sprite Turret damage multipliers, armor health bonuses, and paid rejuvenation reserve consumption.
- Player/controller feel now preserves analog stick movement strength, supports LT/RT analog trigger-axis fallback, gates controller sprint behind near-full stick tilt, and exposes kinematic controller step/snap tuning through `PlayerMovement`.
- Humanoid motion has a new agent-facing roadmap in `docs/playerengine.md`; the first implementation slice expands procedural poses for fall, flight, and one-hand wall slides, and wall sliding now acts as a stamina-backed wall clasp with a tired-slide fallback.
- Star Sabre combat now has upgrade-aware combo waves: Beam Capacitors raise the effective wave tier, opener/mid-chain/finisher slashes can emit single, dual, or triple slash waves by tier, dungeon mode keeps readable low-rank ground waves, and the procedural character animator has a distinct Star Sabre slash pose.
- Chapter scripts now use robot rescue pods and tech caches as authored level rewards, feeding rescued robot pets, robot parts, upgrade-route hints, and rejuvenation reserve into the campaign.
- Airship raid arenas now add four windup laser turrets and two moving cover platforms, making castle boss rematches use the newer armor/rejuvenation/controller movement loop.
- Everest Range terrain now smooths the imported heightmap, paints the main terrain mesh with height/slope-based grass, alpine rock, ice, and snow vertex colors, carves shared mountain-route corridors, spawns physical-looking path slabs with cyan guide studs, and adds matching glowing guide embellishments to Aurora Castle's bridge.
- Terrain tests now guard the current heightmap baseline: the Everest PNG must load, the city core remains flat, the world scale stays at 200 miles / `20_000` units, and the Everest domain keeps meaningful relief.
- Chapter select now acts as a 200 x 200 mile Everest Range fast-travel map. `src/chapters/mod.rs` owns all 14 named chapter locations, `WorldPlugin` spawns matching heightmap beacons/anchors, and chapter start moves existing players to the selected location.
- The open world now has a first full-range art pass: the imported Everest heightmap covers the entire range, the camera/fog settings support distant terrain, and `WorldPlugin` adds snowfields, glacier streams, alpine forest pockets, sci-fantasy outposts, waypoint crystals, and dragon-lair silhouettes around the larger map.
- Exploration settlements now add eight cities, villages, harbors, and outposts to the 200-mile range, with physical building clusters, `WorldAnchor`s, map markers, and saved exploration caches for future subquests.
- Settlement NPCs now open a discussion GUI from `E` / D-pad Down, with data-driven MP3 voice hooks per line in `src/discussion.rs`; Cloudrail City and Switchwork Borough also have Free Peoples guardian ships patrolling above the peaceful mega-city hubs.
- Peaceful mega cities now hide Scallarian spy drones: each city has two non-combat `CitySpyDrone`s that can be shot down, dropping save-backed `SpyData` rewards for credits, XP, and armor.
- Engine M8 counteroffensives now have a first save-backed raid slice: Cloudrail City can enter `UnderAttack`, spawn a visible UFO marker plus Scallarian drone swarm, resolve through player combat or static settlement defenses, and fail softly to `Damaged`.
- Engine M10 tech hacking now has a first save-backed slice: small Scallarian drones can be hacked from interact range, learn `blueprint_scallarian_drone_core`, add a Scout Drone command asset, temporarily link as friendly `HackedUnit`s, pulse nearby hostile targets from owner input, and count as neutralized raid threats.
- Great Scientist temple subquests now fill more of the 200-mile map with optional dungeon-like labs. Their hidden cores grant persistent mechanics upgrades for flight, laser/sabre pressure, nova missiles, and armor traversal.
- Dragon lair dungeons now exist for chapters 6-11, with SNES-RPG-style room/corridor layouts, moving bridge platforms, turret guards, raised lair dais anchors, and save-backed hoard beacons.
- Dragon lair entrances now have interactable big gates that activate single-screen top-down dungeon crawl mode, open sliding door panels, pull the party together, remap movement to top-down axes, and widen hand/Star Sabre attack arcs for hack-and-slash rooms.
- All eight human siblings now have hero combat profiles with unique signature weapon/special names, shared speed/strength/flight/magic axes, and robot-pet amplification at spawn.
- Boss mode now links 2-4 local players into one full-screen party camera for boss-tier enemies and nearby flying-drone wings, then restores split-screen after the threat clears.
- Naming canon now lives in `docs/naming.md`; Chapter 1 is `Invasion of the Scallarians`, and player-facing alien labels use Scallarian/Scallarians.
- The HUD now has a contextual `PlayerGuidance` panel for nearby talk, gate, boat, pickup, spy data, loot, and live city-spy-drone prompts.
- Swappable character parts system: `CharacterPartStyle` (HumanoidClothing/RobotMechanical) per slot, `CharacterLoadout` component, `PlayerPartLoadout` resource, and `swap_character_parts` system watching `Changed<CharacterLoadout>`. Robot spawn functions use the slot config.
- Robot factory geometry fully rewritten in `src/robots/factory.rs` with Capsule3d/Sphere/Cylinder primitives, replacing all Cuboid/box geometry for a rounder cartoon silhouette.
- Character Design (`AppState::CharacterDesign`) is now the single playable-character editor from player select and chapter select: live preview, GLB-inspired base models, silhouette sections, armor layers, color/accent controls, and Enter/Esc return to the screen that opened it.
- Perk Tree UI and Tech Upgrade Shop UI added to chapter select in `ui_plugin.rs`: per-branch and per-track colored rank rows with filled/empty rank bars (`■□`), live update systems (`PerkRowText`, `UpgradeRowText`), A–H spend perk points, Z–N purchase tech upgrades.
- Robot Garage screen (`AppState::RobotGarage`) built in `robot_garage_plugin.rs`: parts inventory and pet roster display, 9-form assembly browser (A/D navigate), auto-pet selection for Enter-to-assemble, MechCommandLink gating for advanced forms (GiantMech/SpaceShip/MegaShip), X to disassemble, [G] from chapter select.
- HP bonus application fixed: `perks.hp_bonus()` + `upgrades.armor_health_bonus()` now added to `player_stats.max_health` at spawn in `player_plugin.rs`; previously these values were computed but never applied.
- Chassis persistence: `CharacterPartStyle` derives Serialize/Deserialize; `SaveData` gains `part_loadout_body/arms/legs/shoulders` fields with `#[serde(default)]` for backward save compatibility; all save paths (`save_game`, `save_current_session`, `autosave_system`, `manual_save_system`, pause-menu save) carry `&PlayerPartLoadout`; hydrated from disk on load.
- Assembly-driven vehicle modes: `VehicleState` rewritten to use `GroundMode` (None/Motorcycle/Tank/GiantMech) and `AirMode` (None/Jet/Ship) enums instead of boolean fields. `vehicle_input()` reads `RobotPetCollection.active_assembly` to determine available modes. `apply_vehicle_buffs()` applies correct speed/jetpack/armor stats per mode (Tank slow+armor, GiantMech very slow+40 armor bonus, Jet enhanced jetpack, Ship 1.5× jet).

## High Priority

### 1. Finish per-player multiplayer ownership
**Files:** `src/plugins/*`

The player-select, input, movement, camera, combat, save, HUD, companion, crafting, chest, hidden reward, enemy loot, damage feedback, vehicle-buff, and death paths support multiple players. Remaining work is now mostly design-level ownership rather than obvious `get_single()` blockers.

Known remaining areas:

- Vehicle mode is owner-keyed, but only one active vehicle mode exists at a time for the party.
- Chapter progression, objective state, kill gates, and boss phases remain campaign-shared.
- Pause/save menu authority still needs final local-multiplayer UX decisions.
- Dev armor element cycling targets P1 until a real per-player equipment UI exists.

**Design direction:** the default ownership policy is now documented. Next, expose the remaining per-player inventory/equipment/menu flows through controller-friendly UI and remove the P1-only debug equipment path.

### 2. Add a real perk training screen
**Files:** `src/plugins/ui_plugin.rs`, `src/perks.rs`

The current chapter-select perk UI is intentionally small: keyboard shortcuts spend points and the text panel updates ranks. This proves the system, but it is not the final UX.

**Design direction:** make a dedicated perk screen with branch tabs, rank pips, disabled states, previews of stat changes, and controller navigation.

### 3. Build the full tech upgrade screen
**Files:** `src/upgrades.rs`, `src/plugins/ui_plugin.rs`, `src/plugins/weapon_plugin.rs`, `src/plugins/player_plugin.rs`, `src/plugins/vehicle_plugin.rs`

The current tech-upgrade panel proves saved ranks, robot-salvage costs, and runtime effects for beams, missile-like explosives, Sprite Turret, max health, and paid rejuvenation. It is still a compact chapter-select readout rather than a production upgrade interface.

**Design direction:** make a full upgrade screen with tabs for Weapons, Missiles, Turrets, Health/Rejuvenation, Robot Mechs, Vehicles, Ships, and Megaships. Show current stats, next-rank deltas, required parts, source hints, affordability, controller navigation, locked future branches, and an explicit reserve meter for rejuvenation.

### 4. Deepen airship levels
**Files:** `src/chapters/mod.rs`, `src/plugins/chapter_plugin.rs`

Airship levels now spawn a solid deck, rail blockers, engine visuals, four windup laser turrets, two moving cover platforms, a guard wave, and a boss rematch. That supports the requested chapter loop, but the levels could become richer platforming spaces.

**Design direction:** add side-scrolling sky debris, engine weak points, airship-specific loot, turret-disabling objectives, and boarding/extraction transitions.

### 5. Add more shared-screen boss opportunities
**Files:** `src/chapters/mod.rs`, `src/plugins/chapter_plugin.rs`, `src/plugins/enemy_plugin.rs`, `src/plugins/player_plugin.rs`

Boss mode now proves the camera and party-gathering rule, but the campaign needs more deliberate encounter hooks that trigger it: drone ships, lair alarm rooms, castle miniboss doors, airship engine rooms, and optional hoard guardians.

**Design direction:** author short shared-screen fights around `BossEnemy` or flying-drone wings, add clear entry/exit beats, and avoid pulling players during vehicle-only sequences until vehicle boss rules are defined.

### 6. Build the robot pet garage loop
**Files:** `src/robot_pets.rs`, `src/plugins/ui_plugin.rs`, `src/plugins/vehicle_plugin.rs`, future rescue/store plugins

Robot pets now have durable data, save support, salvage, store-build costs, authored rescue beacons, and combined-form recipes. They still need a playable garage/store screen, actual vehicle/mech spawning, upgrade slots, and per-player passenger/driver UX.

**Design direction:** add a controller-friendly garage UI, store-built pet purchasing from salvage, pet assignment per player, combined-form previews, and runtime adapters that connect robot forms to existing vehicle mode, boat boarding, and future mech/ship controllers.

### 7. Clarify save-game scope
**File:** `src/plugins/save_plugin.rs`

Save data now persists chapter progress, perks, robot pets, tech upgrades, player-slot character blueprints, and per-player runtime stats. The schema still carries legacy top-level stat fields for old-save compatibility and still mixes shared `wave_number` with newer campaign/chapter data.

**Design direction:** split shared campaign data from per-player character data into separate schema sections or files. Treat max health/stamina as derived values when possible.

### 8. Deepen relic-fragment challenge design
**Files:** `src/chapters/mod.rs`, `src/plugins/chapter_plugin.rs`, `src/plugins/discoverable_plugin.rs`

Fragment puzzles now prove the five-pieces-make-a-relic loop with moving bars, lift platforms, saving, and auto-assembly. The next design step is making each fragment course feel more authored and biome-specific.

**Design direction:** add hazard damage, timed doors, rotating platform chains, airship-style variants, and optional bonus fragments that reward careful platforming without blocking chapter completion.

### 9. Tune traversal toys and obstacle courses
**Files:** `src/components/world.rs`, `src/plugins/world_plugin.rs`, `src/plugins/player_plugin.rs`

Slingshot pads, rotating elevators, ramp towers, and moving brick chains now exist as first-pass traversal toys. They need hands-on tuning for trajectory, camera readability, multiplayer crowding, reset paths, and reward pacing.

**Design direction:** add route signage, checkpoint/return pads, course timers, optional medal rewards, camera assists during large launches, and course variants in caves, airships, islands, and castles.

### 10. Deepen dragon lair dungeons
**Files:** `src/plugins/world_plugin.rs`, `src/chapters/mod.rs`, `src/plugins/chapter_plugin.rs`

Dragon lairs now have physical dungeon layouts, hoard rewards, interactable big gates, and the first shared-screen top-down dungeon camera/combat mode. They still need chapter-specific enemy placement, locked doors, keys/switches, minimap labels, boss staging inside the `dragon_dungeon_chXX` anchors, and unique tile/hazard variants for each villain.

**Design direction:** make each lair a compact but readable SNES-inspired dungeon: one clear villain objective, one optional side hoard, one traversal trick, one robot-pet shortcut, and one boss-room staging beat.

## Medium Priority

### 11. Deepen hidden rewards, cave rewards, and secrets
**Files:** `src/plugins/world_plugin.rs`, `src/plugins/chapter_plugin.rs`, `src/plugins/discoverable_plugin.rs`

Secret caves now exist as discoverable places, and the city has first-pass hidden reward rooms. These prove the reward-cache path, but caves and later levels need more authored secret content.

**Design direction:** add cave-only chests, lore tablets, biome hazards, puzzle-gated doors, hidden relic-fragment shortcuts, unique armor/power-up pools, secret-room completion counts on the map/HUD, and clearer authored clues that lead players to hidden spaces.

### 12. Dual armor tracking can drift
**Files:** `src/components/player.rs`, `src/components/armor.rs`, `src/plugins/armor_plugin.rs`

`PlayerStats` tracks numeric `armor` / `max_armor`, while `ArmorSet` tracks equipped armor pieces and damage reduction. Incoming damage uses both concepts. This should be named and documented as two distinct mechanics, or consolidated.

**Design direction:** either rename `PlayerStats.armor` to `current_armor_points`, or move armor durability into `ArmorSet`.

### 13. Health maximum has multiple writers
**Files:** `src/plugins/player_plugin.rs`, `src/plugins/armor_plugin.rs`, `src/plugins/save_plugin.rs`

Level-up, armor bonuses, perk bonuses, tech upgrades, and loading all write or influence max health. The armor/perk/upgrade sync now recalculates from stable sources, but the data model would be cleaner with one source of truth.

**Design direction:** make `Health.max` the canonical runtime value and derive it from level + equipment + perks.

## Lower Priority

### 14. Second Wind needs an out-of-combat timer
**Files:** `src/plugins/player_plugin.rs`, `src/perks.rs`

`Second Wind` now consumes paid rejuvenation reserve, but it still lacks a real out-of-combat timer.

**Fix:** track recent damage/dealt-damage timestamps and enable regen only after a short quiet window.

### 15. Input conflicts need a final control pass
**Files:** `src/plugins/input_plugin.rs`, `src/plugins/armor_plugin.rs`

Bracket keys currently overlap weapon cycling and developer elemental armor cycling. This is fine for a prototype, but final controls should separate player-facing actions from debug actions.

**Fix:** move element cycling behind a debug flag, menu, or dedicated dev-only chord.

### 16. Controller support still needs hardware smoke passes
**Files:** `src/plugins/input_plugin.rs`, `src/plugins/player_plugin.rs`

The input path now supports Bevy gamepads plus the macOS native fallback, analog move magnitude, trigger-axis fallback, and Select+D-pad specials. It still needs repeated hands-on validation across at least two Xbox-style controllers, DualSense/DualShock, and reconnect ordering.

**Fix:** add an in-game controller diagnostics overlay showing per-player assignment, left/right stick values, LT/RT source, active button states, and last action fired.

---

## Code Quality — Agent Review Suggestions (2026-06-09)

These items were surfaced by a parallel code-review agent and have not yet been addressed in code. Add them to the appropriate milestone when scope and timing are clear.

### 17. Save schema versioning
**File:** `src/plugins/save_plugin.rs`

`SaveData` has no `save_version` field. As the schema grows (world sites, settlement economy, weapon ranks, etc.) it becomes hard to detect when a save was written by an older build and apply the right migration path. The current `#[serde(default)]` approach handles new fields cleanly but cannot detect removed or renamed fields or trigger proactive migration logic.

**Fix:** add `save_version: u32` (default 0 for legacy saves) to `SaveData`. Increment on each breaking schema change. On load, branch on `save_version` to run any needed field migrations before applying the data.

### 18. Better save load error surfacing
**File:** `src/plugins/save_plugin.rs`

`load_save()` currently silently falls back to defaults when the file is missing, which is fine. But deserialization errors (corrupt JSON, truncated file, bad field types) are likely swallowed with the same silent fallback, giving no user-visible signal that progress was lost.

**Fix:** distinguish "file not found" (silent default) from "file corrupt or unreadable" (log a warning + show a UI message or confirmation prompt before overwriting progress).

### 19. Platform-appropriate save path
**File:** `src/plugins/save_plugin.rs`

The save file is currently written next to the binary. On macOS this usually works during development, but shipped apps may not have write access there, and it diverges from platform conventions (macOS `~/Library/Application Support/`, Windows `%APPDATA%\`, Linux `~/.local/share/`).

**Fix:** use the `dirs` or `directories` crate to resolve the platform's user data directory at runtime and place `starfall_i_save.json` there. Add a fallback to the binary directory for dev builds.

### 20. Explicit gamepad-to-player assignment resource
**Files:** `src/plugins/input_plugin.rs`, `src/resources.rs`

Gamepad-to-player mapping is currently derived implicitly (gamepad 0 → P1, gamepad 1 → P2, etc.) from Bevy's gamepad connection order. Reconnect ordering, device hotplug, and native macOS controller fallback can cause assignment drift, especially in 3-4 player sessions.

**Fix:** introduce a small `GamepadAssignment` resource (map from `PlayerIndex` to assigned `Gamepad` or native controller id). Populate it on `GamepadConnected` events; update it on reconnect; expose it in the controller diagnostics overlay.

### 21. Narrow `#[allow(dead_code)]` at crate root
**File:** `src/main.rs` (or crate root attributes)

The crate currently carries a broad `#[allow(dead_code)]` at the crate root that hides real unused-item warnings across all modules. This makes it easier to leave stub functions and removed paths without noticing.

**Fix:** remove the crate-level allowance. Add `#[allow(dead_code)]` at the item level for any stub function that is intentionally kept (e.g., M6 no-op stubs). Let the compiler surface genuinely unused items.

### 22. Heightfield vs trimesh collider evaluation
**Files:** `src/plugins/world_plugin.rs`, `Cargo.toml`

The terrain currently uses a triangle-mesh collider built from the generated mesh triangles. For a 200-mile heightmap, a trimesh with many subdivisions can be slow to query and expensive to build. Avian also supports heightfield-style collider workflows worth evaluating for this terrain scale.

**Fix (evaluate, don't blindly apply):** profile the current trimesh collider build time and query cost. If it is a measurable bottleneck, test switching the main terrain collider to `HeightField` using the same `terrain_surface_y()` sampling grid. Document the outcome either way.

### 23. `WaveInfo` legacy isolation
**Files:** `src/plugins/save_plugin.rs`, `src/resources.rs`, `docs/systems.md`

`WaveInfo` is kept alive only for legacy loot tables and save compatibility. It is still read and written by several systems that predate the `ChapterPlugin` director. Future contributors may not realize it is a deprecated stub and add new behavior to it.

**Fix:** add a `#[doc = "Legacy stub — kept only for save compatibility and loot drop tables. Do not add new game logic here; use ChapterProgress or WorldSiteRegistry instead."]` attribute to `WaveInfo`. Audit and document each remaining read site so a future cleanup pass can remove them safely.

### 24. Replace Placeholder Capsule Visuals with Full Skeletal Animation
**Files:** `src/plugins/character_plugin.rs`

Character models are essentially modular capsules swapped around to emulate poses. Transitioning to complex movesets (e.g. chaining swings, glides, parkour) will become unmaintainable using static pose-swaps.

**Fix:** Adopt `bevy_animation` and migrate character rigs to proper glTF skeletal meshes. Connect these animations directly into the logic of `PlayerStateMachine`, mapping variables like velocity and wall-proximity to blend trees.

### 25. Art Tool Integration & Data-Driven Assets
**Files:** `src/main.rs`, `src/plugins/world_plugin.rs`, `src/character_blueprint.rs`

All character recipes, world colors, and lighting values are currently hardcoded in Rust structs, necessitating a full engine recompile for any graphical tweak. 

**Fix:** Adopt external data formats (JSON or RON) for non-engine assets, enabling hot-reloading using the Bevy asset server. Implement an immediate-mode dev GUI (like `bevy_inspector_egui`) allowing artists and designers to mutate `GameSettings` and character sliders on the fly. 

### 26. Performance Profiling and Asset Streaming
**Files:** `src/plugins/world_plugin.rs`, `src/plugins/enemy_plugin.rs`

The giant 200x200 mile Everest terrain map, its biomes, and cities load massively heavy loops inside `world_plugin.rs`. Spawning this synchronously or computing collision for all distant terrain risks massive lag spikes.

**Fix:** Adopt level-of-detail (LOD), frustum culling overrides, or spatial chunking so Avian and Bevy graphics pipelines only load/compute the active gameplay vicinity while streaming distant blocks asynchronously. Use benchmarking utilities (like `bevy_tracy`) to lock in optimization budgets.
