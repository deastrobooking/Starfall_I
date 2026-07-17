# Starfall I Agent Next Steps

This is the durable next-work guide for future Codex sessions. Use
`agent.md` for the short memory handoff, this file for production priorities,
and `docs/engine_upgrade_milestones.md` for Bevy/physics-backend upgrade procedure.
The canonical current baseline is `docs/current_state.md`. The dated four-track
audit is `docs/game_review_2026-07.md`. The evidence-based
disposition of the external 156-item idea inventory is
`docs/parallel_review_triage_2026-07.md`; use that triage instead of copying the
raw suggestion list into milestones.

The current player-facing and creator-workflow priority is
`docs/player_creator_experience_plan.md`. PX1 aiming, PX3 shop transactions,
the PM1 project registry, PM2 lock/presets, and PM3 multi-level playtest slices
are delivered. The immediate engineering boundary is the reusable app factory
and startup/plugin smoke tests described in N7/N8 and `docs/current_state.md`.

## Current Software Review

The project is in a strong prototype-to-production transition state:

- The engine baseline is current for this branch: Bevy `0.19.0`, Avian `0.7`,
  Bevy messages, the local physics compatibility shim, and local render bundle
  helpers.
- The game loop is broad but playable: local player select, character design,
  chapter select, open-world play, pause/save, combat, boss mode, dungeon
  gates, Great Scientist temple subquests, robot pets, tech upgrades, and fast
  travel all exist.
- The newest terrain baseline is the 200 x 200 mile Everest Range. Keep
  `EVEREST_RANGE_MILES`, `EVEREST_RANGE_WORLD_SIZE`, `chapter_map_locations()`,
  `map_settlements()`, terrain sampling, fast-travel UI projection, and world
  anchors aligned.
- Multiplayer ownership is production-shaped at the data boundary: per-player
  progression, shop state, runtime stats, inventory, HUD, companions, crafting,
  chests, loot, damage feedback, and vehicle ownership resolve by `PlayerIndex`.
  Four-controller hardware acceptance remains open.
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
- The main flow has shared spatial focus, D-pad/stick navigation, Confirm, Back,
  throttled repeat handling, disabled-button styling, and focus-follow scrolling.
  Remaining work is recorded four-pad controller-only acceptance in complex
  panels, not another generic focus implementation.
- A first `PlayerGuidance` HUD prompt now covers nearby interactions and city
  spy drones; future content should feed it instead of adding one-off hint text.
- Robot pets have durable data, recipes, and a button-rendered Robot Garage screen
  (`AppState::RobotGarage`) that assembles forms and drives vehicle modes at runtime.
  The garage/store still needs a controller-friendly screen, per-player passenger UX,
  store purchasing, and runtime 3-D mech/ship controllers.
- Save data still carries legacy top-level fields. Keep compatibility, but
  prefer clean shared campaign sections plus per-player records for new fields.
- Mountain caves now have monumental exterior ancient gates plus encounter-owned
  Star Chamber seals and reward reveals. Next cave pass should add authored
  reward effects/pickups, reset policy, multi-wave sequencing, and extend the
  reusable room/encounter contracts into dragon lairs and Scientist temples.
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
- Engine M8 raids are complete: Cloudrail City tutorial raid, 6 raid kinds,
  full warning→active→resolved lifecycle, static + command-asset defense,
  save/load. Route blockades land in M13.
- Engine M9 command layer first slice is complete: 9 asset kinds, defense
  integration, F7 overlay, save/load. Assignment UI and per-mission queuing
  land in M14.
- Engine M10 hacking first slice is complete: `hack_interaction_system`,
  `hack_progress_system`, `hacked_unit_control_system`, drone takeover, blueprint
  save, command asset reward. Tech-hero gating and stagger requirements land in
  M14–M15.
- Engine M11 final war first slice is complete: `FinalWarRegistry`, pressure
  accumulation, coordinated raid trigger, save/load. Narrative beats and win
  condition land in M17.
- Milestones M12–M17 remain the campaign strategy track. The M12 economy panel
  and economy tick have already landed in code, so do not treat the older
  "next immediate priority" wording as current.
- Controller support and the F8 diagnostics overlay are coded, but still need
  repeated four-pad hardware smoke passes and recorded controller-only flows.
- Performance budgets are informal. The 200-mile terrain and split-screen
  camera paths need profiling before dense content is multiplied.
- Audio import slice is complete: MP3 decoding, `F6` Music Deck, external or
  project-local music folders, hot rescan, independent music/SFX volumes,
  validated `actions.json` modular SFX assignments, and procedural arcade
  fallback. Next audio work is a button-based assignment browser/file picker,
  saved playlist preferences, crossfades, metadata/artwork, spatial action
  profiles, and measured four-player voice budgets.

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

### N2: Shared Button Navigation And Controller-First Menus

The visible buttons and shared focus layer now include spatial D-pad/stick input,
repeat, Confirm, Back, and disabled skipping/styles. What remains:

- Decide whether player select needs independent per-controller cursors or keeps
  party-shared menu authority, then hardware-test that policy.
- Record controller-only acceptance across Main Menu, Player Select, Chapter
  Select, Pause, Game Over/Victory, Robot Garage, and Character Design.
- Preserve mouse input and keep player-select ownership explicit for four controllers.

- Replace compact chapter-select tech shortcuts with tabbed controller-friendly
  screens: Weapons, Missiles, Turrets, Health/Rejuvenation, Vehicles, Mechs,
  Ships, and Megaships.
- Add garage store screen for store-built pet purchasing from salvage, per-pet rescue
  records, pet assignment, and combined-form stat previews.
- Show rejuvenation as a paid reserve with cost, available charges, and consume
  feedback.
- Add runtime 3-D mech/ship controllers so assembled forms produce visible 3-D
  vehicles, not just buffed humanoid movement.
- Keep `RobotPetCollection` campaign-shared. Read upgrade ranks from the
  selected/runtime player's `PlayerProgression`; the top-level `UpgradeLedger`
  is a legacy-save compatibility mirror.

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
- `src/plugins/character_design_plugin.rs`
- `src/plugins/save_plugin.rs`
- `src/plugins/vehicle_plugin.rs`

### N3: Multiplayer Ownership Completion

Goal: remove remaining P1-only assumptions from player-facing play.

**2026-07 ownership slice complete:** armor infusion now routes through
per-player input, crafting has per-owner controller cursors plus modal gameplay
capture, the button-based Star Loadout owns Weapons/Armor/Items/Specials/Rides,
inventory and active selections round-trip per player, active party vehicles
reject silent ownership takeover, and boat seats/driver replacement are
deterministic. Remaining work is deliberate pause/drop-in authority and
four-pad hardware acceptance.

- Hardware-test four players opening and changing their own loadouts without
  movement/fire leakage or cross-player selection.
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

**2026-07 foundation complete:** all fourteen mountain caves now enter the
single shared-camera dungeon mode, spawn chapter-scaled combat waves, include
cooperative jump/moving-platform beats, and expose save-unlocked cave checkpoint
buttons on the world map. Gate-keyed return portals now provide a deterministic
four-player exit for caves, lairs, and scientist temples. Fixed three-slab cave
tunnels have also been replaced by continuous terrain-safe profiles and
final-pass mountain insets validated across all fourteen caves. Production
follow-up now has a reusable three-node room/portal graph and party-driven
camera zones for every mountain cave. Expand those nodes into unique authored
graphs, then add encounter-owned locks,
keys, hazards, minibosses, art sets, and completion rewards per cave rather than
expanding the shared procedural layout indefinitely.

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

- EC1 is now default-on: traversal, grapple, and motor run at 64 Hz from a
  per-player latched input buffer, with overstep-interpolated camera follow.
  Hardware-test 30/60/120/144 Hz behavior and preserve the F10/
  `STARFALL_LEGACY_MOTOR=1` comparison path until the results are recorded.
- Tune acceleration, step/snap, wall slide, wall jump, climb, dodge, parry,
  jetpack, beam saber, and hand-combat values through `PlayerMovement` and
  combat data.
- Add explicit momentum, braking/turning, slope response, variable jump height,
  apex shaping, roll/spin, stomp/bounce, and landing recovery mechanics.
- Use `docs/playerengine.md` as the milestone guide for humanoid
  traversal upgrades: wall/ledge/mantle suite, grounded athletic movement,
  flight kit, combat traversal, environment physics, and boss/world integration.
- Use the existing F8 controller diagnostics overlay during acceptance; extend
  it only when a test needs a missing signal or assignment detail.
- Complete EC2 `MoveDef` frame data, cancel windows, hit/hurt layers, per-move
  i-frames, and player-received knockback. Bounded hitstop and the first reaction
  feedback layer are already delivered. Star Sabre and ranged-weapon coverage is
  delivered (July 16, 2026): `MoveLibrary` carries a sabre chain, sabre wave, and
  all six ranged primaries as data (`RangedMoveDef`), synced onto weapon slots
  and consumed by sabre cadence/knockback/damage and projectile stats; special
  weapons and enemy ranged attacks still use component tuning.
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

### N6: Racing Roads And Weapon Presentation

Goal: turn the existing geometry and combat foundations into reliable features.

First runtime slice complete: guided loop adhesion with minimum entry speed,
route checkpoints/recovery, tracking missile acquisition/steering/reacquisition,
per-player SEEK/LOCK HUD feedback, larger beam profiles, and persistent Star
Sabre blade visuals. The stunt-road slice adds 18 elevated grind lines,
same-frame four-player directional springs, jump-off rail transfers, and
turn-strength banking on the large sweeper curves. Settlement rings now provide
four-gate three-lap races with two NPC rivals each; per-player rail/air/spin
combos bank on landing and drive board rotation, stunt FOV, score messages, and
real Bevy gamepad rumble requests. Remaining production work:

- Tune the existing loop adhesion, entry/exit handoff, camera, recovery, and
  checkpoint behavior plus rail/spring camera handoff on representative ground,
  mountain, and aerial routes.
- Add a persistent race HUD/leaderboard, timed starts, opponent lap timing,
  authored trick names, landing-failure penalties, and stunt/race audio cues.
- Add automated grade, connectivity, ramp-direction, landing-clearance, barrier,
  and underside-artifact checks. Road generation extraction is delivered: the
  speed-road subsystem (56 items) lives in `src/plugins/world_plugin/roads.rs`;
  gameplay road-vehicle systems remain in `world_plugin.rs`.
- Validate the newly landed canonical collision profiles and nearest-first
  player/enemy projectile sweeps on thin city walls, dungeon doors, terrain,
  piercing lines, explosions, and NPC road traffic. Explosion cover now has a
  live Avian wall/open-path/target-exclusion fixture. Player melee and Star
  Sabre candidate selection now uses the PlayerHitbox→EnemyHurtbox layer path
  with World cover. Player combo hitboxes remain live for their authored active
  windows with per-move target deduplication. Next, extend that lifecycle to
  Star Sabre and add enemy-hitbox, pushbox, grapple-sensor, and interaction
  collider producers.
- Move the existing beam, lock-ring, trail, Star Sabre blade, and impact visuals
  into explicit profiles with measured four-view effect budgets.

Verification:

- Test loop entry at low, valid, and excessive speeds plus missed-jump recovery.
- Test four simultaneous missile locks and beam/saber fire with NPC traffic.

Primary files:

- `src/plugins/world_plugin.rs` (then extracted road modules)
- `src/components/world.rs`
- `src/components/player.rs`
- `src/plugins/player_plugin.rs`
- `src/components/weapon.rs`
- `src/plugins/weapon_plugin.rs`
- `src/plugins/ui_plugin.rs`

### N7: Performance And Tooling

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

### N8: Codebase Hardening And Stability

Goal: Pay down technical debt and increase stability for save data, input mapping, error handling, and app setup.

- Keep `.github/workflows/rust.yml` active and aligned with the four local Rust
  gates. It was restored on July 16, 2026 after an audit found the entire file
  commented out despite documentation claiming CI was active. Confirm the first
  remote push/pull-request run is green.
- Save hardening delivered (July 16, 2026): `SaveData` is schema version 4 with
  a monotonic `save_generation`; every slot and the settings file write through
  a same-directory temp file with flush/sync and atomic rename; load inspects
  all three slots and picks the valid record with the highest generation while
  warning about corrupt/unreadable slots; v3 files migrate in place and
  unsupported future versions are rejected explicitly; rotation resumes at the
  slot after the newest loaded record. Saves now live in the platform data dir
  (`dirs::data_dir()/starfall_i`) with a non-destructive first-run migration of
  legacy working-directory files. Covered by tests for interrupted writes,
  corrupt newest slots, generation ordering, legacy v3, and future versions.
- Explicit `GamepadAssignments` and Start-to-join ownership are delivered;
  reconnect releases and reclaims the stable player slot rather than assigning
  gameplay by gamepad query order. The remaining work is four-controller
  hardware acceptance, not a second assignment implementation.
- Reachable production panic audit delivered (July 16, 2026): terrain trimesh
  collider failure now warns with mesh identity and spawns terrain without
  physics instead of crashing; terrain height cache locks recover from poison;
  the shared-camera anchor falls back through threats → players → `Vec3::ZERO`.
  All remaining `unwrap`/`expect` hits in `src/` were audited and are either
  guarded by adjacent checks, on const data, or inside `#[cfg(test)]`.
- Follow-up multi-agent findings reconciled (July 17, 2026): the raw panic-like
  count was dominated by tests, and no production `get_single`/`get_single_mut`
  calls were found. Crash reports now use the platform data root, sanitize
  install/home paths, rotate timestamped files, and retain five. Terrain and
  sky-road seed caches are bounded to four entries and recover poisoned locks;
  malformed empty L-system rule symbols are ignored safely. See
  `docs/codebase_alignment_2026-07.md` for accepted and stale findings.
- Follow-up architecture audit triaged (July 17, 2026): terrain patches and
  bounded caches were stale findings; hotspot extraction and measured profiling
  remain valid. A genuine derived-stat gap remains: authored/spawned/loaded max
  health competes with armor sync's hard-coded base. Design an explicit saved
  base-stat contract and pure derived-cap function before changing writers.
  `PlayerStats.armor` is armor durability, while `ArmorSet` is equipment
  mitigation. See `docs/software_audit_triage_2026-07-17.md`.
- Terra gameplay audit triaged (July 17, 2026): projectile world obstruction,
  incremental fixed-combat scheduling, player-event ownership, armor API
  naming, and measured budgets are confirmed. Economy and vehicle findings are
  product-policy gaps with existing partial consequences/roles; Character
  Studio already feeds playable blueprints. See
  `docs/terra_gameplay_audit_triage_2026-07-17.md`.
- Before further hotspot extraction, create a reusable app-construction seam
  and startup/plugin smoke tests. Then split world terrain/settlement/dungeon,
  UI screen/HUD, and Forge panel boundaries in small ordering-preserving slices.
- Unclutter `src/main.rs` by migrating plugin-specific `init_resource` calls and localized configurations into their respective plugin `build()` configurations.
- Platform-agnostic save paths delivered with the save-hardening slice above;
  legacy working-directory files are migrated once, never overwritten.
- Dead-code triage delivered (July 16, 2026): the crate-level
  `#![allow(dead_code)]` is gone; design/roadmap scaffolding modules carry one
  commented module-level allow each, active runtime files carry item-level
  allows, and `cargo clippy --all-targets -- -D warnings` passes clean. The
  triage surfaced and fixed a real defect: pause-menu settings changes were
  never persisted (`save_settings` had no callers); the settings panel now
  saves atomically on every change. Known deliberate orphans kept under
  item-level allows: the perk ammo-cap path (superseded by the unlimited-ammo
  change) and `spawn_native_playable_character` (superseded by the modular
  player mesh).

Verification:

- GitHub Actions runs format, check, and all-target Clippy with every warning
  except the intentional content-catalog `dead_code` inventory denied, plus
  tests for a pull request and a `main` push.
- Intentionally corrupting the newest rotating save surfaces a clear warning and
  loads the newest valid older slot rather than failing silently.
- Disconnecting and reconnecting Player 2's controller correctly preserves their slot without drifting entity IDs.
- Removing or locking the save footprint does not panic the game.
- `cargo clippy --all-targets -- -D warnings` allows tighter linting rules on a module-by-module basis.

Primary files:

- `src/plugins/save_plugin.rs`
- `src/plugins/input_plugin.rs`
- `src/plugins/world_plugin.rs`
- `src/main.rs`

### N9: Graphics, VFX, And Skeletal Animation Foundation

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

### N10: Art Tools And Data-Driven Pipeline

Goal: Unblock content creators by removing hardcoded visual recipes from Rust.

Status: the Forge project registry, atomic manifests/sources/recovery, stable
content IDs, recipe validation/publishing, typed material and World Kit
authoring, immutable presets/locks, modifier stacks, multi-level projects, and
startup-level playtest are delivered. Do not start a second inspector/editor
stack beside Forge.

- Finish PM1 templates, package import/recovery/settings, and typed project
  names/paths through Forge.
- Add preset thumbnails, lock validation-configuration capture, and an explicit
  regenerate-with-same-seed action.
- Add per-level terrain/spawn/encounter/lighting metadata and budgets plus level
  rename/delete/reorder.
- Build Character/Creature production panels on the shared recipe, transaction,
  validation, and modifier services rather than serializing ECS state.

Verification:

- Project reload reproduces the same validated output from locked sources and
  seeds.
- Controller-only create/edit/validate/save/playtest works without mutating the
  shipped world when validation or persistence fails.

Primary files:

- `src/plugins/world_plugin.rs`
- `src/character_blueprint.rs`

### N11: Deep Performance And Asset Streaming

Goal: Ensure the 200-mile Everest range runs flawlessly at true 60fps, even in 4-player split-screen with dense combat encounters.

Status: `N11a` render-only distance culling and `N11b` render tiers are
implemented. The reusable
`SpatialLod` service propagates camera-aware visibility ranges through mesh
hierarchies, currently covers trees and non-enterable building shells, and
reports live coverage in the F11 overlay. Detailed trees now crossfade into
shared two-primitive proxies, building shells merge into far-distance skyline
clusters, and the 320×320 terrain render grid is split into 64 high/low patches.
The original full-resolution terrain and building colliders remain live.
Transparent materials are excluded from skyline merging, and F11 reports
visible/total high terrain, terrain proxy, and building proxy counts. See
[`guides/spatial-lod.md`](guides/spatial-lod.md). Physics and gameplay remain
loaded; profiling is the gate before choosing whether measured chunk streaming
is the next slice.

- Profile `N11b` at the world center and outer fast-travel anchors in one- and
  four-player layouts before choosing `N11c` streaming radii.
- Stream biomes and distant geometry asynchronously so the 200-mile world does not indefinitely bog down the physics solver.
- Refactor heavy queries and giant systems (e.g. `world_plugin.rs`) hitting `clippy::too_many_arguments` to use focused Bevy `SystemParam` structs.
- Implement object pooling for projectiles, enemies, and common SFX.

Verification:

- Profiling tooling (like `bevy_tracy` or frame-time diagnostics) displays 60+ FPS on medium-spec machines during 4-player split-screen boss fights.
- Walking between distant biomes results in no massive stutter frames while meshes generate or physics loads geometry.

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
| `MM#` | Motion mechanics / traversal | `docs/playerengine.md` |
| `AI#` | Enemy AI behavior | `docs/ai_enemy_mechanics_roadmap.md` (future) |
| `EC#` | Engine core (fixed-tick, combat substrate, profiling) | `docs/engine_roadmap.md` |

Examples: "M7 Connected Platformer Route Network", "MM3 Zip Pull", "AI2 Patrol", "EC1 Fixed-Tick Core".

## Work Rules For Future Agents

- Do not shrink the world back to old `1200`-unit assumptions. Use shared
  Everest constants for map math.
- Do not add more query-order ownership. Use `PlayerIndex`.
- Do not make robot pets per-player unless the design explicitly changes.
- Do not hide save migrations. Use `serde(default)` and tests.
- Do not add a new engine version casually. Follow
  `docs/engine_upgrade_milestones.md`.
- Use `M#` / `MM#` / `AI#` / `EC#` prefixes when referencing milestones; do not mix them.
- Keep star-beam, high-fantasy sci-fi, kid-friendly tone. Avoid realistic
  firearm language and grim presentation.
- Update docs when changing controls, save scope, world scale, map flow, boss
  mode, robot pets, upgrades, or engine policy.

## Suggested Next Commit Scope

Run the focused hoverboard/Saber acceptance pass before more feature work:

- Ride a flat road, steep mountain face, crest, banked curve, ramp, and loop;
  verify the board resists gravity without floating away and remains below the
  soles across procedural/imported character scales.
- Compare cruise, sprint/rocket, jump, and B/East overdrive; tune only the
  `TraversalModeState` constants after controller hardware feedback.
- Verify a new player starts with the Saber, gains waves from Solar Sabre Glyph,
  and gains Cyclone, Comet Dash, Meteor Pound, elemental gems, and Starheart
  independently as their world objects are collected.
- Save/reload two players and confirm the party relic grant is present in both
  owned ledgers, then join a later player and confirm discovered relics seed
  correctly.

Code-side presentation delivered with this slice:

- Per-player HUD reports board boost ready/recharge/overdrive, live named Saber
  techniques, owned techniques, gem count, and Legendary Starheart.
- Hoverboard/Saber actions emit safe modular audio ids and use procedural
  fallbacks until authored MP3 assignments exist.

After controller acceptance, the next bounded implementation slice is an
upgrade-catalog panel in Star Loadout: show each world relic's discovered/locked
state, effect, input, and source hint. Follow it with distinct budgeted VFX for
Cyclone, Comet Dash, Meteor Pound, each element, and Starheart; keep the current
procedural effects as the low-effect fallback for four-player split screen.
