# Starfall I Player Mechanics And Animation MVP Plan

This is the agent-facing plan for upgrading Starfall I's player feel from a
prototype platformer into a heroic sci-fantasy traversal system. The design is
inspired by superhero swing movement, mountain parkour, fantasy flight, action
RPG combat readability, and local co-op fairness.

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
- `GrappleHookState` is the first MVP hook foundation: it stores the single-hook
  mode, cable tuning, wind-up/cooldown timers, zip/mountain/attack pull tuning,
  and future attach-point data.

## MVP Milestones

### M1: Traversal Foundation And Grapple Input

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

### M2: Hook Targeting And Attach Rules

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

### M3: Zip And Mountain Pull

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

### M4: Swing Physics

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

### M5: Grapple Combat And Utility Pull

Goal: make the hook part of Starfall combat, not just traversal.

- Pull lightweight enemies or zip to heavy enemies based on target class.
- Add Star Sabre and hand-combat cancel windows from hook arrival.
- Add hook stun, shield yank, turret pull, item pull, rescue pull, and switch
  pull profiles.
- Boss targets can expose temporary hook weak points during phases.

Acceptance:

- One enemy type and one utility object support hook interaction.
- Hook combat cannot corrupt per-player ownership or shared campaign state.

### M6: Flight Mechanics Upgrade

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

### M7: Animation Foundation

Goal: move from pose-only animation toward an animation-ready humanoid system.

- Keep `PlayerStateMachine` as the source of truth for movement poses.
- Expand `CartoonPose` coverage for sprint lean, grapple, zip, swing, glide,
  hover, air dash, hard landing, roll, mantle, parry, stun, and death.
- Add debug readout for state, pose, vertical velocity, grounded, wall contact,
  hook mode, attach distance, stamina, and jetpack fuel.
- Keep procedural poses mapped to future rigged animation clips.

Acceptance:

- Every movement state has a distinct silhouette even before skeletal clips.

### M8: Procedural Animation And IK

Goal: make the hero body respond naturally to terrain, walls, hooks, and flight.

- Add hand placement for hook wind-up, swing, wall slide, and climb.
- Add foot placement on uneven terrain and slopes.
- Add body tilt from velocity, spine twist from swing arc, and head look-at
  toward hook targets/enemies.
- Add landing reactions: soft, hard, roll, slam.

Acceptance:

- The body reads clearly during swing, zip, wall slide, flight, and landing.

### M9: Camera And Multiplayer Readability

Goal: make high-speed movement cinematic without losing local co-op clarity.

- Add speed-based FOV, swing pull-back, vertical look-ahead, and mild roll/tilt.
- Add camera collision avoidance against cliffs/castles/city geometry.
- Add split-screen hook target indicators and off-screen route hints.
- Preserve boss-mode single-screen behavior and party catch-up rules.

Acceptance:

- Grapple and flight feel fast without hiding hazards or other players.

### M10: World, Boss, And Dungeon Integration

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
