# Starfall I — Legacy Feature Notes

This file is retained as a historical reference and does not represent the
current active feature source of truth. The current master feature catalog is
[MASTER_FEATURES.md](MASTER_FEATURES.md).

For *how the code is organized and how to work on it*, see
[DEVELOPMENT.md](DEVELOPMENT.md). Forward-looking engine work lives in
[engine_roadmap.md](engine_roadmap.md); task-focused how-tos live in
[guides/](guides/README.md); superseded reviews, audits, and dated plans are
preserved under [archive/](archive/README.md). Sequel-continuity parity and its
explicit remaining boundaries live in [HEAVY_WATER_PORT.md](HEAVY_WATER_PORT.md).

Runtime state ownership worth knowing up front: `MenuFocus` supplies shared
controller menu navigation, `PlatformerMoveState` owns roll/stomp tuning,
`SpeedLoopTraversalState` and `RoadRecoveryState` own guided loops and
checkpoints, `TrackingMissile` owns Homing Star targeting, and
`WeaponVisualProfile` owns primary beam scale.

## Contents

- [Local Multiplayer](#local-multiplayer)
- [Player Guidance HUD](#player-guidance-hud)
- [Player Movement](#player-movement)
- [Character Studio (Human Generator)](#character-studio-human-generator)
- [Anime Face System](#anime-face-system)
- [Character Authoring](#character-authoring)
- [Hero Combat Identity](#hero-combat-identity)
- [Robot Pets And Combined Forms](#robot-pets-and-combined-forms)
- [Settlement Builder And Economy](#settlement-builder-and-economy)
- [Explorable Building Interiors](#explorable-building-interiors)
- [World Sites / Reclaimable World State](#world-sites--reclaimable-world-state)
- [World Routes / Connected Platformer Network (M7)](#world-routes--connected-platformer-network-m7)
- [Raid Counteroffensives (M8)](#raid-counteroffensives-m8)
- [Tech Hacking And Takeover (M10)](#tech-hacking-and-takeover-m10)
- [Damage Pipeline](#damage-pipeline)
- [Weapons](#weapons)
- [Enemy AI](#enemy-ai)
- [Chapter System](#chapter-system)
- [Level Rewards](#level-rewards)
- [Castle Airship Escalation](#castle-airship-escalation)
- [Secret Caves](#secret-caves)
- [Everest Range Fast Travel](#everest-range-fast-travel)
- [Exploration Settlements](#exploration-settlements)
- [Great Scientist Temple Subquests](#great-scientist-temple-subquests)
- [Traversal Courses](#traversal-courses)
- [Hidden Rewards](#hidden-rewards)
- [Relic Puzzles](#relic-puzzles)
- [Perk Tree](#perk-tree)
- [Tech Upgrades](#tech-upgrades)
- [Save / Load](#save--load)
- [Companions](#companions)
- [Music Deck And Modular Action Audio](#music-deck-and-modular-action-audio)
- [Procedural World](#procedural-world)
- [Character Designer](#character-designer)
- [Vehicle System](#vehicle-system)
- [Robot / Chassis System](#robot--chassis-system)
- [Controller And Player Ownership](#controller-and-player-ownership)
- [Player Music And Action SFX Modding](#player-music-and-action-sfx-modding)
- [Character Studio Workflow](#character-studio-workflow)
- [Creation Toolchain](#creation-toolchain)

## Local Multiplayer

**Resource:** `LocalPlayerConfig` | **Files:** `src/resources.rs`, `src/plugins/input_plugin.rs`, `src/plugins/player_plugin.rs`

Set `LocalPlayerConfig.active` (1-4) before entering `AppState::Playing` to change the player count. The player-select screen writes this resource before chapter select.

| Players | Camera layout |
|---|---|
| 1 | Full screen |
| 2 | Top half / bottom half |
| 3 | Top-left, top-right, bottom full |
| 4 | Four equal quadrants |

Each player can independently switch their viewport between third person and
first person (`P` for keyboard P1, Select + D-pad Left for any assigned
controller). Boss and dungeon party cameras temporarily override the individual
preference, then restore it when the shared view ends.

**Input assignment:** keyboard/mouse remains available to P1. The first
controller connected at boot or hot-plugged automatically claims P1; an
additional unassigned controller pressing Start in Player Select claims the
next available stable P2–P4 `PlayerIndex`. Disconnect releases the device
binding and reconnect can reclaim the joined slot. Connection events establish
the automatic P1 binding; gamepad query order is never player identity.

**Character assignment:** each joined slot can cycle through Vincenzo, Antonio,
Angelo, Joseph, Gabriella, Nova, Aurora, and Fortuna. The default join order
still starts on the brothers for P1-P4, but the selectable roster is now all
eight siblings.

**Architecture:** Each player entity carries a `PlayerInput` component written by `InputPlugin` each `PreUpdate`. `GamepadAssignments` maps controller entities to stable `PlayerIndex` identities through automatic P1 connection events plus Start-to-join for P2–P4; runtime input never depends on gamepad enumeration order. Each player's camera entity is stored in a `PlayerCameraRef(Entity)` component, while `PlayerView` owns that player's perspective and character-scaled eye height. Owner-specific avatar render layers hide only the local body in first person without removing it from another split-screen camera. Weapon, movement, camera shake, damage flash, and interaction systems continue to resolve the correct player.

**Controller feel:** gamepad movement uses circular deadzone remapping and preserves analog stick magnitude in `PlayerMovement`, so partial tilt produces partial travel speed. Right-stick look uses a quadratic curve for low-deflection precision. LT/RT aim/fire supports both Bevy digital trigger buttons and `LeftZ` / `RightZ` analog trigger axes, with frame-local just-press tracking for single-shot weapons.

**Game over:** triggers only when ALL players are dead simultaneously.

**Pause:** `Esc` / controller Start toggles between `Playing` and `Paused`. The pause menu keeps the current HUD/world entities alive, pauses the Avian `Time<Physics>` clock, offers party-wide save and save-and-title actions, and includes controls/tips and settings pages. The same Settings & Accessibility page is available before play from the title screen. Settings persist UI scale from 80–140%, a 0–8% safe-area inset, controller glyph style and face-button layout, discrete Player 1 keyboard bindings, invert-look Y, difficulty, music, SFX, rumble, damage-number visibility, stronger UI contrast, reduced UI motion, and subtitles. Reduced UI motion stops floating damage-text travel, subtitles remain visible automatically when a dialogue line has no voice asset, and high contrast strengthens menu focus and HUD panels. Returning to title from pause cleans up preserved play-session entities.

**Input prompts:** shared navigation, pause, loadout, crafting, and Robot Garage
help text tracks the most recently used keyboard or gamepad. Keyboard prompts
show Arrow/WASD, Enter, and Escape bindings; controller prompts show D-pad/stick
and face-button bindings. Controller glyphs can be pinned to Xbox, PlayStation,
or Nintendo conventions, or left on Auto to use the active controller's USB
vendor family.

**Configurable controls:** Settings exposes four complete face-button layouts
(Standard, Nintendo, Swap A/B, and Swap X/Y), thirteen discrete Player 1
keyboard actions, invert-look Y for mouse and stick together, and a reset to
defaults. Face buttons are remapped at the input source for both Bevy and native
macOS controller backends, so modified buttons carry their chords with them.
Movement/look axes plus LB, Select, and D-pad modifiers remain fixed as the
control grammar, preventing configurations that strand an action. The sparse
keyboard overrides and layout choice persist inside `GameSettings`; older
settings files fall back to shipped defaults for actions added later.
The face layout is currently profile-wide and shared by all local controllers;
per-device layouts for mixed Xbox/Nintendo couch co-op are not yet supported.

**Shared boss camera:** when 2-4 players are active, `PlayerPlugin` switches from split-screen to one full-screen party camera during boss-tier enemies or nearby flying-drone wings. P1's camera becomes the shared view, the other cameras are temporarily disabled, and viewports are restored when the threat clears. Distant players are softly pulled toward the encounter anchor, with a hard catch-up if they are far outside the battle space.

**Ownership policy:** `PlayerIndex` is the stable owner key. Chapters, world
state, settlements, raids, robot pets, commands, hacking, and final-war state
are shared. `PlayerProgression` (perks, upgrades, weapon ranks, shop),
inventories, rewards, HUD panels, camera/damage feedback, companions, crafting,
runtime stats, character blueprints, and save `players[]` records are
per-player. Vehicles remain party-shared: one driver/vehicle mode, with
passengers keyed by `PlayerIndex`.

**Known limitations:**
- Chapter director spawns use the party center as the encounter anchor; bespoke chapter beats are still campaign-shared.
- HUD stat/weapon panels, save snapshots, companions, crafting, chests, hidden rewards, enemy loot pickups, damage feedback, and vehicle buffs are keyed by `PlayerIndex`.
- In two-player layouts, HUD panels anchor at the upper-left and lower-left of
  their respective viewports. Three- and four-player layouts anchor panels to
  the matching screen quadrant rather than stacking every player down the
  global left edge. Safe-area changes and window resizes update these anchors
  live.
- Vehicle buffs apply to the activating player, and another player cannot silently
  steal an active ground/air mode. The party still shares one active vehicle.
  In Chapter 1, the boat reserves unique seats, keeps one driver, admits nearby
  late passengers, promotes the lowest-index passenger if the driver leaves,
  and requires docking before disembarking.
- Armor infusion cycling is per-player: every controller uses LT + D-pad
  Left/Right; P1 can use Shift + `[` / `]`. Plain brackets remain primary weapon
  cycling.
- The in-game Star Loadout is a per-player modal button GUI opened with `I` or
  LB+Select. Weapons, Armor, Items, Specials, Rides, and Relics use owner-scoped
  D-pad/stick focus; only the owner's gameplay input is captured. Relics is a
  read-only catalog: ASCII shapes distinguish core, technique, elemental, and
  legendary objects without relying on color, while FOUND/LOCKED rows and the
  detail line show effect, input, and world-source hint.
- Rocket Hoverboard is the production Hoverboard traversal profile. Every
  assigned player can enable it directly with LB + D-pad Up or through
  Loadout → Rides; LB + D-pad Left/Down/Right selects Grapple, Hover Jet, or
  Flight. It provides fast
  ground momentum and loop adhesion plus airborne steering, jump-held rocket
  lift, LB boost flight, fuel drain, a persistent rider pose,
  and visible metallic/emissive twin-thruster board geometry.
- Crafting panels retain an independent recipe cursor for each player. The owner
  uses D-pad/left stick to select, South/A to craft, and Select to close. A
  per-player modal capture preserves those UI actions while suppressing movement,
  combat, and vehicle/interact leakage.

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
| Sprinting / vehicle turbo | Shift / LB + move | On foot, drains stamina 15/sec; in an active non-boat vehicle mode, requests the 0.7-second shared turbo burst |
| Jetpack | Hold Space / South while airborne | Burns `fuel_cost_per_sec = 20`/sec from an 800-unit tank (~40 s thrust, sized for the Everest ranges); regens 45/sec on ground |
| WallSliding | Pushing into wall while falling | One-hand wall clasp; drains light stamina and caps fall speed at `wall_slide_speed = 0.28` |
| Hanging | Interact while falling into a wall | Max hang time 2.5s; drains stamina 12/sec |
| Climbing | Push firmly forward into a vertical surface | Free-climb mountains/buildings on its own HUD energy bar (`CL`, 420 units, drains 26/sec, regens 55/sec grounded). Stick climbs/descends/shimmies; buffered jump mid-climb performs a full wall jump (charges preserved); topping out vaults up-and-over; camera auto-pulls back + widens FOV while climbing |
| Grappling | `G` / Select+RB | Scores forward route sockets, enemies, drones, bosses, and Grapple-mode surface targets; then zips, swings, or pulls with jump/dodge release |

Jumping uses a short input buffer, coyote timer, early-release jump cut, and a short apex float so near-edge jumps, taps, and high-arc jumps feel more responsive. Falling uses a stronger gravity multiplier and a capped terminal velocity. The local `KinematicCharacterController` compatibility component feeds Avian move-and-slide and uses explicit movement-profile fields for offset, step height/width, and snap-to-ground distance so small lips and authored traversal props are easier to tune consistently.

Analog movement preserves stick strength. Sprint requires the sprint input plus near-full stick deflection (`analog_sprint_threshold`) so light controller movement does not unexpectedly drain stamina or snap into sprint. Three jump presses inside a 0.48-second rolling window toggle the player's persistent flight/hover preference; hover damps vertical speed and supports controlled lateral travel while ordinary flight retains thrust/glide behavior.

Wall slide is the default wall-contact behavior while falling; it now behaves as a stamina-backed one-hand wall clasp, drains a small amount of stamina, refreshes wall-jump charges, slows descent, and falls back to a faster tired slide when stamina is gone. Hanging is intentional through `E` / D-pad Down. Wall jump is triggered from buffered jump input while `wall_contact_timer > 0` and airborne. It pushes away from wall normal + 25% input direction, has a short steering lockout for cleaner arcs, and carries two wall-jump charges that refresh on landing or renewed wall slide contact.

Intentional edge grabs validate three pieces of level geometry before entering
`Hanging`: a chest-height vertical wall, head clearance above that wall, and a
walkable top surface within reach. The resulting root anchor/top point remain
stable throughout the hang instead of following noisy triangle contacts.
Hoverboard mode does not enter ordinary edge hangs. Grapple-mode surface
fallbacks raycast World geometry, and zip arrival at a valid lip hands off to
an anchored hang or a short mantle. Hard traversal constraints clear both the
fixed-tick accumulator and its smoothed carry so pre-catch movement cannot leak
into a hang, rail/loop snap, recovery, or teleport.

Procedural character poses now distinguish idle, walk, run, jump, fall, flight,
one-hand wall slide, hang, combat, Star Sabre slash, and grapple wind-up. The
long-form roadmap for turning this into a full humanoid traversal/combat system
lives in `docs/archive/playerengine.md`.

**Fixed-tick motor (EC1, default ON, 2026-07):** the traversal/grapple/motor
chain runs at 64 Hz in `FixedUpdate`, consuming a per-player latched input
buffer (edges fire exactly once per tick at any frame rate) with camera
interpolation by the fixed-clock overstep. Escape hatches:
`STARFALL_LEGACY_MOTOR=1` or F10. See `docs/guides/fixed-tick-motor.md`.

Advanced Character Studio output is playable rather than preview-only. The
`USE IN GAME` action stores both an exact `CharacterSpec` and a derived
`CharacterBlueprint` on the selected `PlayerSlotConfig`; save data serializes
the studio recipe separately so clothing, face, hair, colors, and morphs survive
reload. Player spawning sends that recipe through `build_character_patch()` and
`spawn_human()` while retaining the normal gameplay skeleton, collider, movement
profiles, weapons, and camera setup. `StudioHumanPart` classifies generated mesh
pieces into torso/head/arm/leg regions and tags generated eyes, brows, and mouth
geometry independently. `studio_human_animation_system` rotates
those regions around authored body pivots for walking, running, sprinting,
jumping, falling, shooting, Star Sabre attacks, hovering, gliding, flight, and
air dash, while the visual root adds pose-specific lean, bob, stretch, and
compression. The facial layer provides deterministic blinking, combat brows,
attack mouth opening, and exertion movement. Named good-guy, bad-guy, fantasy,
street-clothes, and Mecha Robot presets remain editable `CharacterSpec` seeds.

Robot and monster generation has a compatible versioned recipe layer in
`robots/creature.rs`. `CreatureSpec` preserves the complete legacy `RobotStyle`
while adding deterministic seed, kind, topology, role, faction, surface, stable
content ID, schema version, modifiers, and validation. Six curated Forge seeds
cover allied, hostile, retro-mecha, crystal, flying, and cave-creature
silhouettes. `AppState::CreatureForge`, reached from Project Hub, edits those
recipes against a live procedural preview and supports controller focus,
undo/reset, project-backed validation/save, and versioned preset export/load.
`spawn_creature()` compiles recipes through the existing robot factory, varies
PBR response by surface, and attaches `GeneratedCreature` metadata to the root.
Published project records whose draft and published hashes match populate
`PublishedCreatureCatalog`; a dungeon `CreatureSpawnOverride(content_id)` can
spawn one as a combat enemy with role-derived stats and falls back to its
built-in enemy when the ID is unavailable. Existing `spawn_robot()` callers and
named drone/boss presets remain unchanged.

Locomotion animation thresholds use the same units as `PlayerMovement`: walk
begins above `0.06`, run above `0.48`, and the explicit `Sprinting` state selects
the faster sprint cycle. The former run threshold of `28.0` was unreachable for
normal movement speeds (`walk ~= 0.38`, `sprint ~= 0.70`).

Grapple hook: `GrappleHookState` is the single-hook state component with
Ready/Windup/Searching/Swinging/Zipping/Recovering/Cooldown modes and bounded
cable, zip, mountain, attack-pull, spring, and damping tuning. Search scores
forward `GrappleSocket`s, open/contested route markers, drones, ordinary
enemies, and boss/heavy weak points within cable range. Grapple traversal mode
also supplies a World-raycast surface latch when no authored target wins;
stunt rails, spring pads, and speed-loop apexes supply authored sockets. Target kind
selects swing or zip behavior; movement pumps a swing, while jump or dodge
releases either mode. Reaching an enemy applies kinetic impact damage and pulls
non-heavy enemies toward the player; boss/heavy targets take the hit without
being displaced. A zip that reaches a validated ledge transfers into the shared
edge hang/mantle path rather than dropping after hook recovery.

Climb-up: `E` / D-pad Down while hanging. Boosts player upward `climb_boost * dt * 60`.

Free climbing (2026-07): pressing the stick firmly into a wall (`input · -wall_normal > 0.35`)
starts a climb when the `CL` bar has ≥30 energy. While climbing the motor moves
in the wall plane (stick Y up/down, stick X shimmy) with a light inward pull to
hold contact. Exits: buffered jump → standard wall jump (uses/keeps the same
charge economy, so the double-jump-off-buildings verb is untouched); losing the
wall → ledge vault (`vault_boost` up + over); dodge, stick-down on ground, or an
empty bar → release. Grapple swing/zip suppresses climb starts.

Speed roads (2026-07): fourteen mountain trunks and cross-links form multiple
closed racing circuits instead of isolated spokes. Decks use a continuous
full-route profile (`speed_road_route_profile`) with stations no farther than
32 units apart. `terrain_surface_y` reproduces the exact two-triangle planes
used by the 62.5-unit terrain collider grid instead of querying a mismatched
continuous height function. Each station pair samples thirteen lanes across
the entire 64+ unit road footprint every 1.5 longitudinal units. The nominal
0.75-unit center clearance and 0.65-unit interpolation envelope keep ordinary
roads close to grade. The terrain mesh carves a 96-unit full-width shelf that
blends back into the mountain by 190 units and smooths summit noise along the
route tangent. Uncapped correction and a mountain-road grade envelope
(`SPEED_ROAD_MAX_GRADE = 1.0`) let short Sonic-style ascents follow that shelf
instead of propagating a remote summit backward and lifting the whole network.
`speed_road_network_profiles` synchronizes shared junction elevations and
re-runs grade propagation across the connected graph, keeping intersections
vertically flush. NPC traffic consumes the same profiles as deck generation.
Non-route settlement rings and spurs retain their local terrain-lift solver.
The southeast Grand Raceway is a dedicated local terrain tile sourced from
`assets/terrain/RACE.png`, feathered into the existing Everest height function
so the render mesh, trimesh collider, road solver, anchors, and recovery queries
all see the same surface. Two access roads connect the district to the Rockies
and Antarctic trunks. Its closed circuit is 94 units wide and adds dense
ground-level boost lanes, four course-specific launch ramps, six ordered
three-lap gates, two NPC rivals, and two `R` world-map travel points. These
travel points reuse the active chapter session without activating dungeon mode.
Boost-pad impulse and effective sustain/cap consume the owning player's Motion
Boot and Aegis upgrade multipliers, clamped to preserve authored course bounds.
Every authored route endpoint gets a terrain-sampled entrance built by
`spawn_speed_road_ground_access`: it begins with its deck surface at ground
level, uses an eased ramp capped around an 8% target grade, and merges flush
with the engineered profile. Approach corridors participate in prop avoidance.
`speed_road_sky_access_chunks` also divides every sustained aerial run into
reachable intervals. `spawn_speed_road_sky_access` searches both sides for
nearby low ground and builds a 120–720 unit supported branch ramp. The matching
main-road chunk now restores a full rail opposite the ramp plus split rails on
the approach side, leaving only the actual ramp mouth open instead of an entire
unprotected aerial chunk. Final ramp segments remain open for the merge.
These side approaches also participate in tree/building corridor avoidance.
Approach selection rejects ground points above the merge and heavily penalizes
terrain that would pierce a straight uphill ramp. Generated ramp stations are
monotonic toward the sky road. The main network no longer asks
`spawn_boost_road_span` to add automatic end ramps to ordinary 48-unit chunks;
those repeated colliders were the metallic bumps that drained player speed.
`spawn_board_boost_ramp` now treats its position as the low entrance and derives
yaw from launch direction, preventing half of the mesh from spawning beneath
the road. Support crossbeams account for road grade and remain below the deck.
Elevated deck sections with at least 8 units of clearance spawn paired,
collidable stone pylons with a metal crossbeam. Pylon feet independently sample
the terrain collider beneath each side of the road, supporting rising mountain
approaches and long aerial viaducts instead of leaving them visibly floating.

**Star City continuation ring (2026-08):** the same speed-road generation pass
spawns one closed elevated landmark at world origin. It uses exactly 56 uniform
chords around radius 280, a 32-unit deck at `y = 80`, overlapping deck seams,
continuous two-way neon lanes with eight paired trigger stations, a collidable
direction divider, outer safety rails, and terrain-founded collidable supports.
Four world-cardinal approaches run from driveable ground mouths to protected
outer-lane openings at each cardinal chord midpoint, clear of the center
divider; each climb uses 24 eased,
terrain-aware deck sections across a 280-unit run, with rails, support columns,
and real boost pads. The ring and all four approach centerlines participate in
the shared road distance field, so procedural props and buildings preserve the
drivable corridor. Pure tests lock the radius/height/count, chord uniformity,
closed seam, cardinal merge geometry, and keepout coverage.

Road system v4 (freeway-grade): main mountain trunks are widened for high-speed
traffic (`SPEED_ROAD_WIDTH = 256`) and resampled into 32-unit deck slices after a
bounded automatic Hermite-rounding pass. Settlement rings deliberately remain
the smaller road/path family (City 46 / Harbor 42 / Village 38 / Outpost 36).
**Collidable guardrails** on both edges of every trunk deck (4.6 tall and 1.05 thick,
`spawn_deck_guardrails`) — players and vehicles can no longer slide off the
network. Each two-way boost overlay also has a 2.4-unit-wide, 7.5-unit-tall
collidable center divider, preventing momentum from carrying a player onto the
arrows facing the opposite direction. **Banked corner fillets** (`spawn_route_corner_fillets`): every
interior route waypoint is rounded with a quadratic-arc of short decks rolled
0.15 rad into the turn, so corners drive like curves instead of sharp joints.
**Vertical loops**: qualifying straights (>1800) spawn a drivable 360° loop
(radius 24, 22 segments, `spawn_banked_deck_segment` — full yaw/pitch/roll,
unclamped pitch) offset beside the lane so NPC traffic passes underneath.
Loop gates qualify on 1200-unit spans, and up to 28 three-position trick-ramp
candidates are distributed across spans longer than 900 units.
**Stunt park layer**: up to 18 long road spans receive an elevated luminous
`StuntGrindRail`, structural posts, a directional `SpringJumpPad` approach, and
alternating opposite-lane transfer springs. Springs trigger on contact for every
local player inside the pad on the same frame and preserve the authored race
direction; approach springs equip the Rocket Hoverboard. `RailGrindState` is
per player, projects onto the closest point of the line, selects forward/reverse
travel from incoming momentum, snaps the board above the rail, and preserves
speed through either an automatic end launch or a player-requested jump-off.
Sweeper overlays now use `spawn_banked_deck_segment` with turn-strength banking
up to 0.34 radians, making the large curves read and drive like futuristic
Mario-Kart-style velodrome turns rather than flat duplicate roads.
**Race and combo layer**: each settlement speed ring spawns four ordered
`StuntRaceGate`s and two `StuntRaceOpponent` hovercraft using the established
`NpcRoadVehicle` path controller. `StuntRaceProgress` is player-owned: only gate
zero starts a course, every intermediate gate must be crossed in order, and
three complete laps produce a finish before the course can restart. Race state
does not leak between the four local players. `StuntRunState` keeps rail time,
airtime, transfer count, stick-driven spin degrees, provisional score, and an
eight-times-bounded multiplier. A line banks only after a clean landing; board
visuals display the accumulated yaw/roll, the individual camera pulls back and
widens FOV with stunt intensity, and landing sends both owner-labelled score
feedback and a short owner-mapped `GamepadRumbleRequest` when rumble is enabled.
Normal airborne hoverboard travel retains its horizontal vector instead of
converging immediately on the low air target. No-input coasting has reduced
drag, and steering blends into existing momentum so mountain traversal reads as
surfing a large wave. A downward World-layer probe begins easing descending
velocity inside 4.8 units of terrain; contact then plays a short squash/pitch
landing pose. The rendered board is uniformly 10% larger before boost and
landing deformation.
**Clear corridor**: tree scatter skips anything within road half-width + 10 of
the network — the props vehicles used to crash into. Buildings are reactive to
the network: if the road centerline crosses a footprint, `spawn_building` emits
a **tunnel gateway** (two flank towers + header deck above a 10-unit clearance,
30-unit passage, oriented via the road distance-field gradient) or skips the
building entirely when too small to straddle.

Dodge: invulnerable during the `dodge_duration = 0.3s` window; costs 20 stamina; 0.5s cooldown.

Parry: 0.2s window on press; absorbs the next hit; 1.0s cooldown.

Stamina regens at 10/sec while not dodging.

## Character Studio (Human Generator)

**Plugin:** `CharacterStudioPlugin` (`src/character_studio/`) | **State:** `AppState::CharacterStudio`

The first pillar of the in-game asset-designer suite: a human character
generator with a Blender-like turntable viewport, built entirely from math
preset templates — no external model files. Entered from the **Robot Builder**
(the renamed hero armor designer on `AppState::CharacterDesign`) with **F6**;
dev boot: `STARFALL_STUDIO=1 cargo run`.

Pipeline (single source of truth, per the design doc):

```
keys/presets/randomize → CharacterSpec → PresetGenerators → CharacterPatch → generated meshes
```

- `spec.rs` — `CharacterSpec { sex, body, face, style }`; 26 normalized morph
  fields (eight body controls, including chest shape, plus 18 face controls),
  serialized as JSON (F5 save / F8 load → `assets/presets/humans/`).
- `generators.rs` — `PresetGenerator` trait; Body/Face/Skin/Hair/Outfit
  generators emit **named morphs** ("body_muscle", "face_jaw_wide", …), named
  material colours, and outfit slot contents into a `CharacterPatch`. Named
  morphs are the forward-compat seam: when a rigged shape-keyed `.glb` base
  exists, the same names bind to Bevy `MorphWeights` with no editor changes.
- `human_mesh.rs` — anthropometric mesh generator (lofted superellipse columns
  + ovoids; hip 0.52 H, shoulder 0.815 H, head 0.928 H): male/female bases,
  jaw/chin/nose/brow/cheek/eye/mouth face features, 9 hair styles, and three
  outfit layer systems — **Clothes** (shirt/sleeves/pants/shoes overlays),
  **Super Suit** (skin-tight recolor + emissive emblem/belt/trim), **Mecha
  Armor** (Megaman-style helmet + crest, chest core, shoulder spheres, right
  **buster cannon**, oversized boots).
**GUI (2026-07):** fully button-driven — no keyboard commands. Every control is
a clickable widget row (preset buttons, draggable morph sliders with stepper
buttons, `<`/`>` style cyclers) that also participates in **gamepad focus
navigation**: D-pad Up/Down moves rows, Left/Right adjusts or moves within a
row, South activates, East backs out; mouse hover pulls focus so both inputs
share one highlight. Viewport orbit: right-mouse drag + wheel zoom, or right
stick + triggers. The modeling pass adds a reset button to every morph/style,
an undo-safe RESET ALL action, live palette swatches, approximate physical
height/model information, and explicit FRONT/PROFILE/BACK plus FULL BODY/FACE
CLOSE-UP camera tools. Generated humans face -Z, so the default camera now
opens on the actual front. Controller West resets the focused field and
LB/RB jumps among preset, body, face, wardrobe, and saved-library sections.
The action panel uses explicit Body Presets, Face Presets, Edit History, Mesh
Modifiers, View Angle, Framing, Expression Preview, Preview Backend, and
Project headings. Soft/Heroic/Chibi/Rival seeds modify only facial parameters;
Neutral/Joy/Determined/Surprised preview semantic facial parts without changing
the serialized neutral recipe.

**Editable mesh kernel (2026-07):** imported render meshes now cross an
explicit compiler boundary into `EditableMeshDocument` before topology-aware
selection. The document owns `f64` positions, generational vertex/edge/
half-edge/face IDs, triangle half-edge adjacency, independent topology and
position revisions, validation, transactional vertex moves, and a deterministic
bake map from each render triangle back to its canonical `FaceId`. Bevy `Mesh`
remains the import/render representation rather than the authoritative editing
model. Canonical edge split, flip, and link-condition-checked collapse commands
now run as validated candidate transactions. They preserve unaffected element
IDs, invalidate removed IDs generationally, reject face inversion, apply
explicit semantic-region and crease transfer policies, advance granular dirty
revisions, and produce exact bounded undo/redo deltas guarded by document
fingerprints. Existing selection indices remain as a compatibility-facing
dense view; editor controls can adopt these commands incrementally without
changing saved GLB face assignments in one disruptive migration. Corner UV,
paint, and skin-weight stores still need to migrate into the document before
topology buttons are exposed for production assets.

**Wardrobe (2026-07):** independent slots, mix-and-match — Top (None/T-Shirt/
Long Shirt/Tunic/Jacket/Robe/Tank Top/Bomber/Moto/Blazer/Arcane Coat/Star
Knight Tunic), Bottom
(Underwear/Shorts/Pants/Skirt/Jeans/Cargo/Flared/Pleated Skirt), Footwear
(Barefoot/Shoes/Boots/Tall Boots/Sneakers/High-Tops/Loafers/Combat/Heeled
Boots), Hands (Bare/Gloves/Gauntlets), Armor
(None/Super Suit/Mecha Armor/Crystal Mecha/Dragon Mecha — armor overrides
wardrobe visuals), and independent Fantasy Flair (None/Star Gem/Moon Gem/Royal
Mantle/Arcane Halo/Mecha Wings). Primary/
secondary colours tint the whole outfit. A fitted base garment always encloses
the pelvis and upper thighs, so Underwear is a real visible option and outer
garments cannot expose skin through the glute/leg seams.

**Anime MVP appearance (2026-07):** the procedural head uses a larger cranium,
tapered jaw/chin, smaller nose, signed-power parametric almond eyes, fitted
lower lids, upper lash lines, iris
catchlights, cheek color, and a dedicated lip material for a broad 1980s cel
anime look. Face length, eye shape/depth, nose bridge/tip, chin width, and
sex-aware chest definition
increase silhouette control without changing the gameplay collider. Feathered,
spiky, bob, and side-ponytail hair join the original
styles. Clavicles, elbows, knees, calves, and grounded shoe soles improve the
full-body silhouette. Skin, eye, hair, cloth, denim, leather, sole, metal, and
emissive accents use distinct roughness/reflectance treatment, and palette
entries now have readable names across expanded anime and 80s color ranges.

**Versioned saves (2026-07):** SAVE VERSION never overwrites — each save writes
the next `human_vNNN.json` to `assets/presets/humans/`; the SAVED VERSIONS
panel lists every version with LOAD/DEL buttons (mouse or gamepad).

Because every vertex is generated in-engine, direct rigid `.glb` export of the
authored preview is available. Skinned export with weights, the canonical
skeleton, morph targets, sockets, and animation clips remains a follow-up.

The current editor is intentionally a constrained parametric modeler, not a
free vertex editor: this keeps characters compatible with gameplay collision,
animation, equipment slots, and local multiplayer. See
`docs/FEATURES.md` for its workflow, architecture boundaries, and the
recommended route toward an RPG-maker-grade creator.

## Anime Face System

**(2026-07-27)** Faces are authored, not fixed. The previous head had one
emissive sphere per eye, a fixed brow bar, a cuboid mouth, and no customization
beyond iris colour; `src/character/face.rs` replaces that with a full anime-RPG
face model, and every part of it is pure data and maths so the whole face is
unit-tested without spawning a rig.

**Layered eyes.** Each eye is five stacked shapes — sclera, iris, pupil,
catchlight, and a heavy upper lash line — solved by `FaceRecipe::eye_layout`.
The catchlight is placed up and outward with both eyes sharing one light
direction; without it an eye reads as a doll's, which is why it is an explicit
slider (`SPARKLE`) rather than a constant.

**Seven eye styles.** Round, Almond, Sharp (tsurime — upswept outer corner),
Gentle (tareme — downswept), Sleepy, Wide, and Fierce. Each carries its own
silhouette, corner tilt, lid heaviness, and iris ratio, so style sets a
character's personality while the sliders set their individuality. Five brow
styles (Soft, Straight, Arched, Thick, Thin) layer on top.

**Seven expressions.** Neutral, Happy, Angry, Surprised, Sad, Determined, and
Joyful resolve through `ExpressionPose` into brow angle and height, lid closure,
mouth curve, mouth opening, and iris dilation. Expressions are *derived*, so a
designer picks "Angry" instead of dialing eleven numbers, and gameplay can drive
the same poses at runtime. Brows mirror per side, which is what makes anger
converge inward rather than look quizzical; Joyful closes the eyes into the
classic `^_^` lash arc. The mouth is three segments so corners genuinely lift
into a smile or drop into a frown.

**Designer controls.** The Character Designer gained a **FACE** section: cycle
buttons for EYES / BROWS / MOOD and ten live sliders (eye size, spacing, height,
tilt, pupil, sparkle, lashes, brow height, nose, mouth). Every value is clamped
by `FaceRecipe::sanitized`, so a slider can never produce inside-out geometry.
Faces save with the character blueprint (`serde(default)`, so older records load
as a neutral round-eyed face) and each of the eight heroes ships with a distinct
authored face.

## Character Authoring

**Files:** `src/character/blueprint.rs`, `src/character/presets.rs`, `src/plugins/character_design_plugin.rs`, `src/plugins/player_plugin.rs`

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

Confirmed blueprints are kept on each player-select slot, saved in the rotating
schema-v5 campaign slots, and restored on load. When a player has not authored a
custom body yet, the runtime creates an upgraded default hero blueprint so
movement stats, collider proportions, visible body shape, and animation stride
all use the same data path.

The Hero Atelier editor shows the active GLB-inspired base model, marks when that model has custom edits, and exposes prefab export/import buttons for `assets/Characters/design_prefabs/*.json`. Base model selection remains visible while the player tweaks proportions or swaps individual body, arm, leg, shoulder, and helmet presets.

`CartoonCharacter` carries stride/agility metadata derived from `BodyRecipe`, and `CharacterPlugin` uses it to tune walk/run phase speed and limb swing. The character renderer also applies a visual ground lift so oversized cartoon feet sit on the terrain instead of dipping below the player capsule.

The full procedural mesh/rig/timeline editor is still future work; the saved schema is intentionally larger than the current renderer so it can grow without replacing save data.

---

## Hero Combat Identity

**Files:** `src/character/hero_roster.rs`, `src/plugins/player_plugin.rs`, `src/components/weapon.rs`

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

**File:** `src/world/robot_pets.rs` | **Runtime hooks:** `src/plugins/enemy_plugin.rs`, `src/plugins/save_plugin.rs`

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
| Tank / Scallarian siege tank | Tread Unit |
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

Future runtime work: controller-friendly garage/store UI, independent physical
vehicle collision/mount/destruction beneath the current owner-following 3-D
bodies, per-player passenger/driver UX, and store-built pet purchasing from
salvage.

---

## Settlement Builder And Economy

**Files:** `src/world/settlement_economy.rs`, `src/plugins/world_plugin.rs`, `src/plugins/save_plugin.rs`

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

## Explorable Building Interiors

**Files:** `src/components/world.rs`, `src/plugins/world_plugin.rs`

Selected downtown, industrial, residential, and authored-settlement buildings
use `EnterableBuilding` instead of one solid cuboid. The generator builds four
independent exterior walls around a real doorway, textured interior floors,
partition walls with wide room openings, sparse non-blocking furniture,
alternating warm/cool lighting, exterior windows, and an accessible roof.

Floors are assembled around a stairwell opening. Each flight uses one smooth
rotated collider so movement does not catch on decorative step edges; separate
non-colliding treads retain a readable stair silhouette. Accessible floor count
is derived from building height and capped at twelve to bound world-generation,
rendering, lighting, and physics costs in four-player split screen. Remaining
background buildings continue using the lightweight solid representation.

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

**Files:** `src/world/raids.rs`, `src/plugins/world_plugin.rs`, `src/plugins/save_plugin.rs`

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
- When a `RouteBlockade` record becomes active, one member per four-unit cadence
  (indices 0, 4, 8, …) is a procedural Scallarian siege tank and the intervening
  units remain Soldiers.
  Tanks sample terrain at their individual X/Z spawn point instead of inheriting
  the airborne raid offset. Other `RaidKind` enemy mappings are unchanged.

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

**Files:** `src/world/hacking.rs`, `src/plugins/hacking_plugin.rs`,
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

**File:** `src/combat/damage.rs`, `src/plugins/player_plugin.rs`

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

**Armor terminology:** `ArmorSet` is equipped protection and computes the
defense reduction applied to an incoming hit. `PlayerStats.armor` is the
separate rechargeable armor-durability meter shown in the HUD; it absorbs 70%
of the already-mitigated hit while health receives 30%. Do not use the
durability meter as equipment defense or apply either layer twice. Every player
owns `ArmorRechargeState`: an incoming `PlayerDamagedEvent` resets a three-second
delay, after which durability refills at 18 points per second up to
`PlayerStats.max_armor`.

**Derived cap contract:** `PlayerBaseStats` owns the stable level-one
`max_health` and `max_armor` authored by the character blueprint and adjusted
by the intrinsic hero profile. `PlayerStats.max_health` and `max_armor` are
effective caches, never progression inputs:

```text
health cap = base health
           + 10 × (level - 1)
           + equipped armor health
           + perk health
           + tech health

armor cap  = base armor
           + 0.8 × Aegis shield-defense bonus
```

`armor_plugin::sync_derived_player_caps` reconciles those caches and preserves
current fill ratios when equipment, perks, or upgrades change. Level-up changes
`level`, not the cached health cap. Permanent world armor rewards increase the
saved base armor. `PlayerSaveData.base_max_health/base_max_armor` persist the
bases as additive optional fields; older saves infer each base once by
subtracting known modifiers from the saved effective cap. Unknown historical
bonuses remain absorbed in the inferred base so migration never silently
reduces a saved cap.

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
sabre slashes (3.0). Players receive knockback too (2026-07-24): enemy melee
contact (the `EnemyConfig.knockback_force` stat, ÷100 to world scale), enemy
lasers and turret beams (2.2 along the shot), fireball splash and boss
shockwaves (falloff-scaled radial 3.5–5.5), and dragon breath (falloff-scaled
4.0). `player_knockback_intake` drains `Damageable.pending_knockback` into a
horizontal decaying shove that the motor integrates through the character
controller, so knockback respects walls instead of teleporting the player.

**Melee collision roles (2026-07):** canonical PlayerHitbox, EnemyHitbox,
Pushbox, and GrappleSensor layers now complement body/hurtbox and projectile
roles. Player combo and Star Sabre strikes query the Avian broad phase with the
PlayerHitbox profile, then apply the existing authored reach/arc rules and a
World-cover check. This removes the every-enemy candidate scan. Player combo
hitboxes remain live throughout the authored active window, follow the current
attack origin, and retain a per-move target set so each hurtbox resolves once.
Star Sabre slashes run the same persistent lifecycle as of 2026-07-24
(`BeamSabre.slash_phase`/`slash_hits`), including buffered trigger presses that
honor the authored recovery cancel window. Enemy melee is also volume-based now:
`execute_enemy_melee_hit` intersects a Player-layer sphere (the `EnemyHitbox`
collision role), supports a facing arc, respects World cover, and strikes
every player inside — replacing the unconditional closest-player range check.
Moves and sabre techniques may author `iframes` (seconds of invulnerability
granted at move start through the shared invulnerability timer; Comet Dash
ships 0.28 s because it claims the dodge input while the sabre is drawn).
Enemy melee is telegraphed: committing to a swing spawns the shockwave ring
over the strike zone for `EnemyConfig.attack_windup` seconds (scouts 0.22 s
up to brutes 0.5 s) before the volume resolves, and a committed swing spends
its cooldown even on a whiff — dodging the telegraph is the player's reward.
Losing the target mid-windup aborts the swing with a short 0.4 s lockout.
**Controller layout (2026-07-27):** the bare D-pad now swaps what RT fires —
left/right cycle primary weapons, up/down cycle the special through
`none → 0 → 1 → 2 → 3 → none` so the primary is always one more press away.
LB stays the global modifier (LB + D-pad selects traversal/flight mode), RB is
the primary attack (Star Sabre swing / physical weapon), and the displaced
utility actions moved onto Select + D-pad (up = enter vehicle, down =
interact, left = toggle first/third person, right = open map). The old Select +
D-pad direct special-slot chord was retired since it would double-fire with the
utilities; keyboard 7–0 still selects specials directly.

**Blaster feel pass (2026-07-28):** the primaries were built around stopping to
line up a shot, which fought the fast-movement game around them. Every primary
now steers toward a target, with per-weapon `tracking_strength` authored in
`assets/combat/moves.json` (0 = dumb-fire, 1 = locked on) and turn rate,
acquisition range, and acquisition cone all scaling from it — a strong tracker
finds targets more readily as well as turning harder. Strength is tuned against
how hard each weapon already is to land while moving: slow ordnance (Nova Orb,
Star Bubble Bombs) gets the most help, the hitscan-fast Rainbow Ray the least,
and Sparkle Fan pellets stay loose so a spread still reads as a spread rather
than a cone converging on one enemy. Bolts are 20–45% larger via an authored
`blast_scale` that touches the mesh only, so bigger never silently means
stronger. Shots also pulse as they fly — a scale-and-brightness swell applied
as a *multiplier* on the per-weapon stretch, so a long thin laser pulses
thicker without growing longer.

**Charging is no longer a gate.** It previously required a DariaCannon arm,
which hid the mechanic from most of the roster; now every blaster charges while
the trigger is held, and arm-cannon characters simply wind up 1.6x faster —
hardware as an advantage rather than a prerequisite. An unreleased charge bleeds
away instead of banking between bursts. The wind-up reads clearly: motes gather
*inward* toward the muzzle instead of spraying out, a core swells with the
charge, and at full charge a ring snaps out each frame as a "release me" tell.
The HUD's ammo line becomes a six-segment charge bar with a percentage while
winding up (ammo is unlimited and never urgent) and an unmistakable
`CHARGED — RELEASE` at the top.

**Weapon Forge (2026-07-28):** a modular weapon designer whose
governing rule is that **stats are derived, not typed in**. A design is a
physical description — grip style, guard, emitter, pommel, blade length and
width, colour — and damage, wave output, swing speed, and chain length fall
out of it (`src/combat/weapon_forge.rs`). A longer blade hits harder and swings
slower; a heavier pommel pulls the balance point back toward the hand and buys
that speed back; the emitter decides the wave behaviour (focused / piercing /
explosive / siphon); a reactor pommel grants fast technique recovery only when
the emitter has not already claimed the trait slot. The output is a
`BladeProfile` — the same type the shop sells — so a forged weapon equips
exactly like a purchased one and inherits the hilt mount and HUD readout for
free. Designs cannot cheat the shop's sidegrade rule: any build that comes out
better in every respect pays for it in swing speed, and a test sweeps all 3,072
part combinations to prove no design is a strict upgrade over the issued blade.
Validation separates blocking errors (no name) from advisory warnings (an
extended grip on a short blade), so odd experiments still save. Saves route
through the versioned project contract as `ContentCategory::Weapon` records
(`engine_tools::weapon_records`), inheriting the same atomic writes and
recovery snapshots as every other content type.

The authoring screen (`AppState::WeaponForge`, reached from the Project Hub or
`STARFALL_WEAPON_FORGE=1`) puts a live 3D preview beside the numbers. The
preview is assembled from the *same* measurements the derivation reads — grip
length, blade length and width — rather than separate display constants, so the
picture can never drift from the maths: lengthen the blade and you watch it grow
and watch the swing speed drop in the same frame. Tool windows cover PARTS
(cycle grip / guard / emitter / pommel / colour, step blade length and width),
PERFORMANCE (damage, wave, swing speed as percentages of the issued blade, plus
chain delta, balance point, physical length, trait, and estimated value, with
blocking problems called out before you try to save), and PRESETS & SAVE (four
starting points — Duelist, Warblade, Wavecaster, Leech).

**Star Sabre hilt (2026-07-28):** the sabre is now a physical weapon held in
the character's hand. Previously the blade was positioned by guessing where the
hand was — `player.translation()` plus fixed offsets — so it ignored the animated
arm entirely, and the hand posed as if gripping something invisible. A `SabreHilt`
(grip, blade-tinted emitter ring, pommel counterweight) is parented to the
character's real hand entity: the rigged `RightWrist` joint when the character has
a skeleton, otherwise the cartoon `RightHand` part. Because it is a child in the
transform hierarchy, the weapon inherits every arm pose, swing, and
designer-edited body proportion for free. The energy blade is in turn a child of
the hilt, so its transform is purely local — it projects out of the emitter and
ignites by extending along that axis instead of appearing at full length.
Sheathing does not delete the weapon — the hilt moves to the pelvis joint (or
belt mesh) and hangs on the hip, so a sabre is visible equipment whether drawn
or stowed; only a drawn hilt ignites a blade. Heavy committed techniques
(cyclone, spiral, meteor pound) bring the off hand onto the hilt for a
two-handed grip, while the mobile verbs stay one-handed and a thrown blade
naturally gets no second hand.

**Star Sabre blades (2026-07-27):** the shop's Weapons category now sells
*blades*, and equipping one actually restats and recolours the sabre —
previously `equipped_weapon` was saved but never read, so buying a weapon
changed nothing. Eight profiles live in `src/combat/blades.rs`: the never-sold
**Standard Issue** (exactly neutral, so "nothing equipped" plays as before)
plus seven purchasable hilts — Solar Sabre, Crimson Edge (huge hits, short
chain, slow swing), Emerald Lash (longer faster chain, lifesteal), Violet
Tempest (piercing waves at 1.55x), Gold Regent (light relentless swings,
techniques recover 32% faster), Frost Vigil (every wave detonates), and Void
Requiem (devastating and hungry, slow to recover). Blades are **sidegrades,
not a ladder**: a test fails the build if any blade is a strict upgrade over
the starter. Multipliers compose on top of `BeamSabre::set_level`, so blade
choice and sabre progression both keep working, and the chain can never drop
below one slash. Each blade carries its own aura colour (built once at startup,
indexed by colour) while the core stays hot and near-white for readability, and
the HUD leads with the blade name and its special behaviour.

**Star Sabre moveset (2026-07-27):** techniques resolve through one pure
`resolve_sabre_technique` predicate (stance + input → verb), so the control
scheme is unit-testable and a bare swing always falls through to the base
slash chain — the fighting moveset never changes under the player. Heavy is
Cyclone Slash grounded / **Spiral Slash** airborne; dodge is Comet Dash
grounded / Meteor Pound airborne; holding **up** + attack is the launching
**Rising Slash** and **down** + attack is the **Sabre Throw**, a piercing
returning blade that rides the normal projectile pipeline (so world cover and
sweeps apply). All six are authored in `sabre_techniques` in
`assets/combat/moves.json`, now including `rise_speed`, `throw_speed`, and
`throw_lifetime`. The starter kit ships wave + cyclone + dash + pound +
rising + spiral; the **Sabre Throw Blueprint** and **Tempest Wave Core**
(which adds a third-slash wave burst on top of the second-slash wave) remain
exploration relics, alongside the elemental/legendary gems that recolor the
blade and stack damage. The slash pose is driven by the real phase machine
(`sabre_swing_position`) rather than a free-running sine: the blade winds back
through startup, snaps across the active window, and settles through recovery,
alternating sides per slash so a chain reads as a combo.

**Hoverboard tricks (2026-07-25):** the deck's rendered pose is damped rather
than recomputed raw each frame — the contact normal (which pops between
trimesh triangles and vanishes on brief airborne frames), carve bank, nose
pitch, and trick spin all ease toward their targets, and the old hard
`speed > 0.35` bank threshold is a smoothstep, removing the flicker when
cruising near that speed. Riders stand in a real sideways skate stance
(`board_stance_pose`): torso yawed onto the deck, knees bent, arms out and
counter-balancing through carves, with distinct poses per trick category —
grab folds over the deck and reaches a hand down, flip tucks tight, grind
squats wide, manual leans back over the tail. The same stance drives both the
cartoon-part and joint-rig paths so imported characters ride identically.
Tricks themselves are authored data in `assets/combat/tricks.json`
(`src/combat/tricks.rs`): 23 shipped tricks across Grab/Flip/Grind/Manual/Lip plus
multiplier-gated Specials (The 900, Christ Air, Kickflip McTwist, Darkslide).
While the board is out it claims the combat buttons — flip on `melee_light`,
grab on `melee_heavy`, grind on `dodge`, manual on `parry` — with the stick
direction selecting the variant, so melee, evasion, and sabre techniques do
not also fire. B/East remains overdrive off rails, but once rail-bound that
input belongs exclusively to Grind and cannot trigger both mechanics. Air
tricks must be landed before their hold expires or the
rider bails and loses the combo; scoring applies the spin bonus (180° = 1.5x,
360° = 2x) and Tony Hawk repeat decay (2nd ÷1.33, 3rd ÷1.5, 4th+ ÷2). The
board HUD line shows the live trick, pending combo, and multiplier.

Co-op players shoulder past each other (pushbox-lite): inside ~1.1 units on
the same level, `player_pushbox_separation` feeds a gentle overlap-scaled
shove (up to 4.5 u/s) into the shared external-shove channel, so separation
resolves through the character controller and fades on its own; a vertical
gap over 2.2 units (jumping over a teammate) disables it.

**Hit reactions (2026-07, EC2 first slice):** landed hits now produce bounded
**hitstop** (`src/combat/hitstop.rs`: 28–90 ms scaled by damage, max-not-sum, drains
on real time; simulation chains skip via `run_if(hitstop_inactive)` while
cameras/UI keep running), plus the `src/combat/feedback.rs` layer: enemy
**flinch** (AI/attack skip `With<Flinch>` entities), impact **hit-flash** orbs,
**death dissolve** (shrink-out over the despawn window), split-screen-aware
floating **damage numbers** (per-camera `world_to_viewport`, capped 32),
proximity-scaled outgoing **camera shake**, and the rumble hook. See
`docs/guides/combat-feel.md` for the pipeline and tuning knobs.


---

## Weapons

**Component:** `WeaponInventory` (slots 1-6), `SpecialWeaponInventory` (slots 7-0), `BeamSabre`

| Slot | Name | Type | Auto | Notes |
|---|---|---|---|---|
| 1 | Starlight Popper | Pistol | No | Unlimited ammo |
| 2 | Comet Stream | Rifle | Yes | Unlimited ammo, fast fire |
| 3 | Sparkle Fan | Shotgun | No | 10 pellets, wide spread |
| 4 | Nova Orb | Rocket | No | Explosive, r=6.5 |
| 5 | Rainbow Ray | Laser | Yes | Unlimited ammo, high-speed projectile |
| 6 | Star Bubble Bombs | Grenade | No | Arc trajectory, r=8.5 |

| Special | Slot | Cooldown | Ammo |
|---|---|---|---|
| Homing Star | 7 | 2.0s | Unlimited |
| Tri-Star Burst | 8 | 1.5s | Unlimited |
| Moon Bubble | 9 | 4.0s | Unlimited |
| Sprite Turret | 0 | 10.0s | Unlimited |

Primary and special ammo counters are retained for save compatibility but no longer gate or decrement during play; cooldown/fire rate is the only firing limit. Player and enemy projectile collision sweeps the full per-frame travel segment through typed Avian layers. Candidate impacts resolve nearest-first, piercing player shots continue through hostile sensor hurtboxes but stop at world geometry, and explosive shots detonate at the resolved surface. Radial player explosives, enemy fireballs, and boss shockwaves require World-layer line of sight to each target before applying falloff, so walls provide real blast cover. Aim assist targets living enemy torsos up to long combat range, uses a broader reticle cone, and fully converges while LT/RMB aim is held. Flying and city-spy drones target their center-rooted hurtboxes instead of adding the grounded torso offset and receive a modestly wider acquisition cone. Tracking weapons display a pulsing world-space lock ring: gold for Homing Star and cyan for magic beams. Characters designed with `DariaCannon` arms can hold and release a charge shot. Charged hits resolve through the shared critical path at 1.5x damage and add a damage/element-scaled hot-pink impact core. Heroes with magic power 1.10 or higher add fast tracking steering and a beam trail to primary shots.

**Star Sabre / Beam Sabre** (`BeamSabre`): controller ownership follows the
assigned `PlayerIndex`. The base Saber is starter equipment; toggle `T` or
controller LB+North/Y, with LT+North/Y, Guide, and L3+R3 as fallbacks. RB/LMB
performs the animated slash while active; RT remains dedicated to the selected
ranged weapon even while the Saber is drawn. Its second combo slash launches
one small starter wave. Normal beam weapons use that same player's LT aim and
RT fire inputs. Saber level, Solar Sabre Glyph, and Beam Capacitors combine into a
bounded wave tier: upgrades grow the projectile mesh, damage, and speed; add up
to four spread waves; add piercing at tier 3; add another emission on slash four
when the combo is long enough; and add splash at tier 5.

The Solar Sabre Glyph upgrades the starter wave rather than unlocking the base
weapon or withholding its first projectile. Save-backed `UpgradeLedger.relics` holds extensible gem/blueprint ids per
player. Cyclone Slash (heavy/L3) performs a full radial cut; Comet Dash (B/East,
or Q on keyboard) performs two advancing cuts; Meteor Pound turns that input
into an airborne radial slam. While the Saber is drawn, an input owned by an
applicable technique is consumed exclusively: heavy never also buffers the fist
Heavy chain or airborne stomp, and dodge never also triggers the evasive roll,
platformer roll, flight dash, or wall/climb bail (holster to use those verbs).
Technique tuning (multipliers, reach, cooldown, dash/plunge speeds) is authored
as `sabre_techniques` in `assets/combat/moves.json` alongside the slash chain's
frame data, which now runs the same persistent Startup→Active→Recovery hit
window as hand melee. Solar Fire, Storm, Frost, and Void gems stack
damage and select supported damage types, while the Legendary Starheart Gem
adds a larger multiplier. Eight initial objects are distributed around the
Great Scientist sites as ordinary `HiddenReward` pickups. Collection grants the
campaign discovery to the active party and mirrors ownership into each player's
save record; later-created players inherit already-discovered Saber relics.

The per-player HUD has dedicated traversal and Saber status lines. Hoverboard
status distinguishes ready, recharge percentage, and active overdrive. Saber
status shows the live named technique first, otherwise the owned wave/spin/dash/
pound set, elemental-gem count, and Legendary Starheart ownership.

Saber presentation uses shared projectile meshes and material handles rather
than creating materials per attack. Cyclone emits crossed rotating rings,
Comet Dash emits three directional streaks, Meteor Pound emits an expanding
ground ring plus vertical impact column, and energy waves emit a short muzzle
ring. Solar Fire, Storm, Frost, and Void choose distinct shared colors for both
technique effects and waves; Starheart adds a second pink tilted ring. Transient
Saber VFX are capped globally at 24 entities so simultaneous four-player use
degrades by dropping excess decorations while attacks and damage still resolve.
F11 counts these entities in total combat VFX and displays the dedicated Saber
count against its cap. Player spawning seeds campaign-discovered Saber relics
into the joining player's save-backed ledger; automated coverage verifies that
existing ownership remains idempotent and unrelated discoveries are excluded.
The persistent blade uses two shared additive energy materials: a broad blue
aura around a narrower near-white HDR core. Its longer visual is centered from
the hand along the animated blade direction, and standard slash radius/offset
were increased with it so the rendered weapon and gameplay reach agree better.

In `DungeonCrawlState`, hand melee and Star Sabre attacks prefer movement/facing direction over camera forward, use wider hit arcs, and Star Sabre fires short ground waves even before the late dual-wave rank. This keeps castle interiors readable from the single shared top-down camera.

Special tools are selected directly with keyboard `7`, `8`, `9`, `0`, or cycled
with controller D-pad Up/Down, then fired with RT/LMB. The cycle includes
`none`, so one more press returns RT to the primary weapon; D-pad Left/Right
also selects the previous/next primary immediately. The selected special's name
replaces the primary weapon name in the HUD.

**Heavy Water special continuation (2026-08):** all four tools now consume
their saved `SpecialWeapon.level` instead of applying damage scaling alone.
Authored hidden-city rewards promote Homing Star, Tri-Star Burst, Moon Bubble,
and Sprite Turret to at least level 2.

- Homing Star remains one tracking missile at levels 1–2 and launches three at
  level 3.
- Tri-Star bolts gain capped tracking at level 2 and stronger tracking at level
  3, while preserving the normal attributed projectile/damage path.
- Moon Bubble level 2+ splits once on detonation into four cardinal explosive
  children. Children cannot recursively split.
- Sprite Turret deploys one persistent combat drone per owner instead of a
  one-frame/static shot. It orbits and follows its player, chooses the nearest
  line-of-sight target within 25 units, and fires owner-attributed projectiles.
  Duration is 30/45/60 seconds and fire cadence improves at levels 2–3; level 3
  also displays a player-following shield aura without adding a second damage
  reduction formula. Redeploying replaces only that owner's prior drone.

With the Star Sabre drawn, hold aim and press fire (RMB + LMB, or LT + RT) to
trigger the owner-scoped **Mega Beam Cannon**. It emits a 220-unit, radius-5
layered beam for 1.4 seconds at 1,800 base damage and launches twenty
deterministically arranged homing explosive seekers (90 base damage,
5-unit blast radius). A World-layer center ray clips both the rendered beam and
its damage capsule to the same maximum endpoint, while every contained enemy
must also pass its own line-of-sight cover check before the one allowed hit.
Each player has an independent six-second cooldown and a one-beam/one-volley
population cap; Tank mode owns primary fire and therefore suppresses the combo.
Seekers reuse the shared projectile/tracking path without per-frame
trail-entity churn.

---

**Faction boss controllers (2026-07):** every boss faction now has a distinct
brain, keyed at `spawn_named_enemy`:
- **DragonBoss** (DragonRoyalty/Exile) — flying orbit, fireball spreads,
  breath cone, ground slam (unchanged).
- **RiftBoss** (Scallarians + default) — hover-weave, teleport blinks with
  portal flashes on a shrinking ring, rift-laser volleys, portal
  reinforcements from phase 2 (drones, then soldiers), and an 8-way laser
  nova on phase-3 blinks.
- **MechBoss** (CorruptedHuman/WizardScientist) — grounded standoff strafing,
  laser barrages, telegraphed committed charge dash ending in a damaging
  shockwave, and cycling invulnerable reactor-shield windows from phase 2.
All share `boss_phase()` thresholds (66%/33% HP) and are excluded from generic
chase AI/attack; hitstop gates them like all sim systems.

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
| Tank | 600 | 45 | Slow | Procedural Scallarian siege tank; defense 18, 28-unit stand-off, explosive shell, 120 XP |
| SpikeAlien | 80 | 20 | Fast | Aggressive Scallarian spike alien, 20 XP |
| Hybrid | 1000 | 40 | Med | Boss-tier Scallarian rift champion, 200 XP |

Dragon-faction boss fights add `DragonBoss`: large scaled boss bodies orbit above the closest player while leashed to their arena, advance through health phases, fire volleys, breathe cone fire, and create slam shockwaves.

Flying-drone wings can also trigger the shared boss camera when at least three nearby drones are active. Combat craft now carry a `DroneVariant` with distinct procedural silhouette, durability, flight envelope, and volley read: Scout fires one centered bolt, HeavyFighter fires three alternating bolts 120 ms apart, and SiegeGunship fires four simultaneous laterally spread lasers. Authored wasteland patrols deploy a six-craft 3 Scout / 2 HeavyFighter / 1 SiegeGunship formation outside the starting safe radius. This remains the lightweight ship/drone encounter hook for future aerial mini-bosses, boarding fights, and dragon-lair alarm rooms.

`EnemyType::Tank` does not use generic contact melee. Its kinematic procedural tread/hull/turret/barrel craft chases slowly, parks inside a 28-unit envelope, and fires a real `Shell` projectile with 5.5-unit splash on a 3.5-second cadence. Active RouteBlockade raids mix one in every four units. Both raid and chapter spawners sample the exact local terrain surface for tanks. Chapters 4 and 13 add `DimensionalAlien` tank groups; campaign tests ensure tank encounters are not assigned to `DragonRoyalty` or `DragonExile`. A runtime tank kill grants ordinary Tread Unit robot salvage through the canonical enemy-label mapping; rare-context salvage is not part of the kill-reward path.

Everest `MountainSpiderMech`s retain their eight-leg terrain-following patrol, but their attack is now ranged artillery rather than the old generic melee sphere. At 32 units they stop, aim at the nearest player, and launch two target-bearing homing splash missiles every 2.8 seconds. Ordinary lasers, tank shells, and homing missiles share the enemy-projectile collision/damage pipeline; only missiles pay steering-target lookup cost.

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

Every chapter now has one optional cave dungeon to discover. A single authored
`SECRET_CAVE_LOCATIONS` table supplies world generation, chapter discovery,
and the fast-travel map so cave coordinates cannot drift between systems.
`WorldPlugin` spawns the physical cave systems when the world is generated:

- Stone and dark-rock tunnel entrances.
- Walkable tunnel floors, side walls, back chambers, and colliders. Tunnel
  stations sample the final piecewise-planar terrain collider, enforce bounded
  spacing/grade/clearance, and spawn overlapping pitch-aligned floors with
  matching walls. A broad blended cave inset runs after road terracing so road
  shelves cannot push a mountain wedge back through the dungeon. The final
  tunnel station and chamber front share an exact surface position.
- Brushed-metal ribs, glass panels, glowing crystals, and point lights so the caves keep the ancient/new style.
- Two small moving platforms inside each chamber.
- A `WorldAnchor` named `secret_cave_ch01` through `secret_cave_ch14` at each cave's inner discovery point.
- A monumental `AncientCaveGate` at every mountain entrance: 13.5-meter stone
  pylons, a deep lintel, luminous crown runes/keystone, recessed cave mouth,
  and animated collision-backed stone doors. These are canonical world-space
  entrances; cave fast travel remains an earned optional shortcut. Interact
  gathers up to four players, switches all viewports to one top-down camera,
  remaps movement to fixed dungeon axes, and enables the wider dungeon melee
  and Star Sabre arcs.
- A gate-keyed glowing return portal inside every cave, dragon lair, and Great
  Scientist temple. A separate `E` / D-pad Down press returns all four players
  to distinct safe exterior slots, clears their travel momentum/boat state,
  and releases the shared camera. The activation-frame arming guard prevents
  the gate-opening press from immediately exiting again.
- A reusable `DungeonRoomZone` / `DungeonRoomPortal` graph. Mountain caves
  currently generate Entrance Gallery, Traversal Tunnel, and Star Chamber
  nodes connected by two explicit adjacency edges. `DungeonRoomState` tracks
  the active gate/room plus visited and cleared room identities for the session.
  Party-centroid nearest-room selection uses an adjacency check and hysteresis;
  entering a room updates `DungeonCrawlState.focus/radius`, producing stable
  shared-camera moves without skipping a graph edge or bouncing at a midpoint.
- Two chapter-scaled Gauntlet-style enemy waves per cave, plus broad alternating
  jump pads and the existing vertical moving platforms for Mario-3D-World-style
  cooperative traversal that remains readable on one screen.
- Star Chamber waves use `DungeonEncounterEnemy` ownership rather than global
  enemy counts. `DungeonEncounterDoor` lowers after the chamber wave spawns,
  remains sealed while any owned enemy lives, then opens permanently for the
  session and reveals the matching `DungeonEncounterReward`. Unrelated enemies
  elsewhere in the cave or world cannot incorrectly hold the seal closed.
- A glowing chamber checkpoint. Once the matching cave discovery is saved,
  Chapter Select enables a purple `C` map button at the real cave coordinates.
  Selecting it transports every active player to the checkpoint and activates
  shared-screen dungeon mode immediately.

Chapter scripts use `PlaceSecretCave` to spawn a green discovery beacon at the matching anchor. Collecting it stores the cave id in `ChapterProgress.discoverables`, sends a UI message/radio line, prevents that chapter's cave beacon from respawning after save/load, and permanently unlocks its cave fast-travel point. The pending destination is transient; the discovery flag uses the existing save schema.

## Everest Range Fast Travel

**Files:** `src/chapters/mod.rs`, `src/plugins/world_plugin.rs`, `src/plugins/player_plugin.rs`, `src/plugins/chapter_plugin.rs`, `src/plugins/ui_plugin.rs`

The campaign map is now the 200 x 200 mile Everest-range heightmap world. `EVEREST_RANGE_MILES` and `EVEREST_RANGE_WORLD_SIZE` define the shared scale (`20_000` world units at `100` units per mile), and `chapter_map_locations()` defines one named map location per chapter, including the `WorldAnchor` id, region label, landmark, X/Z position, and party-facing yaw.

`WorldPlugin` turns those records into visible in-world chapter beacons and matching `WorldAnchor`s after terrain generation. It also adds range-scale snowfields, glacier streams, alpine forest pockets, sci-fantasy outposts, waypoint crystal lines, and dragon-lair silhouettes. All anchors and large props sample the same `terrain_surface_y()` path used by the terrain mesh/collider, so fast travel, visuals, and collision agree.

Chapter select renders those locations as the Everest Range fast-travel map. Keyboard chapter picks and clickable unlocked map markers both set `CurrentChapter.id`; entering `Playing` spawns or moves players to the matching heightmap beacon and clears travel momentum/boat passenger state.

## Exploration Settlements

**Files:** `src/chapters/mod.rs`, `src/world/discussion.rs`, `src/plugins/world_plugin.rs`, `src/plugins/ui_plugin.rs`

`map_settlements()` defines optional cities, villages, harbors, and outposts that fill the 200-mile map between chapter anchors. `WorldPlugin` samples local terrain relief for each settlement and assigns a grounded, terraced, or sky-district layout profile. Plazas, segmented roads, building pads, landmarks, discussion NPCs, caches, anchors, and spy activity all use that shared floor profile so interactables and visuals stay aligned with the heightmap. Mega cities now sit on large walkable floating platforms with four world-cardinal ground ramps independent of facade yaw, guide lights, tower pads, and underside lift columns; rough mountain edges can spawn inset stone/metal gates with dark openings and threshold paths. Chapter select renders matching non-clickable map markers: `C` city, `V` village, `H` harbor, and `O` outpost. Markers turn green after their cache reward id is collected.

Each settlement also has one `DiscussionNpc` near its plaza. Press `E` / D-pad Down near that NPC to open the discussion panel. Dialogue scripts live in `src/world/discussion.rs`; each line may reference an MP3 under `assets/voice/...`, and the HUD spawns that clip once when the line appears. Missing voice files are acceptable during prototyping, but final recorded lines should keep the documented paths or update the script.

City settlements are peaceful mega-city hubs protected by orbiting `FreePeopleGuardianShip` patrols from the Free Peoples of Earth. They also hide a small counter-spy activity: Cloudrail City and Switchwork Borough each spawn two non-combat `CitySpyDrone`s. Spy drones carry an EnemyHurtbox sensor despite using kinematic patrol motion, so ordinary projectiles drive the shared enemy-damage, hit-flash, and floating-number path. Shoot them down, then collect the spawned `SpyData` beacon to earn save-backed credits, XP, and armor rewards.

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
| Solar Sabre Observatory | Solar Sabre Glyph | Upgrades the starter second-slash wave into a wider multi-wave and also boosts hand/sabre-style pressure, Rainbow Ray, and Tri-Star Burst |
| Nova Missile Foundry | Nova Missile Matrix | Boosts Nova Orb missile damage, explosion radius, ammo, and Homing Star capacity |
| Aegis Frame Archive | Aegis Armor Frame | Grants armor rewards and improves traversal reliability through stronger snap/step and extra wall-jump charge |

Legacy temple effects are reapplied by `apply_scientist_temple_progress()`.
Saber gems and blueprints additionally persist per player through the
serde-defaulted `UpgradeLedger.relics` field, preserving old-save compatibility.

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

**File:** `src/combat/perks.rs` | **Resource:** `PerkTree`

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

**File:** `src/combat/upgrades.rs` | **Resource:** `UpgradeLedger`

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

Save files: three rotating campaign slots in
`dirs::data_dir()/starfall_i` (or the documented working-directory fallback),
plus atomic settings. Legacy working-directory saves migrate non-destructively.

- **Autosave**: every 30 seconds while `Playing`.
- **Manual save**: F5 key.
- **Schema**: v5 with a monotonic generation; loading selects the valid slot
  with the highest generation, skips corrupt slots, migrates older schemas, and
  rejects unsupported future versions.
- **Crash reports**: timestamped, path-sanitized logs share the platform data
  root and retain the newest five files.
- **Load**: chapter progress is hydrated on startup; the selected record is
  cached for the first `OnEnter(Playing)`, avoiding a second three-slot scan and
  repeated global-state hydration. Player components are applied only after
  player spawn. Schema-v4 24-slot inventories expand to 100 slots without
  reordering or dropping stacks.
- **Feedback and diagnostics**: chapter travel reveals a pre-spawned loading
  card before the state transition. World startup logs elapsed generation,
  standard-material count, and mesh counts for base, city, traversal,
  mountains, range, nature, and landmarks so save I/O and world stalls are
  distinguishable.

Saved shared fields include `wave_number`, completed chapters, discoverables,
recruited companions, recovered scientist relics/fragments, perk ranks, robot
pets, tech upgrades, character blueprints and part loadouts, crafting queue,
the Heavy economy/Bio/combat/region/vehicle continuation record and Bio clock,
plus campaign/player-scoped Heavy invasion/manual state.

Saved per-player fields live in `players[]` records keyed by `player_index`:
level, experience, credits, health, stamina, armor values, inventory stacks,
primary/special selection, armor infusion, quick item, and traversal ride. Older
records default the new loadout fields and top-level stats remain accepted for
legacy migration.

Save ownership tests cover per-player record round-trips, legacy top-level stat hydration, matching by `player_index` instead of record order, and clamping loaded runtime values into valid health/stamina/armor ranges.

## Companions

**Plugin:** `CompanionPlugin` | **Component:** `Companion`

Companions now carry an `owner: u8` matching `PlayerIndex`.

- Default medic drones and pets spawn once per active player when play starts.
- Follow and heal behavior resolves the owning player instead of using a single-player query.
- Combat assist targets enemies near the companion and near the owning player.
- Companion recruit beacons assign the new ally to the player who collected the beacon.

---

## Music Deck And Modular Action Audio

`MusicPlayerPlugin` is a bus separate from combat/reward SFX. At startup it
scans `assets/user_music` (or `STARFALL_MUSIC_DIR`) for MP3 files, reads them
into Bevy `AudioSource` assets, and plays the sorted playlist through a single
tracked `MusicPlayback` entity. Natural track completion advances the deck;
manual previous/next despawns only that music entity. Live sink volume is
`master_volume * music_volume`, so changing music settings never mutes SFX.

`F6` opens the player-facing deck. While open it owns P1's modal input capture:
Left/Right or D-pad select tracks, Space/A pauses, `S` toggles deterministic
shuffle, `R` rescans both custom libraries, and Escape/F6 closes. The overlay
shows track status, both source folders, and the number of assigned action SFX.

`SfxPlugin` still synthesizes all ten arcade sounds at startup. A separate
`ActionSfxRegistry` reads `assets/user_sfx/actions.json` (or
`STARFALL_SFX_DIR`) and maps validated modular IDs to MP3 one-shots. Standard
combat readers request IDs such as `weapon.fire`, `melee.slash`, and
`combat.parry`; an assigned MP3 wins, while a missing/invalid/unreadable clip
falls back to the original synthesized handle. Generic engine/gameplay modules
can emit `ModularActionSfxEvent` for additional manifest IDs. Paths must remain
relative to the configured SFX directory, IDs are bounded to safe ASCII tokens,
and custom one-shots play at original pitch on the SFX/master volume bus.
Unassigned modular actions now select a synthesized fallback instead of going
silent. The current traversal/Saber ids are `hoverboard.overdrive`,
`hoverboard.land`,
`sabre.cyclone`, `sabre.comet_dash`, `sabre.meteor_pound`, and `sabre.wave`.

See `docs/FEATURES.md` for the player workflow and manifest schema.

## Procedural World

**Module:** `src/lsystem/` | **Plugin:** `WorldPlugin`

- Terrain heightmap via deterministic layered waves/ridges plus `assets/terrain/everest.png` mapped across the full 200 x 200 mile range; seed from `GameSettings.world_seed`.
- Current heightmap import is full-map scoped: `assets/terrain/everest.png` is loaded once through Bevy image decoding, sampled bilinearly with a smoothing transform, faded at its outer edges, and blended into the deterministic terrain so collision and visuals share the same height function.
- Terrain visuals use Bevy mesh vertex colors as the current texture layer: low grass, alpine meadow, exposed rock, darker strata, blue ice, high snow, and shared mountain-route trail tint are derived from height, slope, seeded detail noise, and route influence.
- `mountain_routes()` is the shared source for path color corridors, light guide waylines, carved pass relief, and spawned slab/stud path pieces. Keep future chapter routes on this helper so map art, terrain shaping, and navigation markers stay aligned.
- Outer districts, spaceports, trees, crystals, mountains, and authored anchors sample the terrain surface so upgraded props sit on the generated ground rather than the old flat plane.
- Decorative trees via L-system string rewriting (`lsystem/mod.rs`) + 3-D turtle interpreter (`lsystem/turtle.rs`).
- City-safe terrain is clamped to the invisible gameplay floor, keeping terrain visuals and collision from diverging below Y=0.
- Lush procedural nature adds forest pockets, water gardens, reeds, flowers, shrubs, mossy rocks, and darker stone outcrops.
- Repeated road, facade, rooftop, trim, forest, bridge, and moss geometry shares
  cached unit cube/sphere/cylinder/cone meshes and expresses authored dimensions
  through entity transforms. Visual sizes, colliders, entity layout, trigger
  positions, and sway scales remain unchanged while render-asset creation is
  bounded. A fixed-seed M3 Pro optimized-development smoke reduced initial world
  assets from 108,755 meshes to 9,257, queued in 0.80-0.90 seconds.
- Grass is a streamed, GPU-driven field (`src/plugins/world_plugin/grass.rs` + `assets/shaders/grass.wgsl`) rather than per-blade entities:
  - Every blade in a 16 m chunk is baked into one merged mesh, so a chunk costs one entity and one draw call. Per-blade sway phase, tint, and the terrain colour beneath the blade ride in the vertex stream (`UV_0` = width/height, `COLOR` = ground colour + blade hash).
  - Chunks stream around `PlayerCamera` (falling back to `Player`), bake at most 2 fresh meshes and 24,000 candidate vertices per frame, and cache meshes by coordinate so re-crossing old ground is free. Placement is a pure function of `(world_seed, chunk)`, so an evicted chunk rebuilds identically.
  - Three LOD tiers (Near/Mid/Far: 4/3/2-segment blades, denser to wider) have overlapping distance windows; the shader dithers each tier in and out so density changes read as a dissolve, not a ring.
  - Blades only grow on ground the terrain agrees with: slope, altitude band, city clearance, mountain-route corridors, lake basins, and the coastline all mask density, and a two-octave value-noise clump mask breaks the field into tufts.
  - The shader adds gusting multi-octave wind (waves travel along the wind direction), wrapped diffuse, a subsurface-scattering approximation for back-lit rim light, tip sheen, root occlusion, and blade-width curl shading. Chunks carry explicit `Aabb`s for frustum culling and are `NotShadowCaster`.
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

**Files:** `src/character/presets.rs`, `src/plugins/character_design_plugin.rs`, `src/plugins/player_plugin.rs`

- Player slots store optional outfit/accent/hair preset indices plus accessory toggles.
- Preset indices are normalized before preview, saving, and player spawning, so stale save data cannot panic by indexing outside the palette.
- The designer preview camera faces the character's front side, while the same spawned character parts are reused by the in-game player model.

---

## Vehicle System

**Plugin:** `VehiclePlugin` | **File:** `src/plugins/vehicle_plugin.rs`

`VehicleState` uses two enums instead of boolean flags:

- `GroundMode`: `None`, `Motorcycle`, `Tank`, `GiantMech`
- `AirMode`: `None`, `Jet`, `Ship`
- `JetBikeMode`: `Ground`, `Flight` (one hybrid vehicle owning exactly one of
  the above physics profiles at a time)

**J / Enter Vehicle** toggles the available single-mode vehicle, handles nearby
boats, or cycles an assembled Jet Bike through ground → flight → off. With no
active robot assembly, owning both loose motorcycle and jet blueprints provides
the same fallback cycle; a selected Car/Motorcycle/Tank/Mech assembly always
keeps its authored ground profile. **M / Open Map** is not consumed by vehicles.
Access is gated by either a `PlayerLoadout` blueprint or
`RobotPetCollection.active_assembly`:

| Assembly form | Ground mode | Air mode | Runtime body |
|---|---|---|---|
| Car / Motorcycle | Motorcycle | — | Procedural all-terrain vehicle |
| Jet Bike | Motorcycle | Jet | Existing wheel/wing dual-mode Jet Bike |
| Tank | Tank | — | Procedural treaded siege tank with articulated turret/barrel |
| GiantMech | GiantMech | — | Procedural biped giant mech |
| SpaceJet or `jet_blueprint` | — | Jet | Procedural space fighter |
| SpaceShip / MegaShip | — | Ship | Procedural starship (larger ship profile) |

The Heavy Water continuation bodies are party-owned presentation entities. One
body follows the authoritative `active_owner` and active mode; switching mode
replaces the incompatible body. Tank turret yaw and barrel pitch consume the
same `AimSolution` as the HUD reticle and shell. The player motor remains the
authoritative controller underneath these bodies.

`apply_vehicle_buffs()` applies owner-only tuning:

| Mode | Normal / turbo tuning | Armor bonus | Notes |
|---|---|---|---|
| Motorcycle / ATV | sprint floor 1.10 / 1.42 | — | Fast ground travel |
| Tank | sprint cap 0.42 / 0.58 | +25 | Knockback suppressed; active cannon |
| GiantMech | sprint cap 0.34 / 0.46 | +40 | Knockback suppressed; heavy armor |
| Jet / fighter | jet force 0.12 / 0.17; vertical cap 0.70 / 0.90 | — | Enhanced airborne |
| Ship | jet force 0.18 / 0.25; vertical cap 1.10 / 1.42 | — | Strongest air profile |

Holding Sprint (`Shift` / controller LB) while any non-boat ground or air mode
is active requests a 0.7-second turbo burst. A shared cooldown prevents form
switching from stacking bursts and leaves 1.6 seconds of recharge after the
burst ends. The same input remains ordinary sprint while on foot.

Tank mode owns primary fire: RT/LMB suppresses handheld primary/special firing
and launches a 95-damage explosive shell along the canonical aim solution. The
shell travels at 68 (82 during turbo), uses a 7-unit explosion radius, has a
1.2-second cooldown, adds muzzle flash and recoil, and remains attributed to the
owning player for the normal swept collision/reward pipeline. The temporary +25
armor bonus is removed/clamped without inflating saved base armor.

The party still shares one native active vehicle mode at a time, and those
procedural bodies remain player-motor driven. Separately,
`HeavyWaterProgress::vehicles` is a durable independent ATV/fighter authority:
it owns positions, 200 HP, access leases, nearest mounting, eject/respawn,
swept AABB motion, turbo/orbital cruise, destruction, and Ghost Ride. The
`HeavyVehiclePlugin` binds stable local operators and advances that authority
at fixed tick, emitting typed spawn/mount/motion/damage/detonation descriptors.
It deliberately does not write Bevy transforms, Avian bodies, player input,
camera state, or combat health. A physical ECS/Avian consumer remains before
these records appear as independently mountable world entities; authoritative
online play remains a separate networking/security program.

The two-pet Jet Bike assembly is the first spawned hybrid body: a single
party-owned visual follows its rider, shows wheels in ground mode, deploys
wings/thrusters in flight mode, and transitions between the existing
Motorcycle and Jet tuning without stacking both movement profiles. Enter
Vehicle cycles ground → flight → off. Its assembly recipe uses Scrap Frames, Servo Motors, Hover
Jets, and Energy Cores, and Scout/Pilot rescue forms can support it.

---

## Robot / Chassis System

**Module:** `src/robots/` | **Plugins:** `RobotGaragePlugin`

`RobotStyle` in `designer.rs` defines colors and part config. `factory.rs` spawns the physical entity using Capsule3d/Sphere/Cylinder geometry. `presets.rs` has named robot builds (`amp()` is the default player chassis). Playable-character loadouts are now edited through `CharacterDesignPlugin`, while the robot garage handles pet assembly and advanced vehicle/mech forms.

## Controller And Player Ownership

This document defines local-player identity independently from controller
connection order. `PlayerIndex` is the stable runtime/save identity; a gamepad
is a replaceable input device assigned to that identity.

### PO1 — Automatic P1 and Start-to-join assignment

- The first Bevy controller connection automatically claims P1. A native-only
  macOS controller also claims P1 after a short dual-backend discovery window.
  When Bevy and GameController discover the lone pad together, the native
  mapping may overlay its associated Bevy P1 binding; that inferred overlay is
  disabled as soon as a Bevy controller owns P2-P4. The native backend tracks
  the stable `GCController` object instance so an aggregate switch while a
  secondary slot is owned cannot silently replace P1. Keyboard and mouse remain
  available to P1.
- In Player Select, pressing Start on an additional unassigned controller
  claims the first joined-but-unassigned P2–P4 slot, then the next open slot.
- A controller already assigned to a player cannot claim another slot.
- Disconnecting releases the device binding but preserves the joined player,
  character, readiness, and save identity. A newly connected controller
  automatically reclaims P1 when it is free; Start reclaims other joined slots.
- Runtime input and F8 diagnostics resolve through `GamepadAssignments`; they
  must never infer ownership from gamepad query/enumeration order.
- Native disconnects release only the fallback side of a paired P1 binding.
  macOS exposes no shared hardware ID across these two APIs, so the bounded
  discovery and Start-edge correlation is deliberately conservative rather
  than a cross-API identity guarantee; multi-pad native fallback remains part
  of the required physical-controller acceptance matrix.

### PO2 — Individual save ownership

Campaign state remains shared: chapters, world sites/routes, settlements,
raids, robot-pet collection, command assets, hacking, and final-war state.

Each `PlayerSaveData`, keyed by `player_index`, owns:

- level, XP, credits, health, stamina, and armor points;
- inventory/acquired loot and equipped quick item;
- selected primary/special weapon, armor element, and traversal mode;
- perk tree, tech-upgrade ledger, and weapon ranks;
- character blueprint, Studio recipe, and modular part loadout through the
  corresponding player-select slot.

Save schema version 4 retains the optional per-player progression fields added
in v3 and adds atomic rotating generations. Old saves
continue to load by seeding `PlayerProgression` from the legacy campaign-wide
perk, upgrade, and weapon-rank records. New saves write independent progression
for every active `PlayerIndex`.

### Delivered progression ownership slice

Each player-select slot now carries its own `PlayerProgression`. Chapter Select
shows one `P# UPGRADES` owner button per joined player; perk, tech, and weapon
rank purchases mutate only the selected slot. Spawning copies that slot into the
runtime player, and level-up awards, movement perks, health regeneration, armor
bonuses, aim assists, ammo caps, ranged/special damage, melee upgrades, and
Star Sabre upgrades consume the owning player's component.

Disk hydration seeds every slot from its matching `player_index`. Legacy saves
seed all slots from the old campaign-wide progression fields, while optional v3
player records override their matching slots. Entering gameplay no longer
reapplies stale disk progression after Chapter Select, so a purchase made just
before launch survives into the session and the next save.

The legacy top-level perk, upgrade, and weapon-rank resources remain serialized
for backward compatibility. The PX3 shop now uses the same explicit owner
selector and persists per-player ownership/equipment in `players[]`; vehicles
remain party-shared showcase entries. New runtime consumers must use
`PlayerProgression`, not the compatibility resources.

Manual acceptance still required: four physical controllers joining in arbitrary
order, disconnect/reconnect in every slot, save with distinct inventories and
progression, restart, reload, and verify no cross-player swaps.

## Player Music And Action SFX Modding

Starfall keeps background music, custom action MP3s, and built-in arcade effects
on separate layers. Custom files are optional; an empty library remains fully
playable and audible.

### Background music

1. Copy MP3 files into `assets/user_music`.
2. Launch Starfall and press `F6`.
3. Press `R` to rescan after adding or removing files.
4. Use Left/Right or D-pad Left/Right to change tracks.
5. Use Space or controller A to pause/resume. `S` toggles shuffle.

Set `STARFALL_MUSIC_DIR=/absolute/path/to/music` before launch to keep personal
music outside the installation. Files are sorted case-insensitively by name.
The deck uses the Settings music slider and master volume; SFX remains active.

### Action sound assignments

Put MP3 one-shots in `assets/user_sfx`, then edit `actions.json`:

```json
{
  "actions": {
    "weapon.fire": "laser_shot.mp3",
    "melee.slash": "energy_slash.mp3",
    "hoverboard.grind_start": "rail_start.mp3"
  }
}
```

Press `F6`, then `R` to hot reload. Set `STARFALL_SFX_DIR` to use an external
folder containing both `actions.json` and its MP3 files.

Built-in IDs are `weapon.fire`, `weapon.reload`, `melee.slash`, `combat.hit`,
`combat.parry`, `combat.kill`, `player.hurt`, `player.level_up`, `reward.loot`,
and `reward.chest`. Modular systems may emit additional IDs through
`ModularActionSfxEvent`. The stunt-road slice currently emits
`hoverboard.spring`, `hoverboard.grind_start`, `hoverboard.grind_exit`, and
`hoverboard.trick_land`.

The hoverboard/Saber progression pass also emits `hoverboard.overdrive`,
`hoverboard.land`,
`sabre.cyclone`, `sabre.comet_dash`, `sabre.meteor_pound`, and `sabre.wave`.
If an id is absent from `actions.json`, the game uses the appropriate synthesized
arcade fallback so the action remains audible during prototyping.

Action IDs may contain ASCII letters, digits, `.`, `_`, and `-`. MP3 paths must
be relative and cannot contain `..`. Missing, rejected, unreadable, or malformed standard
assignments automatically use Starfall's synthesized arcade effect.

Only use music and sound effects you have permission to play and distribute.

## Character Studio Workflow

The Character Studio is Starfall I's player-facing humanoid authoring tool. It
uses a constrained, non-destructive workflow inspired by RPG character makers
and Blender's turntable/model-inspection tools. Players edit a compact
`CharacterSpec`; procedural generators rebuild the preview. They never edit
runtime mesh entities directly.

### Player workflow

1. Choose a male/female base or a gameplay archetype: STAR HERO,
   SHADOW RAIDER, MANA ADVENTURER, STREET RUNNER, or MECHA ROBOT. Presets are
   editable seeds, never locked character classes.
2. Sculpt body and face proportions with sliders or precise `-` / `+` steps.
   Soft, Heroic, Chibi, and Rival face seeds update only facial parameters and
   leave the current body, wardrobe, colors, and flair intact.
3. Use `R` on a row to restore only that field, or RESET ALL to return to a
   neutral model. Both are protected by the 64-step undo history.
4. Inspect the design with FRONT, PROFILE, BACK VIEW, FULL BODY, and FACE
   CLOSE-UP. Preview Neutral, Joy, Determined, and Surprised expressions
   non-destructively; expression choice is not written into the character
   recipe. Right-drag/right-stick provides free orbit; wheel/triggers zoom.
5. Mix wardrobe slots and colors. Palette fields show their actual color next
   to the value instead of only a numeric index.
6. SAVE VERSION writes a new JSON preset. Existing versions remain available
   through the scrollable library. USE IN GAME assigns the exact generated
   model to the active local-player slot.

Controller layout:

- D-pad or left stick: navigate rows; Left/Right changes the focused value.
- South / A / Cross: activate the focused button.
- West / X / Square: reset the focused sculpt or style field.
- Left/Right shoulder: jump between presets, body, face, wardrobe, and library.
- East / B / Circle: return to the standard character designer.
- Right stick: orbit; triggers: zoom.

### Architecture

The authoring path is:

```text
UI controls -> CharacterSpec -> preset generators -> CharacterPatch -> procedural meshes
```

- `spec.rs` is the serialized source of truth. Morph values are normalized to
  `0.0..=1.0`, with `0.5` as neutral.
- `generators.rs` turns stable named properties into morph, material, and
  wardrobe-slot patches.
- `human_mesh.rs` builds the current mathematical preview. The named patch
  boundary is also where future rigged GLB shape keys can be connected.
- `mod.rs` owns the editor workspace, input, undo, camera, library, and preview
  rebuild scheduling.

Keeping the saved format parametric is important: a player-created character
can remain animation-, equipment-, and gameplay-compatible after the rendering
implementation is upgraded.

### MVP visual library

The generated base deliberately targets a broad 1980s cel-anime language:
larger cranium, tapered lower face, wide eyes with upper lashes and catchlights,
small readable nose, warm cheek marks, and graphic hair silhouettes. Eye
spacing, tilt, and depth; brow angle; nose bridge; chin width; and lip fullness
join the original face controls. The mouth has separate cavity and
upper/lower-lip geometry. Generated
eyes, brows, and mouth carry semantic feature tags so runtime animation can
blink, emote, breathe, and open the mouth during attacks independently of the
head. These are original procedural forms and are not tied to one show or
artist.

The current face pass adds Face Length, Eye Shape, and Nose Tip controls. Eye
whites now use an original shallow parametric lens whose signed-power outline
interpolates from rounded to pointed almond shapes. Skin-toned lower lids,
short outer lashes, broad shallow cheek planes, a narrower bridge/tip nose, and
restrained lip volumes keep the face cohesive in front and three-quarter view.
This follows the same compact-parametric principle as Alan Barr's
[superquadric formulation](https://authors.library.caltech.edu/records/rtr62-f2882/latest)
and the low-control-cage/smooth-limit-surface rationale documented by
[OpenSubdiv](https://graphics.pixar.com/opensubdiv/overview.html), while all
forms and code remain original to Starfall.

The workflow follows the preset-first/fine-adjustment pattern used by
[MetaHuman Creator](https://dev.epicgames.com/documentation/metahuman/metahuman-creator-in-unreal-engine)
and [VRoid Studio](https://vroid.com/en/news/1FI7DuaA77iAntDfxBFTRT).
Expression preview is deliberately separate from the neutral saved design,
matching the authoring distinction described by VRoid's
[Expression Editor](https://vroid.pixiv.help/hc/en-us/articles/4408150140825-How-to-use-the-Expression-Editor).
Starfall keeps its own original anime geometry, parameters, and runtime.

The MECHA ROBOT preset preserves the generator's successful armored silhouette
as a first-class character seed and now combines Crystal Mecha armor with mecha
wings. STAR HERO and SHADOW RAIDER provide readable good-guy/bad-guy contrast;
MANA ADVENTURER uses an arcane coat and moon gem for compact shared-camera
fantasy play; STREET RUNNER demonstrates the city clothing stack.

Wardrobe options currently include twelve top silhouettes, eight bottoms, nine
footwear silhouettes, nine hairstyles, gloves/gauntlets, and four full armor
overrides. The Arcane Coat and Star Knight Tunic add fantasy layering to the
existing modern wardrobe. Mecha Armor, Crystal Mecha, and Dragon Mecha provide
distinct armor suites. An independent fantasy-flair field offers None, Star
Gem, Moon Gem, Royal Mantle, Arcane Halo, and Mecha Wings without consuming a
wardrobe slot. Clothing uses separate cloth, denim, leather, sole, metal, and
emissive material responses.

Chest Shape is an explicit neutral morph shared by both bases. It drives fitted
clothing depth and sex-aware procedural chest definition: broader pectoral
planes on the male base and softer paired forms on the female base. The control
remains editable on either body and does not change gameplay collision.

Every non-armor character receives a fitted coverage garment around the pelvis
and upper thighs before outer clothing is generated. This is an invariant of
the mesh builder, not a UI convention, so randomization and old presets cannot
create an accidentally uncovered model.

### Unified procedural/imported rig MVP

The studio has two visual backends feeding one animation contract. **GENERATED**
builds the editable procedural character. **RIG TEST** loads
`Characters/AMP.glb`, discovers its bone hierarchy after scene instantiation,
and maps it atomically into the same 17-joint `SkeletonRig` used by procedural
characters. Imported joints preserve authored translation, rotation, and scale;
the runtime builds rest-pose-adjusted MVP clips for that rig and enables the
same `CartoonAnimator`, gameplay pose selection, hand state, IK, and sockets.

Character Studio reports `Pending`, `Ready`, or `Invalid` import status. A rig
is rejected rather than partially animated when required joints are missing,
multiple bones map to one canonical joint, or the hierarchy cannot be resolved.
If the scene asset itself fails to load, the generated preview is restored.

The repository asset audit found that AMP has one 40-bone skin and covers all
17 Starfall gameplay joints, but contains no animations and no morph targets.
Antonio, Chroma, Daria, and Vincenzo currently contain static meshes without a
skin. RIG TEST is therefore a diagnostic integration seam, not a replacement
for the editable generated model yet.

Production Blender/export bone names:

```text
SF_PELVIS  SF_SPINE  SF_CHEST  SF_NECK  SF_HEAD
SF_SHOULDER_L  SF_ELBOW_L  SF_WRIST_L
SF_SHOULDER_R  SF_ELBOW_R  SF_WRIST_R
SF_HIP_L  SF_KNEE_L  SF_ANKLE_L
SF_HIP_R  SF_KNEE_R  SF_ANKLE_R
```

`rig_bridge.rs` accepts AMP's existing names plus common Mixamo, Blender
Rigify, and Unreal-style upper-arm/forearm/hand/thigh/calf/foot aliases. Saved
gameplay data references canonical joints only, never vendor bone names.

Production shape keys must use the existing patch names:

```text
body_height body_muscle body_weight shoulders_wide waist_width hips_wide limb_length
body_chest_shape face_length face_jaw_wide face_chin_long face_chin_wide
face_nose_long face_nose_wide face_nose_bridge face_nose_tip face_brow_heavy
face_cheek_full face_eye_large face_eye_shape face_eye_spacing face_eye_tilt
face_eye_depth face_brow_angle face_mouth_wide face_lip_full
```

This contract means a future Blender model can replace the preview renderer
without changing `CharacterSpec` JSON or invalidating saved player designs.

### Review and next milestones

The studio now has a viable modeling loop, but it is not yet a Blender-style
mesh editor. The next upgrades should preserve constrained output while adding
more visual authoring power:

1. Add collapsible Body, Face, Hair, Wardrobe, and Materials tabs with preset
   presets and thumbnail grids. This is the largest remaining RPG-maker UX gap.
2. Author a production humanoid base using the documented 17-bone and
   shape-key contract. Runtime rig discovery, validation, rest-pose retargeting,
   shared animation, and fallback are complete; AMP still lacks editable shape
   keys and authored animation clips.
3. Add region selection in the viewport: selecting the head, torso, arm, or leg
   should open the matching parameter group and outline that region.
4. Add paired/asymmetric controls for ear shape, hand/foot scale, arm/leg length
   separately, and optional left/right details.
5. Promote the delivered neutral/joy/determined/surprised Studio previews into
   authorable expression channels (fear, anger, pain, phonemes), then add
   locomotion/combat preview poses and equipment-clearance checks.
6. Add named character projects, duplicate/rename, thumbnail capture, and an
   explicit APPLY TO PLAYER SLOT action. Keep version history for recovery.
7. Upgrade generated GLB export from rigid preview geometry to a skinned
   canonical skeleton with weights, sockets, morphs, and optional animations.

The next rig-tooling phase is an import wizard that persists one reusable rig
profile per asset: detected convention, manual mapping overrides, axis/unit
normalization, T/A-pose calibration, root-motion policy, socket offsets, and
validation results. After that come animation layers/masks, blend trees,
timeline events, IK gizmos, retarget comparison, and equipment-clearance tests.

Free vertex sculpting is deliberately not the immediate target. Unrestricted
topology edits would make animation, collision, clothing, and multiplayer asset
budgets unreliable; shape-key and part-based modeling provides the useful
creative range while keeping authored characters game-ready.

## Creation Toolchain

Starfall is both a four-player game and a constrained game-making system. The
editor strategy is therefore not a collection of debug menus. Every tool must
compile a small, versioned recipe into validated runtime content through the
same deterministic pipeline.

```text
editable recipe -> generator -> preview -> validators -> compiled runtime asset
       |                |           |             |                |
   JSON/RON save    seeded RNG   undo/orbit   budget/playtest   game/save ID
```

### Scientific foundation

All generators follow these rules:

- The recipe is the source of truth; generated entities and meshes are output.
- Every schema has a version and migration path. Old player saves remain valid.
- Random generation accepts an explicit seed and produces repeatable output.
- Dimensions use meters, angles use radians internally, colors use linear or
  documented sRGB values, and normalized creative controls use `0.0..=1.0`.
- Generators expose semantic tags (face feature, robot socket, road lane, room,
  cave portal) instead of depending on entity names or scene hierarchy.
- Validation runs before publish: geometry/material budgets, collision,
  traversal clearance, spawn safety, connectivity, and four-player camera fit.
- Preview and runtime use the same compiler. A beautiful editor-only result
  that changes in gameplay is a defect.
- Recipes receive stable content IDs so levels reference content without
  copying generated geometry into campaign saves.

The broader Blender-inspired capability triage, current-Bevy corrections, and
ET1–ET11 delivery gates are specified in `docs/archive/engine_tools_multistage_pass.md`.

### Tool sequence

#### GM1 — Humanoid Character Studio (active)

The current `CharacterSpec -> CharacterPatch -> procedural mesh` path is the
reference implementation. Its 26-morph contract includes face length, eye
shape/depth, nose bridge/tip, chin width, and chest shape; fantasy
gems/mantles/halos/wings layer
independently over twelve tops and three mecha suites. Four face seeds and four
non-destructive expression previews provide a preset-first refinement loop.
Immediate completion gates are authorable expression channels, animation
preview poses, thumbnail presets, attachment
clearance, seeded randomize, and explicit schema versioning. Art direction
combines compact fantasy readability, colorful action robots, and original
1970s space-opera shapes without copying protected characters or assets.

#### GM2 — Robot and Monster Forge (playable vertical slice active)

Unify the existing `src/robots` archetypes and drone factory behind a serializable
`CreatureSpec`. Use composable morphology modules rather than separate hardcoded
models:

- topology: humanoid, quadruped, flyer, crawler, serpent, drone;
- parts: core, head/sensor, torso, limbs, wings, tail, armor, weapon sockets;
- behavior role: ally, civilian, scout, bruiser, artillery, boss;
- materials: painted metal, bare alloy, crystal, organic, energy;
- rig contract and procedural locomotion selected from topology;
- faction palette and silhouette rules applied as a layer, not baked geometry.

Acceptance: create, save, reload, animate, validate, and spawn at least three
distinct robots and three monsters from recipes; generated enemies must work
with targeting, damage, navigation, and campaign saves.

Current status: `src/robots/creature.rs` provides schema-versioned JSON recipes,
deterministic seeded generation, robot/monster kinds, six topologies, seven
gameplay roles, six factions, five material responses, publish validation, and
six curated seeds. `CreatureSpec::from_legacy` wraps an existing preset without
losing its `RobotStyle`, while `spawn_creature` compiles topology into the
procedural factory and adds stable runtime metadata.

The Project Hub now opens a player-facing `AppState::CreatureForge` workspace
with live preview, controller focus, bounded morphology/features/modifier
editing, undo/reset, project-backed validation/save, and versioned preset
export/load. Published creature payloads enter `PublishedCreatureCatalog` only
when published and draft hashes match. Dungeon spawners can resolve a
`CreatureSpawnOverride` by stable content ID, derive combat stats from the
authored role, and fall back to their built-in enemy safely. Remaining GM2 work
is topology-specific animation rigs, thumbnail publishing, window-layout
persistence, and Level Composer authoring for creature spawn overrides.

#### GM1b — Imported Character Forge (mesh selection + modular parts active)

Project Hub also opens `AppState::ImportedCharacterForge`. Authors can drop an
external binary glTF file, after which Forge copies it to
`assets/imported_characters/`, inventories its named meshes, and reconstructs
the node transforms in a live rotating preview. Each rigid mesh owns an
independent non-destructive stack using the shared Mirror, Array, Twist,
Subdivide, Smooth, and Noise Displace tools. Root width, height, and depth are
editable independently.

The Mesh-Aware Selection window works per GLB primitive and offers direct
Alt-click face picking (Shift+Alt-click selects its linked island), face
stepping and toggle, all/none/invert, connected-island selection, grow/shrink,
similar-facing selection, and X-axis mirror. The preview splits the display-only
mesh by source face index and highlights selected triangles in orange. These
operations never rewrite the imported vertex, skin, or morph buffers.

Selections can be turned into modular parts with quick body/head/limb/hand/foot,
hair, cape, shoulder, accessory, weapon, and hurtbox presets. Each assignment
stores a stable ID, source GLB, mesh and primitive indices, sorted face IDs,
slot, canonical animation joint, local centroid pivot, and gameplay role.
Assignments save in imported Character schema v5 (first introduced in v2).
**SAVE TO LIBRARY** creates a
separately tagged reusable project record; **ADD LIBRARY PART** references its
source-backed assignment in the current character, enabling cross-character
mix-and-match without duplicating geometry. **ASSEMBLE RUNTIME PREVIEW** runs
the actual asynchronous multi-source assembler beside the editor preview.

The Color + Texture window preserves each primitive's authored GLB material
until an override is created. Authors can cycle a curated color palette and
tune metallic, perceptual roughness, and emissive strength. PNG/JPG drops target
an armed base-color, normal, packed metallic/roughness, or emissive channel;
maps can be cleared independently or the whole primitive can return to its
authored material. External images are copied into
`assets/imported_textures/`, while in-project images keep their existing
project-relative path. Character schema v5 persists these overrides.
Saving a modular part includes its matching material override, and loading the
part replaces the same source-primitive material binding deterministically.

The UV + Face Paint window supports authored UV preservation, XY/XZ/YZ planar
projection, Y-axis cylindrical projection, and split-vertex box projection
with automatic hard seams. U/V scale and offset plus rotation remain
non-destructive. **PAINT SELECTION** records bounded source-face color strokes;
later strokes override earlier strokes, and runtime meshes receive matching
vertex colors. **MARK BOUNDARY SEAM** turns the selected region outline into
manual cut edges; seam unwrap separates connected islands and grid-packs them
with padding. **BAKE PAINT PNG** rasterizes the strokes through the current UVs
into a versioned 256×256 texture under `assets/imported_textures/`. UV edits,
seams, paint, and the baked texture travel with reusable part records.

`ImportedModularRuntimePlugin` is registered with the character runtime.
`request_imported_character(commands, root, spec)` asynchronously loads every
referenced source GLB, preserves node transforms, extracts assigned faces,
applies UV and PBR overrides, and spawns renderable regions.
`ImportedRuntimePart` exposes slot/joint/pivot data and
`ImportedGameplayRegion` exposes visual, hurtbox, weapon, shield, traversal,
interaction, or cosmetic ownership. Rigid regions bind to a root's
`SkeletonRig` joint when available.
Morph-only parts retain source morph targets when the UV edit preserves vertex
correspondence and receive `MeshMorphWeights` plus named
`ImportedMorphRegion` metadata. `ImportedDeformationStatus` reports this count
and marks skinned sources that currently use rigid joint fallback.

Meshes carrying joint weights or morph targets remain preview-safe and retain
authored deformation data; topology modifiers are recorded but not baked onto
those meshes yet. Runtime extraction is rigid and does not rewrite skin
weights, so continuously deforming torso/cloth regions remain on their authored
skinned source until deform-aware rebinding lands. Next work is
box/circle/lasso selection, brush displacement/masking, pressure-aware pixel
brushes, freeform island transforms, thumbnails, socket orientation editing, and a
skin/morph-aware GLB
bake/export path. See
[`docs/guides/imported-mesh-editor.md`](guides/imported-mesh-editor.md).

#### GM3 — World Kit Forge

Create three recipe families sharing sockets and metrics:

- `RoadKitSpec`: deck profile, lane count, spline grade/curvature limits, ramp,
  loop, jump, guard, support-column, and terrain-clearance policies.
- `SettlementKitSpec`: street graph, parcels, building/castle modules, doors,
  rooms, stairs, encounter sockets, materials, and faction dressing.
- `DungeonKitSpec`: cave/room graph, elevation layers, gates, secrets, hazards,
  fast-travel anchors, and shared-camera encounter bounds.

Generators publish both visuals and gameplay metadata. Roads must output a
connected traversal graph; buildings must output navigable room/portal graphs;
caves must output encounter and fast-travel graphs.

#### GM4 — Level Scene Composer

The scene editor consumes published character, creature, and world-kit assets.
It needs a full-level map plus local 3D viewport, selection/outliner,
translate/rotate/scale gizmos, terrain height/paint tools, spline editing,
prefab placement, duplicate/delete, undo/redo, and property inspection.

Terrain authoring uses non-destructive layers: base heightmap, sculpt delta,
material/biome masks, exclusion masks, and placed landmarks. Cities, castles,
caves, roads, encounters, and fast-travel points are referenced by stable IDs.
The editor must visualize road connectivity, cave portals, spawn radii,
navigation, camera bounds, and terrain collision before publishing.

Acceptance: load an entire chapter, add a terrain landmark, connect a road,
place a generated castle and cave, add spawns and fast travel, validate, save,
reload, then play the result without code changes.

### Shared editor services

Build these once and reuse them across all four tools:

1. Versioned asset registry and migrations.
2. Command-based undo/redo transactions.
3. Selection, outliner, inspector, searchable palette, and thumbnail browser.
4. Orbit/free cameras plus transform and spline gizmos.
5. Seeded generation and variant comparison.
6. Validation report with errors, warnings, metrics, and viewport highlights.
7. Draft/published lifecycle with atomic save and recovery snapshots.
8. Performance budgets and automated headless compile/playability tests.

### Near-term order

1. Finish GM1 expression channels, animation preview, schema version, and
   thumbnail preset browser.
2. Adapt the existing robot designer/factory to the shared recipe/validator
   contract; do not replace its working drone geometry.
3. Extract shared editor services only after both GM1 and GM2 exercise them.
4. Implement road-kit recipes first in GM3 because terrain-aware roads already
   have measurable grade, clearance, ramp, and connectivity tests.
5. Add settlement/castle and cave kits, then build GM4 on their published
   content contracts.

This order keeps the game playable while the maker layer grows and prevents the
scene editor from being designed before its asset schemas exist.
