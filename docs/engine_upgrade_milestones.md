# Starfall I Engine Upgrade And Milestone Runbook

This runbook is the durable guide for future Codex and engineer work on
Starfall I engine upgrades and production milestones. The current stable
baseline is Bevy `0.18.1` with `bevy_rapier3d` `0.34`.

## Baseline

- Engine stack: Rust 2021, Bevy `0.18.1`, `bevy_rapier3d` `0.34`, `serde`,
  `serde_json`, `rand`, and `noise`.
- Current Bevy message model: gameplay communication uses `Message`,
  `MessageReader`, `MessageWriter`, and `App::add_message`.
- Current compatibility debt: `src/rendering.rs` provides local bundle aliases
  for Bevy bundle shapes, and the migration touched hierarchy/despawn,
  camera/HDR, cursor options, render imports, ambient lighting, image data, and
  Rapier trimesh construction.
- Current validation baseline: `cargo fmt`, `cargo check`, and `cargo test`
  passed after the Bevy 0.18 migration. Automated tests are still thin.
- Upgrade policy: future engine upgrades are milestone-gated. Do not chase every
  release automatically.

## Local And CI Gates

Run these before pushing Rust or engine work:

```sh
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI mirrors these commands without the `dynamic` feature. The `dynamic` feature
is for local incremental development only.

Manual macOS smoke validation is required for engine bumps:

- Launch with keyboard/mouse.
- Validate at least two Xbox-style controllers.
- Join players through player select and enter `Playing`.
- Confirm split-screen cameras spawn for local players.
- Pause/resume, manual save, and save-and-title from pause.
- Exercise Chapter 1 movement: run, jump, wall slide, wall jump, hang, climb,
  dodge, parry, jetpack, slingshot, moving platform, and rotating elevator.
- Fire primary beams, use one special tool, defeat at least one enemy, and check
  HUD feedback on the correct player.
- Board the Chapter 1 boat with one driver and at least one passenger, dock, and
  disembark.
- Save, quit to title, reload, and verify per-player stats/blueprints survive.

## Future Engine Upgrade Procedure

1. Create a branch named `codex/engine-upgrade-<version>`.
2. Read the official Bevy migration guide, Bevy release notes, Rapier/Bevy
   compatibility notes, and any affected crate release notes.
3. Update `Cargo.toml` and regenerate `Cargo.lock`.
4. Fix compile/API changes by subsystem, keeping changes scoped:
   - app/bootstrap/plugins
   - events/messages
   - rendering/materials/cameras
   - hierarchy/despawn/children
   - input/window/cursor
   - physics/colliders/Rapier config
   - UI/text/layout
   - save/progression tests
5. Run the local gates and the manual macOS smoke checklist.
6. Update this runbook with a short migration note.
7. Push the branch and merge only after local and CI gates pass.

If the upgrade requires a risky architecture choice, stop and document the
decision before changing broad gameplay code.

## Milestones

### M0: Baseline And Documentation

Goal: make the Bevy 0.18 migration a clean, discoverable baseline.

- Record Bevy `0.18.1` and Rapier `0.34` in active docs.
- Keep `agent.md` as the concise Codex handoff and point it here for details.
- Keep `README.md`, `docs/architecture.md`, `docs/systems.md`, and
  `docs/improvements.md` aligned with the current engine.
- Remove stale Bevy `0.15` and Rapier `0.28` references from active docs.

Acceptance:

- No stale engine-version references remain in active docs.
- CI exists and mirrors the local gate commands.
- The repo documents known migration debt instead of hiding it.

### M1: Multiplayer Ownership And Save Foundation

Goal: make local multiplayer ownership and saves reliable before broad polish.

Default ownership policy:

- Campaign-shared: chapter progress, chapter objectives, kill gates, boss
  phases, unlock progression, and `PerkTree`.
- Per-player: inventory, rewards, HUD, feedback, companions, crafting ownership,
  runtime stats, character blueprints, and save `players[]` records.
- Vehicle mode stays party-shared for now: one active driver/vehicle mode with
  passengers keyed by `PlayerIndex`.
- Any player may pause/resume. Save actions stay party-wide.

First automated tests:

- Current save round-trip preserves all `players[]` records.
- Legacy top-level save fields still hydrate old saves.
- Per-player stats and character blueprints do not drift between slots.
- Reward, feedback, and companion ownership use `PlayerIndex`, not query order.

Acceptance:

- Ownership rules are documented in architecture/system docs.
- First save/ownership tests run in `cargo test`.
- New multiplayer code does not introduce global single-player assumptions.

### M2: Engine Quality Gates

Goal: make upgrades and high-risk gameplay changes repeatable.

- Keep GitHub Actions green for formatting, check, strict clippy, and tests.
- Add smoke tests for app/plugin setup and message registration after the
  ownership/save tests exist.
- Track manual macOS controller validation for each engine bump.
- Add a short verification note to PRs or commits that touch engine systems.

Acceptance:

- Future engine bumps are not complete unless local, CI, and manual gates pass.
- Any intentionally skipped gate is documented with a concrete blocker.

### M3: Engine Hygiene And Upgrade Repeatability

Goal: pay down migration debt without broad rewrites.

- Keep local render bundle shims in `src/rendering.rs` only while they reduce
  churn. Replace them gradually when touching nearby spawn code.
- Prefer Bevy 0.18-native APIs for new work: messages, current hierarchy
  components, current camera/HDR patterns, current cursor resources, and
  current UI/text layout types.
- Keep Rapier collider creation fallibility explicit; generated geometry may
  use `expect` only when the mesh generation invariant is documented.
- Keep `cargo clippy --all-targets -- -D warnings` passing. Bevy system
  signature allowances belong at crate level; do not hide correctness warnings.

Acceptance:

- Every future engine upgrade ends with a short migration note in this file.
- New code follows current Bevy/Rapier APIs instead of extending compatibility
  debt.

### M4: Full Roadmap After Foundation

Goal: connect engine work to production game progress.

After M1-M3, prioritize:

- Chapter 1 vertical slice polish: final movement feel, controller-first HUD and
  perk UI, authored secret/cave/relic beats, boat route validation, combat
  feedback, and chapter-complete rewards.
- Production systems: content authoring formats, asset pipeline rules, debug
  overlays, deterministic smoke scenes, profiling budgets, and save/version
  tooling.
- Core game depth: enemy roles, dragon/domain boss phases, cooperative rules,
  biome-specific caves/airships/relic courses, and RPG build choices.
- Presentation: production assets, animation, audio, VFX, lighting, weather,
  post-processing, and split-screen-safe composition.
- Ship readiness: settings, accessibility, remapping, captions, difficulty,
  save backups, crash-safe writes, packaging, localization hooks, and
  reproducible builds.

Acceptance:

- Gameplay roadmap work remains tied to engine needs when it touches input,
  controller UX, save schema, debug overlays, profiling, asset pipeline,
  split-screen performance, or crash-safe saves.

## Migration Notes

### 2026-06-07: Bevy 0.18 Baseline

- Upgraded Bevy to `0.18.1` and `bevy_rapier3d` to `0.34`.
- Migrated buffered gameplay events to Bevy messages.
- Updated hierarchy/despawn, query single helpers, camera/HDR, cursor options,
  render imports, bloom, ambient lighting, optional image data, UI border
  colors, and fallible Rapier trimesh creation.
- Added `.DS_Store` ignore/removal while publishing the baseline.
- Verified with `cargo fmt`, `cargo check`, and `cargo test`.

### 2026-06-07: M1 Ownership/Save Foundation

- Documented the default shared-vs-per-player ownership policy in architecture
  and systems docs.
- Added pure save tests for per-player record round-trips, legacy save
  hydration, `player_index` matching independent of record order, sorted save
  data output, and clamped runtime application.
