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
}

// ── Damage Result ─────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Default)]
pub struct DamageResult {
    pub damage_amount: f32,
    pub was_killed: bool,
    pub was_blocked: bool,
    pub was_parried: bool,
}

// ── Damageable Component ──────────────────────────────────────────────────────
/// Marks an entity as capable of receiving damage.
/// Health is tracked in a separate `Health` component; this holds metadata.
#[derive(Component, Debug, Clone)]
pub struct Damageable {
    pub is_invulnerable: bool,
    pub invulnerability_timer: f32,
    pub resistances: Vec<DamageResistance>,
}

impl Default for Damageable {
    fn default() -> Self {
        Self {
            is_invulnerable: false,
            invulnerability_timer: 0.0,
            resistances: Vec::new(),
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
    let final_damage = (info.amount * multiplier).max(1.0);
    let actual = health.apply_damage(final_damage);

    DamageResult {
        damage_amount: actual,
        was_killed: !health.is_alive(),
        was_blocked: false,
        was_parried: false,
    }
}

/// Area-of-effect damage with distance falloff.
pub fn area_damage_falloff(base_damage: f32, distance: f32, radius: f32) -> f32 {
    let t = (distance / radius).clamp(0.0, 1.0);
    base_damage * (1.0 - t)
}
