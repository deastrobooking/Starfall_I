# Starfall I — Project Plan

This project is built around two connected editions of the same codebase:

| Edition | Audience | Scope |
|---|---|---|
| Game | Players | Campaign, combat, traversal, co-op, save flow, and player-facing creation tools such as character design and garage-style content. |
| Designer | Creators | Everything in the Game plus the authoring layer: Project Hub, Forge tools, live editing, published content pipelines, and versioned design records. |

The product boundary is intentionally simple: the Game is the consumer subset, and the Designer edition adds creation and authoring tools without creating a second product tree. The repository continues to use a single cargo workspace and a single engine stack, with the `designer` feature gating tool entry points.

## Current state

The project is strong enough to be called a real prototype-to-production game, but it still contains areas where scope, ownership, and architecture are wider than the current team structure. The game already has meaningful movement, combat, world exploration, and co-op foundations; the next work should reduce risk instead of simply adding more feature surface area.

The active feature inventory lives in [MASTER_FEATURES.md](MASTER_FEATURES.md). The active engine and editor strategy lives in [engine_roadmap.md](engine_roadmap.md) and [editor_roadmap.md](editor_roadmap.md). The historical milestone material remains in [archive/](archive/README.md) for reference only.

## Product strategy

1. Keep the Game edition compact and stable.
2. Keep the Designer edition authoritative for authored content and validation.
3. Treat the Game and Designer editions as one product family, not two divergent codebases.
4. Make the next milestone work about clarity and integration, not about adding every remaining idea.

## Next workstreams

### 1. Architecture and ownership cleanup

Priority: reduce ambiguity before broadening scope.

Work:
- keep ownership explicit for players, rewards, vehicles, and shared world state
- reduce plugin-level overloading in the largest gameplay systems
- improve domain boundaries between world logic, UI logic, and player simulation
- continue the editor/game separation work started in [editor_roadmap.md](editor_roadmap.md)

Deliverables:
- clearer ownership rules
- fewer giant system hotspots
- simpler debugging and validation loops

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
  and selection/space/snap/undo transform-gizmo adapter are active; next add
  semantic socket, spawn, route, water-flow, and mode-boundary text labels
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
