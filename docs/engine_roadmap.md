# Starfall Engine Core Roadmap (`EC#`)

For the July 2026 audit of menu input, racing roads, movement feel, and weapons,
see `docs/game_review_2026-07.md`.

> **Track prefix:** `EC#` — *Engine Core*. The substrate that makes the game feel
> like a tight character-action game: fixed-tick determinism, input buffering,
> frame-data combat, profiling, and (eventually) reusable engine crates.
>
> Sits alongside the existing roadmaps:
> - `M#`  — engine/campaign strategy (`docs/engine_upgrade_milestones.md`)
> - `MM#` — motion mechanics / traversal (`docs/playerengine.md`)
> - `AI#` — enemy AI behavior (future)
>
> **North star:** *A low-latency, 4-player-first character-action engine on Bevy.*
> Bevy owns ECS/render/assets/scheduling/windowing. Our engine layer owns the
> parts that make the game feel insane: movement, combat, camera, hitstop,
> determinism, controller latency, and ability logic.

## Game intent fit

Starfall I is not an abstract engine lab. Engine work should serve the playable
promise first:

- **Family/couch co-op first.** One to four local players, shared/split camera,
  controller ownership, identity readability, and fair off-screen behavior are
  core engine requirements, not polish extras.
- **Fast traversal across a real heightmap.** The signature feel is star-tech
  movement: swing, jetpack/hover, dash, parkour, roads, bridges, and mountain
  routes that invite fast travel. Travel surfaces must be terrain-aware: visual
  mesh, collider, grade, jump clearance, and heightmap elevation should agree.
- **Readable action-RPG combat.** Combat should land between Mario readability
  and Secret-of-Mana pacing: simple to parse in co-op, but precise enough for
  frame data, hitstop, cancels, knockback, and character identity.
- **RPG/world systems support the action.** Settlements, raids, pets, chapter
  select, hacking, and save-backed world state are there to create reasons to
  move and fight, not to turn EC# into a detached strategy engine.
- **Extract only after Starfall proves the need.** The reusable "20 games"
  engine is a payoff of making Starfall feel excellent first.

## Guiding principles

1. **Substrate-first, then feel, then content/extraction.** Make the loop
   deterministic and measured before tuning feel; tune feel before extracting
   reusable crates. *"The engine is the controller."*
2. **Non-destructive.** The existing motor, camera, state machines, and content
   stay. We slide a deterministic, profiled, frame-data substrate *underneath*
   them — migrations, not rewrites.
3. **Physics for queries, custom motor for feel.** Avian handles colliders,
   shape/ray casts, moving platforms, props, ragdolls. The custom motor owns
   acceleration, swing, dash, buffering, coyote time, lock-on, hitstop.
4. **Measure before optimizing.** Profiling lands first (EC0). Assembly/SIMD are
   the last 1%, only where the profiler proves a hot loop (likely
   particles/projectiles/crowds — never the 4 player motors).
5. **Extract from a proven game, never speculate.** The workspace crate split
   (EC7) happens only after the core is real and used in 2+ contexts.

## Engine version policy

- **Track current stable Bevy deliberately.** In open source, staying close to
  the active engine release keeps us near docs, examples, bug fixes, community
  answers, and plugin momentum.
- Current local engine branch baseline is Bevy `0.19.0` + Avian `0.7`. Local
  compile, strict clippy, and automated test gates pass; manual macOS
  controller/play smoke is still required before merging the engine branch.
- Bevy 0.19 is available: the official news post announced it on June 19, 2026,
  and GitHub's `v0.19.0` release tag is marked latest. Its relevant wins matter
  now, not only later:
  - Visual/world scale: big-scene rendering improvements, contact shadows,
    improved skinned-mesh culling, and contiguous query access.
  - Tools/UI: text input, app settings, diagnostics overlay, Feathers/BSN
    improvements, and richer authored scene workflows.
- Upgrade on an isolated branch. If physics-backend parity is not ready,
  document the exact blocker and choose deliberately: wait for compatibility,
  evaluate an alternative backend, or park the branch. Do not mix the engine bump
  with feel tuning.

---

## Maturity scorecard (2026-06-26 intent/status review)

| Subsystem | Maturity | Key gap |
|---|---|---|
| 4-player camera (split↔shared, threat-aware) | Premium | Off-screen indicators; in-game drop-in/out; per-player identity polish |
| Multi-controller input | Strong | Fixed input buffer exists, but motor/combat do not consume it yet; no remap/replay history |
| Character motor (swing/jetpack/edge-grab/dodge/buffer/coyote) | Strong (~70%) | Default path still frame-rate dependent; fixed motor experimental; no wall-run |
| Movement tuning (`PlayerMovement`/`MovementProfile`) | Data-driven | — |
| State machines | Clean | — |
| Animation (procedural, motor-driven, 2-bone IK) | Elegant but rigid | No authored clips / no skinning |
| Combat | Functional (~30% data) | No frame data, cancel windows, hitstop; knockback unused; no hit/hurt layers |
| Loop / timing | Foundation in place | Fixed tick + input buffer + default-off fixed motor exist; no interpolation/full gameplay migration |
| Profiling | Basic EC0 | Tracy feature, diagnostics, F11 overlay; no deep per-system budgets/hot-loop traces |

---

## Milestones

### EC0 — Instrumentation & ordering *(safety net — landed; assignment polish remains)*
**Goal:** Be able to measure the frame, and establish explicit system ordering.
- `tracy` cargo feature → `bevy/trace_tracy` (`cargo run --release --features tracy`).
- `FrameTimeDiagnosticsPlugin` + `EntityCountDiagnosticsPlugin`.
- In-game perf overlay (FPS / frame ms / entity count), F11 toggle, matching the
  `ControllerDiagState` (F8) pattern.
- `GameSet` ordering skeleton: `SampleInput → BuildCommands → Motor → Combat →
  PhysicsRead → Animation → Camera → Presentation`, configured centrally.
**Acceptance:** overlay shows live FPS/frame-time; `GameSet` ordering compiles and
is configured; budgets documented (sim < 2–4 ms, motor < 0.5 ms, combat < 0.5 ms).
**Status:** perf overlay + Tracy flag + `GameSet` scaffold landed via `GameLoopPlugin`
(`src/game_loop.rs`). Per-system `.in_set()` assignment completes in EC1.

### EC0.5 — Current Bevy tracking *(open-source freshness gate)*
**Goal:** Move the project to the current stable Bevy line without breaking the
playable Starfall loop.
- Create `codex/engine-upgrade-0.19`.
- Read the official 0.18→0.19 migration guide, 0.19 release notes, and
  physics-backend compatibility notes before changing code.
- Update Bevy and the physics backend together when a compatible path exists.
  If physics parity is blocked, capture the blocker and the fallback decision
  instead of hiding it inside gameplay work.
- Fix migration changes by subsystem: app/plugins, schedules, rendering,
  UI/text, asset loading, physics/colliders, input, diagnostics, and tests.
**Acceptance:** `cargo check`, strict clippy, targeted road/terrain/controller
tests, full `cargo test`, and a manual macOS smoke route pass. Main stays on the
last green engine baseline until the branch passes.
**Status:** local compile, clippy, and test gates pass on Bevy `0.19.0` + Avian
`0.7`; manual smoke remains before merge.

### EC1 — Fixed-tick simulation core *(the keystone)*
**Goal:** Deterministic, frame-rate-independent simulation.
- Move motor / combat / enemy-AI / physics-read into `FixedUpdate` (start 64 Hz,
  A/B test 120 Hz). Configure `Time<Fixed>`.
- Per-player **input ring buffer** sampled in `RunFixedMainLoop` *before* fixed
  loop; expose `pressed / held / released / buffered` commands; consume on tick.
- **Render interpolation:** cache `PreviousTransform`, lerp visuals by tick alpha
  in `RunFixedMainLoop` *after* fixed loop.
- Assign all gameplay systems to the EC0 `GameSet`s.
**Acceptance:** gameplay speed identical at 30/60/120/144 Hz refresh; visuals
smooth (no jitter) under frame drops; input latency visible in overlay.
**Status:**
- *EC1a (done)* — `Time<Fixed>` pinned to `FIXED_HZ` (64), `FixedTickCount` + perf
  overlay "sim ticks" line (`src/game_loop.rs`), and a unit-tested per-player
  input command buffer (`src/input_buffer.rs`: latch every render frame in
  `RunFixedMainLoop::BeforeFixedMainLoop`, consume once per `FixedUpdate` tick →
  `PlayerInputBuffers::fixed(idx)`). **Additive** — motor still reads `PlayerInput`
  in `Update`, so behavior is unchanged; 3 tests cover the buffer, and the local
  suite was green when this landed.
- *EC1b (implemented behind a default-off toggle — needs hardware feel-testing)* —
  `SimConfig.fixed_motor` (env `STARFALL_FIXED_MOTOR=1` or **F10** at runtime).
  OFF = the original `Update` chain, byte-identical (default). ON = the sim
  sub-chain (`traversal → grapple → motor → impact`) runs in `FixedUpdate` at
  `FIXED_HZ`; per-tick translation accumulates in `PlayerMovement.motor_accum`
  and `flush_motor_translation` applies the sum to the compatibility controller
  once per frame in `PostUpdate` before `PhysicsCompatSet::CharacterController`.
  Smoke-tested: ON path boots + runs with no panic;
  local suite was green at landing. **Still to do in EC1b polish:**
  `PreviousTransform` render interpolation (expect minor judder >`FIXED_HZ` fps
  until then); switch the motor to read the EC1a `FixedInput` buffer instead of
  `PlayerInput` directly; revisit Avian stepping if per-frame coalescing proves
  insufficient.

### EC2 — Collision layers + frame-data combat
**Goal:** Fighting-game precision inside the action RPG.
- Physics layers: `body / hurtbox / hitbox / pushbox / grapple-sensor /
  interaction` (Avian collision layers/groups).
- `MoveDef` data assets: startup/active/recovery frames, cancel windows, hitstop,
  self-freeze, knockback, meter cost, armor, movement lock. The hardcoded
  `LIGHT_COMBO` tuples become data.
- Hitbox lifecycle (spawn on active frames, despawn on recovery); wire the unused
  `DamageInfo.knockback_force`; per-move i-frames; hitstop; on-hit screen shake
  (reuse per-player `CameraShake`).
**Acceptance:** a move's frames/cancels are editable as data without recompiling
gameplay logic; hits produce hitstop + knockback + shake.

### EC3 — Motor to "feels amazing"
**Goal:** Lock in Spider-Man + Mega Man feel on the deterministic base.
- Wall-run, swing apex control, dash-snap, combat lock-on, PvP soft pushboxes.
- Heightmap-aware travel surfaces: roads, bridges, mountain paths, and speed
  loops sample terrain elevation, smooth grades/curves, and keep visual mesh,
  collider, and jump-clearance checks aligned.
- Folds into the `MM#` motion roadmap.
**Acceptance:** the movement prototype feels great in a focused test arena.

### EC4 — Animation upgrade
**Goal:** Authored clips + procedural layering; decide skinning.
- Keep the procedural motor-driven layer; add Bevy `AnimationGraph` authored
  clips blended on top; layered IK (head look, hand-to-target, foot placement).
- Decide skinned-mesh vs. rigid-part. If Bevy 0.19 has landed by then, lean on
  its improved skinned-mesh culling for production characters.
- Aligns with `MM7–MM9`.
**Acceptance:** base locomotion uses authored clips; procedural IK corrects to
ground/targets; motor state drives blend weights.

### EC5 — 4-player polish
**Goal:** Close the couch-co-op gaps.
- Off-screen player arrows; in-game drop-in/drop-out; per-player in-world
  identity coloring; input remapping; pause/menu ownership finalized.
**Acceptance:** a 4th controller can join mid-session; players never lose track of
their character in split/shared transitions.

### EC6 — Determinism payoff
**Goal:** Cash in EC1's determinism.
- Deterministic input log → instant replay, ghosts, training mode, combo trials,
  desync detection. On-ramp to future online rollback (not online yet).
**Acceptance:** record + replay a session deterministically; replay matches live.

### EC7 — Engine extraction (the 20-games play, LAST)
**Goal:** Reusable engine for future games.
- Convert to a Cargo workspace; extract proven layers: `forge_geometry`,
  `forge_assembly` (socket builder), `forge_render` (toon/wind/cloth shaders),
  `forge_motor`, `forge_combat`, `forge_play` (4-player rig).
- Data-drive content (parts/recipes/heroes/worlds as RON assets).
- Prove it by standing up game #2 as a mostly content-only crate.
**Acceptance:** Starfall I compiles against the engine crates; game #2 is data +
thin glue.

---

## What to explicitly NOT do yet
- ❌ Hand-written assembly / SIMD (wait for EC0 profiler evidence).
- ❌ Ungated engine bumps on main (current-Bevy tracking happens on a branch and
  lands only after compile/test/smoke validation).
- ❌ Workspace split before EC7 (premature; boundaries unproven).
- ❌ `bevy_silk` cloth (cape is an EC4 vertex-shader job reusing `grass.wgsl`).
- ❌ Rollback netcode now (EC6 deterministic logs first).

## Open questions
- **Genre spread of the "20 games":** all character-action like Starfall, or
  genuinely different (racing/builder/shooter)? Decides how general the EC7
  crates (`forge_play`, `forge_world`) must be.
- **Sim tick rate:** 64 vs 120 Hz — settle with EC0 profiler data in EC1.
# Rendering shader pass

The staged custom-material and shader plan is tracked in
`docs/rendering_shader_pass.md`. Rendering R1 delivered branchless scene-lit
Toon, configurable GPU Grass, and procedural world Water. R2 now integrates a
bounded five-handle Energy combat palette, per-player damage/parry Shield
pulses, and typed Ice/Snow plus Lava materials in Forge. Clustered-light
evaluation, authoring sliders, and measured four-viewport GPU budgets remain.
