//! WebAssembly bindings for Grove tree generator.
//!
//! This crate provides a WebAssembly interface for using grove in web browsers
//! and other WASM-compatible environments. It wraps the core functionality
//! from grove-core with wasm-bindgen bindings.
//!
//! # Features
//!
//! - Generate trees directly in the browser
//! - Multiple LOD levels for efficient rendering
//! - Real-time parameter adjustment with immediate preview

use grove_core::{
    export_lod_meshes_to_bytes,
    generate_tree as core_generate_tree,
    lod::{generate_lod_meshes_with_config, LodGenerationConfig},
    ExportConfig, Mesh, Species,
};
use wasm_bindgen::prelude::*;

/// Initialize panic hook for better error messages in WASM.
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Tree generator that holds a parsed species definition.
///
/// Create a generator from a TOML string, then use it to generate
/// trees with different seeds.
#[wasm_bindgen]
pub struct GroveGenerator {
    species: Species,
}

#[wasm_bindgen]
impl GroveGenerator {
    /// Create a new generator from a TOML species definition string.
    #[wasm_bindgen(constructor)]
    pub fn new(toml: &str) -> Result<GroveGenerator, JsValue> {
        let species = Species::from_toml(toml)
            .map_err(|e| JsValue::from_str(&format!("Parse error: {}", e)))?;
        Ok(Self { species })
    }

    /// Get the species name.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.species.species.name.clone()
    }

    /// Generate a tree and return mesh data as a JavaScript object.
    ///
    /// Returns an object containing all LOD levels with their mesh data.
    #[wasm_bindgen]
    pub fn generate(&self, seed: u64) -> Result<JsValue, JsValue> {
        let tree = core_generate_tree(&self.species, seed);

        // Generate LOD meshes using balanced config
        let lod_config = LodGenerationConfig::balanced();
        let lods = generate_lod_meshes_with_config(&tree, &self.species, &lod_config);

        // Convert to JS-friendly format
        let result = MeshOutput::from_lods(&lods);
        serde_wasm_bindgen::to_value(&result)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }

    /// Generate only a specific LOD level.
    #[wasm_bindgen]
    pub fn generate_lod(&self, seed: u64, lod_level: u32) -> Result<JsValue, JsValue> {
        let tree = core_generate_tree(&self.species, seed);

        let lod_config = LodGenerationConfig::balanced();
        let lods = generate_lod_meshes_with_config(&tree, &self.species, &lod_config);

        if let Some(lod) = lods.get(lod_level) {
            let result = SingleMeshOutput::from_mesh(&lod.mesh, &lod.name);
            serde_wasm_bindgen::to_value(&result)
                .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
        } else {
            Err(JsValue::from_str(&format!(
                "LOD level {} not found",
                lod_level
            )))
        }
    }

    /// Get generation statistics without full mesh data.
    ///
    /// Useful for previewing tree complexity before generating full mesh.
    #[wasm_bindgen]
    pub fn get_stats(&self, seed: u64) -> Result<JsValue, JsValue> {
        let tree = core_generate_tree(&self.species, seed);

        let stats = TreeStats {
            stem_count: tree.stems.len() as u32,
            leaf_count: tree.leaves.len() as u32,
            bounds_min: [tree.bounds.min.x, tree.bounds.min.y, tree.bounds.min.z],
            bounds_max: [tree.bounds.max.x, tree.bounds.max.y, tree.bounds.max.z],
        };

        serde_wasm_bindgen::to_value(&stats)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }

    /// Export tree as GLB binary data.
    ///
    /// Returns a Uint8Array containing the complete GLB file.
    #[wasm_bindgen]
    pub fn export_glb(&self, seed: u64) -> Result<js_sys::Uint8Array, JsValue> {
        let tree = core_generate_tree(&self.species, seed);

        // Generate LOD meshes
        let lod_config = LodGenerationConfig::balanced();
        let lods = generate_lod_meshes_with_config(&tree, &self.species, &lod_config);

        // Export to GLB bytes
        let config = ExportConfig::default();
        let glb_bytes = export_lod_meshes_to_bytes(&lods, &config)
            .map_err(|e| JsValue::from_str(&format!("Export error: {}", e)))?;

        // Convert to JS Uint8Array
        let array = js_sys::Uint8Array::new_with_length(glb_bytes.len() as u32);
        array.copy_from(&glb_bytes);
        Ok(array)
    }
}

/// Quick generation without creating a generator instance.
///
/// Convenience function for one-off tree generation.
#[wasm_bindgen]
pub fn generate_tree_from_toml(toml: &str, seed: u64) -> Result<JsValue, JsValue> {
    let generator = GroveGenerator::new(toml)?;
    generator.generate(seed)
}

// Serializable output structures

/// Complete mesh output containing all LOD levels.
#[derive(serde::Serialize)]
struct MeshOutput {
    lods: Vec<LodOutput>,
}

/// Single LOD level output.
#[derive(serde::Serialize)]
struct LodOutput {
    name: String,
    vertices: VertexData,
    indices: Vec<u32>,
    submeshes: Vec<SubmeshOutput>,
    vertex_count: u32,
    triangle_count: u32,
}

/// Submesh output for multi-material rendering.
#[derive(serde::Serialize)]
struct SubmeshOutput {
    /// Starting index in the indices array
    start: u32,
    /// Number of indices in this submesh
    count: u32,
    /// Material type: 0 = Bark, 1 = Leaf
    material_type: u32,
}

/// Single mesh output (for generate_lod).
#[derive(serde::Serialize)]
struct SingleMeshOutput {
    name: String,
    vertices: VertexData,
    indices: Vec<u32>,
    vertex_count: u32,
    triangle_count: u32,
}

/// Vertex attribute data in flat arrays for WebGL/WebGPU.
#[derive(serde::Serialize)]
struct VertexData {
    /// Positions as flat array [x,y,z, x,y,z, ...]
    positions: Vec<f32>,
    /// Normals as flat array [x,y,z, x,y,z, ...]
    normals: Vec<f32>,
    /// UV coordinates as flat array [u,v, u,v, ...]
    uvs: Vec<f32>,
    /// Pivot painter UV2 data [u,v, u,v, ...]
    uv2s: Vec<f32>,
    /// Vertex colors as flat array [r,g,b,a, r,g,b,a, ...]
    colors: Vec<f32>,
}

/// Tree statistics without mesh data.
#[derive(serde::Serialize)]
struct TreeStats {
    stem_count: u32,
    leaf_count: u32,
    bounds_min: [f32; 3],
    bounds_max: [f32; 3],
}

impl MeshOutput {
    fn from_lods(lods: &grove_core::LodMeshSet) -> Self {
        Self {
            lods: lods
                .meshes
                .iter()
                .map(|lod| LodOutput {
                    name: lod.name.clone(),
                    vertices: VertexData::from_mesh(&lod.mesh),
                    indices: lod.mesh.indices.clone(),
                    submeshes: lod
                        .mesh
                        .submeshes
                        .iter()
                        .map(|s| SubmeshOutput {
                            start: s.index_start,
                            count: s.index_count,
                            material_type: match s.material {
                                grove_core::MaterialType::Bark => 0,
                                grove_core::MaterialType::Leaves => 1,
                            },
                        })
                        .collect(),
                    vertex_count: lod.stats.vertex_count,
                    triangle_count: lod.stats.triangle_count,
                })
                .collect(),
        }
    }
}

impl SingleMeshOutput {
    fn from_mesh(mesh: &Mesh, name: &str) -> Self {
        Self {
            name: name.to_string(),
            vertices: VertexData::from_mesh(mesh),
            indices: mesh.indices.clone(),
            vertex_count: mesh.vertices.len() as u32,
            triangle_count: (mesh.indices.len() / 3) as u32,
        }
    }
}

impl VertexData {
    fn from_mesh(mesh: &Mesh) -> Self {
        let mut positions = Vec::with_capacity(mesh.vertices.len() * 3);
        let mut normals = Vec::with_capacity(mesh.vertices.len() * 3);
        let mut uvs = Vec::with_capacity(mesh.vertices.len() * 2);
        let mut uv2s = Vec::with_capacity(mesh.vertices.len() * 2);
        let mut colors = Vec::with_capacity(mesh.vertices.len() * 4);

        for v in &mesh.vertices {
            positions.extend_from_slice(&[v.position.x, v.position.y, v.position.z]);
            normals.extend_from_slice(&[v.normal.x, v.normal.y, v.normal.z]);
            uvs.extend_from_slice(&[v.uv.x, v.uv.y]);
            uv2s.extend_from_slice(&[v.uv2.x, v.uv2.y]);
            colors.extend_from_slice(&[v.color.x, v.color.y, v.color.z, v.color.w]);
        }

        Self {
            positions,
            normals,
            uvs,
            uv2s,
            colors,
        }
    }
}
