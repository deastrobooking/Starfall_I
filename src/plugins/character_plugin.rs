use bevy::{app::AnimationSystems, prelude::*, transform::TransformSystems};
use std::collections::HashMap;
use std::f32::consts::PI;

use crate::character::parts::{
    spawn_arms, spawn_body, spawn_head, spawn_legs, spawn_shoulders, CharacterLoadout,
    CharacterVisualConfig, PartSlotTag,
};
use crate::character_studio::human_mesh::{
    PlayableStudioHuman, StudioBodyRegion, StudioFaceFeature, StudioHumanPart,
};
use crate::combat::data::MeleePhase;
use crate::combat::tricks::{TrickCategory, TrickState};
use crate::components::character::{
    default_joint_for_part, CartoonAnimator, CartoonCharacter, CartoonPart, CartoonPartKind,
    CartoonPose, CharacterIkPose, HandEngine, HandGrip, HandPoseState, HandSide, IkTarget,
    JointKind, JointMarker, SkeletonRig,
};
use crate::components::player::{
    DodgeState, EdgeGrabState, FlightMode, GrappleHookMode, GrappleHookState, JetpackState,
    MotionAnimationState, ParryState, PlayerInput, PlayerMovement, PlayerState, PlayerStateMachine,
    TraversalMode, TraversalModeState,
};
use crate::components::weapon::{BeamSabre, MeleeCombo};
use crate::engine::state::AppState;

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            crate::character::animation_mvp::CharacterAnimationMvpPlugin,
            crate::character::imported_modular::ImportedModularRuntimePlugin,
        ))
        .add_systems(
            Update,
            (
                bind_parts_to_skeleton,
                cartoon_animation_system.run_if(
                    in_state(AppState::Playing).or_else(in_state(AppState::CharacterDesign)),
                ),
                crate::character::animation_mvp::drive_graph_animation,
                studio_human_animation_system.run_if(in_state(AppState::Playing)),
                swap_character_parts,
            )
                .chain(),
        )
        .add_systems(
            PostUpdate,
            apply_post_graph_motion_response
                .after(AnimationSystems)
                .before(TransformSystems::Propagate),
        );
    }
}

fn bind_parts_to_skeleton(
    mut commands: Commands,
    mut parts: Query<(Entity, &mut CartoonPart, &mut Transform), Added<CartoonPart>>,
    rigs: Query<&SkeletonRig>,
    joints: Query<&JointMarker>,
) {
    for (entity, mut part, mut transform) in &mut parts {
        let joint_kind = part.joint.or_else(|| default_joint_for_part(part.kind));
        part.joint = joint_kind;

        let Some(joint_kind) = joint_kind else {
            continue;
        };
        let Ok(rig) = rigs.get(part.root) else {
            continue;
        };
        let Some(&joint_entity) = rig.joints.get(&joint_kind) else {
            continue;
        };
        let Ok(marker) = joints.get(joint_entity) else {
            continue;
        };
        if marker.root != part.root {
            continue;
        }

        transform.translation -= marker.rest_translation;
        part.base_translation = transform.translation;
        commands.entity(joint_entity).add_child(entity);
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

        let swappable = [
            PartSlotTag::Body,
            PartSlotTag::Arms,
            PartSlotTag::Legs,
            PartSlotTag::Shoulders,
            PartSlotTag::Head,
            PartSlotTag::HeadGear,
        ];
        for (entity, part, tag) in &parts {
            if part.root == root && swappable.contains(tag) {
                commands.entity(entity).try_despawn();
            }
        }

        spawn_body(
            &mut commands,
            &mut meshes,
            &mut materials,
            root,
            visual_cfg,
            lift,
            loadout.body,
        );
        spawn_arms(
            &mut commands,
            &mut meshes,
            &mut materials,
            root,
            visual_cfg,
            lift,
            loadout.arms,
        );
        spawn_legs(
            &mut commands,
            &mut meshes,
            &mut materials,
            root,
            visual_cfg,
            lift,
            loadout.legs,
        );
        spawn_shoulders(
            &mut commands,
            &mut meshes,
            &mut materials,
            root,
            visual_cfg,
            lift,
            loadout.shoulders,
        );
        spawn_head(
            &mut commands,
            &mut meshes,
            &mut materials,
            root,
            visual_cfg,
            lift,
            loadout.head,
        );
    }
}

#[derive(Component, Clone, Copy)]
struct PoseSample {
    pose: CartoonPose,
    phase: f32,
    speed: f32,
    scale: f32,
    stride: f32,
    agility: f32,
    vertical_velocity: f32,
    local_velocity: Vec3,
    grounded: bool,
    wall_normal_local: Vec3,
    hanging: bool,
    dodge_direction_local: Vec3,
    grapple_attach_local: Option<Vec3>,
    ik: CharacterIkPose,
    hands: HandEngine,
    wall_clasp_time: f32,
    /// Trick being performed on the hoverboard, if any. Modifies the riding
    /// stance rather than replacing it — the rider is still on the board.
    board_trick: Option<TrickCategory>,
    /// Signed carve input (-1..1) for stance lean and counter-balance arms.
    board_carve: f32,
    /// Signed sabre swing position (-1..1) driven by the real slash phase
    /// machine; see [`sabre_swing_position`].
    sabre_swing: f32,
    /// Presentation-only landing squash sampled from the motor's contact edge.
    landing_compression: f32,
}

/// Signed swing position of a Star Sabre slash, in `-1..=1`.
///
/// The old pose drove itself from a free-running sine, so the blade waved
/// whether or not a strike was happening. Reading the real phase machine
/// instead gives the slash an actual attack shape — wound back through
/// startup, snapping across during the active window, settling through
/// recovery — and alternating sides per slash so a chain reads as a combo.
fn sabre_swing_position(phase: MeleePhase, remaining: f32, slash_index: u32) -> f32 {
    // Nominal phase lengths from the shipped sabre MoveDef; only used to
    // normalize a visual, so authored retunes stay close enough.
    const STARTUP: f32 = 0.05;
    const ACTIVE: f32 = 0.08;
    const RECOVERY: f32 = 0.12;
    let side = if slash_index.is_multiple_of(2) {
        1.0
    } else {
        -1.0
    };
    let magnitude = match phase {
        // Wind back: -0.15 → -0.85 as the strike approaches.
        MeleePhase::Startup => {
            let t = 1.0 - (remaining / STARTUP).clamp(0.0, 1.0);
            -0.15 - 0.70 * t
        }
        // Snap through the arc: -0.85 → +1.0 across the hit window.
        MeleePhase::Active => {
            let t = 1.0 - (remaining / ACTIVE).clamp(0.0, 1.0);
            -0.85 + 1.85 * t
        }
        // Follow through and settle back toward neutral.
        MeleePhase::Recovery => {
            let t = 1.0 - (remaining / RECOVERY).clamp(0.0, 1.0);
            1.0 - 0.85 * t
        }
    };
    magnitude * side
}

/// Resolved hoverboard riding stance: the sideways skate posture plus
/// whatever the active trick layers on top. Kept as plain data so the pose
/// can be unit-tested without spawning a character rig.
#[derive(Clone, Copy, Debug, PartialEq)]
struct BoardStancePose {
    /// Torso yaw onto the board's long axis (regular stance, ~60°).
    stance_yaw: f32,
    torso_pitch: f32,
    torso_roll: f32,
    head_pitch: f32,
    /// How far the hips drop — deeper while grinding or tucking a flip.
    crouch: f32,
    left_arm_out: f32,
    right_arm_out: f32,
    left_arm_pitch: f32,
    right_arm_pitch: f32,
    front_foot_yaw: f32,
    back_foot_yaw: f32,
    front_knee: f32,
    back_knee: f32,
}

/// Base skate stance, carve response, and per-trick overlay.
fn board_stance_pose(sample: PoseSample) -> BoardStancePose {
    // Regular stance: torso rotated onto the deck's long axis.
    const STANCE_YAW: f32 = 1.05;
    let carve = sample.board_carve.clamp(-1.0, 1.0);
    let airborne = !sample.grounded;
    // Faster riding = lower, more committed stance.
    let speed_crouch = (sample.speed * 0.02).clamp(0.0, 0.10);

    let mut pose = BoardStancePose {
        stance_yaw: STANCE_YAW,
        torso_pitch: -0.16,
        // Lean into the turn.
        torso_roll: carve * 0.28,
        head_pitch: -0.08,
        crouch: 0.06 + speed_crouch,
        // Arms out for balance; the trailing arm lifts higher through a carve
        // so the rider visibly counter-balances instead of holding a T-pose.
        left_arm_out: -0.72 - carve * 0.30,
        right_arm_out: 0.72 - carve * 0.30,
        left_arm_pitch: -0.10 - carve * 0.18,
        right_arm_pitch: -0.10 + carve * 0.18,
        front_foot_yaw: 0.55,
        back_foot_yaw: 0.42,
        front_knee: 0.32,
        back_knee: 0.30,
    };

    if airborne {
        // Tuck slightly and bring the arms in when off the ground.
        pose.crouch += 0.04;
        pose.front_knee += 0.16;
        pose.back_knee += 0.16;
        pose.left_arm_out += 0.10;
        pose.right_arm_out -= 0.10;
    }

    match sample.board_trick {
        // Grab: reach down and hold the deck, torso folds over the knees.
        Some(TrickCategory::Grab) => {
            pose.torso_pitch = -0.62;
            pose.crouch += 0.16;
            pose.front_knee = 1.05;
            pose.back_knee = 0.95;
            // Leading hand drops to the board, trailing arm flares for style.
            pose.left_arm_out = -0.28;
            pose.left_arm_pitch = 1.15;
            pose.right_arm_out = 1.05;
            pose.right_arm_pitch = -0.35;
            pose.head_pitch = 0.22;
        }
        // Flip: tight tuck, knees to chest, arms pulled in to spin fast.
        Some(TrickCategory::Flip) => {
            pose.torso_pitch = -0.48;
            pose.crouch += 0.20;
            pose.front_knee = 1.30;
            pose.back_knee = 1.20;
            pose.left_arm_out = -0.34;
            pose.right_arm_out = 0.34;
            pose.left_arm_pitch = 0.45;
            pose.right_arm_pitch = 0.45;
            pose.head_pitch = 0.16;
        }
        // Grind: deep squat, board locked to the rail, arms wide to hold it.
        Some(TrickCategory::Grind) => {
            pose.torso_pitch = -0.30;
            pose.torso_roll = carve * 0.16;
            pose.crouch += 0.22;
            pose.front_knee = 0.78;
            pose.back_knee = 0.72;
            pose.left_arm_out = -1.05;
            pose.right_arm_out = 1.05;
            pose.left_arm_pitch = -0.22;
            pose.right_arm_pitch = -0.22;
        }
        // Manual: weight back over the tail, chest up, arms level.
        Some(TrickCategory::Manual) => {
            pose.torso_pitch = 0.24;
            pose.crouch = (pose.crouch - 0.02).max(0.0);
            pose.front_knee = 0.14;
            pose.back_knee = 0.52;
            pose.left_arm_out = -0.95;
            pose.right_arm_out = 0.95;
            pose.left_arm_pitch = -0.30;
            pose.right_arm_pitch = -0.30;
            pose.head_pitch = -0.18;
        }
        // Lip / Special: big showboat extension.
        Some(TrickCategory::Lip) | Some(TrickCategory::Special) => {
            pose.torso_pitch = -0.72;
            pose.crouch += 0.10;
            pose.left_arm_out = -1.25;
            pose.right_arm_out = 1.25;
            pose.left_arm_pitch = -0.55;
            pose.right_arm_pitch = -0.55;
            pose.front_knee = 0.60;
            pose.back_knee = 0.55;
        }
        None => {}
    }
    pose
}

/// Natural locomotion keeps the arms close to the torso and swings them along
/// the character's forward axis. Returning explicit shoulder pitches keeps the
/// Studio, joint-rig, and fallback cartoon renderers on the same motion contract.
fn locomotion_arm_pitches(pose: CartoonPose, phase: f32, agility: f32) -> (f32, f32) {
    let amplitude = match pose {
        CartoonPose::Walk => 0.48,
        CartoonPose::Run => 0.82,
        CartoonPose::Sprint => 1.02,
        _ => 0.0,
    } * agility.clamp(0.82, 1.18);
    let swing = (phase * 1.1).sin() * amplitude;
    (swing, -swing)
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
    flight_mode: FlightMode,
    traversal_mode: TraversalMode,
    grapple_mode: Option<GrappleHookMode>,
    grapple_pose: bool,
    sabre_slashing: bool,
    melee_attacking: bool,
    dodging: bool,
    parrying: bool,
    hard_landing: bool,
    mantling: bool,
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
    if input.mantling {
        return CartoonPose::Mantle;
    }
    if input.hard_landing {
        return CartoonPose::HardLand;
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
    if input.state == Some(PlayerState::Climbing) {
        // Free-climb reuses the hang pose family; the hand engine already
        // supplies HandGrip::Climb for wall contact.
        return CartoonPose::Hang;
    }
    if input.wall_sliding || input.state == Some(PlayerState::WallSliding) {
        return CartoonPose::WallSlide;
    }
    if input.state == Some(PlayerState::Swimming) {
        return CartoonPose::Glide;
    }
    if input.traversal_mode == TraversalMode::Hoverboard
        && (input.horizontal_speed > 0.08 || input.jetpack_active || !input.grounded)
    {
        return CartoonPose::Hoverboard;
    }
    if input.jetpack_active {
        if input.flight_mode == FlightMode::AirDash {
            return CartoonPose::AirDash;
        }
        if input.flight_mode == FlightMode::Glide {
            return CartoonPose::Glide;
        }
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
    // PlayerMovement uses per-tick world speeds (walk ~= 0.38, sprint ~= 0.70),
    // so the old 28.0 threshold made the authored Run pose unreachable.
    if input.horizontal_speed > 0.48 {
        return CartoonPose::Run;
    }
    if input.horizontal_speed > 0.06 {
        return CartoonPose::Walk;
    }
    CartoonPose::Idle
}

#[derive(Clone, Copy)]
struct IkPoseInput {
    pose: CartoonPose,
    phase: f32,
    scale: f32,
    visual_ground_lift: f32,
    grounded: bool,
    hanging: bool,
    wall_normal_local: Vec3,
    local_velocity: Vec3,
    dodge_direction_local: Vec3,
    grapple_attach_local: Option<Vec3>,
    wall_clasp_time: f32,
}

#[derive(Clone, Copy)]
struct HandEngineInput {
    pose: CartoonPose,
    phase: f32,
    scale: f32,
    visual_ground_lift: f32,
    fire: bool,
    aim: bool,
    melee_light: bool,
    melee_heavy: bool,
    sabre_active: bool,
    sabre_slashing: bool,
    /// Both hands are on the hilt for a heavy committed technique.
    sabre_two_handed: bool,
    parrying: bool,
    hanging: bool,
    wall_sliding: bool,
    traversal_mode: TraversalMode,
    grapple_attach_local: Option<Vec3>,
}

fn build_character_ik_pose(input: IkPoseInput) -> CharacterIkPose {
    let mut pose = CharacterIkPose::default();
    let scale = input.scale.max(0.1);
    let lift = input.visual_ground_lift;
    let ground_speed = input.local_velocity.with_y(0.0).length();
    let move_amount = (ground_speed * 1.55).clamp(0.0, 1.0);

    if input.grounded
        && matches!(
            input.pose,
            CartoonPose::Idle | CartoonPose::Walk | CartoonPose::Run | CartoonPose::Sprint
        )
    {
        let step = (input.phase * 1.1).sin();
        let side = 0.165 * scale;
        let foot_y = -1.07 * scale + lift;
        let foot_z = -0.025 * scale;
        let stride_reach = 0.065 * scale * move_amount;
        let idle_settle = (1.0 - move_amount) * 0.018 * scale;
        let foot_weight = (0.88 - move_amount * 0.22).clamp(0.56, 0.9);

        pose.left_foot = Some(
            IkTarget::new(
                JointKind::LeftAnkle,
                Vec3::new(
                    -side,
                    foot_y + step.max(0.0) * 0.025 * scale - idle_settle,
                    foot_z + step * stride_reach,
                ),
                foot_weight,
            )
            .with_pole(Vec3::Z),
        );
        pose.right_foot = Some(
            IkTarget::new(
                JointKind::RightAnkle,
                Vec3::new(
                    side,
                    foot_y + (-step).max(0.0) * 0.025 * scale - idle_settle,
                    foot_z - step * stride_reach,
                ),
                foot_weight,
            )
            .with_pole(Vec3::Z),
        );
    }

    if input.hanging || input.pose == CartoonPose::Hang {
        let grip_y = 0.58 * scale + lift;
        pose.left_hand = Some(IkTarget::new(
            JointKind::LeftWrist,
            Vec3::new(-0.34 * scale, grip_y, -0.42 * scale),
            0.96,
        ));
        pose.right_hand = Some(IkTarget::new(
            JointKind::RightWrist,
            Vec3::new(0.34 * scale, grip_y, -0.42 * scale),
            0.96,
        ));
        pose.look_at = Some(Vec3::new(0.0, 0.45 * scale + lift, -1.2 * scale));
    } else if input.pose == CartoonPose::WallSlide {
        let wall = normalized_or_zero(input.wall_normal_local);
        let scrape = (input.wall_clasp_time * 18.0).sin() * 0.025 * scale;
        let hand_z = (-0.44 + wall.z * 0.08) * scale;
        pose.left_hand = Some(IkTarget::new(
            JointKind::LeftWrist,
            Vec3::new(-0.42 * scale + scrape, 0.16 * scale + lift, hand_z),
            0.82,
        ));
        pose.right_hand = Some(IkTarget::new(
            JointKind::RightWrist,
            Vec3::new(0.34 * scale, -0.16 * scale + lift, -0.18 * scale),
            0.38,
        ));
        pose.look_at = Some(Vec3::new(wall.x * 0.45 * scale, 0.1 * scale + lift, -scale));
    }

    if matches!(
        input.pose,
        CartoonPose::Grapple | CartoonPose::Swing | CartoonPose::Zip
    ) {
        if let Some(target) = input.grapple_attach_local {
            let dir = normalized_or_zero(target);
            let reach_y = (0.14 * scale + lift + dir.y.clamp(-0.2, 0.8) * 0.45 * scale)
                .clamp(-0.18 * scale + lift, 0.72 * scale + lift);
            let reach_z = (dir.z * 0.62 * scale).clamp(-0.72 * scale, 0.18 * scale);
            let reach_x = (dir.x * 0.48 * scale).clamp(-0.55 * scale, 0.55 * scale);
            let pull_weight = match input.pose {
                CartoonPose::Swing => 0.9,
                CartoonPose::Zip => 0.82,
                _ => 0.72,
            };

            pose.right_hand = Some(IkTarget::new(
                JointKind::RightWrist,
                Vec3::new(reach_x.max(0.12 * scale), reach_y, reach_z),
                pull_weight,
            ));
            pose.left_hand = Some(IkTarget::new(
                JointKind::LeftWrist,
                Vec3::new(-0.24 * scale, 0.02 * scale + lift, -0.20 * scale),
                (pull_weight * 0.48).clamp(0.0, 0.58),
            ));
            pose.look_at = Some(target);
        }
    }

    if input.pose == CartoonPose::Roll {
        let dodge = normalized_or_zero(input.dodge_direction_local);
        pose.look_at = Some(Vec3::new(dodge.x * 0.35 * scale, lift, -scale));
    }

    pose
}

fn build_hand_engine(input: HandEngineInput) -> HandEngine {
    let scale = input.scale.max(0.1);
    let lift = input.visual_ground_lift;
    let wave = input.phase.sin();

    if input.hanging || input.pose == CartoonPose::Hang {
        let grip_y = 0.58 * scale + lift;
        return HandEngine {
            left: HandPoseState::new(HandGrip::Climb, 0.92, 0.08)
                .with_target(Vec3::new(-0.34 * scale, grip_y, -0.42 * scale), 0.98)
                .with_twist(-0.24),
            right: HandPoseState::new(HandGrip::Climb, 0.92, 0.08)
                .with_target(Vec3::new(0.34 * scale, grip_y, -0.42 * scale), 0.98)
                .with_twist(0.24),
        };
    }

    if input.wall_sliding || input.pose == CartoonPose::WallSlide {
        return HandEngine {
            left: HandPoseState::new(HandGrip::Climb, 0.86, 0.10)
                .with_target(
                    Vec3::new(
                        -0.42 * scale + wave * 0.018 * scale,
                        0.16 * scale + lift,
                        -0.44 * scale,
                    ),
                    0.86,
                )
                .with_twist(-0.18),
            right: HandPoseState::new(HandGrip::Balance, 0.30, 0.62)
                .with_target(
                    Vec3::new(0.34 * scale, -0.16 * scale + lift, -0.18 * scale),
                    0.36,
                )
                .with_twist(0.38),
        };
    }

    if matches!(
        input.pose,
        CartoonPose::Grapple | CartoonPose::Swing | CartoonPose::Zip
    ) {
        let target = input
            .grapple_attach_local
            .map(|target| {
                let dir = normalized_or_zero(target);
                let reach_y = (0.14 * scale + lift + dir.y.clamp(-0.2, 0.8) * 0.45 * scale)
                    .clamp(-0.18 * scale + lift, 0.72 * scale + lift);
                let reach_z = (dir.z * 0.62 * scale).clamp(-0.72 * scale, 0.18 * scale);
                let reach_x = (dir.x * 0.48 * scale).clamp(-0.55 * scale, 0.55 * scale);
                Vec3::new(reach_x.max(0.12 * scale), reach_y, reach_z)
            })
            .unwrap_or(Vec3::new(0.30 * scale, 0.28 * scale + lift, -0.58 * scale));
        let pull_weight = match input.pose {
            CartoonPose::Swing => 0.92,
            CartoonPose::Zip => 0.84,
            _ => 0.74,
        };
        return HandEngine {
            left: HandPoseState::new(HandGrip::Brace, 0.58, 0.24)
                .with_target(
                    Vec3::new(-0.24 * scale, 0.02 * scale + lift, -0.20 * scale),
                    0.52,
                )
                .with_twist(-0.20),
            right: HandPoseState::new(HandGrip::Grapple, 0.82, 0.12)
                .with_target(target, pull_weight)
                .with_twist(0.30),
        };
    }

    if input.parrying || input.pose == CartoonPose::Parry {
        return HandEngine {
            left: HandPoseState::new(HandGrip::Brace, 0.72, 0.16)
                .with_target(
                    Vec3::new(-0.24 * scale, 0.16 * scale + lift, -0.36 * scale),
                    0.78,
                )
                .with_twist(-0.42),
            right: HandPoseState::new(HandGrip::Brace, 0.78, 0.14)
                .with_target(
                    Vec3::new(0.24 * scale, 0.14 * scale + lift, -0.40 * scale),
                    0.82,
                )
                .with_twist(0.42),
        };
    }

    if input.sabre_active || input.sabre_slashing || input.pose == CartoonPose::SabreSlash {
        let slash = (input.phase * 1.25).sin();
        return HandEngine {
            // Two-handed swings bring the off hand onto the hilt, just below
            // the sword hand, and close its grip; otherwise it braces wide for
            // counter-balance.
            left: if input.sabre_two_handed {
                HandPoseState::new(HandGrip::Sabre, 0.84, 0.10)
                    .with_target(
                        Vec3::new(
                            0.26 * scale + slash * 0.07 * scale,
                            0.02 * scale + lift,
                            -0.42 * scale,
                        ),
                        0.78,
                    )
                    .with_twist(0.48)
                    .with_recoil(slash.abs() * 0.30)
            } else {
                HandPoseState::new(HandGrip::Brace, 0.44, 0.32)
                    .with_target(
                        Vec3::new(-0.22 * scale, 0.02 * scale + lift, -0.26 * scale),
                        0.42,
                    )
                    .with_twist(-0.12)
            },
            right: HandPoseState::new(HandGrip::Sabre, 0.88, 0.08)
                .with_target(
                    Vec3::new(
                        0.34 * scale + slash * 0.08 * scale,
                        0.12 * scale + lift,
                        -0.48 * scale,
                    ),
                    if input.sabre_slashing { 0.82 } else { 0.62 },
                )
                .with_twist(0.60)
                .with_recoil(slash.abs() * 0.35),
        };
    }

    if input.melee_light || input.melee_heavy || input.pose == CartoonPose::Attack {
        let punch = (input.phase * 1.7).sin().max(0.0);
        let heavy = input.melee_heavy;
        return HandEngine {
            left: HandPoseState::new(HandGrip::Brace, 0.50, 0.24)
                .with_target(
                    Vec3::new(-0.22 * scale, 0.03 * scale + lift, -0.24 * scale),
                    0.34,
                )
                .with_twist(-0.16),
            right: HandPoseState::new(HandGrip::Fist, 0.96, 0.04)
                .with_target(
                    Vec3::new(
                        0.26 * scale,
                        (0.05 + punch * 0.08) * scale + lift,
                        (-0.32 - punch * if heavy { 0.28 } else { 0.18 }) * scale,
                    ),
                    if heavy { 0.82 } else { 0.70 },
                )
                .with_twist(0.24)
                .with_recoil(punch * 0.22),
        };
    }

    if input.fire || input.aim {
        let recoil = if input.fire { 0.45 } else { 0.0 };
        return HandEngine {
            left: HandPoseState::new(HandGrip::Brace, 0.44, 0.26)
                .with_target(
                    Vec3::new(-0.16 * scale, 0.07 * scale + lift, -0.42 * scale),
                    0.46,
                )
                .with_twist(-0.12),
            right: HandPoseState::new(HandGrip::Trigger, 0.70, 0.12)
                .with_target(
                    Vec3::new(0.22 * scale, 0.10 * scale + lift, -0.52 * scale),
                    0.68,
                )
                .with_twist(0.18)
                .with_recoil(recoil),
        };
    }

    if input.traversal_mode == TraversalMode::Hoverboard || input.pose == CartoonPose::Hoverboard {
        return HandEngine {
            left: HandPoseState::new(HandGrip::Balance, 0.18, 0.72).with_twist(-0.32),
            right: HandPoseState::new(HandGrip::Balance, 0.18, 0.72).with_twist(0.32),
        };
    }

    HandEngine {
        left: HandPoseState::new(HandGrip::Relaxed, 0.20 + wave * 0.03, 0.20),
        right: HandPoseState::new(HandGrip::Relaxed, 0.20 - wave * 0.03, 0.20),
    }
}

fn apply_hand_engine_to_ik(ik: &mut CharacterIkPose, hands: HandEngine) {
    if let Some(target) = hands.left.ik_target(HandSide::Left) {
        ik.left_hand = Some(target);
    }
    if let Some(target) = hands.right.ik_target(HandSide::Right) {
        ik.right_hand = Some(target);
    }
}

fn root_local_vector(transform: &Transform, vector: Vec3) -> Vec3 {
    transform.rotation.inverse() * vector
}

fn world_point_to_root_local(transform: &Transform, point: Vec3) -> Vec3 {
    root_local_vector(transform, point - transform.translation)
}

fn normalized_or_zero(vector: Vec3) -> Vec3 {
    if vector.length_squared() > f32::EPSILON {
        vector.normalize()
    } else {
        Vec3::ZERO
    }
}

fn cartoon_animation_system(
    mut commands: Commands,
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
            Option<&TraversalModeState>,
            Option<&GrappleHookState>,
            Option<&DodgeState>,
            Option<&ParryState>,
            Option<&PlayerInput>,
            Option<&BeamSabre>,
            (
                Option<&MotionAnimationState>,
                Option<&MeleeCombo>,
                Option<&TrickState>,
            ),
        ),
        (Without<CartoonPart>, Without<JointMarker>),
    >,
    mut root_pose_state: Query<
        (Option<&mut HandEngine>, Option<&mut CharacterIkPose>),
        (Without<CartoonPart>, Without<JointMarker>),
    >,
    mut joints: Query<(&JointMarker, &mut Transform), Without<CartoonPart>>,
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
        traversal,
        grapple,
        dodge,
        parry,
        player_input,
        sabre,
        (motion_animation, melee, tricks),
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
        let melee_attacking = melee.map(|m| m.active.is_some()).unwrap_or(false);
        let board_trick = tricks.and_then(|t| t.active.as_ref().map(|a| a.category));
        let sabre_swing = sabre
            .filter(|s| s.is_slashing)
            .map(|s| sabre_swing_position(s.slash_phase, s.slash_timer, s.slash_index))
            .unwrap_or(0.0);
        let board_carve = player_input.map(|pi| pi.move_axis.x).unwrap_or(0.0);
        animator.pose = select_cartoon_pose(PoseInput {
            state: current_state,
            grounded: movement.map(|m| m.is_grounded).unwrap_or(true),
            horizontal_speed,
            vertical_velocity,
            hanging: edge_grab.map(|e| e.is_hanging).unwrap_or(false),
            wall_sliding: current_state == Some(PlayerState::WallSliding),
            jetpack_active: jetpack.map(|j| j.is_active).unwrap_or(false),
            flight_mode: jetpack.map(|j| j.mode).unwrap_or(FlightMode::Grounded),
            traversal_mode: traversal
                .map(|t| t.active)
                .unwrap_or(TraversalMode::Grapple),
            grapple_mode: grapple.map(|g| g.mode),
            grapple_pose: grapple.map(|g| g.wants_animation_pose()).unwrap_or(false),
            sabre_slashing,
            melee_attacking,
            dodging: dodge.map(|d| d.is_dodging).unwrap_or(false),
            parrying: parry.map(|p| p.is_parrying).unwrap_or(false),
            hard_landing: motion_animation
                .map(|motion| motion.hard_landing && motion.landing_timer > 0.0)
                .unwrap_or(false),
            mantling: motion_animation
                .map(|motion| motion.mantle_timer > 0.0)
                .unwrap_or(false),
        });

        let stride = character.map(|c| c.stride).unwrap_or(1.0).max(0.45);
        let agility = character.map(|c| c.agility).unwrap_or(1.0).max(0.45);
        let scale = character.map(|c| c.scale).unwrap_or(1.0).max(0.1);
        let visual_ground_lift = character.map(|c| c.visual_ground_lift).unwrap_or(0.0);
        let ground_velocity = movement
            .map(|m| m.ground_velocity)
            .unwrap_or(delta.with_y(0.0) / dt.max(0.001));
        let local_velocity = root_local_vector(transform, ground_velocity);
        let grounded = movement.map(|m| m.is_grounded).unwrap_or(true);
        let wall_normal_local = edge_grab
            .map(|e| root_local_vector(transform, e.wall_normal))
            .unwrap_or(Vec3::ZERO);
        let dodge_direction_local = dodge
            .map(|d| root_local_vector(transform, d.dodge_direction))
            .unwrap_or(Vec3::ZERO);
        let grapple_attach_local = grapple
            .and_then(|g| g.attach_point)
            .map(|point| world_point_to_root_local(transform, point));
        let wall_clasp_time = edge_grab.map(|e| e.wall_clasp_timer).unwrap_or(0.0);
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
            CartoonPose::Hoverboard => 5.8,
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

        let hanging = edge_grab.map(|e| e.is_hanging).unwrap_or(false);
        let mut ik = build_character_ik_pose(IkPoseInput {
            pose: animator.pose,
            phase: animator.phase,
            scale,
            visual_ground_lift,
            grounded,
            hanging,
            wall_normal_local,
            local_velocity,
            dodge_direction_local,
            grapple_attach_local,
            wall_clasp_time,
        });
        let hands = build_hand_engine(HandEngineInput {
            pose: animator.pose,
            phase: animator.phase,
            scale,
            visual_ground_lift,
            fire: player_input.map(|input| input.fire).unwrap_or(false),
            aim: player_input.map(|input| input.aim).unwrap_or(false),
            melee_light: player_input.map(|input| input.melee_light).unwrap_or(false),
            melee_heavy: player_input.map(|input| input.melee_heavy).unwrap_or(false),
            sabre_active: sabre.map(|s| s.unlocked && s.active).unwrap_or(false),
            sabre_two_handed: sabre
                .map(|s| {
                    s.unlocked && s.active && s.technique_timer > 0.0 && s.technique.is_two_handed()
                })
                .unwrap_or(false),
            sabre_slashing,
            parrying: parry.map(|p| p.is_parrying).unwrap_or(false),
            hanging,
            wall_sliding: current_state == Some(PlayerState::WallSliding),
            traversal_mode: traversal
                .map(|t| t.active)
                .unwrap_or(TraversalMode::Grapple),
            grapple_attach_local,
        });
        apply_hand_engine_to_ik(&mut ik, hands);
        if let Ok((hand_engine, root_ik_pose)) = root_pose_state.get_mut(entity) {
            if let Some(mut hand_engine) = hand_engine {
                *hand_engine = hands;
            }
            if let Some(mut root_ik_pose) = root_ik_pose {
                *root_ik_pose = ik;
            }
        }

        let sample = PoseSample {
            pose: animator.pose,
            phase: animator.phase,
            speed: animator.speed,
            scale,
            stride,
            agility,
            vertical_velocity,
            local_velocity,
            grounded,
            wall_normal_local,
            hanging,
            dodge_direction_local,
            grapple_attach_local,
            ik,
            hands,
            wall_clasp_time,
            board_trick,
            board_carve,
            sabre_swing,
            landing_compression: motion_animation
                .map(MotionAnimationState::landing_compression)
                .unwrap_or(0.0),
        };
        commands.entity(entity).insert(sample);
        samples.insert(entity, sample);
    }

    let joint_rests = joints
        .iter()
        .map(|(marker, _)| ((marker.root, marker.kind), marker.rest_translation))
        .collect::<HashMap<_, _>>();

    for (marker, mut transform) in joints.iter_mut() {
        let Some(sample) = samples.get(&marker.root) else {
            continue;
        };
        apply_joint_pose(marker, &mut transform, *sample, &joint_rests);
    }

    for (part, mut transform) in parts.iter_mut() {
        let Some(sample) = samples.get(&part.root) else {
            continue;
        };
        apply_part_pose(part, &mut transform, *sample);
    }
}

/// Bevy evaluates graph clips in `PostUpdate`. Reapply only the procedural
/// layers owned by Starfall after that evaluation so graph-controlled torso,
/// shoulder, and hip joints retain velocity response and contact IK. The base
/// pose remains graph-authored; unsupported poses stay wholly procedural.
fn apply_post_graph_motion_response(
    roots: Query<(
        Entity,
        &PoseSample,
        &crate::character::animation_mvp::CharacterAnimationBackend,
    )>,
    mut joints: Query<(&JointMarker, &mut Transform), Without<CartoonPart>>,
) {
    let samples_by_root = roots
        .iter()
        .filter_map(|(entity, sample, backend)| {
            (*backend == crate::character::animation_mvp::CharacterAnimationBackend::GraphMvp)
                .then_some((entity, *sample))
        })
        .collect::<HashMap<_, _>>();
    if samples_by_root.is_empty() {
        return;
    }
    let joint_rests = joints
        .iter()
        .map(|(marker, _)| ((marker.root, marker.kind), marker.rest_translation))
        .collect::<HashMap<_, _>>();

    for (marker, mut transform) in joints.iter_mut() {
        if !crate::character::animation_mvp::graph_owns_joint(marker.kind) {
            continue;
        }
        let Some(sample) = samples_by_root.get(&marker.root).copied() else {
            continue;
        };
        apply_motion_response(marker, &mut transform, sample);
        apply_contact_ik(marker, &mut transform, sample, &joint_rests);
    }
}

fn studio_human_animation_system(
    players: Query<&CartoonAnimator>,
    mut roots: Query<(Entity, &PlayableStudioHuman, &mut Transform)>,
    mut parts: Query<(&StudioHumanPart, &mut Transform), Without<PlayableStudioHuman>>,
) {
    let mut poses = HashMap::new();
    for (root_entity, visual, mut transform) in roots.iter_mut() {
        let Ok(animator) = players.get(visual.owner) else {
            continue;
        };
        *transform = visual.rest;
        let wave = animator.phase.sin();
        match animator.pose {
            CartoonPose::Idle => transform.translation.y += wave * 0.012,
            CartoonPose::Walk => transform.translation.y += wave.abs() * 0.018,
            CartoonPose::Run => {
                transform.translation.y += wave.abs() * 0.032;
                transform.rotation *= Quat::from_rotation_x(-0.08);
            }
            CartoonPose::Sprint => {
                transform.translation.y += wave.abs() * 0.045;
                transform.rotation *= Quat::from_rotation_x(-0.16);
            }
            CartoonPose::Jump => {
                transform.rotation *= Quat::from_rotation_x(-0.08);
                transform.scale *= Vec3::new(0.97, 1.035, 0.97);
            }
            CartoonPose::Fall => {
                transform.rotation *= Quat::from_rotation_x(0.10);
                transform.scale *= Vec3::new(1.025, 0.97, 1.025);
            }
            CartoonPose::Fly | CartoonPose::Glide | CartoonPose::AirDash => {
                let pitch = if animator.pose == CartoonPose::AirDash {
                    -0.48
                } else if animator.pose == CartoonPose::Glide {
                    -0.28
                } else {
                    -0.38
                };
                transform.rotation *= Quat::from_rotation_x(pitch);
                transform.translation.y += wave * 0.018;
            }
            CartoonPose::Hover => transform.translation.y += wave * 0.028,
            CartoonPose::Attack | CartoonPose::SabreSlash => {
                transform.rotation *= Quat::from_rotation_y(wave * 0.06);
            }
            _ => {}
        }
        poses.insert(root_entity, (animator.pose, animator.phase));
    }

    for (part, mut transform) in parts.iter_mut() {
        let Some((pose, phase)) = poses.get(&part.root).copied() else {
            continue;
        };
        let wave = phase.sin();
        let side = match part.region {
            StudioBodyRegion::LeftArm | StudioBodyRegion::LeftLeg => -1.0,
            StudioBodyRegion::RightArm | StudioBodyRegion::RightLeg => 1.0,
            _ => 0.0,
        };
        let rotation = match pose {
            CartoonPose::Walk => match part.region {
                StudioBodyRegion::LeftArm | StudioBodyRegion::RightArm => {
                    let (left, right) = locomotion_arm_pitches(pose, phase, 1.0);
                    Quat::from_rotation_x(if side < 0.0 { left } else { right })
                }
                StudioBodyRegion::LeftLeg | StudioBodyRegion::RightLeg => {
                    Quat::from_rotation_x(wave * side * 0.52)
                }
                StudioBodyRegion::Torso => Quat::from_rotation_z(wave * 0.025),
                StudioBodyRegion::Head => Quat::from_rotation_y(wave * 0.035),
            },
            CartoonPose::Run | CartoonPose::Sprint => {
                let amount = if pose == CartoonPose::Sprint {
                    0.92
                } else {
                    0.72
                };
                match part.region {
                    StudioBodyRegion::LeftArm | StudioBodyRegion::RightArm => {
                        let (left, right) = locomotion_arm_pitches(pose, phase, 1.0);
                        Quat::from_rotation_x(if side < 0.0 { left } else { right })
                    }
                    StudioBodyRegion::LeftLeg | StudioBodyRegion::RightLeg => {
                        Quat::from_rotation_x(wave * side * amount * 0.92)
                    }
                    StudioBodyRegion::Torso => Quat::from_rotation_z(wave * 0.045),
                    StudioBodyRegion::Head => Quat::from_rotation_y(wave * 0.055),
                }
            }
            CartoonPose::Jump => match part.region {
                StudioBodyRegion::LeftArm => Quat::from_rotation_z(-0.72),
                StudioBodyRegion::RightArm => Quat::from_rotation_z(0.72),
                StudioBodyRegion::LeftLeg => Quat::from_rotation_x(-0.30),
                StudioBodyRegion::RightLeg => Quat::from_rotation_x(0.30),
                _ => Quat::IDENTITY,
            },
            CartoonPose::Fall | CartoonPose::Glide => match part.region {
                StudioBodyRegion::LeftArm => Quat::from_rotation_z(-1.02),
                StudioBodyRegion::RightArm => Quat::from_rotation_z(1.02),
                StudioBodyRegion::LeftLeg => Quat::from_rotation_x(-0.16),
                StudioBodyRegion::RightLeg => Quat::from_rotation_x(0.16),
                _ => Quat::IDENTITY,
            },
            CartoonPose::Fly | CartoonPose::Hover | CartoonPose::AirDash => match part.region {
                StudioBodyRegion::LeftArm => {
                    Quat::from_rotation_z(-0.46) * Quat::from_rotation_x(0.20 + wave * 0.08)
                }
                StudioBodyRegion::RightArm => {
                    Quat::from_rotation_z(0.46) * Quat::from_rotation_x(0.20 - wave * 0.08)
                }
                StudioBodyRegion::LeftLeg => Quat::from_rotation_x(-0.10),
                StudioBodyRegion::RightLeg => Quat::from_rotation_x(0.10),
                _ => Quat::IDENTITY,
            },
            CartoonPose::Attack => match part.region {
                StudioBodyRegion::RightArm => Quat::from_rotation_x(1.24),
                StudioBodyRegion::LeftArm => {
                    Quat::from_rotation_x(0.78) * Quat::from_rotation_z(-0.20)
                }
                StudioBodyRegion::Torso => Quat::from_rotation_y(-0.10),
                _ => Quat::IDENTITY,
            },
            CartoonPose::SabreSlash => match part.region {
                StudioBodyRegion::RightArm => {
                    Quat::from_rotation_y(wave * 0.92) * Quat::from_rotation_x(0.86)
                }
                StudioBodyRegion::LeftArm => Quat::from_rotation_x(0.42),
                StudioBodyRegion::Torso => Quat::from_rotation_y(wave * 0.22),
                _ => Quat::IDENTITY,
            },
            _ => Quat::IDENTITY,
        };
        transform.translation = part.pivot + rotation * (part.rest.translation - part.pivot);
        transform.rotation = rotation * part.rest.rotation;
        transform.scale = part.rest.scale;

        // Facial pieces have semantic tags from the procedural generator.
        // A short, deterministic blink works in every locomotion state; attack
        // poses add a readable shout and stronger brows without requiring an
        // authored skeletal face rig.
        let blink = (phase * 0.23).sin().abs().powf(38.0);
        let attacking = matches!(pose, CartoonPose::Attack | CartoonPose::SabreSlash);
        match part.face_feature {
            Some(StudioFaceFeature::LeftEye | StudioFaceFeature::RightEye) => {
                transform.scale.y *= (1.0 - blink * 0.90).max(0.08);
            }
            Some(StudioFaceFeature::LeftBrow) => {
                transform.rotation *= Quat::from_rotation_z(if attacking { -0.16 } else { 0.0 });
                transform.translation.y += blink * 0.008;
            }
            Some(StudioFaceFeature::RightBrow) => {
                transform.rotation *= Quat::from_rotation_z(if attacking { 0.16 } else { 0.0 });
                transform.translation.y += blink * 0.008;
            }
            Some(StudioFaceFeature::Mouth) => {
                let talk = if attacking {
                    2.2
                } else if matches!(
                    pose,
                    CartoonPose::Run | CartoonPose::Sprint | CartoonPose::Fly
                ) {
                    1.0 + phase.sin().abs() * 0.35
                } else {
                    1.0
                };
                transform.scale.y *= talk;
            }
            None => {}
        }
    }
}

fn apply_joint_pose(
    marker: &JointMarker,
    transform: &mut Transform,
    sample: PoseSample,
    joint_rests: &HashMap<(Entity, JointKind), Vec3>,
) {
    transform.translation = marker.local_translation;
    transform.rotation = marker.rest_rotation;
    transform.scale = marker.rest_scale;

    let wave = sample.phase.sin();
    let step = (sample.phase * 1.1).sin();
    let walk_amount = (sample.speed * (2.8 / sample.stride)).clamp(0.0, 1.0);
    let swing = sample.agility.clamp(0.72, 1.28);

    match sample.pose {
        CartoonPose::Idle => match marker.kind {
            JointKind::Pelvis => transform.translation.y += wave * 0.018,
            JointKind::Spine | JointKind::Chest => {
                transform.rotation *= Quat::from_rotation_x(-0.025 + wave * 0.018);
            }
            JointKind::Head | JointKind::Neck => {
                transform.rotation *= Quat::from_rotation_y(wave * 0.025);
            }
            JointKind::LeftShoulder => {
                transform.rotation *= Quat::from_rotation_z(-0.08 + wave * 0.035);
            }
            JointKind::RightShoulder => {
                transform.rotation *= Quat::from_rotation_z(0.08 - wave * 0.035);
            }
            _ => {}
        },
        CartoonPose::Walk | CartoonPose::Run | CartoonPose::Sprint => {
            let run_scale = match sample.pose {
                CartoonPose::Walk => 0.72 * walk_amount,
                CartoonPose::Run => 1.0,
                CartoonPose::Sprint => 1.22,
                _ => 1.0,
            } * swing;
            let lean = match sample.pose {
                CartoonPose::Walk => -0.04,
                CartoonPose::Run => -0.12,
                CartoonPose::Sprint => -0.22,
                _ => 0.0,
            };
            let (left_arm_pitch, right_arm_pitch) =
                locomotion_arm_pitches(sample.pose, sample.phase, sample.agility);
            match marker.kind {
                JointKind::Pelvis => {
                    transform.translation.y += wave.abs() * 0.055 * run_scale.max(0.4);
                    transform.rotation *= Quat::from_rotation_z(step * 0.045 * run_scale);
                }
                JointKind::Spine | JointKind::Chest => {
                    transform.rotation *=
                        Quat::from_rotation_x(lean) * Quat::from_rotation_z(step * 0.035);
                }
                JointKind::Head | JointKind::Neck => {
                    transform.rotation *=
                        Quat::from_rotation_x(lean * 0.45) * Quat::from_rotation_y(step * 0.08);
                }
                JointKind::LeftShoulder => {
                    transform.rotation *= Quat::from_rotation_x(left_arm_pitch);
                }
                JointKind::RightShoulder => {
                    transform.rotation *= Quat::from_rotation_x(right_arm_pitch);
                }
                JointKind::LeftElbow => {
                    transform.rotation *= Quat::from_rotation_x(-0.46 - step.abs() * 0.16);
                }
                JointKind::RightElbow => {
                    transform.rotation *= Quat::from_rotation_x(-0.46 - step.abs() * 0.16);
                }
                JointKind::LeftHip => {
                    transform.rotation *= Quat::from_rotation_x(-step * 0.86 * run_scale);
                }
                JointKind::RightHip => {
                    transform.rotation *= Quat::from_rotation_x(step * 0.86 * run_scale);
                }
                JointKind::LeftKnee => {
                    transform.rotation *= Quat::from_rotation_x(step.max(0.0) * 0.36 * run_scale);
                }
                JointKind::RightKnee => {
                    transform.rotation *=
                        Quat::from_rotation_x((-step).max(0.0) * 0.36 * run_scale);
                }
                _ => {}
            }
        }
        CartoonPose::Jump => match marker.kind {
            JointKind::Pelvis => transform.translation.y += 0.08,
            JointKind::Spine | JointKind::Chest => {
                transform.rotation *= Quat::from_rotation_x(-0.16);
            }
            JointKind::LeftShoulder => transform.rotation *= Quat::from_rotation_x(-1.00),
            JointKind::RightShoulder => transform.rotation *= Quat::from_rotation_x(-0.82),
            JointKind::LeftHip => transform.rotation *= Quat::from_rotation_x(0.38),
            JointKind::RightHip => transform.rotation *= Quat::from_rotation_x(-0.30),
            JointKind::LeftKnee | JointKind::RightKnee => {
                transform.rotation *= Quat::from_rotation_x(0.28);
            }
            _ => {}
        },
        CartoonPose::Fall
        | CartoonPose::Fly
        | CartoonPose::Glide
        | CartoonPose::Hover
        | CartoonPose::AirDash => {
            let fall_pull = (-sample.vertical_velocity).clamp(0.0, 2.2);
            let pitch = match sample.pose {
                CartoonPose::Fly => -0.36,
                CartoonPose::Glide => -0.22,
                CartoonPose::Hover => -0.06,
                CartoonPose::AirDash => -0.48,
                _ => 0.08 + fall_pull * 0.04,
            };
            match marker.kind {
                JointKind::Spine | JointKind::Chest => {
                    transform.rotation *= Quat::from_rotation_x(pitch);
                }
                JointKind::Head | JointKind::Neck => {
                    transform.rotation *= Quat::from_rotation_x(pitch * 0.55);
                }
                JointKind::LeftShoulder => {
                    transform.rotation *= Quat::from_rotation_z(-0.82 + wave * 0.06);
                }
                JointKind::RightShoulder => {
                    transform.rotation *= Quat::from_rotation_z(0.82 - wave * 0.06);
                }
                JointKind::LeftHip => transform.rotation *= Quat::from_rotation_x(0.26),
                JointKind::RightHip => transform.rotation *= Quat::from_rotation_x(0.18),
                _ => {}
            }
        }
        // Same sideways skate stance as the cartoon-part path, mapped onto a
        // joint rig so imported/Studio characters ride identically.
        CartoonPose::Hoverboard => {
            let board = board_stance_pose(sample);
            let idle_sway = (sample.phase * 0.8).sin() * 0.04;
            match marker.kind {
                JointKind::Pelvis => {
                    transform.translation.y -= board.crouch;
                    transform.translation.x += board.torso_roll * 0.06 * sample.scale;
                    transform.rotation *= Quat::from_rotation_y(board.stance_yaw)
                        * Quat::from_rotation_z(board.torso_roll + idle_sway);
                }
                JointKind::Spine | JointKind::Chest => {
                    transform.rotation *= Quat::from_rotation_x(board.torso_pitch)
                        * Quat::from_rotation_z(-board.torso_roll * 0.35);
                }
                JointKind::Head | JointKind::Neck => {
                    // Keep the eyes down the direction of travel.
                    transform.rotation *= Quat::from_rotation_y(-board.stance_yaw * 0.62)
                        * Quat::from_rotation_x(board.head_pitch);
                }
                JointKind::LeftShoulder => {
                    transform.rotation *= Quat::from_rotation_z(board.left_arm_out)
                        * Quat::from_rotation_x(board.left_arm_pitch);
                }
                JointKind::RightShoulder => {
                    transform.rotation *= Quat::from_rotation_z(board.right_arm_out)
                        * Quat::from_rotation_x(board.right_arm_pitch);
                }
                JointKind::LeftHip => {
                    transform.rotation *= Quat::from_rotation_y(board.front_foot_yaw)
                        * Quat::from_rotation_x(board.front_knee * 0.55)
                        * Quat::from_rotation_z(-0.12);
                }
                JointKind::RightHip => {
                    transform.rotation *= Quat::from_rotation_y(board.back_foot_yaw)
                        * Quat::from_rotation_x(board.back_knee * 0.55)
                        * Quat::from_rotation_z(0.12);
                }
                JointKind::LeftKnee => {
                    transform.rotation *= Quat::from_rotation_x(board.front_knee);
                }
                JointKind::RightKnee => {
                    transform.rotation *= Quat::from_rotation_x(board.back_knee);
                }
                _ => {}
            }
        }
        CartoonPose::WallSlide => {
            let scrape = (sample.wall_clasp_time * 18.0).sin() * 0.035;
            match marker.kind {
                JointKind::Pelvis => transform.translation.x += scrape,
                JointKind::Spine | JointKind::Chest => {
                    transform.rotation *=
                        Quat::from_rotation_z(-0.16) * Quat::from_rotation_x(0.10);
                }
                JointKind::Head | JointKind::Neck => {
                    transform.rotation *= Quat::from_rotation_z(-0.10);
                }
                JointKind::LeftShoulder => {
                    transform.rotation *=
                        Quat::from_rotation_x(-1.38) * Quat::from_rotation_z(-0.36);
                }
                JointKind::RightShoulder => {
                    transform.rotation *=
                        Quat::from_rotation_x(-0.28) * Quat::from_rotation_z(0.58);
                }
                JointKind::LeftHip => {
                    transform.rotation *=
                        Quat::from_rotation_x(0.34) * Quat::from_rotation_z(-0.16);
                }
                JointKind::RightHip => {
                    transform.rotation *=
                        Quat::from_rotation_x(-0.42) * Quat::from_rotation_z(0.22);
                }
                _ => {}
            }
        }
        CartoonPose::Grapple | CartoonPose::Swing | CartoonPose::Zip | CartoonPose::Hang => {
            let is_zip = sample.pose == CartoonPose::Zip;
            let overhead = if sample.pose == CartoonPose::Hang || sample.pose == CartoonPose::Swing
            {
                -1.55
            } else if is_zip {
                -1.34
            } else {
                -1.15
            };
            match marker.kind {
                JointKind::Spine | JointKind::Chest => {
                    let pitch = if is_zip { -0.32 } else { -0.12 };
                    transform.rotation *=
                        Quat::from_rotation_x(pitch) * Quat::from_rotation_y(0.16);
                }
                JointKind::Head | JointKind::Neck => {
                    transform.rotation *=
                        Quat::from_rotation_x(-0.08) * Quat::from_rotation_y(0.20);
                }
                JointKind::LeftShoulder => {
                    transform.rotation *=
                        Quat::from_rotation_x(overhead * 0.86) * Quat::from_rotation_z(-0.24);
                }
                JointKind::RightShoulder => {
                    transform.rotation *=
                        Quat::from_rotation_x(overhead) * Quat::from_rotation_z(0.22);
                }
                JointKind::LeftElbow | JointKind::RightElbow => {
                    transform.rotation *= Quat::from_rotation_x(-0.22);
                }
                JointKind::LeftHip => {
                    transform.rotation *= Quat::from_rotation_x(if is_zip { 0.34 } else { 0.16 });
                }
                JointKind::RightHip => {
                    transform.rotation *= Quat::from_rotation_x(if is_zip { 0.22 } else { -0.18 });
                }
                _ => {}
            }
        }
        CartoonPose::Roll | CartoonPose::Parry => {
            let is_parry = sample.pose == CartoonPose::Parry;
            match marker.kind {
                JointKind::Pelvis => transform.translation.y -= if is_parry { 0.02 } else { 0.12 },
                JointKind::Spine | JointKind::Chest => {
                    transform.rotation *= if is_parry {
                        Quat::from_rotation_y(-0.18) * Quat::from_rotation_x(-0.05)
                    } else {
                        Quat::from_rotation_x(0.62) * Quat::from_rotation_z(wave * 0.18)
                    };
                }
                JointKind::Head | JointKind::Neck => {
                    transform.rotation *= if is_parry {
                        Quat::from_rotation_y(-0.16)
                    } else {
                        Quat::from_rotation_x(0.38)
                    };
                }
                JointKind::LeftShoulder => {
                    transform.rotation *=
                        Quat::from_rotation_x(-1.0) * Quat::from_rotation_z(-0.55);
                }
                JointKind::RightShoulder => {
                    transform.rotation *= Quat::from_rotation_x(-1.1) * Quat::from_rotation_z(0.52);
                }
                JointKind::LeftHip | JointKind::RightHip => {
                    transform.rotation *= Quat::from_rotation_x(if is_parry { 0.16 } else { 0.55 });
                }
                _ => {}
            }
        }
        CartoonPose::Attack | CartoonPose::SabreSlash => {
            let sweep = (sample.phase * 1.25).sin();
            let snap = sweep.signum() * sweep.abs().sqrt();
            let is_sabre = sample.pose == CartoonPose::SabreSlash;
            match marker.kind {
                JointKind::Spine | JointKind::Chest => {
                    transform.rotation *= if is_sabre {
                        Quat::from_rotation_y(0.42 * snap) * Quat::from_rotation_z(-0.08 * snap)
                    } else {
                        Quat::from_rotation_y(0.18 + sweep * 0.08) * Quat::from_rotation_x(-0.06)
                    };
                }
                JointKind::Head | JointKind::Neck => {
                    transform.rotation *= Quat::from_rotation_y(if is_sabre {
                        0.30 * snap
                    } else {
                        0.12 + sweep * 0.05
                    });
                }
                JointKind::LeftShoulder => {
                    transform.rotation *= if is_sabre {
                        Quat::from_rotation_x(-0.78)
                            * Quat::from_rotation_y(-0.28)
                            * Quat::from_rotation_z(-0.72 * snap)
                    } else {
                        Quat::from_rotation_x(-0.55) * Quat::from_rotation_z(-0.35)
                    };
                }
                JointKind::RightShoulder => {
                    transform.rotation *= if is_sabre {
                        Quat::from_rotation_x(-1.55)
                            * Quat::from_rotation_y(0.34)
                            * Quat::from_rotation_z(1.05 * snap)
                    } else {
                        Quat::from_rotation_x(-1.12 + sweep * 0.12) * Quat::from_rotation_z(0.42)
                    };
                }
                JointKind::LeftHip => {
                    transform.rotation *=
                        Quat::from_rotation_x(if is_sabre { -0.22 } else { -0.10 });
                }
                JointKind::RightHip => {
                    transform.rotation *= Quat::from_rotation_x(if is_sabre { 0.24 } else { 0.16 });
                }
                _ => {}
            }
        }
        CartoonPose::Stun | CartoonPose::Death | CartoonPose::HardLand | CartoonPose::Mantle => {
            match marker.kind {
                JointKind::Pelvis => transform.translation.y -= 0.08,
                JointKind::Spine | JointKind::Chest => {
                    transform.rotation *= Quat::from_rotation_x(0.28);
                }
                JointKind::Head | JointKind::Neck => {
                    transform.rotation *= Quat::from_rotation_x(0.18);
                }
                _ => {}
            }
        }
    }

    apply_motion_response(marker, transform, sample);
    apply_contact_ik(marker, transform, sample, joint_rests);
}

fn apply_motion_response(marker: &JointMarker, transform: &mut Transform, sample: PoseSample) {
    let scale = sample.scale.max(0.1);
    let horizontal_velocity = sample.local_velocity.with_y(0.0);
    let speed_norm = (horizontal_velocity.length() * 1.45).clamp(0.0, 1.0);
    let grounded = if sample.grounded { 1.0 } else { 0.55 };
    let strafe = (sample.local_velocity.x * 1.4).clamp(-1.0, 1.0);
    let forward = (-sample.local_velocity.z * 1.2).clamp(-1.0, 1.0);
    let dodge = normalized_or_zero(sample.dodge_direction_local);
    let wall = normalized_or_zero(sample.wall_normal_local);
    let hook = sample
        .grapple_attach_local
        .map(normalized_or_zero)
        .unwrap_or(Vec3::ZERO);

    match marker.kind {
        JointKind::Pelvis => {
            transform.translation.x += strafe * 0.018 * scale * grounded;
            transform.translation.y -= speed_norm * 0.012 * scale * grounded;
            transform.translation.y -= sample.landing_compression * 0.085 * scale;
            transform.rotation *= Quat::from_rotation_z(-strafe * 0.07 * speed_norm);
            if sample.pose == CartoonPose::Roll {
                transform.translation.x += dodge.x * 0.055 * scale;
                transform.rotation *= Quat::from_rotation_z(-dodge.x * 0.22);
            }
        }
        JointKind::Spine => {
            transform.rotation *= Quat::from_rotation_x(-forward * 0.055 * speed_norm * grounded)
                * Quat::from_rotation_y(strafe * 0.05 * speed_norm + hook.x * 0.04)
                * Quat::from_rotation_z(-strafe * 0.035 * speed_norm);
            transform.rotation *= Quat::from_rotation_x(sample.landing_compression * 0.12);
            if sample.pose == CartoonPose::WallSlide {
                transform.rotation *= Quat::from_rotation_z(-wall.x * 0.08);
            }
        }
        JointKind::Chest => {
            transform.rotation *= Quat::from_rotation_x(-forward * 0.075 * speed_norm * grounded)
                * Quat::from_rotation_y(strafe * 0.075 * speed_norm + hook.x * 0.09)
                * Quat::from_rotation_z(-strafe * 0.05 * speed_norm + dodge.x * 0.06);
            transform.rotation *= Quat::from_rotation_x(sample.landing_compression * 0.09);
            if sample.hanging {
                transform.rotation *= Quat::from_rotation_x(0.08);
            }
            if sample.pose == CartoonPose::WallSlide {
                transform.translation.z += wall.z * 0.018 * scale;
                transform.rotation *= Quat::from_rotation_z(-wall.x * 0.12);
            }
        }
        JointKind::Neck | JointKind::Head => {
            if let Some(look) = sample.ik.look_at {
                let dir = normalized_or_zero(look);
                if dir != Vec3::ZERO {
                    let yaw = dir.x.atan2(-dir.z).clamp(-0.55, 0.55);
                    let pitch = dir
                        .y
                        .atan2(Vec2::new(dir.x, dir.z).length().max(0.001))
                        .clamp(-0.34, 0.34);
                    let head_weight = if marker.kind == JointKind::Head {
                        0.62
                    } else {
                        0.36
                    };
                    transform.rotation *= Quat::from_rotation_y(yaw * head_weight)
                        * Quat::from_rotation_x(-pitch * head_weight);
                }
            } else {
                transform.rotation *= Quat::from_rotation_y(strafe * 0.055 * speed_norm)
                    * Quat::from_rotation_x(-forward * 0.035 * speed_norm);
            }
        }
        JointKind::LeftShoulder => {
            transform.rotation *= Quat::from_rotation_z(-strafe.max(0.0) * 0.05 * speed_norm);
        }
        JointKind::RightShoulder => {
            transform.rotation *= Quat::from_rotation_z((-strafe).max(0.0) * 0.05 * speed_norm);
        }
        _ => {}
    }
}

fn apply_contact_ik(
    marker: &JointMarker,
    transform: &mut Transform,
    sample: PoseSample,
    joint_rests: &HashMap<(Entity, JointKind), Vec3>,
) {
    if let Some(solution) = solve_leg_ik(marker.root, true, sample.ik.left_foot, joint_rests) {
        apply_leg_ik_rotation(marker.kind, transform, true, solution);
    }
    if let Some(solution) = solve_leg_ik(marker.root, false, sample.ik.right_foot, joint_rests) {
        apply_leg_ik_rotation(marker.kind, transform, false, solution);
    }

    if let Some(target) = ik_target_for_joint(sample.ik, marker.kind) {
        let translation_weight = match marker.kind {
            JointKind::LeftAnkle | JointKind::RightAnkle => 0.42,
            JointKind::LeftWrist | JointKind::RightWrist => 0.78,
            _ => 0.0,
        };
        apply_root_local_target_translation(marker, transform, target, translation_weight);
    }
}

fn apply_leg_ik_rotation(
    joint: JointKind,
    transform: &mut Transform,
    left: bool,
    solution: TwoBoneIkAngles,
) {
    let (hip, knee, ankle) = if left {
        (
            JointKind::LeftHip,
            JointKind::LeftKnee,
            JointKind::LeftAnkle,
        )
    } else {
        (
            JointKind::RightHip,
            JointKind::RightKnee,
            JointKind::RightAnkle,
        )
    };

    if joint == hip {
        transform.rotation *= Quat::from_rotation_x(solution.hip_pitch);
    } else if joint == knee {
        transform.rotation *= Quat::from_rotation_x(solution.knee_pitch);
    } else if joint == ankle {
        transform.rotation *= Quat::from_rotation_x(solution.ankle_pitch);
    }
}

fn apply_root_local_target_translation(
    marker: &JointMarker,
    transform: &mut Transform,
    target: IkTarget,
    weight_scale: f32,
) {
    if target.joint != marker.kind {
        return;
    }
    let weight = (target.weight * weight_scale).clamp(0.0, 1.0);
    if weight <= 0.0 {
        return;
    }

    let parent_rest = marker.rest_translation - marker.local_translation;
    let local_target = target.target - parent_rest;
    transform.translation = transform.translation.lerp(local_target, weight);
}

fn ik_target_for_joint(ik: CharacterIkPose, joint: JointKind) -> Option<IkTarget> {
    match joint {
        JointKind::LeftAnkle => ik.left_foot,
        JointKind::RightAnkle => ik.right_foot,
        JointKind::LeftWrist => ik.left_hand,
        JointKind::RightWrist => ik.right_hand,
        _ => None,
    }
    .filter(|target| target.joint == joint)
}

#[derive(Debug, Clone, Copy, Default)]
struct TwoBoneIkAngles {
    hip_pitch: f32,
    knee_pitch: f32,
    ankle_pitch: f32,
}

fn solve_leg_ik(
    root: Entity,
    left: bool,
    target: Option<IkTarget>,
    joint_rests: &HashMap<(Entity, JointKind), Vec3>,
) -> Option<TwoBoneIkAngles> {
    let (hip, knee, ankle) = if left {
        (
            JointKind::LeftHip,
            JointKind::LeftKnee,
            JointKind::LeftAnkle,
        )
    } else {
        (
            JointKind::RightHip,
            JointKind::RightKnee,
            JointKind::RightAnkle,
        )
    };
    let target = target.filter(|target| target.joint == ankle && target.weight > 0.0)?;
    solve_two_bone_leg_ik(
        *joint_rests.get(&(root, hip))?,
        *joint_rests.get(&(root, knee))?,
        *joint_rests.get(&(root, ankle))?,
        target,
    )
}

fn solve_two_bone_leg_ik(
    hip: Vec3,
    knee: Vec3,
    ankle_rest: Vec3,
    target: IkTarget,
) -> Option<TwoBoneIkAngles> {
    let upper = hip.distance(knee).max(0.001);
    let lower = knee.distance(ankle_rest).max(0.001);
    let to_target = target.target - hip;
    let reach = to_target.length();
    if reach <= 0.001 {
        return None;
    }

    let max_reach = (upper + lower - 0.001).max(0.001);
    let min_reach = ((upper - lower).abs() + 0.001).min(max_reach);
    let clamped_reach = reach.clamp(min_reach, max_reach);
    let hip_cos = ((upper.powi(2) + clamped_reach.powi(2) - lower.powi(2))
        / (2.0 * upper * clamped_reach))
        .clamp(-1.0, 1.0);
    let knee_cos = ((upper.powi(2) + lower.powi(2) - clamped_reach.powi(2))
        / (2.0 * upper * lower))
        .clamp(-1.0, 1.0);

    let target_pitch = to_target.z.atan2((-to_target.y).max(0.001));
    let hip_bend = hip_cos.acos();
    let knee_bend = PI - knee_cos.acos();
    let weight = target.weight.clamp(0.0, 1.0);
    let hip_pitch = (target_pitch - hip_bend * 0.34).clamp(-0.9, 0.9) * weight;
    let knee_pitch = (knee_bend * 0.78).clamp(0.0, 1.35) * weight;
    let ankle_pitch = (-hip_pitch - knee_pitch * 0.42).clamp(-0.8, 0.8) * weight;

    Some(TwoBoneIkAngles {
        hip_pitch,
        knee_pitch,
        ankle_pitch,
    })
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
            let (left_arm_pitch, right_arm_pitch) =
                locomotion_arm_pitches(sample.pose, sample.phase, sample.agility);
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
                    transform.rotation *= Quat::from_rotation_x(left_arm_pitch);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *= Quat::from_rotation_x(right_arm_pitch);
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
            let (left_arm_pitch, right_arm_pitch) =
                locomotion_arm_pitches(sample.pose, sample.phase, sample.agility);
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
                    transform.rotation *= Quat::from_rotation_x(left_arm_pitch);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *= Quat::from_rotation_x(right_arm_pitch);
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

        // ── Hoverboard ───────────────────────────────────────────────────────
        // Skate stance: the rider stands *sideways* on the deck (regular
        // stance, left shoulder leading), knees bent, arms out for balance.
        // Carving leans the whole body into the turn while the arms
        // counter-rotate, and an active trick layers on top of that base.
        CartoonPose::Hoverboard => {
            let board = board_stance_pose(sample);
            let idle_sway = (sample.phase * 0.8).sin() * 0.04;
            match part.kind {
                CartoonPartKind::Body => {
                    transform.translation.y -= board.crouch;
                    transform.rotation *= Quat::from_rotation_y(board.stance_yaw)
                        * Quat::from_rotation_x(board.torso_pitch)
                        * Quat::from_rotation_z(board.torso_roll + idle_sway);
                }
                CartoonPartKind::Head
                | CartoonPartKind::Hair
                | CartoonPartKind::Hood
                | CartoonPartKind::Hat => {
                    transform.translation.y -= board.crouch * 0.5;
                    // The head stays looking down the direction of travel,
                    // counter-rotating against the sideways torso.
                    transform.rotation *= Quat::from_rotation_y(-board.stance_yaw * 0.62)
                        * Quat::from_rotation_x(board.head_pitch);
                }
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.rotation *= Quat::from_rotation_z(board.left_arm_out)
                        * Quat::from_rotation_x(board.left_arm_pitch);
                }
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *= Quat::from_rotation_z(board.right_arm_out)
                        * Quat::from_rotation_x(board.right_arm_pitch);
                }
                CartoonPartKind::LeftLeg
                | CartoonPartKind::LeftFoot
                | CartoonPartKind::LeftBoot => {
                    // Front foot: angled across the deck, absorbing the ride.
                    transform.rotation *= Quat::from_rotation_y(board.front_foot_yaw)
                        * Quat::from_rotation_x(board.front_knee)
                        * Quat::from_rotation_z(-0.08);
                }
                CartoonPartKind::RightLeg
                | CartoonPartKind::RightFoot
                | CartoonPartKind::RightBoot => {
                    // Back foot over the tail, deeper bend when tucking.
                    transform.rotation *= Quat::from_rotation_y(board.back_foot_yaw)
                        * Quat::from_rotation_x(board.back_knee)
                        * Quat::from_rotation_z(0.08);
                }
                CartoonPartKind::Shadow => {
                    transform.scale *= Vec3::new(1.24, 1.0, 0.68);
                }
                _ => {}
            }
            if is_face {
                transform.translation.y -= board.crouch * 0.5;
                transform.rotation *= Quat::from_rotation_y(-board.stance_yaw * 0.62);
            }
            if is_body_acc {
                transform.translation.y -= board.crouch;
                transform.rotation *= Quat::from_rotation_y(board.stance_yaw)
                    * Quat::from_rotation_x(board.torso_pitch)
                    * Quat::from_rotation_z(board.torso_roll + idle_sway);
            }
            if is_cape {
                transform.rotation *= Quat::from_rotation_x(cape_flap * 1.25);
                transform.rotation *= Quat::from_rotation_z(board.torso_roll * cape_flap);
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
            // Driven by the real slash phase machine: wound back on startup,
            // snapping across the active window, settling through recovery.
            // Falls back to the old idle sway when no slash is live (e.g. the
            // sabre is merely drawn).
            let snap = if sample.sabre_swing.abs() > f32::EPSILON {
                sample.sabre_swing
            } else {
                let sweep = (sample.phase * 1.25).sin();
                sweep.signum() * sweep.abs().sqrt()
            };
            let sweep = snap.abs();
            // Torso counter-rotates and drops slightly as the blade crosses,
            // so the swing looks driven from the hips rather than the wrist.
            let commit = (1.0 - (snap.abs() - 1.0).abs()).clamp(0.0, 1.0);
            match part.kind {
                CartoonPartKind::Body => {
                    transform.rotation *= Quat::from_rotation_y(0.52 * snap)
                        * Quat::from_rotation_z(-0.12 * snap)
                        * Quat::from_rotation_x(-0.10 * commit);
                    transform.translation.y += sweep * 0.055 - commit * 0.04;
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
                // Off hand trails the swing for counter-balance.
                CartoonPartKind::LeftArm | CartoonPartKind::LeftHand => {
                    transform.rotation *= Quat::from_rotation_x(-0.78 - commit * 0.25)
                        * Quat::from_rotation_y(-0.28)
                        * Quat::from_rotation_z(-0.72 * snap);
                }
                // Blade hand: reaches overhead on the wind-up and drives all
                // the way across on the strike.
                CartoonPartKind::RightArm | CartoonPartKind::RightHand => {
                    transform.rotation *= Quat::from_rotation_x(-1.72 + commit * 0.85)
                        * Quat::from_rotation_y(0.34 + 0.30 * snap)
                        * Quat::from_rotation_z(1.15 * snap);
                }
                // Legs brace into the swing: front foot plants, back leg drives.
                CartoonPartKind::LeftLeg
                | CartoonPartKind::LeftFoot
                | CartoonPartKind::LeftBoot => {
                    transform.rotation *=
                        Quat::from_rotation_x(-0.26 - commit * 0.20) * Quat::from_rotation_z(0.10);
                }
                CartoonPartKind::RightLeg
                | CartoonPartKind::RightFoot
                | CartoonPartKind::RightBoot => {
                    transform.rotation *=
                        Quat::from_rotation_x(0.28 + commit * 0.18) * Quat::from_rotation_z(-0.12);
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

    apply_hand_grip_pose(part, transform, sample);
}

fn apply_hand_grip_pose(part: &CartoonPart, transform: &mut Transform, sample: PoseSample) {
    let side = match part.kind {
        CartoonPartKind::LeftHand => HandSide::Left,
        CartoonPartKind::RightHand => HandSide::Right,
        _ => return,
    };
    let hand = sample.hands.pose(side);
    let side_sign = side.sign();
    let curl = hand.curl.clamp(0.0, 1.0);
    let spread = hand.spread.clamp(0.0, 1.0);

    let squash = 1.0 - curl * 0.16;
    let knuckle = 1.0 + curl * 0.13;
    let fan = 1.0 + spread * 0.11;
    transform.scale *= Vec3::new(fan, squash, knuckle);
    transform.translation.z -= hand.recoil * 0.035 * sample.scale;
    transform.rotation *= Quat::from_rotation_x(-curl * 0.22 + hand.recoil * 0.10)
        * Quat::from_rotation_y(side_sign * spread * 0.12)
        * Quat::from_rotation_z(side_sign * hand.twist * 0.34);

    match hand.grip {
        HandGrip::Relaxed => {
            transform.rotation *= Quat::from_rotation_z(side_sign * 0.04);
        }
        HandGrip::Open | HandGrip::Balance => {
            transform.scale.x *= 1.06;
            transform.rotation *= Quat::from_rotation_y(side_sign * 0.10);
        }
        HandGrip::Fist => {
            transform.scale.y *= 0.90;
            transform.scale.z *= 1.08;
        }
        HandGrip::Trigger => {
            transform.scale.x *= 0.96;
            transform.rotation *= Quat::from_rotation_x(-0.08);
        }
        HandGrip::Sabre => {
            transform.scale.y *= 0.92;
            transform.rotation *= Quat::from_rotation_z(side_sign * 0.18);
        }
        HandGrip::Grapple | HandGrip::Climb => {
            transform.scale.z *= 1.12;
            transform.rotation *= Quat::from_rotation_x(-0.12);
        }
        HandGrip::Brace => {
            transform.scale.x *= 1.03;
            transform.rotation *= Quat::from_rotation_y(side_sign * 0.14);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rest_leg() -> (Vec3, Vec3, Vec3) {
        (
            Vec3::new(-0.33, -0.55, 0.0),
            Vec3::new(-0.33, -0.86, -0.015),
            Vec3::new(-0.33, -1.07, -0.025),
        )
    }

    #[test]
    fn two_bone_ik_keeps_rest_leg_relaxed() {
        let (hip, knee, ankle) = rest_leg();
        let solution = solve_two_bone_leg_ik(
            hip,
            knee,
            ankle,
            IkTarget::new(JointKind::LeftAnkle, ankle, 1.0),
        )
        .expect("rest leg should solve");

        assert!(solution.hip_pitch.abs() < 0.12);
        assert!(solution.knee_pitch < 0.14);
        assert!(solution.ankle_pitch.abs() < 0.16);
    }

    #[test]
    fn two_bone_ik_bends_knee_for_closer_target() {
        let (hip, knee, ankle) = rest_leg();
        let closer_target = ankle + Vec3::new(0.0, 0.22, 0.04);
        let solution = solve_two_bone_leg_ik(
            hip,
            knee,
            ankle,
            IkTarget::new(JointKind::LeftAnkle, closer_target, 1.0),
        )
        .expect("closer leg target should solve");

        assert!(solution.knee_pitch > 0.35);
        assert!(solution.ankle_pitch < 0.0);
    }

    fn hand_input() -> HandEngineInput {
        HandEngineInput {
            pose: CartoonPose::Idle,
            phase: 0.0,
            scale: 1.0,
            visual_ground_lift: 0.0,
            fire: false,
            aim: false,
            melee_light: false,
            melee_heavy: false,
            sabre_active: false,
            sabre_two_handed: false,
            sabre_slashing: false,
            parrying: false,
            hanging: false,
            wall_sliding: false,
            traversal_mode: TraversalMode::Grapple,
            grapple_attach_local: None,
        }
    }

    fn locomotion_pose_input(speed: f32) -> PoseInput {
        PoseInput {
            state: Some(PlayerState::Moving),
            grounded: true,
            horizontal_speed: speed,
            vertical_velocity: 0.0,
            hanging: false,
            wall_sliding: false,
            jetpack_active: false,
            flight_mode: FlightMode::Grounded,
            traversal_mode: TraversalMode::Grapple,
            grapple_mode: None,
            grapple_pose: false,
            sabre_slashing: false,
            melee_attacking: false,
            dodging: false,
            parrying: false,
            hard_landing: false,
            mantling: false,
        }
    }

    #[test]
    fn grounded_locomotion_reaches_walk_run_and_sprint_poses() {
        assert_eq!(
            select_cartoon_pose(locomotion_pose_input(0.30)),
            CartoonPose::Walk
        );
        assert_eq!(
            select_cartoon_pose(locomotion_pose_input(0.58)),
            CartoonPose::Run
        );
        let mut sprint = locomotion_pose_input(0.70);
        sprint.state = Some(PlayerState::Sprinting);
        assert_eq!(select_cartoon_pose(sprint), CartoonPose::Sprint);
    }

    #[test]
    fn landing_and_mantle_edges_reach_their_authored_poses() {
        let mut landing = locomotion_pose_input(0.0);
        landing.hard_landing = true;
        assert_eq!(select_cartoon_pose(landing), CartoonPose::HardLand);

        let mut mantle = locomotion_pose_input(0.0);
        mantle.mantling = true;
        assert_eq!(select_cartoon_pose(mantle), CartoonPose::Mantle);
    }

    #[test]
    fn running_arms_swing_forward_and_back_without_outward_flare() {
        let (left, right) = locomotion_arm_pitches(CartoonPose::Run, 1.0, 1.0);
        assert!(left.abs() > 0.4);
        assert!((left + right).abs() < f32::EPSILON);
        assert!(left.abs() <= 0.82);

        let (sprint_left, sprint_right) = locomotion_arm_pitches(CartoonPose::Sprint, 1.0, 1.0);
        assert!(sprint_left.abs() > left.abs());
        assert!((sprint_left + sprint_right).abs() < f32::EPSILON);
        assert!(sprint_left.abs() <= 1.02);
    }

    #[test]
    fn hand_engine_aim_uses_trigger_and_support_hand() {
        let mut input = hand_input();
        input.aim = true;

        let hands = build_hand_engine(input);

        assert_eq!(hands.right.grip, HandGrip::Trigger);
        assert_eq!(hands.left.grip, HandGrip::Brace);
        assert!(hands.right.ik_target(HandSide::Right).is_some());
    }

    #[test]
    fn hand_engine_grapple_overrides_sabre_hold() {
        let mut input = hand_input();
        input.sabre_active = true;
        let sabre = build_hand_engine(input);

        input.pose = CartoonPose::Grapple;
        input.grapple_attach_local = Some(Vec3::new(0.3, 0.4, -1.0));
        let grapple = build_hand_engine(input);

        assert_eq!(sabre.right.grip, HandGrip::Sabre);
        assert_eq!(grapple.right.grip, HandGrip::Grapple);
        assert!(grapple.right.weight > sabre.right.weight);
    }

    fn board_sample(carve: f32, grounded: bool, trick: Option<TrickCategory>) -> PoseSample {
        PoseSample {
            pose: CartoonPose::Hoverboard,
            phase: 0.0,
            speed: 6.0,
            scale: 1.0,
            stride: 1.0,
            agility: 1.0,
            vertical_velocity: 0.0,
            local_velocity: Vec3::ZERO,
            grounded,
            wall_normal_local: Vec3::ZERO,
            hanging: false,
            dodge_direction_local: Vec3::ZERO,
            grapple_attach_local: None,
            ik: CharacterIkPose::default(),
            hands: HandEngine::default(),
            wall_clasp_time: 0.0,
            board_trick: trick,
            board_carve: carve,
            sabre_swing: 0.0,
            landing_compression: 0.0,
        }
    }

    #[test]
    fn sabre_swing_follows_the_phase_machine_and_alternates_sides() {
        // Wound back through startup, crossing during the active window,
        // settling in recovery — the shape of an actual strike.
        let wind_up = sabre_swing_position(MeleePhase::Startup, 0.0, 0);
        let strike_start = sabre_swing_position(MeleePhase::Active, 0.08, 0);
        let strike_end = sabre_swing_position(MeleePhase::Active, 0.0, 0);
        let settled = sabre_swing_position(MeleePhase::Recovery, 0.0, 0);

        assert!(wind_up < 0.0, "blade winds back before the hit");
        assert!(strike_start < 0.0 && strike_end > 0.0, "swing crosses over");
        assert!(strike_end > strike_start, "swing travels forward");
        assert!(settled.abs() < strike_end.abs(), "follow-through settles");

        // Consecutive slashes swing from opposite sides so a chain reads as
        // a combo instead of the same cut repeated.
        let even = sabre_swing_position(MeleePhase::Active, 0.0, 0);
        let odd = sabre_swing_position(MeleePhase::Active, 0.0, 1);
        assert!(even.signum() != odd.signum());
        assert!(
            (even.abs() - odd.abs()).abs() < 1e-6,
            "mirrored, not weaker"
        );

        // Every position stays in range, so the pose can never over-rotate.
        for index in 0..4u32 {
            for (phase, t) in [
                (MeleePhase::Startup, 0.05),
                (MeleePhase::Active, 0.04),
                (MeleePhase::Recovery, 0.12),
            ] {
                assert!(sabre_swing_position(phase, t, index).abs() <= 1.0);
            }
        }
    }

    #[test]
    fn board_stance_is_sideways_with_bent_knees_and_open_arms() {
        let pose = board_stance_pose(board_sample(0.0, true, None));
        // Rider stands across the deck, not facing forward.
        assert!(pose.stance_yaw > 0.9, "stance should be sideways");
        // Knees bent and hips dropped.
        assert!(pose.front_knee > 0.2 && pose.back_knee > 0.2);
        assert!(pose.crouch > 0.0);
        // Arms out to both sides for balance.
        assert!(pose.left_arm_out < -0.5 && pose.right_arm_out > 0.5);
    }

    #[test]
    fn carving_leans_the_body_and_counter_balances_the_arms() {
        let left = board_stance_pose(board_sample(-1.0, true, None));
        let neutral = board_stance_pose(board_sample(0.0, true, None));
        let right = board_stance_pose(board_sample(1.0, true, None));

        // Torso rolls into the turn, opposite ways for opposite carves.
        assert!(left.torso_roll < neutral.torso_roll);
        assert!(right.torso_roll > neutral.torso_roll);
        // Arms swing as a pair to counter-balance rather than staying fixed.
        assert!(right.left_arm_out < neutral.left_arm_out);
        assert!(left.left_arm_out > neutral.left_arm_out);
        assert!(left.left_arm_pitch > neutral.left_arm_pitch);
        assert!(right.left_arm_pitch < neutral.left_arm_pitch);
    }

    #[test]
    fn each_trick_category_produces_a_distinct_readable_pose() {
        let ride = board_stance_pose(board_sample(0.0, false, None));
        let grab = board_stance_pose(board_sample(0.0, false, Some(TrickCategory::Grab)));
        let flip = board_stance_pose(board_sample(0.0, false, Some(TrickCategory::Flip)));
        let grind = board_stance_pose(board_sample(0.0, true, Some(TrickCategory::Grind)));
        let manual = board_stance_pose(board_sample(0.0, true, Some(TrickCategory::Manual)));

        // Grab folds over the board and reaches a hand down to the deck.
        assert!(grab.torso_pitch < ride.torso_pitch);
        assert!(grab.left_arm_pitch > 1.0, "leading hand reaches the deck");
        // Flip is the tightest tuck of them all.
        assert!(flip.front_knee > grab.front_knee);
        assert!(
            flip.left_arm_out > ride.left_arm_out,
            "arms pull in to spin"
        );
        // Grind squats lowest with the widest arms.
        assert!(grind.crouch > ride.crouch);
        assert!(grind.left_arm_out < ride.left_arm_out);
        // Manual leans back over the tail — the only pose pitching backward.
        assert!(manual.torso_pitch > 0.0);
        assert!(manual.back_knee > manual.front_knee);
    }

    #[test]
    fn heavy_techniques_put_the_off_hand_on_the_hilt() {
        let mut one_handed = hand_input();
        one_handed.sabre_active = true;
        let one = build_hand_engine(one_handed);

        let mut two_handed = hand_input();
        two_handed.sabre_active = true;
        two_handed.sabre_two_handed = true;
        let two = build_hand_engine(two_handed);

        // Sword hand is unchanged; only the off hand commits.
        assert_eq!(one.right.grip, HandGrip::Sabre);
        assert_eq!(two.right.grip, HandGrip::Sabre);

        // One-handed: the off hand braces wide and stays open.
        assert_eq!(one.left.grip, HandGrip::Brace);
        // Two-handed: it closes on the hilt beside the sword hand.
        assert_eq!(two.left.grip, HandGrip::Sabre);
        assert!(two.left.curl > one.left.curl, "off hand must close");
        assert!(two.left.weight > one.left.weight, "and commit to the grip");
    }

    #[test]
    fn only_the_heavy_committed_techniques_are_two_handed() {
        use crate::components::weapon::SabreTechnique;
        // Full-body swings use both hands.
        for technique in [
            SabreTechnique::CycloneSlash,
            SabreTechnique::SpiralSlash,
            SabreTechnique::MeteorPound,
        ] {
            assert!(
                technique.is_two_handed(),
                "{technique:?} should be two-handed"
            );
        }
        // Mobile verbs stay one-handed, and a thrown blade leaves the hand.
        for technique in [
            SabreTechnique::CometDash,
            SabreTechnique::RisingSlash,
            SabreTechnique::SabreThrow,
            SabreTechnique::Ready,
        ] {
            assert!(
                !technique.is_two_handed(),
                "{technique:?} should be one-handed"
            );
        }
    }

    #[test]
    fn hand_engine_hoverboard_opens_balance_hands() {
        let mut input = hand_input();
        input.traversal_mode = TraversalMode::Hoverboard;

        let hands = build_hand_engine(input);

        assert_eq!(hands.left.grip, HandGrip::Balance);
        assert_eq!(hands.right.grip, HandGrip::Balance);
        assert!(hands.left.spread > 0.6);
        assert!(hands.right.spread > 0.6);
    }

    #[test]
    fn imported_joint_rest_rotation_and_scale_survive_pose_reset() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let rest_rotation = Quat::from_euler(EulerRot::XYZ, 0.18, -0.27, 0.09);
        let rest_scale = Vec3::new(1.0, 0.96, 1.04);
        let marker = JointMarker {
            root,
            kind: JointKind::Head,
            local_translation: Vec3::new(0.0, 0.24, 0.0),
            rest_translation: Vec3::new(0.0, 1.72, 0.0),
            rest_rotation,
            rest_scale,
        };
        let sample = PoseSample {
            pose: CartoonPose::Idle,
            phase: 0.0,
            speed: 0.0,
            scale: 1.0,
            stride: 1.0,
            agility: 1.0,
            vertical_velocity: 0.0,
            local_velocity: Vec3::ZERO,
            grounded: true,
            wall_normal_local: Vec3::ZERO,
            hanging: false,
            dodge_direction_local: Vec3::ZERO,
            grapple_attach_local: None,
            ik: CharacterIkPose::default(),
            hands: HandEngine::default(),
            wall_clasp_time: 0.0,
            board_trick: None,
            board_carve: 0.0,
            sabre_swing: 0.0,
            landing_compression: 0.0,
        };
        let mut transform = Transform::default();

        apply_joint_pose(&marker, &mut transform, sample, &HashMap::new());

        assert!(transform.rotation.angle_between(rest_rotation) < 0.001);
        assert!(transform.scale.distance(rest_scale) < 0.001);
        assert!(transform.translation.distance(marker.local_translation) < 0.001);
    }

    #[test]
    fn studio_playable_run_pose_animates_generated_limbs() {
        let mut app = App::new();
        app.add_systems(Update, studio_human_animation_system);
        let owner = app
            .world_mut()
            .spawn((
                Transform::default(),
                CartoonAnimator {
                    phase: 1.2,
                    last_position: Vec3::ZERO,
                    speed: 0.7,
                    pose: CartoonPose::Run,
                },
            ))
            .id();
        let visual_root = app
            .world_mut()
            .spawn((
                Transform::default(),
                PlayableStudioHuman {
                    owner,
                    rest: Transform::default(),
                },
            ))
            .id();
        let rest = Transform::from_xyz(0.4, 0.8, 0.0);
        let limb = app
            .world_mut()
            .spawn((
                rest,
                StudioHumanPart {
                    root: visual_root,
                    rest,
                    pivot: Vec3::new(0.3, 1.2, 0.0),
                    region: StudioBodyRegion::RightArm,
                    face_feature: None,
                },
            ))
            .id();

        app.update();

        let animated = app.world().get::<Transform>(limb).unwrap();
        assert_ne!(animated.rotation, Quat::IDENTITY);
        assert_ne!(animated.translation, rest.translation);
    }
}
