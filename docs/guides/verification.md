# Verification — the gates every change passes

## Automated gates and boot smoke (run all, in order)

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked                     # Designer edition
cargo check --workspace --all-targets --locked --no-default-features  # minimal framework
cargo clippy --workspace --all-targets --locked --no-default-features -- -D warnings
cargo test --workspace --locked --no-default-features
cargo check --workspace --all-targets --locked --no-default-features --features heavy-water-demo
cargo clippy --workspace --all-targets --locked --no-default-features --features heavy-water-demo -- -D warnings
cargo test --workspace --locked --no-default-features --features heavy-water-demo  # demo runtime
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --locked --no-deps
cargo build --locked
STARFALL_AUTOSTART=1 ./target/debug/starfall-i
                               # observe ~15s in Playing, then quit; zero panics
```

GitHub Actions runs formatting once, then applies the all-target check, strict
Clippy, and test gates independently to the Designer, demo-runtime, and minimal
framework profiles on macOS. Local verification keeps the same locked graph.

Framework work also adds a focused consumer or feature-combination check for
every new public bundle. A module is not reusable merely because the complete
Heavy Water application compiles with it present.

The minimal public facade has a separate consumer package and dependency gate:

```sh
cargo run -p starfall-framework-consumer --locked
python3 scripts/check_framework_dependencies.py
```

The gate checks the minimal facade, its `dynamic,tracy` modifier combination,
and the isolated consumer for rendering, physics, window, and native audio
dependencies. Run it on the host platform; CI also runs it on Linux in a job
without native game system libraries. Workspace-wide checks alone cannot prove
isolation because Cargo unifies features requested by workspace packages.

The isolated rendering capability adds a fourth CI profile:

```sh
cargo check --workspace --all-targets --locked --no-default-features --features render-lab
cargo clippy --workspace --all-targets --locked --no-default-features --features render-lab -- -D warnings
cargo test --workspace --locked --no-default-features --features render-lab
```

For lab/WGSL changes, run the native GPU/CPU probe comparison before collecting
performance evidence; its exit status fails on mismatched results or timeout:

```sh
cargo run --release --locked --no-default-features --features render-lab \
  --example render_lab -- --views 4 --validate-probes \
  --output target/render-lab/probe-check.json
```

Run both lab scenes at 1/2/4 views for renderer changes, with distinct report
names. Reports identify their compiled lab source, reject replacement of old
evidence, and omit unavailable GPU timings. See
[RENDERING_PROGRAM.md](../RENDERING_PROGRAM.md) for coverage and promotion gates.

For app/plugin registration changes, the fast headless guard is:

```sh
cargo test app_smoke_tests
```

`build_starfall_app(StarfallAppMode::Headless)` uses the production state,
resource, physics, and game-plugin graph while omitting the process-global
window and renderer. The smoke tests finalize that graph, execute Startup and
one steady frame, and verify core assets, menu UI/camera creation, messages,
plugins, and state. Keep this green before extracting systems from
`world_plugin`, `ui_plugin`, or `player_plugin`.

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
