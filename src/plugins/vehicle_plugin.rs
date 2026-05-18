//! Vehicle plugin — motorcycle (M) and jet (J) summon-on-call.
//! Gated by blueprint discoverables.
//! Vehicle buffs apply only to the player who activated the current vehicle.

use bevy::prelude::*;

use crate::components::mods::PlayerLoadout;
use crate::components::player::{JetpackState, Player, PlayerIndex, PlayerInput, PlayerMovement};
use crate::components::world::BoatVehicle;
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
                (vehicle_input, apply_vehicle_buffs, boat_follow_system)
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

#[derive(Resource, Debug, Default)]
pub struct VehicleState {
    pub motorcycle_active: bool,
    pub jet_active: bool,
    pub boat_active: bool,
    pub active_owner: Option<u8>,
    pub active_boat: Option<Entity>,
}

fn reset_vehicle_state_on_enter(
    transition: Res<PlaySessionTransition>,
    mut state: ResMut<VehicleState>,
) {
    if !transition.resuming_from_pause {
        *state = VehicleState::default();
    }
}

fn vehicle_input(
    player_q: Query<(&PlayerIndex, &Transform, &PlayerInput), With<Player>>,
    boat_q: Query<(Entity, &Transform, &BoatVehicle)>,
    loadout: Res<PlayerLoadout>,
    mut state: ResMut<VehicleState>,
    mut msg_ev: EventWriter<UiMessageEvent>,
) {
    for (idx, player_transform, pi) in player_q.iter() {
        if pi.open_map {
            if loadout.has_blueprint("motorcycle_blueprint") {
                let toggled_on = !(state.motorcycle_active && state.active_owner == Some(idx.0));
                state.motorcycle_active = toggled_on;
                state.jet_active = false;
                state.boat_active = false;
                state.active_boat = None;
                state.active_owner = toggled_on.then_some(idx.0);
                msg_ev.send(UiMessageEvent {
                    text: if toggled_on {
                        format!("P{} Motorcycle: ON", idx.0 + 1)
                    } else {
                        format!("P{} Motorcycle: OFF", idx.0 + 1)
                    },
                    duration: 1.5,
                });
            } else {
                msg_ev.send(UiMessageEvent {
                    text: "Motorcycle blueprint required.".into(),
                    duration: 2.0,
                });
            }
        }

        if pi.enter_vehicle {
            if state.boat_active && state.active_owner == Some(idx.0) {
                state.boat_active = false;
                state.active_boat = None;
                state.active_owner = None;
                msg_ev.send(UiMessageEvent {
                    text: format!("P{} Boat: docked", idx.0 + 1),
                    duration: 1.8,
                });
                continue;
            }

            if let Some((boat_entity, _, boat)) = boat_q
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
                let trip_distance = boat.dock_position.distance(boat.island_position);
                msg_ev.send(UiMessageEvent {
                    text: format!(
                        "P{} Boat: ON - follow the wake toward the island ({:.0}m)",
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
                state.active_owner = toggled_on.then_some(idx.0);
                msg_ev.send(UiMessageEvent {
                    text: if toggled_on {
                        format!("P{} Jet: ON", idx.0 + 1)
                    } else {
                        format!("P{} Jet: OFF", idx.0 + 1)
                    },
                    duration: 1.5,
                });
            } else {
                msg_ev.send(UiMessageEvent {
                    text: "Jet blueprint required.".into(),
                    duration: 2.0,
                });
            }
        }
    }
}

/// Apply vehicle speed/force buffs only to the player who activated the vehicle.
fn apply_vehicle_buffs(
    state: Res<VehicleState>,
    mut q: Query<(&PlayerIndex, &mut PlayerMovement, &mut JetpackState), With<Player>>,
) {
    let base = PlayerMovement::default();
    for (idx, mut mv, mut jet) in q.iter_mut() {
        let is_active_owner = state.active_owner == Some(idx.0);
        if state.boat_active && is_active_owner {
            mv.walk_speed = 0.58;
            mv.sprint_speed = 0.82;
        } else if state.motorcycle_active && is_active_owner {
            mv.walk_speed = 0.7;
            mv.sprint_speed = 1.1;
        } else {
            mv.walk_speed = base.walk_speed;
            mv.sprint_speed = base.sprint_speed;
        }
        if state.jet_active && is_active_owner {
            jet.force = 0.12;
            jet.regen_rate = 80.0;
            jet.max_vertical_vel = 0.7;
        } else {
            jet.force = 0.06;
            jet.regen_rate = 30.0;
            jet.max_vertical_vel = 0.35;
        }
    }
}

fn boat_follow_system(
    time: Res<Time>,
    state: Res<VehicleState>,
    player_q: Query<(&PlayerIndex, &Transform), With<Player>>,
    mut boat_q: Query<&mut Transform, With<BoatVehicle>>,
) {
    if !state.boat_active {
        return;
    }
    let (Some(owner), Some(boat_entity)) = (state.active_owner, state.active_boat) else {
        return;
    };
    let Some((_, player_transform)) = player_q.iter().find(|(idx, _)| idx.0 == owner) else {
        return;
    };
    let Ok(mut boat_transform) = boat_q.get_mut(boat_entity) else {
        return;
    };

    let bob = (time.elapsed_secs() * 2.8).sin() * 0.10;
    boat_transform.translation = Vec3::new(
        player_transform.translation.x,
        0.42 + bob,
        player_transform.translation.z,
    );
    boat_transform.rotation = player_transform.rotation;
}
