use bevy::{camera::RenderTarget, prelude::*, render::render_resource::TextureFormat};
use serde::{Deserialize, Serialize};

#[cfg(feature = "render-lab-meshlets")]
use super::GeometryBackendReport;
use super::{config::view_rects, LabConfig, LabScene, LabState};

#[cfg(feature = "render-lab-meshlets")]
use bevy::pbr::experimental::meshlet::{MeshletMesh, MeshletMesh3d};

#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneInventory {
    pub mesh_instances: u32,
    /// Subset of mesh_instances rendered by the experimental backend.
    #[serde(default)]
    pub meshlet_instances: u32,
    /// Submitted source topology before visibility/LOD; not a GPU counter.
    pub source_triangles: usize,
}

#[derive(Component)]
pub(crate) struct LabView(pub u32);

#[derive(Component)]
pub(crate) struct MovingOccluder;

pub(crate) fn setup(
    mut commands: Commands,
    config: Res<LabConfig>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    #[cfg(feature = "render-lab-meshlets")] mut backend: ResMut<GeometryBackendReport>,
    #[cfg(feature = "render-lab-meshlets")] meshlet_assets: Option<ResMut<Assets<MeshletMesh>>>,
) {
    let mut inventory = SceneInventory::default();
    match config.scene {
        LabScene::Geometry => {
            let mesh = geometry_fixture_mesh();
            let triangles = mesh.indices().map_or(0, |indices| indices.len() / 3);
            #[cfg(feature = "render-lab-meshlets")]
            let meshlet = if backend.active == super::LabRenderer::Meshlets {
                let start = std::time::Instant::now();
                let cooked = MeshletMesh::from_mesh(&mesh, config.meshlet_precision);
                backend.fixture_cook_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
                match (cooked, meshlet_assets) {
                    (Ok(mesh), Some(mut assets)) => Some(assets.add(mesh)),
                    (Err(error), _) => {
                        backend.fallback(format!("Fixture cooking failed: {error:?}"));
                        None
                    }
                    (_, None) => {
                        backend.fallback("Meshlet asset storage was not initialized".into());
                        None
                    }
                }
            } else {
                None
            };
            let mesh = meshes.add(mesh);
            let material = materials.add(StandardMaterial {
                base_color: Color::srgb(0.35, 0.55, 0.75),
                perceptual_roughness: 0.65,
                ..default()
            });
            for z in 0..config.grid {
                for x in 0..config.grid {
                    let h = 1.0 + ((x * 7 + z * 13) % 5) as f32 * 0.3;
                    let mut entity = commands.spawn((
                        MeshMaterial3d(material.clone()),
                        Transform::from_xyz(
                            (x as f32 - (config.grid - 1) as f32 * 0.5) * 3.0,
                            h,
                            (z as f32 - (config.grid - 1) as f32 * 0.5) * 3.0,
                        )
                        .with_scale(Vec3::new(1.0, h, 1.0)),
                    ));
                    #[cfg(feature = "render-lab-meshlets")]
                    if let Some(meshlet) = &meshlet {
                        entity.insert(MeshletMesh3d(meshlet.clone()));
                        inventory.meshlet_instances += 1;
                    } else {
                        entity.insert(Mesh3d(mesh.clone()));
                    }
                    #[cfg(not(feature = "render-lab-meshlets"))]
                    entity.insert(Mesh3d(mesh.clone()));
                    inventory.mesh_instances += 1;
                    inventory.source_triangles += triangles;
                }
            }
        }
        LabScene::Lighting => {
            let cube = meshes.add(Cuboid::default());
            for (position, scale, color) in [
                (
                    Vec3::new(-5.0, 3.0, 0.0),
                    Vec3::new(0.25, 6.0, 10.0),
                    Color::srgb(0.8, 0.08, 0.04),
                ),
                (
                    Vec3::new(5.0, 3.0, 0.0),
                    Vec3::new(0.25, 6.0, 10.0),
                    Color::srgb(0.06, 0.7, 0.15),
                ),
                (
                    Vec3::new(0.0, 3.0, -5.0),
                    Vec3::new(10.0, 6.0, 0.25),
                    Color::srgb(0.7, 0.7, 0.7),
                ),
                (
                    Vec3::new(-1.5, 1.5, 0.0),
                    Vec3::new(2.0, 3.0, 2.0),
                    Color::WHITE,
                ),
            ] {
                commands.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(materials.add(color)),
                    Transform::from_translation(position).with_scale(scale),
                ));
            }
            commands.spawn((
                Mesh3d(cube),
                MeshMaterial3d(materials.add(Color::srgb(0.3, 0.3, 0.3))),
                Transform::from_xyz(1.8, 1.5, 1.0).with_scale(Vec3::new(0.35, 3.0, 2.0)),
                MovingOccluder,
            ));
            commands.spawn((
                PointLight {
                    intensity: 250_000.0,
                    shadow_maps_enabled: config.shadows,
                    range: 20.0,
                    ..default()
                },
                Transform::from_xyz(0.0, 5.0, 1.0),
            ));
            inventory.mesh_instances = 5;
            inventory.source_triangles = 5 * 12;
        }
    }
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(150.0, 150.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 0.3, 0.3))),
    ));
    inventory.mesh_instances += 1;
    inventory.source_triangles += 2;
    commands.insert_resource(inventory);

    commands.spawn((
        DirectionalLight {
            illuminance: 8_000.0,
            shadow_maps_enabled: config.shadows,
            ..default()
        },
        Transform::from_xyz(8.0, 16.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // Bevy 0.19.1 meshlets allocate depth at viewport size; a sub-viewport
    // into a larger window creates mismatched render attachments. Full-target
    // cameras plus a shared compositor avoid that upstream restriction.
    // PBR uses the same path so comparative timings include identical composition.
    let compositor = commands
        .spawn((
            Camera2d,
            Camera {
                order: config.views as isize,
                ..default()
            },
            IsDefaultUiCamera,
            Msaa::Off,
        ))
        .id();
    for (index, rect) in view_rects(config.views, UVec2::new(config.width, config.height))
        .into_iter()
        .enumerate()
    {
        let image = images.add(Image::new_target_texture(
            rect.width(),
            rect.height(),
            TextureFormat::Bgra8UnormSrgb,
            None,
        ));
        commands.spawn((
            Camera3d::default(),
            Camera {
                order: index as isize,
                ..default()
            },
            RenderTarget::Image(image.clone().into()),
            Msaa::Off,
            Transform::default(),
            LabView(index as u32),
        ));
        commands.spawn((
            ImageNode::new(image),
            Node {
                position_type: PositionType::Absolute,
                left: px(rect.min.x as f32),
                top: px(rect.min.y as f32),
                width: px(rect.width() as f32),
                height: px(rect.height() as f32),
                ..default()
            },
            UiTargetCamera(compositor),
        ));
    }
}

fn geometry_fixture_mesh() -> Mesh {
    let mut mesh = Sphere::new(1.0).mesh().uv(64, 32);
    // The UV generator evaluates sin(TAU) and cos(PI/2), leaving nearly
    // coincident seam/pole positions. The meshlet processor's position remap
    // uses exact equality: separate simplification boundaries then open cracks.
    // Canonicalize the known unit-sphere duplicates in BOTH renderer inputs;
    // keep their distinct UVs and all original triangle indices.
    for attribute in [Mesh::ATTRIBUTE_POSITION, Mesh::ATTRIBUTE_NORMAL] {
        if let Some(bevy::mesh::VertexAttributeValues::Float32x3(values)) =
            mesh.attribute_mut(attribute)
        {
            for row in values.chunks_exact_mut(65) {
                row[64] = row[0];
            }
            values[..65].fill([0.0, 0.0, 1.0]);
            values[32 * 65..].fill([0.0, 0.0, -1.0]);
        }
    }
    mesh
}

pub(crate) fn animate(
    config: Res<LabConfig>,
    state: Res<LabState>,
    mut views: Query<(&LabView, &mut Transform), Without<MovingOccluder>>,
    mut occluders: Query<&mut Transform, With<MovingOccluder>>,
) {
    // Frame-indexed path gives every run identical views regardless of its speed.
    // The path restarts at sampling so warmup duration does not shift the experiment.
    let frame = state
        .frame
        .saturating_sub(u32::from(state.finished))
        .saturating_sub(config.warmup_frames);
    let phase = frame as f32 * 0.006;
    for (view, mut transform) in &mut views {
        let angle = phase + view.0 as f32 * std::f32::consts::TAU / config.views as f32;
        let position = match config.scene {
            LabScene::Geometry => {
                let radius = config.grid as f32 * 2.1 + 6.0;
                Vec3::new(angle.sin() * radius, radius * 0.48, angle.cos() * radius)
            }
            LabScene::Lighting => Vec3::new(angle.sin() * 3.5, 3.8, 11.0 + angle.cos()),
        };
        *transform =
            Transform::from_translation(position).looking_at(Vec3::new(0.0, 1.5, 0.0), Vec3::Y);
    }
    for mut transform in &mut occluders {
        transform.translation.x = 1.5 + phase.sin() * 1.2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_is_closed_under_exact_position_welding() {
        use std::collections::BTreeMap;
        let mesh = geometry_fixture_mesh();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("fixture must have positions");
        };
        let mut vertices = BTreeMap::new();
        let remap: Vec<_> = positions
            .iter()
            .map(|p| {
                let next = vertices.len();
                *vertices.entry(p.map(f32::to_bits)).or_insert(next)
            })
            .collect();
        let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
        let mut edges = BTreeMap::new();
        for triangle in indices.chunks_exact(3) {
            let [a, b, c] = [remap[triangle[0]], remap[triangle[1]], remap[triangle[2]]];
            assert!(a != b && b != c && c != a, "degenerate fixture triangle");
            for (u, v) in [(a, b), (b, c), (c, a)] {
                *edges.entry((u.min(v), u.max(v))).or_insert(0) += 1;
            }
        }
        assert!(
            edges.values().all(|count| *count == 2),
            "open or non-manifold seam"
        );
    }
}
