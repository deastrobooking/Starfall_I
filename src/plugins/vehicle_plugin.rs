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

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use crate::combat::damage::{DamageType, Damageable};
use crate::components::mods::PlayerLoadout;
use crate::components::player::{
    AimSolution, BoardBoostState, JetpackState, Player, PlayerIndex, PlayerInput, PlayerMovement,
    PlayerStats, TraversalMode, TraversalModeState,
};
use crate::components::weapon::{Projectile, ProjectileOwner};
use crate::components::world::{BoatPassenger, BoatVehicle};
use crate::engine::game_loop::GameSet;
use crate::engine::rendering::{player_render_layer, PbrBundle, SpatialBundle};
use crate::engine::state::AppState;
use crate::events::{UiMessageEvent, WeaponFiredEvent};
use crate::plugins::weapon_plugin::ProjectileAssets;
use crate::resources::PlaySessionTransition;
use crate::world::robot_pets::{RobotAssemblyForm, RobotPetCollection};

pub struct VehiclePlugin;

/// Ordering boundary for systems that change authoritative party-vehicle
/// ownership. World traversal affordances read vehicle state after this set.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VehicleSet {
    State,
}

impl Plugin for VehiclePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VehicleState>()
            .add_systems(Startup, setup_heavy_water_vehicle_visual_assets)
            .add_systems(OnEnter(AppState::Playing), reset_vehicle_state_on_enter)
            .add_systems(OnExit(AppState::Playing), cleanup_vehicle_visuals_on_exit)
            .add_systems(OnEnter(AppState::MainMenu), cleanup_vehicle_visuals)
            .add_systems(
                Update,
                (
                    vehicle_input,
                    sync_vehicle_traversal,
                    update_vehicle_turbo,
                    clear_stale_boat_passengers_system,
                    apply_vehicle_buffs,
                    boat_drive_system,
                )
                    .chain()
                    .in_set(VehicleSet::State)
                    // Entering a tank must take ownership of primary fire
                    // before handheld weapons consume the same input edge;
                    // legacy Update motors also see this frame's vehicle
                    // stats rather than inheriting them one frame later.
                    .before(GameSet::Motor)
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(
                Update,
                (
                    sync_jet_bike_visual,
                    sync_heavy_water_vehicle_visual,
                    bind_vehicle_visual_render_layers,
                    tank_cannon_system,
                    tank_muzzle_flash_system,
                )
                    .chain()
                    // Combat refreshes AimSolution first; articulated tank aim
                    // and cannon fire therefore use this frame's reticle. Run
                    // after pose work and before cameras consume presentation.
                    .after(GameSet::Animation)
                    .before(GameSet::Camera)
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

#[derive(Debug, Clone, Copy, Default)]
struct VehicleArmorBuffer {
    capacity: f32,
    remaining: f32,
    last_total: Option<f32>,
}

impl VehicleArmorBuffer {
    fn remaining_at_total(self, armor_total: f32) -> f32 {
        let unobserved_loss = self
            .last_total
            .map_or(0.0, |last_total| (last_total - armor_total).max(0.0));
        (self.remaining - unobserved_loss).max(0.0)
    }
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
    armor_buffers: [VehicleArmorBuffer; 4],
    /// The loadout-selected traversal tool is suspended while a vehicle owns
    /// this player, then restored exactly on exit. This keeps hoverboard and
    /// vehicle motors/visuals from stacking without changing durable loadout.
    suspended_traversals: [Option<TraversalMode>; 4],
    /// Remaining Heavy Water-style turbo burst and recharge time. Turbo is
    /// shared by ground and flight profiles so switching forms never stacks it.
    pub turbo_timer: f32,
    pub turbo_cooldown: f32,
    /// The assembled tank owns primary fire while active. Keeping the timer on
    /// the party vehicle state makes ownership transfer deterministic.
    pub tank_cannon_cooldown: f32,
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

/// Procedural successor bodies for Heavy Water's ATV, fighter, tank, and the
/// Starfall-only giant forms. These are gameplay silhouettes rather than loose
/// props: the active profile follows the authoritative vehicle owner and tank
/// weapon systems use the same aim solution as the HUD reticle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeavyWaterVehicleProfile {
    Atv,
    Tank,
    GiantMech,
    SpaceFighter,
    Starship,
}

impl HeavyWaterVehicleProfile {
    fn label(self) -> &'static str {
        match self {
            Self::Atv => "All-Terrain Vehicle",
            Self::Tank => "Siege Tank",
            Self::GiantMech => "Giant Mech",
            Self::SpaceFighter => "Space Fighter",
            Self::Starship => "Starship",
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
struct HeavyWaterVehicleVisual {
    owner: u8,
    profile: HeavyWaterVehicleProfile,
}

#[derive(Component, Debug, Clone, Copy)]
struct TankTurretVisual {
    owner: u8,
}

#[derive(Component, Debug, Clone, Copy)]
struct TankBarrelVisual {
    owner: u8,
}

#[derive(Component, Debug, Clone, Copy)]
struct TankMuzzleFlash {
    remaining: f32,
}

struct AtvVisualMeshes {
    body: Handle<Mesh>,
    cowling: Handle<Mesh>,
    seat: Handle<Mesh>,
    wheel: Handle<Mesh>,
    strut: Handle<Mesh>,
    lamp: Handle<Mesh>,
    handlebar: Handle<Mesh>,
}

struct TankVisualMeshes {
    tread: Handle<Mesh>,
    tread_glow: Handle<Mesh>,
    hull: Handle<Mesh>,
    glacis: Handle<Mesh>,
    turret_ring: Handle<Mesh>,
    turret: Handle<Mesh>,
    sensor: Handle<Mesh>,
    barrel: Handle<Mesh>,
}

struct MechVisualMeshes {
    torso: Handle<Mesh>,
    cockpit: Handle<Mesh>,
    leg: Handle<Mesh>,
    arm: Handle<Mesh>,
    accent: Handle<Mesh>,
}

struct FlightVisualMeshes {
    fuselage: Handle<Mesh>,
    wing: Handle<Mesh>,
    canopy: Handle<Mesh>,
    cannon: Handle<Mesh>,
    thruster: Handle<Mesh>,
    tail: Handle<Mesh>,
}

/// Strong handles for the finite Heavy Water vehicle kit. Visual entities may
/// be rebuilt as modes change, but their CPU/GPU assets are authored only once.
#[derive(Resource)]
struct HeavyWaterVehicleVisualAssets {
    armor_material: Handle<StandardMaterial>,
    secondary_material: Handle<StandardMaterial>,
    dark_material: Handle<StandardMaterial>,
    glow_material: Handle<StandardMaterial>,
    glass_material: Handle<StandardMaterial>,
    atv: AtvVisualMeshes,
    tank: TankVisualMeshes,
    mech: MechVisualMeshes,
    fighter: FlightVisualMeshes,
    starship: FlightVisualMeshes,
}

fn flight_visual_meshes(
    meshes: &mut Assets<Mesh>,
    canopy: &Handle<Mesh>,
    scale: f32,
) -> FlightVisualMeshes {
    FlightVisualMeshes {
        fuselage: meshes.add(Cuboid::new(2.35 * scale, 0.84 * scale, 6.2 * scale)),
        wing: meshes.add(Cuboid::new(8.4 * scale, 0.18 * scale, 2.4 * scale)),
        canopy: canopy.clone(),
        cannon: meshes.add(Cylinder::new(0.18 * scale, 2.8 * scale)),
        thruster: meshes.add(Cylinder::new(0.44 * scale, 1.25 * scale)),
        // Preserve the original thin shared fin profile: only Y/Z scaled up.
        tail: meshes.add(Cuboid::new(0.24, 2.2 * scale, 2.4 * scale)),
    }
}

fn setup_heavy_water_vehicle_visual_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let armor_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.16, 0.22, 0.30),
        metallic: 0.78,
        perceptual_roughness: 0.27,
        ..default()
    });
    let secondary_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.62, 0.08, 0.34),
        metallic: 0.68,
        perceptual_roughness: 0.24,
        ..default()
    });
    let dark_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.018, 0.026, 0.045),
        metallic: 0.38,
        perceptual_roughness: 0.58,
        ..default()
    });
    let glow_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.05, 0.84, 1.0),
        emissive: LinearRgba::new(0.10, 2.8, 4.4, 1.0),
        metallic: 0.32,
        perceptual_roughness: 0.16,
        ..default()
    });
    let glass_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.08, 0.55, 0.82, 0.70),
        emissive: LinearRgba::new(0.04, 0.55, 0.9, 1.0),
        metallic: 0.28,
        perceptual_roughness: 0.12,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    let canopy = meshes.add(Sphere::new(1.0));
    let fighter = flight_visual_meshes(&mut meshes, &canopy, 1.0);
    let starship = flight_visual_meshes(&mut meshes, &canopy, 1.55);
    commands.insert_resource(HeavyWaterVehicleVisualAssets {
        armor_material,
        secondary_material,
        dark_material,
        glow_material,
        glass_material,
        atv: AtvVisualMeshes {
            body: meshes.add(Cuboid::new(2.7, 0.62, 3.8)),
            cowling: meshes.add(Cuboid::new(2.25, 0.48, 1.35)),
            seat: meshes.add(Cuboid::new(1.25, 0.36, 1.25)),
            wheel: meshes.add(Torus {
                major_radius: 0.58,
                minor_radius: 0.20,
            }),
            strut: meshes.add(Cuboid::new(0.16, 1.42, 0.16)),
            lamp: meshes.add(Sphere::new(0.22)),
            handlebar: meshes.add(Cuboid::new(2.0, 0.16, 0.16)),
        },
        tank: TankVisualMeshes {
            tread: meshes.add(Cuboid::new(0.92, 0.82, 5.55)),
            tread_glow: meshes.add(Cuboid::new(0.10, 0.18, 4.65)),
            hull: meshes.add(Cuboid::new(3.35, 1.18, 4.62)),
            glacis: meshes.add(Cuboid::new(3.0, 0.58, 1.35)),
            turret_ring: meshes.add(Cylinder::new(1.26, 0.40)),
            turret: meshes.add(Cuboid::new(2.42, 1.0, 2.62)),
            sensor: meshes.add(Cuboid::new(0.92, 0.18, 0.10)),
            barrel: meshes.add(Cylinder::new(0.22, 3.8)),
        },
        mech: MechVisualMeshes {
            torso: meshes.add(Cuboid::new(3.8, 4.2, 2.5)),
            cockpit: meshes.add(Cuboid::new(2.1, 1.8, 1.9)),
            leg: meshes.add(Cuboid::new(1.28, 4.3, 1.55)),
            arm: meshes.add(Cuboid::new(1.05, 4.1, 1.15)),
            accent: meshes.add(Cuboid::new(3.1, 0.28, 0.22)),
        },
        fighter,
        starship,
    });
}

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
    pub fn player_owns_tank(&self, player: u8) -> bool {
        self.active_owner == Some(player) && self.tank_active()
    }
    pub fn player_owns_active_vehicle(&self, player: u8) -> bool {
        self.active_owner == Some(player)
            && (self.ground_mode != GroundMode::None
                || self.air_mode != AirMode::None
                || self.boat_active)
    }
    pub fn player_owns_ground_vehicle(&self, player: u8) -> bool {
        self.active_owner == Some(player) && self.ground_mode != GroundMode::None
    }
    pub fn player_owns_air_vehicle(&self, player: u8) -> bool {
        self.active_owner == Some(player) && self.air_mode != AirMode::None
    }
    /// Traversal written to a durable save remains the selected loadout tool,
    /// not the neutral mode temporarily installed by an active vehicle.
    pub fn persistent_traversal(
        &self,
        player: u8,
        runtime_traversal: TraversalMode,
    ) -> TraversalMode {
        self.suspended_traversals
            .get(player as usize)
            .and_then(|mode| *mode)
            .unwrap_or(runtime_traversal)
    }
    /// Changes the selection that will be restored when the vehicle releases
    /// locomotion, while keeping the live component in its neutral sentinel.
    pub fn select_traversal_while_active(&mut self, player: u8, selected: TraversalMode) {
        if let Some(slot) = self.suspended_traversals.get_mut(player as usize) {
            *slot = Some(selected);
        }
    }
    pub fn turbo_active(&self) -> bool {
        self.turbo_timer > 0.0
    }
    /// Armor written to durable saves excludes the unspent temporary vehicle
    /// buffer, including damage received after the most recent reconciliation.
    pub fn persistent_armor(&self, player: u8, runtime_armor: f32) -> f32 {
        let remaining = self
            .armor_buffers
            .get(player as usize)
            .map_or(0.0, |buffer| buffer.remaining_at_total(runtime_armor));
        (runtime_armor - remaining).max(0.0)
    }
    fn deactivate_ground(&mut self) {
        self.ground_mode = GroundMode::None;
        self.jet_bike_mode = None;
        self.mech_armor_bonus = 0.0;
        self.turbo_timer = 0.0;
        self.tank_cannon_cooldown = 0.0;
    }
    fn deactivate_air(&mut self) {
        self.air_mode = AirMode::None;
        self.jet_bike_mode = None;
        self.turbo_timer = 0.0;
    }
    fn deactivate_all_vehicle(&mut self) {
        self.deactivate_ground();
        self.deactivate_air();
        self.boat_active = false;
        self.active_boat = None;
        self.active_owner = None;
        self.jet_bike_mode = None;
        self.turbo_timer = 0.0;
        self.turbo_cooldown = 0.0;
        self.tank_cannon_cooldown = 0.0;
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

fn cleanup_vehicle_visuals(
    mut commands: Commands,
    visual_q: Query<
        Entity,
        Or<(
            With<JetBikeVisual>,
            With<HeavyWaterVehicleVisual>,
            With<TankMuzzleFlash>,
        )>,
    >,
) {
    for entity in visual_q.iter() {
        commands.entity(entity).despawn();
    }
}

fn cleanup_vehicle_visuals_on_exit(
    mut commands: Commands,
    transition: Res<PlaySessionTransition>,
    visual_q: Query<
        Entity,
        Or<(
            With<JetBikeVisual>,
            With<HeavyWaterVehicleVisual>,
            With<TankMuzzleFlash>,
        )>,
    >,
) {
    if transition.pausing {
        return;
    }
    for entity in visual_q.iter() {
        commands.entity(entity).despawn();
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VehicleEntryProfile {
    None,
    Ground(GroundMode),
    Air(AirMode),
}

/// Selects the physical form represented by the current robot assembly before
/// falling back to loose blueprints.  Otherwise a late jet-blueprint unlock
/// shadows an assembled Tank/Giant Mech forever because both forms share the
/// same enter-vehicle input.
fn preferred_vehicle_entry_profile(
    robot_pets: &RobotPetCollection,
    loadout: &PlayerLoadout,
) -> VehicleEntryProfile {
    if let Some(mode) = assembled_ground_mode(robot_pets) {
        return VehicleEntryProfile::Ground(mode);
    }
    let air = assembled_air_mode(robot_pets, loadout);
    if air != AirMode::None {
        VehicleEntryProfile::Air(air)
    } else if let Some(mode) = available_ground_mode(robot_pets, loadout) {
        VehicleEntryProfile::Ground(mode)
    } else {
        VehicleEntryProfile::None
    }
}

fn jet_bike_available(robot_pets: &RobotPetCollection, loadout: &PlayerLoadout) -> bool {
    robot_pets
        .active_assembly
        .as_ref()
        .is_some_and(|assembly| assembly.form == RobotAssemblyForm::JetBike)
        || (robot_pets.active_assembly.is_none()
            && available_ground_mode(robot_pets, loadout) == Some(GroundMode::Motorcycle)
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
    state.tank_cannon_cooldown = 0.0;
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

fn reconcile_vehicle_traversal(
    vehicle_active: bool,
    suspended: &mut Option<TraversalMode>,
    traversal: &mut TraversalModeState,
    board_boost: &mut BoardBoostState,
) {
    if vehicle_active {
        if suspended.is_none() {
            *suspended = Some(traversal.active);
            // Drop only stale/manual board momentum on the entry edge. Road
            // pads may subsequently boost a vehicle and that sustained timer
            // must survive later reconciliation frames.
            *board_boost = BoardBoostState::default();
        }
        traversal.active = TraversalMode::Vehicle;
    } else if let Some(selected) = suspended.take() {
        traversal.active = selected;
    }
}

/// Gives the active vehicle exclusive ownership of locomotion without
/// permanently changing the player's selected traversal tool. Boat passengers
/// are included even though only the driver owns the shared VehicleState.
fn sync_vehicle_traversal(
    mut state: ResMut<VehicleState>,
    mut player_q: Query<
        (
            &PlayerIndex,
            &mut TraversalModeState,
            &mut BoardBoostState,
            Option<&BoatPassenger>,
        ),
        With<Player>,
    >,
) {
    for (index, mut traversal, mut board_boost, passenger) in player_q.iter_mut() {
        let slot = index.0 as usize;
        if slot >= state.suspended_traversals.len() {
            continue;
        }
        let vehicle_active = state.player_owns_active_vehicle(index.0) || passenger.is_some();
        reconcile_vehicle_traversal(
            vehicle_active,
            &mut state.suspended_traversals[slot],
            &mut traversal,
            &mut board_boost,
        );
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
        let dual_jet_bike = jet_bike_available(&robot_pets, &loadout);
        let preferred_entry = preferred_vehicle_entry_profile(&robot_pets, &loadout);

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
                if vehicle_owned_by_other(&state, idx.0) {
                    msg_ev.write(UiMessageEvent {
                        text: vehicle_in_use_message(&state),
                        duration: 1.8,
                    });
                    continue;
                }
                state.boat_active = true;
                state.ground_mode = GroundMode::None;
                state.air_mode = AirMode::None;
                state.jet_bike_mode = None;
                state.mech_armor_bonus = 0.0;
                state.turbo_timer = 0.0;
                state.turbo_cooldown = 0.0;
                state.tank_cannon_cooldown = 0.0;
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
            // shared enter-vehicle input. Other active robot assemblies keep
            // their authored profile even when fallback blueprints are owned.
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
            } else if let VehicleEntryProfile::Air(mode) = preferred_entry {
                if vehicle_owned_by_other(&state, idx.0) {
                    msg_ev.write(UiMessageEvent {
                        text: vehicle_in_use_message(&state),
                        duration: 1.8,
                    });
                    continue;
                }
                let currently_on = state.air_mode == mode && state.active_owner == Some(idx.0);
                if currently_on {
                    state.deactivate_air();
                    state.active_owner = None;
                    msg_ev.write(UiMessageEvent {
                        text: format!("P{} {}: OFF", idx.0 + 1, air_mode_label(mode)),
                        duration: 1.5,
                    });
                } else {
                    state.air_mode = mode;
                    state.jet_bike_mode = None;
                    state.ground_mode = GroundMode::None;
                    state.mech_armor_bonus = 0.0;
                    state.turbo_timer = 0.0;
                    state.turbo_cooldown = 0.0;
                    state.boat_active = false;
                    state.active_boat = None;
                    remove_boat_passengers(&mut commands, &player_q, None);
                    state.active_owner = Some(idx.0);
                    msg_ev.write(UiMessageEvent {
                        text: format!("P{} {}: ON", idx.0 + 1, air_mode_label(mode)),
                        duration: 1.5,
                    });
                }
            } else if let VehicleEntryProfile::Ground(mode) = preferred_entry {
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
                    state.mech_armor_bonus = match mode {
                        GroundMode::Tank => 25.0,
                        GroundMode::GiantMech => 40.0,
                        GroundMode::None | GroundMode::Motorcycle => 0.0,
                    };
                    state.turbo_timer = 0.0;
                    state.turbo_cooldown = 0.0;
                    state.tank_cannon_cooldown = 0.0;
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

/// Heavy Water used the same short turbo verb for both ATVs and fighters.
/// Starfall keeps that muscle memory: holding sprint starts a 0.7-second burst
/// when the shared 1.6-second post-burst recharge is ready.
fn update_vehicle_turbo(
    time: Res<Time>,
    mut state: ResMut<VehicleState>,
    player_q: Query<(&PlayerIndex, &PlayerInput), With<Player>>,
) {
    let dt = time.delta_secs();
    state.turbo_timer = (state.turbo_timer - dt).max(0.0);
    state.turbo_cooldown = (state.turbo_cooldown - dt).max(0.0);
    state.tank_cannon_cooldown = (state.tank_cannon_cooldown - dt).max(0.0);

    let vehicle_can_turbo = !state.boat_active
        && (state.ground_mode != GroundMode::None || state.air_mode != AirMode::None);
    if !vehicle_can_turbo || state.turbo_cooldown > 0.0 {
        return;
    }
    let Some(owner) = state.active_owner else {
        return;
    };
    let turbo_requested = player_q
        .iter()
        .any(|(index, input)| index.0 == owner && input.sprint);
    if turbo_requested {
        const TURBO_DURATION: f32 = 0.7;
        const POST_BURST_RECHARGE: f32 = 1.6;
        state.turbo_timer = TURBO_DURATION;
        state.turbo_cooldown = TURBO_DURATION + POST_BURST_RECHARGE;
    }
}

fn active_heavy_water_profile(state: &VehicleState) -> Option<HeavyWaterVehicleProfile> {
    if state.boat_active || state.jet_bike_mode.is_some() {
        return None;
    }
    match state.ground_mode {
        GroundMode::Motorcycle => Some(HeavyWaterVehicleProfile::Atv),
        GroundMode::Tank => Some(HeavyWaterVehicleProfile::Tank),
        GroundMode::GiantMech => Some(HeavyWaterVehicleProfile::GiantMech),
        GroundMode::None => match state.air_mode {
            AirMode::Jet => Some(HeavyWaterVehicleProfile::SpaceFighter),
            AirMode::Ship => Some(HeavyWaterVehicleProfile::Starship),
            AirMode::None => None,
        },
    }
}

/// Vehicle bodies live beside the player hierarchy so their transforms can be
/// replaced independently when a mode changes. Bind every renderable child to
/// the driver's avatar layer anyway: that owner's first-person camera hides
/// the cockpit/body, while every other split-screen camera still sees it.
fn bind_vehicle_visual_render_layers(
    mut commands: Commands,
    jet_roots: Query<(Entity, Ref<JetBikeVisual>)>,
    heavy_roots: Query<(Entity, Ref<HeavyWaterVehicleVisual>)>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    new_renderables: Query<
        (Entity, Option<&RenderLayers>),
        (With<Mesh3d>, Or<(Added<Mesh3d>, Changed<ChildOf>)>),
    >,
    all_renderables: Query<Option<&RenderLayers>, With<Mesh3d>>,
) {
    // New or reparented meshes discover their nearest vehicle root without
    // requiring a scan of the stable world renderables.
    for (entity, existing) in new_renderables.iter() {
        let mut current = entity;
        let owner = loop {
            if let Ok((_, visual)) = jet_roots.get(current) {
                break Some(visual.owner);
            }
            if let Ok((_, visual)) = heavy_roots.get(current) {
                break Some(visual.owner);
            }
            let Ok(parent) = parents.get(current) else {
                break None;
            };
            current = parent.parent();
        };
        let Some(owner) = owner else {
            continue;
        };
        let target = RenderLayers::layer(player_render_layer(owner));
        if existing != Some(&target) {
            commands.entity(entity).insert(target);
        }
    }

    // A same-profile handoff keeps the existing root and child hierarchy.
    // Traverse only that root when its owner component actually changed;
    // newly-added roots are already covered by the Added<Mesh3d> path above.
    for (root, visual) in jet_roots.iter() {
        if visual.is_changed() && !visual.is_added() {
            rebind_vehicle_descendants(
                &mut commands,
                root,
                visual.owner,
                &children,
                &all_renderables,
            );
        }
    }
    for (root, visual) in heavy_roots.iter() {
        if visual.is_changed() && !visual.is_added() {
            rebind_vehicle_descendants(
                &mut commands,
                root,
                visual.owner,
                &children,
                &all_renderables,
            );
        }
    }
}

fn rebind_vehicle_descendants(
    commands: &mut Commands,
    root: Entity,
    owner: u8,
    children: &Query<&Children>,
    renderables: &Query<Option<&RenderLayers>, With<Mesh3d>>,
) {
    let target = RenderLayers::layer(player_render_layer(owner));
    for descendant in children.iter_descendants(root) {
        let Ok(existing) = renderables.get(descendant) else {
            continue;
        };
        if existing != Some(&target) {
            commands.entity(descendant).insert(target.clone());
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
        if visual.owner != owner {
            visual.owner = owner;
        }
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

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sync_heavy_water_vehicle_visual(
    mut commands: Commands,
    state: Res<VehicleState>,
    time: Res<Time>,
    player_q: Query<(&PlayerIndex, &Transform, &PlayerMovement, &AimSolution), With<Player>>,
    mut visual_q: Query<
        (Entity, &mut HeavyWaterVehicleVisual, &mut Transform),
        (
            Without<Player>,
            Without<TankTurretVisual>,
            Without<TankBarrelVisual>,
        ),
    >,
    mut turret_q: Query<
        (&TankTurretVisual, &mut Transform),
        (
            Without<Player>,
            Without<HeavyWaterVehicleVisual>,
            Without<TankBarrelVisual>,
        ),
    >,
    mut barrel_q: Query<
        (&TankBarrelVisual, &mut Transform),
        (
            Without<Player>,
            Without<HeavyWaterVehicleVisual>,
            Without<TankTurretVisual>,
        ),
    >,
    assets: Res<HeavyWaterVehicleVisualAssets>,
) {
    let Some(profile) = active_heavy_water_profile(&state) else {
        for (entity, _, _) in visual_q.iter_mut() {
            commands.entity(entity).despawn();
        }
        return;
    };
    let Some(owner) = state.active_owner else {
        return;
    };
    let Some((_, rider_transform, movement, aim)) =
        player_q.iter().find(|(index, _, _, _)| index.0 == owner)
    else {
        return;
    };

    let speed = movement.ground_velocity.with_y(0.0).length();
    let turbo_lift = if state.turbo_active() { 0.10 } else { 0.0 };
    let bob = match profile {
        HeavyWaterVehicleProfile::SpaceFighter | HeavyWaterVehicleProfile::Starship => {
            (time.elapsed_secs() * 7.0).sin() * 0.08 + turbo_lift
        }
        HeavyWaterVehicleProfile::Atv => {
            (time.elapsed_secs() * 10.0).sin() * (speed * 0.006).min(0.04)
        }
        HeavyWaterVehicleProfile::Tank | HeavyWaterVehicleProfile::GiantMech => 0.0,
    };
    let offset = match profile {
        HeavyWaterVehicleProfile::Atv => Vec3::new(0.0, -0.72 + bob, 0.12),
        HeavyWaterVehicleProfile::Tank => Vec3::new(0.0, -0.82, 0.18),
        HeavyWaterVehicleProfile::GiantMech => Vec3::new(0.0, -0.12, 0.12),
        HeavyWaterVehicleProfile::SpaceFighter => Vec3::new(0.0, -0.35 + bob, 0.15),
        HeavyWaterVehicleProfile::Starship => Vec3::new(0.0, -0.55 + bob, 0.25),
    };
    let visual_transform = Transform::from_translation(
        rider_transform.translation + rider_transform.rotation * offset,
    )
    .with_rotation(rider_transform.rotation);

    let mut matching_visual = false;
    for (entity, mut visual, mut transform) in visual_q.iter_mut() {
        if matching_visual || visual.profile != profile {
            commands.entity(entity).despawn();
            continue;
        }
        if visual.owner != owner {
            visual.owner = owner;
        }
        *transform = visual_transform;
        matching_visual = true;
    }

    // Turret yaw and barrel pitch both derive from the canonical AimSolution,
    // so the visible cannon, reticle, and actual shell cannot disagree.
    if profile == HeavyWaterVehicleProfile::Tank {
        let horizontal = aim.direction.with_y(0.0).normalize_or(Vec3::NEG_Z);
        let local_direction = rider_transform.rotation.inverse() * horizontal;
        let local_yaw = (-local_direction.x).atan2(-local_direction.z);
        let pitch = aim
            .direction
            .y
            .atan2(aim.direction.with_y(0.0).length().max(0.001))
            .clamp(-0.42, 0.55);
        for (turret, mut transform) in turret_q.iter_mut() {
            if turret.owner == owner {
                transform.rotation = Quat::from_rotation_y(local_yaw);
            }
        }
        for (barrel, mut transform) in barrel_q.iter_mut() {
            if barrel.owner == owner {
                transform.rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2 + pitch);
            }
        }
    }

    if matching_visual {
        return;
    }

    let armor_material = &assets.armor_material;
    let secondary_material = &assets.secondary_material;
    let dark_material = &assets.dark_material;
    let glow_material = &assets.glow_material;
    let glass_material = &assets.glass_material;

    commands
        .spawn((
            Name::new(profile.label()),
            SpatialBundle::from_transform(visual_transform),
            HeavyWaterVehicleVisual { owner, profile },
        ))
        .with_children(|vehicle| match profile {
            HeavyWaterVehicleProfile::Atv => {
                vehicle.spawn(PbrBundle {
                    mesh: Mesh3d(assets.atv.body.clone()),
                    material: MeshMaterial3d(armor_material.clone()),
                    transform: Transform::from_xyz(0.0, 0.55, 0.0),
                    ..default()
                });
                vehicle.spawn(PbrBundle {
                    mesh: Mesh3d(assets.atv.cowling.clone()),
                    material: MeshMaterial3d(secondary_material.clone()),
                    transform: Transform::from_xyz(0.0, 0.78, -1.82)
                        .with_rotation(Quat::from_rotation_x(0.18)),
                    ..default()
                });
                vehicle.spawn(PbrBundle {
                    mesh: Mesh3d(assets.atv.seat.clone()),
                    material: MeshMaterial3d(dark_material.clone()),
                    transform: Transform::from_xyz(0.0, 1.16, 0.45),
                    ..default()
                });
                for x in [-1.43_f32, 1.43] {
                    for z in [-1.22_f32, 1.22] {
                        vehicle.spawn(PbrBundle {
                            mesh: Mesh3d(assets.atv.wheel.clone()),
                            material: MeshMaterial3d(dark_material.clone()),
                            transform: Transform::from_xyz(x, 0.24, z)
                                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                            ..default()
                        });
                    }
                }
                for x in [-0.92_f32, 0.92] {
                    vehicle.spawn(PbrBundle {
                        mesh: Mesh3d(assets.atv.strut.clone()),
                        material: MeshMaterial3d(secondary_material.clone()),
                        transform: Transform::from_xyz(x, 1.63, 0.54)
                            .with_rotation(Quat::from_rotation_x(-0.18)),
                        ..default()
                    });
                    vehicle.spawn(PbrBundle {
                        mesh: Mesh3d(assets.atv.lamp.clone()),
                        material: MeshMaterial3d(glow_material.clone()),
                        transform: Transform::from_xyz(x * 0.72, 0.82, -2.46),
                        ..default()
                    });
                }
                vehicle.spawn(PbrBundle {
                    mesh: Mesh3d(assets.atv.handlebar.clone()),
                    material: MeshMaterial3d(secondary_material.clone()),
                    transform: Transform::from_xyz(0.0, 2.28, 0.30),
                    ..default()
                });
            }
            HeavyWaterVehicleProfile::Tank => {
                for x in [-1.48_f32, 1.48] {
                    vehicle.spawn(PbrBundle {
                        mesh: Mesh3d(assets.tank.tread.clone()),
                        material: MeshMaterial3d(dark_material.clone()),
                        transform: Transform::from_xyz(x, 0.42, 0.0),
                        ..default()
                    });
                    vehicle.spawn(PbrBundle {
                        mesh: Mesh3d(assets.tank.tread_glow.clone()),
                        material: MeshMaterial3d(glow_material.clone()),
                        transform: Transform::from_xyz(x.signum() * 1.96, 1.02, 0.05),
                        ..default()
                    });
                }
                vehicle.spawn(PbrBundle {
                    mesh: Mesh3d(assets.tank.hull.clone()),
                    material: MeshMaterial3d(armor_material.clone()),
                    transform: Transform::from_xyz(0.0, 1.02, 0.0),
                    ..default()
                });
                vehicle.spawn(PbrBundle {
                    mesh: Mesh3d(assets.tank.glacis.clone()),
                    material: MeshMaterial3d(secondary_material.clone()),
                    transform: Transform::from_xyz(0.0, 1.25, -2.22)
                        .with_rotation(Quat::from_rotation_x(-0.22)),
                    ..default()
                });
                vehicle
                    .spawn((
                        SpatialBundle::from_transform(Transform::from_xyz(0.0, 1.86, -0.10)),
                        TankTurretVisual { owner },
                    ))
                    .with_children(|turret| {
                        turret.spawn(PbrBundle {
                            mesh: Mesh3d(assets.tank.turret_ring.clone()),
                            material: MeshMaterial3d(armor_material.clone()),
                            ..default()
                        });
                        turret.spawn(PbrBundle {
                            mesh: Mesh3d(assets.tank.turret.clone()),
                            material: MeshMaterial3d(armor_material.clone()),
                            transform: Transform::from_xyz(0.0, 0.55, -0.08),
                            ..default()
                        });
                        turret.spawn(PbrBundle {
                            mesh: Mesh3d(assets.tank.sensor.clone()),
                            material: MeshMaterial3d(glow_material.clone()),
                            transform: Transform::from_xyz(0.0, 0.78, -1.42),
                            ..default()
                        });
                        turret.spawn((
                            PbrBundle {
                                mesh: Mesh3d(assets.tank.barrel.clone()),
                                material: MeshMaterial3d(armor_material.clone()),
                                transform: Transform::from_xyz(0.0, 0.52, -2.65).with_rotation(
                                    Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
                                ),
                                ..default()
                            },
                            TankBarrelVisual { owner },
                        ));
                    });
            }
            HeavyWaterVehicleProfile::GiantMech => {
                vehicle.spawn(PbrBundle {
                    mesh: Mesh3d(assets.mech.torso.clone()),
                    material: MeshMaterial3d(armor_material.clone()),
                    transform: Transform::from_xyz(0.0, 4.6, 0.0),
                    ..default()
                });
                vehicle.spawn(PbrBundle {
                    mesh: Mesh3d(assets.mech.cockpit.clone()),
                    material: MeshMaterial3d(glass_material.clone()),
                    transform: Transform::from_xyz(0.0, 7.35, -0.42),
                    ..default()
                });
                for x in [-1.45_f32, 1.45] {
                    vehicle.spawn(PbrBundle {
                        mesh: Mesh3d(assets.mech.leg.clone()),
                        material: MeshMaterial3d(dark_material.clone()),
                        transform: Transform::from_xyz(x, 1.92, 0.10),
                        ..default()
                    });
                }
                for x in [-2.75_f32, 2.75] {
                    vehicle.spawn(PbrBundle {
                        mesh: Mesh3d(assets.mech.arm.clone()),
                        material: MeshMaterial3d(secondary_material.clone()),
                        transform: Transform::from_xyz(x, 4.5, 0.0)
                            .with_rotation(Quat::from_rotation_z(x.signum() * -0.12)),
                        ..default()
                    });
                }
                vehicle.spawn(PbrBundle {
                    mesh: Mesh3d(assets.mech.accent.clone()),
                    material: MeshMaterial3d(glow_material.clone()),
                    transform: Transform::from_xyz(0.0, 4.88, -1.38),
                    ..default()
                });
            }
            HeavyWaterVehicleProfile::SpaceFighter | HeavyWaterVehicleProfile::Starship => {
                let scale = if profile == HeavyWaterVehicleProfile::Starship {
                    1.55
                } else {
                    1.0
                };
                let flight_meshes = if profile == HeavyWaterVehicleProfile::Starship {
                    &assets.starship
                } else {
                    &assets.fighter
                };
                vehicle.spawn(PbrBundle {
                    mesh: Mesh3d(flight_meshes.fuselage.clone()),
                    material: MeshMaterial3d(armor_material.clone()),
                    transform: Transform::from_xyz(0.0, 0.35 * scale, 0.0),
                    ..default()
                });
                vehicle.spawn(PbrBundle {
                    mesh: Mesh3d(flight_meshes.wing.clone()),
                    material: MeshMaterial3d(secondary_material.clone()),
                    transform: Transform::from_xyz(0.0, 0.18 * scale, 0.35 * scale)
                        .with_rotation(Quat::from_rotation_y(0.08)),
                    ..default()
                });
                vehicle.spawn(PbrBundle {
                    mesh: Mesh3d(flight_meshes.canopy.clone()),
                    material: MeshMaterial3d(glass_material.clone()),
                    transform: Transform::from_xyz(0.0, 0.92 * scale, -0.95 * scale)
                        .with_scale(Vec3::new(0.95, 0.62, 1.65) * scale),
                    ..default()
                });
                for x in [-1.85_f32, 1.85] {
                    vehicle.spawn(PbrBundle {
                        mesh: Mesh3d(flight_meshes.cannon.clone()),
                        material: MeshMaterial3d(glow_material.clone()),
                        transform: Transform::from_xyz(x * scale, 0.15 * scale, -2.45 * scale)
                            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                        ..default()
                    });
                    vehicle.spawn(PbrBundle {
                        mesh: Mesh3d(flight_meshes.thruster.clone()),
                        material: MeshMaterial3d(glow_material.clone()),
                        transform: Transform::from_xyz(x * 0.48 * scale, 0.1, 3.2 * scale)
                            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                        ..default()
                    });
                }
                vehicle.spawn(PbrBundle {
                    mesh: Mesh3d(flight_meshes.tail.clone()),
                    material: MeshMaterial3d(secondary_material.clone()),
                    transform: Transform::from_xyz(0.0, 1.1 * scale, 2.35 * scale)
                        .with_rotation(Quat::from_rotation_x(-0.18)),
                    ..default()
                });
            }
        });
}

fn tank_cannon_system(
    mut commands: Commands,
    mut state: ResMut<VehicleState>,
    assets: Res<ProjectileAssets>,
    mut player_q: Query<
        (
            Entity,
            &PlayerIndex,
            &Transform,
            &PlayerInput,
            &AimSolution,
            &mut PlayerMovement,
        ),
        With<Player>,
    >,
    mut fired_ev: MessageWriter<WeaponFiredEvent>,
) {
    let Some(owner) = state
        .active_owner
        .filter(|owner| state.player_owns_tank(*owner))
    else {
        return;
    };
    let Some((player, _, transform, input, aim, mut movement)) = player_q
        .iter_mut()
        .find(|(_, index, _, _, _, _)| index.0 == owner)
    else {
        return;
    };
    if !input.fire_just || state.tank_cannon_cooldown > 0.0 {
        return;
    }

    let direction = aim.direction.normalize_or(transform.forward().as_vec3());
    let muzzle = transform.translation + Vec3::Y * 1.55 + direction * 4.25;
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(assets.sphere_md.clone()),
            material: MeshMaterial3d(assets.mat_rocket.clone()),
            transform: Transform::from_translation(muzzle)
                .looking_to(direction, Vec3::Y)
                .with_scale(Vec3::new(0.9, 0.9, 2.2)),
            ..default()
        },
        Projectile {
            damage: 95.0,
            damage_type: DamageType::Explosive,
            speed: if state.turbo_active() { 82.0 } else { 68.0 },
            direction,
            lifetime: 4.0,
            is_explosive: true,
            explosion_radius: 7.0,
            weapon_type: ProjectileOwner::Player,
            owner: Some(player),
            piercing: false,
            gravity_affected: false,
            vertical_velocity: 0.0,
        },
    ));
    commands.spawn((
        PbrBundle {
            mesh: Mesh3d(assets.flash_sphere.clone()),
            material: MeshMaterial3d(assets.mat_muzzle_flash.clone()),
            transform: Transform::from_translation(muzzle).with_scale(Vec3::splat(2.2)),
            ..default()
        },
        TankMuzzleFlash { remaining: 0.13 },
    ));
    movement.ground_velocity -= direction.with_y(0.0) * 0.055;
    state.tank_cannon_cooldown = 1.20;
    fired_ev.write(WeaponFiredEvent);
}

fn tank_muzzle_flash_system(
    mut commands: Commands,
    time: Res<Time>,
    mut flash_q: Query<(Entity, &mut TankMuzzleFlash, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, mut flash, mut transform) in flash_q.iter_mut() {
        flash.remaining -= dt;
        transform.scale *= 1.0 + dt * 7.5;
        if flash.remaining <= 0.0 {
            commands.entity(entity).despawn();
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
            &mut Damageable,
        ),
        With<Player>,
    >,
) {
    let turbo_active = state.turbo_active();
    for (idx, mut mv, mut jet, mut stats, mut damageable) in q.iter_mut() {
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
                    mv.sprint_speed =
                        baseline
                            .sprint_speed
                            .max(if turbo_active { 1.42 } else { 1.10 });
                }
                GroundMode::Tank => {
                    // Slow, armored, and immune to accumulated knockback.
                    mv.walk_speed = baseline
                        .walk_speed
                        .min(if turbo_active { 0.46 } else { 0.32 });
                    mv.sprint_speed =
                        baseline
                            .sprint_speed
                            .min(if turbo_active { 0.58 } else { 0.42 });
                    mv.knockback_velocity = Vec3::ZERO;
                    damageable.pending_knockback = Vec3::ZERO;
                }
                GroundMode::GiantMech => {
                    mv.walk_speed = baseline
                        .walk_speed
                        .min(if turbo_active { 0.34 } else { 0.24 });
                    mv.sprint_speed =
                        baseline
                            .sprint_speed
                            .min(if turbo_active { 0.46 } else { 0.34 });
                    mv.knockback_velocity = Vec3::ZERO;
                    damageable.pending_knockback = Vec3::ZERO;
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
                    jet.force = baseline
                        .jet_force
                        .max(if turbo_active { 0.17 } else { 0.12 });
                    jet.regen_rate = baseline.jet_regen_rate.max(80.0);
                    jet.max_vertical_vel =
                        baseline
                            .jet_max_vertical_vel
                            .max(if turbo_active { 0.90 } else { 0.7 });
                }
                AirMode::Ship => {
                    jet.force = baseline
                        .jet_force
                        .max(if turbo_active { 0.25 } else { 0.18 });
                    jet.regen_rate = baseline.jet_regen_rate.max(120.0);
                    jet.max_vertical_vel =
                        baseline
                            .jet_max_vertical_vel
                            .max(if turbo_active { 1.42 } else { 1.1 });
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
        apply_vehicle_armor_bonus(
            &mut stats,
            &mut state.armor_buffers[slot_index],
            desired_armor_bonus,
        );
    }
}

fn apply_vehicle_armor_bonus(
    stats: &mut PlayerStats,
    buffer: &mut VehicleArmorBuffer,
    desired_bonus: f32,
) {
    let desired_bonus = desired_bonus.max(0.0);

    // Damage consumes the temporary buffer before base armor. Positive armor
    // changes are healing and must remain base armor rather than refilling a
    // spent vehicle buffer for free.
    if buffer.capacity > 0.0 {
        buffer.remaining = buffer.remaining_at_total(stats.armor);
    }

    if desired_bonus > buffer.capacity {
        let added = desired_bonus - buffer.capacity;
        stats.armor += added;
        buffer.remaining += added;
    } else if desired_bonus < buffer.capacity {
        let allowed_remaining = buffer.remaining.min(desired_bonus);
        stats.armor = (stats.armor - (buffer.remaining - allowed_remaining)).max(0.0);
        buffer.remaining = allowed_remaining;
    }

    buffer.capacity = desired_bonus;
    let unclamped = stats.armor;
    stats.armor = stats.armor.min(stats.max_armor + desired_bonus);
    buffer.remaining = (buffer.remaining - (unclamped - stats.armor).max(0.0)).max(0.0);
    buffer.last_total = (desired_bonus > 0.0).then_some(stats.armor);
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

        boat_transform.translation = clamp_boat_to_route(boat_transform.translation, boat);

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

fn clamp_boat_to_route(position: Vec3, boat: &BoatVehicle) -> Vec3 {
    let start = Vec2::new(boat.dock_position.x, boat.dock_position.z);
    let end = Vec2::new(boat.island_position.x, boat.island_position.z);
    let axis = end - start;
    let length = axis.length();
    if length <= 0.001 {
        return Vec3::new(start.x, position.y, start.y);
    }

    let forward = axis / length;
    let right = Vec2::new(forward.y, -forward.x);
    let point = Vec2::new(position.x, position.z);
    let offset = point - start;
    let along = offset
        .dot(forward)
        .clamp(-boat.dock_radius, length + boat.dock_radius);
    let lateral = offset
        .dot(right)
        .clamp(-boat.route_half_width, boat.route_half_width);
    let clamped = start + forward * along + right * lateral;
    Vec3::new(clamped.x, position.y, clamped.y)
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
    use crate::world::robot_pets::RobotAssembly;

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
    fn air_blueprint_does_not_turn_an_assembled_atv_into_a_jet_bike() {
        let mut loadout = PlayerLoadout::default();
        loadout.add_blueprint("jet_blueprint");

        for form in [RobotAssemblyForm::Car, RobotAssemblyForm::Motorcycle] {
            let robots = robot_pets_with_assembly(form);
            assert!(!jet_bike_available(&robots, &loadout));
            assert_eq!(
                preferred_vehicle_entry_profile(&robots, &loadout),
                VehicleEntryProfile::Ground(GroundMode::Motorcycle)
            );
        }
    }

    #[test]
    fn paired_loose_blueprints_keep_the_no_assembly_jet_bike_fallback() {
        let robots = RobotPetCollection::default();
        let mut loadout = PlayerLoadout::default();
        loadout.add_blueprint("motorcycle_blueprint");
        loadout.add_blueprint("jet_blueprint");

        assert!(jet_bike_available(&robots, &loadout));
    }

    #[test]
    fn assembled_heavy_ground_vehicle_wins_over_air_blueprint_fallback() {
        let mut loadout = PlayerLoadout::default();
        loadout.add_blueprint("jet_blueprint");

        for (form, expected) in [
            (RobotAssemblyForm::Tank, GroundMode::Tank),
            (RobotAssemblyForm::GiantMech, GroundMode::GiantMech),
        ] {
            assert_eq!(
                preferred_vehicle_entry_profile(&robot_pets_with_assembly(form), &loadout),
                VehicleEntryProfile::Ground(expected)
            );
        }
    }

    #[test]
    fn assembled_ship_wins_over_ground_blueprint_fallback() {
        let mut loadout = PlayerLoadout::default();
        loadout.add_blueprint("motorcycle_blueprint");

        assert_eq!(
            preferred_vehicle_entry_profile(
                &robot_pets_with_assembly(RobotAssemblyForm::SpaceShip),
                &loadout
            ),
            VehicleEntryProfile::Air(AirMode::Ship)
        );
    }

    #[test]
    fn assembled_space_jet_wins_over_ground_blueprint_fallback() {
        let mut loadout = PlayerLoadout::default();
        loadout.add_blueprint("motorcycle_blueprint");
        let robots = robot_pets_with_assembly(RobotAssemblyForm::SpaceJet);

        assert!(!jet_bike_available(&robots, &loadout));
        assert_eq!(
            preferred_vehicle_entry_profile(&robots, &loadout),
            VehicleEntryProfile::Air(AirMode::Jet)
        );
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
    fn vehicle_traversal_suspends_restores_and_saves_the_selected_tool() {
        let mut suspended = None;
        let mut traversal = TraversalModeState {
            active: TraversalMode::Hoverboard,
            ..default()
        };
        let mut boost = BoardBoostState {
            timer: 1.4,
            speed_mult: 3.2,
            direction: Vec3::X,
            ..default()
        };

        reconcile_vehicle_traversal(true, &mut suspended, &mut traversal, &mut boost);
        assert_eq!(suspended, Some(TraversalMode::Hoverboard));
        assert_eq!(traversal.active, TraversalMode::Vehicle);
        assert_eq!(boost.timer, 0.0);
        assert_eq!(boost.speed_mult, 1.0);

        // Reconciliation is idempotent and cannot replace the saved selection
        // with the neutral runtime mode while the vehicle remains active. A
        // road-pad boost applied after entry also remains active.
        boost.timer = 1.7;
        boost.speed_mult = 3.1;
        reconcile_vehicle_traversal(true, &mut suspended, &mut traversal, &mut boost);
        assert_eq!(suspended, Some(TraversalMode::Hoverboard));
        assert_eq!(boost.timer, 1.7);
        assert_eq!(boost.speed_mult, 3.1);

        let mut state = VehicleState::default();
        state.suspended_traversals[0] = suspended;
        assert_eq!(
            state.persistent_traversal(0, traversal.active),
            TraversalMode::Hoverboard
        );

        reconcile_vehicle_traversal(false, &mut suspended, &mut traversal, &mut boost);
        assert_eq!(suspended, None);
        assert_eq!(traversal.active, TraversalMode::Hoverboard);
    }

    #[test]
    fn heavy_water_vehicle_profiles_follow_authoritative_modes() {
        let mut state = VehicleState {
            ground_mode: GroundMode::Motorcycle,
            active_owner: Some(1),
            ..default()
        };
        assert_eq!(
            active_heavy_water_profile(&state),
            Some(HeavyWaterVehicleProfile::Atv)
        );

        state.ground_mode = GroundMode::Tank;
        assert_eq!(
            active_heavy_water_profile(&state),
            Some(HeavyWaterVehicleProfile::Tank)
        );
        assert!(state.player_owns_tank(1));
        assert!(!state.player_owns_tank(0));

        state.ground_mode = GroundMode::None;
        state.air_mode = AirMode::Jet;
        assert_eq!(
            active_heavy_water_profile(&state),
            Some(HeavyWaterVehicleProfile::SpaceFighter)
        );

        state.air_mode = AirMode::Ship;
        assert_eq!(
            active_heavy_water_profile(&state),
            Some(HeavyWaterVehicleProfile::Starship)
        );

        state.boat_active = true;
        assert_eq!(active_heavy_water_profile(&state), None);
    }

    #[test]
    fn heavy_water_vehicle_visual_assets_are_authored_once() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .add_systems(Startup, setup_heavy_water_vehicle_visual_assets);

        app.update();

        let assets = app.world().resource::<HeavyWaterVehicleVisualAssets>();
        assert_eq!(assets.fighter.canopy, assets.starship.canopy);
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 31);
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), 5);
    }

    #[test]
    fn same_profile_owner_handoff_rebinds_only_vehicle_descendants() {
        let mut app = App::new();
        app.add_systems(Update, bind_vehicle_visual_render_layers);

        let heavy_root = app
            .world_mut()
            .spawn(HeavyWaterVehicleVisual {
                owner: 0,
                profile: HeavyWaterVehicleProfile::Tank,
            })
            .id();
        let heavy_group = app.world_mut().spawn_empty().id();
        let heavy_mesh = app.world_mut().spawn(Mesh3d::default()).id();
        app.world_mut()
            .entity_mut(heavy_group)
            .add_child(heavy_mesh);
        app.world_mut()
            .entity_mut(heavy_root)
            .add_child(heavy_group);

        let jet_root = app.world_mut().spawn(JetBikeVisual { owner: 1 }).id();
        let jet_mesh = app.world_mut().spawn(Mesh3d::default()).id();
        app.world_mut().entity_mut(jet_root).add_child(jet_mesh);

        let sentinel = RenderLayers::layer(31);
        let unrelated = app
            .world_mut()
            .spawn((Mesh3d::default(), sentinel.clone()))
            .id();

        app.update();
        assert_eq!(
            app.world().get::<RenderLayers>(heavy_mesh),
            Some(&RenderLayers::layer(player_render_layer(0)))
        );
        assert_eq!(
            app.world().get::<RenderLayers>(jet_mesh),
            Some(&RenderLayers::layer(player_render_layer(1)))
        );

        // A stable owner does not traverse or rewrite its existing hierarchy.
        app.world_mut()
            .entity_mut(heavy_mesh)
            .insert(sentinel.clone());
        app.world_mut()
            .entity_mut(jet_mesh)
            .insert(sentinel.clone());
        app.update();
        assert_eq!(app.world().get::<RenderLayers>(heavy_mesh), Some(&sentinel));
        assert_eq!(app.world().get::<RenderLayers>(jet_mesh), Some(&sentinel));

        // Retaining the same profile/root while changing owners rebinds every
        // descendant, but never scans or mutates unrelated world meshes.
        app.world_mut()
            .get_mut::<HeavyWaterVehicleVisual>(heavy_root)
            .unwrap()
            .owner = 2;
        app.world_mut()
            .get_mut::<JetBikeVisual>(jet_root)
            .unwrap()
            .owner = 3;
        app.update();
        assert_eq!(
            app.world().get::<RenderLayers>(heavy_mesh),
            Some(&RenderLayers::layer(player_render_layer(2)))
        );
        assert_eq!(
            app.world().get::<RenderLayers>(jet_mesh),
            Some(&RenderLayers::layer(player_render_layer(3)))
        );
        assert_eq!(app.world().get::<RenderLayers>(unrelated), Some(&sentinel));
    }

    #[test]
    fn tank_and_mech_suppress_fresh_pending_knockback_before_motor_intake() {
        for mode in [GroundMode::Tank, GroundMode::GiantMech] {
            let mut app = App::new();
            app.add_systems(Update, apply_vehicle_buffs);
            app.insert_resource(VehicleState {
                ground_mode: mode,
                active_owner: Some(0),
                ..default()
            });
            let player = app
                .world_mut()
                .spawn((
                    Player,
                    PlayerIndex(0),
                    PlayerMovement {
                        knockback_velocity: Vec3::X * 8.0,
                        ..default()
                    },
                    JetpackState::default(),
                    PlayerStats::default(),
                    Damageable {
                        pending_knockback: Vec3::Z * 12.0,
                        ..default()
                    },
                ))
                .id();

            app.update();

            assert_eq!(
                app.world()
                    .get::<PlayerMovement>(player)
                    .unwrap()
                    .knockback_velocity,
                Vec3::ZERO
            );
            assert_eq!(
                app.world()
                    .get::<Damageable>(player)
                    .unwrap()
                    .pending_knockback,
                Vec3::ZERO
            );
        }
    }

    #[test]
    fn mech_armor_bonus_applies_once_and_clamps_when_removed() {
        let mut stats = PlayerStats {
            armor: 60.0,
            max_armor: 100.0,
            ..default()
        };
        let mut buffer = VehicleArmorBuffer::default();

        apply_vehicle_armor_bonus(&mut stats, &mut buffer, 40.0);
        assert_eq!(stats.armor, 100.0);

        apply_vehicle_armor_bonus(&mut stats, &mut buffer, 40.0);
        assert_eq!(stats.armor, 100.0);

        stats.armor = 80.0;
        apply_vehicle_armor_bonus(&mut stats, &mut buffer, 0.0);
        assert_eq!(stats.armor, 60.0);
    }

    #[test]
    fn tank_armor_is_a_temporary_twenty_five_point_buffer() {
        let mut stats = PlayerStats {
            armor: 80.0,
            max_armor: 100.0,
            ..default()
        };
        let mut buffer = VehicleArmorBuffer::default();

        apply_vehicle_armor_bonus(&mut stats, &mut buffer, 25.0);
        assert_eq!(stats.armor, 105.0);

        apply_vehicle_armor_bonus(&mut stats, &mut buffer, 25.0);
        assert_eq!(stats.armor, 105.0);

        // Fifteen damage consumes fifteen temporary points; exiting removes
        // only the ten that remain, restoring the exact pre-vehicle armor.
        stats.armor = 90.0;
        let mut active_state = VehicleState::default();
        active_state.armor_buffers[0] = buffer;
        assert_eq!(active_state.persistent_armor(0, stats.armor), 80.0);
        apply_vehicle_armor_bonus(&mut stats, &mut buffer, 0.0);
        assert_eq!(stats.armor, 80.0);

        // Repeated toggles cannot manufacture permanent armor.
        apply_vehicle_armor_bonus(&mut stats, &mut buffer, 25.0);
        assert_eq!(stats.armor, 105.0);
        apply_vehicle_armor_bonus(&mut stats, &mut buffer, 0.0);
        assert_eq!(stats.armor, 80.0);
    }

    #[test]
    fn vehicle_armor_buffer_preserves_real_healing_without_refilling_itself() {
        let mut stats = PlayerStats {
            armor: 80.0,
            max_armor: 100.0,
            ..default()
        };
        let mut buffer = VehicleArmorBuffer::default();

        apply_vehicle_armor_bonus(&mut stats, &mut buffer, 25.0);
        stats.armor = 90.0;
        apply_vehicle_armor_bonus(&mut stats, &mut buffer, 25.0);
        assert_eq!(buffer.remaining, 10.0);

        stats.armor += 10.0;
        apply_vehicle_armor_bonus(&mut stats, &mut buffer, 25.0);
        assert_eq!(buffer.remaining, 10.0);
        apply_vehicle_armor_bonus(&mut stats, &mut buffer, 0.0);
        assert_eq!(stats.armor, 90.0);
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
    fn boat_route_clamp_supports_diagonal_and_east_west_lanes() {
        let diagonal = BoatVehicle {
            embark_radius: 8.0,
            passenger_radius: 12.0,
            dock_radius: 20.0,
            route_half_width: 15.0,
            speed: 24.0,
            dock_position: Vec3::new(-100.0, 4.0, -50.0),
            island_position: Vec3::new(200.0, 4.0, 250.0),
        };
        let clamped = clamp_boat_to_route(Vec3::new(400.0, 4.0, -200.0), &diagonal);
        let route = (diagonal.island_position - diagonal.dock_position)
            .with_y(0.0)
            .normalize();
        let right = Vec3::new(route.z, 0.0, -route.x);
        let lateral = (clamped - diagonal.dock_position).dot(right).abs();
        assert!(lateral <= diagonal.route_half_width + 0.001);

        let east_west = BoatVehicle {
            island_position: Vec3::new(500.0, 4.0, 0.0),
            dock_position: Vec3::new(0.0, 4.0, 0.0),
            ..diagonal
        };
        let clamped = clamp_boat_to_route(Vec3::new(250.0, 4.0, 80.0), &east_west);
        assert!((clamped.z.abs() - east_west.route_half_width).abs() < 0.001);
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
