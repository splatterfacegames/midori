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
    let tree = generate_tree(&species, 42).unwrap();

    assert_eq!(tree.stem_count(), 334);
    assert_eq!(tree.leaf_count(), 2183);
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
            (0, 62090, 70775, 2183),
            (1, 23912, 30146, 873),
            (2, 4171, 6203, 1)
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
    let tree = generate_tree(&species, 7).unwrap();

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

/// Per-LOD `(index, vertices, triangles, leaves)`.
type LodStats = Vec<(u32, u32, u32, u32)>;

fn preset_snapshot(
    preset: &str,
    state: Option<&str>,
    seed: u64,
) -> (usize, usize, usize, LodStats) {
    let path = workspace_preset(&format!("presets/species/{preset}.toml"));
    let species = Species::from_file_with_state(&path, state).unwrap();
    let tree = generate_tree(&species, seed).unwrap();
    assert!(tree.bounds.is_valid());
    let lods = generate_lod_meshes_with_config(&tree, &species, &LodGenerationConfig::balanced());
    let stats = lods
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
    let glb = export_lod_meshes_to_bytes(&lods, &ExportConfig::default()).unwrap();
    assert_eq!(&glb[0..4], b"glTF");
    (
        tree.stem_count(),
        tree.leaf_count(),
        tree.organs.len(),
        stats,
    )
}

#[test]
fn grass_tree_preset_seed_42_snapshot() {
    let (stems, leaves, organs, stats) = preset_snapshot("wa_grass_tree", None, 42);
    assert_eq!((stems, leaves, organs), (1, 460, 242));
    assert_eq!(
        stats,
        vec![
            (0, 12922, 14112, 460),
            (1, 2890, 2766, 184),
            (2, 90, 144, 1)
        ]
    );
}

#[test]
fn grass_tree_forked_state_seed_42_snapshot() {
    let (stems, leaves, organs, stats) = preset_snapshot("wa_grass_tree", Some("forked"), 42);
    assert_eq!((stems, leaves, organs), (4, 960, 454));
    assert_eq!(
        stats,
        vec![
            (0, 26500, 29176, 960),
            (1, 5938, 5722, 384),
            (2, 183, 279, 1)
        ]
    );
}

#[test]
fn firewood_banksia_preset_seed_42_snapshot() {
    let (stems, leaves, organs, stats) = preset_snapshot("wa_firewood_banksia", None, 42);
    assert_eq!((stems, leaves, organs), (157, 1680, 90));
    assert_eq!(
        stats,
        vec![
            (0, 44104, 48677, 1680),
            (1, 12976, 15165, 672),
            (2, 1957, 2841, 1)
        ]
    );
}

#[test]
fn jacaranda_flowering_state_seed_42_snapshot() {
    let (stems, leaves, organs, stats) = preset_snapshot("perth_jacaranda", Some("flowering"), 42);
    assert_eq!((stems, leaves, organs), (183, 149, 2800));
    assert_eq!(
        stats,
        vec![
            (0, 37415, 33073, 149),
            (1, 13268, 14611, 59),
            (2, 2495, 3647, 1)
        ]
    );
}

fn generate_lod_meshes(tree: &midori_core::Tree, species: &Species) -> midori_core::LodMeshSet {
    midori_core::generate_lod_meshes(tree, species)
}
