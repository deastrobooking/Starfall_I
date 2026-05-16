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
| Moving | WASD / left stick | Smoothed acceleration toward `walk_speed = 0.38` |
| Sprinting | Shift / LB + move | Drains stamina 15/sec |
| Jetpack | Hold Space / South while airborne | Burns `fuel_cost_per_sec = 20`/sec; regens on ground |
| WallSliding | Pushing into wall while falling | Caps fall speed at 0.35 |
| Hanging | Falling into wall with forward input | Max hang time 2.5s; drains stamina 12/sec |

Jumping uses a short input buffer, coyote timer, early-release jump cut, and a short apex float so near-edge jumps, taps, and high-arc jumps feel more responsive. Falling uses a stronger gravity multiplier and a capped terminal velocity.

Wall jump: triggered from buffered jump input while `wall_contact_timer > 0` and airborne. Pushes away from wall normal + 25% input direction.

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
| Drone | 50 | 8 | Fast | Flying orbit scout with laser shots, 15 XP |
| Soldier | 100 | 15 | Med | Standard invader, 25 XP |
| Heavy | 300 | 25 | Slow | Dragon brute, 50 XP |
| SpikeAlien | 80 | 20 | Fast | Aggressive, 20 XP |
| Hybrid | 1000 | 40 | Med | Boss-tier, 200 XP |

Dragon-faction boss fights add `DragonBoss`: large scaled boss bodies orbit above the closest player while leashed to their arena, advance through health phases, fire volleys, breathe cone fire, and create slam shockwaves.

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
| `PlaceRelicPuzzle` | Ordered switch puzzle solved, then relic beacon spawned |
| `Outro` | Fires `ChapterCompletedEvent` |

`CurrentChapter.step_index` is advanced by `ChapterPlugin` each time the current step's condition fires.

Chapter unlock: `ChapterProgress.is_unlocked(id)` returns true if the previous chapter is in `completed`. Chapter 1 is always unlocked.

## Relic Puzzles

**Files:** `src/chapters/mod.rs`, `src/plugins/chapter_plugin.rs`, `src/plugins/discoverable_plugin.rs`

Scientist relics are now chapter objectives rather than free pickups. A `PlaceRelicPuzzle` step now references authored world anchors rather than player-relative offsets and spawns:

- An ordered set of glowing puzzle switches.
- A `PuzzleRelicEncounter` runtime tracker.
- A scientist relic beacon that appears only after the switch sequence is solved.

Supported archetypes:

- `OrderedSwitches`: touch pylons in authored order.
- `TimedCrystalChain`: light every crystal before the countdown expires.
- `CoOpFloorPlates`: keep enough floor plates held for a short duration.
- `BeamRouting`: energize relay nodes from source to sink without collapsing the route.

Runtime behavior:

- Touch switches in the authored order.
- Touching a wrong switch after making progress resets the whole sequence.
- Solving the full sequence spawns the relic reward and clears `CurrentChapter.awaiting_puzzle` so the chapter can continue.
- Collecting the reward stores `scientist:relic_id` in `ChapterProgress.scientist_relics`.

Current seeded relic objectives:

- Chapter 1: Giacoma's Star Engine Focus
- Chapter 4: Giacoma's Harmonic Seal
- Chapter 9: Giovanni's Garden Prism
- Chapter 12: Gabrio's Mana Compass

HUD support:

- The playing HUD shows the active relic objective, archetype-specific progress, timer/hold state, and current node count while a puzzle is active.

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
- **Load**: chapter progress is hydrated on startup; player stats are applied on `OnEnter(Playing)`.

Saved fields: `level`, `experience`, `credits`, `max_health`, `max_stamina`, `max_armor`, `wave_number`, completed chapters, discoverables, recruited companions, recovered scientist relics.

Still not saved: `PerkTree`. See [improvements.md](improvements.md#1-save-data-doesnt-persist-chapter-progress-or-perks).

---

## Procedural World

**Module:** `src/lsystem/` | **Plugin:** `WorldPlugin`

- Terrain heightmap via deterministic layered waves/ridges; seed from `GameSettings.world_seed`.
- Outer districts, spaceports, trees, crystals, mountains, and authored anchors sample the terrain surface so upgraded props sit on the generated ground rather than the old flat plane.
- Decorative trees via L-system string rewriting (`lsystem/mod.rs`) + 3-D turtle interpreter (`lsystem/turtle.rs`).
- City-safe terrain is clamped to the invisible gameplay floor, keeping terrain visuals and collision from diverging below Y=0.
- Lush procedural nature adds dense grass, forest pockets, water gardens, reeds, flowers, shrubs, mossy rocks, and darker stone outcrops.
- `NatureSway` gives trees, water surfaces, reeds, flowers, shrubs, and moss caps a subtle hand-animated motion.
- Smaller residential and outer-district buildings receive stone plinths, brick or stone courses, corner blocks, roof caps, moss, and warm/cool/dark window panels.
- Moving platforms bridge rooftops, castles, and high paths; the platform system carries grounded or landing players while avoiding midair drag.
- Laser turrets track nearby players, show a brief beam windup, then apply laser damage through the same player damage/parry path.

## Character Designer

**Files:** `src/characters.rs`, `src/plugins/character_design_plugin.rs`, `src/plugins/player_plugin.rs`

- Player slots store optional outfit/accent/hair preset indices plus accessory toggles.
- Preset indices are normalized before preview, saving, and player spawning, so stale save data cannot panic by indexing outside the palette.
- The designer preview camera faces the character's front side, while the same spawned character parts are reused by the in-game player model.

---

## Robot / Chassis System

**Module:** `src/robots/` | **Plugin:** `ChassisEditorPlugin`

`RobotStyle` in `designer.rs` defines colors and part config. `factory.rs` spawns the physical entity. `presets.rs` has named robot builds (e.g., `amp()` is the default player chassis). The chassis editor (`ChassisEditorPlugin`) lets the player restyle their robot before entering a chapter.
