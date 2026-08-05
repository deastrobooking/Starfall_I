use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

pub const WORLD_RENDER_LAYER: usize = 0;
pub const PLAYER_RENDER_LAYER_BASE: usize = 1;

pub fn player_render_layer(index: u8) -> usize {
    PLAYER_RENDER_LAYER_BASE + usize::from(index.min(3))
}

/// World geometry plus all possible local-player avatar layers.
///
/// Gameplay and Forge cameras that can coexist with live players use this
/// mask. Individual first-person cameras remove only their owner's layer.
pub fn all_gameplay_render_layers() -> RenderLayers {
    RenderLayers::from_layers(&[
        WORLD_RENDER_LAYER,
        player_render_layer(0),
        player_render_layer(1),
        player_render_layer(2),
        player_render_layer(3),
    ])
}

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

/// Custom-material bundle used by the streamed high/proxy terrain patches.
#[derive(Bundle)]
pub struct TerrainPbrBundle {
    pub mesh: Mesh3d,
    pub material: MeshMaterial3d<TerrainMaterial>,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub visibility: Visibility,
    pub inherited_visibility: InheritedVisibility,
    pub view_visibility: ViewVisibility,
}

impl Default for TerrainPbrBundle {
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

/// Custom-material equivalent of [`StarfallPbrBundle`] for emissive combat VFX.
#[derive(Bundle)]
pub struct EnergyPbrBundle {
    pub mesh: Mesh3d,
    pub material: MeshMaterial3d<EnergyMaterial>,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub visibility: Visibility,
    pub inherited_visibility: InheritedVisibility,
    pub view_visibility: ViewVisibility,
}

impl Default for EnergyPbrBundle {
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

/// Custom-material equivalent used by persistent per-player armor shells.
#[derive(Bundle)]
pub struct ShieldPbrBundle {
    pub mesh: Mesh3d,
    pub material: MeshMaterial3d<ShieldMaterial>,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
    pub visibility: Visibility,
    pub inherited_visibility: InheritedVisibility,
    pub view_visibility: ViewVisibility,
}

impl Default for ShieldPbrBundle {
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

// ── Terrain Material ─────────────────────────────────────────────────────────

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct TerrainMaterial {
    #[uniform(0)]
    pub settings: TerrainMaterialUniform,
    #[texture(1)]
    #[sampler(2)]
    pub grass_texture: Option<Handle<Image>>,
    #[texture(3)]
    #[sampler(4)]
    pub rock_texture: Option<Handle<Image>>,
    #[texture(5)]
    #[sampler(6)]
    pub snow_texture: Option<Handle<Image>>,
    #[texture(7)]
    #[sampler(8)]
    pub path_texture: Option<Handle<Image>>,
}

#[derive(ShaderType, Debug, Clone, Copy)]
pub struct TerrainMaterialUniform {
    pub grass_tint: Vec4,
    pub rock_tint: Vec4,
    pub snow_tint: Vec4,
    pub ice_tint: Vec4,
    pub path_tint: Vec4,
    /// X texture scale, Y tri-planar sharpness, Z normal detail, W path strength.
    pub detail: Vec4,
    /// X/Y rock slope blend, Z/W snow altitude blend.
    pub biome: Vec4,
    /// X ice start altitude, Y ice slope start, Z wetness height, W specular strength.
    pub climate: Vec4,
}

impl Default for TerrainMaterialUniform {
    fn default() -> Self {
        Self {
            grass_tint: Vec4::new(0.48, 0.68, 0.40, 1.0),
            rock_tint: Vec4::new(0.64, 0.61, 0.57, 1.0),
            snow_tint: Vec4::new(0.90, 0.95, 1.0, 1.0),
            ice_tint: Vec4::new(0.48, 0.78, 0.94, 1.0),
            path_tint: Vec4::new(0.58, 0.48, 0.34, 1.0),
            detail: Vec4::new(0.018, 5.0, 0.055, 0.92),
            biome: Vec4::new(0.10, 0.42, 470.0, 760.0),
            climate: Vec4::new(640.0, 0.20, 150.0, 0.26),
        }
    }
}

impl Material for TerrainMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/terrain.wgsl".into()
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
    /// RGB foam color; W is foam brightness.
    pub foam_color: Vec4,
    /// RGB horizon reflection tint; W is unused padding.
    pub horizon_color: Vec4,
    /// X speed, Y spatial frequency, Z amplitude, W secondary-wave scale.
    pub wave: Vec4,
    /// X Fresnel exponent, Y base opacity, Z edge opacity, W light tint.
    pub surface: Vec4,
    /// X specular strength, Y highlight exponent, Z forward scatter, W reflection.
    pub optics: Vec4,
    /// X crest threshold, Y softness, Z shoreline UV width, W shoreline strength.
    pub foam: Vec4,
}

impl Default for WaterMaterialUniform {
    fn default() -> Self {
        Self {
            deep_color: Vec4::new(0.008, 0.055, 0.16, 0.86),
            shallow_color: Vec4::new(0.025, 0.38, 0.50, 0.74),
            foam_color: Vec4::new(0.82, 0.96, 1.0, 0.90),
            horizon_color: Vec4::new(0.36, 0.62, 0.82, 0.0),
            wave: Vec4::new(0.68, 0.050, 0.18, 0.58),
            surface: Vec4::new(4.2, 0.70, 0.94, 0.32),
            optics: Vec4::new(0.95, 96.0, 0.28, 0.58),
            foam: Vec4::new(0.68, 0.12, 0.035, 0.75),
        }
    }
}

impl WaterMaterialUniform {
    /// Calmer inland water with no mesh-edge shoreline term. River sections are
    /// tiled, so disabling that term prevents artificial foam at every seam.
    pub fn river() -> Self {
        Self {
            deep_color: Vec4::new(0.012, 0.09, 0.18, 0.88),
            shallow_color: Vec4::new(0.035, 0.34, 0.38, 0.78),
            wave: Vec4::new(0.48, 0.075, 0.085, 0.44),
            optics: Vec4::new(0.72, 72.0, 0.36, 0.46),
            foam: Vec4::new(0.77, 0.10, 0.0, 0.0),
            ..Self::default()
        }
    }

    /// Broad inland lakes retain readable reflections but use shorter, softer
    /// waves than either open ocean or fast river water.
    pub fn lake() -> Self {
        Self {
            deep_color: Vec4::new(0.008, 0.075, 0.16, 0.90),
            shallow_color: Vec4::new(0.035, 0.30, 0.32, 0.80),
            wave: Vec4::new(0.31, 0.042, 0.055, 0.36),
            surface: Vec4::new(4.8, 0.74, 0.90, 0.28),
            optics: Vec4::new(0.82, 88.0, 0.30, 0.52),
            foam: Vec4::new(0.82, 0.09, 0.018, 0.30),
            ..Self::default()
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

// ── Ice / Snow Material ─────────────────────────────────────────────────────

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct IceMaterial {
    #[uniform(0)]
    pub settings: IceMaterialUniform,
}

#[derive(ShaderType, Debug, Clone, Copy)]
pub struct IceMaterialUniform {
    pub snow_color: Vec4,
    pub ice_color: Vec4,
    /// X detail scale, Y frost coverage, Z sparkle strength, W Fresnel strength.
    pub surface: Vec4,
}

impl Default for IceMaterialUniform {
    fn default() -> Self {
        Self {
            snow_color: Vec4::new(0.84, 0.94, 1.0, 1.0),
            ice_color: Vec4::new(0.08, 0.42, 0.68, 1.0),
            surface: Vec4::new(0.18, 0.42, 0.34, 0.38),
        }
    }
}

impl Material for IceMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/ice.wgsl".into()
    }
}

// ── Lava Material ───────────────────────────────────────────────────────────

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct LavaMaterial {
    #[uniform(0)]
    pub settings: LavaMaterialUniform,
}

#[derive(ShaderType, Debug, Clone, Copy)]
pub struct LavaMaterialUniform {
    pub crust_color: Vec4,
    pub hot_color: Vec4,
    /// X flow speed, Y cell scale, Z seam width, W emissive strength.
    pub motion: Vec4,
}

impl Default for LavaMaterialUniform {
    fn default() -> Self {
        Self {
            crust_color: Vec4::new(0.055, 0.025, 0.018, 1.0),
            hot_color: Vec4::new(1.0, 0.16, 0.015, 1.0),
            motion: Vec4::new(0.34, 0.21, 0.24, 1.65),
        }
    }
}

impl Material for LavaMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/lava.wgsl".into()
    }
}

#[cfg(test)]
mod shader_contract_tests {
    use super::*;

    const TOON_SHADER: &str = include_str!("../../assets/shaders/toon.wgsl");
    const TERRAIN_SHADER: &str = include_str!("../../assets/shaders/terrain.wgsl");
    const GRASS_SHADER: &str = include_str!("../../assets/shaders/grass.wgsl");
    const WATER_SHADER: &str = include_str!("../../assets/shaders/water.wgsl");
    const ENERGY_SHADER: &str = include_str!("../../assets/shaders/energy.wgsl");
    const SHIELD_SHADER: &str = include_str!("../../assets/shaders/shield.wgsl");
    const ICE_SHADER: &str = include_str!("../../assets/shaders/ice.wgsl");
    const LAVA_SHADER: &str = include_str!("../../assets/shaders/lava.wgsl");

    #[test]
    fn custom_shaders_use_bevy_material_bind_group_contract() {
        for shader in [
            TOON_SHADER,
            TERRAIN_SHADER,
            GRASS_SHADER,
            WATER_SHADER,
            ENERGY_SHADER,
            SHIELD_SHADER,
            ICE_SHADER,
            LAVA_SHADER,
        ] {
            assert!(shader.contains("#{MATERIAL_BIND_GROUP}"));
            assert!(!shader.contains("@group(1)"));
        }
    }

    #[test]
    fn scene_lit_surface_shaders_consume_bevy_lights() {
        for shader in [
            TOON_SHADER,
            TERRAIN_SHADER,
            GRASS_SHADER,
            WATER_SHADER,
            ICE_SHADER,
            LAVA_SHADER,
        ] {
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
    fn terrain_biome_thresholds_and_triplanar_contract_are_safe() {
        let settings = TerrainMaterialUniform::default();

        assert!(settings.detail.x > 0.0);
        assert!(settings.detail.y >= 1.0);
        assert!((0.0..=1.0).contains(&settings.detail.z));
        assert!((0.0..=1.0).contains(&settings.detail.w));
        assert!(settings.biome.x < settings.biome.y);
        assert!(settings.biome.z < settings.biome.w);
        assert!(settings.climate.x < settings.biome.w);
        assert!(TERRAIN_SHADER.contains("fn triplanar_sample"));
        assert!(TERRAIN_SHADER.contains("in.color.a"));
        assert!(TERRAIN_SHADER.contains("smoothstep(terrain.biome.z"));
    }

    #[test]
    fn transparent_material_defaults_stay_bounded() {
        let water = WaterMaterialUniform::default();
        let energy = EnergyMaterialUniform::default();
        let shield = ShieldMaterialUniform::default();
        let ice = IceMaterialUniform::default();
        let lava = LavaMaterialUniform::default();
        assert!((0.0..=1.0).contains(&water.surface.y));
        assert!((0.0..=1.0).contains(&water.surface.z));
        assert!((0.0..=1.0).contains(&water.optics.x));
        assert!(water.optics.y >= 1.0);
        assert!((0.0..=1.0).contains(&water.foam.w));
        assert!((0.0..=1.0).contains(&energy.motion.w));
        assert!((0.0..=1.0).contains(&shield.pattern.w));
        assert!((0.0..=1.0).contains(&ice.surface.y));
        assert!(lava.motion.w > 0.0 && lava.motion.w <= 2.0);
    }

    #[test]
    fn realistic_water_keeps_ocean_and_tiled_river_profiles_safe() {
        let ocean = WaterMaterialUniform::default();
        let river = WaterMaterialUniform::river();
        let lake = WaterMaterialUniform::lake();

        assert!(ocean.wave.z > river.wave.z);
        assert!(river.wave.z > lake.wave.z);
        assert!(ocean.foam.w > 0.0);
        assert!(lake.surface.y > 0.0 && lake.surface.y <= 1.0);
        assert_eq!(river.foam.z, 0.0);
        assert_eq!(river.foam.w, 0.0);
        assert!(WATER_SHADER.contains("let phase_d"));
        assert!(WATER_SHADER.contains("reflect(-view_direction, normal)"));
        assert!(WATER_SHADER.contains("let sun_glint"));
        assert!(WATER_SHADER.contains("let shoreline"));
        assert!(WATER_SHADER.contains("smoothstep(crest_start"));
    }
}
