//! Hit-reaction feedback (roadmap `EC2`): the read-side of combat events that
//! previously fired into the void. Everything here is presentation — no
//! damage math lives in this module.
//!
//! * **Flinch** — enemies briefly stop chasing/attacking when hit
//!   (`enemy_ai_system`/`enemy_attack_system` skip entities `With<Flinch>`).
//! * **Hit-flash** — a short additive flash orb at the impact point (no shared
//!   material mutation, so batched enemy materials stay untouched).
//! * **Death dissolve** — dying enemies shrink out over their despawn window
//!   instead of blinking away.
//! * **Damage numbers** — split-screen aware floating numbers, spawned per
//!   active player camera via `world_to_viewport`.
//! * **Outgoing-hit shake + rumble** — proximity-scaled trauma on the existing
//!   per-player rumble hook. Camera motion stays stable by design.

use bevy::prelude::*;

use crate::components::enemy::DeadEnemy;
use crate::components::player::{Player, PlayerCamera, PlayerIndex};
use crate::engine::state::AppState;
use crate::events::{EnemyDamagedEvent, EnemyKilledEvent};
use crate::plugins::input_plugin::trigger_player_rumble;
use crate::resources::GameSettings;

// ── Flinch ────────────────────────────────────────────────────────────────────

/// While present, enemy AI/attack systems skip this entity (`Without<Flinch>`
/// filters on their queries).
#[derive(Component)]
pub struct Flinch {
    pub timer: f32,
}

const FLINCH_SECONDS: f32 = 0.16;

fn apply_flinch(
    mut commands: Commands,
    mut damaged: MessageReader<EnemyDamagedEvent>,
    enemies: Query<Entity, With<crate::components::enemy::Enemy>>,
) {
    for ev in damaged.read() {
        if enemies.contains(ev.entity) {
            commands.entity(ev.entity).insert(Flinch {
                timer: FLINCH_SECONDS,
            });
        }
    }
}

fn tick_flinch(
    time: Res<Time>,
    mut commands: Commands,
    mut flinching: Query<(Entity, &mut Flinch)>,
) {
    let dt = time.delta_secs();
    for (entity, mut flinch) in flinching.iter_mut() {
        flinch.timer -= dt;
        if flinch.timer <= 0.0 {
            commands.entity(entity).remove::<Flinch>();
        }
    }
}

// ── Hit-flash orb ─────────────────────────────────────────────────────────────

#[derive(Component)]
struct HitFlash {
    timer: f32,
    max: f32,
}

#[derive(Resource)]
struct FeedbackAssets {
    flash_mesh: Handle<Mesh>,
    flash_mat: Handle<StandardMaterial>,
}

fn setup_feedback_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(FeedbackAssets {
        flash_mesh: meshes.add(Mesh::from(Sphere::new(0.55))),
        flash_mat: materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.98, 0.90, 0.85),
            emissive: LinearRgba::new(3.2, 3.0, 2.4, 1.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
    });
}

fn spawn_hit_flash(
    mut commands: Commands,
    assets: Option<Res<FeedbackAssets>>,
    mut damaged: MessageReader<EnemyDamagedEvent>,
    existing: Query<(), With<HitFlash>>,
) {
    let Some(assets) = assets else { return };
    // Budget guard for 4-player firefights.
    let mut budget = 24usize.saturating_sub(existing.iter().count());
    for ev in damaged.read() {
        if budget == 0 {
            break;
        }
        budget -= 1;
        commands.spawn((
            Mesh3d(assets.flash_mesh.clone()),
            MeshMaterial3d(assets.flash_mat.clone()),
            Transform::from_translation(ev.position + Vec3::Y * 1.0).with_scale(Vec3::splat(0.55)),
            HitFlash {
                timer: 0.12,
                max: 0.12,
            },
        ));
    }
}

fn tick_hit_flash(
    time: Res<Time>,
    mut commands: Commands,
    mut flashes: Query<(Entity, &mut HitFlash, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, mut flash, mut transform) in flashes.iter_mut() {
        flash.timer -= dt;
        if flash.timer <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        // Pop out then collapse.
        let t = 1.0 - flash.timer / flash.max;
        let scale = 0.55 + 0.9 * t;
        transform.scale = Vec3::splat(scale * (1.0 - t * t));
    }
}

// ── Death dissolve ────────────────────────────────────────────────────────────

#[derive(Component)]
struct DeathDissolve {
    initial_scale: Vec3,
    total: f32,
}

fn arm_death_dissolve(
    mut commands: Commands,
    newly_dead: Query<(Entity, &Transform, &DeadEnemy), Added<DeadEnemy>>,
) {
    for (entity, transform, dead) in newly_dead.iter() {
        commands.entity(entity).insert(DeathDissolve {
            initial_scale: transform.scale,
            total: dead.despawn_timer.max(0.2),
        });
    }
}

fn tick_death_dissolve(mut dying: Query<(&DeadEnemy, &DeathDissolve, &mut Transform)>) {
    for (dead, dissolve, mut transform) in dying.iter_mut() {
        let t = (dead.despawn_timer / dissolve.total).clamp(0.0, 1.0);
        // Hold briefly, then shrink out with an ease-in so the kill reads.
        let s = (t * 1.35).clamp(0.0, 1.0);
        transform.scale = dissolve.initial_scale * (s * s);
    }
}

// ── Damage numbers ────────────────────────────────────────────────────────────

#[derive(Component)]
struct DamageNumber {
    timer: f32,
    rise: f32,
    base_pos: Vec2,
}

const DAMAGE_NUMBER_LIFETIME: f32 = 0.7;
const DAMAGE_NUMBER_CAP: usize = 32;

fn spawn_damage_numbers(
    mut commands: Commands,
    mut damaged: MessageReader<EnemyDamagedEvent>,
    settings: Res<GameSettings>,
    cameras: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    windows: Query<&Window>,
    existing: Query<(), With<DamageNumber>>,
) {
    if !settings.show_damage_numbers {
        damaged.clear();
        return;
    }
    let scale_factor = windows
        .iter()
        .next()
        .map(|w| w.scale_factor())
        .unwrap_or(1.0);
    let mut live = existing.iter().count();
    for ev in damaged.read() {
        for (camera, cam_transform) in cameras.iter() {
            if live >= DAMAGE_NUMBER_CAP {
                return;
            }
            if !camera.is_active {
                continue;
            }
            let Ok(viewport_pos) =
                camera.world_to_viewport(cam_transform, ev.position + Vec3::Y * 2.1)
            else {
                continue;
            };
            // world_to_viewport is viewport-local; offset into window space so
            // each split-screen quadrant places its own copy correctly.
            let offset = camera
                .viewport
                .as_ref()
                .map(|v| v.physical_position.as_vec2() / scale_factor)
                .unwrap_or(Vec2::ZERO);
            let pos = viewport_pos + offset;
            let magnitude = ev.damage.max(1.0);
            let font_px = (13.0 + magnitude.sqrt() * 1.6).min(26.0);
            commands.spawn((
                DamageNumber {
                    timer: DAMAGE_NUMBER_LIFETIME,
                    rise: 34.0,
                    base_pos: pos,
                },
                Text::new(format!("{}", magnitude.round() as i64)),
                TextFont {
                    font_size: FontSize::Px(font_px),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.86, 0.30)),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(pos.x),
                    top: Val::Px(pos.y),
                    ..default()
                },
                GlobalZIndex(900),
            ));
            live += 1;
        }
    }
}

fn tick_damage_numbers(
    time: Res<Time>,
    settings: Res<GameSettings>,
    mut commands: Commands,
    mut numbers: Query<(Entity, &mut DamageNumber, &mut Node, &mut TextColor)>,
) {
    let dt = time.delta_secs();
    for (entity, mut number, mut node, mut color) in numbers.iter_mut() {
        number.timer -= dt;
        if number.timer <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let t = 1.0 - number.timer / DAMAGE_NUMBER_LIFETIME;
        let rise = if settings.reduced_ui_motion {
            0.0
        } else {
            number.rise
        };
        node.top = Val::Px(number.base_pos.y - rise * t);
        color.0 = color.0.with_alpha((1.0 - t * t).clamp(0.0, 1.0));
    }
}

fn cleanup_damage_numbers(mut commands: Commands, numbers: Query<Entity, With<DamageNumber>>) {
    for entity in numbers.iter() {
        commands.entity(entity).despawn();
    }
}

// ── Outgoing-hit rumble ──────────────────────────────────────────────────────

fn outgoing_hit_rumble(
    mut damaged: MessageReader<EnemyDamagedEvent>,
    mut kills: MessageReader<EnemyKilledEvent>,
    players: Query<(&PlayerIndex, &Transform), With<Player>>,
) {
    // Proximity attribution: whoever is close to the impact feels it. Keeps
    // co-op fair without threading attacker identity through the damage path.
    let add = |position: Vec3, base: f32| {
        for (index, transform) in players.iter() {
            let dist = transform.translation.distance(position);
            let falloff = (1.0 - dist / 46.0).clamp(0.0, 1.0);
            if falloff <= 0.0 {
                continue;
            }
            let amount = base * falloff;
            trigger_player_rumble(index.0 as usize, amount);
        }
    };
    for ev in damaged.read() {
        add(ev.position, 0.09);
    }
    for ev in kills.read() {
        add(ev.position, 0.16);
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────────

pub struct CombatFeedbackPlugin;

impl Plugin for CombatFeedbackPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Playing), setup_feedback_assets)
            .add_systems(OnExit(AppState::Playing), cleanup_damage_numbers)
            .add_systems(
                Update,
                (
                    apply_flinch,
                    tick_flinch,
                    spawn_hit_flash,
                    tick_hit_flash,
                    arm_death_dissolve,
                    tick_death_dissolve,
                    spawn_damage_numbers,
                    tick_damage_numbers,
                    outgoing_hit_rumble,
                )
                    .run_if(in_state(AppState::Playing)),
            );
    }
}
