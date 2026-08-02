//! Engine-core loop scaffolding (roadmap `EC0`).
//!
//! Owns the cross-cutting "character action engine" substrate:
//! * [`GameSet`] — the canonical gameplay system order
//!   (`SampleInput → BuildCommands → Motor → Combat → PhysicsRead → Animation →
//!   Camera → Presentation`). EC0 *defines and orders* the sets; per-system
//!   `.in_set(..)` assignment across gameplay plugins is incremental. EC1 has
//!   moved traversal, grapple, and the motor to the default-on fixed chain;
//!   combat and AI migration remain scoped follow-up work.
//! * Profiling: frame-time + entity-count diagnostics and an in-game perf overlay
//!   (F11), alongside the collider overlay (F9) and controller diagnostics (F8). Build with
//!   `--features tracy` to stream spans to the Tracy profiler.
//!
//! This module is intentionally self-contained so it can become an engine crate
//! (`forge_play`) at `EC7` without untangling game-specific imports.

use bevy::diagnostic::{
    DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::components::weapon::Projectile;
use crate::engine::rendering::{
    EnergyMaterial, IceMaterial, LavaMaterial, ShieldMaterial, ToonMaterial, WaterMaterial,
};
use crate::engine::spatial_lod::{ManagedSpatialLod, SpatialLod, SpatialLodProxy};
use crate::plugins::weapon_plugin::{HitParticle, SabreTechniqueVfx, SABRE_VFX_ENTITY_BUDGET};
use crate::plugins::world_plugin::{
    BuildingClusterLodProxy, TerrainHighLodPatch, TerrainProxyLodPatch,
};

/// Canonical per-frame gameplay ordering. Systems opt in with `.in_set(GameSet::X)`.
///
/// The canonical order spans schedules: input is sampled before the fixed loop,
/// EC1 traversal/grapple/motor run in `FixedUpdate`, and presentation stays in
/// frame schedules. Combat and physics-read systems opt into the remaining sets
/// as their fixed-tick migrations land.
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
/// loop is live and feeds the F11 perf overlay; later useful for replay timestamps.
#[derive(Resource, Default)]
pub struct FixedTickCount(pub u64);

/// EC1b render interpolation: world position captured at the START of the most
/// recent fixed tick. Presentation systems (camera follow) lerp between this
/// and the live transform by `Time<Fixed>::overstep_fraction()`, hiding the
/// tick staircase at render rates above `FIXED_HZ`. Costs ≤1 tick of camera
/// latency; only active while `SimConfig.fixed_motor` is on.
#[derive(Component, Debug, Clone, Copy)]
pub struct PreviousTickPosition(pub Vec3);

/// EC1b runtime toggle for the experimental fixed-tick player motor.
///
/// **Default ON** (EC1b shipped): the motor's simulation systems run in
/// `FixedUpdate` and their translation is flushed to physics once per frame.
/// Escape hatches: launch with `STARFALL_LEGACY_MOTOR=1` or toggle **F10** at
/// runtime to fall back to the legacy per-frame `Update` motor.
#[derive(Resource)]
pub struct SimConfig {
    pub fixed_motor: bool,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            // EC1b shipped: fixed-tick motor is the default path. Escape
            // hatches: launch with STARFALL_LEGACY_MOTOR=1 or toggle F10.
            fixed_motor: std::env::var_os("STARFALL_LEGACY_MOTOR").is_none(),
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
    // Dev escape hatch — Designer edition only; consumers never flip the
    // motor out from under the game (docs/PROJECT_PLAN.md P3).
    if !cfg!(feature = "designer") {
        return;
    }
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

// ── Perf overlay (F11) ──────────────────────────────────────────────────────

#[derive(Resource, Default)]
struct PerfOverlay {
    visible: bool,
}

#[derive(Component)]
struct PerfOverlayRoot;

#[derive(Component)]
struct PerfOverlayText;

#[derive(SystemParam)]
struct RenderPerfCounts<'w, 's> {
    cameras: Query<'w, 's, &'static Camera>,
    projectiles: Query<'w, 's, (), With<Projectile>>,
    transient_vfx: Query<'w, 's, (), With<HitParticle>>,
    sabre_vfx: Query<'w, 's, (), With<SabreTechniqueVfx>>,
    lod_profiles: Query<'w, 's, (), With<SpatialLod>>,
    lod_renderables: Query<'w, 's, (), With<ManagedSpatialLod>>,
    lod_proxies: Query<'w, 's, (), With<SpatialLodProxy>>,
    terrain_high: Query<'w, 's, &'static ViewVisibility, With<TerrainHighLodPatch>>,
    terrain_proxies: Query<'w, 's, &'static ViewVisibility, With<TerrainProxyLodPatch>>,
    building_proxies: Query<'w, 's, &'static ViewVisibility, With<BuildingClusterLodProxy>>,
    standard: Res<'w, Assets<StandardMaterial>>,
    toon: Res<'w, Assets<ToonMaterial>>,
    water: Res<'w, Assets<WaterMaterial>>,
    energy: Res<'w, Assets<EnergyMaterial>>,
    shield: Res<'w, Assets<ShieldMaterial>>,
    ice: Res<'w, Assets<IceMaterial>>,
    lava: Res<'w, Assets<LavaMaterial>>,
}

impl RenderPerfCounts<'_, '_> {
    fn active_cameras(&self) -> usize {
        self.cameras
            .iter()
            .filter(|camera| camera.is_active)
            .count()
    }

    fn custom_materials(&self) -> usize {
        self.toon.len()
            + self.water.len()
            + self.energy.len()
            + self.shield.len()
            + self.ice.len()
            + self.lava.len()
    }
}

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
    // The perf overlay is a Designer affordance, not a consumer feature.
    if !cfg!(feature = "designer") {
        return;
    }
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
    render: RenderPerfCounts,
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
    let cameras = render.active_cameras();
    let projectiles = render.projectiles.iter().count();
    let sabre_vfx = render.sabre_vfx.iter().count();
    let transient_vfx = render.transient_vfx.iter().count() + sabre_vfx;
    let standard_materials = render.standard.len();
    let custom_materials = render.custom_materials();
    let lod_profiles = render.lod_profiles.iter().count();
    let lod_renderables = render.lod_renderables.iter().count();
    let lod_proxies = render.lod_proxies.iter().count();
    let terrain_high_total = render.terrain_high.iter().count();
    let terrain_high_visible = render
        .terrain_high
        .iter()
        .filter(|visibility| visibility.get())
        .count();
    let terrain_proxy_total = render.terrain_proxies.iter().count();
    let terrain_proxy_visible = render
        .terrain_proxies
        .iter()
        .filter(|visibility| visibility.get())
        .count();
    let building_proxy_total = render.building_proxies.iter().count();
    let building_proxy_visible = render
        .building_proxies
        .iter()
        .filter(|visibility| visibility.get())
        .count();

    // Budgets (EC0): 60 FPS = 16.67 ms/frame; sim < 2–4 ms target (EC1 splits this out).
    for mut text in &mut text_q {
        *text = Text::new(format!(
            "PERF (F11)\nFPS:   {fps:5.1}\nframe: {frame_ms:5.2} ms\nents:  {entities:.0}\ncams:  {cameras}\nshots: {projectiles}  vfx: {transient_vfx}\nsaber: {sabre_vfx}/{SABRE_VFX_ENTITY_BUDGET}\nmats:  {custom_materials} custom / {standard_materials} standard\nlod:   {lod_profiles} profiles / {lod_renderables} meshes / {lod_proxies} proxies\nterrain visible: {terrain_high_visible}/{terrain_high_total} high  {terrain_proxy_visible}/{terrain_proxy_total} proxy\nbuilding proxy:  {building_proxy_visible}/{building_proxy_total} visible\nsim:   {} ticks @{:.0}Hz",
            ticks.0, FIXED_HZ,
        ));
    }
}
