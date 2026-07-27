//! Anime-RPG face system: eye styles, expressions, and the layered geometry
//! that makes a face read as a character rather than a mannequin.
//!
//! The previous face was a fixed sphere per eye, a fixed brow bar, a cuboid
//! mouth, and no customization beyond iris colour. Anime faces carry almost all
//! of their identity in the eyes, so this module models them properly:
//!
//! * **Layered eyes.** A real anime eye is five stacked shapes — sclera, iris,
//!   pupil, catchlight, and a heavy upper lash line. The catchlight in
//!   particular is what makes eyes look alive; a flat iris reads as a doll.
//! * **Shape as a style, proportion as a slider.** [`EyeStyle`] picks the
//!   silhouette (round, almond, sharp, gentle…) and the recipe's scalar knobs
//!   size and place it. Style sets *character*; sliders set *individuality*.
//! * **Expression is derived, not authored.** [`Expression`] resolves into brow
//!   angle/height, lid opening, and mouth curve through [`ExpressionPose`], so
//!   a designer picks "Angry" rather than dialing eleven numbers, and gameplay
//!   can drive the same poses at runtime.
//!
//! Everything here is pure data and maths so the whole face can be unit-tested
//! without spawning a rig. `character/parts.rs` consumes
//! [`FaceRecipe::eye_layout`] to place actual meshes.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Eye silhouette. The single biggest lever on how a character reads: the same
/// proportions with a different style land as a different personality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect, Default)]
pub enum EyeStyle {
    /// Tall and circular — youthful, earnest. The genre default.
    #[default]
    Round,
    /// Balanced oval, slightly longer than tall.
    Almond,
    /// Narrow with a strong upward outer corner (tsurime) — confident, sharp.
    Sharp,
    /// Soft with a downward outer corner (tareme) — gentle, warm.
    Gentle,
    /// Half-lidded and wide — bored, cool, or unbothered.
    Sleepy,
    /// Very tall and open with a large iris — innocent, expressive.
    Wide,
    /// Hard, narrow, low lash line — stoic or villainous.
    Fierce,
}

impl EyeStyle {
    pub const ALL: [EyeStyle; 7] = [
        EyeStyle::Round,
        EyeStyle::Almond,
        EyeStyle::Sharp,
        EyeStyle::Gentle,
        EyeStyle::Sleepy,
        EyeStyle::Wide,
        EyeStyle::Fierce,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EyeStyle::Round => "Round",
            EyeStyle::Almond => "Almond",
            EyeStyle::Sharp => "Sharp",
            EyeStyle::Gentle => "Gentle",
            EyeStyle::Sleepy => "Sleepy",
            EyeStyle::Wide => "Wide",
            EyeStyle::Fierce => "Fierce",
        }
    }

    /// (width, height) multipliers on the base eye size.
    fn silhouette(self) -> (f32, f32) {
        match self {
            EyeStyle::Round => (1.00, 1.15),
            EyeStyle::Almond => (1.15, 0.92),
            EyeStyle::Sharp => (1.20, 0.80),
            EyeStyle::Gentle => (1.08, 1.00),
            EyeStyle::Sleepy => (1.12, 0.66),
            EyeStyle::Wide => (1.05, 1.34),
            EyeStyle::Fierce => (1.22, 0.70),
        }
    }

    /// Outer-corner tilt in radians. Positive lifts the outer corner
    /// (tsurime); negative drops it (tareme).
    fn corner_tilt(self) -> f32 {
        match self {
            EyeStyle::Round => 0.02,
            EyeStyle::Almond => 0.06,
            EyeStyle::Sharp => 0.20,
            EyeStyle::Gentle => -0.14,
            EyeStyle::Sleepy => -0.06,
            EyeStyle::Wide => 0.0,
            EyeStyle::Fierce => 0.17,
        }
    }

    /// How much of the eye the upper lash line covers, 0..1. Heavier lids read
    /// as older, cooler, or more dangerous.
    fn lid_coverage(self) -> f32 {
        match self {
            EyeStyle::Round => 0.10,
            EyeStyle::Almond => 0.14,
            EyeStyle::Sharp => 0.22,
            EyeStyle::Gentle => 0.12,
            EyeStyle::Sleepy => 0.42,
            EyeStyle::Wide => 0.06,
            EyeStyle::Fierce => 0.30,
        }
    }

    /// Iris size relative to the sclera. Large irises are the genre's shorthand
    /// for youth and sincerity; small ones for menace.
    fn iris_ratio(self) -> f32 {
        match self {
            EyeStyle::Round => 0.78,
            EyeStyle::Almond => 0.72,
            EyeStyle::Sharp => 0.66,
            EyeStyle::Gentle => 0.76,
            EyeStyle::Sleepy => 0.70,
            EyeStyle::Wide => 0.86,
            EyeStyle::Fierce => 0.58,
        }
    }
}

/// Eyebrow silhouette, the second-biggest expression carrier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect, Default)]
pub enum BrowStyle {
    #[default]
    Soft,
    Straight,
    Arched,
    Thick,
    Thin,
}

impl BrowStyle {
    pub const ALL: [BrowStyle; 5] = [
        BrowStyle::Soft,
        BrowStyle::Straight,
        BrowStyle::Arched,
        BrowStyle::Thick,
        BrowStyle::Thin,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BrowStyle::Soft => "Soft",
            BrowStyle::Straight => "Straight",
            BrowStyle::Arched => "Arched",
            BrowStyle::Thick => "Thick",
            BrowStyle::Thin => "Thin",
        }
    }

    /// (length, thickness) multipliers.
    fn shape(self) -> (f32, f32) {
        match self {
            BrowStyle::Soft => (1.00, 1.00),
            BrowStyle::Straight => (1.10, 0.95),
            BrowStyle::Arched => (0.95, 0.88),
            BrowStyle::Thick => (1.05, 1.55),
            BrowStyle::Thin => (0.92, 0.60),
        }
    }

    /// Resting arch in radians, before expression is applied.
    fn resting_angle(self) -> f32 {
        match self {
            BrowStyle::Soft => 0.10,
            BrowStyle::Straight => 0.02,
            BrowStyle::Arched => 0.22,
            BrowStyle::Thick => 0.08,
            BrowStyle::Thin => 0.14,
        }
    }
}

/// A named emotional pose. Chosen by designers and drivable at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect, Default)]
pub enum Expression {
    #[default]
    Neutral,
    Happy,
    Angry,
    Surprised,
    Sad,
    Determined,
    /// Eyes closed in a smile — the classic ^_^.
    Joyful,
}

impl Expression {
    pub const ALL: [Expression; 7] = [
        Expression::Neutral,
        Expression::Happy,
        Expression::Angry,
        Expression::Surprised,
        Expression::Sad,
        Expression::Determined,
        Expression::Joyful,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Expression::Neutral => "Neutral",
            Expression::Happy => "Happy",
            Expression::Angry => "Angry",
            Expression::Surprised => "Surprised",
            Expression::Sad => "Sad",
            Expression::Determined => "Determined",
            Expression::Joyful => "Joyful",
        }
    }

    /// Resolve into concrete pose deltas.
    pub fn pose(self) -> ExpressionPose {
        match self {
            Expression::Neutral => ExpressionPose {
                brow_angle: 0.0,
                brow_height: 0.0,
                lid_close: 0.0,
                mouth_curve: 0.0,
                mouth_open: 0.0,
                iris_scale: 1.0,
            },
            // Brows up and relaxed, eyes softly narrowed, clear smile.
            Expression::Happy => ExpressionPose {
                brow_angle: -0.06,
                brow_height: 0.014,
                lid_close: 0.12,
                mouth_curve: 0.85,
                mouth_open: 0.10,
                iris_scale: 1.04,
            },
            // Inner brows driven down and together, lids tight, mouth set.
            Expression::Angry => ExpressionPose {
                brow_angle: 0.42,
                brow_height: -0.020,
                lid_close: 0.28,
                mouth_curve: -0.55,
                mouth_open: 0.05,
                iris_scale: 0.88,
            },
            // Everything opens; the iris shrinks inside a wide sclera.
            Expression::Surprised => ExpressionPose {
                brow_angle: -0.16,
                brow_height: 0.038,
                lid_close: -0.30,
                mouth_curve: 0.0,
                mouth_open: 0.55,
                iris_scale: 0.82,
            },
            // Inner brows lifted — the shape that reads as sadness.
            Expression::Sad => ExpressionPose {
                brow_angle: -0.34,
                brow_height: 0.010,
                lid_close: 0.20,
                mouth_curve: -0.60,
                mouth_open: 0.0,
                iris_scale: 1.08,
            },
            Expression::Determined => ExpressionPose {
                brow_angle: 0.26,
                brow_height: -0.012,
                lid_close: 0.14,
                mouth_curve: -0.15,
                mouth_open: 0.0,
                iris_scale: 0.96,
            },
            // Closed happy eyes: the lids meet completely.
            Expression::Joyful => ExpressionPose {
                brow_angle: -0.12,
                brow_height: 0.020,
                lid_close: 1.0,
                mouth_curve: 1.0,
                mouth_open: 0.25,
                iris_scale: 1.0,
            },
        }
    }
}

/// Concrete deltas an [`Expression`] applies on top of the resting face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpressionPose {
    /// Added brow arch, radians. **Positive drives the inner ends down** (the
    /// anger shape); negative lifts them (the sadness shape). The per-side
    /// mirroring in [`FaceRecipe::brow_layout`] turns this into opposite
    /// rotations so both brows converge instead of tilting the same way.
    pub brow_angle: f32,
    /// Vertical brow offset in head-local units.
    pub brow_height: f32,
    /// Extra lid closure, 0..1. Negative opens the eye wider than rest.
    pub lid_close: f32,
    /// Mouth corner curve, -1 (frown) to 1 (smile).
    pub mouth_curve: f32,
    /// Mouth opening, 0..1.
    pub mouth_open: f32,
    /// Iris scale multiplier — dilation carries a surprising amount of mood.
    pub iris_scale: f32,
}

/// A complete authored face. Serializable so it saves with the character and
/// `serde(default)`-friendly for older records.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub struct FaceRecipe {
    pub eye_style: EyeStyle,
    pub brow_style: BrowStyle,
    pub expression: Expression,
    /// Overall eye scale. Anime eyes run large; 1.0 is already stylized.
    pub eye_size: f32,
    /// Horizontal separation multiplier.
    pub eye_spacing: f32,
    /// Vertical placement on the face; positive is higher.
    pub eye_height: f32,
    /// Extra tilt on top of the style's own, radians.
    pub eye_tilt: f32,
    /// Pupil size relative to the iris.
    pub pupil_scale: f32,
    /// Catchlight size relative to the iris. Zero removes the sparkle and the
    /// face immediately reads as lifeless — kept adjustable for exactly that.
    pub highlight_scale: f32,
    /// Upper lash thickness multiplier.
    pub lash_thickness: f32,
    pub brow_height: f32,
    /// Anime noses are barely there; 1.0 is already small.
    pub nose_scale: f32,
    pub mouth_width: f32,
}

impl FaceRecipe {
    /// The neutral round-eyed face. A `const` so presets can spread it with
    /// `..FaceRecipe::DEFAULT` and only name the fields they change.
    pub const DEFAULT: Self = Self {
        eye_style: EyeStyle::Round,
        brow_style: BrowStyle::Soft,
        expression: Expression::Neutral,
        eye_size: 1.0,
        eye_spacing: 1.0,
        eye_height: 0.0,
        eye_tilt: 0.0,
        pupil_scale: 0.46,
        highlight_scale: 0.30,
        lash_thickness: 1.0,
        brow_height: 0.0,
        nose_scale: 1.0,
        mouth_width: 1.0,
    };
}

impl Default for FaceRecipe {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Sane authoring ranges. The designer clamps to these so a slider can never
/// produce a face that renders inside-out.
pub const EYE_SIZE_RANGE: (f32, f32) = (0.65, 1.75);
pub const EYE_SPACING_RANGE: (f32, f32) = (0.72, 1.35);
pub const EYE_HEIGHT_RANGE: (f32, f32) = (-0.05, 0.05);
pub const EYE_TILT_RANGE: (f32, f32) = (-0.35, 0.35);
pub const PUPIL_RANGE: (f32, f32) = (0.20, 0.80);
pub const HIGHLIGHT_RANGE: (f32, f32) = (0.0, 0.55);
pub const LASH_RANGE: (f32, f32) = (0.0, 2.2);
pub const NOSE_RANGE: (f32, f32) = (0.0, 1.8);
pub const MOUTH_WIDTH_RANGE: (f32, f32) = (0.6, 1.6);
pub const BROW_HEIGHT_RANGE: (f32, f32) = (-0.03, 0.04);

fn clamp_to(value: f32, range: (f32, f32)) -> f32 {
    value.clamp(range.0, range.1)
}

/// Resolved placement and size of every piece of one eye, in head-local units
/// relative to the eye centre. `parts.rs` turns these into meshes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EyeLayout {
    /// Horizontal offset from face centre (already signed for the side).
    pub offset_x: f32,
    pub offset_y: f32,
    /// Sclera half-extents.
    pub sclera_half_width: f32,
    pub sclera_half_height: f32,
    /// Iris radius.
    pub iris_radius: f32,
    pub pupil_radius: f32,
    /// Catchlight radius, and its offset from the iris centre. Placing it up
    /// and toward the outer corner is what sells a glossy anime eye.
    pub highlight_radius: f32,
    pub highlight_offset: Vec2,
    /// Rotation of the whole eye, radians, signed for the side.
    pub tilt: f32,
    /// Upper lash bar thickness, and how far down it sits over the sclera.
    pub lash_thickness: f32,
    pub lash_drop: f32,
    /// 0..1 — how closed the eye is. 1.0 means fully shut (draw a lash arc
    /// instead of an open eye).
    pub closure: f32,
}

impl EyeLayout {
    /// A fully closed eye is drawn as a curved lash line, not a squashed one.
    pub fn is_closed(&self) -> bool {
        self.closure >= 0.995
    }
}

/// Resolved brow placement for one side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrowLayout {
    pub offset_x: f32,
    pub offset_y: f32,
    pub length: f32,
    pub thickness: f32,
    /// Signed rotation for the side.
    pub angle: f32,
}

/// Resolved mouth shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouthLayout {
    pub width: f32,
    pub height: f32,
    /// Vertical offset of the corners; positive curves up into a smile.
    pub corner_lift: f32,
    pub open: f32,
}

/// Which side of the face a feature belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceSide {
    Left,
    Right,
}

impl FaceSide {
    /// -1 for the character's left, +1 for their right.
    fn sign(self) -> f32 {
        match self {
            FaceSide::Left => -1.0,
            FaceSide::Right => 1.0,
        }
    }
}

/// Base eye half-width before styling, in head-local units.
const BASE_EYE_HALF_WIDTH: f32 = 0.062;
const BASE_EYE_HALF_HEIGHT: f32 = 0.052;
const BASE_EYE_SEPARATION: f32 = 0.132;

impl FaceRecipe {
    /// Clamp every slider into its authoring range. Called after loading a
    /// record and after any UI edit, so out-of-range data can never reach the
    /// mesh builder.
    pub fn sanitized(mut self) -> Self {
        self.eye_size = clamp_to(self.eye_size, EYE_SIZE_RANGE);
        self.eye_spacing = clamp_to(self.eye_spacing, EYE_SPACING_RANGE);
        self.eye_height = clamp_to(self.eye_height, EYE_HEIGHT_RANGE);
        self.eye_tilt = clamp_to(self.eye_tilt, EYE_TILT_RANGE);
        self.pupil_scale = clamp_to(self.pupil_scale, PUPIL_RANGE);
        self.highlight_scale = clamp_to(self.highlight_scale, HIGHLIGHT_RANGE);
        self.lash_thickness = clamp_to(self.lash_thickness, LASH_RANGE);
        self.nose_scale = clamp_to(self.nose_scale, NOSE_RANGE);
        self.mouth_width = clamp_to(self.mouth_width, MOUTH_WIDTH_RANGE);
        self.brow_height = clamp_to(self.brow_height, BROW_HEIGHT_RANGE);
        self
    }

    /// Solve one eye's geometry, style and expression combined.
    pub fn eye_layout(&self, side: FaceSide) -> EyeLayout {
        let face = self.sanitized();
        let pose = face.expression.pose();
        let (style_w, style_h) = face.eye_style.silhouette();
        let sign = side.sign();

        // Lid closure combines the style's resting heaviness with the
        // expression, clamped so an eye can shut but never invert.
        let closure = (face.eye_style.lid_coverage() + pose.lid_close).clamp(0.0, 1.0);
        let half_width = BASE_EYE_HALF_WIDTH * style_w * face.eye_size;
        // Closing narrows the visible sclera vertically.
        let open_fraction = 1.0 - closure;
        let half_height = BASE_EYE_HALF_HEIGHT * style_h * face.eye_size * open_fraction.max(0.02);

        let iris_radius = half_width.min(BASE_EYE_HALF_HEIGHT * style_h * face.eye_size)
            * face.eye_style.iris_ratio()
            * pose.iris_scale;

        EyeLayout {
            offset_x: sign * BASE_EYE_SEPARATION * face.eye_spacing,
            offset_y: face.eye_height,
            sclera_half_width: half_width,
            sclera_half_height: half_height,
            iris_radius,
            pupil_radius: iris_radius * face.pupil_scale,
            highlight_radius: iris_radius * face.highlight_scale,
            // Up and toward the outer corner — a shared light source reads as
            // deliberate; centring it reads as a dead stare.
            highlight_offset: Vec2::new(sign * iris_radius * 0.34, iris_radius * 0.38),
            tilt: sign * (face.eye_style.corner_tilt() + face.eye_tilt),
            lash_thickness: half_height.max(0.004) * 0.30 * face.lash_thickness,
            lash_drop: half_height * 0.92,
            closure,
        }
    }

    /// Solve one brow.
    pub fn brow_layout(&self, side: FaceSide) -> BrowLayout {
        let face = self.sanitized();
        let pose = face.expression.pose();
        let (length_mult, thickness_mult) = face.brow_style.shape();
        let eye = face.eye_layout(side);
        let sign = side.sign();

        BrowLayout {
            offset_x: eye.offset_x,
            // Sits above the eye, lifted by expression and the slider.
            offset_y: eye.offset_y + 0.085 * face.eye_size + face.brow_height + pose.brow_height,
            length: 0.135 * length_mult * face.eye_size,
            thickness: 0.024 * thickness_mult,
            // The sign flip is what makes both brows angle *inward* together;
            // without it an angry face looks quizzical on one side.
            angle: sign * (face.brow_style.resting_angle() + pose.brow_angle),
        }
    }

    /// Solve the mouth.
    pub fn mouth_layout(&self) -> MouthLayout {
        let face = self.sanitized();
        let pose = face.expression.pose();
        MouthLayout {
            width: 0.128 * face.mouth_width,
            height: 0.018 + pose.mouth_open * 0.055,
            corner_lift: pose.mouth_curve * 0.026,
            open: pose.mouth_open,
        }
    }

    /// Nose size multiplier, already sanitized.
    pub fn nose_size(&self) -> f32 {
        self.sanitized().nose_scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_face_is_symmetric_and_open() {
        let face = FaceRecipe::default();
        let left = face.eye_layout(FaceSide::Left);
        let right = face.eye_layout(FaceSide::Right);

        // Mirrored horizontally, identical vertically.
        assert!((left.offset_x + right.offset_x).abs() < 1e-6);
        assert!(left.offset_x < 0.0 && right.offset_x > 0.0);
        assert_eq!(left.offset_y, right.offset_y);
        assert_eq!(left.sclera_half_width, right.sclera_half_width);
        assert!((left.tilt + right.tilt).abs() < 1e-6, "tilt must mirror");
        assert!(!left.is_closed());
        // A default face has a visible catchlight — without it eyes read dead.
        assert!(left.highlight_radius > 0.0);
        assert!(left.iris_radius > left.pupil_radius);
        assert!(left.sclera_half_width > left.iris_radius);
    }

    #[test]
    fn eye_styles_produce_genuinely_different_silhouettes() {
        let of = |style| {
            FaceRecipe {
                eye_style: style,
                ..Default::default()
            }
            .eye_layout(FaceSide::Left)
        };
        let round = of(EyeStyle::Round);
        let sharp = of(EyeStyle::Sharp);
        let gentle = of(EyeStyle::Gentle);
        let sleepy = of(EyeStyle::Sleepy);
        let wide = of(EyeStyle::Wide);

        // Sharp is narrower and tilts the outer corner up; gentle tilts down.
        assert!(sharp.sclera_half_height < round.sclera_half_height);
        assert!(sharp.tilt.abs() > round.tilt.abs());
        assert!(
            gentle.tilt.signum() != sharp.tilt.signum(),
            "tsurime and tareme must lean opposite ways"
        );
        // Sleepy is the most lidded of the open styles; wide is the tallest.
        assert!(sleepy.closure > round.closure);
        assert!(wide.sclera_half_height > round.sclera_half_height);
        assert!(wide.iris_radius > sharp.iris_radius, "wide eyes, big iris");
    }

    #[test]
    fn expressions_move_brows_the_way_faces_actually_do() {
        let face_with = |expression| FaceRecipe {
            expression,
            ..Default::default()
        };

        let neutral = face_with(Expression::Neutral).brow_layout(FaceSide::Left);
        let angry = face_with(Expression::Angry).brow_layout(FaceSide::Left);
        let sad = face_with(Expression::Sad).brow_layout(FaceSide::Left);
        let surprised = face_with(Expression::Surprised).brow_layout(FaceSide::Left);

        // Anger drives brows down and in; sadness lifts the inner ends;
        // surprise raises them bodily.
        assert!(angry.angle < neutral.angle);
        assert!(sad.angle > neutral.angle);
        assert!(surprised.offset_y > neutral.offset_y);
        assert!(angry.offset_y < neutral.offset_y);

        // Both brows must angle together, not both the same absolute way.
        let angry_right = face_with(Expression::Angry).brow_layout(FaceSide::Right);
        assert!(
            (angry.angle + angry_right.angle).abs() < 1e-6,
            "brows must mirror so anger does not look quizzical"
        );
    }

    #[test]
    fn expressions_reshape_eyes_and_mouth() {
        let of = |expression| FaceRecipe {
            expression,
            ..Default::default()
        };
        let neutral = of(Expression::Neutral);
        let surprised = of(Expression::Surprised);
        let joyful = of(Expression::Joyful);

        // Surprise opens the lids and shrinks the iris inside a wide sclera.
        assert!(
            surprised.eye_layout(FaceSide::Left).sclera_half_height
                > neutral.eye_layout(FaceSide::Left).sclera_half_height
        );
        assert!(
            surprised.eye_layout(FaceSide::Left).iris_radius
                < neutral.eye_layout(FaceSide::Left).iris_radius
        );
        assert!(surprised.mouth_layout().open > neutral.mouth_layout().open);

        // Joyful closes the eyes completely — the ^_^ face.
        assert!(joyful.eye_layout(FaceSide::Left).is_closed());
        assert!(joyful.mouth_layout().corner_lift > 0.0, "and smiles");

        // Smile vs frown must actually differ in sign.
        assert!(of(Expression::Sad).mouth_layout().corner_lift < 0.0);
        assert!(of(Expression::Happy).mouth_layout().corner_lift > 0.0);
    }

    #[test]
    fn a_closed_eye_never_inverts_no_matter_the_style() {
        // Sleepy already carries heavy lids; Joyful closes fully. Combined
        // they must clamp shut rather than produce negative geometry.
        for style in EyeStyle::ALL {
            let layout = FaceRecipe {
                eye_style: style,
                expression: Expression::Joyful,
                ..Default::default()
            }
            .eye_layout(FaceSide::Left);
            assert!(layout.closure <= 1.0);
            assert!(layout.sclera_half_height > 0.0, "{style:?} inverted");
            assert!(layout.is_closed(), "{style:?} should shut when joyful");
        }
    }

    #[test]
    fn sliders_are_clamped_so_the_designer_cannot_break_a_face() {
        let wild = FaceRecipe {
            eye_size: 99.0,
            eye_spacing: -12.0,
            pupil_scale: 5.0,
            highlight_scale: -3.0,
            lash_thickness: 50.0,
            nose_scale: -1.0,
            mouth_width: 0.0,
            eye_tilt: 12.0,
            ..Default::default()
        }
        .sanitized();

        assert!(wild.eye_size <= EYE_SIZE_RANGE.1);
        assert!(wild.eye_spacing >= EYE_SPACING_RANGE.0);
        assert!(wild.pupil_scale <= PUPIL_RANGE.1);
        assert!(wild.highlight_scale >= 0.0);
        assert!(wild.lash_thickness <= LASH_RANGE.1);
        assert!(wild.nose_scale >= 0.0);
        assert!(wild.mouth_width >= MOUTH_WIDTH_RANGE.0);
        assert!(wild.eye_tilt <= EYE_TILT_RANGE.1);

        // And the resulting geometry is still sane.
        let layout = wild.eye_layout(FaceSide::Left);
        assert!(layout.sclera_half_width > 0.0 && layout.sclera_half_height > 0.0);
        assert!(layout.pupil_radius <= layout.iris_radius);
    }

    #[test]
    fn eye_sliders_scale_the_whole_eye_together() {
        let small = FaceRecipe {
            eye_size: 0.7,
            ..Default::default()
        }
        .eye_layout(FaceSide::Left);
        let large = FaceRecipe {
            eye_size: 1.6,
            ..Default::default()
        }
        .eye_layout(FaceSide::Left);

        // Every layer scales, so a big eye is not a small iris in a big white.
        assert!(large.sclera_half_width > small.sclera_half_width);
        assert!(large.iris_radius > small.iris_radius);
        assert!(large.pupil_radius > small.pupil_radius);
        assert!(large.highlight_radius > small.highlight_radius);

        // Spacing moves eyes apart without resizing them.
        let wide = FaceRecipe {
            eye_spacing: 1.3,
            ..Default::default()
        }
        .eye_layout(FaceSide::Left);
        let narrow = FaceRecipe {
            eye_spacing: 0.8,
            ..Default::default()
        }
        .eye_layout(FaceSide::Left);
        assert!(wide.offset_x.abs() > narrow.offset_x.abs());
        assert_eq!(wide.sclera_half_width, narrow.sclera_half_width);
    }

    #[test]
    fn catchlight_sits_up_and_outward_on_both_eyes() {
        let face = FaceRecipe::default();
        let left = face.eye_layout(FaceSide::Left);
        let right = face.eye_layout(FaceSide::Right);

        // Above centre on both, and mirrored horizontally so the pair reads as
        // one light source rather than two unrelated sparkles.
        assert!(left.highlight_offset.y > 0.0 && right.highlight_offset.y > 0.0);
        assert!((left.highlight_offset.x + right.highlight_offset.x).abs() < 1e-6);
        assert!(left.highlight_radius < left.iris_radius);
    }

    #[test]
    fn designer_cycling_visits_every_choice_and_returns_home() {
        // Mirrors the FACE panel's cycle buttons: pressing one N times must
        // walk the whole list and land back where it started, so no style is
        // ever unreachable in the UI.
        fn next<T: PartialEq + Copy>(all: &[T], current: T) -> T {
            let index = all.iter().position(|v| *v == current).unwrap_or(0);
            all[(index + 1) % all.len()]
        }

        let mut style = EyeStyle::default();
        let mut seen = vec![style];
        for _ in 1..EyeStyle::ALL.len() {
            style = next(&EyeStyle::ALL, style);
            assert!(!seen.contains(&style), "cycle repeated before wrapping");
            seen.push(style);
        }
        assert_eq!(seen.len(), EyeStyle::ALL.len(), "a style is unreachable");
        assert_eq!(
            next(&EyeStyle::ALL, style),
            EyeStyle::default(),
            "must wrap"
        );

        let mut expression = Expression::default();
        for _ in 0..Expression::ALL.len() {
            expression = next(&Expression::ALL, expression);
        }
        assert_eq!(expression, Expression::default());
    }

    #[test]
    fn every_style_and_expression_is_labelled_and_round_trips() {
        for style in EyeStyle::ALL {
            assert!(!style.label().is_empty());
            let json = serde_json::to_string(&style).unwrap();
            assert_eq!(serde_json::from_str::<EyeStyle>(&json).unwrap(), style);
        }
        for brow in BrowStyle::ALL {
            assert!(!brow.label().is_empty());
        }
        for expression in Expression::ALL {
            assert!(!expression.label().is_empty());
            let json = serde_json::to_string(&expression).unwrap();
            assert_eq!(
                serde_json::from_str::<Expression>(&json).unwrap(),
                expression
            );
        }
        // The whole recipe survives a save round trip.
        let face = FaceRecipe {
            eye_style: EyeStyle::Sharp,
            expression: Expression::Determined,
            eye_size: 1.3,
            ..Default::default()
        };
        let json = serde_json::to_string(&face).unwrap();
        assert_eq!(serde_json::from_str::<FaceRecipe>(&json).unwrap(), face);
    }
}
