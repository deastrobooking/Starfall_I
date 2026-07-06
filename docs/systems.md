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

**Character assignment:** each joined slot can cycle through Vincenzo, Antonio,
Angelo, Joseph, Gabriella, Nova, Aurora, and Fortuna. The default join order
still starts on the brothers for P1-P4, but the selectable roster is now all
eight siblings.

**Architecture:** Each player entity carries a `PlayerInput` component written by `InputPlugin` each `PreUpdate`. Each player's camera entity is stored in a `PlayerCameraRef(Entity)` component so weapon, movement, camera shake, damage flash, and interaction systems can resolve the correct player.

**Controller feel:** gamepad movement uses circular deadzone remapping and preserves analog stick magnitude in `PlayerMovement`, so partial tilt produces partial travel speed. Right-stick look uses a quadratic curve for low-deflection precision. LT/RT aim/fire supports both Bevy digital trigger buttons and `LeftZ` / `RightZ` analog trigger axes, with frame-local just-press tracking for single-shot weapons.

**Game over:** triggers only when ALL players are dead simultaneously.

**Pause:** `Esc` / controller Start toggles between `Playing` and `Paused`. The pause menu keeps the current HUD/world entities alive, pauses the Avian `Time<Physics>` clock, offers party-wide save and save-and-title actions, and includes a controls/tips page. Returning to title from pause cleans up preserved play-session entities.

**Shared boss camera:** when 2-4 players are active, `PlayerPlugin` switches from split-screen to one full-screen party camera during boss-tier enemies or nearby flying-drone wings. P1's camera becomes the shared view, the other cameras are temporarily disabled, and viewports are restored when the threat clears. Distant players are softly pulled toward the encounter anchor, with a hard catch-up if they are far outside the battle space.

**Ownership policy:** `PlayerIndex` is the stable owner key. Campaign progress, chapter objectives, kill gates, boss phases, unlocks, and `PerkTree` are shared. Inventories, rewards, HUD panels, camera/damage feedback, companions, crafting ownership, runtime stats, character blueprints, and save `players[]` records are per-player. Vehicles remain party-shared for now: one driver/vehicle mode, with passengers keyed by `PlayerIndex`.

**Known limitations:**
- Chapter director spawns use the party center as the encounter anchor; bespoke chapter beats are still campaign-shared.
- HUD stat/weapon panels, save snapshots, companions, crafting, chests, hidden rewards, enemy loot pickups, damage feedback, and vehicle buffs are keyed by `PlayerIndex`.
- Vehicle buffs apply to the activating player, but the party still shares one active vehicle mode at a time. In Chapter 1, a boardable boat at the north dock locks one driver, boards nearby players into passenger seats, steers along the visible ocean wake route, and requires docking at the city or island before disembarking.
- Dev-only armor element cycling applies to P1 from keyboard shortcuts; final equipment UI should expose per-player selection.

---

## Player Guidance HUD

**Resource:** `PlayerGuidance` | **File:** `src/plugins/ui_plugin.rs`

The HUD now has a small bottom-right contextual guidance panel. It is intentionally separate from the larger objective and message systems: objectives explain campaign intent, messages report one-shot events, and guidance tells the nearest active player what action is available right now.

Current guidance sources:

| Source | Prompt |
|---|---|
| `DiscussionNpc` | `E` / D-pad Down to talk |
| `DungeonCrawlGate` | `E` / D-pad Down to open the gate and gather the party |
| `BoatVehicle` | `J` / D-pad Up to board |
| `Discoverable` / `SpyData` | walk into the beacon/core to collect or recover |
| `WorldLoot` | walk over the pickup |
| live `CitySpyDrone` | aim and fire to shoot down the spy, then recover its data |

The panel hides while a discussion is active so the dialogue UI can own the bottom-center guidance.

## Player Movement

**Plugin:** `PlayerPlugin` | **Component:** `PlayerMovement`, `EdgeGrabState`, `ClimbState`, `JetpackState`

| State | Trigger | Notes |
|---|---|---|
| Idle | No input, grounded | Default ground state |
| Moving | WASD / left stick | Smoothed acceleration toward `walk_speed = 0.38` |
| Sprinting | Shift / LB + move | Drains stamina 15/sec |
| Jetpack | Hold Space / South while airborne | Burns `fuel_cost_per_sec = 20`/sec from an 800-unit tank (~40 s thrust, sized for the Everest ranges); regens 45/sec on ground |
| WallSliding | Pushing into wall while falling | One-hand wall clasp; drains light stamina and caps fall speed at `wall_slide_speed = 0.28` |
| Hanging | Interact while falling into a wall | Max hang time 2.5s; drains stamina 12/sec |
| Climbing | Push firmly forward into a vertical surface | Free-climb mountains/buildings on its own HUD energy bar (`CL`, 420 units, drains 26/sec, regens 55/sec grounded). Stick climbs/descends/shimmies; buffered jump mid-climb performs a full wall jump (charges preserved); topping out vaults up-and-over; camera auto-pulls back + widens FOV while climbing |
| Grappling | `G` / Select+RB | First MVP slice: hook wind-up pose and cooldown only; targeting and pull physics come next |

Jumping uses a short input buffer, coyote timer, early-release jump cut, and a short apex float so near-edge jumps, taps, and high-arc jumps feel more responsive. Falling uses a stronger gravity multiplier and a capped terminal velocity. The local `KinematicCharacterController` compatibility component feeds Avian move-and-slide and uses explicit movement-profile fields for offset, step height/width, and snap-to-ground distance so small lips and authored traversal props are easier to tune consistently.

Analog movement preserves stick strength. Sprint requires the sprint input plus near-full stick deflection (`analog_sprint_threshold`) so light controller movement does not unexpectedly drain stamina or snap into sprint.

Wall slide is the default wall-contact behavior while falling; it now behaves as a stamina-backed one-hand wall clasp, drains a small amount of stamina, refreshes wall-jump charges, slows descent, and falls back to a faster tired slide when stamina is gone. Hanging is intentional through `E` / D-pad Down. Wall jump is triggered from buffered jump input while `wall_contact_timer > 0` and airborne. It pushes away from wall normal + 25% input direction, has a short steering lockout for cleaner arcs, and carries two wall-jump charges that refresh on landing or renewed wall slide contact.

Procedural character poses now distinguish idle, walk, run, jump, fall, flight,
one-hand wall slide, hang, combat, Star Sabre slash, and grapple wind-up. The
long-form roadmap for turning this into a full humanoid traversal/combat system
lives in `docs/playerengine.md`.

Grapple hook foundation: `GrappleHookState` is a single-hook state component
with Ready/Windup/Searching/Swinging/Zipping/Recovering/Cooldown modes, cable
tuning, zip/mountain/attack pull tuning, and attach-point fields for future
raycast work. The current milestone only plays the wind-up/cooldown animation
path; M2 adds target raycasts and M3-M4 add zip/swing physics.

Climb-up: `E` / D-pad Down while hanging. Boosts player upward `climb_boost * dt * 60`.

Free climbing (2026-07): pressing the stick firmly into a wall (`input · -wall_normal > 0.35`)
starts a climb when the `CL` bar has ≥30 energy. While climbing the motor moves
in the wall plane (stick Y up/down, stick X shimmy) with a light inward pull to
hold contact. Exits: buffered jump → standard wall jump (uses/keeps the same
charge economy, so the double-jump-off-buildings verb is untouched); losing the
wall → ledge vault (`vault_boost` up + over); dodge, stick-down on ground, or an
empty bar → release. Grapple swing/zip suppresses climb starts.

Speed roads (2026-07): mountain-route decks are built from a grade-limited
longitudinal profile (`SPEED_ROAD_MAX_GRADE = 0.22`, `grade_limit_profile`) —
sampled at chunk boundaries and relaxed so routes crest ridges on sustained
viaduct approaches instead of spiking up every bump; shared boundary heights
keep chunk seams flush. Non-route decks (rings/spurs) keep classic lift; all
profiled decks absorb ≤2.5 units of intra-chunk bumps. Buildings are reactive to
the network: if the road centerline crosses a footprint, `spawn_building` emits
a **tunnel gateway** (two flank towers + header deck above a 10-unit clearance,
30-unit passage, oriented via the road distance-field gradient) or skips the
building entirely when too small to straddle.

Dodge: invulnerable during the `dodge_duration = 0.3s` window; costs 20 stamina; 0.5s cooldown.

Parry: 0.2s window on press; absorbs the next hit; 1.0s cooldown.

Stamina regens at 10/sec while not dodging.

## Character Authoring

**Files:** `src/character_blueprint.rs`, `src/characters.rs`, `src/plugins/character_design_plugin.rs`, `src/plugins/player_plugin.rs`

The character designer now stores confirmed edits as `CharacterBlueprint` data rather than only transient UI overrides. The current runtime style target is a taller Dreamcast-anime sci-fantasy hero silhouette: smoother low-poly capsule limbs, expressive eyes/hair, layered armor plates, glow accents, and less tiny voxel/block mass. A blueprint includes:

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

The Hero Atelier editor shows the active GLB-inspired base model, marks when that model has custom edits, and exposes prefab export/import buttons for `assets/Characters/design_prefabs/*.json`. Base model selection remains visible while the player tweaks proportions or swaps individual body, arm, leg, shoulder, and helmet presets.

`CartoonCharacter` carries stride/agility metadata derived from `BodyRecipe`, and `CharacterPlugin` uses it to tune walk/run phase speed and limb swing. The character renderer also applies a visual ground lift so oversized cartoon feet sit on the terrain instead of dipping below the player capsule.

The full procedural mesh/rig/timeline editor is still future work; the saved schema is intentionally larger than the current renderer so it can grow without replacing save data.

---

## Hero Combat Identity

**Files:** `src/hero_roster.rs`, `src/plugins/player_plugin.rs`, `src/components/weapon.rs`

All eight human siblings share the same core power categories: speed, strength,
flight, and magic. Each hero has a distinct signature weapon/special profile
that currently drives starting active weapon slot, special name/tuning, movement
and stamina multipliers, jetpack/flight tuning, melee strength, primary weapon
damage, and special damage.

| Hero | Signature weapon | Signature special | Bias |
|---|---|---|---|
| Vincenzo | Guardian Comet Lance | Family Aegis Moon | strength / guard magic |
| Antonio | Rift Skater Star Fans | Shortcut Homing Star | speed / flight |
| Angelo | Harmonic Star Hammer | Bridge Sprite Turret | magic / support strength |
| Joseph | Little Joe Nova Gauntlets | Pocket Tri-Star Burst | strength / burst |
| Gabriella | Prism Crown Staff | Prism Moon Bloom | magic / flight |
| Nova | Nova Rainbow Ray | Spectrum Tri-Star | magic / speed |
| Aurora | Aurora Halo Guard | Halo Sprite Bastion | strength / magic guard |
| Fortuna | Fortune Star Cards | Lucky Homing Draw | speed / magic |

Robot pets amplify these shared power categories at spawn. Scout pets favor
speed, healer pets favor magic, defender pets favor strength, builder pets split
strength/magic, and pilot pets favor flight. Active combined forms add a larger
future-facing bonus, so the garage/mech/ship runtime can grow without replacing
the hero identity data.

---

## Robot Pets And Combined Forms

**File:** `src/robot_pets.rs` | **Runtime hooks:** `src/plugins/enemy_plugin.rs`, `src/plugins/save_plugin.rs`

Robot pets are a campaign-shared collection that can eventually bridge companions, store crafting, vehicle modes, giant mechs, spaceships, and megaships. The current foundation stores:

- `RobotPetBlueprint`: rescued or store-built robot pets with role, level, affection, stat bias, and supported forms.
- `RobotPartKind`: stable robot salvage/material kinds with inventory item IDs for future stores and crafting UI.
- `RobotPetCollection`: saved party-wide pets, parts, and the active combined form.

Defeated enemies now grant robot salvage directly to the shared collection. Generic bad guys map to starter parts, while high-value kills grant larger quantities:

| Enemy type | Robot salvage |
|---|---|
| Drone | Circuit Board |
| Soldier / unknown | Scrap Frame |
| Heavy | Tread Unit |
| SpikeAlien | Servo Motor |
| Hybrid | Energy Core |

The collection supports store-built pets through `store_pet_recipe()` and campaign rescues through `rescue_pet()`. Chapter scripts now place save-backed robot rescue pods with `DiscoverableKind::RobotPetRescue`; current authored rescues include Spark Pup, Bolt Mason, Rivet Guard, and Tide Mender. Combined-form recipes are milestone gates, not final balance:

| Form | Pet count | Role |
|---|---:|---|
| Car / Motorcycle / Boat | 1 | Early travel forms |
| Tank / Submarine / Space Jet | 2 | Specialized terrain and combat forms |
| Giant Mech | 4 | Party-scale combat build |
| Space Ship | 6 | Late campaign travel/combat shell |
| Megaship | 10 | Endgame fleet-scale build |

Save/load persists the whole `robot_pets` section with `serde(default)`, so older saves without robot data continue to load as an empty collection.

The Robot Garage screen (`AppState::RobotGarage`, `src/plugins/robot_garage_plugin.rs`) is the current player-facing hub. A/D browsing selects one of the 9 `RobotAssemblyForm` variants; pressing Enter auto-selects the first N eligible pets and calls `robot_pets.assemble(form, &pet_ids)`; X disassembles. `MechCommandLink` upgrade rank gates GiantMech, SpaceShip, and MegaShip. Assembled forms drive `VehicleState` at runtime (see Vehicle System below).

Future runtime work: controller-friendly garage/store UI, runtime 3-D mech/ship controllers, per-player passenger/driver UX, and store-built pet purchasing from salvage.

---

## Settlement Builder And Economy

**Files:** `src/settlement_economy.rs`, `src/plugins/world_plugin.rs`, `src/plugins/save_plugin.rs`

`SettlementEconomy` is the current vertical slice for the Civilization-style
reclamation layer. It is campaign-shared and save-backed, with a shared
stockpile, per-settlement build records, deterministic output ticks, and
robot-part construction costs.

Current build types:

| Build | Role |
|---|---|
| Farm | food, workers, recovery support |
| Factory | alloy, circuits, robot-part industry |
| Spaceport | fuel, hangar capacity, future rapid response |
| Power Plant | power, shield/bridge/factory support |
| Research Lab | research, blueprint and hacking future hooks |
| Defense Outpost | defense capacity and turret future hooks |
| Bridge Hub | route repair, mountain pass, and sky-ramp future hooks |

Every authored `map_settlements()` location spawns a `SettlementBuildTerminal`.
Press `E` / D-pad Down near the terminal after collecting that settlement's
cache or liberating the matching world site. The terminal chooses the next
recommended build order for that settlement kind, spends shared resources and
robot parts, spawns a visible world module, and shows a message when the build
is blocked by missing resources or parts.

Saved builds respawn as physical props when the world is generated. The current
gameplay effect is bounded passive output into the shared stockpile; route
unlocks, assigned defense units, raid pressure, local missions, and map-panel
management are M7-M9 work.

Tests currently cover successful multi-build setup, deterministic output ticks,
missing-part errors, and save round-tripping of settlement economy state.

---

## World Sites / Reclaimable World State

**Files:** `src/resources.rs`, `src/components/world.rs`, `src/plugins/world_plugin.rs`, `src/plugins/save_plugin.rs`, `src/plugins/ui_plugin.rs`

**Resource:** `WorldSiteRegistry` (holds `Vec<WorldSite>`)

World sites are named strategic positions in the 200-mile Everest Range that players can liberate, defend, and rebuild. Each site has a stable `WorldSiteId(u16)`, world position, kind, current `WorldSiteState`, owner faction, and liberation progress tracking.

### Site State Enum

| State | Map badge color | Meaning |
|---|---|---|
| `EnemyHeld` | Red | Active enemy presence; defenders not yet defeated |
| `Contested` | Orange | Liberation in progress |
| `Liberated` | Green | All defenders defeated; command terminal online |
| `Building` | Blue | Active construction at the site |
| `Damaged` | Yellow | Previously liberated; took raid damage |
| `UnderAttack` | Dark orange | Active raid in progress |
| `Shielded` | Cyan | Defense shield active |
| `OccupiedAgain` | Dark red | Re-captured by enemy after liberation |
| `Hidden` | (no badge) | Site not yet revealed |

### Site Kinds

`WorldSiteKind`: City, Village, Farm, Factory, Spaceport, PowerPlant, ResearchLab, DefenseOutpost, Harbor, Temple, Castle, Mine, BridgeHub, MountainGate.

### Liberation Flow

1. Player approaches an enemy-held site. The invisible `WorldSiteEnemySentinel` entity (spawned at the site center during `spawn_world_site_props`) detects the player inside `trigger_radius` (default 100 units).
2. `world_site_enemy_spawner_system` spawns `enemy_count` Soldiers around the site and marks each with `WorldSiteMarker { id }`. Sets `sentinel.spawned = true`.
3. `site_liberation_system` polls for `sentinel.spawned && !sentinel.liberated_spawned` and checks that no enemies with the matching `WorldSiteMarker` remain alive.
4. When all defenders are dead: `WorldSiteRegistry` flips the site to `Liberated`, a `SiteCommandTerminal` prop (green glowing terminal + blue NPC) is spawned, and a UI message confirms liberation.
5. Save/load preserves `WorldSiteSaveRecord` (id, state, owner, enemies_defeated) so the liberated state and command terminal survive restarts.

### Current World Sites

| Id | Name | Region | Kind | Initial State | Defenders |
|---|---|---|---|---|---|
| 1 | Iron Watchpost | Starfall Zone | DefenseOutpost | EnemyHeld | 5 |
| 2 | Riftglass Village | Rift Foothills | Village | EnemyHeld | 8 |
| 3 | Starfell Outpost | Crown Road | DefenseOutpost | EnemyHeld | 6 |
| 4 | Cloudrail City | High Sky Rail | City | Liberated (FreePeoples) | 0 |
| 5 | Lantern Hamlet | Tibet Snow Road | Village | EnemyHeld | 8 |
| 6 | Star Orchard | Fangroot Meadow | Village | EnemyHeld | 6 |
| 7 | Frost Harbor | Antarctic Range | Harbor | EnemyHeld | 10 |
| 8 | Granite Market | Rockies Gate | Village | EnemyHeld | 7 |
| 9 | Switchwork Borough | Mana Switchworks | City | Liberated (FreePeoples) | 0 |

Sites 2–9 correspond to `map_settlements()` entries via `MapSettlement.site_id`. The chapter-select map renders the world-site badge as the canonical marker for linked settlements; the generic `C`/`V`/`H`/`O` marker is suppressed.

### Map Badges

`spawn_fast_travel_map` in `ui_plugin.rs` renders a 14×14px colored badge at each site's mapped position on the chapter-select fast-travel map. Liberated sites show "✓" text and a green border; enemy-held sites show "!" and a red border. A name label appears below each badge.

### Save Schema

`SaveData.world_sites: Vec<WorldSiteSaveRecord>` uses `#[serde(default)]` so older saves without site data continue to load with the initial default site list applied. `SaveParams` includes `world_site_registry: Res<WorldSiteRegistry>` alongside other shared campaign registries.

### Spawn Entity Tagging

`spawn_enemy_entity` in `enemy_plugin.rs` returns `Entity` (changed from `()` in M5) so callers can immediately insert `WorldSiteMarker` onto newly spawned enemies. Existing call sites that don't need the return value simply ignore it.

---

## World Routes / Connected Platformer Network (M7)

**File:** `src/resources.rs` (types), `src/plugins/world_plugin.rs` (systems), `src/plugins/save_plugin.rs` (persistence)

**Resource:** `WorldRouteRegistry` (holds `Vec<WorldRoute>`)

World routes are typed, persistent connections between two `WorldSite` nodes. They model the physical traversal spaces players use to move across the strategic map: roads, mountain trails, sky bridges, rail lines, rivers, ocean lanes, tunnels, space lanes, and dungeon gates.

### Route State Machine

`WorldRouteState`:
- `Locked` — impassable; cannot be used for fast-travel or physical traversal (default).
- `Open` — passable; physical props spawned, fast-travel allowed via liberated anchors.
- `Contested` — open but under enemy pressure; spawns a raid encounter in the traversal space.
- `Blocked` — temporarily sealed; requires a specific player action to reopen.

### Route Kinds

`WorldRouteKind`: Road, MountainPath, SkyBridge, Rail, River, OceanLane, Tunnel, SpaceLane, DungeonGate.

### Auto-Unlock Rule

Each `WorldRoute` has a `required_site: WorldSiteId`. Every frame, `world_route_unlock_system` scans Locked routes and opens any route whose required site has become `Liberated`. Routes requiring sites that start liberated (Cloudrail City, Switchwork Borough) are initialized `Open` in `initial_world_routes()`.

### Initial Route Table (6 routes)

| ID | From → To | Kind | Initial State |
|----|-----------|------|---------------|
| 1 | Iron Watchpost → Riftglass Village | SkyBridge | Locked (req site 1) |
| 2 | Riftglass Village → Cloudrail City | Road | Locked (req site 2) |
| 3 | Cloudrail City → Granite Market | Rail | Open (city pre-liberated) |
| 4 | Starfell Outpost → Frost Harbor | MountainPath | Locked (req site 3) |
| 5 | Star Orchard → Lantern Hamlet | Road | Locked (req site 6) |
| 6 | Switchwork Borough → Frost Harbor | OceanLane | Open (city pre-liberated) |

### World Marker

`WorldRouteMarker { id: WorldRouteId }` tags beacon entities spawned at route midpoints. These are valid hook-attach targets (MM2+) and encounter anchor points.

### Save Schema

`SaveData.world_routes: Vec<WorldRouteSaveRecord>` (`#[serde(default)]`) stores only `id + state` per route. On load, `initial_world_routes()` seeds the registry first, then `apply_save_records` patches states from disk. `SaveParams` carries `world_route_registry` alongside the other shared campaign registries.

---

## Raid Counteroffensives (M8)

**Files:** `src/raids.rs`, `src/plugins/world_plugin.rs`, `src/plugins/save_plugin.rs`

**Resource:** `RaidRegistry` (holds `Vec<RaidRecord>`)

Engine M8 adds the first save-backed counteroffensive layer. A `RaidRecord`
stores a stable `RaidId`, `RaidKind`, `RaidPhase`, target `WorldSiteId`,
optional `WorldRouteId`, attacker faction, warning/active timers, required
static defense score, spawned flag, wave size, consequence, and resolution.

Current first slice:

- Cloudrail City (`WorldSiteId(4)`) can trigger one tutorial Scallarian
  `DroneSwarm` after it is liberated.
- Warning phase sets the site to `UnderAttack` and announces the incoming raid.
- Active phase spawns a visible UFO marker above the site and tagged drone
  enemies through `spawn_enemy_entity`.
- Defeating all tagged `RaidThreatMarker` enemies resolves the raid and returns
  the site to `Liberated`.
- Timeout/failure marks the site `Damaged`; early raids do not erase builds or
  permanently reoccupy major progress.

Static defense is computed from settlement builds linked to the target site.
`DefenseOutpost` contributes the most, while `PowerPlant`, `Spaceport`, and
`ResearchLab` provide smaller support. If the defense score meets the raid's
required score during warning, the site becomes `Shielded` and the raid resolves
without spawning enemies.

`SaveData.raids: Vec<RaidRecord>` uses `#[serde(default)]` for legacy saves.
Runtime raid enemies/UFOs are not serialized; active saved raids respawn their
tagged threats when the world is generated again. Route blockade raids have an
optional `target_route`, but actual route blocking should stay behind Engine M7
route behavior.

---

## Tech Hacking And Takeover (M10)

**Files:** `src/hacking.rs`, `src/plugins/hacking_plugin.rs`,
`src/plugins/enemy_plugin.rs`, `src/plugins/weapon_plugin.rs`,
`src/plugins/save_plugin.rs`

**Resource:** `HackingRegistry` (saved blueprint and capture records)

Engine M10 starts the tech hero hacking path. The current slice gives small
Scallarian drones a `Hackable::scallarian_drone()` component when they spawn.
Any active player can use the existing interact input near the drone to start a
short hack link; staying in range completes the hack.

On completion:

- `HackingRegistry` learns `blueprint_scallarian_drone_core` once and records
  each capture attempt.
- `CommandRegistry` gains a new unassigned `ScoutDrone` asset, so hacked enemy
  tech feeds the command strategy layer.
- The drone becomes a temporary `HackedUnit` owned by the initiating
  `PlayerIndex`.
- Hacked drones are excluded from normal hostile AI, enemy attack systems, and
  player weapon aim/damage queries while linked.
- If the drone was part of an active raid, its `RaidThreatMarker` is removed so
  the takeover counts as neutralizing that hostile unit.

Linked drones follow their owning player for a short timer. Pressing fire or
melee while linked triggers a basic laser pulse against the nearest still-hostile
enemy in range, using the normal enemy damage/damaged/killed event path. The
linked drone powers down safely when its timer expires.

`SaveData.hacking: HackingRegistry` uses `#[serde(default)]` for legacy saves.
Only blueprint/capture knowledge is persisted; runtime linked drone entities are
not serialized.

Future M10 work should add tech/scientist gating, true remote possession camera
rules, target stagger requirements for larger robots/mechs/ships, deterministic
blueprint reward tables, and UFO weak-point hacking.

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
          ├── resistance_multiplier() — sums Damageable.resistances entries
          ├── toughness scaling — 100 / (100 + Damageable.defense)
          ├── health.apply_damage(final)
          └── pending_knockback += hit_direction × knockback_force
```

`area_damage_falloff(base, distance, radius)` returns `base * (1 - distance/radius)` for explosive splash.

**Resistances & defense (2026-06):** enemies spawn with faction-flavoured
resistances and their `EnemyConfig.defense` via
`enemy_plugin::enemy_damageable()` — DragonRoyalty resists Fire 0.6/Melee 0.15,
DragonExile Fire 0.3/Laser 0.25, CorruptedHuman Rift 0.4, WizardScientist
Electric 0.5, Scallarians Plasma 0.25, drone chassis +Kinetic 0.3. Weapon choice
now matters per faction.

**Knockback (2026-06):** `apply_damage` accumulates an impulse in
`Damageable.pending_knockback`; `enemy_plugin::apply_enemy_knockback` drains it
as an exponentially-decaying shove (~0.25 s). Wired: projectiles (2.2),
explosions (falloff-scaled 1.0–5.5), melee combos (authored table values),
sabre slashes (3.0). Player-receiving knockback is not yet drained (EC2).

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

**Star Sabre** (`BeamSabre`): locked until Ch.1 discoverable. Toggle `T`. Levels 1–5 increase slash damage, wave damage, slash count, and at level 3+ gains piercing; level 4+ fires dual wave; level 5 adds AoE splash. Beam Capacitors now also raise the Star Sabre's effective wave tier for combo behavior: finishers gain single waves first, mid-chain slashes gain waves next, dual waves arrive at high tier, and level-5/tier-5 finishers throw a triple spread.

In `DungeonCrawlState`, hand melee and Star Sabre attacks prefer movement/facing direction over camera forward, use wider hit arcs, and Star Sabre fires short ground waves even before the late dual-wave rank. This keeps castle interiors readable from the single shared top-down camera.

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
| Drone | 50 | 8 | Fast | Flying Scallarian scout with laser shots, 15 XP |
| Soldier | 100 | 15 | Med | Standard Scallarian invader, 25 XP |
| Heavy | 300 | 25 | Slow | Dragon brute, 50 XP |
| SpikeAlien | 80 | 20 | Fast | Aggressive Scallarian spike alien, 20 XP |
| Hybrid | 1000 | 40 | Med | Boss-tier Scallarian rift champion, 200 XP |

Dragon-faction boss fights add `DragonBoss`: large scaled boss bodies orbit above the closest player while leashed to their arena, advance through health phases, fire volleys, breathe cone fire, and create slam shockwaves.

Flying-drone wings can also trigger the shared boss camera when at least three nearby drones are active. This is the lightweight "ship/drone encounter" hook for future aerial mini-bosses, boarding fights, and dragon-lair alarm rooms.

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

## Level Rewards

**Files:** `src/chapters/mod.rs`, `src/components/discoverable.rs`, `src/plugins/discoverable_plugin.rs`

Chapter-authored discoverables now bridge story progression into the robot and tech systems:

- `RobotPetRescue` adds a named rescued robot pet to the campaign-shared `RobotPetCollection`, with duplicate-safe save behavior.
- `TechCache` grants robot parts, an optional `TechUpgradeId` route hint, and a fixed amount of paid rejuvenation reserve once per save.
- Existing companion, weapon, armor, secret-cave, hidden-reward, relic, and lore beacons still use the same pickup/event path.

Current campaign placement introduces starter beam materials and Spark Pup in Chapter 1, missile materials in Chapter 2, turret materials and Bolt Mason in Chapter 4, armor materials in Chapter 6, mech-link materials and Rivet Guard in Chapter 8, and rejuvenation materials plus Tide Mender in Chapter 11.

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

The airship deck is a temporary chapter-spawned platform with colliders, rail blockers, engine visuals, faction-colored materials, four windup laser turrets, and two moving cover platforms. It is cleaned up when the next chapter starts.

## Secret Caves

**Files:** `src/chapters/mod.rs`, `src/plugins/chapter_plugin.rs`, `src/plugins/world_plugin.rs`, `src/plugins/discoverable_plugin.rs`

Every chapter now has one optional cave route to discover. `WorldPlugin` spawns the physical cave systems when the world is generated:

- Stone and dark-rock tunnel entrances.
- Walkable tunnel floors, side walls, back chambers, and colliders.
- Brushed-metal ribs, glass panels, glowing crystals, and point lights so the caves keep the ancient/new style.
- Two small moving platforms inside each chamber.
- A `WorldAnchor` named `secret_cave_ch01` through `secret_cave_ch14` at each cave's inner discovery point.

Chapter scripts use `PlaceSecretCave` to spawn a green discovery beacon at the matching anchor. Collecting it stores the cave id in `ChapterProgress.discoverables`, sends a UI message/radio line, and prevents that chapter's cave beacon from respawning after save/load.

## Everest Range Fast Travel

**Files:** `src/chapters/mod.rs`, `src/plugins/world_plugin.rs`, `src/plugins/player_plugin.rs`, `src/plugins/chapter_plugin.rs`, `src/plugins/ui_plugin.rs`

The campaign map is now the 200 x 200 mile Everest-range heightmap world. `EVEREST_RANGE_MILES` and `EVEREST_RANGE_WORLD_SIZE` define the shared scale (`20_000` world units at `100` units per mile), and `chapter_map_locations()` defines one named map location per chapter, including the `WorldAnchor` id, region label, landmark, X/Z position, and party-facing yaw.

`WorldPlugin` turns those records into visible in-world chapter beacons and matching `WorldAnchor`s after terrain generation. It also adds range-scale snowfields, glacier streams, alpine forest pockets, sci-fantasy outposts, waypoint crystal lines, and dragon-lair silhouettes. All anchors and large props sample the same `terrain_surface_y()` path used by the terrain mesh/collider, so fast travel, visuals, and collision agree.

Chapter select renders those locations as the Everest Range fast-travel map. Keyboard chapter picks and clickable unlocked map markers both set `CurrentChapter.id`; entering `Playing` spawns or moves players to the matching heightmap beacon and clears travel momentum/boat passenger state.

## Exploration Settlements

**Files:** `src/chapters/mod.rs`, `src/discussion.rs`, `src/plugins/world_plugin.rs`, `src/plugins/ui_plugin.rs`

`map_settlements()` defines optional cities, villages, harbors, and outposts that fill the 200-mile map between chapter anchors. `WorldPlugin` samples local terrain relief for each settlement and assigns a grounded, terraced, or sky-district layout profile. Plazas, segmented roads, building pads, landmarks, discussion NPCs, caches, anchors, and spy activity all use that shared floor profile so interactables and visuals stay aligned with the heightmap. Mega cities now sit on large walkable floating platforms with giant ramp/bridge approaches, guide lights, tower pads, and underside lift columns; rough mountain edges can spawn inset stone/metal gates with dark openings and threshold paths. Chapter select renders matching non-clickable map markers: `C` city, `V` village, `H` harbor, and `O` outpost. Markers turn green after their cache reward id is collected.

Each settlement also has one `DiscussionNpc` near its plaza. Press `E` / D-pad Down near that NPC to open the discussion panel. Dialogue scripts live in `src/discussion.rs`; each line may reference an MP3 under `assets/voice/...`, and the HUD spawns that clip once when the line appears. Missing voice files are acceptable during prototyping, but final recorded lines should keep the documented paths or update the script.

City settlements are peaceful mega-city hubs protected by orbiting `FreePeopleGuardianShip` patrols from the Free Peoples of Earth. They also hide a small counter-spy activity: Cloudrail City and Switchwork Borough each spawn two non-combat `CitySpyDrone`s. Shoot them down with normal player weapons, then collect the spawned `SpyData` beacon to earn save-backed credits, XP, and armor rewards.

Current settlements:

| Settlement | Kind | Region | Reward id |
|---|---|---|---|
| Riftglass Village | Village | Rift Foothills | `settlement_riftglass_cache` |
| Cloudrail City | City | High Sky Rail | `settlement_cloudrail_cache` |
| Lantern Hamlet | Village | Tibet Snow Road | `settlement_lantern_cache` |
| Star Orchard | Village | Fangroot Meadow | `settlement_star_orchard_cache` |
| Frost Harbor | Harbor | Antarctic Range | `settlement_frost_harbor_cache` |
| Granite Market | Village | Rockies Gate | `settlement_granite_market_cache` |
| Switchwork Borough | City | Mana Switchworks | `settlement_switchwork_cache` |
| Starfell Outpost | Outpost | Crown Road | `settlement_starfell_cache` |

## Great Scientist Temple Subquests

**Files:** `src/plugins/world_plugin.rs`, `src/plugins/player_plugin.rs`, `src/plugins/discoverable_plugin.rs`, `src/resources.rs`

Great Scientist temples are optional dungeon-like labs placed across the wider Everest Range. They reuse `DungeonCrawlGate`, `DungeonGateDoor`, `DungeonCrawlState`, and `HiddenReward`, so interacting with a temple gate switches the party into one shared top-down dungeon camera and collecting the inner cache saves through `ChapterProgress.discoverables`.

Chapter select also renders non-clickable temple hint markers. They are purple while undiscovered and gold after the matching reward id is collected.

Current temple rewards:

| Temple | Reward | Runtime effect |
|---|---|---|
| Temple of the Sky Equation | Ancient Flight Core | Improves air control and jetpack/flight fuel, force, regen, and vertical velocity |
| Solar Sabre Observatory | Solar Sabre Glyph | Boosts hand/sabre-style melee pressure, Rainbow Ray damage/ammo, and Tri-Star Burst capacity |
| Nova Missile Foundry | Nova Missile Matrix | Boosts Nova Orb missile damage, explosion radius, ammo, and Homing Star capacity |
| Aegis Frame Archive | Aegis Armor Frame | Grants armor rewards and improves traversal reliability through stronger snap/step and extra wall-jump charge |

Temple reward IDs are reapplied to newly spawned players by `apply_scientist_temple_progress()`, so movement and weapon upgrades persist through save/load without adding a new save section.

## Traversal Courses

**Files:** `src/components/world.rs`, `src/plugins/world_plugin.rs`, `src/plugins/player_plugin.rs`

The city now has authored optional traversal courses outside the starter zone:

| Course | Mechanics | Reward |
|---|---|---|
| Sling Tower | Slingshot launch pad, ramp, wall-jump shaft, moving bricks, rotating elevator | Sling Tower Cache |
| Ramp Brick Tower | Slingshot launch pad, angled ramps, moving brick chain, rotating elevator | Ramp Brick Tower Cache |

`SlingShotPad` launches a player when jump/interact is pressed near the pad. It writes horizontal velocity into `PlayerMovement.ground_velocity`, vertical launch into `PlayerMovement.velocity.y`, clears coyote/jump buffers, and forces airborne movement state.

`RotatingElevator` is a kinematic platform that orbits an authored center while bobbing vertically. Its system carries grounded or landing players by the platform delta, matching the existing `MovingPlatform` behavior.

Both Level 1 courses now include visible marker posts, recovery platforms, and return slingshots so missed jumps have a readable reset path.

## Hidden Rewards

**Files:** `src/components/discoverable.rs`, `src/plugins/discoverable_plugin.rs`, `src/plugins/world_plugin.rs`

Hidden reward rooms use `DiscoverableKind::HiddenReward` to grant save-backed optional rewards. A cache can award credits, XP, armor capacity/refill, a power-up unlock id, and a special-ability upgrade/refill. Current rooms also include authored puzzle dressing, pressure-plate props, visible gates, and reward staging so secrets read as small challenges instead of loose pickups.

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
| Heart | Second Wind | +0.5 HP/sec/rank while rejuvenation reserve is available | 4 |
| Star | Star Focus | +5% beam damage/rank | 5 |
| Star | Pocket Constellation | +15% max charges/rank | 3 |
| Acrobat | Wall-Dancer Evasion | -10% dodge stamina cost | 3 |
| Acrobat | Lucky Parry | +0.05s parry window/rank | 3 |

Current wiring:

- `Family Vitality` increases max HP through the armor/perk max-health sync.
- `Second Wind` contributes regen rate, but actual healing now spends the saved tech-upgrade rejuvenation reserve.
- `Star Focus` increases primary beam, special tool, and Star Sabre damage.
- `Pocket Constellation` increases primary ammo and special tool charge caps.
- `Wall-Dancer Evasion` lowers dodge stamina cost.
- `Lucky Parry` extends the parry window.

---

## Tech Upgrades

**File:** `src/upgrades.rs` | **Resource:** `UpgradeLedger`

Tech upgrades are the campaign-shared production spine for weapon, missile, turret, health, rejuvenation, and future mech/vehicle/ship upgrade paths. Spend robot salvage in chapter select with `Z/X/C/V/B/N`. The current UI is intentionally compact, but it already displays rank, next-rank cost, affordability, and rejuvenation reserve.

| Key | Upgrade | Current runtime effect |
|---|---|---|
| `Z` | Beam Capacitors | +6% primary beam and Star Sabre damage/rank |
| `X` | Nova Missile Forge | +10% explosive nova/missile damage/rank |
| `C` | Sprite Turret Lattice | +12% Sprite Turret damage/rank |
| `V` | Armor Plating | +12 max HP/rank through armor/perk health sync |
| `B` | Rejuvenation Matrix | Adds paid rejuvenation reserve and improves healing efficiency |
| `N` | Mech Command Link | Saved rank gate for later robot vehicle/mech/ship upgrades |

Rejuvenation is deliberately not free passive healing. `Second Wind` and `Rejuvenation Matrix` set the healing rate, while `UpgradeLedger.rejuvenation_charge` is consumed for each HP restored. Buying Rejuvenation Matrix ranks spends robot salvage, refills the reserve, and lowers charge-per-HP cost.

Future work should split the compact chapter-select panel into a full garage/upgrade screen with tabs, previews, material sources, weapon/turret stat diffs, and controller navigation.

---

## Save / Load

**File:** `src/plugins/save_plugin.rs`

Save file: `starfall_i_save.json` (written next to the binary).

- **Autosave**: every 30 seconds while `Playing`.
- **Manual save**: F5 key.
- **Load**: chapter progress is hydrated on startup; player stats are applied on `OnEnter(Playing)`.

Saved shared fields: `wave_number`, completed chapters, discoverables, recruited companions, recovered scientist relics, recovered relic fragments, unspent perk points, perk ranks, robot pets, tech upgrade ranks, rejuvenation reserve, player-slot character blueprints, and chassis part loadout (`part_loadout_body/arms/legs/shoulders` as `CharacterPartStyle`).

Saved per-player fields live in `players[]` records keyed by `player_index`: level, experience, credits, health, stamina, and armor values. Older top-level stat fields are still accepted for legacy save migration, but new saves use the per-player records as the authoritative source.

Save ownership tests cover per-player record round-trips, legacy top-level stat hydration, matching by `player_index` instead of record order, and clamping loaded runtime values into valid health/stamina/armor ranges.

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

- Terrain heightmap via deterministic layered waves/ridges plus `assets/terrain/everest.png` mapped across the full 200 x 200 mile range; seed from `GameSettings.world_seed`.
- Current heightmap import is full-map scoped: `assets/terrain/everest.png` is loaded once through Bevy image decoding, sampled bilinearly with a smoothing transform, faded at its outer edges, and blended into the deterministic terrain so collision and visuals share the same height function.
- Terrain visuals use Bevy mesh vertex colors as the current texture layer: low grass, alpine meadow, exposed rock, darker strata, blue ice, high snow, and shared mountain-route trail tint are derived from height, slope, seeded detail noise, and route influence.
- `mountain_routes()` is the shared source for path color corridors, light guide waylines, carved pass relief, and spawned slab/stud path pieces. Keep future chapter routes on this helper so map art, terrain shaping, and navigation markers stay aligned.
- Outer districts, spaceports, trees, crystals, mountains, and authored anchors sample the terrain surface so upgraded props sit on the generated ground rather than the old flat plane.
- Decorative trees via L-system string rewriting (`lsystem/mod.rs`) + 3-D turtle interpreter (`lsystem/turtle.rs`).
- City-safe terrain is clamped to the invisible gameplay floor, keeping terrain visuals and collision from diverging below Y=0.
- Lush procedural nature adds dense grass, forest pockets, water gardens, reeds, flowers, shrubs, mossy rocks, and darker stone outcrops.
- `NatureSway` gives trees, water surfaces, reeds, flowers, shrubs, and moss caps a subtle hand-animated motion.
- Downtown towers mix transparent glass panels, glowing window facades, metal mullion grids, brushed-metal skins, and occasional stone-brick bodies for the ancient/new skyline style.
- Industrial buildings can receive ribbed metal cladding and factory ribbon windows.
- Smaller residential and outer-district buildings receive stone-brick variants, mortar courses, stone plinths, corner blocks, roof caps, moss, and warm/cool/dark window panels.
- Aurora Castle now uses brick/mortar castle materials, framed glowing windows, animated wind banners, a real front bridge and open gate, and a hollow keep built from room walls, balconies, lights, and explorable interior spaces.
- Dragon lair dungeons are spawned for Chapters 6-11. Each dungeon uses an old-school RPG room-and-corridor layout with an entrance arch, branching side rooms, hazard strips, moving bridge platforms, turret guards, a raised lair dais, a save-backed hoard beacon, and a `WorldAnchor` named `dragon_dungeon_ch06` through `dragon_dungeon_ch11`.
- Each dragon lair entrance now has a `DungeonCrawlGate` and sliding `DungeonGateDoor` panels. Interacting near the big gate activates `DungeonCrawlState`, opens the door, switches to a single shared top-down camera, pulls the party toward the dungeon focus, and remaps movement to fixed top-down axes until the party leaves the dungeon radius.
- Secret caves are spawned from authored chapter specs and combine stone tunnels, metal ribs, glass panels, glowing crystals, small moving platforms, and save-backed discovery anchors.
- Moving platforms bridge rooftops, castles, and high paths; the platform system carries grounded or landing players while avoiding midair drag.
- Laser turrets track nearby players, show a brief beam windup, then apply laser damage through the same player damage/parry path.

## Character Designer

**Files:** `src/characters.rs`, `src/plugins/character_design_plugin.rs`, `src/plugins/player_plugin.rs`

- Player slots store optional outfit/accent/hair preset indices plus accessory toggles.
- Preset indices are normalized before preview, saving, and player spawning, so stale save data cannot panic by indexing outside the palette.
- The designer preview camera faces the character's front side, while the same spawned character parts are reused by the in-game player model.

---

## Vehicle System

**Plugin:** `VehiclePlugin` | **File:** `src/plugins/vehicle_plugin.rs`

`VehicleState` uses two enums instead of boolean flags:

- `GroundMode`: `None`, `Motorcycle`, `Tank`, `GiantMech`
- `AirMode`: `None`, `Jet`, `Ship`

**M key** cycles ground mode. **J key** cycles air/boat mode. Access is gated by either a `PlayerLoadout` blueprint OR `RobotPetCollection.active_assembly`:

| Assembly form | Ground mode | Air mode |
|---|---|---|
| Car / Motorcycle | Motorcycle | — |
| Tank | Tank | — |
| GiantMech | GiantMech | — |
| SpaceJet or jet_blueprint | — | Jet |
| SpaceShip / MegaShip | — | Ship |

`apply_vehicle_buffs()` applies per-mode stats:

| Mode | Speed cap | Armor bonus | Notes |
|---|---|---|---|
| Motorcycle | ×1.58 walk | — | Fast ground travel |
| Tank | max 0.42 | — | Slow, armored |
| GiantMech | max 0.34 | +40 | Very slow, heavy armor |
| Jet | jetpack boosted | — | Enhanced airborne |
| Ship | 1.5× jet | — | Best air mode |

The party still shares one active vehicle mode at a time. 3-D mech/ship controller runtimes are future work.

---

## Robot / Chassis System

**Module:** `src/robots/` | **Plugins:** `RobotGaragePlugin`

`RobotStyle` in `designer.rs` defines colors and part config. `factory.rs` spawns the physical entity using Capsule3d/Sphere/Cylinder geometry. `presets.rs` has named robot builds (`amp()` is the default player chassis). Playable-character loadouts are now edited through `CharacterDesignPlugin`, while the robot garage handles pet assembly and advanced vehicle/mech forms.
