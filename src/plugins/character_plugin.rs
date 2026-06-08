use bevy::prelude::*;
use std::collections::HashMap;

use crate::components::character::{
    CartoonAnimator, CartoonCharacter, CartoonPart, CartoonPartKind, CartoonPose,
};
use crate::components::player::{
    EdgeGrabState, JetpackState, PlayerMovement, PlayerState, PlayerStateMachine,
};
use crate::state::AppState;

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            cartoon_animation_system
                .run_if(in_state(AppState::Playing).or(in_state(AppState::CharacterDesign))),
        );
    }
}

#[derive(Clone, Copy)]
struct PoseSample {
    pose: CartoonPose,
    phase: f32,
    speed: f32,
    stride: f32,
    agility: f32,
    vertical_velocity: f32,
    wall_clasp_time: f32,
}

fn cartoon_animation_system(
    time: Res<Time>,
    mut roots: Query<
        (
            Entity,
            &Transform,
            &mut CartoonAnimator,
            Option<&CartoonCharacter>,
            Option<&PlayerMovement>,
            Option<&EdgeGrabState>,
            Option<&PlayerStateMachine>,
            Option<&JetpackState>,
        ),
        Without<CartoonPart>,
    >,
    mut parts: Query<(&CartoonPart, &mut Transform), Without<CartoonAnimator>>,
) {
    let dt = time.delta_secs();
    let mut samples = HashMap::new();

    for (entity, transform, mut animator, character, movement, edge_grab, state, jetpack) in
        roots.iter_mut()
    {
        let delta = transform.translation - animator.last_position;
        let raw_speed = delta.with_y(0.0).length() / dt.max(0.001);
        let blend = 1.0 - (-dt * 14.0).exp();
        animator.speed += (raw_speed - animator.speed) * blend;
        if animator.speed < 0.04 {
            animator.speed = 0.0;
        }
        animator.last_position = transform.translation;

        let current_state = state.map(|s| s.current);
        let vertical_velocity = movement.map(|m| m.velocity.y).unwrap_or(0.0);
        animator.pose = if edge_grab.map(|e| e.is_hanging).unwrap_or(false) {
            CartoonPose::Hang
        } else if current_state == Some(PlayerState::WallSliding) {
            CartoonPose::WallSlide
        } else if jetpack.map(|j| j.is_active).unwrap_or(false) {
            CartoonPose::Fly
        } else if movement.map(|m| !m.is_grounded).unwrap_or(false) {
            if vertical_velocity < -0.12 {
                CartoonPose::Fall
            } else {
                CartoonPose::Jump
            }
        } else if animator.speed > 4.5 {
            CartoonPose::Run
        } else if animator.speed > 0.06 {
            CartoonPose::Walk
        } else {
            CartoonPose::Idle
        };

        let stride = character.map(|c| c.stride).unwrap_or(1.0).max(0.45);
        let agility = character.map(|c| c.agility).unwrap_or(1.0).max(0.45);
        let phase_rate = match animator.pose {
            CartoonPose::Idle => 2.0,
            CartoonPose::Walk => (5.0 + animator.speed * 8.0 / stride).clamp(5.0, 13.0),
            CartoonPose::Run => (10.0 + animator.speed * 5.0 / stride).clamp(10.0, 18.0),
            CartoonPose::Jump => 2.5,
            CartoonPose::Fall => 3.6,
            CartoonPose::Fly => 8.0,
            CartoonPose::WallSlide => 4.8,
            CartoonPose::Hang => 1.4,
        } * (0.88 + agility * 0.12);
        animator.phase += dt * phase_rate;

        samples.insert(
            entity,
            PoseSample {
                pose: animator.pose,
                phase: animator.phase,
                speed: animator.speed,
                stride,
                agility,
                vertical_velocity,
                wall_clasp_time: edge_grab.map(|e| e.wall_clasp_timer).unwrap_or(0.0),
            },
        );
    }

    for (part, mut transform) in parts.iter_mut() {
        let Some(sample) = samples.get(&part.root) else {
            continue;
        };
        apply_part_pose(part, &mut transform, *sample);
    }
}

fn apply_part_pose(part: &CartoonPart, transform: &mut Transform, sample: PoseSample) {
    transform.translation = part.base_translation;
    transform.rotation = part.base_rotation;
    transform.scale = part.base_scale;

    let wave = sample.phase.sin();
    let step = (sample.phase * 1.1).sin();
    let walk_amount = (sample.speed * (2.8 / sample.stride)).clamp(0.0, 1.0);
    let swing = sample.agility.clamp(0.72, 1.28);

    // Face parts follow the Head's animation deltas exactly.
    let is_face = matches!(
        part.kind,
        CartoonPartKind::LeftEye
            | CartoonPartKind::RightEye
            | CartoonPartKind::LeftEyebrow
            | CartoonPartKind::RightEyebrow
            | CartoonPartKind::Nose
            | CartoonPartKind::Mouth
            | CartoonPartKind::Visor
    );

    // Body-anchored accessories (not cape — handled separately for per-panel flap).
    let is_body_acc = matches!(
        part.kind,
        CartoonPartKind::StarBadge
            | CartoonPartKind::ShoulderPadLeft
            | CartoonPartKind::ShoulderPadRight
            | CartoonPartKind::SpineRidge
            | CartoonPartKind::Belt
    );

    // Cape: flap magnitude increases with distance from shoulders (lower = more).
    let cape_flap = if part.kind == CartoonPartKind::Cape {
        let y = part.base_translation.y;
        if y > 0.15 {
            0.03 // collar — almost rigid
        } else if y > -0.25 {
            0.10 // upper panel
        } else if y > -0.55 {
            0.24 // mid panel
        } else {
            0.42 // tip flap
        }
    } else {
        0.0
    };
    let is_cape = cape_flap > 0.0;

    match sample.pose {
        // ── Idle ─────────────────────────────────────────────────────────────
        CartoonPose::Idle => {
            match part.kind {
                CartoonPartKind::Body => {
                    transform.translation.y += wave * 0.025;
                    transform.scale.y *= 1.0 + wave * 0.012; // gentle breath
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.translation.y += wave * 0.025;
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.rotation *= Quat::from_rotation_z(-0.08 + wave * 0.04);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *= Quat::from_rotation_z(0.08 - wave * 0.04);
                }
                CartoonPartKind::Shadow => {
                    transform.scale *=
                        Vec3::new(1.0 + wave.abs() * 0.03, 1.0, 1.0 + wave.abs() * 0.03);
                }
                _ => {}
            }
            if is_face {
                transform.translation.y += wave * 0.025;
            }
            if is_body_acc {
                transform.translation.y += wave * 0.025;
            }
            if is_cape {
                transform.translation.y += wave * 0.018;
                transform.rotation *= Quat::from_rotation_x(wave * 0.04 * cape_flap);
            }
        }

        // ── Walk ─────────────────────────────────────────────────────────────
        CartoonPose::Walk => {
            match part.kind {
                CartoonPartKind::Body => {
                    transform.translation.y += wave.abs() * 0.07;
                    transform.rotation *= Quat::from_rotation_z(step * 0.035);
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.translation.y += wave.abs() * 0.05;
                    transform.rotation *= Quat::from_rotation_y(step * 0.06);
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.rotation *= Quat::from_rotation_x(step * 0.85 * walk_amount * swing);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *= Quat::from_rotation_x(-step * 0.85 * walk_amount * swing);
                }
                CartoonPartKind::LeftLeg
                | CartoonPartKind::LeftFoot
                | CartoonPartKind::LeftBoot => {
                    transform.rotation *= Quat::from_rotation_x(-step * 0.65 * walk_amount * swing);
                }
                CartoonPartKind::RightLeg
                | CartoonPartKind::RightFoot
                | CartoonPartKind::RightBoot => {
                    transform.rotation *= Quat::from_rotation_x(step * 0.65 * walk_amount * swing);
                }
                CartoonPartKind::Tail => {
                    transform.rotation *= Quat::from_rotation_y(step * 0.25);
                }
                CartoonPartKind::Shadow => {
                    transform.scale *= Vec3::new(1.05, 1.0, 0.95);
                }
                _ => {}
            }
            if is_face {
                transform.translation.y += wave.abs() * 0.05;
                transform.rotation *= Quat::from_rotation_y(step * 0.06);
            }
            if is_body_acc {
                transform.translation.y += wave.abs() * 0.07;
                transform.rotation *= Quat::from_rotation_z(step * 0.035);
            }
            if is_cape {
                // Each panel sweeps back proportional to its cape_flap weight.
                transform.rotation *= Quat::from_rotation_x(walk_amount * cape_flap);
                transform.translation.y += wave.abs() * 0.06;
                transform.rotation *= Quat::from_rotation_z(step * 0.025);
            }
        }

        // ── Run ──────────────────────────────────────────────────────────────
        CartoonPose::Run => {
            match part.kind {
                CartoonPartKind::Body => {
                    transform.translation.y += wave.abs() * 0.10;
                    transform.rotation *=
                        Quat::from_rotation_z(step * 0.060) * Quat::from_rotation_x(-0.10);
                    // forward lean
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.translation.y += wave.abs() * 0.07;
                    transform.rotation *=
                        Quat::from_rotation_y(step * 0.10) * Quat::from_rotation_x(-0.08);
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.rotation *= Quat::from_rotation_x(step * 1.35 * swing);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *= Quat::from_rotation_x(-step * 1.35 * swing);
                }
                CartoonPartKind::LeftLeg
                | CartoonPartKind::LeftFoot
                | CartoonPartKind::LeftBoot => {
                    transform.rotation *= Quat::from_rotation_x(-step * 0.95 * swing);
                }
                CartoonPartKind::RightLeg
                | CartoonPartKind::RightFoot
                | CartoonPartKind::RightBoot => {
                    transform.rotation *= Quat::from_rotation_x(step * 0.95 * swing);
                }
                CartoonPartKind::Tail => {
                    transform.rotation *= Quat::from_rotation_y(step * 0.42);
                }
                CartoonPartKind::Shadow => {
                    transform.scale *= Vec3::new(1.08, 1.0, 0.90);
                }
                _ => {}
            }
            if is_face {
                transform.translation.y += wave.abs() * 0.07;
                transform.rotation *=
                    Quat::from_rotation_y(step * 0.10) * Quat::from_rotation_x(-0.08);
            }
            if is_body_acc {
                transform.translation.y += wave.abs() * 0.10;
                transform.rotation *=
                    Quat::from_rotation_z(step * 0.060) * Quat::from_rotation_x(-0.10);
            }
            if is_cape {
                // Cape streams fully back at run speed.
                transform.rotation *= Quat::from_rotation_x(cape_flap * 1.0);
                transform.translation.y += wave.abs() * 0.09;
                transform.rotation *= Quat::from_rotation_z(step * 0.045);
            }
        }

        // ── Jump ─────────────────────────────────────────────────────────────
        CartoonPose::Jump => {
            match part.kind {
                CartoonPartKind::Body => {
                    transform.translation.y += 0.10;
                    transform.rotation *= Quat::from_rotation_x(-0.12);
                    transform.scale.y *= 0.93; // squash on launch
                    transform.scale.x *= 1.04;
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.translation.y += 0.12;
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.rotation *= Quat::from_rotation_x(-1.10);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *= Quat::from_rotation_x(-0.90);
                }
                CartoonPartKind::LeftLeg => {
                    transform.rotation *= Quat::from_rotation_x(0.45);
                }
                CartoonPartKind::RightLeg => {
                    transform.rotation *= Quat::from_rotation_x(-0.35);
                }
                CartoonPartKind::Mouth => {
                    // Slight surprised-open stretch in the air.
                    transform.scale.x *= 1.18;
                    transform.scale.y *= 1.12;
                }
                CartoonPartKind::Tail => {
                    transform.rotation *= Quat::from_rotation_x(0.45 + wave * 0.10);
                }
                CartoonPartKind::Shadow => {
                    transform.scale *= Vec3::new(0.82, 1.0, 0.82);
                }
                _ => {}
            }
            if is_face {
                transform.translation.y += 0.12;
            }
            if is_body_acc {
                transform.translation.y += 0.10;
                transform.rotation *= Quat::from_rotation_x(-0.12);
            }
            if is_cape {
                // Billows backward during airtime.
                transform.rotation *= Quat::from_rotation_x(cape_flap * 0.9);
            }
        }

        // ── Fall ─────────────────────────────────────────────────────────────
        CartoonPose::Fall => {
            let fall_pull = (-sample.vertical_velocity).clamp(0.0, 2.2);
            match part.kind {
                CartoonPartKind::Body => {
                    transform.translation.y -= 0.04;
                    transform.rotation *= Quat::from_rotation_x(0.08 + fall_pull * 0.04);
                    transform.scale.y *= 1.0 + fall_pull * 0.025;
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.translation.y -= 0.02;
                    transform.rotation *= Quat::from_rotation_x(0.06);
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.rotation *=
                        Quat::from_rotation_z(-0.95) * Quat::from_rotation_x(-0.35 + wave * 0.08);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *=
                        Quat::from_rotation_z(0.95) * Quat::from_rotation_x(-0.35 - wave * 0.08);
                }
                CartoonPartKind::LeftLeg
                | CartoonPartKind::LeftFoot
                | CartoonPartKind::LeftBoot => {
                    transform.rotation *= Quat::from_rotation_x(0.22 + wave * 0.08);
                }
                CartoonPartKind::RightLeg
                | CartoonPartKind::RightFoot
                | CartoonPartKind::RightBoot => {
                    transform.rotation *= Quat::from_rotation_x(-0.18 - wave * 0.08);
                }
                CartoonPartKind::Shadow => {
                    transform.scale *= Vec3::new(0.76, 1.0, 0.76);
                }
                _ => {}
            }
            if is_face {
                transform.translation.y -= 0.02;
            }
            if is_body_acc {
                transform.translation.y -= 0.04;
                transform.rotation *= Quat::from_rotation_x(0.08 + fall_pull * 0.04);
            }
            if is_cape {
                transform.rotation *= Quat::from_rotation_x(cape_flap * (1.1 + fall_pull * 0.18));
                transform.rotation *= Quat::from_rotation_z(wave * 0.06 * cape_flap);
            }
        }

        // ── Fly ──────────────────────────────────────────────────────────────
        CartoonPose::Fly => {
            let engine_pulse = wave * 0.04;
            match part.kind {
                CartoonPartKind::Body => {
                    transform.rotation *= Quat::from_rotation_x(-0.36);
                    transform.translation.y += 0.06 + engine_pulse;
                    transform.scale.z *= 1.04;
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.rotation *= Quat::from_rotation_x(-0.22);
                    transform.translation.y += 0.08 + engine_pulse;
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.rotation *=
                        Quat::from_rotation_x(0.28) * Quat::from_rotation_z(-0.72 + wave * 0.05);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *=
                        Quat::from_rotation_x(0.28) * Quat::from_rotation_z(0.72 - wave * 0.05);
                }
                CartoonPartKind::LeftLeg
                | CartoonPartKind::LeftFoot
                | CartoonPartKind::LeftBoot => {
                    transform.rotation *= Quat::from_rotation_x(0.34 + wave * 0.05);
                }
                CartoonPartKind::RightLeg
                | CartoonPartKind::RightFoot
                | CartoonPartKind::RightBoot => {
                    transform.rotation *= Quat::from_rotation_x(0.26 - wave * 0.05);
                }
                CartoonPartKind::Shadow => {
                    transform.scale *= Vec3::new(0.66, 1.0, 0.66);
                }
                _ => {}
            }
            if is_face {
                transform.rotation *= Quat::from_rotation_x(-0.22);
                transform.translation.y += 0.08 + engine_pulse;
            }
            if is_body_acc {
                transform.rotation *= Quat::from_rotation_x(-0.36);
                transform.translation.y += 0.06 + engine_pulse;
            }
            if is_cape {
                transform.rotation *= Quat::from_rotation_x(cape_flap * 1.35);
                transform.rotation *= Quat::from_rotation_z(wave * 0.08 * cape_flap);
            }
        }

        // ── Wall slide ───────────────────────────────────────────────────────
        CartoonPose::WallSlide => {
            let scrape = (sample.wall_clasp_time * 18.0).sin() * 0.035;
            match part.kind {
                CartoonPartKind::Body => {
                    transform.translation.y -= 0.08;
                    transform.translation.x += scrape;
                    transform.rotation *=
                        Quat::from_rotation_z(-0.16) * Quat::from_rotation_x(0.10);
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.translation.y -= 0.03;
                    transform.translation.x += scrape * 0.6;
                    transform.rotation *= Quat::from_rotation_z(-0.10);
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    // One hand stays high and planted against the wall.
                    transform.translation.y += 0.34;
                    transform.translation.x -= 0.08;
                    transform.rotation *=
                        Quat::from_rotation_x(-1.62) * Quat::from_rotation_z(-0.36 + scrape);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    // Free arm trails for balance so the pose reads as one-hand sliding.
                    transform.translation.y -= 0.04;
                    transform.rotation *=
                        Quat::from_rotation_x(-0.32) * Quat::from_rotation_z(0.58 - scrape);
                }
                CartoonPartKind::LeftLeg
                | CartoonPartKind::LeftFoot
                | CartoonPartKind::LeftBoot => {
                    transform.rotation *=
                        Quat::from_rotation_x(0.36) * Quat::from_rotation_z(-0.18);
                }
                CartoonPartKind::RightLeg
                | CartoonPartKind::RightFoot
                | CartoonPartKind::RightBoot => {
                    transform.rotation *=
                        Quat::from_rotation_x(-0.44) * Quat::from_rotation_z(0.24);
                }
                CartoonPartKind::Mouth => {
                    transform.scale.x *= 1.10;
                }
                CartoonPartKind::Shadow => {
                    transform.scale *= Vec3::new(0.70, 1.0, 0.70);
                }
                _ => {}
            }
            if is_face {
                transform.translation.y -= 0.03;
                transform.rotation *= Quat::from_rotation_z(-0.10);
            }
            if is_body_acc {
                transform.translation.y -= 0.08;
                transform.rotation *= Quat::from_rotation_z(-0.16) * Quat::from_rotation_x(0.10);
            }
            if is_cape {
                transform.rotation *= Quat::from_rotation_x(cape_flap * 0.25);
                transform.rotation *= Quat::from_rotation_z(-0.18 * cape_flap + wave * 0.04);
            }
        }

        // ── Hang ─────────────────────────────────────────────────────────────
        CartoonPose::Hang => {
            match part.kind {
                CartoonPartKind::Body => {
                    transform.translation.y -= 0.12;
                    transform.rotation *= Quat::from_rotation_x(0.08);
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.translation.y -= 0.06;
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.translation.y += 0.32;
                    transform.rotation *= Quat::from_rotation_x(-1.7);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.translation.y += 0.32;
                    transform.rotation *= Quat::from_rotation_x(-1.7);
                }
                CartoonPartKind::LeftLeg => {
                    transform.rotation *= Quat::from_rotation_x(0.35 + wave * 0.08);
                }
                CartoonPartKind::RightLeg => {
                    transform.rotation *= Quat::from_rotation_x(-0.35 - wave * 0.08);
                }
                CartoonPartKind::Shadow => {
                    transform.scale *= Vec3::new(0.72, 1.0, 0.72);
                }
                _ => {}
            }
            if is_face {
                transform.translation.y -= 0.06;
            }
            if is_body_acc {
                transform.translation.y -= 0.12;
                transform.rotation *= Quat::from_rotation_x(0.08);
            }
            if is_cape {
                // Hangs naturally downward (gravity effect).
                transform.rotation *= Quat::from_rotation_x(-cape_flap * 0.5 + wave * 0.03);
            }
        }
    }
}
