use bevy::prelude::*;

// ── Companion Type ────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompanionKind {
    Ally,
    Pet,
    MedicDrone,
}

// ── Companion Component ───────────────────────────────────────────────────────
#[derive(Component, Debug, Clone)]
pub struct Companion {
    pub kind: CompanionKind,
    pub preset_name: String,
    pub health: f32,
    pub max_health: f32,
    pub follow_distance: f32,
    pub orbit_angle: f32,
    pub can_attack: bool,
    pub attack_damage: f32,
    pub attack_cooldown: f32,
    pub attack_timer: f32,
    pub attack_range: f32,
    pub can_heal: bool,
    pub heal_amount: f32,
    pub heal_cooldown: f32,
    pub heal_timer: f32,
    pub is_alive: bool,
}

impl Companion {
    pub fn ally(preset_name: impl Into<String>) -> Self {
        Self {
            kind: CompanionKind::Ally,
            preset_name: preset_name.into(),
            health: 150.0,
            max_health: 150.0,
            follow_distance: 6.0,
            orbit_angle: 0.0,
            can_attack: true,
            attack_damage: 12.0,
            attack_cooldown: 2.0,
            attack_timer: 0.0,
            attack_range: 18.0,
            can_heal: false,
            heal_amount: 0.0,
            heal_cooldown: 0.0,
            heal_timer: 0.0,
            is_alive: true,
        }
    }

    pub fn pet(preset_name: impl Into<String>) -> Self {
        Self {
            kind: CompanionKind::Pet,
            preset_name: preset_name.into(),
            health: 50.0,
            max_health: 50.0,
            follow_distance: 3.0,
            orbit_angle: 0.0,
            can_attack: false,
            attack_damage: 0.0,
            attack_cooldown: 0.0,
            attack_timer: 0.0,
            attack_range: 0.0,
            can_heal: true,
            heal_amount: 2.0,
            heal_cooldown: 10.0,
            heal_timer: 0.0,
            is_alive: true,
        }
    }

    pub fn medic_drone(preset_name: impl Into<String>) -> Self {
        Self {
            kind: CompanionKind::MedicDrone,
            preset_name: preset_name.into(),
            health: 100.0,
            max_health: 100.0,
            follow_distance: 5.0,
            orbit_angle: 0.0,
            can_attack: false,
            attack_damage: 0.0,
            attack_cooldown: 0.0,
            attack_timer: 0.0,
            attack_range: 0.0,
            can_heal: true,
            heal_amount: 8.0,
            heal_cooldown: 6.0,
            heal_timer: 0.0,
            is_alive: true,
        }
    }
}

// ── Companion Projectile ──────────────────────────────────────────────────────
#[derive(Component, Debug, Clone)]
pub struct CompanionProjectile {
    pub damage: f32,
    pub speed: f32,
    pub direction: Vec3,
    pub lifetime: f32,
}
