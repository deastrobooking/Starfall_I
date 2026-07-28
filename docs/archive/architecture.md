# Starfall I — Architecture Overview

Current verified baseline and production order: `docs/current_state.md`.
Historical UI/road/movement/weapon evidence: `docs/game_review_2026-07.md`.

Bevy 0.19 + Avian 3D, with a local physics compatibility shim in
`src/physics.rs`. Feature plugins coordinate gameplay while reusable engine,
data, editor, and platform modules live at the crate root.

## Module Map

```
src/
  main.rs                   App bootstrap, plugin registration, global resources
  game_loop.rs              Fixed-tick configuration, GameSet ordering, diagnostics overlay
  input_buffer.rs           Per-player render-to-fixed command buffering
  spatial_lod.rs            Shared distance/visibility LOD profiles and helpers
  platform_paths.rs         Platform data root plus sanitized, bounded crash reports
  state.rs                  AppState enum for game, creator, pause, game-over, and victory flows
  events.rs                 All game events + EventsPlugin
  damage.rs                 Health, Damageable, DamageInfo, apply_damage(), area_damage_falloff()
  resources.rs              Shared resources (WaveInfo, GameSettings, CurrentChapter, WorldSiteRegistry, WorldSite, WorldSiteId, WorldSiteKind, WorldSiteState, WorldSiteOwner, WorldSiteSaveRecord, initial_world_sites())
  character_blueprint.rs    Serializable editable character recipes: body, parts, materials, sockets, rig, animation, movement
  character_parts.rs        CharacterPartStyle (HumanoidClothing/RobotMechanical), CharacterLoadout, PlayerPartLoadout resource
  perks.rs                  PerkTree, PerkBranch, PerkDef — Heart / Star / Acrobat branches
  robot_pets.rs             Saved robot pet collection, salvage parts, store-build recipes, and combined vehicle/mech/ship form gates
  upgrades.rs               Saved tech upgrade ledger for beams, missiles, turrets, health, rejuvenation, and future mech links
  commands.rs               Save-backed command asset roster for workers, drones, mechs, ships, assignments, and defense scores
  raids.rs                  Save-backed raid counteroffensive records, phases, defense scoring, and raid markers
  hacking.rs                Hackable targets, takeover profiles, hacked units, blueprint/capture registry
  characters.rs             Cartoon character construction, hero color configs, designer presets
  chapters/mod.rs           All 14 ChapterDef scripts + Biome palettes
  components/
    player.rs               Player, PlayerStats, PlayerMovement, EdgeGrabState, JetpackState,
                            DodgeState, ParryState, PlayerStateMachine, CameraPitch
    weapon.rs               Weapon, WeaponInventory, SpecialWeapon, SpecialWeaponInventory,
                            BeamSabre, Projectile, MeleeCombo
    enemy.rs                EnemyType, EnemyConfig, EnemyStateMachine, Enemy, FlyingDrone,
                            DragonBoss, EnemyProjectile, DeadEnemy, BossEnemy
    character.rs            CartoonCharacter, CartoonAnimator, CartoonPart, CartoonRole
    armor.rs                ArmorSet (damage reduction)
    companion.rs            Companion component
    faction.rs              Faction enum (WizardScientist, HeroBrother, HeroSister, DragonRoyalty…)
    inventory.rs            Inventory, item slots
    mods.rs                 WeaponMod, ArmorMod data
    discoverable.rs         DiscoverableKind, Discoverable marker, relic puzzle and secret cave markers
    world.rs                World/terrain, moving platform, turret components; WorldSiteMarker, WorldSiteEnemySentinel, SiteCommandTerminal for reclaimable site props
  plugins/
    input_plugin.rs         Writes per-player PlayerInput from keyboard + gamepads
    player_plugin.rs        Movement, ledge hang, wall jump, jetpack, dodge, parry, perks, death
    weapon_plugin.rs        Projectile firing, melee combo, Star Sabre, specials, perk/tech ammo and damage, VFX
    enemy_plugin.rs         AI state machine, spawning, loot drops
    hacking_plugin.rs       Interact-to-hack drone links, hacked-unit follow/pulse behavior, blueprint capture rewards
    character_plugin.rs     Idle/walk/jump/hang cartoon pose animation; swap_character_parts
    chapter_plugin.rs       Chapter director, secret cave beacons, relic puzzles, relic-fragment obstacle courses, castle airship escalation
    ui_plugin.rs            HUD, menus, chapter select, perk training, upgrade shop, damage numbers
    world_plugin.rs         Deterministic 64-patch terrain/LOD, cities, caves, props, lighting and world sites; speed-road generation is extracted under world_plugin/roads.rs
    save_plugin.rs          F5 manual save + 30s autosave -> rotating schema-v4 slots in the platform data root; persists per-player and campaign registries via SaveParams
    armor_plugin.rs         Armor repair / equip systems
    chest_plugin.rs         Chest spawn, interact, loot roll
    crafting_plugin.rs      Crafting menu, recipe matching
    companion_plugin.rs     Companion follow AI, assist attacks
    discoverable_plugin.rs  Beacon spawn, collection trigger, secret cave charting, relic puzzle runtime, fragment assembly
    radio_plugin.rs         RadioChatter queue drain → UiMessage
    vehicle_plugin.rs       Vehicle enter/exit; GroundMode (Motorcycle/Tank/GiantMech) and AirMode (Jet/Ship) driven by assembly or blueprint
    robot_garage_plugin.rs  Assembly form browser; auto-selects eligible pets; MechCommandLink gating
    creature_forge_plugin.rs Project-backed CreatureSpec authoring, live preview, validation, and preset library
    imported_character_forge_plugin.rs GLB ingestion, per-mesh non-destructive sculpt stacks, and project records
  lsystem/                  L-system string rewriting + 3-D turtle for procedural trees
  engine_tools/             Forge project/level persistence, validation, editor runtime and panels
    editable_mesh.rs        Renderer-independent generational half-edge topology, validation, revisions, and deterministic geometry bake mapping
    mesh_selection.rs       Dense compatibility selection view compiled from EditableMeshDocument
  mesh_modifiers/           Reusable procedural mesh modifier pipeline
  robots/                   Creature recipes, procedural robot factory, designer, and presets
```

## State Flow

```
MainMenu ──► PlayerSelect ──► ChapterSelect ──► Playing ⇄ Paused
   │              │                 │              │
   │              ├─► CharacterDesign             ├─► GameOver
   │              └─► CharacterStudio              └─► Victory
   │
   └─► ProjectHub ──┬─► CreatureForge
                    ├─► ImportedCharacterForge
                    └─► Forge Editing in Playing ──► startup-level Playtest

ChapterSelect ──► CharacterDesign / RobotGarage ──► ChapterSelect
```

`CharacterDesign` and `RobotGarage` are both entered from `ChapterSelect` and return to it on Esc/confirm. Neither transitions to `Playing` directly.
`CharacterDesign` and `CharacterStudio` are also available per joined slot from
`PlayerSelect`. `CreatureForge` and `ImportedCharacterForge` return to
`ProjectHub`; opening or creating a project enters `Playing` with
`EngineToolMode::Editing` rather than creating a second editor-only gameplay
state. Imported Character Forge copies external `.glb` files under
`assets/imported_characters/`, preserves their node transforms for preview, and
stores root proportions, per-mesh modifier stacks, and source-topology modular
part assignments as Character payloads in the active Forge project. A part
assignment binds a GLB source/mesh/primitive/face set to a stable part ID,
modular slot, canonical animation joint, local pivot, and gameplay role.
Reusable assignments are separate tagged Character records, so library parts
from different source GLBs can be referenced by one imported-character spec
without copying or destructively splitting the authored mesh. Character schema
v5 keys non-destructive material and UV overrides by source/mesh/primitive.
Overrides store PBR scalar/color controls and project-relative texture paths;
standalone part records carry the matching override so appearance travels with
the reusable component. `ImportedModularRuntimePlugin` accepts an
`ImportedCharacterAssemblyRequest` on any character root, loads every referenced
GLB, extracts assigned source-face meshes, applies UV/material edits, and
attaches regions to `SkeletonRig` joints when present. Gameplay systems query
`ImportedGameplayRegion` for stable part IDs and authored roles.
Morph-only regions retain source morph deltas when projection has not split the
vertex stream. `ImportedDeformationStatus` reports preserved target count and
whether a skinned source is using the rigid-joint fallback.

`EditableMeshDocument` is the authoritative local-topology boundary for mesh
editing. It imports triangle-list `Mesh` data, rejects malformed, degenerate,
duplicate-directed-edge, and non-manifold surfaces, then exposes stable
generational IDs and explicit half-edge adjacency. Its deterministic bake view
retains a triangle-to-canonical-face map for picking and selection. Bevy
`Mesh` remains derived render/import data; persisted procedural recipes and
source-face assignments remain the authored content contract while topology
commands migrate incrementally onto the document. `MeshCommand::{Split, Flip,
Collapse}` evaluates on a cloned candidate, preserves unaffected vertex/face/
edge IDs during connectivity rebuilds, applies `AttributeTransferPolicy`,
checks collapse links and surviving-face orientation, validates, then commits a
`MeshDelta`. `MeshCommandHistory` bounds exact snapshot undo/redo to 128 entries
and fingerprints canonical state so a delta cannot be replayed into a different
same-revision document. Full corner-domain UV/color and vertex-domain skin
attribute transfer is the next prerequisite before runtime UI exposes these
topology mutations on imported production meshes.

## Core Data Flow

```
InputPlugin
    │
    ▼
PlayerInput components on each Player
    │
    ├─► PlayerPlugin: mutates PlayerMovement / StateMachine → KinematicCharacterController
    ├─► WeaponPlugin: fires Projectile entities, triggers melee, applies perk damage/ammo caps
    └─► UiPlugin: handles menu/chapter/perk/crafting input

EnemyPlugin:  reads Projectile + Player position → updates EnemyStateMachine, drones, bosses
ChapterPlugin: listens to EnemyKilledEvent / BossDefeatedEvent → advances EncounterStep
DiscoverablePlugin: handles collectible beacons, secret cave charting, ordered relic-switch puzzles, and relic-fragment assembly
UiPlugin:     listens to all events → updates per-player HUD panels, radio chatter, damage numbers
EnemyPlugin:  listens to EnemyKilledEvent → party robot salvage collection
SavePlugin:   reads PlayerIndex + PlayerStats + Health + WaveInfo + ChapterProgress + PerkTree + CharacterBlueprints + RobotPetCollection + UpgradeLedger + PlayerPartLoadout + WorldSiteRegistry → JSON on disk
```

## Ownership Policy

Starfall treats `PlayerIndex` as the stable identity for local multiplayer
runtime state. Query order is not an ownership signal.

Campaign-shared resources:

- `ChapterProgress`, chapter objectives, kill gates, boss phases, world
  sites/routes, settlements, raids, robot pets, command assets, hacking, and
  final-war state.

Per-player state:

- `PlayerProgression` (perks, tech upgrades, weapon ranks, and shop ownership),
  inventory/rewards, HUD, camera feedback, damage feedback, companions,
  crafting ownership, runtime stats, character blueprints, and save `players[]`
  records. Legacy top-level perk/upgrade/rank resources are compatibility
  mirrors, not the authority for new runtime work.

`players[]` now persists runtime stats plus inventory stacks, primary/special
selection, armor infusion, quick-item selection, and traversal ride. Legacy
records default those fields safely; load clamps slot indices and rejects an
equipped quick item that is absent from the restored inventory.

Party-shared exceptions:

- Vehicle mode remains party-shared for now: one active driver/vehicle mode,
  with passengers keyed by `PlayerIndex`.
- Pause/resume may be requested by any active player. Save actions remain
  party-wide.

## Key Design Choices

- **Platform-owned persistence and diagnostics**: saves and crash reports use
  `dirs::data_dir()/starfall_i` (with a working-directory fallback). Save slots
  are atomic and rotating. Crash reports are path-sanitized, timestamped, and
  bounded to the newest five files.
- **Bounded deterministic world caches**: terrain height grids and sky-road
  access corridors are keyed by world seed, recover poisoned locks, and retain
  at most four recently used seeds. Render terrain uses 64 high/low patches
  while physics keeps the full-resolution collider.
- **Mechanical hotspot extraction**: large feature plugins are split only at
  stable subsystem boundaries with ordering preserved and tests passing. Roads
  are the first world extraction. The shared
  `build_starfall_app(StarfallAppMode)` boundary now constructs production and
  headless profiles from the same Starfall registration graph; headless smoke
  coverage executes Startup and a steady frame without a window or renderer.
  Terrain, settlements, dungeons, UI screens/HUD, and player subsystems can now
  move behind that guardrail in small behavior-preserving slices.
- **Shared UI foundation, screen-owned behavior**: `ui_foundation` owns the
  semantic palette, fixed-unit UI scaling, stable display-text keys, and
  last-used-device/controller-family prompt rendering. Individual menus and
  gameplay panels retain their actions and layout, so further extraction does
  not centralize unrelated screen state.

- **Editable character recipes, not baked meshes**: `CharacterBlueprint` stores body sliders, procedural part recipes, materials, sockets, rig metadata, animation profiles, movement profiles, and gameplay stats. The current cartoon renderer consumes the body/material portions, while the data model leaves room for fuller mesh, rig, and editor tooling.
- **Forge content is payload-driven, not ECS-serialized**: stable `ContentRecord`
  manifests point to atomic per-record source documents. `ContentPayload` owns
  the versioned Scene/Character/Creature/Road/Building/Cave/Material/UI codec
  boundary, while Bevy `Entity` values remain transient. The first Material
  payload contract maps to configurable three-band toon and rim-light settings;
  Forge renders a live shader preview before those records are promoted into
  the broader runtime material catalog.
- **Published creature drafts do not leak into play**: Creature Forge writes
  validated `ContentPayload::Creature` records through the active project store.
  Only records whose published hash matches their draft hash enter
  `PublishedCreatureCatalog`; `CreatureSpawnOverride` resolves those stable IDs
  for dungeon enemies and retains the built-in spawn as its fallback.
- **Kinematic controller physics**: Player movement is computed manually each
  frame through the local `KinematicCharacterController` compatibility component,
  then applied with Avian move-and-slide. This gives full control over wall
  jumps, edge grabs, and jetpack without fighting a dynamic solver.
- **Base stats are authored; caps are derived**: `PlayerBaseStats` persists
  blueprint/hero level-one health and armor capacity. `PlayerStats` caches the
  effective caps derived from level, equipment, perks, and upgrades.
  `ArmorPlugin` owns ordinary-frame reconciliation and preserves fill ratios;
  save hydration uses the same pure derivation and infers missing bases from
  legacy effective-cap records.
- **State machines on components**: Both `PlayerStateMachine` and `EnemyStateMachine` use allow-list transition tables so illegal state jumps are caught at the call site. `force()` bypasses the table for death/reset paths.
- **Chapter director replaces wave loop**: `CurrentChapter` + `ChapterPlugin` replaces the old `WaveInfo`-driven loop. `WaveInfo` is kept alive only for legacy loot and save compatibility.
- **Cached chapter catalog**: `chapters/mod.rs` builds the 14 chapter definitions once through `OnceLock`; `get_chapter()` clones from that catalog instead of rebuilding scripts every frame.
- **Puzzle objectives are data-driven**: relic recovery uses chapter-scripted `PlaceRelicPuzzle` and `PlaceRelicFragmentPuzzle` steps so full relic challenges and five-piece fragment hunts can be added without bespoke level code.
- **Secret caves are shared-screen side dungeons**: the canonical fourteen-entry
  `SECRET_CAVE_LOCATIONS` table drives world placement and map projection.
  `WorldPlugin` creates each entrance gate, tunnel, combat spawners, cooperative
  platform course, checkpoint, and inner anchor. `PlaceSecretCave` supplies the
  save-backed discovery beacon; discovered ids unlock cave-specific map buttons.
  `FastTravelDestination` is a one-shot request consumed by `ChapterPlugin`,
  which moves the complete local party to the cave anchor and activates
  `DungeonCrawlState`. Failed/missing anchors leave the request pending rather
  than dropping players at an invalid position. `DungeonCrawlState.gate_id`
  binds the active shared-camera session to exactly one `DungeonExitPortal`;
  deliberate interaction returns the complete party to authored exterior slots
  and clears the mode instead of relying on contradictory distance thresholds.
  Secret-cave terrain insets are the final terrain-mesh height authority after
  mountain-road shelves. Cave profiles sample that exact cached trimesh,
  enforce bounded seams/grade/clearance, and connect directly to the chamber.
  `DungeonRoomZone` nodes and undirected `DungeonRoomPortal` edges form the
  reusable runtime room graph. `DungeonRoomState` owns transient visited/cleared
  progression, while the active node drives `DungeonCrawlState` camera focus
  and radius from the complete party centroid. Every mountain graph is entered
  through a physical `AncientCaveGate`; fast travel is not required. Combat
  spawners tag their chamber-owned enemies, allowing `DungeonEncounterDoor` and
  `DungeonEncounterReward` state to derive from that room's living encounter
  entities instead of unreliable world-wide enemy counts.
- **Castle airships are encounter steps**: boss escapes and airship-deck raids are authored as `EncounterStep`s, so castle chapters can add a flying rematch without creating a separate top-level game state.
- **PlayerInput abstraction**: All gameplay input is written into a `PlayerInput` component per player, keeping keyboard/gamepad mapping in `input_plugin.rs` and player behavior in feature plugins.
- **Audio buses remain independent**: `MusicPlayerPlugin` owns playlist/import,
  music entities, and `music_volume`; `SfxPlugin` owns procedural arcade effects,
  modular MP3 overrides, cooldowns, and `sfx_volume`. Standard action playback
  resolves custom-first/fallback-arcade, so a user manifest cannot erase the
  baseline game feedback. Both buses multiply by `master_volume`.
- **PlayerIndex ownership**: Per-player save records, progression, HUD panels,
  crafting ownership, companion ownership, rewards, and feedback use
  `PlayerIndex` as the shared key. Shared campaign systems intentionally live
  in resources such as `ChapterProgress`, `WorldSiteRegistry`, and
  `RobotPetCollection`.
- **Per-player progression**: `PlayerProgression` owns perks, tech upgrades,
  weapon ranks, and shop state for each `PlayerIndex`. Chapter Select selects an
  explicit player before spending, runtime systems read the owning component,
  and schema-v4 `players[]` persists it. Legacy top-level resources seed old
  saves only.
- **Robot pets as the vehicle spine**: `RobotPetCollection` is a shared campaign resource that stores rescued/store-built pets, enemy salvage parts, and the active combined form. The Robot Garage (`AppState::RobotGarage`) lets players assemble forms from collected pets. Assembled forms drive `GroundMode`/`AirMode` in the vehicle plugin at runtime. 3-D mech/ship controller runtimes are still future work.
- **Tech upgrades as the production upgrade spine**: each player's
  `PlayerProgression.upgrades` owns beam, missile, turret, health,
  rejuvenation, and mech-link ranks. Chapter Select spends through an explicit
  player owner; weapon, armor, and regeneration systems consume that owner's
  ledger. The top-level `UpgradeLedger` remains a legacy compatibility mirror.
- **Assembly-driven vehicle modes**: `VehicleState` uses `GroundMode` (None/Motorcycle/Tank/GiantMech) and `AirMode` (None/Jet/Ship) enums. `vehicle_input()` checks `RobotPetCollection.active_assembly` first, falling back to `PlayerLoadout` blueprints. Each mode applies its own speed/jetpack/armor buffs via `apply_vehicle_buffs()`. M toggles ground mode, J toggles air/boat mode.
- **Chassis persistence**: `CharacterPartStyle` is serializable. `PlayerPartLoadout` (body/arms/legs/shoulders slot choices) is saved and hydrated through all save paths using `#[serde(default)]` for backward compatibility.
- **Reclaimable world state via WorldSite**: the 200-mile Everest Range world can be liberated, defended, rebuilt, and attacked again. `WorldSiteRegistry` (a `Resource` holding `Vec<WorldSite>`) is the single canonical list of named world positions with state, owner, and liberation progress. Save data compacts this to `Vec<WorldSiteSaveRecord>` (id/state/owner/enemies_defeated). At runtime, `WorldSiteEnemySentinel` entities anchor each enemy-held site; `world_site_enemy_spawner_system` triggers a spawn when the player enters the sentinel radius; `site_liberation_system` flips the site to `Liberated` and spawns a `SiteCommandTerminal` when all tagged defenders are dead. Chapter-select badges on the fast-travel map update color to reflect each site's current `WorldSiteState`.
- **Recoverable raid counteroffensives**: `RaidRegistry` stores save-backed `RaidRecord`s for warning, active, and resolved counterattacks. The first Engine M8 slice targets Cloudrail City with a Scallarian drone swarm, uses settlement builds for static defense auto-resolution, and fails softly to `Damaged` instead of deleting progress.
- **Tech hacking as the takeover spine**: `HackingRegistry` stores saved blueprint and capture records. The first Engine M10 slice lets players hack small Scallarian drones, add a `ScoutDrone` command asset, temporarily link the drone as a friendly `HackedUnit`, pulse nearby hostile enemies from the owner's input, and neutralize raid threat markers without taking over the player camera yet.
- **Damage pipeline**: `DamageInfo → apply_damage() → DamageResult` with resistance multipliers, then callers emit events. Parry and armor are handled in `damage_player()` in player_plugin before the generic pipeline.
