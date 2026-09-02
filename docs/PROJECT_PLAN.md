# Starfall Engine, Forge, and Heavy Water — Project Plan

Starfall is one native product family with three clear ownership surfaces:

| Surface | Audience | Scope |
|---|---|---|
| Engine | Developers and technical creators | Reusable runtime services, gameplay kits, schemas, plugin composition, and native extension contracts. |
| Forge / Studio | Artists and creators | Project Hub, graph and object tools, live preview, validation, publishing, and eventually integrated code workflows. |
| Heavy Water Demo Game | Players and framework learners | Complete campaign plus open-world, platformer, racer, RPG, co-op, and creator-feature examples. |

The existing `designer` Cargo feature remains a transitional build boundary.
The target framework lets games select capability features and plugin groups;
Heavy Water is the first-party consumer and the all-in-one application includes
it by default. Focused templates come after the module contract is proven by
the complete demo.

## Current state

The project is already a substantial native engine/game/tool application, but
most packaging still reflects one binary game: feature dependencies are broad,
runtime code crosses editor boundaries, and several world/UI/tool plugins are
monolithic. A public library facade and thin native launcher establish the main
compatibility seam. `starfall-graph` and `starfall-project` are now independent
workspace crates, versioned manifests are validated and round-trip tested, and
Heavy Water has a public `shared`/`platformer` ownership facade. The next work
extracts a compiled graph vertical slice and removes platformer scene spawning
from the historical global world plugin.

The architecture authority is [FRAMEWORK_ARCHITECTURE.md](FRAMEWORK_ARCHITECTURE.md),
the filesystem contract is [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md), and the
Heavy Water inventory is [MASTER_FEATURES.md](MASTER_FEATURES.md). Engine and
Forge implementation tracks remain in [engine_roadmap.md](engine_roadmap.md) and
[editor_roadmap.md](editor_roadmap.md).

## Product strategy

1. Make Heavy Water use the same public contracts available to another game.
2. Keep the native all-in-one Create/Learn/Play experience first class.
3. Let developers compile and activate only the capabilities they need.
4. Make graphs excellent authoring formats while retaining unlimited native
   Rust, Bevy, WGSL, animation, UI, and tool extensions.
5. Standardize project/module files before automating creation and upgrades.
6. Extract crates from proven dependency seams rather than directory aesthetics.

## Next workstreams

### 0. Framework and module foundation

Priority: establish the reusable product contract before adding more global
feature surface.

Work:
- keep `src/main.rs` a thin launcher and move complete application composition
  through the public library boundary
- define explicit Core, Runtime, Gameplay Kit, Forge, Heavy Water Shared, Open
  World, Platformer, Racer, RPG, Campaign, and Presentation plugin ownership
- add capability Cargo features with representative minimal consumer builds
- introduce a neutral schema/runtime catalog boundary before moving Forge code
- establish one typed graph kernel with domain-specific object, behavior,
  animation, UI, shader, world/city, and narrative graph languages
- define native registration contracts for nodes, inspectors, widgets,
  importers, validators, shaders, animation processors, and publish steps
- split Heavy Water by removable game-mode modules and prove the public API with
  one focused external example
- extend the delivered versioned project/module manifests only alongside
  parser migrations, validation, and round-trip fixtures

Deliverables:
- public application/library seam with behavior parity
- documented and tested dependency direction
- first selectable capability bundle and consumer example
- first compiled graph vertical slice from Forge source to runtime behavior
- Heavy Water Platformer extracted as the initial game-mode module
- documentation and package checks in CI

### 1. Architecture and ownership cleanup

Priority: reduce ambiguity before broadening scope.

Work:
- keep ownership explicit for players, rewards, vehicles, and shared world state
- runtime/save validation now repairs non-finite or out-of-range settings and
  character body/palette data, clamps local parties to 1–4, validates built-in
  site/route identity and endpoints after world setup, and skips malformed,
  unknown, or duplicate world save records individually with diagnostics;
  fast-travel requests now own their anchor/label strings so published Forge
  content is not restricted to compile-time string literals
- `PlayerScore` is now four explicit owner slots with bounded APIs and computed
  party totals; chest proximity resolves the actual nearest `PlayerIndex`,
  updates only that player's score, and emits owner-bearing chest events. Kill
  and damage scoring remain disabled until enemy damage/lifecycle events carry
  an authoritative player source; adding that attribution is the next score
  ownership step
- reduce plugin-level overloading in the largest gameplay systems
- improve domain boundaries between world logic, UI logic, and player simulation
- continue the editor/game separation work started in [editor_roadmap.md](editor_roadmap.md)

Deliverables:
- clearer ownership rules
- fewer giant system hotspots
- simpler debugging and validation loops

Review disposition for the August resource audit: the README authority map and
archived-document boundary were already current; `Cargo.toml`, lockfile, and the
Bevy adoption plan were already on 0.19.1, while two stale `agent.md` references
were corrected. A physical `resources.rs` split, per-player score migration,
dungeon save-policy redesign, fixed-tick expansion, neutral schema extraction,
and the published-level consumer test remain separate behavior/architecture
slices and should not be mixed into the validation pass.

### 2. Controller-first UX and menu quality

Priority: make the product feel consistent in local multiplayer.

Work:
- finalize the remaining controller-only flows
- reduce hidden assumptions in menu focus, pause, and player-select screens
- improve per-player interactions in loadout, crafting, and reward systems
- ensure the UI handles 1–4 players without cross-player leakage

Deliverables:
- consistent controller navigation
- reliable co-op menu ownership
- clearer input feedback and accessibility

### 3. World and content integration

Priority: make the current campaign feel coherent rather than just large.

Work:
- tighten the most important world loops: chapter flow, dungeon states, map readability, settlements, and route flow
- the top-down arcade reference dungeon now uses a four-room RPG cadence
  (entrance → traversal → mob pit → elite reliquary), ordered waves, encounter
  seals, and one-time party recovery/reward caches; next, tune enemy mixes,
  timings, room dressing, and additional authored dungeon definitions
- the first bounded shared-screen platformer course is now selectable directly
  from the title screen and reuses the production movement/combat stack with a
  P1-led side camera, automatic catch-up bubbles, strict depth/end boundaries,
  checkpoints, co-op switches, mini battles, and workshop-part reward tiers;
  next, conduct four-pad camera/bubble tuning and promote the reward tiers into
  persistent authored prefab/vehicle/weapon/armor/pet unlock records
- shared runtime adapters for every authored platformer behavior are active;
  the Designer now seeds a dedicated introduction → development → twist →
  culmination showcase with a parallel recovery lane; next, playtest and tune
  its spacing/reset timing with the shipping movement controller
- validate world density and performance before adding more content breadth
- ensure the strongest systems are fully legible in play, not just technically present

Deliverables:
- stronger chapter and dungeon identity
- playtest third-person soft-lock acquire/release angles and top-down
  move/facing-based target choice against ground and aerial enemy groups
- tune the showcase ladder, spring, moving/rotating platform, flip, collapse,
  and hazard beats while preserving its readable bypass/reset path
- improved world readability
- better performance-aware content decisions

### 4. Core quality of combat and movement

Priority: keep the action feel strong and understandable.

Work:
- continue tuning movement and combat feel as data rather than code hardening
- validate boss pacing, attack readability, and counterplay
- keep the fixed-tick and response model consistent with the game’s design philosophy

Deliverables:
- clearer combat readability
- more stable boss loops
- better player-feedback fidelity

### 5. Editor and content pipeline discipline

Priority: enforce the product boundary and make authoring reliable.

Work:
- keep Game edition free from editor dependencies
- continue the publish and runtime catalog boundary work
- complete the Bevy 0.19 Forge viewport rollout: the technicolor infinite grid
  and selection/space/snap/undo transform-gizmo adapter are active; native 3D
  stroke labels now identify sockets, spawns, routes, water flow, and gameplay
  boundaries; the contained, controller-navigable Inspector settings panel now
  persists grid, label range/budget, shadow, and Forge key-light contact-shadow
  preferences through Bevy 0.19 machine-local settings; the Outliner filter now
  shares a reusable Feathers-themed `EditableText` adapter with Registry search,
  content-ID rename, project name, and active-level name; clipboard/IME,
  controller apply/cancel, headless fallback, validation, and transactional
  undo/redo are preserved; next extend metadata with description/tags and pilot
  one Feathers property row
- the first Dialogue Forge production slice now stores typed modular graphs in
  versioned project records: linked/branching dialogue nodes, stable runtime
  cursors, gameplay versus cutscene policy, and ordered animation/camera/audio/
  visibility/gameplay-signal cues share one format; the Registry can create a
  graph, append nodes and animation cues, switch modes, and record/attach a
  bounded project-local 96-kbps mono MP3 voice take with undo-safe metadata;
  Forge uses LAME or FFmpeg for production encoding and retains no PCM/WAV take;
  next add
  node/choice property rows, a visual timeline, waveform trim/take selection,
  and a runtime director that adapts published graphs into the existing
  discussion, radio, camera, and animation systems
- extend the shared semantic character geometry foundation now used by both
  Character Studio and Creature Forge: five style coordinates drive preview,
  playable player recipes, and compiled enemy geometry; reusable cubic Bézier
  paths, Bézier radius profiles, stable sweep topology, and parallel-transport
  frames now drive Studio hair, playable hero locks, and continuous enemy
  tails; shared stable-ring lofts now drive all Studio columns plus compiled
  enemy torsos and arms, and Studio edits carry body/face/hair/wardrobe/material
  dependency masks with no-op rebuild suppression and visible generation
  diagnostics; next split the interleaved head and wardrobe spawners into
  independently replaceable subgraphs, then generate skin weights and morph
  targets
- bake level scenes and platformer prefab recipes into the published generation
  and load them without a Forge project store
- protect authored project content with validation and stable revision behavior

Deliverables:
- clean runtime/editor separation
- reliable content publication flow
- one semantic player/enemy character vocabulary without duplicated generator
  controls or anonymous-vertex source data
- Designer-authored platformer routes that run in the consumer Game edition
- less risk of editor changes breaking Player builds

### 6. Performance and validation gates

Priority: protect the large world from silent performance regressions.

Work:
- define a small set of concrete performance gates for terrain, world scale, and split-screen play
- run the relevant validation before major content additions
- keep the build/test loop reliable for both Designer and Game editions

Deliverables:
- measured performance floor
- fewer silent world regressions
- stronger release confidence

## Current working target

The next milestone should focus on a small set of integrated improvements, in roughly this order:

1. ownership and architecture clarity
2. menu and controller consistency
3. world and content integration quality
4. editor/game separation and content pipeline hygiene
5. performance validation before deeper content expansion

This is the preferred sequence because it reduces the chance that new content makes the existing systems harder to reason about.

## Definition of done for the next plan cycle

The next milestone is successful when:

- the project’s major systems are easier to understand and debug
- local multiplayer remains coherent across menus and play
- the core campaign loop feels intentional and readable
- the editor/game boundary is more explicit and better enforced
- the repository documentation matches the codebase more closely than it does today

At that point the team can confidently expand content without adding new architecture chaos.
