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
