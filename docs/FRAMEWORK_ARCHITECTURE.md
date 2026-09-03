# Starfall Framework Architecture

This is the authoritative target architecture for turning the current native
Starfall application into a reusable framework without weakening the Heavy
Water demo. It describes ownership and dependency direction. Shipped APIs are
identified in the README and code; items explicitly marked *target* are staged
work.

## Product surfaces

| Surface | Owns | Must not own |
|---|---|---|
| Starfall Engine | App services, schedules, schemas, registries, runtime contracts | Heavy Water story, chapters, named locations |
| Gameplay kits | Reusable movement, combat, multiplayer, world, vehicle, UI capabilities | Campaign sequencing or demo branding |
| Starfall Forge | Documents, graph editing, import, validation, preview, publishing | Shipping gameplay state |
| Heavy Water Demo Game | Game modes, campaign, content, presets, presentation | Private engine access unavailable to another game |
| Starfall Studio | Native shell that hosts Create, Learn, and Play workflows | Feature logic that belongs to a module |

The dependency direction is:

```text
schema
  ↓
core ──→ runtime ──→ gameplay kits
                       ↓
                    game modules ──→ campaign/application
  ↑                    ↑
  └──────── Forge/document/compiler ┘
```

Runtime code may consume published schemas. It must not depend on Forge UI,
editor sessions, undo history, or draft-only records.

## Graph architecture

Starfall uses one graph kernel with multiple typed graph languages. It does not
use one untyped graph for every problem.

The extracted `starfall-graph` crate currently owns:

- stable graph, node, port, and connection IDs
- typed values, signals, object references, and asset references
- deterministic serialization and source-control-friendly ordering
- node definitions, categories, documentation, and registry discovery
- graph and node schema versions plus structural validation diagnostics

The next graph-kernel slices add migrations, command-based editing,
transactions, undo/redo, subgraphs, instance overrides, dependency tracking,
and compilation contracts. Domain compilation remains outside the neutral
crate.

Domain graph families include Object/Prefab, Behavior, Animation, UI, Material
and Shader, World and City, Dialogue, Mission, Encounter, and Campaign graphs.
Each family defines legal nodes, port types, scheduling semantics, validation,
and compilation output.

### Authoring and runtime boundary

Graphs are authoritative project sources. Live ECS entities are disposable
previews and must never become persistent identity.

```text
graph/document
    → validate
    → compile
    → preview or simulate
    → publish immutable runtime bundle
    → runtime catalog loads compiled records
```

Shipping builds should not interpret large editor graphs every frame. Object
graphs compile to spawn plans, behavior graphs to event/state programs,
animation graphs to controllers, UI graphs to screen/focus definitions, shader
graphs to WGSL/material descriptors, and world graphs to streamable manifests.

## Object composition

An authored object is a stable identity plus capabilities, never a deep class
hierarchy. A city building can combine Visual, Collision, Interior, Door,
Shop, QuestLocation, Destructible, NightLighting, Audio, and SavePolicy modules.
Instances override bounded data without copying the complete object.

Object connections use stable IDs and typed selectors rather than Bevy
`Entity`, asset handles, display names, or query order. Runtime adapters resolve
those identities when a published object is spawned.

## Native extension boundary

Visual tools are not a ceiling. A game or module must be able to register:

- graph nodes and compilers
- ECS components, resources, events, and systems
- object behaviors and runtime adapters
- importers, exporters, inspectors, and validation rules
- editor tools, panels, commands, and project generators
- UI widgets, screens, focus policies, and themes
- animation processors, rig controls, and state providers
- material nodes, complete WGSL shaders, and render extensions
- publish steps and package checks

Every extension has a stable type ID, version, declared inputs/outputs,
dependencies, and documentation. Forge discovers registrations through plugins;
it does not maintain a closed central enum of every possible extension.

## Module contract

A Starfall module is the distributable unit of capability. It may contain Rust
code, graphs, objects, UI, shaders, animations, assets, docs, and tests. Its
manifest declares requirements and provided registrations.

Manifest schema version 1 is implemented by the independent
`starfall-project` crate. It supplies safe TOML parsing, legacy migration,
stable-ID and semantic-version validation, project-relative path validation,
duplicate dependency diagnostics, and deterministic round trips. It performs
no filesystem mutation.

Every module documents:

1. What it owns.
2. Which engine features and modules it requires.
3. Which events and stable records it consumes and produces.
4. What it saves and how that data migrates.
5. Which content Forge can author.
6. How native code extends or replaces it.
7. Which isolated example proves it works.
8. How it behaves when the module is absent.

See [creating-a-module.md](guides/creating-a-module.md) for the filesystem and
code pattern.

## Heavy Water as the reference consumer

Heavy Water is divided by game mode rather than accumulated in global world or
UI plugins:

```text
HeavyWaterShared
  ├── OpenWorld
  ├── Platformer
  ├── Racer
  ├── RPG
  └── Presentation
          ↓
       Campaign
```

Open World, Platformer, and Racer may depend on shared Heavy Water identity and
progression contracts, but not on one another. Campaign composition selects and
transitions between them. Each must be removable without leaving unrelated
modules uncompilable.

The first implemented mode boundary is `HeavyWaterPlatformerPlugin`. It owns
its state, entry/reset, authored-route spawning with fallback, continuous
rules, scene tagging, and teardown. `WorldPlugin` owns only campaign entities
tagged `WorldOwned`; application composition installs the two plugins as
siblings. The reusable prefab intent, movement-envelope math, chunk/socket
validation, route schema, and route assembly now live in the independent
`starfall-platformer` gameplay-kit crate. Heavy Water supplies the themed
catalog, materials, encounters, rewards, and runtime scene adapter.

## Native Studio experience

The eventual Starfall Studio shell has three first-class activities:

- **Create:** manage projects and use Forge tools.
- **Learn:** inspect guided Heavy Water objects, graphs, modules, and code.
- **Play:** run the current project or the installed demo natively.

"Fork This Game" will copy the demo, generate new stable project/package IDs,
rename display branding, allow feature selection, validate dependencies, and
open the result in Forge. Explicit manifests drive this automation; tools must
not guess project meaning from folder names alone.

## Architectural enforcement

- Engine modules cannot import Heavy Water modules.
- Runtime modules cannot import editor-only implementation.
- Backend-specific physics access goes through the engine adapter.
- Persistent records never contain transient ECS entities or Bevy handles.
- Feature communication crosses typed events, commands, traits, and schemas.
- Persistent mutations cross a command boundary and are undoable in Forge.
- Each supported feature combination has a compile or consumer test.
- New public schemas require a version and migration policy.
- Documentation changes in the same work that changes a public contract.

The current code does not satisfy every rule yet. The largest known inversions
are tracked in [PROJECT_PLAN.md](PROJECT_PLAN.md) and are migration work, not
reasons to disguise the current state.
