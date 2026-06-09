# Starfall I Motion Mechanics Roadmap

This is the agent-facing plan for turning Starfall I from a charming prototype
movement set into a full humanoid sci-fantasy traversal system. The target feel
is heroic, readable, and physical: mountain-platformer challenge, Spider-Man
freedom, Assassin's Creed climbing language, Halo combat clarity, and old-school
action RPG accessibility for local multiplayer.

## Current Baseline

- Players are Rapier kinematic capsules driven through `PlayerMovement`.
- Current mechanics include analog walking/sprinting, stamina, coyote time, jump
  buffering, early jump release, apex float, stronger fall gravity, wall slide,
  wall jump charges, intentional hang/climb, dodge, parry, jetpack lift, and
  controller trigger-axis fallback.
- Current procedural character visuals use `CartoonPose` with separate idle,
  walk, run, jump, fall, flight, one-hand wall slide, and hang poses.
- Wall sliding is now a stamina-backed wall clasp: pushing into a wall while
  falling refreshes wall-jump charges, lightly drains stamina, slows descent,
  and uses a one-hand slide pose. `E` / D-pad Down still converts a slide into
  an intentional hang/climb attempt.
- The world already has mountain routes, path slabs, moving platforms,
  slingshots, dungeon gates, boss single-screen pull-together, caves, castle
  bridges, and large terrain height changes that can host traversal challenges.

## Motion Pillars

1. Readable Hero Body
   - Every mechanical state needs a silhouette: run lean, hard stop, jump
     stretch, falling brace, flight streamline, wall clasp, hang, mantle, dodge,
     parry, saber swing, and stunned recovery.
   - Procedural poses are acceptable for now, but every state should map cleanly
     to future rigged animation clips.

2. Mountain Parkour
   - The Everest range should support routes that demand wall jumps, ledge
     grabs, cliff slides, mantles, slope runs, bridge launches, rope/beam rails,
     wind gusts, moving platforms, and safe recovery pads.
   - Path markings and glowing guide studs should show optional routes without
     turning the world into a tutorial sign farm.

3. Superhero Flight
   - Flight should start as short jet-assisted traversal, then upgrade through
     Great Scientist temples and robot-pet amplification into hover, air dash,
     glide, boost climb, and squad-friendly regroup tools.
   - Flight must not erase platforming. Use fuel, heat, wind zones, storm
     pressure, anti-flight boss fields, and interior ceilings to keep routes
     meaningful.

4. Combat And Traversal Are One System
   - Beam saber, hand combat, missiles, turret command, robot pets, and flight
     should all have movement hooks: dash strikes, wall-kick attacks, air combos,
     slam landings, parry vaults, grapple pulls, drone boosts, and shield bashes.
   - Dungeon top-down mode can use simpler versions: dash slash, roll, guard,
     knockback, hazards, switch floors, and boss patterns.

5. Multiplayer Fairness
   - Four players need forgiving regroup tools: boss camera pull, soft tether,
     revive windows, shared lifts, split-screen route markers, and ways for a
     fallen player to rejoin without spoiling the lead player's challenge.
   - Any movement upgrade must be tested with keyboard/mouse and at least two
     controllers.

## Milestone Path

### M0: Animation State Expansion

- Keep `CartoonPose` aligned with `PlayerStateMachine`.
- Add or tune procedural poses for idle, walk, run, sprint lean, jump, fall,
  flight, wall slide, hang, mantle, dodge, parry, attack, stun, and death.
- Add animation debug text or overlay showing current state, vertical velocity,
  grounded state, wall contact, wall jump charges, stamina, and jetpack fuel.
- Acceptance: every movement state has a visibly distinct body silhouette.

### M1: Wall, Ledge, And Mantle Suite

- Preserve current wall slide and wall jump as the foundation.
- Add ledge detection separate from wall contact: forward probe, head clearance
  probe, and landing target validation.
- Add mantle/climb-up as a short controlled movement arc rather than a raw
  upward translation.
- Add one-hand wall slide sparks/dust, stamina drain tuning, and a tired slide
  state.
- Add climbable tags for castle walls, dungeon gates, cliffs, towers, giant
  vehicles, and boss armor plates.
- Tests: wall jump charge refresh, hang timeout, stamina exhaustion behavior,
  mantle target validation, and state transition allow-list coverage.

### M2: Grounded Athletic Movement

- Add acceleration curves per gait: walk, jog, sprint, uphill strain, downhill
  speed gain, mud/snow slowdown, and ice slip.
- Add hard stop, pivot, sprint-start, landing recovery, and slope alignment.
- Add short vaults over low obstacles, parkour step-ups over medium ledges, and
  roll recovery from high drops.
- Add terrain-surface tags or procedural material zones for snow, rock, grass,
  bridge, dungeon, metal, ice, water shallows, and alien corruption.
- Tests: movement input strength, stamina drain/regeneration gates, slope
  material modifiers, fall damage thresholds if enabled, and moving-platform
  carry behavior.

### M3: Air And Flight Kit

- Split airborne motion into jump, fall, glide, hover, jet boost, air dash, and
  slam.
- Make Ancient Flight Core unlock hover/boost improvements. Robot pilot pets can
  amplify fuel, control, and cooldown recovery.
- Add wind physics volumes: mountain updrafts, storm downdrafts, side gusts,
  dragon wing shockwaves, city fan lifts, and airship turbulence.
- Add air-route collectibles that teach flight without forcing every player to
  master the hardest route.
- Tests: fuel drain/regen, upgrade reapplication, air dash cooldown, wind volume
  force direction, and no infinite hover without the intended upgrades.

### M4: Combat Traversal

- Add movement-cancel rules: dodge cancels, parry cancel, jump cancel, air saber,
  wall-kick strike, landing slam, shield bash, and robot-pet boost assist.
- Make beam saber and hand combat use momentum: sprint strike reach, air strike
  arc, wall slide kick, heavy landing shock, and knockback into hazards. Current
  Star Sabre baseline already has upgrade-aware combo waves and a distinct
  procedural slash pose; future work should add cancel windows, air sabre
  variants, and hit-stop/VFX timing.
- Add enemies that require motion: shield aliens, cliff turrets, drone swarms,
  wall-crawlers, boss weak points, and airship boarding guards.
- Tests: hit arcs in normal and dungeon mode, knockback clamps, parry windows,
  stamina costs, and boss camera pull safety.

### M5: Environment Physics Playground

- Add authored physics toys across the mountain world: swinging chains, hanging
  banners, breakable bridges, falling rocks, sliding snow shelves, lift fans,
  anti-grav panels, magnet rails, rope bridges, zip beams, spring roots, and
  collapsible dungeon floors.
- Add environmental damage and recovery: lava, alien slime, freezing wind,
  crushing doors, rotating blades, lightning rods, falling rubble, and safe
  rollback pads.
- Add puzzle traversal: weight switches, moving mirrors, robot-pet power locks,
  grapple sockets, energy gates, water wheels, and airship cranes.
- Tests: trigger activation, reset paths, save/load persistence for solved
  traversal puzzles, and no player soft-lock after failure.

### M6: World And Boss Integration

- Use mountain routes for traversal chapters: beginner pass, wall-jump canyon,
  glide ridge, castle climb, dragon lair ascent, storm airship route, and alien
  corruption descent.
- Boss mode should pull the party to one screen and use arenas with verticality:
  climbable boss armor, turret platforms, drone boost pads, destructible cover,
  moving weak points, gusts, lava/snow hazards, and regroup portals.
- Every dragon lair should have a matching traversal identity before its boss:
  ice climb, lava bridge, wind tower, rock slide, crystal maze, and shadow
  underpass.
- Acceptance: each major chapter teaches or tests one movement verb and one
  combat traversal verb.

## Implementation Notes

- Keep `PlayerMovement` the home for tunable movement values. Add fields there
  before scattering constants through systems.
- Keep `EdgeGrabState` for wall/ledge/mantle timers until the behavior is large
  enough to deserve a dedicated `TraversalState`.
- Keep `PlayerStateMachine` as the animation/state source of truth. New states
  need allow-list transitions and a matching `CartoonPose` or future animation
  clip.
- Use `WorldGeometry`, `WalkableSurface`, and explicit marker components for
  traversal props. Avoid inferring game logic from material color or mesh name.
- Prefer module-level pure tests for math and state transitions before adding
  heavyweight Bevy app tests.
- Manual smoke path: keyboard/mouse, two Xbox-style controllers, player select,
  split-screen spawn, run, sprint, jump, wall slide, wall jump, hang, climb,
  jetpack, dodge, parry, slingshot, moving platform, boss-mode camera pull, and
  one save/load cycle.
