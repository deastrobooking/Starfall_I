//! Capability-gated adapter to Bevy's pinned experimental meshlet renderer.

use bevy::{
    prelude::*,
    render::{
        renderer::{RenderAdapterInfo, RenderDevice},
        settings::{Backends, WgpuFeatures},
    },
};
use serde::{Deserialize, Serialize};

use super::{LabConfig, LabRenderer, LabScene};

/// Matches Bevy 0.19.1; kept available in builds without its meshlet module.
pub(crate) fn required_features() -> WgpuFeatures {
    WgpuFeatures::TEXTURE_INT64_ATOMIC
        | WgpuFeatures::TEXTURE_ATOMIC
        | WgpuFeatures::SHADER_INT64
        | WgpuFeatures::SUBGROUP
        | WgpuFeatures::DEPTH_CLIP_CONTROL
        | WgpuFeatures::IMMEDIATES
}

/// Actual runtime selection, separate from the requested CLI renderer.
#[derive(Resource, Debug, Default, Clone, Serialize, Deserialize)]
pub struct GeometryBackendReport {
    pub active: LabRenderer,
    pub meshlet_support_compiled: bool,
    pub fallback_reason: Option<String>,
    pub plugin_installed: bool,
    /// Preallocated cluster slots, not a count of rendered clusters.
    pub cluster_buffer_slots: u32,
    /// One procedural source mesh cooked during Startup, before warmup.
    pub fixture_cook_ms: Option<f64>,
    pub meshlet_asset_version: Option<u64>,
}

impl GeometryBackendReport {
    pub(crate) fn fallback(&mut self, reason: String) {
        warn!("Meshlet request fell back to PBR: {reason}");
        self.active = LabRenderer::Pbr;
        self.fallback_reason = Some(reason);
    }
}

pub(crate) struct GeometryBackendPlugin;

impl Plugin for GeometryBackendPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GeometryBackendReport {
            meshlet_support_compiled: cfg!(feature = "render-lab-meshlets"),
            ..default()
        });
    }

    fn finish(&self, app: &mut App) {
        // Registered after DefaultPlugins: RenderPlugin::finish has installed
        // the actual device. Upstream's startup check exits the process on an
        // unsupported GPU, so do not install its plugin until this gate passes.
        let config = app.world().resource::<LabConfig>();
        if config.renderer == LabRenderer::Pbr {
            return;
        }
        let device = app.world().get_resource::<RenderDevice>();
        let adapter = app.world().get_resource::<RenderAdapterInfo>();
        let reason = eligibility_error(
            cfg!(feature = "render-lab-meshlets"),
            config.scene,
            adapter.is_some_and(|adapter| {
                (Backends::METAL | Backends::VULKAN).contains(adapter.backend.into())
            }),
            device.map_or(WgpuFeatures::empty(), RenderDevice::features),
        );
        if let Some(reason) = reason {
            app.world_mut()
                .resource_mut::<GeometryBackendReport>()
                .fallback(reason);
        } else {
            #[cfg(feature = "render-lab-meshlets")]
            install_meshlets(app);
        }
    }
}

#[cfg(feature = "render-lab-meshlets")]
fn install_meshlets(app: &mut App) {
    use bevy::pbr::experimental::meshlet::{MeshletPlugin, MESHLET_MESH_ASSET_VERSION};
    // 8 MiB of cluster slots; bounded independently of source density.
    let cluster_buffer_slots = 1 << 21;
    let plugin = MeshletPlugin {
        cluster_buffer_slots,
    };
    app.add_plugins(plugin);
    // App::finish's registry iteration excludes plugins just added here.
    // The pinned plugin currently has no finish hook; forward it explicitly
    // so that the lifecycle remains complete. Cleanup runs via the registry.
    MeshletPlugin {
        cluster_buffer_slots,
    }
    .finish(app);
    app.insert_resource(GeometryBackendReport {
        active: LabRenderer::Meshlets,
        meshlet_support_compiled: true,
        plugin_installed: true,
        cluster_buffer_slots,
        meshlet_asset_version: Some(MESHLET_MESH_ASSET_VERSION),
        ..default()
    });
}

fn eligibility_error(
    compiled: bool,
    scene: LabScene,
    backend_supported: bool,
    features: WgpuFeatures,
) -> Option<String> {
    if !compiled {
        Some("Build with --features render-lab-meshlets to enable the experimental backend".into())
    } else if scene != LabScene::Geometry {
        Some("The meshlet comparison currently supports only the geometry fixture".into())
    } else if !backend_supported {
        Some("The pinned meshlet renderer requires a Metal or Vulkan device".into())
    } else if !features.contains(required_features()) {
        Some(format!(
            "Device lacks required meshlet features: {:?}",
            required_features().difference(features)
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_device_keeps_the_app_alive_with_a_recorded_fallback() {
        let config = LabConfig::parse(
            ["--renderer", "meshlets", "--output", "unused.json"].map(String::from),
        )
        .unwrap();
        let mut app = App::new();
        app.insert_resource(config)
            .add_plugins(GeometryBackendPlugin);
        app.finish();
        let selection = app.world().resource::<GeometryBackendReport>();
        assert_eq!(selection.active, LabRenderer::Pbr);
        assert!(selection.fallback_reason.is_some());
        assert!(!selection.plugin_installed);
        assert_eq!(selection.cluster_buffer_slots, 0);
    }

    #[test]
    fn unsupported_requests_preserve_a_reason_and_cannot_install_the_backend() {
        let features = required_features();
        assert!(eligibility_error(true, LabScene::Geometry, true, features).is_none());
        for (compiled, scene, backend, available) in [
            (false, LabScene::Geometry, true, features),
            (true, LabScene::Lighting, true, features),
            (true, LabScene::Geometry, false, features),
            (
                true,
                LabScene::Geometry,
                true,
                features - WgpuFeatures::SUBGROUP,
            ),
        ] {
            assert!(eligibility_error(compiled, scene, backend, available).is_some());
        }
    }

    #[cfg(feature = "render-lab-meshlets")]
    #[test]
    fn capability_mask_matches_the_pinned_renderer() {
        assert_eq!(
            required_features(),
            bevy::pbr::experimental::meshlet::MeshletPlugin::required_wgpu_features()
        );
    }
}
