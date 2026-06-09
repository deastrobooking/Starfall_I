use bevy::prelude::*;
use std::collections::HashMap;

use crate::character_parts::{
    spawn_robot_arms, spawn_robot_body, spawn_robot_legs, spawn_robot_shoulders, CharacterLoadout,
    CharacterPartStyle, CharacterVisualConfig, PartSlotTag,
};
use crate::components::character::{
    CartoonAnimator, CartoonCharacter, CartoonPart, CartoonPartKind, CartoonPose,
};
use crate::components::player::{
    DodgeState, EdgeGrabState, GrappleHookMode, GrappleHookState, JetpackState, ParryState,
    PlayerMovement, PlayerState, PlayerStateMachine,
};
use crate::components::weapon::{BeamSabre, MeleeCombo};
use crate::state::AppState;

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                cartoon_animation_system
                    .run_if(in_state(AppState::Playing).or(in_state(AppState::CharacterDesign))),
                swap_character_parts,
            ),
        );
    }
}

/// Watches for `CharacterLoadout` changes and swaps visual parts for any slot
/// whose style changed to `RobotMechanical` (or back to `HumanoidClothing` is
/// handled by despawn only — a full humanoid re-spawn is a future extension).
fn swap_character_parts(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    changed: Query<(Entity, &CharacterLoadout, &CharacterVisualConfig), Changed<CharacterLoadout>>,
    parts: Query<(Entity, &CartoonPart, &PartSlotTag)>,
) {
    for (root, loadout, visual_cfg) in &changed {
        let lift = visual_cfg.visual_ground_lift;

        let slots = [
            (PartSlotTag::Body, loadout.body),
            (PartSlotTag::Arms, loadout.arms),
            (PartSlotTag::Legs, loadout.legs),
            (PartSlotTag::Shoulders, loadout.shoulders),
        ];

        for (slot_tag, style) in slots {
            // Despawn all existing parts for this slot on this root.
            for (entity, part, tag) in &parts {
                if part.root == root && *tag == slot_tag {
                    commands.entity(entity).try_despawn();
                }
            }

            // Spawn the new style.
            match style {
                CharacterPartStyle::RobotMechanical => match slot_tag {
                    PartSlotTag::Body => {
                        spawn_robot_body(
                            &mut commands,
                            &mut meshes,
                            &mut materials,
                            root,
                            visual_cfg,
                            lift,
                        );
                    }
                    PartSlotTag::Arms => {
                        spawn_robot_arms(
                            &mut commands,
                            &mut meshes,
                            &mut materials,
                            root,
                            visual_cfg,
                            lift,
                        );
                    }
                    PartSlotTag::Legs => {
                        spawn_robot_legs(
                            &mut commands,
                            &mut meshes,
                            &mut materials,
                            root,
                            visual_cfg,
                            lift,
                        );
                    }
                    PartSlotTag::Shoulders => {
                        spawn_robot_shoulders(
                            &mut commands,
                            &mut meshes,
                            &mut materials,
                            root,
                            visual_cfg,
                            lift,
                        );
                    }
                    _ => {}
                },
                CharacterPartStyle::HumanoidClothing => {
                    // Humanoid parts were just despawned; full re-spawn requires
                    // calling attach_cartoon_character again from outside this system.
                    // The caller (e.g. chassis editor) is responsible for that.
                }
            }
        }
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

#[derive(Clone, Copy, Default)]
struct PoseInput {
    state: Option<PlayerState>,
    grounded: bool,
    horizontal_speed: f32,
    vertical_velocity: f32,
    hanging: bool,
    wall_sliding: bool,
    jetpack_active: bool,
    grapple_mode: Option<GrappleHookMode>,
    grapple_pose: bool,
    sabre_slashing: bool,
    melee_attacking: bool,
    dodging: bool,
    parrying: bool,
}

fn select_cartoon_pose(input: PoseInput) -> CartoonPose {
    if input.state == Some(PlayerState::Dead) {
        return CartoonPose::Death;
    }
    if input.state == Some(PlayerState::Stunned) {
        return CartoonPose::Stun;
    }
    if input.sabre_slashing {
        return CartoonPose::SabreSlash;
    }
    if input.state == Some(PlayerState::Attacking) || input.melee_attacking {
        return CartoonPose::Attack;
    }
    if input.parrying {
        return CartoonPose::Parry;
    }
    if input.dodging || input.state == Some(PlayerState::Dodging) {
        return CartoonPose::Roll;
    }

    match input.grapple_mode {
        Some(GrappleHookMode::Swinging) => return CartoonPose::Swing,
        Some(GrappleHookMode::Zipping) => return CartoonPose::Zip,
        Some(GrappleHookMode::Windup)
        | Some(GrappleHookMode::Searching)
        | Some(GrappleHookMode::Recovering)
            if input.grapple_pose || input.state == Some(PlayerState::Grappling) =>
        {
            return CartoonPose::Grapple;
        }
        _ => {}
    }

    if input.hanging {
        return CartoonPose::Hang;
    }
    if input.wall_sliding || input.state == Some(PlayerState::WallSliding) {
        return CartoonPose::WallSlide;
    }
    if input.jetpack_active {
        if input.vertical_velocity.abs() < 0.06 {
            return CartoonPose::Hover;
        }
        return CartoonPose::Fly;
    }
    if !input.grounded {
        if input.vertical_velocity < -0.18 && input.horizontal_speed > 0.30 {
            return CartoonPose::Glide;
        }
        if input.vertical_velocity < -0.12 {
            return CartoonPose::Fall;
        }
        return CartoonPose::Jump;
    }
    if input.state == Some(PlayerState::Sprinting) {
        return CartoonPose::Sprint;
    }
    if input.horizontal_speed > 28.0 {
        return CartoonPose::Run;
    }
    if input.horizontal_speed > 2.0 {
        return CartoonPose::Walk;
    }
    CartoonPose::Idle
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
            Option<&GrappleHookState>,
            Option<&DodgeState>,
            Option<&ParryState>,
            Option<&BeamSabre>,
            Option<&MeleeCombo>,
        ),
        Without<CartoonPart>,
    >,
    mut parts: Query<(&CartoonPart, &mut Transform), Without<CartoonAnimator>>,
) {
    let dt = time.delta_secs();
    let mut samples = HashMap::new();

    for (
        entity,
        transform,
        mut animator,
        character,
        movement,
        edge_grab,
        state,
        jetpack,
        grapple,
        dodge,
        parry,
        sabre,
        melee,
    ) in roots.iter_mut()
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
        let horizontal_speed = movement
            .map(|m| m.ground_velocity.length())
            .unwrap_or(animator.speed);
        let sabre_slashing = sabre
            .map(|s| s.unlocked && s.active && s.is_slashing)
            .unwrap_or(false);
        let melee_attacking = melee.map(|m| m.is_attacking).unwrap_or(false);
        animator.pose = select_cartoon_pose(PoseInput {
            state: current_state,
            grounded: movement.map(|m| m.is_grounded).unwrap_or(true),
            horizontal_speed,
            vertical_velocity,
            hanging: edge_grab.map(|e| e.is_hanging).unwrap_or(false),
            wall_sliding: current_state == Some(PlayerState::WallSliding),
            jetpack_active: jetpack.map(|j| j.is_active).unwrap_or(false),
            grapple_mode: grapple.map(|g| g.mode),
            grapple_pose: grapple.map(|g| g.wants_animation_pose()).unwrap_or(false),
            sabre_slashing,
            melee_attacking,
            dodging: dodge.map(|d| d.is_dodging).unwrap_or(false),
            parrying: parry.map(|p| p.is_parrying).unwrap_or(false),
        });

        let stride = character.map(|c| c.stride).unwrap_or(1.0).max(0.45);
        let agility = character.map(|c| c.agility).unwrap_or(1.0).max(0.45);
        let phase_rate: f32 = match animator.pose {
            CartoonPose::Idle => 2.0,
            CartoonPose::Walk => (5.0 + animator.speed * 8.0 / stride).clamp(5.0, 13.0),
            CartoonPose::Run => (10.0 + animator.speed * 5.0 / stride).clamp(10.0, 18.0),
            CartoonPose::Sprint => (14.0 + animator.speed * 5.5 / stride).clamp(14.0, 22.0),
            CartoonPose::Jump => 2.5,
            CartoonPose::Fall => 3.6,
            CartoonPose::Fly => 8.0,
            CartoonPose::Glide => 5.0,
            CartoonPose::Hover => 6.5,
            CartoonPose::AirDash => 14.0,
            CartoonPose::WallSlide => 4.8,
            CartoonPose::Mantle => 6.0,
            CartoonPose::Grapple => 10.0,
            CartoonPose::Swing => 9.0,
            CartoonPose::Zip => 13.5,
            CartoonPose::Roll => 12.0,
            CartoonPose::Parry => 7.2,
            CartoonPose::Attack => 8.5,
            CartoonPose::SabreSlash => 12.0,
            CartoonPose::HardLand => 7.5,
            CartoonPose::Stun => 3.0,
            CartoonPose::Death => 1.0,
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
        CartoonPose::Sprint => {
            match part.kind {
                CartoonPartKind::Body => {
                    transform.translation.y += wave.abs() * 0.12;
                    transform.rotation *=
                        Quat::from_rotation_z(step * 0.08) * Quat::from_rotation_x(-0.22);
                    transform.scale.z *= 1.04;
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.translation.y += wave.abs() * 0.08;
                    transform.rotation *=
                        Quat::from_rotation_y(step * 0.13) * Quat::from_rotation_x(-0.14);
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.rotation *=
                        Quat::from_rotation_x(step * 1.58 * swing) * Quat::from_rotation_z(-0.10);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *=
                        Quat::from_rotation_x(-step * 1.58 * swing) * Quat::from_rotation_z(0.10);
                }
                CartoonPartKind::LeftLeg
                | CartoonPartKind::LeftFoot
                | CartoonPartKind::LeftBoot => {
                    transform.rotation *= Quat::from_rotation_x(-step * 1.18 * swing);
                }
                CartoonPartKind::RightLeg
                | CartoonPartKind::RightFoot
                | CartoonPartKind::RightBoot => {
                    transform.rotation *= Quat::from_rotation_x(step * 1.18 * swing);
                }
                CartoonPartKind::Shadow => {
                    transform.scale *= Vec3::new(1.18, 1.0, 0.80);
                }
                _ => {}
            }
            if is_face {
                transform.translation.y += wave.abs() * 0.08;
                transform.rotation *=
                    Quat::from_rotation_y(step * 0.13) * Quat::from_rotation_x(-0.14);
            }
            if is_body_acc {
                transform.translation.y += wave.abs() * 0.12;
                transform.rotation *=
                    Quat::from_rotation_z(step * 0.08) * Quat::from_rotation_x(-0.22);
            }
            if is_cape {
                transform.rotation *= Quat::from_rotation_x(cape_flap * 1.45);
                transform.rotation *= Quat::from_rotation_z(step * 0.06 * cape_flap);
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
        CartoonPose::Glide | CartoonPose::Hover | CartoonPose::AirDash => {
            let is_hover = sample.pose == CartoonPose::Hover;
            let is_dash = sample.pose == CartoonPose::AirDash;
            let pitch = if is_hover {
                -0.06
            } else if is_dash {
                -0.48
            } else {
                -0.22
            };
            match part.kind {
                CartoonPartKind::Body => {
                    transform.rotation *= Quat::from_rotation_x(pitch);
                    transform.translation.y += if is_hover { wave * 0.055 } else { 0.04 };
                    if is_dash {
                        transform.scale.z *= 1.10;
                    }
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.rotation *= Quat::from_rotation_x(pitch * 0.55);
                    transform.translation.y += if is_hover { 0.06 + wave * 0.04 } else { 0.05 };
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.rotation *= if is_hover {
                        Quat::from_rotation_z(-0.55 + wave * 0.08)
                    } else {
                        Quat::from_rotation_x(0.18) * Quat::from_rotation_z(-1.05)
                    };
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *= if is_hover {
                        Quat::from_rotation_z(0.55 - wave * 0.08)
                    } else {
                        Quat::from_rotation_x(0.18) * Quat::from_rotation_z(1.05)
                    };
                }
                CartoonPartKind::LeftLeg
                | CartoonPartKind::LeftFoot
                | CartoonPartKind::LeftBoot => {
                    transform.rotation *= Quat::from_rotation_x(if is_dash { 0.46 } else { 0.18 });
                }
                CartoonPartKind::RightLeg
                | CartoonPartKind::RightFoot
                | CartoonPartKind::RightBoot => {
                    transform.rotation *= Quat::from_rotation_x(if is_dash { 0.38 } else { 0.12 });
                }
                CartoonPartKind::Shadow => {
                    transform.scale *= if is_hover {
                        Vec3::new(0.78, 1.0, 0.78)
                    } else {
                        Vec3::new(0.60, 1.0, 0.60)
                    };
                }
                _ => {}
            }
            if is_face {
                transform.rotation *= Quat::from_rotation_x(pitch * 0.55);
                transform.translation.y += if is_hover { 0.06 + wave * 0.04 } else { 0.05 };
            }
            if is_body_acc {
                transform.rotation *= Quat::from_rotation_x(pitch);
                transform.translation.y += if is_hover { wave * 0.055 } else { 0.04 };
            }
            if is_cape {
                transform.rotation *=
                    Quat::from_rotation_x(cape_flap * if is_dash { 1.65 } else { 1.10 });
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

        // ── Grapple ──────────────────────────────────────────────────────────
        CartoonPose::Mantle => {
            let pull = wave.abs();
            match part.kind {
                CartoonPartKind::Body => {
                    transform.translation.y += 0.18 + pull * 0.04;
                    transform.rotation *= Quat::from_rotation_x(-0.20);
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.translation.y += 0.20;
                    transform.rotation *= Quat::from_rotation_x(-0.12);
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.translation.y += 0.28;
                    transform.rotation *=
                        Quat::from_rotation_x(-1.34) * Quat::from_rotation_z(-0.30);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.translation.y += 0.28;
                    transform.rotation *=
                        Quat::from_rotation_x(-1.34) * Quat::from_rotation_z(0.30);
                }
                CartoonPartKind::LeftLeg | CartoonPartKind::RightLeg => {
                    transform.rotation *= Quat::from_rotation_x(0.42);
                }
                CartoonPartKind::Shadow => {
                    transform.scale *= Vec3::new(0.86, 1.0, 0.86);
                }
                _ => {}
            }
            if is_face {
                transform.translation.y += 0.20;
                transform.rotation *= Quat::from_rotation_x(-0.12);
            }
            if is_body_acc {
                transform.translation.y += 0.18 + pull * 0.04;
                transform.rotation *= Quat::from_rotation_x(-0.20);
            }
            if is_cape {
                transform.rotation *= Quat::from_rotation_x(cape_flap * 0.35);
            }
        }

        // ── Grapple ──────────────────────────────────────────────────────────
        CartoonPose::Grapple => {
            let throw = (sample.phase * 1.2).sin().abs();
            match part.kind {
                CartoonPartKind::Body => {
                    transform.rotation *=
                        Quat::from_rotation_x(-0.16) * Quat::from_rotation_y(0.18);
                    transform.translation.y += 0.03 + throw * 0.035;
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.rotation *=
                        Quat::from_rotation_x(-0.10) * Quat::from_rotation_y(0.20);
                    transform.translation.y += 0.04;
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.rotation *=
                        Quat::from_rotation_x(-0.42) * Quat::from_rotation_z(-0.52);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.translation.y += 0.16;
                    transform.rotation *= Quat::from_rotation_x(-1.62 + throw * 0.16)
                        * Quat::from_rotation_y(0.20)
                        * Quat::from_rotation_z(0.18);
                }
                CartoonPartKind::LeftLeg
                | CartoonPartKind::LeftFoot
                | CartoonPartKind::LeftBoot => {
                    transform.rotation *=
                        Quat::from_rotation_x(-0.20) * Quat::from_rotation_z(-0.10);
                }
                CartoonPartKind::RightLeg
                | CartoonPartKind::RightFoot
                | CartoonPartKind::RightBoot => {
                    transform.rotation *= Quat::from_rotation_x(0.28) * Quat::from_rotation_z(0.12);
                }
                CartoonPartKind::Shadow => {
                    transform.scale *= Vec3::new(1.14, 1.0, 0.88);
                }
                _ => {}
            }
            if is_face {
                transform.rotation *= Quat::from_rotation_y(0.20);
                transform.translation.y += 0.04;
            }
            if is_body_acc {
                transform.rotation *= Quat::from_rotation_x(-0.16) * Quat::from_rotation_y(0.18);
                transform.translation.y += 0.03 + throw * 0.035;
            }
            if is_cape {
                transform.rotation *= Quat::from_rotation_x(cape_flap * 0.95);
                transform.rotation *= Quat::from_rotation_z((0.10 + throw * 0.10) * cape_flap);
            }
        }

        // ── Attack ───────────────────────────────────────────────────────────
        CartoonPose::Swing | CartoonPose::Zip => {
            let is_zip = sample.pose == CartoonPose::Zip;
            let stretch = if is_zip { 1.18 } else { 1.02 };
            match part.kind {
                CartoonPartKind::Body => {
                    transform.rotation *= if is_zip {
                        Quat::from_rotation_x(-0.32)
                    } else {
                        Quat::from_rotation_z(wave * 0.18) * Quat::from_rotation_x(-0.10)
                    };
                    transform.scale.z *= stretch;
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.rotation *= Quat::from_rotation_x(if is_zip { -0.20 } else { -0.06 });
                    transform.translation.y += 0.06;
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.translation.y += if is_zip { 0.10 } else { 0.26 };
                    transform.rotation *=
                        Quat::from_rotation_x(-1.42) * Quat::from_rotation_z(-0.22);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.translation.y += 0.30;
                    transform.rotation *=
                        Quat::from_rotation_x(-1.72) * Quat::from_rotation_z(0.22);
                }
                CartoonPartKind::LeftLeg
                | CartoonPartKind::LeftFoot
                | CartoonPartKind::LeftBoot => {
                    transform.rotation *=
                        Quat::from_rotation_x(if is_zip { 0.36 } else { 0.16 + wave * 0.18 });
                }
                CartoonPartKind::RightLeg
                | CartoonPartKind::RightFoot
                | CartoonPartKind::RightBoot => {
                    transform.rotation *=
                        Quat::from_rotation_x(if is_zip { 0.24 } else { -0.20 - wave * 0.18 });
                }
                CartoonPartKind::Shadow => {
                    transform.scale *= Vec3::new(0.64, 1.0, 0.64);
                }
                _ => {}
            }
            if is_face {
                transform.translation.y += 0.06;
                transform.rotation *= Quat::from_rotation_x(if is_zip { -0.20 } else { -0.06 });
            }
            if is_body_acc {
                transform.scale.z *= stretch;
            }
            if is_cape {
                transform.rotation *=
                    Quat::from_rotation_x(cape_flap * if is_zip { 1.55 } else { 1.15 });
                transform.rotation *= Quat::from_rotation_z(wave * 0.12 * cape_flap);
            }
        }

        // ── Roll / Parry ─────────────────────────────────────────────────────
        CartoonPose::Roll | CartoonPose::Parry => {
            let is_parry = sample.pose == CartoonPose::Parry;
            match part.kind {
                CartoonPartKind::Body => {
                    transform.translation.y -= if is_parry { 0.02 } else { 0.18 };
                    transform.rotation *= if is_parry {
                        Quat::from_rotation_y(-0.18) * Quat::from_rotation_x(-0.05)
                    } else {
                        Quat::from_rotation_x(0.82) * Quat::from_rotation_z(wave * 0.22)
                    };
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.translation.y -= if is_parry { 0.0 } else { 0.12 };
                    transform.rotation *= if is_parry {
                        Quat::from_rotation_y(-0.16)
                    } else {
                        Quat::from_rotation_x(0.55)
                    };
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.rotation *= if is_parry {
                        Quat::from_rotation_x(-1.05) * Quat::from_rotation_z(-0.65)
                    } else {
                        Quat::from_rotation_x(0.72) * Quat::from_rotation_z(-0.40)
                    };
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *= if is_parry {
                        Quat::from_rotation_x(-1.12) * Quat::from_rotation_z(0.58)
                    } else {
                        Quat::from_rotation_x(0.68) * Quat::from_rotation_z(0.40)
                    };
                }
                CartoonPartKind::LeftLeg
                | CartoonPartKind::LeftFoot
                | CartoonPartKind::LeftBoot
                | CartoonPartKind::RightLeg
                | CartoonPartKind::RightFoot
                | CartoonPartKind::RightBoot => {
                    transform.rotation *= if is_parry {
                        Quat::from_rotation_x(0.16)
                    } else {
                        Quat::from_rotation_x(0.70)
                    };
                }
                CartoonPartKind::Shadow => {
                    transform.scale *= if is_parry {
                        Vec3::new(1.12, 1.0, 0.90)
                    } else {
                        Vec3::new(1.24, 1.0, 0.70)
                    };
                }
                _ => {}
            }
            if is_face {
                transform.rotation *= if is_parry {
                    Quat::from_rotation_y(-0.16)
                } else {
                    Quat::from_rotation_x(0.55)
                };
            }
            if is_body_acc {
                transform.rotation *= if is_parry {
                    Quat::from_rotation_y(-0.18)
                } else {
                    Quat::from_rotation_x(0.82)
                };
            }
            if is_cape {
                transform.rotation *=
                    Quat::from_rotation_x(cape_flap * if is_parry { 0.45 } else { 0.20 });
            }
        }

        // ── Attack ───────────────────────────────────────────────────────────
        CartoonPose::Attack => {
            let punch = (sample.phase * 1.7).sin();
            match part.kind {
                CartoonPartKind::Body => {
                    transform.rotation *=
                        Quat::from_rotation_y(0.18 + punch * 0.08) * Quat::from_rotation_x(-0.06);
                    transform.translation.y += punch.abs() * 0.035;
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.rotation *= Quat::from_rotation_y(0.12 + punch * 0.05);
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.rotation *=
                        Quat::from_rotation_x(-0.55) * Quat::from_rotation_z(-0.35);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *=
                        Quat::from_rotation_x(-1.20 + punch * 0.12) * Quat::from_rotation_z(0.42);
                }
                CartoonPartKind::LeftLeg
                | CartoonPartKind::LeftFoot
                | CartoonPartKind::LeftBoot => {
                    transform.rotation *= Quat::from_rotation_x(-0.12);
                }
                CartoonPartKind::RightLeg
                | CartoonPartKind::RightFoot
                | CartoonPartKind::RightBoot => {
                    transform.rotation *= Quat::from_rotation_x(0.18);
                }
                CartoonPartKind::Shadow => {
                    transform.scale *= Vec3::new(1.10, 1.0, 0.92);
                }
                _ => {}
            }
            if is_face {
                transform.rotation *= Quat::from_rotation_y(0.12 + punch * 0.05);
            }
            if is_body_acc {
                transform.rotation *=
                    Quat::from_rotation_y(0.18 + punch * 0.08) * Quat::from_rotation_x(-0.06);
            }
            if is_cape {
                transform.rotation *= Quat::from_rotation_x(cape_flap * 0.65);
                transform.rotation *= Quat::from_rotation_z(punch * 0.08 * cape_flap);
            }
        }

        // ── Star Sabre slash ─────────────────────────────────────────────────
        CartoonPose::SabreSlash => {
            let sweep = (sample.phase * 1.25).sin();
            let snap = sweep.signum() * sweep.abs().sqrt();
            match part.kind {
                CartoonPartKind::Body => {
                    transform.rotation *=
                        Quat::from_rotation_y(0.42 * snap) * Quat::from_rotation_z(-0.08 * snap);
                    transform.translation.y += sweep.abs() * 0.055;
                    transform.scale.x *= 1.02;
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.rotation *=
                        Quat::from_rotation_y(0.30 * snap) * Quat::from_rotation_x(-0.05);
                    transform.translation.y += sweep.abs() * 0.035;
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.rotation *= Quat::from_rotation_x(-0.78)
                        * Quat::from_rotation_y(-0.28)
                        * Quat::from_rotation_z(-0.72 * snap);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *= Quat::from_rotation_x(-1.72)
                        * Quat::from_rotation_y(0.34)
                        * Quat::from_rotation_z(1.05 * snap);
                }
                CartoonPartKind::LeftLeg
                | CartoonPartKind::LeftFoot
                | CartoonPartKind::LeftBoot => {
                    transform.rotation *=
                        Quat::from_rotation_x(-0.26) * Quat::from_rotation_z(0.10);
                }
                CartoonPartKind::RightLeg
                | CartoonPartKind::RightFoot
                | CartoonPartKind::RightBoot => {
                    transform.rotation *=
                        Quat::from_rotation_x(0.28) * Quat::from_rotation_z(-0.12);
                }
                CartoonPartKind::StarBadge | CartoonPartKind::Belt => {
                    transform.rotation *= Quat::from_rotation_y(0.42 * snap);
                }
                CartoonPartKind::Shadow => {
                    transform.scale *= Vec3::new(1.18, 1.0, 0.86);
                }
                _ => {}
            }
            if is_face {
                transform.rotation *= Quat::from_rotation_y(0.30 * snap);
                transform.translation.y += sweep.abs() * 0.035;
            }
            if is_body_acc {
                transform.rotation *=
                    Quat::from_rotation_y(0.42 * snap) * Quat::from_rotation_z(-0.08 * snap);
                transform.translation.y += sweep.abs() * 0.055;
            }
            if is_cape {
                transform.rotation *= Quat::from_rotation_x(cape_flap * 1.20);
                transform.rotation *= Quat::from_rotation_z((-0.24 + snap * 0.20) * cape_flap);
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
        _ => {}
    }
}
