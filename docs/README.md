# Starfall Documentation

Starfall is documented as three connected products: the reusable **Starfall
Engine**, the native **Starfall Forge** authoring environment, and the complete
**Heavy Water Demo Game**. Each active concern has one authority.

## Start here

| Audience | First document |
|---|---|
| Evaluating or running Starfall | [Repository README](../README.md) |
| Building a game with the framework | [Building With Starfall](guides/building-with-starfall.md) |
| Creating a reusable feature | [Creating a Starfall Module](guides/creating-a-module.md) |
| Understanding ownership and graph compilation | [Framework Architecture](FRAMEWORK_ARCHITECTURE.md) |
| Navigating or generating projects | [Repeatable Project Structure](PROJECT_STRUCTURE.md) |
| Contributing native engine/game code | [Developer Documentation](DEVELOPMENT.md) |
| Authoring content in Forge | [Designer Workflow](guides/designer-workflow.md) |
| Exporting a standalone native game | [Exporting a Game](guides/exporting-a-game.md) |
| Learning from the complete game | [Heavy Water Demo Game](games/heavy-water/README.md) |

## Authorities

| Document | Owns |
|---|---|
| [FRAMEWORK_ARCHITECTURE.md](FRAMEWORK_ARCHITECTURE.md) | Engine/kit/Forge/demo boundaries, graph model, module contract |
| [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md) | Repeatable workspace, game, module, content, and build paths |
| [PROJECT_PLAN.md](PROJECT_PLAN.md) | Active priorities and migration sequence |
| [DEVELOPMENT.md](DEVELOPMENT.md) | Current build, code organization, implementation rules, verification |
| [MASTER_FEATURES.md](MASTER_FEATURES.md) | Heavy Water's current gameplay and creator-feature inventory |
| [engine_roadmap.md](engine_roadmap.md) | Simulation, input, combat substrate, and engine extraction milestones |
| [RENDERING_PROGRAM.md](RENDERING_PROGRAM.md) | Dynamic GI, virtual geometry, rendering benchmarks and their Forge integration |
| [editor_roadmap.md](editor_roadmap.md) | Forge document, preview, publishing, and extensibility milestones |
| [naming.md](naming.md) | Canonical Heavy Water names and terminology |
| [HEAVY_WATER_PORT.md](HEAVY_WATER_PORT.md) | Heavy Water continuity and port ledger |
| [guides/README.md](guides/README.md) | Task-oriented how-to index |

`FEATURES.md` is a legacy detailed snapshot. Current feature statements belong
in `MASTER_FEATURES.md`; historical audits and milestone logs belong under
[archive/](archive/README.md).

## Documentation rules

- Label target APIs and filesystem layouts as target until code and tests ship.
- Engine docs contain no Heavy Water story assumptions.
- Heavy Water docs may explain how the demo composes public engine contracts.
- A public code, schema, feature, graph, manifest, or path change updates its
  authority in the same work.
- Avoid copying the same live fact into several documents; link to its owner.
- Old dated plans move to the archive instead of competing with active plans.
- If documentation and code disagree, code describes current behavior and the
  mismatch becomes explicit migration work.
