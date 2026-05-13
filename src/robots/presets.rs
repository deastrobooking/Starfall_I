use super::designer::{ArmStyle, HeadShape, LegStyle, RobotArchetype, RobotStyle, VisorStyle};
use bevy::prelude::*;

// ── Enemy Presets ─────────────────────────────────────────────────────────────

pub fn scout_prime() -> RobotStyle {
    RobotStyle {
        archetype: RobotArchetype::Scout,
        scale: 1.0,
        torso_width: 18.0,
        torso_height: 36.0,
        torso_depth: 13.0,
        head_size: 10.0,
        head_shape: HeadShape::Box,
        arm_length: 26.0,
        arm_thickness: 5.0,
        arm_style: ArmStyle::Box,
        leg_length: 28.0,
        leg_thickness: 7.0,
        leg_style: LegStyle::Box,
        shoulder_pad_size: 6.0,
        hip_pad_size: 5.0,
        has_visor: true,
        visor_style: VisorStyle::Slit,
        has_cannons: false,
        primary: Color::srgb(0.2, 0.4, 0.8),
        secondary: Color::srgb(0.1, 0.2, 0.4),
        emissive: Color::srgb(0.0, 0.6, 1.0),
        ..RobotStyle::default()
    }
}

pub fn brute_forge() -> RobotStyle {
    RobotStyle {
        archetype: RobotArchetype::Brute,
        scale: 1.1,
        torso_width: 32.0,
        torso_height: 50.0,
        torso_depth: 22.0,
        head_size: 13.0,
        head_shape: HeadShape::Box,
        arm_length: 32.0,
        arm_thickness: 11.0,
        arm_style: ArmStyle::Box,
        leg_length: 32.0,
        leg_thickness: 13.0,
        leg_style: LegStyle::Box,
        shoulder_pad_size: 12.0,
        hip_pad_size: 8.0,
        has_visor: true,
        visor_style: VisorStyle::Slit,
        has_cannons: true,
        cannon_size: 5.0,
        has_horns: true,
        horn_length: 14.0,
        extra_plating: 2,
        asymmetry: 0.2,
        primary: Color::srgb(0.7, 0.15, 0.05),
        secondary: Color::srgb(0.35, 0.07, 0.02),
        emissive: Color::srgb(1.0, 0.4, 0.0),
        ..RobotStyle::default()
    }
}

pub fn jet_warden() -> RobotStyle {
    RobotStyle {
        archetype: RobotArchetype::Flyer,
        scale: 1.0,
        torso_width: 16.0,
        torso_height: 34.0,
        torso_depth: 12.0,
        head_size: 9.0,
        head_shape: HeadShape::Sphere,
        arm_length: 26.0,
        arm_thickness: 5.0,
        arm_style: ArmStyle::Cylinder,
        leg_length: 22.0,
        leg_thickness: 6.0,
        leg_style: LegStyle::Hoverpads,
        shoulder_pad_size: 7.0,
        hip_pad_size: 4.0,
        has_wings: true,
        wing_span: 40.0,
        wing_angle: 35.0,
        has_visor: true,
        visor_style: VisorStyle::Full,
        has_backpack: true,
        backpack_size: 12.0,
        primary: Color::srgb(0.2, 0.1, 0.4),
        secondary: Color::srgb(0.1, 0.05, 0.2),
        emissive: Color::srgb(0.6, 0.0, 1.0),
        ..RobotStyle::default()
    }
}

pub fn tank_titan() -> RobotStyle {
    RobotStyle {
        archetype: RobotArchetype::Tank,
        scale: 1.2,
        torso_width: 38.0,
        torso_height: 56.0,
        torso_depth: 28.0,
        head_size: 14.0,
        head_shape: HeadShape::Box,
        arm_length: 34.0,
        arm_thickness: 13.0,
        arm_style: ArmStyle::Box,
        leg_length: 36.0,
        leg_thickness: 15.0,
        leg_style: LegStyle::Box,
        shoulder_pad_size: 15.0,
        hip_pad_size: 10.0,
        has_visor: true,
        visor_style: VisorStyle::Slit,
        has_shield: true,
        shield_size: 40.0,
        has_horns: true,
        horn_length: 18.0,
        extra_plating: 3,
        asymmetry: 0.1,
        primary: Color::srgb(0.1, 0.3, 0.1),
        secondary: Color::srgb(0.05, 0.15, 0.05),
        emissive: Color::srgb(0.0, 1.0, 0.3),
        ..RobotStyle::default()
    }
}

pub fn insectoid_stalker() -> RobotStyle {
    RobotStyle {
        archetype: RobotArchetype::Insectoid,
        scale: 0.9,
        torso_width: 16.0,
        torso_height: 32.0,
        torso_depth: 11.0,
        head_size: 9.0,
        head_shape: HeadShape::Sphere,
        arm_length: 30.0,
        arm_thickness: 5.0,
        arm_style: ArmStyle::Tapered,
        leg_length: 34.0,
        leg_thickness: 7.0,
        leg_style: LegStyle::Digitigrade,
        shoulder_pad_size: 5.0,
        hip_pad_size: 4.0,
        has_visor: true,
        visor_style: VisorStyle::Full,
        has_wings: true,
        wing_span: 35.0,
        wing_angle: 50.0,
        has_tail: true,
        tail_length: 28.0,
        tail_segments: 5,
        has_antennae: true,
        antenna_length: 22.0,
        primary: Color::srgb(0.1, 0.35, 0.05),
        secondary: Color::srgb(0.05, 0.18, 0.02),
        emissive: Color::srgb(0.2, 0.9, 0.0),
        ..RobotStyle::default()
    }
}

pub fn hybrid_omega() -> RobotStyle {
    RobotStyle {
        archetype: RobotArchetype::Hybrid,
        scale: 1.4,
        torso_width: 28.0,
        torso_height: 50.0,
        torso_depth: 20.0,
        head_size: 14.0,
        head_shape: HeadShape::Cone,
        arm_length: 34.0,
        arm_thickness: 9.0,
        arm_style: ArmStyle::Box,
        leg_length: 36.0,
        leg_thickness: 11.0,
        leg_style: LegStyle::Digitigrade,
        shoulder_pad_size: 12.0,
        hip_pad_size: 8.0,
        has_wings: true,
        wing_span: 55.0,
        wing_angle: 40.0,
        has_tail: true,
        tail_length: 35.0,
        tail_segments: 6,
        has_shield: true,
        shield_size: 44.0,
        has_cannons: true,
        cannon_size: 7.0,
        has_horns: true,
        horn_length: 20.0,
        has_antennae: true,
        antenna_length: 18.0,
        has_visor: true,
        visor_style: VisorStyle::Full,
        extra_plating: 2,
        asymmetry: 0.3,
        primary: Color::srgb(0.25, 0.0, 0.35),
        secondary: Color::srgb(0.12, 0.0, 0.18),
        emissive: Color::srgb(0.9, 0.0, 1.0),
        ..RobotStyle::default()
    }
}

// ── Ally Presets ──────────────────────────────────────────────────────────────

pub fn guardian_unit() -> RobotStyle {
    RobotStyle {
        archetype: RobotArchetype::Ally,
        scale: 1.0,
        torso_width: 20.0,
        torso_height: 40.0,
        torso_depth: 15.0,
        has_shield: true,
        shield_size: 32.0,
        has_cannons: true,
        cannon_size: 4.0,
        has_backpack: true,
        backpack_size: 11.0,
        has_visor: true,
        visor_style: VisorStyle::Slit,
        primary: Color::srgb(0.1, 0.6, 0.7),
        secondary: Color::srgb(0.05, 0.3, 0.35),
        emissive: Color::srgb(0.0, 1.0, 1.0),
        ..RobotStyle::default()
    }
}

pub fn medic_drone_style() -> RobotStyle {
    RobotStyle {
        archetype: RobotArchetype::Ally,
        scale: 0.8,
        torso_width: 14.0,
        torso_height: 28.0,
        torso_depth: 11.0,
        leg_style: LegStyle::Hoverpads,
        has_antennae: true,
        antenna_length: 18.0,
        has_backpack: true,
        backpack_size: 8.0,
        has_visor: true,
        visor_style: VisorStyle::Round,
        primary: Color::srgb(0.1, 0.55, 0.2),
        secondary: Color::srgb(0.05, 0.27, 0.1),
        emissive: Color::srgb(0.0, 1.0, 0.4),
        ..RobotStyle::default()
    }
}

pub fn scout_companion() -> RobotStyle {
    RobotStyle {
        archetype: RobotArchetype::Ally,
        scale: 0.85,
        torso_width: 15.0,
        torso_height: 32.0,
        torso_depth: 12.0,
        leg_style: LegStyle::Digitigrade,
        has_cannons: true,
        cannon_size: 3.0,
        has_visor: true,
        visor_style: VisorStyle::Slit,
        primary: Color::srgb(0.15, 0.55, 0.15),
        secondary: Color::srgb(0.07, 0.27, 0.07),
        emissive: Color::srgb(0.0, 0.9, 0.2),
        ..RobotStyle::default()
    }
}

// ── Pet Presets ───────────────────────────────────────────────────────────────

pub fn spark_pup() -> RobotStyle {
    RobotStyle {
        archetype: RobotArchetype::Pet,
        scale: 0.45,
        torso_width: 12.0,
        torso_height: 20.0,
        torso_depth: 10.0,
        head_size: 8.0,
        head_shape: HeadShape::Box,
        has_tail: true,
        tail_length: 18.0,
        tail_segments: 4,
        has_antennae: true,
        antenna_length: 14.0,
        has_visor: true,
        visor_style: VisorStyle::Round,
        primary: Color::srgb(0.85, 0.7, 0.1),
        secondary: Color::srgb(0.5, 0.4, 0.05),
        emissive: Color::srgb(1.0, 0.9, 0.0),
        ..RobotStyle::default()
    }
}

pub fn neon_cat() -> RobotStyle {
    RobotStyle {
        archetype: RobotArchetype::Pet,
        scale: 0.4,
        torso_width: 11.0,
        torso_height: 18.0,
        torso_depth: 9.0,
        head_size: 7.0,
        head_shape: HeadShape::Sphere,
        leg_style: LegStyle::Digitigrade,
        has_tail: true,
        tail_length: 22.0,
        tail_segments: 5,
        has_horns: true,
        horn_length: 8.0,
        has_visor: true,
        visor_style: VisorStyle::Slit,
        primary: Color::srgb(0.35, 0.05, 0.5),
        secondary: Color::srgb(0.18, 0.02, 0.25),
        emissive: Color::srgb(0.8, 0.0, 1.0),
        ..RobotStyle::default()
    }
}

pub fn hover_orb() -> RobotStyle {
    RobotStyle {
        archetype: RobotArchetype::Pet,
        scale: 0.35,
        torso_width: 10.0,
        torso_height: 10.0,
        torso_depth: 10.0,
        head_size: 8.0,
        head_shape: HeadShape::Sphere,
        leg_style: LegStyle::Hoverpads,
        has_wings: true,
        wing_span: 20.0,
        wing_angle: 30.0,
        has_visor: true,
        visor_style: VisorStyle::Full,
        primary: Color::srgb(0.1, 0.7, 0.8),
        secondary: Color::srgb(0.05, 0.35, 0.4),
        emissive: Color::srgb(0.0, 1.0, 1.0),
        ..RobotStyle::default()
    }
}

// ── Lookup by Name ────────────────────────────────────────────────────────────
pub fn robot_by_name(name: &str) -> Option<RobotStyle> {
    match name {
        "ScoutPrime" => Some(scout_prime()),
        "BruteForge" => Some(brute_forge()),
        "JetWarden" => Some(jet_warden()),
        "TankTitan" => Some(tank_titan()),
        "InsectoidStalker" => Some(insectoid_stalker()),
        "HybridOmega" => Some(hybrid_omega()),
        "GuardianUnit" => Some(guardian_unit()),
        "MedicDrone" => Some(medic_drone_style()),
        "ScoutCompanion" => Some(scout_companion()),
        "SparkPup" => Some(spark_pup()),
        "NeonCat" => Some(neon_cat()),
        "HoverOrb" => Some(hover_orb()),
        // ── Synthetics ─────────────────────────────────────────────────
        "Amp" | "amp" => Some(amp()),
        "Atlas" => Some(atlas()),
        "Volt" => Some(volt()),
        "Valor" => Some(valor()),
        "Aria" => Some(aria()),
        "Chroma" => Some(chroma()),
        "Daria" => Some(daria()),
        "Prima" => Some(prima()),
        "Theta" => Some(theta()),
        "Ion" => Some(ion()),
        "Epsilon" => Some(epsilon()),
        "Lambda" => Some(lambda()),
        // ── Mechanoids ─────────────────────────────────────────────────
        "Apollo" => Some(apollo()),
        "Saturn" => Some(saturn()),
        "Mercury" => Some(mercury()),
        "Axe" => Some(axe()),
        "Octavius" => Some(octavius()),
        "Helios" => Some(helios()),
        "Selene" => Some(selene()),
        // ── Insectoid Bosses ───────────────────────────────────────────
        "AracnoidQueen" => Some(aracnoid_queen()),
        "Punisher" => Some(punisher()),
        "InsectoidGeneral" => Some(insectoid_general()),
        "FormicAvatar" => Some(formic_avatar()),
        // ── Animatons ──────────────────────────────────────────────────
        "HarvesterMech" => Some(harvester_mech()),
        "WolfAnimaton" => Some(wolf_animaton()),
        "TigerAnimaton" => Some(tiger_animaton()),
        // ── Charred ────────────────────────────────────────────────────
        "CharredCaptain" => Some(charred_captain()),
        // ── Swarm ──────────────────────────────────────────────────────
        "Cygnus" => Some(cygnus()),
        "Cygni" => Some(cygni()),
        "Brutus" => Some(brutus()),
        "Nero" => Some(nero()),
        "Minerva" => Some(minerva()),
        "Caliguon" => Some(caliguon()),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Legacy procedural body presets reused by Starfall I characters and bosses.
// ─────────────────────────────────────────────────────────────────────────────

// ── Synthetics ───────────────────────────────────────────────────────────────
pub fn amp() -> RobotStyle {
    RobotStyle {
        archetype: RobotArchetype::Ally,
        scale: 1.0,
        torso_width: 17.0,
        torso_height: 36.0,
        torso_depth: 12.0,
        head_size: 9.0,
        head_shape: HeadShape::Sphere,
        arm_length: 28.0,
        arm_thickness: 5.5,
        arm_style: ArmStyle::Tapered,
        leg_length: 30.0,
        leg_thickness: 7.0,
        leg_style: LegStyle::Box,
        shoulder_pad_size: 6.0,
        hip_pad_size: 5.0,
        has_visor: true,
        visor_style: VisorStyle::Round,
        primary: Color::srgb(0.95, 0.95, 1.0),
        secondary: Color::srgb(0.55, 0.6, 0.7),
        emissive: Color::srgb(0.2, 0.85, 1.0),
        ..RobotStyle::default()
    }
}
pub fn atlas() -> RobotStyle {
    let mut s = amp();
    s.torso_width = 22.0;
    s.torso_height = 42.0;
    s.torso_depth = 16.0;
    s.arm_thickness = 8.0;
    s.leg_thickness = 9.0;
    s.shoulder_pad_size = 10.0;
    s.primary = Color::srgb(0.85, 0.55, 0.2);
    s.emissive = Color::srgb(1.0, 0.6, 0.15);
    s
}
pub fn volt() -> RobotStyle {
    let mut s = amp();
    s.has_antennae = true;
    s.antenna_length = 22.0;
    s.primary = Color::srgb(0.95, 0.85, 0.1);
    s.emissive = Color::srgb(1.0, 1.0, 0.2);
    s
}
pub fn valor() -> RobotStyle {
    let mut s = amp();
    s.has_shield = true;
    s.shield_size = 28.0;
    s.primary = Color::srgb(0.2, 0.7, 0.3);
    s.emissive = Color::srgb(0.3, 1.0, 0.4);
    s
}
pub fn aria() -> RobotStyle {
    let mut s = amp();
    s.scale = 0.95;
    s.head_shape = HeadShape::Sphere;
    s.primary = Color::srgb(1.0, 0.6, 0.85);
    s.emissive = Color::srgb(1.0, 0.5, 0.85);
    s
}
pub fn chroma() -> RobotStyle {
    let mut s = amp();
    s.primary = Color::srgb(0.7, 0.2, 0.9);
    s.emissive = Color::srgb(0.8, 0.1, 1.0);
    s
}
pub fn daria() -> RobotStyle {
    let mut s = amp();
    s.primary = Color::srgb(0.2, 0.55, 1.0);
    s.emissive = Color::srgb(0.3, 0.7, 1.0);
    s
}
pub fn prima() -> RobotStyle {
    let mut s = amp();
    s.primary = Color::srgb(1.0, 0.85, 0.4);
    s.emissive = Color::srgb(1.0, 0.9, 0.3);
    s
}
pub fn theta() -> RobotStyle {
    let mut s = amp();
    s.primary = Color::srgb(0.7, 1.0, 0.85);
    s.emissive = Color::srgb(0.5, 1.0, 0.85);
    s.has_wings = true;
    s.wing_span = 22.0;
    s
}
pub fn ion() -> RobotStyle {
    let mut s = amp();
    s.scale = 0.9;
    s.head_shape = HeadShape::Cylinder;
    s.primary = Color::srgb(0.4, 0.95, 1.0);
    s.emissive = Color::srgb(0.0, 1.0, 1.0);
    s
}
pub fn epsilon() -> RobotStyle {
    let mut s = amp();
    s.has_horns = true;
    s.horn_length = 10.0;
    s.primary = Color::srgb(0.45, 0.05, 0.55);
    s.emissive = Color::srgb(0.7, 0.0, 0.85);
    s.archetype = RobotArchetype::Insectoid;
    s
}
pub fn lambda() -> RobotStyle {
    let mut s = epsilon();
    s.primary = Color::srgb(0.55, 0.15, 0.05);
    s.emissive = Color::srgb(1.0, 0.2, 0.0);
    s
}

// ── Mechanoids ──────────────────────────────────────────────────────────────
pub fn apollo() -> RobotStyle {
    let mut s = brute_forge();
    s.primary = Color::srgb(0.95, 0.8, 0.3);
    s.emissive = Color::srgb(1.0, 0.85, 0.0);
    s
}
pub fn saturn() -> RobotStyle {
    let mut s = brute_forge();
    s.primary = Color::srgb(0.6, 0.5, 0.4);
    s.emissive = Color::srgb(0.9, 0.7, 0.3);
    s
}
pub fn mercury() -> RobotStyle {
    let mut s = scout_prime();
    s.primary = Color::srgb(0.85, 0.85, 0.9);
    s.emissive = Color::srgb(0.8, 0.85, 1.0);
    s
}
pub fn axe() -> RobotStyle {
    let mut s = brute_forge();
    s.has_cannons = true;
    s.cannon_size = 7.0;
    s.primary = Color::srgb(0.4, 0.4, 0.45);
    s.emissive = Color::srgb(1.0, 0.3, 0.0);
    s
}
pub fn octavius() -> RobotStyle {
    let mut s = tank_titan();
    s.primary = Color::srgb(0.3, 0.4, 0.6);
    s.emissive = Color::srgb(0.0, 0.85, 1.0);
    s
}
pub fn helios() -> RobotStyle {
    let mut s = jet_warden();
    s.primary = Color::srgb(1.0, 0.9, 0.2);
    s.emissive = Color::srgb(1.0, 0.85, 0.0);
    s
}
pub fn selene() -> RobotStyle {
    let mut s = scout_prime();
    s.primary = Color::srgb(0.8, 0.85, 0.95);
    s.emissive = Color::srgb(0.7, 0.8, 1.0);
    s
}

// ── Insectoid bosses ────────────────────────────────────────────────────────
pub fn aracnoid_queen() -> RobotStyle {
    let mut s = insectoid_stalker();
    s.scale = 1.6;
    s.has_horns = true;
    s.horn_length = 24.0;
    s.has_cannons = true;
    s.cannon_size = 6.0;
    s.primary = Color::srgb(0.4, 0.05, 0.35);
    s.emissive = Color::srgb(1.0, 0.1, 0.6);
    s
}
pub fn punisher() -> RobotStyle {
    let mut s = insectoid_stalker();
    s.scale = 1.2;
    s.extra_plating = 2;
    s.primary = Color::srgb(0.25, 0.3, 0.05);
    s.emissive = Color::srgb(0.7, 1.0, 0.0);
    s
}
pub fn insectoid_general() -> RobotStyle {
    let mut s = hybrid_omega();
    s.archetype = RobotArchetype::Insectoid;
    s.primary = Color::srgb(0.15, 0.45, 0.05);
    s.emissive = Color::srgb(0.4, 1.0, 0.1);
    s
}
pub fn formic_avatar() -> RobotStyle {
    let mut s = hybrid_omega();
    s.scale = 1.1;
    s.primary = Color::srgb(0.05, 0.4, 0.15);
    s.emissive = Color::srgb(0.0, 1.0, 0.3);
    s
}

// ── Animatons ───────────────────────────────────────────────────────────────
pub fn harvester_mech() -> RobotStyle {
    let mut s = tank_titan();
    s.scale = 1.5;
    s.has_cannons = true;
    s.cannon_size = 9.0;
    s.primary = Color::srgb(0.5, 0.1, 0.0);
    s.emissive = Color::srgb(1.0, 0.3, 0.0);
    s
}
pub fn wolf_animaton() -> RobotStyle {
    let mut s = insectoid_stalker();
    s.has_tail = true;
    s.tail_length = 30.0;
    s.primary = Color::srgb(0.4, 0.4, 0.45);
    s.emissive = Color::srgb(1.0, 0.4, 0.1);
    s
}
pub fn tiger_animaton() -> RobotStyle {
    let mut s = wolf_animaton();
    s.primary = Color::srgb(0.95, 0.55, 0.1);
    s.emissive = Color::srgb(1.0, 0.5, 0.0);
    s
}

// ── Charred ─────────────────────────────────────────────────────────────────
pub fn charred_captain() -> RobotStyle {
    let mut s = brute_forge();
    s.primary = Color::srgb(0.45, 0.18, 0.05);
    s.emissive = Color::srgb(1.0, 0.4, 0.05);
    s
}

// ── Swarm ───────────────────────────────────────────────────────────────────
pub fn cygnus() -> RobotStyle {
    let mut s = hybrid_omega();
    s.scale = 1.7;
    s.has_horns = true;
    s.horn_length = 28.0;
    s.primary = Color::srgb(0.1, 0.0, 0.2);
    s.emissive = Color::srgb(0.9, 0.0, 0.6);
    s
}
pub fn cygni() -> RobotStyle {
    let mut s = cygnus();
    s.primary = Color::srgb(0.3, 0.0, 0.1);
    s.emissive = Color::srgb(1.0, 0.0, 0.3);
    s
}
pub fn brutus() -> RobotStyle {
    let mut s = brute_forge();
    s.primary = Color::srgb(0.15, 0.0, 0.2);
    s.emissive = Color::srgb(0.7, 0.0, 0.9);
    s
}
pub fn nero() -> RobotStyle {
    let mut s = jet_warden();
    s.primary = Color::srgb(0.2, 0.0, 0.15);
    s.emissive = Color::srgb(0.95, 0.0, 0.6);
    s
}
pub fn minerva() -> RobotStyle {
    let mut s = tank_titan();
    s.primary = Color::srgb(0.25, 0.05, 0.3);
    s.emissive = Color::srgb(0.8, 0.0, 0.7);
    s
}
pub fn caliguon() -> RobotStyle {
    let mut s = hybrid_omega();
    s.primary = Color::srgb(0.3, 0.0, 0.05);
    s.emissive = Color::srgb(1.0, 0.1, 0.0);
    s
}
