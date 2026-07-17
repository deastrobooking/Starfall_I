# Starfall I — Local Co-op Gameplay Review (2026-07)

> Historical July review snapshot. Use `current_state.md` and
> `agent_next_steps.md` for current status and execution order.

This review recorded the code baseline for its production push: button-first
menus, high-speed racing roads, platformer movement, and star-tech weapons. Its
findings are historical input to the living plans.

## Executive Summary

| Track | Current code baseline | Main gap |
|---|---|---|
| Menu UI | Shared automatic focus now supports spatial keyboard/D-pad/stick navigation, held repeat, mouse synchronization, Confirm, Back, hidden/disabled skipping, and common styles. | Per-controller independent cursors are deferred; current menus use party-shared authority. Hardware/TV-layout validation remains. |
| Speed roads | Authored mountain routes generate interconnected decks, settlement rings/spurs, boosts, banked curves, ramps, loop geometry, NPC traffic, checkpoints, recovery, and guided hoverboard loop adhesion. | Camera roll/look-ahead, visual entry cues, collision-seam smoke tests, and four-player performance need hardware validation. |
| Player movement | Existing traversal now adds explicit momentum roll, stomp bounce, downhill slope acceleration, route recovery, and testable per-player action state. | Animation/audio feedback, enemy-specific bounce reactions, and broad course-by-course tuning remain. |
| Weapons | Primary/special ammo is unlimited, aiming locks to enemy torsos, swept collision prevents fast-shot tunneling, arm cannons charge, magic users fire tracking beams, Homing Star is persistently selectable from Select+D-pad Up, world-space gold/cyan rings identify tracking locks, charged shots use shared critical resolution and differentiated impact cores, and the Star Sabre has a reliable LB+North toggle plus fallbacks and procedural animation. | Authored audio, broader element impact variants, frame data/cancel windows, and split-screen effect-budget profiling remain. |

## UI Findings

The main UI, character designer, and robot garage use actual `Button` components.
`ui_plugin` automatically registers visible buttons, selects a deterministic
top-left initial focus, navigates by screen-space direction, follows mouse hover,
styles focus/press states, and converts Enter/Space/controller South into the
existing `Interaction::Pressed` action path.

The substrate now also has persistent disabled styling, throttled held repeat,
East/Escape Back routing, and focus-follow scrolling. Keep party menus on one
shared cursor unless a screen has a verified need for owner-scoped interaction;
the remaining gate is recorded four-pad controller-only and TV-safe layout
acceptance across boot, character, chapter, settings, garage, pause, and return.

## Speed-road Findings

`world_plugin.rs` owns fourteen authored trunks/cross-links, including six
cross-mountain connectors that create multiple circuits. Continuous full-width
terrain profiles query the actual triangle-mesh surface, densely sample
thirteen lanes across the complete road width, apply uncapped clearance plus a
safety envelope, carve and longitudinally smooth a blended mountain shelf,
keep the deck above mountain ridges, synchronize elevations at shared
junctions, permit short terrain-following speed climbs,
limits, add ground-level entrance ramps at every route end, spawn paired
supports only beneath genuinely aerial spans, add recurring supported side
ramps with guardrail openings throughout sustained sky-road runs, and drive NPC
traffic paths.
Collidable guardrails, boost spans, rounded/banked curves, settlement
connections, helical ramps, expanded hoverboard jumps and loop gates, selected
360-degree loop geometry, checkpoints, and terrain/grade/junction tests are in
place.

Loops now have guided adhesion, a minimum entry speed, entry/exit handoff,
checkpoints, and recovery. Production work is course-by-course tuning of the
contact frame, camera, seams, ramp direction, barriers, landing clearance, and
four-player performance. Extract road data/generation only along a boundary
proven by those validators; do not mechanically split the large world plugin.

## Movement Findings

Unify the existing verbs around three readable layers:

1. Ground feel: acceleration, braking, turn radius, slope acceleration, momentum
   preservation, skid feedback, and crouch/roll/spin.
2. Air feel: variable-height jump, early-release gravity, apex shaping, air
   control, stomp, bounce, enemy spring, and landing recovery.
3. Route feel: boost carry, bank/loop adhesion, trick jumps, camera look-ahead,
   checkpoint recovery, and party tether rules.

All tuning belongs in explicit movement/traversal components or data tables.
Four-player behavior must stay keyed by `PlayerIndex`.

## Weapon Findings

The Star Sabre supports keyboard and controller unlock/toggle, chained animated
slashes, a persistent held blade, waves, dual waves, piercing/AOE upgrades, and
dungeon arcs. Bounded global hitstop and the first combat-reaction layer have now
landed. Production presentation still needs authored audio, afterimages,
move-specific frame data/cancel rules, and split-screen effect budgets.

`Homing Star` and high-magic primary beams now acquire valid hostile targets in a
configurable cone/range, store player ownership and target entity, steer with a
capped turn rate, survive target loss, and use readable trails. Primary aiming
locks toward living enemy torsos and projectile collision sweeps between frames.
Typed World/Player/Enemy/projectile layers now make those sweeps
world-authoritative: nearest impacts win, piercing stops at blocking geometry,
and enemy fire obeys the same obstruction rule. Radial player/enemy/boss
damage also checks World-layer cover before applying distance falloff.
The next presentation pass should consolidate existing beam, trail, lock-ring,
blade, muzzle, and impact choices into data-driven profiles sized against measured
split-screen budgets. Projectile collision now needs representative thin-wall,
terrain, piercing, and four-player load validation; explosive cover has a live
Avian wall/open-path/target-exclusion regression fixture.
Player combo and Star Sabre candidate selection also uses the typed
PlayerHitbox→EnemyHurtbox Avian path and rejects targets through World cover;
player combos remain query-active for their full authored window with per-move
target deduplication. Star Sabre active-window lifetime and enemy hitboxes remain
next.

## Recommended Delivery Order

1. Record controller-only main-flow and four-player ownership acceptance.
2. Validate EC1 fixed movement at 30/60/120/144 Hz and tune ground/air/route feel.
3. Extend EC2's landed body/projectile layer foundation with move-scoped melee
   hitbox/pushbox/grapple/interaction roles, cancel windows, and reactions.
4. Add road/ramp/landing/barrier validators and projectile collision fixtures.
5. Profile data-driven beam/Sabre/lock/impact effects across four viewports.
6. Expand roads and combat content through the validated Forge/tool boundaries.

Each slice should pass `cargo fmt --check`, `cargo check`, strict clippy, and
`cargo test`, followed by a relevant manual 1–4 player controller smoke pass.
