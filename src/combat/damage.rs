#![allow(dead_code)] // Design/roadmap scaffolding not yet consumed by systems; narrow per-item as features land.
use bevy::prelude::*;

// ── Damage Types ──────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum DamageType {
    Plasma,
    Kinetic,
    Explosive,
    Laser,
    Melee,
    Fire,
    Electric,
    Rift,
    Collision,
    Drowning,
}

// ── Resistance ────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Component)]
pub struct DamageResistance {
    pub damage_type: DamageType,
    /// 0.0 = no reduction, 1.0 = immune
    pub reduction: f32,
}

// ── Damage Info ───────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct DamageInfo {
    pub amount: f32,
    pub damage_type: DamageType,
    pub hit_point: Option<Vec3>,
    pub hit_direction: Option<Vec3>,
    pub attacker: Option<Entity>,
    pub is_critical: bool,
    pub knockback_force: f32,
}

impl DamageInfo {
    pub fn new(amount: f32, damage_type: DamageType) -> Self {
        Self {
            amount,
            damage_type,
            hit_point: None,
            hit_direction: None,
            attacker: None,
            is_critical: false,
            knockback_force: 0.0,
        }
    }

    pub fn with_knockback(mut self, force: f32) -> Self {
        self.knockback_force = force;
        self
    }

    pub fn with_hit_direction(mut self, direction: Vec3) -> Self {
        self.hit_direction = Some(direction);
        self
    }

    /// Mark this hit as critical. Critical damage is resolved centrally so
    /// every weapon path uses the same multiplier and feedback metadata.
    pub fn critical(mut self) -> Self {
        self.is_critical = true;
        self
    }
}

// ── Damage Result ─────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Default)]
pub struct DamageResult {
    pub damage_amount: f32,
    pub was_killed: bool,
    pub was_blocked: bool,
    pub was_parried: bool,
    pub was_critical: bool,
}

// ── Damageable Component ──────────────────────────────────────────────────────
/// Marks an entity as capable of receiving damage.
/// Health is tracked in a separate `Health` component; this holds metadata.
#[derive(Component, Debug, Clone)]
pub struct Damageable {
    pub is_invulnerable: bool,
    pub invulnerability_timer: f32,
    pub resistances: Vec<DamageResistance>,
    /// Flat toughness: incoming damage is scaled by `100 / (100 + defense)`.
    /// 0 = no reduction (default). Populated from `EnemyConfig.defense` at spawn.
    pub defense: f32,
    /// Knockback impulse accumulated by [`apply_damage`] and drained by the
    /// victim's reaction system (enemies: `apply_enemy_knockback`). Kept here so
    /// every damage path gains knockback without extra plumbing.
    pub pending_knockback: Vec3,
}

impl Default for Damageable {
    fn default() -> Self {
        Self {
            is_invulnerable: false,
            invulnerability_timer: 0.0,
            resistances: Vec::new(),
            defense: 0.0,
            pending_knockback: Vec3::ZERO,
        }
    }
}

impl Damageable {
    /// Damageable with toughness and elemental resistances (enemy spawn path).
    pub fn with_defense(defense: f32, resistances: Vec<DamageResistance>) -> Self {
        Self {
            defense,
            resistances,
            ..Self::default()
        }
    }
}

// ── Health Component ──────────────────────────────────────────────────────────
#[derive(Component, Debug, Clone)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Health {
    pub fn new(amount: f32) -> Self {
        Self {
            current: amount,
            max: amount,
        }
    }

    pub fn is_alive(&self) -> bool {
        self.current > 0.0
    }

    /// Apply final damage (after resistances). Returns actual damage dealt.
    pub fn apply_damage(&mut self, amount: f32) -> f32 {
        let actual = amount.min(self.current);
        self.current = (self.current - amount).max(0.0);
        actual
    }

    pub fn heal(&mut self, amount: f32) {
        self.current = (self.current + amount).min(self.max);
    }
}

// ── Resistance Helper ─────────────────────────────────────────────────────────
/// Compute the resistance multiplier for a damage type (0.0 = immune, 1.0 = full damage).
pub fn resistance_multiplier(damageable: &Damageable, damage_type: DamageType) -> f32 {
    let reduction = damageable
        .resistances
        .iter()
        .filter(|r| r.damage_type == damage_type)
        .map(|r| r.reduction)
        .sum::<f32>()
        .min(0.99); // never fully immune via resistance alone
    1.0 - reduction
}

/// Process a damage info against a health + damageable pair.
/// Returns the DamageResult. Caller is responsible for emitting events.
pub fn apply_damage(
    health: &mut Health,
    damageable: &mut Damageable,
    info: &DamageInfo,
) -> DamageResult {
    if !health.is_alive() || damageable.is_invulnerable {
        return DamageResult::default();
    }

    let multiplier = resistance_multiplier(damageable, info.damage_type);
    // Flat-toughness scaling: 0 defense → 1.0, 50 → 0.67, 100 → 0.5.
    let toughness = 100.0 / (100.0 + damageable.defense.max(0.0));
    let critical_multiplier = if info.is_critical { 1.5 } else { 1.0 };
    let final_damage = (info.amount * critical_multiplier * multiplier * toughness).max(1.0);
    let actual = health.apply_damage(final_damage);

    // Accumulate knockback for the victim's reaction system to drain.
    if info.knockback_force > 0.0 {
        if let Some(dir) = info.hit_direction {
            damageable.pending_knockback += dir.normalize_or_zero() * info.knockback_force;
        }
    }

    DamageResult {
        damage_amount: actual,
        was_killed: !health.is_alive(),
        was_blocked: false,
        was_parried: false,
        was_critical: info.is_critical,
    }
}

/// Area-of-effect damage with distance falloff.
pub fn area_damage_falloff(base_damage: f32, distance: f32, radius: f32) -> f32 {
    let t = (distance / radius).clamp(0.0, 1.0);
    base_damage * (1.0 - t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn critical_damage_uses_shared_multiplier_and_reports_identity() {
        let mut health = Health::new(100.0);
        let mut damageable = Damageable::default();
        let result = apply_damage(
            &mut health,
            &mut damageable,
            &DamageInfo::new(20.0, DamageType::Laser).critical(),
        );

        assert_eq!(result.damage_amount, 30.0);
        assert_eq!(health.current, 70.0);
        assert!(result.was_critical);
    }

    #[test]
    fn invulnerable_target_never_reports_a_critical_hit() {
        let mut health = Health::new(100.0);
        let mut damageable = Damageable {
            is_invulnerable: true,
            ..default()
        };
        let result = apply_damage(
            &mut health,
            &mut damageable,
            &DamageInfo::new(20.0, DamageType::Laser).critical(),
        );

        assert_eq!(result.damage_amount, 0.0);
        assert!(!result.was_critical);
    }
}
