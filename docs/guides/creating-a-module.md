# Creating a Starfall Module

A module packages one coherent capability with its native code, authored
graphs, objects, assets, documentation, and tests. Heavy Water game modes use
the same contract expected of future third-party modules.

## Start with the minimum

```text
my_feature/
├── starfall.module.toml
├── mod.rs
├── plugin.rs
└── tests.rs
```

A native package declares its identity and dependencies explicitly:

```toml
schema_version = 1

[module]
id = "my_game.platformer"
name = "My Platformer"
version = "0.1.0"
kind = "game-feature"
source_root = "src"

[[dependencies]]
id = "starfall.gameplay.platformer"
version = "^0.1"

[provides]
graph_nodes = ["my_game.platformer.launch_pad"]
```

The IDs use Starfall stable-ID syntax, versions use semantic versioning, paths
must stay within the module, and dependencies may not be duplicated. The
`starfall-project` crate parses, migrates, validates, and re-encodes this format.

Add `components.rs`, `events.rs`, `resources.rs`, `systems/`, `data/`, or `ui/`
only when those responsibilities exist.

`mod.rs` documents:

- the feature's responsibility and non-responsibilities
- required modules and engine services
- inputs, outputs, and stable IDs
- save and migration policy
- authoring and publishing support
- the public exports another module may use

## Compose through a plugin

```rust,ignore
pub struct PlatformerGamePlugin;

impl Plugin for PlatformerGamePlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<CheckpointReached>()
            .init_resource::<PlatformerSession>()
            .add_systems(OnEnter(GameMode::Platformer), setup_course)
            .add_systems(
                FixedUpdate,
                update_platformer_rules.in_set(GameSet::Motor),
            )
            .add_systems(OnExit(GameMode::Platformer), teardown_course);
    }
}
```

Setup and teardown ownership must be symmetrical. A mode cannot leave cameras,
UI, physics state, input capture, or entities behind after removal.

## Communicate through contracts

Prefer:

- typed events for something that happened
- commands for requested mutations
- stable content IDs for authored references
- narrow traits for replaceable native behavior
- versioned records for persistence

Avoid importing another feature's private systems, querying by display name, or
depending on entity creation order.

## Prepare for visual authoring

A module that exposes nodes or objects declares:

- stable type ID and schema version
- typed ports or properties
- category and searchable documentation
- validation rules and disabled reasons
- compiler/runtime adapter
- migration behavior for older records

The node editor discovers registrations. Adding a module should not require
expanding a closed global node enum.

## Test the contract

At minimum, test:

1. The plugin registers in an otherwise minimal app.
2. Its public data round-trips when serialized.
3. Invalid authored data produces a diagnostic instead of a panic.
4. Entering and exiting leaves no owned runtime state behind.
5. The game still compiles when the module is absent.

Large modules should include an isolated playable example or preview scene.

## Document removal

Every module README includes an **Enable**, **Use**, **Extend**, and **Remove**
section. Removal instructions are a practical dependency test: if a feature
cannot explain how to remove itself, its ownership boundary is incomplete.
