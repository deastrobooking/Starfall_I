//! Hand-rolled glTF 2.0 binary (`.glb`) writer for studio characters.
//!
//! We own every vertex the generator produces, so export needs no external
//! crate: a GLB is a 12-byte header + a JSON chunk + a binary chunk. Each
//! studio part becomes one node+mesh+material; part transforms are stored as
//! node matrices relative to the preview root. Rigid parts only — skinning is
//! an EC4 decision.
//!
//! The output targets broad compatibility: 16-byte-aligned-free tightly packed
//! `f32`/`u32` buffer views, `POSITION` min/max (required by the spec), and
//! `pbrMetallicRoughness` base-color materials. Files open in macOS Quick
//! Look, Blender, and Bevy's own glTF loader.

use serde_json::json;

/// One rigid part of an exported character.
pub struct GlbPart {
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    /// Column-major node matrix relative to the character root.
    pub matrix: [f32; 16],
}

const GLB_MAGIC: u32 = 0x4654_6C67; // "glTF"
const CHUNK_JSON: u32 = 0x4E4F_534A; // "JSON"
const CHUNK_BIN: u32 = 0x004E_4942; // "BIN\0"

/// Build a complete `.glb` byte stream from parts. Returns `None` for empty
/// input (nothing to export is a caller-visible condition, not a panic).
pub fn build_glb(parts: &[GlbPart]) -> Option<Vec<u8>> {
    if parts.is_empty() || parts.iter().all(|p| p.positions.is_empty()) {
        return None;
    }

    let mut bin: Vec<u8> = Vec::new();
    let mut buffer_views = Vec::new();
    let mut accessors = Vec::new();
    let mut meshes = Vec::new();
    let mut materials = Vec::new();
    let mut nodes = Vec::new();

    for part in parts {
        if part.positions.is_empty() || part.indices.is_empty() {
            continue;
        }
        // ── POSITION ──
        let (min, max) = position_bounds(&part.positions);
        let pos_accessor = push_accessor(
            &mut bin,
            &mut buffer_views,
            &mut accessors,
            bytemuck_f32(&part.positions.iter().flatten().copied().collect::<Vec<_>>()),
            "VEC3",
            5126, // FLOAT
            part.positions.len(),
            Some((min, max)),
        );
        // ── NORMAL (optional) ──
        let normal_accessor = (!part.normals.is_empty()
            && part.normals.len() == part.positions.len())
        .then(|| {
            push_accessor(
                &mut bin,
                &mut buffer_views,
                &mut accessors,
                bytemuck_f32(&part.normals.iter().flatten().copied().collect::<Vec<_>>()),
                "VEC3",
                5126,
                part.normals.len(),
                None,
            )
        });
        // ── TEXCOORD_0 (optional) ──
        let uv_accessor = (!part.uvs.is_empty() && part.uvs.len() == part.positions.len())
            .then(|| {
                push_accessor(
                    &mut bin,
                    &mut buffer_views,
                    &mut accessors,
                    bytemuck_f32(&part.uvs.iter().flatten().copied().collect::<Vec<_>>()),
                    "VEC2",
                    5126,
                    part.uvs.len(),
                    None,
                )
            });
        // ── indices ──
        let index_bytes: Vec<u8> = part
            .indices
            .iter()
            .flat_map(|i| i.to_le_bytes())
            .collect();
        let index_accessor = push_accessor(
            &mut bin,
            &mut buffer_views,
            &mut accessors,
            index_bytes,
            "SCALAR",
            5125, // UNSIGNED_INT
            part.indices.len(),
            None,
        );

        let material_index = materials.len();
        materials.push(json!({
            "name": format!("{}_mat", part.name),
            "pbrMetallicRoughness": {
                "baseColorFactor": part.base_color,
                "metallicFactor": part.metallic,
                "roughnessFactor": part.roughness,
            },
            "doubleSided": false,
        }));

        let mut attributes = serde_json::Map::new();
        attributes.insert("POSITION".into(), json!(pos_accessor));
        if let Some(n) = normal_accessor {
            attributes.insert("NORMAL".into(), json!(n));
        }
        if let Some(uv) = uv_accessor {
            attributes.insert("TEXCOORD_0".into(), json!(uv));
        }
        let mesh_index = meshes.len();
        meshes.push(json!({
            "name": part.name,
            "primitives": [{
                "attributes": attributes,
                "indices": index_accessor,
                "material": material_index,
                "mode": 4, // TRIANGLES
            }],
        }));
        nodes.push(json!({
            "name": part.name,
            "mesh": mesh_index,
            "matrix": part.matrix,
        }));
    }

    if nodes.is_empty() {
        return None;
    }

    // Root node parents every part so DCCs import one object.
    let child_indices: Vec<usize> = (0..nodes.len()).collect();
    let root_index = nodes.len();
    nodes.push(json!({ "name": "starfall_character", "children": child_indices }));

    let gltf = json!({
        "asset": { "version": "2.0", "generator": "Starfall I Character Studio" },
        "buffers": [{ "byteLength": bin.len() }],
        "bufferViews": buffer_views,
        "accessors": accessors,
        "materials": materials,
        "meshes": meshes,
        "nodes": nodes,
        "scenes": [{ "nodes": [root_index] }],
        "scene": 0,
    });

    // ── Assemble the container ──
    let mut json_bytes = serde_json::to_vec(&gltf).ok()?;
    while !json_bytes.len().is_multiple_of(4) {
        json_bytes.push(b' ');
    }
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let total = 12 + 8 + json_bytes.len() + 8 + bin.len();

    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&GLB_MAGIC.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&CHUNK_JSON.to_le_bytes());
    out.extend_from_slice(&json_bytes);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(&CHUNK_BIN.to_le_bytes());
    out.extend_from_slice(&bin);
    Some(out)
}

fn bytemuck_f32(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn position_bounds(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for p in positions {
        for axis in 0..3 {
            min[axis] = min[axis].min(p[axis]);
            max[axis] = max[axis].max(p[axis]);
        }
    }
    (min, max)
}

/// Append a data block to the binary buffer and register its view + accessor.
/// Returns the accessor index.
#[allow(clippy::too_many_arguments)]
fn push_accessor(
    bin: &mut Vec<u8>,
    buffer_views: &mut Vec<serde_json::Value>,
    accessors: &mut Vec<serde_json::Value>,
    bytes: Vec<u8>,
    accessor_type: &str,
    component_type: u32,
    count: usize,
    bounds: Option<([f32; 3], [f32; 3])>,
) -> usize {
    // 4-byte alignment for every view (f32/u32 blocks keep it naturally, but
    // be defensive).
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let view_index = buffer_views.len();
    buffer_views.push(json!({
        "buffer": 0,
        "byteOffset": bin.len(),
        "byteLength": bytes.len(),
    }));
    bin.extend_from_slice(&bytes);

    let accessor_index = accessors.len();
    let mut accessor = json!({
        "bufferView": view_index,
        "componentType": component_type,
        "count": count,
        "type": accessor_type,
    });
    if let Some((min, max)) = bounds {
        accessor["min"] = json!(min);
        accessor["max"] = json!(max);
    }
    accessors.push(accessor);
    accessor_index
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle_part() -> GlbPart {
        GlbPart {
            name: "tri".into(),
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            indices: vec![0, 1, 2],
            base_color: [0.8, 0.2, 0.3, 1.0],
            metallic: 0.1,
            roughness: 0.9,
            matrix: bevy::math::Mat4::IDENTITY.to_cols_array(),
        }
    }

    #[test]
    fn glb_container_is_well_formed() {
        let bytes = build_glb(&[triangle_part()]).expect("export");
        assert_eq!(&bytes[0..4], b"glTF");
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 2);
        let total = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        assert_eq!(total, bytes.len());
        let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        assert_eq!(&bytes[16..20], b"JSON");
        assert_eq!(json_len % 4, 0);
        // BIN chunk header sits right after the JSON chunk.
        let bin_head = 20 + json_len;
        let bin_len =
            u32::from_le_bytes(bytes[bin_head..bin_head + 4].try_into().unwrap()) as usize;
        assert_eq!(&bytes[bin_head + 4..bin_head + 7], b"BIN");
        assert_eq!(bin_head + 8 + bin_len, bytes.len());
    }

    #[test]
    fn gltf_json_round_trips_counts_and_material() {
        let part = triangle_part();
        let bytes = build_glb(&[part]).expect("export");
        let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let gltf: serde_json::Value = serde_json::from_slice(&bytes[20..20 + json_len]).unwrap();

        assert_eq!(gltf["asset"]["version"], "2.0");
        assert_eq!(gltf["meshes"].as_array().unwrap().len(), 1);
        // Root node + 1 part node.
        assert_eq!(gltf["nodes"].as_array().unwrap().len(), 2);
        let pos_accessor = &gltf["accessors"][0];
        assert_eq!(pos_accessor["count"], 3);
        assert_eq!(pos_accessor["type"], "VEC3");
        assert_eq!(pos_accessor["min"][0], 0.0);
        assert_eq!(pos_accessor["max"][1], 1.0);
        let color = gltf["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"]
            .as_array()
            .unwrap();
        assert!((color[0].as_f64().unwrap() - 0.8).abs() < 1e-6);
        // Buffer byteLength must match the BIN chunk payload.
        let bin_head = 20 + json_len;
        let bin_len =
            u32::from_le_bytes(bytes[bin_head..bin_head + 4].try_into().unwrap()) as u64;
        assert_eq!(gltf["buffers"][0]["byteLength"].as_u64().unwrap(), bin_len);
    }

    #[test]
    fn empty_export_is_none_not_panic() {
        assert!(build_glb(&[]).is_none());
    }
}
