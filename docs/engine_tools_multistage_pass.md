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

## ET2 — Safe editor workspace shell (in progress)

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

### ET2a delivered

- Tab or Select + Start enters and exits the workspace while a chapter is live;
- gameplay input is zeroed, virtual simulation and Avian physics are paused,
  gameplay cameras are disabled, and cursor ownership is transferred atomically;
- a dedicated real-time 6-DOF editor camera supports WASD/QE, right-mouse look,
  controller sticks, and Shift speed boost;
- controller/keyboard focus and mouse activation work across labeled toolbar,
  outliner, and inspector buttons;
- authorable empty anchors can be created, selected, duplicated, deleted, and
  moved on a one-metre X grid;
- transform nudges use ET1 transactions and are undoable/redoable;
- the status bar reports selection and command results.

### ET2b delivered

- cursor-ray viewport selection is restricted to roots marked `Authorable` and
  respects approximate scaled bounds; Shift-click supports multi-selection;
- selected objects receive visible red/green/blue world axes and a ground
  footprint over the editor grid;
- the inspector exposes snapped XYZ translation, 15-degree Y rotation, and
  proportional scale controls, all routed through ET1 undo transactions;
- fly/orbit camera switching and frame-selection are available from labeled,
  controller-focusable buttons and keyboard shortcuts;
- the creation palette spawns visible block, pillar, beacon, and empty-anchor
  draft objects in front of the editor camera;
- the outliner has live name filtering, a matching object list, and filtered
  previous/next selection;
- dirty drafts are visible in the status bar and require a deliberate second
  exit command before returning to play mode.

### ET2c delivered

- colored viewport axes now accept direct mouse drags in translate, rotate, and
  per-axis scale modes;
- a complete drag previews continuously but commits exactly one ET1 undo
  transaction when released;
- rotation mode draws three colored rings and snaps to 15-degree increments;
- transform space switches between world and object-local axes;
- translation snapping cycles through 0.25, 0.5, 1.0, and 2.0 metres and is
  shared by direct handles and inspector movement;
- live world adapters expose chapter anchors, cave gates, settlement terminals,
  and route markers as editable stable roots;
- moving a cave or settlement adapter synchronizes its gameplay entry/focus or
  construction origin instead of changing only its visible transform;
- generated road loops and explorable-building roots appear as selectable,
  searchable read-only adapters. Editing them is blocked until their complete
  spline/building recipes can regenerate every dependent mesh and collider;
- world adapters cannot be duplicated or deleted accidentally.

ET2's persistence gate is now delivered by ET3a. Draft primitives and editable
world-adapter transforms save and reload through stable identities. The road and
building forges must still replace read-only adapters with complete recipe
compilers before generated geometry can be edited safely.

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

### ET3a delivered — July 2026

- `src/engine_tools/persistence.rs` defines schema-versioned project manifests,
  content records, scene drafts, primitive records, and stable adapter
  overrides without serializing Bevy `Entity` values.
- Forge now exposes controller-focusable `SAVE`, `LOAD`, `RECOVER`, and
  `PUBLISH` actions. Keyboard shortcuts are Command/Control-S, Command/Control-O,
  Shift-Command/Control-O, and Shift-Command/Control-S respectively.
- Saves use a temporary file, file synchronization, atomic rename, directory
  synchronization, and three rotating recovery snapshots at
  `starfall_forge/project.json`.
- Normal load automatically falls back to the newest valid recovery snapshot;
  explicit recovery loads snapshot 1. Loading clears stale selection and undo
  history, restores authored IDs/names/transforms, and reapplies cave,
  settlement, anchor, and route-marker overrides through stable keys.
- Validation rejects future schemas, invalid or duplicate content IDs, missing
  dependencies, zero or duplicate editor IDs, blank or duplicate adapter keys,
  non-finite transforms, and zero-length rotations.
- Publishing validates the current draft, refreshes deterministic scene hashes,
  promotes them to `published_hash`, and writes through the atomic save path.
- Automated coverage includes interrupted-write behavior, corrupt-primary
  recovery, migration, dependency/reference rename, draft hashing, publishing,
  invalid records, and an editor-world primitive/adapter round trip.

### ET3b registry/source slice delivered — July 2026

- Scene registry records now write atomic per-record JSON source documents at
  their project-relative `source_path`; unsafe traversal paths and duplicate
  source paths are rejected.
- The manifest retains an embedded scene recovery generation. Load verifies the
  source document's content ID, schema, and deterministic draft hash. A mixed
  generation caused by an interrupted source/manifest save falls back to the
  matching embedded copy instead of loading inconsistent data.
- Forge includes a third, scrollable `CONTENT REGISTRY` panel with project and
  record summaries, category, draft/published state, source path, abbreviated
  hash, and live structural/on-disk validation diagnostics.
- Controller-focusable previous/next, rename, and validate buttons are orders
  33–36 in the existing deterministic focus ring. Rename preserves dependency
  references; keyboard text entry accepts Enter and controller A accepts the
  current buffer, while Escape/controller B cancels.
- Text input capture now consumes its accept/cancel event for the frame, so an
  Enter/A used to complete filter or rename input cannot also activate the
  focused toolbar button underneath it.
- Tests cover authoritative source hydration, corrupt source diagnostics,
  project-relative path enforcement, source-path uniqueness, interrupted
  cross-file generations, and registry selection wraparound.

### ET3c record lifecycle/material bridge delivered — July 2026

- `ContentPayload` is now the versioned codec boundary for Scene, Character,
  Creature, Road, Building, Cave, Material, and UI records. Non-scene recipes
  use schema-versioned JSON field maps until their production workspaces replace
  them with stronger domain schemas.
- Forge can create seeded toon-material records, duplicate non-scene records,
  dependency-check and remove records, and retain deleted sources on disk for
  manual recovery. The single active scene cannot be duplicated or deleted by
  this workspace.
- Registry search filters by content ID, display name, or category. Previous and
  next navigation cycle only through matches, and the search input shares the
  frame-safe keyboard/controller capture used by rename and outliner filtering.
- Content-ID rename migrates the payload map and dependencies. `SYNC SOURCE
  PATH` performs a guarded filesystem rename to a canonical category/ID path,
  rolls back a failed directory sync, and never overwrites another record.
- All payload categories write through the per-record atomic source codec and
  participate in draft hashing, source diagnostics, publishing, embedded
  recovery, and schema migration.
- Material recipes seed the configurable `toon_v1` contract: base color, light
  direction, three lighting bands, and rim color/strength/shape. Forge spawns a
  live preview orb so the Bevy 0.19 shader pipeline is compiled during editor
  smoke tests.
- `assets/shaders/toon.wgsl` now uses Bevy's default `VertexOutput` contract and
  material bind-group macro, preserving skinning/morph/instancing compatibility
  while replacing the placeholder hardcoded shader with configurable anime
  bands and tinted rim light.

ET3's registry foundation is now sufficient for ET4 production workspace
integration. Road/building adapters remain read-only until ET5 supplies recipe
regeneration contracts for dependent meshes and colliders.

### ET3d typed material authoring delivered — July 2026

- The legacy-compatible Material payload codec now resolves into typed Toon,
  Water, Energy, Shield, Ice/Snow, and Lava specifications instead of exposing
  unbounded JSON values directly to preview materials.
- Every family supplies three presets and bounded named controls. Six new
  controller-focusable registry actions cycle family/preset/parameter, step the
  value down or up, and reset the current preset.
- The selected Material record updates its matching live Forge preview without
  recreating material assets. Invalid shaders, presets, numeric types, and
  control ranges block validation and publishing.
- The F11 performance overlay reports active cameras, projectiles, transient
  VFX, and custom/Standard material counts. Per-view GPU timing and draw-call
  metrics still require renderer instrumentation and measured hardware passes.

### ET3e runtime material catalogs delivered — July 2026

- Published Material records compile into a runtime catalog keyed by stable
  content IDs and backed by typed Bevy handles. ECS/asset handles are never
  serialized.
- Scene primitives persist an optional Material content ID. Bindings validate
  category and existence, migrate during content-ID rename, block unsafe record
  deletion, and become explicit scene-record dependencies during save/publish.
- Controller-focusable Apply/Clear actions bind catalog materials to selected
  renderable objects. The outliner exposes bindings, and load reconstructs the
  typed material component from the stable ID.
- The tested workflow is create/tune → publish Material → apply to object →
  publish scene. Unpublished bindings retain their ID but fall back safely to
  the Standard preview until a matching published catalog entry exists.

### ET3f procedural recipe catalogs delivered — July 2026

- Biome, Road, Building, Cave, and City records now have deterministic
  seed/revision recipes with category-specific material slots. Existing generic
  Road/Building/Cave source files remain readable through serde defaults.
- Publish synchronizes recipe material dependencies and builds a second stable-ID
  runtime catalog. Rename, validation, and dependent-delete protection cover
  every slot without persisting Bevy entities or asset handles.
- Terrain, speed-road deck/barrier/support geometry, building exteriors, and
  cave rock/floor/accent geometry carry stable recipe-slot components. An
  epoch-based resolver applies published typed materials once per catalog
  revision and restores captured Standard fallbacks when a binding disappears.
- This completes the safe runtime boundary for ET5. The next pass is the
  controller-facing World Kit UI and transactional recipe regeneration, not
  additional ad-hoc mutations of generated ECS children.

### ET5a World Kit controls delivered — July 2026

- The controller-focusable registry now creates Biome, Road, Building, Cave,
  and City recipes. The chosen type, copied published Material, selected named
  slot, seed, revision, and current binding remain visible in the registry.
- The authoring loop is explicit: publish a Material, `COPY MATERIAL`, select or
  create a World recipe, choose `NEXT MATERIAL SLOT`, then bind or clear it.
  Binding and clearing use recipe snapshot commands in the shared bounded undo/
  redo stack instead of mutating generated ECS entities.
- `REGENERATE PREVIEW` advances the recipe revision as an atomic undoable
  transaction. A camera-relative four-object preview is destroyed and rebuilt
  from the deterministic seed/revision, so repeated regeneration cannot leak or
  accumulate preview entities.
- All editor panels now own real `ScrollPosition` state. Controller focus keeps
  the active button visible inside the correct panel, including World Kit
  controls below the original registry viewport.

ET5b begins the production generator editor with typed scalar/count controls
and validation budgets. Typed spline points, room/portal graphs, biome zone
painting, and transactional live-world root replacement remain ET5c work.

### ET5b typed generator controls delivered — July 2026

- Every World Kit family now publishes a bounded parameter contract instead of
  relying on undocumented JSON numbers. Controller steppers expose the active
  label, value, range, reset, and deterministic seed controls.
- Road recipes cover width, maximum grade, curve radius, loop count, jump count,
  and support spacing. Building recipes cover footprint, floors, rooms, stairs,
  and entrances. Cave recipes cover tunnel width, rooms, vertical layers,
  secrets, fast-travel points, and the four-player shared-camera radius.
- Biome recipes cover zone radius, relief, water level, hazards, and landmarks;
  City recipes cover block/road scale, building density, districts, and
  landmarks. Whole-number controls are enforced independently from scalars.
- Validation blocks nonnumeric/out-of-range values and unsafe generation
  budgets: 180 road segments, 160 building rooms, 96 cave room/layers, cave
  camera coverage, and city road-to-block clearance. Invalid recipes cannot
  advance their regeneration revision.
- The bounded four-object compiler now reflects these typed parameters in its
  meshes, transforms, height, spacing, grade, and scale. Parameter, seed, reset,
  and regeneration edits are all recipe-snapshot transactions with undo/redo.

The ET5c target was the safe live-world compiler boundary: typed spline points and
room/portal graphs, navigation/collision preflight, generated-root ownership,
and atomic swap of a validated replacement root. The shipped world is not
destructively regenerated from a preview-only recipe.

### ET5c topology compiler and sandbox roots delivered — July 2026

- `ProceduralRecipeDraft` now persists backward-compatible stable-ID road
  spline points plus typed topology nodes and Door/Stair/Tunnel/Portal edges.
  Nodes identify rooms, entrances, fast-travel points, zones, and landmarks.
- Preflight enforces finite bounded geometry, unique point/node IDs, valid edge
  endpoints, 2–64 road points, maximum road grade, 96-node/192-edge caps,
  connected Building/Cave navigation from an entrance, non-overlapping room
  volumes, and enough authored Cave fast-travel nodes for the recipe request.
- `SEED DEFAULT TOPOLOGY` creates a valid undoable starter spline or graph for
  every World Kit family. `COMPILE SANDBOX ROOT` builds a pure intermediate list
  first, then spawns a single owned root and its bounded physics/render children.
- Root replacement is atomic: the previous valid root is removed only after the
  replacement has compiled and spawned. Invalid edits retain the last valid
  root. Undo/redo recipe changes automatically recompile an active sandbox.
- Sandbox parts share one cached cube mesh and fallback material, preventing
  asset growth across regeneration. `CLEAR SANDBOX ROOT` removes the owned root
  and all linked children without touching shipped world geometry.

### ET5d direct topology editing — controller slices delivered July 2026

- The controller-scrollable World Kit panel selects previous/next road spline
  points or biome/building/cave/city topology nodes and displays the stable ID
  plus current XYZ recipe coordinates.
- Six controller-focusable nudge controls move the selected element on X/Y/Z
  using the shared 0.25/0.5/1/2-meter editor snap setting. Each nudge is one
  recipe-snapshot transaction and therefore participates in the existing
  bounded undo/redo history.
- An active sandbox draws a gold three-axis viewport locator at the selected
  recipe coordinate. The locator is derived editor visualization, never saved
  geometry or an independently mutable ECS child.
- Empty recipes reject movement safely. Invalid intermediate drafts remain
  undoable while the ET5c sandbox compiler retains its last valid atomic root.
- Typed graph-edge authoring marks a stable start node, selects a destination,
  cycles Door/Stair/Tunnel/Portal, and connects or removes the exact undirected
  typed edge through the same recipe undo stack. It rejects road recipes,
  self-links, stale endpoints, duplicate edges, and nonexistent removals.
- The marked endpoint is cyan and a live cyan guide connects it to the gold
  selected destination. Removing a required cave connector may make the draft
  invalid, but validation retains the previous compiled sandbox root until the
  edge is restored or the edit is undone.
- Direct viewport manipulation draws red/green/blue recipe-space handles on the
  selected point or node. Mouse dragging projects onto the chosen screen-space
  handle, snaps in the shared recipe-meter increments, and previews by safely
  recompiling the sandbox. Mouse release restores the pre-drag snapshot and
  commits one atomic recipe transaction, avoiding per-frame undo entries.

ET5d remains in progress: spline tangents and junctions, room doorway/socket
placement, terrain
projection, structured validation highlights, and explicit promotion of a
published sandbox root into a scoped world-generation layer are later slices.

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

The next implementation slice is ET4 Character/Creature production workspaces,
followed by ET5 safe world-recipe regeneration.
Road/building adapters become editable only as their recipe compilers acquire
safe regeneration boundaries.
