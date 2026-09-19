//! glTF 2.0 export for tree meshes.

use crate::{LodMeshSet, Mesh, mesh::MaterialType, mesh::Submesh, textures::TextureSet};
use glam::{Vec3, Vec4};
use std::io::Write;
use std::path::Path;

/// Export configuration
#[derive(Debug, Clone)]
pub struct ExportConfig {
    /// Export format
    pub format: ExportFormat,
    /// Enable Draco compression (requires separate processing)
    pub draco: bool,
    /// Material maps to embed into the export. `Some` emits real textured
    /// materials (bark albedo+normal, leaf card, baked impostor atlases);
    /// `None` exports plain colored materials. Build with
    /// `TextureSet::generate` or `TextureSet::resolve` for file slots.
    pub textures: Option<TextureSet>,
    /// Add pivot_painter flag in extras
    pub pivot_painter_extras: bool,
    /// Optional asset metadata for node naming and glTF extras.
    pub metadata: Option<ExportMetadata>,
}

/// Optional metadata to carry through glTF export.
#[derive(Debug, Clone, Default)]
pub struct ExportMetadata {
    /// Species common name.
    pub species_name: String,
    /// Latin/scientific name.
    pub scientific_name: String,
    /// Generation seed.
    pub seed: Option<u64>,
    /// LOD screen-height thresholds, ordered by LOD index.
    pub lod_screen_heights: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportFormat {
    /// Binary glTF
    #[default]
    Glb,
    /// JSON + separate binary
    GlTf,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            format: ExportFormat::Glb,
            draco: false,
            textures: None,
            pivot_painter_extras: true,
            metadata: None,
        }
    }
}

/// Export error types
#[derive(Debug)]
pub enum ExportError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Texture(crate::textures::TextureError),
    NoMeshes,
}

impl From<std::io::Error> for ExportError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for ExportError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl From<crate::textures::TextureError> for ExportError {
    fn from(e: crate::textures::TextureError) -> Self {
        Self::Texture(e)
    }
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
            Self::Texture(e) => write!(f, "texture error: {e}"),
            Self::NoMeshes => write!(f, "No meshes to export"),
        }
    }
}

impl std::error::Error for ExportError {}

/// Export a single mesh to glTF
pub fn export_mesh(mesh: &Mesh, path: &Path, config: &ExportConfig) -> Result<(), ExportError> {
    if mesh.vertices.is_empty() {
        return Err(ExportError::NoMeshes);
    }

    let gltf_data = build_gltf_single(mesh, config)?;
    write_gltf(path, &gltf_data, config.format)?;
    Ok(())
}

/// Export LOD mesh set to glTF (multiple meshes in one file)
pub fn export_lod_meshes(
    lods: &LodMeshSet,
    path: &Path,
    config: &ExportConfig,
) -> Result<(), ExportError> {
    if lods.meshes.is_empty() {
        return Err(ExportError::NoMeshes);
    }

    let gltf_data = build_gltf_lods(lods, config)?;
    write_gltf(path, &gltf_data, config.format)?;
    Ok(())
}

/// Export LOD mesh set to GLB bytes (for WASM/in-memory use)
pub fn export_lod_meshes_to_bytes(
    lods: &LodMeshSet,
    config: &ExportConfig,
) -> Result<Vec<u8>, ExportError> {
    if lods.meshes.is_empty() {
        return Err(ExportError::NoMeshes);
    }

    let gltf_data = build_gltf_lods(lods, config)?;
    build_glb_bytes(&gltf_data)
}

/// Export LOD mesh set as separate `.gltf` JSON + `.bin` parts (in-memory)
///
/// Returns `(gltf_json, bin)` bytes. `bin_name` is recorded as the buffer URI
/// inside the glTF document, matching what `write_gltf_separate` produces on disk.
pub fn export_lod_meshes_to_parts(
    lods: &LodMeshSet,
    bin_name: &str,
    config: &ExportConfig,
) -> Result<(Vec<u8>, Vec<u8>), ExportError> {
    if lods.meshes.is_empty() {
        return Err(ExportError::NoMeshes);
    }

    let gltf_data = build_gltf_lods(lods, config)?;
    let mut json = gltf_data.json;
    if let Some(buffers) = json.get_mut("buffers").and_then(|b| b.as_array_mut())
        && let Some(buffer) = buffers.first_mut()
    {
        buffer["uri"] = serde_json::Value::String(bin_name.to_string());
    }
    let json_bytes = serde_json::to_vec_pretty(&json)?;
    Ok((json_bytes, gltf_data.binary))
}

/// Build GLB file format in memory
fn build_glb_bytes(data: &GltfData) -> Result<Vec<u8>, ExportError> {
    let json_bytes = serde_json::to_vec(&data.json)?;

    // Pad JSON to 4-byte alignment
    let json_padding = (4 - (json_bytes.len() % 4)) % 4;
    let json_chunk_length = json_bytes.len() + json_padding;

    // Pad binary to 4-byte alignment
    let bin_padding = (4 - (data.binary.len() % 4)) % 4;
    let bin_chunk_length = data.binary.len() + bin_padding;

    // Calculate total file size
    let total_length = 12  // GLB header
        + 8 + json_chunk_length  // JSON chunk header + data
        + 8 + bin_chunk_length; // BIN chunk header + data

    let mut buffer = Vec::with_capacity(total_length);

    // GLB header
    buffer.extend_from_slice(b"glTF"); // magic
    buffer.extend_from_slice(&2u32.to_le_bytes()); // version
    buffer.extend_from_slice(&(total_length as u32).to_le_bytes()); // length

    // JSON chunk
    buffer.extend_from_slice(&(json_chunk_length as u32).to_le_bytes()); // chunk length
    buffer.extend_from_slice(&0x4E4F534Au32.to_le_bytes()); // chunk type "JSON"
    buffer.extend_from_slice(&json_bytes);
    buffer.extend_from_slice(&vec![0x20u8; json_padding]); // padding with spaces

    // BIN chunk
    buffer.extend_from_slice(&(bin_chunk_length as u32).to_le_bytes()); // chunk length
    buffer.extend_from_slice(&0x004E4942u32.to_le_bytes()); // chunk type "BIN\0"
    buffer.extend_from_slice(&data.binary);
    buffer.extend_from_slice(&vec![0u8; bin_padding]); // padding with zeros

    Ok(buffer)
}

/// glTF data container (simplified representation)
struct GltfData {
    json: serde_json::Value,
    binary: Vec<u8>,
}

fn build_gltf_single(mesh: &Mesh, config: &ExportConfig) -> Result<GltfData, ExportError> {
    let mut binary = Vec::new();

    // Build vertex buffer
    let positions_offset = binary.len();
    let positions = build_positions_buffer(mesh);
    binary.extend_from_slice(&positions);

    let normals_offset = binary.len();
    let normals = build_normals_buffer(mesh);
    binary.extend_from_slice(&normals);

    let tangents_offset = binary.len();
    let tangents = build_tangents_buffer(mesh);
    binary.extend_from_slice(&tangents);

    let texcoord0_offset = binary.len();
    let texcoord0 = build_texcoord0_buffer(mesh);
    binary.extend_from_slice(&texcoord0);

    let texcoord1_offset = binary.len();
    let texcoord1 = build_texcoord1_buffer(mesh);
    binary.extend_from_slice(&texcoord1);

    let color0_offset = binary.len();
    let color0 = build_color0_buffer(mesh);
    binary.extend_from_slice(&color0);

    let indices_offset = binary.len();
    let indices = build_indices_buffer(mesh);
    binary.extend_from_slice(&indices);

    // Compute bounds
    let (min_pos, max_pos) = compute_bounds(mesh);

    // Embed material maps (bark/leaf; a bare Mesh carries no impostor atlas).
    let mut images = Vec::new();
    append_material_images(&mut binary, &mut images, config)?;

    // Build glTF JSON
    let json = build_gltf_json(
        &[MeshBufferInfo {
            name: lod_node_name(config, 0),
            vertex_count: mesh.vertices.len(),
            index_count: mesh.indices.len(),
            positions_offset,
            normals_offset,
            tangents_offset,
            texcoord0_offset,
            texcoord1_offset,
            color0_offset,
            indices_offset,
            min_pos,
            max_pos,
            submeshes: mesh.submeshes.clone(),
            impostor_image: None,
        }],
        &images,
        binary.len(),
        config,
    )?;

    Ok(GltfData { json, binary })
}

fn build_gltf_lods(lods: &LodMeshSet, config: &ExportConfig) -> Result<GltfData, ExportError> {
    let mut binary = Vec::new();
    let mut mesh_infos = Vec::new();

    for lod in &lods.meshes {
        let mesh = &lod.mesh;
        if mesh.vertices.is_empty() {
            continue;
        }

        let positions_offset = binary.len();
        binary.extend_from_slice(&build_positions_buffer(mesh));

        let normals_offset = binary.len();
        binary.extend_from_slice(&build_normals_buffer(mesh));

        let tangents_offset = binary.len();
        binary.extend_from_slice(&build_tangents_buffer(mesh));

        let texcoord0_offset = binary.len();
        binary.extend_from_slice(&build_texcoord0_buffer(mesh));

        let texcoord1_offset = binary.len();
        binary.extend_from_slice(&build_texcoord1_buffer(mesh));

        let color0_offset = binary.len();
        binary.extend_from_slice(&build_color0_buffer(mesh));

        let indices_offset = binary.len();
        binary.extend_from_slice(&build_indices_buffer(mesh));

        let (min_pos, max_pos) = compute_bounds(mesh);

        mesh_infos.push(MeshBufferInfo {
            name: lod_node_name(config, lod.index),
            vertex_count: mesh.vertices.len(),
            index_count: mesh.indices.len(),
            positions_offset,
            normals_offset,
            tangents_offset,
            texcoord0_offset,
            texcoord1_offset,
            color0_offset,
            indices_offset,
            min_pos,
            max_pos,
            submeshes: mesh.submeshes.clone(),
            // Filled below once the atlas PNG is appended.
            impostor_image: None,
        });
    }

    // Image payloads live in the same buffer, after the vertex data.
    let mut images = Vec::new();
    append_material_images(&mut binary, &mut images, config)?;

    // Impostor atlases are material content, not optional decoration — a LOD
    // carrying one always embeds it so the crossed quads render correctly.
    for (i, lod) in lods
        .meshes
        .iter()
        .filter(|l| !l.mesh.vertices.is_empty())
        .enumerate()
    {
        if let Some(atlas) = &lod.impostor_atlas {
            let png = atlas.to_png()?;
            let image_index = images.len();
            images.push(ImageInfo {
                name: format!("impostor_lod{}", lod.index),
                offset: binary.len(),
                len: png.len(),
                // Clamp-to-edge so the two atlas halves never bleed.
                sampler: SamplerKind::Clamp,
            });
            binary.extend_from_slice(&png);
            pad_binary(&mut binary);
            if let Some(info) = mesh_infos.get_mut(i) {
                info.impostor_image = Some(image_index);
            }
        }
    }

    let json = build_gltf_json(&mesh_infos, &images, binary.len(), config)?;
    Ok(GltfData { json, binary })
}

/// Texture addressing mode for an embedded image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SamplerKind {
    /// Wrap both axes — tiling bark/leaf maps.
    Repeat,
    /// Clamp — atlas halves must not bleed into each other.
    Clamp,
}

/// An embedded image payload inside the binary buffer.
struct ImageInfo {
    name: String,
    offset: usize,
    len: usize,
    sampler: SamplerKind,
}

/// Pad the binary buffer to a 4-byte boundary.
fn pad_binary(binary: &mut Vec<u8>) {
    binary.resize((binary.len() + 3) & !3, 0);
}

/// Append bark/leaf material maps as PNG payloads; returns the image list.
fn append_material_images(
    binary: &mut Vec<u8>,
    images: &mut Vec<ImageInfo>,
    config: &ExportConfig,
) -> Result<(), ExportError> {
    let Some(textures) = &config.textures else {
        return Ok(());
    };
    for (name, tex) in [
        ("bark_albedo", &textures.bark_albedo),
        ("bark_normal", &textures.bark_normal),
        ("leaf_card", &textures.leaf_card),
    ] {
        let png = tex.to_png()?;
        images.push(ImageInfo {
            name: name.to_string(),
            offset: binary.len(),
            len: png.len(),
            sampler: SamplerKind::Repeat,
        });
        binary.extend_from_slice(&png);
        pad_binary(binary);
    }
    Ok(())
}

struct MeshBufferInfo {
    name: String,
    vertex_count: usize,
    index_count: usize,
    positions_offset: usize,
    normals_offset: usize,
    tangents_offset: usize,
    texcoord0_offset: usize,
    texcoord1_offset: usize,
    color0_offset: usize,
    indices_offset: usize,
    min_pos: [f32; 3],
    max_pos: [f32; 3],
    submeshes: Vec<Submesh>,
    /// Image index of this LOD's baked impostor atlas, when embedded.
    impostor_image: Option<usize>,
}

fn build_positions_buffer(mesh: &Mesh) -> Vec<u8> {
    let mut data = Vec::with_capacity(mesh.vertices.len() * 12);
    for v in &mesh.vertices {
        data.extend_from_slice(&v.position.x.to_le_bytes());
        data.extend_from_slice(&v.position.y.to_le_bytes());
        data.extend_from_slice(&v.position.z.to_le_bytes());
    }
    data
}

fn build_normals_buffer(mesh: &Mesh) -> Vec<u8> {
    let mut data = Vec::with_capacity(mesh.vertices.len() * 12);
    for v in &mesh.vertices {
        data.extend_from_slice(&v.normal.x.to_le_bytes());
        data.extend_from_slice(&v.normal.y.to_le_bytes());
        data.extend_from_slice(&v.normal.z.to_le_bytes());
    }
    data
}

fn build_tangents_buffer(mesh: &Mesh) -> Vec<u8> {
    let tangents = compute_vertex_tangents(mesh);
    let mut data = Vec::with_capacity(tangents.len() * 16);
    for tangent in tangents {
        data.extend_from_slice(&tangent.x.to_le_bytes());
        data.extend_from_slice(&tangent.y.to_le_bytes());
        data.extend_from_slice(&tangent.z.to_le_bytes());
        data.extend_from_slice(&tangent.w.to_le_bytes());
    }
    data
}

fn compute_vertex_tangents(mesh: &Mesh) -> Vec<Vec4> {
    let mut tan1 = vec![Vec3::ZERO; mesh.vertices.len()];
    let mut tan2 = vec![Vec3::ZERO; mesh.vertices.len()];

    for triangle in mesh.indices.chunks(3) {
        if triangle.len() != 3 {
            continue;
        }

        let i0 = triangle[0] as usize;
        let i1 = triangle[1] as usize;
        let i2 = triangle[2] as usize;
        if i0 >= mesh.vertices.len() || i1 >= mesh.vertices.len() || i2 >= mesh.vertices.len() {
            continue;
        }

        let v0 = &mesh.vertices[i0];
        let v1 = &mesh.vertices[i1];
        let v2 = &mesh.vertices[i2];

        let edge1 = v1.position - v0.position;
        let edge2 = v2.position - v0.position;
        let uv1 = v1.uv - v0.uv;
        let uv2 = v2.uv - v0.uv;
        let determinant = uv1.x * uv2.y - uv2.x * uv1.y;
        if determinant.abs() <= 1.0e-8 {
            continue;
        }

        let inv_det = 1.0 / determinant;
        let tangent = (edge1 * uv2.y - edge2 * uv1.y) * inv_det;
        let bitangent = (edge2 * uv1.x - edge1 * uv2.x) * inv_det;

        tan1[i0] += tangent;
        tan1[i1] += tangent;
        tan1[i2] += tangent;
        tan2[i0] += bitangent;
        tan2[i1] += bitangent;
        tan2[i2] += bitangent;
    }

    mesh.vertices
        .iter()
        .enumerate()
        .map(|(index, vertex)| {
            let normal = normalized_or_fallback(vertex.normal, Vec3::Y);
            let tangent3 = orthonormal_tangent(normal, tan1[index]);
            let handedness = if normal.cross(tangent3).dot(tan2[index]) < 0.0 {
                -1.0
            } else {
                1.0
            };
            Vec4::new(tangent3.x, tangent3.y, tangent3.z, handedness)
        })
        .collect()
}

fn orthonormal_tangent(normal: Vec3, tangent: Vec3) -> Vec3 {
    let projected = tangent - normal * normal.dot(tangent);
    if projected.length_squared() > 1.0e-8 {
        projected.normalize()
    } else {
        fallback_tangent(normal)
    }
}

fn fallback_tangent(normal: Vec3) -> Vec3 {
    let helper = if normal.y.abs() < 0.9 {
        Vec3::Y
    } else {
        Vec3::X
    };
    normal.cross(helper).normalize_or_zero()
}

fn normalized_or_fallback(value: Vec3, fallback: Vec3) -> Vec3 {
    if value.length_squared() > 1.0e-8 {
        value.normalize()
    } else {
        fallback
    }
}

fn build_texcoord0_buffer(mesh: &Mesh) -> Vec<u8> {
    let mut data = Vec::with_capacity(mesh.vertices.len() * 8);
    for v in &mesh.vertices {
        data.extend_from_slice(&v.uv.x.to_le_bytes());
        data.extend_from_slice(&v.uv.y.to_le_bytes());
    }
    data
}

fn build_texcoord1_buffer(mesh: &Mesh) -> Vec<u8> {
    let mut data = Vec::with_capacity(mesh.vertices.len() * 8);
    for v in &mesh.vertices {
        data.extend_from_slice(&v.uv2.x.to_le_bytes());
        data.extend_from_slice(&v.uv2.y.to_le_bytes());
    }
    data
}

fn build_color0_buffer(mesh: &Mesh) -> Vec<u8> {
    let mut data = Vec::with_capacity(mesh.vertices.len() * 16);
    for v in &mesh.vertices {
        data.extend_from_slice(&v.color.x.to_le_bytes());
        data.extend_from_slice(&v.color.y.to_le_bytes());
        data.extend_from_slice(&v.color.z.to_le_bytes());
        data.extend_from_slice(&v.color.w.to_le_bytes());
    }
    data
}

fn build_indices_buffer(mesh: &Mesh) -> Vec<u8> {
    let mut data = Vec::with_capacity(mesh.indices.len() * 4);
    for &idx in &mesh.indices {
        data.extend_from_slice(&idx.to_le_bytes());
    }
    data
}

fn compute_bounds(mesh: &Mesh) -> ([f32; 3], [f32; 3]) {
    if mesh.vertices.is_empty() {
        return ([0.0; 3], [0.0; 3]);
    }

    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];

    for v in &mesh.vertices {
        min[0] = min[0].min(v.position.x);
        min[1] = min[1].min(v.position.y);
        min[2] = min[2].min(v.position.z);
        max[0] = max[0].max(v.position.x);
        max[1] = max[1].max(v.position.y);
        max[2] = max[2].max(v.position.z);
    }

    (min, max)
}

fn build_gltf_json(
    meshes: &[MeshBufferInfo],
    images: &[ImageInfo],
    buffer_size: usize,
    config: &ExportConfig,
) -> Result<serde_json::Value, serde_json::Error> {
    use serde_json::json;

    let mut accessors = Vec::new();
    let mut buffer_views = Vec::new();
    let mut gltf_meshes = Vec::new();

    let mut accessor_idx = 0;
    let mut buffer_view_idx = 0;

    // Material table: 0 = bark, 1 = leaves, then impostor materials — one
    // textured material per embedded atlas, or a single shared untextured
    // material for impostor submeshes whose LOD has no atlas.
    let textured = config.textures.is_some();
    let mut impostor_material: Vec<Option<usize>> = Vec::with_capacity(meshes.len());
    let mut next_material = 2;
    let mut shared_untextured: Option<usize> = None;
    for info in meshes {
        let has_impostor = info
            .submeshes
            .iter()
            .any(|s| s.material == MaterialType::Impostor);
        if !has_impostor {
            impostor_material.push(None);
            continue;
        }
        let index = if info.impostor_image.is_some() {
            let i = next_material;
            next_material += 1;
            i
        } else {
            *shared_untextured.get_or_insert_with(|| {
                let i = next_material;
                next_material += 1;
                i
            })
        };
        impostor_material.push(Some(index));
    }

    for (mesh_i, mesh_info) in meshes.iter().enumerate() {
        let vertex_count = mesh_info.vertex_count;
        let index_count = mesh_info.index_count;

        // Buffer views
        let pos_bv = buffer_view_idx;
        buffer_view_idx += 1;
        let norm_bv = buffer_view_idx;
        buffer_view_idx += 1;
        let tan_bv = buffer_view_idx;
        buffer_view_idx += 1;
        let tc0_bv = buffer_view_idx;
        buffer_view_idx += 1;
        let tc1_bv = buffer_view_idx;
        buffer_view_idx += 1;
        let col_bv = buffer_view_idx;
        buffer_view_idx += 1;
        let idx_bv = buffer_view_idx;
        buffer_view_idx += 1;

        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": mesh_info.positions_offset,
            "byteLength": vertex_count * 12,
            "target": 34962  // ARRAY_BUFFER
        }));
        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": mesh_info.normals_offset,
            "byteLength": vertex_count * 12,
            "target": 34962
        }));
        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": mesh_info.tangents_offset,
            "byteLength": vertex_count * 16,
            "target": 34962
        }));
        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": mesh_info.texcoord0_offset,
            "byteLength": vertex_count * 8,
            "target": 34962
        }));
        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": mesh_info.texcoord1_offset,
            "byteLength": vertex_count * 8,
            "target": 34962
        }));
        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": mesh_info.color0_offset,
            "byteLength": vertex_count * 16,
            "target": 34962
        }));
        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": mesh_info.indices_offset,
            "byteLength": mesh_info.index_count * 4,
            "target": 34963  // ELEMENT_ARRAY_BUFFER
        }));

        // Accessors
        let pos_acc = accessor_idx;
        accessor_idx += 1;
        let norm_acc = accessor_idx;
        accessor_idx += 1;
        let tan_acc = accessor_idx;
        accessor_idx += 1;
        let tc0_acc = accessor_idx;
        accessor_idx += 1;
        let tc1_acc = accessor_idx;
        accessor_idx += 1;
        let col_acc = accessor_idx;
        accessor_idx += 1;

        accessors.push(json!({
            "bufferView": pos_bv,
            "componentType": 5126,  // FLOAT
            "count": vertex_count,
            "type": "VEC3",
            "min": mesh_info.min_pos,
            "max": mesh_info.max_pos
        }));
        accessors.push(json!({
            "bufferView": norm_bv,
            "componentType": 5126,
            "count": vertex_count,
            "type": "VEC3"
        }));
        accessors.push(json!({
            "bufferView": tan_bv,
            "componentType": 5126,
            "count": vertex_count,
            "type": "VEC4"
        }));
        accessors.push(json!({
            "bufferView": tc0_bv,
            "componentType": 5126,
            "count": vertex_count,
            "type": "VEC2"
        }));
        accessors.push(json!({
            "bufferView": tc1_bv,
            "componentType": 5126,
            "count": vertex_count,
            "type": "VEC2"
        }));
        accessors.push(json!({
            "bufferView": col_bv,
            "componentType": 5126,
            "count": vertex_count,
            "type": "VEC4"
        }));
        let attributes = json!({
            "POSITION": pos_acc,
            "NORMAL": norm_acc,
            "TANGENT": tan_acc,
            "TEXCOORD_0": tc0_acc,
            "TEXCOORD_1": tc1_acc,
            "COLOR_0": col_acc
        });

        let mut push_index_accessor = |start: u32, count: u32| {
            let idx_acc = accessor_idx;
            accessor_idx += 1;
            accessors.push(json!({
                "bufferView": idx_bv,
                "byteOffset": (start as usize) * 4,
                "componentType": 5125,  // UNSIGNED_INT
                "count": count as usize,
                "type": "SCALAR"
            }));
            idx_acc
        };

        // Build primitives for submeshes, preserving material assignment.
        let primitives = if mesh_info.submeshes.is_empty() {
            let idx_acc = push_index_accessor(0, index_count as u32);
            vec![json!({
                "attributes": attributes.clone(),
                "indices": idx_acc,
                "mode": 4,  // TRIANGLES
                "material": 0
            })]
        } else {
            mesh_info
                .submeshes
                .iter()
                .filter(|submesh| submesh.index_count > 0)
                .map(|submesh| {
                    let idx_acc = push_index_accessor(submesh.index_start, submesh.index_count);
                    let material = match submesh.material {
                        MaterialType::Bark => 0,
                        MaterialType::Leaves => 1,
                        MaterialType::Impostor => impostor_material[mesh_i].unwrap_or(1),
                    };
                    json!({
                        "attributes": attributes.clone(),
                        "indices": idx_acc,
                        "mode": 4,
                        "material": material
                    })
                })
                .collect()
        };

        gltf_meshes.push(json!({
            "name": mesh_info.name,
            "primitives": primitives
        }));
    }

    // Image bufferViews come after all mesh views.
    for image in images {
        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": image.offset,
            "byteLength": image.len
        }));
    }
    let image_view_base = buffer_views.len() - images.len();

    // Build materials
    let mut materials = vec![
        if textured {
            json!({
                "name": "bark",
                "pbrMetallicRoughness": {
                    "baseColorTexture": { "index": 0 },
                    "metallicFactor": 0.0,
                    "roughnessFactor": 0.85
                },
                "normalTexture": { "index": 1 },
                "doubleSided": false
            })
        } else {
            json!({
                "name": "bark",
                "pbrMetallicRoughness": {
                    "baseColorFactor": [0.4, 0.3, 0.2, 1.0],
                    "metallicFactor": 0.0,
                    "roughnessFactor": 0.85
                },
                "doubleSided": false
            })
        },
        if textured {
            json!({
                "name": "leaves",
                "pbrMetallicRoughness": {
                    "baseColorTexture": { "index": 2 },
                    "metallicFactor": 0.0,
                    "roughnessFactor": 0.6
                },
                "doubleSided": true,
                "alphaMode": "MASK",
                "alphaCutoff": 0.5
            })
        } else {
            json!({
                "name": "leaves",
                "pbrMetallicRoughness": {
                    "baseColorFactor": [0.2, 0.5, 0.2, 1.0],
                    "metallicFactor": 0.0,
                    "roughnessFactor": 0.6
                },
                "doubleSided": true,
                "alphaMode": "MASK",
                "alphaCutoff": 0.5
            })
        },
    ];

    // Impostor materials fill their pre-assigned slots so shared untextured
    // materials never misalign the table.
    let mut impostor_slots: Vec<Option<serde_json::Value>> = vec![None; next_material - 2];
    for (mesh_i, info) in meshes.iter().enumerate() {
        let Some(index) = impostor_material[mesh_i] else {
            continue;
        };
        let slot = &mut impostor_slots[index - 2];
        if slot.is_some() {
            continue; // shared untextured material already emitted
        }
        *slot = Some(if let Some(image_idx) = info.impostor_image {
            json!({
                "name": format!("impostor_{}", info.name),
                "pbrMetallicRoughness": {
                    "baseColorTexture": { "index": image_idx },
                    "metallicFactor": 0.0,
                    "roughnessFactor": 0.8
                },
                "doubleSided": true,
                "alphaMode": "MASK",
                "alphaCutoff": 0.5
            })
        } else {
            json!({
                "name": "impostor",
                "pbrMetallicRoughness": {
                    "baseColorFactor": [0.3, 0.45, 0.2, 1.0],
                    "metallicFactor": 0.0,
                    "roughnessFactor": 0.8
                },
                "doubleSided": true
            })
        });
    }
    materials.extend(impostor_slots.into_iter().flatten());

    // Build nodes (one per mesh)
    let nodes: Vec<_> = (0..meshes.len())
        .map(|i| json!({ "mesh": i, "name": &meshes[i].name }))
        .collect();

    let scene = json!({
        "name": scene_name(config),
        "nodes": (0..meshes.len()).collect::<Vec<_>>()
    });

    let mut root = json!({
        "asset": {
            "generator": "midori",
            "version": "2.0"
        },
        "scene": 0,
        "scenes": [scene],
        "nodes": nodes,
        "meshes": gltf_meshes,
        "materials": materials,
        "accessors": accessors,
        "bufferViews": buffer_views,
        "buffers": [{
            "byteLength": buffer_size
        }]
    });

    if !images.is_empty() {
        // Samplers: 0 = repeat+linear (tiling maps), 1 = clamp (atlases).
        root["samplers"] = json!([
            {
                "magFilter": 9729,   // LINEAR
                "minFilter": 9987,   // LINEAR_MIPMAP_LINEAR
                "wrapS": 10497,      // REPEAT
                "wrapT": 10497
            },
            {
                "magFilter": 9729,
                "minFilter": 9987,
                "wrapS": 33071,      // CLAMP_TO_EDGE
                "wrapT": 33071
            }
        ]);
        root["images"] = images
            .iter()
            .enumerate()
            .map(|(i, image)| {
                json!({
                    "name": image.name,
                    "mimeType": "image/png",
                    "bufferView": image_view_base + i
                })
            })
            .collect();
        root["textures"] = images
            .iter()
            .enumerate()
            .map(|(i, image)| {
                json!({
                    "source": i,
                    "sampler": match image.sampler {
                        SamplerKind::Repeat => 0,
                        SamplerKind::Clamp => 1,
                    }
                })
            })
            .collect();
    }

    // Add pivot painter extras
    if config.pivot_painter_extras {
        root["extras"] = json!({
            "pivot_painter": true,
            "pivot_painter_version": "2.0"
        });
    }

    if let Some(metadata) = &config.metadata {
        if root.get("extras").is_none() {
            root["extras"] = json!({});
        }
        root["extras"]["midori"] = json!({
            "species_name": metadata.species_name,
            "scientific_name": metadata.scientific_name,
            "seed": metadata.seed,
            "lod_screen_heights": metadata.lod_screen_heights,
        });
    }

    Ok(root)
}

fn scene_name(config: &ExportConfig) -> String {
    config
        .metadata
        .as_ref()
        .and_then(|metadata| {
            if metadata.species_name.is_empty() {
                None
            } else {
                Some(metadata.species_name.clone())
            }
        })
        .unwrap_or_else(|| "Tree".to_string())
}

fn lod_node_name(config: &ExportConfig, lod_index: u32) -> String {
    let base = config
        .metadata
        .as_ref()
        .and_then(|metadata| {
            if metadata.species_name.is_empty() {
                None
            } else {
                Some(metadata.species_name.as_str())
            }
        })
        .unwrap_or("Tree");
    format!("{}_LOD{}", sanitize_name(base), lod_index)
}

fn sanitize_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "Tree".to_string()
    } else {
        trimmed.to_string()
    }
}

fn write_gltf(path: &Path, data: &GltfData, format: ExportFormat) -> Result<(), std::io::Error> {
    match format {
        ExportFormat::Glb => write_glb(path, data),
        ExportFormat::GlTf => write_gltf_separate(path, data),
    }
}

fn write_glb(path: &Path, data: &GltfData) -> Result<(), std::io::Error> {
    let json_bytes = serde_json::to_vec(&data.json)?;

    // Pad JSON to 4-byte alignment
    let json_padding = (4 - (json_bytes.len() % 4)) % 4;
    let json_chunk_length = json_bytes.len() + json_padding;

    // Pad binary to 4-byte alignment
    let bin_padding = (4 - (data.binary.len() % 4)) % 4;
    let bin_chunk_length = data.binary.len() + bin_padding;

    // Calculate total file size
    let total_length = 12  // GLB header
        + 8 + json_chunk_length  // JSON chunk header + data
        + 8 + bin_chunk_length; // BIN chunk header + data

    let mut file = std::fs::File::create(path)?;

    // GLB header
    file.write_all(b"glTF")?; // magic
    file.write_all(&2u32.to_le_bytes())?; // version
    file.write_all(&(total_length as u32).to_le_bytes())?; // length

    // JSON chunk
    file.write_all(&(json_chunk_length as u32).to_le_bytes())?; // chunk length
    file.write_all(&0x4E4F534Au32.to_le_bytes())?; // chunk type "JSON"
    file.write_all(&json_bytes)?;
    file.write_all(&vec![0x20u8; json_padding])?; // padding with spaces

    // BIN chunk
    file.write_all(&(bin_chunk_length as u32).to_le_bytes())?; // chunk length
    file.write_all(&0x004E4942u32.to_le_bytes())?; // chunk type "BIN\0"
    file.write_all(&data.binary)?;
    file.write_all(&vec![0u8; bin_padding])?; // padding with zeros

    Ok(())
}

fn write_gltf_separate(path: &Path, data: &GltfData) -> Result<(), std::io::Error> {
    // Determine the binary filename
    let bin_filename = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| format!("{s}.bin"))
        .unwrap_or_else(|| "buffer.bin".to_string());

    // Update JSON to reference external buffer
    let mut json = data.json.clone();
    if let Some(buffers) = json.get_mut("buffers").and_then(|b| b.as_array_mut())
        && let Some(buffer) = buffers.first_mut()
    {
        buffer["uri"] = serde_json::Value::String(bin_filename.clone());
    }

    // Write JSON file
    let json_path = path.with_extension("gltf");
    let json_file = std::fs::File::create(&json_path)?;
    serde_json::to_writer_pretty(json_file, &json)?;

    // Write binary file
    let bin_path = path.with_extension("bin");
    std::fs::write(&bin_path, &data.binary)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LodMesh, LodStats, Vertex, mesh::MaterialType};
    use glam::{Vec2, Vec3, Vec4};
    use std::fs;
    use tempfile::tempdir;

    fn create_test_mesh() -> Mesh {
        let mut mesh = Mesh::new();

        // Create a simple triangle
        mesh.vertices.push(Vertex::with_pivot_painter(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::Y,
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        ));
        mesh.vertices.push(Vertex::with_pivot_painter(
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::Y,
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 0.5),
            Vec4::new(1.0, 0.0, 0.0, 1.0),
        ));
        mesh.vertices.push(Vertex::with_pivot_painter(
            Vec3::new(0.5, 1.0, 0.0),
            Vec3::Y,
            Vec2::new(0.5, 1.0),
            Vec2::new(1.0, 1.0),
            Vec4::new(0.5, 1.0, 0.0, 1.0),
        ));

        mesh.indices.extend_from_slice(&[0, 1, 2]);

        mesh.submeshes.push(Submesh {
            index_start: 0,
            index_count: 3,
            material: MaterialType::Bark,
        });

        mesh
    }

    fn create_multi_material_mesh() -> Mesh {
        let mut mesh = Mesh::new();

        mesh.vertices
            .push(Vertex::new(Vec3::ZERO, Vec3::Y, Vec2::ZERO));
        mesh.vertices
            .push(Vertex::new(Vec3::X, Vec3::Y, Vec2::ZERO));
        mesh.vertices
            .push(Vertex::new(Vec3::Y, Vec3::Y, Vec2::ZERO));
        mesh.vertices
            .push(Vertex::new(Vec3::Z, Vec3::Y, Vec2::ZERO));
        mesh.vertices
            .push(Vertex::new(Vec3::new(1.0, 0.0, 1.0), Vec3::Y, Vec2::ZERO));
        mesh.vertices
            .push(Vertex::new(Vec3::new(0.0, 1.0, 1.0), Vec3::Y, Vec2::ZERO));

        mesh.indices.extend_from_slice(&[0, 1, 2, 3, 4, 5]);
        mesh.submeshes.push(Submesh {
            index_start: 0,
            index_count: 3,
            material: MaterialType::Bark,
        });
        mesh.submeshes.push(Submesh {
            index_start: 3,
            index_count: 3,
            material: MaterialType::Leaves,
        });

        mesh
    }

    fn create_lod_mesh_set_for_export() -> LodMeshSet {
        let high_mesh = create_multi_material_mesh();
        let low_mesh = create_multi_material_mesh();

        LodMeshSet {
            meshes: vec![
                LodMesh {
                    index: 0,
                    name: "High".to_string(),
                    screen_height: 0.3,
                    stats: LodStats {
                        vertex_count: high_mesh.vertex_count() as u32,
                        triangle_count: high_mesh.triangle_count() as u32,
                        branch_count: 1,
                        leaf_count: 1,
                    },
                    mesh: high_mesh,
                    impostor_atlas: None,
                },
                LodMesh {
                    index: 1,
                    name: "Low".to_string(),
                    screen_height: 0.1,
                    stats: LodStats {
                        vertex_count: low_mesh.vertex_count() as u32,
                        triangle_count: low_mesh.triangle_count() as u32,
                        branch_count: 1,
                        leaf_count: 1,
                    },
                    mesh: low_mesh,
                    impostor_atlas: None,
                },
            ],
        }
    }

    fn parse_glb(glb: &[u8]) -> (serde_json::Value, &[u8]) {
        assert!(glb.len() >= 28, "GLB should contain header and two chunks");
        assert_eq!(&glb[0..4], b"glTF");
        assert_eq!(read_u32(glb, 4), 2);
        assert_eq!(read_u32(glb, 8) as usize, glb.len());

        let json_chunk_len = read_u32(glb, 12) as usize;
        let json_chunk_type = read_u32(glb, 16);
        assert_eq!(json_chunk_type, 0x4E4F534A);
        let json_start = 20;
        let json_end = json_start + json_chunk_len;
        assert!(json_end + 8 <= glb.len());

        let bin_chunk_len = read_u32(glb, json_end) as usize;
        let bin_chunk_type = read_u32(glb, json_end + 4);
        assert_eq!(bin_chunk_type, 0x004E4942);
        let bin_start = json_end + 8;
        let bin_end = bin_start + bin_chunk_len;
        assert!(bin_end <= glb.len());

        let json = std::str::from_utf8(&glb[json_start..json_end])
            .unwrap()
            .trim_end_matches(' ');
        let parsed = serde_json::from_str(json).unwrap();

        (parsed, &glb[bin_start..bin_end])
    }

    fn read_u32(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    }

    fn assert_primitive_attributes(json: &serde_json::Value, primitive: &serde_json::Value) {
        let attributes = primitive["attributes"].as_object().unwrap();
        for name in [
            "POSITION",
            "NORMAL",
            "TANGENT",
            "TEXCOORD_0",
            "TEXCOORD_1",
            "COLOR_0",
        ] {
            let accessor_index = attributes[name].as_u64().unwrap() as usize;
            assert!(
                json["accessors"].get(accessor_index).is_some(),
                "{name} accessor should exist"
            );
        }

        let index_accessor = primitive["indices"].as_u64().unwrap() as usize;
        assert_eq!(json["accessors"][index_accessor]["type"], "SCALAR");
        assert_eq!(json["accessors"][index_accessor]["componentType"], 5125);
    }

    #[test]
    fn test_export_config_default() {
        let config = ExportConfig::default();
        assert_eq!(config.format, ExportFormat::Glb);
        assert!(!config.draco);
        assert!(config.textures.is_none());
        assert!(config.pivot_painter_extras);
    }

    #[test]
    fn test_export_empty_mesh_error() {
        let mesh = Mesh::new();
        let config = ExportConfig::default();
        let dir = tempdir().unwrap();
        let path = dir.path().join("empty.glb");

        let result = export_mesh(&mesh, &path, &config);
        assert!(matches!(result, Err(ExportError::NoMeshes)));
    }

    #[test]
    fn test_compute_bounds() {
        let mesh = create_test_mesh();
        let (min, max) = compute_bounds(&mesh);

        assert_eq!(min[0], 0.0);
        assert_eq!(min[1], 0.0);
        assert_eq!(min[2], 0.0);
        assert_eq!(max[0], 1.0);
        assert_eq!(max[1], 1.0);
        assert_eq!(max[2], 0.0);
    }

    #[test]
    fn test_compute_bounds_empty() {
        let mesh = Mesh::new();
        let (min, max) = compute_bounds(&mesh);

        assert_eq!(min, [0.0; 3]);
        assert_eq!(max, [0.0; 3]);
    }

    #[test]
    fn test_build_positions_buffer() {
        let mesh = create_test_mesh();
        let buffer = build_positions_buffer(&mesh);

        // 3 vertices * 3 floats * 4 bytes = 36 bytes
        assert_eq!(buffer.len(), 36);
    }

    #[test]
    fn test_build_normals_buffer() {
        let mesh = create_test_mesh();
        let buffer = build_normals_buffer(&mesh);

        assert_eq!(buffer.len(), 36);
    }

    #[test]
    fn test_build_tangents_buffer() {
        let mesh = create_test_mesh();
        let buffer = build_tangents_buffer(&mesh);

        // 3 vertices * 4 floats * 4 bytes = 48 bytes
        assert_eq!(buffer.len(), 48);
        for chunk in buffer.as_chunks::<16>().0 {
            let x = f32::from_le_bytes(chunk[0..4].try_into().unwrap());
            let y = f32::from_le_bytes(chunk[4..8].try_into().unwrap());
            let z = f32::from_le_bytes(chunk[8..12].try_into().unwrap());
            let w = f32::from_le_bytes(chunk[12..16].try_into().unwrap());
            let length = (x * x + y * y + z * z).sqrt();
            assert!((length - 1.0).abs() < 0.001);
            assert!(y.abs() < 0.001);
            assert_eq!(w.abs(), 1.0);
        }
    }

    #[test]
    fn test_build_texcoord_buffers() {
        let mesh = create_test_mesh();

        let tc0 = build_texcoord0_buffer(&mesh);
        let tc1 = build_texcoord1_buffer(&mesh);

        // 3 vertices * 2 floats * 4 bytes = 24 bytes
        assert_eq!(tc0.len(), 24);
        assert_eq!(tc1.len(), 24);
    }

    #[test]
    fn test_build_color_buffer() {
        let mesh = create_test_mesh();
        let buffer = build_color0_buffer(&mesh);

        // 3 vertices * 4 floats * 4 bytes = 48 bytes
        assert_eq!(buffer.len(), 48);
    }

    #[test]
    fn test_build_indices_buffer() {
        let mesh = create_test_mesh();
        let buffer = build_indices_buffer(&mesh);

        // 3 indices * 4 bytes = 12 bytes
        assert_eq!(buffer.len(), 12);
    }

    #[test]
    fn test_export_mesh_glb() {
        let mesh = create_test_mesh();
        let config = ExportConfig::default();
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.glb");

        let result = export_mesh(&mesh, &path, &config);
        assert!(result.is_ok());

        // Verify file exists
        assert!(path.exists());

        // Verify GLB magic bytes
        let data = fs::read(&path).unwrap();
        assert!(data.len() >= 12);
        assert_eq!(&data[0..4], b"glTF");
        assert_eq!(u32::from_le_bytes([data[4], data[5], data[6], data[7]]), 2); // version
    }

    #[test]
    fn test_export_mesh_gltf() {
        let mesh = create_test_mesh();
        let config = ExportConfig {
            format: ExportFormat::GlTf,
            ..Default::default()
        };
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.gltf");

        let result = export_mesh(&mesh, &path, &config);
        assert!(result.is_ok());

        // Verify both files exist
        assert!(path.exists());
        let bin_path = path.with_extension("bin");
        assert!(bin_path.exists());

        // Verify JSON is valid
        let json_data = fs::read_to_string(&path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_data).unwrap();

        assert_eq!(json["asset"]["version"], "2.0");
        assert_eq!(json["asset"]["generator"], "midori");
    }

    #[test]
    fn test_export_lod_meshes_empty() {
        let lods = LodMeshSet { meshes: vec![] };
        let config = ExportConfig::default();
        let dir = tempdir().unwrap();
        let path = dir.path().join("empty_lods.glb");

        let result = export_lod_meshes(&lods, &path, &config);
        assert!(matches!(result, Err(ExportError::NoMeshes)));
    }

    #[test]
    fn test_gltf_json_structure() {
        let mesh = create_test_mesh();
        let config = ExportConfig::default();
        let gltf_data = build_gltf_single(&mesh, &config).unwrap();

        let json = &gltf_data.json;

        // Verify required glTF fields
        assert!(json.get("asset").is_some());
        assert!(json.get("scene").is_some());
        assert!(json.get("scenes").is_some());
        assert!(json.get("nodes").is_some());
        assert!(json.get("meshes").is_some());
        assert!(json.get("accessors").is_some());
        assert!(json.get("bufferViews").is_some());
        assert!(json.get("buffers").is_some());
        assert!(json.get("materials").is_some());

        // Verify extras for pivot painter
        assert!(json.get("extras").is_some());
        assert_eq!(json["extras"]["pivot_painter"], true);
    }

    #[test]
    fn test_gltf_materials() {
        let mesh = create_test_mesh();
        let config = ExportConfig::default();
        let gltf_data = build_gltf_single(&mesh, &config).unwrap();

        let materials = gltf_data.json["materials"].as_array().unwrap();
        assert_eq!(materials.len(), 2);

        // Bark material
        assert_eq!(materials[0]["name"], "bark");
        assert_eq!(materials[0]["doubleSided"], false);

        // Leaves material
        assert_eq!(materials[1]["name"], "leaves");
        assert_eq!(materials[1]["doubleSided"], true);
        assert_eq!(materials[1]["alphaMode"], "MASK");
    }

    #[test]
    fn test_gltf_primitives_split_by_submesh_material() {
        let mesh = create_multi_material_mesh();
        let config = ExportConfig::default();
        let gltf_data = build_gltf_single(&mesh, &config).unwrap();

        let primitives = gltf_data.json["meshes"][0]["primitives"]
            .as_array()
            .unwrap();
        assert_eq!(primitives.len(), 2);
        assert_eq!(primitives[0]["material"], 0);
        assert_eq!(primitives[1]["material"], 1);

        let accessors = gltf_data.json["accessors"].as_array().unwrap();
        let first_indices = primitives[0]["indices"].as_u64().unwrap() as usize;
        let second_indices = primitives[1]["indices"].as_u64().unwrap() as usize;
        assert_eq!(accessors[first_indices]["count"], 3);
        assert_eq!(accessors[second_indices]["count"], 3);
        assert_eq!(accessors[second_indices]["byteOffset"], 12);
    }

    #[test]
    fn test_gltf_metadata_and_lod_node_name() {
        let mesh = create_test_mesh();
        let config = ExportConfig {
            metadata: Some(ExportMetadata {
                species_name: "English Oak".to_string(),
                scientific_name: "Quercus robur".to_string(),
                seed: Some(42),
                lod_screen_heights: vec![0.3],
            }),
            ..Default::default()
        };
        let gltf_data = build_gltf_single(&mesh, &config).unwrap();
        let json = &gltf_data.json;

        assert_eq!(json["nodes"][0]["name"], "English_Oak_LOD0");
        assert_eq!(json["scenes"][0]["name"], "English Oak");
        assert_eq!(json["extras"]["midori"]["species_name"], "English Oak");
        assert_eq!(json["extras"]["midori"]["scientific_name"], "Quercus robur");
        assert_eq!(json["extras"]["midori"]["seed"], 42);
        let screen_height = json["extras"]["midori"]["lod_screen_heights"][0]
            .as_f64()
            .unwrap();
        assert!((screen_height - 0.3).abs() < 0.0001);
        assert_eq!(json["extras"]["pivot_painter"], true);
    }

    #[test]
    fn test_export_lod_glb_bytes_parse_with_materials_lods_accessors_and_extras() {
        let lods = create_lod_mesh_set_for_export();
        let config = ExportConfig {
            metadata: Some(ExportMetadata {
                species_name: "English Oak".to_string(),
                scientific_name: "Quercus robur".to_string(),
                seed: Some(42),
                lod_screen_heights: vec![0.3, 0.1],
            }),
            ..Default::default()
        };

        let glb = export_lod_meshes_to_bytes(&lods, &config).unwrap();
        let (json, bin) = parse_glb(&glb);

        assert_eq!(json["asset"]["version"], "2.0");
        assert_eq!(json["asset"]["generator"], "midori");
        assert_eq!(
            json["buffers"][0]["byteLength"].as_u64().unwrap() as usize,
            bin.len()
        );

        assert_eq!(json["extras"]["pivot_painter"], true);
        assert_eq!(json["extras"]["midori"]["species_name"], "English Oak");
        assert_eq!(json["extras"]["midori"]["scientific_name"], "Quercus robur");
        assert_eq!(json["extras"]["midori"]["seed"], 42);
        let screen_heights = json["extras"]["midori"]["lod_screen_heights"]
            .as_array()
            .unwrap();
        assert!((screen_heights[0].as_f64().unwrap() - 0.3).abs() < 0.0001);
        assert!((screen_heights[1].as_f64().unwrap() - 0.1).abs() < 0.0001);

        let materials = json["materials"].as_array().unwrap();
        assert_eq!(materials[0]["name"], "bark");
        assert_eq!(materials[1]["name"], "leaves");

        assert_eq!(json["scenes"][0]["name"], "English Oak");
        assert_eq!(
            json["scenes"][0]["nodes"].as_array().unwrap(),
            &[serde_json::json!(0), serde_json::json!(1)]
        );
        assert_eq!(json["nodes"][0]["name"], "English_Oak_LOD0");
        assert_eq!(json["nodes"][1]["name"], "English_Oak_LOD1");

        let meshes = json["meshes"].as_array().unwrap();
        assert_eq!(meshes.len(), 2);
        for (mesh_index, mesh) in meshes.iter().enumerate() {
            assert_eq!(mesh["name"], format!("English_Oak_LOD{}", mesh_index));
            let primitives = mesh["primitives"].as_array().unwrap();
            assert_eq!(primitives.len(), 2);
            assert_eq!(primitives[0]["material"], 0);
            assert_eq!(primitives[1]["material"], 1);
            for primitive in primitives {
                assert_primitive_attributes(&json, primitive);
            }
        }

        let buffer_byte_length = json["buffers"][0]["byteLength"].as_u64().unwrap() as usize;
        for view in json["bufferViews"].as_array().unwrap() {
            let offset = view["byteOffset"].as_u64().unwrap() as usize;
            let length = view["byteLength"].as_u64().unwrap() as usize;
            assert!(offset + length <= buffer_byte_length);
        }
    }

    #[test]
    fn test_export_without_pivot_painter_extras() {
        let mesh = create_test_mesh();
        let config = ExportConfig {
            pivot_painter_extras: false,
            ..Default::default()
        };
        let gltf_data = build_gltf_single(&mesh, &config).unwrap();

        assert!(gltf_data.json.get("extras").is_none());
    }

    #[test]
    fn test_export_error_display() {
        let io_err = ExportError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "test"));
        assert!(io_err.to_string().contains("IO error"));

        let no_mesh_err = ExportError::NoMeshes;
        assert!(no_mesh_err.to_string().contains("No meshes"));
    }

    fn create_two_material_mesh() -> Mesh {
        // 3 verts bark + 3 verts leaves = 2 triangles, 2 submeshes.
        let mut mesh = Mesh::new();
        for i in 0..6 {
            mesh.vertices.push(Vertex::new(
                Vec3::new(i as f32, (i % 3) as f32, 0.0),
                Vec3::Y,
                Vec2::new(0.0, 0.0),
            ));
        }
        mesh.indices.extend_from_slice(&[0, 1, 2, 3, 4, 5]);
        mesh.submeshes.push(Submesh {
            index_start: 0,
            index_count: 3,
            material: MaterialType::Bark,
        });
        mesh.submeshes.push(Submesh {
            index_start: 3,
            index_count: 3,
            material: MaterialType::Leaves,
        });
        mesh
    }

    #[test]
    fn test_submeshes_split_into_primitives_with_materials() {
        let mesh = create_two_material_mesh();
        let config = ExportConfig::default();
        let gltf_data = build_gltf_single(&mesh, &config).unwrap();
        let json = &gltf_data.json;

        let primitives = json["meshes"][0]["primitives"].as_array().unwrap();
        assert_eq!(primitives.len(), 2, "one primitive per submesh");
        assert_eq!(primitives[0]["material"], 0); // bark
        assert_eq!(primitives[1]["material"], 1); // leaves

        // Each primitive has its own index accessor covering 3 indices.
        let a0 = primitives[0]["indices"].as_u64().unwrap() as usize;
        let a1 = primitives[1]["indices"].as_u64().unwrap() as usize;
        let accessors = json["accessors"].as_array().unwrap();
        assert_eq!(accessors[a0]["count"], 3);
        assert_eq!(accessors[a1]["count"], 3);
        // Both primitives share the index bufferView; the leaves primitive's
        // accessor is offset 12 bytes in (past the bark range).
        assert_eq!(
            accessors[a0]["bufferView"], accessors[a1]["bufferView"],
            "both primitives share the index bufferView"
        );
        assert_eq!(accessors[a1]["byteOffset"].as_u64().unwrap(), 12);
    }

    #[test]
    fn test_textured_export_embeds_images_and_materials() {
        use crate::species::Species;
        use crate::textures::TextureSet;

        let species = Species::from_toml(
            r#"
[species]
name = "Export Tex Test"
[trunk]
height = 4.0
radius = 0.2
[textures]
resolution = 64
"#,
        )
        .unwrap();

        let mesh = create_two_material_mesh();
        let config = ExportConfig {
            textures: Some(TextureSet::generate(&species)),
            ..Default::default()
        };
        let gltf_data = build_gltf_single(&mesh, &config).unwrap();
        let json = &gltf_data.json;

        // 3 embedded PNG images: bark albedo, bark normal, leaf card.
        let images = json["images"].as_array().unwrap();
        assert_eq!(images.len(), 3);
        assert!(images.iter().all(|i| i["mimeType"] == "image/png"));
        assert!(json["textures"].as_array().unwrap().len() == 3);
        assert!(json["samplers"].as_array().unwrap().len() == 2);

        let materials = json["materials"].as_array().unwrap();
        assert_eq!(
            materials[0]["pbrMetallicRoughness"]["baseColorTexture"]["index"],
            0
        );
        assert_eq!(materials[0]["normalTexture"]["index"], 1);
        assert_eq!(
            materials[1]["pbrMetallicRoughness"]["baseColorTexture"]["index"],
            2
        );
        assert_eq!(materials[1]["alphaMode"], "MASK");

        // The PNG payloads sit inside the binary buffer.
        assert!(gltf_data.binary.len() > 1000);
    }

    #[test]
    fn test_impostor_lod_exports_atlas_material() {
        use crate::lod::LodMesh;
        use crate::species::Species;
        use crate::textures::{RgbaTexture, TextureSet};

        let species = Species::from_toml(
            r#"
[species]
name = "Impostor Export"
[trunk]
height = 4.0
radius = 0.2
[textures]
resolution = 64
"#,
        )
        .unwrap();

        let mut mesh = create_two_material_mesh();
        // Add an impostor quad submesh.
        let base = mesh.vertices.len() as u32;
        for i in 0..4u32 {
            mesh.vertices.push(Vertex::new(
                Vec3::new((i % 2) as f32, (i / 2) as f32 + 2.0, 0.0),
                Vec3::Z,
                Vec2::new((i % 2) as f32, (i / 2) as f32),
            ));
        }
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        mesh.submeshes.push(Submesh {
            index_start: 6,
            index_count: 6,
            material: MaterialType::Impostor,
        });

        let lod = LodMesh {
            index: 0,
            name: "Test".to_string(),
            mesh,
            screen_height: 0.5,
            impostor_atlas: Some(RgbaTexture::new(16, 8)),
            stats: Default::default(),
        };
        let lods = LodMeshSet { meshes: vec![lod] };
        let config = ExportConfig {
            textures: Some(TextureSet::generate(&species)),
            ..Default::default()
        };
        let gltf_data = build_gltf_lods(&lods, &config).unwrap();
        let json = &gltf_data.json;

        // bark_albedo + bark_normal + leaf_card + impostor atlas
        assert_eq!(json["images"].as_array().unwrap().len(), 4);
        let materials = json["materials"].as_array().unwrap();
        assert_eq!(materials.len(), 3); // bark, leaves, impostor
        assert_eq!(
            materials[2]["pbrMetallicRoughness"]["baseColorTexture"]["index"],
            3
        );
        assert_eq!(materials[2]["alphaMode"], "MASK");

        let primitives = json["meshes"][0]["primitives"].as_array().unwrap();
        assert_eq!(primitives.len(), 3);
        assert_eq!(primitives[2]["material"], 2);
    }

    #[test]
    fn test_glb_bytes_contain_png_chunks() {
        use crate::species::Species;
        use crate::textures::TextureSet;

        let species = Species::from_toml(
            r#"
[species]
name = "GLB Tex"
[trunk]
height = 4.0
radius = 0.2
[textures]
resolution = 64
"#,
        )
        .unwrap();
        let mesh = create_two_material_mesh();
        let config = ExportConfig {
            textures: Some(TextureSet::generate(&species)),
            ..Default::default()
        };
        let data = build_gltf_single(&mesh, &config).unwrap();
        let glb = build_glb_bytes(&data).unwrap();
        assert_eq!(&glb[0..4], b"glTF");
        // PNG magic must appear inside the BIN chunk payload.
        let png_magic = [0x89, 0x50, 0x4E, 0x47];
        assert!(
            glb.windows(4).any(|w| w == png_magic),
            "GLB should contain embedded PNG data"
        );
    }
}
