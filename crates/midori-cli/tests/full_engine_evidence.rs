#![recursion_limit = "512"]

mod support;

use midori_cli::evidence::{CheckStatus, ReportStatus, verify_engine_evidence};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use support::full_evidence_fixture::{Mutation, SyntheticFullEvidence};

const REQUIRED_CHECK_FAMILIES: &[&str] = &[
    "midori.",
    "summary.",
    "unreal_dry_run.",
    "unity.",
    "unreal.",
    "profile.notes",
];

#[test]
fn complete_synthetic_fixture_passes_every_required_family() {
    let fixture = SyntheticFullEvidence::create();
    let report = verify_engine_evidence(&fixture.options());
    assert_eq!(report.status, ReportStatus::Passed);
    assert!(
        report
            .checks
            .iter()
            .all(|check| check.status == CheckStatus::Passed)
    );
    for required in REQUIRED_CHECK_FAMILIES {
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.name.starts_with(required)),
            "{required}"
        );
    }
}

#[test]
fn missing_top_level_evidence_is_pending_and_failed_input_dominates() {
    let fixture = SyntheticFullEvidence::create();
    fixture.remove_editor_only_evidence();
    let report = verify_engine_evidence(&fixture.options());
    assert_eq!(report.status, ReportStatus::Pending);
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.name == "unreal_dry_run.report"
                && check.status == CheckStatus::Missing)
    );
    assert!(report.checks.iter().any(
        |check| check.name == "unity.import_screenshot" && check.status == CheckStatus::Missing
    ));
    assert!(
        report
            .checks
            .iter()
            .all(|check| check.status != CheckStatus::Failed)
    );

    let fixture = SyntheticFullEvidence::create();
    fixture.remove_editor_only_evidence();
    fixture.mutate(Mutation::MemoryFootprint);
    let report = verify_engine_evidence(&fixture.options());
    assert_eq!(report.status, ReportStatus::Failed);
    assert_failed(&report, "midori.memory_footprint.total_payload_bytes");
    assert_eq!(
        check_status(&report, "unreal_dry_run.report"),
        CheckStatus::Missing
    );
    assert_eq!(
        check_status(&report, "unity.import_screenshot"),
        CheckStatus::Missing
    );
}

#[test]
fn malformed_present_json_is_a_failed_check_not_a_panic() {
    let fixture = SyntheticFullEvidence::create();
    let root = fixture.validation_root();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("forest_floor_midori_validation_report.json"),
        b"{not-json",
    )
    .unwrap();
    let report = verify_engine_evidence(&fixture.options());
    assert_eq!(report.status, ReportStatus::Failed);
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.name == "midori.report" && check.status == CheckStatus::Failed)
    );
}

#[test]
fn each_complete_fixture_mutation_fails_its_named_check() {
    let cases = [
        (Mutation::MidoriIdentity, "unity.source_file_count"),
        (Mutation::ManifestChecksum, "unity.manifest_file_checksum"),
        (Mutation::PngDimensions, "unity.import_screenshot"),
        (Mutation::PngContent, "unity.import_screenshot"),
        (Mutation::PngFilter, "unity.import_screenshot"),
        (Mutation::PngDecompression, "unity.import_screenshot"),
        (Mutation::PngIncompleteZlib, "unity.import_screenshot"),
        (
            Mutation::PrototypeAttribute,
            "midori.prototype.rock_prototype_lod0.glb.has_normals",
        ),
        (
            Mutation::MapRelationship,
            "midori.map_relationships.normal_green_flip_mismatches",
        ),
        (
            Mutation::ScatterParity,
            "midori.scatter_parity.record_checksum_mismatch_count",
        ),
        (
            Mutation::ScatterRange,
            "midori.scatter_json_summary.0.yaw_range",
        ),
        (
            Mutation::MaterialRecipe,
            "midori.material_recipe.terrain_surface.engine_targets",
        ),
        (
            Mutation::MaterialParameter,
            "midori.material_parameters.terrain_surface.semantics",
        ),
        (
            Mutation::EngineImportRecipe,
            "midori.engine_import_recipe.unity.profile",
        ),
        (
            Mutation::SurfaceOverlay,
            "midori.surface_overlay.moss.channel",
        ),
        (
            Mutation::PrototypeTargets,
            "midori.prototype_surface_targets.rock_prototype",
        ),
        (
            Mutation::ProfileBudget,
            "midori.profile_budget.mobile.lod0_budget",
        ),
        (
            Mutation::MemoryFootprint,
            "midori.memory_footprint.total_payload_bytes",
        ),
        (
            Mutation::Scaffold,
            "summary.project_scaffolds.unreal_plugin.PythonScriptPlugin",
        ),
        (
            Mutation::CompileStub,
            "summary.unity_preflight_compile_stub_report_detail_prototype_failures",
        ),
        (
            Mutation::FakeEditor,
            "summary.unreal_fake_editor_report_foliage_count",
        ),
        (Mutation::UnityField, "unity.mobile_density"),
        (Mutation::UnrealImportTasks, "unreal.import_task_files"),
        (Mutation::UnrealFoliage, "unreal.foliage_type_count"),
        (Mutation::UnrealCull, "unreal.foliage_cull_start_cm"),
        (Mutation::Notes, "profile.notes.length"),
    ];
    for (mutation, expected_name) in cases {
        let fixture = SyntheticFullEvidence::create();
        fixture.mutate(mutation);
        let report = verify_engine_evidence(&fixture.options());
        assert_eq!(report.status, ReportStatus::Failed, "{expected_name}");
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.name == expected_name && check.status == CheckStatus::Failed),
            "{expected_name} did not fail"
        );
    }
}

fn check_status<'a>(report: &'a midori_cli::evidence::EvidenceReport, name: &str) -> CheckStatus {
    report
        .checks
        .iter()
        .find(|check| check.name == name)
        .unwrap_or_else(|| panic!("missing check {name}"))
        .status
}

fn edit_json(path: impl AsRef<Path>, edit: impl FnOnce(&mut Value)) {
    let path = path.as_ref();
    let mut value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    edit(&mut value);
    fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn set_json(path: impl AsRef<Path>, pointer: &str, value: Value) {
    edit_json(path, |document| {
        *document.pointer_mut(pointer).unwrap() = value;
    });
}

fn assert_failed(report: &midori_cli::evidence::EvidenceReport, name: &str) {
    assert_eq!(check_status(report, name), CheckStatus::Failed, "{name}");
}

#[test]
fn present_directories_are_failed_not_pending() {
    let fixture = SyntheticFullEvidence::create();
    let root = fixture.validation_root();
    for relative in [
        "forest_floor_midori_validation_report.json",
        "screenshots/unity_import.png",
        "profile-notes.md",
    ] {
        fs::remove_file(root.join(relative)).unwrap();
        fs::create_dir(root.join(relative)).unwrap();
    }
    let report = verify_engine_evidence(&fixture.options());
    assert_eq!(report.status, ReportStatus::Failed);
    assert_failed(&report, "midori.report");
    assert_failed(&report, "unity.import_screenshot");
    assert_failed(&report, "profile.notes");

    let fixture = SyntheticFullEvidence::create();
    let package_file = fixture
        .validation_root()
        .join("forest_floor/maps/height_u16.png");
    fs::remove_file(&package_file).unwrap();
    fs::create_dir(&package_file).unwrap();
    let report = verify_engine_evidence(&fixture.options());
    assert_failed(&report, "unity.package_identity");
}

#[test]
fn malformed_nested_scalars_and_recipe_collections_fail() {
    let fixture = SyntheticFullEvidence::create();
    set_json(
        fixture
            .validation_root()
            .join("forest_floor_midori_validation_report.json"),
        "/map_summaries/0/channel_min/0",
        json!({}),
    );
    assert_failed(
        &verify_engine_evidence(&fixture.options()),
        "midori.map.height_u16.png.channel_0_varies",
    );

    let fixture = SyntheticFullEvidence::create();
    set_json(
        fixture
            .validation_root()
            .join("forest_floor_midori_validation_report.json"),
        "/map_summaries/1/channel_min/0",
        json!({}),
    );
    assert_failed(
        &verify_engine_evidence(&fixture.options()),
        "midori.map.normal_yplus.png.normal_variation",
    );

    let fixture = SyntheticFullEvidence::create();
    set_json(
        fixture
            .validation_root()
            .join("forest_floor_unity_import_report.json"),
        "/detailPrototypeFallbackErrors",
        json!("error"),
    );
    assert_failed(
        &verify_engine_evidence(&fixture.options()),
        "unity.detail_prototype_fallback_errors",
    );

    let fixture = SyntheticFullEvidence::create();
    set_json(
        fixture
            .validation_root()
            .join("forest_floor_midori_validation_report.json"),
        "/material_recipe_summaries",
        json!({}),
    );
    assert_failed(
        &verify_engine_evidence(&fixture.options()),
        "midori.material_recipe_summaries",
    );

    let fixture = SyntheticFullEvidence::create();
    set_json(
        fixture
            .validation_root()
            .join("forest_floor_midori_validation_report.json"),
        "/engine_import_recipe_summaries",
        json!({}),
    );
    assert_failed(
        &verify_engine_evidence(&fixture.options()),
        "midori.engine_import_recipe_summaries",
    );

    let fixture = SyntheticFullEvidence::create();
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_midori_validation_report.json"),
        |report| {
            report["manifest"].as_object_mut().unwrap().remove("unity");
        },
    );
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_unity_import_report.json"),
        |report| {
            let object = report.as_object_mut().unwrap();
            object.remove("unityHintHeightmap");
            object.remove("unityHintDensityMap");
            object.remove("unityHintNormalMap");
        },
    );
    let report = verify_engine_evidence(&fixture.options());
    assert_failed(&report, "unity.hint_heightmap");
    assert_failed(&report, "unity.hint_density");
    assert_failed(&report, "unity.hint_normal");
}

#[test]
fn missing_numeric_manifest_scalar_cannot_default_to_zero() {
    let fixture = SyntheticFullEvidence::create();
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_midori_validation_report.json"),
        |report| {
            report["manifest"]["mobile"]
                .as_object_mut()
                .unwrap()
                .remove("density_scale");
        },
    );
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_unity_import_report.json"),
        |report| {
            report["mobileDensityScale"] = json!(0.0);
        },
    );
    assert_failed(
        &verify_engine_evidence(&fixture.options()),
        "unity.mobile_density",
    );
}

#[test]
fn exact_mixed_integer_and_float_above_two_to_the_53rd_power_compares_equal() {
    let fixture = SyntheticFullEvidence::create();
    let exact = 18_014_398_509_481_984u64;
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_midori_validation_report.json"),
        |report| {
            report["manifest"]["prototypes"][0]["lods"][0]["vertex_count"] = json!(exact);
        },
    );
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_midori_validation_report.json"),
        |report| {
            report["prototype_summaries"][0]["vertex_count"] = json!(exact as f64);
        },
    );
    let report = verify_engine_evidence(&fixture.options());
    assert_eq!(report.status, ReportStatus::Passed);
    assert_eq!(
        check_status(
            &report,
            "midori.prototype.rock_prototype_lod0.glb.vertex_count"
        ),
        CheckStatus::Passed
    );
}

#[test]
fn nonfinite_numeric_values_fail_closeness() {
    let fixture = SyntheticFullEvidence::create();
    set_json(
        fixture
            .validation_root()
            .join("forest_floor_unity_import_report.json"),
        "/mobileDensityScale",
        json!("inf"),
    );
    assert_failed(
        &verify_engine_evidence(&fixture.options()),
        "unity.mobile_density",
    );
}

#[test]
fn unreal_manifest_float_scalars_cannot_default_to_zero() {
    let cases = [
        (
            "tile_size",
            "tile_size_meters",
            "unreal.tile_size",
            "unreal_dry_run.tile_size",
            true,
        ),
        (
            "density_scale",
            "console_density_scale",
            "unreal.console_density",
            "unreal_dry_run.console_density",
            false,
        ),
        (
            "lod0_max_distance",
            "console_lod0_max_distance_meters",
            "unreal.console_lod0_distance_m",
            "unreal_dry_run.console_lod0_distance_m",
            false,
        ),
        (
            "lod1_max_distance",
            "console_lod1_max_distance_meters",
            "unreal.console_lod1_distance_m",
            "unreal_dry_run.console_lod1_distance_m",
            false,
        ),
        (
            "lod2_max_distance",
            "console_lod2_max_distance_meters",
            "unreal.console_lod2_distance_m",
            "unreal_dry_run.console_lod2_distance_m",
            false,
        ),
        (
            "cull_start",
            "console_cull_start_meters",
            "unreal.console_cull_start_m",
            "unreal_dry_run.console_cull_start_m",
            false,
        ),
        (
            "cull_end",
            "console_cull_end_meters",
            "unreal.console_cull_end_m",
            "unreal_dry_run.console_cull_end_m",
            false,
        ),
    ];
    for (manifest_field, report_field, editor_check, dry_run_check, top_level) in cases {
        for null_value in [false, true] {
            let fixture = SyntheticFullEvidence::create();
            edit_json(
                fixture
                    .validation_root()
                    .join("forest_floor_midori_validation_report.json"),
                |report| {
                    let target = if top_level {
                        report["manifest"].as_object_mut().unwrap()
                    } else {
                        report["manifest"]["console"].as_object_mut().unwrap()
                    };
                    if null_value {
                        target.insert(manifest_field.to_string(), Value::Null);
                    } else {
                        target.remove(manifest_field);
                    }
                },
            );
            for report_file in [
                "forest_floor_unreal_editor_report.json",
                "forest_floor_unreal_dry_run_report.json",
            ] {
                edit_json(fixture.validation_root().join(report_file), |report| {
                    report[report_field] = json!(0.0);
                });
            }
            let report = verify_engine_evidence(&fixture.options());
            assert_failed(&report, editor_check);
            assert_failed(&report, dry_run_check);
        }
    }
}

#[test]
fn unity_and_summary_manifest_tile_size_cannot_default_to_zero() {
    for null_value in [false, true] {
        let fixture = SyntheticFullEvidence::create();
        edit_json(
            fixture
                .validation_root()
                .join("forest_floor_midori_validation_report.json"),
            |report| {
                let manifest = report["manifest"].as_object_mut().unwrap();
                if null_value {
                    manifest.insert("tile_size".to_string(), Value::Null);
                } else {
                    manifest.remove("tile_size");
                }
            },
        );
        edit_json(
            fixture
                .validation_root()
                .join("forest_floor_unity_import_report.json"),
            |report| {
                report["tileSizeMeters"] = json!(0.0);
                report["terrainSize"]["x"] = json!(0.0);
                report["terrainSize"]["z"] = json!(0.0);
            },
        );
        edit_json(
            fixture
                .validation_root()
                .join("forest_floor_unity_compile_stub_report.json"),
            |report| {
                report["terrain_size_x"] = json!(0.0);
                report["terrain_size_z"] = json!(0.0);
            },
        );
        let report = verify_engine_evidence(&fixture.options());
        for name in [
            "unity.tile_size",
            "unity.terrain_size_x",
            "unity.terrain_size_z",
            "summary.unity_preflight_compile_stub_report_terrain_size_x",
            "summary.unity_preflight_compile_stub_report_terrain_size_z",
        ] {
            assert_failed(&report, name);
        }
    }
}

#[test]
fn unity_and_summary_manifest_terrain_height_span_cannot_default_to_zero() {
    // Each case removes exactly one bound and pairs the engine reports with the
    // value the unguarded zero-default arithmetic would have produced, so the
    // case can only fail once the manifest bound itself is required.
    let cases = [
        ("height_min", 0.3901228_f64, 0.3901228_f64),
        ("height_max", 0.01_f64, 0.0_f64),
    ];
    for (manifest_field, unity_terrain_height, stub_terrain_size_y) in cases {
        for null_value in [false, true] {
            let fixture = SyntheticFullEvidence::create();
            edit_json(
                fixture
                    .validation_root()
                    .join("forest_floor_midori_validation_report.json"),
                |report| {
                    let terrain = report["manifest"]["terrain"].as_object_mut().unwrap();
                    if null_value {
                        terrain.insert(manifest_field.to_string(), Value::Null);
                    } else {
                        terrain.remove(manifest_field);
                    }
                },
            );
            edit_json(
                fixture
                    .validation_root()
                    .join("forest_floor_unity_import_report.json"),
                |report| {
                    report["terrainSize"]["y"] = json!(unity_terrain_height);
                },
            );
            edit_json(
                fixture
                    .validation_root()
                    .join("forest_floor_unity_compile_stub_report.json"),
                |report| {
                    report["terrain_size_y"] = json!(stub_terrain_size_y);
                },
            );
            let report = verify_engine_evidence(&fixture.options());
            assert_failed(&report, "unity.terrain_height");
            assert_failed(
                &report,
                "summary.unity_preflight_compile_stub_report_terrain_size_y",
            );
        }
    }
}

#[test]
fn present_zero_manifest_scalars_are_still_accepted() {
    // The manifest guards must reject absent or non-numeric scalars without
    // rejecting a scalar that is legitimately present and exactly zero.
    let fixture = SyntheticFullEvidence::create();
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_midori_validation_report.json"),
        |report| {
            report["manifest"]["console"]["cull_start"] = json!(0.0);
            report["manifest"]["console"]["density_scale"] = json!(0.0);
            report["manifest"]["terrain"]["height_min"] = json!(0.0);
            report["manifest"]["terrain"]["height_max"] = json!(0.0);
        },
    );
    for report_file in [
        "forest_floor_unreal_editor_report.json",
        "forest_floor_unreal_dry_run_report.json",
    ] {
        edit_json(fixture.validation_root().join(report_file), |report| {
            report["console_cull_start_meters"] = json!(0.0);
            report["console_density_scale"] = json!(0.0);
        });
    }
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_unity_import_report.json"),
        |report| {
            // A zero span still floors at the legacy 0.01 minimum terrain height.
            report["terrainSize"]["y"] = json!(0.01);
        },
    );
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_unity_compile_stub_report.json"),
        |report| {
            report["terrain_size_y"] = json!(0.0);
        },
    );
    let report = verify_engine_evidence(&fixture.options());
    for name in [
        "unreal.console_cull_start_m",
        "unreal_dry_run.console_cull_start_m",
        "unreal.console_density",
        "unreal_dry_run.console_density",
        "unity.terrain_height",
        "summary.unity_preflight_compile_stub_report_terrain_size_y",
    ] {
        assert_eq!(check_status(&report, name), CheckStatus::Passed, "{name}");
    }
}

#[test]
fn unreal_manifest_numeric_scalars_reject_wrong_types() {
    let cases = [
        (
            "density_scale",
            "console_density_scale",
            "unreal.console_density",
            "unreal_dry_run.console_density",
        ),
        (
            "material_slots",
            "console_material_slots",
            "unreal.console_material_slots",
            "unreal_dry_run.console_material_slots",
        ),
        (
            "max_instances_per_tile",
            "console_max_instances_per_tile",
            "unreal.console_max_instances_per_tile",
            "unreal_dry_run.console_max_instances_per_tile",
        ),
        (
            "max_instances_per_chunk",
            "console_max_instances_per_chunk",
            "unreal.console_max_instances_per_chunk",
            "unreal_dry_run.console_max_instances_per_chunk",
        ),
    ];
    for (manifest_field, report_field, editor_check, dry_run_check) in cases {
        let fixture = SyntheticFullEvidence::create();
        edit_json(
            fixture
                .validation_root()
                .join("forest_floor_midori_validation_report.json"),
            |report| {
                report["manifest"]["console"][manifest_field] = json!({});
            },
        );
        for report_file in [
            "forest_floor_unreal_editor_report.json",
            "forest_floor_unreal_dry_run_report.json",
        ] {
            edit_json(fixture.validation_root().join(report_file), |report| {
                report[report_field] = json!({});
            });
        }
        let report = verify_engine_evidence(&fixture.options());
        assert_failed(&report, editor_check);
        assert_failed(&report, dry_run_check);
    }
}

#[test]
fn checked_accumulations_reject_overflow() {
    let fixture = SyntheticFullEvidence::create();
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_unity_import_report.json"),
        |report| {
            report["scatterChunkReports"][0]["instanceCount"] = json!(i64::MAX);
            report["scatterChunkReports"][1]["instanceCount"] = json!(i64::MAX);
        },
    );
    assert_failed(
        &verify_engine_evidence(&fixture.options()),
        "unity.scatter_chunk_report_instances",
    );

    let fixture = SyntheticFullEvidence::create();
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_unreal_editor_report.json"),
        |report| {
            report["scatter_chunks"][0]["instance_count"] = json!(i64::MAX);
            report["scatter_chunks"][1]["instance_count"] = json!(i64::MAX);
        },
    );
    assert_failed(
        &verify_engine_evidence(&fixture.options()),
        "unreal.scatter_chunk_report_instances",
    );

    let fixture = SyntheticFullEvidence::create();
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_midori_validation_report.json"),
        |report| {
            report["manifest"]["scatter"]["binary_files"][0]["instance_count"] = json!(i64::MAX);
            report["manifest"]["scatter"]["binary_files"][1]["instance_count"] = json!(i64::MAX);
        },
    );
    assert_failed(
        &verify_engine_evidence(&fixture.options()),
        "midori.memory_footprint.scatter_instance_count",
    );

    let fixture = SyntheticFullEvidence::create();
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_unity_import_report.json"),
        |report| {
            report["detailPrototypesLoadedFromAssets"] = json!(i64::MAX);
            report["detailPrototypesGeneratedFromGlb"] = json!(i64::MAX);
        },
    );
    assert_failed(
        &verify_engine_evidence(&fixture.options()),
        "unity.detail_prototype_source_count",
    );
}

#[test]
fn fixed_lod_declarations_remain_22() {
    let cases = [
        (
            "forest_floor_unity_compile_stub_report.json",
            "/lodFilesDeclared",
            "summary.unity_preflight_compile_stub_report_lod_files_declared",
        ),
        (
            "forest_floor_unity_import_report.json",
            "/lodFilesDeclared",
            "unity.lod_files_declared",
        ),
        (
            "forest_floor_unreal_editor_report.json",
            "/lod_files_declared",
            "unreal.lod_files_declared",
        ),
        (
            "forest_floor_unreal_dry_run_report.json",
            "/lod_files_declared",
            "unreal_dry_run.lod_files_declared",
        ),
    ];
    for (report_file, pointer, check) in cases {
        let fixture = SyntheticFullEvidence::create();
        let root = fixture.validation_root();
        set_json(root.join(report_file), pointer, json!(21));
        edit_json(
            root.join("forest_floor_midori_validation_report.json"),
            |report| {
                report["manifest"]["prototypes"][0]["lods"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            },
        );
        let report = verify_engine_evidence(&fixture.options());
        assert_failed(&report, check);
    }
}

#[test]
fn helper_numeric_and_relative_parity_is_preserved() {
    let fixture = SyntheticFullEvidence::create();
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_midori_validation_report.json"),
        |report| {
            report["manifest"]["mobile"]["density_scale"] = json!(1_000_000_000.0);
        },
    );
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_unity_import_report.json"),
        |report| {
            report["mobileDensityScale"] = json!(1_000_000_000.5);
        },
    );
    let report = verify_engine_evidence(&fixture.options());
    assert_eq!(report.status, ReportStatus::Passed);
    assert_eq!(
        check_status(&report, "unity.mobile_density"),
        CheckStatus::Passed
    );

    let fixture = SyntheticFullEvidence::create();
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_midori_validation_report.json"),
        |report| {
            report["manifest"]["mobile"]["max_instances_per_tile"] =
                json!(9_007_199_254_740_993u64);
        },
    );
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_unity_import_report.json"),
        |report| {
            report["mobileMaxInstancesPerTile"] = json!(9_007_199_254_740_992u64);
        },
    );
    let report = verify_engine_evidence(&fixture.options());
    assert_failed(&report, "unity.mobile_max_instances_per_tile");

    let fixture = SyntheticFullEvidence::create();
    set_json(
        fixture
            .validation_root()
            .join("forest_floor_midori_validation_report.json"),
        "/material_recipe_summaries/0/file_checksum",
        json!("9223372036854775808"),
    );
    let report = verify_engine_evidence(&fixture.options());
    assert_eq!(
        check_status(
            &report,
            "midori.material_recipe_summary.terrain_surface.checksum"
        ),
        CheckStatus::Passed
    );

    let fixture = SyntheticFullEvidence::create();
    edit_json(
        fixture
            .validation_root()
            .join("forest_floor_midori_validation_report.json"),
        |report| {
            report["manifest"]["prototypes"][0]["lods"][0]["index"] = json!(0.5);
        },
    );
    let report = verify_engine_evidence(&fixture.options());
    assert_failed(&report, "unreal.imported_lod0_prototype_files");
}

#[test]
fn unreal_destination_parent_normalizes_dot_components() {
    let fixture = SyntheticFullEvidence::create();
    fixture.mutate_destination_dot();
    let report = verify_engine_evidence(&fixture.options());
    assert_eq!(report.status, ReportStatus::Passed);
    assert_eq!(
        check_status(&report, "unreal.import_task_destinations"),
        CheckStatus::Passed
    );
    assert_eq!(
        check_status(&report, "unreal_dry_run.import_task_destinations"),
        CheckStatus::Passed
    );
}

#[test]
fn ordered_check_names_and_continuation_match_legacy() {
    let fixture = SyntheticFullEvidence::create();
    let report = verify_engine_evidence(&fixture.options());
    let names = report
        .checks
        .iter()
        .map(|check| check.name.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        names
            .iter()
            .filter(|name| *name == "unreal_dry_run.console_cull_start_m")
            .count(),
        1
    );
    let material = names
        .iter()
        .position(|name| name == "unreal_dry_run.console_material_slots")
        .unwrap();
    let cull = names
        .iter()
        .position(|name| name == "unreal_dry_run.console_cull_start_m")
        .unwrap();
    let cull_cm = names
        .iter()
        .position(|name| name == "unreal_dry_run.console_cull_start_cm")
        .unwrap();
    assert!(material < cull && cull < cull_cm);
    assert!(
        names
            .iter()
            .any(|name| name == "summary.unreal_fake_editor_import_screenshot_metric_checksum")
    );
    assert!(names.iter().any(
        |name| name == "summary.unreal_fake_editor_foliage_settings_screenshot_metric_checksum"
    ));

    let fixture = SyntheticFullEvidence::create();
    set_json(
        fixture
            .validation_root()
            .join("forest_floor_unity_compile_stub_report.json"),
        "/package_dir",
        json!(42),
    );
    let report = verify_engine_evidence(&fixture.options());
    assert_failed(
        &report,
        "summary.unity_preflight_compile_stub_report_package_dir",
    );
    assert_eq!(
        check_status(
            &report,
            "summary.unity_preflight_compile_stub_report_scatter_instances"
        ),
        CheckStatus::Passed
    );
}

#[test]
fn checker_families_are_independently_emitted_and_failed_inputs_dominate() {
    let fixture = SyntheticFullEvidence::create();
    let report = verify_engine_evidence(&fixture.options());
    for name in [
        "midori.material_parameters.terrain_surface.semantics",
        "midori.material_recipe_summary_count",
        "midori.engine_import_recipe_summary_count",
        "summary.unity_preflight_compile_stub_report.material_parameter_set_count",
        "summary.unity_preflight_compile_stub_report.material_recipe_count",
        "summary.unity_preflight_compile_stub_report.engine_import_recipe_count",
        "unity.material_parameter_set_count",
        "unity.material_recipe_count",
        "unity.engine_import_recipe_count",
        "unreal.material_parameter_set_count",
        "unreal.material_recipe_count",
        "unreal.engine_import_recipe_count",
    ] {
        assert_eq!(check_status(&report, name), CheckStatus::Passed, "{name}");
    }

    let fixture = SyntheticFullEvidence::create();
    fixture.remove_editor_only_evidence();
    fs::write(
        fixture
            .validation_root()
            .join("engine_validation_summary.json"),
        b"{malformed",
    )
    .unwrap();
    let report = verify_engine_evidence(&fixture.options());
    assert_eq!(report.status, ReportStatus::Failed);
    assert_failed(&report, "summary.report");
    assert_eq!(check_status(&report, "unity.report"), CheckStatus::Missing);
    assert_eq!(
        check_status(&report, "unity.import_screenshot"),
        CheckStatus::Missing
    );
}

#[test]
fn material_and_import_recipe_report_families_have_independent_mutations() {
    for (report_file, pointer, check) in [
        (
            "forest_floor_unity_import_report.json",
            "/materialParameterSemantics/0",
            "unity.material_parameter_semantics",
        ),
        (
            "forest_floor_unity_import_report.json",
            "/materialRecipeCount",
            "unity.material_recipe_count",
        ),
        (
            "forest_floor_unity_import_report.json",
            "/engineImportRecipeCount",
            "unity.engine_import_recipe_count",
        ),
        (
            "forest_floor_unreal_editor_report.json",
            "/material_parameter_semantics/0",
            "unreal.material_parameter_semantics",
        ),
        (
            "forest_floor_unreal_editor_report.json",
            "/material_recipe_count",
            "unreal.material_recipe_count",
        ),
        (
            "forest_floor_unreal_editor_report.json",
            "/engine_import_recipe_count",
            "unreal.engine_import_recipe_count",
        ),
    ] {
        let fixture = SyntheticFullEvidence::create();
        let value = if pointer.ends_with("/0") {
            json!("mutated")
        } else {
            json!(0)
        };
        set_json(fixture.validation_root().join(report_file), pointer, value);
        assert_failed(&verify_engine_evidence(&fixture.options()), check);
    }
}

#[test]
fn complete_fixture_contains_structural_payloads_and_derived_sizes() {
    let fixture = SyntheticFullEvidence::create();
    let package = fixture.validation_root().join("forest_floor");
    let native_report = midori_core::validate_nature_package(&package)
        .expect("synthetic package must parse through Midori's native format validator");
    assert_eq!(native_report.prototype_summaries.len(), 22);
    assert_eq!(native_report.scatter_json_instances, 222);
    assert_eq!(native_report.scatter_binary_instances, 222);
    let manifest: Value =
        serde_json::from_slice(&fs::read(package.join("midori_nature.json")).unwrap()).unwrap();
    let maps = [
        ("height_u16.png", 1usize),
        ("normal_yplus.png", 3),
        ("normal_yminus.png", 3),
        ("masks_rgba.png", 4),
        ("grass_density.png", 1),
    ];
    for (name, channels) in maps {
        let decoder = png::Decoder::new(fs::File::open(package.join("maps").join(name)).unwrap());
        let mut reader = decoder.read_info().unwrap();
        let mut data = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut data).unwrap();
        assert_eq!(info.width, 8, "{name}");
        assert_eq!(info.height, 8, "{name}");
        assert_eq!(info.color_type.samples(), channels, "{name}");
    }
    assert_eq!(manifest["prototypes"].as_array().unwrap().len(), 8);
    for prototype in manifest["prototypes"].as_array().unwrap() {
        for lod in prototype["lods"].as_array().unwrap() {
            let bytes = fs::read(package.join(lod["file"].as_str().unwrap())).unwrap();
            assert_eq!(&bytes[..4], b"glTF");
            assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 2);
            assert_eq!(
                u32::try_from(bytes.len()).unwrap(),
                u32::from_le_bytes(bytes[8..12].try_into().unwrap())
            );
        }
    }
    let scatter: Value =
        serde_json::from_slice(&fs::read(package.join("scatter/scatter.json")).unwrap()).unwrap();
    let sets = scatter.as_array().unwrap();
    assert_eq!(sets.len(), 1);
    let chunks = sets[0]["chunks"].as_array().unwrap();
    assert_eq!(chunks.len(), 26);
    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk["instances"].as_array().unwrap().len())
            .sum::<usize>(),
        222
    );
    for file in manifest["scatter"]["binary_files"].as_array().unwrap() {
        let bytes = fs::read(package.join(file["file"].as_str().unwrap())).unwrap();
        assert_eq!(&bytes[..4], b"MDSI");
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 32);
        let count = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        assert_eq!(bytes.len(), 16 + count * 32);
        assert!(count > 0);
    }
    for file in [
        "materials/terrain_surface.recipe.json",
        "materials/groundcover_foliage.recipe.json",
        "engines/unity_import.recipe.json",
        "engines/unreal_import.recipe.json",
    ] {
        let value: Value = serde_json::from_slice(&fs::read(package.join(file)).unwrap()).unwrap();
        assert!(value.is_object(), "{file}");
    }
    let footprint = &manifest["memory_footprint"];
    let encoded_maps = maps
        .iter()
        .map(|(name, _)| fs::metadata(package.join("maps").join(name)).unwrap().len())
        .sum::<u64>();
    assert_eq!(footprint["encoded_map_bytes"], json!(encoded_maps));
    assert_eq!(
        footprint["scatter_json_bytes"],
        json!(
            fs::metadata(package.join("scatter/scatter.json"))
                .unwrap()
                .len()
        )
    );
    assert_eq!(footprint["scatter_binary_record_bytes"], json!(222u64 * 32));
    assert_eq!(footprint["scatter_binary_header_bytes"], json!(26u64 * 16));
    let scatter_bytes = manifest["scatter"]["binary_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| {
            fs::metadata(package.join(file["file"].as_str().unwrap()))
                .unwrap()
                .len()
        })
        .sum::<u64>();
    assert_eq!(footprint["scatter_binary_bytes"], json!(scatter_bytes));
}
