//! WebAssembly bindings for Grove tree generator.
//!
//! This crate provides a WebAssembly interface for using grove in web browsers
//! and other WASM-compatible environments. It wraps the core functionality
//! from grove-core with wasm-bindgen bindings.
//!
//! # Features
//!
//! - Generate trees directly in the browser
//! - Multiple LOD levels for efficient rendering (driven by the species' `[lod]` config)
//! - Species parameters round-trip between JSON (editors) and TOML (document authority)
//! - glTF 2.0 export as single-file GLB or `.gltf` + `.bin` parts

use grove_core::{
    ExportConfig, Mesh, Species, TextureSet, export_lod_meshes_to_bytes,
    export_lod_meshes_to_parts, generate_tree as core_generate_tree,
    lod::{LodGenerationConfig, generate_lod_meshes_with_config},
    mesh::MaterialType,
};
use wasm_bindgen::prelude::*;

/// Initialize panic hook for better error messages in WASM.
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Tree generator that holds a parsed species definition.
///
/// Create a generator from a TOML string or a JSON species object, then use it
/// to generate trees with different seeds.
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

    /// Create a new generator from a species JSON object.
    ///
    /// Accepts the same shape produced by `toJson()`, so editors can mutate the
    /// object and round-trip it through the engine for validation.
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(value: JsValue) -> Result<GroveGenerator, JsValue> {
        let species = serde_wasm_bindgen::from_value::<Species>(value)
            .map_err(|e| JsValue::from_str(&format!("Invalid species: {}", e)))?;
        Ok(Self { species })
    }

    /// Return the species definition as a plain JS object.
    ///
    /// All defaulted fields are present in the output, so editors can render
    /// every parameter without knowing the defaults.
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.species)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }

    /// Serialize the species definition back to TOML.
    #[wasm_bindgen(js_name = toToml)]
    pub fn to_toml(&self) -> Result<String, JsValue> {
        toml::to_string_pretty(&self.species)
            .map_err(|e| JsValue::from_str(&format!("TOML serialization error: {}", e)))
    }

    /// Get the species name.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.species.species.name.clone()
    }

    /// Generate a tree and return mesh data as a JavaScript object.
    ///
    /// Returns an object containing all LOD levels with their mesh data.
    /// LOD generation is driven by the species' `[lod]` configuration.
    #[wasm_bindgen]
    pub fn generate(&self, seed: u64) -> Result<JsValue, JsValue> {
        let tree = core_generate_tree(&self.species, seed);
        let lods = self.generate_lods(&tree);

        let result = MeshOutput::from_lods(&lods);
        serde_wasm_bindgen::to_value(&result)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }

    /// Generate only a specific LOD level.
    #[wasm_bindgen]
    pub fn generate_lod(&self, seed: u64, lod_level: u32) -> Result<JsValue, JsValue> {
        let tree = core_generate_tree(&self.species, seed);
        let lods = self.generate_lods(&tree);

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
    /// Returns a Uint8Array containing the complete GLB file. When
    /// `embed_textures` is true the species' generated material maps (bark
    /// albedo+normal, leaf card) are embedded; baked impostor atlases are
    /// always embedded when a LOD uses `crown_impostor`.
    #[wasm_bindgen]
    pub fn export_glb(
        &self,
        seed: u64,
        embed_textures: bool,
    ) -> Result<js_sys::Uint8Array, JsValue> {
        let tree = core_generate_tree(&self.species, seed);
        let lods = self.generate_lods(&tree);

        let config = self.export_config(embed_textures);
        let glb_bytes = export_lod_meshes_to_bytes(&lods, &config)
            .map_err(|e| JsValue::from_str(&format!("Export error: {}", e)))?;

        let array = js_sys::Uint8Array::new_with_length(glb_bytes.len() as u32);
        array.copy_from(&glb_bytes);
        Ok(array)
    }

    /// Export tree as separate `.gltf` JSON + `.bin` parts.
    ///
    /// `bin_name` is written into the glTF buffer URI. Returns an object with
    /// `gltf` and `bin` Uint8Array fields. Embedded images ride inside `.bin`
    /// via bufferView references.
    #[wasm_bindgen(js_name = exportGltf)]
    pub fn export_gltf(
        &self,
        seed: u64,
        bin_name: &str,
        embed_textures: bool,
    ) -> Result<JsValue, JsValue> {
        let tree = core_generate_tree(&self.species, seed);
        let lods = self.generate_lods(&tree);

        let config = self.export_config(embed_textures);
        let (gltf, bin) = export_lod_meshes_to_parts(&lods, bin_name, &config)
            .map_err(|e| JsValue::from_str(&format!("Export error: {}", e)))?;

        let result = js_sys::Object::new();
        let gltf_array = js_sys::Uint8Array::new_with_length(gltf.len() as u32);
        gltf_array.copy_from(&gltf);
        let bin_array = js_sys::Uint8Array::new_with_length(bin.len() as u32);
        bin_array.copy_from(&bin);
        js_sys::Reflect::set(&result, &"gltf".into(), &gltf_array)?;
        js_sys::Reflect::set(&result, &"bin".into(), &bin_array)?;
        Ok(result.into())
    }

    /// Generate the species' material maps as PNG bytes.
    ///
    /// Returns `{ bark_albedo, bark_normal, leaf_card }` Uint8Array PNGs —
    /// deterministic for the species' `[textures]` parameters. The browser
    /// has no filesystem, so file-slot overrides are ignored here (procedural
    /// maps are used); native hosts resolve slots via `TextureSet::resolve`.
    #[wasm_bindgen(js_name = generateMaps)]
    pub fn generate_maps(&self) -> Result<JsValue, JsValue> {
        let textures = TextureSet::generate(&self.species);

        let result = js_sys::Object::new();
        for (name, png) in [
            ("bark_albedo", textures.bark_albedo.to_png()),
            ("bark_normal", textures.bark_normal.to_png()),
            ("leaf_card", textures.leaf_card.to_png()),
        ] {
            let bytes =
                png.map_err(|e| JsValue::from_str(&format!("Texture encode error: {}", e)))?;
            let array = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
            array.copy_from(&bytes);
            js_sys::Reflect::set(&result, &name.into(), &array)?;
        }
        Ok(result.into())
    }

    fn export_config(&self, embed_textures: bool) -> ExportConfig {
        ExportConfig {
            textures: embed_textures.then(|| TextureSet::generate(&self.species)),
            ..ExportConfig::default()
        }
    }

    fn generate_lods(&self, tree: &grove_core::Tree) -> grove_core::LodMeshSet {
        let lod_config = LodGenerationConfig::from_species(&self.species);
        generate_lod_meshes_with_config(tree, &self.species, &lod_config)
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
    /// Material ranges over `indices`, for split bark/leaf rendering.
    submeshes: Vec<SubmeshOutput>,
    /// Baked impostor atlas as PNG bytes (absent unless crown_impostor).
    #[serde(skip_serializing_if = "Option::is_none")]
    impostor_atlas: Option<Vec<u8>>,
    vertex_count: u32,
    triangle_count: u32,
}

/// Single mesh output (for generate_lod).
#[derive(serde::Serialize)]
struct SingleMeshOutput {
    name: String,
    vertices: VertexData,
    indices: Vec<u32>,
    submeshes: Vec<SubmeshOutput>,
    vertex_count: u32,
    triangle_count: u32,
}

/// A material range over the index buffer.
#[derive(serde::Serialize)]
struct SubmeshOutput {
    index_start: u32,
    index_count: u32,
    material: &'static str,
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
                    submeshes: submesh_outputs(&lod.mesh),
                    impostor_atlas: lod.impostor_atlas.as_ref().and_then(|a| a.to_png().ok()),
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
            submeshes: submesh_outputs(mesh),
            vertex_count: mesh.vertices.len() as u32,
            triangle_count: (mesh.indices.len() / 3) as u32,
        }
    }
}

fn submesh_outputs(mesh: &Mesh) -> Vec<SubmeshOutput> {
    mesh.submeshes
        .iter()
        .map(|sub| SubmeshOutput {
            index_start: sub.index_start,
            index_count: sub.index_count,
            material: match sub.material {
                MaterialType::Bark => "bark",
                MaterialType::Leaves => "leaves",
                MaterialType::Impostor => "impostor",
            },
        })
        .collect()
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
