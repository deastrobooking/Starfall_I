# Controller And Player Ownership (`PO#`)

This document defines local-player identity independently from controller
connection order. `PlayerIndex` is the stable runtime/save identity; a gamepad
is a replaceable input device assigned to that identity.

## PO1 — Start-to-join assignment

- In Player Select, pressing Start on an unassigned controller claims the first
  joined-but-unassigned slot, then the next open slot.
- The first claim normally attaches to P1, which remains available to keyboard
  and mouse. Later claims attach to P2–P4.
- A controller already assigned to a player cannot claim another slot.
- Disconnecting releases the device binding but preserves the joined player,
  character, readiness, and save identity. Pressing Start after reconnect can
  reclaim that unassigned joined slot.
- Runtime input and F8 diagnostics resolve through `GamepadAssignments`; they
  must never infer ownership from gamepad query/enumeration order.
- The native macOS controller fallback may claim a slot only when Bevy did not
  report a Start press in the same frame.

## PO2 — Individual save ownership

Campaign state remains shared: chapters, world sites/routes, settlements,
raids, robot-pet collection, command assets, hacking, and final-war state.

Each `PlayerSaveData`, keyed by `player_index`, owns:

- level, XP, credits, health, stamina, and armor points;
- inventory/acquired loot and equipped quick item;
- selected primary/special weapon, armor element, and traversal mode;
- perk tree, tech-upgrade ledger, and weapon ranks;
- character blueprint, Studio recipe, and modular part loadout through the
  corresponding player-select slot.

Save schema version 3 adds optional per-player progression fields. Old saves
continue to load by seeding `PlayerProgression` from the legacy campaign-wide
perk, upgrade, and weapon-rank records. New saves write independent progression
for every active `PlayerIndex`.

## Delivered progression ownership slice

Each player-select slot now carries its own `PlayerProgression`. Chapter Select
shows one `P# UPGRADES` owner button per joined player; perk, tech, and weapon
rank purchases mutate only the selected slot. Spawning copies that slot into the
runtime player, and level-up awards, movement perks, health regeneration, armor
bonuses, aim assists, ammo caps, ranged/special damage, melee upgrades, and
Star Sabre upgrades consume the owning player's component.

Disk hydration seeds every slot from its matching `player_index`. Legacy saves
seed all slots from the old campaign-wide progression fields, while optional v3
player records override their matching slots. Entering gameplay no longer
reapplies stale disk progression after Chapter Select, so a purchase made just
before launch survives into the session and the next save.

The legacy top-level perk, upgrade, and weapon-rank resources remain serialized
for backward compatibility and for systems not yet audited. PX3 should apply the
same explicit owner selector to the transactional item shop before those fields
are retired.

Manual acceptance still required: four physical controllers joining in arbitrary
order, disconnect/reconnect in every slot, save with distinct inventories and
progression, restart, reload, and verify no cross-player swaps.
