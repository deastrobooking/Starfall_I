#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
use bevy::prelude::*;

// ── Enemy Type ────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum EnemyType {
    Drone,
    SpyDrone,
    Soldier,
    Heavy,
    Tank,
    SpikeAlien,
    Hybrid,
}

/// Readable visual/combat castes used by the humanoid dragon factions.
///
/// These deliberately map onto the existing enemy statlines so authored
/// encounters and save data remain compatible while the faction gets its own
/// silhouettes.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DragonCaste {
    LizardLegionary,
    RaptorRunner,
    HumanoidDragon,
}

impl DragonCaste {
    pub fn for_enemy(enemy_type: EnemyType) -> Self {
        match enemy_type {
            EnemyType::Soldier | EnemyType::Drone | EnemyType::SpyDrone => Self::LizardLegionary,
            EnemyType::SpikeAlien => Self::RaptorRunner,
            EnemyType::Heavy | EnemyType::Tank | EnemyType::Hybrid => Self::HumanoidDragon,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::LizardLegionary => "Lizard Legionary",
            Self::RaptorRunner => "Raptor Runner",
            Self::HumanoidDragon => "Humanoid Dragon",
        }
    }
}

impl EnemyType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EnemyType::Drone => "Scallarian scout",
            EnemyType::SpyDrone => "Scallarian spy drone",
            EnemyType::Soldier => "Scallarian invader",
            EnemyType::Heavy => "dragon brute",
            EnemyType::Tank => "Scallarian siege tank",
            EnemyType::SpikeAlien => "Scallarian spike alien",
            EnemyType::Hybrid => "Scallarian rift champion",
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
    /// Telegraph windup: seconds between committing to a melee swing (ring
    /// telegraph spawns) and the hit volume resolving. Dodgeable window —
    /// heavier enemies wind up longer.
    pub attack_windup: f32,
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
                attack_windup: 0.28,
                knockback_force: 200.0,
                experience_value: 15,
                detection_range: 25.0,
                chase_range: 35.0,
                attack_range: 15.0,
                patrol_speed: 0.06,
                chase_speed: 0.12,
                credits: 10,
            },
            EnemyType::SpyDrone => Self {
                max_health: 65.0,
                attack_damage: 0.0,
                defense: 2.0,
                movement_speed: 10.0,
                attack_cooldown: 99.0,
                attack_windup: 0.3,
                knockback_force: 120.0,
                experience_value: 35,
                detection_range: 0.0,
                chase_range: 0.0,
                attack_range: 0.0,
                patrol_speed: 0.0,
                chase_speed: 0.0,
                credits: 25,
            },
            EnemyType::Soldier => Self {
                max_health: 100.0,
                attack_damage: 15.0,
                defense: 5.0,
                movement_speed: 4.0,
                attack_cooldown: 1.8,
                attack_windup: 0.32,
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
                attack_windup: 0.5,
                knockback_force: 600.0,
                experience_value: 50,
                detection_range: 15.0,
                chase_range: 25.0,
                attack_range: 8.0,
                patrol_speed: 0.03,
                chase_speed: 0.07,
                credits: 50,
            },
            // Heavy Water's tank is siege artillery, not a contact brawler:
            // it trundles into a long 28 m firing envelope, parks, and shells
            // the party on a deliberately slow cadence.
            EnemyType::Tank => Self {
                max_health: 600.0,
                attack_damage: 45.0,
                defense: 18.0,
                movement_speed: 1.8,
                attack_cooldown: 3.5,
                attack_windup: 0.65,
                knockback_force: 1100.0,
                experience_value: 120,
                detection_range: 55.0,
                chase_range: 80.0,
                attack_range: 28.0,
                patrol_speed: 0.025,
                chase_speed: 0.045,
                credits: 90,
            },
            EnemyType::SpikeAlien => Self {
                max_health: 80.0,
                attack_damage: 20.0,
                defense: 4.0,
                movement_speed: 6.0,
                attack_cooldown: 1.2,
                attack_windup: 0.22,
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
                attack_windup: 0.42,
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
    /// Current patrol destination (one of patrol_waypoints)
    pub patrol_target: Vec3,
    /// Origin spawn point
    pub spawn_origin: Vec3,
    pub attack_cooldown_timer: f32,
    /// Remaining telegraph time of a committed melee swing; `None` when not
    /// winding up. The hit volume resolves when it reaches zero.
    pub attack_windup_timer: Option<f32>,
    pub velocity: Vec3,
    /// Scale multiplier from wave difficulty
    pub difficulty_scale: f32,
    /// Three patrol waypoints generated at spawn; cycled in Idle→Patrol transitions
    pub patrol_waypoints: Vec<Vec3>,
    pub patrol_index: usize,
}

impl Enemy {
    pub fn new(enemy_type: EnemyType, position: Vec3, difficulty_scale: f32) -> Self {
        let config = EnemyConfig::for_type(enemy_type);
        // Three deterministic waypoints at 120° intervals around spawn origin.
        let base_angle = position.x * 0.013 + position.z * 0.009;
        let radius = 12.0 + (position.x.abs() % 9.0);
        let patrol_waypoints: Vec<Vec3> = (0..3)
            .map(|i| {
                let a = base_angle + std::f32::consts::TAU * (i as f32 / 3.0);
                position + Vec3::new(a.cos() * radius, 0.0, a.sin() * radius)
            })
            .collect();
        Self {
            enemy_type,
            config,
            patrol_target: position,
            spawn_origin: position,
            attack_cooldown_timer: 0.0,
            attack_windup_timer: None,
            velocity: Vec3::ZERO,
            difficulty_scale,
            patrol_waypoints,
            patrol_index: 0,
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
    pub speed_multiplier: f32,
    pub fire_interval_multiplier: f32,
    /// Heavy fighters release their three bolts over several frames instead
    /// of spawning an unreadable stack of projectiles in one instant.
    pub burst_shots_remaining: u8,
    pub burst_timer: f32,
}

impl FlyingDrone {
    pub fn new(position: Vec3) -> Self {
        Self::for_variant(position, DroneVariant::Scout)
    }

    pub fn for_variant(position: Vec3, variant: DroneVariant) -> Self {
        let (altitude, orbit_radius, speed_multiplier, fire_interval_multiplier) = match variant {
            DroneVariant::Scout => (
                5.0 + (position.x * 0.07).sin().abs() * 2.0,
                9.0 + (position.z * 0.05).cos().abs() * 5.0,
                1.0,
                1.0,
            ),
            DroneVariant::HeavyFighter => (8.5, 13.0, 0.68, 1.35),
            DroneVariant::SiegeGunship => (11.5, 17.0, 0.42, 1.8),
        };
        Self {
            altitude,
            orbit_radius,
            orbit_phase: position.x * 0.11 + position.z * 0.07,
            fire_timer: 0.8 * fire_interval_multiplier,
            speed_multiplier,
            fire_interval_multiplier,
            burst_shots_remaining: 0,
            burst_timer: 0.0,
        }
    }
}

/// Visual and combat class for true flying craft. Humanoid enemies never
/// receive this component, which keeps the ground/air silhouette rule clear.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum DroneVariant {
    Scout,
    HeavyFighter,
    SiegeGunship,
}

/// A non-combat Scallarian surveillance drone hiding above peaceful cities.
/// It is damageable like an enemy, but city-spy systems own its patrol/reward.
#[derive(Component, Debug, Clone)]
pub struct CitySpyDrone {
    pub id: &'static str,
    pub label: &'static str,
    pub reward_id: &'static str,
    pub home: Vec3,
    pub radius: f32,
    pub altitude: f32,
    pub angular_speed: f32,
    pub phase: f32,
    pub bob: f32,
    pub credits: u32,
    pub experience: u32,
    pub armor: u32,
    pub data_spawned: bool,
    /// Set once this drone has applied intel pressure to FinalWarRegistry.
    pub pressure_applied: bool,
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

/// Scallarian rift champion boss: a teleporting summoner. Blinks around the
/// arena, fires rift-laser volleys, and tears open portals for reinforcements.
/// Phases escalate at 66%/33% health like [`DragonBoss`].
#[derive(Component, Debug, Clone)]
pub struct RiftBoss {
    pub home: Vec3,
    pub phase: u8,
    pub volley_timer: f32,
    pub blink_timer: f32,
    pub summon_timer: f32,
    /// Deterministic angle cursor for blink/summon placement (no RNG needed).
    pub weave_angle: f32,
}

impl RiftBoss {
    pub fn new(position: Vec3) -> Self {
        Self {
            home: position,
            phase: 1,
            volley_timer: 2.2,
            blink_timer: 5.0,
            summon_timer: 7.5,
            weave_angle: 0.0,
        }
    }
}

/// Corrupted-human reactor-suit boss: a grounded brawler-artillery mech.
/// Strafes at standoff range, fires laser barrages, telegraphs a charging
/// dash that ends in a shockwave, and cycles an invulnerable shield in later
/// phases.
#[derive(Component, Debug, Clone)]
pub struct MechBoss {
    pub home: Vec3,
    pub phase: u8,
    pub barrage_timer: f32,
    pub charge_timer: f32,
    /// Remaining dash seconds while > 0 (the boss is committed to the charge).
    pub charging: f32,
    pub charge_dir: Vec3,
    pub shield_cycle_timer: f32,
    pub shielded_remaining: f32,
    pub strafe_dir: f32,
}

impl MechBoss {
    pub fn new(position: Vec3) -> Self {
        Self {
            home: position,
            phase: 1,
            barrage_timer: 2.0,
            charge_timer: 6.0,
            charging: 0.0,
            charge_dir: Vec3::ZERO,
            shield_cycle_timer: 8.0,
            shielded_remaining: 0.0,
            strafe_dir: 1.0,
        }
    }
}

/// Ambient boss-scale spider machine that patrols the Everest wilderness.
/// World generation owns its terrain-following locomotion while the standard
/// enemy pipeline owns damage, melee telegraphs, death, loot, and salvage.
#[derive(Component, Debug, Clone)]
pub struct MountainSpiderMech {
    pub home: Vec3,
    pub patrol_radius: f32,
    pub patrol_angle: f32,
    pub body_clearance: f32,
    pub gait_phase: f32,
}

/// Animated hip pivot belonging to one of a spider mech's eight legs.
#[derive(Component, Debug, Clone, Copy)]
pub struct SpiderMechLeg {
    pub mech: Entity,
    pub base_rotation: Quat,
    pub phase_offset: f32,
    pub side: f32,
}

/// Shared boss phase thresholds: 1 above 66% health, 2 above 33%, 3 below.
pub fn boss_phase(current: f32, max: f32) -> u8 {
    let ratio = (current / max.max(1.0)).clamp(0.0, 1.0);
    if ratio < 0.33 {
        3
    } else if ratio < 0.66 {
        2
    } else {
        1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyProjectileKind {
    Laser,
    Fireball,
    Shell,
}

/// Projectile fired by enemy drones, bosses, and siege platforms.
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

/// Optional steering brain for enemy missiles. Keeping it separate from
/// [`EnemyProjectile`] means ordinary lasers and tank shells pay no target
/// lookup or turn-rate cost.
#[derive(Component, Debug, Clone)]
pub struct EnemyHomingProjectile {
    pub target: Option<Entity>,
    pub turn_rate_radians: f32,
    pub acquisition_range: f32,
}

impl EnemyHomingProjectile {
    pub fn new(target: Entity) -> Self {
        Self {
            target: Some(target),
            turn_rate_radians: 2.8,
            acquisition_range: 140.0,
        }
    }
}

/// Short-lived enemy attack telegraph/impact visual.
#[derive(Component, Debug, Clone)]
pub struct EnemyAttackVfx {
    pub timer: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_enemy_type_telegraphs_its_melee_swing() {
        let types = [
            EnemyType::Drone,
            EnemyType::SpyDrone,
            EnemyType::Soldier,
            EnemyType::Heavy,
            EnemyType::Tank,
            EnemyType::SpikeAlien,
            EnemyType::Hybrid,
        ];
        for t in types {
            let config = EnemyConfig::for_type(t);
            assert!(
                config.attack_windup > 0.0,
                "{t:?} must have a dodgeable windup"
            );
        }
        // Heavier hitters wind up longer than the fastest scout.
        let spike = EnemyConfig::for_type(EnemyType::SpikeAlien).attack_windup;
        assert!(EnemyConfig::for_type(EnemyType::Heavy).attack_windup > spike);
        assert!(EnemyConfig::for_type(EnemyType::Tank).attack_windup > spike);
        assert!(EnemyConfig::for_type(EnemyType::Hybrid).attack_windup > spike);
    }

    #[test]
    fn tank_is_authored_as_long_range_siege_artillery() {
        let tank = EnemyConfig::for_type(EnemyType::Tank);
        let heavy = EnemyConfig::for_type(EnemyType::Heavy);
        assert_eq!(tank.max_health, 600.0);
        assert_eq!(tank.attack_damage, 45.0);
        assert_eq!(tank.defense, 18.0);
        assert_eq!(tank.attack_cooldown, 3.5);
        assert_eq!(tank.attack_range, 28.0);
        assert!(tank.attack_range > heavy.attack_range * 3.0);
        assert!(tank.chase_speed < heavy.chase_speed);
    }

    #[test]
    fn boss_phase_thresholds() {
        assert_eq!(boss_phase(100.0, 100.0), 1);
        assert_eq!(boss_phase(67.0, 100.0), 1);
        assert_eq!(boss_phase(65.0, 100.0), 2);
        assert_eq!(boss_phase(34.0, 100.0), 2);
        assert_eq!(boss_phase(32.0, 100.0), 3);
        assert_eq!(boss_phase(0.0, 100.0), 3);
        // Degenerate max never divides by zero.
        assert_eq!(boss_phase(0.0, 0.0), 3);
    }

    #[test]
    fn dragon_faction_castes_cover_each_existing_enemy_statline() {
        for enemy_type in [EnemyType::Soldier, EnemyType::Drone, EnemyType::SpyDrone] {
            assert_eq!(
                DragonCaste::for_enemy(enemy_type),
                DragonCaste::LizardLegionary
            );
        }
        assert_eq!(
            DragonCaste::for_enemy(EnemyType::SpikeAlien),
            DragonCaste::RaptorRunner
        );
        for enemy_type in [EnemyType::Heavy, EnemyType::Tank, EnemyType::Hybrid] {
            assert_eq!(
                DragonCaste::for_enemy(enemy_type),
                DragonCaste::HumanoidDragon
            );
        }
        assert_eq!(DragonCaste::RaptorRunner.display_name(), "Raptor Runner");
    }
}
