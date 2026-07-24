# Software Architecture Audit Triage — July 17, 2026

> Dated disposition of the follow-up “scaling wall” audit. Use
> `current_state.md` and `agent_next_steps.md` for execution order.

## Accepted winners

### App smoke seam before extraction

Large world, UI, and Forge files remain a collaboration and review risk. Keep
the proposed subsystem boundaries, but first add a reusable app-construction
seam and startup/plugin smoke tests. Then extract one behavior-preserving module
at a time while preserving schedule order. Roads are already the first world
extraction in `src/plugins/world_plugin/roads.rs`.

### Armor semantics and derived health ownership — delivered July 23, 2026

`ArmorSet` equipment defense mitigates incoming damage first. The fields
`PlayerStats.armor` and `max_armor` are a separate rechargeable durability
buffer that absorbs 70 percent of the post-mitigation amount; health receives
the remaining 30 percent. They are not duplicate defense calculations.

`PlayerBaseStats` now owns the blueprint/hero level-one health and armor bases.
Effective caps derive from those bases plus level, equipment, perks, and
upgrades, with ordinary-frame reconciliation owned by `ArmorPlugin`. Level-up
changes level instead of mutating the cached health cap, and permanent world
armor rewards change the base.

`PlayerSaveData` adds optional base fields without rejecting schema-v4 records.
Legacy saves infer each missing base from their effective cap and known
modifiers; unknown historical bonuses remain absorbed in the inferred base.
Tests cover authored blueprints, level/modifier math, equip/reset behavior,
fill-ratio preservation, and legacy reload without double application.

### Stable persisted IDs

Centralize high-value IDs at the domain owner before considering broad enums.
This pass establishes a constant for the Scallarian drone-core blueprint. Next
targets should be IDs shared across two or more modules or stored in saves.
Keep strings at serialization boundaries so old data remains compatible.

### Measured large-world profiling

Retain the recommendation to profile one-to-four-camera terrain, entity,
physics, and render costs before implementing N11c streaming. Optimization
choices must follow measurements from representative anchors and encounters.

## Already delivered or stale

- Terrain already renders as 64 high/low patches using camera-aware spatial LOD
  over an unchanged full-resolution collider.
- Terrain-height and sky-road caches already use bounded four-seed LRU storage
  and poisoned-lock recovery.
- The raw “400 panic vectors” count mixes tests, guarded invariants, and normal
  APIs. There are no production `get_single`/`get_single_mut` calls. The one
  redundant guarded dungeon-room `expect` identified in this review was removed
  for clarity; the crafting example exists only inside a unit test.

## Rejected sequencing

- Do not split terrain again; measure the shipped patch implementation.
- Do not perform repository-wide panic replacement without reachability and
  invariant analysis.
- Do not move the monoliths wholesale or invent parallel `/src/world` and
  `/src/ui` architectures in one change. Continue from existing plugin module
  boundaries.
- The prerequisite is now delivered: the binary exposes a reusable app factory
  internally, with headless construction/Startup/steady-frame smoke coverage.
  Keep root integration tests deferred until there is a real library boundary;
  a `/tests` file still cannot exercise private binary modules by itself.
