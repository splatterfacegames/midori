//! Midori - Procedural tree generation library
//!
//! A standalone procedural tree generation tool for real-time game engines.
//! Generates 3D tree meshes with LOD, wind animation data, and AI-generated textures.
//!
//! # Example
//!
//! ```
//! use midori_core::{Species, generate_tree};
//!
//! let toml = r#"
//! [species]
//! name = "Oak"
//!
//! [trunk]
//! height = 6.0
//! radius = 0.45
//!
//! [branches.level1]
//! count = 5
//! length = 3.0
//! "#;
//!
//! let species = Species::from_toml(toml).unwrap();
//! let tree = generate_tree(&species, 12345).unwrap();
//!
//! println!("Generated {} stems", tree.stem_count());
//! ```

pub mod constants;
pub mod error;
pub mod export;
pub mod generation;
pub mod impostor;
pub mod leaves;
pub mod lod;
pub mod math;
pub mod mesh;
pub mod mesh_builder;
pub mod nature;
pub mod rng;
pub mod species;
pub mod textures;
pub mod tree;

pub use constants::*;
pub use error::Error;
pub use export::{
    ExportConfig, ExportError, ExportFormat, ExportMetadata, export_lod_meshes,
    export_lod_meshes_to_bytes, export_lod_meshes_to_parts, export_mesh,
};
pub use generation::{
    GenerationBudget, GenerationError, GenerationEstimate, estimate_tree, generate_tree,
    generate_tree_with_budget,
};
pub use impostor::{Impostor, bake_impostor};
pub use leaves::{LeafConfig, add_leaves_to_tree, generate_leaf_mesh, place_leaves};
pub use lod::{
    LodGenerationConfig, LodLevelConfig, LodMesh, LodMeshSet, LodStats, generate_lod_meshes,
    generate_lod_meshes_with_config,
};
pub use mesh::{MaterialType, Mesh, Submesh, Vertex};
pub use mesh_builder::{MeshBuilder, MeshConfig, build_mesh, build_mesh_with_config};
pub use nature::{
    AxisConventions, GroundcoverConfig, GroundcoverKind, GroundcoverLayer, GroundcoverPrototype,
    GroundcoverPrototypeLod, GroundcoverPrototypeLodManifest, GroundcoverPrototypeManifest,
    MapChannel, MaskSample, MaterialSlotManifest, NatureAssetInfo, NatureExportManifest,
    NatureMapSet, NaturePackageConfig, NaturePackageSummary, NaturePackageValidationReport,
    NaturePatch, NaturePatchError, NatureProfile, NatureProfiles, NormalConventions, PatchParams,
    ScatterBinaryFileManifest, ScatterBinaryFormatManifest, ScatterChunk, ScatterInstance,
    ScatterManifest, ScatterSet, SoilCracks, SoilParams, TerrainField, TerrainManifest,
    TerrainSample, UnityImportHints, UnrealImportHints, WindPackingManifest, WindParams,
    validate_nature_package,
};
pub use rng::Rng;
pub use species::{
    BarkStyle, ControlGroup, ControlSpec, GeneratorConfig, GeneratorFamily, LeafCardLayout,
    LeafShape, MaterialPlaceholders, TextureParams,
};
pub use species::{Species, SpeciesError, SpeciesFieldError};
pub use textures::{RgbaTexture, TextureError, TextureSet, species_texture_seed};
pub use tree::{BoundingBox, Leaf, Segment, Stem, Tree};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_generation() {
        let toml = r#"
[species]
name = "Test Oak"

[trunk]
height = 6.0
radius = 0.45
segments = 6

[branches.level1]
count = 5
length = 3.0
segments = 4

[crown]
shape = "spherical"
"#;

        let species = Species::from_toml(toml).unwrap();
        let tree = generate_tree(&species, 42).unwrap();

        // Verify basic tree structure
        assert_eq!(tree.species_name, "Test Oak");
        assert_eq!(tree.seed, 42);
        assert!(!tree.stems.is_empty());

        // Verify trunk exists
        let trunk = tree.trunk().unwrap();
        assert_eq!(trunk.level, 0);
        assert!(!trunk.segments.is_empty());

        // Verify branches were generated
        let branch_count = tree.stems_at_level(1).count();
        assert!(branch_count > 0);

        // Verify bounds are valid
        assert!(tree.bounds.is_valid());
    }

    #[test]
    fn test_deterministic_generation() {
        let toml = r#"
[species]
name = "Determinism Test"

[trunk]
height = 5.0

[branches.level1]
count = 4
length = 2.0
"#;

        let species = Species::from_toml(toml).unwrap();

        let tree1 = generate_tree(&species, 12345).unwrap();
        let tree2 = generate_tree(&species, 12345).unwrap();

        assert_eq!(tree1.stem_count(), tree2.stem_count());

        for (s1, s2) in tree1.stems.iter().zip(tree2.stems.iter()) {
            assert_eq!(s1.id, s2.id);
            assert_eq!(s1.level, s2.level);
            assert_eq!(s1.segments.len(), s2.segments.len());
        }
    }

    #[test]
    fn test_mesh_generation_integration() {
        let toml = r#"
[species]
name = "Mesh Test Oak"

[trunk]
height = 6.0
radius = 0.45
segments = 4

[branches.level1]
count = 5
length = 3.0
segments = 3
"#;

        let species = Species::from_toml(toml).unwrap();
        let tree = generate_tree(&species, 42).unwrap();

        // Generate mesh with default config
        let mesh = build_mesh(&tree);

        // Verify mesh was generated
        assert!(!mesh.is_empty(), "Mesh should not be empty");
        assert!(mesh.vertex_count() > 0, "Should have vertices");
        assert!(mesh.triangle_count() > 0, "Should have triangles");

        // Verify submesh
        assert_eq!(mesh.submeshes.len(), 1, "Should have one submesh");
        assert_eq!(mesh.submeshes[0].material, MaterialType::Bark);

        // Verify Pivot Painter data was encoded
        for vertex in &mesh.vertices {
            // UV2.x (depth) should be valid
            assert!(vertex.uv2.x >= 0.0 && vertex.uv2.x <= 1.0);
        }
    }

    #[test]
    fn test_mesh_config_customization() {
        let toml = r#"
[species]
name = "Config Test"

[trunk]
height = 4.0
radius = 0.3
"#;

        let species = Species::from_toml(toml).unwrap();
        let tree = generate_tree(&species, 123).unwrap();

        // Generate mesh with custom config
        let config = MeshConfig {
            ring_resolution: [8, 6, 4, 3],
            texture_v_scale: 2.0,
            pivot_painter: false,
            ..MeshConfig::default()
        };
        let mesh = build_mesh_with_config(&tree, config);

        // Verify mesh was generated
        assert!(!mesh.is_empty());

        // Verify pivot painter data is NOT encoded (disabled in config)
        for vertex in &mesh.vertices {
            assert_eq!(vertex.uv2, glam::Vec2::ZERO);
            assert_eq!(vertex.color, glam::Vec4::ZERO);
        }
    }
}
