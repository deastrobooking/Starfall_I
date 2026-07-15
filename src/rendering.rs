use bevy::prelude::*;

/// Local mesh bundle that keeps Starfall's existing field-style spawn code
/// without relying on Bevy's deprecated `PbrBundle` alias.
#[derive(Bundle)]
pub struct StarfallPbrBundle {
    pub mesh: Mesh3d,
    pub material: MeshMaterial3d<StandardMaterial>,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub visibility: Visibility,
    pub inherited_visibility: InheritedVisibility,
    pub view_visibility: ViewVisibility,
}

impl Default for StarfallPbrBundle {
    fn default() -> Self {
        Self {
            mesh: Mesh3d(default()),
            material: MeshMaterial3d(default()),
            transform: default(),
            global_transform: default(),
            visibility: default(),
            inherited_visibility: default(),
            view_visibility: default(),
        }
    }
}

/// Local spatial bundle for transform roots that used Bevy's deprecated
/// `SpatialBundle` helper.
#[derive(Bundle)]
pub struct StarfallSpatialBundle {
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub visibility: Visibility,
    pub inherited_visibility: InheritedVisibility,
    pub view_visibility: ViewVisibility,
}

impl StarfallSpatialBundle {
    pub fn from_transform(transform: Transform) -> Self {
        Self {
            transform,
            ..default()
        }
    }
}

impl Default for StarfallSpatialBundle {
    fn default() -> Self {
        Self {
            transform: default(),
            global_transform: default(),
            visibility: default(),
            inherited_visibility: default(),
            view_visibility: default(),
        }
    }
}

pub type PbrBundle = StarfallPbrBundle;
pub type SpatialBundle = StarfallSpatialBundle;

#[derive(Bundle)]
pub struct StarfallCamera3dBundle {
    pub camera_3d: Camera3d,
    pub camera: Camera,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub visibility: Visibility,
    pub inherited_visibility: InheritedVisibility,
    pub view_visibility: ViewVisibility,
}

impl Default for StarfallCamera3dBundle {
    fn default() -> Self {
        Self {
            camera_3d: default(),
            camera: default(),
            transform: default(),
            global_transform: default(),
            visibility: default(),
            inherited_visibility: default(),
            view_visibility: default(),
        }
    }
}

#[derive(Bundle)]
pub struct StarfallDirectionalLightBundle {
    pub directional_light: DirectionalLight,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub visibility: Visibility,
    pub inherited_visibility: InheritedVisibility,
    pub view_visibility: ViewVisibility,
}

impl Default for StarfallDirectionalLightBundle {
    fn default() -> Self {
        Self {
            directional_light: default(),
            transform: default(),
            global_transform: default(),
            visibility: default(),
            inherited_visibility: default(),
            view_visibility: default(),
        }
    }
}

#[derive(Bundle)]
pub struct StarfallPointLightBundle {
    pub point_light: PointLight,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub visibility: Visibility,
    pub inherited_visibility: InheritedVisibility,
    pub view_visibility: ViewVisibility,
}

impl Default for StarfallPointLightBundle {
    fn default() -> Self {
        Self {
            point_light: default(),
            transform: default(),
            global_transform: default(),
            visibility: default(),
            inherited_visibility: default(),
            view_visibility: default(),
        }
    }
}

pub type Camera3dBundle = StarfallCamera3dBundle;
pub type DirectionalLightBundle = StarfallDirectionalLightBundle;
pub type PointLightBundle = StarfallPointLightBundle;

use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

// ── Toon Material ─────────────────────────────────────────────────────────────
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ToonMaterial {
    #[uniform(0)]
    pub settings: ToonMaterialUniform,
}

#[derive(ShaderType, Debug, Clone, Copy)]
pub struct ToonMaterialUniform {
    pub color: Vec4,
    pub light_direction: Vec4,
    /// X/Y are the shadow and light thresholds; Z/W are shadow and mid levels.
    pub bands: Vec4,
    /// RGB is the rim color and W is rim strength.
    pub rim_color_strength: Vec4,
    /// X is rim threshold and Y is rim exponent.
    pub rim_shape: Vec4,
}

impl Default for ToonMaterialUniform {
    fn default() -> Self {
        Self {
            color: Vec4::new(0.18, 0.72, 1.0, 1.0),
            light_direction: Vec4::new(0.45, 1.0, 0.3, 0.0),
            bands: Vec4::new(0.12, 0.52, 0.28, 0.64),
            rim_color_strength: Vec4::new(0.55, 0.92, 1.0, 0.48),
            rim_shape: Vec4::new(0.58, 2.4, 0.0, 0.0),
        }
    }
}

impl Material for ToonMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/toon.wgsl".into()
    }
}

// ── Water Material ───────────────────────────────────────────────────────────

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct WaterMaterial {
    #[uniform(0)]
    pub settings: WaterMaterialUniform,
}

#[derive(ShaderType, Debug, Clone, Copy)]
pub struct WaterMaterialUniform {
    pub deep_color: Vec4,
    pub shallow_color: Vec4,
    /// X speed, Y spatial frequency, Z amplitude, W secondary-wave scale.
    pub wave: Vec4,
    /// X Fresnel exponent, Y base opacity, Z edge opacity, W light tint.
    pub surface: Vec4,
}

impl Default for WaterMaterialUniform {
    fn default() -> Self {
        Self {
            deep_color: Vec4::new(0.015, 0.10, 0.28, 0.82),
            shallow_color: Vec4::new(0.05, 0.56, 0.72, 0.72),
            wave: Vec4::new(0.72, 0.055, 0.16, 0.63),
            surface: Vec4::new(3.2, 0.66, 0.90, 0.25),
        }
    }
}

impl Material for WaterMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/water.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/water.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

// ── Energy Material ──────────────────────────────────────────────────────────

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct EnergyMaterial {
    #[uniform(0)]
    pub settings: EnergyMaterialUniform,
}

#[derive(ShaderType, Debug, Clone, Copy)]
pub struct EnergyMaterialUniform {
    pub core_color: Vec4,
    pub edge_color: Vec4,
    /// X flow speed, Y spatial frequency, Z pulse speed, W opacity.
    pub motion: Vec4,
}

impl Default for EnergyMaterialUniform {
    fn default() -> Self {
        Self {
            core_color: Vec4::new(1.0, 0.96, 0.72, 1.0),
            edge_color: Vec4::new(0.10, 0.72, 1.0, 1.0),
            motion: Vec4::new(2.4, 5.0, 3.2, 0.82),
        }
    }
}

impl Material for EnergyMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/energy.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }
}

// ── Shield Material ──────────────────────────────────────────────────────────

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ShieldMaterial {
    #[uniform(0)]
    pub settings: ShieldMaterialUniform,
}

#[derive(ShaderType, Debug, Clone, Copy)]
pub struct ShieldMaterialUniform {
    pub color: Vec4,
    /// X grid scale, Y line width, Z pulse speed, W opacity.
    pub pattern: Vec4,
    /// X Fresnel exponent, Y edge strength, Z impact pulse, W reserved.
    pub edge: Vec4,
}

impl Default for ShieldMaterialUniform {
    fn default() -> Self {
        Self {
            color: Vec4::new(0.20, 0.78, 1.0, 1.0),
            pattern: Vec4::new(7.0, 0.08, 2.2, 0.38),
            edge: Vec4::new(2.4, 0.85, 0.0, 0.0),
        }
    }
}

impl Material for ShieldMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/shield.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }
}

#[cfg(test)]
mod shader_contract_tests {
    use super::*;

    const TOON_SHADER: &str = include_str!("../assets/shaders/toon.wgsl");
    const GRASS_SHADER: &str = include_str!("../assets/shaders/grass.wgsl");
    const WATER_SHADER: &str = include_str!("../assets/shaders/water.wgsl");
    const ENERGY_SHADER: &str = include_str!("../assets/shaders/energy.wgsl");
    const SHIELD_SHADER: &str = include_str!("../assets/shaders/shield.wgsl");

    #[test]
    fn custom_shaders_use_bevy_material_bind_group_contract() {
        for shader in [
            TOON_SHADER,
            GRASS_SHADER,
            WATER_SHADER,
            ENERGY_SHADER,
            SHIELD_SHADER,
        ] {
            assert!(shader.contains("#{MATERIAL_BIND_GROUP}"));
            assert!(!shader.contains("@group(1)"));
        }
    }

    #[test]
    fn scene_lit_surface_shaders_consume_bevy_lights() {
        for shader in [TOON_SHADER, GRASS_SHADER, WATER_SHADER] {
            assert!(shader.contains("lights.n_directional_lights"));
            assert!(shader.contains("direction_to_light"));
        }
    }

    #[test]
    fn toon_bands_are_ordered_and_branchless() {
        let settings = ToonMaterialUniform::default();
        assert!(settings.bands.x < settings.bands.y);
        assert!(settings.bands.z < settings.bands.w);
        assert!(settings.bands.w < 1.0);
        assert!(TOON_SHADER.contains("step(toon.bands.x"));
        assert!(!TOON_SHADER.contains("if ndotl"));
    }

    #[test]
    fn transparent_material_defaults_stay_bounded() {
        let water = WaterMaterialUniform::default();
        let energy = EnergyMaterialUniform::default();
        let shield = ShieldMaterialUniform::default();
        assert!((0.0..=1.0).contains(&water.surface.y));
        assert!((0.0..=1.0).contains(&water.surface.z));
        assert!((0.0..=1.0).contains(&energy.motion.w));
        assert!((0.0..=1.0).contains(&shield.pattern.w));
    }
}
