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
  perks.rs                  PerkTree, PerkBranch, PerkDef — Heart / Star / Acrobat branches
  characters.rs             Cartoon character construction, hero color configs
  chapters/mod.rs           All 14 ChapterDef scripts + Biome palettes
  components/
    player.rs               Player, PlayerStats, PlayerMovement, EdgeGrabState, JetpackState,
                            DodgeState, ParryState, PlayerStateMachine, CameraPitch
    weapon.rs               Weapon, WeaponInventory, SpecialWeapon, SpecialWeaponInventory,
                            BeamSabre, Projectile, MeleeCombo
    enemy.rs                EnemyType, EnemyConfig, EnemyStateMachine, Enemy, DeadEnemy, BossEnemy
    character.rs            CartoonCharacter, CartoonAnimator, CartoonPart, CartoonRole
    armor.rs                ArmorSet (damage reduction)
    companion.rs            Companion component
    faction.rs              Faction enum (WizardScientist, HeroBrother, HeroSister, DragonRoyalty…)
    inventory.rs            Inventory, item slots
    mods.rs                 WeaponMod, ArmorMod data
    discoverable.rs         DiscoverableKind, Discoverable marker
    world.rs                World/terrain components
  plugins/
    input_plugin.rs         GameInput resource — unified keyboard + gamepad abstraction
    player_plugin.rs        Movement, ledge hang, wall jump, jetpack, dodge, parry, level-up, death
    weapon_plugin.rs        Projectile firing, melee combo, Star Sabre, specials, VFX
    enemy_plugin.rs         AI state machine, spawning, loot drops
    character_plugin.rs     Idle/walk/jump/hang cartoon pose animation
    chapter_plugin.rs       Chapter director — advances EncounterStep sequence
    ui_plugin.rs            HUD, menus, damage numbers, radio chatter overlay
    world_plugin.rs         Terrain generation (noise), biome swap, lighting
    save_plugin.rs          F5 manual save + 30s autosave → starfall_i_save.json
    armor_plugin.rs         Armor repair / equip systems
    chest_plugin.rs         Chest spawn, interact, loot roll
    crafting_plugin.rs      Crafting menu, recipe matching
    companion_plugin.rs     Companion follow AI, assist attacks
    discoverable_plugin.rs  Beacon spawn, collection trigger, relic puzzle runtime
    radio_plugin.rs         RadioChatter queue drain → UiMessage
    vehicle_plugin.rs       Vehicle enter/exit, driving physics
    chassis_editor_plugin.rs Robot chassis color/part editor
  lsystem/                  L-system string rewriting + 3-D turtle for procedural trees
  robots/                   Robot style presets (designer.rs, factory.rs, presets.rs)
```

## State Flow

```
MainMenu ──► ChapterSelect ──► ChassisEditor
                                     │
                                     ▼
                               Playing ◄──── resume ──── Paused
                                  │
                                  ▼
                               GameOver
```

## Core Data Flow

```
InputPlugin (GameInput resource)
    │
    ▼
PlayerPlugin: reads GameInput → mutates PlayerMovement / StateMachine → KinematicCharacterController
WeaponPlugin: reads GameInput → fires Projectile entities, triggers melee
EnemyPlugin:  reads Projectile + Player position → updates EnemyStateMachine
ChapterPlugin: listens to EnemyKilledEvent / BossDefeatedEvent → advances EncounterStep
DiscoverablePlugin: handles collectible beacons and ordered relic-switch puzzles
UiPlugin:     listens to all events → updates HUD, radio chatter, damage numbers
SavePlugin:   reads PlayerStats + Health + WaveInfo → JSON on disk
```

## Key Design Choices

- **KinematicPositionBased physics**: Player is a Rapier kinematic capsule; movement is computed manually each frame via `KinematicCharacterController.translation`. This gives full control over wall jumps, edge grabs, and jetpack without fighting Rapier's dynamic solver.
- **State machines on components**: Both `PlayerStateMachine` and `EnemyStateMachine` use allow-list transition tables so illegal state jumps are caught at the call site. `force()` bypasses the table for death/reset paths.
- **Chapter director replaces wave loop**: `CurrentChapter` + `ChapterPlugin` replaces the old `WaveInfo`-driven loop. `WaveInfo` is kept alive only for legacy loot and save compatibility.
- **Puzzle objectives are data-driven**: relic recovery uses chapter-scripted `PlaceRelicPuzzle` steps so new collectible objectives can be added without bespoke level code.
- **GameInput abstraction**: All input is funneled through the `GameInput` resource so keyboard and gamepad bindings are swappable in one place.
- **Damage pipeline**: `DamageInfo → apply_damage() → DamageResult` with resistance multipliers, then callers emit events. Parry and armor are handled in `damage_player()` in player_plugin before the generic pipeline.
