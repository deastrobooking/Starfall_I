# Bevy 0.19 for a 4-Player 3D Game

This assumes local co-op / split-screen (the most common "4-player" setup) — the same ECS patterns apply almost unchanged if you're networking instead, just swap local input for replicated input.

## 1. Model each player as data, not as four copies of code

Since a `Resource` is now also a component on a dedicated entity in 0.19, keep global game state (match settings, round timer) as `Resource`, and everything per-player as `Component` on four separate entities. Never derive both on one type.

```rust
#[derive(Component)]
struct PlayerIndex(u8); // 0..3

#[derive(Component)]
struct PlayerInput {
    gamepad: Option<Gamepad>,
}

#[derive(Component)]
struct PlayerStats {
    health: f32,
    speed: f32,
}
```

Query with `With<PlayerIndex>` / `Without<IsResource>` rather than broad `Query<Entity>`, since resource entities now show up in unfiltered queries.

## 2. One BSN scene, spawned four times with overrides

Define the player as a single BSN scene, then patch per-instance data (spawn point, color, gamepad) at spawn time rather than writing four hand-built spawns.

```rust
fn player_scene(index: u8, spawn: Vec3, gamepad: Gamepad) -> impl Scene {
    bsn! {
        #Player
        Name(format!("Player{index}"))
        Transform::from_translation(spawn)
        PlayerIndex(index)
        PlayerInput { gamepad: Some(gamepad) }
        PlayerStats { health: 100.0, speed: 6.0 }

        Children [
            Name("CameraRig")
            CameraRig
        ]
    }
}
```

Because BSN scenes are composable patches, a shared `base_player_scene()` can hold the common hierarchy (rig, weapon socket, hitbox), and each of the four calls only overrides index/spawn/input — this is where BSN earns its keep over plain `commands.spawn`, since you get the same guaranteed child hierarchy (camera rig, hand socket) for all four without repeating it.

## 3. Split-screen cameras

Four players means four viewports. Give each player's `CameraRig` child a `Camera3d` with a `Viewport` rect sized to a quadrant, keyed off `PlayerIndex`:

```rust
fn layout_viewports(windows: Query<&Window>, mut cams: Query<(&PlayerIndex, &mut Camera)>) {
    let window = windows.single();
    let (w, h) = (window.resolution.physical_width() / 2, window.resolution.physical_height() / 2);
    for (idx, mut cam) in &mut cams {
        let (x, y) = match idx.0 { 0 => (0, 0), 1 => (w, 0), 2 => (0, h), _ => (w, h) };
        cam.viewport = Some(Viewport { physical_position: UVec2::new(x, y), physical_size: UVec2::new(w, h), ..default() });
    }
}
```

## 4. Input: one system per player, gated by observer run conditions

Route each gamepad to its `PlayerInput.gamepad`, then run one movement system per player filtered by `PlayerIndex`, or a single system iterating all four — either works, but keep the system generic over `PlayerIndex` rather than writing `player1_move`, `player2_move`, etc.

Use 0.19's observer run conditions for menu/pause-sensitive input (e.g., a "ready up" button per player) so the observer is registered once but only fires while the lobby is open, instead of manually checking state inside every handler.

## 5. Persist per-player settings with the new SettingsPlugin

Bind gamepad, dead-zone, and camera-sensitivity per player through the settings plugin so the choices survive a restart:

```rust
#[derive(Resource, SettingsGroup, Reflect, Default)]
#[reflect(Resource, SettingsGroup, Default)]
struct PlayerControlSettings {
    p0_gamepad: Option<usize>,
    p1_gamepad: Option<usize>,
    p2_gamepad: Option<usize>,
    p3_gamepad: Option<usize>,
    dead_zone: f32,
}

App::new()
    .add_plugins(SettingsPlugin::new("com.randy.fourplayer"))
    .run();
```

## 6. Performance with four viewports

Four simultaneous 3D viewports roughly multiply your per-frame render cost, so:

- Keep contact shadows on the key light(s) only — they get expensive fast across multiple cameras.
- Use the render-world `add_systems` scheduling (replacing the old render graph) if you add any custom per-camera post-processing, since it's simpler to gate per-viewport than a custom graph node was.
- Measure with your actual four-camera scene rather than the single-camera `bevy_city` benchmark — split-screen cost is dominated by camera count, not just entity count.

## 7. Keep structure and simulation separate

- BSN answers "what does a player consist of" (rig, sockets, starting stats).
- Components answer "what state does a player have right now."
- Systems answer "how does that state change each frame," and should stay player-index-agnostic so you're not duplicating movement/combat/AI logic four times.

## 8. Use `commands.delay(...)` for respawns and round timers

0.19 adds a built-in delayed-command API (`commands.delay(seconds).spawn(...)`, or delayed on an existing command) so you no longer need to hand-roll a timer resource + system just to respawn a player a couple seconds after death, or stagger a "ready, set, go" round start. This is a direct fit for local co-op:

```rust
fn on_player_died(mut commands: Commands, spawn: Vec3, index: u8) {
    commands.delay(Duration::from_secs(2)).spawn(player_scene(index, spawn, /* ... */));
}
```

Use it for respawn delays, pickup/powerup expiry, and staggered round-start countdowns — anything that was previously a one-off `Timer` component plus a polling system.

## 9. Lighting and post-processing worth the render cost across 4 viewports

A few 0.19 rendering features are genuinely cheap enough to use per-viewport, others are not:

- **Rectangular area lights** — good for readability in indoor/arena maps (light spilling from doorways, overhead strip lights), and unlike a point light they don't need per-camera tuning to look right from four different angles.
- **Screen-space reflections** — nice on wet floors/glossy arena surfaces, but it's a per-camera post effect; with 4 viewports it's one of the first things to disable or downres if you're framerate-constrained, before touching gameplay-critical shadows.
- **Vignette / lens distortion** — cheap camera post-effects, useful as a *player-specific* damage/status cue (e.g. vignette tightens when a player is low health) rather than just a permanent look — each `Camera3d` can carry its own settings, so this is naturally per-player without extra plumbing.
- **Diagnostics overlay** — turn this on per-viewport during development to catch which of the four cameras is actually the expensive one, rather than guessing from overall frame time.

## 10. Delay full BSN adoption where it doesn't help

Procedural/large-scene generation (the "Bevy City" style demo) still favors plain `commands.spawn` in a loop over BSN — BSN's value is guaranteeing a fixed hierarchy (like your player prefab), not building thousands of similar-but-varied instances at runtime. Keep BSN scoped to player/prop/UI prefabs and leave procedural level content as regular spawns.
