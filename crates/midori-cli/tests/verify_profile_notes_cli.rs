use std::path::Path;
use std::process::{Command, Output};

const GOOD: &str = include_str!("fixtures/profile-notes-complete.md");

fn run(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_midori"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn actual_binary_status_exit_matrix_and_output() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("good.md"), GOOD).unwrap();
    std::fs::write(dir.path().join("bad.md"), "TODO").unwrap();
    for (file, expected_status, strict, relaxed) in [
        ("good.md", "passed", 0, 0),
        ("absent.md", "pending", 1, 0),
        ("bad.md", "failed", 1, 1),
    ] {
        for allow in [false, true] {
            let mut args = vec![
                "verify-profile-notes",
                "--profile-notes",
                file,
                "--output",
                "reports/profile-only.json",
            ];
            if allow {
                args.push("--allow-pending");
            }
            let output = run(dir.path(), &args);
            assert_eq!(
                output.status.code(),
                Some(if allow { relaxed } else { strict })
            );
            let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(json["status"], expected_status);
            assert_eq!(
                std::fs::read(dir.path().join("reports/profile-only.json")).unwrap(),
                output.stdout
            );
            assert!(output.stdout.ends_with(b"\n"));
            assert!(output.stderr.is_empty());
        }
    }
}

#[test]
fn defaults_are_cwd_relative_and_output_failures_are_not_waived() {
    let dir = tempfile::tempdir().unwrap();
    let missing = run(dir.path(), &["verify-profile-notes"]);
    assert_eq!(missing.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&missing.stdout).unwrap();
    assert_eq!(json["status"], "pending");
    assert!(
        json["checks"][0]["detail"]
            .as_str()
            .unwrap()
            .contains("midori-nature-engine-profile-notes.md")
    );
    let path = dir.path().join("docs/validation");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("midori-nature-engine-profile-notes.md"), GOOD).unwrap();
    assert_eq!(
        run(dir.path(), &["verify-profile-notes"]).status.code(),
        Some(0)
    );
    std::fs::write(dir.path().join("blocked-parent"), "file").unwrap();
    let failed = run(
        dir.path(),
        &[
            "verify-profile-notes",
            "--output",
            "blocked-parent/report.json",
            "--allow-pending",
        ],
    );
    assert_eq!(failed.status.code(), Some(1));
    assert!(!failed.stderr.is_empty());
}

#[test]
fn unknown_flags_are_argument_errors_and_help_states_scope() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        run(
            dir.path(),
            &["verify-profile-notes", "--validation-root", "x"],
        )
        .status
        .code(),
        Some(2)
    );
    let help = run(dir.path(), &["verify-profile-notes", "--help"]);
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout).unwrap();
    assert!(text.contains("not full engine evidence"));
    assert!(text.contains("never failed"));
}
