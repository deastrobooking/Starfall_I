# Starfall Game Maker Toolchain

Starfall is both a four-player game and a constrained game-making system. The
editor strategy is therefore not a collection of debug menus. Every tool must
compile a small, versioned recipe into validated runtime content through the
same deterministic pipeline.

```text
editable recipe -> generator -> preview -> validators -> compiled runtime asset
       |                |           |             |                |
   JSON/RON save    seeded RNG   undo/orbit   budget/playtest   game/save ID
```

## Scientific foundation

All generators follow these rules:

- The recipe is the source of truth; generated entities and meshes are output.
- Every schema has a version and migration path. Old player saves remain valid.
- Random generation accepts an explicit seed and produces repeatable output.
- Dimensions use meters, angles use radians internally, colors use linear or
  documented sRGB values, and normalized creative controls use `0.0..=1.0`.
- Generators expose semantic tags (face feature, robot socket, road lane, room,
  cave portal) instead of depending on entity names or scene hierarchy.
- Validation runs before publish: geometry/material budgets, collision,
  traversal clearance, spawn safety, connectivity, and four-player camera fit.
- Preview and runtime use the same compiler. A beautiful editor-only result
  that changes in gameplay is a defect.
- Recipes receive stable content IDs so levels reference content without
  copying generated geometry into campaign saves.

## Tool sequence

### GM1 — Humanoid Character Studio (active)

The current `CharacterSpec -> CharacterPatch -> procedural mesh` path is the
reference implementation. Immediate completion gates are expression channels,
animation preview, thumbnail presets, attachment clearance, seeded randomize,
and explicit schema versioning. Art direction combines compact fantasy
readability, colorful action robots, and original 1970s space-opera shapes
without copying protected characters or assets.

### GM2 — Robot and Monster Forge (foundation active)

Unify the existing `src/robots` archetypes and drone factory behind a serializable
`CreatureSpec`. Use composable morphology modules rather than separate hardcoded
models:

- topology: humanoid, quadruped, flyer, crawler, serpent, drone;
- parts: core, head/sensor, torso, limbs, wings, tail, armor, weapon sockets;
- behavior role: ally, civilian, scout, bruiser, artillery, boss;
- materials: painted metal, bare alloy, crystal, organic, energy;
- rig contract and procedural locomotion selected from topology;
- faction palette and silhouette rules applied as a layer, not baked geometry.

Acceptance: create, save, reload, animate, validate, and spawn at least three
distinct robots and three monsters from recipes; generated enemies must work
with targeting, damage, navigation, and campaign saves.

Foundation status: `src/robots/creature.rs` now provides schema-versioned JSON
recipes, deterministic seeded generation, robot/monster kinds, six topologies,
seven gameplay roles, six factions, five material responses, publish validation,
and six curated seeds. `CreatureSpec::from_legacy` wraps any existing preset
without losing its `RobotStyle`, while `spawn_creature` compiles topology into
the existing procedural factory and adds stable runtime metadata. Remaining GM2
work is the player-facing Forge workspace, topology-specific animation rigs,
enemy-system binding, thumbnail publishing, and campaign asset registration.

### GM3 — World Kit Forge

Create three recipe families sharing sockets and metrics:

- `RoadKitSpec`: deck profile, lane count, spline grade/curvature limits, ramp,
  loop, jump, guard, support-column, and terrain-clearance policies.
- `SettlementKitSpec`: street graph, parcels, building/castle modules, doors,
  rooms, stairs, encounter sockets, materials, and faction dressing.
- `DungeonKitSpec`: cave/room graph, elevation layers, gates, secrets, hazards,
  fast-travel anchors, and shared-camera encounter bounds.

Generators publish both visuals and gameplay metadata. Roads must output a
connected traversal graph; buildings must output navigable room/portal graphs;
caves must output encounter and fast-travel graphs.

### GM4 — Level Scene Composer

The scene editor consumes published character, creature, and world-kit assets.
It needs a full-level map plus local 3D viewport, selection/outliner,
translate/rotate/scale gizmos, terrain height/paint tools, spline editing,
prefab placement, duplicate/delete, undo/redo, and property inspection.

Terrain authoring uses non-destructive layers: base heightmap, sculpt delta,
material/biome masks, exclusion masks, and placed landmarks. Cities, castles,
caves, roads, encounters, and fast-travel points are referenced by stable IDs.
The editor must visualize road connectivity, cave portals, spawn radii,
navigation, camera bounds, and terrain collision before publishing.

Acceptance: load an entire chapter, add a terrain landmark, connect a road,
place a generated castle and cave, add spawns and fast travel, validate, save,
reload, then play the result without code changes.

## Shared editor services

Build these once and reuse them across all four tools:

1. Versioned asset registry and migrations.
2. Command-based undo/redo transactions.
3. Selection, outliner, inspector, searchable palette, and thumbnail browser.
4. Orbit/free cameras plus transform and spline gizmos.
5. Seeded generation and variant comparison.
6. Validation report with errors, warnings, metrics, and viewport highlights.
7. Draft/published lifecycle with atomic save and recovery snapshots.
8. Performance budgets and automated headless compile/playability tests.

## Near-term order

1. Finish GM1 expression channels, animation preview, schema version, and
   thumbnail preset browser.
2. Adapt the existing robot designer/factory to the shared recipe/validator
   contract; do not replace its working drone geometry.
3. Extract shared editor services only after both GM1 and GM2 exercise them.
4. Implement road-kit recipes first in GM3 because terrain-aware roads already
   have measurable grade, clearance, ramp, and connectivity tests.
5. Add settlement/castle and cave kits, then build GM4 on their published
   content contracts.

This order keeps the game playable while the maker layer grows and prevents the
scene editor from being designed before its asset schemas exist.
