//! Vehicle plugin — ground and air vehicle modes, gated by blueprints and robot assembly.
//!
//! Blueprint vehicles (discoverable pickups):
//!   J / enter_vehicle → motorcycle when only a ground form is available
//!   J / enter_vehicle → jet when only an air form is available
//!   J / enter_vehicle → Jet Bike ground → flight → off when assembled
//!
//! Assembly vehicles (Robot Garage assembles a form from pets + parts):
//!   J / enter_vehicle → tank/mech/ship mode when its assembly is active
//!
//! Only one ground mode and one air mode may be active at a time.

use bevy::prelude::*;

use crate::components::mods::PlayerLoadout;
use crate::components::player::{
    JetpackState, Player, PlayerIndex, PlayerInput, PlayerMovement, PlayerStats,
};
use crate::components::world::{BoatPassenger, BoatVehicle};
use crate::events::UiMessageEvent;
use crate::rendering::{PbrBundle, SpatialBundle};
use crate::resources::PlaySessionTransition;
use crate::robot_pets::{RobotAssemblyForm, RobotPetCollection};
use crate::state::AppState;

pub struct VehiclePlugin;

impl Plugin for VehiclePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VehicleState>()
            .add_systems(OnEnter(AppState::Playing), reset_vehicle_state_on_enter)
            .add_systems(
                Update,
                (
                    vehicle_input,
                    clear_stale_boat_passengers_system,
                    apply_vehicle_buffs,
                    sync_jet_bike_visual,
                    boat_drive_system,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

/// Which ground-mode vehicle is currently active (at most one at a time).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroundMode {
    #[default]
    None,
    Motorcycle,
    Tank,
    GiantMech,
}

/// Which air-mode vehicle is currently active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AirMode {
    #[default]
    None,
    Jet,
    Ship,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JetBikeMode {
    Ground,
    Flight,
}

#[derive(Debug, Clone, Copy)]
struct VehicleBaseline {
    walk_speed: f32,
    sprint_speed: f32,
    jet_force: f32,
    jet_regen_rate: f32,
    jet_max_vertical_vel: f32,
}

#[derive(Resource, Debug, Default)]
pub struct VehicleState {
    pub ground_mode: GroundMode,
    pub air_mode: AirMode,
    pub boat_active: bool,
    pub active_owner: Option<u8>,
    pub active_boat: Option<Entity>,
    pub boat_heading: f32,
    pub jet_bike_mode: Option<JetBikeMode>,
    baselines: [Option<VehicleBaseline>; 4],
    applied_armor_bonuses: [f32; 4],
    // Legacy accessors used by external code — kept as computed properties.
    pub mech_armor_bonus: f32,
}

#[derive(Component, Debug, Clone, Copy)]
struct JetBikeVisual {
    owner: u8,
}

#[derive(Component)]
struct JetBikeGroundPart;

#[derive(Component)]
struct JetBikeFlightPart;

#[allow(dead_code)] // Mode query helpers for HUD/UI consumers that key off enum modes directly today.
impl VehicleState {
    pub fn motorcycle_active(&self) -> bool {
        self.ground_mode == GroundMode::Motorcycle
    }
    pub fn jet_active(&self) -> bool {
        self.air_mode == AirMode::Jet
    }
    pub fn tank_active(&self) -> bool {
        self.ground_mode == GroundMode::Tank
    }
    pub fn mech_active(&self) -> bool {
        self.ground_mode == GroundMode::GiantMech
    }
    pub fn ship_active(&self) -> bool {
        self.air_mode == AirMode::Ship
    }
    fn deactivate_ground(&mut self) {
        self.ground_mode = GroundMode::None;
        self.jet_bike_mode = None;
        self.mech_armor_bonus = 0.0;
    }
    fn deactivate_air(&mut self) {
        self.air_mode = AirMode::None;
        self.jet_bike_mode = None;
    }
    fn deactivate_all_vehicle(&mut self) {
        self.deactivate_ground();
        self.deactivate_air();
        self.boat_active = false;
        self.active_boat = None;
        self.active_owner = None;
        self.jet_bike_mode = None;
    }
}

fn reset_vehicle_state_on_enter(
    transition: Res<PlaySessionTransition>,
    mut state: ResMut<VehicleState>,
) {
    if !transition.resuming_from_pause {
        *state = VehicleState::default();
    }
}

// ── Assembly form helpers ─────────────────────────────────────────────────────

fn assembled_ground_mode(robot_pets: &RobotPetCollection) -> Option<GroundMode> {
    robot_pets
        .active_assembly
        .as_ref()
        .and_then(|a| match a.form {
            RobotAssemblyForm::Car | RobotAssemblyForm::Motorcycle | RobotAssemblyForm::JetBike => {
                Some(GroundMode::Motorcycle)
            }
            RobotAssemblyForm::Tank => Some(GroundMode::Tank),
            RobotAssemblyForm::GiantMech => Some(GroundMode::GiantMech),
            _ => None,
        })
}

fn assembled_air_mode(robot_pets: &RobotPetCollection, loadout: &PlayerLoadout) -> AirMode {
    if let Some(a) = &robot_pets.active_assembly {
        if matches!(
            a.form,
            RobotAssemblyForm::SpaceJet | RobotAssemblyForm::JetBike
        ) {
            return AirMode::Jet;
        }
        if matches!(
            a.form,
            RobotAssemblyForm::SpaceShip | RobotAssemblyForm::MegaShip
        ) {
            return AirMode::Ship;
        }
    }
    if loadout.has_blueprint("jet_blueprint") {
        return AirMode::Jet;
    }
    AirMode::None
}

fn available_ground_mode(
    robot_pets: &RobotPetCollection,
    loadout: &PlayerLoadout,
) -> Option<GroundMode> {
    assembled_ground_mode(robot_pets).or_else(|| {
        loadout
            .has_blueprint("motorcycle_blueprint")
            .then_some(GroundMode::Motorcycle)
    })
}

fn jet_bike_available(robot_pets: &RobotPetCollection, loadout: &PlayerLoadout) -> bool {
    robot_pets
        .active_assembly
        .as_ref()
        .is_some_and(|assembly| assembly.form == RobotAssemblyForm::JetBike)
        || (available_ground_mode(robot_pets, loadout) == Some(GroundMode::Motorcycle)
            && assembled_air_mode(robot_pets, loadout) == AirMode::Jet)
}

fn set_jet_bike_mode(state: &mut VehicleState, owner: u8, mode: Option<JetBikeMode>) {
    let Some(mode) = mode else {
        state.deactivate_all_vehicle();
        return;
    };
    state.active_owner = Some(owner);
    state.active_boat = None;
    state.boat_active = false;
    state.mech_armor_bonus = 0.0;
    state.jet_bike_mode = Some(mode);
    match mode {
        JetBikeMode::Ground => {
            state.ground_mode = GroundMode::Motorcycle;
            state.air_mode = AirMode::None;
        }
        JetBikeMode::Flight => {
            state.ground_mode = GroundMode::None;
            state.air_mode = AirMode::Jet;
        }
    }
}

#[allow(clippy::type_complexity)]
fn vehicle_input(
    mut commands: Commands,
    player_q: Query<
        (
            Entity,
            &PlayerIndex,
            &Transform,
            &PlayerInput,
            Option<&BoatPassenger>,
        ),
        With<Player>,
    >,
    boat_q: Query<(Entity, &Transform, &BoatVehicle)>,
    loadout: Res<PlayerLoadout>,
    robot_pets: Res<RobotPetCollection>,
    mut state: ResMut<VehicleState>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    // Commands are deferred, so reserve seats locally as players board during
    // this system run to prevent simultaneous requests claiming the same seat.
    let mut reserved_boat_seats = player_q
        .iter()
        .filter_map(|(_, _, _, _, passenger)| passenger.map(|p| (p.boat, p.seat)))
        .collect::<Vec<_>>();

    for (entity, idx, player_transform, pi, passenger) in player_q.iter() {
        let available_ground = available_ground_mode(&robot_pets, &loadout);
        let available_air = assembled_air_mode(&robot_pets, &loadout);
        let dual_jet_bike = jet_bike_available(&robot_pets, &loadout);

        // ── Party vehicle toggle (J key / D-pad Up) ───────────────────────────
        if pi.enter_vehicle {
            // Boat passenger disembark takes priority.
            if let Some(passenger) = passenger {
                let Ok((_, boat_transform, boat)) = boat_q.get(passenger.boat) else {
                    commands.entity(entity).remove::<BoatPassenger>();
                    continue;
                };
                if !boat_is_at_dock(boat_transform.translation, boat) {
                    msg_ev.write(UiMessageEvent {
                        text: "Dock at the city or island before disembarking.".into(),
                        duration: 2.0,
                    });
                    continue;
                }
                if passenger.is_driver {
                    state.boat_active = false;
                    state.active_boat = None;
                    state.active_owner = None;
                    remove_boat_passengers(&mut commands, &player_q, Some(passenger.boat));
                } else {
                    commands.entity(entity).remove::<BoatPassenger>();
                }
                msg_ev.write(UiMessageEvent {
                    text: format!("P{} Boat: docked", idx.0 + 1),
                    duration: 1.8,
                });
                continue;
            }

            // A party boat has one driver. Players who board after departure
            // become passengers instead of silently replacing its owner and
            // creating a second driver.
            if state.boat_active {
                let Some(active_boat) = state.active_boat else {
                    state.deactivate_all_vehicle();
                    continue;
                };
                let Ok((_, boat_transform, boat)) = boat_q.get(active_boat) else {
                    state.deactivate_all_vehicle();
                    continue;
                };
                let occupied_seats = reserved_boat_seats
                    .iter()
                    .filter_map(|(boat, seat)| (*boat == active_boat).then_some(*seat))
                    .collect::<Vec<_>>();
                if player_transform
                    .translation
                    .distance(boat_transform.translation)
                    > boat.passenger_radius
                {
                    msg_ev.write(UiMessageEvent {
                        text: format!(
                            "P{}: party boat is underway — meet it at a dock.",
                            idx.0 + 1
                        ),
                        duration: 2.0,
                    });
                } else if let Some(seat) = first_available_boat_seat(&occupied_seats) {
                    reserved_boat_seats.push((active_boat, seat));
                    commands.entity(entity).insert(BoatPassenger {
                        boat: active_boat,
                        seat,
                        is_driver: false,
                    });
                    msg_ev.write(UiMessageEvent {
                        text: format!(
                            "P{} boarded seat {} — P{} is driving",
                            idx.0 + 1,
                            seat + 1,
                            state.active_owner.unwrap_or(0) + 1
                        ),
                        duration: 2.0,
                    });
                } else {
                    msg_ev.write(UiMessageEvent {
                        text: "Party boat is full (4/4).".into(),
                        duration: 1.8,
                    });
                }
                continue;
            }

            // Embark nearby boat if present.
            if let Some((boat_entity, boat_transform, boat)) = boat_q
                .iter()
                .filter(|(_, bt, b)| {
                    player_transform.translation.distance(bt.translation) <= b.embark_radius
                })
                .min_by(|(_, lt, _), (_, rt, _)| {
                    player_transform
                        .translation
                        .distance(lt.translation)
                        .total_cmp(&player_transform.translation.distance(rt.translation))
                })
            {
                state.boat_active = true;
                state.ground_mode = GroundMode::None;
                state.air_mode = AirMode::None;
                state.jet_bike_mode = None;
                state.active_owner = Some(idx.0);
                state.active_boat = Some(boat_entity);
                state.boat_heading = yaw_from_rotation(boat_transform.rotation);
                let trip_distance = boat.dock_position.distance(boat.island_position);
                reserved_boat_seats.extend(board_nearby_players(
                    &mut commands,
                    &player_q,
                    boat_entity,
                    boat_transform,
                    boat,
                    entity,
                ));
                msg_ev.write(UiMessageEvent {
                    text: format!(
                        "P{} Boat: ON - steer along the wake ({:.0}m)",
                        idx.0 + 1,
                        trip_distance
                    ),
                    duration: 2.4,
                });
                continue;
            }

            // A dual-mode Jet Bike cycles Ground → Flight → Off from the
            // shared enter-vehicle input. M/open-map remains a direct ground
            // mode shortcut.
            if dual_jet_bike {
                if vehicle_owned_by_other(&state, idx.0) {
                    msg_ev.write(UiMessageEvent {
                        text: vehicle_in_use_message(&state),
                        duration: 1.8,
                    });
                    continue;
                }
                let next_mode = if state.active_owner != Some(idx.0) {
                    Some(JetBikeMode::Ground)
                } else {
                    match state.jet_bike_mode {
                        None => Some(JetBikeMode::Ground),
                        Some(JetBikeMode::Ground) => Some(JetBikeMode::Flight),
                        Some(JetBikeMode::Flight) => None,
                    }
                };
                set_jet_bike_mode(&mut state, idx.0, next_mode);
                remove_boat_passengers(&mut commands, &player_q, None);
                msg_ev.write(UiMessageEvent {
                    text: format!(
                        "P{} Jet Bike: {}",
                        idx.0 + 1,
                        match next_mode {
                            Some(JetBikeMode::Ground) => "GROUND MODE",
                            Some(JetBikeMode::Flight) => "FLIGHT MODE",
                            None => "OFF",
                        }
                    ),
                    duration: 1.8,
                });
            } else if available_air != AirMode::None {
                if vehicle_owned_by_other(&state, idx.0) {
                    msg_ev.write(UiMessageEvent {
                        text: vehicle_in_use_message(&state),
                        duration: 1.8,
                    });
                    continue;
                }
                let currently_on =
                    state.air_mode == available_air && state.active_owner == Some(idx.0);
                if currently_on {
                    state.deactivate_air();
                    state.active_owner = None;
                    msg_ev.write(UiMessageEvent {
                        text: format!("P{} {}: OFF", idx.0 + 1, air_mode_label(available_air)),
                        duration: 1.5,
                    });
                } else {
                    state.air_mode = available_air;
                    state.jet_bike_mode = None;
                    state.ground_mode = GroundMode::None;
                    state.boat_active = false;
                    state.active_boat = None;
                    remove_boat_passengers(&mut commands, &player_q, None);
                    state.active_owner = Some(idx.0);
                    msg_ev.write(UiMessageEvent {
                        text: format!("P{} {}: ON", idx.0 + 1, air_mode_label(available_air)),
                        duration: 1.5,
                    });
                }
            } else if let Some(mode) = available_ground {
                if vehicle_owned_by_other(&state, idx.0) {
                    msg_ev.write(UiMessageEvent {
                        text: vehicle_in_use_message(&state),
                        duration: 1.8,
                    });
                    continue;
                }
                let currently_on = state.ground_mode == mode && state.active_owner == Some(idx.0);
                if currently_on {
                    state.deactivate_ground();
                    state.active_owner = None;
                } else {
                    state.ground_mode = mode;
                    state.air_mode = AirMode::None;
                    state.jet_bike_mode = None;
                    state.mech_armor_bonus = if mode == GroundMode::GiantMech {
                        40.0
                    } else {
                        0.0
                    };
                    state.boat_active = false;
                    state.active_boat = None;
                    state.active_owner = Some(idx.0);
                    remove_boat_passengers(&mut commands, &player_q, None);
                }
                msg_ev.write(UiMessageEvent {
                    text: format!(
                        "P{} {}: {}",
                        idx.0 + 1,
                        ground_mode_label(mode),
                        if currently_on { "OFF" } else { "ON" }
                    ),
                    duration: 1.5,
                });
            } else {
                msg_ev.write(UiMessageEvent {
                    text: "No vehicle — assemble one in the Robot Garage or find a blueprint."
                        .into(),
                    duration: 2.0,
                });
            }
        }
    }
}

fn ground_mode_label(mode: GroundMode) -> &'static str {
    match mode {
        GroundMode::None => "None",
        GroundMode::Motorcycle => "Motorcycle",
        GroundMode::Tank => "Tank",
        GroundMode::GiantMech => "Giant Mech",
    }
}

fn air_mode_label(mode: AirMode) -> &'static str {
    match mode {
        AirMode::None => "None",
        AirMode::Jet => "Jet",
        AirMode::Ship => "Ship",
    }
}

fn vehicle_owned_by_other(state: &VehicleState, requester: u8) -> bool {
    state.active_owner.is_some_and(|owner| owner != requester)
        && (state.ground_mode != GroundMode::None
            || state.air_mode != AirMode::None
            || state.boat_active)
}

fn vehicle_in_use_message(state: &VehicleState) -> String {
    format!(
        "Party vehicle is in use by P{} — they must exit first.",
        state.active_owner.unwrap_or(0) + 1
    )
}

#[allow(clippy::type_complexity)]
fn board_nearby_players(
    commands: &mut Commands,
    player_q: &Query<
        (
            Entity,
            &PlayerIndex,
            &Transform,
            &PlayerInput,
            Option<&BoatPassenger>,
        ),
        With<Player>,
    >,
    boat_entity: Entity,
    boat_transform: &Transform,
    boat: &BoatVehicle,
    driver_entity: Entity,
) -> Vec<(Entity, u8)> {
    let mut riders: Vec<(Entity, u8, bool)> = player_q
        .iter()
        .filter(|(_, _, player_transform, _, passenger)| {
            passenger.is_none()
                && player_transform
                    .translation
                    .distance(boat_transform.translation)
                    <= boat.passenger_radius
        })
        .map(|(entity, idx, _, _, _)| (entity, idx.0, entity == driver_entity))
        .collect();

    riders.sort_by_key(|(_, index, is_driver)| (!*is_driver, *index));
    let mut occupied = Vec::new();
    for (seat, (entity, _, is_driver)) in riders.into_iter().take(4).enumerate() {
        let seat = seat as u8;
        commands.entity(entity).insert(BoatPassenger {
            boat: boat_entity,
            seat,
            is_driver,
        });
        occupied.push((boat_entity, seat));
    }
    occupied
}

#[allow(clippy::type_complexity)]
fn remove_boat_passengers(
    commands: &mut Commands,
    player_q: &Query<
        (
            Entity,
            &PlayerIndex,
            &Transform,
            &PlayerInput,
            Option<&BoatPassenger>,
        ),
        With<Player>,
    >,
    boat_filter: Option<Entity>,
) {
    for (entity, _, _, _, passenger) in player_q.iter() {
        let Some(passenger) = passenger else {
            continue;
        };
        if boat_filter.is_none_or(|boat| passenger.boat == boat) {
            commands.entity(entity).remove::<BoatPassenger>();
        }
    }
}

fn boat_is_at_dock(position: Vec3, boat: &BoatVehicle) -> bool {
    position.distance(boat.dock_position) <= boat.dock_radius
        || position.distance(boat.island_position) <= boat.dock_radius
}

fn yaw_from_rotation(rotation: Quat) -> f32 {
    let (yaw, _, _) = rotation.to_euler(EulerRot::YXZ);
    yaw
}

fn clear_stale_boat_passengers_system(
    state: Res<VehicleState>,
    mut commands: Commands,
    passenger_q: Query<(Entity, &BoatPassenger)>,
) {
    for (entity, passenger) in passenger_q.iter() {
        let passenger_stale = !state.boat_active || state.active_boat != Some(passenger.boat);
        if passenger_stale {
            commands.entity(entity).remove::<BoatPassenger>();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_jet_bike_visual(
    mut commands: Commands,
    state: Res<VehicleState>,
    time: Res<Time>,
    player_q: Query<(&PlayerIndex, &Transform, &PlayerMovement), With<Player>>,
    mut bike_q: Query<(Entity, &mut JetBikeVisual, &mut Transform), Without<Player>>,
    mut ground_parts: Query<&mut Visibility, (With<JetBikeGroundPart>, Without<JetBikeFlightPart>)>,
    mut flight_parts: Query<&mut Visibility, (With<JetBikeFlightPart>, Without<JetBikeGroundPart>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(mode) = state.jet_bike_mode else {
        for (entity, _, _) in bike_q.iter_mut() {
            commands.entity(entity).despawn();
        }
        return;
    };
    let Some(owner) = state.active_owner else {
        return;
    };
    let Some((_, rider_transform, movement)) =
        player_q.iter().find(|(index, _, _)| index.0 == owner)
    else {
        return;
    };

    let flight_mode = mode == JetBikeMode::Flight;
    for mut visibility in ground_parts.iter_mut() {
        *visibility = if flight_mode {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
    for mut visibility in flight_parts.iter_mut() {
        *visibility = if flight_mode {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    let speed = movement.ground_velocity.with_y(0.0).length();
    let hover = if flight_mode {
        (time.elapsed_secs() * 7.0).sin() * 0.07
    } else {
        (time.elapsed_secs() * 10.0).sin() * (speed * 0.006).min(0.04)
    };
    let offset = if flight_mode {
        Vec3::new(0.0, -0.38 + hover, 0.12)
    } else {
        Vec3::new(0.0, -0.72 + hover, 0.10)
    };
    let visual_transform = Transform::from_translation(
        rider_transform.translation + rider_transform.rotation * offset,
    )
    .with_rotation(rider_transform.rotation);

    if let Some((_, mut visual, mut transform)) = bike_q.iter_mut().next() {
        visual.owner = owner;
        *transform = visual_transform;
        return;
    }

    let body_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.84, 0.08, 0.48),
        metallic: 0.72,
        perceptual_roughness: 0.24,
        ..default()
    });
    let energy_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.08, 0.88, 1.0),
        metallic: 0.35,
        perceptual_roughness: 0.18,
        ..default()
    });
    let tire_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.025, 0.035, 0.055),
        metallic: 0.25,
        perceptual_roughness: 0.58,
        ..default()
    });

    commands
        .spawn((
            Name::new("Dual Mode Jet Bike"),
            SpatialBundle::from_transform(visual_transform),
            JetBikeVisual { owner },
        ))
        .with_children(|bike| {
            bike.spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(1.7, 0.55, 3.5))),
                material: MeshMaterial3d(body_material.clone()),
                transform: Transform::from_xyz(0.0, 0.0, 0.0),
                ..default()
            });
            bike.spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(1.05, 0.38, 1.35))),
                material: MeshMaterial3d(energy_material.clone()),
                transform: Transform::from_xyz(0.0, 0.08, 2.10)
                    .with_rotation(Quat::from_rotation_x(-0.20)),
                ..default()
            });
            bike.spawn(PbrBundle {
                mesh: Mesh3d(meshes.add(Cuboid::new(0.95, 0.28, 1.25))),
                material: MeshMaterial3d(tire_material.clone()),
                transform: Transform::from_xyz(0.0, 0.45, -0.45),
                ..default()
            });

            let wheel_mesh = meshes.add(Torus {
                major_radius: 0.62,
                minor_radius: 0.16,
            });
            for z in [-1.25_f32, 1.25] {
                bike.spawn((
                    PbrBundle {
                        mesh: Mesh3d(wheel_mesh.clone()),
                        material: MeshMaterial3d(tire_material.clone()),
                        transform: Transform::from_xyz(0.0, -0.48, z)
                            .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                        visibility: if flight_mode {
                            Visibility::Hidden
                        } else {
                            Visibility::Inherited
                        },
                        ..default()
                    },
                    JetBikeGroundPart,
                ));
            }

            bike.spawn((
                PbrBundle {
                    mesh: Mesh3d(meshes.add(Cuboid::new(5.2, 0.16, 1.45))),
                    material: MeshMaterial3d(energy_material.clone()),
                    transform: Transform::from_xyz(0.0, 0.02, -0.20),
                    visibility: if flight_mode {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    },
                    ..default()
                },
                JetBikeFlightPart,
            ));
            let thruster_mesh = meshes.add(Cylinder::new(0.34, 1.25));
            for x in [-1.35_f32, 1.35] {
                bike.spawn((
                    PbrBundle {
                        mesh: Mesh3d(thruster_mesh.clone()),
                        material: MeshMaterial3d(energy_material.clone()),
                        transform: Transform::from_xyz(x, -0.05, -1.45)
                            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                        visibility: if flight_mode {
                            Visibility::Inherited
                        } else {
                            Visibility::Hidden
                        },
                        ..default()
                    },
                    JetBikeFlightPart,
                ));
            }
        });
}

/// Apply vehicle speed/force buffs only to the player who activated the vehicle.
fn apply_vehicle_buffs(
    mut state: ResMut<VehicleState>,
    mut q: Query<
        (
            &PlayerIndex,
            &mut PlayerMovement,
            &mut JetpackState,
            &mut PlayerStats,
        ),
        With<Player>,
    >,
) {
    for (idx, mut mv, mut jet, mut stats) in q.iter_mut() {
        let slot_index = idx.0 as usize;
        let Some(slot) = state.baselines.get_mut(slot_index) else {
            continue;
        };
        let baseline = *slot.get_or_insert(VehicleBaseline {
            walk_speed: mv.walk_speed,
            sprint_speed: mv.sprint_speed,
            jet_force: jet.force,
            jet_regen_rate: jet.regen_rate,
            jet_max_vertical_vel: jet.max_vertical_vel,
        });

        let is_active_owner = state.active_owner == Some(idx.0);

        // Ground movement
        if is_active_owner {
            match state.ground_mode {
                GroundMode::None => {
                    if !state.boat_active {
                        mv.walk_speed = baseline.walk_speed;
                        mv.sprint_speed = baseline.sprint_speed;
                    } else {
                        // Slow on boat deck
                        mv.walk_speed = baseline.walk_speed.min(0.58);
                        mv.sprint_speed = baseline.sprint_speed.min(0.82);
                    }
                }
                GroundMode::Motorcycle => {
                    mv.walk_speed = baseline.walk_speed.max(0.70);
                    mv.sprint_speed = baseline.sprint_speed.max(1.10);
                }
                GroundMode::Tank => {
                    // Slow but cannot be knocked back
                    mv.walk_speed = baseline.walk_speed.min(0.32);
                    mv.sprint_speed = baseline.sprint_speed.min(0.42);
                }
                GroundMode::GiantMech => {
                    mv.walk_speed = baseline.walk_speed.min(0.24);
                    mv.sprint_speed = baseline.sprint_speed.min(0.34);
                }
            }
        } else {
            mv.walk_speed = baseline.walk_speed;
            mv.sprint_speed = baseline.sprint_speed;
        }

        // Air / jetpack
        if is_active_owner {
            match state.air_mode {
                AirMode::None => {
                    jet.force = baseline.jet_force;
                    jet.regen_rate = baseline.jet_regen_rate;
                    jet.max_vertical_vel = baseline.jet_max_vertical_vel;
                }
                AirMode::Jet => {
                    jet.force = baseline.jet_force.max(0.12);
                    jet.regen_rate = baseline.jet_regen_rate.max(80.0);
                    jet.max_vertical_vel = baseline.jet_max_vertical_vel.max(0.7);
                }
                AirMode::Ship => {
                    jet.force = baseline.jet_force.max(0.18);
                    jet.regen_rate = baseline.jet_regen_rate.max(120.0);
                    jet.max_vertical_vel = baseline.jet_max_vertical_vel.max(1.1);
                }
            }
        } else {
            jet.force = baseline.jet_force;
            jet.regen_rate = baseline.jet_regen_rate;
            jet.max_vertical_vel = baseline.jet_max_vertical_vel;
        }

        // Tank / Mech armor bonus applied to current stats (not max, to avoid save drift)
        let desired_armor_bonus = if is_active_owner {
            state.mech_armor_bonus
        } else {
            0.0
        };
        let previous_armor_bonus = state.applied_armor_bonuses[slot_index];
        apply_vehicle_armor_bonus(&mut stats, previous_armor_bonus, desired_armor_bonus);
        state.applied_armor_bonuses[slot_index] = desired_armor_bonus;
    }
}

fn apply_vehicle_armor_bonus(stats: &mut PlayerStats, previous_bonus: f32, desired_bonus: f32) {
    let previous_bonus = previous_bonus.max(0.0);
    let desired_bonus = desired_bonus.max(0.0);
    let desired_cap = stats.max_armor + desired_bonus;

    if desired_bonus > previous_bonus {
        stats.armor = (stats.armor + desired_bonus - previous_bonus).min(desired_cap);
    } else {
        stats.armor = stats.armor.min(desired_cap);
    }
}

#[allow(clippy::type_complexity)]
fn boat_drive_system(
    time: Res<Time>,
    mut state: ResMut<VehicleState>,
    mut queries: ParamSet<(
        Query<(&mut Transform, &BoatVehicle)>,
        Query<(&PlayerIndex, &PlayerInput, &mut BoatPassenger), With<Player>>,
        Query<(&mut Transform, &mut PlayerMovement, &BoatPassenger), With<Player>>,
    )>,
) {
    if !state.boat_active {
        return;
    }
    let (Some(owner), Some(boat_entity)) = (state.active_owner, state.active_boat) else {
        return;
    };

    let mut driver_axis = Vec2::ZERO;
    let riders = queries
        .p1()
        .iter()
        .filter(|(_, _, passenger)| passenger.boat == boat_entity)
        .map(|(index, _, passenger)| (index.0, passenger.is_driver))
        .collect::<Vec<_>>();
    let Some(driver) = replacement_boat_driver(&riders, Some(owner)) else {
        state.deactivate_all_vehicle();
        return;
    };
    state.active_owner = Some(driver);
    for (idx, input, mut passenger) in queries.p1().iter_mut() {
        if passenger.boat != boat_entity {
            continue;
        }
        passenger.is_driver = idx.0 == driver;
        if passenger.is_driver {
            driver_axis = input.move_axis;
        }
    }

    let dt = time.delta_secs();
    let (boat_translation, boat_rotation) = {
        let mut boat_q = queries.p0();
        let Ok((mut boat_transform, boat)) = boat_q.get_mut(boat_entity) else {
            return;
        };

        let mut horizontal = Vec3::new(driver_axis.x, 0.0, driver_axis.y);
        if horizontal.length_squared() > 0.01 {
            horizontal = horizontal.normalize();
            boat_transform.translation += horizontal * boat.speed * dt;
            state.boat_heading = horizontal.x.atan2(horizontal.z);
        }

        let min_z = boat.dock_position.z.min(boat.island_position.z) - boat.dock_radius;
        let max_z = boat.dock_position.z.max(boat.island_position.z) + boat.dock_radius;
        let route_center_x = (boat.dock_position.x + boat.island_position.x) * 0.5;
        boat_transform.translation.x = boat_transform.translation.x.clamp(
            route_center_x - boat.route_half_width,
            route_center_x + boat.route_half_width,
        );
        boat_transform.translation.z = boat_transform.translation.z.clamp(min_z, max_z);

        let bob = (time.elapsed_secs() * 2.8).sin() * 0.10;
        boat_transform.translation.y = boat.dock_position.y + bob;
        boat_transform.rotation = Quat::from_rotation_y(state.boat_heading);

        (boat_transform.translation, boat_transform.rotation)
    };

    for (mut player_transform, mut movement, passenger) in queries.p2().iter_mut() {
        if passenger.boat != boat_entity {
            continue;
        }
        movement.velocity = Vec3::ZERO;
        movement.ground_velocity = Vec3::ZERO;
        movement.is_grounded = true;

        let seat = boat_seat_offset(passenger.seat);
        player_transform.translation = boat_translation + boat_rotation * seat;
        player_transform.rotation = boat_rotation;
    }
}

fn boat_seat_offset(seat: u8) -> Vec3 {
    match seat {
        0 => Vec3::new(0.0, 1.08, 1.35),
        1 => Vec3::new(-1.35, 1.02, -0.55),
        2 => Vec3::new(1.35, 1.02, -0.55),
        _ => Vec3::new(0.0, 1.02, -2.25),
    }
}

fn first_available_boat_seat(occupied: &[u8]) -> Option<u8> {
    (0..4).find(|seat| !occupied.contains(seat))
}

fn replacement_boat_driver(riders: &[(u8, bool)], current_owner: Option<u8>) -> Option<u8> {
    if let Some(owner) = current_owner.filter(|owner| {
        riders
            .iter()
            .any(|(index, is_driver)| index == owner && *is_driver)
    }) {
        return Some(owner);
    }
    riders.iter().map(|(index, _)| *index).min()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::robot_pets::RobotAssembly;

    fn robot_pets_with_assembly(form: RobotAssemblyForm) -> RobotPetCollection {
        RobotPetCollection {
            active_assembly: Some(RobotAssembly {
                form,
                pet_ids: vec!["test-pet".to_string()],
            }),
            ..default()
        }
    }

    #[test]
    fn assembled_ship_takes_priority_over_jet_blueprint() {
        let mut loadout = PlayerLoadout::default();
        loadout.add_blueprint("jet_blueprint");

        assert_eq!(
            assembled_air_mode(
                &robot_pets_with_assembly(RobotAssemblyForm::SpaceShip),
                &loadout
            ),
            AirMode::Ship
        );
        assert_eq!(
            assembled_air_mode(
                &robot_pets_with_assembly(RobotAssemblyForm::MegaShip),
                &loadout
            ),
            AirMode::Ship
        );
    }

    #[test]
    fn jet_blueprint_remains_air_fallback_without_ship_assembly() {
        let mut loadout = PlayerLoadout::default();
        loadout.add_blueprint("jet_blueprint");

        assert_eq!(
            assembled_air_mode(&RobotPetCollection::default(), &loadout),
            AirMode::Jet
        );
    }

    #[test]
    fn jet_bike_assembly_exposes_both_vehicle_profiles() {
        let robots = robot_pets_with_assembly(RobotAssemblyForm::JetBike);
        let loadout = PlayerLoadout::default();
        assert_eq!(
            available_ground_mode(&robots, &loadout),
            Some(GroundMode::Motorcycle)
        );
        assert_eq!(assembled_air_mode(&robots, &loadout), AirMode::Jet);
        assert!(jet_bike_available(&robots, &loadout));
    }

    #[test]
    fn jet_bike_mode_transition_keeps_one_owner_and_one_physics_profile() {
        let mut state = VehicleState::default();
        set_jet_bike_mode(&mut state, 2, Some(JetBikeMode::Ground));
        assert_eq!(state.active_owner, Some(2));
        assert_eq!(state.jet_bike_mode, Some(JetBikeMode::Ground));
        assert_eq!(state.ground_mode, GroundMode::Motorcycle);
        assert_eq!(state.air_mode, AirMode::None);

        set_jet_bike_mode(&mut state, 2, Some(JetBikeMode::Flight));
        assert_eq!(state.jet_bike_mode, Some(JetBikeMode::Flight));
        assert_eq!(state.ground_mode, GroundMode::None);
        assert_eq!(state.air_mode, AirMode::Jet);

        set_jet_bike_mode(&mut state, 2, None);
        assert_eq!(state.active_owner, None);
        assert_eq!(state.jet_bike_mode, None);
        assert_eq!(state.ground_mode, GroundMode::None);
        assert_eq!(state.air_mode, AirMode::None);
    }

    #[test]
    fn mech_armor_bonus_applies_once_and_clamps_when_removed() {
        let mut stats = PlayerStats {
            armor: 60.0,
            max_armor: 100.0,
            ..default()
        };

        apply_vehicle_armor_bonus(&mut stats, 0.0, 40.0);
        assert_eq!(stats.armor, 100.0);

        apply_vehicle_armor_bonus(&mut stats, 40.0, 40.0);
        assert_eq!(stats.armor, 100.0);

        stats.armor = 135.0;
        apply_vehicle_armor_bonus(&mut stats, 40.0, 0.0);
        assert_eq!(stats.armor, 100.0);
    }

    #[test]
    fn late_boat_boarding_uses_first_free_seat_without_driver_takeover() {
        assert_eq!(first_available_boat_seat(&[0, 2]), Some(1));
        assert_eq!(first_available_boat_seat(&[0, 1, 2, 3]), None);
    }

    #[test]
    fn boat_driver_is_stable_then_promotes_lowest_player_index() {
        assert_eq!(
            replacement_boat_driver(&[(0, false), (2, true), (3, false)], Some(2)),
            Some(2)
        );
        assert_eq!(
            replacement_boat_driver(&[(3, false), (1, false)], Some(2)),
            Some(1)
        );
        assert_eq!(replacement_boat_driver(&[], Some(2)), None);
    }

    #[test]
    fn active_party_vehicle_cannot_be_silently_stolen() {
        let state = VehicleState {
            ground_mode: GroundMode::Motorcycle,
            active_owner: Some(1),
            ..default()
        };

        assert!(!vehicle_owned_by_other(&state, 1));
        assert!(vehicle_owned_by_other(&state, 0));
        assert!(vehicle_in_use_message(&state).contains("P2"));
    }
}
