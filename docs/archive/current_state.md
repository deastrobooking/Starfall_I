# Starfall I — Current State

> Living agent handoff. Last reconciled August 12, 2026. Update this file when a
> change alters architecture, ownership, persistence, controls, verification,
> or the next production slice.

## Baseline

- Rust 2021, Bevy `0.19.0`, Avian `0.7`; one binary crate.
- Automated baseline: 939 Designer-edition tests and 927 Game-edition tests,
  locked all-target checks, `cargo fmt --all -- --check`, and strict Clippy for
  both feature sets pass locally. CI runs formatting once, then verifies the
  Designer and Game editions independently on macOS.
- Manual macOS one-to-four-controller, TV-layout, terrain traversal, and
  split-screen performance acceptance is still required. Automated success is
  not evidence that those hardware gates passed.
- Core states are `MainMenu`, `ProjectHub`, `CreatureForge`,
  `ImportedCharacterForge`, `PlayerSelect`, `CharacterDesign`,
  `CharacterStudio`, `ChapterSelect`, `RobotGarage`, `Playing`, `Paused`,
  `GameOver`, and `Victory`.

## Delivered foundations

- Start-to-join assigns controllers to stable `PlayerIndex` slots; reconnects
  reclaim a free joined slot without using gamepad enumeration order.
- `PlayerProgression` owns each player's perks, upgrades, weapon ranks, and
  shop ownership. Stats, inventory, loot, quick item, loadout, traversal,
  character recipes, HUD, and feedback are also per-player. Chapters, world
  sites/routes, settlements, raids, robot pets, command assets, hacking, and
  final-war state are campaign-shared. Vehicles remain party-shared with an
  explicit owner/driver and indexed passengers.
- Schema-v4 saves use three atomic rotating slots in the platform data folder,
  choose the valid highest generation, migrate legacy saves, and reject future
  schemas. Settings are atomic. Crash reports share the platform data root,
  sanitize local paths, and retain the newest five timestamped files.
- The default player motor runs at 64 Hz with buffered input and camera
  interpolation; the legacy motor remains an A/B escape hatch.
- The 200-mile Everest world uses deterministic terrain, 64 high/low render
  patches over a full-resolution collider, spatial LOD for foliage/buildings,
  bounded four-seed terrain/road caches, connected speed roads, caves,
  settlements, dungeons, raids, and campaign anchors.
- Authoritative per-player aiming, viewport reticles, Homing Star tracking,
  independent RB Star Sabre slash and RT ranged-fire controls, airborne-target
  aim assistance, procedural weapon poses, loot attraction, road ramp
  alignment tests, and the MVP animation/rig bridge are present.
- Each local player owns an independent first-/third-person camera preference
  (`P` for keyboard P1 or Select + D-pad Left on the assigned controller).
  Character-scaled eye height and owner-only avatar render layers keep the
  first-person view clear without hiding that hero from other split-screen
  viewports. Boss and dungeon shared cameras remain authoritative and restore
  the individual preference afterward.
- Character Studio exposes 26 body/face morphs, including face length, eye
  shape/depth, nose bridge/tip, chin width, and sex-aware chest shape. Its
  original parametric almond eyes, fitted lids, cheek planes, and restrained
  mouth improve the cartoon-anime read. Soft/Heroic/Chibi/Rival face seeds and
  non-destructive Neutral/Joy/Determined/Surprised previews accelerate
  iteration without mutating the saved neutral recipe. Twelve top styles, independent
  fantasy gems/mantles/halos/wings, and Mecha/Crystal/Dragon armor suites all
  persist through the same playable `CharacterSpec` recipe.
- The Rocket Hoverboard now has reduced airborne gravity, a stronger board
  jump/rocket profile, B/East overdrive, uphill wave assistance, contact-normal
  banking, forward-momentum retention in the air and during coast, a short
  forward-looking ground-approach descent/braking assist, slope-tangent rocket
  lift, turn-sensitive steering, and 15% less excess-speed/heading inertia.
  Its black, red, orange, and yellow 1980s-anime deck uses pink rear
  afterburners. Rail-bound B/East belongs exclusively to Grind instead of also
  firing overdrive. Fixed-motor carry is cleared at hard traversal constraints,
  so rail/loop snaps, recovery, ledge hangs, grapple arrivals, and teleports
  cannot leak delayed movement. Edge grabs validate a wall, head clearance,
  and walkable top before anchoring; Grapple-mode surface latches raycast real
  geometry and hand off into a mantle or stable hang. Stunt rails, springs, and
  loop apexes expose authored grapple sockets. The Star Saber is starter equipment
  with a small second-slash wave; the Solar Glyph and Beam Capacitors add wider,
  larger, stronger waves while save-backed world blueprints/gems unlock cyclone,
  double-dash/pound, elemental, and legendary upgrades per player.
- Each player HUD now reports hoverboard boost readiness/recharge, the live
  Saber technique, owned wave/technique unlocks, elemental gem count, and
  Starheart ownership. Hoverboard overdrive and Saber actions emit modular SFX
  ids with synthesized fallbacks when no custom MP3 is assigned.
- Saber techniques now have bounded procedural silhouettes: crossed Cyclone
  rings, Comet Dash streaks, a Meteor Pound ring/column, and wave muzzle rings.
  Elemental gems select shared effect/wave materials and Starheart adds a pink
  signature ring. A global 24-entity transient budget protects the four-player
  split-screen case without allocating per-use materials. F11 includes these
  entities in total combat VFX and reports a dedicated `saber: current/24`
  reading for the acceptance pass.
- The persistent Saber blade now uses a larger two-layer additive HDR core/aura,
  extends roughly twice the previous rendered length, and has a correspondingly
  wider normal-slash reach. Starter waves launch only after slash two; progression
  scales them from one small projectile to four larger, faster, stronger waves.
- Main mountain freeway trunks are 64 units wide, sampled into 32-unit deck
  slices, smoothed through bounded Hermite centerlines, and protected by taller
  4.6-unit edge barriers. Settlement rings remain narrower road/path variants.
  Forge road recipes compile finer Hermite curves, curved 12-slice junctions,
  deck barriers, and expose an `AUTO ROUND ALL SPLINE POINTS` controller action.
- `assets/terrain/RACE.png` is blended as a feathered southeast terrain tile.
  Its Grand Raceway district connects to the Rockies and Antarctic trunks,
  carries a 94-unit-wide closed terrain-following course, six ordered race
  gates, four launch ramps, boost lanes, two rivals, recovery checkpoints, and
  two chapter-map `R` fast-travel anchors. Road-pad impulse, cap, and sustain
  now scale conservatively from Motion Boot and Aegis logic upgrades.
- The Robot Garage now offers a save-compatible two-pet `JetBike` assembly.
  It spawns one party-owned transforming bike visual around the active rider,
  uses the Motorcycle profile in ground mode and the Jet profile in flight
  mode, and preserves the existing one-driver vehicle authority. Enter Vehicle
  cycles ground → flight → off while ordinary single-mode forms toggle from the
  same input. Map input no longer leaks into ground-vehicle activation.
- City spy drones now carry canonical enemy hurtboxes, restoring projectile
  damage, hit flash, and damage-number feedback. Per-player armor durability
  begins recharging at 18 points/second after three seconds without a hit.
- Starfall Forge has a title launcher and project hub, persistent project
  registry, stable recipe/content IDs, atomic project/source saves and recovery,
  validation/publishing, immutable presets and lock documents, deterministic
  modifier stacks, typed World Kit recipes, multi-level projects, and startup
  level playtest.
- The August Forge M0 contract pass replaces numeric focus orders with one
  stable action/navigation registry; validates, hashes, and guards references
  across every authored level; adds an OS-backed writer lock, atomic
  primary-manifest revision conflicts, explicit recovery promotion, and
  clean external reload; preserves authored duplicate names/modifiers; evicts
  failed stale history; caches unchanged disk diagnostics; assigns unique new
  project identities; and publishes Weapon/Creature output through immutable
  hashed generations selected once per load by atomically promoted `current.json`.
  Designer/Game locked CI coverage is active. The remaining architecture path
  is `docs/editor_roadmap.md`; M0 does not yet isolate editor/runtime worlds or
  provide universal command history and complete content cooking.
- Multi-level projects expose the active level's order/startup state and support
  controller-driven reordering plus guarded deletion. Deletion requires a
  same-level second confirmation, cannot remove the final level, and repairs
  active/startup identity before the replacement scene is respawned.
- Creature Forge is a Project Hub branch with live procedural preview,
  controller menu focus, bounded morphology and modifier editing, undo/reset,
  project-backed validation/save, and versioned preset export/load. Published
  creature records populate `PublishedCreatureCatalog`; dungeon spawners can
  opt into them by stable content ID and safely fall back when unavailable.
- Imported Character Forge is a second Project Hub branch. It accepts external
  `.glb` drag-and-drop, safely copies imports into the asset tree, inspects and
  selects named meshes, preserves node hierarchy transforms in its rotating
  preview, applies the shared Mirror/Array/Twist/Subdivide/Smooth/Noise stack
  per rigid mesh, and adds source-topology triangle selection by primitive:
  all/none/invert, linked island, grow/shrink, similar normal, and X mirror.
  Selection topology is now compiled through an engine-owned
  `EditableMeshDocument`: generational element IDs, triangle half-edges,
  `f64` modeling positions, validation, independent revisions, transactional
  position edits, and render-triangle-to-`FaceId` bake mappings keep Bevy
  vertex-buffer offsets from becoming the long-term editing identity. Its
  command layer now supports validated edge split, flip, and link-condition
  collapse with face-inversion checks, explicit semantic/crease transfer,
  unaffected-ID preservation, generational invalidation, and fingerprinted
  bounded undo/redo snapshots. UV, paint, and skin attributes remain on the
  render/import side until their corner/vertex-domain stores are implemented.
  Selected regions can be assigned stable modular part IDs, character slots,
  canonical animation joints, pivots, and gameplay roles. Character payload
  schema v2 persists those assignments, while separately tagged modular-part
  records provide a project library that can be mixed into other character
  specs. Per-source primitive material overrides preserve authored GLB
  materials by default and add palette tint, metallic, roughness, emission,
  and imported base-color/normal/metallic-roughness/emissive maps. Reusable
  part records carry their matching override. UV schema v5 adds authored,
  planar, cylindrical, and split box projection; scale/offset/rotation; and
  source-face color painting plus selected-boundary seam marking, island
  packing, and versioned PNG paint baking. A globally registered runtime assembler resolves
  multi-GLB part specs asynchronously, extracts assigned regions, applies their
  UV/material edits, exposes gameplay-region metadata, and binds rigid regions
  to canonical skeleton joints. Morph-only extracted regions preserve their
  targets when vertex correspondence survives; skinned regions expose an
  explicit rigid-fallback diagnostic. Skinning and morph source data remain intact:
  selections reference authored face IDs and derived runtime parts strip
  deformation metadata without mutating the source asset.
- App construction now has one authoritative
  `build_starfall_app(StarfallAppMode)` boundary. Production keeps the full
  window/render/audio stack; the headless profile keeps state, assets, physics,
  gizmos, and every Starfall game plugin without creating process-global
  presentation backends. Smoke tests verify registration and execute Startup
  plus a steady frame, including generated assets and menu UI/camera creation.
- Per-player health and armor caps now follow a save-compatible derived-stat
  contract. `PlayerBaseStats` owns blueprint/hero level-one bases;
  `PlayerStats` caches caps derived from level, equipment, perks, and upgrades.
  Optional base fields preserve new saves, while legacy effective caps infer a
  base once without double-applying known bonuses.
- The accepted product/art direction and performance gates are recorded in
  `docs/product_art_direction_triage_2026-07-17.md`. A shared semantic
  `UiTheme` now drives the Main Menu, Player Select accents, and per-player HUD;
  the Star Loadout shell now follows it and includes a controller-owned,
  shape-coded relic catalog with save-backed discovery, effects, inputs, and
  source hints. Persisted UI scale (80–140%) and safe-area controls now drive
  Bevy's fixed-unit scaling and viewport-local multiplayer HUD margins. Player
  HUD panels anchor independently inside the four split-screen quadrants, and
  creator tool windows clamp their complete initial bounds after spawn, resize,
  or scale changes. Core Studio/Forge controls now use larger interaction
  targets. Title and Pause expose the same persisted accessibility controls for
  damage numbers, high-contrast focus/HUD presentation, reduced floating-text
  motion, and voiced-dialogue subtitles. The extracted `ui_foundation` module
  owns the semantic theme, global UI scale, stable text-catalog keys, and
  device-aware prompt rendering. Shared help copy follows the last-used
  keyboard or gamepad across menus and common in-game panels, with persisted
  Auto/Xbox/PlayStation/Nintendo controller glyph selection and vendor-aware
  Auto detection. Settings now persist four complete face-button layouts,
  thirteen discrete Player 1 keyboard bindings, and combined mouse/stick
  invert-look Y. Both controller backends remap face buttons at the source so
  chords follow the selected layout; LB, Select, D-pad modifiers, and movement
  axes remain fixed to keep every action reachable. Remaining screens still
  migrate incrementally to the semantic theme and text catalog; translated
  locale assets remain.

## Current production order

1. Maintain the reusable app-construction seam and headless startup/plugin
   smoke tests covering construction, presentation-backend exclusion, Startup,
   and a steady frame.
2. Maintain the schema-compatible base-stat/derived-cap contract and keep new
   permanent bonuses on `PlayerBaseStats` rather than effective cap caches.
3. Execute Forge M1 from `docs/editor_roadmap.md`: add the library/composition
   seam, extract neutral content/runtime ownership, and split editor modules
   without changing behavior.
4. Complete manual one-to-four-controller acceptance for joining, aiming,
   shops, editor launch, save/reload identity, disconnect/reconnect, and
   controller-only menu flows.
5. Profile N11 large-world entity, simulation, terrain, and one-to-four-camera
   render costs before adding streaming complexity or denser content.
6. Hardware-test and tune the new hoverboard/Saber/road pass: steep ascents, crests,
   banked curves, B overdrive, board-to-feet alignment, second-slash starter
   waves and upgraded multi-wave growth, softened landings, forward coasting,
   freeway barrier containment, rounded connectors,
   technique inputs/VFX readability, elemental and Starheart differentiation,
   four-player effect-budget behavior, and per-player save/reload ownership.
7. Continue behavior-preserving world/UI hotspot extraction and PM1–PM3 creator
   gaps behind the new seams: templates/import/recovery/settings, preset
   thumbnails and validation capture, and per-level metadata/budgets plus
   rename.

## Authority map

- Immediate order and acceptance: `docs/agent_next_steps.md`
- Architecture and ownership: `docs/architecture.md`
- Runtime behavior: `docs/systems.md`
- Creator workflow: `docs/player_creator_experience_plan.md`
- Engine core: `docs/engine_roadmap.md` (`EC#`)
- Campaign/engine milestones: `docs/engine_upgrade_milestones.md` (`M#`)
- Movement/animation: `docs/playerengine.md` (`MM#`)
- Verification procedure: `docs/guides/verification.md`
- Dated reviews under the snapshot section of `docs/README.md` are evidence,
  not current instructions.

## Rules that prevent regressions

- Never infer player ownership from query or controller order.
- Do not serialize ECS entities or generated meshes as authored source data.
- Preserve save defaults and migrations; persisted ownership changes require an
  explicit schema plan.
- Keep generated terrain visuals, collider sampling, roads, ramps, caves, and
  anchors on the same height authority.
- Measure before optimizing and do not claim manual/hardware acceptance from
  unit tests.
- Prefer small tested extractions over repository-wide rewrites.
