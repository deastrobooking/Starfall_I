# Repeatable Project Structure

This document defines the filesystem contract Starfall projects and modules
are converging on. Consistency supports human navigation today and safe project
generation, validation, upgrading, and IDE workflows later.

## Target workspace

```text
starfall/
├── Cargo.toml
├── crates/
│   ├── starfall/                 Public facade and feature bundles
│   ├── starfall-schema/          Stable IDs and versioned neutral records
│   ├── starfall-core/            Schedules and platform/runtime services
│   ├── starfall-runtime/         Published catalogs and runtime adapters
│   ├── starfall-gameplay/        Reusable gameplay kits
│   ├── starfall-platformer/      Extracted renderer-neutral platformer kit
│   ├── starfall-graph/           Typed graph kernel and compilation contracts
│   ├── starfall-forge/           Native authoring environment
│   └── starfall-project/         Manifests, discovery, validation, scaffolding
├── apps/starfall-studio/         Native Create/Learn/Play shell
├── games/heavy-water-demo-game/ First-party reference consumer
├── templates/                    Focused generated starting points
├── examples/                     Small API demonstrations
└── docs/
```

Crates are extracted only after imports obey the intended dependency direction.
Folders may represent these boundaries before they become packages.

Current extraction status:

- `crates/starfall-graph` is an independent package containing the typed graph
  document and native-node registration kernel.
- `crates/starfall-project` is an independent package containing project and
  module manifest parsing, migration, validation, and deterministic encoding.
- `crates/starfall-platformer` is an independent gameplay-kit package
  containing prefab intent, movement envelopes, chunk/socket validation, route
  assembly, and route validation with only a vector-math dependency.
- `src/lib.rs` remains the transitional all-in-one facade and re-exports both.
- Heavy Water exposes `shared` and `platformer` ownership through
  `src/heavy_water/`. Its platformer plugin owns scene entry, spawning, runtime
  rules, tagging, and teardown; its themed catalog, rendering adapters,
  encounters, rewards, and presentation consume the reusable platformer crate.

## Game project

```text
my-game/
├── Cargo.toml
├── starfall.project.toml
├── README.md
├── src/
│   ├── lib.rs
│   ├── main.rs
│   ├── game.rs
│   ├── prelude.rs
│   ├── shared/
│   ├── features/
│   └── presentation/
├── content/
│   ├── project.json
│   ├── objects/
│   ├── graphs/
│   ├── levels/
│   └── records/
├── assets/
│   ├── source/
│   └── imported/
├── build/
│   ├── cache/
│   └── published/
├── docs/
└── tests/
```

Project source, imported originals, derived cache, and published output are
separate roots. A packaged game consumes published output; it does not require
Forge drafts or import metadata.

## Feature module

```text
features/platformer/
├── starfall.module.toml
├── README.md
├── mod.rs
├── plugin.rs
├── components.rs
├── events.rs
├── resources.rs
├── systems/
├── data/
├── ui/
└── tests.rs
```

Small modules begin with `mod.rs`, `plugin.rs`, and tests. Do not create empty
files merely to imitate the complete tree. Split a file when it gains a distinct
owner or reason to change.

`mod.rs` is the feature map: it documents responsibility and exports only the
intended public surface. `plugin.rs` is composition, not a storage place for all
systems. Data definitions stay separate from runtime behavior and presentation.

## Heavy Water target

```text
games/heavy-water-demo-game/src/
├── lib.rs
├── main.rs
├── demo_game.rs
├── shared/
│   ├── identity.rs
│   ├── progression.rs
│   ├── save_data.rs
│   └── ui_theme.rs
├── features/
│   ├── open_world/
│   ├── platformer/
│   ├── racer/
│   ├── rpg/
│   └── campaign/
└── presentation/
```

Each game-mode directory follows the feature-module contract. Shared contains
only contracts genuinely shared by two or more modes. Campaign composes modes;
it does not become a second global dumping ground.

## Naming

| Kind | Convention | Example |
|---|---|---|
| Display product | Title Case | `Heavy Water Demo Game` |
| Cargo package | kebab-case | `heavy-water-demo-game` |
| Rust crate/module | snake_case | `heavy_water_demo_game` |
| Stable content/module ID | reverse-domain or namespaced lowercase | `heavy_water.platformer.spring_pad` |
| Rust plugin | PascalCase + `Plugin` | `PlatformerGamePlugin` |
| Graph/object file | snake_case | `cloudrail_shop.object.json` |

Renaming display text must not change stable IDs. ID changes require an explicit
migration or alias.

## Manifest principle

Directories optimize discovery; manifests provide authority. A future IDE must
read `starfall.project.toml` and `starfall.module.toml` rather than infer enabled
features, launch scenes, asset roots, or dependencies from paths.

Manifest schema version 1 is now accepted project input. Its parser, legacy
version-0 display-name migration, semantic validation, repository fixtures, and
round-trip tests live in `crates/starfall-project`. Future schema changes must
add migrations and fixtures before increasing either schema version.

Manifest paths always use `/`, are relative to the manifest directory, and may
not contain `.` or `..` segments, drive prefixes, or backslashes. This makes the
same document safe and deterministic on macOS, Linux, and Windows.

## Placement questions

Before adding a file, ask:

1. Is it an engine capability, reusable kit, game feature, or campaign detail?
2. Is it persistent data, runtime behavior, presentation, or authoring UI?
3. Can the owning feature be removed without breaking unrelated features?
4. Does the file import toward lower-level contracts rather than higher-level
   application content?
5. Is a stable ID or event contract more appropriate than a direct module
   reference?

If ownership is unclear, define the contract before creating another global
resource or extending a monolithic plugin.
