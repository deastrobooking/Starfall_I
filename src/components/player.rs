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
        }
    }
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
}

impl Default for JetpackState {
    fn default() -> Self {
        Self {
            fuel: 200.0,
            max_fuel: 200.0,
            force: 0.06,
            fuel_cost_per_sec: 20.0,
            regen_rate: 30.0,
            max_vertical_vel: 0.35,
            is_active: false,
        }
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
            ),
            Dodging => matches!(next, Idle | Moving | Sprinting | Stunned | Dead),
            Attacking => matches!(next, Idle | Moving | Dodging | Stunned | Dead),
            Stunned => matches!(next, Idle | Dead),
            Dead => false,
            Jetpack => matches!(next, Idle | Moving | Stunned | Dead | WallSliding | Hanging),
            WallSliding => matches!(next, Idle | Moving | Jetpack | Hanging | Stunned | Dead),
            Hanging => matches!(next, Idle | Moving | Jetpack | Stunned | Dead),
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
    /// Digit 7/8/9/0 or Select + D-pad → slots 0–3.
    pub special_slot: Option<u8>,
    pub gamepad_active: bool,
}

// ── Camera-player link ────────────────────────────────────────────────────────
/// Entity of this player's `PlayerCamera` child. Set during spawn.
#[derive(Component, Debug, Clone, Copy)]
pub struct PlayerCameraRef(pub Entity);
