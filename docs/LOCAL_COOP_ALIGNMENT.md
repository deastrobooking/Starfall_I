# Local co-op feature alignment

Starfall and the Zig Heavy Water 3 project share a four-player couch co-op
direction, but they are separate implementations with different maturity and
ownership boundaries. This document records the shared product contract and
the current feature gap; it does not imply code or save compatibility.

## Shared product contract

- Support one to four local players with stable player slots and independent
  inputs, cameras, HUD feedback, and gameplay ownership.
- Keep shared campaign/world state explicit and personal rewards, equipment,
  and progression attached to the player who earned them.
- Treat join, leave, reconnect, pause, menus, interactions, vehicles, and
  rewards as four-player gameplay paths, not just controller plumbing.
- Keep the engine reusable: game-specific story, characters, and policy stay
  in the game layer and do not become framework assumptions.

## Current comparison

Starfall's Rust game already has player-select, stable `PlayerIndex` slots,
drop-in controller assignment, 1–4 split-screen, party camera modes, per-player
HUD and saved progression, owner-scoped shops and crafting, player-attributed
chest score, and co-op platformer switches. See [the feature catalog](MASTER_FEATURES.md)
and [the detailed multiplayer reference](FEATURES.md#local-multiplayer).

Heavy Water 3's Zig roadmap also has independent saved `Kit`s, shared `Progress`,
guest movement and traversal, independent views, and four-player input. Its
roadmap still lists guest driving/flying, conversations, fabrication, and
personal reward progression through the complete city loop as unfinished. See
[Heavy Water 3's roadmap](../../Heavy-Water-3/docs/roadmap.md#next).

These are capability comparisons only. The projects do not share runtime code,
save formats, engine contracts, or feature parity commitments.

## Feature pass: Starfall priorities

1. Make shared interactions deterministic and owner-attributed. Guidance,
   chest claims, loot pickup, and indexed enemy-target selection now resolve
   equal-distance players by stable slot rather than ECS query order; loot
   events identify the recipient and each player's HUD shows their own loot
   receipt. NPC conversations choose the closest eligible player and resolve
   exact ties by slot and NPC identity; concurrent hack starts resolve ties by
   slot. Continue auditing simultaneous interact, shop, and reward paths for
   explicit ownership and feedback.
2. Close player-scoped UX gaps: menu focus, controller prompts, loadout,
   crafting, rewards, and accessibility must identify the acting player and
   avoid leaking another player's state.
3. Complete four-player performance and hands-on validation for split-screen,
   shared-camera transitions, controller disconnect/reconnect, and mixed
   keyboard/controller play. Record measured results in the validation docs.
4. Use Heavy Water 3's remaining guest-loop items as a cross-project review
   checklist: drive/fly, talk, fabricate, and earn individual rewards across a
   complete mission loop. Implement each in the owning game layer, then verify
   the corresponding engine contracts remain reusable.

The active delivery order remains in [Project Plan](PROJECT_PLAN.md); this
alignment note is a focused co-op lens, not a second roadmap.
