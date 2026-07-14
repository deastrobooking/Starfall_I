use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// ── Weapon Type ───────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WeaponType {
    Pistol,  // 1: compact star beam
    Rifle,   // 2: rapid comet beam
    Shotgun, // 3: wide sparkle burst
    Rocket,  // 4: arcing nova orb
    Laser,   // 5: continuous rainbow ray
    Grenade, // 6: bouncing star bubble
}

impl WeaponType {
    pub fn display_name(&self) -> &'static str {
        match self {
            WeaponType::Pistol => "Starlight Popper",
            WeaponType::Rifle => "Comet Stream",
            WeaponType::Shotgun => "Sparkle Fan",
            WeaponType::Rocket => "Nova Orb",
            WeaponType::Laser => "Rainbow Ray",
            WeaponType::Grenade => "Star Bubble Bombs",
        }
    }
}

// ── Primary Weapon Component ──────────────────────────────────────────────────
#[derive(Component, Debug, Clone)]
pub struct Weapon {
    pub weapon_type: WeaponType,
    pub damage: f32,
    pub fire_rate: f32, // seconds between shots (base)
    pub ammo: u32,
    pub max_ammo: u32,
    pub speed: f32, // projectile speed (units/sec)
    pub spread: f32,
    pub automatic: bool,
    pub fire_timer: f32,
    pub pellets: u32, // for shotgun
    pub is_explosive: bool,
    pub explosion_radius: f32,
    // ── Per-slot upgrade rank ──────────────────────────────────────────────
    pub rank: u32, // 0..=MAX_WEAPON_RANK, synced from WeaponRanks resource
    // ── Charge blast state ─────────────────────────────────────────────────
    pub charge_progress: f32, // 0.0..=1.0; builds while fire is held
    pub charge_held: bool,    // tracks whether fire was held last frame
}

pub const MAX_WEAPON_RANK: u32 = 4;

impl Weapon {
    pub fn new(weapon_type: WeaponType) -> Self {
        match weapon_type {
            WeaponType::Pistol => Self {
                weapon_type,
                damage: 16.0,
                fire_rate: 0.26,
                ammo: 60,
                max_ammo: 60,
                speed: 68.0,
                spread: 0.02,
                automatic: false,
                fire_timer: 0.0,
                pellets: 1,
                is_explosive: false,
                explosion_radius: 0.0,
                rank: 0,
                charge_progress: 0.0,
                charge_held: false,
            },
            WeaponType::Rifle => Self {
                weapon_type,
                damage: 18.0,
                fire_rate: 0.075,
                ammo: 150,
                max_ammo: 150,
                speed: 105.0,
                spread: 0.025,
                automatic: true,
                fire_timer: 0.0,
                pellets: 1,
                is_explosive: false,
                explosion_radius: 0.0,
                rank: 0,
                charge_progress: 0.0,
                charge_held: false,
            },
            WeaponType::Shotgun => Self {
                weapon_type,
                damage: 7.0,
                fire_rate: 0.7,
                ammo: 36,
                max_ammo: 36,
                speed: 80.0,
                spread: 0.18,
                automatic: false,
                fire_timer: 0.0,
                pellets: 10,
                is_explosive: false,
                explosion_radius: 0.0,
                rank: 0,
                charge_progress: 0.0,
                charge_held: false,
            },
            WeaponType::Rocket => Self {
                weapon_type,
                damage: 90.0,
                fire_rate: 1.2,
                ammo: 10,
                max_ammo: 10,
                speed: 34.0,
                spread: 0.0,
                automatic: false,
                fire_timer: 0.0,
                pellets: 1,
                is_explosive: true,
                explosion_radius: 6.5,
                rank: 0,
                charge_progress: 0.0,
                charge_held: false,
            },
            WeaponType::Laser => Self {
                weapon_type,
                damage: 26.0,
                fire_rate: 0.045,
                ammo: 220,
                max_ammo: 220,
                speed: 320.0,
                spread: 0.0,
                automatic: true,
                fire_timer: 0.0,
                pellets: 1,
                is_explosive: false,
                explosion_radius: 0.0,
                rank: 0,
                charge_progress: 0.0,
                charge_held: false,
            },
            WeaponType::Grenade => Self {
                weapon_type,
                damage: 75.0,
                fire_rate: 0.95,
                ammo: 8,
                max_ammo: 8,
                speed: 16.0,
                spread: 0.0,
                automatic: false,
                fire_timer: 0.0,
                pellets: 1,
                is_explosive: true,
                explosion_radius: 8.5,
                rank: 0,
                charge_progress: 0.0,
                charge_held: false,
            },
        }
    }

    pub fn can_fire(&self) -> bool {
        self.fire_timer <= 0.0 && self.ammo > 0
    }

    pub fn reload(&mut self) {
        self.ammo = self.max_ammo;
    }

    // ── Rank scaling ──────────────────────────────────────────────────────────
    pub fn rank_damage_mult(&self) -> f32 {
        1.0 + self.rank as f32 * 0.20 // +20 % per rank
    }

    pub fn rank_effective_fire_rate(&self) -> f32 {
        // Negative means faster: -6 % cooldown per rank, floor at 40 % of base
        let reduction = (1.0 - self.rank as f32 * 0.06).max(0.4);
        self.fire_rate * reduction
    }

    // ── Charge blast helpers ──────────────────────────────────────────────────
    pub fn min_charge_time(&self) -> f32 {
        match self.weapon_type {
            WeaponType::Pistol => 1.0,
            WeaponType::Rifle => 1.5,
            WeaponType::Shotgun => 1.0,
            WeaponType::Rocket => 2.0,
            WeaponType::Laser => 1.2,
            WeaponType::Grenade => 1.5,
        }
    }

    // Multiplier on top of rank_damage_mult + perk/tech multipliers.
    pub fn charge_damage_mult(&self) -> f32 {
        3.5 + self.rank as f32 * 0.5
    }

    pub fn charge_explosion_radius(&self) -> f32 {
        match self.weapon_type {
            WeaponType::Pistol => 3.5,
            WeaponType::Rifle => 2.0,
            WeaponType::Shotgun => 6.5,
            WeaponType::Rocket => self.explosion_radius * 2.5,
            WeaponType::Laser => 5.0,
            WeaponType::Grenade => self.explosion_radius * 2.0,
        }
    }

    // Visual stretch factor along travel direction for the projectile mesh.
    pub fn proj_stretch(&self) -> Vec3 {
        self.visual_profile().projectile_scale
    }

    pub fn visual_profile(&self) -> WeaponVisualProfile {
        WeaponVisualProfile::for_type(self.weapon_type)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WeaponVisualProfile {
    pub projectile_scale: Vec3,
    pub muzzle_scale: f32,
    pub impact_scale: f32,
}

impl WeaponVisualProfile {
    pub const fn for_type(weapon_type: WeaponType) -> Self {
        match weapon_type {
            WeaponType::Pistol => Self::new(Vec3::new(1.15, 1.15, 3.0), 1.0, 1.0),
            WeaponType::Rifle => Self::new(Vec3::new(0.9, 0.9, 6.2), 1.15, 1.1),
            WeaponType::Shotgun => Self::new(Vec3::new(1.25, 1.25, 2.2), 1.3, 1.15),
            WeaponType::Rocket => Self::new(Vec3::splat(1.5), 1.45, 1.8),
            WeaponType::Laser => Self::new(Vec3::new(0.75, 0.75, 8.5), 1.2, 1.2),
            WeaponType::Grenade => Self::new(Vec3::splat(1.35), 1.25, 1.6),
        }
    }

    const fn new(projectile_scale: Vec3, muzzle_scale: f32, impact_scale: f32) -> Self {
        Self {
            projectile_scale,
            muzzle_scale,
            impact_scale,
        }
    }
}

// ── Weapon Ranks Resource ─────────────────────────────────────────────────────
#[derive(Resource, Default, Debug, Clone, Serialize, Deserialize)]
pub struct WeaponRanks {
    pub ranks: [u32; 6], // indexed by WeaponInventory.active_slot (0-5)
}

impl WeaponRanks {
    pub fn upgrade_cost(current_rank: u32) -> u32 {
        match current_rank {
            0 => 8,
            1 => 15,
            2 => 25,
            3 => 40,
            _ => u32::MAX,
        }
    }

    pub fn rank_label(rank: u32) -> &'static str {
        match rank {
            0 => "Basic",
            1 => "Tuned",
            2 => "Charged",
            3 => "Overclocked",
            _ => "Nova",
        }
    }
}

// ── Charge Blast Marker ───────────────────────────────────────────────────────
#[derive(Component, Debug)]
pub struct ChargeBlastTag;

// ── Active Weapon Tracker (on Player) ────────────────────────────────────────
#[derive(Component, Debug, Clone)]
pub struct WeaponInventory {
    pub slots: [Weapon; 6],
    pub active_slot: usize, // 0-5 = keys 1-6
}

impl Default for WeaponInventory {
    fn default() -> Self {
        Self {
            slots: [
                Weapon::new(WeaponType::Pistol),
                Weapon::new(WeaponType::Rifle),
                Weapon::new(WeaponType::Shotgun),
                Weapon::new(WeaponType::Rocket),
                Weapon::new(WeaponType::Laser),
                Weapon::new(WeaponType::Grenade),
            ],
            active_slot: 0,
        }
    }
}

impl WeaponInventory {
    pub fn active(&self) -> &Weapon {
        &self.slots[self.active_slot]
    }

    pub fn active_mut(&mut self) -> &mut Weapon {
        &mut self.slots[self.active_slot]
    }
}

// ── Special Weapon Slot ───────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpecialSlot {
    Slot7, // Homing star
    Slot8, // Tri-star burst
    Slot9, // Moon bubble
    Slot0, // Sprite turret
}

#[derive(Component, Debug, Clone)]
pub struct SpecialWeapon {
    pub slot: SpecialSlot,
    pub name: &'static str,
    pub base_damage: f32,
    pub cooldown: f32,
    pub cooldown_timer: f32,
    pub ammo: u32,
    pub max_ammo: u32,
    pub level: u32,
}

impl SpecialWeapon {
    pub fn new(slot: SpecialSlot) -> Self {
        match slot {
            SpecialSlot::Slot7 => Self {
                slot,
                name: "Homing Star",
                base_damage: 60.0,
                cooldown: 2.0,
                cooldown_timer: 0.0,
                ammo: 10,
                max_ammo: 10,
                level: 1,
            },
            SpecialSlot::Slot8 => Self {
                slot,
                name: "Tri-Star Burst",
                base_damage: 45.0,
                cooldown: 1.5,
                cooldown_timer: 0.0,
                ammo: 15,
                max_ammo: 15,
                level: 1,
            },
            SpecialSlot::Slot9 => Self {
                slot,
                name: "Moon Bubble",
                base_damage: 120.0,
                cooldown: 4.0,
                cooldown_timer: 0.0,
                ammo: 5,
                max_ammo: 5,
                level: 1,
            },
            SpecialSlot::Slot0 => Self {
                slot,
                name: "Sprite Turret",
                base_damage: 20.0,
                cooldown: 10.0,
                cooldown_timer: 0.0,
                ammo: 3,
                max_ammo: 3,
                level: 1,
            },
        }
    }

    pub fn can_fire(&self) -> bool {
        self.cooldown_timer <= 0.0 && self.ammo > 0
    }

    pub fn effective_damage(&self) -> f32 {
        let mult = match self.level {
            1 => 1.0,
            2 => 1.4,
            _ => 1.7,
        };
        self.base_damage * mult
    }
}

// ── Special Weapon Inventory (on Player) ─────────────────────────────────────
#[derive(Component, Debug, Clone)]
pub struct SpecialWeaponInventory {
    pub slot7: SpecialWeapon,
    pub slot8: SpecialWeapon,
    pub slot9: SpecialWeapon,
    pub slot0: SpecialWeapon,
}

impl Default for SpecialWeaponInventory {
    fn default() -> Self {
        Self {
            slot7: SpecialWeapon::new(SpecialSlot::Slot7),
            slot8: SpecialWeapon::new(SpecialSlot::Slot8),
            slot9: SpecialWeapon::new(SpecialSlot::Slot9),
            slot0: SpecialWeapon::new(SpecialSlot::Slot0),
        }
    }
}

// ── Star Sabre ────────────────────────────────────────────────────────────────
#[derive(Component, Debug, Clone)]
pub struct BeamSabre {
    pub active: bool,
    /// Starfall I: Star Sabre is locked until the Ch.1 discoverable is collected.
    pub unlocked: bool,
    pub level: u32,
    pub slash_damage: f32,
    pub wave_damage: f32,
    pub slash_count: u32,
    pub cooldown: f32,
    pub cooldown_timer: f32,
    pub slash_timer: f32,
    pub slash_index: u32,
    pub is_slashing: bool,
}

/// Marker on the player while the Star Sabre has not been discovered yet.
#[derive(Component, Debug, Default)]
pub struct BeamSabreLocked;

impl Default for BeamSabre {
    fn default() -> Self {
        Self {
            active: false,
            unlocked: false,
            level: 1,
            slash_damage: 25.0,
            wave_damage: 40.0,
            slash_count: 2,
            cooldown: 0.8,
            cooldown_timer: 0.0,
            slash_timer: 0.0,
            slash_index: 0,
            is_slashing: false,
        }
    }
}

impl BeamSabre {
    pub fn set_level(&mut self, level: u32) {
        self.level = level;
        let (sd, wd, sc, cd) = match level {
            1 => (25.0, 40.0, 2, 0.8),
            2 => (35.0, 60.0, 2, 0.7),
            3 => (50.0, 80.0, 3, 0.6),
            4 => (65.0, 100.0, 4, 0.5),
            _ => (85.0, 150.0, 5, 0.4),
        };
        self.slash_damage = sd;
        self.wave_damage = wd;
        self.slash_count = sc;
        self.cooldown = cd;
    }

    pub fn is_piercing(&self) -> bool {
        self.level >= 3
    }
    pub fn fires_dual_wave(&self) -> bool {
        self.level >= 4
    }
    pub fn has_aoe_splash(&self) -> bool {
        self.level >= 5
    }
}

// ── Projectile ────────────────────────────────────────────────────────────────
#[derive(Component, Debug, Clone)]
pub struct Projectile {
    pub damage: f32,
    pub damage_type: crate::damage::DamageType,
    pub speed: f32,
    pub direction: Vec3,
    pub lifetime: f32,
    pub is_explosive: bool,
    pub explosion_radius: f32,
    pub weapon_type: ProjectileOwner,
    pub owner: Option<Entity>,
    pub piercing: bool,
    /// For grenade arc
    pub gravity_affected: bool,
    pub vertical_velocity: f32,
}

/// Steering state for the Nova tracking missile (`Homing Star` in the current
/// loadout). Kept separate from `Projectile` so ordinary beams pay no targeting
/// query or state cost.
#[derive(Component, Debug, Clone)]
pub struct TrackingMissile {
    pub owner_player: u8,
    pub target: Option<Entity>,
    pub acquisition_range: f32,
    pub acquisition_cone_cos: f32,
    pub turn_rate_radians: f32,
    pub reacquire_timer: f32,
    pub trail_timer: f32,
}

impl TrackingMissile {
    pub fn new(owner_player: u8) -> Self {
        Self {
            owner_player,
            target: None,
            acquisition_range: 96.0,
            acquisition_cone_cos: 0.28,
            turn_rate_radians: 4.6,
            reacquire_timer: 0.0,
            trail_timer: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectileOwner {
    Player,
    Enemy,
    Companion,
    SpriteTurret,
    HomingStar,
    TriStarBurst,
    MoonBubble,
}

// ── Melee Combo ───────────────────────────────────────────────────────────────
#[derive(Component, Debug, Clone, Default)]
pub struct MeleeCombo {
    pub light_index: usize,
    pub heavy_index: usize,
    pub light_timer: f32,
    pub heavy_timer: f32,
    pub active_timer: f32,
    pub is_attacking: bool,
    pub buffered_light: bool,
    pub buffered_heavy: bool,
    pub damage_multiplier: f32,
}

impl MeleeCombo {
    pub fn new() -> Self {
        Self {
            damage_multiplier: 1.0,
            ..default()
        }
    }
}
