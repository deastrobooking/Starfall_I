//! Runtime projection for authored Spacecraft Forge recipes.
//!
//! This adapter deliberately stays beside, rather than inside, the legacy
//! Heavy Water and command-save enums. It owns session-stable parked rides and
//! strategic presentation; `VehiclePlugin` remains the only mount/motor owner.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;

use crate::components::player::{Player, PlayerIndex};
use crate::components::world::{FreePeopleGuardianShip, WorldGeometry};
use crate::engine::state::AppState;
use crate::resources::{PlaySessionTransition, WorldSiteId, WorldSiteRegistry};
use crate::spaceship_forge::{
    despawn_generated_spacecraft, spacecraft_presentation_scale, spawn_spacecraft,
    GeneratedSpacecraftAssets, PublishedSpacecraftCatalog, SpacecraftClass, SpacecraftSpec,
};
use crate::world::published_craft::{
    PublishedFleet, PublishedFleetRole, PublishedFleetVisual, PublishedRide,
};

const RIDE_PRESENTATION_EXTENT: f32 = 6.8;
const RIDE_SPACING: f32 = 11.0;
const RIDE_ROWS: usize = 4;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishedSpacecraftRideVisual;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PublishedFleetPatrol {
    pub center: Vec3,
    pub radius: f32,
    pub altitude: f32,
    pub angular_speed: f32,
    pub phase: f32,
    pub bob: f32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishedOrbitalBase;

pub struct PublishedSpacecraftRuntimePlugin;

impl Plugin for PublishedSpacecraftRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PublishedFleet>()
            .add_systems(
                Update,
                reconcile_published_spacecraft_runtime.run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                OnExit(AppState::Playing),
                cleanup_published_spacecraft_runtime.run_if(non_pause_transition),
            )
            // The pause-menu Title path does not pass through Playing again.
            // MainMenu therefore owns the same asset-aware terminal cleanup.
            .add_systems(
                OnEnter(AppState::MainMenu),
                cleanup_published_spacecraft_runtime,
            );
    }
}

/// A sorted, deterministic projection used by both reconciliation and tests.
pub fn pilotable_spacecraft_specs(catalog: &PublishedSpacecraftCatalog) -> Vec<&SpacecraftSpec> {
    catalog
        .content_ids()
        .filter_map(|content_id| catalog.get(content_id))
        .filter(|spec| {
            matches!(
                spec.class,
                SpacecraftClass::Fighter | SpacecraftClass::Bomber
            )
        })
        .collect()
}

fn first_player_anchor(players: &Query<(&PlayerIndex, &Transform), With<Player>>) -> Option<Vec3> {
    players
        .iter()
        .min_by_key(|(index, _)| index.0)
        .map(|(_, transform)| transform.translation)
}

fn parked_ride_offset(index: usize) -> Vec3 {
    let column = index % RIDE_ROWS;
    let row = index / RIDE_ROWS;
    Vec3::new(
        (column as f32 - (RIDE_ROWS as f32 - 1.0) * 0.5) * RIDE_SPACING,
        1.0,
        13.0 + row as f32 * RIDE_SPACING,
    )
}

fn sorted_liberated_sites(sites: &WorldSiteRegistry) -> Vec<(WorldSiteId, Vec3)> {
    let mut liberated = sites
        .sites
        .iter()
        .filter(|site| site.is_liberated())
        .map(|site| (site.id, Vec3::new(site.world_x, 0.0, site.world_z)))
        .collect::<Vec<_>>();
    liberated.sort_by_key(|(site, _)| site.0);
    liberated
}

fn fleet_presentation(role: PublishedFleetRole, index: usize) -> FleetPresentation {
    let phase = index as f32 * 2.399_963_1;
    match role {
        PublishedFleetRole::Fighter => FleetPresentation::Patrol {
            radius: 52.0,
            altitude: 42.0,
            angular_speed: 0.18,
            phase,
            bob: 1.5,
            extent: 7.0,
        },
        PublishedFleetRole::Bomber => FleetPresentation::Patrol {
            radius: 66.0,
            altitude: 50.0,
            angular_speed: 0.13,
            phase,
            bob: 1.8,
            extent: 9.0,
        },
        PublishedFleetRole::Destroyer => FleetPresentation::Patrol {
            radius: 92.0,
            altitude: 70.0,
            angular_speed: 0.075,
            phase,
            bob: 2.4,
            extent: 17.0,
        },
        PublishedFleetRole::CommandShip => FleetPresentation::Patrol {
            radius: 118.0,
            altitude: 88.0,
            angular_speed: 0.045,
            phase,
            bob: 3.0,
            extent: 22.0,
        },
        PublishedFleetRole::OrbitalBase => FleetPresentation::Stationary {
            altitude: 118.0,
            extent: 28.0,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FleetPresentation {
    Patrol {
        radius: f32,
        altitude: f32,
        angular_speed: f32,
        phase: f32,
        bob: f32,
        extent: f32,
    },
    Stationary {
        altitude: f32,
        extent: f32,
    },
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn reconcile_published_spacecraft_runtime(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    catalog: Res<PublishedSpacecraftCatalog>,
    sites: Res<WorldSiteRegistry>,
    mut fleet: ResMut<PublishedFleet>,
    players: Query<(&PlayerIndex, &Transform), With<Player>>,
    ride_q: Query<(Entity, &PublishedRide), With<PublishedSpacecraftRideVisual>>,
    fleet_visual_q: Query<(
        Entity,
        &PublishedFleetVisual,
        Option<&GeneratedSpacecraftAssets>,
    )>,
) {
    // Existing rides are immutable session snapshots. In particular, a live
    // catalog replacement must never despawn a mounted craft beneath its owner.
    let existing_rides = ride_q
        .iter()
        .map(|(_, ride)| ride.content_id.as_str())
        .collect::<BTreeSet<_>>();
    if let Some(player_anchor) = first_player_anchor(&players) {
        for (new_index, spec) in pilotable_spacecraft_specs(&catalog)
            .into_iter()
            .filter(|spec| !existing_rides.contains(spec.content_id.as_str()))
            .enumerate()
        {
            let Some(ride) = PublishedRide::flight(spec) else {
                continue;
            };
            let position = player_anchor + parked_ride_offset(existing_rides.len() + new_index);
            let Ok(root) =
                spawn_spacecraft(&mut commands, &mut meshes, &mut materials, spec, position)
            else {
                continue;
            };
            commands.entity(root).insert((
                ride,
                PublishedSpacecraftRideVisual,
                WorldGeometry,
                Transform::from_translation(position).with_scale(Vec3::splat(
                    spacecraft_presentation_scale(spec, RIDE_PRESENTATION_EXTENT),
                )),
            ));
        }
    }

    let liberated = sorted_liberated_sites(&sites);
    let liberated_ids = liberated.iter().map(|(id, _)| *id).collect::<Vec<_>>();
    fleet.reconcile_catalog(&catalog, &liberated_ids);

    let site_positions = liberated
        .into_iter()
        .map(|(site, position)| (site.0, position))
        .collect::<BTreeMap<_, _>>();
    let desired = fleet
        .assets
        .values()
        .filter_map(|asset| {
            let site = asset.assigned_site?;
            site_positions.get(&site.0)?;
            catalog.get(&asset.content_id)?;
            Some((
                asset.instance_id.clone(),
                (site, asset.role, asset.content_id.as_str()),
            ))
        })
        .collect::<BTreeMap<_, _>>();

    let mut existing = BTreeSet::new();
    for (entity, visual, generated_assets) in &fleet_visual_q {
        if desired
            .get(&visual.instance_id)
            .is_some_and(|(site, role, content_id)| {
                *site == visual.site
                    && *role == visual.role
                    && *content_id == visual.content_id.as_str()
            })
            && existing.insert(visual.instance_id.clone())
        {
            continue;
        }
        despawn_generated_spacecraft(
            &mut commands,
            &mut meshes,
            &mut materials,
            entity,
            generated_assets,
        );
    }

    for (index, asset) in fleet.assets.values().enumerate() {
        if existing.contains(&asset.instance_id) {
            continue;
        }
        let Some(site) = asset.assigned_site else {
            continue;
        };
        let Some(center) = site_positions.get(&site.0).copied() else {
            continue;
        };
        let presentation = fleet_presentation(asset.role, index);
        let (position, extent) = match presentation {
            FleetPresentation::Patrol {
                radius,
                altitude,
                phase,
                extent,
                ..
            } => (
                center + Vec3::new(phase.cos() * radius, altitude, phase.sin() * radius),
                extent,
            ),
            FleetPresentation::Stationary { altitude, extent } => {
                (center + Vec3::Y * altitude, extent)
            }
        };
        let Ok(root) = spawn_spacecraft(
            &mut commands,
            &mut meshes,
            &mut materials,
            &asset.spec,
            position,
        ) else {
            continue;
        };
        commands.entity(root).insert((
            PublishedFleetVisual {
                instance_id: asset.instance_id.clone(),
                content_id: asset.content_id.clone(),
                site,
                role: asset.role,
            },
            WorldGeometry,
            Transform::from_translation(position).with_scale(Vec3::splat(
                spacecraft_presentation_scale(&asset.spec, extent),
            )),
        ));
        match presentation {
            FleetPresentation::Patrol {
                radius,
                altitude,
                angular_speed,
                phase,
                bob,
                ..
            } => {
                commands.entity(root).insert((
                    PublishedFleetPatrol {
                        center,
                        radius,
                        altitude,
                        angular_speed,
                        phase,
                        bob,
                    },
                    FreePeopleGuardianShip {
                        center,
                        radius,
                        altitude,
                        angular_speed,
                        phase,
                        bob,
                    },
                ));
            }
            FleetPresentation::Stationary { .. } => {
                commands.entity(root).insert(PublishedOrbitalBase);
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn non_pause_transition(transition: Res<PlaySessionTransition>) -> bool {
    !transition.pausing
}

fn cleanup_published_spacecraft_runtime(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fleet: ResMut<PublishedFleet>,
    generated_q: Query<
        (Entity, Option<&GeneratedSpacecraftAssets>),
        Or<(
            With<PublishedSpacecraftRideVisual>,
            With<PublishedFleetVisual>,
        )>,
    >,
) {
    for (entity, generated_assets) in &generated_q {
        despawn_generated_spacecraft(
            &mut commands,
            &mut meshes,
            &mut materials,
            entity,
            generated_assets,
        );
    }
    fleet.assets.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::AssetPlugin;
    use bevy::state::app::StatesPlugin;

    fn spec(class: SpacecraftClass, suffix: &str) -> SpacecraftSpec {
        let mut spec = SpacecraftSpec::preset(class);
        spec.content_id = format!("starfall.spaceship.{suffix}");
        spec
    }

    fn runtime_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, AssetPlugin::default()))
            .init_state::<AppState>()
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_resource::<PublishedSpacecraftCatalog>()
            .init_resource::<WorldSiteRegistry>()
            .init_resource::<PlaySessionTransition>()
            .add_plugins(PublishedSpacecraftRuntimePlugin);
        app.world_mut()
            .spawn((Player, PlayerIndex(0), Transform::from_xyz(10.0, 2.0, 20.0)));
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Playing);
        app.update();
        app
    }

    fn seed_catalog(app: &mut App, specs: impl IntoIterator<Item = SpacecraftSpec>) {
        app.world_mut()
            .resource_mut::<PublishedSpacecraftCatalog>()
            .replace(specs);
    }

    fn component_count<T: Component>(app: &mut App) -> usize {
        let world = app.world_mut();
        let mut query = world.query::<&T>();
        query.iter(world).count()
    }

    #[test]
    fn every_class_has_the_expected_runtime_mapping() {
        let expected = [true, true, false, false, false];
        for (class, expected) in SpacecraftClass::ALL.into_iter().zip(expected) {
            assert_eq!(
                matches!(class, SpacecraftClass::Fighter | SpacecraftClass::Bomber),
                expected
            );
            assert_eq!(
                PublishedRide::flight(&SpacecraftSpec::preset(class)).is_some(),
                expected
            );
            assert_eq!(
                PublishedFleetRole::from(class),
                match class {
                    SpacecraftClass::Fighter => PublishedFleetRole::Fighter,
                    SpacecraftClass::Bomber => PublishedFleetRole::Bomber,
                    SpacecraftClass::Destroyer => PublishedFleetRole::Destroyer,
                    SpacecraftClass::CommandShip => PublishedFleetRole::CommandShip,
                    SpacecraftClass::Base => PublishedFleetRole::OrbitalBase,
                }
            );
        }
    }

    #[test]
    fn late_catalog_reconciliation_is_sorted_idempotent_and_snapshot_stable() {
        let mut app = runtime_app();
        assert_eq!(
            component_count::<PublishedSpacecraftRideVisual>(&mut app),
            0
        );

        let fighter = spec(SpacecraftClass::Fighter, "zeta");
        let bomber = spec(SpacecraftClass::Bomber, "alpha");
        seed_catalog(&mut app, [fighter.clone(), bomber.clone()]);
        app.update();
        app.update();

        let world = app.world_mut();
        let mut ride_query = world.query::<(&PublishedRide, &Transform)>();
        let ride_positions = ride_query
            .iter(world)
            .map(|(ride, transform)| (ride.content_id.clone(), transform.translation))
            .collect::<BTreeMap<_, _>>();
        let mut rides = ride_positions.keys().cloned().collect::<Vec<_>>();
        rides.sort();
        assert_eq!(
            rides,
            vec![bomber.content_id.clone(), fighter.content_id.clone()]
        );
        assert!(ride_positions[&bomber.content_id].x < ride_positions[&fighter.content_id].x);

        let mut changed = fighter.clone();
        changed.display_name = "Republished Name".into();
        seed_catalog(&mut app, [changed]);
        app.update();
        let world = app.world_mut();
        let mut ride_query = world.query::<&PublishedRide>();
        let rides = ride_query
            .iter(world)
            .map(|ride| (ride.content_id.clone(), ride.display_name.clone()))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(rides.len(), 2, "session rides survive catalog replacement");
        assert_eq!(rides[&fighter.content_id], fighter.display_name);
    }

    #[test]
    fn all_fleet_roles_spawn_once_and_only_base_is_stationary() {
        let mut app = runtime_app();
        seed_catalog(
            &mut app,
            SpacecraftClass::ALL
                .into_iter()
                .enumerate()
                .map(|(index, class)| spec(class, &format!("role_{index}"))),
        );
        app.update();
        app.update();

        let visual_count = component_count::<PublishedFleetVisual>(&mut app);
        let patrol_count = component_count::<PublishedFleetPatrol>(&mut app);
        let base_count = component_count::<PublishedOrbitalBase>(&mut app);
        assert_eq!(visual_count, 5);
        assert_eq!(patrol_count, 4);
        assert_eq!(base_count, 1);
    }

    #[test]
    fn non_pause_exit_clears_session_but_pause_preserves_it() {
        let mut app = runtime_app();
        seed_catalog(&mut app, [spec(SpacecraftClass::Fighter, "cleanup")]);
        app.update();
        assert!(!app.world().resource::<PublishedFleet>().assets.is_empty());

        app.world_mut()
            .resource_mut::<PlaySessionTransition>()
            .pausing = true;
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Paused);
        app.update();
        assert!(!app.world().resource::<PublishedFleet>().assets.is_empty());

        app.world_mut()
            .resource_mut::<PlaySessionTransition>()
            .pausing = false;
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::MainMenu);
        app.update();
        assert!(app.world().resource::<PublishedFleet>().assets.is_empty());
        assert_eq!(
            component_count::<PublishedSpacecraftRideVisual>(&mut app),
            0
        );
    }
}
