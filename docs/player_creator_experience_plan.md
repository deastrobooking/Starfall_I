# Player Experience And Creator Workflow Plan (`PX#` / `PM#`)

This plan records the production priority chosen after the July gameplay and
Forge reviews. It temporarily precedes further EC2 combat-collision expansion:
we must first make aiming truthful, weapon presentation readable, the shop
usable, and creator results repeatable by someone other than the original
author.

## Design contracts

- The crosshair, character pose, muzzle, projectile, and lock-on systems consume
  one per-player aim solution. A screen graphic must never claim an impact point
  that gameplay does not use.
- Weapon timing continues to come from `MoveDef` and weapon data. Presentation
  profiles add poses, recoil, trails, audio, and effects without creating a
  second combat clock.
- Buying and equipping are validated transactions. Campaign-shared currency and
  per-player ownership are displayed explicitly.
- The title screen exposes two first-class paths: `START GAME` and
  `START EDITOR`. Editor entry opens a project hub before loading an authoring
  workspace.
- Authored recipes are the source of truth. Seeds, generator versions,
  dependencies, and content hashes are persisted so another user can regenerate
  the same result.
- Levels reference stable published content IDs; generated ECS entities and
  meshes are never saved as authored state.

## Delivery order

### PX1 — Authoritative aiming and crosshair

Create a per-player `AimSolution` that resolves the owning camera, viewport aim
ray, optional soft-lock target, final world aim point, muzzle direction, and
reticle state. Use it for primary and special weapons. Render one reticle per
player viewport with idle, aiming, target, locked, obstructed, and charging
feedback.

Acceptance:

- Camera pitch affects projectile direction and the reticle agrees with the
  actual firing direction.
- Mouse and controller paths use the same solution.
- Split-screen players have independent aim state.
- Pure tests cover fallback direction, muzzle-to-point correction, soft-lock
  selection, and reticle-state selection.

**First slice delivered:** every player now owns an `AimSolution`; camera pitch,
world-ray obstruction, visible soft targets, and muzzle parallax resolve before
primary/special firing. The HUD spawns and projects one state-colored reticle per
viewport. Arm-cannon charge and all current ranged special tools consume the
same direction. Manual controller/TV acceptance remains.

### PX2 — Weapon animation and presentation profiles

Add data-driven presentation profiles for ready, aim, charge, fire, recoil,
recover, melee wind-up, active, and follow-through. Profiles bind animation
events to muzzle flashes, projectile release, hitbox activation, trails, audio,
and bounded camera feedback. Prove the contract with the Arm Cannon, Homing
Star, and Star Sabre before expanding the weapon roster.

### PX3 — Transactional shop GUI

Replace the pause-menu text browser with controller-first category navigation,
item cards, preview, stat comparison, owned/equipped/locked/affordable states,
confirmation, and buy/equip/unequip actions. Share the card and transaction
contracts with tech upgrades, perk training, and the Robot Garage while keeping
their inventories distinct.

Acceptance requires save/load persistence, insufficient-funds protection,
duplicate-purchase protection, explicit party-versus-player ownership, and a
controller-only purchase/equip flow.

### PM1 — Title launcher and project hub

Change the home page to offer `START GAME`, `START EDITOR`, `SETTINGS`, and
`QUIT`. `START EDITOR` opens a project hub with Continue Last Project, New,
Open, Create From Template, Recover, Import Package, and Project Settings.
Opening a project then exposes Character Studio, Creature Forge, Materials,
World Kit, Level Composer, UI/Dialogue, Validate, Playtest, and Publish.

The hub owns the active project path. Do not hard-code all users to
`starfall_forge/project.json`.

**Shell delivered:** the title page now exposes `START GAME` and `START EDITOR`.
`START EDITOR` opens `AppState::ProjectHub`; its Open Current Project action
loads the existing versioned Starfall workspace through the protected
`EngineToolMode::Editing` boundary. New/Open/Template/Import project selection
and a user-owned active project path remain PM1 work.

### PM2 — Project packages and reproducible presets

Each project is a directory containing a manifest, content source folders,
levels, presets, thumbnails, recovery, validation reports, and published
output. Add a lock document containing generator versions, seeds, dependency
IDs and hashes, source hashes, preset revisions, validation configuration, and
engine/build version.

Presets are immutable versioned content records with stable IDs, complete
parameters, seed, dependencies, thumbnail, tags, and generator fingerprint.
Users may Apply, Compare, Fork, Reset, Regenerate With Same Seed, or Generate a
New-Seed Variant.

### PM3 — Multi-level project support

Replace the current single-scene workspace restriction with separate level
scene documents and an `active_level_id`. A level stores terrain reference,
published kit instances, player spawns, encounters, gameplay graphs,
checkpoints, fast travel, camera volumes, lighting/audio/biome profiles, and
validation budgets.

Initial templates: Empty Test Arena, Open-World Chapter, Dungeon, Airship, Boss
Arena, Racing Course, and Settlement Hub. The complete workflow is New Level →
Compose → Validate → Save → Playtest → Publish → Set Startup/Campaign Level.

## Immediate scope

PX1 and the PM1 launcher shell are delivered. The first PX3 ownership foundation
is also delivered: Chapter Select can target a joined P1-P4 profile, purchases
are stored per slot, runtime combat/stat consumers use that player's progression,
and v3 saves reload it by stable `PlayerIndex`. Next, build the bounded PX3 shop
transaction/card flow on this owner contract. After that, complete the PM1
project browser/path model before the larger PM2/PM3 persistence migration.
Manual one-to-four-player aim, ownership, and editor-launch smoke acceptance
remains required.
