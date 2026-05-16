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
    pose: CartoonPose,
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
        } else if animator.speed > 0.06 {
            CartoonPose::Walk
        } else {
            CartoonPose::Idle
        };

        let phase_rate = match animator.pose {
            CartoonPose::Idle => 2.0,
            CartoonPose::Walk => (5.0 + animator.speed * 8.0).clamp(5.0, 13.0),
            CartoonPose::Jump => 2.5,
            CartoonPose::Hang => 1.4,
        };
        animator.phase += dt * phase_rate;

        samples.insert(
            entity,
            PoseSample {
                pose: animator.pose,
                phase: animator.phase,
                speed: animator.speed,
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
    let walk_amount = (sample.speed * 2.8).clamp(0.0, 1.0);

    // Face parts (eyes, eyebrows, nose, visor) are positioned relative to
    // the head's base offset and must receive the same animation deltas as Head.
    let is_face = matches!(
        part.kind,
        CartoonPartKind::LeftEye
            | CartoonPartKind::RightEye
            | CartoonPartKind::LeftEyebrow
            | CartoonPartKind::RightEyebrow
            | CartoonPartKind::Nose
            | CartoonPartKind::Visor
    );
    // Body-anchored accessories follow Body animation deltas.
    let is_body_acc = matches!(
        part.kind,
        CartoonPartKind::StarBadge
            | CartoonPartKind::ShoulderPadLeft
            | CartoonPartKind::ShoulderPadRight
            | CartoonPartKind::Cape
            | CartoonPartKind::SpineRidge
    );

    match sample.pose {
        CartoonPose::Idle => {
            match part.kind {
                CartoonPartKind::Body => {
                    transform.translation.y += wave * 0.025;
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
        }
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
                    transform.rotation *= Quat::from_rotation_x(step * 0.85 * walk_amount);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *= Quat::from_rotation_x(-step * 0.85 * walk_amount);
                }
                CartoonPartKind::LeftLeg | CartoonPartKind::LeftFoot | CartoonPartKind::LeftBoot => {
                    transform.rotation *= Quat::from_rotation_x(-step * 0.65 * walk_amount);
                }
                CartoonPartKind::RightLeg | CartoonPartKind::RightFoot | CartoonPartKind::RightBoot => {
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
                transform.rotation *= Quat::from_rotation_y(step * 0.06);
            }
            if is_body_acc {
                transform.translation.y += wave.abs() * 0.07;
                transform.rotation *= Quat::from_rotation_z(step * 0.035);
            }
        }
        CartoonPose::Jump => {
            match part.kind {
                CartoonPartKind::Body => {
                    transform.translation.y += 0.08;
                    transform.rotation *= Quat::from_rotation_x(-0.08);
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.translation.y += 0.10;
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.rotation *= Quat::from_rotation_x(-1.05);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *= Quat::from_rotation_x(-0.85);
                }
                CartoonPartKind::LeftLeg => {
                    transform.rotation *= Quat::from_rotation_x(0.45);
                }
                CartoonPartKind::RightLeg => {
                    transform.rotation *= Quat::from_rotation_x(-0.35);
                }
                CartoonPartKind::Tail => {
                    transform.rotation *= Quat::from_rotation_x(0.45 + wave * 0.1);
                }
                CartoonPartKind::Shadow => {
                    transform.scale *= Vec3::new(0.85, 1.0, 0.85);
                }
                _ => {}
            }
            if is_face {
                transform.translation.y += 0.10;
            }
            if is_body_acc {
                transform.translation.y += 0.08;
                transform.rotation *= Quat::from_rotation_x(-0.08);
            }
        }
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
        }
    }
}
