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

For the minimal typed-graph framework without Heavy Water modules:

```toml
[dependencies]
starfall-i = { path = "../Starfall_I", default-features = false }
```

Tools can depend on the extracted contracts without pulling in the root game
package:

```toml
[dependencies]
starfall-graph = { path = "../Starfall_I/crates/starfall-graph" }
starfall-platformer = { path = "../Starfall_I/crates/starfall-platformer" }
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
