//! Runtime assembly of source-backed modular parts authored in Imported Forge.

#![allow(dead_code)] // Public request/component API is consumed incrementally by gameplay authoring flows.

use std::collections::{BTreeSet, HashMap, HashSet};

use bevy::asset::AssetId;
use bevy::gltf::{Gltf, GltfMaterial, GltfMesh, GltfNode};
use bevy::prelude::*;

use crate::components::character::{JointKind, JointMarker, SkeletonRig};
use crate::engine::rendering::PbrBundle;
use crate::engine_tools::character_records::{
    ImportedAnimationJoint, ImportedCharacterSpec, ImportedGameplayRole, ImportedMaterialEdit,
    ImportedPartSlot,
};
use crate::engine_tools::mesh_selection::selected_face_mesh;
use crate::engine_tools::mesh_uv::{apply_uv_edit, static_mesh_copy};

pub struct ImportedModularRuntimePlugin;

impl Plugin for ImportedModularRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                begin_imported_assembly,
                finish_imported_assembly,
                bind_imported_parts_to_rig,
            )
                .chain(),
        );
    }
}

#[derive(Component, Clone)]
pub struct ImportedCharacterAssemblyRequest {
    pub spec: ImportedCharacterSpec,
}

#[derive(Component, Clone)]
pub struct ImportedCharacterAssembly {
    pub spec: ImportedCharacterSpec,
    pub part_count: usize,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub enum ImportedAssemblyStatus {
    Loading,
    Ready { part_count: usize },
    Failed(String),
}

#[derive(Component)]
struct PendingImportedAssembly {
    sources: HashMap<String, Handle<Gltf>>,
}

#[derive(Component, Debug, Clone)]
pub struct ImportedRuntimePart {
    pub root: Entity,
    pub part_id: String,
    pub slot: ImportedPartSlot,
    pub joint: ImportedAnimationJoint,
    pub gameplay_role: ImportedGameplayRole,
    pub pivot: Vec3,
    rest_transform: Transform,
    bound: bool,
}

#[derive(Component, Debug, Clone)]
pub struct ImportedGameplayRegion {
    pub root: Entity,
    pub part_id: String,
    pub role: ImportedGameplayRole,
}

pub fn request_imported_character(
    commands: &mut Commands,
    root: Entity,
    spec: ImportedCharacterSpec,
) {
    commands.entity(root).insert((
        ImportedCharacterAssemblyRequest { spec },
        ImportedAssemblyStatus::Loading,
    ));
}

fn begin_imported_assembly(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    requests: Query<
        (Entity, &ImportedCharacterAssemblyRequest),
        Added<ImportedCharacterAssemblyRequest>,
    >,
    existing_parts: Query<(Entity, &ImportedRuntimePart)>,
) {
    for (root, request) in &requests {
        for (entity, part) in &existing_parts {
            if part.root == root {
                commands.entity(entity).try_despawn();
            }
        }
        if let Err(error) = request.spec.validate() {
            commands
                .entity(root)
                .insert(ImportedAssemblyStatus::Failed(error))
                .remove::<ImportedCharacterAssemblyRequest>();
            continue;
        }
        if request.spec.part_assignments.is_empty() {
            commands
                .entity(root)
                .insert(ImportedAssemblyStatus::Failed(
                    "Imported assembly has no modular part assignments".into(),
                ))
                .remove::<ImportedCharacterAssemblyRequest>();
            continue;
        }
        let sources = request
            .spec
            .part_assignments
            .iter()
            .map(|part| part.source_asset.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|path| {
                let handle = asset_server.load(path.clone());
                (path, handle)
            })
            .collect();
        commands
            .entity(root)
            .insert(PendingImportedAssembly { sources });
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_imported_assembly(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    gltfs: Option<Res<Assets<Gltf>>>,
    gltf_meshes: Option<Res<Assets<GltfMesh>>>,
    gltf_nodes: Option<Res<Assets<GltfNode>>>,
    gltf_materials: Option<Res<Assets<GltfMaterial>>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    pending: Query<(
        Entity,
        &ImportedCharacterAssemblyRequest,
        &PendingImportedAssembly,
        Option<&SkeletonRig>,
    )>,
    joints: Query<&JointMarker>,
) {
    let (Some(gltfs), Some(gltf_meshes), Some(gltf_nodes), Some(gltf_materials)) =
        (gltfs, gltf_meshes, gltf_nodes, gltf_materials)
    else {
        return;
    };
    for (root, request, pending, rig) in &pending {
        let mut loaded = HashMap::new();
        let mut waiting = false;
        for (path, handle) in &pending.sources {
            let Some(gltf) = gltfs.get(handle) else {
                waiting = true;
                break;
            };
            loaded.insert(path.as_str(), gltf);
        }
        if waiting {
            continue;
        }

        let mut prepared = Vec::with_capacity(request.spec.part_assignments.len());
        let mut failure = None;
        for assignment in &request.spec.part_assignments {
            let result = (|| {
                let gltf = loaded.get(assignment.source_asset.as_str())?;
                let gltf_mesh = gltf_meshes.get(gltf.meshes.get(assignment.mesh_index)?)?;
                let primitive = gltf_mesh.primitives.get(assignment.primitive_index)?;
                let source = meshes.get(&primitive.mesh)?;
                let uv_mesh = request
                    .spec
                    .uv_edits
                    .iter()
                    .find(|edit| {
                        edit.source_asset == assignment.source_asset
                            && edit.mesh_index == assignment.mesh_index
                            && edit.primitive_index == assignment.primitive_index
                    })
                    .and_then(|edit| apply_uv_edit(source, edit));
                let display_source;
                let source = if let Some(uv_mesh) = uv_mesh.as_ref() {
                    uv_mesh
                } else {
                    display_source = static_mesh_copy(source);
                    &display_source
                };
                let selected: BTreeSet<u32> = assignment.face_indices.iter().copied().collect();
                let mesh = selected_face_mesh(source, &selected)?;
                let mut material = standard_material_from_gltf(
                    primitive
                        .material
                        .as_ref()
                        .and_then(|handle| gltf_materials.get(handle)),
                );
                if let Some(edit) = request.spec.material_edits.iter().find(|edit| {
                    edit.source_asset == assignment.source_asset
                        && edit.mesh_index == assignment.mesh_index
                        && edit.primitive_index == assignment.primitive_index
                }) {
                    apply_material_edit(&mut material, edit, &asset_server);
                }
                let transform = mesh_instance_transform(gltf, &gltf_nodes, assignment.mesh_index)
                    .unwrap_or(Transform::IDENTITY);
                Some((assignment.clone(), mesh, material, transform))
            })();
            match result {
                Some(part) => prepared.push(part),
                None => {
                    failure = Some(format!(
                        "Could not resolve {} from {} mesh {} primitive {}",
                        assignment.part_id,
                        assignment.source_asset,
                        assignment.mesh_index,
                        assignment.primitive_index
                    ));
                    break;
                }
            }
        }
        if let Some(error) = failure {
            commands
                .entity(root)
                .insert(ImportedAssemblyStatus::Failed(error))
                .remove::<(ImportedCharacterAssemblyRequest, PendingImportedAssembly)>();
            continue;
        }

        let part_count = prepared.len();
        for (assignment, mesh, material, mut transform) in prepared {
            transform.translation *= Vec3::from_array(request.spec.root_scale);
            transform.scale *= Vec3::from_array(request.spec.root_scale);
            let rest_transform = transform;
            let mut runtime_part = ImportedRuntimePart {
                root,
                part_id: assignment.part_id.clone(),
                slot: assignment.slot,
                joint: assignment.joint,
                gameplay_role: assignment.gameplay_role,
                pivot: Vec3::from_array(assignment.pivot),
                rest_transform,
                bound: false,
            };
            let parent =
                bind_parent_and_transform(root, rig, &joints, assignment.joint, &mut transform);
            runtime_part.bound = parent != root;
            let entity = commands
                .spawn((
                    Name::new(format!("Imported part {}", assignment.part_id)),
                    runtime_part,
                    ImportedGameplayRegion {
                        root,
                        part_id: assignment.part_id,
                        role: assignment.gameplay_role,
                    },
                    PbrBundle {
                        mesh: Mesh3d(meshes.add(mesh)),
                        material: MeshMaterial3d(materials.add(material)),
                        transform,
                        ..default()
                    },
                ))
                .id();
            commands.entity(parent).add_child(entity);
        }
        commands
            .entity(root)
            .insert((
                ImportedCharacterAssembly {
                    spec: request.spec.clone(),
                    part_count,
                },
                ImportedAssemblyStatus::Ready { part_count },
            ))
            .remove::<(ImportedCharacterAssemblyRequest, PendingImportedAssembly)>();
    }
}

fn bind_imported_parts_to_rig(
    mut commands: Commands,
    rigs: Query<&SkeletonRig>,
    joints: Query<&JointMarker>,
    mut parts: Query<(Entity, &mut ImportedRuntimePart, &mut Transform)>,
) {
    for (entity, mut part, mut transform) in &mut parts {
        if part.bound {
            continue;
        }
        let Ok(rig) = rigs.get(part.root) else {
            continue;
        };
        *transform = part.rest_transform;
        let parent =
            bind_parent_and_transform(part.root, Some(rig), &joints, part.joint, &mut transform);
        if parent != part.root {
            part.bound = true;
            commands.entity(parent).add_child(entity);
        }
    }
}

fn bind_parent_and_transform(
    root: Entity,
    rig: Option<&SkeletonRig>,
    joints: &Query<&JointMarker>,
    imported_joint: ImportedAnimationJoint,
    transform: &mut Transform,
) -> Entity {
    let Some(joint_kind) = joint_kind(imported_joint) else {
        return root;
    };
    let Some(rig) = rig else {
        return root;
    };
    let Some(entity) = rig.joints.get(&joint_kind).copied() else {
        return root;
    };
    let Ok(marker) = joints.get(entity) else {
        return root;
    };
    if marker.root != root {
        return root;
    }
    transform.translation -= marker.rest_translation;
    entity
}

fn joint_kind(joint: ImportedAnimationJoint) -> Option<JointKind> {
    Some(match joint {
        ImportedAnimationJoint::Pelvis => JointKind::Pelvis,
        ImportedAnimationJoint::Spine => JointKind::Spine,
        ImportedAnimationJoint::Chest => JointKind::Chest,
        ImportedAnimationJoint::Neck => JointKind::Neck,
        ImportedAnimationJoint::Head => JointKind::Head,
        ImportedAnimationJoint::LeftShoulder => JointKind::LeftShoulder,
        ImportedAnimationJoint::LeftElbow => JointKind::LeftElbow,
        ImportedAnimationJoint::LeftWrist => JointKind::LeftWrist,
        ImportedAnimationJoint::RightShoulder => JointKind::RightShoulder,
        ImportedAnimationJoint::RightElbow => JointKind::RightElbow,
        ImportedAnimationJoint::RightWrist => JointKind::RightWrist,
        ImportedAnimationJoint::LeftHip => JointKind::LeftHip,
        ImportedAnimationJoint::LeftKnee => JointKind::LeftKnee,
        ImportedAnimationJoint::LeftAnkle => JointKind::LeftAnkle,
        ImportedAnimationJoint::RightHip => JointKind::RightHip,
        ImportedAnimationJoint::RightKnee => JointKind::RightKnee,
        ImportedAnimationJoint::RightAnkle => JointKind::RightAnkle,
        ImportedAnimationJoint::None => return None,
    })
}

fn collect_node_instances(
    handle: &Handle<GltfNode>,
    parent_transform: Transform,
    nodes: &Assets<GltfNode>,
    mesh_handles: &[Handle<GltfMesh>],
    visited: &mut HashSet<AssetId<GltfNode>>,
    instances: &mut Vec<(usize, Transform)>,
) -> Option<()> {
    if !visited.insert(handle.id()) {
        return Some(());
    }
    let node = nodes.get(handle)?;
    let transform = parent_transform.mul_transform(node.transform);
    if let Some(mesh) = node.mesh.as_ref() {
        if let Some(index) = mesh_handles.iter().position(|candidate| candidate == mesh) {
            instances.push((index, transform));
        }
    }
    for child in &node.children {
        collect_node_instances(child, transform, nodes, mesh_handles, visited, instances)?;
    }
    Some(())
}

fn mesh_instance_transform(
    gltf: &Gltf,
    nodes: &Assets<GltfNode>,
    mesh_index: usize,
) -> Option<Transform> {
    if gltf.nodes.is_empty() {
        return Some(Transform::IDENTITY);
    }
    let mut child_ids = HashSet::new();
    for handle in &gltf.nodes {
        child_ids.extend(nodes.get(handle)?.children.iter().map(Handle::id));
    }
    let mut instances = Vec::new();
    let mut visited = HashSet::new();
    for root in gltf
        .nodes
        .iter()
        .filter(|handle| !child_ids.contains(&handle.id()))
    {
        collect_node_instances(
            root,
            Transform::IDENTITY,
            nodes,
            &gltf.meshes,
            &mut visited,
            &mut instances,
        )?;
    }
    instances
        .into_iter()
        .find(|(index, _)| *index == mesh_index)
        .map(|(_, transform)| transform)
        .or(Some(Transform::IDENTITY))
}

fn standard_material_from_gltf(source: Option<&GltfMaterial>) -> StandardMaterial {
    let Some(source) = source else {
        return StandardMaterial::default();
    };
    StandardMaterial {
        base_color: source.base_color,
        base_color_channel: source.base_color_channel.clone(),
        base_color_texture: source.base_color_texture.clone(),
        emissive: source.emissive,
        emissive_channel: source.emissive_channel.clone(),
        emissive_texture: source.emissive_texture.clone(),
        perceptual_roughness: source.perceptual_roughness,
        metallic: source.metallic,
        metallic_roughness_channel: source.metallic_roughness_channel.clone(),
        metallic_roughness_texture: source.metallic_roughness_texture.clone(),
        reflectance: source.reflectance,
        specular_tint: source.specular_tint,
        normal_map_channel: source.normal_map_channel.clone(),
        normal_map_texture: source.normal_map_texture.clone(),
        double_sided: source.double_sided,
        cull_mode: source.cull_mode,
        alpha_mode: source.alpha_mode,
        ..default()
    }
}

fn apply_material_edit(
    material: &mut StandardMaterial,
    edit: &ImportedMaterialEdit,
    asset_server: &AssetServer,
) {
    material.base_color = Color::srgba(
        edit.base_color[0],
        edit.base_color[1],
        edit.base_color[2],
        edit.base_color[3],
    );
    material.metallic = edit.metallic;
    material.perceptual_roughness = edit.perceptual_roughness;
    material.emissive = LinearRgba::rgb(
        edit.emissive[0] * edit.emissive_strength,
        edit.emissive[1] * edit.emissive_strength,
        edit.emissive[2] * edit.emissive_strength,
    );
    assign_texture(
        &mut material.base_color_texture,
        &edit.base_color_texture,
        edit.cleared_texture_channels[0],
        asset_server,
    );
    assign_texture(
        &mut material.normal_map_texture,
        &edit.normal_texture,
        edit.cleared_texture_channels[1],
        asset_server,
    );
    assign_texture(
        &mut material.metallic_roughness_texture,
        &edit.metallic_roughness_texture,
        edit.cleared_texture_channels[2],
        asset_server,
    );
    assign_texture(
        &mut material.emissive_texture,
        &edit.emissive_texture,
        edit.cleared_texture_channels[3],
        asset_server,
    );
}

fn assign_texture(
    target: &mut Option<Handle<Image>>,
    path: &Option<String>,
    cleared: bool,
    asset_server: &AssetServer,
) {
    if cleared {
        *target = None;
    } else if let Some(path) = path {
        *target = Some(asset_server.load(path.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_imported_animation_joint_maps_to_the_runtime_rig() {
        let joints = [
            ImportedAnimationJoint::Pelvis,
            ImportedAnimationJoint::Spine,
            ImportedAnimationJoint::Chest,
            ImportedAnimationJoint::Neck,
            ImportedAnimationJoint::Head,
            ImportedAnimationJoint::LeftShoulder,
            ImportedAnimationJoint::LeftElbow,
            ImportedAnimationJoint::LeftWrist,
            ImportedAnimationJoint::RightShoulder,
            ImportedAnimationJoint::RightElbow,
            ImportedAnimationJoint::RightWrist,
            ImportedAnimationJoint::LeftHip,
            ImportedAnimationJoint::LeftKnee,
            ImportedAnimationJoint::LeftAnkle,
            ImportedAnimationJoint::RightHip,
            ImportedAnimationJoint::RightKnee,
            ImportedAnimationJoint::RightAnkle,
        ];
        assert!(joints.into_iter().all(|joint| joint_kind(joint).is_some()));
        assert_eq!(joint_kind(ImportedAnimationJoint::None), None);
    }
}
