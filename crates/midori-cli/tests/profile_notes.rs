use midori_cli::evidence::{
    Check, CheckStatus, EvidenceReport, ReportStatus, verify_profile_notes_text,
};

const GOOD: &str = include_str!("fixtures/profile-notes-complete.md");

fn check(name: &str, status: CheckStatus) -> Check {
    Check {
        name: name.into(),
        status,
        detail: "test".into(),
    }
}

#[test]
fn report_precedence_and_exit_policy() {
    for (checks, status, strict, relaxed) in [
        (
            vec![check("a", CheckStatus::Passed)],
            ReportStatus::Passed,
            0,
            0,
        ),
        (
            vec![check("a", CheckStatus::Missing)],
            ReportStatus::Pending,
            1,
            0,
        ),
        (
            vec![check("a", CheckStatus::Failed)],
            ReportStatus::Failed,
            1,
            1,
        ),
        (
            vec![
                check("a", CheckStatus::Missing),
                check("b", CheckStatus::Failed),
            ],
            ReportStatus::Failed,
            1,
            1,
        ),
    ] {
        let report = EvidenceReport::from_checks(checks);
        assert_eq!(report.status, status);
        assert_eq!(report.exit_code(false), strict);
        assert_eq!(report.exit_code(true), relaxed);
    }
}

#[test]
fn complete_notes_keep_exact_order_and_json_shape() {
    let report = verify_profile_notes_text("notes.md", GOOD);
    assert_eq!(report.status, ReportStatus::Passed);
    assert_eq!(
        report
            .checks
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        [
            "profile.notes.file",
            "profile.notes.length",
            "profile.notes.unresolved_markers",
            "profile.notes.sections",
            "profile.notes.required_terms",
            "profile.notes",
        ]
    );
    let value = serde_json::to_value(&report).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 2);
    assert_eq!(
        value["checks"][0],
        serde_json::json!({
            "name": "profile.notes.file", "status": "passed", "detail": "notes.md loaded"
        })
    );
    assert_eq!(
        value["checks"][5]["detail"],
        "notes.md contains Unity/Unreal profiling evidence"
    );
}

#[test]
fn thin_notes_report_every_failure_without_final_success() {
    let report = verify_profile_notes_text(
        "thin.md",
        "Unity Unreal Frame Debugger RenderDoc instanced scale density cull\n",
    );
    assert_eq!(report.status, ReportStatus::Failed);
    assert_eq!(report.checks.len(), 5);
    assert_eq!(report.checks[1].status, CheckStatus::Failed);
    assert_eq!(report.checks[3].status, CheckStatus::Failed);
    assert_eq!(report.checks[4].status, CheckStatus::Failed);
}

#[test]
fn markers_preserve_label_order_case_and_python_boundaries() {
    let all = format!(
        "{GOOD}\n[fill value] missing notes not captured NOT\u{1c}RUN pending placeholder TBD TODO"
    );
    let report = verify_profile_notes_text("m.md", &all);
    assert_eq!(
        report.checks[2].detail,
        "m.md contains unresolved markers: TODO, TBD, placeholder, pending, not run, not captured, missing evidence, fill marker"
    );
    for marker in [
        "TODO",
        "tBd",
        "placeholder",
        "PENDİNG",
        "pendıng",
        "not\nrun",
        "not captured",
        "miſſing artifacts",
        "[REPLACE value]",
        "TODO\u{301}",
    ] {
        let r = verify_profile_notes_text("m.md", &format!("{GOOD}\n{marker}"));
        assert_eq!(r.checks[2].status, CheckStatus::Failed, "{marker}");
    }
    for ordinary in [
        "TODO_item",
        "pendingly",
        "not runner",
        "[fill\nvalue]",
        "xTODO",
    ] {
        let r = verify_profile_notes_text("m.md", &format!("{GOOD}\n{ordinary}"));
        assert_eq!(r.checks[2].status, CheckStatus::Passed, "{ordinary}");
    }
}

#[test]
fn length_counts_characters_and_trims_python_whitespace() {
    for (size, status) in [(999, CheckStatus::Failed), (1000, CheckStatus::Passed)] {
        let text = format!("\u{1c} {} \u{1f}", "界".repeat(size));
        let r = verify_profile_notes_text("n.md", &text);
        assert_eq!(r.checks[1].status, status);
    }
}

#[test]
fn bom_case_and_universal_newlines_preserve_policy() {
    let baseline = verify_profile_notes_text("n.md", GOOD);
    for text in [
        format!("\u{feff}{}", GOOD.replace('\n', "\r\n")),
        GOOD.replace('\n', "\r"),
        GOOD.to_uppercase(),
    ] {
        assert_eq!(verify_profile_notes_text("n.md", &text), baseline);
    }
}

#[test]
fn missing_section_lists_are_ordered() {
    let r = verify_profile_notes_text("n.md", &"x".repeat(1000));
    assert_eq!(
        r.checks[3].detail,
        "n.md is missing sections: ## Evidence Artifacts, ## Unity, ## Unreal, ## Verdict"
    );
    assert_eq!(
        r.checks[4].detail,
        "n.md is missing required terms: Unity, Unreal, Unity Editor version, Unreal Editor version, mobile, console, LOD, Frame Debugger, RenderDoc, instanced, scale, density, cull, material slot, wind, strict verifier, forest_floor_unity_import_report.json, forest_floor_unreal_editor_report.json, unity_forest_floor_import.png, unity_forest_floor_density.png, unreal_forest_floor_import.png, unreal_forest_floor_foliage_settings.png, detailPrototypesCreated, detailPrototypesGeneratedFromGlb, detailPrototypeFailures, detailPrototypeFallbackErrors, scatterBinaryRecordsRead, scatterChunkReports, foliage_type_count, foliage_type_assets, 3500, 7000"
    );
}

#[test]
fn removing_each_requirement_fails_the_corresponding_gate() {
    let terms = [
        "Unity",
        "Unreal",
        "Unity Editor version",
        "Unreal Editor version",
        "mobile",
        "console",
        "LOD",
        "Frame Debugger",
        "RenderDoc",
        "instanced",
        "scale",
        "density",
        "cull",
        "material slot",
        "wind",
        "strict verifier",
        "forest_floor_unity_import_report.json",
        "forest_floor_unreal_editor_report.json",
        "unity_forest_floor_import.png",
        "unity_forest_floor_density.png",
        "unreal_forest_floor_import.png",
        "unreal_forest_floor_foliage_settings.png",
        "detailPrototypesCreated",
        "detailPrototypesGeneratedFromGlb",
        "detailPrototypeFailures",
        "detailPrototypeFallbackErrors",
        "scatterBinaryRecordsRead",
        "scatterChunkReports",
        "foliage_type_count",
        "foliage_type_assets",
        "3500",
        "7000",
    ];
    for term in terms {
        let changed = GOOD.to_lowercase().replace(&term.to_lowercase(), "removed");
        let r = verify_profile_notes_text("n.md", &changed);
        assert_eq!(r.checks[4].status, CheckStatus::Failed, "{term}");
    }
    for section in [
        "## Evidence Artifacts",
        "## Unity",
        "## Unreal",
        "## Verdict",
    ] {
        let r = verify_profile_notes_text("n.md", &GOOD.replace(section, "## Removed"));
        assert_eq!(r.checks[3].status, CheckStatus::Failed, "{section}");
    }
}

#[test]
fn profile_self_test_four_legacy_cases_and_file_errors() {
    use midori_cli::evidence::verify_profile_notes;
    let dir = tempfile::tempdir().unwrap();
    let absent = dir.path().join("absent.md");
    let r = verify_profile_notes(&absent);
    assert_eq!(r.status, ReportStatus::Pending);
    assert_eq!(r.checks.len(), 1);
    assert_eq!(r.checks[0].name, "profile.notes");
    assert_eq!(
        r.checks[0].detail,
        format!("{} is missing or empty", absent.display())
    );

    let copied = dir.path().join("copied-template.md");
    std::fs::write(
        &copied,
        include_str!("../../../docs/validation/midori-nature-engine-profile-notes.template.md"),
    )
    .unwrap();
    let r = verify_profile_notes(&copied);
    assert_eq!(r.status, ReportStatus::Failed);
    assert_eq!(r.checks[2].name, "profile.notes.unresolved_markers");
    assert_eq!(r.checks[2].status, CheckStatus::Failed);

    let thin = dir.path().join("thin.md");
    std::fs::write(
        &thin,
        "Unity Unreal Frame Debugger RenderDoc instanced scale density cull\n",
    )
    .unwrap();
    assert_eq!(verify_profile_notes(&thin).status, ReportStatus::Failed);
    let good = dir.path().join("good.md");
    std::fs::write(&good, format!("\u{feff}{}", GOOD.replace('\n', "\r\n"))).unwrap();
    assert_eq!(verify_profile_notes(&good).status, ReportStatus::Passed);

    let empty = dir.path().join("empty.md");
    std::fs::write(&empty, []).unwrap();
    assert_eq!(verify_profile_notes(&empty).status, ReportStatus::Pending);
    assert_eq!(
        verify_profile_notes(dir.path()).status,
        ReportStatus::Pending
    );
    let invalid = dir.path().join("invalid.md");
    std::fs::write(&invalid, [0xff]).unwrap();
    let r = verify_profile_notes(&invalid);
    assert_eq!(r.status, ReportStatus::Failed);
    assert_eq!(r.checks[0].name, "profile.notes");
    assert_eq!(r.exit_code(true), 1);
}
