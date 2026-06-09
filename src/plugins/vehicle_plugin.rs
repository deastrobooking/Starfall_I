//! Vehicle plugin — ground and air vehicle modes, gated by blueprints and robot assembly.
//!
//! Blueprint vehicles (discoverable pickups):
//!   M / open_map  → motorcycle (walk_speed × 1.58, sprint × 1.58)
//!   J / enter_vehicle → jet (boosted jetpack)
//!
//! Assembly vehicles (Robot Garage assembles a form from pets + parts):
//!   M / open_map  → tank mode  (assembled Tank)     — slow but heavy armor bonus
//!   M / open_map  → mech mode  (assembled GiantMech) — very slow, massive melee power
//!   J / enter_vehicle → ship mode (SpaceShip/MegaShip)  — sustained flight upgrade
//!
//! Only one ground mode and one air mode may be active at a time.

use bevy::prelude::*;

use crate::components::mods::PlayerLoadout;
use crate::components::player::{
    JetpackState, Player, PlayerIndex, PlayerInput, PlayerMovement, PlayerStats,
};
use crate::components::world::{BoatPassenger, BoatVehicle};
use crate::events::UiMessageEvent;
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
    baselines: [Option<VehicleBaseline>; 4],
    applied_armor_bonuses: [f32; 4],
    // Legacy accessors used by external code — kept as computed properties.
    pub mech_armor_bonus: f32,
}

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
        self.mech_armor_bonus = 0.0;
    }
    fn deactivate_air(&mut self) {
        self.air_mode = AirMode::None;
    }
    fn deactivate_all_vehicle(&mut self) {
        self.deactivate_ground();
        self.deactivate_air();
        self.boat_active = false;
        self.active_boat = None;
        self.active_owner = None;
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
            RobotAssemblyForm::Car | RobotAssemblyForm::Motorcycle => Some(GroundMode::Motorcycle),
            RobotAssemblyForm::Tank => Some(GroundMode::Tank),
            RobotAssemblyForm::GiantMech => Some(GroundMode::GiantMech),
            _ => None,
        })
}

fn assembled_air_mode(robot_pets: &RobotPetCollection, loadout: &PlayerLoadout) -> AirMode {
    if let Some(a) = &robot_pets.active_assembly {
        if matches!(a.form, RobotAssemblyForm::SpaceJet) {
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
    for (entity, idx, player_transform, pi, passenger) in player_q.iter() {
        // ── Ground vehicle toggle (M key / open_map) ──────────────────────────
        if pi.open_map {
            // Determine which ground mode is available (assembly takes priority over blueprint).
            let available_ground = if let Some(mode) = assembled_ground_mode(&robot_pets) {
                Some(mode)
            } else if loadout.has_blueprint("motorcycle_blueprint") {
                Some(GroundMode::Motorcycle)
            } else {
                None
            };

            if let Some(mode) = available_ground {
                let currently_on = state.ground_mode == mode && state.active_owner == Some(idx.0);
                if currently_on {
                    state.deactivate_ground();
                    state.active_owner = None;
                    msg_ev.write(UiMessageEvent {
                        text: format!("P{} {}: OFF", idx.0 + 1, ground_mode_label(mode)),
                        duration: 1.5,
                    });
                } else {
                    state.ground_mode = mode;
                    state.mech_armor_bonus = if mode == GroundMode::GiantMech {
                        40.0
                    } else {
                        0.0
                    };
                    state.air_mode = AirMode::None;
                    state.boat_active = false;
                    state.active_boat = None;
                    remove_boat_passengers(&mut commands, &player_q, None);
                    state.active_owner = Some(idx.0);
                    msg_ev.write(UiMessageEvent {
                        text: format!("P{} {}: ON", idx.0 + 1, ground_mode_label(mode)),
                        duration: 1.5,
                    });
                }
            } else {
                msg_ev.write(UiMessageEvent {
                    text: "No ground vehicle — assemble one in the Garage or find a blueprint."
                        .into(),
                    duration: 2.0,
                });
            }
        }

        // ── Air/boat toggle (J key / enter_vehicle) ───────────────────────────
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

            // Air mode toggle.
            let available_air = assembled_air_mode(&robot_pets, &loadout);
            if available_air != AirMode::None {
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
            } else {
                msg_ev.write(UiMessageEvent {
                    text:
                        "No air vehicle — assemble SpaceJet in the Garage or find a jet blueprint."
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
}
