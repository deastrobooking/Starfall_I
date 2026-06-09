# Starfall I Agent Next Steps

This is the durable next-work guide for future Codex sessions. Use
`agent.md` for the short memory handoff, this file for production priorities,
and `docs/engine_upgrade_milestones.md` for Bevy/Rapier upgrade procedure.

## Current Software Review

The project is in a strong prototype-to-production transition state:

- The engine baseline is current for this branch: Bevy `0.18.1`,
  `bevy_rapier3d` `0.34`, Bevy messages, and local render bundle helpers.
- The game loop is broad but playable: local player select, character design,
  chapter select, open-world play, pause/save, combat, boss mode, dungeon
  gates, Great Scientist temple subquests, robot pets, tech upgrades, and fast
  travel all exist.
- The newest terrain baseline is the 200 x 200 mile Everest Range. Keep
  `EVEREST_RANGE_MILES`, `EVEREST_RANGE_WORLD_SIZE`, `chapter_map_locations()`,
  `map_settlements()`, terrain sampling, fast-travel UI projection, and world
  anchors aligned.
- Multiplayer ownership is partly production-shaped: most player runtime,
  HUD, save records, companions, crafting, chests, enemy loot, damage feedback,
  and vehicle buffs are keyed by `PlayerIndex`.
- The largest code hot spots are `src/plugins/world_plugin.rs`,
  `src/plugins/ui_plugin.rs`, and `src/plugins/player_plugin.rs`. Add focused
  helpers or data tables when touching these files; avoid broad rewrites unless
  a system boundary is truly blocked.
- Automated tests now cover save compatibility, ownership basics, robot pets,
  upgrades, controller deadzones, boss camera math, hero identity, naming, and
  terrain scale. More app/plugin smoke tests are still needed.

## Production Risks

Handle these before widening content too much:

- World generation is visually richer but still centralized in one very large
  plugin. New world content should be table-driven and share terrain helpers.
- The chapter-select perk and tech panels prove data flow, but are not final
  controller-friendly production screens.
- A first `PlayerGuidance` HUD prompt now covers nearby interactions and city
  spy drones; future content should feed it instead of adding one-off hint text.
- Robot pets have durable data, recipes, and a keyboard-driven Robot Garage screen
  (`AppState::RobotGarage`) that assembles forms and drives vehicle modes at runtime.
  The garage/store still needs a controller-friendly screen, per-player passenger UX,
  store purchasing, and runtime 3-D mech/ship controllers.
- Save data still carries legacy top-level fields. Keep compatibility, but
  prefer clean shared campaign sections plus per-player records for new fields.
- Dragon lair dungeons have physical gates and top-down camera mode, but need
  keys, locks, room objectives, enemy placement, minimap labels, and boss-room
  staging.
- Great Scientist temples now grant core mechanics upgrades, but need map
  labels, enemy/puzzle room content, and clearer route clues before they feel
  like full side quests.
- Exploration settlements now fill the map physically and have first-pass
  discussion NPCs with MP3 voice hooks plus first peaceful-city spy-drone
  reward activities, but still need quest boards, shops, local enemy routes,
  route clues, and recorded voice assets.
- Engine M8 now has a first recoverable raid slice: Cloudrail City can warn,
  spawn a Scallarian drone swarm/UFO marker, resolve through combat or static
  defenses, and persist raid state. Next raid work should expand targets,
  consequences, route blockades, and assigned defenses after Engine M7 route
  behavior settles.
- Engine M10 now has a first tech-hacking slice: small Scallarian drones can be
  hacked into temporary friendly linked units, save the first drone blueprint,
  add a Scout Drone command asset, and neutralize raid threats. Next hacking
  work should add tech-hero gating, true possession camera/control rules,
  stagger requirements for larger targets, and deterministic blueprint tables.
- Controller support is coded, but still needs repeated hardware smoke passes
  and an in-game diagnostics overlay.
- Performance budgets are informal. The 200-mile terrain and split-screen
  camera paths need profiling before dense content is multiplied.

## Immediate Milestones

### N1: Stabilize The 200-Mile World

Goal: make the Everest Range feel like a real explorable world without breaking
Chapter 1.

- Add map labels/region bands to chapter select for hub, settlements, dragon
  domains, snowfields, and outposts.
- Add in-world route signs and return pads near chapter anchors, settlements,
  and Great Scientist temples.
- Use the eight `map_settlements()` anchors as future subquest boards, rescue
  hubs, village shops, local rumor sources, and voiced conversation hubs.
- Give range biomes clear local identity: snow, glacier, forest, garden,
  granite, icebreaker, city, lab, and Scallarian zones.
- Give temple subquests visible identity as ancient high-fantasy science labs,
  not just reward rooms.
- Add a small debug teleport or route overlay for testing far anchors.
- Keep all world props grounded through `terrain_surface_y()`.

Verification:

- Fast travel to every chapter marker and confirm players spawn on valid ground.
- Confirm Chapter 1 still starts unobstructed and boat route remains usable.
- Run `cargo test` terrain and chapter map tests.

Primary files:

- `src/chapters/mod.rs`
- `src/plugins/world_plugin.rs`
- `src/plugins/ui_plugin.rs`
- `docs/systems.md`

### N2: Controller-First Upgrade And Garage UI *(keyboard shell complete)*

The Robot Garage (`AppState::RobotGarage`) keyboard-driven shell is now working:
A/D browse 9 assembly forms, Enter assembles, X disassembles, assembled forms drive
`GroundMode`/`AirMode` in the vehicle plugin, and `MechCommandLink` gates advanced
forms. The chassis editor also persists part loadout to disk. What remains:

- Replace compact chapter-select tech shortcuts with tabbed controller-friendly
  screens: Weapons, Missiles, Turrets, Health/Rejuvenation, Vehicles, Mechs,
  Ships, and Megaships.
- Add garage store screen for store-built pet purchasing from salvage, per-pet rescue
  records, pet assignment, and combined-form stat previews.
- Show rejuvenation as a paid reserve with cost, available charges, and consume
  feedback.
- Add runtime 3-D mech/ship controllers so assembled forms produce visible 3-D
  vehicles, not just buffed humanoid movement.
- Keep `RobotPetCollection` and `UpgradeLedger` campaign-shared until a
  deliberate per-player design changes that policy.

Verification:

- Save/load after purchasing upgrades and building/rescuing pets.
- Controller navigation works without keyboard shortcuts.
- Costs, affordability, locks, and rank deltas render clearly.
- Assembling a form in the garage and then entering Playing reflects GroundMode/AirMode in `VehicleState`.

Primary files:

- `src/robot_pets.rs`
- `src/upgrades.rs`
- `src/plugins/ui_plugin.rs`
- `src/plugins/robot_garage_plugin.rs`
- `src/plugins/chassis_editor_plugin.rs`
- `src/plugins/save_plugin.rs`
- `src/plugins/vehicle_plugin.rs`

### N3: Multiplayer Ownership Completion

Goal: remove remaining P1-only assumptions from player-facing play.

- Build per-player inventory/equipment UI before expanding armor complexity.
- Decide pause/save authority UX for local multiplayer and document it.
- Move developer-only equipment cycling behind a debug overlay or chord.
- Add tests for reward ownership, feedback ownership, and save preservation
  when four players have different stats.

Verification:

- Four-player local smoke with separate HUD, damage feedback, pickups, and
  saves.
- Legacy save still hydrates active player slots.

Primary files:

- `src/plugins/input_plugin.rs`
- `src/plugins/ui_plugin.rs`
- `src/plugins/save_plugin.rs`
- `src/plugins/armor_plugin.rs`
- `src/components/player.rs`

### N4: Side-Dungeon Production Pass

Goal: make dragon lairs and Great Scientist temples compact SNES-style
top-down side dungeons instead of proof-of-concept room chains.

- Give each lair one main objective, one optional hoard, one robot-pet shortcut,
  one traversal trick, and one boss-room staging beat.
- Give each Great Scientist temple one mechanics trial, one puzzle lock, one
  optional cache, and one clear upgrade altar.
- Add locks, keys, switches, room labels, minimap hints, and gated boss doors.
- Place chapter-specific enemy waves inside lairs, not only around world
  anchors.
- Keep boss mode single-screen rules clear when a lair boss starts.

Verification:

- Enter and exit every lair gate.
- Enter and exit every Great Scientist temple gate.
- Confirm top-down movement, Star Sabre arcs, hand combat, rewards, and boss
  camera transitions work for 1-4 players.

Primary files:

- `src/plugins/world_plugin.rs`
- `src/plugins/chapter_plugin.rs`
- `src/chapters/mod.rs`
- `src/components/world.rs`
- `src/resources.rs`

### N5: Movement, Combat, And Controller Feel

Goal: make the core hero feel reliable before adding many more levels.

- Tune acceleration, step/snap, wall slide, wall jump, climb, dodge, parry,
  jetpack, beam saber, and hand-combat values through `PlayerMovement` and
  combat data.
- Use `docs/motion_mechanics_roadmap.md` as the milestone guide for humanoid
  traversal upgrades: wall/ledge/mantle suite, grounded athletic movement,
  flight kit, combat traversal, environment physics, and boss/world integration.
- Add controller diagnostics overlay showing player assignment, stick values,
  trigger source, and last fired action.
- Add manual smoke notes for at least two Xbox-style controllers and one
  PlayStation-style controller when available.

Verification:

- Keyboard/mouse plus two controllers through player select, Chapter 1,
  dungeon gate, boss mode, pause/save, boat, and one save/load cycle.
- `cargo test` for input/controller math remains green.

Primary files:

- `src/plugins/input_plugin.rs`
- `src/plugins/player_plugin.rs`
- `src/plugins/weapon_plugin.rs`
- `src/components/player.rs`
- `src/components/weapon.rs`

### N6: Performance And Tooling

Goal: make the growing open world safe to expand.

- Add app/plugin startup smoke tests for message registration and core states.
- Add debug overlays for anchors, terrain height, collider blockers, controller
  assignment, dungeon mode, and boss camera mode.
- Start profiling terrain/entity counts and split-screen render cost.
- Keep CI and local gates aligned.

Verification:

- `cargo fmt --check`
- `cargo check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- Manual macOS smoke when input, rendering, terrain, or engine systems change.

### N7: Codebase Hardening And Stability

Goal: Pay down technical debt and increase stability for save data, input mapping, error handling, and app setup.

- Add an explicit schema version or magic header to `SaveData` to make future save-format upgrades reliable, checking the version on load and surfacing corrupt saves gracefully.
- Tie multiplayer gamepad assignment to persistent `GamepadConnectionEvent` tracking rather than sorting by volatile `Entity::index()`, which loses deterministic tracking across reconnects.
- Audit `unwrap`/`expect` usage (especially Rapier trimesh in `src/plugins/world_plugin.rs` and save path lookups) providing fallback behaviors or explicit typed errors instead of silent panics.
- Unclutter `src/main.rs` by migrating plugin-specific `init_resource` calls and localized configurations into their respective plugin `build()` configurations.
- Move towards platform-agnostic save paths via standard library/directory hooks instead of writing `starfall_i_save.json` to the current working directory.

Verification:

- Intentionally modifying data in `starfall_i_save.json` surfaces a clear console log rather than failing silently.
- Disconnecting and reconnecting Player 2's controller correctly preserves their slot without drifting entity IDs.
- Removing or locking the save footprint does not panic the game.
- `cargo clippy --all-targets -- -D warnings` allows tighter linting rules on a module-by-module basis.

Primary files:

- `src/plugins/save_plugin.rs`
- `src/plugins/input_plugin.rs`
- `src/plugins/world_plugin.rs`
- `src/main.rs`

### N8: Graphics, VFX, And Skeletal Animation Foundation

Goal: Move beyond primitive capsule visuals to a full AAA cartoon aesthetic.

- Integrate skeletal animation support via `bevy_animation` or external glTF skeletal assets to replace static `CartoonPose` transitions.
- Enhance render pipeline with dedicated cel-shaded materials, rim lighting, and distinct visual treatments for energy beams and particle VFX.
- Bring in structural post-processing (Bloom, HDR, color grading) dynamically mapping to the current active biome/chapter room.
- Expand Inverse Kinematics (IK) for precise foot placement on sloped terrain and dynamic hand-IK during wall sliding, ledge hanging, and grappling.

Verification:

- Character models smoothly blend between animations (e.g. walk-to-run, jump-to-fall, parry).
- Projectiles and collision sparks emit particle/VFX efficiently.

Primary files:

- `src/plugins/character_plugin.rs`
- `src/rendering.rs`

### N9: Art Tools And Data-Driven Pipeline

Goal: Unblock content creators by removing hardcoded visual recipes from Rust.

- Implement hot-reloading for character presets, level configs, and robot pet geometries using RON/JSON asset loaders.
- Integrate immediate-mode dev UI (e.g. `bevy_inspector_egui` or custom) to provide a dev-only in-engine tweaker for character sliders, terrain color palettes, and lighting variables.
- Establish a prop-placement and level-design workflow that either serializes developer camera actions into world state or integrates directly via Blender glTF scenes.

Verification:

- Changing a JSON/RON configuration file automatically updates the respective entities in the live game without recompiling.
- Egui-based dev panel successfully mutates game state/variables at runtime.

Primary files:

- `src/plugins/world_plugin.rs`
- `src/character_blueprint.rs`

### N10: Deep Performance And Asset Streaming

Goal: Ensure the 200-mile Everest range runs flawlessly at true 60fps, even in 4-player split-screen with dense combat encounters.

- Establish level-of-detail (LOD) generation and distance culling for cities, rendering, and terrain meshes.
- Stream biomes and distant geometry asynchronously so the 200-mile world does not indefinitely bog down the Rapier physics solver.
- Refactor heavy queries and giant systems (e.g. `world_plugin.rs`) hitting `clippy::too_many_arguments` to use focused Bevy `SystemParam` structs.
- Implement object pooling for projectiles, enemies, and common SFX.

Verification:

- Profiling tooling (like `bevy_tracy` or frame-time diagnostics) displays 60+ FPS on medium-spec machines during 4-player split-screen boss fights.
- Walking between distant biomes results in no massive stutter frames while meshes generate or Rapier loads geometry.

Primary files:

- `src/plugins/world_plugin.rs`
- `src/main.rs`
- `src/plugins/enemy_plugin.rs`

## Milestone Naming Conventions

Use these prefixes consistently in commits, comments, and planning docs to
avoid ambiguity across the three living roadmaps:

| Prefix | Roadmap | File |
|--------|---------|------|
| `M#` | Engine / campaign strategy | `docs/engine_upgrade_milestones.md` |
| `MM#` | Motion mechanics / traversal | `docs/motion_mechanics_roadmap.md` |
| `AI#` | Enemy AI behavior | `docs/ai_enemy_mechanics_roadmap.md` (future) |

Examples: "M7 Connected Platformer Route Network", "MM3 Zip Pull", "AI2 Patrol".

## Work Rules For Future Agents

- Do not shrink the world back to old `1200`-unit assumptions. Use shared
  Everest constants for map math.
- Do not add more query-order ownership. Use `PlayerIndex`.
- Do not make robot pets per-player unless the design explicitly changes.
- Do not hide save migrations. Use `serde(default)` and tests.
- Do not add a new engine version casually. Follow
  `docs/engine_upgrade_milestones.md`.
- Use `M#` / `MM#` / `AI#` prefixes when referencing milestones; do not mix them.
- Keep star-beam, high-fantasy sci-fi, kid-friendly tone. Avoid realistic
  firearm language and grim presentation.
- Update docs when changing controls, save scope, world scale, map flow, boss
  mode, robot pets, upgrades, or engine policy.

## Suggested Next Commit Scope

For the next coding pass, keep the scope tight:

1. Add chapter-select region labels and route/readability markers for the
   200-mile map (N1 — high value, low risk).
2. Deepen one dragon lair or Great Scientist temple with keys, locked doors,
   enemy placement, and a boss-room staging beat (N4 starter).
3. Add a controller diagnostics overlay (stick values, player assignment,
   last action) for hardware smoke passes (N5 prerequisite).
4. Prototype a data-driven hot-reloading asset format for `CharacterBlueprint` or `RobotPetBlueprint` (N9 starter).
5. Add tests only around pure data and ownership behavior touched by the pass.
