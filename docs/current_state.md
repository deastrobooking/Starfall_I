# Starfall I — Current State

> Living agent handoff. Last reconciled July 17, 2026. Update this file when a
> change alters architecture, ownership, persistence, controls, verification,
> or the next production slice.

## Baseline

- Rust 2021, Bevy `0.19.0`, Avian `0.7`; one binary crate.
- Automated baseline: 371 tests, `cargo build`, `cargo fmt --check`, and
  `cargo clippy --all-targets -- -D warnings` pass locally.
- Manual macOS one-to-four-controller, TV-layout, terrain traversal, and
  split-screen performance acceptance is still required. Automated success is
  not evidence that those hardware gates passed.
- Core states are `MainMenu`, `ProjectHub`, `PlayerSelect`, `CharacterDesign`,
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
- The Rocket Hoverboard now has reduced airborne gravity, a stronger board
  jump/rocket profile, B/East overdrive, uphill wave assistance, contact-normal
  banking, and a larger sole-height visual. The Star Saber is starter equipment;
  the Solar Glyph unlocks waves and save-backed world blueprints/gems unlock
  cyclone, double-dash/pound, elemental, and legendary upgrades per player.
- Each player HUD now reports hoverboard boost readiness/recharge, the live
  Saber technique, owned wave/technique unlocks, elemental gem count, and
  Starheart ownership. Hoverboard overdrive and Saber actions emit modular SFX
  ids with synthesized fallbacks when no custom MP3 is assigned.
- Starfall Forge has a title launcher and project hub, persistent project
  registry, stable recipe/content IDs, atomic project/source saves and recovery,
  validation/publishing, immutable presets and lock documents, deterministic
  modifier stacks, typed World Kit recipes, multi-level projects, and startup
  level playtest.
- The accepted product/art direction and performance gates are recorded in
  `docs/product_art_direction_triage_2026-07-17.md`. A shared semantic
  `UiTheme` now drives the Main Menu, Player Select accents, and per-player HUD;
  the Star Loadout shell now follows it and includes a controller-owned,
  shape-coded relic catalog with save-backed discovery, effects, inputs, and
  source hints. Remaining screens migrate incrementally.

## Current production order

1. Maintain the reusable app-construction seam and headless startup/plugin
   smoke test now covering the authoritative registration boundary.
2. Design the schema-compatible base-stat/derived-cap contract so authored,
   leveled, equipped, and loaded maximum health cannot overwrite each other.
3. Continue mechanical hotspot extraction: world terrain, settlements, and
   dungeons; UI screens and HUD; then isolated Forge panels. Preserve schedule
   ordering and behavior in each slice.
4. Complete manual one-to-four-controller acceptance for joining, aiming,
   shops, editor launch, save/reload identity, disconnect/reconnect, and
   controller-only menu flows.
5. Profile N11 large-world entity, simulation, terrain, and one-to-four-camera
   render costs before adding streaming complexity or denser content.
6. Hardware-test and tune the new hoverboard/Saber pass: steep ascents, crests,
   banked curves, B overdrive, board-to-feet alignment, Saber wave discovery,
   technique inputs, and per-player save/reload ownership.
7. Continue PM1–PM3 creator gaps: templates/import/recovery/settings, preset
   thumbnails and validation capture, and per-level metadata/budgets plus
   rename/delete/reorder.

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
