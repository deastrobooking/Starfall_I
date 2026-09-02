# Heavy Water Demo Game

Heavy Water is Starfall's complete native demo game and first-party reference
consumer. It remains a playable family-friendly action RPG while serving as the
worked example for building a large modular game with Starfall.

## Responsibilities

Heavy Water owns its story, heroes, antagonists, chapters, locations, campaign
progression, branded presentation, and content presets. It must not own engine
services or receive private access unavailable to another game.

The current code still mixes some of these layers. Extraction will organize the
game around:

```text
HeavyWaterShared
  ├── OpenWorldGamePlugin
  ├── PlatformerGamePlugin
  ├── RacerGamePlugin
  ├── RpgGamePlugin
  ├── HeavyWaterPresentationPlugin
  └── HeavyWaterCampaignPlugin
```

Open World, Platformer, and Racer remain independent modes. Shared identity,
save, and progression contracts may be used by several modes; campaign code
selects and transitions between them.

The first source-level boundary is available now:

- `starfall_i::heavy_water::shared` exposes shared progression contracts.
- `starfall_i::heavy_water::platformer` exposes the bounded platformer data,
  route language, spawning API, and `HeavyWaterPlatformerPlugin`.

This is a compatibility facade while the implementation leaves `world/`.
The platformer plugin now owns its workshop state and continuous rules; world
scene activation and teardown are the next ownership transfer.

## Why the demo remains complete

Focused templates will eventually provide smaller starting points, but the
all-in-one demo has a separate purpose:

- prove that engine features work together at real scale
- show complete code, content, save, UI, and publishing patterns
- provide inspectable examples inside Starfall Studio
- let users fork, rename, and remove modules from a working game
- serve users who want the complete engine and game rather than a minimal kit

The demo therefore should not be reduced to a toy sample. It should become a
clean, navigable production example.

## Learning path

Each extracted game mode will include:

1. A module README explaining ownership and dependencies.
2. A small architecture map.
3. Direct links from guides to working source.
4. An isolated test or playable scene.
5. Enable and removal instructions.
6. Examples of both Forge-authored graphs and native Rust extensions.

## Current references

- [Master feature catalog](../../MASTER_FEATURES.md)
- [Framework architecture](../../FRAMEWORK_ARCHITECTURE.md)
- [Repeatable project structure](../../PROJECT_STRUCTURE.md)
- [Designer workflow](../../guides/designer-workflow.md)
- [Naming guide](../../naming.md)

Detailed player controls and campaign information remain in the feature catalog
until the demo documentation is split without duplicating active facts.
