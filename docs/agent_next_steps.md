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
- Robot pets have durable data and recipes, but the garage/store and runtime
  mech/ship controllers are still missing.
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

### N2: Controller-First Upgrade And Garage UI

Goal: turn robot pets and tech upgrades from data foundations into a usable
player loop.

- Replace compact chapter-select tech shortcuts with tabbed controller-friendly
  screens: Weapons, Missiles, Turrets, Health/Rejuvenation, Vehicles, Mechs,
  Ships, and Megaships.
- Add robot garage/store screens for rescue records, store-built pets, salvage
  costs, pet assignment, and combined-form previews.
- Show rejuvenation as a paid reserve with cost, available charges, and consume
  feedback.
- Keep `RobotPetCollection` and `UpgradeLedger` campaign-shared until a
  deliberate per-player design changes that policy.

Verification:

- Save/load after purchasing upgrades and building/rescuing pets.
- Controller navigation works without keyboard shortcuts.
- Costs, affordability, locks, and rank deltas render clearly.

Primary files:

- `src/robot_pets.rs`
- `src/upgrades.rs`
- `src/plugins/ui_plugin.rs`
- `src/plugins/chassis_editor_plugin.rs`
- `src/plugins/save_plugin.rs`

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

## Work Rules For Future Agents

- Do not shrink the world back to old `1200`-unit assumptions. Use shared
  Everest constants for map math.
- Do not add more query-order ownership. Use `PlayerIndex`.
- Do not make robot pets per-player unless the design explicitly changes.
- Do not hide save migrations. Use `serde(default)` and tests.
- Do not add a new engine version casually. Follow
  `docs/engine_upgrade_milestones.md`.
- Keep star-beam, high-fantasy sci-fi, kid-friendly tone. Avoid realistic
  firearm language and grim presentation.
- Update docs when changing controls, save scope, world scale, map flow, boss
  mode, robot pets, upgrades, or engine policy.

## Suggested Next Commit Scope

For the next coding pass, keep the scope tight:

1. Add chapter-select region labels and route/readability markers for the
   200-mile map.
2. Add a small controller-friendly screen shell for tech upgrades and robot
   garage tabs.
3. Add one manual debug overlay for chapter anchors or controller assignment.
4. Add tests only around pure data and ownership behavior touched by the pass.
