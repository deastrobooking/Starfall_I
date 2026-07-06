use avian3d::{
    math::AdjustPrecision,
    prelude::{
        Collider as AvianCollider, MoveAndSlide, MoveAndSlideConfig, MoveAndSlideHitResponse,
        RigidBody as AvianRigidBody, ShapeCastConfig, SpatialQueryFilter,
    },
};
use bevy::prelude::*;
use core::time::Duration;

pub mod prelude {
    pub use avian3d::prelude::{Physics, PhysicsPlugins, PhysicsTime};

    pub use crate::physics::{
        CharacterAutostep, CharacterLength, Collider, ColliderScale, KinematicCharacterController,
        KinematicCharacterControllerOutput, PhysicsCompatPlugin, PhysicsCompatSet, RigidBody,
    };
}

pub struct PhysicsCompatPlugin;

impl Plugin for PhysicsCompatPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            PostUpdate,
            PhysicsCompatSet::CharacterController.before(TransformSystems::Propagate),
        )
        .add_systems(
            PreUpdate,
            (sync_compat_rigid_bodies, sync_compat_colliders)
                .chain()
                .in_set(PhysicsCompatSet::SyncComponents),
        )
        .add_systems(
            PostUpdate,
            apply_kinematic_character_controllers.in_set(PhysicsCompatSet::CharacterController),
        );
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhysicsCompatSet {
    SyncComponents,
    CharacterController,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigidBody {
    Fixed,
    KinematicPositionBased,
    Dynamic,
}

impl RigidBody {
    fn to_avian(self) -> AvianRigidBody {
        match self {
            Self::Fixed => AvianRigidBody::Static,
            Self::KinematicPositionBased => AvianRigidBody::Kinematic,
            Self::Dynamic => AvianRigidBody::Dynamic,
        }
    }
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct Collider {
    shape: ColliderShape,
}

#[derive(Debug, Clone, PartialEq)]
enum ColliderShape {
    Cuboid {
        half_extents: Vec3,
    },
    Cylinder {
        half_height: f32,
        radius: f32,
    },
    CapsuleY {
        half_height: f32,
        radius: f32,
    },
    Trimesh {
        vertices: Vec<Vec3>,
        indices: Vec<[u32; 3]>,
    },
}

impl Collider {
    pub fn cuboid(x: f32, y: f32, z: f32) -> Self {
        Self {
            shape: ColliderShape::Cuboid {
                half_extents: Vec3::new(x, y, z),
            },
        }
    }

    pub fn cylinder(half_height: f32, radius: f32) -> Self {
        Self {
            shape: ColliderShape::Cylinder {
                half_height,
                radius,
            },
        }
    }

    pub fn capsule_y(half_height: f32, radius: f32) -> Self {
        Self {
            shape: ColliderShape::CapsuleY {
                half_height,
                radius,
            },
        }
    }

    pub fn trimesh(vertices: Vec<Vec3>, indices: Vec<[u32; 3]>) -> Result<Self, &'static str> {
        if vertices.is_empty() || indices.is_empty() {
            return Err("trimesh collider needs vertices and triangle indices");
        }
        Ok(Self {
            shape: ColliderShape::Trimesh { vertices, indices },
        })
    }

    fn to_avian(&self, scale_mode: Option<&ColliderScale>, transform_scale: Vec3) -> AvianCollider {
        let compensation = collider_scale_compensation(scale_mode, transform_scale);

        match &self.shape {
            ColliderShape::Cuboid { half_extents } => {
                let size = *half_extents * 2.0 * compensation;
                AvianCollider::cuboid(size.x.abs(), size.y.abs(), size.z.abs())
            }
            ColliderShape::Cylinder {
                half_height,
                radius,
            } => {
                let radial_scale = compensation.x.abs().max(compensation.z.abs());
                AvianCollider::cylinder(
                    radius * radial_scale,
                    (half_height * 2.0 * compensation.y).abs(),
                )
            }
            ColliderShape::CapsuleY {
                half_height,
                radius,
            } => {
                let radial_scale = compensation.x.abs().max(compensation.z.abs());
                AvianCollider::capsule(
                    radius * radial_scale,
                    (half_height * 2.0 * compensation.y).abs(),
                )
            }
            ColliderShape::Trimesh { vertices, indices } => AvianCollider::trimesh(
                vertices
                    .iter()
                    .map(|vertex| *vertex * compensation)
                    .collect(),
                indices.clone(),
            ),
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub enum ColliderScale {
    Absolute(Vec3),
}

fn collider_scale_compensation(scale_mode: Option<&ColliderScale>, transform_scale: Vec3) -> Vec3 {
    match scale_mode {
        Some(ColliderScale::Absolute(scale)) => Vec3::new(
            safe_scale_ratio(scale.x, transform_scale.x),
            safe_scale_ratio(scale.y, transform_scale.y),
            safe_scale_ratio(scale.z, transform_scale.z),
        ),
        None => Vec3::ONE,
    }
}

fn safe_scale_ratio(target: f32, visual_scale: f32) -> f32 {
    if visual_scale.abs() <= f32::EPSILON {
        target
    } else {
        target / visual_scale.abs()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CharacterLength {
    Absolute(f32),
}

impl CharacterLength {
    fn absolute(self) -> f32 {
        match self {
            Self::Absolute(value) => value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterAutostep {
    pub max_height: CharacterLength,
    pub min_width: CharacterLength,
    pub include_dynamic_bodies: bool,
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct KinematicCharacterController {
    pub up: Vec3,
    pub offset: CharacterLength,
    pub slide: bool,
    pub autostep: Option<CharacterAutostep>,
    pub snap_to_ground: Option<CharacterLength>,
    pub translation: Option<Vec3>,
}

impl Default for KinematicCharacterController {
    fn default() -> Self {
        Self {
            up: Vec3::Y,
            offset: CharacterLength::Absolute(0.0),
            slide: true,
            autostep: None,
            snap_to_ground: None,
            translation: None,
        }
    }
}

#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct KinematicCharacterControllerOutput {
    pub grounded: bool,
    pub desired_translation: Vec3,
    pub effective_translation: Vec3,
    pub collisions: Vec<KinematicCharacterCollision>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KinematicCharacterCollision {
    pub hit: KinematicCharacterHit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KinematicCharacterHit {
    pub details: Option<KinematicCharacterHitDetails>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KinematicCharacterHitDetails {
    pub normal2: Vec3,
}

fn sync_compat_rigid_bodies(
    mut commands: Commands,
    body_q: Query<(Entity, &RigidBody), Or<(Added<RigidBody>, Changed<RigidBody>)>>,
) {
    for (entity, body) in body_q.iter() {
        commands.entity(entity).insert(body.to_avian());
    }
}

fn sync_compat_colliders(
    mut commands: Commands,
    collider_q: Query<
        (
            Entity,
            &Collider,
            Option<&ColliderScale>,
            Option<&Transform>,
        ),
        Or<(
            Added<Collider>,
            Changed<Collider>,
            Added<ColliderScale>,
            Changed<ColliderScale>,
        )>,
    >,
) {
    for (entity, collider, scale_mode, transform) in collider_q.iter() {
        let transform_scale = transform
            .map(|transform| transform.scale)
            .unwrap_or(Vec3::ONE);
        commands
            .entity(entity)
            .insert(collider.to_avian(scale_mode, transform_scale));
    }
}

fn apply_kinematic_character_controllers(
    time: Res<Time>,
    move_and_slide: MoveAndSlide,
    mut controller_q: Query<(
        Entity,
        &AvianCollider,
        &mut Transform,
        &mut KinematicCharacterController,
        &mut KinematicCharacterControllerOutput,
    )>,
) {
    let delta = if time.delta().is_zero() {
        Duration::from_secs_f32(1.0 / 60.0)
    } else {
        time.delta()
    };
    let delta_secs = delta.as_secs_f32().max(1.0 / 600.0);

    for (entity, collider, mut transform, mut controller, mut output) in controller_q.iter_mut() {
        let desired_translation = controller.translation.take().unwrap_or(Vec3::ZERO);
        let start = transform.translation;
        output.desired_translation = desired_translation;
        output.collisions.clear();

        if desired_translation.length_squared() > 0.0 {
            let filter = SpatialQueryFilter::from_excluded_entities([entity]);
            let velocity = desired_translation / delta_secs;
            let config = MoveAndSlideConfig::default();
            let out = move_and_slide.move_and_slide(
                collider,
                start.adjust_precision(),
                transform.rotation.adjust_precision(),
                velocity.adjust_precision(),
                delta,
                &config,
                &filter,
                |hit| {
                    output.collisions.push(KinematicCharacterCollision {
                        hit: KinematicCharacterHit {
                            details: Some(KinematicCharacterHitDetails {
                                normal2: hit.normal.as_vec3(),
                            }),
                        },
                    });
                    MoveAndSlideHitResponse::Accept
                },
            );
            transform.translation = out.position;
        }

        output.effective_translation = transform.translation - start;
        // A character that just moved up (jump/jetpack launch) must not be
        // re-grounded by the generous snap probe: at 120–144 Hz the first jump
        // frame rises less than `snap_to_ground` (0.28), which used to re-latch
        // `grounded`, refresh coyote, and swallow the jump entirely.
        let rising = output.effective_translation.dot(controller.up) > 1e-4;
        output.grounded = character_grounded(
            entity,
            collider,
            &transform,
            &controller,
            &move_and_slide,
            rising,
        );
    }
}

fn character_grounded(
    entity: Entity,
    collider: &AvianCollider,
    transform: &Transform,
    controller: &KinematicCharacterController,
    move_and_slide: &MoveAndSlide,
    rising: bool,
) -> bool {
    // Rising: only a thin contact skin counts as grounded. Falling/level: the
    // full snap distance keeps slope/stair walking stable.
    let snap_distance = if rising {
        controller.offset.absolute().max(0.02)
    } else {
        controller
            .snap_to_ground
            .map(CharacterLength::absolute)
            .unwrap_or(0.12)
            .max(controller.offset.absolute())
            .max(0.05)
    };
    let down = Dir3::new(-controller.up).unwrap_or(Dir3::NEG_Y);
    let filter = SpatialQueryFilter::from_excluded_entities([entity]);
    move_and_slide
        .spatial_query
        .cast_shape(
            collider,
            transform.translation.adjust_precision(),
            transform.rotation.adjust_precision(),
            down,
            &ShapeCastConfig::from_max_distance(snap_distance),
            &filter,
        )
        .is_some_and(|hit| hit.normal1.dot(controller.up) > 0.45)
}
