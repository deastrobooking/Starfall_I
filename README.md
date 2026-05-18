# Starfall I

A Bevy 0.15 action platformer RPG prototype about a family of star-powered heroes defending Earth from dimension-hopping aliens, dragon royalty, and Dr. Bile's mirror humans.

The current build keeps the existing open 3D world, chapter director, RPG stats, loot, crafting, armor, companions, and Bevy/Rapier physics stack, then rethemes the game around cartoon star beams, energy tools, wall jumps, ledge hanging, and Mario-style platforming layered over Secret of Mana-style combat pacing.

## Current Build

Implemented:

- Player-select flow for 1-4 local players, split-screen cameras, per-player HUD panels, per-player save snapshots, keyboard/gamepad input, and character customization.
- Runtime character-blueprint foundation with serializable body recipes, procedural part/material/socket/rig data, in-game body steppers, and gameplay-linked body stats.
- Open 3D world generation with authored anchors, moving platforms, laser turrets, terrain biomes, foliage, glass/metal/stone-brick city facades, secret cave systems, and dragon-domain spaces.
- Chapter 1 north-coast ocean route with an island behind the mountain range, dock markers, a visible wake lane, and a boardable boat.
- Chapter director with 14 scripted chapters, dialogue, spawn waves, full relic puzzles, five-piece relic-fragment sub puzzles, per-chapter secret-cave discoveries, discoverable beacons, bosses, and unlock progression.
- Castle boss escalation: key dragon/domain bosses escape to airships after their castle defeat, forcing an airship-deck guard fight and rematch.
- Platforming movement: acceleration, sprinting, jump buffering, coyote time, wall slides, wall jumps, ledge hangs, climb-ups, dodges, parries, and jetpack lift.
- RPG combat with six primary star beams, four special energy tools, Star Sabre unlock, melee combos, armor elements, XP, perks, crafting, chests, companions, and save/load.

In progress:

- Local multiplayer is playable at the input/camera/player level, and save snapshots, HUD panels, companions, crafting, chests, and vehicle buffs are now keyed per player. Some reward paths, armor debug cycling, and shared combat feedback still need a full per-player pass.
- Perks are functional and saved, but the chapter-select perk UI is intentionally lightweight and keyboard-only.
- `WaveInfo` remains as legacy compatibility data while the chapter director owns the main progression loop.
- The robot/chassis editor exists as a simple preset and scale screen; deeper part-by-part design is still future design work.

## Cast

Wizard Scientists:
Giacoma, Giovanni, Gabrio

Hero Brothers:
Vincenzo, Antonio aka Tony, Angelo, Joseph aka Little Joe

Hero Sisters:
Gabriella, Nova, Aurora, Fortuna

Dragon Royalty and Domains:
Collosar, King of the Dragons in Tibet; Tarack, his wife; Spikey, their youngest son; Shread, their oldest son; Pink Flame, their daughter; Ragar, uncle to the king in the Colorado Rockies; Blackskull, uncle to the king in Antarctica.

Rivals and Villains:
Space aliens invading Earth from another dimension, Dr. Bile, and the four mirror humans Zark, Crush, Fang, and Sharp.

## Gameplay Direction

- Classic action platforming with tuned acceleration, jump buffering, coyote time, wall jumps, edge grabs, ledge hanging, and climb-ups.
- Simple retro RPG-style cartoon characters with idle, walk, jump, and hanging poses.
- RPG combat with light/heavy melee combos, parry, dodge, armor elements, loot, crafting, XP, perks, and chapter progression.
- Cartoon star beams and energy weapons instead of guns.
- Open-world level spaces with puzzle gates, moving platforms, windup laser turrets, hidden cave systems, five-piece relic fragments inside moving obstacle courses, encounter waves, and boss fights.
- Castle bosses now turn into two-stage set pieces: win the castle fight, chase the boss onto their airship, clear the deck, then defeat them again.
- Flying drones and large dragon bosses add aerial pressure, fireballs, breath attacks, and shockwave hazards.
- 4-player local multiplayer remains the design target; the current implementation has the core player split plus per-player HUD/save/companions/crafting/chests/vehicle buffs, but still needs per-player support in a few reward and feedback systems.

## Quick Start

```sh
cargo run
```

For faster incremental builds:

```sh
cargo run --features dynamic
```

## Game Flow

1. Main Menu
2. Player Select
3. Character Design
4. Chapter Select
5. Chassis Editor
6. Playing
7. Paused
8. Game Over

Chapter select uses `1-9`, `0`, `Q`, `W`, `R`, and `T` for chapters 1-14. Press `E` from chapter select for the chassis editor. Press `Esc` / controller Start during play to pause or resume. The pause menu freezes physics/gameplay, can save, and can save-and-return to the title.

Character design supports outfit/accent/hair swatches, accessory toggles, and body-shape steppers for height, shoulders, chest, arms, legs, hands, feet, head, and mass. Confirming stores an editable character blueprint; body proportions feed the visible character, collider size, movement tuning, stamina, armor capacity, and health.

Perk training is also in chapter select. Leveling up grants one perk point; spend points with:

| Key | Perk |
|---|---|
| `A` | Family Vitality |
| `S` | Second Wind |
| `D` | Star Focus |
| `F` | Pocket Constellation |
| `G` | Wall-Dancer Evasion |
| `H` | Lucky Parry |

## Controls

Keyboard and mouse:

| Input | Action |
|---|---|
| `WASD` | Move |
| Mouse | Look |
| `Space` | Jump, wall jump, hold for jetpack |
| Hold toward wall/ledge while falling | Grab and hang |
| `E` while hanging | Climb up |
| `E` | Interact |
| `Q` | Dodge or drop from hang |
| `LMB` | Fire active star beam / Star Sabre slash |
| `RMB` | Aim |
| `Shift` | Sprint |
| `R` | Reload active star beam |
| `V` / `B` | Light / heavy mana combo |
| `F` | Parry |
| `T` | Toggle Star Sabre after unlock |
| `1-6` | Select primary star beam |
| `7` | Homing Star |
| `8` | Tri-Star Burst |
| `9` | Moon Bubble |
| `0` | Sprite Turret |
| `C` | Crafting |
| `J` | Enter vehicle / board nearby boat |
| `M` | Open map |
| `Esc` | Back / pause |

Pause menu shortcuts:

| Input | Action |
|---|---|
| `Esc` / Start | Resume |
| `S` / `F5` / Select | Save |
| `T` | Save and return to title |
| `F5` | Save |

Controller:

| Input | Action |
|---|---|
| Left stick | Move |
| Right stick | Look |
| South | Jump, wall jump, hold for jetpack |
| East | Dodge / drop |
| West | Reload active star beam |
| North | Parry |
| RT | Fire star beam |
| LT | Aim |
| LB | Sprint |
| RB | Next beam |
| D-Pad Left | Previous beam |
| D-Pad Down | Interact / climb |
| D-Pad Up | Enter vehicle |
| D-Pad Right | Open map |
| Select | Crafting |
| Select + D-Pad Up | Homing Star |
| Select + D-Pad Down | Tri-Star Burst |
| Select + D-Pad Left | Moon Bubble |
| Select + D-Pad Right | Sprite Turret |
| Start | Pause |
| Guide / L3 + R3 | Toggle Star Sabre |
| R3 / L3 | Light / heavy combo |

## Star Beam Loadout

| Slot | Weapon |
|---|---|
| 1 | Starlight Popper |
| 2 | Comet Stream |
| 3 | Sparkle Fan |
| 4 | Nova Orb |
| 5 | Rainbow Ray |
| 6 | Star Bubble Bombs |

Special tools:
Homing Star, Tri-Star Burst, Moon Bubble, and Sprite Turret.

## Chapters

1. Starfall Lab - Giacoma opens the sky; Star Engine Grotto is hidden nearby.
2. Tony's Shortcut - wall jumps across the rift city, Giovanni's scattered rift-caliper fragments, and the Rift-Glass Underpass.
3. Sisters Of The Star - Gabriella, Nova, Aurora, and Fortuna join near the Sister Starwell Cave.
4. Four Brothers - Angelo and Little Joe complete the team around the Brother Trial Burrow.
5. Dr. Bile - Zark, Crush, Fang, and Sharp emerge around Gabrio's mirror-resonator fragments and the Mirror Sludge Cavern.
6. Tibet Peak - Collosar tests the heroes, then flees from Crownroot Ice Cave to the Crown Airship.
7. Tarack's Ember - the dragon queen tests the family around Ember Breathing Hollow and aboard the Ember Airship.
8. Spikes And Shreds - Spikey and Shread run wild before the Fangroot Scrap Tunnel and Shread's Scrapwing rematch.
9. Pink Flame - garden puzzles, rift blooms, and Pink Flame Root Cave.
10. Rockies Domain - Ragar's Colorado mountain domain, Giovanni's granite-sextant fragments, Granite Echo Cave, and Granite Airship.
11. Blackskull Ice - Antarctica opens below with Icebreaker Under-Cave, then the Icebreaker Airship hunts overhead.
12. Mana Switchworks - open-world puzzle battle through Mana Gear Grotto.
13. Dimension Front - the crown gate appears above the Crown Gate Underpath.
14. Starfall - the family closes the sky inside the Starfall Core Hollow.

## Documentation

- [Architecture Overview](docs/architecture.md)
- [Gameplay Systems Reference](docs/systems.md)
- [Improvement Notes](docs/improvements.md)

## Project Structure

```text
src/
  main.rs                         App bootstrap and plugin registration
  state.rs                        AppState flow
  events.rs                       Event definitions and EventsPlugin
  damage.rs                       Health, resistances, and shared damage helpers
  rendering.rs                    Local Bevy render bundles used by world/entity spawners
  resources.rs                    Shared resources and progression state
  character_blueprint.rs          Serializable character recipes, procedural parts, sockets, rig, animation, movement data
  perks.rs                        Heart / Star / Acrobat perk tree
  characters.rs                   Retro cartoon character construction, colors, and presets
  chapters/mod.rs                 Starfall I chapter scripts and biomes
  components/player.rs            Player stats, movement, wall jump, edge grab, input state
  components/weapon.rs            Star beam, special tool, projectile, and Star Sabre definitions
  components/enemy.rs             Enemy stats, flying drones, dragon bosses, projectiles
  components/faction.rs           Story groups and radio colors
  components/discoverable.rs      Discoverable, relic puzzle, relic fragment, and secret cave data
  components/armor.rs             Armor sets and elemental damage reduction
  components/companion.rs         Companion identity and assist behavior data
  components/inventory.rs         Inventory item stacks
  components/mods.rs              Weapon and armor mod definitions
  components/world.rs             Buildings, chests, moving platforms, turrets, anchors, loot
  plugins/input_plugin.rs         Keyboard/gamepad input mapping
  plugins/player_plugin.rs        Movement feel, ledge hang, wall jump, stamina, perks, damage
  plugins/character_plugin.rs     Simple idle/walk/jump/hang animation poses
  plugins/character_design_plugin.rs Color/accessory designer and preview
  plugins/chapter_plugin.rs       Chapter director and encounter progression
  plugins/weapon_plugin.rs        Star beam firing, specials, melee, Star Sabre, VFX
  plugins/enemy_plugin.rs         Enemy spawning, AI, drones, bosses, rewards, loot
  plugins/world_plugin.rs         Terrain, mixed ancient/new city facades, secret caves, props, platforms, turrets
  plugins/discoverable_plugin.rs  Discoverable pickups, secret caves, relic puzzles, and fragment assembly
  plugins/armor_plugin.rs         Armor repair, elemental cycling, and perk max-health sync
  plugins/chest_plugin.rs         Chest spawn, interaction, and loot rolls
  plugins/crafting_plugin.rs      Crafting recipes and crafting panel state
  plugins/companion_plugin.rs     Companion follow, healing, and assist attacks
  plugins/radio_plugin.rs         Radio chatter queue to UI messages
  plugins/vehicle_plugin.rs       Vehicle enter/exit and driving physics
  plugins/ui_plugin.rs            Menus, HUD, crafting panel, chapter/perk UI
  plugins/save_plugin.rs          Save/load and autosave
  plugins/chassis_editor_plugin.rs Robot chassis editor flow
  robots/                         Chassis editor data, robot presets, and factory
  lsystem/                        Procedural tree grammar and turtle interpreter
assets/
  shaders/grass.wgsl              Wind-animated grass shader
```
