# Starfall I

A Bevy 0.18 action platformer RPG prototype about a family of star-powered heroes defending Earth from the dimension-hopping Scallarians, dragon royalty, and Dr. Bile's mirror humans.

The current build keeps the existing open 3D world, chapter director, RPG stats, loot, crafting, armor, companions, and Bevy/Rapier physics stack, then rethemes the game around cartoon star beams, energy tools, wall jumps, ledge hanging, and Mario-style platforming layered over Secret of Mana-style combat pacing.

## Current Build

Implemented:

- Player-select flow for 1-4 local players, split-screen cameras, shared boss-mode camera, per-player HUD panels, per-player save snapshots, keyboard/gamepad input, and character customization.
- Runtime character-blueprint foundation with serializable body recipes, procedural part/material/socket/rig data, taller Dreamcast-anime sci-fantasy default heroes, in-game body steppers, gameplay-linked body stats, stride-aware animation, and visual foot grounding.
- Eight-sibling hero roster foundation: Vincenzo, Antonio, Angelo, Joseph, Gabriella, Nova, Aurora, and Fortuna have distinct default looks, signature weapon/special identities, and shared speed/strength/flight/magic power axes.
- Campaign-shared robot pet foundation: rescued/store-built pet records, robot salvage from defeated enemies, named robot-part materials, save/load persistence, and combination recipes for cars, motorcycles, tanks, boats, submarines, space jets, giant mechs, spaceships, and megaships.
- Chapter-select tech-upgrade foundation: spend robot salvage on beam, missile, Sprite Turret, armor health, rejuvenation, and mech command ranks; upgrades save/load and already affect weapon damage, turret damage, max health, and paid rejuvenation reserve.
- Authored level rewards now introduce robot rescue pods and tech caches across the campaign, feeding robot parts, upgrade-route hints, rejuvenation reserve, and small robot-pet power amplification into chapter progression.
- Open 3D world generation with authored anchors, moving platforms, laser turrets, terrain biomes, foliage, glass/metal/stone-brick city facades, hidden city reward rooms, secret cave systems, and dragon-domain spaces.
- Everest-range world-map foundation: the imported Everest heightmap now spans a 200 x 200 mile `20_000`-unit range, with smoothed height/slope terrain color layers, snowfields, glacier streams, alpine forests, sci-fantasy outposts, carved mountain path corridors, glowing guide studs, dragon-lair silhouettes, visible fast-travel beacons, and clickable chapter-select map markers for all 14 chapters.
- Exploration settlement foundation: eight additional cities, villages, harbors, and outposts now appear physically in the range, use terrain-aware grounded/terraced/sky-district layouts with floating mega-city ramps and mountain-inset gates, show as map markers, expose `WorldAnchor`s for future subquests, and hold saved exploration caches.
- Settlement builder/economy vertical slice: settlement terminals unlock after cache recovery or site liberation, spend shared resources and robot salvage on farms, factories, spaceports, power plants, research labs, defense outposts, and bridge hubs, save/load build tiers, tick bounded passive outputs, and spawn physical rebuilt modules in the world.
- Raid counteroffensive slice: liberated Cloudrail City can enter an `UnderAttack` warning, spawn a visible Scallarian UFO marker plus drone swarm, resolve through player combat or static settlement defenses, and save/load raid state.
- Command strategy first slice: `CommandRegistry` tracks 9 commandable asset kinds (Worker/Scout/FighterDrones, TurretDrone, GroundMech, Boat, FighterJet, Ship, MegaShip) with health/readiness/assignment; assets assigned to a liberated site add to its raid defense score so players can auto-resolve threats with positioned forces; F7 overlay shows asset roster grouped by site; save/load persistent.
- Tech hacking first slice: small Scallarian drones are hackable with interact, grant the saved `blueprint_scallarian_drone_core`, add a Scout Drone command asset, temporarily link as a friendly follower, and pulse nearby hostile enemies from the owner's fire/melee input.
- Great Scientist temple subquests now fill the wider map with optional dungeon-like labs and chapter-select map hints that grant full mechanics upgrades: Ancient Flight Core, Solar Sabre Glyph, Nova Missile Matrix, and Aegis Armor Frame.
- Traversal toy courses with slingshot launch pads, rotating elevators, moving brick jumps, wall-jump shafts, and ramp towers that reward optional exploration.
- Chapter 1 north-coast ocean route with an island behind the mountain range, dock markers, a visible wake lane, and a boardable boat.
- Chapter director with 14 scripted chapters, dialogue, spawn waves, full relic puzzles, five-piece relic-fragment sub puzzles, per-chapter secret-cave discoveries, discoverable beacons, bosses, and unlock progression.
- Castle/dungeon crawl foundation: dragon lair gates now open into single-screen top-down hack-and-slash mode, pulling the party together and switching movement/combat to dungeon-friendly axes.
- Castle boss escalation: key dragon/domain bosses escape to airships after their castle defeat, forcing an airship-deck guard fight and rematch.
- Boss and aerial-threat encounters can link local multiplayer into one full-screen party camera and pull distant players toward the fight before restoring split-screen afterward.
- Platforming movement: acceleration, sprinting, jump buffering, coyote time, wall slides, wall jumps, ledge hangs, climb-ups, dodges, parries, and jetpack lift.
- Player mechanics MVP foundation now includes a single star-tech grappling hook state, `G` / Select+RB input, and a procedural grapple wind-up pose; hook targeting, mountain pull, swing physics, and hook combat are planned in `docs/motion_mechanics_roadmap.md` (milestones MM1–MM10).
- Controller feel now preserves analog movement strength, supports trigger-axis fallback for LT/RT aim/fire, and uses explicit kinematic-controller step/snap tuning for smoother traversal over small terrain lips.
- RPG combat with six primary star beams, four special energy tools, Star Sabre unlock, melee combos, dungeon-wide melee arcs, armor elements, XP, perks, crafting, chests, hidden reward caches, companion rescue rewards, and save/load.

In progress:

- Local multiplayer is playable at the input/camera/player level, and save snapshots, HUD panels, companions, crafting, chests, hidden rewards, enemy loot pickups, camera shake, damage flash, and vehicle buffs are now keyed per player. Chapter scripting uses the party center for encounter placement, while some campaign systems remain intentionally shared.
- Perks are functional and saved, but the chapter-select perk UI is intentionally lightweight and keyboard-only.
- `WaveInfo` remains as legacy compatibility data while the chapter director owns the main progression loop.
- Character design is the single playable-character editor, with GLB-inspired base models, modular silhouette presets, armor layers, and saved per-slot loadouts.

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
The Scallarians invading Earth from another dimension, Dr. Bile, and the four mirror humans Zark, Crush, Fang, and Sharp.

## Gameplay Direction

- Classic action platforming with tuned acceleration, jump buffering, coyote time, wall slides, chained wall jumps, edge grabs, ledge hanging, and climb-ups.
- Simple retro RPG-style cartoon characters with idle, walk, jump, and hanging poses.
- RPG combat with light/heavy melee combos, parry, dodge, armor elements, loot, crafting, XP, perks, and chapter progression.
- Robot pets are the long-term vehicle/mech spine: rescue pets during the campaign or build them from enemy salvage, then combine them into ground, water, air, space, mech, and megaship forms as production systems come online.
- All human heroes share star-powered speed, strength, flight, and magic, but each sibling starts with a different signature weapon/special profile; rescued robot pets amplify those shared power axes by role.
- Cartoon star beams and energy weapons instead of guns.
- Open-world level spaces with puzzle gates, cities, villages, harbors, outposts, moving platforms, rotating elevators, slingshot launch pads, windup laser turrets, hidden city reward rooms, hidden cave systems, Great Scientist temple labs, sprawling dragon lair dungeons, five-piece relic fragments inside moving obstacle courses, encounter waves, and boss fights.
- Castle bosses now turn into two-stage set pieces: win the castle fight, chase the boss onto a turret-guarded airship deck with moving cover, clear the guards, then defeat them again.
- Flying drones and large dragon bosses add aerial pressure, fireballs, breath attacks, shockwave hazards, and shared-screen party battle moments.
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
5. Playing
6. Paused
7. Game Over

Chapter select is now the 200 x 200 mile Everest Range fast-travel map. It uses `1-9`, `0`, `Q`, `W`, `R`, and `T` for chapters 1-14, and unlocked map markers are clickable. Starting a chapter moves the party to that chapter's in-world heightmap beacon. Press `E` from chapter select for the character editor. Press `Esc` / controller Start during play to pause or resume. The pause menu freezes physics/gameplay, can save, can save-and-return to the title, and has a controls/tips page.

Character design supports GLB-inspired base model buttons, visible prefab export/import, outfit/accent/hair swatches, accessory toggles, and body-shape steppers for height, shoulders, chest, arms, legs, hands, feet, head, and mass. Confirming stores an editable character blueprint; body proportions feed the visible character, collider size, movement tuning, stamina, armor capacity, and health.

Perk training is also in chapter select. Leveling up grants one perk point; spend points with:

| Key | Perk |
|---|---|
| `A` | Family Vitality |
| `S` | Second Wind |
| `D` | Star Focus |
| `F` | Pocket Constellation |
| `G` | Wall-Dancer Evasion |
| `H` | Lucky Parry |

Tech upgrades are also in chapter select and spend robot salvage:

| Key | Upgrade |
|---|---|
| `Z` | Beam Capacitors |
| `X` | Nova Missile Forge |
| `C` | Sprite Turret Lattice |
| `V` | Armor Plating |
| `B` | Rejuvenation Matrix |
| `N` | Mech Command Link |

## Controls

Keyboard and mouse:

| Input | Action |
|---|---|
| `WASD` | Move |
| Mouse | Look |
| `Space` | Jump, wall jump, hold for jetpack; trigger slingshots when standing on one |
| Hold toward wall while falling | Wall slide |
| `E` near wall while falling/hanging | Hang or climb up |
| `E` | Interact; trigger nearby slingshots |
| `G` | Grapple hook wind-up foundation |
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
| `J` | Enter vehicle / board nearby boat; dock before disembarking |
| `M` | Open map |
| `Esc` | Back / pause |
| `F9` | Toggle collider debug overlay during play |

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
| Left stick | Analog move |
| Right stick | Look |
| South | Jump, wall jump, hold for jetpack; trigger slingshots |
| East | Dodge / drop |
| West | Reload active star beam |
| North | Parry |
| RT | Fire star beam |
| LT | Aim |
| LB | Sprint |
| RB | Next beam |
| Select + RB | Grapple hook wind-up foundation |
| D-Pad Left | Previous beam |
| D-Pad Down | Interact / hang / climb / trigger slingshots |
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

1. Invasion of the Scallarians - the Starfall Lab opens under attack, and Star Engine Grotto is hidden nearby.
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
13. Scallarian Front - the crown gate appears above the Crown Gate Underpath.
14. Starfall - the family closes the sky inside the Starfall Core Hollow.

## Documentation

- [Architecture Overview](docs/architecture.md)
- [Gameplay Systems Reference](docs/systems.md)
- [Improvement Notes](docs/improvements.md)
- [Motion Mechanics Roadmap](docs/motion_mechanics_roadmap.md)
- [Naming Guide](docs/naming.md)
- [Agent Next Steps](docs/agent_next_steps.md)
- [Engine Upgrade Milestones](docs/engine_upgrade_milestones.md) — campaign/engine milestones `M#`; also defines the `MM#` / `AI#` naming convention

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
  discussion.rs                   Settlement dialogue scripts and MP3 voice hooks
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
  components/world.rs             Buildings, chests, moving platforms, turrets, anchors, NPCs, loot
  plugins/input_plugin.rs         Keyboard/gamepad input mapping
  plugins/player_plugin.rs        Movement feel, ledge hang, wall jump, stamina, perks, damage
  plugins/character_plugin.rs     Simple idle/walk/jump/hang animation poses
  plugins/character_design_plugin.rs Color/accessory designer and preview
  plugins/chapter_plugin.rs       Chapter director and encounter progression
  plugins/weapon_plugin.rs        Star beam firing, specials, melee, Star Sabre, VFX
  plugins/enemy_plugin.rs         Enemy spawning, AI, drones, bosses, rewards, loot
  plugins/world_plugin.rs         Terrain, settlements, dialogue NPCs, guardian ships, caves, props, turrets
  plugins/discoverable_plugin.rs  Discoverable pickups, secret caves, relic puzzles, and fragment assembly
  plugins/armor_plugin.rs         Armor repair, elemental cycling, and perk max-health sync
  plugins/chest_plugin.rs         Chest spawn, interaction, and loot rolls
  plugins/crafting_plugin.rs      Crafting recipes and crafting panel state
  plugins/companion_plugin.rs     Companion follow, healing, and assist attacks
  plugins/radio_plugin.rs         Radio chatter queue to UI messages
  plugins/vehicle_plugin.rs       Vehicle enter/exit and driving physics
  plugins/ui_plugin.rs            Menus, HUD, discussion GUI, crafting panel, chapter/perk UI
  plugins/save_plugin.rs          Save/load and autosave
  robots/                         Robot presets, style data, and factory
  lsystem/                        Procedural tree grammar and turtle interpreter
assets/
  shaders/grass.wgsl              Wind-animated grass shader
```
