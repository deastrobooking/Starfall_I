# Starfall I — Local Co-op Gameplay Review (2026-07)

This review records the code baseline for the current production push: button-first
menus, high-speed racing roads, platformer movement, and star-tech weapons. It is
the decision record for the next work in `docs/agent_next_steps.md`.

## Executive Summary

| Track | Current code baseline | Main gap |
|---|---|---|
| Menu UI | Shared automatic focus now supports spatial keyboard/D-pad/stick navigation, held repeat, mouse synchronization, Confirm, Back, hidden/disabled skipping, and common styles. | Per-controller independent cursors are deferred; current menus use party-shared authority. Hardware/TV-layout validation remains. |
| Speed roads | Authored mountain routes generate interconnected decks, settlement rings/spurs, boosts, banked curves, ramps, loop geometry, NPC traffic, checkpoints, recovery, and guided hoverboard loop adhesion. | Camera roll/look-ahead, visual entry cues, collision-seam smoke tests, and four-player performance need hardware validation. |
| Player movement | Existing traversal now adds explicit momentum roll, stomp bounce, downhill slope acceleration, route recovery, and testable per-player action state. | Animation/audio feedback, enemy-specific bounce reactions, and broad course-by-course tuning remain. |
| Weapons | Homing Star now acquires/reacquires targets with capped steering, ownership, trail, and HUD SEEK/LOCK. Primary beam profiles are larger and the Star Sabre has a held blade visual. | Authored particles/audio, lock reticle graphics, impact variants, and split-screen effect-budget profiling remain. |

## UI Findings

The main UI, character designer, and robot garage use actual `Button` components.
`ui_plugin` automatically registers visible buttons, selects a deterministic
top-left initial focus, navigates by screen-space direction, follows mouse hover,
styles focus/press states, and converts Enter/Space/controller South into the
existing `Interaction::Pressed` action path.

Finish this substrate before converting screens again: add persistent disabled/
locked states, held D-pad repeat, East/Escape Back routing, per-controller focus
ownership where required, text plus color/icon meaning, and TV-safe layouts.

## Speed-road Findings

`world_plugin.rs` owns fourteen authored trunks/cross-links, including six
cross-mountain connectors that create multiple circuits. Continuous full-width
terrain profiles keep the deck above mountain ridges, synchronize elevations at
shared junctions, enforce grade limits, and drive the NPC traffic paths.
Collidable guardrails, boost spans, rounded/banked curves, settlement
connections, helical ramps, expanded hoverboard jumps and loop gates, selected
360-degree loop geometry, checkpoints, and terrain/grade/junction tests are in
place.

Visual loops alone do not provide Sonic-style traversal. The kinematic controller
needs an explicit road-contact frame, surface normal/gravity policy, adhesion rule,
minimum entry speed, exit handoff, camera behavior, and safe recovery. Treat loops
as a traversal feature, not only geometry. Before adding more pieces, extract road
data/generation from the 15k-line world plugin and add a route/checkpoint overlay.

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

The Star Sabre already supports unlock/toggle, chained slashes, waves, dual waves,
piercing/AOE upgrades, and dungeon arcs. Its production presentation still needs a
held blade/core, swing trail, afterimage, hit flash/stop, and effect budgets.

Evolve `Homing Star` into the requested tracking missile: acquire valid hostile
targets in a configurable cone/range; store player ownership and target entity;
steer with a capped turn rate; survive target loss; expose lock state in the
owner's HUD; and use readable trail/splash effects. Add data-driven beam visual
profiles (width, length, core/glow color, trail, muzzle, impact), sized for
split-screen readability rather than simply maximizing screen coverage.

## Recommended Delivery Order

1. Shared UI navigation substrate and controller-complete main flow.
2. Movement characterization tests and ground/air feel tuning.
3. Road-contact/loop traversal prototype on a small test course.
4. Tracking missile vertical slice with ownership, HUD lock, steering, and VFX.
5. Beam/Sabre visual profiles with four-player effect budgets.
6. Expand the interconnected road network after loop and recovery rules work.

Each slice should pass `cargo fmt --check`, `cargo check`, strict clippy, and
`cargo test`, followed by a relevant manual 1–4 player controller smoke pass.
