#![recursion_limit = "512"]

mod support;

use midori_cli::evidence::{CheckStatus, ReportStatus, verify_engine_evidence};
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
