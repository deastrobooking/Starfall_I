# Starfall I — Developer Documentation

Everything needed to build, navigate, change, and verify the codebase. For
*what the game does*, see [FEATURES.md](FEATURES.md).

## Contents

- [Quick start](#quick-start)
- [Running and driving the game](#running-and-driving-the-game)
- [Repository layout](#repository-layout)
- [Architectural rules](#architectural-rules)
- [The simulation frame](#the-simulation-frame)
- [Data-driven combat](#data-driven-combat)
- [Local multiplayer and ownership](#local-multiplayer-and-ownership)
- [UI architecture](#ui-architecture)
- [Saves](#saves)
- [Verification](#verification)
- [Conventions](#conventions)
- [Naming reference](#naming-reference)

---

## Quick start

```bash
cargo run                 # play
cargo test                # full suite (fast; no GPU or window needed)
cargo clippy --all-targets
cargo build               # debug binary at target/debug/starfall-i
```

The project is **Bevy 0.19 + Avian 3D**, with a local physics compatibility
shim so gameplay code never talks to the physics backend directly.

Ship no new dependency without cause: the audio, mesh, and GLB-export paths are
all hand-rolled specifically to avoid one.

## Running and driving the game

Environment switches, all optional:

| Variable | Effect |
|---|---|
| `STARFALL_AUTOSTART=1` | Boot straight into `AppState::Playing` (skips menus) |
| `STARFALL_SEED=<n>` | Fix the RNG seed; the seed used is always logged |
| `STARFALL_LEGACY_MOTOR=1` | Run the player motor per-frame instead of fixed-tick |
| `STARFALL_STUDIO=1` | Boot into Character Studio |
| `STARFALL_EDITOR=1` | Boot into the editor flow |
| `STARFALL_MUSIC_DIR`, `STARFALL_SFX_DIR` | Override audio asset roots |

Useful in-game keys: **F6** music deck, **F8** controller diagnostics,
**F10** toggle the fixed-tick motor, **F11** performance overlay.

A headless boot smoke (no window, no GPU) is the cheapest way to prove the app
still starts and every plugin registers:

```bash
cargo build
STARFALL_AUTOSTART=1 STARFALL_SEED=42 \
  perl -e 'alarm 25; exec "./target/debug/starfall-i"' > /tmp/smoke.log 2>&1
grep -ci panic /tmp/smoke.log     # expect 0
```

## Repository layout

Source is grouped by **domain**, and each folder's `mod.rs` states what belongs
in it. When adding a file, put it where its *reason to change* lives.

```
src/
  main.rs        App bootstrap: plugin registration and boot overrides only

  commands.rs    ┐ Cross-cutting vocabulary every layer speaks. These stay at
  components.rs  │ the root deliberately — they are not clutter, they are the
  events.rs      │ shared types that would otherwise create circular folder
  resources.rs   ┘ dependencies.

  engine/        How the game RUNS, never what it is about. Fixed-tick loop and
                 GameSet ordering, Avian shim, per-player input latch, render
                 material plumbing, LOD, seeded RNG, AppState, platform paths.
                 Changing a weapon should never touch this folder.

  character/     The recipe → spawned figure pipeline: blueprint vocabulary,
                 shipped presets, the part/slot catalog, modular assembly, mesh
                 generation and deformation, hero stat profiles, base clips.

  combat/        Frame data (`data.rs` → assets/combat/moves.json), the single
                 `apply_damage` path, hitstop, hit feedback, hoverboard tricks,
                 and the progression that modifies them (upgrades, perks).

  audio/         Procedural SFX bus (`synth` generates every sound at startup,
                 `sfx` maps gameplay events onto it) plus the independent user
                 music deck.

  world/         Content rules: missions, raids, final war, settlement economy,
                 shop transactions, robot pets, hacking, dialogue.

  plugins/       Bevy plugins — the systems that drive everything above. One
                 plugin per feature area (player, weapon, enemy, world, ui, …).

  chapters/      The 14 chapter scripts and biome palettes.
  character_studio/, engine_tools/, robots/, lsystem/
                 In-game creation tools and their shared services.
```

Two rules keep this honest:

1. **`plugins/` holds systems; the domain folders hold rules and data.** A
   plugin reads `combat::data`; it does not invent its own damage math.
2. **Dependencies point inward.** `world/` may use `combat/` and `engine/`;
   `engine/` must not know about either.

## Architectural rules

**Physics goes through the shim.** `engine::physics` re-exports a stable
`Collider` / `RigidBody` / `KinematicCharacterController` surface over Avian.
Gameplay code imports `crate::engine::physics::prelude::*` and never Avian
directly, so a backend change stays contained.

**Collision layers are semantic.** `CollisionProfile` (World, Player,
EnemyHurtbox, PlayerHitbox, EnemyHitbox, PlayerProjectile, EnemyProjectile,
Pushbox, GrappleSensor, Interaction) converts into Avian `CollisionLayers`.
Assign the role, not the raw mask.

**One damage path.** Everything that hurts anything calls
`combat::damage::apply_damage`, which owns resistances, flat toughness, crits,
and knockback accumulation. Player-received damage additionally routes through
`damage_player` for armor and parry.

**Input ownership is explicit.** When a mode claims a button, it says so with a
predicate rather than relying on system order — see `sabre_claims_heavy_input`
and `hoverboard_claims_trick_input`. Those predicates are deliberately
independent of cooldowns so behaviour can never depend on which system ran
first.

## Editions

One codebase produces two products (see [PROJECT_PLAN.md](PROJECT_PLAN.md)):
the default build is the **Designer** edition; `--no-default-features` is the
consumer **Game** edition. The `designer` cargo feature gates every authoring
entry point — the Project Hub menu button, the Creature/Weapon Forge plugins,
the live-editor toggle (Tab / Select+Start), and designer env hooks. When you
add a tool, gate its entry point the same way; when you add gameplay, never
reference designer-gated modules from ungated code.

## The simulation frame

`GameSet` defines the canonical per-frame order, configured centrally in
`engine::game_loop`:

```
SampleInput → BuildCommands → Motor → Combat → PhysicsRead
            → Animation → Camera → Presentation
```

Systems opt in with `.in_set(GameSet::X)`. Player actions run in `Motor` and
attacks in `Combat`, so an action has always consumed its input before combat
resolves.

The player motor runs in `FixedUpdate` at `FIXED_HZ` (64). Two consequences
worth internalizing:

- **Edge inputs must be latched.** `engine::input_buffer` records presses every
  render frame and hands them to the tick. A new edge-triggered field on
  `PlayerInput` that is not added to the buffer *will* be dropped.
- **Translation delivery is smoothed.** Fixed ticks land on irregular frames, so
  `flush_motor_translation` banks movement in `motor_carry` and delivers a
  fraction per frame (`split_motor_carry`). Total displacement is conserved
  exactly; the constant trades smoothness against latency.

## Data-driven combat

Frame data is authored, not compiled. On first run the defaults are written to
disk; edit and relaunch to retune without recompiling. Delete a file to restore
defaults. Both libraries validate on load and fall back to defaults with a
warning rather than shipping broken combat.

| File | Contents |
|---|---|
| `assets/combat/moves.json` | Melee chains, sabre slash chain and techniques, energy wave, ranged weapon slots |
| `assets/combat/tricks.json` | Hoverboard trick roster, scoring, revert window |

A melee move runs a **Startup → Active → Recovery** phase machine with a
per-move hit registry (so a persistent hit window strikes each target once) and
a `cancel_after` window for chaining. Star Sabre slashes use the same machine.

## Local multiplayer and ownership

`PlayerIndex` is the stable runtime and save identity; a gamepad is a
*replaceable device* bound to that identity, never the identity itself. Pressing
Start on an unassigned controller claims the next open slot; disconnecting
releases the device but preserves the player. Every per-player system must key
off `PlayerIndex`, and per-player state belongs in components on the player
entity — a global resource silently breaks 2–4 player co-op.

## UI architecture

UI setup and teardown are anchored to `AppState` via `OnEnter`/`OnExit`, so no
node hierarchy outlives its state. Panels live in `plugins/ui_plugin.rs` and the
creator tools in their own plugins; shared draggable tool windows come from
`engine_tools::tool_windows`. `MenuFocus` provides shared controller menu
navigation so every screen is playable without a mouse.

## Saves

Saves are versioned and written atomically (temp file + rename) so a crash
mid-write cannot corrupt a profile. Per-player progression (`PlayerProgression`:
perks, upgrades, weapon ranks, shop ownership) is stored per `PlayerIndex`.
Legacy saves are migrated on load — when adding a field, provide a default and a
migration path rather than invalidating existing profiles.

## Verification

Both editions must stay green: run checks with default features (Designer) and
with `--no-default-features` (Game). A change that only compiles as Designer
is a broken consumer build.

In rough order of cost:

1. `cargo test` — unit and integration tests, no GPU required.
2. `cargo clippy --all-targets` — the tree is warning-clean apart from a few
   known dead-code items; keep it that way.
3. Headless boot smoke (above) — proves plugins register and assets load.
4. Manual play — the only way to judge feel. Tuning constants are deliberately
   exposed in JSON or as named constants for this reason.

Detailed procedures live in [guides/verification.md](guides/verification.md).

## Conventions

- **Comments explain why, not what.** Prefer a sentence about the constraint or
  the bug being prevented over restating the code.
- **Tests pin behaviour, not implementation.** Name them after the property
  being protected.
- **Tuning is data or a named constant** — never a bare number buried in a
  system.
- **`#![allow(dead_code)]` at file top is scaffolding debt.** Narrow it per item
  as features land.
- Keep public API docs on module `mod.rs` files current; they are the map new
  contributors read first.

## Naming reference

Player-facing text should use these names. Internal Rust identifiers may keep
older spellings when changing them would cause save or API churn.

- **Game:** `Starfall I` (short: `Starfall`); first chapter,
  `Invasion of the Scallarians`
- **Wizard Scientists:** Giacoma, Giovanni, Gabrio
- **Hero Brothers:** Vincenzo (oldest), and siblings — see the cast section of
  [FEATURES.md](FEATURES.md)
- **Antagonists:** the Scallarians

The full cast, place-name, and terminology list is preserved in
[naming.md](naming.md).
