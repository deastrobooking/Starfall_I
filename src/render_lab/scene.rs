use bevy::{camera::Viewport, prelude::*};
use serde::{Deserialize, Serialize};

use super::{config::view_rects, LabConfig, LabScene, LabState};

#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneInventory {
    pub mesh_instances: u32,
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
) {
    let mut inventory = SceneInventory::default();
    match config.scene {
        LabScene::Geometry => {
            let mesh = Sphere::new(1.0).mesh().uv(64, 32);
            let triangles = mesh.indices().map_or(0, |indices| indices.len() / 3);
            let mesh = meshes.add(mesh);
            let material = materials.add(StandardMaterial {
                base_color: Color::srgb(0.35, 0.55, 0.75),
                perceptual_roughness: 0.65,
                ..default()
            });
            for z in 0..config.grid {
                for x in 0..config.grid {
                    let h = 1.0 + ((x * 7 + z * 13) % 5) as f32 * 0.3;
                    commands.spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::from_xyz(
                            (x as f32 - (config.grid - 1) as f32 * 0.5) * 3.0,
                            h,
                            (z as f32 - (config.grid - 1) as f32 * 0.5) * 3.0,
                        )
                        .with_scale(Vec3::new(1.0, h, 1.0)),
                    ));
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
                    shadow_maps_enabled: true,
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
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(8.0, 16.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    for (index, rect) in view_rects(config.views, UVec2::new(config.width, config.height))
        .into_iter()
        .enumerate()
    {
        commands.spawn((
            Camera3d::default(),
            Camera {
                order: index as isize,
                viewport: Some(Viewport {
                    physical_position: rect.min,
                    physical_size: rect.size(),
                    ..default()
                }),
                ..default()
            },
            Msaa::Off,
            Transform::default(),
            LabView(index as u32),
        ));
    }
}

pub(crate) fn animate(
    config: Res<LabConfig>,
    state: Res<LabState>,
    mut views: Query<(&LabView, &mut Transform), Without<MovingOccluder>>,
    mut occluders: Query<&mut Transform, With<MovingOccluder>>,
) {
    // Frame-indexed path gives every run identical views regardless of its speed.
    // The path restarts at sampling so warmup duration does not shift the experiment.
    let frame = state.frame.saturating_sub(config.warmup_frames);
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
