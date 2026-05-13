# Starfall I

A Bevy 0.15 action platformer RPG prototype about a family of star-powered heroes defending Earth from dimension-hopping aliens, dragon royalty, and Dr. Bile's mirror humans.

The current build keeps the existing open 3D world, chapter director, RPG stats, loot, crafting, armor, companions, and Bevy/Rapier physics stack, then rethemes the game around cartoon star beams, energy tools, wall jumps, ledge hanging, and Mario-style platforming layered over Secret of Mana-style combat pacing.

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

- Classic action platforming with wall jumps, edge grabs, ledge hanging, and climb-ups.
- Simple retro RPG-style cartoon characters with idle, walk, jump, and hanging poses.
- RPG combat with light/heavy melee combos, parry, dodge, armor elements, loot, crafting, XP, and chapter progression.
- Cartoon star beams and energy weapons instead of guns.
- Open-world level spaces with puzzle gates, encounter waves, and boss fights.
- 4-player local multiplayer is the design target; the current code path is still primarily single-player and ready for a future multiplayer input/entity split.

## Controls

Keyboard and mouse:

| Input | Action |
|---|---|
| `WASD` | Move |
| Mouse | Look |
| `Space` | Jump, wall jump, hold for jetpack |
| Hold toward wall/ledge while falling | Grab and hang |
| `E` while hanging | Climb up |
| `Q` | Dodge or drop from hang |
| `LMB` | Fire active star beam / Star Sabre slash |
| `RMB` | Aim |
| `Shift` | Sprint |
| `V` / `B` | Light / heavy mana combo |
| `F` | Parry |
| `T` | Toggle Star Sabre after unlock |
| `1-6` | Select primary star beam |
| `7` | Homing Star |
| `8` | Tri-Star Burst |
| `9` | Moon Bubble |
| `0` | Sprite Turret |
| `C` | Crafting |
| `F5` | Save |

Controller:

| Input | Action |
|---|---|
| Left stick | Move |
| Right stick | Look |
| South | Jump, wall jump, hold for jetpack |
| East | Dodge / drop |
| West | Recharge active beam |
| North | Parry |
| RT | Fire star beam |
| LT | Aim |
| LB | Sprint |
| RB / D-Pad Right | Next beam |
| D-Pad Left | Previous beam |
| D-Pad Down | Interact / climb |
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

1. Starfall Lab - Giacoma opens the sky.
2. Tony's Shortcut - wall jumps across the rift city.
3. Sisters Of The Star - Gabriella, Nova, Aurora, and Fortuna join.
4. Four Brothers - Angelo and Little Joe complete the team.
5. Dr. Bile - Zark, Crush, Fang, and Sharp emerge.
6. Tibet Peak - Collosar tests the heroes.
7. Tarack's Ember - the dragon queen tests the family.
8. Spikes And Shreds - Spikey and Shread run wild.
9. Pink Flame - garden puzzles and rift blooms.
10. Rockies Domain - Ragar's Colorado mountain domain.
11. Blackskull Ice - Antarctica opens below.
12. Mana Switchworks - open-world puzzle battle.
13. Dimension Front - the crown gate appears.
14. Starfall - the family closes the sky.

## Quick Start

```sh
cargo run
```

For faster incremental builds:

```sh
cargo run --features dynamic
```

## Project Structure

```text
src/
  main.rs                  App bootstrap and plugin registration
  characters.rs            Retro cartoon character construction and color roles
  chapters/mod.rs          Starfall I chapter scripts and biomes
  components/character.rs  Cartoon body parts, roles, and pose animator state
  components/player.rs     Player stats, movement, wall jump, edge grab state
  components/weapon.rs     Star beam and energy tool definitions
  components/enemy.rs      Enemy stats and rift/dragon labels
  components/faction.rs    Story groups and radio colors
  plugins/character_plugin.rs Simple idle/walk/jump/hang animation poses
  plugins/player_plugin.rs Movement, ledge hang, wall jump, stamina, damage
  plugins/weapon_plugin.rs Star beam firing, specials, melee, VFX
  plugins/enemy_plugin.rs  Enemy spawning, AI, rewards, loot
  plugins/ui_plugin.rs     Menus and HUD
```
