use bevy::prelude::*;
use std::collections::HashMap;

use crate::components::character::{CartoonAnimator, CartoonPart, CartoonPartKind, CartoonPose};
use crate::components::player::{EdgeGrabState, PlayerMovement};
use crate::state::AppState;

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            cartoon_animation_system.run_if(
                in_state(AppState::Playing).or(in_state(AppState::CharacterDesign)),
            ),
        );
    }
}

#[derive(Clone, Copy)]
struct PoseSample {
    pose:  CartoonPose,
    phase: f32,
    speed: f32,
}

fn cartoon_animation_system(
    time: Res<Time>,
    mut roots: Query<
        (
            Entity,
            &Transform,
            &mut CartoonAnimator,
            Option<&PlayerMovement>,
            Option<&EdgeGrabState>,
        ),
        Without<CartoonPart>,
    >,
    mut parts: Query<(&CartoonPart, &mut Transform), Without<CartoonAnimator>>,
) {
    let dt = time.delta_secs();
    let mut samples = HashMap::new();

    for (entity, transform, mut animator, movement, edge_grab) in roots.iter_mut() {
        let delta = transform.translation - animator.last_position;
        animator.speed = delta.with_y(0.0).length() / dt.max(0.001);
        animator.last_position = transform.translation;

        animator.pose = if edge_grab.map(|e| e.is_hanging).unwrap_or(false) {
            CartoonPose::Hang
        } else if movement.map(|m| !m.is_grounded).unwrap_or(false) {
            CartoonPose::Jump
        } else if animator.speed > 4.5 {
            CartoonPose::Run
        } else if animator.speed > 0.06 {
            CartoonPose::Walk
        } else {
            CartoonPose::Idle
        };

        let phase_rate = match animator.pose {
            CartoonPose::Idle => 2.0,
            CartoonPose::Walk => (5.0 + animator.speed * 8.0).clamp(5.0, 13.0),
            CartoonPose::Run  => (10.0 + animator.speed * 5.0).clamp(10.0, 18.0),
            CartoonPose::Jump => 2.5,
            CartoonPose::Hang => 1.4,
        };
        animator.phase += dt * phase_rate;

        samples.insert(
            entity,
            PoseSample {
                pose:  animator.pose,
                phase: animator.phase,
                speed: animator.speed,
            },
        );
    }

    for (part, mut transform) in parts.iter_mut() {
        let Some(sample) = samples.get(&part.root) else { continue; };
        apply_part_pose(part, &mut transform, *sample);
    }
}

fn apply_part_pose(part: &CartoonPart, transform: &mut Transform, sample: PoseSample) {
    transform.translation = part.base_translation;
    transform.rotation    = part.base_rotation;
    transform.scale       = part.base_scale;

    let wave        = sample.phase.sin();
    let step        = (sample.phase * 1.1).sin();
    let walk_amount = (sample.speed * 2.8).clamp(0.0, 1.0);

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
            0.03  // collar — almost rigid
        } else if y > -0.25 {
            0.10  // upper panel
        } else if y > -0.55 {
            0.24  // mid panel
        } else {
            0.42  // tip flap
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
                    transform.scale.y       *= 1.0 + wave * 0.012; // gentle breath
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
                    transform.scale *= Vec3::new(
                        1.0 + wave.abs() * 0.03,
                        1.0,
                        1.0 + wave.abs() * 0.03,
                    );
                }
                _ => {}
            }
            if is_face     { transform.translation.y += wave * 0.025; }
            if is_body_acc { transform.translation.y += wave * 0.025; }
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
                    transform.rotation      *= Quat::from_rotation_z(step * 0.035);
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.translation.y += wave.abs() * 0.05;
                    transform.rotation      *= Quat::from_rotation_y(step * 0.06);
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.rotation *= Quat::from_rotation_x(step * 0.85 * walk_amount);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *= Quat::from_rotation_x(-step * 0.85 * walk_amount);
                }
                CartoonPartKind::LeftLeg
                | CartoonPartKind::LeftFoot
                | CartoonPartKind::LeftBoot => {
                    transform.rotation *= Quat::from_rotation_x(-step * 0.65 * walk_amount);
                }
                CartoonPartKind::RightLeg
                | CartoonPartKind::RightFoot
                | CartoonPartKind::RightBoot => {
                    transform.rotation *= Quat::from_rotation_x(step * 0.65 * walk_amount);
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
                transform.rotation      *= Quat::from_rotation_y(step * 0.06);
            }
            if is_body_acc {
                transform.translation.y += wave.abs() * 0.07;
                transform.rotation      *= Quat::from_rotation_z(step * 0.035);
            }
            if is_cape {
                // Each panel sweeps back proportional to its cape_flap weight.
                transform.rotation      *= Quat::from_rotation_x(walk_amount * cape_flap);
                transform.translation.y += wave.abs() * 0.06;
                transform.rotation      *= Quat::from_rotation_z(step * 0.025);
            }
        }

        // ── Run ──────────────────────────────────────────────────────────────
        CartoonPose::Run => {
            match part.kind {
                CartoonPartKind::Body => {
                    transform.translation.y += wave.abs() * 0.10;
                    transform.rotation      *= Quat::from_rotation_z(step * 0.060)
                        * Quat::from_rotation_x(-0.10); // forward lean
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.translation.y += wave.abs() * 0.07;
                    transform.rotation      *= Quat::from_rotation_y(step * 0.10)
                        * Quat::from_rotation_x(-0.08);
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.rotation *= Quat::from_rotation_x(step * 1.35);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *= Quat::from_rotation_x(-step * 1.35);
                }
                CartoonPartKind::LeftLeg
                | CartoonPartKind::LeftFoot
                | CartoonPartKind::LeftBoot => {
                    transform.rotation *= Quat::from_rotation_x(-step * 0.95);
                }
                CartoonPartKind::RightLeg
                | CartoonPartKind::RightFoot
                | CartoonPartKind::RightBoot => {
                    transform.rotation *= Quat::from_rotation_x(step * 0.95);
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
                transform.rotation      *= Quat::from_rotation_y(step * 0.10)
                    * Quat::from_rotation_x(-0.08);
            }
            if is_body_acc {
                transform.translation.y += wave.abs() * 0.10;
                transform.rotation      *= Quat::from_rotation_z(step * 0.060)
                    * Quat::from_rotation_x(-0.10);
            }
            if is_cape {
                // Cape streams fully back at run speed.
                transform.rotation      *= Quat::from_rotation_x(cape_flap * 1.0);
                transform.translation.y += wave.abs() * 0.09;
                transform.rotation      *= Quat::from_rotation_z(step * 0.045);
            }
        }

        // ── Jump ─────────────────────────────────────────────────────────────
        CartoonPose::Jump => {
            match part.kind {
                CartoonPartKind::Body => {
                    transform.translation.y += 0.10;
                    transform.rotation      *= Quat::from_rotation_x(-0.12);
                    transform.scale.y       *= 0.93; // squash on launch
                    transform.scale.x       *= 1.04;
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
            if is_face     { transform.translation.y += 0.12; }
            if is_body_acc {
                transform.translation.y += 0.10;
                transform.rotation      *= Quat::from_rotation_x(-0.12);
            }
            if is_cape {
                // Billows backward during airtime.
                transform.rotation *= Quat::from_rotation_x(cape_flap * 0.9);
            }
        }

        // ── Hang ─────────────────────────────────────────────────────────────
        CartoonPose::Hang => {
            match part.kind {
                CartoonPartKind::Body => {
                    transform.translation.y -= 0.12;
                    transform.rotation      *= Quat::from_rotation_x(0.08);
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.translation.y -= 0.06;
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.translation.y += 0.32;
                    transform.rotation      *= Quat::from_rotation_x(-1.7);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.translation.y += 0.32;
                    transform.rotation      *= Quat::from_rotation_x(-1.7);
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
            if is_face     { transform.translation.y -= 0.06; }
            if is_body_acc {
                transform.translation.y -= 0.12;
                transform.rotation      *= Quat::from_rotation_x(0.08);
            }
            if is_cape {
                // Hangs naturally downward (gravity effect).
                transform.rotation *= Quat::from_rotation_x(-cape_flap * 0.5 + wave * 0.03);
            }
        }
    }
}
