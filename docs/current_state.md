# Starfall I — Current State

> Living agent handoff. Last reconciled July 23, 2026. Update this file when a
> change alters architecture, ownership, persistence, controls, verification,
> or the next production slice.

## Baseline

- Rust 2021, Bevy `0.19.0`, Avian `0.7`; one binary crate.
- Automated baseline: 432 tests across the main crate and example targets,
  `cargo build`, `cargo fmt --check`, and
  `cargo clippy --all-targets -- -D warnings` pass locally.
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
  Star Sabre controls, procedural weapon poses, loot attraction, road ramp
  alignment tests, and the MVP animation/rig bridge are present.
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
  ground-approach descent assist, a compressed landing pose, and a 10% larger
  sole-height visual. The Star Saber is starter equipment
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
- City spy drones now carry canonical enemy hurtboxes, restoring projectile
  damage, hit flash, and damage-number feedback. Per-player armor durability
  begins recharging at 18 points/second after three seconds without a hit.
- Starfall Forge has a title launcher and project hub, persistent project
  registry, stable recipe/content IDs, atomic project/source saves and recovery,
  validation/publishing, immutable presets and lock documents, deterministic
  modifier stacks, typed World Kit recipes, multi-level projects, and startup
  level playtest.
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
  per rigid mesh, and saves root proportions plus non-destructive edit stacks
  into Character payloads in the active Forge project. Skinning and morph data
  remain intact: topology-changing preview operations are deliberately bypassed
  for deformable meshes until a weight/morph-aware bake path is implemented.
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
  source hints. Remaining screens migrate incrementally.

## Current production order

1. Maintain the reusable app-construction seam and headless startup/plugin
   smoke tests covering construction, presentation-backend exclusion, Startup,
   and a steady frame.
2. Maintain the schema-compatible base-stat/derived-cap contract and keep new
   permanent bonuses on `PlayerBaseStats` rather than effective cap caches.
3. Continue mechanical hotspot extraction: world terrain, settlements, and
   dungeons; UI screens and HUD; then isolated Forge panels. Preserve schedule
   ordering and behavior in each slice.
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
7. Continue PM1–PM3 creator gaps: templates/import/recovery/settings, preset
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
