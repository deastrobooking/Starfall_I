use avian3d::{
    math::AdjustPrecision,
    prelude::{
        Collider as AvianCollider, CollisionLayers, MoveAndSlide, MoveAndSlideConfig,
        MoveAndSlideHitResponse, PhysicsLayer, RigidBody as AvianRigidBody, ShapeCastConfig,
        SpatialQuery, SpatialQueryFilter,
    },
};
use bevy::prelude::*;
use core::time::Duration;

pub mod prelude {
    pub use avian3d::prelude::{Physics, PhysicsPlugins, PhysicsTime};

    pub use crate::physics::{
        CharacterAutostep, CharacterLength, Collider, ColliderScale, CollisionProfile,
        GameCollisionLayer, KinematicCharacterController, KinematicCharacterControllerOutput,
        PhysicsCompatPlugin, PhysicsCompatSet, RigidBody,
    };
}

/// Returns whether a target point is reachable from an effect origin without
/// crossing a World-layer collider. A small target-facing bias makes an
/// explosion on a wall damage actors on the impact side while keeping actors
/// behind the wall protected.
pub fn world_line_of_sight(
    spatial_query: &SpatialQuery,
    origin: Vec3,
    target: Vec3,
    excluded_target: Option<Entity>,
) -> bool {
    const ORIGIN_BIAS: f32 = 0.05;
    const TARGET_SKIN: f32 = 0.08;

    let offset = target - origin;
    let distance = offset.length();
    if distance <= ORIGIN_BIAS + TARGET_SKIN {
        return true;
    }
    let Ok(direction) = Dir3::new(offset) else {
        return true;
    };
    let biased_origin = origin + direction.as_vec3() * ORIGIN_BIAS;
    let max_distance = distance - ORIGIN_BIAS;
    let filter = SpatialQueryFilter::from_mask(GameCollisionLayer::World)
        .with_excluded_entities(excluded_target);

    spatial_query
        .cast_ray(biased_origin, direction, max_distance, true, &filter)
        .is_none_or(|hit| hit.distance + TARGET_SKIN >= max_distance)
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

/// Canonical collision memberships shared by physics movement and gameplay
/// spatial queries. Keeping these meanings centralized prevents each weapon or
/// interaction system from inventing an incompatible bit mask.
#[derive(PhysicsLayer, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameCollisionLayer {
    #[default]
    World,
    Player,
    Enemy,
    PlayerHitbox,
    EnemyHitbox,
    PlayerProjectile,
    EnemyProjectile,
    Pushbox,
    GrappleSensor,
    #[allow(dead_code)] // Reserved for editor/gameplay trigger colliders landing next.
    Interaction,
}

/// Semantic collision role for compatibility colliders. The compatibility
/// sync converts this into Avian's immutable `CollisionLayers` component.
#[derive(Component, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionProfile {
    #[default]
    World,
    Player,
    EnemyHurtbox,
    VehicleHurtbox,
    PlayerHitbox,
    #[allow(dead_code)] // Reserved for enemy move-scoped attack volumes.
    EnemyHitbox,
    PlayerProjectile,
    EnemyProjectile,
    #[allow(dead_code)] // Separate non-damaging character separation volume.
    Pushbox,
    #[allow(dead_code)] // Assigned as grapple sockets gain authored colliders.
    GrappleSensor,
    #[allow(dead_code)] // Reserved for editor/gameplay trigger colliders landing next.
    Interaction,
}

impl CollisionProfile {
    pub fn layers(self) -> CollisionLayers {
        use GameCollisionLayer as Layer;

        match self {
            Self::World => CollisionLayers::new(
                Layer::World,
                [
                    Layer::Player,
                    Layer::Enemy,
                    Layer::PlayerHitbox,
                    Layer::EnemyHitbox,
                    Layer::PlayerProjectile,
                    Layer::EnemyProjectile,
                ],
            ),
            Self::Player => CollisionLayers::new(
                Layer::Player,
                [
                    Layer::World,
                    Layer::Enemy,
                    Layer::EnemyHitbox,
                    Layer::EnemyProjectile,
                    Layer::Interaction,
                ],
            ),
            Self::EnemyHurtbox => CollisionLayers::new(
                Layer::Enemy,
                [
                    Layer::World,
                    Layer::Player,
                    Layer::PlayerHitbox,
                    Layer::PlayerProjectile,
                    Layer::GrappleSensor,
                ],
            ),
            Self::VehicleHurtbox => CollisionLayers::new(
                [Layer::World, Layer::Enemy],
                [
                    Layer::World,
                    Layer::Player,
                    Layer::PlayerHitbox,
                    Layer::PlayerProjectile,
                    Layer::GrappleSensor,
                ],
            ),
            Self::PlayerHitbox => CollisionLayers::new(Layer::PlayerHitbox, Layer::Enemy),
            Self::EnemyHitbox => CollisionLayers::new(Layer::EnemyHitbox, Layer::Player),
            Self::PlayerProjectile => {
                CollisionLayers::new(Layer::PlayerProjectile, [Layer::World, Layer::Enemy])
            }
            Self::EnemyProjectile => {
                CollisionLayers::new(Layer::EnemyProjectile, [Layer::World, Layer::Player])
            }
            Self::Pushbox => CollisionLayers::new(Layer::Pushbox, Layer::Pushbox),
            Self::GrappleSensor => {
                CollisionLayers::new(Layer::GrappleSensor, [Layer::Player, Layer::Enemy])
            }
            Self::Interaction => CollisionLayers::new(Layer::Interaction, Layer::Player),
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigidBody {
    Fixed,
    KinematicPositionBased,
    #[allow(dead_code)]
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
            Option<&CollisionProfile>,
        ),
        Or<(
            Added<Collider>,
            Changed<Collider>,
            Added<ColliderScale>,
            Changed<ColliderScale>,
            Added<CollisionProfile>,
            Changed<CollisionProfile>,
        )>,
    >,
) {
    for (entity, collider, scale_mode, transform, profile) in collider_q.iter() {
        let transform_scale = transform
            .map(|transform| transform.scale)
            .unwrap_or(Vec3::ONE);
        commands.entity(entity).insert((
            collider.to_avian(scale_mode, transform_scale),
            profile.copied().unwrap_or_default().layers(),
        ));
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
            let filter = SpatialQueryFilter::from_mask(GameCollisionLayer::World)
                .with_excluded_entities([entity]);
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
    let filter =
        SpatialQueryFilter::from_mask(GameCollisionLayer::World).with_excluded_entities([entity]);
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

#[cfg(test)]
mod collision_layer_tests {
    use super::*;
    use avian3d::prelude::{CollisionEnd, CollisionStart, PhysicsPlugins};

    #[derive(Resource, Default)]
    struct LineOfSightFixture {
        blocked: bool,
        clear_above_wall: bool,
        target: Option<Entity>,
        target_does_not_block_itself: bool,
        enemy: Option<Entity>,
        player_hitbox_finds_enemy_only: bool,
    }

    fn sample_line_of_sight_fixture(
        spatial_query: SpatialQuery,
        mut result: ResMut<LineOfSightFixture>,
    ) {
        result.blocked =
            !world_line_of_sight(&spatial_query, Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), None);
        result.clear_above_wall = world_line_of_sight(
            &spatial_query,
            Vec3::new(0.0, 4.0, 0.0),
            Vec3::new(10.0, 4.0, 0.0),
            None,
        );
        result.target_does_not_block_itself = result.target.is_some_and(|target| {
            world_line_of_sight(
                &spatial_query,
                Vec3::new(6.5, 0.0, 0.0),
                Vec3::new(10.0, 0.0, 0.0),
                Some(target),
            )
        });
        result.player_hitbox_finds_enemy_only = result.enemy.is_some_and(|enemy| {
            let hitbox_layers = CollisionProfile::PlayerHitbox.layers();
            let candidates = spatial_query.shape_intersections(
                &AvianCollider::sphere(3.0),
                Vec3::ZERO,
                Quat::IDENTITY,
                &SpatialQueryFilter::from_mask(hitbox_layers.filters),
            );
            candidates == [enemy]
        });
    }

    #[test]
    fn collision_profiles_have_stable_memberships() {
        assert!(CollisionProfile::World
            .layers()
            .memberships
            .has_all(GameCollisionLayer::World));
        assert!(CollisionProfile::Player
            .layers()
            .memberships
            .has_all(GameCollisionLayer::Player));
        assert!(CollisionProfile::EnemyHurtbox
            .layers()
            .memberships
            .has_all(GameCollisionLayer::Enemy));
        let vehicle = CollisionProfile::VehicleHurtbox.layers();
        assert!(vehicle.memberships.has_all(GameCollisionLayer::World));
        assert!(vehicle.memberships.has_all(GameCollisionLayer::Enemy));
    }

    #[test]
    fn projectile_profiles_interact_only_with_intended_sides() {
        let world = CollisionProfile::World.layers();
        let player = CollisionProfile::Player.layers();
        let enemy = CollisionProfile::EnemyHurtbox.layers();
        let player_shot = CollisionProfile::PlayerProjectile.layers();
        let enemy_shot = CollisionProfile::EnemyProjectile.layers();

        assert!(player_shot.interacts_with(world));
        assert!(player_shot.interacts_with(enemy));
        assert!(!player_shot.interacts_with(player));
        assert!(enemy_shot.interacts_with(world));
        assert!(enemy_shot.interacts_with(player));
        assert!(!enemy_shot.interacts_with(enemy));

        let player_hitbox = CollisionProfile::PlayerHitbox.layers();
        let enemy_hitbox = CollisionProfile::EnemyHitbox.layers();
        assert!(player_hitbox.interacts_with(enemy));
        assert!(!player_hitbox.interacts_with(player));
        assert!(enemy_hitbox.interacts_with(player));
        assert!(!enemy_hitbox.interacts_with(enemy));

        let pushbox = CollisionProfile::Pushbox.layers();
        assert!(pushbox.interacts_with(pushbox));
        let grapple = CollisionProfile::GrappleSensor.layers();
        assert!(grapple.interacts_with(enemy));
    }

    #[test]
    fn world_line_of_sight_blocks_cover_but_excludes_the_target_collider() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            PhysicsPlugins::default(),
        ))
        .init_asset::<Mesh>()
        .add_message::<CollisionStart>()
        .add_message::<CollisionEnd>()
        .init_resource::<LineOfSightFixture>()
        .add_systems(Update, sample_line_of_sight_fixture);

        app.world_mut().spawn((
            AvianCollider::cuboid(1.0, 4.0, 4.0),
            CollisionProfile::World.layers(),
            AvianRigidBody::Static,
            Transform::from_xyz(5.0, 0.0, 0.0),
        ));
        let target = app
            .world_mut()
            .spawn((
                AvianCollider::cuboid(1.0, 2.0, 2.0),
                CollisionProfile::VehicleHurtbox.layers(),
                AvianRigidBody::Static,
                Transform::from_xyz(10.0, 0.0, 0.0),
            ))
            .id();
        let enemy = app
            .world_mut()
            .spawn((
                AvianCollider::sphere(0.8),
                CollisionProfile::EnemyHurtbox.layers(),
                AvianRigidBody::Kinematic,
                Transform::from_xyz(2.0, 0.0, 0.0),
            ))
            .id();
        let mut fixture = app.world_mut().resource_mut::<LineOfSightFixture>();
        fixture.target = Some(target);
        fixture.enemy = Some(enemy);

        // The first update registers colliders in the spatial-query pipeline;
        // the second samples the populated broad phase.
        app.update();
        app.update();

        let result = app.world().resource::<LineOfSightFixture>();
        assert!(result.blocked);
        assert!(result.clear_above_wall);
        assert!(result.target_does_not_block_itself);
        assert!(result.player_hitbox_finds_enemy_only);
    }
}
