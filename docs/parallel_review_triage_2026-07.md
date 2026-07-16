# Parallel Review Triage — July 2026

This document is the durable repository memory for the 156-item external review
received after ET2c. The source list is useful as an idea inventory, but it was
generated from a mixture of current code, older documentation, and inference.
Items below are classified from direct repository evidence before scheduling.

## Review policy

- Verify behavior in code and tests before treating a suggestion as a defect.
- Finish four-player ownership and controller-complete existing workflows before
  adding new game modes or large content branches.
- Prefer one coherent production slice over many disconnected feature stubs.
- Preserve Starfall's local-co-op scope. Network multiplayer, matchmaking,
  guilds, DLC, and crossover systems are not implicit roadmap commitments.
- Do not call the project production-ready until controller-only, four-player,
  save-recovery, and performance acceptance tests pass on representative levels.

## Already present or materially stale claims

The following suggestions describe systems that already exist. They may still
need refinement, but should not be reimplemented:

- chapter selection has labeled action buttons, fast-travel buttons, controller
  scrolling, and shared menu-focus infrastructure;
- the pause menu exposes difficulty, music, SFX, and rumble settings, and
  settings have independent persistence;
- crafting is raised through each player's `PlayerInput.crafting` action and the
  crafting panel tracks an owning `PlayerIndex`;
- controller diagnostics exist on F8 and report per-player device state;
- the F11 performance overlay reports FPS, frame time, entity count, fixed tick
  count, and simulation rate;
- dynamic day/night lighting is implemented in `WorldPlugin`;
- primary and special weapons are unlimited, tracking missiles are selectable,
  swept collision exists, arm-cannon charging exists, magic tracking exists,
  and the Star Sabre has controller mappings;
- mountain caves, shared-screen dungeon gates, enterable building shells,
  connected mountain routes, sky-road access corridors, barriers, loops, ramps,
  and support columns already have implementation and regression tests;
- per-player HUD, save records, companions, crafting ownership, chest/loot
  routing, damage feedback ownership, and vehicle baselines already use
  `PlayerIndex` in substantial parts of the runtime.

## July 15 follow-up audit reconciliation

The later six-item deficiency memo was checked against the July 15 branch. It
contains several concrete follow-up ideas, but its highest-severity conclusions
were based on older code or inferred behavior:

| Memo finding | Verified disposition | Durable follow-up |
|---|---|---|
| Companions collapse onto P1 | **Stale.** Default companions spawn once per active `PlayerIndex`; follow, attack, heal, and recruited-ally paths resolve `Companion.owner`. | Hardware-test multiple companions during four-player combat and budget their effects. |
| Co-op chests call `get_single_mut()` and panic | **Stale.** Chests iterate eligible players, choose the nearest by distance, and mutate that entity's stats/health. Enemy loot likewise resolves the nearest eligible player entity. | Add an explicit equal-distance tie-break test. Per-player inventory/loadout persistence is now delivered. |
| Saves overwrite all players with P1 or permit NaN armor | **Mostly stale.** `players[]` is keyed by `player_index`, values are bounded when applied, and legacy flat fields are compatibility fallback only when `players[]` is empty. Inventory stacks and active loadout choices now round-trip per player. | Add finite-number rejection and schema migration/recovery UX. `PerkTree` remains deliberately campaign-shared. |
| Garage modes buff the whole party | **Partly stale.** Vehicle state is party-shared by policy, but effects are gated by `active_owner`; boats have deterministic driver/passenger ownership. Runtime meshes/controllers are still only a baseline. | Build visible assembly-driven vehicle bodies and document drop-in, driver handoff, and pause authority. |
| Menus have no repeat throttle and are mouse-biased | **Mostly stale.** Shared menu focus supports D-pad/stick, Confirm/Back, disabled skipping, focus-follow scrolling, a 0.34 s initial repeat delay, and 0.11 s repeat cadence. | The present policy is one party-shared cursor for party menus plus owner-scoped specialist panels. Record four-pad controller-only acceptance before considering independent cursors. |
| Aim has no cone; missiles have no swept collision; movement remains per-frame | **Mostly stale.** Aim assistance and missile acquisition use forward cones; projectile-to-target collision sweeps the previous/current segment; EC1 motor/traversal/grapple is now fixed at 64 Hz by default with buffered edges and camera interpolation. | Add world-obstacle casting for fast missiles, migrate the remaining combat/AI timing deliberately, and complete EC2 frame-data collision layers. |

Do not preserve the memo's red/critical labels in milestones. The labels do not
match current code evidence and would cause completed ownership and input work
to be reimplemented.

## Confirmed or partially confirmed gaps

### Ship-blocking interaction gaps

- Armor element cycling remains a keyboard-oriented development path and needs
  a controller-friendly, per-player ownership model.
- Vehicle buffs are owner-keyed, but the campaign still uses one party-shared
  active ground/air mode. Boat driver/passenger behavior exists; the wider
  driver-seat and drop-in/out policy needs clearer runtime authority.
- Some controller workflows exist as isolated buttons/scroll systems without a
  proven end-to-end focus contract, especially settings, perk/upgrade panels,
  and Robot Garage persistence.
- The fixed input buffer is additive infrastructure; the complete movement and
  combat simulation has not migrated to buffered command consumption.

### High-priority feel/readability gaps

- Aim assist has target selection, but a clear split-screen-safe lock indicator
  is still needed.
- Weapon visuals are partly hardcoded; beam width, color, glow, muzzle, trail,
  and impact presentation need a coherent profile.
- `DamageInfo` carries critical and knockback data, but differentiated critical
  presentation, bounded hitstop, stagger/launch response, and debris/spark
  feedback are not a complete end-to-end system.
- Frame data and cancel windows remain roadmap work. They should be implemented
  as data and tests rather than scattered timing conditionals.
- Per-player effect budgets are not formally enforced for four-view rendering.

### Engine and content-production gaps

- ET1–ET5c now provide safe tool mode, stable IDs, selection, transactions,
  controller UI, viewport picking, transform handles, versioned atomic project
  persistence/recovery, typed materials, deterministic world recipes, topology
  validation, and atomically replaced protected sandbox roots.
- ET5d is the next World Kit dependency: direct point/node/edge editing and an
  explicit, scoped promotion transaction. Shipped world roots must remain
  untouched until that boundary passes rollback and dependency tests.
- Generated road/building/cave parts compile as bounded sandbox roots, but broad
  shipped-world promotion still needs dependent collider/guide/barrier/room/
  stair replacement and rollback as one transaction.
- `world_plugin.rs` and `ui_plugin.rs` are large. Extraction should follow proven
  ownership boundaries; a mechanical split during active feature work would
  create merge risk without improving behavior.

## Prioritized execution waves

### Wave A — Current parallel production push

1. Four-player equipment/crafting/vehicle ownership audit and confirmed fixes.
2. Controller-complete menu, settings, chapter/perk, and Robot Garage flow.
3. Combat readability slice: lock feedback, beam profiles, and bounded impact
   differentiation.
4. Full integration tests, strict Clippy, executable rebuild, and live smoke.

Delivered in the first parallel push: per-player armor controls, controller
crafting with input isolation, vehicle/boat authority hardening, focus-reveal
scrolling and compact-menu fixes, Garage scrolling/persistence verification,
world-space tracking lock rings, shared charged-shot critical resolution, and
critical impact differentiation. Automated integration gates pass; recorded
four-controller hardware and TV-layout acceptance remain.

### Wave B — Engine persistence and World Kit foundation *(delivered)*

ET3 through ET5c delivered the project manifest, versioned registries, atomic
draft/recovery persistence, stable adapters, published material/recipe catalogs,
typed generator parameters, topology preflight, and protected sandbox compile.
ET5d promotion is intentionally not implied by this completion.

### Wave C — Deterministic action engine *(current)*

1. Hardware-feel and refresh-rate validate the default-on EC1 fixed motor and
   retain the documented legacy escape hatch until acceptance is recorded.
2. Complete EC2 move definitions, frame data, cancel windows, hit/hurt layers,
   per-move i-frames, and player-received knockback. Bounded hitstop, flinch,
   damage numbers, dissolve, rumble, and camera shake are already the first slice.
3. Establish four-player effect, physics, audio, and render budgets, including
   missile world-obstacle casts and simultaneous companion/combat effects.

### Wave D — Content enrichment

After Waves A–C pass acceptance, prioritize cave loot/hazards, dungeon room
variety, chapter enemy identities, settlement navigation, road signage, and
boss phase environments. These should be authored through the toolchain rather
than added as more hardcoded one-off geometry.

## Deferred idea bank

PvP, tournament brackets, network multiplayer, matchmaking, guilds, seasonal
content, DLC, crossover characters, full replay video, HRTF, screen-space
reflections, fluid simulation, procedural campaign generation, and similar
large expansions remain ideas only. They require separate product decisions and
must not displace four-player campaign completion or the game-maker toolchain.

## Acceptance evidence required

- controller-only traversal from boot through character, chapter, settings,
  garage, gameplay, pause, and return flows;
- four active players independently exercising equipment, crafting, weapons,
  rewards, feedback, and vehicle/passenger actions;
- no gameplay input or simulation leakage while Starfall Forge is open;
- representative split-screen combat meeting frame/effect budgets;
- save interruption, corruption, migration, and recovery tests (delivered
  July 16, 2026 with the v4 atomic-save hardening in `save_plugin.rs`);
- `cargo test`, strict all-target Clippy, debug/release builds, and startup/editor
  smoke tests remain green.
