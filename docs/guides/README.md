# Developer Guides — Process How-Tos

Short, task-oriented guides for working on Starfall I. Each answers "how do I
do X here" with the project's actual conventions. Reference docs (what exists
and why) live one level up in `docs/` — see [`docs/README.md`](../README.md)
for the map.

| Guide | Use it when you… |
|---|---|
| [verification.md](verification.md) | are about to land ANY change — gates, debug keys, env flags, smoke runs |
| [fixed-tick-motor.md](fixed-tick-motor.md) | touch player movement, input, or add a simulation system |
| [combat-feel.md](combat-feel.md) | add/tune hit feedback (hitstop, knockback, flinch, numbers, shake) or hook a new gameplay event |
| [character-studio-pipeline.md](character-studio-pipeline.md) | extend the character generator (morphs, wardrobe, presets, saves) |
| [spatial-lod.md](spatial-lod.md) | tune large-world render distance, add LOD coverage, or verify split-screen culling |

House rules that apply to every guide:
- **Milestone prefixes:** `M#` campaign · `MM#` motion · `AI#` enemy AI ·
  `EC#` engine core. Never mix them.
- **Lane separation:** `src/engine_tools/` (Starfall Forge) is its own
  workstream; coordinate before editing it. New road/world *content* shapes go
  through Forge recipes, not new hardcoded procgen.
- **Save compatibility:** new persisted fields always take `#[serde(default)]`.
- **Ownership:** per-player state is keyed by `PlayerIndex`, never query order.
