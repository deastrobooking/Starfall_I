# Starfall I Player Mechanics, Skeleton, Animation, And Customization Plan

Implementation status and the current Sonic/Mario-inspired feel priorities are
summarized in `docs/game_review_2026-07.md`; this remains the detailed MM# roadmap.

> **Naming convention:** Motion mechanics milestones use the `MM#` prefix to
> distinguish them from engine/campaign milestones (`M#` in
> `docs/engine_upgrade_milestones.md`), future enemy-AI milestones (`AI#`), and
> future character customization milestones (`CC#`).

This is the agent-facing plan for upgrading Starfall I's player feel from a
prototype platformer into a heroic sci-fantasy traversal system backed by a
full skeletal, swappable-mesh character pipeline. The design is inspired by
superhero swing movement, mountain parkour, fantasy flight, action RPG combat
readability, expressive character customization, and local co-op fairness.

Label convention: this document's milestones should be referenced as
`Motion MM#`. Bare `M#` labels belong to `docs/engine_upgrade_milestones.md`.
Future enemy behavior planning should use `Enemy AI AI#` labels. Future
character creator, clothing, armor, and prefab-modulation planning should use
`Character Customization CC#` labels. Movement, engine, enemy AI, and character
customization work should cross-reference each other without reusing bare
numbers.

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

## Skeleton-First AAA Movement And Art Contract

Starfall's long-term character system is a full skeletal modular character
pipeline, not a collection of independent pose-swapped meshes. Every player
mechanic after MM7 should assume that a hero has:

- A gameplay root: the Avian-backed kinematic capsule and `PlayerMovement` remain the
  authoritative world position, velocity, collision body, and save/load anchor.
- A visual skeleton: `SkeletonRig` and `JointKind` own visual pose, FK, IK,
  motion response, sockets, and attachment transforms.
- Swappable visual layers: body mesh, head mesh, hair, clothing, armor, boots,
  gloves, capes, robot parts, weapons, tools, and future costume pieces bind to
  named joints or sockets instead of hardcoded root offsets.
- A data-driven identity layer: `CharacterBlueprint` remains the durable record
  for proportions, palette, rig recipe, cosmetic selections, equipment visuals,
  and future prefab choices.
- A preview/runtime parity rule: main-menu character previews, character designer
  previews, live players, and save-loaded players must instantiate the same rig
  and attachment path.

Movement work must never make mesh attachments authoritative. Feet, hands,
weapons, capes, armor, and clothing may visually react to terrain, walls, hooks,
wind, and combat, but they do not pull the player root through collision or
change campaign state directly.

Target feel:

- Locomotion should read like a modern third-person action game: responsive
  input, clear acceleration, believable body lean, foot locking, hand contact,
  anticipation, recovery, and readable cancel windows.
- Traversal should chain cleanly: run, sprint, jump, wall slide, wall jump,
  ledge hang, climb, grapple wind-up, zip, swing, glide, hover, jet boost, air
  dash, dodge, parry, sabre slash, and landing reactions should all share one
  animation-state bridge.
- Customization should preserve silhouette and gameplay readability. Armor and
  clothing can be wild, heroic, and modular, but split-screen players must still
  identify their own character, movement state, team role, and hazards quickly.

Non-goals for MM7-MM11:

- Do not build the full clothing/armor creator inside the movement plugin.
- Do not encode gameplay unlocks in mesh names, material colors, or prefab file
  names.
- Do not introduce root motion as the gameplay authority until there is a
  deliberate root-motion integration plan that keeps local co-op, collision,
  save/load, and boss catch-up stable.

## Current Baseline

- Players are Avian-backed kinematic capsules driven through `PlayerMovement`
  and the local `KinematicCharacterController` compatibility component.
- Current mechanics include analog walking/sprinting, stamina, coyote time, jump
  buffering, early jump release, apex float, stronger fall gravity, wall slide,
  wall jump charges, intentional hang/climb, dodge, parry, jetpack lift,
  traversal quick modes, glide/hover/boost/air-dash/slam flight shaping,
  hoverboard ground stance, slingshot pads, moving platforms, rotating
  elevators, and boss-mode party pull.
- Current procedural character visuals use `CartoonPose` with separate idle,
  walk, run, jump, fall, flight, one-hand wall slide, attack, Star Sabre slash,
  hang, and grapple wind-up poses.
- Runtime characters now spawn a lightweight 17-joint `SkeletonRig` under the
  character root. Individual `CartoonPart` entities still keep compatibility
  base transforms, but humanoid and robot parts are rebound to `JointKind`
  parents by `bind_parts_to_skeleton` instead of remaining permanently attached
  to the root.
- Character roots now carry `CharacterIkPose` data, and the runtime animation
  path resolves first-pass root-local foot, wrist, and look targets into the
  skeleton as visual-only pose layers.
- `CharacterBlueprint` already stores rig, socket, IK, and animation recipe
  data. The live renderer uses a fixed procedural humanoid joint table for now;
  mapping saved blueprint rig recipes into runtime joints remains future work.
- Chassis/body-part swaps already point toward modular customization, but the
  full main-menu clothing/armor/prefab creator should be planned as a separate
  `CC#` roadmap once the skeleton and attachment contracts are stable.
- `GrappleHookState` is the first MVP hook foundation: it stores the single-hook
  mode, target classification, cable tuning, wind-up/search/recovery/cooldown
  timers, heat, zip/mountain/attack pull tuning, and attach-point data.
- D-pad quick modes select traversal intent: Grapple, Hover Jet, Flight, and
  Hoverboard. Select+D-pad keeps legacy utility shortcuts such as interact,
  vehicle, previous weapon, and map.

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

Implementation status as of 2026-06-09:

- `GrappleSocket` and `GrappleTargetKind` are implemented as the common hook
  target contract.
- Hook search now runs after wind-up and scores authored sockets, world-route
  beacons, enemies, boss weak points, drones, and a conservative broad-surface
  fallback in player aim direction.
- Route beacons spawned by `WorldRouteMarker` are tagged as hook sockets; locked
  and blocked routes are excluded from targeting.
- Failed search returns cleanly into recovery and emits a short player message.

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

Implementation status as of 2026-06-09:

- Zip, route pull, mountain pull, and enemy pull now drive the same kinematic
  `PlayerMovement` velocity path as normal movement.
- Zip speed, mountain speed, attack-pull speed, arrival radius, cable limits,
  cooldown, and hook heat live on `GrappleHookState`.
- Jump or dodge cancels a zip and carries momentum back into normal movement.

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

Implementation status as of 2026-06-09:

- Swing targets use kinematic cable math: cable length, radial correction,
  damping, tangent velocity preservation, steering pump, and release carry.
- Long upward route/broad-surface targets select Swinging instead of Zipping.
- The skeleton already reads active Swing/Zip/Grapple state for wrist and
  look-at IK. A dedicated rendered cable/trail VFX pass remains future work.

- Use kinematic-friendly swing math first: cable direction, desired cable
  length, radial correction, tangential momentum preservation, and damping.
- Only introduce physics joints if the kinematic solution proves insufficient.
- Add pump/steer inputs, release impulse, swing length tuning, and coyote-style
  release forgiveness.
- Add visual line, attach spark, and release trail.

Acceptance:

- Player can attach to a bridge/tower and swing across a gap.
- Release preserves useful momentum without becoming uncontrollable.
- Swing state interacts safely with boss-mode party pull and split screen.

### MM5: Grapple Combat And Utility Pull

Goal: make the hook part of Starfall combat, not just traversal.

Implementation status as of 2026-06-09:

- Enemies and boss-tier enemies are valid hook targets through the shared target
  search.
- Light enemy hook arrivals yank the target toward the player and apply kinetic
  damage. Heavy/boss targets resolve as weak-point zip impacts without yanking
  the boss body.
- Route and utility sockets resolve through the same targeting and recovery
  path. Bespoke switch/gate profiles remain future work.

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

Implementation status as of 2026-06-09:

- `TraversalModeState` selects Hover Jet, Flight, Hoverboard, or Grapple.
- `JetpackState` now tracks `FlightMode`: Jump, Fall, Glide, Hover, JetBoost,
  AirDash, Slam, Hoverboard, and Grounded.
- Hover Jet mode keeps the old stable vertical lift feel. Flight mode adds
  glide, sprint boost, dodge air-dash, and down-input slam. Hoverboard mode
  boosts ground speed and air control while selecting a hoverboard animation
  stance.
- Ancient Flight Core upgrades now improve glide, boost, air dash, and cooldown
  tuning in addition to fuel/lift.

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

Implementation status as of 2026-06-09:

- `JointKind`, `JointMarker`, and `SkeletonRig` are implemented in
  `src/components/character.rs`.
- `spawn_skeleton_rig` in `src/characters.rs` creates the 17-joint hierarchy
  from `BodyRecipe`, including rest/local translations for each joint.
- Humanoid and robot swap parts now carry default joint bindings; newly spawned
  parts are converted from root-local to joint-local space and reparented by
  `bind_parts_to_skeleton`.
- `cartoon_animation_system` now includes a first-pass FK joint animation layer
  for idle, walk, run, sprint, jump, fall, flight, wall slide, grapple, swing,
  zip, hang, roll, parry, attack, sabre, stun, death, hard-land, and mantle
  poses.
- Existing per-part pose animation remains as a compatibility layer until MM8
  replaces the remaining loose-piece offsets with IK and deeper joint-chain
  animation.

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
  movement and collision remain owned by `PlayerMovement` and Avian-backed
  controller physics.

### MM8: Procedural Animation, IK, And AAA Motion Response

Goal: make the skeleton respond naturally to terrain, walls, hooks, flight,
combat, and player velocity while preserving the fast kinematic controller.

MM8 assumes MM7's `SkeletonRig` exists. Do not build IK directly against loose
mesh parts. This milestone is the bridge between simple FK pose animation and a
production animation graph.

MM8 implementation rule: pose selection still flows from gameplay state
(`PlayerStateMachine`, `PlayerMovement`, `GrappleHookState`, weapon/combat
state), but final visual output should be layered:

1. Base locomotion pose: idle, walk, run, sprint, jump, fall, flight, hover,
   wall slide, hang, grapple, swing, zip, dodge, parry, attack, sabre.
2. Additive motion response: velocity lean, turn anticipation, spine twist,
   landing compression, cape/cloth response, recoil, hook pull, wind/updraft
   response.
3. Contact IK: feet on walkable surfaces, hands on walls/ledges/hooks, weapon
   aim/sabre target correction.
4. Cosmetic constraints: armor, clothing, capes, and robot pieces follow their
   sockets without changing the gameplay root.

Implementation status as of 2026-06-09:

- `IkTarget` and `CharacterIkPose` are implemented in
  `src/components/character.rs`; every character root gets a pose cache when the
  rig is attached.
- `cartoon_animation_system` now builds root-local foot, hand, hook look, wall
  contact, hang, and dodge look targets from `PlayerMovement`, `EdgeGrabState`,
  `GrappleHookState`, and `DodgeState`.
- A clamped additive motion-response layer gives pelvis, spine, chest, neck,
  head, and shoulders velocity lean, hook/wall orientation, dodge bias, and
  hang shaping after the base FK pose.
- A first analytical two-bone leg IK helper blends hip/knee/ankle rotation and
  ankle translation toward foot targets. Wrist targets for wall slide, hang,
  swing, zip, and grapple are also blended in root-local skeleton space.
- The first MM8 slice is visual-only and does not move the physics player root.
  True Avian foot raycasts, authored ledge/socket wrist targets, foot locking,
  and slope-normal ankle roll remain the next MM8 implementation bridge.

#### MM8.1 IK Target Components

Add lightweight runtime targets and pose caches:

```rust
#[derive(Component, Debug, Clone, Copy)]
pub struct IkTarget {
    pub joint: JointKind,
    pub target: Vec3,
    pub pole: Option<Vec3>,
    pub weight: f32,
}

#[derive(Component, Debug, Clone, Default)]
pub struct CharacterIkPose {
    pub left_foot: Option<IkTarget>,
    pub right_foot: Option<IkTarget>,
    pub left_hand: Option<IkTarget>,
    pub right_hand: Option<IkTarget>,
    pub look_at: Option<Vec3>,
}
```

The first implementation can keep targets on the character root as data
components/resources rather than spawning separate target entities. The key is
that IK resolves into joint rotations, not mesh transforms.

#### MM8.2 Foot Placement

- Sample walkable ground under `LeftAnkle` and `RightAnkle`.
- Prefer Avian raycasts against `WalkableSurface`/world geometry when available;
  a procedural terrain-height fallback is acceptable for early visual-only IK.
- Adjust ankle targets up/down within a short range so feet sit on slopes.
- Use an analytical two-bone leg solve for hip/knee/ankle rotation. Clamp
  target distance, knee bend, pelvis compensation, and ankle roll so extreme
  terrain cannot fold the character inside out.
- Add foot locking during planted walk/run frames, then blend out on step
  lift-off. Foot IK should never jitter at split-screen camera distances.
- Disable or reduce foot IK during jump, fall, zip, swing, roll, hard land
  recovery, flight, hover, air dash, vehicle mode, and any scripted boss pull.

#### MM8.3 Hand And Wall Contact

- One-hand wall slide: place the leading wrist against the wall normal and tilt
  chest/spine into the wall.
- Ledge hang/climb: both wrists lock to the ledge while pelvis and knees tuck.
- Grapple wind-up: lead wrist aims at target; off hand counterbalances.
- Swing: one-hand and two-hand hang variants driven by speed/angle.
- Sabre and melee: weapon hand follows attack arcs while the off hand and chest
  counter-rotate for readable strikes.
- Parry: wrists and forearms brace toward the incoming direction or camera aim
  when exact attack direction is unavailable.

#### MM8.4 Motion-Responsive Body

- Add body tilt from velocity.
- Add spine twist from swing arc and dodge direction.
- Add head look-at toward hook targets, enemies, discussion NPCs, and boss weak
  points.
- Add landing reactions: soft bend, hard land, roll, slam.
- Add turn anticipation: shoulders/head start rotating slightly before hips when
  the player sharply changes direction.
- Add vertical-state shaping: jump extension, apex float pose, fall bracing,
  glide spread, hover stabilization, air-dash streamlining, and slam compression.
- Add local-co-op readability clamps so extreme IK, cape/armor motion, or swing
  poses do not hide the character silhouette.

#### MM8.5 Animation Graph And Blend Contract

Goal: define the runtime graph that will later host authored clips, procedural
layers, and imported glTF animations.

Implementation status as of 2026-07-16:

- `CharacterAnimationMvpPlugin` creates one shared Bevy `AnimationGraph` and an
  independent `AnimationPlayer`/`AnimationTransitions` pair for every player
  skeleton, so local players never share playback state.
- The first canonical clip set covers idle, walk, run, sprint, jump, fall, wall
  slide/climb, and hang. Movement speed scales walk/run/sprint playback and pose
  changes crossfade over bounded 80-140 ms windows.
- Graph clips own only spine, chest, shoulder, and hip base rotations. The
  existing procedural layer remains authoritative for knees, ankles, wrists,
  weapon sockets, and contact IK; gameplay root motion remains disabled.
- Run and sprint arms swing fore/aft on the local X axis, with tests guarding
  against the previous sideways-arm motion.
- Poses without an MVP graph clip (including attack, sabre, grapple, and dodge)
  stop graph playback and use the procedural fallback rather than retaining a
  stale locomotion clip.
- Next validation gate: tune these clips during four-controller play, verify
  wall/hang contacts and weapon sockets visually, then add dedicated grapple,
  sabre, ranged-recoil, dodge, land, damage, and death clips.

- Keep `CartoonPose` as the compact state bridge for now, but begin separating
  animation output into base clip/pose, additive overlays, and IK/contact solve.
- Define blend weights for ground locomotion, airborne, wall, ledge, grapple,
  flight, combat, damage, and death layers.
- Add motion tags such as `Grounded`, `Airborne`, `WallContact`, `HookContact`,
  `CombatUpperBody`, `WeaponLocked`, `FootIkAllowed`, and `HandIkAllowed`.
- Make cancel windows explicit for hook arrival, sabre slash, melee strike,
  dodge recovery, parry brace, hard land, and flight recovery.
- Treat root motion as visual-only until a separate root-motion authority plan
  is accepted.

Acceptance:

- Foot placement visibly conforms to mild slopes without jittering.
- Wall slide, ledge hang, grapple, swing, and sabre poses use wrist/arm chains
  rather than whole-mesh offsets.
- IK never pulls the character root through walls or terrain.
- Controller/local-co-op readability remains intact in split screen and boss
  mode.
- Existing robot and armor part swaps still attach through `JointKind` sockets
  and do not break IK.
- Character designer previews show the same skeleton, IK settings, and socket
  bindings as runtime players, with non-gameplay preview controls allowed.

### MM9: External Skeletal Asset Bridge

Goal: prepare the internal skeleton for real rigged glTF characters without
blocking the procedural MVP.

- Define a stable Starfall humanoid bone naming table matching `JointKind`.
- Document required Blender/export conventions: scale, forward axis, rest pose,
  clip names, material slots, sockets, and optional robot-part attachment bones.
- Add an asset-loader seam that can map a glTF skeleton to `JointKind` while
  preserving `CharacterBlueprint` identity, color, stats, and save data.
- Runtime rule: if a glTF rig is missing or invalid, fall back to the procedural
  MM7 skeleton and modular meshes.
- Support socketed procedural pieces: current capsule/anime/robot meshes parented
  to joints or sockets.
- Support skinned mesh prefabs: authored glTF meshes weighted to the Starfall
  humanoid armature.
- Keep clothing and armor compatible with both modes. Early clothing may be
  socketed rigid pieces; production clothing should support skinned meshes or
  segmented cloth panels where needed.

Acceptance:

- The game has one canonical bone naming table.
- A future glTF hero can be imported without changing save schema.
- Missing external assets never prevent the procedural character from spawning.
- The importer can reject invalid rigs with clear diagnostics instead of
  silently producing broken characters.

**2026-07 first slice:** `character_studio/rig_bridge.rs` defines the canonical
17-joint naming table, AMP aliases, 15 required morph names, duplicate/missing
joint validation, and contract tests. Character Studio exposes GENERATED and
RIG TEST backends; RIG TEST loads the skinned AMP scene and falls back to the
procedural character on load failure. Asset inspection confirms AMP maps all
17 gameplay joints but currently has no morph targets or animation clips, so
it remains diagnostic until a Blender-authored production base is exported.

### MM10: Camera And Multiplayer Readability

Goal: make high-speed movement cinematic without losing local co-op clarity.

Implementation status as of 2026-06-09:

- Player cameras now add speed-based pullback, flight lift, hook pullback,
  hoverboard pullback, and dynamic FOV response in split-screen mode.
- Boss and dungeon shared-camera modes remain authoritative when active.

- Add speed-based FOV, swing pull-back, vertical look-ahead, and mild roll/tilt.
- Add camera collision avoidance against cliffs/castles/city geometry.
- Add split-screen hook target indicators and off-screen route hints.
- Preserve boss-mode single-screen behavior and party catch-up rules.

Acceptance:

- Grapple and flight feel fast without hiding hazards or other players.

### MM11: World, Boss, And Dungeon Integration

Goal: make movement verbs matter across the full game.

Implementation status as of 2026-06-09:

- Existing `WorldRouteMarker` beacon props are now hook sockets, giving open
  mountain paths, sky bridges, space lanes, route sockets, and dungeon gates
  traversal intent without a parallel level-authoring path.
- Enemy, drone, and boss markers participate in the same hook targeting layer.
- Full route-specific encounter scripting, dragon-lair verb lessons, and
  dungeon top-down hook simplification remain future content passes.

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

## Future Character Customization Track (`CC#`)

The main-menu character creator, clothing system, armor visuals, and prefab
modulation pipeline should be planned separately from motion mechanics once the
MM7-MM9 skeleton contract is stable. The customization track should use `CC#`
labels so agents can discuss it without overloading movement milestones.

Customization vision:

- Players can customize heroes from the main menu before entering the campaign.
- The editor supports body proportions, colors, hair, headgear, clothing,
  armor, gloves, boots, capes, robot/mech parts, weapons/tool visuals, and
  eventually saved outfit presets.
- The game can make new character items from authored prefab recipes rather than
  hardcoded Rust-only mesh layouts.
- Clothing and armor can be socketed rigid pieces at first, then graduate to
  skinned glTF meshes, segmented cloth panels, or physics-assisted cloth when
  the asset pipeline supports it.

### CC0: Customization Architecture Plan

Goal: create a separate durable roadmap for character customization.

- Define `CharacterCustomization` / `CharacterOutfit` save data separate from
  movement state.
- Define visual slots: BaseBody, Head, Hair, Face, Hood, Hat, TorsoClothing,
  LegsClothing, Gloves, Boots, ShoulderArmor, ChestArmor, Belt, Cape, Back,
  Tail, Horns, Visor, WeaponSkin, ToolSkin, RobotChassis, RobotArms, RobotLegs,
  RobotShoulders.
- Define slot compatibility rules: humanoid, robot, alien, dragon, rival, boss,
  child/teen/adult scale variants, and future non-humanoid rigs.
- Define unlock sources: story reward, rescued robot pet, crafted item, city
  economy, boss drop, relic, debug/dev grant, and default starter wardrobe.
- Define preview requirements: same skeleton path as runtime, local lighting,
  pose cycling, split-screen color readability preview, and clear save/cancel
  behavior.

### CC1: Prefab Modulation Data Model

Goal: let new clothes/armor/parts be authored as data.

- Add prefab records with stable ids, display names, slot, allowed rigs,
  required `JointKind`/socket bindings, material palette channels, default
  scale, optional stat tags, and unlock metadata.
- Use data files for non-engine assets where practical. Rust may still spawn
  procedural fallback pieces, but authored content should not require a full
  gameplay-code change.
- Preserve save compatibility: saves store prefab ids and palette overrides, not
  transient entity ids.
- Missing prefab rule: if an item id is missing, fall back to a safe default and
  show a diagnostic in dev builds.

### CC2: Main Menu Character Creator

Goal: move from simple toggles toward a usable in-game customization surface.

- Build a main-menu/editor flow that edits the selected player slot.
- Let players browse slots, cycle items, adjust colors/material channels, and
  save named outfit presets.
- Preview movement-critical poses: idle, run, sprint, jump, wall slide, hang,
  grapple, swing, flight, parry, sabre slash, and hard land.
- Validate silhouettes: capes, shoulder armor, helmets, and robot parts should
  not hide hands, feet, hook poses, or weapon arcs.
- Keep gameplay stats and visuals separable unless an equipment system
  explicitly links them.

### CC3: Clothing, Armor, And Equipment Runtime

Goal: make custom outfits survive play, save/load, robot swaps, and future
asset imports.

- Spawn customization pieces through the same joint/socket binding path as MM7.
- Support layered draw order and conflict rules, such as hood versus large hair,
  chest armor over shirt, cape under back gear, and boots over leg clothing.
- Add material-channel overrides for team/readability colors.
- Keep collision conservative: visual armor does not resize the player collider
  unless a separate gameplay equipment rule changes the body type.
- Ensure local co-op: each active player keeps independent outfit data,
  cosmetics, palette, and preview state.

Boundary with motion milestones:

- MM7-MM9 define skeletons, sockets, IK, animation graph rules, and asset import
  compatibility.
- CC# defines authoring, UI, save data, cosmetic slots, unlocks, and prefab
  modulation.
- Shared contract: all CC# assets must bind to `JointKind` or named sockets,
  and all MM# animation systems must tolerate missing/disabled cosmetic pieces.

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
- Bevy hierarchy research note: child `Transform` values are local to their
  parent/`ChildOf` relationship, while `GlobalTransform` is propagated by Bevy.
  MM7 therefore mutates joint local `Transform`s and never treats joint
  `GlobalTransform`s as gameplay authority.
- Use explicit world marker components for hook targets; do not infer gameplay
  from material color or mesh name.
- Prefer pure tests for math/state transitions before heavyweight Bevy app
  tests.
- Manual smoke path: keyboard/mouse, two Xbox-style controllers, player select,
  split-screen spawn, run, sprint, jump, wall slide, wall jump, hang, climb,
  grapple wind-up, jetpack, dodge, parry, slingshot, moving platform,
  boss-mode camera pull, and one save/load cycle.
