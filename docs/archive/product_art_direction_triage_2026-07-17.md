# Product And Art Direction Triage — 2026-07-17

This note reconciles the product/art review with the shipped engine and current
MVP order. Its core direction—Dreamcast-era sci-fantasy readability expressed
through star-tech instruments, ancient materials, strong silhouettes, and
performance-aware effects—is accepted.

## Accepted and started

- `UiTheme` is now the semantic source for the main-menu canvas, PLAY/CREATE
  signals, text hierarchy, HUD panels/resources, and four player accents.
- Main Menu now separates PLAY and CREATE visually without adding a heavy
  background scene or post-processing cost.
- Per-player HUD panels use a strong accent stripe and already report health,
  armor, stamina, flight, climb, element, weapon, traversal, Saber techniques,
  gems, and contextual guidance.
- Shader/material detail, shared handles, spatial LOD, terrain proxies, merged
  skyline clusters, and transparent-material exclusions remain the art rule.
- Movement upgrades stay organized around Ground, Air, Grapple, and Route
  verbs. New work must include pose, sound, camera, VFX, landing, rumble, and HUD
  feedback in proportion to the action.
- Technique unlocks take priority over percentage inflation. The Saber
  blueprints/relic ledger are the first reusable implementation of this rule.

## Accepted next slices

1. Extend `UiTheme` into Pause, Project Hub, Chapter Select, Star Loadout, Shop,
   and Garage as each screen is touched; do not perform an unsafe monolithic UI
   rewrite.
2. Extend the shipped Star Loadout relic catalog as new save-backed world
   objects are authored; its shape, state, effect, input, and source fields are
   now the required metadata contract.
3. Add the colorblind shape layer to chapter, temple, settlement, raid, and
   dungeon map nodes.
4. Add licensed/project-owned display and utility font assets, then route font
   handles through the theme. Default Bevy fonts remain until those assets are
   selected and checked into the project.
5. Define a shared traversal-feedback contract before adding another movement
   verb. Existing hoverboard and Saber modular action IDs are the starting seam.
6. Expose soft build identities (Vanguard, Striker, Ranger, Rift Runner,
   Technomancer) as derived guidance, never permanent classes.

## Already delivered or superseded

- Reusable app construction and startup/plugin smoke coverage already exist.
- The first performance-budgeted technique VFX slice is delivered for Cyclone,
  Comet Dash, Meteor Pound, elemental Saber waves, and Starheart. It reuses
  shared opaque materials/meshes and caps transient Saber decoration at 24
  entities globally; transparent particles and dynamic lights remain gated.
- Projectile/world obstruction, typed collision layers, swept impacts, radial
  cover checks, player-combo broad phase, and Saber candidate resolution are
  already implemented; remaining collision work is active-window/hurtbox depth.
- Main-menu access to START GAME and START EDITOR/Project Hub already exists.
- Controller-owned HUD, save progression, shops, crafting, loadouts, and camera
  feedback already use `PlayerIndex`.

## Performance-gated

- Animated 3D menu backgrounds, denser transparent effects, additional dynamic
  lights, pooled ambient particles, and increased world prop density wait for
  one-/two-/four-player F11/Tracy captures.
- Particle, transparent-object, dynamic-light, VFX-entity, material-instance,
  and UI-update budgets must be measured from the four-player worst case.
- HUD change detection is worthwhile, but follows correctness testing of the
  new progression status lines; it must not miss fixed-tick cooldown changes.

## Deferred design migrations

- Renaming legacy `poison_dps`-style fields to generalized status damage needs a
  serde-compatible schema migration and distinct Fire/Ice/Electric/Dark/Rift
  runtime behaviors. Do not perform it as a cosmetic rename.
- Settlement buildings gaining visible world consequences is accepted, but is
  a separate campaign/world-authority milestone rather than UI polish.
- A tabbed Chapter Select is desirable after its existing panels are decomposed
  into focused systems; preserve controller focus and per-player spending
  ownership throughout that extraction.
