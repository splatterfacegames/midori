use std::path::Path;

use midori_core::{
    ExportConfig, LodGenerationConfig, MaterialType, NaturePackageConfig, NaturePatch, Species,
    TextureSet, export_lod_meshes_to_bytes, generate_lod_meshes_with_config, generate_tree,
    validate_nature_package,
};

fn build_and_export(
    toml: &str,
) -> (
    midori_core::Tree,
    midori_core::LodMeshSet,
    TextureSet,
    Vec<u8>,
) {
    let species = Species::from_toml(toml).expect("valid species TOML");
    let tree = generate_tree(&species, 42).expect("generate tree");
    assert!(tree.bounds.is_valid());
    let lods = generate_lod_meshes_with_config(&tree, &species, &LodGenerationConfig::balanced());
    let maps = TextureSet::generate(&species);
    let mut cfg = ExportConfig::default();
    if let Some(r) = species.canopy.roughness {
        cfg.leaf_roughness = Some(r);
    }
    if let Some(r) = species.inflorescence.as_ref().and_then(|i| i.roughness) {
        cfg.organ_roughness = Some(r);
    }
    let glb = export_lod_meshes_to_bytes(&lods, &cfg).expect("export glb");
    assert_eq!(&glb[0..4], b"glTF");
    (tree, lods, maps, glb)
}

#[test]
fn multistem_and_level4_and_upper_bark_split() {
    let toml = r#"
[species]
name = "Test Half-Barked Mallee"

[trunk]
height = 10.0
radius = 0.42
segments = 10
stems = 3
stem_spread = 22.0
stem_offset = 0.22
stem_height_variation = 0.20

[branches.level1]
count = 5
length = 3.5
radius_ratio = 0.52
angle = 42.0
segments = 5

[branches.level2]
count = 3
length = 1.8
radius_ratio = 0.55
angle = 38.0
segments = 4

[branches.level3]
count = 2
length = 0.9
radius_ratio = 0.55
angle = 35.0
segments = 3

[branches.level4]
count = 2
length = 0.45
radius_ratio = 0.55
angle = 30.0
segments = 2
tip_upturn = 18.0
whorl_spacing_variance = 0.25

[leaves]
count = 400
min_level = 3
size = 0.35
distribution = "whorl"
shape = "broad_eucalypt"

[textures]
resolution = 128
bark_style = "tessellated"
bark_color = [0.24, 0.22, 0.20]
upper_bark_style = "patchy"
upper_bark_color = [0.76, 0.75, 0.72]
upper_bark_patch_colors = [[0.62, 0.64, 0.66], [0.82, 0.79, 0.74]]
bark_transition_height = 2.8
bark_transition_span = 0.8
leaf_color = [0.44, 0.50, 0.38]
leaf_underside_color = [0.60, 0.64, 0.54]
leaf_new_growth_color = [0.58, 0.30, 0.22]
leaf_new_growth_fraction = 0.15
leaf_dead_fraction = 0.08
leaf_dead_color = [0.56, 0.44, 0.24]
"#;
    let (tree, lods, maps, _glb) = build_and_export(toml);
    let trunks = tree.stems_at_level(0).count();
    assert_eq!(trunks, 3, "expected 3 co-dominant trunks");
    assert!(
        tree.stems_at_level(4).count() > 0,
        "expected level-4 fine twigs"
    );
    assert!(maps.upper_bark_albedo.is_some());
    assert!(maps.upper_bark_normal.is_some());

    let lod0 = &lods.meshes[0].mesh;
    let has_bark = lod0
        .submeshes
        .iter()
        .any(|s| s.material == MaterialType::Bark && s.index_count > 0);
    let has_upper_bark = lod0
        .submeshes
        .iter()
        .any(|s| s.material == MaterialType::UpperBark && s.index_count > 0);
    assert!(has_bark, "expected lower Bark submesh");
    assert!(has_upper_bark, "expected UpperBark submesh");
}

#[test]
fn separate_organ_material_and_secondary_inflorescence() {
    let toml = r#"
[species]
name = "Test Dual Organ Tree"

[trunk]
height = 8.0
radius = 0.38
segments = 8

[branches.level1]
count = 5
length = 3.0
radius_ratio = 0.5
angle = 45.0
segments = 5

[branches.level2]
count = 3
length = 1.6
radius_ratio = 0.55
angle = 40.0
segments = 4

[leaves]
count = 500
min_level = 2
size = 0.42
distribution = "whorl"
shape = "bull_banksia"

[textures]
resolution = 128
bark_style = "furrowed"
leaf_shape = "bull_banksia"
leaf_color = [0.42, 0.48, 0.35]

[inflorescence]
kind = "cluster"
count = 80
min_level = 2
placement = "endpoint"
orientation = "up"
length = 0.38
width = 0.22
color = [0.95, 0.93, 0.85]
color_tip = [0.88, 0.82, 0.65]
separate_material = true
roughness = 0.62

[secondary_inflorescence]
kind = "nut"
count = 35
min_level = 2
placement = "along_branch"
orientation = "hang"
length = 0.14
width = 0.11
color = [0.34, 0.28, 0.22]
color_tip = [0.22, 0.18, 0.14]
"#;
    let (tree, lods, maps, _glb) = build_and_export(toml);
    assert!(
        tree.organs.len() > 80,
        "expected both primary and secondary organs, got {}",
        tree.organs.len()
    );
    assert!(maps.organ_albedo.is_some());
    let lod0 = &lods.meshes[0].mesh;
    let has_organ = lod0
        .submeshes
        .iter()
        .any(|s| s.material == MaterialType::Organ && s.index_count > 0);
    assert!(has_organ, "expected dedicated Organ submesh");
}

#[test]
fn pruning_modes_and_hero_trunk_features() {
    for mode in ["lopped", "pollard", "crown_lift"] {
        let toml = format!(
            r#"
[species]
name = "Test Pruned {mode}"

[trunk]
height = 9.0
radius = 0.55
segments = 10
buttress = 2.2
buttress_height = 2.4
surface_root_reach = 4.2
aerial_roots = 8
hollow_height = 1.8
hollow_width = 1.1

[branches.level1]
count = 6
length = 4.2
radius_ratio = 0.55
angle = 55.0
segments = 6

[branches.level2]
count = 3
length = 2.0
radius_ratio = 0.55
angle = 42.0
segments = 4

[leaves]
count = 400
min_level = 2
size = 0.4
shape = "trifoliate"

[textures]
resolution = 128
leaf_shape = "trifoliate"
leaf_color = [0.35, 0.48, 0.28]

[pruning]
mode = "{mode}"
cut_height = 5.5
lift_height = 2.6
regrowth_per_stub = 4
regrowth_length = 1.2
epicormic_count = 6
"#
        );
        let (tree, lods, _maps, _glb) = build_and_export(&toml);
        assert!(tree.stem_count() > 5);
        assert!(lods.meshes[0].stats.triangle_count > 100);
    }
}

#[test]
fn rosette_fan_palm_diamond_boss_runners_and_plume_panicle() {
    let fan_palm = r#"
[species]
name = "Test Cotton Palm"

[generator]
family = "rosette"

[trunk]
height = 10.0
radius = 0.34
taper = 0.25
segments = 12

[textures]
resolution = 128
bark_style = "palm_diamond"
bark_color = [0.46, 0.42, 0.37]

[rosette]
blades = 42
blade_length = 2.1
blade_width = 1.1
blade_card = "fan"
crown_boss = 0.45
skirt = 0.30
skirt_blades = 28
color_base = [0.42, 0.50, 0.34]
color_tip = [0.54, 0.58, 0.38]

[rosette.spike]
style = "panicle"
count = 4
scape_length = 0.6
length = 1.4
radius = 0.22
color = [0.80, 0.64, 0.24]
"#;
    let (tree, lods, _maps, _glb) = build_and_export(fan_palm);
    assert!(tree.leaf_count() >= 42);
    assert!(!tree.organs.is_empty());
    assert!(lods.meshes[0].stats.triangle_count > 200);

    let dune_grass = r#"
[species]
name = "Test Spinifex"

[generator]
family = "rosette"

[trunk]
height = 0.1
radius = 0.08
segments = 2

[textures]
resolution = 128

[rosette]
blades = 64
blade_length = 0.8
blade_width = 0.04
blade_card = "tuft"
runners = 4
runner_length = 1.5
color_base = [0.54, 0.58, 0.52]
color_tip = [0.68, 0.70, 0.62]

[rosette.spike]
style = "plume"
count = 6
scape_length = 0.45
length = 0.32
radius = 0.09
color = [0.76, 0.72, 0.60]
"#;
    let (tree2, _lods2, _maps2, _glb2) = build_and_export(dune_grass);
    assert!(
        tree2.leaf_count() > 64,
        "runners should add secondary tuft blades"
    );
    assert!(
        !tree2.organs.is_empty(),
        "plume spike should produce organs"
    );
}

#[test]
fn paperbark_leaf_shape_renders_soft_non_needle_card() {
    let toml = r#"
[species]
name = "Test Paperbark"

[trunk]
height = 7.0
radius = 0.32
stems = 3
stem_spread = 20.0

[branches.level1]
count = 5
length = 2.6
angle = 45.0

[branches.level2]
count = 3
length = 1.4
angle = 40.0

[leaves]
count = 300
size = 0.45
shape = "paperbark"

[textures]
resolution = 128
bark_style = "papery"
bark_color = [0.84, 0.80, 0.73]
leaf_shape = "paperbark"
leaf_card = "cluster"
leaf_color = [0.46, 0.51, 0.40]
"#;
    let (_tree, _lods, maps, _glb) = build_and_export(toml);
    let opaque = maps
        .leaf_card
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] >= 128)
        .count();
    let total = (maps.leaf_card.width * maps.leaf_card.height) as usize;
    let coverage = opaque as f32 / total as f32;
    assert!(
        coverage > 0.12 && coverage < 0.85,
        "paperbark card coverage {coverage:.3} out of expected range"
    );
}

#[test]
fn wa_nature_patch_exports_ground_albedo_and_validates() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../presets/nature/wa_banksia_woodland_floor.toml");
    let mut patch = NaturePatch::from_file(&path).expect("load WA nature patch");
    patch.groundcover.card_atlas.cell = 32;
    let out_dir = std::env::temp_dir().join("midori_test_wa_ground_albedo");
    let _ = std::fs::remove_dir_all(&out_dir);
    let cfg = NaturePackageConfig {
        map_resolution: 16,
        preview_resolution: 16,
        scatter_chunk_size: 8.0,
        ..NaturePackageConfig::default()
    };
    patch
        .write_package(&out_dir, &cfg)
        .expect("export WA nature patch");
    let report = validate_nature_package(&out_dir).expect("validate WA nature package");
    assert!(
        report.manifest.ground_albedo.is_some(),
        "expected ground_albedo in manifest for soil-tinted WA patch"
    );
    assert!(
        out_dir.join("maps/ground_albedo.png").exists(),
        "expected maps/ground_albedo.png on disk"
    );
    let _ = std::fs::remove_dir_all(&out_dir);
}
