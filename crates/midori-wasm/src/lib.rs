//! WebAssembly bindings for Midori tree generator.
//!
//! This crate provides a WebAssembly interface for using Midori in web browsers
//! and other WASM-compatible environments. It wraps the core functionality
//! from midori-core with wasm-bindgen bindings.
//!
//! # Features
//!
//! - Generate trees directly in the browser
//! - Multiple LOD levels for efficient rendering
//! - Real-time parameter adjustment with immediate preview

use midori_core::{
    ExportConfig, ExportMetadata, GeneratorFamily, GroundcoverKind, Mesh, NatureExportManifest,
    NaturePatch, ScatterSet, Species, export_lod_meshes_to_bytes,
    generate_tree as core_generate_tree,
    lod::{LodGenerationConfig, generate_lod_meshes_with_config},
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
pub struct MidoriGenerator {
    species: Species,
}

/// Nature patch generator that holds a parsed NaturePatch definition.
#[wasm_bindgen]
pub struct MidoriNatureGenerator {
    patch: NaturePatch,
}

#[wasm_bindgen]
impl MidoriGenerator {
    /// Create a new generator from a TOML species definition string.
    #[wasm_bindgen(constructor)]
    pub fn new(toml: &str) -> Result<MidoriGenerator, JsValue> {
        let species = Species::from_toml(toml)
            .map_err(|e| JsValue::from_str(&format!("Parse error: {}", e)))?;
        Ok(Self { species })
    }

    /// Get the species name.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.species.species.name.clone()
    }

    /// Get parsed species metadata for editor/tooling use.
    #[wasm_bindgen]
    pub fn metadata(&self) -> Result<JsValue, JsValue> {
        let metadata = SpeciesMetadata::from_species(&self.species);
        serde_wasm_bindgen::to_value(&metadata)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
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
            branch_count: tree.stems.iter().filter(|stem| stem.level > 0).count() as u32,
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
        let mut config = ExportConfig::default();
        config.metadata = Some(ExportMetadata {
            species_name: self.species.species.name.clone(),
            scientific_name: self.species.latin_name().to_string(),
            seed: Some(seed),
            lod_screen_heights: lods
                .meshes
                .iter()
                .map(|lod_mesh| lod_mesh.screen_height)
                .collect(),
        });
        let glb_bytes = export_lod_meshes_to_bytes(&lods, &config)
            .map_err(|e| JsValue::from_str(&format!("Export error: {}", e)))?;

        // Convert to JS Uint8Array
        let array = js_sys::Uint8Array::new_with_length(glb_bytes.len() as u32);
        array.copy_from(&glb_bytes);
        Ok(array)
    }
}

#[wasm_bindgen]
impl MidoriNatureGenerator {
    /// Create a new nature generator from a TOML NaturePatch definition string.
    #[wasm_bindgen(constructor)]
    pub fn new(toml: &str) -> Result<MidoriNatureGenerator, JsValue> {
        let patch = NaturePatch::from_toml(toml)
            .map_err(|e| JsValue::from_str(&format!("Nature patch parse error: {}", e)))?;
        Ok(Self { patch })
    }

    /// Get the patch display name.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.patch.asset.name.clone()
    }

    /// Generate terrain, prototype, scatter, and manifest data for browser preview.
    #[wasm_bindgen]
    pub fn preview(
        &self,
        preview_resolution: u32,
        scatter_chunk_size: f32,
    ) -> Result<JsValue, JsValue> {
        if preview_resolution < 2 {
            return Err(JsValue::from_str("preview resolution must be at least 2"));
        }
        if scatter_chunk_size <= 0.0 || !scatter_chunk_size.is_finite() {
            return Err(JsValue::from_str(
                "scatter chunk size must be a positive finite number",
            ));
        }

        let field = self.patch.terrain_field();
        let terrain_mesh = field.build_preview_mesh(preview_resolution);
        let terrain = MeshPreviewOutput::from_mesh(&terrain_mesh, "terrain_tile");
        let prototypes: Vec<NaturePrototypeOutput> = self
            .patch
            .generate_groundcover_prototypes()
            .into_iter()
            .map(|prototype| NaturePrototypeOutput {
                name: prototype.name,
                kind: groundcover_kind_name(prototype.kind).to_string(),
                lods: prototype
                    .lods
                    .into_iter()
                    .map(|lod| MeshPreviewOutput::from_mesh(&lod.mesh, &lod.name))
                    .collect(),
            })
            .collect();
        let prototype_count = prototypes.len() as u32;
        let scatter_sets = self.patch.generate_scatter_sets(scatter_chunk_size);
        let scatter_instance_count = scatter_sets
            .iter()
            .flat_map(|set| &set.chunks)
            .map(|chunk| chunk.instances.len())
            .sum::<usize>() as u32;
        let manifest = self.patch.export_manifest(preview_resolution);
        let result = NaturePreviewOutput {
            manifest,
            terrain,
            prototypes,
            scatter_sets,
            stats: NaturePreviewStats {
                tile_size: self.patch.patch.size,
                terrain_vertex_count: terrain_mesh.vertex_count() as u32,
                terrain_triangle_count: terrain_mesh.triangle_count() as u32,
                prototype_count,
                scatter_instance_count,
            },
        };

        serde_wasm_bindgen::to_value(&result)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))
    }
}

/// Quick generation without creating a generator instance.
///
/// Convenience function for one-off tree generation.
#[wasm_bindgen]
pub fn generate_tree_from_toml(toml: &str, seed: u64) -> Result<JsValue, JsValue> {
    let generator = MidoriGenerator::new(toml)?;
    generator.generate(seed)
}

/// Convenience function for one-off nature preview generation.
#[wasm_bindgen]
pub fn generate_nature_preview_from_toml(
    toml: &str,
    preview_resolution: u32,
    scatter_chunk_size: f32,
) -> Result<JsValue, JsValue> {
    let generator = MidoriNatureGenerator::new(toml)?;
    generator.preview(preview_resolution, scatter_chunk_size)
}

// Serializable output structures

/// Browser preview output for a NaturePatch.
#[derive(serde::Serialize)]
struct NaturePreviewOutput {
    manifest: NatureExportManifest,
    terrain: MeshPreviewOutput,
    prototypes: Vec<NaturePrototypeOutput>,
    scatter_sets: Vec<ScatterSet>,
    stats: NaturePreviewStats,
}

/// Groundcover prototype plus LOD meshes for preview.
#[derive(serde::Serialize)]
struct NaturePrototypeOutput {
    name: String,
    kind: String,
    lods: Vec<MeshPreviewOutput>,
}

/// Mesh output used by the nature preview path.
#[derive(serde::Serialize)]
struct MeshPreviewOutput {
    name: String,
    vertices: VertexData,
    indices: Vec<u32>,
    submeshes: Vec<SubmeshOutput>,
    vertex_count: u32,
    triangle_count: u32,
}

/// Nature preview stats for UI overlays and smoke checks.
#[derive(serde::Serialize)]
struct NaturePreviewStats {
    tile_size: f32,
    terrain_vertex_count: u32,
    terrain_triangle_count: u32,
    prototype_count: u32,
    scatter_instance_count: u32,
}

/// Complete mesh output containing all LOD levels.
#[derive(serde::Serialize)]
struct MeshOutput {
    lods: Vec<LodOutput>,
}

/// Single LOD level output.
#[derive(serde::Serialize)]
struct LodOutput {
    index: u32,
    name: String,
    vertices: VertexData,
    indices: Vec<u32>,
    submeshes: Vec<SubmeshOutput>,
    vertex_count: u32,
    triangle_count: u32,
    branch_count: u32,
    leaf_count: u32,
    screen_height: f32,
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
    index: u32,
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
    branch_count: u32,
    leaf_count: u32,
    bounds_min: [f32; 3],
    bounds_max: [f32; 3],
}

/// Species metadata exposed without requiring callers to parse TOML.
#[derive(serde::Serialize)]
struct SpeciesMetadata {
    name: String,
    scientific: String,
    latin: String,
    biome: String,
    tags: Vec<String>,
    generator_family: &'static str,
    material_bark: String,
    material_foliage: String,
    material_notes: String,
    control_group_count: u32,
}

impl SpeciesMetadata {
    fn from_species(species: &Species) -> Self {
        Self {
            name: species.species.name.clone(),
            scientific: species.species.scientific.clone(),
            latin: species.latin_name().to_string(),
            biome: species.species.biome.clone(),
            tags: species.species.tags.clone(),
            generator_family: generator_family_name(species.generator.family),
            material_bark: species.materials.bark.clone(),
            material_foliage: species.materials.foliage.clone(),
            material_notes: species.materials.notes.clone(),
            control_group_count: species.control_groups.len() as u32,
        }
    }
}

fn generator_family_name(family: GeneratorFamily) -> &'static str {
    match family {
        GeneratorFamily::WeberPenn => "weber_penn",
        GeneratorFamily::Dichotomous => "dichotomous",
        GeneratorFamily::Cactus => "cactus",
        GeneratorFamily::PadChain => "pad_chain",
        GeneratorFamily::Custom => "custom",
    }
}

impl MeshOutput {
    fn from_lods(lods: &midori_core::LodMeshSet) -> Self {
        Self {
            lods: lods
                .meshes
                .iter()
                .map(|lod| LodOutput {
                    index: lod.index,
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
                                midori_core::MaterialType::Bark => 0,
                                midori_core::MaterialType::Leaves => 1,
                            },
                        })
                        .collect(),
                    vertex_count: lod.stats.vertex_count,
                    triangle_count: lod.stats.triangle_count,
                    branch_count: lod.stats.branch_count,
                    leaf_count: lod.stats.leaf_count,
                    screen_height: lod.screen_height,
                })
                .collect(),
        }
    }
}

impl SingleMeshOutput {
    fn from_mesh(mesh: &Mesh, name: &str) -> Self {
        Self {
            index: 0,
            name: name.to_string(),
            vertices: VertexData::from_mesh(mesh),
            indices: mesh.indices.clone(),
            vertex_count: mesh.vertices.len() as u32,
            triangle_count: (mesh.indices.len() / 3) as u32,
        }
    }
}

impl MeshPreviewOutput {
    fn from_mesh(mesh: &Mesh, name: &str) -> Self {
        Self {
            name: name.to_string(),
            vertices: VertexData::from_mesh(mesh),
            indices: mesh.indices.clone(),
            submeshes: mesh
                .submeshes
                .iter()
                .map(|s| SubmeshOutput {
                    start: s.index_start,
                    count: s.index_count,
                    material_type: match s.material {
                        midori_core::MaterialType::Bark => 0,
                        midori_core::MaterialType::Leaves => 1,
                    },
                })
                .collect(),
            vertex_count: mesh.vertex_count() as u32,
            triangle_count: mesh.triangle_count() as u32,
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

fn groundcover_kind_name(kind: GroundcoverKind) -> &'static str {
    match kind {
        GroundcoverKind::Grass => "grass",
        GroundcoverKind::Moss => "moss",
        GroundcoverKind::Flower => "flower",
        GroundcoverKind::Weed => "weed",
        GroundcoverKind::Litter => "litter",
        GroundcoverKind::Shrub => "shrub",
        GroundcoverKind::Rock => "rock",
        GroundcoverKind::Log => "log",
    }
}
