use bevy::prelude::*;

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
    pub max_health: f32,
    pub armor: f32,
    pub max_armor: f32,
    pub stamina: f32,
    pub max_stamina: f32,
    pub credits: u32,
    pub experience: u32,
    pub level: u32,
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

impl PlayerStats {
    pub fn xp_for_next_level(&self) -> u32 {
        self.level * 100
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
        }
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

// ── Edge Grab / Wall Jump ────────────────────────────────────────────────────
#[derive(Component, Debug, Clone)]
pub struct EdgeGrabState {
    pub is_hanging: bool,
    pub wall_normal: Vec3,
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
            TraversalMode::Hoverboard => "Hoverboard",
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct TraversalModeState {
    pub active: TraversalMode,
    pub hoverboard_speed_mult: f32,
    pub hoverboard_air_control_mult: f32,
}

impl Default for TraversalModeState {
    fn default() -> Self {
        Self {
            active: TraversalMode::Grapple,
            hoverboard_speed_mult: 1.65,
            hoverboard_air_control_mult: 1.45,
        }
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct BoardBoostState {
    pub timer: f32,
    pub speed_mult: f32,
    pub direction: Vec3,
}

impl Default for BoardBoostState {
    fn default() -> Self {
        Self {
            timer: 0.0,
            speed_mult: 1.0,
            direction: Vec3::ZERO,
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
            ),
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
    pub parry: bool,
    pub interact: bool,
    pub grapple: bool,
    pub grapple_just: bool,
    pub melee_light: bool,
    pub melee_heavy: bool,
    pub crafting: bool,
    pub pause: bool,
    pub weapon_next: bool,
    pub weapon_prev: bool,
    pub enter_vehicle: bool,
    pub open_map: bool,
    pub sabre_toggle: bool,
    pub weapon_slot: Option<usize>,
    pub traversal_mode_switch: Option<TraversalMode>,
    /// Digit 7/8/9/0 or Select + D-pad → slots 0–3.
    pub special_slot: Option<u8>,
    pub gamepad_active: bool,
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
        assert_eq!(TraversalMode::Hoverboard.label(), "Hoverboard");
    }

    #[test]
    fn hoverboard_tuning_supports_fast_air_control() {
        let traversal = TraversalModeState::default();

        assert!(traversal.hoverboard_speed_mult >= 1.6);
        assert!(traversal.hoverboard_air_control_mult >= 1.4);
    }
}
