//! Vehicle plugin — motorcycle (M) and jet (J) summon-on-call.
//! Gated by blueprint discoverables.
//! Vehicle buffs apply only to the player who activated the current vehicle.

use bevy::prelude::*;

use crate::components::mods::PlayerLoadout;
use crate::components::player::{JetpackState, Player, PlayerIndex, PlayerInput, PlayerMovement};
use crate::events::UiMessageEvent;
use crate::state::AppState;

pub struct VehiclePlugin;

impl Plugin for VehiclePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VehicleState>().add_systems(
            Update,
            (vehicle_input, apply_vehicle_buffs).run_if(in_state(AppState::Playing)),
        );
    }
}

#[derive(Resource, Debug, Default)]
pub struct VehicleState {
    pub motorcycle_active: bool,
    pub jet_active: bool,
    pub active_owner: Option<u8>,
}

fn vehicle_input(
    player_q: Query<(&PlayerIndex, &PlayerInput), With<Player>>,
    loadout: Res<PlayerLoadout>,
    mut state: ResMut<VehicleState>,
    mut msg_ev: EventWriter<UiMessageEvent>,
) {
    for (idx, pi) in player_q.iter() {
        if pi.open_map {
            if loadout.has_blueprint("motorcycle_blueprint") {
                let toggled_on = !(state.motorcycle_active && state.active_owner == Some(idx.0));
                state.motorcycle_active = toggled_on;
                state.jet_active = false;
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
            if loadout.has_blueprint("jet_blueprint") {
                let toggled_on = !(state.jet_active && state.active_owner == Some(idx.0));
                state.jet_active = toggled_on;
                state.motorcycle_active = false;
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
        if state.motorcycle_active && is_active_owner {
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
