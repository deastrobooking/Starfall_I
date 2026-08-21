# Bevy 0.19.1 Adoption Plan

Reviewed 2026-08-20 against the official Bevy 0.19 release notes, migration
guide, and 0.19.1 crate metadata. This is the implementation plan for using
Bevy 0.19 as Starfall's reusable four-player game and editor foundation.

## Decision

Starfall will track Bevy `0.19.1`, the current stable patch, with Avian `0.7`.
We will adopt the editor and rendering additions in measured slices rather than
rewrite mature systems. BSN owns fixed entity structure; Starfall components,
systems, and mode directors continue to own runtime state and simulation.

Official references:

- [Bevy 0.19 release notes](https://bevy.org/news/bevy-0-19/)
- [Bevy 0.18 to 0.19 migration guide](https://bevy.org/learn/migration-guides/0-18-to-0-19/)
- [Bevy 0.19.1 crate](https://docs.rs/crate/bevy/0.19.1)
- [Bevy SettingsPlugin](https://docs.rs/bevy/0.19.1/bevy/settings/struct.SettingsPlugin.html)

## Current Starfall fit

| Area | Existing foundation | Decision |
|---|---|---|
| Four-player ECS | `PlayerIndex`, player components, generic systems, and `GamepadAssignments` already avoid four copies of gameplay code | Keep; add a resource-query safety audit |
| Cameras | One-to-four viewports, shared/split transitions, per-player render layers, and mode-specific camera behavior already exist | Keep; benchmark every quality feature in one-, two-, and four-view layouts |
| Settings | Atomic JSON persistence already covers controls, audio, UI, accessibility, and world seed | Preserve save compatibility; pilot Bevy settings for machine/editor preferences only |
| Editor grid | Forge draws a finite grid with gizmos | Replace the visual grid with `InfiniteGrid`; retain Starfall snapping and controller focus |
| Transform tools | Forge has translate/rotate/scale modes, snapping, direct manipulation, and controller actions | Adapt Bevy's `TransformGizmo` behind the existing tool API; do not remove the current path until parity passes |
| Prefabs | Procedural recipes, generated assets, and runtime spawn functions are established | Add BSN only for fixed hierarchies and reusable editor UI; keep procedural world generation imperative |
| Diagnostics | F11 already reports frame/entity/LOD data | Add Bevy diagnostic presets behind the same toggle and add per-camera render budgets |
| Rendering | Toon/PBR materials, custom materials, multiple gameplay cameras, and many authored lights exist | Add quality-tiered contact shadows and selective reflections; area lights are a feature-gated pilot |

## Corrections to the supplied guide

1. Bevy 0.19's delayed syntax is `commands.delayed().secs(1.0)`, not
   `commands.delay(...)`. Delayed commands have no built-in cancellation, so
   Starfall's cancellable respawn, round, boss, and transition directors should
   remain explicit. Delayed commands are suitable for disposable VFX and audio
   cues whose origin entity can be validated.
2. Do not persist Bevy `Gamepad` entity IDs. They are runtime ECS identities,
   not stable hardware identifiers. Persist per-slot dead zones, sensitivity,
   glyph preference, and optional device-profile hints; rebuild entity ownership
   from connected devices in the lobby.
3. Resources are components on dedicated singleton entities in 0.19, but this
   is not a reason to add `Without<IsResource>` to every query. Use domain
   components such as `With<Player>` or `With<Authorable>` as the normal safety
   boundary and reserve `Without<IsResource>` for intentionally broad tooling.
4. BSN 0.19 is code-first. Bevy does not ship a first-party `.bsn` asset loader
   yet, and the new scene API is expected to keep evolving. Starfall's saved
   editor projects must remain in its versioned, migration-aware format.
5. Four viewports do not guarantee exactly four times the render cost, but
   screen-space and visibility work is repeated per camera. Decisions require
   measurements from Starfall's actual split-screen scenes.

## Adoption sequence

### B19-0 — Patch baseline and ECS safety

- Update Bevy and the full Bevy lockfile family to `0.19.1`; retain Avian `0.7`.
- Run compile, strict clippy, full tests, and a release build.
- Audit unfiltered entity queries used by cleanup, serialization, editor
  discovery, and diagnostics. Domain-filter them where resource entities could
  produce incorrect behavior.
- Add a dependency-version check to release documentation or CI so patch drift
  is visible without silently adopting a new Bevy minor release.

Acceptance: all automated gates pass; the main menu, player lobby, one-player
camera, four-player camera, Forge entry/exit, and one combat encounter pass a
manual smoke test.

### B19-1 — Forge viewport foundation *(core landed 2026-08-21)*

- Add `InfiniteGridPlugin` only while Forge is active. Configure per-camera
  colors, major-axis emphasis, line scale, and fade distance to match the
  technicolor Starfall palette.
- Keep the current finite Gizmos grid behind a fallback flag until Metal and
  low-quality tests pass.
- Add Bevy `TransformGizmoPlugin` through a `ForgeTransformAdapter` that maps
  existing translate/rotate/scale, world/local space, snapping, undo batches,
  selection, mouse drag, and controller actions.
- Use text gizmos for author-only labels such as sockets, spawn points, nav
  links, water flow, road lanes, and gameplay-mode boundaries.

Acceptance: mouse and controller can select and transform the same objects;
undo/redo creates one transaction per drag; grid and handles remain readable at
near, city, and world scales.

Status: the production Designer now enables Bevy's `InfiniteGridPlugin` and
`TransformGizmoPlugin`. Forge maps its persistent active selection, tool mode,
world/local space, translation/rotation/scale snapping, and undo transaction
boundary through a Starfall adapter. The original finite grid and transform
implementation remain available to reduced/headless plugin harnesses. Manual
Metal readability and pointer-over-window smoke testing remains; semantic text
labels are the next contained viewport addition.

### B19-1.5 — Semantic character geometry *(foundation landed 2026-08-21)*

- Share a serializable five-axis style vector across Character Studio and
  Creature Forge: Fantasy, Cute/Chibi, Heroic, Mechanical, and Creature.
- Project style intent into generator-specific anatomy rather than directly
  editing vertices. Human preview geometry, playable `BodyRecipe` output, and
  compiled creature/enemy geometry all consume the same coordinates.
- Keep style projection non-destructive: saved base dimensions remain stable,
  compilation works from a clone, smoothstep interpolation avoids slider kinks,
  and old files deserialize to a neutral vector.
- Reusable cubic Bézier paths, Bézier radius profiles, capped stable-topology
  sweeps, superellipse cross-sections, and parallel-transport frames landed in
  the shared character module. Character Studio side ponytails, playable hero
  hair locks, and compiled enemy tails consume the same primitive; tail segment
  count now controls continuous sweep tessellation instead of disconnected
  capsules, while existing modifier stacks remain downstream-compatible.
- Continue extraction in this order: shared lofted torso/limb topology;
  dependency-scoped regeneration; generated skin weights; then morph-target
  authoring.

Acceptance: both editors expose all five axes; presets establish distinct
style-space positions; Character Studio exports the visible proportions into
playable characters; Creature Forge compilation never compounds dimensions;
legacy recipes load unchanged.

Sweep-slice acceptance: curve edits retain vertex/index counts at a fixed
tessellation; generated positions and normals remain finite; neighboring
parallel-transport frames do not flip; player and enemy runtime generators both
consume the shared primitive without a recipe migration.

### B19-2 — Editor UI and settings pilot

- Pilot `EditableText` and Feathers on one contained Forge panel: prefab search,
  property editing, or project metadata. Preserve controller focus and existing
  accessibility styles.
- Create separate settings groups for `RenderQualitySettings`,
  `EditorPreferences`, and per-slot `PlayerControlPreferences`.
- Keep campaign progress, character data, authored projects, and world state in
  Starfall's save system. Migrate existing machine settings once, then continue
  reading old JSON fields for backward compatibility.
- Save deliberately on Apply/exit using Bevy's save commands; do not assume the
  settings plugin automatically writes changed values.

Acceptance: preferences survive restart, corrupt settings recover safely,
controller-only navigation remains complete, and old settings files load.

### B19-3 — Prefab kernel with BSN

- Introduce `engine::prefab` with a stable Starfall-facing descriptor and spawn
  API. Keep Bevy scene details behind that boundary so future scene migrations
  do not spread through gameplay code.
- Pilot three fixed hierarchies:
  1. an interactable with prompt anchor, collider, highlight, and logic socket;
  2. a mode portal with destination, transition volume, marker, and preview;
  3. a player equipment rig with hand/back/vehicle sockets, without replacing
     the complete player spawn path.
- Use `SceneComponent` only where the root component must guarantee the child
  hierarchy. Test every invariant after spawn and after editor duplication.
- Let Forge serialize Starfall prefab descriptors and overrides. Do not make
  `.bsn` files a user-facing project format in 0.19.

Acceptance: each pilot can be spawned in gameplay and Forge, duplicated,
overridden, saved, loaded, and upgraded without losing stable editor IDs.

### B19-4 — Gameplay-style recipes and transitions

- Define a data-driven `GameplayStyleRecipe` for open world, shared-camera
  platformer, top-down dungeon, hoverboard race, arena, and editor preview.
- Each recipe declares camera policy, boundary policy, movement profile, combat
  profile, HUD, respawn/bubble rules, active systems, lighting preset, and exit
  contract. Runtime state remains in components/resources, not BSN.
- Route every transition through one director with preload, fade, spawn/restore,
  camera handoff, input ownership, and rollback-on-failure stages.
- Expose recipes and prefab sockets in Forge so a creator can build a single-
  style game or a world that moves fluidly among styles.

Acceptance: automated transition tests cover every style pair that the shipped
game uses; four players retain identity, loadout, health/progression policy, and
controller ownership across transitions.

### B19-5 — Lighting, shadows, and reflections

- Add render tiers: `Performance`, `Balanced`, and `Cinematic`, with automatic
  split-screen overrides.
- Enable contact shadows on the principal directional light first and add the
  camera component only for supported gameplay/editor views. Avoid enabling
  them on many local lights: cost scales with covered pixels and enabled lights.
- Pilot rectangular area lights in one interior/editor preview scene. They need
  Bevy's `area_light_luts` Cargo feature and currently do not cast shadows.
- Use screen-space reflections only on tagged wet/glossy materials and only in
  shared/single-camera high-quality modes until four-view budgets pass.
- Use vignette/lens distortion for short, player-specific feedback such as
  damage, warp, or extreme speed. Respect reduced-motion/accessibility settings
  and avoid duplicating the existing HUD damage vignette.
- Validate toon materials with the corrected PBR energy behavior and test
  animated imported characters against 0.19's improved skinned-mesh culling.

Acceptance budgets at the target resolution: stable frame pacing in all camera
layouts; no material regressions; every costly feature can be changed live and
persists as a machine preference.

### B19-6 — Diagnostics, asset publishing, and custom rendering

- Fold `DiagnosticsOverlayPlugin` presets into F11, including frame time,
  entities, meshes/materials, camera count, visible objects per camera, shadowed
  lights, and active post effects.
- Capture benchmark scenes for city traversal, four-player combat, platformer,
  race road, dungeon interior, and Forge world view. Store tier baselines.
- Evaluate `AssetSaver` for generated thumbnails and derived/imported assets;
  keep Starfall's atomic versioned project persistence as the source of truth.
- Any future custom camera pass must use 0.19 render-world systems scheduled in
  `Core3d`/`Core2d`, with explicit per-view filtering and four-camera budgets.
- Use contiguous query access only after profiling proves a hot table-shaped
  loop; do not contort ordinary gameplay systems for theoretical speed.

Acceptance: performance regressions are visible per scenario and render tier;
saved editor outputs are atomic, reopenable, and migration-tested.

## Priority

Immediate work is B19-0, then B19-1. The infinite grid and transform adapter
produce the highest editor value with low gameplay risk. B19-3 follows with a
small prefab kernel before any broad player/UI conversion. Rendering work lands
only after the four-camera benchmark harness exists, so visual upgrades remain
scalable for couch co-op.
