use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// ── Archetypes ────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum RobotArchetype {
    #[default]
    Scout,
    Brute,
    Flyer,
    Tank,
    Insectoid,
    Hybrid,
    Pet,
    Ally,
}

// ── Head Shape ────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum HeadShape {
    #[default]
    Box,
    Sphere,
    Cylinder,
    Cone,
}

// ── Arm Style ─────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ArmStyle {
    #[default]
    Cylinder,
    Box,
    Tapered,
}

// ── Leg Style ─────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum LegStyle {
    #[default]
    Box,
    Digitigrade,
    Hoverpads,
}

// ── Visor Style ───────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum VisorStyle {
    #[default]
    Slit,
    Round,
    Full,
}

// ── Robot Style ───────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RobotStyle {
    // Base
    pub archetype: RobotArchetype,
    pub scale: f32,

    // Torso
    pub torso_width: f32,
    pub torso_height: f32,
    pub torso_depth: f32,

    // Head
    pub head_size: f32,
    pub head_shape: HeadShape,

    // Arms
    pub arm_length: f32,
    pub arm_thickness: f32,
    pub arm_style: ArmStyle,

    // Legs
    pub leg_length: f32,
    pub leg_thickness: f32,
    pub leg_style: LegStyle,

    // Pads
    pub shoulder_pad_size: f32,
    pub hip_pad_size: f32,

    // Wings
    pub has_wings: bool,
    pub wing_span: f32,
    pub wing_angle: f32,

    // Cannons
    pub has_cannons: bool,
    pub cannon_size: f32,

    // Backpack
    pub has_backpack: bool,
    pub backpack_size: f32,

    // Visor
    pub has_visor: bool,
    pub visor_style: VisorStyle,

    // Horns
    pub has_horns: bool,
    pub horn_length: f32,

    // Tail
    pub has_tail: bool,
    pub tail_length: f32,
    pub tail_segments: u32,

    // Antennae
    pub has_antennae: bool,
    pub antenna_length: f32,

    // Shield
    pub has_shield: bool,
    pub shield_size: f32,

    // Plating
    pub extra_plating: u32,
    pub asymmetry: f32,

    // Colors
    #[serde(with = "color_serde")]
    pub primary: Color,
    #[serde(with = "color_serde")]
    pub secondary: Color,
    #[serde(with = "color_serde")]
    pub emissive: Color,
}

mod color_serde {
    use bevy::prelude::Color;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(color: &Color, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let c = color.to_srgba();
        [c.red, c.green, c.blue, c.alpha].serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Color, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [r, g, b, a] = <[f32; 4]>::deserialize(deserializer)?;
        Ok(Color::srgba(r, g, b, a))
    }
}

impl Default for RobotStyle {
    fn default() -> Self {
        Self {
            archetype: RobotArchetype::Scout,
            scale: 1.0,
            torso_width: 18.0,
            torso_height: 38.0,
            torso_depth: 14.0,
            head_size: 10.0,
            head_shape: HeadShape::Box,
            arm_length: 28.0,
            arm_thickness: 6.0,
            arm_style: ArmStyle::Box,
            leg_length: 30.0,
            leg_thickness: 8.0,
            leg_style: LegStyle::Box,
            shoulder_pad_size: 7.0,
            hip_pad_size: 5.0,
            has_wings: false,
            wing_span: 30.0,
            wing_angle: 45.0,
            has_cannons: false,
            cannon_size: 3.0,
            has_backpack: false,
            backpack_size: 10.0,
            has_visor: true,
            visor_style: VisorStyle::Slit,
            has_horns: false,
            horn_length: 12.0,
            has_tail: false,
            tail_length: 20.0,
            tail_segments: 4,
            has_antennae: false,
            antenna_length: 20.0,
            has_shield: false,
            shield_size: 30.0,
            extra_plating: 0,
            asymmetry: 0.0,
            primary: Color::srgb(0.3, 0.5, 0.8),
            secondary: Color::srgb(0.15, 0.25, 0.4),
            emissive: Color::srgb(0.0, 0.8, 1.0),
        }
    }
}

impl RobotStyle {
    /// Build archetype-specific defaults.
    pub fn for_archetype(archetype: RobotArchetype) -> Self {
        let mut s = Self {
            archetype,
            ..Self::default()
        };
        match archetype {
            RobotArchetype::Scout => {}
            RobotArchetype::Brute => {
                s.torso_width = 30.0;
                s.torso_height = 50.0;
                s.torso_depth = 22.0;
                s.arm_thickness = 10.0;
                s.leg_thickness = 12.0;
                s.has_cannons = true;
                s.has_horns = true;
                s.extra_plating = 2;
                s.primary = Color::srgb(0.7, 0.1, 0.1);
                s.secondary = Color::srgb(0.35, 0.05, 0.05);
                s.emissive = Color::srgb(1.0, 0.3, 0.0);
            }
            RobotArchetype::Flyer => {
                s.has_wings = true;
                s.wing_span = 45.0;
                s.leg_style = LegStyle::Hoverpads;
                s.has_backpack = true;
                s.backpack_size = 14.0;
                s.primary = Color::srgb(0.3, 0.1, 0.5);
                s.secondary = Color::srgb(0.15, 0.05, 0.25);
                s.emissive = Color::srgb(0.6, 0.0, 1.0);
            }
            RobotArchetype::Tank => {
                s.torso_width = 38.0;
                s.torso_height = 55.0;
                s.torso_depth = 28.0;
                s.scale = 1.2;
                s.has_shield = true;
                s.has_horns = true;
                s.extra_plating = 3;
                s.arm_thickness = 12.0;
                s.leg_thickness = 14.0;
                s.primary = Color::srgb(0.15, 0.35, 0.15);
                s.secondary = Color::srgb(0.08, 0.18, 0.08);
                s.emissive = Color::srgb(0.0, 1.0, 0.2);
            }
            RobotArchetype::Insectoid => {
                s.leg_style = LegStyle::Digitigrade;
                s.has_wings = true;
                s.wing_span = 35.0;
                s.has_tail = true;
                s.has_antennae = true;
                s.scale = 0.9;
                s.primary = Color::srgb(0.15, 0.4, 0.1);
                s.secondary = Color::srgb(0.07, 0.2, 0.05);
                s.emissive = Color::srgb(0.2, 0.8, 0.0);
            }
            RobotArchetype::Hybrid => {
                s.scale = 1.4;
                s.head_shape = HeadShape::Cone;
                s.has_wings = true;
                s.has_tail = true;
                s.has_shield = true;
                s.has_cannons = true;
                s.has_horns = true;
                s.extra_plating = 2;
                s.primary = Color::srgb(0.3, 0.0, 0.4);
                s.secondary = Color::srgb(0.15, 0.0, 0.2);
                s.emissive = Color::srgb(0.8, 0.0, 1.0);
            }
            RobotArchetype::Pet => {
                s.scale = 0.4;
                s.has_tail = true;
                s.has_antennae = true;
                s.primary = Color::srgb(0.9, 0.75, 0.1);
                s.emissive = Color::srgb(1.0, 0.9, 0.0);
            }
            RobotArchetype::Ally => {
                s.has_cannons = true;
                s.has_backpack = true;
                s.primary = Color::srgb(0.1, 0.6, 0.6);
                s.emissive = Color::srgb(0.0, 1.0, 1.0);
            }
        }
        s
    }

    /// Clamp all numeric values to valid ranges.
    pub fn validate(&mut self) {
        self.scale = self.scale.clamp(0.3, 3.0);
        self.torso_width = self.torso_width.clamp(10.0, 50.0);
        self.torso_height = self.torso_height.clamp(20.0, 70.0);
        self.torso_depth = self.torso_depth.clamp(8.0, 35.0);
        self.head_size = self.head_size.clamp(5.0, 25.0);
        self.arm_length = self.arm_length.clamp(15.0, 55.0);
        self.arm_thickness = self.arm_thickness.clamp(3.0, 15.0);
        self.leg_length = self.leg_length.clamp(20.0, 55.0);
        self.leg_thickness = self.leg_thickness.clamp(5.0, 18.0);
        self.shoulder_pad_size = self.shoulder_pad_size.clamp(3.0, 18.0);
        self.hip_pad_size = self.hip_pad_size.clamp(2.0, 15.0);
        self.wing_span = self.wing_span.clamp(20.0, 70.0);
        self.wing_angle = self.wing_angle.clamp(0.0, 90.0);
        self.cannon_size = self.cannon_size.clamp(1.0, 10.0);
        self.backpack_size = self.backpack_size.clamp(4.0, 25.0);
        self.horn_length = self.horn_length.clamp(5.0, 35.0);
        self.tail_length = self.tail_length.clamp(10.0, 45.0);
        self.tail_segments = self.tail_segments.clamp(2, 8);
        self.antenna_length = self.antenna_length.clamp(10.0, 45.0);
        self.shield_size = self.shield_size.clamp(15.0, 55.0);
        self.extra_plating = self.extra_plating.min(3);
        self.asymmetry = self.asymmetry.clamp(0.0, 1.0);
    }
}
