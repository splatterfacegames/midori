use midori_core::{
    ExportConfig, LodGenerationConfig, Species, export_lod_meshes_to_bytes,
    generate_lod_meshes_with_config, generate_tree,
};
use std::path::PathBuf;

fn workspace_preset(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(path)
}

#[test]
fn oak_preset_seed_42_snapshot() {
    let species = Species::from_file(&workspace_preset("presets/species/oak.toml")).unwrap();
    let tree = generate_tree(&species, 42);

    assert_eq!(tree.stem_count(), 147);
    assert_eq!(tree.leaf_count(), 2936);
    assert!(tree.bounds.is_valid());

    let lods = generate_lod_meshes_with_config(&tree, &species, &LodGenerationConfig::balanced());
    let stats: Vec<_> = lods
        .meshes
        .iter()
        .map(|lod| {
            (
                lod.index,
                lod.stats.vertex_count,
                lod.stats.triangle_count,
                lod.stats.leaf_count,
            )
        })
        .collect();

    assert_eq!(
        stats,
        vec![
            (0, 27984, 30021, 2936),
            (1, 8556, 10014, 1174),
            (2, 1314, 1899, 1)
        ]
    );

    let glb = export_lod_meshes_to_bytes(&lods, &ExportConfig::default()).unwrap();
    assert_eq!(&glb[0..4], b"glTF");
}

#[test]
fn minimal_species_seed_7_snapshot() {
    let species = Species::from_toml(
        r#"
[species]
name = "Minimal Fixture"

[trunk]
height = 4.0
radius = 0.25
segments = 4
"#,
    )
    .unwrap();
    let tree = generate_tree(&species, 7);

    assert_eq!(tree.stem_count(), 1);
    assert_eq!(tree.leaf_count(), 0);
    assert!(tree.bounds.is_valid());

    let lods = generate_lod_meshes(&tree, &species);
    let stats: Vec<_> = lods
        .meshes
        .iter()
        .map(|lod| {
            (
                lod.index,
                lod.stats.vertex_count,
                lod.stats.triangle_count,
                lod.stats.leaf_count,
            )
        })
        .collect();

    assert_eq!(stats, vec![(0, 46, 72, 0), (1, 36, 54, 0), (2, 34, 44, 1)]);
}

fn generate_lod_meshes(tree: &midori_core::Tree, species: &Species) -> midori_core::LodMeshSet {
    midori_core::generate_lod_meshes(tree, species)
}
