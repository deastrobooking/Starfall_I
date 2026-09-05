# Building With Starfall

Starfall provides a minimal public library plus an optional complete Heavy Water
runtime. Fine-grained engine/gameplay capabilities remain in progress. This
guide deliberately separates shipped interfaces from target APIs.

## Run or fork the complete framework

The fastest supported path is the Heavy Water all-in-one application:

```sh
cargo run
```

Its executable is intentionally thin. The same application can be launched
from another local Rust package:

```toml
[dependencies]
starfall-i = { path = "../Starfall_I" }
```

```rust
fn main() {
    starfall_i::build_app().run();
}
```

`build_headless_app` constructs the same game registration without window,
renderer, or audio backends for tests and tools.

For the minimal graph, project-manifest, and platformer contracts without Heavy
Water modules or native rendering, physics, and audio dependencies:

```toml
[dependencies]
starfall-i = { path = "../Starfall_I", default-features = false }
```

The minimal facade still uses Bevy's app/ECS/math crates through the extracted
contracts. The full `bevy` package and its window/GPU/audio backends are selected
by native capabilities: `heavy-water-demo` (also implied by `designer`) or the
isolated `render-lab`. Avian stays behind `heavy-water-demo`.
The `dynamic` and `tracy` modifiers configure Bevy only when a native capability is selected;
they do not activate it on their own.

The separate `examples/framework_consumer` package depends only on this public
facade. It authors a two-chunk route, round-trips the compiled JSON, assembles
the route against its own catalog, and checks missing-catalog rejection:

```sh
cargo run -p starfall-framework-consumer --locked
python3 scripts/check_framework_dependencies.py
```

Select the consumer by package when checking isolation. A `--workspace` build
with the root package's defaults enabled unifies the demo features into the
consumer too. CI runs the isolated consumer on Linux without native game
system libraries and guards its dependency tree against runtime backends.

Tools can depend on the extracted contracts without pulling in the root game
package:

```toml
[dependencies]
starfall-graph = { path = "../Starfall_I/crates/starfall-graph" }
starfall-platformer = { path = "../Starfall_I/crates/starfall-platformer" }
starfall-platformer-graph = { path = "../Starfall_I/crates/starfall-platformer-graph" }
starfall-project = { path = "../Starfall_I/crates/starfall-project" }
```

`starfall-graph` includes a runnable native-node registration example:

```sh
cargo run -p starfall-graph --example native_extension
```

A focused platformer game can consume `MovementProfile`, `JumpEnvelope`,
`ChunkDef`, `RouteDefinition`, and `PlatformerPrefabKind` from
`starfall-platformer` without compiling Heavy Water, Forge, rendering, or ECS.
Run its complete custom-catalog example with:

```sh
cargo run -p starfall-platformer --example custom_route
```

Add `starfall-platformer-graph` only when routes should be authored as typed
graphs. Its `register_platformer_nodes` function contributes route Start,
Chunk, and End definitions to any `NodeRegistry` without changing the neutral
kernel. `compile_platformer_graph` produces an owned
`PlatformerRouteDocument`; serialize that as deterministic JSON, then call
`compile_runtime` with the game's chunk catalog and movement envelope. The
catalog keeps visuals and game content outside the reusable compiler.

Run the complete vertical slice:

```sh
cargo run -p starfall-platformer-graph --example route_graph
```

In the all-in-one Designer build,
`engine_tools::platformer_route_records::create_platformer_route` stores that
same graph as a typed Forge project record. Its project `content_id` identifies
the editable asset; the separate graph `route_id` is the durable runtime/save
identity. Publishing compiles all such records into
`platformer_routes.json`. Heavy Water uses a valid published document when its
runtime ID matches a built-in route and otherwise keeps the built-in level.

Select the complete demo runtime without Forge explicitly:

```toml
[dependencies]
starfall-i = {
    path = "../Starfall_I",
    default-features = false,
    features = ["heavy-water-demo"],
}
```

## Learn the current domains

For isolated rendering experiments, use the optional `render-lab` capability:

```sh
cargo run --release --locked --no-default-features --features render-lab \
  --example render_lab -- --views 4 --validate-probes \
  --output target/render-lab/four-views.json
```

This runs without Heavy Water or Forge plugins and exits after a bounded
warmup/measurement sequence. See the [rendering feature program](../RENDERING_PROGRAM.md)
for scene coverage, report semantics and the GI/geometry integration sequence.
Select `render-lab-meshlets` and pass `--renderer meshlets` for the experimental
resident geometry comparison. The lab records unsupported-device/build fallback
to PBR. `--capture new-file.png` captures the held final pose after measurement.

The public prelude contains only application and engine-level entry points.
Import gameplay types from their owning modules:

```rust
use starfall_i::components::player::PlayerIndex;
use starfall_i::engine::game_loop::GameSet;
use starfall_i::plugins::PlayerPlugin;
```

Keeping ownership visible prevents a convenience prelude from turning into an
unstable export of the entire codebase.

Heavy Water code should use the transitional public ownership facade:

```rust
use starfall_i::heavy_water::platformer::HeavyWaterPlatformerPlugin;
use starfall_i::heavy_water::shared::HeavyWaterProgress;
```

## Target custom-game workflow

After capability extraction, a game will choose compile-time features and then
activate matching runtime plugin groups:

```toml
# Target API; not shipped yet.
starfall = {
    path = "../../crates/starfall",
    default-features = false,
    features = [
        "core",
        "physics-avian",
        "local-multiplayer",
        "character",
        "movement",
        "combat",
        "platformer",
    ],
}
```

```rust,ignore
// Target API; not shipped yet.
App::new()
    .add_plugins(StarfallCorePlugins)
    .add_plugins(LocalMultiplayerPlugins)
    .add_plugins(CharacterActionPlugins)
    .add_plugins(PlatformerPlugins)
    .add_plugins(MyGamePlugin)
    .run();
```

Cargo features control compilation and dependency cost. Plugin groups control
which selected capabilities run in a particular application or mode.

## Add game logic now

Until game modules are physically extracted, follow the target ownership rules
inside the current tree:

1. Create a feature directory with a documented plugin boundary.
2. Keep components/data separate from systems and presentation.
3. Communicate with typed events or stable records.
4. Register the plugin in application composition.
5. Add a focused unit or headless plugin test.
6. Document how the feature is enabled and removed.

Use [creating-a-module.md](creating-a-module.md) for the repeatable layout.

## Native extension philosophy

Forge graphs should cover the common creative workflow, but native code remains
available for custom gameplay, complex menus, GUI frameworks, render features,
animation controllers, shader functions, importers, and validators. A native
extension should register through a discoverable contract rather than require a
permanent edit to Forge's central UI.

## Compatibility status

The current application factory is usable for local consumers and tests, but
the crate is not published and its public framework surface is pre-stable. New
code should prefer explicit module paths and avoid treating every `pub` item as
a semver promise until the facade and module contracts are finalized.
