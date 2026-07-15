# Starfall Engine Tools — Multistage Pass

This plan translates the supplied Blender feature inventory and single-binary
MVP specification into a Starfall-specific production program. Blender is a
capability reference, not a requirement to reproduce 676 features inside the
game. Starfall's goal is a focused game-maker that creates valid content for
its four-player action RPG.

## Architectural decisions

Keep the single executable during the proving phase. The current repository is
already a working Bevy 0.19 + Avian game; immediately splitting it into nine
crates would add API boundaries before the shared services are understood.
Extract crates only after Character Studio, Creature Forge, and World Kit Forge
all use the same service in production.

Use two orthogonal states:

```text
AppState:        MainMenu / CharacterStudio / Playing / ...
EngineToolMode:  Playing / Editing
```

`EngineToolMode` eventually overlays an editor on a loaded chapter. Entering
Editing must capture gameplay input, release the cursor, pause simulation,
switch camera authority, and expose only editor schedules. A naked Tab toggle
is intentionally deferred until those safeguards exist.

Persist authored recipes, not the live ECS world. Whole-world `DynamicScene`
serialization would include transient players, projectiles, particles, AI
timers, cameras, generated mesh children, and unstable `Entity` references.
Published scene recipes instead reference stable `EditorEntityId` and content
IDs; generators reconstruct runtime entities.

Use Bevy 0.19 APIs and the existing Avian physics layer. Code samples in the
attached MVP that use Bevy 0.14 `Input`, `Windows`, old bundles, Rapier, or old
scene APIs are conceptual pseudocode and must not be copied verbatim.

## Capability tiers

### Build into Starfall

- project/content registry, stable IDs, recovery saves, version migrations;
- viewport selection, outliner, properties, transform/spline gizmos;
- bounded transactional undo/redo;
- character, creature, road, settlement, castle, and cave recipes;
- layered terrain sculpt/paint and landmark placement;
- animation preview, pose libraries, sockets, constraints, and clearance tests;
- focused material presets and palette editing;
- typed gameplay node graphs;
- scene validation, performance budgets, publish/playtest workflow;
- glTF import/export at validated boundaries.

### Add only after core authoring is proven

- half-edge polygon editing, bevel/inset/extrude, UV editing;
- advanced material/geometry node graphs;
- animation timeline, curves, NLA-like clip layering;
- collaborative editing and command replication;
- plugin/scripting SDK and remote asset libraries;
- VR scene inspection.

### Keep in external DCC tools

Photoreal path tracing, compositing, video editing, camera tracking, production
audio mixing, full fluid/smoke/ocean simulation, high-end sculpt/retopology,
hair grooming, and unrestricted Python execution remain Blender or specialist
tool responsibilities. Starfall imports their validated outputs.

## ET1 — Core authoring substrate (implemented)

`src/engine_tools/mod.rs` now provides:

- independent `EngineToolMode` state;
- serializable/reflected `EditorEntityId`, never Bevy `Entity`, as stable identity;
- collision-safe ID allocation/reservation;
- deterministic multi-selection with an active entity;
- atomic multi-command transform transactions;
- 128-entry bounded undo/redo with descriptions and failure preservation;
- preflight validation that prevents half-applied multi-entity edits.

Acceptance: IDs survive save-oriented serialization, invalid transaction
targets do not modify valid targets, and execute/undo/redo restore exact
transforms. Automated tests cover each requirement.

## ET2 — Safe editor workspace shell

Build a native Bevy UI overlay first, reusing the project's controller focus
system. Add:

- explicit EDIT LEVEL entry plus Tab toggle after input capture exists;
- editor camera with orbit, pan, zoom, and 6-DOF fly modes;
- cursor ownership and gameplay-input suppression;
- ray/shape selection against authorable roots only;
- outliner, inspector, searchable creation palette, status/validation panel;
- translate/rotate/scale gizmos with local/world space and grid/angle snapping;
- Ctrl/Cmd-Z, redo, duplicate, delete, frame selection;
- visible dirty/draft state and exit confirmation.

Acceptance: edit a test arena for ten minutes without firing a weapon, moving a
player, advancing AI, or losing controller menu access. Every transform gesture
is one undo transaction.

## ET3 — Content registry, files, and publishing

Define a versioned project manifest and registry records:

```text
content_id / schema_version / category / source_path / dependencies
display_name / tags / thumbnail / draft_hash / published_hash
```

Add atomic write-to-temp + rename, rotating recovery snapshots, missing
dependency reports, duplicate-ID detection, rename/reference migration, and
draft versus published assets. JSON remains appropriate for player-facing
recipes; RON may be used for engine-authored scene manifests after evaluation.

Acceptance: simulate an interrupted save, recover the previous valid project,
rename a referenced asset without breaking a level, and reject duplicate IDs.

## ET4 — Character and Creature production workspaces

Move Character Studio and `CreatureSpec` onto ET1–ET3 services:

- thumbnail preset browser and searchable catalogs;
- explicit projects, duplicate/rename, seeded variant comparison;
- animation/face-expression preview and topology locomotion preview;
- attachment sockets and equipment clearance visualization;
- material response and palette panels;
- creature behavior/faction bindings and enemy spawn test chamber;
- publish validation for rig, hitbox, navigation, camera, and VFX budgets.

Acceptance: publish three heroes, three robots, and three monsters; reload them,
spawn them through gameplay factories, and pass targeting/damage/save tests.

## ET5 — World Kit Forge

Author versioned `RoadKitSpec`, `SettlementKitSpec`, and `DungeonKitSpec`:

- spline roads with grade, curvature, lane, boost direction, ramp, barrier,
  support, loop, jump, terrain-clearance, and junction rules;
- modular buildings/castles with parcel footprints, rooms, doors, stairs,
  interiors, encounter sockets, and faction dressing;
- caves/dungeons as connected room/portal graphs with vertical layers, secrets,
  hazards, fast travel, and shared-camera encounter bounds.

Acceptance: validators prove road connectivity and direction safety, building
room reachability, cave portal reachability, spawn clearance, and fast-travel
integrity before publish.

## ET6 — Terrain and Level Scene Composer

Create a chapter scene recipe referencing published assets by ID. Terrain is a
non-destructive stack:

```text
base heightmap + sculpt delta + biome/material masks + exclusion masks
```

Add raise/lower, flatten, smooth, terrace, erosion-preview, material paint,
road-conform, and restore brushes. Provide whole-level map and local 3D views,
streamed preview cells, landmark/city/castle/cave placement, spline editing,
spawn/encounter/fast-travel tools, and visualization for navigation, portals,
camera bounds, road graphs, terrain collision, and budgets.

Acceptance: modify terrain, add a connected road, place a castle and cave,
configure spawns and fast travel, save/reload, validate, and play without code
changes.

## ET7 — Typed gameplay graph

Do not compile arbitrary node `String` types directly into an unrestricted
interpreter. Define versioned typed node enums, typed pins, graph-cycle policy,
deterministic event ordering, instruction/time budgets, and allow-listed
actions. Entity references use content/editor IDs resolved at runtime.

Initial nodes: OnStart, OnEnterVolume, OnInteract, OnEnemiesDefeated, Branch,
Counter, Delay, OpenGate, SpawnEncounter, SetCheckpoint, PlayDialogue,
GrantReward, SetWorldSiteState, and CompleteObjective.

Acceptance: compile-time errors identify invalid pins/cycles/references; runtime
budgets prevent infinite execution; save/replay produces deterministic results.

## ET8 — UI and dialogue authoring

Build on Bevy UI and the existing controller focus layer: anchored layouts,
safe areas, panels, text, image, button, slider, list, navigation graph, style
tokens, localization keys, and preview resolutions. Author menus, HUD elements,
dialogue, tutorials, and interaction prompts without bypassing couch-controller
accessibility.

## ET9 — Advanced asset authoring

Only after ET4–ET6 prove demand, implement constrained mesh editing using a
tested half-edge core, plus UV tools, modifier-like non-destructive operations,
animation timeline/curves, clip layering, and focused material/geometry graphs.
All output still passes Starfall topology, rig, collider, and performance
validators. Free sculpting is not allowed to silently invalidate game assets.

## ET10 — Import/export and Blender bridge

Publish canonical glTF contracts for meshes, materials, armatures, animations,
morph targets, sockets, collision proxies, and metadata. Add a Blender-side
validation/export helper rather than embedding Blender's full tool surface.
Round-trip tests compare IDs, transforms, skeleton mapping, material slots, and
animation duration.

## ET11 — Collaboration and extensibility

Networking is last. Replicate validated editor transactions using stable IDs,
host authority, sequence numbers, acknowledgements, conflict policy, and
snapshots. Never transmit raw reflected component bytes by type-name without a
versioned schema and allow-list. Extract reusable crates and expose plugins only
after at least three Starfall workspaces share the APIs.

## Required quality gates for every stage

- unit tests for recipe/command/validator logic;
- serialization migration and corrupt-input tests;
- strict Clippy and formatting;
- startup plus editor/play transition smoke tests;
- controller-only workflow test;
- four-player camera and input-isolation test where applicable;
- performance metrics: entity, mesh, material, collider, draw-call, memory, and
  generator-time budgets;
- no editor preview/runtime output divergence.

The next implementation slice is ET2's safe workspace shell. Camera and input
capture land before the Tab shortcut, followed by selection/outliner and the
first transform gizmo using ET1 transactions.
