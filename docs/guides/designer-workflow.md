# Designer Workflow — Authoring Content That Ships

How to take content from an idea to a consumer build, using the Designer
edition's tools. For what the editions *are* and where the boundary sits, read
[PROJECT_PLAN.md](../PROJECT_PLAN.md) first.

## The loop at a glance

```
Project Hub → open/create a project
   ↓
Author (Creature / Weapon / Vehicle / Spaceship Forge / live editor)
   ↓ NAME + SAVE          — versioned records, atomic writes, snapshots
TEST (where supported)     — Creature/Weapon tools can jump into play
   ↓ BACK, tweak, validate again
PUBLISH TO GAME            — validate everything, bake assets/published/
   ↓
cargo build --no-default-features   — the consumer Game edition, content included
```

## Projects

Everything you author lives in a **ForgeProject**: a versioned store with
atomic writes, recovery snapshots, and draft/published hashes. Open or create
one from the main menu's **CREATOR TOOLS** (the Project Hub). Tools refuse to
save without an active project, and tell you so.

## Tool windows

Authoring workspaces use one shared DCC-style window shell: drag the title bar
to move, drag the **◢ corner grip** to resize, double-click the title to
collapse, and use the **—** button to minimize. The mouse wheel scrolls the
topmost panel under the pointer. Grabbing a panel or one of its controls raises
it above its siblings.

Windows use logical coordinates at every UI scale and fit to their actual
workspace, so a wrapped toolbar or resized application cannot strand their
chrome off-screen. A minimized window keeps only its title footprint and hides
its resize grip, leaving the restore button unobstructed. Pointer input over a
window belongs to the UI: it cannot also select, sculpt, orbit, or zoom the 3D
preview behind it.

Dense tools open with the primary task panels expanded and secondary panels
collapsed. This is intentional progressive disclosure, especially for 720p
and 140% UI scale; restore any secondary panel when needed. Project Hub and
Robot Garage remain full-screen workflow surfaces rather than fake floating
windows, but both scroll and reflow for narrow or short viewports.

## Weapon Forge

Reachable from the hub, or directly with `STARFALL_WEAPON_FORGE=1` while
iterating on the tool itself.

- **PARTS** — cycle grip / guard / emitter / pommel / colour; step blade
  length and width. **Stats are derived from the build** — you are designing a
  physical object, not filling in a spreadsheet. The preview is assembled from
  the same measurements the derivation reads, so it cannot lie.
- **PERFORMANCE** — the derived numbers as percentages of the issued blade,
  plus chain delta, balance point, length, trait, and estimated shop value.
  Blocking problems are listed here *before* you try to save.
- **NAME** — toggles typing mode (type, Backspace, Enter to finish). The name
  matters: the content id — the identity saves, the shop, and publishing all
  reference — derives from it. Rename-then-save creates a new design.
- **TEST** — registers the current design under a test id, jumps into play
  with every player holding it, and consumes itself after one session. Tweak,
  TEST again; the registry upserts, so iteration always takes effect.
- **LIBRARY** — the active project's saved designs; LOAD or DEL per row.
  Deletion goes through the same atomic store as saves.

No design can be a strict upgrade over the issued blade — an all-upside build
pays for it in swing speed. This is enforced by a build-failing test that
sweeps every part combination, so don't fight it; trade something.

## Creature Forge

Same window vocabulary against the robot geometry factory. Creatures publish
into the same catalog dungeon spawners resolve `CreatureSpawnOverride`
records through — a published creature id in a spawner override fields your
creature in a consumer build.

## Vehicle Forge

Reachable from the Project Hub, or directly with
`STARFALL_VEHICLE_FORGE=1` while iterating on the tool. Start from Boat, Car,
Truck, or Motorcycle presets, then author the drive/body choices and bounded
dimensions, wheels, clearance, power, mass, cargo, cabin, aerodynamics,
suspension, armor, and seats. Derived performance and validation stay beside
the live procedural preview. SAVE writes the active project through the same
revision-checked record contract as the other Forges; the library can load or
delete saved recipes. This first slice authors and publishes vehicle data; it
does not claim that every recipe already drives a gameplay physics body.

## Spaceship Forge

Reachable from the Project Hub, or directly with
`STARFALL_SPACESHIP_FORGE=1` during tool development. It follows the same
project-backed library, validation, shared-window, controller, and live-preview
contract as Vehicle Forge while keeping spacecraft recipes a distinct content
type. Fighter, Bomber, Destroyer, Command Ship, and Base presets lead into
shape, system, and palette controls with derived statistics; the project
library supports SAVE, LOAD, and DELETE. Vehicle and spacecraft records remain
separate so ground-vehicle and flight-runtime adapters can evolve without
ambiguous payloads. Published cars, trucks, and motorcycles become nearby
mountable rides through the existing vehicle input; boats reskin authored
world routes. Fighters and bombers use the existing flight motor, while every
spacecraft class also projects into the strategic fleet at liberated sites.
Destroyers, command ships, and orbital bases are strategic assets rather than
avatar-scale mounts.

Runtime craft are reconstructed deterministically from the published
generation for each play session. Their stable recipe IDs and compiled tuning
do not alter the legacy campaign vehicle enums; per-craft damage/readiness
persistence is a separate save-schema milestone.

## Publishing

**PUBLISH TO GAME** in the Project Hub:

1. Runs the store's validate-and-promote gate — one broken record publishes
   nothing.
2. Persists published hashes into the project.
3. Bakes weapons, creatures, vehicles, and spacecraft into
   `assets/published/` as deterministic JSON (sorted by id — republishing
   without edits is byte-identical).

The result appears in the hub status line and refreshes craft catalogs in the
same Designer process. The Game edition loads `assets/published/` read-only
before the first state transition: published weapons go on sale priced by the
forge's own derivation, published creatures become spawnable, and vehicle and
spacecraft recipes populate their gameplay adapters. Packaged builds may set
`STARFALL_ASSET_ROOT`; otherwise an `assets/` folder beside the executable is
preferred before the development checkout fallback.
Missing files are a first-class empty state, so a build with no published
content is simply the base game.

## Verifying your content ships

```
cargo test                                        # both content drift guards run here
cargo build --no-default-features                 # the consumer build
STARFALL_AUTOSTART=1 ./target/debug/starfall-i    # boot straight into play
```

If it plays in that binary, it ships. See
[verification.md](verification.md) for the full gate list.
