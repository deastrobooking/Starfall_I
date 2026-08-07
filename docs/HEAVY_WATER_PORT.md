# Heavy Water port ledger

This is the source-of-truth parity ledger for carrying features from the
sibling `../Heavy_Water` repository into Starfall I. It records gameplay
parity, not TypeScript file parity. Heavy Water's executable source is
authoritative when its documentation disagrees with it.

The ledger is evidence based. A feature is not considered shipped merely
because an input, enum, recipe, visual prototype, or roadmap entry exists.
Update this document when implementation and verification land together.

## Status definitions

| Status | Meaning |
|---|---|
| **Native/superseded** | Starfall already provides the Heavy Water behavior or a stronger Rust-native equivalent. |
| **Ported in this slice** | The feature is present in the current source tree and was completed as part of the active continuation work. |
| **Ported domain/runtime foundation** | The complete deterministic catalog/rules and durable runtime seam are present, but a listed world, presentation, or player-facing UI adapter still remains. |
| **Remaining** | The complete player-facing behavior is absent. Notes may say **planned/in progress**, but that is not a shipped claim. |
| **Separate secure networking program** | This requires an authoritative online service and security design; Heavy Water's prototype must not be copied. |

## Current slice

| Domain | Status | Evidence and boundary |
|---|---|---|
| Per-player first-/third-person switching | **Ported in this slice** | `PlayerView` owns each player's perspective; `input_plugin.rs` maps keyboard and controller intents; `player_plugin.rs` updates owner-specific cameras and avatar render layers. It works independently in split screen and is covered by perspective/render-layer tests. See `src/components/player.rs`, `src/plugins/input_plugin.rs`, and `src/plugins/player_plugin.rs`. |
| Star City road continuity | **Ported in this slice** | The speed-road generator now builds one 56-chord elevated ring at world origin (`radius = 280`, deck `y = 80`) with a 32-unit drivable deck, continuous two-way neon lanes, eight paired boost stations, collidable median/outer rails, terrain-founded supports, and four 24-section world-cardinal approaches from driveable ground mouths to divider-clear outer-lane merges. The ring and ramp corridors participate in prop/building keepout. Floating mega-city foundations independently changed from three rotated approaches to four world-cardinal ramps. |
| Scallarian tanks and Heavy-style encounters | **Ported in this slice** | `EnemyType::Tank` has a procedural tread/hull/turret/barrel craft, siege stats, and a real splash shell at a 28-unit stand-off. Every fourth RouteBlockade unit is a tank; both raid and scripted chapter tank spawns sample their exact local terrain surface. Chapters 4 and 13 contain `DimensionalAlien` tank groups, while dragon factions do not. |
| Differentiated aerial craft and spider artillery | **Ported in this slice** | Wasteland patrols mix Scout, HeavyFighter, and SiegeGunship silhouettes. Scouts fire one bolt, heavy fighters stage a readable three-shot burst 120 ms apart, and gunships fire a simultaneous four-laser spread. Mountain spider mechs hold at 32 units and launch paired homing splash missiles on a 2.8-second cadence. |
| Party vehicle and special-tool continuation | **Ported in this slice** | The shared assembly/mode controller now renders procedural owner-following ATV, siege-tank, giant-mech, space-fighter, and starship bodies. Sprint/LB triggers a bounded shared turbo burst; Tank mode adds armor, aim-driven turret/barrel presentation, and an RT/LMB explosive cannon. Current special levels drive tracking, cluster, multi-missile, duration/fire-rate, and damage upgrades; Sprite Turret now deploys one persistent owner-scoped orbiting combat drone per player. Aiming and firing with the Sabre drawn triggers an owner-scoped, cover-clipped Mega Beam and deterministic twenty-seeker volley. |
| Offline economy, Bio, and legacy-region authority | **Ported domain/runtime foundation** | Rust now carries the 100-slot material economy, all 25 recipes, 19 blocks, 16 prefabs, five vendors, jewels, deterministic rewards/mining, all 133 Bio species, gardens/care/companions/rescues, all 11 legacy regions, atmosphere, map rules, props, and configurable local-arena rules. `HeavyWaterProgress` and the Bio clock persist in schema v5; runtime bridges synchronize local owners, tick gardens/props, seed mining, and expose typed regional travel. The owner-aware Workshop now operates crafting, atomic jewels, the twelve-plot garden, and atomic material-vendor purchases/sales; several physical world adapters remain explicit below. |
| Independent Heavy vehicle simulation and online play | **Remaining** | The new vehicle bodies follow the authoritative player/mode and are not independently colliding, damageable, destructible, or world-mountable vehicles. Ghost Ride/ram-detonation remains absent. Authoritative online play stays a separate security program rather than copying Heavy's trusted-client prototype. |

## Engine, presentation, and authoring

| Heavy Water domain | Status | Starfall parity or remaining work |
|---|---|---|
| Babylon engine lifecycle, `Game.tsx`, animation glue | **Native/superseded** | Bevy plugins, explicit app states, fixed-step gameplay, Avian physics, typed components, render layers, and cleanup schedules replace the browser/Babylon lifecycle. Do not translate singleton classes or animation loops one for one. |
| String `EventBus` and class `StateMachine` | **Native/superseded** | Bevy Messages, components, resources, observers, and ordered schedules are the canonical seams. Add typed, owner-bearing messages for new features. |
| Player locomotion, traversal, camera, and animation | **Native/superseded** | Starfall already has richer grounded/air movement, wall and ledge traversal, grapple/swing/zip/glide/hover tools, hoverboard tricks, character-scale animation, camera collision, split-screen cameras, and shared encounter framing. Heavy-specific input names should map to semantic actions rather than replace these systems. |
| Character editor, humanoid presets, armor-part factories | **Native/superseded** | Character Studio, human mesh generation, saved character designs, outfits, armor, and the shop provide the stronger authoring/runtime path. |
| Robot designer/factory/presets and robot armor parts | **Native/superseded** | Robot Forge, versioned creature specs, named presets, procedural assembly, robot pets, Garage, and published catalogs replace the Babylon factories. Preserve stable preset/spec IDs when importing continuity content. |
| Vehicle designer/factory | **Remaining** | Runtime profiles now build procedural owner-following ATV, tank, giant-mech, fighter, and ship silhouettes, but there is no player-facing vehicle designer or independent published vehicle-body factory. Extend the existing robot/vehicle catalogs rather than adding another editor architecture. |
| HUD, menus, accessibility, keyboard/gamepad input | **Native/superseded** | Starfall's Bevy UI, semantic theme, pause/settings flow, per-player HUDs, stable controller assignment, glyph/layout settings, focus management, and gameplay capture are stronger than Heavy's React overlays and synthetic keyboard/mouse gamepad events. |
| Heavy feature panels and first-run guide | **Remaining** | The existing owner-aware Workshop now has live Crafting, Jewels, Bio Garden, and Market Uplink tabs, including per-player cursors and canonical inventory/wallet transactions. Build preview, full Bio Dex/creature care, companion roster/upgrades, live-map panels, and a durable first-run tutorial flag remain. |
| Sound effects and music deck | **Native/superseded** | Deterministic synthesized SFX with cooldown/pitch handling, custom MP3 overrides, and the validated music deck supersede Heavy's `HTMLAudioElement` pools and track probing. |
| Material-specific prop impacts and ambience | **Remaining** | Add typed surface/prop impact events, material profiles, throttling, and spatial ambience to the existing SFX path. Do not copy Heavy audio assets until licensing is confirmed. |

## World, roads, regions, and environment

| Heavy Water domain | Status | Starfall parity or remaining work |
|---|---|---|
| Procedural sci-fi city and enterable buildings | **Native/superseded** | Starfall generates a larger city and settlements with multi-floor interiors, stairs, ladders, lifts, roof access, dressing, hidden rewards, physical colliders, and traversal networks. Reuse `world_plugin` builders rather than translating `CityGenerator.ts`. |
| Rounded roads, intersections, ramps, rails, supports, loops, and race features | **Native/superseded** | `src/plugins/world_plugin/roads.rs` already provides centripetal Catmull-Rom centerlines, rounded/banked corner fillets, settlement rings, sky-road access ramps, mountain helices, boost ramps, loops, guardrails, and terrain agreement. Heavy's ordinary ground highways are mostly straight and are not a more advanced road model. |
| Heavy's elevated Detroit/Star City ring | **Ported in this slice** | `spawn_speed_road_network` creates the ring once at world origin: 56 uniform closed chords around radius 280, deck surface at `y = 80`, 32-unit width, continuous neon two-way lanes with eight paired boost stations, collidable rails/median/supports, and protected divider-clear outer-lane merges. Four terrain-aware cardinal ramps run outward 280 units, start at driveable terrain height, and use 24 smooth deck sections each. Pure tests cover parameters, boost density, ground mouths, divider clearance, section count, closure/chord continuity, cardinal joins, and keepout distance. Heavy's exact Detroit labels remain legacy-region content. |
| Sky cities, floating platforms, bridges, and mega-city hubs | **Native/superseded** | Starfall already has walkable sky settlements, floating mega-city platforms, rooftop routes, towers, ports, and guardian ships. Floating city foundations now expose four world-cardinal ground ramps instead of three facade-rotated approaches. Heavy-specific layouts beyond that remain content recipes, not engine gaps. |
| Large terrain, mountain traversal, water, caves, dungeons, and settlements | **Native/superseded** | Starfall's 20,000-unit / 200×200-mile authored-scale Everest range, render/collider LOD, mountains/glaciers, waters, caves, dragon lairs, dungeons, castles, and authored settlements supersede Heavy's base 1.2 km world. |
| Heavy legacy regions and side-zone lifecycle | **Ported domain/runtime foundation** | `heavy_regions.rs` defines all 11 stable Heavy regions, exact entry anchors, sourced progression/travel gates, preserved return anchors, persistent Pontiac/Swarm/fortress flags, generation-safe teardown-before-mount plans, and save validation without reinterpreting them as Starfall chapters. The runtime bridge accepts typed travel requests and derives active visibility/director state; physical region geometry/director mounting remains. |
| Michigan heightmap and landmarks | **Remaining** | Add Michigan as a regional terrain patch/tile using the source heightmap only after verifying asset rights. Reuse world sites, settlements, enemy spawners, ships, and terrain code for its bases, labs, towers, wrecks, patrols, and encounters. Do not replace Everest terrain. |
| Heavy desert, junkyard, jungle, mountain villages/temples | **Remaining** | Starfall has richer mountain, forest, water-garden, bio-city, temple, and industrial foundations, but the exact legacy desert/junkyard districts and named set pieces are absent. Implement them as data-backed region content. |
| Day/night and sky | **Native/superseded** | Starfall has a longer sun/moon cycle and sky dome. |
| Weather, regional time/tint, and space presentation | **Ported domain/runtime foundation** | Exact regional time, cycle rate, sky/city tint, fog/weather, and deep-space samples are typed in `heavy_regions.rs`; the runtime bridge publishes the active derived atmosphere. Applying every field to Starfall's sky/fog renderer remains presentation work. |
| L-system foliage and LOD | **Native/superseded** | Starfall has a reusable 3-D turtle, merged geometry, Bevy visibility ranges, fade tiers, and proxies. Extend the existing tree/template catalog for missing birch, willow, shrub, fern, alien coral, and legacy palettes rather than porting Babylon's renderer or manual LOD poller. |
| Damageable environment prop clusters | **Ported domain/runtime foundation** | Six environment props plus four mining-node kinds have stable records, sourced health/radius/drop tables, strict 60/30% damage stages, shared-cache exactly-once claims, deterministic respawn events, validation, and a 220-prop cap. ECS meshes, colliders, hit routing, loot presentation, and authored placement remain. |
| Ground mountain boundary ring and Heavy temples | **Remaining** | The elevated Star City speed ring is shipped. Heavy's separate compact ground/mountain boundary treatment, per-level themes, guardian tables, and named continuity landmarks remain authored content; Starfall's broader mountains and Great Scientist temples already supply the native foundation. |
| Ashur farming and animal clinic | **Ported domain/runtime foundation** | The durable Bio domain has twelve plots, two sourced 22-second growth stages, atomic planting/harvest effects, seed/crop/feed yields, care/bond rules, four lab-animal records, milestones, and a persisted deterministic garden clock. The Workshop exposes per-player planting, growth state, full-bag-safe harvest, inventory counts, and Dex/roster summaries; physical garden/clinic interactions and full care management remain. |

## Building, economy, inventory, and rewards

| Heavy Water domain | Status | Starfall parity or remaining work |
|---|---|---|
| Settlement structures and upgrades | **Native/superseded** | The save-backed settlement economy already has tiered Farm, Factory, Spaceport, Power Plant, Research Lab, Defense Outpost, and Bridge Hub structures, resource costs, outputs, raids, and world-site integration. |
| Garden, Workshop, Robot Foundry, Shipyard functional modules | **Remaining** | Add only missing functions to the existing settlement economy. Do not introduce a duplicate settlement state machine. |
| Freeform block building and mining placed blocks | **Ported domain/runtime foundation** | All 19 blocks, 2 m snap, authored rotation steps/costs/health, owner/distance/slope/support/road/protected-zone/obstruction checks, atomic placement/mining/refunds, base upgrades, and stable placed-build save records are ported. A player preview tool and ECS geometry/collider adapter remain. |
| Runtime prefab placement | **Ported domain/runtime foundation** | All 16 executable-source prefab plans across Defense, Housing, Industry, and Decoration have stable IDs, costs, footprints, placement validation, base functions, and durable build records. Procedural mesh/module instantiation remains. |
| Inventory foundation | **Native/superseded** | Starfall has typed per-player slots, item definitions, rarity, stacks, quick-use selection, full-inventory leftovers, and exact save persistence. |
| Heavy materials and capacity policy | **Ported in this slice** | Canonical inventory now defaults to Heavy's executable-source 100 slots, migrates smaller saves upward, and includes gear, Bio essence, all six weapon-part IDs, seeds/crops/feed, and three jewel tiers. The Heavy catalog is appended without replacing richer Starfall definitions; staged transactions and crafting delivery preserve full-bag leftovers. |
| Timed crafting queue and level gates | **Native/superseded** | Starfall's crafting queue is genuinely timed, owner aware, and enforces level gates, unlike Heavy's current implementation. |
| Complete crafting catalog and outcomes | **Ported domain/runtime foundation** | The crafting panel and timed owner-aware queue now use all 25 Heavy recipes, canonical stack limits, level gates, and schema-v5 queue persistence; a full bag holds completed output for later delivery. Base/robot/drone/city/ship/vehicle result tokens still need dispatch into their owning physical subsystem. |
| Enemy loot pickup/magnet and salvage | **Native/superseded** | Starfall already has owner-aware world loot, tiered enemy drops, homing collection, full-inventory leftovers, credits/resources, and robot salvage. |
| Heavy drop tables, jewels, and companion auto-loot | **Ported domain/runtime foundation** | Deterministic salvage, captain/commander/battleship/vault jewel rolls, chest/vault rewards, weapon parts, and three Power Jewel tiers are ported. Rare world presentation and owner-attributed companion pickup movement remain. |
| Destructible mining nodes | **Ported domain/runtime foundation** | Scrap piles, crystals, Bio pods, and gear caches have exact health, drops, hit radii, respawn timing, deterministic seeded placement, durable state, and runtime cooldown ticking. ECS damage/pickup presentation remains. |
| Weapon jewel sockets | **Ported in this slice** | The per-owner save ledger supports rough/cut/flawless mount, swap, and unmount atomically with rollback-safe refunds. Primary and Homing Star damage read the matching socket exactly once in the canonical damage calculation. The owner-aware Workshop lists all seven sockets, mounted tiers, bag counts, and performs replay-protected canonical mount/unmount transactions. |
| Chests | **Native/superseded** | Starfall already spawns 20 proximity chests with Heavy's reward distribution and nearest-player attribution. |
| Chest completion and presentation | **Ported in this slice** | Ammo now refills the active weapon and grants its canonical ammo item; weapon-upgrade rewards raise the active owner slot up to rank cap with a gear fallback. Chests animate a child lid and timed light before despawning while retaining nearest-player attribution. Stable campaign one-shot IDs remain only where authored chapter persistence requires them. |
| Durable equipment/vehicle shop | **Native/superseded** | Starfall's per-player, save-backed ownership/equip system and Garage gating are stronger than Heavy's generic item purchase flow. |
| Material vendor, selling, stock, and restock | **Ported domain/runtime foundation** | Five sourced vendor instances have stable shared stock/restock records and atomic owner buy/sell transactions that preflight funds, capacity, quantity, wallet range, and replay IDs. The Workshop Market Uplink cycles every vendor and commits stock, canonical inventory, and canonical credits together. Physical shop meshes/proximity terminals remain; the existing durable equipment shop stays separate. |

## Combat, armor, enemies, vehicles, and raids

| Heavy Water domain | Status | Starfall parity or remaining work |
|---|---|---|
| Damage, projectile collision, explosions, aim, and six primary weapon roles | **Native/superseded** | Starfall uses Avian swept collision, nearest-hit resolution, piercing rules, line-of-sight blast cover, aim assist, tracking locks, charge attacks, typed damage, deterministic effects, and owner attribution. These are stronger foundations than Heavy's browser implementation. |
| Exact Heavy primary balance and weapon-part upgrade table | **Remaining** | Import only deliberately selected weapon identities, balance, and upgrade costs through Starfall's data catalogs. Preserve Starfall collision/damage behavior. |
| Beam sabre | **Native/superseded** | Starfall has a six-step chain, wave attacks, six authored techniques, unlock progression, modular blade profiles, equipped visuals, and saved upgrades. |
| Smash/heavy attack | **Native/superseded** | Heavy melee input, charged attacks, sabre Cyclone/Meteor techniques, trick grabs, impact VFX, and the shared damage path already cover and exceed Heavy's standalone smash system. Import only deliberate balance or presentation details. |
| Glaive, twin daggers, plasma axe, and chain whip | **Remaining** | Heavy's four alternate melee classes, combo unlocks, and specials are not full Starfall weapon classes yet. Add them through the move library and weapon forge, not bespoke monolithic systems. |
| Homing missile, energy burst, bomb, and persistent combat drone | **Ported in this slice** | Starfall keeps its four named slots and shared projectile pipeline while preserving Heavy-style level breakpoints. Homing Star gains a three-missile level-3 cast; Tri-Star gains tracking at levels 2/3; level-2 Moon Bubble splits once into four cardinal children; and Sprite Turret deploys one owner-scoped orbiting drone that follows, checks line of sight within 25 units, and fires ordinary attributed player projectiles for 30/45/60 seconds. Level 3 adds faster drone fire and a visual shield aura. Authored hidden rewards currently promote each tool to at least level 2. |
| Elemental active specials | **Remaining** | Implement the selected lightning, ice, fire, wind, psychic, beam/dome behavior and upgrades as data-driven actions in the shared damage/effect pipeline. |
| Mega Beam Cannon and twenty-seeker volley | **Ported in this slice** | With the Star Sabre drawn, owner aim + fire triggers a 1.4-second beam with 220-unit maximum length, radius 5, and 1,800 base damage, plus twenty deterministic homing explosive seekers at 90 base damage and radius 5. A World-layer center ray clips the shared visual and maximum damage endpoint, every contained hostile must separately pass line-of-sight cover, and each can be hit at most once. The per-owner six-second cooldown and one-beam/one-volley cap bound population. Seekers use the canonical attributed projectile/tracking/damage path without per-frame trail-entity spawning. Homing Star's separate level-3 branch remains a three-missile cast. |
| Armor, elemental mitigation, modules, capsules, and equipment presentation | **Native/superseded** | Starfall already has armor/equipment ownership, elements/resistances, modular upgrades, shop integration, character visuals, and save persistence. Port only Heavy-specific armor identities or capsule encounters that serve the sequel. |
| Heavy armor-capsule encounter placement | **Remaining** | The generic armor/equipment system is native, but Heavy's exact world capsule roster, unlock pacing, and presentation are not continuity content in Starfall yet. Add them as authored Discoverables if selected. |
| Enemy factions, ground AI, bosses, and health presentation | **Native/superseded** | Starfall has typed factions, enemy archetypes, boss phases, hit feedback, floating damage, shared encounter cameras, chapter scripting, and richer procedural character/creature visuals. |
| Scout, heavy-fighter, siege-gunship, and city-spy drones | **Ported in this slice** | All three combat craft have distinct procedural geometry, health/defense/flight tuning, typed hurtboxes, and attack reads: single Scout bolt, staged three-shot HeavyFighter burst, and four-way SiegeGunship spread. Authored wasteland patrol anchors spawn a 3/2/1 mixed formation. Existing hacking, city spies, and raid hooks remain native. |
| Additional aerial formations and fortress waves | **Remaining** | The mixed wasteland patrol and differentiated attacks are shipped. More Heavy regional formations, aerial directors, and turret-to-core fortress encounters remain authored work. |
| Scallarian siege tank enemy | **Ported in this slice** | `EnemyType::Tank` uses a dedicated procedural treaded silhouette, 600 base HP, defense, slow chase, a 28-unit ranged stand-off, and a 5.5-radius explosive shell rather than generic melee. RouteBlockade waves choose Tank at indices divisible by four; both raid and chapter spawners place tanks on their exact local terrain surface. DimensionalAlien tank groups occur in Chapters 4 and 13; tests prevent dragon-faction tank encounters. Runtime kills grant ordinary Tread Unit salvage; no rare Hull Plate reward is claimed. |
| Tank/giant-mech assembly and vehicle modes | **Native/superseded** | Robot Garage recipes, saved assembly, vehicle gating, and party ownership remain the native foundation. This slice extends active Tank with a temporary +25 armor-cap/current-armor bonus, knockback suppression, aim-driven turret/barrel presentation, and a 95-damage, radius-7 explosive cannon on a 1.2-second cooldown. GiantMech retains +40 armor. |
| Procedural ATV, tank, fighter, mech, and ship bodies | **Ported in this slice** | `VehiclePlugin` maps the active shared mode to owner-following procedural ATV, Tank, GiantMech, SpaceFighter, or Starship geometry; the existing JetBike remains its own dual-mode body. These bodies present the active assembly and reuse authoritative player movement/aim. They are not independent physical vehicles. |
| Independent vehicle collision, mounting, damage, and destruction | **Remaining** | The new bodies have no separate rigid body, collider, health, world spawn, seats, or destruction lifecycle. Add independent traversal/collision, world mounting, damage/destruction, and vehicle-camera behavior before claiming Heavy's physical vehicle simulation. |
| Turbo and active tank cannon | **Ported in this slice** | Holding the owning player's Sprint input (`Shift` / LB) starts a 0.7-second turbo on ground and air modes when the shared 1.6-second post-burst recharge is ready; boats are excluded. Tank RT/LMB fire uses the canonical aim solution, shell projectile, explosion path, muzzle flash, recoil, and owner attribution. |
| Ghost Ride, ramming, and general mounted-weapon amplification | **Remaining** | Boosted ejection, an unmanned collision/detonation phase, vehicle ramming damage, and amplification of ordinary mounted weapons are absent. Tank cannon is a dedicated replacement weapon, not proof of those systems. |
| Mountain spider mechs and heavy enemy foundation | **Ported in this slice** | Starfall's native giant eight-leg MountainSpiderMechs retain terrain-following patrol and typed hitboxes. They now stop at a 32-unit attack envelope and launch paired target-bearing homing splash missiles every 2.8 seconds through the shared enemy-projectile pipeline. Exact Saginaw spider-tank region content remains a recipe. |
| Invasions and raid state | **Native/superseded** | Save-backed world-site states, warning/active/resolved raids, settlement defense, hacking, routes, and final-war state supersede Heavy's basic invasion state model. This slice extends active RouteBlockade raids with periodic terrain-grounded siege tanks in the existing soldier wave without changing other raid-kind mappings. |
| Recurring multi-site invasion scheduler and Heavy enemy fortresses | **Remaining** | If desired, extend `RaidRegistry` with post-campaign cadence and multiple eligible sites. Add the mirror-sabre captain and turret-to-core fortress as authored encounters, not a second invasion system. |
| Tiered explosion presentation pool | **Remaining** | Damage/explosion mechanics exist, but Heavy-style size-tiered pooled presentation can be added within Starfall's VFX budget and action-SFX system. |

## Companions, bio creatures, NPCs, and rescue

| Heavy Water domain | Status | Starfall parity or remaining work |
|---|---|---|
| Companion follow, attack, healing, and recruitment foundation | **Native/superseded** | Starfall companions are owner aware and already follow, attack, heal, and recruit in local co-op. Robot pets separately provide campaign-shared rescue, roles, affection, level, salvage, and assembly. |
| Personal companion roster, cap, upgrades, death/revive, and save hydration | **Ported domain/runtime foundation** | Each stable local player key owns a durable capped roster, three active slots, nine sourced robot presets, body/weapon upgrades with atomic inventory effects, downed/death state, and upgrade-preserving revival. Spawning those records as assembled ECS companions and replacing unconditional visual defaults remains. |
| Bio species catalog | **Ported in this slice** | All 133 executable-source species mechanically match Heavy across 24 archetypes, 11 elements, palettes, rarity, base stats, spawn weights, descriptions, and stable IDs; catalog parity is tested. |
| Wild spawning, capture, Dex, care, bond, and passive bonuses | **Ported domain/runtime foundation** | Deterministic capture now incorporates actual weakened health, rarity, garden bonus, capacity, and Bio Essence consumption; per-player Dex/roster/active slots, care, levels, bond, and capped passives persist. Wild ECS spawning, encounter visuals, and owner-aware panels remain. |
| Bio garden and farming | **Ported domain/runtime foundation** | Garden tier/cap/bonus, all twelve plot records, deterministic growth/harvest, seed returns, crop/feed effects, runtime ticking, and the save-backed clock are ported with atomic full-inventory rollback. World plots and UI remain. |
| Friendly NPC dialogue | **Native/superseded** | Starfall's discussion NPC/panel and radio/subtitle systems are stronger. Port Heavy's named cast and lines as authored dialogue records where continuity calls for them. |
| Synthetic and animal rescues | **Ported domain/runtime foundation** | All 15 synthetic rescues and four lab animals have exact stable definitions, idempotent completion/milestone rules, persistent IDs, and the one-time Mini General Voidcrown reward with safe roster-cap handling. Cage interaction, story, and shatter presentation remain. |
| Lab and garden world interactions | **Remaining** | Research Lab settlement structures exist, but Heavy's companion-cap Lab UI, animal cages, garden capture management, and functional building hooks need gameplay components and saved state. |

## Map, progression, persistence, and online scope

| Heavy Water domain | Status | Starfall parity or remaining work |
|---|---|---|
| Campaign chapters, world sites, routes, fast travel, settlements, raids, and final war | **Native/superseded** | Starfall's fourteen chapters and shared campaign registries are richer. Heavy levels should become optional legacy regions/content, not replacement chapter IDs. |
| Player, weapon, tech, perk, relic, and travel progression | **Native/superseded** | Starfall has save-backed per-player progression plus campaign-shared tech/site progression and player-aware chapter travel. Heavy-specific costs and unlock identities should enter existing ledgers/catalogs rather than a parallel Upgrade Bay state. |
| Live minimap/full gameplay map | **Ported domain/runtime foundation** | A typed deterministic marker registry covers players, enemies, shops, gardens, chests, bases, and bosses with explicit 24-player visibility, stable priority/ID ordering, projection, distance falloff, and off-screen arrows. Feeding it into each split-screen HUD/map panel remains. |
| Local saves, autosave, recovery, and migration | **Native/superseded** | Starfall's versioned v5 schema, atomic rotating generations, corrupt-save fallback, legacy defaults, 30-second autosave, per-player ordering, and shared registries are substantially safer than Heavy's optional-field JSON snapshot. |
| New feature persistence | **Ported in this slice** | Schema v5 persists the timed crafting queue, economy owners/vendors/builds/mining/chests/jewel sockets, Bio Dex/gardens/companions/rescues plus deterministic clock, and legacy region/prop records through one validated `HeavyWaterProgress` seam. Legacy saves default safely; no ECS `Entity` IDs are serialized. |
| Cloud account, cross-device save, and leaderboards | **Separate secure networking program** | Heavy's web auth/Postgres JSON flow is not a safe drop-in backend. Define authenticated identity, authorization, schema validation, migrations, conflict policy, privacy, rate limits, observability, and recovery before adding cloud writes. |
| Online co-op rooms, transforms, chat, and enemy damage | **Separate secure networking program** | Heavy trusts client identity, positions, actions, and damage and mostly relays them. Starfall needs an authoritative Rust simulation/service, stable network IDs, validated inputs, snapshots, reconciliation/interpolation, abuse controls, secure sessions, and persistence boundaries. |
| Local versus arena and match rules | **Ported domain/runtime foundation** | Heavy's exact 640×640 arena and 24 spawn-ring facts are preserved, while configurable offline FFA/team rules provide validated participants, timer, score, eliminations, deterministic winner/draw state, and tests. Combat/respawn/HUD wiring and physical arena construction remain; no unsourced score/time constants were invented. |
| Online versus | **Separate secure networking program** | Add only after authoritative networking exists. Never port Heavy's client-decided hit/damage relay. |

## Heavy source versus documentation

The following discrepancies are confirmed in executable Heavy Water source and
must not be “corrected” toward stale prose during the port:

- `InventorySystem.ts` defaults to 100 slots, not 24.
- `PrefabSystem.ts` is a fixed registry of 16 recipes. `BaseSystem` persists
  only maximum levels by structure kind. Placed blocks and prefabs are absent
  from the normal progress snapshot and require manual `LevelSerializer`
  export/import.
- `CraftingSystem.ts` contains 25 recipes, but `requiredLevel` is ignored,
  results are granted immediately, and queued work is never completed/removed
  by a real queue processor. Several undefined result items become generic
  materials.
- The bio catalog contains 133 species. Capture probability currently ignores
  target health even though guide text tells players to weaken creatures.
- Sanctuary farming creates 12 plots with a roughly 44-second three-stage
  cycle, not five plots and 90 seconds.
- Rescue source contains 15 synthetic rescues across levels 1, 2, 3, 5, and
  11, plus four lab animals; older documentation says 12.
- Heavy's general highways are straight. Its signature “rounded road” is
  primarily the 56-segment elevated sky ring with four ramps.
- `LevelSystem.ts` has a stale comment suggesting values above level 4 are
  clamped, while the implementation supports all 11. `spacelike` and `lair`
  are hard-coded helpers rather than fields in the source level interface.
- Ann Arbor's current layout has 18 peripheral buildings plus five crushed
  towers, not the approximate count in older comments.
- `VehicleSystem.ts` implements only ATV and space-fighter kinds despite broad
  vehicle/tank language elsewhere. Its tank-shell presentation applies damage
  immediately while the shell visual interpolates afterward.
- Heavy weapons refill to maximum, making ammo effectively unlimited.
- Current Beam Sabre values exceed older developer-guide tables.
  `ElementalSpecialsSystem` collapses former large tracking volleys into a
  single aggregate long-range beam while older prose still describes tracking
  casts.
- `swarm_drone.glb` in `Game.tsx` is an ambient bob/spin model, not the
  persistent combat drone from `SpecialWeaponsSystem`.
- WebSocket room codes are six characters. Actual caps are 16 co-op and 24
  versus. The arena is 640 by 640, its layout is not room-code seeded, and it
  has no complete score/team/timer/win loop.

## Architectural port rules

1. **Port behavior and content, not TypeScript class structure.** Use Bevy
   components, resources, typed Messages, plugins, explicit schedules, Avian
   collision, and Starfall's fixed-step loop.
2. **Extend native systems before adding parallel ones.** Roads go through the
   road kit, creatures through versioned specs, buildings through published
   procedural catalogs, combat through the shared damage/move libraries,
   settlements through existing registries, and UI through Bevy panels.
3. **Make ownership explicit.** Inventory, sockets, captured creatures,
   companion rosters, UI cursors, and personal rewards are keyed by
   `PlayerIndex`. Settlements, world sites, raids, routes, and current robot-pet
   collection remain campaign shared unless an approved design says otherwise.
4. **Keep editor and gameplay persistence separate.** Forge/world-kit records
   author assets. Runtime player builds reference stable published IDs and own
   transforms/state; they are not mutable editor projects.
5. **Use stable IDs and migrations.** Save domain records, never ECS entities.
   Add serde defaults, version migrations, deterministic ordering, round-trip
   tests, and corruption-recovery coverage with every schema expansion.
6. **Use deterministic seams.** Random placement, drops, captures, mining,
   recipes, and encounter selection use `GameRng` or authored seeds. Avoid
   browser time and unordered ECS iteration as authoritative outcomes.
7. **Keep transactions atomic.** Validate cost, capacity, ownership, collision,
   and unlock state before consuming inventory or replacing equipment.
8. **Do not regress stronger Starfall behavior.** Preserve swept collision,
   line-of-sight blast cover, split-screen ownership, rounded road terrain
   agreement, LOD, accessibility, save recovery, and physical colliders.
9. **Treat online play as a separate security program.** No trusted-client
   identity, positions, hits, damage, inventory, chat, or save writes.
10. **Verify assets and licenses.** Do not copy Heavy music, textures, models,
    or fonts into distribution until provenance and reuse rights are known.

## Recommended dependency order

1. Stable catalogs, owner-bearing events, save-schema records/migrations, and
   an ownership ADR.
2. Finish chest rewards; add mining, expanded loot, jewels, and socket UI.
3. Add player block/prefab placement and connect functional structures to the
   existing settlement economy.
4. Add caged rescues and the saved personal companion roster/upgrades.
5. Add bio species, capture/Dex, garden farming/care, and active pets.
6. Add the legacy-region lifecycle and atmosphere, then named Heavy regions
   using existing terrain/city/dungeon/enemy builders. Reuse the shipped Star
   City elevated ring instead of constructing a second central road landmark.
7. Turn the current owner-following vehicle bodies into independently spawned,
   colliding, mountable, damageable vehicles; then add destruction, Ghost Ride,
   and ramming. Keep the shipped turbo/tank cannon distinct from those gaps.
8. Add minimap, material vendors, prop/audio polish, tutorial, and local versus.
9. Begin cloud or online work only under the separate authoritative networking
   and security program.
