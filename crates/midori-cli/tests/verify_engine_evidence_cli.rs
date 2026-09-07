#![recursion_limit = "512"]
//! Actual-binary contract for `midori verify-engine-evidence`.
//!
//! Every fixture below is synthetic evidence written into a `TempDir` owned by
//! the test. Passing here proves the verifier and its CLI adapter agree on the
//! recorded-evidence contract; it is NOT real-engine acceptance and it does not
//! certify that a Unity or Unreal editor was ever launched.

mod support;

use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};
use support::full_evidence_fixture::{Mutation, SyntheticFullEvidence};

fn run(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_midori"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap()
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Arguments that steer the verifier at the fixture's own synthetic tree.
fn fixture_args(fixture: &SyntheticFullEvidence) -> Vec<String> {
    let root = fixture.validation_root();
    vec![
        "verify-engine-evidence".to_string(),
        "--validation-root".to_string(),
        text(&root),
        "--unity-import-screenshot".to_string(),
        text(&root.join("screenshots/unity_import.png")),
        "--unity-density-screenshot".to_string(),
        text(&root.join("screenshots/unity_density.png")),
        "--unreal-import-screenshot".to_string(),
        text(&root.join("screenshots/unreal_import.png")),
        "--unreal-foliage-settings-screenshot".to_string(),
        text(&root.join("screenshots/unreal_foliage.png")),
        "--profile-notes".to_string(),
        text(&root.join("profile-notes.md")),
    ]
}

fn borrow(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

#[test]
fn actual_binary_passes_complete_synthetic_evidence_and_writes_identical_output() {
    let fixture = SyntheticFullEvidence::create();
    let cwd = tempfile::tempdir().unwrap();
    let mut args = fixture_args(&fixture);
    args.push("--output".to_string());
    args.push("reports/engine-evidence.json".to_string());
    let output = run(cwd.path(), &borrow(&args));
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "passed");
    assert!(json["checks"].as_array().unwrap().len() > 100);
    assert!(output.stdout.ends_with(b"\n"));
    assert_eq!(
        std::fs::read(cwd.path().join("reports/engine-evidence.json")).unwrap(),
        output.stdout
    );
}

#[test]
fn every_report_path_override_is_honoured() {
    let fixture = SyntheticFullEvidence::create();
    let cwd = tempfile::tempdir().unwrap();
    let diverted = fixture.divert_reports();

    // Without the overrides the default names no longer exist, so the run is
    // pending rather than passed.
    let base = fixture_args(&fixture);
    let pending = run(cwd.path(), &borrow(&base));
    assert_eq!(pending.status.code(), Some(1));
    let json: Value = serde_json::from_slice(&pending.stdout).unwrap();
    assert_eq!(json["status"], "pending");

    let mut args = base;
    for (flag, path) in &diverted {
        args.push((*flag).to_string());
        args.push(text(path));
    }
    let passed = run(cwd.path(), &borrow(&args));
    assert_eq!(passed.status.code(), Some(0));
    let json: Value = serde_json::from_slice(&passed.stdout).unwrap();
    assert_eq!(json["status"], "passed");
}

#[test]
fn strict_and_allow_pending_exit_matrix() {
    for (missing, mutate, expected_status, strict, relaxed) in [
        (false, false, "passed", 0, 0),
        (true, false, "pending", 1, 0),
        (false, true, "failed", 1, 1),
        (true, true, "failed", 1, 1),
    ] {
        for allow in [false, true] {
            let fixture = SyntheticFullEvidence::create();
            if missing {
                fixture.remove_editor_only_evidence();
            }
            if mutate {
                fixture.mutate(Mutation::MemoryFootprint);
            }
            let cwd = tempfile::tempdir().unwrap();
            let mut args = fixture_args(&fixture);
            if allow {
                args.push("--allow-pending".to_string());
            }
            let output = run(cwd.path(), &borrow(&args));
            assert_eq!(
                output.status.code(),
                Some(if allow { relaxed } else { strict }),
                "{expected_status} allow_pending={allow}"
            );
            let json: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(json["status"], expected_status);
        }
    }
}

#[test]
fn defaults_are_cwd_relative() {
    let cwd = tempfile::tempdir().unwrap();
    let empty = run(cwd.path(), &["verify-engine-evidence"]);
    assert_eq!(empty.status.code(), Some(1));
    let json: Value = serde_json::from_slice(&empty.stdout).unwrap();
    assert_eq!(json["status"], "pending");
    let details = json["checks"]
        .as_array()
        .unwrap()
        .iter()
        // Path details are Debug-formatted, so a Windows separator arrives as an
        // escaped backslash pair; normalize both spellings to a forward slash.
        .map(|check| {
            check["detail"]
                .as_str()
                .unwrap()
                .replace("\\\\", "/")
                .replace('\\', "/")
        })
        .collect::<Vec<_>>()
        .join("\n");
    for expected in [
        "target/midori_engine_validation/forest_floor_midori_validation_report.json",
        "target/midori_engine_validation/engine_validation_summary.json",
        "docs/validation/screenshots/unity_forest_floor_import.png",
        "docs/validation/screenshots/unity_forest_floor_density.png",
        "docs/validation/screenshots/unreal_forest_floor_import.png",
        "docs/validation/screenshots/unreal_forest_floor_foliage_settings.png",
        "docs/validation/midori-nature-engine-profile-notes.md",
    ] {
        assert!(details.contains(expected), "{expected}");
    }

    let fixture = SyntheticFullEvidence::create();
    fixture.install_default_layout(cwd.path());
    let installed = run(cwd.path(), &["verify-engine-evidence"]);
    let json: Value = serde_json::from_slice(&installed.stdout).unwrap();
    assert_eq!(json["status"], "passed");
    assert_eq!(installed.status.code(), Some(0));
}

#[test]
fn blocked_output_path_is_an_error_that_allow_pending_never_waives() {
    let fixture = SyntheticFullEvidence::create();
    let cwd = tempfile::tempdir().unwrap();
    std::fs::write(cwd.path().join("blocked-parent"), "file").unwrap();
    let mut args = fixture_args(&fixture);
    args.push("--output".to_string());
    args.push("blocked-parent/report.json".to_string());
    args.push("--allow-pending".to_string());
    let output = run(cwd.path(), &borrow(&args));
    assert_eq!(output.status.code(), Some(1));
    assert!(!output.stderr.is_empty());
}

#[test]
fn unknown_option_is_an_argument_error_and_help_states_scope() {
    let cwd = tempfile::tempdir().unwrap();
    assert_eq!(
        run(cwd.path(), &["verify-engine-evidence", "--launch-unity"])
            .status
            .code(),
        Some(2)
    );
    let help = run(cwd.path(), &["verify-engine-evidence", "--help"]);
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("does not launch or prove real engines"));
    assert!(help.contains("never failed"));
}
