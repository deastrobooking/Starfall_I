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

Only worth doing once particle counts or module complexity actually pressure
frame time on the CPU path (measure first, per this codebase's engine-roadmap
principle). When it is:

1. Each `ModuleSpec` gains a WGSL snippet alongside its Rust interpretation in
   `apply_update_module` (both stay in sync off one source of truth — the
   `ModuleSpec` — the same way `platformer_chunks` keeps one validator both
   the game and its tests consult).
2. A compiler pass concatenates the WGSL for a `CompiledEmitter`'s modules
   into one compute shader, cached by content hash of the module list so
   identical module combinations across emitters/systems share a pipeline.
3. Particles move from ECS entities to a storage buffer per emitter; spawn
   and update become two dispatched compute passes; rendering becomes one
   indirect draw per emitter instead of N entity draws.
4. CPU path stays as the reference implementation and as the low-particle-count
   fallback (mobile/low-power, or debug builds where GPU readback for tests
   matters).

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
