# Starfall Engine

Starfall is a native Rust and Bevy framework for building modular 3D games with
local multiplayer, action gameplay, procedural worlds, and in-engine creation
tools. **Starfall Forge** is the authoring environment, and the complete
**Heavy Water Demo Game** is the framework's native feature showcase, learning
project, and forkable all-in-one starting point.

The project is both a playable game and an engine under active extraction. The
existing application already runs the complete demo and creator tools; the
public library boundary landed first, and graph, project-manifest, and
platformer-kit workspace crates are now independently buildable. The first
domain compiler turns typed platformer graphs into versioned route documents
and catalog-resolved runtime geometry. Forge project records now publish those
documents into the same atomic content generation as other assets; Game builds
validate them against Heavy Water's catalog and prefer matching route IDs over
built-in definitions. The Project Hub can now turn the active project into a
versioned standalone native Game folder with one background **EXPORT GAME**
action. Fine-grained runtime Cargo features and complete asset cooking remain
active framework work. The documentation distinguishes current behavior from
target architecture rather than presenting roadmap interfaces as shipped APIs.

## What is included

| Surface | Purpose | Current state |
|---|---|---|
| Starfall Engine | Bevy/Avian runtime, fixed simulation, input, rendering, reusable gameplay systems | Running in the demo; library facade available |
| Starfall Forge | Project, character, creature, weapon, vehicle, spaceship, world, dialogue, and platformer-route authoring | Native Designer build |
| Game exporter | Publish, optimized Game-only build, selected content snapshot, and standalone folder assembly | Project Hub background action |
| Heavy Water Demo Game | Open-world action RPG, shared platformer, racing/traversal, campaign, and 1–4 player examples | Complete native demo application |
| Graph framework | Typed object, behavior, animation, UI, shader, world, and narrative graphs | Neutral typed kernel plus platformer route compiler extracted |
| Templates and IDE | Generated projects, feature scaffolding, code integration, and guided workflows | Planned after module contracts stabilize |

The design principle is simple: **limit setup and accidental complexity, not
creative range**. Artists receive strong defaults and visual workflows;
programmers retain native Rust, Bevy, and WGSL extension points.

## Run the all-in-one application

Requirements: a current stable Rust toolchain and the native dependencies
required by Bevy, Avian, and audio capture.

```sh
git clone <your-fork-or-repository-url>
cd Starfall_I
cargo run
```

The default **Designer** build includes Forge and Heavy Water. To run the
complete demo without Forge entry points:

```sh
cargo run --no-default-features --features heavy-water-demo
```

To ship the active Forge project, open **CREATOR TOOLS**, choose or create the
project, and press **EXPORT GAME**. Forge publishes the project, builds the
optimized Game-only executable, and creates a new immutable folder under
`<project>/build/exports/`. The folder contains the executable, `assets/`, and
`starfall.bundle.json`; it does not need the Forge project or repository to
run. See [Exporting a Game](docs/guides/exporting-a-game.md).

The minimal framework exposes typed graphs, versioned manifests, the
renderer-neutral platformer kit, and its optional graph compiler without
compiling demo modules or the native executable:

```sh
cargo check --no-default-features
```

For faster local iteration with Bevy dynamic linking:

```sh
cargo run --features dynamic
```

Useful direct boot modes:

```sh
STARFALL_AUTOSTART=1 cargo run
STARFALL_PLATFORMER=1 cargo run
STARFALL_STUDIO=1 cargo run
STARFALL_EDITOR=1 cargo run
```

See [developer documentation](docs/DEVELOPMENT.md) for the complete environment
and verification reference.

## Use the current framework boundary

The repository now has a library target plus independently buildable graph,
platformer, platformer-graph, and project-manifest crates. During the
workspace transition, a local game or tool can depend on the all-in-one
framework directly:

```toml
[dependencies]
starfall-i = { path = "../Starfall_I" }
```

The complete native application factory is public:

```rust
fn main() {
    starfall_i::build_app().run();
}
```

This is intentionally the first compatibility boundary, not the final modular
API. New games will progressively select capability features and plugin groups
such as core, local multiplayer, character action, platforming, world, racing,
and Forge. Track that transition in the
[framework architecture](docs/FRAMEWORK_ARCHITECTURE.md) and
[project plan](docs/PROJECT_PLAN.md).

Tools and focused games can select only the extracted contracts they need:

```toml
[dependencies]
starfall-graph = { path = "../Starfall_I/crates/starfall-graph" }
starfall-platformer = { path = "../Starfall_I/crates/starfall-platformer" }
starfall-platformer-graph = { path = "../Starfall_I/crates/starfall-platformer-graph" }
starfall-project = { path = "../Starfall_I/crates/starfall-project" }
```

The checked-in `starfall.project.toml` and per-crate
`starfall.module.toml` files are schema-versioned, migrated, validated, and
round-trip tested. The all-in-one facade re-exports them as
`starfall_i::graph`, `starfall_i::platformer`, `starfall_i::platformer_graph`,
and `starfall_i::project`.

The platformer graph adapter is deliberately optional: code-authored games can
use `starfall-platformer` alone, while Forge-style route authoring adds
`starfall-platformer-graph`. Try the end-to-end graph, JSON, catalog, and
runtime assembly example with:

```sh
cargo run -p starfall-platformer-graph --example route_graph
```

## Heavy Water Demo Game

Heavy Water is the first-party consumer of Starfall and must use the same
public contracts available to future games. It demonstrates:

- one-to-four-player local input, split-screen, and shared-camera play
- an open 3D action-RPG campaign with traversal, combat, progression, cities,
  dungeons, settlements, vehicles, and world events
- a bounded shared-screen co-op platformer using the production player stack
  with an independently composed scene/rules lifecycle
- road, hoverboard, water, air, and space traversal/racing foundations
- procedural and imported characters, data-driven combat, audio, shaders, and
  published Forge content
- native creator workflows that move from project records to validated runtime
  catalogs

The game remains intentionally substantial. Users can play it, learn from it,
fork and rename it, remove feature modules, or keep the complete all-in-one
engine/game experience. Its detailed feature inventory is in
[Heavy Water features](docs/MASTER_FEATURES.md); its architecture role is in
[the demo-game guide](docs/games/heavy-water/README.md).

## Framework model

Starfall uses four ownership layers:

```text
Engine capability
      ↓
Reusable gameplay kit
      ↓
Heavy Water game-mode module
      ↓
Campaign and application composition
```

- Engine code supplies schedules, services, schemas, registries, and runtime
  contracts.
- Gameplay kits supply reusable mechanics such as movement, combat, local
  multiplayer, vehicles, and platforming.
- Demo modules combine those mechanics into Open World, Platformer, Racer, RPG,
  and Campaign features.
- Forge authors graphs and records, validates them, previews disposable ECS
  worlds, and publishes deterministic runtime data.

Graphs are the authoring format; typed plugins, ECS components, assets, and
compiled execution plans are the runtime format. Native code can register new
nodes, components, inspectors, importers, widgets, shader functions, validators,
and build steps.

## Current repository map

```text
crates/
  starfall-graph/    Independent typed graph documents and node registry
  starfall-platformer/ Renderer-neutral prefab, movement, chunk, and route kit
  starfall-platformer-graph/ Optional typed route authoring/compiler adapter
  starfall-project/  Independent project/module manifest parser and validator

src/
  lib.rs               Public framework boundary and deliberately small prelude
  main.rs              Thin native launcher
  app.rs               Complete Heavy Water/Forge application composition
  heavy_water/         Stable demo-game ownership facade during extraction
  engine/              Simulation, physics adapter, input buffer, rendering, paths
  engine_tools/        Current Forge records, editor services, and publishing
  plugins/             Bevy runtime and authoring system plugins
  components/          Shared ECS vocabulary
  character/           Character recipes, assembly, meshes, rigs, and presets
  character_studio/    Advanced native character authoring
  combat/              Damage, frame data, feedback, perks, tricks, and upgrades
  world/               World rules, game modes, missions, economy, and demo content
  audio/               Procedural SFX and user audio
  robots/              Robot/creature construction
  chapters/            Heavy Water campaign content

assets/                Current shared and Heavy Water assets (separation staged)
docs/                  Living architecture, guides, game reference, and archive
examples/              Focused code examples
starfall.project.toml  Authoritative project identity, paths, and modules
```

The target repeatable workspace and module filesystem is documented in
[PROJECT_STRUCTURE.md](docs/PROJECT_STRUCTURE.md). Do not infer the target tree
from the current transitional source layout.

## Documentation

- [Documentation map](docs/README.md)
- [Framework architecture](docs/FRAMEWORK_ARCHITECTURE.md)
- [Repeatable project structure](docs/PROJECT_STRUCTURE.md)
- [Build with Starfall](docs/guides/building-with-starfall.md)
- [Export a standalone game](docs/guides/exporting-a-game.md)
- [Create a module](docs/guides/creating-a-module.md)
- [Developer and verification guide](docs/DEVELOPMENT.md)
- [Starfall Forge roadmap](docs/editor_roadmap.md)
- [Engine roadmap](docs/engine_roadmap.md)
- [Heavy Water feature catalog](docs/MASTER_FEATURES.md)
- [Active project plan](docs/PROJECT_PLAN.md)

## Verification

```sh
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo check --workspace --all-targets --locked --no-default-features
cargo test --workspace --locked --no-default-features
cargo check --workspace --all-targets --locked --no-default-features --features heavy-water-demo
cargo test --workspace --locked --no-default-features --features heavy-water-demo
```

Detailed gates and native smoke tests are in
[docs/guides/verification.md](docs/guides/verification.md).

## Project status

Starfall is production-minded but pre-stable framework software. Save formats,
published content, and demo behavior already have compatibility concerns; the
new public framework API, module manifests, graph schemas, and package layout do
not yet carry a stable semver guarantee. Crates.io publishing remains disabled
until licensing, package contents, and compatibility policy are explicit.
