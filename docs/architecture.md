# Starfall I — Architecture Overview

Bevy 0.15 + Rapier 3D. Plugin-per-feature structure; all game logic lives in `src/plugins/`.

## Module Map

```
src/
  main.rs                   App bootstrap, plugin registration, global resources
  state.rs                  AppState enum (MainMenu → ChapterSelect → Playing → GameOver)
  events.rs                 All game events + EventsPlugin
  damage.rs                 Health, Damageable, DamageInfo, apply_damage(), area_damage_falloff()
  resources.rs              Shared resources (WaveInfo, GameSettings, CurrentChapter, ...)
  character_blueprint.rs    Serializable editable character recipes: body, parts, materials, sockets, rig, animation, movement
  perks.rs                  PerkTree, PerkBranch, PerkDef — Heart / Star / Acrobat branches
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
    world.rs                World/terrain, moving platform, and turret components
  plugins/
    input_plugin.rs         Writes per-player PlayerInput from keyboard + gamepads
    player_plugin.rs        Movement, ledge hang, wall jump, jetpack, dodge, parry, perks, death
    weapon_plugin.rs        Projectile firing, melee combo, Star Sabre, specials, perk ammo/damage, VFX
    enemy_plugin.rs         AI state machine, spawning, loot drops
    character_plugin.rs     Idle/walk/jump/hang cartoon pose animation
    chapter_plugin.rs       Chapter director, secret cave beacons, relic puzzles, relic-fragment obstacle courses, castle airship escalation
    ui_plugin.rs            HUD, menus, chapter select, perk training, damage numbers
    world_plugin.rs         Deterministic terrain generation, mixed ancient/new city facades, secret cave systems, prop placement, lighting
    save_plugin.rs          F5 manual save + 30s autosave -> starfall_i_save.json
    armor_plugin.rs         Armor repair / equip systems
    chest_plugin.rs         Chest spawn, interact, loot roll
    crafting_plugin.rs      Crafting menu, recipe matching
    companion_plugin.rs     Companion follow AI, assist attacks
    discoverable_plugin.rs  Beacon spawn, collection trigger, secret cave charting, relic puzzle runtime, fragment assembly
    radio_plugin.rs         RadioChatter queue drain → UiMessage
    vehicle_plugin.rs       Vehicle enter/exit, driving physics
    chassis_editor_plugin.rs Robot chassis color/part editor
  lsystem/                  L-system string rewriting + 3-D turtle for procedural trees
  robots/                   Robot style presets (designer.rs, factory.rs, presets.rs)
```

## State Flow

```
MainMenu ──► PlayerSelect ──► CharacterDesign
                  ▲                  │
                  └──────────────────┘
                  │
                  ▼
             ChapterSelect
                  │
                  ▼
             ChassisEditor
                  │
                  ▼
             Playing ◄──── resume ──── Paused
                │
                ▼
             GameOver
```

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
SavePlugin:   reads PlayerIndex + PlayerStats + Health + WaveInfo + ChapterProgress + PerkTree + CharacterBlueprints → JSON on disk
```

## Key Design Choices

- **Editable character recipes, not baked meshes**: `CharacterBlueprint` stores body sliders, procedural part recipes, materials, sockets, rig metadata, animation profiles, movement profiles, and gameplay stats. The current cartoon renderer consumes the body/material portions, while the data model leaves room for fuller mesh, rig, and editor tooling.
- **KinematicPositionBased physics**: Player is a Rapier kinematic capsule; movement is computed manually each frame via `KinematicCharacterController.translation`. This gives full control over wall jumps, edge grabs, and jetpack without fighting Rapier's dynamic solver.
- **State machines on components**: Both `PlayerStateMachine` and `EnemyStateMachine` use allow-list transition tables so illegal state jumps are caught at the call site. `force()` bypasses the table for death/reset paths.
- **Chapter director replaces wave loop**: `CurrentChapter` + `ChapterPlugin` replaces the old `WaveInfo`-driven loop. `WaveInfo` is kept alive only for legacy loot and save compatibility.
- **Cached chapter catalog**: `chapters/mod.rs` builds the 14 chapter definitions once through `OnceLock`; `get_chapter()` clones from that catalog instead of rebuilding scripts every frame.
- **Puzzle objectives are data-driven**: relic recovery uses chapter-scripted `PlaceRelicPuzzle` and `PlaceRelicFragmentPuzzle` steps so full relic challenges and five-piece fragment hunts can be added without bespoke level code.
- **Secret caves are authored world anchors**: `WorldPlugin` spawns one cave system per chapter, while `PlaceSecretCave` drops a save-backed discovery beacon at that cave's inner anchor.
- **Castle airships are encounter steps**: boss escapes and airship-deck raids are authored as `EncounterStep`s, so castle chapters can add a flying rematch without creating a separate top-level game state.
- **PlayerInput abstraction**: All gameplay input is written into a `PlayerInput` component per player, keeping keyboard/gamepad mapping in `input_plugin.rs` and player behavior in feature plugins.
- **PlayerIndex ownership**: Per-player save records, HUD panels, crafting ownership, and companion ownership use `PlayerIndex` as the shared key. Shared campaign systems still intentionally live in resources like `ChapterProgress` and `PerkTree`.
- **Global perk tree**: `PerkTree` is a shared campaign resource. Level-ups award points, chapter select spends them, combat/movement systems read the resulting multipliers, and save/load persists the ranks.
- **Damage pipeline**: `DamageInfo → apply_damage() → DamageResult` with resistance multipliers, then callers emit events. Parry and armor are handled in `damage_player()` in player_plugin before the generic pipeline.
