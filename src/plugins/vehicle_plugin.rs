//! Vehicle plugin — motorcycle (M) and jet (J) summon-on-call.
//! Gated by blueprint discoverables.
//! Vehicles are driven by P1's input; all players receive the speed/force buffs.

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
}

fn vehicle_input(
    player_q: Query<(&PlayerIndex, &PlayerInput), With<Player>>,
    loadout: Res<PlayerLoadout>,
    mut state: ResMut<VehicleState>,
    mut msg_ev: EventWriter<UiMessageEvent>,
) {
    // Vehicles are controlled by P1.
    let Some((_, pi)) = player_q.iter().find(|(idx, _)| idx.0 == 0) else {
        return;
    };

    if pi.open_map {
        if loadout.has_blueprint("motorcycle_blueprint") {
            state.motorcycle_active = !state.motorcycle_active;
            if state.motorcycle_active {
                state.jet_active = false;
            }
            msg_ev.send(UiMessageEvent {
                text: if state.motorcycle_active { "Motorcycle: ON" } else { "Motorcycle: OFF" }
                    .into(),
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
            state.jet_active = !state.jet_active;
            if state.jet_active {
                state.motorcycle_active = false;
            }
            msg_ev.send(UiMessageEvent {
                text: if state.jet_active { "Jet: ON" } else { "Jet: OFF" }.into(),
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

/// Apply vehicle speed/force buffs to all players while a vehicle is active.
fn apply_vehicle_buffs(
    state: Res<VehicleState>,
    mut q: Query<(&mut PlayerMovement, &mut JetpackState), With<Player>>,
) {
    for (mut mv, mut jet) in q.iter_mut() {
        if state.motorcycle_active {
            mv.walk_speed = 0.7;
            mv.sprint_speed = 1.1;
        } else {
            mv.walk_speed = 0.3;
            mv.sprint_speed = 0.55;
        }
        if state.jet_active {
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
