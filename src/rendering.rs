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
