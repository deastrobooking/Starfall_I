# Starfall I Engine Upgrade And Milestone Runbook

This runbook is the durable guide for future Codex and engineer work on
Starfall I engine upgrades and production milestones. The current stable
baseline is Bevy `0.18.1` with `bevy_rapier3d` `0.34`.

## Baseline

- Engine stack: Rust 2021, Bevy `0.18.1`, `bevy_rapier3d` `0.34`, `serde`,
  `serde_json`, `rand`, and `noise`.
- Current Bevy message model: gameplay communication uses `Message`,
  `MessageReader`, `MessageWriter`, and `App::add_message`.
- Current compatibility debt: `src/rendering.rs` provides local bundle aliases
  for Bevy bundle shapes, and the migration touched hierarchy/despawn,
  camera/HDR, cursor options, render imports, ambient lighting, image data, and
  Rapier trimesh construction.
- Current validation baseline: `cargo fmt`, `cargo check`, and `cargo test`
  passed after the Bevy 0.18 migration. Automated tests are still thin.
- Upgrade policy: future engine upgrades are milestone-gated. Do not chase every
  release automatically.

## Local And CI Gates

Run these before pushing Rust or engine work:

```sh
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI mirrors these commands without the `dynamic` feature. The `dynamic` feature
is for local incremental development only.

Manual macOS smoke validation is required for engine bumps:

- Launch with keyboard/mouse.
- Validate at least two Xbox-style controllers, including reconnect order,
  analog left-stick tilt for walk speed, right-stick look, LT/RT aim/fire,
  RB/LB shoulder actions, Select+D-pad special tools, and Start pause.
- Join players through player select and enter `Playing`.
- Confirm split-screen cameras spawn for local players.
- Pause/resume, manual save, and save-and-title from pause.
- Exercise Chapter 1 movement: run, jump, wall slide, wall jump, hang, climb,
  dodge, parry, jetpack, slingshot, moving platform, and rotating elevator.
- Fire primary beams, use one special tool, defeat at least one enemy, and check
  HUD feedback on the correct player.
- Board the Chapter 1 boat with one driver and at least one passenger, dock, and
  disembark.
- Save, quit to title, reload, and verify per-player stats/blueprints survive.

## Future Engine Upgrade Procedure

1. Create a branch named `codex/engine-upgrade-<version>`.
2. Read the official Bevy migration guide, Bevy release notes, Rapier/Bevy
   compatibility notes, and any affected crate release notes.
3. Update `Cargo.toml` and regenerate `Cargo.lock`.
4. Fix compile/API changes by subsystem, keeping changes scoped:
   - app/bootstrap/plugins
   - events/messages
   - rendering/materials/cameras
   - hierarchy/despawn/children
   - input/window/cursor
   - physics/colliders/Rapier config
   - UI/text/layout
   - save/progression tests
5. Run the local gates and the manual macOS smoke checklist.
6. Update this runbook with a short migration note.
7. Push the branch and merge only after local and CI gates pass.

If the upgrade requires a risky architecture choice, stop and document the
decision before changing broad gameplay code.

## Milestone Naming Conventions

To prevent label collision across the three living roadmaps, use these prefixes everywhere — in code comments, commit messages, and planning docs:

| Prefix | Roadmap | File |
|--------|---------|------|
| `M#` | Engine / campaign strategy milestones | `docs/engine_upgrade_milestones.md` (this file) |
| `MM#` | Motion mechanics / traversal milestones | `docs/motion_mechanics_roadmap.md` |
| `AI#` | Enemy AI behavior milestones | `docs/ai_enemy_mechanics_roadmap.md` (future) |

Examples: "M7 Connected Platformer Route Network", "MM3 Zip And Mountain Pull", "AI2 Patrol Behavior".

## Milestones

### M0: Baseline And Documentation

Goal: make the Bevy 0.18 migration a clean, discoverable baseline.

- Record Bevy `0.18.1` and Rapier `0.34` in active docs.
- Keep `agent.md` as the concise Codex handoff and point it here for details.
- Keep `README.md`, `docs/architecture.md`, `docs/systems.md`, and
  `docs/improvements.md` aligned with the current engine.
- Remove stale Bevy `0.15` and Rapier `0.28` references from active docs.

Acceptance:

- No stale engine-version references remain in active docs.
- CI exists and mirrors the local gate commands.
- The repo documents known migration debt instead of hiding it.

### M1: Multiplayer Ownership And Save Foundation

Goal: make local multiplayer ownership and saves reliable before broad polish.

Default ownership policy:

- Campaign-shared: chapter progress, chapter objectives, kill gates, boss
  phases, unlock progression, and `PerkTree`.
- Per-player: inventory, rewards, HUD, feedback, companions, crafting ownership,
  runtime stats, character blueprints, and save `players[]` records.
- Vehicle mode stays party-shared for now: one active driver/vehicle mode with
  passengers keyed by `PlayerIndex`.
- Any player may pause/resume. Save actions stay party-wide.

First automated tests:

- Current save round-trip preserves all `players[]` records.
- Legacy top-level save fields still hydrate old saves.
- Per-player stats and character blueprints do not drift between slots.
- Reward, feedback, and companion ownership use `PlayerIndex`, not query order.

Acceptance:

- Ownership rules are documented in architecture/system docs.
- First save/ownership tests run in `cargo test`.
- New multiplayer code does not introduce global single-player assumptions.

### M2: Engine Quality Gates

Goal: make upgrades and high-risk gameplay changes repeatable.

- Keep GitHub Actions green for formatting, check, strict clippy, and tests.
- Add smoke tests for app/plugin setup and message registration after the
  ownership/save tests exist.
- Track manual macOS controller validation for each engine bump.
- Add a short verification note to PRs or commits that touch engine systems.

Acceptance:

- Future engine bumps are not complete unless local, CI, and manual gates pass.
- Any intentionally skipped gate is documented with a concrete blocker.

### M3: Engine Hygiene And Upgrade Repeatability

Goal: pay down migration debt without broad rewrites.

- Keep local render bundle shims in `src/rendering.rs` only while they reduce
  churn. Replace them gradually when touching nearby spawn code.
- Prefer Bevy 0.18-native APIs for new work: messages, current hierarchy
  components, current camera/HDR patterns, current cursor resources, and
  current UI/text layout types.
- Keep Rapier collider creation fallibility explicit; generated geometry may
  use `expect` only when the mesh generation invariant is documented.
- Keep `cargo clippy --all-targets -- -D warnings` passing. Bevy system
  signature allowances belong at crate level; do not hide correctness warnings.

Acceptance:

- Every future engine upgrade ends with a short migration note in this file.
- New code follows current Bevy/Rapier APIs instead of extending compatibility
  debt.

### M4: Full Roadmap After Foundation

Goal: connect engine work to production game progress.

After M1-M3, prioritize:

- Chapter 1 vertical slice polish: final movement feel, controller-first HUD and
  perk UI, authored secret/cave/relic beats, boat route validation, combat
  feedback, and chapter-complete rewards.
- Production systems: content authoring formats, asset pipeline rules, debug
  overlays, deterministic smoke scenes, profiling budgets, and save/version
  tooling.
- Core game depth: enemy roles, dragon/domain boss phases, cooperative rules,
  biome-specific caves/airships/relic courses, and RPG build choices.
- Presentation: production assets, animation, audio, VFX, lighting, weather,
  post-processing, and split-screen-safe composition.
- Ship readiness: settings, accessibility, remapping, captions, difficulty,
  save backups, crash-safe writes, packaging, localization hooks, and
  reproducible builds.

Acceptance:

- Gameplay roadmap work remains tied to engine needs when it touches input,
  controller UX, save schema, debug overlays, profiling, asset pipeline,
  split-screen performance, or crash-safe saves.

### M5: Reclaimable World State Foundation

Goal: make the 200-mile Everest Range a modular strategic world that can be
liberated, defended, rebuilt, and attacked again without breaking the action
RPG layer.

Core idea:

- Every major city, village, harbor, outpost, temple, castle, factory, farm,
  spaceport, power plant, research lab, defense site, bridge, and mountain gate
  should eventually be represented by a `WorldSite`-style record.
- A site has a stable id, map position, terrain layout profile, owner faction,
  liberation state, threat level, prosperity, population/refugee capacity,
  connected routes, build slots, defense slots, resource outputs, active
  missions, and save-backed damage/repair state.
- Reclaiming a site changes both the open-world map and the local action level:
  enemy props despawn or become ruins, friendly NPCs return, fast travel opens,
  building terminals appear, and new platformer routes/ramps/bridges become
  available.
- Enemy-held sites remain dangerous, with UFO shadows, drone patrols, locked
  gates, corrupted factories, blocked bridges, and boss or mini-boss anchors.

First data model:

- `WorldSiteKind`: City, Village, Farm, Factory, Spaceport, PowerPlant,
  ResearchLab, DefenseOutpost, Harbor, Temple, Castle, Mine, BridgeHub,
  MountainGate.
- `WorldSiteState`: Hidden, EnemyHeld, Contested, Liberated, Building,
  Damaged, UnderAttack, Shielded, OccupiedAgain.
- `WorldSiteOwner`: FreePeoples, Scallarian, DragonDomain, Neutral,
  PlayerAlliance.
- `WorldRouteKind`: Road, MountainPath, SkyBridge, Rail, River, OceanLane,
  Tunnel, SpaceLane, DungeonGate.

Engine/data requirements:

- Keep site definitions data-driven and deterministic. Do not hard-code all
  site behavior into one large spawn function.
- Save only compact state: owner, state, build levels, damage, assigned units,
  active raid timers, discovered secrets, and unlocked route ids.
- Spawn visuals from site state at runtime so save files stay small.
- Keep terrain placement using `terrain_surface_y()` and the settlement terrain
  layout helper so cities and build sites stay aligned with the heightmap.

First playable slice:

- Pick one reclaimed starter-adjacent outpost or village.
- Defeat the local enemy group.
- Flip the site to `Liberated`.
- Spawn a simple command terminal, friendly NPC, build marker, and one defense
  slot.
- Save/load the liberated state and prove the enemy group does not respawn
  unless a raid starts.

Acceptance:

- Site ids are stable and unique.
- Liberated/contested/enemy-held state survives save/load.
- Reclaiming a site changes world props, map marker color, available NPCs, and
  fast travel/route access.
- No site system assumes single-player ownership or a single active character.

### M6: Settlement Builder And Economy Vertical Slice

Goal: add a kid-friendly civilization-style layer inside the action RPG without
turning every moment into a menu.

Player fantasy:

- Free people from Scallarian and dragon-domain control.
- Spend money, salvage, rescued robot parts, and discovered blueprints to build
  safer cities, farms, factories, spaceports, power plants, research labs, and
  defenses.
- See rebuilt sites physically appear in the world as platformer-friendly
  structures: ramps, sky bridges, walls, hangars, shield towers, farms, conveyor
  belts, research domes, drone pads, and power conduits.

Resource model:

- Credits: general construction, repairs, NPC services.
- Food: farms, refugees, city growth, long defense events.
- Power: shields, factories, research labs, city lights, rail/bridge systems.
- Alloy: defenses, bridges, turrets, ships, mechs, factory upgrades.
- Circuits: drones, turrets, hacking tools, research, ship systems.
- Research: tech-player upgrades, component blueprints, enemy secrets.
- Fuel/Cells: vehicles, ships, rapid response, megaship deployment.
- Population/Workers: construction speed and passive output.
- Hangar Capacity: limits assigned drones, fighters, ships, and mechs.

Build site types:

| Site type | Primary output | Action-platformer expression |
|---|---|---|
| City Core | Population, services, fast travel | NPC hub, rooftops, elevators, shield plaza |
| Farm | Food, medicinal herbs | Field traversal, irrigation puzzles, defense lanes |
| Factory | Alloy, circuits, robot parts | Conveyors, hazards, assembly-line fights |
| Spaceport | Ships, fuel, rapid response | Hangars, launch ramps, anti-air fights |
| Power Plant | Power, shields | Reactor rooms, cable routes, overload missions |
| Research Lab | Research, blueprints, hacking secrets | Puzzle rooms, terminals, prototype arenas |
| Defense Outpost | Turrets, walls, patrols | Watchtowers, gun decks, shield gates |
| Harbor | Boats, submarines, sea routes | Docks, cranes, water combat, submarine gates |
| Bridge Hub | Route access | Repairable bridges, sky ramps, mountain passes |

Building tiers:

- Tier 0: Ruined/blocked. Enemy control or damaged shell.
- Tier 1: Basic rebuilt structure with one function and one local mission.
- Tier 2: Upgraded production plus visible platformer expansion.
- Tier 3: Advanced sci-fantasy form with shields, drones, or special routes.
- Tier 4: Late-game regional anchor that contributes to fleet-scale defense.

UI and interaction requirements:

- Use a world-site terminal or map panel, not a separate strategy game screen at
  first.
- Show site status, owner, threat, outputs, storage, build slots, assigned
  units, active construction, and route links.
- Keep local co-op rules clear: any player can inspect; spending party resources
  requires a short confirmation/hold action to avoid accidental purchases.
- Make build previews visible in the world before confirming when possible.

Acceptance:

- One liberated site can build at least three structures: farm, power plant, and
  defense outpost or turret grid.
- Resource costs are saved and outputs tick in a deterministic, bounded way.
- Built structures have physical world props and at least one gameplay effect.
- The UI shows why a build is blocked: missing credits, power, workers,
  blueprint, route, or prerequisite.

Current implementation note:

- `src/settlement_economy.rs` owns the saved `SettlementEconomy` resource:
  shared stockpile, build records, tier costs, robot-part costs, deterministic
  outputs, and first pure tests.
- `SaveData` persists `settlement_economy` with `serde(default)` so older saves
  still load.
- Every `map_settlements()` entry spawns a `SettlementBuildTerminal`.
  Interacting builds the next recommended structure after the settlement cache
  is recovered or the matching M5 world site is liberated.
- Built modules respawn from save data as physical world props: farms,
  factories, spaceports, power plants, research labs, defense outposts, and
  bridge hubs.
- Still missing: confirmation-hold UX, chapter-select economy panel, route
  unlock effects, assigned units, raid pressure, and per-building local
  missions.

### M7: Connected Platformer Level Network

Goal: make the strategic world feed directly into action-platformer exploration.

Connection model:

- A world site is not just a menu node. It owns entrances into local spaces:
  open-world exterior, dungeon/castle interior, factory interior, power core,
  farm field, hangar, research lab, mountain gate, sky bridge, or boss arena.
- Routes between sites are unlockable platformer spaces. Repairing a bridge,
  powering a rail, opening a tunnel, or building a sky ramp should unlock a real
  traversable connection as well as a map route.
- Some routes can be contested. Enemy UFOs, drones, mechs, or dragon forces can
  temporarily block them until players clear a raid encounter.

Level connection rules:

- `WorldRoute` definitions should list from-site, to-site, route kind, lock
  state, required build/unlock, encounter profile, and traversal props.
- Every route needs a world marker and a local spawn/return anchor.
- Route props must be collision-safe for split screen and boss mode.
- Route unlocks should preserve save/load and avoid duplicate spawned props.

First playable slice:

- Reclaim a village.
- Build/repair a bridge hub or sky ramp.
- Open a physical route to a farm or factory site.
- Enter the connected action space, clear a small encounter, collect a reward,
  and return to the map with the route saved as open.

Acceptance:

- A route's map state and physical world props agree after save/load.
- Fast travel can use liberated route anchors but cannot skip enemy-held route
  locks unless a debug setting is active.
- Connected spaces provide clear player guidance without relying only on text.

### M8: Raids, Counteroffensives, And Regional Defense

Goal: let the bad guys push back with giant UFOs, fighters, drones, robots,
mechs, and ships so liberation stays exciting after a site is freed.

Raid model:

- Enemy factions track pressure by region.
- Liberated or high-output sites can trigger counteroffensive timers.
- A raid has an attacker faction, target site, wave profile, warning time,
  required defense score, action encounter fallback, and consequences.
- Players can respond directly in action mode or assign drones/ships/defenses
  to help if they are elsewhere.

Raid types:

- Drone swarm: many small flyers, light damage, good tutorial raid.
- Robot landing: troop pods, ground mechs, factory sabotage.
- Fighter attack: fast aerial enemies, anti-air defenses matter.
- Giant UFO siege: boss-style ship, shield phases, turrets, boarding option.
- Dragon-domain assault: monsters, flame/ice hazards, castle-style threats.
- Route blockade: enemies occupy a bridge, sky lane, tunnel, or harbor route.

Defense layers:

- Static: walls, shields, turret towers, anti-air guns, minefields, patrol
  routes.
- Assigned units: drones, robot pets, fighters, ships, mechs, repair crews.
- Player action: direct combat, hacking, boss boarding, rescue civilians,
  repair power cores, protect generators.
- Strategic support: spend resources for emergency shields, rapid deployment,
  evacuation, or orbital/megaship support.

Consequences should be fun, not punitive:

- A failed defense damages buildings, lowers output, blocks a route, captures a
  terminal, or spawns a rescue mission.
- Avoid deleting major progress permanently. Prefer repair/reclaim missions.
- Early raids should be small and recoverable. Late raids can be spectacular.

Acceptance:

- One liberated city can enter `UnderAttack`, spawn a visible UFO or wave
  threat, resolve through combat or assigned defenses, then return to
  `Liberated` or `Damaged`.
- Raid state survives save/load.
- Raid UI clearly tells players where to go, what is attacking, and what is at
  risk.

Current implementation note:

- `src/raids.rs` owns the first save-backed raid layer: `RaidKind`,
  `RaidPhase`, `RaidConsequence`, `RaidResolution`, `RaidId`, `RaidRecord`,
  and `RaidRegistry`.
- The first playable slice is a tutorial Scallarian `DroneSwarm` targeting
  Cloudrail City (`WorldSiteId(4)`). A liberated Cloudrail can enter
  `UnderAttack`, warn players, spawn a visible UFO marker plus tagged drone
  enemies, and resolve back to `Liberated` when players clear the threat.
- Static settlement defense is calculated from built modules. `DefenseOutpost`
  contributes the most; `PowerPlant`, `Spaceport`, and `ResearchLab` provide
  smaller support. If Cloudrail has enough defense before the warning expires,
  the raid resolves as `Shielded`.
- Failed early raids mark the site `Damaged` instead of deleting progress or
  returning it to `EnemyHeld`.
- `SaveData.raids` persists raid records with `serde(default)`. Active raids can
  respawn their tagged enemies after save/load because runtime entities are not
  serialized.
- Route blockade support is intentionally only an integration seam for now:
  `RaidRecord.target_route: Option<WorldRouteId>` exists, but actual route
  blocking should land after Engine M7 route behavior is stable.

### 2026-06-09: M9 Command Strategy Layer — First Slice

- Added `src/commands.rs` with `CommandAssetKind`, `CommandAssetAssignment`,
  `CommandAssetId`, `CommandAsset`, `CommandAssetSaveRecord`,
  `CommandRegistry`, `CommandOverlayState`, and `initial_command_assets()`
  (2 WorkerDrones + 1 ScoutDrone at campaign start).
- `CommandRegistry.defense_score_for_site()` sums defense contributions from
  assets assigned to a site; `FighterDrone` = 4, `TurretDrone` = 5, etc.
  Assets below 30 readiness or at 0 health contribute nothing.
- `raid_counteroffensive_start_system` and `raid_tick_system` in
  `world_plugin.rs` now add `command_registry.defense_score_for_site()` to
  the static settlement defense score, so assigned fighters and mechs can
  auto-resolve or delay raids before players arrive.
- `SaveData.command_assets: Vec<CommandAssetSaveRecord>` added with
  `#[serde(default)]` for backward compatibility. `SaveParams` extended to
  14 fields (within the 16-param limit).
- `hydrate_progress_from_disk` and `load_save_on_enter` initialize from
  `initial_command_assets()` if empty, then apply save records.
- F7 toggles a bottom-left command overlay in Playing/Paused showing all
  assets grouped by assigned site plus an unassigned reserve section.
- 4 new pure tests: fighter at site contributes defense, low-readiness gives
  zero, save round-trip preserves health/assignment, initial assets count.
  72 tests passing.

### M9: Command Strategy Layer

Goal: introduce endgame-scale resource management where players control the
forces they have built, rescued, hacked, and manufactured.

Commandable assets:

- Worker drones: repair, construction, farming, salvage.
- Scout drones: reveal threats, find secrets, mark weak points.
- Fighter drones: local defense, escort, anti-fighter missions.
- Turret drones: deployable temporary defense.
- Robot pets: special bonuses, rescue missions, mech/vehicle assembly.
- Ground mechs: city defense, factory battles, heavy route clearing.
- Boats/submarines: harbor defense, sea routes, underwater objectives.
- Fighters/space jets: anti-UFO, rapid response, sky-lane escort.
- Ships/megaships: late-game region defense and final war support.

Assignment model:

- Assets are assigned to sites, routes, patrols, construction, raids, or reserve.
- Each asset has role, strength, maintenance cost, travel time, health/readiness,
  and special traits.
- Assignment should be understandable through simple icons and bars first.
- Simulated outcomes should be deterministic from saved state plus world seed.

Endgame command loop:

1. Review liberated regions and incoming threats.
2. Assign workers to rebuild and repair.
3. Assign drones/ships/mechs to defend sites and routes.
4. Launch action missions for high-risk attacks or juicy rewards.
5. Salvage enemy tech, unlock blueprints, and improve the fleet.

Acceptance:

- A command screen or map overlay lists assets, sites, threats, and assignments.
- At least one raid can be influenced by assigned defenses before players arrive.
- Asset health/readiness, repair, and redeployment persist through save/load.

### M10: Tech Hero Hacking, Takeover Mode, And Blueprints

Goal: make the tech/scientist fantasy special by allowing players to hack enemy
robots, drones, mechs, ships, and eventually parts of UFOs.

Hackable targets:

- Small drones: short hack, temporary takeover or instant disable.
- Worker robots: capture for parts, workers, or base automation.
- Combat robots: temporary ally, sabotage, or blueprint secret.
- Mechs: high-risk hack during stagger/boss phase.
- Fighters/ships: boarding or remote takeover from weak points.
- UFO systems: shield nodes, hangar doors, turrets, tractor beams, data cores.

Takeover mode:

- Player can switch control to a hacked unit or ship for a limited time.
- The original player body should be protected, parked, or follow in a safe
  autopilot state.
- Local co-op must stay readable: takeover should not strand other players.
- Boss-mode single-screen camera may be required for large ship/mech takeover.
- Takeover should expose unique controls but reuse the existing input action map
  where possible.

Blueprint/secrets rewards:

- New weapon component designs.
- Turret modules and ammo types.
- Drone chassis parts.
- Ship engines, shields, hangars, scanners.
- City building upgrades.
- Enemy weakness entries and boss phase hints.
- Research lab projects.

Engine implications:

- Add a `Hackable` component with difficulty, faction, target class, required
  range/line-of-sight, reward table, and takeover profile.
- Separate possession/control from entity ownership so local multiplayer,
  save/load, and AI can coexist.
- Add camera rules for player body, possessed drone, vehicle, ship, and boss
  arena.
- Add tests for hack reward determinism, possession handoff, and cleanup when
  the hacked unit dies or the timer expires.

Acceptance:

- One tech-player action can hack a small enemy drone, temporarily control it,
  fire/use its basic ability, return control safely, and save any blueprint
  reward.
- Hacking cannot corrupt player ownership, split-screen camera assignment, or
  party-shared save state.

Current implementation note:

- `src/hacking.rs` owns the first hacking data model: `HackTargetClass`,
  `TakeoverProfile`, `Hackable`, `HackProgress`, `HackedUnit`,
  `HackBlueprintRecord`, `HackCaptureRecord`, and the save-backed
  `HackingRegistry`.
- `src/plugins/hacking_plugin.rs` adds the first playable action: any active
  player can hold the existing interact input near a small Scallarian drone to
  complete a short hack link. The first slice deliberately uses the shared
  input map before adding a dedicated tech-hero-only UX.
- Enemy drones spawned through `spawn_enemy_entity()` now receive
  `Hackable::scallarian_drone()`. Completing the hack learns
  `blueprint_scallarian_drone_core`, records the capture in `HackingRegistry`,
  adds a `ScoutDrone` to the `CommandRegistry`, and converts the drone into a
  temporary `HackedUnit` that follows the owning player.
- While linked, pressing fire or a melee input makes the hacked drone pulse the
  nearest still-hostile enemy, using the normal enemy damage/kill event path
  before entering a short cooldown.
- Hacked units are filtered out of hostile drone AI, enemy attack systems, and
  player weapon aim/damage queries.
  If the hacked drone was part of a raid, the raid threat marker is removed so
  the takeover counts as neutralizing that threat.
- `SaveData.hacking` persists blueprint/capture records with
  `#[serde(default)]`. The temporary linked drone entity itself is runtime-only
  and safely powers down after its timer expires.

Next M10 work:

- Gate advanced hacking strength by tech/scientist hero identity, upgrade rank,
  target stagger state, and research-lab discoveries.
- Add true possession camera/control mode for remote drones, then extend the
  same handoff rules to worker robots, combat robots, mechs, ships, and UFO
  weak-point systems.
- Replace the simple follow-and-pulse prototype with true remote movement,
  aiming, and animation poses once possession camera rules are stable.
- Add deterministic reward tables for blueprints, enemy weakness entries,
  research projects, and command/fleet component designs.

### M11: Endgame Reclamation War

Goal: combine action RPG, platforming, cities, fleets, drones, resources,
bosses, and strategy into the final act.

Endgame structure:

- The map is divided into liberated, contested, and enemy stronghold regions.
- Giant UFO carriers and dragon-domain fortresses pressure multiple regions.
- Players choose where to intervene personally and where to assign built forces.
- Megaships, city shields, research projects, and fleet composition matter.
- The final chapters should still be playable as action missions, not only
  simulated strategy outcomes.

Final war beats:

- Defend multiple cities from synchronized raids.
- Board or take over a giant UFO.
- Use hacked enemy secrets to disable shield networks.
- Deploy megaships or giant mechs to protect reclaimed regions.
- Fight boss encounters that pull all players into single-screen boss mode.
- Keep civilians, farms, power plants, and spaceports alive enough to support
  the final push.

Acceptance:

- Endgame systems reuse the site, route, raid, asset, hacking, and save layers
  instead of creating one-off final mission state.
- Strategy outcomes affect available reinforcements and route safety but do not
  hard-lock players into an unwinnable campaign.
- The final act still supports 1-4 local players, controller play, save/load,
  and clear player guidance.

## Migration Notes

### 2026-06-07: Bevy 0.18 Baseline

- Upgraded Bevy to `0.18.1` and `bevy_rapier3d` to `0.34`.
- Migrated buffered gameplay events to Bevy messages.
- Updated hierarchy/despawn, query single helpers, camera/HDR, cursor options,
  render imports, bloom, ambient lighting, optional image data, UI border
  colors, and fallible Rapier trimesh creation.
- Added `.DS_Store` ignore/removal while publishing the baseline.
- Verified with `cargo fmt`, `cargo check`, and `cargo test`.

### 2026-06-07: M1 Ownership/Save Foundation

- Documented the default shared-vs-per-player ownership policy in architecture
  and systems docs.
- Added pure save tests for per-player record round-trips, legacy save
  hydration, `player_index` matching independent of record order, sorted save
  data output, and clamped runtime application.

### 2026-06-09: M5 Reclaimable World State — First Slice

- Added `WorldSiteKind`, `WorldSiteState`, `WorldSiteOwner`, `WorldSiteId`,
  `WorldSite`, `WorldSiteSaveRecord`, `WorldSiteRegistry`, and
  `initial_world_sites()` to `src/resources.rs`.
- Added `WorldSiteMarker`, `WorldSiteEnemySentinel`, and `SiteCommandTerminal`
  components to `src/components/world.rs`.
- Added `world_sites: Vec<WorldSiteSaveRecord>` to `SaveData` with
  `#[serde(default)]` for backward save compatibility.
- Added `world_site_registry: Res<WorldSiteRegistry>` to `SaveParams` (10th
  field; stays within Bevy's 16-param `SystemParamFunction` limit).
- Added `setup_world_site_registry`, `spawn_world_site_props`,
  `world_site_enemy_spawner_system`, and `site_liberation_system` to
  `world_plugin.rs`. Also added `spawn_site_terminal` and M6 economy no-op
  stubs to fix pre-existing compile errors.
- Changed `spawn_enemy_entity` in `enemy_plugin.rs` to return `Entity` so
  callers can insert `WorldSiteMarker` immediately after spawn.
- Updated `spawn_fast_travel_map` in `ui_plugin.rs` to accept
  `world_site_registry: &WorldSiteRegistry` and render colored site badges on
  the chapter-select map.
- Three initial sites defined: Iron Watchpost (Starfall Zone), Riftglass
  Village (Rift Foothills), Starfell Outpost (Crown Road).
- Removed pre-existing broken stub registrations
  (`settlement_economy_tick_system`, `settlement_build_terminal_system`) from
  `WorldPlugin::build`.

### 2026-06-09: M5 Reclaimable World State — Second Slice (Full Settlement Registry)

- `initial_world_sites()` expanded from 3 to 9 entries covering all 8
  `map_settlements()` locations plus Iron Watchpost.
- Cloudrail City (site 4) and Switchwork Borough (site 9) start `Liberated /
  FreePeoples`; the 6 remaining settlements start `EnemyHeld / Scallarian`.
- `MapSettlement` gained `site_id: Option<u16>` field; all 8 entries populated
  with matching site IDs 2–9.
- `spawn_fast_travel_map` now skips generic settlement map markers when a
  `site_id` is set — the world-site colored badge is the canonical marker.
- Added 4 tests: unique site IDs, count = 9, liberated cities have 0 defenders,
  enemy-held sites have > 0 defenders. 62 tests passing.

### 2026-06-09: N4 Dragon Lair Deepening — Chapters 7–11

- Added `spawn_ch7_ember_dungeon_extras` through `spawn_ch11_icebreaker_dungeon_extras`
  in `world_plugin.rs`.
- Each function adds a themed key gem (colored orb + point light), two sliding
  boss-gate slabs, and three `DungeonEnemySpawner` entities at entrance/mid/boss
  positions.
- Dispatch in `spawn_dragon_lair_dungeons` changed from `if chapter == 6` to a
  full `match chapter { 6..=11 }` block.

### 2026-06-09: M7 Connected Platformer Level Network — First Slice

- Added `WorldRouteKind`, `WorldRouteState`, `WorldRouteId`, `WorldRoute`,
  `WorldRouteSaveRecord`, `WorldRouteRegistry`, and `initial_world_routes()` to
  `src/resources.rs`. Six initial routes defined (IDs 1–6) covering the 9-site
  world: two start `Open` (requiring already-liberated cities), four start
  `Locked`.
- Added `WorldRouteMarker` component to `src/components/world.rs`.
- Added `world_routes: Vec<WorldRouteSaveRecord>` to `SaveData` with
  `#[serde(default)]` for backward save compatibility.
- Added `world_route_registry: Res<WorldRouteRegistry>` to `SaveParams` (12th
  field; stays within Bevy's 16-param limit).
- Added `setup_world_route_registry` and `world_route_unlock_system` to
  `world_plugin.rs`. `world_route_unlock_system` runs every frame in Playing,
  opening any Locked route whose `required_site` has become Liberated.
- `hydrate_progress_from_disk` and `load_save_on_enter` now initialize and apply
  route records from disk.
- 68 tests passing.

### 2026-06-09: M10 Tech Hero Hacking — First Drone Takeover Slice

- Added `src/hacking.rs` and `src/plugins/hacking_plugin.rs` for hack target
  classes, takeover profiles, `Hackable`, `HackProgress`, `HackedUnit`,
  blueprint/capture records, and `HackingRegistry`.
- Registered `HackingPlugin` and `HackingRegistry`, and wired
  `SaveData.hacking` / `SaveParams.hacking_registry` with `#[serde(default)]`
  compatibility.
- Small Scallarian drones spawned through `spawn_enemy_entity()` now become
  hackable. Holding interact near one completes a short hack, saves
  `blueprint_scallarian_drone_core`, records the capture, and adds a
  `ScoutDrone` command asset.
- Completed hacks convert the drone into a temporary friendly `HackedUnit` that
  follows the owning player, pulses nearby hostile enemies from owner fire/melee
  input, and powers down safely when its timer expires.
- Hacked units are excluded from hostile drone AI, enemy attack systems, and
  player weapon aim/damage queries. Raid threat markers are removed on takeover
  so hacking a raid drone counts as neutralizing it.
- `LoadRegistriesParam` SystemParam bundle carries `raid_registry`,
  `command_registry`, `hacking_registry`, and `final_war_registry` to keep
  load systems within Bevy's 16-param limit as more save resources accrue.

### 2026-06-09: M11 Endgame Reclamation War — First Slice

- New module `src/final_war.rs`:
  - `FinalWarPhase` enum: `Dormant` / `Escalating` / `Active` / `Climax`.
  - `FactionPressure { faction, pressure }` — per-faction enemy pressure counter.
  - `FinalWarRegistry` resource: phase, faction pressures, pressure tick timer,
    coordinated raid cooldown timer; `add_pressure()` auto-advances phase when
    thresholds are crossed (50 / 150 / 300 total pressure).
  - `FinalWarSaveRecord` — serde round-trip; `apply_save_record` restores full state.
  - `initial_faction_pressures()` seeds DimensionalAlien, DragonRoyalty, DragonExile at 0.
  - `pressure_accumulation_system` — ticks every 10 s; each enemy-held site owned by
    Scallarian or DragonDomain contributes +1 pressure to its faction.
  - `coordinated_raid_trigger_system` — fires when cooldown expires; targets up to
    N liberated sites simultaneously (1/2/4 based on phase); attacker/raid kind
    derived from dominant faction. Cooldown resets to 120/60/30 s by phase.
- `push_new_raid()` added to `RaidRegistry` — public constructor that allocates
  an ID and pushes a `RaidRecord` with `RaidPhase::Warning`.
- Both systems registered in `world_plugin.rs` Update chain for `Playing` state.
- `FinalWarRegistry` registered as resource in `main.rs`.
- `SaveParams` gains `final_war_registry` (16th field); `SaveData` gains
  `pub final_war: FinalWarSaveRecord`; wired into `build_save_data`, `save_game`,
  `save_current_session`, `hydrate_progress_from_disk`, and `load_save_on_enter`.
- 4 new tests: phase escalation, dominant faction, save round-trip, cooldown ordering.
- 80 tests passing.

### 2026-06-09: Local Co-op Viewport Transitions & Hysteresis Camera Tweaks

- Added `CameraDisplayTransition` resource to `src/plugins/player_plugin.rs` to track smooth transitions between split-screen and shared-screen camera modes.
- Implemented Hermite S-curve interpolation ($s = 3p^2 - 2p^3$) for camera position, rotation, and viewports during transitions.
  - Lead camera (P1) viewports expand seamlessly from split-screen to full screen.
  - Secondary player cameras (P2-P4) slide and shrink down to $1 \times 1$ corners inside their frames during transitions before turning off.
- Introduced a dual-threshold hysteresis filter for drone-based cooperative cameras:
  - Cameras group and trigger shared mode when $\ge 3$ active drones remain within $72.0\text{m}$.
  - Cameras stay linked until drone count drops $< 2$ or distance exceeds $88.0\text{m}$.
- Added a `reversion_cooldown: f32` timer of $2.5\text{s}$ to `SharedEncounterCamera` to prevent rapid toggling/camera flicker when crossing threat boundaries.
- Introduced elastic tether speed dampening ($speed\_factor = 1.0 - progress \times dot \times 0.85$) for players running *away* from co-op anchors inside shared modes, creating a tactile "heavy tug" feel near the hard radius limit while allowing unrestricted speed when moving back toward the group.
- Verified all modifications compiling cleanly. 80 tests passing.
