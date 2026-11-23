//! Comprehensive integration and acceptance tests for Issue #40:
//! Volumetric Nature Stands (`nature_stand`), seamless world-space tiling,
//! mask/exclusion edge response, budget enforcement, `--foliage opaque`,
//! and all 7 Perth / WA stand presets.

use midori_core::{
    ExportConfig, FoliageRenderMode, LodGenerationConfig, NatureStand, Species, StandBudget,
    StandExportOptions, StandPattern, StandRaster, StandTileInputs, export_lod_meshes,
    generate_lod_meshes_with_foliage_mode, generate_stand_tiles, generate_tree,
    stand::inspect_glb_summary, validate_stand_package,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn collect_all_files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    fn walk(base: &Path, cur: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let mut entries: Vec<_> = std::fs::read_dir(cur)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let p = entry.path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                let rel = p
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read(&p).unwrap());
            }
        }
    }
    walk(root, root, &mut out);
    out
}

const ALL_SEVEN_STANDS: &[&str] = &[
    "presets/stands/wa_banksia_woodland.toml",
    "presets/stands/wa_jarrah_marri_forest.toml",
    "presets/stands/wa_tuart_woodland.toml",
    "presets/stands/wa_paperbark_swamp.toml",
    "presets/stands/wa_flooded_gum_fringe.toml",
    "presets/stands/wa_coastal_heath.toml",
    "presets/stands/perth_pine_plantation.toml",
];

#[test]
fn all_seven_wa_stand_presets_parse_validate_and_export() {
    let root = repo_root();
    for rel in ALL_SEVEN_STANDS {
        let path = root.join(rel);
        let stand =
            NatureStand::from_file(&path).unwrap_or_else(|e| panic!("failed loading {rel}: {e}"));
        assert_eq!(stand.asset.kind, "nature_stand");

        let dir = tempfile::tempdir().unwrap();
        let opts = StandExportOptions {
            tile_size_m: 64.0,
            foliage_mode: FoliageRenderMode::Opaque,
            budget: StandBudget::default(),
            write_library: false,
            raster_resolution: 32,
        };
        let (manifest, library, stats) = generate_stand_tiles(
            &stand,
            path.parent().unwrap(),
            dir.path(),
            &[[0, 0]],
            &StandTileInputs::default(),
            &opts,
        )
        .unwrap_or_else(|e| panic!("failed generating {rel}: {e}"));

        assert!(!library.variants.is_empty(), "{rel}: empty plant library");
        assert_eq!(manifest.tiles.len(), 1);
        assert!(
            stats[0].total_stems > 10,
            "{rel}: expected > 10 stems, got {}",
            stats[0].total_stems
        );
        assert!(
            stats[0].achieved_total_cover > 0.15,
            "{rel}: expected meaningful canopy/shrub cover, got {:.2}",
            stats[0].achieved_total_cover
        );

        let report = validate_stand_package(dir.path())
            .unwrap_or_else(|e| panic!("validate_stand_package failed for {rel}: {e}"));
        assert!(report.ok, "{rel}: validator did not report ok");
        assert!(report.opaque_verified, "{rel}: opaque verification failed");
        assert!(report.max_materials_seen <= 4);
    }
}

/// Issue #40 Acceptance — Seam test:
/// A 4 x 4 block of tiles generated in shuffled order MUST equal the same
/// 4 x 4 block generated in sequential order, byte for byte!
#[test]
fn seam_test_4x4_shuffled_equals_sequential_byte_for_byte() {
    let root = repo_root();
    let preset = root.join("presets/stands/wa_banksia_woodland.toml");
    let stand = NatureStand::from_file(&preset).unwrap();

    let mut ordered = Vec::new();
    for z in -1..=2 {
        for x in -1..=2 {
            ordered.push([x, z]);
        }
    }
    let shuffled = vec![
        [2, 1],
        [-1, 2],
        [0, -1],
        [1, 0],
        [-1, -1],
        [2, 2],
        [1, -1],
        [0, 2],
        [2, -1],
        [-1, 0],
        [0, 1],
        [1, 2],
        [2, 0],
        [-1, 1],
        [1, 1],
        [0, 0],
    ];
    assert_eq!(ordered.len(), 16);
    assert_eq!(shuffled.len(), 16);

    let opts = StandExportOptions {
        tile_size_m: 64.0,
        foliage_mode: FoliageRenderMode::Opaque,
        budget: StandBudget::default(),
        write_library: false,
        raster_resolution: 32,
    };

    let dir_ord = tempfile::tempdir().unwrap();
    let dir_shuf = tempfile::tempdir().unwrap();

    generate_stand_tiles(
        &stand,
        preset.parent().unwrap(),
        dir_ord.path(),
        &ordered,
        &StandTileInputs::default(),
        &opts,
    )
    .unwrap();
    generate_stand_tiles(
        &stand,
        preset.parent().unwrap(),
        dir_shuf.path(),
        &shuffled,
        &StandTileInputs::default(),
        &opts,
    )
    .unwrap();

    let files_ord = collect_all_files(dir_ord.path());
    let files_shuf = collect_all_files(dir_shuf.path());

    assert_eq!(
        files_ord.keys().collect::<Vec<_>>(),
        files_shuf.keys().collect::<Vec<_>>(),
        "file lists must match identically between sequential and shuffled 4x4 tile runs"
    );
    for (rel, bytes_ord) in &files_ord {
        let bytes_shuf = &files_shuf[rel];
        assert_eq!(
            bytes_ord, bytes_shuf,
            "byte mismatch in {rel} between sequential and shuffled 4x4 generation"
        );
    }
}

/// Issue #40 Acceptance — Budget test + Opaque foliage mode:
/// `wa_banksia_woodland` at 128 m tile stays under 300 instances, <= 40k tris LOD0,
/// <= 4k tris LOD1, <= 200 tris LOD2, `--foliage opaque`, <= 4 shared materials,
/// zero alpha-masked/blended materials, and reports achieved cover in stats.
#[test]
fn budget_test_banksia_woodland_128m_opaque_with_full_library() {
    let root = repo_root();
    let preset = root.join("presets/stands/wa_banksia_woodland.toml");
    let stand = NatureStand::from_file(&preset).unwrap();
    let dir = tempfile::tempdir().unwrap();

    let budget = StandBudget {
        max_instances_per_tile: 300,
        max_triangles_lod0: 40_000,
        max_triangles_lod1: 4_000,
        max_triangles_lod2: 200,
        max_materials: 4,
    };
    let opts = StandExportOptions {
        tile_size_m: 128.0,
        foliage_mode: FoliageRenderMode::Opaque,
        budget,
        write_library: true,
        raster_resolution: 64,
    };

    let (manifest, library, stats) = generate_stand_tiles(
        &stand,
        preset.parent().unwrap(),
        dir.path(),
        &[[0, 0]],
        &StandTileInputs::default(),
        &opts,
    )
    .unwrap();

    let st = &stats[0];
    assert!(
        st.instanced_count <= 300,
        "instanced={}",
        st.instanced_count
    );
    assert!(st.merged_count > 0, "expected merged interior stems");
    assert!(st.triangles_lod0 <= 40_000, "lod0={}", st.triangles_lod0);
    assert!(st.triangles_lod1 <= 4_000, "lod1={}", st.triangles_lod1);
    assert!(st.triangles_lod2 <= 200, "lod2={}", st.triangles_lod2);
    assert!(
        st.triangles_lod0 > 1_000,
        "lod0 unexpectedly sparse: {}",
        st.triangles_lod0
    );
    assert!(
        st.triangles_lod1 > 100,
        "lod1 unexpectedly empty: {}",
        st.triangles_lod1
    );
    assert!(
        st.triangles_lod2 > 20,
        "lod2 unexpectedly empty: {}",
        st.triangles_lod2
    );
    assert!(st.material_count <= 4);
    assert!(
        st.achieved_total_cover > 0.30,
        "achieved_cover={}",
        st.achieved_total_cover
    );
    assert!(!library.variants.is_empty());
    assert_eq!(manifest.shared_materials.len(), 3);

    // Full package validation verifies every library GLB + tile GLB is strictly OPAQUE
    let report = validate_stand_package(dir.path()).unwrap();
    assert!(report.ok);
    assert!(report.opaque_verified);
    assert!(report.max_materials_seen <= 4);
    assert_eq!(report.library_variants, library.variants.len());
}

/// Issue #40 Acceptance — Mask test:
/// Straight road + winding track + circular clearing -> zero stems inside exclusions,
/// edge plants flagged `near_edge`, and stand mask trims outer boundary cleanly.
#[test]
fn mask_and_exclusion_test_road_winding_track_and_clearing() {
    let root = repo_root();
    let preset = root.join("presets/stands/wa_banksia_woodland.toml");
    let stand = NatureStand::from_file(&preset).unwrap();

    let res = 128u32;
    let mut mask_data = vec![1.0_f32; (res * res) as usize];
    let mut excl_data = vec![0.0_f32; (res * res) as usize];
    for y in 0..res {
        let v = y as f32 / (res - 1) as f32;
        let track_u = 0.72 + 0.08 * (v * std::f32::consts::TAU * 1.2).sin();
        for x in 0..res {
            let u = x as f32 / (res - 1) as f32;
            let idx = (y * res + x) as usize;
            // Outer stand mask boundary along x < 0.08
            if u < 0.08 {
                mask_data[idx] = 0.0;
            }
            // Straight sealed road at u in [0.22, 0.30]
            let on_road = (u - 0.26).abs() < 0.04;
            // Winding bush track
            let on_track = (u - track_u).abs() < 0.025;
            // Circular picnic clearing
            let dist_clear = ((u - 0.52).powi(2) + (v - 0.52).powi(2)).sqrt();
            let in_clearing = dist_clear < 0.095;
            if on_road || on_track || in_clearing {
                excl_data[idx] = 1.0;
            }
        }
    }

    let exclude_raster = StandRaster {
        width: res,
        height: res,
        data: excl_data,
    };
    let inputs = StandTileInputs {
        mask: Some(StandRaster {
            width: res,
            height: res,
            data: mask_data,
        }),
        exclude: Some(exclude_raster.clone()),
        height: None,
        density_overrides: BTreeMap::new(),
    };

    let dir = tempfile::tempdir().unwrap();
    let opts = StandExportOptions {
        tile_size_m: 128.0,
        foliage_mode: FoliageRenderMode::Opaque,
        budget: StandBudget::default(),
        write_library: false,
        raster_resolution: 64,
    };

    let (manifest, _, stats) = generate_stand_tiles(
        &stand,
        preset.parent().unwrap(),
        dir.path(),
        &[[0, 0]],
        &inputs,
        &opts,
    )
    .unwrap();

    let inst_json =
        std::fs::read_to_string(dir.path().join(&manifest.tiles[0].instances_json)).unwrap();
    let instances: Vec<midori_core::StandInstanceRecord> =
        serde_json::from_str(&inst_json).unwrap();

    assert!(!instances.is_empty());
    let mut near_edge_count = 0usize;
    for inst in &instances {
        let u = (inst.position[0] / 128.0).clamp(0.0, 1.0);
        let v = (inst.position[2] / 128.0).clamp(0.0, 1.0);
        assert!(
            exclude_raster.sample(u, v) <= 0.5,
            "stem placed inside exclusion at ({:.2}, {:.2}) (u={:.3}, v={:.3})",
            inst.position[0],
            inst.position[2],
            u,
            v
        );
        assert!(u >= 0.075, "stem placed outside stand mask at u={:.3}", u);
        if inst.near_edge {
            near_edge_count += 1;
        }
    }
    assert!(
        near_edge_count > 10,
        "expected edge-response plants along road/track/clearing, got {near_edge_count}"
    );
    assert!(stats[0].triangles_lod1 > 0);
    assert!(validate_stand_package(dir.path()).unwrap().ok);
}

#[test]
fn coastal_heath_has_no_canopy_and_pine_plantation_is_regular_grid() {
    let root = repo_root();

    let heath = NatureStand::from_file(&root.join("presets/stands/wa_coastal_heath.toml")).unwrap();
    assert!(
        !heath.strata.iter().any(|s| s.name == "canopy"),
        "wa_coastal_heath must have no canopy stratum"
    );

    let pine_path = root.join("presets/stands/perth_pine_plantation.toml");
    let pine = NatureStand::from_file(&pine_path).unwrap();
    let canopy = pine.strata.iter().find(|s| s.name == "canopy").unwrap();
    assert_eq!(canopy.pattern, StandPattern::Regular);

    let dir = tempfile::tempdir().unwrap();
    let opts = StandExportOptions {
        tile_size_m: 64.0,
        foliage_mode: FoliageRenderMode::Opaque,
        budget: StandBudget::default(),
        write_library: false,
        raster_resolution: 32,
    };
    let (manifest, _, stats) = generate_stand_tiles(
        &pine,
        pine_path.parent().unwrap(),
        dir.path(),
        &[[0, 0]],
        &StandTileInputs::default(),
        &opts,
    )
    .unwrap();
    let canopy_st = stats[0].strata.iter().find(|s| s.name == "canopy").unwrap();
    let edge_st = stats[0]
        .strata
        .iter()
        .find(|s| s.name == "edge_scrub")
        .unwrap();
    assert!(canopy_st.stems > 100, "expected dense plantation rows");
    // Under heavy closed pine canopy, `avoid` scrub should be strongly suppressed
    assert!(
        edge_st.stems < canopy_st.stems / 2,
        "expected understory suppression under closed pine canopy (canopy={}, edge_scrub={})",
        canopy_st.stems,
        edge_st.stems
    );
    assert_eq!(manifest.tiles.len(), 1);
}

#[test]
fn condition_modulation_fire_and_dieback_change_library_and_stems() {
    let root = repo_root();
    let preset = root.join("presets/stands/wa_banksia_woodland.toml");
    let mut burnt = NatureStand::from_file(&preset).unwrap();
    burnt.stand.condition.years_since_fire = 1.5;
    burnt.stand.condition.dieback = 0.40;

    let lib = midori_core::build_stand_library(
        &burnt,
        preset.parent().unwrap(),
        None,
        FoliageRenderMode::Opaque,
        false,
    )
    .unwrap();

    assert!(
        lib.variants.iter().any(|v| v.is_dead),
        "dieback=0.40 should produce dead stag variants for susceptible banksia/jarrah"
    );
    assert!(
        lib.variants
            .iter()
            .any(|v| v.preset.contains("grass_tree") && v.state.as_deref() == Some("burnt")),
        "years_since_fire=1.5 should activate burnt state on grass trees"
    );
}

#[test]
fn single_tree_opaque_foliage_mode_has_zero_alpha_materials() {
    let root = repo_root();
    let dir = tempfile::tempdir().unwrap();
    for sp_rel in [
        "presets/species/wa_jarrah.toml",
        "presets/species/wa_firewood_banksia.toml",
        "presets/species/wa_grass_tree.toml",
    ] {
        let sp = Species::from_file(&root.join(sp_rel)).unwrap();
        let tree = generate_tree(&sp, 42).unwrap();
        let lods = generate_lod_meshes_with_foliage_mode(
            &tree,
            &sp,
            &LodGenerationConfig::balanced(),
            FoliageRenderMode::Opaque,
        );
        let out_glb = dir
            .path()
            .join(format!("{}.glb", sp.species.name.replace(' ', "_")));
        let cfg = ExportConfig {
            opaque_foliage: true,
            ..ExportConfig::default()
        };
        export_lod_meshes(&lods, &out_glb, &cfg).unwrap();
        let (tris, mat_count, has_alpha) = inspect_glb_summary(&out_glb).unwrap();
        assert!(
            tris > 100,
            "{sp_rel}: expected > 100 tris in opaque mode, got {tris}"
        );
        assert!(mat_count <= 4, "{sp_rel}: material count {mat_count} > 4");
        assert!(
            !has_alpha,
            "{sp_rel}: opaque foliage GLB must have zero MASK/BLEND materials"
        );
    }
}

#[test]
fn validate_stand_rejects_path_traversal() {
    let dir = tempfile::tempdir().unwrap();
    let bad_manifest = serde_json::json!({
        "schema_version": 1,
        "kind": "nature_stand",
        "name": "Evil Stand",
        "biome": "test",
        "seed": 1,
        "tile_size_m": 64.0,
        "foliage_mode": "opaque",
        "floor_preset": "presets/nature/wa_banksia_woodland_floor.toml",
        "cross_lod_stitch_rule": "test",
        "shared_materials": [],
        "library": [{
            "variant_id": "bad",
            "variant_index": 0,
            "stratum": "canopy",
            "preset": "x",
            "size_class": "mature",
            "is_dead": false,
            "seed": 1,
            "glb_file": "../escape.glb",
            "height_m": 5.0,
            "crown_radius_m": 2.0,
            "crown_base_y_m": 1.0,
            "crown_top_y_m": 5.0,
            "trunk_radius_m": 0.2,
            "foliage_top_srgb": [0.3, 0.4, 0.2],
            "foliage_bot_srgb": [0.2, 0.3, 0.15],
            "bark_srgb": [0.3, 0.25, 0.2],
            "is_rosette": false,
            "lod0_triangles": 100
        }],
        "tiles": []
    });
    std::fs::write(
        dir.path().join("midori_stand.json"),
        serde_json::to_string_pretty(&bad_manifest).unwrap(),
    )
    .unwrap();
    let err = validate_stand_package(dir.path()).unwrap_err();
    assert!(
        err.to_string().contains("escapes"),
        "expected path-traversal error, got: {err}"
    );
}
