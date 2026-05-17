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
