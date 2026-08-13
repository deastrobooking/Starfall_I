//! Runtime projection for authored Vehicle Forge recipes.
//!
//! The adapter owns presentation and session snapshots only. The existing
//! vehicle gameplay plugin remains the sole mount, route, and motor owner.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use bevy::prelude::*;

use crate::components::player::{Player, PlayerIndex};
use crate::components::world::{BoatVehicle, WorldGeometry};
use crate::engine::state::AppState;
use crate::resources::PlaySessionTransition;
use crate::vehicle_forge::{
    despawn_generated_vehicle, spawn_vehicle, vehicle_presentation_scale, GeneratedVehicleAssets,
    PublishedVehicleCatalog, VehicleClass, VehicleSpec,
};
use crate::world::published_craft::{PublishedBoatBinding, PublishedRide};

const GROUND_PRESENTATION_EXTENT: f32 = 7.5;
const BOAT_PRESENTATION_EXTENT: f32 = 9.0;
const PARKING_SPACING: f32 = 10.0;
const PARKING_COLUMNS: usize = 3;

/// Marks a standalone parked authored vehicle root.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishedGroundVehicle;

/// Marks the generated visual root attached beneath an existing boat route.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishedVehicleVisualRoot;

pub struct PublishedVehicleRuntimePlugin;

impl Plugin for PublishedVehicleRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            reconcile_published_vehicle_runtime.run_if(in_state(AppState::Playing)),
        )
        .add_systems(
            OnExit(AppState::Playing),
            cleanup_published_vehicle_runtime.run_if(non_pause_transition),
        )
        // Leaving from the pause menu never re-enters Playing, so the
        // asset-aware cleanup also belongs to the terminal menu state.
        .add_systems(
            OnEnter(AppState::MainMenu),
            cleanup_published_vehicle_runtime,
        );
    }
}

/// Sorted runtime projection. `PublishedVehicleCatalog` uses a `BTreeMap`, but
/// retaining the explicit sort makes this contract robust to future storage
/// changes and straightforward to test.
pub fn ground_vehicle_specs(catalog: &PublishedVehicleCatalog) -> Vec<&VehicleSpec> {
    let mut specs = catalog
        .content_ids()
        .filter_map(|content_id| catalog.get(content_id))
        .filter(|spec| spec.class != VehicleClass::Boat)
        .collect::<Vec<_>>();
    specs.sort_by(|left, right| left.content_id.cmp(&right.content_id));
    specs
}

pub fn boat_vehicle_specs(catalog: &PublishedVehicleCatalog) -> Vec<&VehicleSpec> {
    let mut specs = catalog
        .content_ids()
        .filter_map(|content_id| catalog.get(content_id))
        .filter(|spec| spec.class == VehicleClass::Boat)
        .collect::<Vec<_>>();
    specs.sort_by(|left, right| left.content_id.cmp(&right.content_id));
    specs
}

fn first_player_transform(
    players: &Query<(&PlayerIndex, &Transform), With<Player>>,
) -> Option<Transform> {
    players
        .iter()
        .min_by_key(|(index, _)| index.0)
        .map(|(_, transform)| *transform)
}

fn parked_vehicle_offset(index: usize) -> Vec3 {
    let column = index % PARKING_COLUMNS;
    let row = index / PARKING_COLUMNS;
    Vec3::new(
        (column as f32 - (PARKING_COLUMNS as f32 - 1.0) * 0.5) * PARKING_SPACING,
        0.0,
        12.0 + row as f32 * PARKING_SPACING,
    )
}

fn compare_vec3(left: Vec3, right: Vec3) -> Ordering {
    left.x
        .total_cmp(&right.x)
        .then_with(|| left.z.total_cmp(&right.z))
        .then_with(|| left.y.total_cmp(&right.y))
}

fn compare_boat_routes(left: &BoatVehicle, right: &BoatVehicle) -> Ordering {
    compare_vec3(left.dock_position, right.dock_position)
        .then_with(|| compare_vec3(left.island_position, right.island_position))
}

fn remove_legacy_render_hierarchy(
    commands: &mut Commands,
    root: Entity,
    children_q: &Query<&Children>,
) {
    let mut pending = vec![root];
    while let Some(entity) = pending.pop() {
        commands.entity(entity).remove::<Mesh3d>();
        commands
            .entity(entity)
            .remove::<MeshMaterial3d<StandardMaterial>>();
        if let Ok(children) = children_q.get(entity) {
            pending.extend(children.iter());
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn reconcile_published_vehicle_runtime(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    catalog: Res<PublishedVehicleCatalog>,
    players: Query<(&PlayerIndex, &Transform), With<Player>>,
    existing_ground_q: Query<&PublishedRide, With<PublishedGroundVehicle>>,
    boats_q: Query<(Entity, &BoatVehicle, Option<&PublishedBoatBinding>)>,
    children_q: Query<&Children>,
) {
    // Existing roots intentionally remain immutable session snapshots. A live
    // catalog replacement therefore cannot despawn an active mounted ride.
    let existing_ground = existing_ground_q
        .iter()
        .map(|ride| ride.content_id.as_str())
        .collect::<BTreeSet<_>>();
    if let Some(player) = first_player_transform(&players) {
        for (new_index, spec) in ground_vehicle_specs(&catalog)
            .into_iter()
            .filter(|spec| !existing_ground.contains(spec.content_id.as_str()))
            .enumerate()
        {
            let Some(ride) = PublishedRide::ground(spec) else {
                continue;
            };
            let index = existing_ground.len() + new_index;
            let position = player.translation + player.rotation * parked_vehicle_offset(index);
            let Ok(root) =
                spawn_vehicle(&mut commands, &mut meshes, &mut materials, spec, position)
            else {
                continue;
            };
            commands.entity(root).insert((
                ride,
                PublishedGroundVehicle,
                WorldGeometry,
                Transform::from_translation(position).with_scale(Vec3::splat(
                    vehicle_presentation_scale(spec, GROUND_PRESENTATION_EXTENT).min(1.0),
                )),
            ));
        }
    }

    let boat_specs = boat_vehicle_specs(&catalog);
    if boat_specs.is_empty() {
        return;
    }
    let mut routes = boats_q.iter().collect::<Vec<_>>();
    routes.sort_by(|(_, left, _), (_, right, _)| compare_boat_routes(left, right));
    for (index, (boat_root, _, binding)) in routes.into_iter().enumerate() {
        if binding.is_some() {
            continue;
        }
        let spec = boat_specs[index % boat_specs.len()];
        let Ok(visual_root) =
            spawn_vehicle(&mut commands, &mut meshes, &mut materials, spec, Vec3::ZERO)
        else {
            continue;
        };
        remove_legacy_render_hierarchy(&mut commands, boat_root, &children_q);
        commands.entity(visual_root).insert((
            PublishedVehicleVisualRoot,
            Transform::from_scale(Vec3::splat(
                vehicle_presentation_scale(spec, BOAT_PRESENTATION_EXTENT).min(1.0),
            )),
        ));
        commands
            .entity(boat_root)
            .add_child(visual_root)
            .insert(PublishedBoatBinding {
                content_id: spec.content_id.clone(),
            });
    }
}

#[allow(clippy::type_complexity)]
fn non_pause_transition(transition: Res<PlaySessionTransition>) -> bool {
    !transition.pausing
}

fn cleanup_published_vehicle_runtime(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    generated_q: Query<
        (Entity, Option<&GeneratedVehicleAssets>),
        Or<(
            With<PublishedGroundVehicle>,
            With<PublishedVehicleVisualRoot>,
        )>,
    >,
    bound_boats_q: Query<(Entity, &PublishedBoatBinding)>,
) {
    for (entity, generated_assets) in &generated_q {
        despawn_generated_vehicle(
            &mut commands,
            &mut meshes,
            &mut materials,
            entity,
            generated_assets,
        );
    }
    for (boat, binding) in &bound_boats_q {
        trace!(
            "unbinding published boat {} during play-session cleanup",
            binding.content_id
        );
        commands.entity(boat).remove::<PublishedBoatBinding>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::AssetPlugin;
    use bevy::state::app::StatesPlugin;

    use crate::vehicle_forge::GeneratedVehicle;

    fn spec(class: VehicleClass, suffix: &str) -> VehicleSpec {
        let mut spec = VehicleSpec::preset(class);
        spec.content_id = format!("starfall.vehicle.{suffix}");
        spec.display_name = format!("{suffix} {}", class.label());
        spec
    }

    fn runtime_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, AssetPlugin::default()))
            .init_state::<AppState>()
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_resource::<PublishedVehicleCatalog>()
            .init_resource::<PlaySessionTransition>()
            .add_plugins(PublishedVehicleRuntimePlugin);
        app.world_mut().spawn((
            Player,
            PlayerIndex(1),
            Transform::from_xyz(1_000.0, 0.0, 1_000.0),
        ));
        app.world_mut()
            .spawn((Player, PlayerIndex(0), Transform::from_xyz(10.0, 0.0, 20.0)));
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Playing);
        app.update();
        app
    }

    fn seed_catalog(app: &mut App, specs: impl IntoIterator<Item = VehicleSpec>) {
        app.world_mut()
            .resource_mut::<PublishedVehicleCatalog>()
            .replace(specs);
    }

    fn component_count<T: Component>(app: &mut App) -> usize {
        let world = app.world_mut();
        let mut query = world.query::<&T>();
        query.iter(world).count()
    }

    #[test]
    fn ground_projection_is_sorted_incremental_idempotent_and_snapshot_stable() {
        let mut app = runtime_app();
        let alpha = spec(VehicleClass::Truck, "alpha");
        let zeta = spec(VehicleClass::Car, "zeta");
        let boat = spec(VehicleClass::Boat, "not_ground");

        seed_catalog(&mut app, [alpha.clone(), boat.clone()]);
        app.update();
        seed_catalog(&mut app, [zeta.clone(), boat, alpha.clone()]);
        app.update();
        app.update();

        let world = app.world_mut();
        let mut query = world.query_filtered::<
            (&PublishedRide, &Transform, &GeneratedVehicle),
            With<PublishedGroundVehicle>,
        >();
        let rides = query
            .iter(world)
            .map(|(ride, transform, generated)| {
                (
                    ride.content_id.clone(),
                    (transform.translation, generated.spec.clone()),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(rides.len(), 2);
        assert_eq!(rides[&alpha.content_id].0, Vec3::new(0.0, 0.0, 32.0));
        assert_eq!(rides[&zeta.content_id].0, Vec3::new(10.0, 0.0, 32.0));
        assert_eq!(rides[&alpha.content_id].1, alpha);
        assert_eq!(rides[&zeta.content_id].1, zeta);
    }

    fn boat_route(dock_x: f32) -> BoatVehicle {
        BoatVehicle {
            embark_radius: 5.0,
            passenger_radius: 3.0,
            dock_radius: 4.0,
            route_half_width: 20.0,
            speed: 8.0,
            dock_position: Vec3::new(dock_x, 0.0, 30.0),
            island_position: Vec3::new(dock_x, 0.0, -30.0),
        }
    }

    fn spawn_legacy_boat(app: &mut App, route: BoatVehicle) -> (Entity, Entity) {
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(2.0, 1.0, 5.0));
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let child = app
            .world_mut()
            .spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::default(),
            ))
            .id();
        let root = app
            .world_mut()
            .spawn((
                route,
                WorldGeometry,
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::default(),
            ))
            .id();
        app.world_mut().entity_mut(root).add_child(child);
        (root, child)
    }

    #[test]
    fn boat_specs_bind_routes_deterministically_without_becoming_ground_rides() {
        let mut app = runtime_app();
        let zeta = spec(VehicleClass::Boat, "zeta_boat");
        let alpha = spec(VehicleClass::Boat, "alpha_boat");
        seed_catalog(&mut app, [zeta.clone(), alpha.clone()]);

        let (east, east_legacy_child) = spawn_legacy_boat(&mut app, boat_route(20.0));
        let (west, west_legacy_child) = spawn_legacy_boat(&mut app, boat_route(-20.0));
        app.update();
        app.update();

        assert_eq!(
            app.world()
                .get::<PublishedBoatBinding>(west)
                .unwrap()
                .content_id,
            alpha.content_id
        );
        assert_eq!(
            app.world()
                .get::<PublishedBoatBinding>(east)
                .unwrap()
                .content_id,
            zeta.content_id
        );
        for (root, legacy_child) in [(west, west_legacy_child), (east, east_legacy_child)] {
            assert!(app.world().get::<BoatVehicle>(root).is_some());
            assert!(app.world().get::<WorldGeometry>(root).is_some());
            assert!(app.world().get::<PublishedRide>(root).is_none());
            assert!(app.world().get::<Mesh3d>(root).is_none());
            assert!(app.world().get::<Mesh3d>(legacy_child).is_none());
        }
        assert_eq!(component_count::<PublishedVehicleVisualRoot>(&mut app), 2);
        let world = app.world_mut();
        let mut generated = world.query::<(&GeneratedVehicle, &PublishedVehicleVisualRoot)>();
        assert_eq!(generated.iter(world).count(), 2);
        assert!(generated
            .iter(world)
            .all(|(vehicle, _)| vehicle.class == VehicleClass::Boat));
    }

    #[test]
    fn pause_preserves_assets_but_paused_to_menu_releases_them() {
        let mut app = runtime_app();
        seed_catalog(&mut app, [spec(VehicleClass::Motorcycle, "cleanup")]);
        app.update();
        assert_eq!(component_count::<PublishedGroundVehicle>(&mut app), 1);
        assert!(!app.world().resource::<Assets<Mesh>>().is_empty());
        assert!(!app
            .world()
            .resource::<Assets<StandardMaterial>>()
            .is_empty());

        app.world_mut()
            .resource_mut::<PlaySessionTransition>()
            .pausing = true;
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Paused);
        app.update();
        assert_eq!(component_count::<PublishedGroundVehicle>(&mut app), 1);

        app.world_mut()
            .resource_mut::<PlaySessionTransition>()
            .pausing = false;
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::MainMenu);
        app.update();
        assert_eq!(component_count::<PublishedGroundVehicle>(&mut app), 0);
        assert!(app.world().resource::<Assets<Mesh>>().is_empty());
        assert!(app
            .world()
            .resource::<Assets<StandardMaterial>>()
            .is_empty());
    }
}
