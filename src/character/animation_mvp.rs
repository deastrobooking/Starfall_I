//! First production animation slice for the procedural humanoid rig.
//!
//! The graph supplies authored-looking base motion for the torso, shoulders,
//! and hips. The existing procedural pass continues to own hands, weapon
//! sockets, knees, ankles, and IK, which keeps gameplay poses usable while the
//! clip library grows.

use std::{collections::HashMap, f32::consts::PI, time::Duration};

use bevy::{
    animation::{animated_field, AnimatedBy, AnimationTargetId},
    prelude::*,
    world_serialization::WorldInstanceReady,
};

use crate::{
    character_studio::rig_bridge::{
        canonical_joint_for_bone, ImportedHumanoidRig, ImportedRigStatus,
    },
    components::character::{
        CartoonAnimator, CartoonPose, CharacterIkPose, HandEngine, JointKind, JointMarker,
        SkeletonRig,
    },
};

const GRAPH_POSES: [CartoonPose; 8] = [
    CartoonPose::Idle,
    CartoonPose::Walk,
    CartoonPose::Run,
    CartoonPose::Sprint,
    CartoonPose::Jump,
    CartoonPose::Fall,
    CartoonPose::WallSlide,
    CartoonPose::Hang,
];

const GRAPH_JOINTS: [JointKind; 6] = [
    JointKind::Spine,
    JointKind::Chest,
    JointKind::LeftShoulder,
    JointKind::RightShoulder,
    JointKind::LeftHip,
    JointKind::RightHip,
];

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CharacterAnimationBackend {
    #[default]
    Procedural,
    GraphMvp,
}

#[derive(Component, Debug, Default)]
pub(crate) struct GraphPlaybackState {
    current: Option<CartoonPose>,
}

#[derive(Resource)]
pub(crate) struct CharacterAnimationLibrary {
    graph: Handle<AnimationGraph>,
    nodes: HashMap<CartoonPose, AnimationNodeIndex>,
}

pub struct CharacterAnimationMvpPlugin;

impl Plugin for CharacterAnimationMvpPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, build_animation_library)
            .add_systems(Update, attach_animation_graph_to_new_rigs)
            .add_observer(bind_imported_humanoid_rig);
    }
}

fn build_animation_library(
    mut commands: Commands,
    mut clips: ResMut<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    let handles = GRAPH_POSES.map(|pose| clips.add(build_clip(pose)));
    let (graph, node_indices) = AnimationGraph::from_clips(handles);
    let nodes = GRAPH_POSES.into_iter().zip(node_indices).collect();

    commands.insert_resource(CharacterAnimationLibrary {
        graph: graphs.add(graph),
        nodes,
    });
}

fn attach_animation_graph_to_new_rigs(
    mut commands: Commands,
    library: Res<CharacterAnimationLibrary>,
    mut clips: ResMut<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    rigs: Query<(Entity, &SkeletonRig, Option<&ImportedHumanoidRig>), Added<SkeletonRig>>,
    joints: Query<&JointMarker>,
) {
    for (root, rig, imported) in &rigs {
        let (graph, nodes) = if imported.is_some() {
            build_retargeted_library(rig, &joints, &mut clips, &mut graphs)
        } else {
            (library.graph.clone(), library.nodes.clone())
        };
        let mut player = AnimationPlayer::default();
        let mut transitions = AnimationTransitions::new();
        transitions
            .play(&mut player, nodes[&CartoonPose::Idle], Duration::ZERO)
            .repeat();

        commands.entity(root).insert((
            AnimationGraphHandle(graph),
            player,
            transitions,
            GraphPlaybackState {
                current: Some(CartoonPose::Idle),
            },
            CharacterAnimationBackend::GraphMvp,
        ));

        for joint_kind in GRAPH_JOINTS {
            if let Some(&joint) = rig.joints.get(&joint_kind) {
                commands
                    .entity(joint)
                    .insert((target_id(joint_kind), AnimatedBy(root)));
            }
        }
    }
}

fn build_retargeted_library(
    rig: &SkeletonRig,
    joints: &Query<&JointMarker>,
    clips: &mut Assets<AnimationClip>,
    graphs: &mut Assets<AnimationGraph>,
) -> (
    Handle<AnimationGraph>,
    HashMap<CartoonPose, AnimationNodeIndex>,
) {
    let rest_rotations = GRAPH_JOINTS
        .into_iter()
        .filter_map(|joint| {
            let entity = *rig.joints.get(&joint)?;
            Some((joint, joints.get(entity).ok()?.rest_rotation))
        })
        .collect::<HashMap<_, _>>();
    let handles = GRAPH_POSES.map(|pose| clips.add(build_clip_with_rest(pose, &rest_rotations)));
    let (graph, node_indices) = AnimationGraph::from_clips(handles);
    let nodes = GRAPH_POSES.into_iter().zip(node_indices).collect();
    (graphs.add(graph), nodes)
}

/// Convert a loaded skinned hierarchy into the same canonical rig used by
/// procedural characters. Binding is atomic: incomplete or ambiguous rigs
/// retain a diagnostic status and receive no partial animation components.
fn bind_imported_humanoid_rig(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    names: Query<&Name>,
    transforms: Query<&Transform>,
    parents: Query<&ChildOf>,
    imported_roots: Query<(
        &ImportedHumanoidRig,
        &Transform,
        Option<&CartoonAnimator>,
        Option<&HandEngine>,
        Option<&CharacterIkPose>,
    )>,
) {
    let root = ready.entity;
    let Ok((imported, root_transform, animator, hands, ik)) = imported_roots.get(root) else {
        return;
    };

    let mut joints = HashMap::new();
    let mut duplicate_targets = Vec::new();
    for entity in children.iter_descendants(root) {
        let Ok(name) = names.get(entity) else {
            continue;
        };
        let Some(kind) = canonical_joint_for_bone(name.as_str()) else {
            continue;
        };
        if joints.insert(kind, entity).is_some() && !duplicate_targets.contains(&kind) {
            duplicate_targets.push(kind);
        }
    }
    duplicate_targets.sort_by_key(|joint| *joint as u8);
    let missing = JointKind::HUMANOID
        .into_iter()
        .filter(|kind| !joints.contains_key(kind))
        .collect::<Vec<_>>();

    if !missing.is_empty() || !duplicate_targets.is_empty() {
        warn!(
            "Imported humanoid '{}' rejected: missing={missing:?}, duplicates={duplicate_targets:?}",
            imported.source
        );
        commands.entity(root).insert(ImportedRigStatus::Invalid {
            missing,
            duplicate_targets,
            unresolved_hierarchy: Vec::new(),
        });
        return;
    }

    let mut bindings = Vec::with_capacity(joints.len());
    let mut unresolved_hierarchy = Vec::new();
    for (&kind, &entity) in &joints {
        let (Ok(local), Some(rest_translation)) = (
            transforms.get(entity),
            root_relative_translation(root, entity, &transforms, &parents),
        ) else {
            unresolved_hierarchy.push(kind);
            continue;
        };
        bindings.push((
            entity,
            JointMarker {
                root,
                kind,
                local_translation: local.translation,
                rest_translation,
                rest_rotation: local.rotation,
                rest_scale: local.scale,
            },
        ));
    }
    unresolved_hierarchy.sort_by_key(|joint| *joint as u8);
    if !unresolved_hierarchy.is_empty() {
        warn!(
            "Imported humanoid '{}' rejected: unresolved hierarchy={unresolved_hierarchy:?}",
            imported.source
        );
        commands.entity(root).insert(ImportedRigStatus::Invalid {
            missing: Vec::new(),
            duplicate_targets: Vec::new(),
            unresolved_hierarchy,
        });
        return;
    }
    for (entity, marker) in bindings {
        commands.entity(entity).insert(marker);
    }

    let mut root_commands = commands.entity(root);
    root_commands.insert((
        SkeletonRig {
            joints: joints.clone(),
        },
        ImportedRigStatus::Ready {
            mapped_joint_count: joints.len(),
        },
    ));
    if animator.is_none() {
        root_commands.insert(CartoonAnimator::new(root_transform.translation));
    }
    if hands.is_none() {
        root_commands.insert(HandEngine::default());
    }
    if ik.is_none() {
        root_commands.insert(CharacterIkPose::default());
    }
    info!(
        "Imported humanoid '{}' bound to {} canonical joints",
        imported.source,
        joints.len()
    );
}

fn root_relative_translation(
    root: Entity,
    entity: Entity,
    transforms: &Query<&Transform>,
    parents: &Query<&ChildOf>,
) -> Option<Vec3> {
    let mut current = entity;
    let mut chain = Vec::new();
    for _ in 0..128 {
        if current == root {
            let relative = chain
                .into_iter()
                .rev()
                .fold(GlobalTransform::IDENTITY, |parent, local| {
                    parent.mul_transform(local)
                });
            return Some(relative.translation());
        }
        chain.push(*transforms.get(current).ok()?);
        current = parents.get(current).ok()?.parent();
    }
    None
}

/// Synchronizes gameplay pose selection with the graph. Unsupported action
/// poses deliberately stop graph playback so the procedural system has full
/// authority until a dedicated clip is added.
pub(crate) fn drive_graph_animation(
    library: Option<Res<CharacterAnimationLibrary>>,
    mut players: Query<(
        &CartoonAnimator,
        &mut CharacterAnimationBackend,
        &mut GraphPlaybackState,
        &mut AnimationPlayer,
        &mut AnimationTransitions,
    )>,
) {
    let Some(library) = library else {
        return;
    };

    for (animator, mut backend, mut state, mut player, mut transitions) in &mut players {
        let Some(&node) = library.nodes.get(&animator.pose) else {
            if state.current.is_some() {
                player.stop_all();
                state.current = None;
            }
            *backend = CharacterAnimationBackend::Procedural;
            continue;
        };

        *backend = CharacterAnimationBackend::GraphMvp;
        if state.current == Some(animator.pose) {
            if let Some(active) = player.animation_mut(node) {
                active.set_speed(playback_speed(animator.pose, animator.speed));
            }
            continue;
        }

        let active = transitions.play(&mut player, node, transition_duration(animator.pose));
        if pose_loops(animator.pose) {
            active.repeat();
        }
        active.set_speed(playback_speed(animator.pose, animator.speed));
        state.current = Some(animator.pose);
    }
}

fn transition_duration(pose: CartoonPose) -> Duration {
    match pose {
        CartoonPose::Jump | CartoonPose::Fall | CartoonPose::WallSlide | CartoonPose::Hang => {
            Duration::from_millis(80)
        }
        _ => Duration::from_millis(140),
    }
}

fn playback_speed(pose: CartoonPose, movement_speed: f32) -> f32 {
    match pose {
        CartoonPose::Idle => 1.0,
        // `CartoonAnimator::speed` is measured from world displacement per
        // second. The motor's authored walk/sprint velocities are delivered at
        // 60x scale, so compare against their real presentation speeds rather
        // than the smaller per-tick tuning values.
        CartoonPose::Walk => (movement_speed / 22.8).clamp(0.68, 1.35),
        CartoonPose::Run => (movement_speed / 32.0).clamp(0.76, 1.45),
        CartoonPose::Sprint => (movement_speed / 42.0).clamp(0.84, 1.5),
        _ => 1.0,
    }
}

fn pose_loops(pose: CartoonPose) -> bool {
    !matches!(pose, CartoonPose::Jump)
}

pub(crate) fn graph_owns_joint(joint: JointKind) -> bool {
    GRAPH_JOINTS.contains(&joint)
}

fn target_id(joint: JointKind) -> AnimationTargetId {
    AnimationTargetId::from_name(&Name::new(format!("StarfallMvp::{joint:?}")))
}

fn build_clip(pose: CartoonPose) -> AnimationClip {
    build_clip_with_rest(pose, &HashMap::new())
}

fn build_clip_with_rest(
    pose: CartoonPose,
    rest_rotations: &HashMap<JointKind, Quat>,
) -> AnimationClip {
    let duration = clip_duration(pose);
    let mut clip = AnimationClip::default();

    for joint in GRAPH_JOINTS {
        let rest = rest_rotations
            .get(&joint)
            .copied()
            .unwrap_or(Quat::IDENTITY);
        let [start, middle, end] = joint_rotations(pose, joint).map(|rotation| rest * rotation);
        clip.add_curve_to_target(
            target_id(joint),
            AnimatableCurve::new(
                animated_field!(Transform::rotation),
                UnevenSampleAutoCurve::new(
                    [0.0, duration * 0.5, duration]
                        .into_iter()
                        .zip([start, middle, end]),
                )
                .expect("MVP rotation clips always contain ordered samples"),
            ),
        );
    }

    clip
}

fn clip_duration(pose: CartoonPose) -> f32 {
    match pose {
        CartoonPose::Idle => 2.4,
        CartoonPose::Walk => 0.82,
        CartoonPose::Run => 0.58,
        CartoonPose::Sprint => 0.46,
        CartoonPose::Jump => 0.55,
        CartoonPose::Fall => 0.7,
        CartoonPose::WallSlide => 0.8,
        CartoonPose::Hang => 1.2,
        _ => 1.0,
    }
}

fn q(x: f32, y: f32, z: f32) -> Quat {
    Quat::from_euler(EulerRot::XYZ, x, y, z)
}

fn joint_rotations(pose: CartoonPose, joint: JointKind) -> [Quat; 3] {
    use JointKind::{Chest, LeftHip, LeftShoulder, RightHip, RightShoulder, Spine};

    let (a, b) = match pose {
        CartoonPose::Idle => match joint {
            Spine => (q(-0.015, -0.025, 0.0), q(0.02, 0.025, 0.0)),
            Chest => (q(0.01, 0.03, 0.0), q(-0.015, -0.03, 0.0)),
            LeftShoulder => (q(0.035, 0.0, 0.0), q(-0.035, 0.0, 0.0)),
            RightShoulder => (q(-0.035, 0.0, 0.0), q(0.035, 0.0, 0.0)),
            _ => (Quat::IDENTITY, Quat::IDENTITY),
        },
        CartoonPose::Walk => locomotion_rotation(joint, 0.44, 0.38, -0.035, 0.07),
        CartoonPose::Run => locomotion_rotation(joint, 0.76, 0.66, -0.11, 0.11),
        CartoonPose::Sprint => locomotion_rotation(joint, 0.96, 0.84, -0.2, 0.14),
        CartoonPose::Jump => match joint {
            Spine | Chest => (q(-0.08, 0.0, 0.0), q(-0.14, 0.0, 0.0)),
            LeftShoulder | RightShoulder => (q(-0.42, 0.0, 0.0), q(-0.62, 0.0, 0.0)),
            LeftHip | RightHip => (q(0.2, 0.0, 0.0), q(0.36, 0.0, 0.0)),
            _ => (Quat::IDENTITY, Quat::IDENTITY),
        },
        CartoonPose::Fall => match joint {
            Spine | Chest => (q(0.06, 0.0, 0.0), q(0.11, 0.0, 0.0)),
            LeftShoulder => (q(-0.2, 0.0, -0.16), q(-0.08, 0.0, -0.22)),
            RightShoulder => (q(-0.2, 0.0, 0.16), q(-0.08, 0.0, 0.22)),
            LeftHip => (q(-0.12, 0.0, 0.0), q(0.1, 0.0, 0.0)),
            RightHip => (q(0.1, 0.0, 0.0), q(-0.12, 0.0, 0.0)),
            _ => (Quat::IDENTITY, Quat::IDENTITY),
        },
        CartoonPose::WallSlide => match joint {
            Spine | Chest => (q(-0.06, -0.08, 0.0), q(-0.1, 0.08, 0.0)),
            LeftShoulder => (q(-0.78, 0.0, -0.08), q(-0.62, 0.0, -0.12)),
            RightShoulder => (q(0.22, 0.0, 0.0), q(0.34, 0.0, 0.0)),
            LeftHip => (q(0.34, 0.0, 0.0), q(0.2, 0.0, 0.0)),
            RightHip => (q(-0.1, 0.0, 0.0), q(-0.22, 0.0, 0.0)),
            _ => (Quat::IDENTITY, Quat::IDENTITY),
        },
        CartoonPose::Hang => match joint {
            Spine | Chest => (q(0.08, 0.0, 0.0), q(0.12, 0.0, 0.0)),
            LeftShoulder => (q(-PI * 0.72, 0.0, -0.08), q(-PI * 0.76, 0.0, -0.05)),
            RightShoulder => (q(-PI * 0.72, 0.0, 0.08), q(-PI * 0.76, 0.0, 0.05)),
            LeftHip => (q(-0.08, 0.0, 0.0), q(0.08, 0.0, 0.0)),
            RightHip => (q(0.08, 0.0, 0.0), q(-0.08, 0.0, 0.0)),
            _ => (Quat::IDENTITY, Quat::IDENTITY),
        },
        _ => (Quat::IDENTITY, Quat::IDENTITY),
    };

    [a, b, a]
}

fn locomotion_rotation(
    joint: JointKind,
    arm_swing: f32,
    hip_swing: f32,
    lean: f32,
    twist: f32,
) -> (Quat, Quat) {
    use JointKind::{Chest, LeftHip, LeftShoulder, RightHip, RightShoulder, Spine};

    match joint {
        Spine => (q(lean, -twist * 0.45, 0.0), q(lean, twist * 0.45, 0.0)),
        Chest => (q(lean * 0.35, -twist, 0.0), q(lean * 0.35, twist, 0.0)),
        LeftShoulder => (q(arm_swing, 0.0, 0.0), q(-arm_swing, 0.0, 0.0)),
        RightShoulder => (q(-arm_swing, 0.0, 0.0), q(arm_swing, 0.0, 0.0)),
        LeftHip => (q(-hip_swing, 0.0, 0.0), q(hip_swing, 0.0, 0.0)),
        RightHip => (q(hip_swing, 0.0, 0.0), q(-hip_swing, 0.0, 0.0)),
        _ => (Quat::IDENTITY, Quat::IDENTITY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mvp_graph_covers_the_core_locomotion_set() {
        for pose in GRAPH_POSES {
            let clip = build_clip(pose);
            assert!(clip.duration() > 0.0);
            assert_eq!(clip.curves().len(), GRAPH_JOINTS.len());
            for joint in GRAPH_JOINTS {
                assert_eq!(clip.curves_for_target(target_id(joint)).unwrap().len(), 1);
            }
        }
        assert!(!GRAPH_POSES.contains(&CartoonPose::Attack));
        assert!(!GRAPH_POSES.contains(&CartoonPose::SabreSlash));
    }

    #[test]
    fn run_and_sprint_shoulders_swing_fore_and_aft() {
        for pose in [CartoonPose::Run, CartoonPose::Sprint] {
            let [left_a, left_b, _] = joint_rotations(pose, JointKind::LeftShoulder);
            let [right_a, right_b, _] = joint_rotations(pose, JointKind::RightShoulder);
            let (left_a_x, _, left_a_z) = left_a.to_euler(EulerRot::XYZ);
            let (left_b_x, _, left_b_z) = left_b.to_euler(EulerRot::XYZ);
            let (right_a_x, _, right_a_z) = right_a.to_euler(EulerRot::XYZ);
            let (right_b_x, _, right_b_z) = right_b.to_euler(EulerRot::XYZ);

            assert!(left_a_x * left_b_x < 0.0);
            assert!(right_a_x * right_b_x < 0.0);
            assert!(left_a_z.abs() < 0.001 && left_b_z.abs() < 0.001);
            assert!(right_a_z.abs() < 0.001 && right_b_z.abs() < 0.001);
        }
    }

    #[test]
    fn movement_playback_speed_is_bounded() {
        assert_eq!(playback_speed(CartoonPose::Run, 0.0), 0.76);
        assert_eq!(playback_speed(CartoonPose::Walk, 22.8), 1.0);
        assert_eq!(playback_speed(CartoonPose::Sprint, 42.0), 1.0);
        assert_eq!(playback_speed(CartoonPose::Run, 100.0), 1.45);
        assert_eq!(playback_speed(CartoonPose::Idle, 100.0), 1.0);
    }

    #[test]
    fn takeoff_is_a_one_shot_while_sustained_poses_loop() {
        assert!(!pose_loops(CartoonPose::Jump));
        assert!(pose_loops(CartoonPose::Walk));
        assert!(pose_loops(CartoonPose::Fall));
    }
}
