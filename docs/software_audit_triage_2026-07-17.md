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

### Armor semantics and derived health ownership

`ArmorSet` equipment defense mitigates incoming damage first. The fields
`PlayerStats.armor` and `max_armor` are a separate rechargeable durability
buffer that absorbs 70 percent of the post-mitigation amount; health receives
the remaining 30 percent. They are not duplicate defense calculations.

The naming is unclear, and maximum-health ownership is genuinely split:

- character blueprints establish authored gameplay health;
- player spawn adds perk and tech bonuses;
- level-up mutates the effective cap;
- save hydration restores the effective cap;
- armor sync reconstructs the cap from a hard-coded base of 100 plus level,
  equipment, perk, and tech bonuses.

Do not patch this with another writer. Design a schema-compatible base-stat
component/save field, a pure derived-cap function, and one sync system. Migrate
legacy saves by deriving a safe base from their effective cap and known bonuses,
then cover equip/unequip, perk reset, level-up, authored blueprint, and reload.

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
- Do not add root integration tests before the binary crate exposes a reusable
  app factory. A `/tests` file cannot exercise private plugin construction by
  itself.
