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
`EngineToolMode::Editing` boundary.

**Browser/path model delivered (July 16, 2026):** a persistent
`ForgeProjectRegistry` (platform data dir, `forge_projects.json`, atomic
writes) owns the project list and the active project path — no user is
hard-coded to `starfall_forge/project.json`; that legacy workspace is only
seeded as the first entry when it already exists. The hub lists every
registered project with CONTINUE (active), OPEN (switch), and NEW PROJECT
(creates `projects/starfall-project-N/project.json` through `ProjectStore`, so
new projects carry the versioned format with recovery snapshots, then enters
the editor). Entering `EngineToolMode::Editing` syncs the editor session store
to the active path and rebuilds published-content catalogs when the project
changed. Remaining PM1 work: Create From Template, Import Package, Recover,
Project Settings, and user-typed project names/paths (needs the editor text
input capture wired into the hub).

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

**Lock document delivered (July 16, 2026):** every successful `ProjectStore`
save now also writes `project.lock.json` atomically beside the project file:
lock schema, engine/build version, project id + schema, every record's source
path, draft/published hashes, and dependency ids (sorted by content id), and
every generator payload's fingerprint (category, schema, seed, revision).
Identical project state produces an identical lock document.

**Preset records delivered (July 16, 2026):** presets are immutable versioned
records in the project's `presets/` folder (`<preset_id>.r<revision>.json`,
atomic writes, overwriting an existing revision is refused). Each record
carries the stable id, revision, name, category, generator fingerprint, source
content id, tags, dependencies, thumbnail slot, and the complete payload
parameters including the seed. Editor registry buttons cover CAPTURE / NEXT /
APPLY / COMPARE / FORK / NEW-SEED VARIANT; apply targets the preset's source
content and marks the document dirty, compare reports seed/field/geometry
differences, forks start a new lineage at revision 1, and new-seed variants
append the next revision with a deterministic LCG successor seed. The library
reloads whenever the active project loads or switches. Remaining PM2 work:
preset thumbnails, validation-configuration capture in the lock document, and
Regenerate-With-Same-Seed as an explicit one-press action (today it is
APPLY, which restores the stored seed and parameters).

### Native modifier stack (Blender-inspired, clean-room)

Decision (July 16, 2026): Blender itself is GPL and will not be ported;
instead the three design features gain clean-room, Blender-inspired tools one
bounded slice at a time. First slice delivered: `src/mesh_modifiers.rs` — a
pure, deterministic, non-destructive modifier stack (Mirror, Array, Twist,
Subdivide, Smooth, seed-deterministic Noise Displace) over plain triangle
buffers with Bevy mesh interop, capped at 262k triangles. World Kit consumes
it first: scene objects carry a `modifiers` list in the project schema
(`serde(default)`, saved/loaded/locked with the project), and the object panel
gains MODIFIER KIND / ADD MODIFIER / REMOVE MODIFIER buttons that re-derive
the render mesh from the pristine base primitive on every stack edit.

All three design features now consume the stack (July 16, 2026):

- **World Kit** additionally has parameter editing — MOD PARAM cycles the
  editable parameter of the selected object's last modifier and MOD VALUE −/+
  steps it (axis cycling, clamped numeric steps), remeshing live.
- **Character Studio** specs carry four `Copy`-friendly modifier slots applied
  to every generated body-part mesh; the studio gains a MOD KIND / ADD MOD /
  REMOVE MOD action row with undo support, and the same spec drives the
  playable in-game character mesh, so the equipped look matches the preview.
- **Creature Forge** robot styles carry a modifier stack applied to all 29
  generated robot part meshes (pets, creatures, and `CreatureSpec` recipes
  included); legacy style JSON loads with an empty stack. Interactive Forge
  authoring UI arrives with the Forge screen itself.

Remaining modifier work: studio/forge parameter editing UIs (World Kit has it
first), proportional terrain editing, and constraint/driver concepts for the
animation MVP.

### PM3 — Multi-level project support

Replace the current single-scene workspace restriction with separate level
scene documents and an `active_level_id`. A level stores terrain reference,
published kit instances, player spawns, encounters, gameplay graphs,
checkpoints, fast travel, camera volumes, lighting/audio/biome profiles, and
validation budgets.

Initial templates: Empty Test Arena, Open-World Chapter, Dungeon, Airship, Boss
Arena, Racing Course, and Settlement Hub. The complete workflow is New Level →
Compose → Validate → Save → Playtest → Publish → Set Startup/Campaign Level.

**First slice delivered (July 16, 2026):** `ForgeProject` now carries
`levels: Vec<LevelDocument>` (id, display name, template, scene document)
plus `active_level_id` and `startup_level_id`, with `serde(default)` and
migration so legacy single-scene projects normalize into a "Main World" level
on load. The working `scene` mirrors the active level: load mirrors
level → workspace, save stashes workspace → level (the workspace is the
source of truth on save). All seven templates seed recognizable primitive
arrangements with project-wide collision-free editor ids. The editor toolbar
gains NEXT LEVEL / LEVEL TEMPLATE / NEW LEVEL / SET STARTUP — switching
stashes live edits, respawns the new level's objects (materials and modifier
stacks included), and marks the document dirty; validation rejects empty or
duplicate level ids.

**Playtest flow delivered (July 17, 2026):** the editor toolbar's PLAYTEST
button completes the New Level → Compose → Save → Playtest loop — it syncs
the working scene, switches to the `startup_level_id` level (respawning its
objects), saves atomically, and only then leaves the protected
`EngineToolMode::Editing` boundary into play; a failed save keeps you in the
editor with the error on the status line. Remaining PM3 work: per-level
terrain/spawn/encounter/lighting metadata, per-level validation budgets, and
level rename/delete/reorder.

## Immediate scope

PX1 and the PM1 launcher shell are delivered. The first PX3 ownership foundation
is also delivered: Chapter Select can target a joined P1-P4 profile, purchases
are stored per slot, runtime combat/stat consumers use that player's progression,
and saves (v4 schema) reload it by stable `PlayerIndex`. The bounded PX3 shop
transaction/card flow is now delivered on this owner contract (July 16, 2026):
the pause-menu Star Shop has controller-focusable category tabs and item cards
with EQUIPPED/OWNED/BUY/SAVE UP/GARAGE states, a detail panel with preview and
currently-equipped comparison, a select-then-buy confirmation step, and per-player
buy/equip/unequip running through the pure `shop_transactions` contract
(insufficient-funds, duplicate-purchase, and locked-category protection enforced
below the UI). Outfits/Armor/Weapons are owned per player and persist per
`PlayerIndex` in the save's shop record (`serde(default)`, legacy saves load as
nothing-owned); Vehicles remain party-shared showcase entries built in the Robot
Garage. Tech upgrades, perk training, and the Robot Garage should adopt the same
transaction contract in a later slice. The PM1 project registry/path model, PM2
lock/preset records, and PM3 multi-level/playtest slices are now delivered as
described above. Next, add the app/startup smoke-test seam, then finish the
remaining PM1–PM3 gaps without breaking those persisted contracts. Manual
one-to-four-player aim, ownership, shop purchase/equip, and editor-launch smoke
acceptance remains required.
