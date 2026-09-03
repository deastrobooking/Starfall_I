//! Turning an assembled route into real world geometry.
//!
//! [`super::platformer_routes`] decides *what* a level is; this module builds
//! it. Solids become walkable collision, scenery becomes mass the party cannot
//! walk along but can be stopped by, and prefabs are spawned with the same
//! behaviour components the level editor's runtime adapters drive — so a
//! moving platform authored in a chunk behaves exactly like one placed by hand.

use bevy::prelude::*;

use super::platformer_chunks::{ChunkPiece, ChunkTheme, PlacedChunk};
use super::platformer_routes::HeavyWaterRouteExt;
use crate::components::world::{
    CollapsePlatform, CollapsePlatformState, MovingPlatform, SpringJumpPad, WalkableSurface,
    WorldGeometry,
};
use crate::engine::physics::prelude::{Collider, RigidBody};
use crate::engine::rendering::PbrBundle;
use crate::engine_tools::platformer_prefabs::{
    PlatformerBehavior, PlatformerPrefabDesign, PlatformerPrefabKind,
};

/// Marks everything spawned for one shared-screen route, so a level teardown
/// is a single despawn query rather than bookkeeping per piece.
#[derive(Component, Debug, Clone, Copy)]
pub struct RouteGeometry;

/// Materials for one theme, built once per route rather than per piece.
struct ThemeMaterials {
    stone: Handle<StandardMaterial>,
    accent: Handle<StandardMaterial>,
    scenery: Handle<StandardMaterial>,
}

fn theme_materials(theme: ChunkTheme, materials: &mut Assets<StandardMaterial>) -> ThemeMaterials {
    let (stone, accent) = match theme {
        ChunkTheme::Cityscape => (Color::srgb(0.20, 0.24, 0.38), Color::srgb(0.10, 0.85, 1.00)),
        ChunkTheme::Mountain => (Color::srgb(0.34, 0.33, 0.36), Color::srgb(0.75, 0.90, 1.00)),
        ChunkTheme::Castle => (Color::srgb(0.40, 0.36, 0.30), Color::srgb(1.00, 0.78, 0.25)),
        ChunkTheme::Cavern => (Color::srgb(0.17, 0.15, 0.22), Color::srgb(0.55, 0.35, 1.00)),
        ChunkTheme::Custom(_) => (Color::srgb(0.28, 0.30, 0.36), Color::srgb(0.42, 0.72, 0.92)),
    };
    ThemeMaterials {
        stone: materials.add(StandardMaterial {
            base_color: stone,
            perceptual_roughness: 0.85,
            ..default()
        }),
        accent: materials.add(StandardMaterial {
            base_color: accent,
            emissive: LinearRgba::from(accent) * 1.6,
            ..default()
        }),
        // Backdrop mass is deliberately darker and flatter so it reads as
        // depth rather than competing with the route the party must follow.
        scenery: materials.add(StandardMaterial {
            base_color: (stone.to_linear() * 0.55).into(),
            perceptual_roughness: 1.0,
            ..default()
        }),
    }
}

fn spawn_box(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    center: Vec3,
    size: Vec3,
    material: Handle<StandardMaterial>,
    walkable: bool,
) {
    let mut entity = commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
            material: MeshMaterial3d(material),
            transform: Transform::from_translation(center),
            ..default()
        },
        RigidBody::Fixed,
        Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
        WorldGeometry,
        RouteGeometry,
    ));
    // Only route surfaces advertise themselves as footing; scenery collides
    // without inviting the party to path along it.
    if walkable {
        entity.insert(WalkableSurface);
    }
}

/// Spawn a prefab with the behaviour its recipe declares.
fn spawn_prefab(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: Handle<StandardMaterial>,
    kind: PlatformerPrefabKind,
    center: Vec3,
    yaw_degrees: f32,
) {
    let design = PlatformerPrefabDesign::stock(kind);
    let size = design.size;
    let transform = Transform::from_translation(center)
        .with_rotation(Quat::from_rotation_y(yaw_degrees.to_radians()));
    let mut entity = commands.spawn((
        PbrBundle {
            mesh: Mesh3d(meshes.add(design.mesh())),
            material: MeshMaterial3d(material),
            transform,
            ..default()
        },
        WorldGeometry,
        RouteGeometry,
    ));

    match kind.behavior() {
        PlatformerBehavior::Oscillate { distance, seconds } => {
            let half = transform.rotation * Vec3::X * (distance * 0.5);
            entity.insert((
                MovingPlatform {
                    start: center - half,
                    end: center + half,
                    speed: std::f32::consts::TAU * distance / seconds.max(0.1),
                    phase: 0.0,
                    size,
                },
                RigidBody::KinematicPositionBased,
                Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
                WalkableSurface,
            ));
        }
        PlatformerBehavior::Bounce { launch_speed } => {
            entity.insert(SpringJumpPad {
                launch_velocity: Vec3::Y * launch_speed,
                radius: size.x.min(size.z) * 0.48,
                cooldown: 0.6,
                cooldown_timers: [0.0; 4],
                force_hoverboard: false,
            });
        }
        PlatformerBehavior::Collapse { warning_seconds } => {
            entity.insert((
                CollapsePlatform {
                    origin: center,
                    size,
                    warning_seconds,
                    fallen_seconds: 0.0,
                    drop_distance: 24.0,
                    timer: 0.0,
                    state: CollapsePlatformState::Armed,
                },
                RigidBody::Fixed,
                Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
                WalkableSurface,
            ));
        }
        // Climbable and static prefabs are plain solid geometry; the remaining
        // behaviours are driven by the editor's adapters and are not authored
        // into catalogue chunks yet.
        _ => {
            entity.insert((
                RigidBody::Fixed,
                Collider::cuboid(size.x * 0.5, size.y * 0.5, size.z * 0.5),
                WalkableSurface,
            ));
        }
    }
}

/// Build every piece of an assembled route.
///
/// Materials are chosen per **chunk**, not per route: a route may deliberately
/// cross themes (a mountain climb that ends at a city plaza), and painting the
/// whole level in the opening theme's stone would render those chunks wrong.
/// Each theme's materials are built once and reused.
///
/// Returns how many entities were spawned, which the caller can log or assert
/// on — a route that silently builds nothing is a bug worth noticing.
pub fn spawn_route(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    route: &[PlacedChunk],
) -> usize {
    let mut palettes: Vec<(ChunkTheme, ThemeMaterials)> = Vec::new();
    let mut spawned = 0;
    for chunk in route {
        let theme = chunk.def.theme;
        if !palettes.iter().any(|(known, _)| *known == theme) {
            palettes.push((theme, theme_materials(theme, materials)));
        }
        let mats = &palettes
            .iter()
            .find(|(known, _)| *known == theme)
            .expect("just inserted")
            .1;
        // Name each chunk's group so the editor outliner and logs read as the
        // designer's own vocabulary rather than anonymous geometry.
        commands.spawn((
            Name::new(format!("{} [{}]", chunk.def.label, chunk.def.role.label())),
            WorldGeometry,
            RouteGeometry,
        ));
        spawned += 1;
        for piece in chunk.world_pieces() {
            match piece {
                ChunkPiece::Solid { center, size } => {
                    spawn_box(commands, meshes, center, size, mats.stone.clone(), true);
                }
                ChunkPiece::Trim { center, size } => {
                    spawn_box(commands, meshes, center, size, mats.accent.clone(), false);
                }
                ChunkPiece::Scenery { center, size } => {
                    spawn_box(commands, meshes, center, size, mats.scenery.clone(), false);
                }
                ChunkPiece::Prefab {
                    kind,
                    center,
                    yaw_degrees,
                } => {
                    spawn_prefab(
                        commands,
                        meshes,
                        mats.accent.clone(),
                        kind,
                        center,
                        yaw_degrees,
                    );
                }
            }
            spawned += 1;
        }
    }
    spawned
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::platformer_routes::{route_by_id, ROUTES};

    fn spawn_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>();
        app
    }

    fn build(app: &mut App, route_id: &str) -> usize {
        let route = route_by_id(route_id).expect("route exists");
        let placed = route.assemble(Vec3::ZERO).expect("assembles");
        let world = app.world_mut();
        // Nested scopes so mesh and material stores can be held mutably at
        // once without borrowing the world twice.
        let count = world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut materials: Mut<Assets<StandardMaterial>>| {
                let mut commands_queue = bevy::ecs::world::CommandQueue::default();
                let mut commands = Commands::new(&mut commands_queue, world);
                let spawned = spawn_route(&mut commands, &mut meshes, &mut materials, &placed);
                commands_queue.apply(world);
                spawned
            })
        });
        count
    }

    #[test]
    fn a_route_spawns_every_piece_it_declares() {
        let mut app = spawn_app();
        let route = route_by_id("route_castle_stairwell").unwrap();
        let placed = route.assemble(Vec3::ZERO).unwrap();
        // One marker entity per chunk plus every authored piece.
        let expected: usize = placed.len()
            + placed
                .iter()
                .map(|chunk| chunk.world_pieces().len())
                .sum::<usize>();

        let spawned = build(&mut app, "route_castle_stairwell");
        assert_eq!(
            spawned, expected,
            "every authored piece must reach the world"
        );

        let mut geometry = app.world_mut().query::<&RouteGeometry>();
        assert_eq!(geometry.iter(app.world()).count(), expected);
    }

    #[test]
    fn a_route_that_crosses_themes_paints_each_chunk_in_its_own_stone() {
        // route_mountain_ascent deliberately ends at a city plaza; painting
        // that chunk in mountain stone would be wrong.
        let route = route_by_id("route_mountain_ascent").unwrap();
        let themes: Vec<ChunkTheme> = route
            .resolve()
            .unwrap()
            .iter()
            .map(|chunk| chunk.theme)
            .collect();
        assert!(
            themes.iter().any(|theme| *theme != route.theme),
            "this test only means something for a mixed-theme route"
        );

        let mut app = spawn_app();
        let before = app.world().resource::<Assets<StandardMaterial>>().len();
        build(&mut app, "route_mountain_ascent");
        let after = app.world().resource::<Assets<StandardMaterial>>().len();

        let distinct_themes = {
            let mut unique = themes.clone();
            unique.sort_by_key(|theme| format!("{theme:?}"));
            unique.dedup();
            unique.len()
        };
        // Three materials per theme, built once per theme and reused.
        assert_eq!(after - before, distinct_themes * 3);
    }

    #[test]
    fn only_route_surfaces_are_walkable() {
        let mut app = spawn_app();
        build(&mut app, "route_castle_stairwell");

        let mut walkable = app
            .world_mut()
            .query::<(&WalkableSurface, &RouteGeometry)>();
        let walkable_count = walkable.iter(app.world()).count();
        let mut all = app.world_mut().query::<&RouteGeometry>();
        let total = all.iter(app.world()).count();

        assert!(walkable_count > 0, "a route must have footing");
        assert!(
            walkable_count < total,
            "scenery and trim must not advertise themselves as footing"
        );
    }

    #[test]
    fn building_by_progression_index_reports_what_it_made() {
        let mut app = spawn_app();
        let world = app.world_mut();
        let built = world
            .resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                world.resource_scope(|world, mut materials: Mut<Assets<StandardMaterial>>| {
                    let mut queue = bevy::ecs::world::CommandQueue::default();
                    let mut commands = Commands::new(&mut queue, world);
                    let built = spawn_route_by_index(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        0,
                        Vec3::ZERO,
                    );
                    queue.apply(world);
                    built
                })
            })
            .expect("the first route builds");

        assert_eq!(built.id, "route_city_rooftops");
        assert!(built.pieces > 0);
        assert!(!built.brief.is_empty());
        // The city route climbs a scaffold, so its finish is above its start.
        assert!(built.climb > 0.0);
        assert!(built.finish.x > 0.0, "a route travels somewhere");
    }

    #[test]
    fn a_valid_published_route_overrides_the_matching_builtin_level() {
        let mut catalog =
            super::super::platformer_routes::PublishedPlatformerRouteCatalog::default();
        catalog.seed_from_published([crate::platformer_graph::PlatformerRouteDocument {
            schema_version: crate::platformer_graph::PLATFORMER_ROUTE_SCHEMA_VERSION,
            id: crate::graph::StableId::new("route_city_rooftops").unwrap(),
            label: "Forge Rooftop Override".into(),
            brief: "Published route selected".into(),
            theme: crate::graph::StableId::new("heavy_water.theme.cityscape").unwrap(),
            chunks: vec![
                crate::graph::StableId::new("city_rooftop_arrival").unwrap(),
                crate::graph::StableId::new("city_plaza_arena").unwrap(),
            ],
        }]);
        let mut app = spawn_app();
        let world = app.world_mut();
        let built = world
            .resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                world.resource_scope(|world, mut materials: Mut<Assets<StandardMaterial>>| {
                    let mut queue = bevy::ecs::world::CommandQueue::default();
                    let mut commands = Commands::new(&mut queue, world);
                    let built = spawn_route_by_index_with_catalog(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        Some(&catalog),
                        0,
                        Vec3::ZERO,
                    );
                    queue.apply(world);
                    built
                })
            })
            .unwrap();

        assert_eq!(built.id, "route_city_rooftops");
        assert_eq!(built.label, "Forge Rooftop Override");
        assert_eq!(built.brief, "Published route selected");
        assert!(built.pieces > 0);
    }

    #[test]
    fn an_index_past_the_last_level_is_refused_rather_than_guessed() {
        let mut app = spawn_app();
        let world = app.world_mut();
        let result = world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut materials: Mut<Assets<StandardMaterial>>| {
                let mut queue = bevy::ecs::world::CommandQueue::default();
                let mut commands = Commands::new(&mut queue, world);
                let result = spawn_route_by_index(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    999,
                    Vec3::ZERO,
                );
                queue.apply(world);
                result
            })
        });
        assert!(result.is_err());
        // Nothing was built for a refused index.
        let mut geometry = app.world_mut().query::<&RouteGeometry>();
        assert_eq!(geometry.iter(app.world()).count(), 0);
    }

    #[test]
    fn every_shipped_route_builds_without_panicking() {
        for route in ROUTES {
            let mut app = spawn_app();
            let spawned = build(&mut app, route.id);
            assert!(spawned > 0, "{} built nothing", route.label);
        }
    }
}

/// Build the route at a progression index, returning what was built.
///
/// This is the entry point the shared-screen experience uses: the party's
/// progression index selects a level, the catalogue supplies its chunks, and
/// the assembler decides where everything sits. A route whose ids or shape do
/// not validate is refused rather than half-built, so a broken level can never
/// strand players in geometry that does not connect.
pub fn spawn_route_by_index(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    index: usize,
    start: Vec3,
) -> Result<SpawnedRoute, String> {
    spawn_route_by_index_with_catalog(commands, meshes, materials, None, index, start)
}

/// Build a route while allowing a validated published document with the same
/// stable ID to override Heavy Water's built-in definition.
pub fn spawn_route_by_index_with_catalog(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    published: Option<&super::platformer_routes::PublishedPlatformerRouteCatalog>,
    index: usize,
    start: Vec3,
) -> Result<SpawnedRoute, String> {
    use super::platformer_chunks::JumpEnvelope;
    use super::platformer_routes::route_at;

    let route = route_at(index).ok_or_else(|| format!("no route at index {index}"))?;
    if let Some(document) = published.and_then(|catalog| catalog.get(route.id)) {
        let compiled = document
            .compile_runtime(
                start.to_array(),
                JumpEnvelope::standard(),
                super::platformer_chunk_library::chunk_by_id,
            )
            .map_err(|error| format!("published route {} is not playable: {error}", route.id))?;
        let climb = super::platformer_chunks::route_climb(&compiled.chunks);
        let finish = compiled
            .chunks
            .last()
            .map(|chunk| chunk.world_exit())
            .unwrap_or(start);
        let pieces = spawn_route(commands, meshes, materials, &compiled.chunks);
        return Ok(SpawnedRoute {
            id: document.id.to_string(),
            label: document.label.clone(),
            brief: document.brief.clone(),
            pieces,
            climb,
            finish,
        });
    }
    let problems = route.validate(JumpEnvelope::standard());
    if !problems.is_empty() {
        return Err(format!(
            "{} is not playable: {}",
            route.label,
            problems
                .iter()
                .map(|problem| problem.message())
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    let placed = route.assemble(start).map_err(|problem| problem.message())?;
    let pieces = spawn_route(commands, meshes, materials, &placed);
    Ok(SpawnedRoute {
        id: route.id.into(),
        label: route.label.into(),
        brief: route.brief.into(),
        pieces,
        climb: super::platformer_chunks::route_climb(&placed),
        finish: placed
            .last()
            .map(|chunk| chunk.world_exit())
            .unwrap_or(start),
    })
}

/// What a successful route build produced, for HUD text and progression.
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnedRoute {
    pub id: String,
    pub label: String,
    pub brief: String,
    pub pieces: usize,
    pub climb: f32,
    /// Where the route ends — the party's destination for this level.
    pub finish: Vec3,
}
