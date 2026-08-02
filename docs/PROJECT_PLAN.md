# Starfall — Project Plan: Two Editions

This is the living plan for shipping **two products from one codebase**:

| Edition | Working name | Who it is for | What it is |
|---|---|---|---|
| **Game** | *Starfall I* | Players | The consumer game: campaign, combat, traversal, co-op, and the player-facing creators (character design, character studio, GLB import, robot garage). |
| **Designer** | *Starfall Forge* | Us, and eventually creators | Everything in the Game **plus** the authoring layer: Project Hub, versioned Forge projects, Creature Forge, Weapon Forge, and the live in-world editor. |

The Game is a strict subset of the Designer. One repository, one engine, one
test suite; the boundary is a cargo feature (`designer`, on by default) so a
consumer build is produced by *removing* the authoring layer, never by
maintaining a fork.

```
cargo build                            # Designer edition (default)
cargo build --no-default-features      # Game edition (consumer)
```

A player-facing creator is **game content** (players designing characters is
part of this game's identity). A tool is **designer-only** when it authors
versioned project records or edits the world live. That is the whole test for
which side of the line something lands on.

---

## 1. Where the codebase stands (review, 2026-07)

Full detail lives in [FEATURES.md](FEATURES.md); this is the shape of it.

**Engine core (shared)** — Bevy 0.19 + Avian 0.7 behind a compat shim;
fixed-tick motor (64 Hz) with carry-smoothed delivery; `GameSet` ordering;
deterministic RNG streams; per-player input edge buffer; procedural audio bus;
spatial LOD; v4 atomic saves. The EC# track in
[engine_roadmap.md](engine_roadmap.md) continues independently of the edition
split.

**Game systems** — 30+ campaign milestones: chapters, world sites/routes,
raids, hacking, settlements, secret caves, temple subquests, relic puzzles,
perk/tech progression, companions, vehicles, shop. Combat is data-driven
(`assets/combat/*.json`): melee phase machines, the Star Sabre line (blades,
hilt-in-hand, techniques, forgeable), tracking blasters with universal charge,
telegraphed enemies, hoverboard trick system with revert chaining.

**Player-facing creators (game)** — Character Design (player select),
Character Studio (human generator), Imported Character Forge (GLB import +
sculpt), Robot Garage (vehicle assembly). These ship to consumers.

**Designer tools** — Project Hub over a versioned `ForgeProject` store (atomic
writes, recovery snapshots, draft/published hashes), Creature Forge, Weapon
Forge (derived-stat modular weapon designer), live editor mode
(Tab / Select+Start in play), shared draggable tool windows, env boot hooks
(`STARFALL_EDITOR`, `STARFALL_WEAPON_FORGE`, …).

**Control system** — one layout across keyboard and every HID pad, per-player
ownership for 4-way co-op: bare D-pad swaps what RT fires (L/R primaries,
U/D specials), Select+D-pad is utility, LB is the global modifier (LB+D-pad =
traversal mode), RB is the melee/sabre attack. Context claims prevent
double-fire (drawn sabre claims heavy/dodge; the hoverboard claims all four
trick buttons). Verified by a 616+ test suite, clippy-clean, with scripted
boot smokes.

---

## 2. The edition boundary

| Surface | Game | Designer | Mechanism |
|---|---|---|---|
| Campaign, combat, traversal, co-op | ✅ | ✅ | always registered |
| Character Design / Studio / GLB import / Garage | ✅ | ✅ | always registered |
| Main-menu **CREATOR TOOLS** (Project Hub) button | — | ✅ | `cfg!(feature = "designer")` |
| Project Hub state + project registry UI | inert | ✅ | unreachable without the button |
| Creature Forge, Weapon Forge plugins | — | ✅ | `#[cfg(feature = "designer")]` registration |
| Live editor toggle (Tab / Select+Start) | — | ✅ | guard in the toggle system |
| Designer env boot hooks (`STARFALL_EDITOR`, `STARFALL_WEAPON_FORGE`) | — | ✅ | guard in `apply_boot_overrides` |
| `AppState` / `EngineToolMode` enums, tool-window + engine-tools plugins | present | present | kept in both to avoid `cfg` sprawl through matches; they are inert without their entry points |

Phase 0 (this commit) gates **entry points**, which is what "walk away with a
consumer build" needs. Phase 2 below shrinks the consumer binary by gating
modules, which is an optimization, not a correctness requirement.

## 3. Content pipeline between the editions

Designer authors content into **ForgeProject records** (creatures, weapons,
scenes…). Consumers must never depend on a project store existing.

1. **Author** (Designer): tools write draft records through the versioned
   store — atomic writes, recovery snapshots, draft hashes.
2. **Publish** (Designer): a publish step validates every record and
   bakes the published set into `assets/published/` as plain JSON the game
   loads read-only — the same pattern `moves.json` / `tricks.json` already
   prove out.
3. **Ship** (Game): the consumer build loads baked assets only; it has no
   writer for project stores. Player saves (v4) stay completely separate from
   authored content.

## 4. Roadmap

### P0 — Edition skeleton *(landed with this plan)*
`designer` cargo feature (default on); consumer build hides the hub button,
forge plugins, editor toggle, and designer env hooks. Both editions compile
and boot; verification gains a consumer-build check.

### P1 — Publish pipeline (Designer) → consumable content (Game) *(landed)*
- **PUBLISH TO GAME** in the Project Hub runs the store's validate-and-promote
  gate, persists published hashes, and bakes weapons + creatures to
  `assets/published/` as deterministic JSON (sorted, so re-publishing without
  edits is byte-identical). Results land in the hub status line.
- Game-side loader (`world::published_content`, both editions): published
  weapons register into the blade resolver and go on sale in the shop priced
  by the forge's own derivation; published creatures load into a
  `PublishedCreatures` spec pool. Missing files are a first-class empty state.
- Covered by determinism, round-trip, missing-file, resolver, and shop-pricing
  tests. Encounters consume published creatures: the baked seed fills the same
  `PublishedCreatureCatalog` the dungeon spawners already resolve overrides
  through, and only fills an *empty* catalog so a Designer session's live
  project store (drafts included) stays authoritative.

### P2 — Designer UX pass
- ~~Weapon Forge: load/rename/delete existing designs~~ *(landed: LIBRARY
  panel with LOAD/DEL, NAME field with editor-style key capture)*.
- ~~Equip-to-test~~ *(landed: TEST jumps into play with every player holding
  the design; the blade registry upserts so iteration always takes effect)*.
  Still open: character-holding preview inside the forge.
- ~~Shared forge widgets~~ *(landed: `engine_tools::forge_widgets` — action
  buttons, rows, section labels; Weapon and Creature Forge both build on it)*.
- ~~Mouse affordances for tool windows~~ *(landed: corner-grip resize with
  min/viewport clamping, double-click header collapse, and panels that shrink
  to fit when the application window is resized)*.
- ~~A designer manual~~ *(landed: [guides/designer-workflow.md](guides/designer-workflow.md))*.

### P3 — Game edition polish (walk-away list)
- First-run flow: main menu → player select → chapter select with no dev
  affordances (perf overlays and diag toggles behind `designer` too).
- Controller rebinding UI (the one control-system gap; the layout itself is
  stable and documented in `input_plugin.rs`).
- Onboarding: the guidance HUD covers systems; add a first-session tutorial
  route.
- Balance/feel passes driven by play: sabre timings, tracking strengths, trick
  scores — all already data-tunable without recompiling.

### P4 — Packaging
- `cargo build --release --no-default-features` produced per-platform with
  baked assets; the Designer edition ships as the default build.
- Version/branding split (window title, main-menu badge) driven by the same
  feature flag.
- CI matrix: default and `--no-default-features`, suite + clippy on both.

## 5. Walk-away criteria

**Game edition is done when:** a release consumer build boots to the menu with
no authoring entry points, plays the full campaign in 1–4 player co-op from
baked assets, saves/restores cleanly, and passes suite + clippy + boot smoke in
CI on both feature sets.

**Designer edition is done when:** every shipped content type (creature,
weapon, scene, biome, road, building, …) can be authored, validated,
published, and then played in a consumer build **without touching Rust**.

---

*Keep this document honest the same way FEATURES.md is kept honest: when the
boundary or the roadmap changes in code, change it here in the same commit.*
