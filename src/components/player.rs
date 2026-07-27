#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
use bevy::prelude::*;

use crate::components::weapon::WeaponRanks;
use crate::combat::perks::PerkTree;
use crate::combat::upgrades::UpgradeLedger;

// ── Marker ────────────────────────────────────────────────────────────────────
/// Marks the player entity (the physics body).
#[derive(Component, Default)]
pub struct Player;

/// Marks the first-person camera child entity.
#[derive(Component, Default)]
pub struct PlayerCamera;

// ── Stats ─────────────────────────────────────────────────────────────────────
#[derive(Component, Debug, Clone)]
pub struct PlayerStats {
    /// Cached effective health cap derived from [`PlayerBaseStats`].
    pub max_health: f32,
    /// Rechargeable armor durability remaining after equipment mitigation.
    /// This is a consumable buffer, not `ArmorSet::total_defense()`.
    pub armor: f32,
    /// Capacity of the rechargeable armor-durability buffer.
    pub max_armor: f32,
    pub stamina: f32,
    pub max_stamina: f32,
    pub credits: u32,
    pub experience: u32,
    pub level: u32,
}

/// Stable level-one caps authored by the character blueprint and hero profile.
///
/// These values are saved independently from the effective caps in
/// [`PlayerStats`]. Level, equipment, perks, and upgrades are modifiers; they
/// must never be folded back into these bases.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PlayerBaseStats {
    pub max_health: f32,
    pub max_armor: f32,
}

impl Default for PlayerBaseStats {
    fn default() -> Self {
        Self {
            max_health: 100.0,
            max_armor: 100.0,
        }
    }
}

/// Effective caps produced from stable authored bases and current modifiers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DerivedPlayerCaps {
    pub max_health: f32,
    pub max_armor: f32,
}

impl PlayerBaseStats {
    pub const HEALTH_PER_LEVEL: f32 = 10.0;

    pub fn derived_caps(
        self,
        level: u32,
        equipment_health_bonus: f32,
        perk_health_bonus: f32,
        upgrade_health_bonus: f32,
        shield_capacity_bonus: f32,
    ) -> DerivedPlayerCaps {
        let level_health_bonus = level.saturating_sub(1) as f32 * Self::HEALTH_PER_LEVEL;
        DerivedPlayerCaps {
            max_health: (self.max_health.max(1.0)
                + level_health_bonus
                + equipment_health_bonus.max(0.0)
                + perk_health_bonus.max(0.0)
                + upgrade_health_bonus.max(0.0))
            .max(1.0),
            max_armor: (self.max_armor.max(1.0) + shield_capacity_bonus.max(0.0)).max(1.0),
        }
    }

    /// Recover additive-schema bases from a legacy save that stored only
    /// effective caps. Known modifiers are removed once; unknown historical
    /// modifiers remain absorbed in the inferred base to avoid reducing a
    /// player's saved cap.
    pub fn from_legacy_effective(
        effective_health: f32,
        effective_armor: f32,
        level: u32,
        equipment_health_bonus: f32,
        perk_health_bonus: f32,
        upgrade_health_bonus: f32,
        shield_capacity_bonus: f32,
    ) -> Self {
        let known_health_bonus = level.saturating_sub(1) as f32 * Self::HEALTH_PER_LEVEL
            + equipment_health_bonus.max(0.0)
            + perk_health_bonus.max(0.0)
            + upgrade_health_bonus.max(0.0);
        Self {
            max_health: (effective_health.max(1.0) - known_health_bonus).max(1.0),
            max_armor: (effective_armor.max(1.0) - shield_capacity_bonus.max(0.0)).max(1.0),
        }
    }
}

impl Default for PlayerStats {
    fn default() -> Self {
        Self {
            max_health: 100.0,
            armor: 100.0,
            max_armor: 100.0,
            stamina: 100.0,
            max_stamina: 100.0,
            credits: 0,
            experience: 0,
            level: 1,
        }
    }
}

/// Save-backed progression owned by one local player.
///
/// Legacy campaign resources seed this component when old saves are loaded;
/// new saves persist an independent copy for every `PlayerIndex`.
#[derive(Component, Debug, Clone, Default)]
pub struct PlayerProgression {
    pub perks: PerkTree,
    pub upgrades: UpgradeLedger,
    pub weapon_ranks: WeaponRanks,
    /// Per-player shop purchases and equipped choices (PX3).
    pub shop: crate::world::shop_transactions::ShopOwnership,
}

impl PlayerStats {
    pub fn xp_for_next_level(&self) -> u32 {
        self.level * 100
    }
}

#[cfg(test)]
mod derived_cap_tests {
    use super::*;

    #[test]
    fn authored_bases_and_modifiers_derive_without_compounding() {
        let base = PlayerBaseStats {
            max_health: 135.0,
            max_armor: 80.0,
        };

        let caps = base.derived_caps(4, 12.0, 20.0, 15.0, 8.0);

        assert_eq!(caps.max_health, 212.0);
        assert_eq!(caps.max_armor, 88.0);
        assert_eq!(base.max_health, 135.0);
        assert_eq!(base.max_armor, 80.0);
    }

    #[test]
    fn legacy_inference_round_trips_known_modifiers() {
        let inferred =
            PlayerBaseStats::from_legacy_effective(212.0, 88.0, 4, 12.0, 20.0, 15.0, 8.0);

        assert_eq!(
            inferred.derived_caps(4, 12.0, 20.0, 15.0, 8.0),
            DerivedPlayerCaps {
                max_health: 212.0,
                max_armor: 88.0,
            }
        );
    }
}

// ── Movement ──────────────────────────────────────────────────────────────────
#[derive(Component, Debug, Clone)]
pub struct PlayerMovement {
    pub walk_speed: f32,
    pub sprint_speed: f32,
    pub jump_force: f32,
    pub gravity: f32,
    pub fall_gravity_mult: f32,
    pub apex_gravity_mult: f32,
    pub jump_release_cutoff: f32,
    pub max_fall_speed: f32,
    pub ground_accel: f32,
    pub ground_decel: f32,
    pub air_accel: f32,
    pub air_decel: f32,
    pub coyote_time: f32,
    pub coyote_timer: f32,
    pub jump_buffer_time: f32,
    pub jump_buffer_timer: f32,
    pub wall_slide_speed: f32,
    pub wall_jump_charges: u8,
    pub max_wall_jump_charges: u8,
    pub wall_jump_lock_timer: f32,
    pub wall_jump_lock_time: f32,
    pub analog_sprint_threshold: f32,
    pub controller_offset: f32,
    pub ground_snap_distance: f32,
    pub autostep_height: f32,
    pub autostep_min_width: f32,
    pub velocity: Vec3,
    pub is_grounded: bool,
    /// Last horizontal velocity while grounded (for air momentum)
    pub ground_velocity: Vec3,
    /// EC1b: when the motor runs in `FixedUpdate`, per-tick translation is summed
    /// here and flushed to the physics controller once per frame (physics steps
    /// per-frame, so we must not overwrite/double-apply). Unused in `Update` mode.
    pub motor_accum: Vec3,
    /// EC1b judder fix: undelivered translation carried between frames. A
    /// 64 Hz sim rendered at a higher rate hands some frames a whole tick of
    /// motion and others none, which reads as stutter. The flush drains this
    /// buffer by a fraction each frame so delivery is smooth while total
    /// displacement is conserved. See `flush_motor_translation`.
    pub motor_carry: Vec3,
    /// External shove channel in world-units/sec, integrated and
    /// exponentially decayed by the motor through the character controller
    /// (walls still stop the player). Fed by received knockback
    /// (`player_knockback_intake`, mirroring the enemy drain) and by co-op
    /// pushbox separation (`player_pushbox_separation`).
    pub knockback_velocity: Vec3,
}

/// Per-player contact, breath, and wake state for gameplay water volumes.
#[derive(Component, Debug, Clone)]
pub struct WaterTraversalState {
    pub body: Option<Entity>,
    pub surface_y: f32,
    pub swimming: bool,
    pub submerged: bool,
    pub breath: f32,
    pub max_breath: f32,
    pub drowning_tick: f32,
    pub wake_cooldown: f32,
    pub wake_requested: bool,
    pub low_air_warned: bool,
}

impl Default for WaterTraversalState {
    fn default() -> Self {
        Self {
            body: None,
            surface_y: 0.0,
            swimming: false,
            submerged: false,
            breath: 12.0,
            max_breath: 12.0,
            drowning_tick: 1.0,
            wake_cooldown: 0.0,
            wake_requested: false,
            low_air_warned: false,
        }
    }
}

impl Default for PlayerMovement {
    fn default() -> Self {
        Self {
            walk_speed: 0.38,
            sprint_speed: 0.70,
            jump_force: 0.64,
            gravity: 0.024,
            fall_gravity_mult: 1.35,
            apex_gravity_mult: 0.55,
            jump_release_cutoff: 0.30,
            max_fall_speed: 2.25,
            ground_accel: 5.6,
            ground_decel: 7.8,
            air_accel: 1.45,
            air_decel: 0.85,
            coyote_time: 0.16,
            coyote_timer: 0.0,
            jump_buffer_time: 0.18,
            jump_buffer_timer: 0.0,
            wall_slide_speed: 0.28,
            wall_jump_charges: 2,
            max_wall_jump_charges: 2,
            wall_jump_lock_timer: 0.0,
            wall_jump_lock_time: 0.16,
            analog_sprint_threshold: 0.82,
            controller_offset: 0.025,
            ground_snap_distance: 0.28,
            autostep_height: 0.42,
            autostep_min_width: 0.18,
            velocity: Vec3::ZERO,
            is_grounded: true,
            ground_velocity: Vec3::ZERO,
            motor_accum: Vec3::ZERO,
            motor_carry: Vec3::ZERO,
            knockback_velocity: Vec3::ZERO,
        }
    }
}

impl PlayerMovement {
    /// Cancel both fixed-tick translation that has not reached the controller
    /// and the smoothed per-frame remainder. Hard traversal transitions
    /// (teleports, rail snaps, hangs, grapple arrivals) must use this so motion
    /// authored before the transition cannot leak into the constrained state.
    pub fn clear_motor_delivery(&mut self) {
        self.motor_accum = Vec3::ZERO;
        self.motor_carry = Vec3::ZERO;
    }
}

/// Explicit Sonic/Mario-inspired ground/air action state. Keeping these values
/// out of the main motor avoids hidden one-frame flags and makes tuning/test
/// behavior inspectable per local player.
#[derive(Component, Debug, Clone)]
pub struct PlatformerMoveState {
    pub was_grounded: bool,
    pub rolling: bool,
    pub roll_timer: f32,
    pub roll_min_speed: f32,
    pub roll_decel: f32,
    pub roll_steer: f32,
    pub stomp_active: bool,
    pub stomp_speed: f32,
    pub stomp_bounce_force: f32,
}

impl Default for PlatformerMoveState {
    fn default() -> Self {
        Self {
            was_grounded: true,
            rolling: false,
            roll_timer: 0.0,
            roll_min_speed: 0.48,
            roll_decel: 1.15,
            roll_steer: 0.34,
            stomp_active: false,
            stomp_speed: 1.65,
            stomp_bounce_force: 0.72,
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct SpeedLoopTraversalState {
    pub guide: Option<Entity>,
    pub progress: f32,
    pub speed: f32,
    pub cooldown: f32,
}

impl Default for SpeedLoopTraversalState {
    fn default() -> Self {
        Self {
            guide: None,
            progress: 0.0,
            speed: 0.0,
            cooldown: 0.0,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, Default)]
pub struct RoadRecoveryState {
    pub last_checkpoint: Option<Vec3>,
    pub cooldown: f32,
}

/// General safety state for rare terrain-collider seam/tunneling misses.
/// Gameplay still uses physics for normal grounding; this cache is consulted
/// only after the player has fallen materially beneath the rendered terrain.
#[derive(Component, Debug, Clone, Copy)]
pub struct TerrainRecoveryState {
    pub last_safe_position: Vec3,
    pub cooldown: f32,
}

impl TerrainRecoveryState {
    pub fn new(spawn_position: Vec3) -> Self {
        Self {
            last_safe_position: spawn_position,
            cooldown: 0.0,
        }
    }
}

/// Per-player skate/road combo. Points stay provisional until a clean landing,
/// allowing springs, rails, and aerial rotations to chain into one line.
#[derive(Component, Debug, Clone)]
pub struct StuntRunState {
    pub total_score: u64,
    pub pending_score: f32,
    pub multiplier: f32,
    pub airtime: f32,
    pub spin_degrees: f32,
    pub trick_count: u16,
    pub active: bool,
    pub was_grounded: bool,
    pub was_grinding: bool,
}

impl Default for StuntRunState {
    fn default() -> Self {
        Self {
            total_score: 0,
            pending_score: 0.0,
            multiplier: 1.0,
            airtime: 0.0,
            spin_degrees: 0.0,
            trick_count: 0,
            active: false,
            was_grounded: true,
            was_grinding: false,
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct StuntRaceProgress {
    pub course_id: Option<&'static str>,
    pub next_gate: u8,
    pub lap: u8,
    pub races_finished: u16,
    pub gate_cooldown: f32,
}

impl Default for StuntRaceProgress {
    fn default() -> Self {
        Self {
            course_id: None,
            next_gate: 0,
            lap: 0,
            races_finished: 0,
            gate_cooldown: 0.0,
        }
    }
}

#[derive(Component, Debug, Clone, Default)]
pub struct RooftopTrialProgress {
    pub active_course: Option<String>,
    pub active_label: Option<String>,
    pub next_checkpoint: u8,
    pub checkpoint_count: u8,
    pub elapsed: f32,
    pub gate_cooldown: f32,
    pub best_times: Vec<(String, f32)>,
}

impl RooftopTrialProgress {
    pub fn record_time(&mut self, course_key: &str, elapsed: f32) -> bool {
        if let Some((_, best)) = self
            .best_times
            .iter_mut()
            .find(|(key, _)| key == course_key)
        {
            if elapsed >= *best {
                return false;
            }
            *best = elapsed;
            return true;
        }
        self.best_times.push((course_key.to_string(), elapsed));
        true
    }

    pub fn reset_active(&mut self) {
        self.active_course = None;
        self.active_label = None;
        self.next_checkpoint = 0;
        self.checkpoint_count = 0;
        self.elapsed = 0.0;
    }
}

// ── Edge Grab / Wall Jump ────────────────────────────────────────────────────
#[derive(Component, Debug, Clone)]
pub struct EdgeGrabState {
    pub is_hanging: bool,
    pub wall_normal: Vec3,
    /// Stable root position while hanging. Unlike raw wall contact, this comes
    /// from a validated wall/clearance/top-surface ledge probe.
    pub ledge_anchor: Option<Vec3>,
    /// Walkable top point associated with `ledge_anchor`, used by Grapple-mode
    /// arrival to choose a hang or a direct mantle.
    pub ledge_top: Option<Vec3>,
    pub cooldown_timer: f32,
    pub wall_contact_timer: f32,
    pub wall_clasp_timer: f32,
    pub hang_timer: f32,
    pub max_hang_time: f32,
    pub stamina_drain_per_sec: f32,
    pub wall_slide_stamina_drain_per_sec: f32,
    pub exhausted_wall_slide_mult: f32,
    pub wall_jump_push: f32,
    pub wall_jump_vertical: f32,
    pub climb_boost: f32,
    pub grab_cooldown: f32,
}

impl Default for EdgeGrabState {
    fn default() -> Self {
        Self {
            is_hanging: false,
            wall_normal: Vec3::ZERO,
            ledge_anchor: None,
            ledge_top: None,
            cooldown_timer: 0.0,
            wall_contact_timer: 0.0,
            wall_clasp_timer: 0.0,
            hang_timer: 0.0,
            max_hang_time: 2.5,
            stamina_drain_per_sec: 12.0,
            wall_slide_stamina_drain_per_sec: 8.0,
            exhausted_wall_slide_mult: 1.45,
            wall_jump_push: 0.72,
            wall_jump_vertical: 0.74,
            climb_boost: 0.55,
            grab_cooldown: 0.22,
        }
    }
}

impl EdgeGrabState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_hang(&mut self, anchor: Vec3, top: Vec3, normal: Vec3) {
        self.is_hanging = true;
        self.ledge_anchor = Some(anchor);
        self.ledge_top = Some(top);
        self.wall_normal = normal.with_y(0.0).normalize_or_zero();
        self.hang_timer = 0.0;
        self.wall_contact_timer = self.wall_contact_timer.max(0.16);
    }

    pub fn release_hang(&mut self) {
        self.is_hanging = false;
        self.ledge_anchor = None;
        self.ledge_top = None;
    }
}

// ── Jetpack ───────────────────────────────────────────────────────────────────
#[derive(Component, Debug, Clone)]
pub struct JetpackState {
    pub fuel: f32,
    pub max_fuel: f32,
    pub force: f32,
    pub fuel_cost_per_sec: f32,
    pub regen_rate: f32,
    pub max_vertical_vel: f32,
    pub is_active: bool,
    pub mode: FlightMode,
    pub glide_fall_speed: f32,
    pub boost_forward_speed: f32,
    pub air_dash_speed: f32,
    pub air_dash_duration: f32,
    pub air_dash_timer: f32,
    pub air_dash_cooldown: f32,
    pub air_dash_cooldown_timer: f32,
    pub slam_speed: f32,
    /// Triple-tapping jump switches this persistent flight control preference.
    pub hover_mode_enabled: bool,
    pub jump_tap_count: u8,
    pub jump_tap_timer: f32,
}

impl Default for JetpackState {
    fn default() -> Self {
        Self {
            // Sized for traversing the Everest-scale ranges: ~40 s of continuous
            // thrust (was 10 s) with quicker recovery between climbs.
            fuel: 800.0,
            max_fuel: 800.0,
            force: 0.06,
            fuel_cost_per_sec: 20.0,
            regen_rate: 45.0,
            max_vertical_vel: 0.35,
            is_active: false,
            mode: FlightMode::Grounded,
            glide_fall_speed: 0.78,
            boost_forward_speed: 0.95,
            air_dash_speed: 1.55,
            air_dash_duration: 0.18,
            air_dash_timer: 0.0,
            air_dash_cooldown: 0.55,
            air_dash_cooldown_timer: 0.0,
            slam_speed: 1.35,
            hover_mode_enabled: false,
            jump_tap_count: 0,
            jump_tap_timer: 0.0,
        }
    }
}

impl JetpackState {
    pub const JUMP_TAP_WINDOW: f32 = 0.48;

    /// Returns true when a triple tap changed the flight/hover preference.
    pub fn register_jump_tap(&mut self) -> bool {
        self.jump_tap_count = if self.jump_tap_timer > 0.0 {
            self.jump_tap_count.saturating_add(1)
        } else {
            1
        };
        self.jump_tap_timer = Self::JUMP_TAP_WINDOW;
        if self.jump_tap_count >= 3 {
            self.hover_mode_enabled = !self.hover_mode_enabled;
            self.jump_tap_count = 0;
            self.jump_tap_timer = 0.0;
            true
        } else {
            false
        }
    }
}

// ── Climbing ──────────────────────────────────────────────────────────────────
/// Free-climbing up vertical surfaces (mountains, buildings). Push forward into
/// a wall to start; drains its own energy bar (shown in the HUD like jetpack
/// fuel) and hands off to the wall-jump system when the player leaps away, so
/// climbing never costs the double-jump-off-buildings verb.
#[derive(Component, Debug, Clone)]
pub struct ClimbState {
    pub is_climbing: bool,
    /// The climb bar. Sized for scaling real mountain faces.
    pub energy: f32,
    pub max_energy: f32,
    pub drain_per_sec: f32,
    /// Regen while grounded (not clinging).
    pub regen_per_sec: f32,
    /// Vertical climb rate (same unit scale as `walk_speed`).
    pub climb_speed: f32,
    /// Sideways shimmy rate along the wall.
    pub lateral_speed: f32,
    /// Minimum energy required to start a climb (prevents micro-grabs).
    pub min_start_energy: f32,
    /// Upward+forward boost applied when the wall ends mid-climb (ledge vault).
    pub vault_boost: f32,
}

impl Default for ClimbState {
    fn default() -> Self {
        Self {
            is_climbing: false,
            energy: 420.0,
            max_energy: 420.0,
            drain_per_sec: 26.0,
            regen_per_sec: 55.0,
            climb_speed: 0.30,
            lateral_speed: 0.22,
            min_start_energy: 30.0,
            vault_boost: 0.5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FlightMode {
    #[default]
    Grounded,
    Jump,
    Fall,
    Glide,
    Hover,
    JetBoost,
    AirDash,
    Slam,
    Hoverboard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TraversalMode {
    #[default]
    Grapple,
    HoverJet,
    Flight,
    Hoverboard,
}

impl TraversalMode {
    pub fn label(self) -> &'static str {
        match self {
            TraversalMode::Grapple => "Grapple",
            TraversalMode::HoverJet => "Hover Jet",
            TraversalMode::Flight => "Flight",
            TraversalMode::Hoverboard => "Rocket Hoverboard",
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct TraversalModeState {
    pub active: TraversalMode,
    pub hoverboard_speed_mult: f32,
    pub hoverboard_air_control_mult: f32,
    pub hoverboard_rocket_forward_mult: f32,
    pub hoverboard_rocket_lift_mult: f32,
    pub hoverboard_rocket_fuel_mult: f32,
    pub hoverboard_jump_mult: f32,
    pub hoverboard_gravity_mult: f32,
    pub hoverboard_uphill_assist: f32,
    pub hoverboard_manual_boost_mult: f32,
    pub hoverboard_manual_boost_duration: f32,
}

impl Default for TraversalModeState {
    fn default() -> Self {
        Self {
            active: TraversalMode::Grapple,
            hoverboard_speed_mult: 1.65,
            hoverboard_air_control_mult: 1.45,
            hoverboard_rocket_forward_mult: 1.28,
            hoverboard_rocket_lift_mult: 0.82,
            hoverboard_rocket_fuel_mult: 0.68,
            hoverboard_jump_mult: 1.32,
            hoverboard_gravity_mult: 0.42,
            hoverboard_uphill_assist: 8.5,
            hoverboard_manual_boost_mult: 2.35,
            hoverboard_manual_boost_duration: 0.72,
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct BoardBoostState {
    pub timer: f32,
    pub speed_mult: f32,
    pub direction: Vec3,
    pub manual_cooldown: f32,
    /// Short suspension squash/rebound after an airborne board landing.
    pub landing_timer: f32,
    pub airborne_time: f32,
    /// 0–1 proximity used by the board pose while descent assist is active.
    pub landing_approach: f32,
    /// Brief contact memory bridges road-deck seams at racing speed. It is
    /// ignored while rising or jumping, so real airtime remains immediate.
    pub ground_contact_grace: f32,
}

impl Default for BoardBoostState {
    fn default() -> Self {
        Self {
            timer: 0.0,
            speed_mult: 1.0,
            direction: Vec3::ZERO,
            manual_cooldown: 0.0,
            landing_timer: 0.0,
            airborne_time: 0.0,
            landing_approach: 0.0,
            ground_contact_grace: 0.0,
        }
    }
}

// ── Grapple Hook ─────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GrappleHookMode {
    #[default]
    Ready,
    Windup,
    Searching,
    Swinging,
    Zipping,
    Recovering,
    Cooldown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GrappleTargetKind {
    #[default]
    Denied,
    SwingPoint,
    ZipPoint,
    MountainPull,
    EnemyPull,
    UtilityPull,
    BossWeakPoint,
    RouteSocket,
    BroadSurface,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct GrappleSocket {
    pub kind: GrappleTargetKind,
    pub radius: f32,
    pub priority: f32,
}

impl GrappleSocket {
    pub fn new(kind: GrappleTargetKind) -> Self {
        Self {
            kind,
            radius: 2.5,
            priority: 1.0,
        }
    }

    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radius = radius.max(0.1);
        self
    }

    pub fn with_priority(mut self, priority: f32) -> Self {
        self.priority = priority;
        self
    }
}

#[derive(Component, Debug, Clone)]
pub struct GrappleHookState {
    pub mode: GrappleHookMode,
    pub attach_point: Option<Vec3>,
    pub attach_normal: Vec3,
    pub target_entity: Option<Entity>,
    pub target_kind: GrappleTargetKind,
    pub cable_length: f32,
    pub min_cable_length: f32,
    pub max_cable_length: f32,
    pub swing_spring: f32,
    pub swing_damping: f32,
    pub zip_speed: f32,
    pub mountain_pull_speed: f32,
    pub attack_pull_speed: f32,
    pub arrival_radius: f32,
    pub heat: f32,
    pub max_heat: f32,
    pub heat_per_fire: f32,
    pub heat_cool_rate: f32,
    pub windup_time: f32,
    pub windup_timer: f32,
    pub recovery_time: f32,
    pub recovery_timer: f32,
    pub cooldown: f32,
    pub cooldown_timer: f32,
}

impl Default for GrappleHookState {
    fn default() -> Self {
        Self {
            mode: GrappleHookMode::Ready,
            attach_point: None,
            attach_normal: Vec3::Y,
            target_entity: None,
            target_kind: GrappleTargetKind::Denied,
            cable_length: 18.0,
            min_cable_length: 5.0,
            max_cable_length: 96.0,
            swing_spring: 24.0,
            swing_damping: 4.8,
            zip_speed: 2.4,
            mountain_pull_speed: 3.2,
            attack_pull_speed: 1.65,
            arrival_radius: 1.4,
            heat: 0.0,
            max_heat: 100.0,
            heat_per_fire: 14.0,
            heat_cool_rate: 24.0,
            windup_time: 0.14,
            windup_timer: 0.0,
            recovery_time: 0.10,
            recovery_timer: 0.0,
            cooldown: 0.32,
            cooldown_timer: 0.0,
        }
    }
}

impl GrappleHookState {
    pub fn is_ready(&self) -> bool {
        self.mode == GrappleHookMode::Ready
            && self.cooldown_timer <= 0.0
            && self.heat < self.max_heat
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.mode,
            GrappleHookMode::Windup
                | GrappleHookMode::Searching
                | GrappleHookMode::Swinging
                | GrappleHookMode::Zipping
        )
    }

    pub fn wants_animation_pose(&self) -> bool {
        matches!(
            self.mode,
            GrappleHookMode::Windup
                | GrappleHookMode::Searching
                | GrappleHookMode::Swinging
                | GrappleHookMode::Zipping
                | GrappleHookMode::Recovering
        )
    }

    pub fn request_fire(&mut self) -> bool {
        if !self.is_ready() {
            return false;
        }
        self.mode = GrappleHookMode::Windup;
        self.windup_timer = self.windup_time;
        self.attach_point = None;
        self.target_entity = None;
        self.target_kind = GrappleTargetKind::Denied;
        self.heat = (self.heat + self.heat_per_fire).min(self.max_heat);
        true
    }

    pub fn tick_foundation(&mut self, dt: f32) {
        let dt = dt.max(0.0);
        self.cooldown_timer = (self.cooldown_timer - dt).max(0.0);
        self.heat = (self.heat - self.heat_cool_rate * dt).max(0.0);
        match self.mode {
            GrappleHookMode::Windup => {
                self.windup_timer = (self.windup_timer - dt).max(0.0);
                if self.windup_timer <= 0.0 {
                    self.mode = GrappleHookMode::Searching;
                }
            }
            GrappleHookMode::Recovering => {
                self.recovery_timer = (self.recovery_timer - dt).max(0.0);
                if self.recovery_timer <= 0.0 {
                    self.mode = if self.cooldown_timer <= 0.0 {
                        GrappleHookMode::Ready
                    } else {
                        GrappleHookMode::Cooldown
                    };
                }
            }
            GrappleHookMode::Cooldown if self.cooldown_timer <= 0.0 => {
                self.mode = GrappleHookMode::Ready;
            }
            _ => {}
        }
        self.cable_length = self
            .cable_length
            .clamp(self.min_cable_length, self.max_cable_length);
    }

    pub fn attach(
        &mut self,
        point: Vec3,
        normal: Vec3,
        target_entity: Option<Entity>,
        target_kind: GrappleTargetKind,
        mode: GrappleHookMode,
        current_position: Vec3,
    ) {
        self.attach_point = Some(point);
        self.attach_normal = normal.normalize_or_zero();
        self.target_entity = target_entity;
        self.target_kind = target_kind;
        self.mode = mode;
        self.cable_length = current_position
            .distance(point)
            .clamp(self.min_cable_length, self.max_cable_length);
        self.recovery_timer = 0.0;
    }

    pub fn begin_recovery(&mut self) {
        self.mode = GrappleHookMode::Recovering;
        self.recovery_timer = self.recovery_time;
        self.cooldown_timer = self.cooldown;
        self.attach_point = None;
        self.target_entity = None;
        self.target_kind = GrappleTargetKind::Denied;
    }
}

// ── Dodge ─────────────────────────────────────────────────────────────────────
#[derive(Component, Debug, Clone, Default)]
pub struct DodgeState {
    pub is_dodging: bool,
    pub dodge_timer: f32,
    pub cooldown_timer: f32,
    pub dodge_duration: f32,
    pub dodge_cooldown: f32,
    pub dodge_cost: f32,
    pub dodge_speed: f32,
    pub dodge_direction: Vec3,
}

impl DodgeState {
    pub fn new() -> Self {
        Self {
            dodge_duration: 0.3,
            dodge_cooldown: 0.5,
            dodge_cost: 20.0,
            dodge_speed: 1.2,
            ..default()
        }
    }
}

// ── Parry ─────────────────────────────────────────────────────────────────────
#[derive(Component, Debug, Clone, Default)]
pub struct ParryState {
    pub is_parrying: bool,
    pub parry_timer: f32,
    pub cooldown_timer: f32,
    pub parry_window: f32,
    pub parry_cooldown: f32,
}

impl ParryState {
    pub fn new() -> Self {
        Self {
            parry_window: 0.2,
            parry_cooldown: 1.0,
            ..default()
        }
    }
}

// ── Player State Machine ──────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PlayerState {
    #[default]
    Idle,
    Moving,
    Sprinting,
    Dodging,
    Attacking,
    Stunned,
    Dead,
    Jetpack,
    WallSliding,
    Hanging,
    Climbing,
    Grappling,
    Swimming,
}

#[derive(Component, Debug, Clone, Default)]
pub struct PlayerStateMachine {
    pub current: PlayerState,
    pub previous: Option<PlayerState>,
    pub timer: f32,
}

impl PlayerStateMachine {
    /// Attempt a transition. Returns `true` if the transition was valid.
    pub fn transition(&mut self, next: PlayerState) -> bool {
        use PlayerState::*;
        let allowed = match self.current {
            Idle => matches!(
                next,
                Moving
                    | Sprinting
                    | Dodging
                    | Attacking
                    | Stunned
                    | Dead
                    | Jetpack
                    | WallSliding
                    | Hanging
                    | Climbing
                    | Grappling
            ),
            Moving => matches!(
                next,
                Idle | Sprinting
                    | Dodging
                    | Attacking
                    | Stunned
                    | Dead
                    | Jetpack
                    | WallSliding
                    | Hanging
                    | Climbing
                    | Grappling
            ),
            Sprinting => matches!(
                next,
                Idle | Moving
                    | Dodging
                    | Attacking
                    | Stunned
                    | Dead
                    | Jetpack
                    | WallSliding
                    | Hanging
                    | Climbing
                    | Grappling
            ),
            Dodging => matches!(next, Idle | Moving | Sprinting | Stunned | Dead),
            Attacking => matches!(next, Idle | Moving | Dodging | Stunned | Dead | Grappling),
            Stunned => matches!(next, Idle | Dead),
            Dead => false,
            Jetpack => matches!(
                next,
                Idle | Moving | Stunned | Dead | WallSliding | Hanging | Climbing | Grappling
            ),
            WallSliding => matches!(
                next,
                Idle | Moving | Jetpack | Hanging | Climbing | Stunned | Dead | Grappling
            ),
            Hanging => matches!(
                next,
                Idle | Moving | Jetpack | Climbing | Stunned | Dead | Grappling
            ),
            Climbing => matches!(
                next,
                Idle | Moving | Jetpack | Hanging | Stunned | Dead | Grappling
            ),
            Grappling => matches!(
                next,
                Idle | Moving
                    | Sprinting
                    | Dodging
                    | Attacking
                    | Stunned
                    | Dead
                    | Jetpack
                    | WallSliding
                    | Hanging
                    | Climbing
                    | Swimming
            ),
            Swimming => matches!(next, Idle | Moving | Stunned | Dead | Jetpack),
        };
        if allowed {
            self.previous = Some(self.current);
            self.current = next;
            self.timer = 0.0;
        }
        allowed
    }

    /// Force-transition regardless of state rules (e.g. death, reset).
    pub fn force(&mut self, next: PlayerState) {
        self.previous = Some(self.current);
        self.current = next;
        self.timer = 0.0;
    }
}

// ── Pitch tracker (on camera child) ──────────────────────────────────────────
#[derive(Component, Default)]
pub struct CameraPitch(pub f32);

// ── Invulnerability flash timer ────────────────────────────────────────────────
#[derive(Component, Default)]
pub struct InvulnerabilityFlash {
    pub timer: f32,
    pub duration: f32,
}

// ── Player Index ──────────────────────────────────────────────────────────────
/// 0 = P1, 1 = P2, 2 = P3, 3 = P4.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PlayerIndex(pub u8);

// ── Per-Player Input ──────────────────────────────────────────────────────────
/// Written by InputPlugin each PreUpdate. One instance per player entity.
/// P1 = keyboard + mouse + gamepad 0. P2–P4 = gamepad 1–3.
#[derive(Component, Default, Debug, Clone)]
pub struct PlayerInput {
    pub move_axis: Vec2,
    pub look_delta: Vec2,
    pub fire: bool,
    pub aim: bool,
    pub sprint: bool,
    pub jetpack: bool,
    pub jump: bool,
    pub fire_just: bool,
    pub dodge: bool,
    pub reload: bool,
    pub use_quick_item: bool,
    pub parry: bool,
    pub interact: bool,
    pub grapple: bool,
    pub grapple_just: bool,
    pub melee_light: bool,
    pub melee_heavy: bool,
    pub crafting: bool,
    pub loadout_menu: bool,
    pub pause: bool,
    pub weapon_next: bool,
    pub weapon_prev: bool,
    /// Cycle the equipped special weapon (D-pad up/down). Wraps through the
    /// four special slots and back to "no special" so the primary weapon is
    /// always one press away.
    pub special_next: bool,
    pub special_prev: bool,
    pub enter_vehicle: bool,
    pub open_map: bool,
    pub sabre_toggle: bool,
    /// Per-frame Star Sabre slash request. Controller RB is intentionally
    /// separate from RT ranged fire so both weapons remain available.
    pub sabre_attack: bool,
    /// Per-frame request to cycle this player's active armor infusion.
    /// Negative selects the previous element; positive selects the next.
    pub armor_element_delta: i8,
    /// UI-only navigation survives modal gameplay-input capture.
    pub ui_vertical: f32,
    pub ui_up: bool,
    pub ui_down: bool,
    pub ui_left: bool,
    pub ui_right: bool,
    pub ui_confirm: bool,
    /// UI-only cancel/back request (Escape or East/B/Circle).
    pub ui_cancel: bool,
    pub weapon_slot: Option<usize>,
    pub traversal_mode_switch: Option<TraversalMode>,
    /// Digit 7/8/9/0 or Select + D-pad → slots 0–3.
    pub special_slot: Option<u8>,
    pub gamepad_active: bool,
}

// ── Per-player aiming ────────────────────────────────────────────────────────
/// Authoritative ranged-weapon aim computed from this player's camera.
///
/// HUD reticles and weapon systems consume this same component so presentation
/// cannot disagree with the projectile's actual direction.
#[derive(Component, Debug, Clone, Copy)]
pub struct AimSolution {
    pub camera_origin: Vec3,
    pub muzzle_origin: Vec3,
    pub aim_point: Vec3,
    pub direction: Vec3,
    pub target: Option<Entity>,
    pub actively_aiming: bool,
    pub obstructed: bool,
}

impl Default for AimSolution {
    fn default() -> Self {
        Self {
            camera_origin: Vec3::ZERO,
            muzzle_origin: Vec3::ZERO,
            aim_point: Vec3::NEG_Z * 120.0,
            direction: Vec3::NEG_Z,
            target: None,
            actively_aiming: false,
            obstructed: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AimReticleState {
    Idle,
    Aiming,
    Target,
    Locked,
    Obstructed,
    Charging,
}

impl AimSolution {
    pub fn reticle_state(self, charging: bool) -> AimReticleState {
        if charging {
            AimReticleState::Charging
        } else if self.target.is_some() && self.actively_aiming {
            AimReticleState::Locked
        } else if self.target.is_some() {
            AimReticleState::Target
        } else if self.obstructed {
            AimReticleState::Obstructed
        } else if self.actively_aiming {
            AimReticleState::Aiming
        } else {
            AimReticleState::Idle
        }
    }
}

// ── Camera-player link ────────────────────────────────────────────────────────
/// Entity of this player's `PlayerCamera` child. Set during spawn.
#[derive(Component, Debug, Clone, Copy)]
pub struct PlayerCameraRef(pub Entity);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grapple_request_enters_searching_then_recovers() {
        let mut grapple = GrappleHookState::default();

        assert!(grapple.is_ready());
        assert!(grapple.request_fire());
        assert_eq!(grapple.mode, GrappleHookMode::Windup);
        assert!(!grapple.request_fire());

        grapple.tick_foundation(grapple.windup_time + 0.01);
        assert_eq!(grapple.mode, GrappleHookMode::Searching);
        assert!(grapple.heat > 0.0);

        grapple.begin_recovery();
        assert!(grapple.cooldown_timer > 0.0);

        grapple.tick_foundation(grapple.recovery_time + grapple.cooldown + 0.01);
        assert_eq!(grapple.mode, GrappleHookMode::Ready);
        assert!(grapple.is_ready());
    }

    #[test]
    fn grapple_tick_clamps_cable_length_to_tuning_limits() {
        let mut grapple = GrappleHookState {
            cable_length: 999.0,
            ..GrappleHookState::default()
        };
        grapple.tick_foundation(0.0);
        assert_eq!(grapple.cable_length, grapple.max_cable_length);

        grapple.cable_length = 0.2;
        grapple.tick_foundation(0.0);
        assert_eq!(grapple.cable_length, grapple.min_cable_length);
    }

    #[test]
    fn traversal_mode_labels_are_stable() {
        assert_eq!(TraversalMode::Grapple.label(), "Grapple");
        assert_eq!(TraversalMode::Hoverboard.label(), "Rocket Hoverboard");
    }

    #[test]
    fn hoverboard_tuning_supports_fast_air_control() {
        let traversal = TraversalModeState::default();

        assert!(traversal.hoverboard_speed_mult >= 1.6);
        assert!(traversal.hoverboard_air_control_mult >= 1.4);
        assert!(traversal.hoverboard_rocket_forward_mult > 1.0);
        assert!(traversal.hoverboard_rocket_lift_mult > 0.0);
        assert!(traversal.hoverboard_rocket_fuel_mult < 1.0);
        assert!(traversal.hoverboard_jump_mult > 1.0);
        assert!(traversal.hoverboard_gravity_mult < 0.5);
        assert!(traversal.hoverboard_uphill_assist > 5.0);
        assert!(traversal.hoverboard_manual_boost_mult > 2.0);
    }

    #[test]
    fn three_jump_taps_toggle_hover_preference() {
        let mut jetpack = JetpackState::default();
        assert!(!jetpack.register_jump_tap());
        assert!(!jetpack.register_jump_tap());
        assert!(jetpack.register_jump_tap());
        assert!(jetpack.hover_mode_enabled);

        assert!(!jetpack.register_jump_tap());
        assert!(!jetpack.register_jump_tap());
        assert!(jetpack.register_jump_tap());
        assert!(!jetpack.hover_mode_enabled);
    }

    #[test]
    fn aim_reticle_state_prioritizes_charge_and_lock_feedback() {
        let mut aim = AimSolution {
            actively_aiming: true,
            obstructed: true,
            ..default()
        };
        assert_eq!(aim.reticle_state(false), AimReticleState::Obstructed);

        let mut world = World::new();
        aim.target = Some(world.spawn_empty().id());
        assert_eq!(aim.reticle_state(false), AimReticleState::Locked);
        assert_eq!(aim.reticle_state(true), AimReticleState::Charging);
    }

    #[test]
    fn water_traversal_starts_dry_with_a_full_air_reserve() {
        let water = WaterTraversalState::default();

        assert!(!water.swimming);
        assert!(!water.submerged);
        assert_eq!(water.breath, 12.0);
        assert_eq!(water.breath, water.max_breath);
        assert!(!water.wake_requested);
    }
}
