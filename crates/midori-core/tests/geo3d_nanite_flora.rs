//! Verification tests for 2026/2027 Nanite-era 100% opaque 3D geometric botanical flora (`--foliage geo3d`).

use midori_core::{
    ExportConfig, FoliageRenderMode, LodGenerationConfig, MaterialType, Species,
    export_lod_meshes_to_bytes, generate_lod_meshes_with_foliage_mode, generate_tree,
};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn parse_glb_json(glb: &[u8]) -> serde_json::Value {
    assert!(glb.len() >= 20, "GLB too small");
    assert_eq!(&glb[0..4], b"glTF", "GLB magic mismatch");
    let json_len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
    assert_eq!(&glb[16..20], b"JSON");
    serde_json::from_slice(&glb[20..20 + json_len]).expect("valid glTF JSON chunk")
}

struct Geo3dCase {
    preset: &'static str,
    state: Option<&'static str>,
    expect_roots: bool,
    lod0_min: u32,
    lod0_max: u32,
    lod1_min: u32,
    lod1_max: u32,
}

#[test]
fn geo3d_emits_100_percent_opaque_3d_curved_leaves_root_claws_and_attributes() {
    let root = repo_root();
    let cases = [
        Geo3dCase { preset: "presets/species/perth_jacaranda.toml", state: Some("flowering"), expect_roots: true, lod0_min: 25_000, lod0_max: 55_000, lod1_min: 6_000, lod1_max: 14_000 },
        Geo3dCase { preset: "presets/species/perth_moreton_bay_fig.toml", state: None, expect_roots: true, lod0_min: 25_000, lod0_max: 55_000, lod1_min: 6_000, lod1_max: 14_000 },
        Geo3dCase { preset: "presets/species/perth_norfolk_island_pine.toml", state: None, expect_roots: true, lod0_min: 25_000, lod0_max: 55_000, lod1_min: 6_000, lod1_max: 14_000 },
        Geo3dCase { preset: "presets/species/perth_queensland_box.toml", state: None, expect_roots: true, lod0_min: 25_000, lod0_max: 55_000, lod1_min: 6_000, lod1_max: 14_000 },
        Geo3dCase { preset: "presets/species/wa_flooded_gum.toml", state: None, expect_roots: true, lod0_min: 25_000, lod0_max: 55_000, lod1_min: 6_000, lod1_max: 14_000 },
        Geo3dCase { preset: "presets/species/wa_grass_tree.toml", state: None, expect_roots: false, lod0_min: 8_000, lod0_max: 40_000, lod1_min: 2_500, lod1_max: 14_000 },
        Geo3dCase { preset: "presets/species/wa_jarrah.toml", state: None, expect_roots: true, lod0_min: 25_000, lod0_max: 55_000, lod1_min: 6_000, lod1_max: 14_000 },
        Geo3dCase { preset: "presets/species/wa_marri.toml", state: None, expect_roots: true, lod0_min: 25_000, lod0_max: 55_000, lod1_min: 6_000, lod1_max: 14_000 },
        Geo3dCase { preset: "presets/species/wa_peppermint.toml", state: None, expect_roots: true, lod0_min: 25_000, lod0_max: 55_000, lod1_min: 6_000, lod1_max: 14_000 },
        Geo3dCase { preset: "presets/species/wa_slender_banksia.toml", state: None, expect_roots: true, lod0_min: 25_000, lod0_max: 55_000, lod1_min: 6_000, lod1_max: 14_000 },
        Geo3dCase { preset: "presets/species/wa_swamp_paperbark.toml", state: None, expect_roots: true, lod0_min: 25_000, lod0_max: 55_000, lod1_min: 6_000, lod1_max: 14_000 },
        Geo3dCase { preset: "presets/species/wa_tuart.toml", state: None, expect_roots: true, lod0_min: 25_000, lod0_max: 55_000, lod1_min: 6_000, lod1_max: 14_000 },
        Geo3dCase { preset: "presets/species/wa_kangaroo_paw.toml", state: None, expect_roots: false, lod0_min: 1_500, lod0_max: 14_000, lod1_min: 500, lod1_max: 7_000 },
        Geo3dCase { preset: "presets/species/wa_sword_sedge.toml", state: None, expect_roots: false, lod0_min: 1_500, lod0_max: 14_000, lod1_min: 500, lod1_max: 7_000 },
        Geo3dCase { preset: "presets/species/wa_zamia.toml", state: None, expect_roots: false, lod0_min: 3_000, lod0_max: 22_000, lod1_min: 1_000, lod1_max: 10_000 },
    ];

    let lod_cfg = LodGenerationConfig::nanite_2lod();
    assert_eq!(lod_cfg.levels.len(), 2);

    for c in &cases {
        let (rel, state, expect_roots, lod0_min, lod0_max, lod1_min, lod1_max) = (
            c.preset,
            c.state,
            c.expect_roots,
            c.lod0_min,
            c.lod0_max,
            c.lod1_min,
            c.lod1_max,
        );
        let sp = Species::from_file_with_state(&root.join(rel), state)
            .unwrap_or_else(|e| panic!("failed to load {rel}: {e}"));
        let tree = generate_tree(&sp, 101).unwrap();
        let lods = generate_lod_meshes_with_foliage_mode(
            &tree,
            &sp,
            &lod_cfg,
            FoliageRenderMode::Geo3d,
        );
        assert_eq!(lods.meshes.len(), 2, "{rel}: expected 2 Nanite LODs");

        let lod0 = &lods.meshes[0];
        let lod1 = &lods.meshes[1];
        eprintln!(
            "  {:<42} LOD0={:>6} tris ({:>5} units) | LOD1={:>6} tris ({:>5} units)",
            rel,
            lod0.stats.triangle_count,
            lod0.stats.leaf_count,
            lod1.stats.triangle_count,
            lod1.stats.leaf_count
        );
        assert!(
            (lod0_min..=lod0_max).contains(&lod0.stats.triangle_count),
            "{rel}: LOD0 tris {} outside [{lod0_min}, {lod0_max}]",
            lod0.stats.triangle_count
        );
        assert!(
            (lod1_min..=lod1_max).contains(&lod1.stats.triangle_count),
            "{rel}: LOD1 tris {} outside [{lod1_min}, {lod1_max}]",
            lod1.stats.triangle_count
        );
        assert!(
            lod0.stats.triangle_count > lod1.stats.triangle_count,
            "{rel}: LOD0 ({}) must exceed LOD1 ({})",
            lod0.stats.triangle_count,
            lod1.stats.triangle_count
        );

        // Verify valid vertices, TEXCOORD_1 (crown_ao, height_norm), COLOR_0 (rgb, thickness),
        // and branching 3D surface root claws plunging below y = -0.15 m.
        let m0 = &lod0.mesh;
        let mut min_y = f32::MAX;
        let mut max_root_xz = 0.0_f32;
        let mut min_ao = f32::MAX;
        let mut max_ao = f32::MIN;
        let mut has_leaf_submesh = false;
        let mut has_translucent_thickness = false;

        for sub in &m0.submeshes {
            if matches!(sub.material, MaterialType::Leaves | MaterialType::Organ)
                && sub.index_count > 0
            {
                has_leaf_submesh = true;
            }
        }
        assert!(has_leaf_submesh, "{rel}: missing Leaves/Organ submesh");

        for v in &m0.vertices {
            assert!(v.position.is_finite() && v.normal.is_finite());
            assert!(v.uv.is_finite() && v.uv2.is_finite() && v.color.is_finite());
            assert!(
                (0.0..=1.001).contains(&v.uv2.x) && (0.0..=1.001).contains(&v.uv2.y),
                "{rel}: TEXCOORD_1 out of [0,1]: {:?}",
                v.uv2
            );
            assert!(
                v.color.w >= 0.20 && v.color.w <= 1.001,
                "{rel}: COLOR_0.a (thickness) out of range: {}",
                v.color.w
            );
            if v.color.w < 0.85 {
                has_translucent_thickness = true;
            }
            min_ao = min_ao.min(v.uv2.x);
            max_ao = max_ao.max(v.uv2.x);
            min_y = min_y.min(v.position.y);
            if v.position.y <= 0.25 {
                let r_xz = (v.position.x * v.position.x + v.position.z * v.position.z).sqrt();
                max_root_xz = max_root_xz.max(r_xz);
            }
        }
        assert!(
            has_translucent_thickness,
            "{rel}: expected leaf/petal translucency thickness < 0.85 in COLOR_0.a"
        );
        assert!(
            max_ao - min_ao > 0.15,
            "{rel}: expected genuine crown AO variation in TEXCOORD_1.x (got {min_ao:.3}..{max_ao:.3})"
        );
        if expect_roots {
            assert!(
                min_y <= -0.15,
                "{rel}: expected branching root claws & trunk base plunging <= -0.15 m, got min_y = {min_y:.3}"
            );
            assert!(
                max_root_xz > 0.55,
                "{rel}: expected branching surface root claws reaching > 0.55 m radially, got {max_root_xz:.3}"
            );
        }

        // Export GLB and verify 100% OPAQUE materials (zero MASK / BLEND) and TEXCOORD_1 / COLOR_0 present!
        let cfg = ExportConfig {
            opaque_foliage: true,
            double_sided_opaque_foliage: true,
            ..ExportConfig::default()
        };
        let glb = export_lod_meshes_to_bytes(&lods, &cfg).expect("export geo3d glb");
        let json = parse_glb_json(&glb);
        let materials = json["materials"].as_array().expect("materials array");
        assert!(!materials.is_empty(), "{rel}: no materials exported");
        for mat in materials {
            let alpha_mode = mat
                .get("alphaMode")
                .and_then(|v| v.as_str())
                .unwrap_or("OPAQUE");
            assert_eq!(
                alpha_mode, "OPAQUE",
                "{rel}: material {} has alphaMode {alpha_mode}, expected 100% OPAQUE",
                mat["name"]
            );
        }
        let prims = json["meshes"][0]["primitives"]
            .as_array()
            .expect("LOD0 primitives");
        for prim in prims {
            let attrs = prim["attributes"].as_object().expect("attributes object");
            assert!(attrs.contains_key("TEXCOORD_0"), "{rel}: missing TEXCOORD_0");
            assert!(attrs.contains_key("TEXCOORD_1"), "{rel}: missing TEXCOORD_1");
            assert!(attrs.contains_key("COLOR_0"), "{rel}: missing COLOR_0");
        }
    }
}
