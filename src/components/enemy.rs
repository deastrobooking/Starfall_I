use bevy::prelude::*;

// ── Enemy Type ────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum EnemyType {
    Drone,
    Soldier,
    Heavy,
    SpikeAlien,
    Hybrid,
}

impl EnemyType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EnemyType::Drone => "rift scout",
            EnemyType::Soldier => "dimension invader",
            EnemyType::Heavy => "dragon brute",
            EnemyType::SpikeAlien => "spike alien",
            EnemyType::Hybrid => "rift champion",
        }
    }
}

// ── Enemy Config (static stats) ───────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct EnemyConfig {
    pub max_health: f32,
    pub attack_damage: f32,
    pub defense: f32,
    pub movement_speed: f32,
    pub attack_cooldown: f32,
    pub knockback_force: f32,
    pub experience_value: u32,
    pub detection_range: f32,
    pub chase_range: f32,
    pub attack_range: f32,
    pub patrol_speed: f32,
    pub chase_speed: f32,
    pub credits: u32,
}

impl EnemyConfig {
    pub fn for_type(t: EnemyType) -> Self {
        match t {
            EnemyType::Drone => Self {
                max_health: 50.0,
                attack_damage: 8.0,
                defense: 3.0,
                movement_speed: 8.0,
                attack_cooldown: 2.0,
                knockback_force: 200.0,
                experience_value: 15,
                detection_range: 25.0,
                chase_range: 35.0,
                attack_range: 15.0,
                patrol_speed: 0.06,
                chase_speed: 0.12,
                credits: 10,
            },
            EnemyType::Soldier => Self {
                max_health: 100.0,
                attack_damage: 15.0,
                defense: 5.0,
                movement_speed: 4.0,
                attack_cooldown: 1.8,
                knockback_force: 300.0,
                experience_value: 25,
                detection_range: 20.0,
                chase_range: 30.0,
                attack_range: 5.0,
                patrol_speed: 0.05,
                chase_speed: 0.10,
                credits: 20,
            },
            EnemyType::Heavy => Self {
                max_health: 300.0,
                attack_damage: 25.0,
                defense: 15.0,
                movement_speed: 2.0,
                attack_cooldown: 2.5,
                knockback_force: 600.0,
                experience_value: 50,
                detection_range: 15.0,
                chase_range: 25.0,
                attack_range: 8.0,
                patrol_speed: 0.03,
                chase_speed: 0.07,
                credits: 50,
            },
            EnemyType::SpikeAlien => Self {
                max_health: 80.0,
                attack_damage: 20.0,
                defense: 4.0,
                movement_speed: 6.0,
                attack_cooldown: 1.2,
                knockback_force: 250.0,
                experience_value: 20,
                detection_range: 18.0,
                chase_range: 28.0,
                attack_range: 4.0,
                patrol_speed: 0.07,
                chase_speed: 0.13,
                credits: 30,
            },
            EnemyType::Hybrid => Self {
                max_health: 1000.0,
                attack_damage: 40.0,
                defense: 20.0,
                movement_speed: 3.0,
                attack_cooldown: 1.5,
                knockback_force: 800.0,
                experience_value: 200,
                detection_range: 30.0,
                chase_range: 45.0,
                attack_range: 10.0,
                patrol_speed: 0.04,
                chase_speed: 0.09,
                credits: 100,
            },
        }
    }
}

// ── Enemy AI State Machine ────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EnemyAIState {
    #[default]
    Idle,
    Patrol,
    Chase,
    Attack,
    Stunned,
    Dead,
}

#[derive(Component, Debug, Clone, Default)]
pub struct EnemyStateMachine {
    pub current: EnemyAIState,
    pub previous: Option<EnemyAIState>,
    pub timer: f32,
}

impl EnemyStateMachine {
    pub fn transition(&mut self, next: EnemyAIState) -> bool {
        use EnemyAIState::*;
        let allowed = match self.current {
            Idle => matches!(next, Patrol | Chase | Stunned | Dead),
            Patrol => matches!(next, Idle | Chase | Stunned | Dead),
            Chase => matches!(next, Patrol | Attack | Stunned | Dead),
            Attack => matches!(next, Chase | Stunned | Dead),
            Stunned => matches!(next, Chase | Idle | Dead),
            Dead => false,
        };
        if allowed {
            self.previous = Some(self.current);
            self.current = next;
            self.timer = 0.0;
        }
        allowed
    }

    pub fn force(&mut self, next: EnemyAIState) {
        self.previous = Some(self.current);
        self.current = next;
        self.timer = 0.0;
    }
}

// ── Enemy Component ───────────────────────────────────────────────────────────
#[derive(Component, Debug, Clone)]
pub struct Enemy {
    pub enemy_type: EnemyType,
    pub config: EnemyConfig,
    /// Patrol destination (random point near spawn)
    pub patrol_target: Vec3,
    /// Origin spawn point
    pub spawn_origin: Vec3,
    pub attack_cooldown_timer: f32,
    pub velocity: Vec3,
    /// Scale multiplier from wave difficulty
    pub difficulty_scale: f32,
}

impl Enemy {
    pub fn new(enemy_type: EnemyType, position: Vec3, difficulty_scale: f32) -> Self {
        let config = EnemyConfig::for_type(enemy_type);
        Self {
            enemy_type,
            config,
            patrol_target: position,
            spawn_origin: position,
            attack_cooldown_timer: 0.0,
            velocity: Vec3::ZERO,
            difficulty_scale,
        }
    }

    pub fn scaled_health(&self) -> f32 {
        self.config.max_health * self.difficulty_scale
    }

    pub fn scaled_damage(&self) -> f32 {
        self.config.attack_damage * self.difficulty_scale
    }
}

// ── Dead marker (entities with this are pending despawn) ─────────────────────
#[derive(Component, Default)]
pub struct DeadEnemy {
    pub despawn_timer: f32,
}

/// Marks a boss-tier enemy (spawned every 5th wave).
#[derive(Component, Default)]
pub struct BossEnemy;

/// Adds flying strafe/orbit behavior to drone enemies.
#[derive(Component, Debug, Clone)]
pub struct FlyingDrone {
    pub altitude: f32,
    pub orbit_radius: f32,
    pub orbit_phase: f32,
    pub fire_timer: f32,
}

impl FlyingDrone {
    pub fn new(position: Vec3) -> Self {
        Self {
            altitude: 5.0 + (position.x * 0.07).sin().abs() * 2.0,
            orbit_radius: 9.0 + (position.z * 0.05).cos().abs() * 5.0,
            orbit_phase: position.x * 0.11 + position.z * 0.07,
            fire_timer: 0.8,
        }
    }
}

/// Boss controller for dragon-domain bosses.
#[derive(Component, Debug, Clone)]
pub struct DragonBoss {
    pub home: Vec3,
    pub phase: u8,
    pub orbit_angle: f32,
    pub fireball_timer: f32,
    pub breath_timer: f32,
    pub slam_timer: f32,
}

impl DragonBoss {
    pub fn new(position: Vec3) -> Self {
        Self {
            home: position,
            phase: 1,
            orbit_angle: 0.0,
            fireball_timer: 1.5,
            breath_timer: 3.0,
            slam_timer: 5.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyProjectileKind {
    Laser,
    Fireball,
}

/// Projectile fired by enemy drones and dragon bosses.
#[derive(Component, Debug, Clone)]
pub struct EnemyProjectile {
    pub kind: EnemyProjectileKind,
    pub damage: f32,
    pub speed: f32,
    pub direction: Vec3,
    pub lifetime: f32,
    pub hit_radius: f32,
    pub splash_radius: f32,
}

/// Short-lived enemy attack telegraph/impact visual.
#[derive(Component, Debug, Clone)]
pub struct EnemyAttackVfx {
    pub timer: f32,
}
