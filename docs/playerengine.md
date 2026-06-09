# Starfall I Player Mechanics And Animation MVP Plan

> **Naming convention:** Motion mechanics milestones use the `MM#` prefix to
> distinguish them from engine/campaign milestones (`M#` in
> `docs/engine_upgrade_milestones.md`) and future enemy-AI milestones (`AI#`).

This is the agent-facing plan for upgrading Starfall I's player feel from a
prototype platformer into a heroic sci-fantasy traversal system. The design is
inspired by superhero swing movement, mountain parkour, fantasy flight, action
RPG combat readability, and local co-op fairness.

Label convention: this document's milestones should be referenced as
`Motion MM#`. Bare `M#` labels belong to `docs/engine_upgrade_milestones.md`.
Future enemy behavior planning should use `Enemy AI AI#` labels so movement,
engine, and AI work can cross-reference each other without ambiguity.

## Core Concept

Starfall uses one star-tech grappling hook per player, not a two-web system.
The hook should eventually support four verbs:

- Swing: attach to cliffs, bridges, ships, towers, boss armor, and authored hook
  sockets, then preserve momentum through a readable arc.
- Zip: pull the player quickly toward a point to climb mountains or cross gaps.
- Attack pull: yank toward small enemies or pull lighter enemies/items toward
  the player, depending on target class.
- Utility pull: trigger switches, rescue civilians, tug crates, open dungeon
  gates, or latch onto robot-pet boost points.

The hook is a traversal/combat bridge, not a replacement for existing movement.
Wall slides, wall jumps, ledge hangs, jetpack flight, slingshots, dungeons,
vehicles, and boss-mode camera all remain part of the movement vocabulary.

## Current Baseline

- Players are Rapier kinematic capsules driven through `PlayerMovement`.
- Current mechanics include analog walking/sprinting, stamina, coyote time, jump
  buffering, early jump release, apex float, stronger fall gravity, wall slide,
  wall jump charges, intentional hang/climb, dodge, parry, jetpack lift,
  slingshot pads, moving platforms, rotating elevators, and boss-mode party
  pull.
- Current procedural character visuals use `CartoonPose` with separate idle,
  walk, run, jump, fall, flight, one-hand wall slide, attack, Star Sabre slash,
  hang, and grapple wind-up poses.
- Current runtime characters are still skeleton-free modular meshes. Individual
  `CartoonPart` entities store root-relative base transforms and are parented
  directly to the character root. `CharacterBlueprint` already stores rig,
  socket, IK, and animation recipe data, but the live renderer does not yet
  instantiate joint entities from that schema.
- `GrappleHookState` is the first MVP hook foundation: it stores the single-hook
  mode, cable tuning, wind-up/cooldown timers, zip/mountain/attack pull tuning,
  and future attach-point data.

## MVP Milestones

### MM1: Traversal Foundation And Grapple Input

Goal: create a safe foundation for hook, flight, and animation work without
destabilizing existing movement.

- Add a single-hook component with explicit modes: Ready, Windup, Searching,
  Swinging, Zipping, Recovering, and Cooldown.
- Add dedicated input: `G` on keyboard and Select+RB on controller.
- Add `PlayerState::Grappling` and a distinct `CartoonPose::Grapple`.
- Keep this first slice non-physical: pressing grapple only plays the wind-up
  pose and cooldown. Raycasts and movement forces start in M2.
- Add pure tests for hook readiness, cooldown, and cable-length tuning.

Acceptance:

- Game still passes format, check, clippy, and tests.
- Pressing grapple has a visible character pose path.
- Future agents can add raycasts/physics by extending `GrappleHookState`
  instead of inventing a parallel traversal state.

### MM2: Hook Targeting And Attach Rules

Goal: let the single hook find valid targets in the world.

- Add raycast targeting from camera aim and optional soft-lock from player
  forward direction.
- Add `GrappleSocket` or similar marker for authored reliable targets.
- Support broad surface attach for cliffs/buildings only when the surface is
  allowed and the angle/distance are readable.
- Classify target intent: SwingPoint, ZipPoint, EnemyPull, ItemPull, Switch,
  BossWeakPoint, and Denied.
- Add player-facing blocked feedback when no target is found.

Acceptance:

- A test arena can mark several hook sockets.
- Pressing grapple enters Searching and records a valid attach point or cleanly
  fails into cooldown.
- No target path can crash or leave the player stuck.

### MM3: Zip And Mountain Pull

Goal: make the first physical hook verb useful in the Everest range.

- Add zip-to-point force for rapid mountain ascent and gap crossing.
- Clamp speed, vertical lift, and arrival radius so players do not tunnel
  through terrain.
- Add detach on jump, dodge, attack, distance reached, timeout, or blocked line
  of sight.
- Use stamina or hook heat so repeated mountain pulls remain a rhythm.
- Add route props that teach zip ascent on cliffs and castle gates.

Acceptance:

- Player can pull rapidly up a test cliff without using jetpack.
- Zip does not bypass dungeon/boss locks unless a target explicitly permits it.
- Keyboard/mouse and controller controls both work.

### MM4: Swing Physics

Goal: turn attached hook movement into a readable pendulum.

- Use kinematic-friendly swing math first: cable direction, desired cable
  length, radial correction, tangential momentum preservation, and damping.
- Only introduce Rapier joints if the kinematic solution proves insufficient.
- Add pump/steer inputs, release impulse, swing length tuning, and coyote-style
  release forgiveness.
- Add visual line, attach spark, and release trail.

Acceptance:

- Player can attach to a bridge/tower and swing across a gap.
- Release preserves useful momentum without becoming uncontrollable.
- Swing state interacts safely with boss-mode party pull and split screen.

### MM5: Grapple Combat And Utility Pull

Goal: make the hook part of Starfall combat, not just traversal.

- Pull lightweight enemies or zip to heavy enemies based on target class.
- Add Star Sabre and hand-combat cancel windows from hook arrival.
- Add hook stun, shield yank, turret pull, item pull, rescue pull, and switch
  pull profiles.
- Boss targets can expose temporary hook weak points during phases.

Acceptance:

- One enemy type and one utility object support hook interaction.
- Hook combat cannot corrupt per-player ownership or shared campaign state.

### MM6: Flight Mechanics Upgrade

Goal: make flight feel like an evolving hero skill while preserving platforming.

- Split airborne behavior into jump, fall, glide, hover, jet boost, air dash,
  boost climb, and slam.
- Let Ancient Flight Core and pilot robot pets improve fuel, control, dash
  cooldown, and recovery.
- Add wind volumes: updrafts, downdrafts, side gusts, dragon wing shockwaves,
  city fan lifts, and airship turbulence.
- Add anti-flight ceilings/zones for dungeons and boss arenas.

Acceptance:

- Flight has at least one new controllable verb beyond holding jetpack.
- Hook, flight, and wall movement can chain without infinite hover.

### MM7: Joint-Hierarchical Skeleton Mesh Foundation

Goal: replace root-offset pose animation with a real runtime joint hierarchy
while preserving Starfall's modular cartoon part system.

This is not a glTF import milestone yet. It is an internal Bevy scene-graph
skeleton that uses parent-child `Transform` propagation as forward kinematics.
The result should still render the current capsule/anime parts, but those parts
attach to named joints instead of being animated as root-relative loose pieces.

#### MM7.1 Runtime Components

Add the skeleton component model to `src/components/character.rs`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JointKind {
    Pelvis,
    Spine,
    Chest,
    Neck,
    Head,
    LeftShoulder,
    LeftElbow,
    LeftWrist,
    RightShoulder,
    RightElbow,
    RightWrist,
    LeftHip,
    LeftKnee,
    LeftAnkle,
    RightHip,
    RightKnee,
    RightAnkle,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct JointMarker {
    pub kind: JointKind,
}

#[derive(Component, Debug, Clone, Default)]
pub struct SkeletonRig {
    pub joints: std::collections::HashMap<JointKind, Entity>,
}
```

Keep this runtime layer separate from `RigRecipe`: `RigRecipe` is saved design
data, while `SkeletonRig` is the spawned entity map used by systems. Future
agents may build the runtime rig from `RigRecipe`, but the first slice can use a
fixed humanoid table.

#### MM7.2 Humanoid Joint Hierarchy

Add `spawn_skeleton_rig(commands, root, scale, body) -> SkeletonRig` in
`src/characters.rs`. It should spawn `SpatialBundle` joint entities and parent
them in this order:

```text
CharacterRoot
└── Pelvis
    ├── LeftHip
    │   └── LeftKnee
    │       └── LeftAnkle
    ├── RightHip
    │   └── RightKnee
    │       └── RightAnkle
    └── Spine
        └── Chest
            ├── Neck
            │   └── Head
            ├── LeftShoulder
            │   └── LeftElbow
            │       └── LeftWrist
            └── RightShoulder
                └── RightElbow
                    └── RightWrist
```

Joint local offsets should derive from `BodyRecipe` so the same character
sliders that alter collider proportions also alter limb reach, shoulder width,
hip width, neck length, and head height.

#### MM7.3 Part-To-Joint Attachments

Extend `CartoonPart` with an optional joint binding:

```rust
pub struct CartoonPart {
    pub root: Entity,
    pub kind: CartoonPartKind,
    pub joint: Option<JointKind>,
    pub base_translation: Vec3,
    pub base_rotation: Quat,
    pub base_scale: Vec3,
}
```

Add a local mapping helper:

| Part Kind | Joint |
|-----------|-------|
| `Body`, `Belt`, `Cape`, `SpineRidge` | `Chest` or `Spine` |
| `Head`, `Hair`, `Hood`, `Hat`, `Eyes`, `Brows`, `Nose`, `Mouth`, `Visor`, `Horns` | `Head` |
| `LeftArm`, `ShoulderPadLeft` | `LeftShoulder` |
| `RightArm`, `ShoulderPadRight` | `RightShoulder` |
| `LeftHand` | `LeftWrist` |
| `RightHand` | `RightWrist` |
| `LeftLeg` | `LeftHip` |
| `RightLeg` | `RightHip` |
| `LeftFoot`, `LeftBoot` | `LeftAnkle` |
| `RightFoot`, `RightBoot` | `RightAnkle` |
| `Tail` | `Pelvis` |
| `StarBadge` | `Chest` |

Update `spawn_part` to accept `Option<&SkeletonRig>`. If a joint exists, parent
the mesh entity to the joint; otherwise parent to the root as a compatibility
fallback. Robot swap helpers in `character_parts.rs` should use the same mapping
so humanoid and mechanical pieces share one attachment contract.

#### MM7.4 Procedural FK Animator

Refactor `cartoon_animation_system` in `src/plugins/character_plugin.rs` so the
main pose logic drives joint local rotations first. Mesh attachments should keep
their base local transforms, while joints carry the motion.

Initial forward-kinematic targets:

- `Pelvis`: bob, hip yaw, landing squash.
- `Spine` / `Chest`: sprint lean, glide lean, wall-slide hunch, parry brace.
- `Head` / `Neck`: look-at tendency toward movement, hook targets, and enemies.
- Shoulders / elbows / wrists: run arm swing, one-hand wall clasp, grapple
  wind-up, swing hang, sabre slash, melee guard.
- Hips / knees / ankles: walk/run cycle, jump tuck, fall extension, landing
  bend, wall slide foot drag.

Keep `PlayerStateMachine` as the gameplay source of truth and `CartoonPose` as
the animation-state bridge. Do not move gameplay authority into the skeleton.

#### MM7.5 Compatibility And Cleanup

- Existing `CharacterLoadout` swaps must keep working.
- Existing `CharacterDesign` preview must spawn the same skeleton path as live
  players so previews match runtime.
- Existing shadow/body/head part kinds should not disappear when a loadout is
  toggled back from robot to humanoid.
- Leave a fallback path for non-humanoid enemies until creature/boss rigs are
  explicitly authored.

Acceptance:

- A spawned hero has a `SkeletonRig` with all 17 expected joints.
- Current cartoon/anime meshes render in the same rough silhouette but are
  parented to joints, not directly animated from the root.
- Walk/run/jump/fall/wall-slide/grapple/sabre poses visibly move the skeleton
  through joint rotations.
- Robot part swaps attach to the correct joints and survive character designer
  preview reloads.
- No gameplay system reads a joint entity as authoritative player position; root
  movement and collision remain owned by `PlayerMovement` and Rapier.

### MM8: Procedural Animation And IK

Goal: make the skeleton respond naturally to terrain, walls, hooks, and flight.

MM8 assumes MM7's `SkeletonRig` exists. Do not build IK directly against loose
mesh parts.

#### MM8.1 IK Target Components

Add lightweight runtime targets:

```rust
#[derive(Component, Debug, Clone, Copy)]
pub struct IkTarget {
    pub joint: JointKind,
    pub target: Vec3,
    pub pole: Option<Vec3>,
    pub weight: f32,
}
```

The first implementation can keep targets on the character root as data
components/resources rather than spawning separate target entities. The key is
that IK resolves into joint rotations, not mesh transforms.

#### MM8.2 Foot Placement

- Sample terrain under `LeftAnkle` and `RightAnkle`.
- Adjust ankle targets up/down within a short range so feet sit on slopes.
- Bend knees by rotating hip/knee joints; keep pelvis bob smooth.
- Disable or reduce foot IK during jump, fall, zip, swing, roll, and hard land
  recovery.

#### MM8.3 Hand And Wall Contact

- One-hand wall slide: place the leading wrist against the wall normal and tilt
  chest/spine into the wall.
- Ledge hang/climb: both wrists lock to the ledge while pelvis and knees tuck.
- Grapple wind-up: lead wrist aims at target; off hand counterbalances.
- Swing: one-hand and two-hand hang variants driven by speed/angle.

#### MM8.4 Motion-Responsive Body

- Add body tilt from velocity.
- Add spine twist from swing arc and dodge direction.
- Add head look-at toward hook targets, enemies, discussion NPCs, and boss weak
  points.
- Add landing reactions: soft bend, hard land, roll, slam.

Acceptance:

- Foot placement visibly conforms to mild slopes without jittering.
- Wall slide, ledge hang, grapple, swing, and sabre poses use wrist/arm chains
  rather than whole-mesh offsets.
- IK never pulls the character root through walls or terrain.
- Controller/local-co-op readability remains intact in split screen and boss
  mode.

### MM8.5: External Skeletal Asset Bridge

Goal: prepare the internal skeleton for real rigged glTF characters without
blocking the procedural MVP.

- Define a stable Starfall humanoid bone naming table matching `JointKind`.
- Document required Blender/export conventions: scale, forward axis, rest pose,
  clip names, material slots, sockets, and optional robot-part attachment bones.
- Add an asset-loader seam that can map a glTF skeleton to `JointKind` while
  preserving `CharacterBlueprint` identity, color, stats, and save data.
- Runtime rule: if a glTF rig is missing or invalid, fall back to the procedural
  MM7 skeleton and modular meshes.

Acceptance:

- The game has one canonical bone naming table.
- A future glTF hero can be imported without changing save schema.
- Missing external assets never prevent the procedural character from spawning.

### MM9: Camera And Multiplayer Readability

Goal: make high-speed movement cinematic without losing local co-op clarity.

- Add speed-based FOV, swing pull-back, vertical look-ahead, and mild roll/tilt.
- Add camera collision avoidance against cliffs/castles/city geometry.
- Add split-screen hook target indicators and off-screen route hints.
- Preserve boss-mode single-screen behavior and party catch-up rules.

Acceptance:

- Grapple and flight feel fast without hiding hazards or other players.

### MM10: World, Boss, And Dungeon Integration

Goal: make movement verbs matter across the full game.

- Add hook sockets and zip routes to mountain passes, sky cities, castles,
  factories, power plants, dragon lairs, and airship decks.
- Dragon lairs should teach specific verbs: ice climb, lava swing, wind glide,
  rock zip, crystal maze, and shadow underpass.
- Boss arenas should include climbable armor, hook weak points, flying drone
  boosts, destructible cover, and regroup portals.
- Dungeon top-down mode gets simplified hook verbs: short pull, dash slash,
  switch pull, item tug, and boss weak-point yank.

Acceptance:

- Each major chapter teaches or tests one movement verb and one combat traversal
  verb.

## Integration With Engine M7 (Connected Platformer Level Network)

Engine M7 adds `WorldRoute` — a typed, persistent connection between two
`WorldSite` nodes on the strategic map. Motion mechanics intersect with routes
in several concrete ways:

- **Route traversal spaces**: a Road, MountainPath, or SkyBridge route is not
  just a map edge. It is a real 3-D space players traverse. The route's physical
  props (bridge spans, mountain trail sections, sky-lane platforms) live in the
  world and must be safe for MM3/MM4 hook verbs, MM6 flight, and split-screen.
- **Hook attach points on routes**: authored `WorldRouteMarker` beacon entities
  at route midpoints are valid hook targets for MM2 targeting and MM3/MM4 zip/
  swing. Mark them with a hook-socket component when those milestones ship.
- **Route-gated movement verbs**: some routes require specific verbs. A
  `MountainPath` route may require MM3 zip to reach; a `SkyBridge` route may
  require MM6 flight thrust or MM4 swing clearance. Gate these in
  `world_route_unlock_system` by checking the player's unlocked movement state.
- **Route contest encounters**: when a route is `Contested`, the platformer
  space spawns enemies. MM5 grapple combat verbs (attack pull, enemy yank) should
  work in the route space without collision edge cases.
- **Save/load contract**: `WorldRouteState` persists. Motion milestone work must
  not reset route state on respawn or level reload.

Decision record: Engine M7 owns `WorldRoute` types, save records, route spawning,
UI gating, and the first playable route slice. Motion milestones MM2–MM6 will
extend route traversal with hook targeting, zip, swing, and flight once the
physical route spaces exist.

## Technical Notes

- Keep `PlayerMovement` the home for tunable movement values.
- Keep `GrappleHookState` as the single-hook source of truth.
- Keep `PlayerStateMachine` aligned with `CartoonPose`.
- Use explicit world marker components for hook targets; do not infer gameplay
  from material color or mesh name.
- Prefer pure tests for math/state transitions before heavyweight Bevy app
  tests.
- Manual smoke path: keyboard/mouse, two Xbox-style controllers, player select,
  split-screen spawn, run, sprint, jump, wall slide, wall jump, hang, climb,
  grapple wind-up, jetpack, dodge, parry, slingshot, moving platform,
  boss-mode camera pull, and one save/load cycle.
