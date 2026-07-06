//! Engine-core loop scaffolding (roadmap `EC0`).
//!
//! Owns the cross-cutting "character action engine" substrate:
//! * [`GameSet`] — the canonical gameplay system order
//!   (`SampleInput → BuildCommands → Motor → Combat → PhysicsRead → Animation →
//!   Camera → Presentation`). EC0 *defines and orders* the sets; per-system
//!   `.in_set(..)` assignment across the gameplay plugins lands in `EC1` when the
//!   simulation moves to `FixedUpdate`.
//! * Profiling: frame-time + entity-count diagnostics and an in-game perf overlay
//!   (F9), mirroring the controller-diagnostics overlay (F8). Build with
//!   `--features tracy` to stream spans to the Tracy profiler.
//!
//! This module is intentionally self-contained so it can become an engine crate
//! (`forge_play`) at `EC7` without untangling game-specific imports.

use bevy::diagnostic::{
    DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin,
};
use bevy::prelude::*;

/// Canonical per-frame gameplay ordering. Systems opt in with `.in_set(GameSet::X)`.
///
/// The chain is configured for `Update` today; `EC1` moves the simulation sets
/// (`Motor`/`Combat`/`PhysicsRead`) into `FixedUpdate`, leaving `SampleInput`
/// before the fixed loop and `Animation`/`Camera`/`Presentation` after it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameSet {
    /// Read raw devices into per-player input (PreUpdate today).
    SampleInput,
    /// Turn buffered input into discrete commands (jump/dash/attack requests).
    BuildCommands,
    /// Character motor: acceleration, swing, dash, traversal.
    Motor,
    /// Combat: attacks, hitboxes, damage, hitstop.
    Combat,
    /// Read physics results back (grounded state, contacts, casts).
    PhysicsRead,
    /// Pose selection, IK, procedural animation.
    Animation,
    /// Camera follow / split-shared blending.
    Camera,
    /// Final visual reconciliation (render interpolation, lands in EC1).
    Presentation,
}

/// Simulation tick rate (EC1). 64 Hz is Bevy's default; A/B against 120 with the
/// F9 perf overlay before committing. Change here to retune the whole fixed loop.
pub const FIXED_HZ: f64 = 64.0;

/// Monotonic count of fixed simulation ticks executed (EC1). Proves the fixed
/// loop is live and feeds the perf overlay; later useful for replay timestamps.
#[derive(Resource, Default)]
pub struct FixedTickCount(pub u64);

/// EC1b runtime toggle for the experimental fixed-tick player motor.
///
/// **Default OFF** → the motor runs in `Update` exactly as before (shipping path
/// is unchanged). Toggle at runtime with **F10**, or start with the env var
/// `STARFALL_FIXED_MOTOR=1`. When ON, the motor's simulation systems run in
/// `FixedUpdate` and their translation is flushed to physics once per frame.
#[derive(Resource)]
pub struct SimConfig {
    pub fixed_motor: bool,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            fixed_motor: std::env::var_os("STARFALL_FIXED_MOTOR").is_some(),
        }
    }
}

/// Run condition: fixed-tick motor enabled.
pub fn fixed_motor_on(sim: Res<SimConfig>) -> bool {
    sim.fixed_motor
}

/// Run condition: fixed-tick motor disabled (legacy `Update` motor).
pub fn fixed_motor_off(sim: Res<SimConfig>) -> bool {
    !sim.fixed_motor
}

fn toggle_fixed_motor(keyboard: Res<ButtonInput<KeyCode>>, mut sim: ResMut<SimConfig>) {
    if keyboard.just_pressed(KeyCode::F10) {
        sim.fixed_motor = !sim.fixed_motor;
        info!("EC1b fixed-tick motor: {}", sim.fixed_motor);
    }
}

/// EC0 engine-core plugin: ordering skeleton + profiling.
pub struct GameLoopPlugin;

impl Plugin for GameLoopPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            EntityCountDiagnosticsPlugin::default(),
        ))
        // EC1: pin the simulation tick rate.
        .insert_resource(Time::<Fixed>::from_hz(FIXED_HZ))
        .init_resource::<FixedTickCount>()
        .init_resource::<SimConfig>()
        .init_resource::<PerfOverlay>()
        .add_systems(FixedUpdate, count_fixed_tick)
        .add_systems(Update, toggle_fixed_motor)
        .configure_sets(
            Update,
            (
                GameSet::SampleInput,
                GameSet::BuildCommands,
                GameSet::Motor,
                GameSet::Combat,
                GameSet::PhysicsRead,
                GameSet::Animation,
                GameSet::Camera,
                GameSet::Presentation,
            )
                .chain(),
        )
        .add_systems(Startup, setup_perf_overlay)
        .add_systems(Update, (toggle_perf_overlay, update_perf_overlay).chain());
    }
}

fn count_fixed_tick(mut ticks: ResMut<FixedTickCount>) {
    ticks.0 = ticks.0.wrapping_add(1);
}

// ── Perf overlay (F9) ───────────────────────────────────────────────────────

#[derive(Resource, Default)]
struct PerfOverlay {
    visible: bool,
}

#[derive(Component)]
struct PerfOverlayRoot;

#[derive(Component)]
struct PerfOverlayText;

fn setup_perf_overlay(mut commands: Commands) {
    commands
        .spawn((
            PerfOverlayRoot,
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(8.0),
                top: Val::Px(8.0),
                padding: UiRect::all(Val::Px(6.0)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.78)),
            GlobalZIndex(1000),
        ))
        .with_children(|root| {
            root.spawn((
                PerfOverlayText,
                Text::new("PERF (F11)"),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(Color::srgb(0.60, 1.0, 0.72)),
            ));
        });
}

fn toggle_perf_overlay(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<PerfOverlay>,
    mut root_q: Query<&mut Visibility, With<PerfOverlayRoot>>,
) {
    // F11: F9 is collider debug (world_plugin), F10 is the fixed-motor toggle.
    if !keyboard.just_pressed(KeyCode::F11) {
        return;
    }
    state.visible = !state.visible;
    let vis = if state.visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut v in &mut root_q {
        *v = vis;
    }
}

fn update_perf_overlay(
    state: Res<PerfOverlay>,
    diagnostics: Res<DiagnosticsStore>,
    ticks: Res<FixedTickCount>,
    mut text_q: Query<&mut Text, With<PerfOverlayText>>,
) {
    if !state.visible {
        return;
    }
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let frame_ms = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let entities = diagnostics
        .get(&EntityCountDiagnosticsPlugin::ENTITY_COUNT)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);

    // Budgets (EC0): 60 FPS = 16.67 ms/frame; sim < 2–4 ms target (EC1 splits this out).
    for mut text in &mut text_q {
        *text = Text::new(format!(
            "PERF (F11)\nFPS:   {fps:5.1}\nframe: {frame_ms:5.2} ms\nents:  {entities:.0}\nsim:   {} ticks @{:.0}Hz",
            ticks.0, FIXED_HZ
        ));
    }
}
