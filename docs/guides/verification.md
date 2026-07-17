# Verification — the gates every change passes

## Automated gates and boot smoke (run all, in order)

```sh
cargo fmt --all -- --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test                     # full suite (bin crate — there is no lib target)
cargo build
STARFALL_AUTOSTART=1 ./target/debug/starfall-i
                               # observe ~15s in Playing, then quit; zero panics
```

Boot-smoke both motor paths when you touched movement/input/physics:

```sh
STARFALL_AUTOSTART=1 ./target/debug/starfall-i                          # fixed-tick motor (default)
STARFALL_AUTOSTART=1 STARFALL_LEGACY_MOTOR=1 ./target/debug/starfall-i  # legacy per-frame motor
```

Crashes write timestamped `starfall_crash_*.log` reports in the platform data
root. Reports sanitize local home/install paths and retain the newest five.

## Env flags

| Flag | Effect |
|---|---|
| `STARFALL_AUTOSTART=1` | boot straight into `AppState::Playing` |
| `STARFALL_STUDIO=1` | boot straight into the Character Studio |
| `STARFALL_LEGACY_MOTOR=1` | disable the fixed-tick motor (legacy `Update` motor) |

## In-game debug keys

| Key | Overlay / toggle | Owner |
|---|---|---|
| F7 | command-asset roster | `world_plugin` |
| F8 | controller diagnostics | `ui_plugin` |
| F9 | collider debug draw | `world_plugin` |
| F10 | fixed-tick motor A/B toggle | `game_loop` |
| F11 | perf overlay (FPS, frame ms, entities, sim ticks, cameras, materials, total combat VFX, Saber VFX/cap) | `game_loop` |

## Profiling

- Quick: F11 overlay (budgets: 60 fps = 16.67 ms/frame; sim < 2–4 ms).
- Deep: `cargo run --release --features tracy` with a Tracy client attached.
- Profile release builds, never debug.

## Manual smoke (before merging an engine branch)

Controller in hand, on macOS: spawn 2 players → run/jump/dodge in a city →
climb a wall → grapple swing → hoverboard a speed road (curve + loop) → melee
+ beam a drone pack → open pause/save. Watch for input latency, camera judder,
and F11 frame spikes. This is the human gate the automated four can't replace.
