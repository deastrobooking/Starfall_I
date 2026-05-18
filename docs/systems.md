# Starfall I — Gameplay Systems Reference

## Local Multiplayer

**Resource:** `LocalPlayerConfig` | **Files:** `src/resources.rs`, `src/plugins/input_plugin.rs`, `src/plugins/player_plugin.rs`

Set `LocalPlayerConfig.active` (1-4) before entering `AppState::Playing` to change the player count. The player-select screen writes this resource before chapter select.

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

**Architecture:** Each player entity carries a `PlayerInput` component written by `InputPlugin` each `PreUpdate`. Each player's camera entity is stored in a `PlayerCameraRef(Entity)` component so weapon and movement systems can resolve the correct camera per player.

**Game over:** triggers only when ALL players are dead simultaneously.

**Pause:** `Esc` / controller Start toggles between `Playing` and `Paused`. The pause menu keeps the current HUD/world entities alive, freezes the Rapier physics pipeline, offers save and save-and-title actions, and shows control hints. Returning to title from pause cleans up preserved play-session entities.

**Known limitations:**
- Chapter director spawns use the first active player as the encounter anchor.
- HUD stat/weapon panels, save snapshots, companions, crafting, chests, and vehicle buffs are keyed by `PlayerIndex`.
- Some reward pickup paths still need a complete per-player ownership pass.
- Vehicle buffs apply to the activating player, but the party still shares one active vehicle mode at a time. In Chapter 1, a boardable boat at the north dock uses the same vehicle input and follows the owning player along the visible ocean wake route to the island.
- Camera shake is a single global pool - any player being hit shakes all cameras.

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

## Character Authoring

**Files:** `src/character_blueprint.rs`, `src/characters.rs`, `src/plugins/character_design_plugin.rs`, `src/plugins/player_plugin.rs`

The character designer now stores confirmed edits as `CharacterBlueprint` data rather than only transient UI overrides. A blueprint includes:

- `BodyRecipe` proportions: height, shoulders, chest, arms, legs, hands, feet, head, posture/mass fields.
- Procedural part, material, socket, rig, animation, movement, equipment, and editor recipe sections.
- Derived `MovementProfile` and `GameplayStatsRecipe` values.

The current runtime consumes the first practical slice of that model:

| Body value | Runtime effect |
|---|---|
| Height / legs | Character proportions, collider height, stride, jump force |
| Shoulders / chest / hips | Character width, collider radius, armor capacity |
| Arms / hands | Visual reach proportions and derived melee reach metadata |
| Feet / mass | Dodge tuning, stamina, armor, health, knockback metadata |
| Head | Cartoon head and hair scale |

Confirmed blueprints are kept on each player-select slot, saved into `starfall_i_save.json`, and restored on load. When a player has not authored a custom body yet, the runtime now creates an upgraded default hero blueprint so movement stats, collider proportions, visible body shape, and animation stride all use the same data path.

`CartoonCharacter` carries stride/agility metadata derived from `BodyRecipe`, and `CharacterPlugin` uses it to tune walk/run phase speed and limb swing. The character renderer also applies a visual ground lift so oversized cartoon feet sit on the terrain instead of dipping below the player capsule.

The full procedural mesh/rig/timeline editor is still future work; the saved schema is intentionally larger than the current renderer so it can grow without replacing save data.

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

**Component:** `WeaponInventory` (slots 1-6), `SpecialWeaponInventory` (slots 7-0), `BeamSabre`

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

Special tools fire from keyboard `7`, `8`, `9`, `0`, or from controller Select + D-pad Up/Down/Left/Right.

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

14 chapters, each a `Vec<EncounterStep>`. The chapter catalog is built once through `OnceLock`, while `get_chapter()` returns a cloned definition for the active chapter. Step types:

| Step | Completion |
|---|---|
| `Dialogue` | Hold timer elapses |
| `SpawnGroup` | All enemies in group dead |
| `MidBoss` | Boss entity dead |
| `BossFight` | Boss entity dead |
| `AirshipEscape` | Escape airship prop appears; short dialogue timer elapses |
| `AirshipDeckRaid` | Party moves to a spawned airship deck; all deck guards dead |
| `PlaceDiscoverable` | Beacon spawned (collected separately) |
| `PlaceSecretCave` | Secret cave discovery beacon spawned at an authored cave anchor |
| `PlaceRelicPuzzle` | Ordered switch puzzle solved, then relic beacon spawned |
| `PlaceRelicFragmentPuzzle` | Five relic fragments collected from a moving obstacle course |
| `Outro` | Fires `ChapterCompletedEvent` |

`CurrentChapter.step_index` is advanced by `ChapterPlugin` each time the current step's condition fires.

Chapter unlock: `ChapterProgress.is_unlocked(id)` returns true if the previous chapter is in `completed`. Chapter 1 is always unlocked.

## Castle Airship Escalation

**Files:** `src/chapters/mod.rs`, `src/plugins/chapter_plugin.rs`

Major castle/domain bosses can use a three-part escalation:

1. `BossFight` resolves the castle fight.
2. `AirshipEscape` spawns a visible airship and plays the boss escape line.
3. `AirshipDeckRaid` spawns a walkable airship deck, moves active players onto it, and fills it with guards before the boss rematch.

Current chapters using this loop:

- Chapter 6: Collosar's Crown Airship
- Chapter 7: Tarack's Ember Airship
- Chapter 8: Shread's Scrapwing Airship
- Chapter 10: Ragar's Granite Airship
- Chapter 11: Blackskull's Icebreaker Airship

The airship deck is a temporary chapter-spawned platform with colliders, rail blockers, engine visuals, and faction-colored materials. It is cleaned up when the next chapter starts.

## Secret Caves

**Files:** `src/chapters/mod.rs`, `src/plugins/chapter_plugin.rs`, `src/plugins/world_plugin.rs`, `src/plugins/discoverable_plugin.rs`

Every chapter now has one optional cave route to discover. `WorldPlugin` spawns the physical cave systems when the world is generated:

- Stone and dark-rock tunnel entrances.
- Walkable tunnel floors, side walls, back chambers, and colliders.
- Brushed-metal ribs, glass panels, glowing crystals, and point lights so the caves keep the ancient/new style.
- Two small moving platforms inside each chamber.
- A `WorldAnchor` named `secret_cave_ch01` through `secret_cave_ch14` at each cave's inner discovery point.

Chapter scripts use `PlaceSecretCave` to spawn a green discovery beacon at the matching anchor. Collecting it stores the cave id in `ChapterProgress.discoverables`, sends a UI message/radio line, and prevents that chapter's cave beacon from respawning after save/load.

## Hidden Rewards

**Files:** `src/components/discoverable.rs`, `src/plugins/discoverable_plugin.rs`, `src/plugins/world_plugin.rs`

Hidden reward rooms use `DiscoverableKind::HiddenReward` to grant save-backed optional rewards. A cache can award credits, XP, armor capacity/refill, a power-up unlock id, and a special-ability upgrade/refill.

The city currently includes three tucked reward rooms near the opening district:

| Room | Reward |
|---|---|
| Aurora Guard Cache | Credits, XP, armor, `aurora_guard_core` |
| Transit Gold Vault | Credits, XP, armor, Homing Star Overdrive |
| Moon Bubble Workshop | Credits, XP, armor, `moon_bubble_capacitor`, Moon Bubble Overcharge |

Companion recruit beacons now also grant rescue supplies to the collecting player the first time that friend is freed: credits, XP, and armor. The amount varies by story role.

Current cave routes:

- Chapter 1: Star Engine Grotto
- Chapter 2: Rift-Glass Underpass
- Chapter 3: Sister Starwell Cave
- Chapter 4: Brother Trial Burrow
- Chapter 5: Mirror Sludge Cavern
- Chapter 6: Crownroot Ice Cave
- Chapter 7: Ember Breathing Hollow
- Chapter 8: Fangroot Scrap Tunnel
- Chapter 9: Pink Flame Root Cave
- Chapter 10: Granite Echo Cave
- Chapter 11: Icebreaker Under-Cave
- Chapter 12: Mana Gear Grotto
- Chapter 13: Crown Gate Underpath
- Chapter 14: Starfall Core Hollow

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

Relic-fragment sub puzzles use `PlaceRelicFragmentPuzzle` for smaller level-side challenges:

- Each fragment puzzle scatters 5 `RelicFragment` beacons around a temporary obstacle course.
- Red moving bars and carry-aware lift platforms make the fragments harder to reach.
- Each collected piece is stored as `scientist:relic_id:piece` in `ChapterProgress.relic_fragments`.
- When all 5 pieces are collected, the game automatically assembles the full relic, unlocks its `relic_id`, clears `CurrentChapter.awaiting_puzzle`, and despawns the temporary course geometry.

Current seeded relic objectives:

- Chapter 1: Giacoma's Star Engine Focus
- Chapter 4: Giacoma's Harmonic Seal
- Chapter 9: Giovanni's Garden Prism
- Chapter 12: Gabrio's Mana Compass

Current seeded relic-fragment objectives:

- Chapter 2: Giovanni's Rift Caliper
- Chapter 5: Gabrio's Mirror Resonator
- Chapter 10: Giovanni's Granite Sextant

HUD support:

- Full relic puzzles show the active objective, archetype-specific progress, timer/hold state, and current node count while a puzzle is active.
- Fragment puzzles currently use radio chatter and pickup messages for shard count feedback.

---

## Perk Tree

**File:** `src/perks.rs` | **Resource:** `PerkTree`

Three branches, 6 perks total. One point per level-up via `PerkTree.award(1)`. Spend points in chapter select with `A/S/D/F/G/H`.

| Branch | Perk | Effect | Max Rank |
|---|---|---|---|
| Heart | Family Vitality | +15 max HP/rank | 5 |
| Heart | Second Wind | +0.5 HP/sec out of combat | 4 |
| Star | Star Focus | +5% beam damage/rank | 5 |
| Star | Pocket Constellation | +15% max charges/rank | 3 |
| Acrobat | Wall-Dancer Evasion | -10% dodge stamina cost | 3 |
| Acrobat | Lucky Parry | +0.05s parry window/rank | 3 |

Current wiring:

- `Family Vitality` increases max HP through the armor/perk max-health sync.
- `Second Wind` passively restores HP while alive; a true out-of-combat timer is still future work.
- `Star Focus` increases primary beam, special tool, and Star Sabre damage.
- `Pocket Constellation` increases primary ammo and special tool charge caps.
- `Wall-Dancer Evasion` lowers dodge stamina cost.
- `Lucky Parry` extends the parry window.

---

## Save / Load

**File:** `src/plugins/save_plugin.rs`

Save file: `starfall_i_save.json` (written next to the binary).

- **Autosave**: every 30 seconds while `Playing`.
- **Manual save**: F5 key.
- **Load**: chapter progress is hydrated on startup; player stats are applied on `OnEnter(Playing)`.

Saved shared fields: `wave_number`, completed chapters, discoverables, recruited companions, recovered scientist relics, recovered relic fragments, unspent perk points, perk ranks, and player-slot character blueprints.

Saved per-player fields live in `players[]` records keyed by `player_index`: level, experience, credits, health, stamina, and armor values. Older top-level stat fields are still accepted for legacy save migration, but new saves use the per-player records as the authoritative source.

## Companions

**Plugin:** `CompanionPlugin` | **Component:** `Companion`

Companions now carry an `owner: u8` matching `PlayerIndex`.

- Default medic drones and pets spawn once per active player when play starts.
- Follow and heal behavior resolves the owning player instead of using a single-player query.
- Combat assist targets enemies near the companion and near the owning player.
- Companion recruit beacons assign the new ally to the player who collected the beacon.

---

## Procedural World

**Module:** `src/lsystem/` | **Plugin:** `WorldPlugin`

- Terrain heightmap via deterministic layered waves/ridges; seed from `GameSettings.world_seed`.
- Outer districts, spaceports, trees, crystals, mountains, and authored anchors sample the terrain surface so upgraded props sit on the generated ground rather than the old flat plane.
- Decorative trees via L-system string rewriting (`lsystem/mod.rs`) + 3-D turtle interpreter (`lsystem/turtle.rs`).
- City-safe terrain is clamped to the invisible gameplay floor, keeping terrain visuals and collision from diverging below Y=0.
- Lush procedural nature adds dense grass, forest pockets, water gardens, reeds, flowers, shrubs, mossy rocks, and darker stone outcrops.
- `NatureSway` gives trees, water surfaces, reeds, flowers, shrubs, and moss caps a subtle hand-animated motion.
- Downtown towers mix transparent glass panels, glowing window facades, metal mullion grids, brushed-metal skins, and occasional stone-brick bodies for the ancient/new skyline style.
- Industrial buildings can receive ribbed metal cladding and factory ribbon windows.
- Smaller residential and outer-district buildings receive stone-brick variants, mortar courses, stone plinths, corner blocks, roof caps, moss, and warm/cool/dark window panels.
- Secret caves are spawned from authored chapter specs and combine stone tunnels, metal ribs, glass panels, glowing crystals, small moving platforms, and save-backed discovery anchors.
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
