//! Vehicle plugin — motorcycle (M) and jet (J) summon-on-call.
//! Gated by blueprint discoverables.
//! Vehicle buffs apply only to the player who activated the current vehicle.

use bevy::prelude::*;

use crate::components::mods::PlayerLoadout;
use crate::components::player::{JetpackState, Player, PlayerIndex, PlayerInput, PlayerMovement};
use crate::components::world::{BoatPassenger, BoatVehicle};
use crate::events::UiMessageEvent;
use crate::resources::PlaySessionTransition;
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
                    boat_drive_system,
                )
                    .run_if(in_state(AppState::Playing)),
            );
    }
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
    pub motorcycle_active: bool,
    pub jet_active: bool,
    pub boat_active: bool,
    pub active_owner: Option<u8>,
    pub active_boat: Option<Entity>,
    pub boat_heading: f32,
    baselines: [Option<VehicleBaseline>; 4],
}

fn reset_vehicle_state_on_enter(
    transition: Res<PlaySessionTransition>,
    mut state: ResMut<VehicleState>,
) {
    if !transition.resuming_from_pause {
        *state = VehicleState::default();
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
    mut state: ResMut<VehicleState>,
    mut msg_ev: MessageWriter<UiMessageEvent>,
) {
    for (entity, idx, player_transform, pi, passenger) in player_q.iter() {
        if pi.open_map {
            if loadout.has_blueprint("motorcycle_blueprint") {
                let toggled_on = !(state.motorcycle_active && state.active_owner == Some(idx.0));
                state.motorcycle_active = toggled_on;
                state.jet_active = false;
                state.boat_active = false;
                state.active_boat = None;
                remove_boat_passengers(&mut commands, &player_q, None);
                state.active_owner = toggled_on.then_some(idx.0);
                msg_ev.write(UiMessageEvent {
                    text: if toggled_on {
                        format!("P{} Motorcycle: ON", idx.0 + 1)
                    } else {
                        format!("P{} Motorcycle: OFF", idx.0 + 1)
                    },
                    duration: 1.5,
                });
            } else {
                msg_ev.write(UiMessageEvent {
                    text: "Motorcycle blueprint required.".into(),
                    duration: 2.0,
                });
            }
        }

        if pi.enter_vehicle {
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

            if let Some((boat_entity, boat_transform, boat)) = boat_q
                .iter()
                .filter(|(_, boat_transform, boat)| {
                    player_transform
                        .translation
                        .distance(boat_transform.translation)
                        <= boat.embark_radius
                })
                .min_by(|(_, left_t, _), (_, right_t, _)| {
                    player_transform
                        .translation
                        .distance(left_t.translation)
                        .total_cmp(&player_transform.translation.distance(right_t.translation))
                })
            {
                state.boat_active = true;
                state.motorcycle_active = false;
                state.jet_active = false;
                state.active_owner = Some(idx.0);
                state.active_boat = Some(boat_entity);
                state.boat_heading = yaw_from_rotation(boat_transform.rotation);
                let trip_distance = boat.dock_position.distance(boat.island_position);
                board_nearby_players(
                    &mut commands,
                    &player_q,
                    boat_entity,
                    boat_transform,
                    boat,
                    entity,
                );
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

            if loadout.has_blueprint("jet_blueprint") {
                let toggled_on = !(state.jet_active && state.active_owner == Some(idx.0));
                state.jet_active = toggled_on;
                state.motorcycle_active = false;
                state.boat_active = false;
                state.active_boat = None;
                remove_boat_passengers(&mut commands, &player_q, None);
                state.active_owner = toggled_on.then_some(idx.0);
                msg_ev.write(UiMessageEvent {
                    text: if toggled_on {
                        format!("P{} Jet: ON", idx.0 + 1)
                    } else {
                        format!("P{} Jet: OFF", idx.0 + 1)
                    },
                    duration: 1.5,
                });
            } else {
                msg_ev.write(UiMessageEvent {
                    text: "Jet blueprint required.".into(),
                    duration: 2.0,
                });
            }
        }
    }
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
) {
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
    for (seat, (entity, _, is_driver)) in riders.into_iter().take(4).enumerate() {
        commands.entity(entity).insert(BoatPassenger {
            boat: boat_entity,
            seat: seat as u8,
            is_driver,
        });
    }
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

/// Apply vehicle speed/force buffs only to the player who activated the vehicle.
fn apply_vehicle_buffs(
    mut state: ResMut<VehicleState>,
    mut q: Query<(&PlayerIndex, &mut PlayerMovement, &mut JetpackState), With<Player>>,
) {
    for (idx, mut mv, mut jet) in q.iter_mut() {
        let Some(slot) = state.baselines.get_mut(idx.0 as usize) else {
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
        if state.boat_active && is_active_owner {
            mv.walk_speed = baseline.walk_speed.max(0.58);
            mv.sprint_speed = baseline.sprint_speed.max(0.82);
        } else if state.motorcycle_active && is_active_owner {
            mv.walk_speed = baseline.walk_speed.max(0.7);
            mv.sprint_speed = baseline.sprint_speed.max(1.1);
        } else {
            mv.walk_speed = baseline.walk_speed;
            mv.sprint_speed = baseline.sprint_speed;
        }
        if state.jet_active && is_active_owner {
            jet.force = baseline.jet_force.max(0.12);
            jet.regen_rate = baseline.jet_regen_rate.max(80.0);
            jet.max_vertical_vel = baseline.jet_max_vertical_vel.max(0.7);
        } else {
            jet.force = baseline.jet_force;
            jet.regen_rate = baseline.jet_regen_rate;
            jet.max_vertical_vel = baseline.jet_max_vertical_vel;
        }
    }
}

#[allow(clippy::type_complexity)]
fn boat_drive_system(
    time: Res<Time>,
    mut state: ResMut<VehicleState>,
    mut queries: ParamSet<(
        Query<(&mut Transform, &BoatVehicle)>,
        Query<(&PlayerIndex, &PlayerInput, &BoatPassenger), With<Player>>,
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
    for (idx, input, passenger) in queries.p1().iter() {
        if passenger.boat == boat_entity && passenger.is_driver && idx.0 == owner {
            driver_axis = input.move_axis;
            break;
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
