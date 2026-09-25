# VFX Forge — "Niagara-lite" Plan

A data-driven particle VFX system: artist-editable emitter behavior that ships
as data and retunes without an engine recompile. This document reviews what
existed before this work, what Phase 1 ships, and the phased path to a node
graph editor and a GPU compute backend.

## What existed before this plan

Nothing generic. Every visual effect in the game is a hand-rolled, single-purpose
Bevy system: `combat::feedback::spawn_hit_flash` spawns one scaling sphere
entity per hit with a hardcoded 0.12s timer and a fixed color; weapon muzzle
flashes, charge blasts, and death dissolves are each their own bespoke
component and system in `plugins/weapon_plugin.rs`, `robots/*`, and
`combat/feedback.rs`. None of it is data — retuning a color curve or a spawn
count means editing Rust and recompiling, and no two effects share simulation
code even though they all do the same three things (spawn particles with some
initial randomness, integrate motion, fade color/size over a lifetime).

Two pieces of relevant infrastructure already existed and this plan builds on
both rather than reinventing them:

- **`starfall-graph`** — the typed authoring-graph kernel (`StableId`,
  `GraphDocument`, `NodeRegistry`) shared by Forge tools, explicitly scoped to
  cover "object, behavior, animation, UI, **shader**, world, and narrative"
  graphs. This is the reuse point the original design sketch called "the
  generic node-graph crate flagged in the Phase 4 tools roadmap" — it already
  exists here.
- **`starfall-platformer-graph`** — the closest working precedent for the
  exact shape this system needs: an authoring `Document` type with
  `validate_schema()` and `compile_runtime(resolver) -> Compiled...`, kept
  strictly separate from a `NativeNode`/`NodeRegistry` graph-editor adapter.
  `starfall-vfx-graph` (this plan's core crate) mirrors that split precisely.

## Architecture

```
Authoring (data, hand-written today; node-graph UI later):
  VfxSystemDocument { emitters: [EmitterDef] }
    EmitterDef { spawn: SpawnDef, update_modules: [ModuleInstance], renderer, loop_mode }
    ModuleInstance { kind: String, params: [(String, VfxParam)] }
      ↓ VfxSystemDocument::compile_runtime(&ModuleRegistry)
  CompiledVfxSystem { emitters: [CompiledEmitter] }   ← registry-validated, ready to run

Runtime (Phase 1, CPU, shipped):
  SpawnVfxEvent{system_id, position} → VfxEmitterInstance entity (spawn timing only)
    → spawns VfxParticle entities (independent, own Transform + StandardMaterial)
    → per-frame: apply_update_module() per CompiledModule, sample color/size-over-life
    → despawn on lifetime end

Runtime (Phase 2, GPU compute, planned):
  Same CompiledModule list → WGSL codegen (one compute shader per unique module
  combination, cached by content hash) → storage-buffer particle pool →
  indirect draw. CompiledModule is already the exact input this needs; nothing
  in Phase 1 forecloses it.
```

The key design decision — carried over from the original design sketch and
validated against this codebase's own conventions — is keeping **authoring
format**, **compiler/registry**, and **runtime** as three separable layers.
`starfall-platformer-graph` already proves this separation works well here:
the compiler doesn't care whether a document was hand-written, loaded from
`assets/`, or produced by a future graph editor, and a runtime doesn't care
whether the compiler ran offline or in-process.

## Phase 1 — shipped in this pass

**Crate: `crates/starfall-vfx-graph`** (engine-agnostic, no Bevy dependency,
mirrors `starfall-platformer-graph`'s dependency footprint):

- `VfxSystemDocument` / `EmitterDef` / `SpawnDef` / `ModuleInstance` /
  `VfxParam` (Float, Vec3, Curve, Gradient) — the serializable authoring
  format (`serde`, round-trips through JSON).
- `ModuleRegistry` — a fixed table of native module kinds and their
  parameter contracts (`ModuleSpec { kind, params: &[(name, ParamType)] }`).
  `ModuleRegistry::builtin()` ships 10 kinds: `gravity`, `drag`,
  `curl_noise`, `color_over_life`, `size_over_life`, `spawn_position_sphere`,
  `spawn_velocity_cone`, `spawn_lifetime`, `spawn_color`, `spawn_size`.
- `VfxSystemDocument::validate_schema()` / `compile_runtime(&registry)` →
  `CompiledVfxSystem`. An authored document with a typo'd module kind, a
  missing required parameter, or a wrong-typed parameter **fails to compile**
  rather than silently no-opping — the same contract
  `platformer_chunks::ChunkDef::validate` established for chunk geometry.
- `sample_curve` / `sample_gradient` — shared lerp sampling any runtime needs.
- 11 unit tests: compile success/failure per error class, JSON round-trip,
  schema rejection of unsorted bursts/gradients, curve/gradient sampling math.

**Runtime: `src/engine/vfx.rs`** (`VfxPlugin`, registered in
`framework::StarfallFoundationPlugins` — available in both editions):

- `VfxCatalog` — the built-in compiled systems, hand-authored as
  `VfxSystemDocument`s in Rust and compiled once at `Startup`, **plus** any
  `<asset_root>/vfx/*.json` file: `load_authored_documents` parses, validates,
  and compiles each one, overriding a built-in id or adding a new one. A
  missing `vfx/` directory is a first-class empty state and a bad file is
  skipped with a warning rather than crashing — the same contract
  `world::published_content` uses for every other loose content type. This is
  the actual "no engine recompile" delivery: drop a JSON file next to the
  executable and retune or add an effect, no rebuild required.
- Two shipped systems:
  - **`impact_spark`** — a 10-particle one-shot burst, wired live to
    `EnemyDamagedEvent`: every enemy hit in the actual game now fires a
    data-driven spark burst, additively alongside the existing hit-flash orb.
  - **`ember_torch`** — a looping ember field for castle/stairwell torches.
    Compiled and tested, not yet spawned anywhere — see Near-term follow-ups.
- CPU particle simulation: each live particle is an ordinary entity
  (`Mesh3d` sphere + a per-particle `StandardMaterial` instance so
  color-over-life can vary independently), budgeted at
  `MAX_LIVE_PARTICLES = 500` shared across every emitter — the same
  budget-guard idiom `combat::feedback::spawn_hit_flash` already uses.
- `SpawnVfxEvent { system_id, position }` is the public integration point:
  any gameplay system can trigger a built-in effect by writing one message,
  with no dependency on this module's internals.

### What Phase 1 deliberately does not do

| Original design sketch | This slice |
|---|---|
| WGSL codegen, GPU compute simulation | CPU simulation, entity-per-particle |
| Node-graph editor authoring | Hand-authored Rust documents (same schema a future graph would emit) |
| Sprite/ribbon/mesh/light renderers | Particles render as small unlit orbs (matches the existing hit-flash convention) |
| `assets/published/` publish pipeline integration | Loose `assets/vfx/*.json` files load and override the Rust catalog, but there is no Designer authoring UI or Forge project/versioning yet |
| Event system between emitters (death → spawn) | Not modeled; `SpawnVfxEvent` chaining could add this later |
| Data interfaces (skinned-mesh/spline sampling) | Not modeled |

None of these cuts are permanent — they are the same "ship the compiler and a
CPU runtime first" sequencing the original chat itself recommended, and every
one of them consumes the same `CompiledModule`/`ModuleRegistry` surface
Phase 1 already built, so nothing here needs to be rewritten to unlock them.

## Phase 2 — GPU compute backend

The original sequencing here said this was "only worth doing once particle
counts or module complexity actually pressure frame time on the CPU path
(measure first)" — that measurement was never taken; this slice went ahead
anyway at the user's explicit direction. Worth knowing if you're deciding
whether to build on it further: nothing about the CPU path (still the only
thing the live game runs) was shown to need this yet.

**Shipped in this pass — steps 1 and 2, plus real, GPU-verified proof of
correctness, not just working code:**

1. `starfall-vfx-graph::GpuModuleRegistry` gives `gravity`/`drag`/`curl_noise`
   — the three *motion-affecting* update modules — a WGSL translation
   alongside their existing Rust interpretation in `apply_update_module`
   (`src/engine/vfx.rs`, refactored from a `&mut VfxParticle` mutator to a
   pure `(module, velocity, age, dt) -> velocity` function so both sides can
   be called and compared directly). `color_over_life`/`size_over_life` are
   deliberately not covered: they're a single per-particle curve/gradient
   lookup per frame, not the iterative math a compute kernel exists to
   offload.
2. `compile_update_kernel(&GpuModuleRegistry, &[CompiledModule]) ->
   CompiledGpuKernel` concatenates an emitter's update-module chain into one
   self-contained WGSL compute shader (particle struct, storage-buffer
   bindings, every module function the chain needs defined once even if a
   kind repeats, and the entry point that calls them in authored order),
   plus the packed `[f32; 4]`-per-module parameter buffer to upload
   alongside it. `cache_key` (the joined module kind names) is what a future
   runtime would key pipeline reuse on — two emitters with the same ordered
   module *kinds* share one compiled pipeline regardless of parameter
   values. 7 unit tests, all pure string/data assertions, no GPU needed:
   codegen contains the right function names and call order, params pack
   into the right slots, identical chains share a cache key, an unsupported
   module kind (e.g. `color_over_life`) fails to compile rather than being
   silently dropped.
3. **`src/engine/vfx_gpu.rs` + `examples/vfx_gpu_validate.rs`**: a real,
   dispatched, GPU-verified correctness proof — not a claim it "should"
   work. Mirrors `render_lab::probe_gpu`'s exact one-shot
   dispatch/readback/compare pattern: six fixture particles run through the
   compiled `gravity+drag+curl_noise` kernel on the GPU (`ExtractResourcePlugin`
   → `prepare_pipeline`/`prepare_bind_group` → dispatch from the render graph
   → `Readback`/`ReadbackComplete`), and the result is compared against
   `apply_update_module` — the same function the live CPU path calls — run
   in plain Rust for the same fixture and `dt`. Actually run on this
   project's own hardware (Metal, Apple M3 Pro) via
   `cargo run --example vfx_gpu_validate --features heavy-water-demo`:
   **0 mismatches across 6 particles, max absolute error ~1.9×10⁻⁹** — floating-
   point noise, not a real disagreement. Re-run twice more to confirm this
   wasn't a fluke; identical result both times.

**Explicitly not done — this is a correctness proof for a compute kernel, not
a working particle system:**

- **No continuous, steady-state simulation.** This is a one-shot dispatch
  against a hand-built fixture, exactly like `probe_gpu`'s own validation
  pass. Running this every frame against a live, growing/shrinking particle
  pool is a different, harder problem: double-buffered or in-place update
  without racing the spawn pass, particles entering/leaving the buffer as
  emitters spawn and particles expire, and a readback strategy that doesn't
  stall a frame waiting on the GPU (this pass's readback is fire-and-forget
  with no latency budget, fine for a validation tool, not fine for gameplay).
- **No GPU-driven spawning.** New particles still only exist by being
  written into the CPU-visible fixture at setup time here.
- **No rendering.** The fixture never becomes anything visible — no indirect
  draw, no sprite/mesh renderer reading the storage buffer. The CPU path
  (`VfxPlugin` in `src/engine/vfx.rs`) is still the only thing that puts a
  particle on screen, and remains so until a renderer is built.
- **Not wired into `VfxCatalog`/`VfxPlugin` at all.** `vfx_gpu.rs` is
  reachable only from its own example; the live game (both editions) never
  loads `VfxGpuValidationPlugin`.

**The actual next slice**, if this is picked back up: pick one currently-CPU
system (`ember_torch` is the natural choice — it already only uses
`drag`+`curl_noise`, both GPU-covered) and build genuinely continuous
GPU simulation for it: a persistent storage buffer sized to
`max_particles`, a spawn pass that claims dead slots, an update pass that
runs every frame instead of once, and *some* renderer reading that buffer —
even reusing the CPU path's per-particle-entity rendering by copying GPU
results back to Transforms would prove the loop before committing to
indirect draw. That is a materially bigger, real-time-systems-shaped task
than this pass; treat it as its own milestone, not a follow-up.

## Phase 3 — node-graph editor

Once Phase 1's compile target is proven in real gameplay (it now is —
`impact_spark` ships live):

1. Register VFX node kinds against `starfall-graph::NodeRegistry`, following
   `starfall-platformer-graph::register_platformer_nodes` exactly: one native
   node per module kind, a spawn-root node, and an output/renderer node.
2. A graph-editor screen in the shared DCC tool-window shell
   (`docs/guides/designer-workflow.md`'s "Tool windows" contract) that reads
   the registry to populate its node palette generically — no per-module UI
   code, matching the registry's whole reason for existing.
3. "Compile" walks the graph, produces a `VfxSystemDocument`, and calls the
   exact same `compile_runtime` Phase 1 already ships — the editor never
   needs its own validation logic.
4. Wire into the Publish pipeline (`engine_tools::publish`,
   `world::published_content`) the same way platformer routes are published:
   a `vfx_systems.json` manifest file, a `PublishedVfxCatalog` resource, and a
   `valid_published_vfx_systems` loader that validates+compiles at load time
   and treats a bad document as "skip it," never "crash the game" — the
   established contract for every other published content type.

## Shipped since Phase 1's first pass

- **Loose file loading**: `<asset_root>/vfx/*.json` documents load at
  `Startup` alongside the built-in Rust catalog, overriding a matching id or
  adding a new one — a bad file is skipped with a warning, never fatal.
  `SpawnVfxEvent::system_id` and the emitter/particle components moved from
  `&'static str` to `String` to support ids that only exist at runtime.
  2 new tests (`load_authored_documents`: missing-directory empty state,
  good-file-loads-and-bad-file-is-skipped).

## Near-term follow-ups (not gated on Phase 2/3)

- Spawn `ember_torch` from castle/stairwell chunk trim pieces in
  `world::platformer_chunk_library`. `world::platformer_route_spawn::spawn_route`
  is a plain function called once at level-build time (`&mut Commands`, no
  `MessageWriter` in scope) — wiring this in cleanly means either threading a
  list of `(system_id, position)` ember spots back out of `spawn_route` for
  the caller to fire, or giving `Commands` a way to queue a message write
  directly. Either is a small, self-contained change; deferred only because
  it touches the chunk-authoring surface rather than because anything is
  currently broken.
- True camera-facing sprite billboards (a textured quad rotated to face the
  active split-screen camera) instead of the current unlit-orb approximation,
  for effects where a flat glow sprite reads better than a small sphere
  (smoke, dust, glow blooms).
- A `spawn_vfx_on_weapon_fire` hook analogous to the damage hook. Blocked on
  more than just authoring a muzzle-flash document: `WeaponFiredEvent` is
  currently a unit struct with no position, fired from ~8 call sites across
  `plugins/weapon_plugin.rs`. Giving it a position field means updating every
  writer (and checking nothing else consuming the event assumes it stays a
  unit struct) — a real but slightly bigger change than the hook itself,
  intentionally not bundled into this pass to keep it reviewable on its own.

## Verification

```
cargo test -p starfall-vfx-graph      # core crate: schema/compile/sampling
cargo test --lib engine::vfx          # runtime: built-in catalog compiles
cargo build --no-default-features --features heavy-water-demo
cargo build                            # designer edition (default features)
```
