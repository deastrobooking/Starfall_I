# Starfall I — Master Feature Documentation

This is the single game-facing feature catalog for Starfall I. It consolidates the content that previously lived across milestone notes, roadmap documents, gameplay reviews, and feature summaries into one authoritative reference. The goal is clarity: a project-wide view of what exists, what is stable, what is still in progress, and where the next work should land.

This document is intentionally feature-first. It is not the build guide, engine roadmap, or architecture map. For implementation details, see [DEVELOPMENT.md](DEVELOPMENT.md). For the next project plan, see [PROJECT_PLAN.md](PROJECT_PLAN.md). For historical milestone context, use the materials in [archive/](archive/README.md).

---

## 1. Product identity

Starfall I is a cartoon action-platformer RPG built around a family of star-powered heroes defending Earth from supernatural and dimensional threats. The current product direction combines:

- 1–4 local co-op play
- exploration across a large 3D world
- chapter-driven campaign structure
- platforming with movement variety and traversal tools
- light-to-heavy action combat with weapons, sabers, and tech abilities
- RPG progression through perks, upgrades, loot, and gear
- creator-facing systems for character authoring and content generation
- a strong world-building layer with settlements, raids, robot pets, and strategic infrastructure

The project currently sits between a playable prototype and a more polished production-scale game. Core systems are already in place, but several feature domains still need better integration, validation, and content polish.

---

## 2. Core pillars

### 2.1 Movement and traversal

The movement layer is one of the strongest pillars in the project. It centers on responsive platforming, planetary traversal, and movement readability in a co-op context.

Key features:
- grounded movement with acceleration, sprinting, jump buffering, coyote time, variable jump release, and apex gravity tuning
- wall slides, wall jumps, edge grabs, ledge hanging, climbing, and mantle behavior
- jetpack and flight behaviors
- grapple zip/swing mechanics
- hoverboard traversal and stunt-oriented movement
- mountain road and route-based traversal networks
- boat and vehicle integration across ground, water, air, and space contexts
- fast travel and map-driven travel across the Everest range

Player feel improvements already landed include:
- the fixed-tick motor and buffered input model
- analog strength preservation
- controller-friendly traversal input
- route-aware and terrain-aware road generation
- board, rail, and loop support for stunt movement

### 2.2 Combat and action feedback

Combat is a data-driven, action-RPG layer layered on top of a readable 3D action feel. It supports fast melee chains, ranged beam weapons, special abilities, and large boss patterns without requiring code recompilation for many tuning changes.

Key features:
- data-driven melee attacks and combo windows
- star sabre combat and technique-driven attacks
- homing and tracking projectiles
- hitstop, flinch, armor, knockback, and impact response
- boss phases with unique behavior patterns
- per-player damage numbers, feedback, and camera shake
- damage scaling by faction, threat level, and system state

The game currently emphasizes:
- readable action combat over deep enemy-sim complexity
- constant tuning of feel values through authored data rather than hardcoded logic
- distinct boss identities rather than a single generic enemy pattern

### 2.3 RPG structure and progression

Progression is split between player-level systems and campaign-level systems. This is a healthy design direction because player identity and world state are different concerns.

Key features:
- retained hero roster of siblings and party identities
- perk progression and tech upgrades
- robot salvage economy and reinforcement loops
- weapon and armor loadouts
- chapter rewards and hidden progression content
- crafting and settlement economy
- persistent world state for sites, raids, and strategic assets

Important boundary:
- progress tied to the player is per-player
- campaign-scale world state is shared and campaign-owned

### 2.4 World structure and campaign

The game world is built around a broad playable campaign with a strong sense of place. It is not just a set of disconnected rooms.

Key features:
- 14-chapter campaign structure with overall progression
- Everest Range as the primary exploration and fast-travel space
- cities, villages, harbors, and outposts
- caves, dungeons, dragon lairs, and labs
- secret rewards, relic puzzles, and discovery-driven content
- raids and strategic pressure events
- settlement building and economy loops
- route networks and traversal-linked world spaces

The project has carried a strong “content world” ambition: the game is large enough that world dramaturgy and systems integration matter as much as raw mechanics.

### 2.5 Creator and authoring tools

A major part of the game identity is that it is not only a consumer product but also a game-making environment. The codebase already supports a functional creator toolchain beyond a simple debug editor.

Creator-facing systems include:
- Character Design and Character Studio
- imported mesh and rig workbench
- GLB export
- Creature Forge
- Weapon Forge
- Vehicle Forge
- Spaceship Forge
- Project Hub and publish workflows
- level authoring and live editing foundations

The design goal is clear: some content creation is for players, while authoring and world editing remain designed for a creator/editor product boundary.

### 2.6 Audio, presentation, and polish

The project places strong value on readable and immediately distinct presentation. It uses a hybrid model of procedural and authored systems.

Key features:
- deterministic procedural SFX synthesizer
- separate music deck and action-audio selection system
- gameplay-driven sound mapping
- per-player UI and HUD behaviors
- stylized cartoon visuals with energy-driven materials and effects
- accessibility settings and controller layout support

---

## 3. Feature inventory by domain

### 3.1 Local multiplayer and ownership

The project has intentionally shipped a strong local multiplayer model. Ownership rules are central to keeping the game coherent in a 1–4 player setup.

Included features:
- player-select flow with 1–4 local players
- split-screen and shared-screen camera states
- independent per-player HUD panels
- per-player save snapshots and progression
- shared party campaign state with per-player gameplay state
- per-player input ownership and control capture
- owner-scoped loadout and inventory interactions
- party-shared vehicle state with explicit driver/passenger rules

Current strength:
- ownership boundaries are already much clearer than a typical prototype

Current risk:
- some reward, feedback, and menu flows still assume a single owner or a party-shared flow too often

### 3.2 Player movement and traversal

Movement is one of the most mature systems in the project.

Included features:
- analog movement and acceleration tuning
- jump, air control, wall slide, wall jump, and edge catch
- climb and free-climb systems
- grapple system with zip/swing and route scoring
- jetpack, hover, flight, and glide states
- rocket hoverboard traversal and stunt workflows
- major terrain-following and route-aware world surfaces
- support for mountain roads, air routes, rail-based motion, and vertical traverse

Current strength:
- the feel is consistent with the game’s fantasy: readable, responsive, and technically rich

Current risk:
- more of the system is spread across multiple gameplay plugins and states than ideal for long-term maintenance

#### Authored platformer prefab library

Starfall Forge exposes an original, recipe-driven platformer kit rather than
requiring designers to assemble every route from generic blocks. The palette
contains ladders, stairs, Star Towers, Star Castles, floating islands, moving
platforms, Pulse Springs, flip panels, rotating bridges, collapse bridges, and
spike bridges.

Each prefab has:
- a stable serialized primitive identity
- generated geometry with an editable transform and material
- a stock size and repeat/detail count in its designer recipe
- explicit gameplay intent (`climbable`, `static traversal`, `oscillate`,
  `bounce`, `flip on jump`, `rotate`, `collapse`, or `hazard`)
- representative placement in the open-world, dungeon, airship, boss, racing,
  and settlement level templates, plus a dedicated Platformer Showcase route

Current maturity boundary:
- authoring, save/load, duplication, material application, mesh generation, and
  template seeding are active
- all prefab collision is derived from the same geometry as its render mesh;
  moving platforms, Pulse Springs, and rotating bridges compile into the
  existing co-op-aware gameplay adapters
- flip panels debounce nearby jump input and animate to the opposite face;
  collapse bridges warn, fall, and restore without transform drift; spike
  bridges use per-player cooldowns and the canonical armor/parry/damage path
- leaving the editor refreshes runtime origins, path axes, sizes, and rotations
  from the latest authored transforms before immediate playtesting
- the Platformer Showcase template authors an introduction → development →
  twist → culmination route, gives editor objects semantic stage names, aligns
  long bridges with the forward line, and runs a visible static-island recovery
  lane beside every consequence verb
- authored scenes still need a consumer-side scene/prefab loader before these
  prefabs can ship from Designer projects into the Game edition without Rust

The showcase is the reference composition for new routes: introduce one
traversal verb safely, develop it with a clear variation, add a twist, and only
then combine it into a culminating sequence. Exact spacing and behavior tuning
still require hands-on playtests with the shipping movement controller.

### 3.3 Combat systems

Combat is robust and increasingly data-driven.

Included features:
- sword and star-saber melee
- ranged weapons and energy beams
- weapon specials and charge-based effects
- tracking projectiles and smart weapon behavior
- hitstop and reaction timing
- flinch and impact responses
- boss behavior variation across dragons, mechs, and Scallarian threats
- damage, armor, and knockback systems
- third-person open-world soft targeting with a narrow camera-circle acquire
  threshold, wider sticky release threshold, line-of-sight checks, and no
  teammate occlusion; first-person hip fire remains manual
- drone hurtboxes backed by both their authored physics cuboids and a bounded
  logical wing/body sweep for fast projectiles and melee/saber attacks

Current strength:
- combat is more readable and coherent than earlier iterations

Current risk:
- a great deal of combat tuning, boss behavior, and collision logic lives in a broad set of systems and may need consolidation into clearer data contracts

### 3.4 Campaign and world systems

The campaign structure is broad and ambitious.

Included features:
- chapter progression and chapter-select structure
- 14-chapter world and story pacing
- world sites and reclaimable state
- settlements and buildable economies
- raids and defense loops
- secret caves, dragon lairs, and boss spaces
- hard-boundary top-down arcade dungeons with room-linked shared cameras,
  traversal-kit limits, ordered encounter waves, sealing combat thresholds,
  elite culmination rooms, and one-time party relic caches
- a title-screen **Starbound Co-op Platformer** alternative for 1–4 players:
  bounded side-progressing 3D stages, a P1-led shared camera, catch-up bubbles,
  checkpoints, stair/climb towers, moving lifts, pulse springs, collapse-panel
  timing, simultaneous pressure plates, compact mini battles, and a workshop
  part loop that awards each player arena-ready credits and experience while
  exposing prefab → weapon → armor → pet/vehicle build tiers
- temple subquests and exploration rewards
- world map, routes, and travel anchors

Current strength:
- the project clearly has a large campaign vision instead of only a slice of content

Current risk:
- a lot of world logic is concentrated in the same large simulation/plugin layers, which makes it harder to reason about without tooling or documentation support

### 3.5 Robot pets, vehicles, and combined forms

This is one of the strongest “third pillar” systems once the core game loop is stable.

Included features:
- robot salvage and rescue loops
- pet collection and assembly
- vehicle and mecha forms
- ship and high-flying forms
- garage and assembly interfaces
- cycle between ground, air, water, and combat-capable forms

Current strength:
- the fantasy of “robot forms powering hero traversal and strategy” is well established

Current risk:
- the combination of vehicle ownership, party state, and runtime form switching needs stricter clarity to avoid ownership ambiguity

### 3.6 Character and visual authoring

The game contains a strong player-facing character creation stack.

Included features:
- Character Design flow
- Character Studio with parametric body and face editing
- shared Fantasy, Cute/Chibi, Heroic, Mechanical, and Creature style-space
  controls with smooth, non-destructive projection into player and enemy forms
- preset-based and morph-based authoring
- procedural humanoid generator and mesh assembly
- shared cubic Bézier sweep geometry with authored radius profiles,
  stable topology, and parallel-transport frames for seamless player hair and
  enemy tails
- shared stable-ring torso/limb lofts across Character Studio and compiled
  enemies, plus dependency-classified Studio regeneration diagnostics
- style-aware runtime blueprint export so the playable character retains the
  proportions shown in Character Studio
- GLB export and rig import diagnostics
- modular body part and material logic
- generated character animation support

Current strength:
- the authoring pipeline is creative and clearly tied to the game identity

Current risk:
- there is an obvious gap between the procedural generator and a full production-grade import/rigging pipeline; the current system is advanced but not yet a true “full authoring suite”

### 3.7 Creator tools and content pipeline

The project’s editor/product boundary is strong in concept and already well-documented in the roadmap.

Included features:
- Project Hub and project registry
- creature, weapon, vehicle, and spaceship tools
- Creature Forge style-space controls compiled from stable saved dimensions,
  preventing repeated preview rebuilds from compounding morphology
- live editor foundations
- Bevy 0.19 infinite editor grid and transform handles integrated with Forge
  selection, world/local space, snapping, controller actions, and undo
- draft/publish workflows
- manifest-based content bootstrapping
- deterministic game-side content consumption
- a modular platformer prefab palette with generated meshes and semantic
  behavior tags

Current strength:
- the intended separation between game and designer edition is already conceptually sound

Current risk:
- the runtime/editor boundary is not yet fully sealed; the next major work is architectural separation rather than more content expansion

### 3.8 Audio and UX

The audio and interface systems are innovative and useful.

Included features:
- procedural SFX generation
- custom music deck
- action SFX mapping
- input prompt and controller glyph support
- accessibility controls and menu polish
- multiple local-player HUD layouts

Current strength:
- the game feels intentional, not just mechanically functional

Current risk:
- UI density and shared navigation patterns can become hard to manage without stricter UI ownership and more explicit panel contracts

---

## 4. Notable strengths of the codebase

The project has several strong foundations that justify the work already done:

- The game has a coherent fantasy and visual identity.
- The core action loop is readable and adventure-oriented.
- The movement and traversal systems are substantial and enjoyable.
- The world is large enough to support a full campaign and not just a vertical slice.
- Local multiplayer is treated as a real product goal rather than a secondary feature.
- Save, progression, and data ownership are more structured than many Y1 prototypes.
- The authoring and forge tools show that the project has a real creator strategy, not just a game loop.
- The documentation is already rich, and the work of consolidating it into one coherent narrative is now possible.

---

## 5. Improvement areas worth addressing next

These are the areas most likely to need focus if the project is going to move from “strong prototype” to “stable, polished production game.”

### 5.1 World/plugin consolidation

The largest practical risk is that the game world, UI, and player logic have become too concentrated in major plugin systems. This leads to:
- harder onboarding for new contributors
- harder debugging of cross-feature interactions
- more overlap between gameplay systems
- more risk of state ownership ambiguity

Priority: break large feature areas into clearer domain boundaries without changing behavior.

### 5.2 Ownership clarity

The game is generally good about using stable player ownership, but some reward and interaction flows still show lingering assumptions that one player is effectively the active player. This is especially visible in:
- co-op menu flows
- reward pickup ownership
- vehicle mode and passenger rules
- feedback and HUD authority

Priority: close the remaining gaps in player ownership and make local multiplayer behavior explicit in the data model.

### 5.3 UI architecture and menu clarity

The project has made big progress on UI architecture, but the amount of UI logic and shared menu focus still demands a stronger UI contract.

Priority areas:
- unify menu ownership
- shrink one-off UI state exceptions
- improve controller-only and controller-heavy flows
- reduce hidden assumptions in menu/focus routines

### 5.4 Content integration and production QA

The game is large enough that content quality varies by region. Several systems are present but still have a “prototype-quality” feel in production use:
- some world sections feel more authored than integrated
- certain boss, dungeon, or quest patterns need stronger gameplay validation
- some content needs more explicit route clarity and discoverability

Priority: identify the most important content loops and make them feel coherent before expanding more systems.

### 5.5 Tool/editor boundary

This is the clearest long-term technical need. The project already knows the separation between Game and Designer must exist, but it remains closer to a strong concept than a fully sealed architecture.

Priority: enforce the boundary through module ownership and a clear content pipeline instead of relying on workarounds.

### 5.6 Performance and world scale validation

The project is ambitious enough that world scale and split-screen performance must be continuously measured and validated.

Priority: define a handful of concrete performance gates and use them before adding major world density or larger content expansion.

---

## 6. Production readout

### 6.1 Strongest systems today

- traversal and movement
- local co-op foundation
- world scale and campaign ambition
- progression and persistence
- creator tool direction
- action combat readability

### 6.2 Systems still needing sharper discipline

- UI ownership and menu flow consistency
- final ownership boundaries in some co-op flows
- structural separation of game and editor code
- world/plugin consolidation
- content integration and production quality validation
- performance gate discipline for a large world

### 6.3 Likely next output once this doc is used as the canonical source

The next operational plan should focus on the following priorities rather than a generic “make everything better” list:

1. reduce ambiguity in feature boundaries and ownership
2. clean up the largest world and UI hotspot areas
3. harden the editor/game separation
4. validate remaining co-op and controller flows with hardware testing
5. improve the content quality of the campaign’s most important loops before broad expansion

---

## 7. Documentation principle

The purpose of this document is to make the game easier to understand without forcing a reader to piece together dozens of milestone docs. When a system changes, the feature description should be updated here first. Historical milestone and planning documents remain important, but they should support this master reference rather than become the primary source of truth.
