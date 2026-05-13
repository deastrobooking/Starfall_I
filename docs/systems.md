# Starfall I — Gameplay Systems Reference

## Local Multiplayer

**Resource:** `LocalPlayerConfig` | **Files:** `src/resources.rs`, `src/plugins/input_plugin.rs`, `src/plugins/player_plugin.rs`

Set `LocalPlayerConfig.active` (1–4) before entering `AppState::Playing` to change the player count.

| Players | Camera layout |
|---|---|
| 1 | Full screen |
| 2 | Top half / bottom half |
| 3 | Top-left, top-right, bottom full |
| 4 | Four equal quadrants |

**Input assignment:**
- P1 (`PlayerIndex(0)`) — keyboard + mouse + gamepad 0
- P2–P4 (`PlayerIndex(1-3)`) — gamepad 1–3

**Character assignment per index:**
- P1: Vincenzo &nbsp; P2: Antonio &nbsp; P3: Angelo &nbsp; P4: Joseph

**Architecture:** Each player entity carries a `PlayerInput` component written by `InputPlugin` each `PreUpdate`. All game systems iterate over all `Player` entities rather than using `get_single`. Each player's camera entity is stored in a `PlayerCameraRef(Entity)` component so weapon and movement systems can resolve the correct camera per player.

**Game over:** triggers only when ALL players are dead simultaneously.

**Known limitations:**
- Special weapon keys 7–0 are keyboard-only (P1). Controller binding TBD.
- Vehicles are controlled by P1; all players share the speed/force buff.
- Camera shake is a single global pool — any player being hit shakes all cameras.

---

## Player Movement

**Plugin:** `PlayerPlugin` | **Component:** `PlayerMovement`, `EdgeGrabState`, `JetpackState`

| State | Trigger | Notes |
|---|---|---|
| Idle | No input, grounded | Default ground state |
| Moving | WASD / left stick | `walk_speed = 0.3` |
| Sprinting | Shift / LB + move | Drains stamina 15/sec |
| Jetpack | Hold Space / South while airborne | Burns `fuel_cost_per_sec = 20`/sec; regens on ground |
| WallSliding | Pushing into wall while falling | Caps fall speed at 0.35 |
| Hanging | Falling into wall with forward input | Max hang time 2.5s; drains stamina 12/sec |

Wall jump: triggered on jump press while `wall_contact_timer > 0` and airborne. Pushes away from wall normal + 25% input direction.

Climb-up: `E` / D-pad Down while hanging. Boosts player upward `climb_boost * dt * 60`.

Dodge: invulnerable during the `dodge_duration = 0.3s` window; costs 20 stamina; 0.5s cooldown.

Parry: 0.2s window on press; absorbs the next hit; 1.0s cooldown.

Stamina regens at 10/sec while not dodging.

---

## Damage Pipeline

**File:** `src/damage.rs`, `src/plugins/player_plugin.rs`

```
Incoming DamageInfo
    │
    ├── Player path (damage_player)
    │     ├── Check invulnerability / alive
    │     ├── Check parry window → block + emit PlayerParryEvent
    │     ├── ArmorSet.calculate_damage_reduction()
    │     ├── Armor absorbs 70 %, health takes 30 %
    │     ├── apply_damage(health, damageable, modified_info)
    │     ├── 0.2s post-hit invulnerability
    │     └── emit PlayerDamagedEvent
    │
    └── Enemy path (apply_damage direct)
          ├── Check invulnerability / alive
          ├── resistance_multiplier() — sums DamageResistance components
          └── health.apply_damage(final)
```

`area_damage_falloff(base, distance, radius)` returns `base * (1 - distance/radius)` for explosive splash.

---

## Weapons

**Component:** `WeaponInventory` (slots 1–6), `SpecialWeaponInventory` (slots 7–0), `BeamSabre`

| Slot | Name | Type | Auto | Notes |
|---|---|---|---|---|
| 1 | Starlight Popper | Pistol | No | 60 ammo |
| 2 | Comet Stream | Rifle | Yes | 150 ammo, fast fire |
| 3 | Sparkle Fan | Shotgun | No | 10 pellets, wide spread |
| 4 | Nova Orb | Rocket | No | Explosive, r=6.5 |
| 5 | Rainbow Ray | Laser | Yes | 220 ammo, hitscan-speed |
| 6 | Star Bubble Bombs | Grenade | No | Arc trajectory, r=8.5 |

| Special | Slot | Cooldown | Ammo |
|---|---|---|---|
| Homing Star | 7 | 2.0s | 10 |
| Tri-Star Burst | 8 | 1.5s | 15 |
| Moon Bubble | 9 | 4.0s | 5 |
| Sprite Turret | 0 | 10.0s | 3 |

**Star Sabre** (`BeamSabre`): locked until Ch.1 discoverable. Toggle `T`. Levels 1–5 increase slash damage, wave damage, slash count, and at level 3+ gains piercing; level 4+ fires dual wave; level 5 adds AoE splash.

---

## Enemy AI

**Component:** `EnemyStateMachine`, `Enemy`

```
Idle ──► Patrol ──► Chase ──► Attack
  ▲                              │
  └────────── Stunned ◄──────────┘
                  └──► Dead
```

Detection / chase / attack ranges are per-type. `difficulty_scale` (from wave or chapter) multiplies `max_health` and `attack_damage`.

| Type | HP | Dmg | Speed | Notes |
|---|---|---|---|---|
| Drone | 50 | 8 | Fast | Ranged scout, 15 XP |
| Soldier | 100 | 15 | Med | Standard invader, 25 XP |
| Heavy | 300 | 25 | Slow | Dragon brute, 50 XP |
| SpikeAlien | 80 | 20 | Fast | Aggressive, 20 XP |
| Hybrid | 1000 | 40 | Med | Boss-tier, 200 XP |

---

## Chapter System

**File:** `src/chapters/mod.rs`, `src/plugins/chapter_plugin.rs`

14 chapters, each a `Vec<EncounterStep>`. Step types:

| Step | Completion |
|---|---|
| `Dialogue` | Hold timer elapses |
| `SpawnGroup` | All enemies in group dead |
| `MidBoss` | Boss entity dead |
| `BossFight` | Boss entity dead |
| `PlaceDiscoverable` | Beacon spawned (collected separately) |
| `Outro` | Fires `ChapterCompletedEvent` |

`CurrentChapter.step_index` is advanced by `ChapterPlugin` each time the current step's condition fires.

Chapter unlock: `ChapterProgress.is_unlocked(id)` returns true if the previous chapter is in `completed`. Chapter 1 is always unlocked.

---

## Perk Tree

**File:** `src/perks.rs` | **Resource:** `PerkTree`

Three branches, 6 perks total. One point per level-up via `PerkTree.award(1)`.

| Branch | Perk | Effect | Max Rank |
|---|---|---|---|
| Heart | Family Vitality | +15 max HP/rank | 5 |
| Heart | Second Wind | +0.5 HP/sec out of combat | 4 |
| Star | Star Focus | +5% beam damage/rank | 5 |
| Star | Pocket Constellation | +15% max charges/rank | 3 |
| Acrobat | Wall-Dancer Evasion | -10% dodge stamina cost | 3 |
| Acrobat | Lucky Parry | +0.05s parry window/rank | 3 |

> Note: perk multipliers are computed by `PerkTree` methods but are not yet wired into player systems. See [improvements.md](improvements.md#14-perktree-is-never-applied-to-player-stats).

---

## Save / Load

**File:** `src/plugins/save_plugin.rs`

Save file: `starfall_i_save.json` (written next to the binary).

- **Autosave**: every 30 seconds while `Playing`.
- **Manual save**: F5 key.
- **Load**: on `OnEnter(Playing)`.

Saved fields: `level`, `experience`, `credits`, `max_health`, `max_stamina`, `max_armor`, `wave_number`.

Not saved (bug): `ChapterProgress`, `PerkTree`. See [improvements.md](improvements.md#1-save-data-doesnt-persist-chapter-progress-or-perks).

---

## Procedural World

**Module:** `src/lsystem/` | **Plugin:** `WorldPlugin`

- Terrain heightmap via `noise` crate (Perlin); seed from `GameSettings.world_seed`.
- Decorative trees via L-system string rewriting (`lsystem/mod.rs`) + 3-D turtle interpreter (`lsystem/turtle.rs`).
- Biome lighting/fog/palette set by `WorldPlugin` reading `BiomePalette` resource. Palette comes from `Biome::palette()` in `chapters/mod.rs`.

---

## Robot / Chassis System

**Module:** `src/robots/` | **Plugin:** `ChassisEditorPlugin`

`RobotStyle` in `designer.rs` defines colors and part config. `factory.rs` spawns the physical entity. `presets.rs` has named robot builds (e.g., `amp()` is the default player chassis). The chassis editor (`ChassisEditorPlugin`) lets the player restyle their robot before entering a chapter.
