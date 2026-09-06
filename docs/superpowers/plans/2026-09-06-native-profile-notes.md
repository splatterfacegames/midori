# Native Profile Notes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a complete Python-free `midori verify-profile-notes` command and native profile self-tests, without claiming a full engine-verifier port.

**Architecture:** Add a process-independent evidence library to the existing `midori-cli` crate and call it from a thin clap adapter. Preserve ordered legacy profile findings and JSON shape, deliberately fixing the legacy `--allow-pending` success-on-failure bug. Leave every full-engine verifier and wrapper unchanged.

**Tech Stack:** Existing Rust 2024 workspace and clap 4.5; serde 1.0 derive, serde_json 1.0, regex 1.x; tempfile 3.10 for native tests.

**Spec:** `docs/superpowers/specs/2026-09-06-native-profile-notes-design.md`; read the approved parent spec linked there as well.

## Global Constraints

- Application business logic, persistence, local services, CLI entrypoints and desktop command handlers run in Rust; presentation runs in React/TypeScript.
- Historical Python implementations remain migration references until parity is verified. Retire them from active app/build/test entrypoints at cutover, keeping recoverable Git history. Do not delete user data, environments or model caches.
- Production acceptance retains the full audited capabilities and unfinished release blockers. No app is declared complete based on a shell, disabled control, untextured preview, unsupported-format stub or artificially reduced scope.
- Keep build caches on C: while B: is low on space; do not spend on GPU workers or copy large models merely for architecture work.
- This plan is only the profile-notes gate. Do not alter Python scripts, wrappers, engine integration, historical evidence, profile template or actual notes.
- `failed` plus `--allow-pending` exits 1. The old Python behavior is a bug, not a compatibility requirement.
- Work only in `B:/lab/worktrees/midori-python-free`; preserve baseline `a24e4fe93b865330cff724ded9466108690b22b8` and its eight local commits. Parent reviews before Luna execution; independent Astra review follows implementation.
- No paid services, engine launches, new subagents or commits in the planning pass. During execution commit only at the parent's integration direction; the normal per-task commit step is replaced with a review checkpoint below.

---

## Working setup and file map

Execution shell is PowerShell. Before any Cargo command set:

```powershell
$env:CARGO_TARGET_DIR = 'C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target'
git status --short
git rev-parse HEAD
```

Use `apply_patch` for edits. Existing CLI commands must retain behavior. These
files are the only implementation touch set:

```text
crates/midori-cli/Cargo.toml                         serde/regex/test dependencies
Cargo.lock                                          resolved dependency identity
crates/midori-cli/src/lib.rs                         pub mod evidence
crates/midori-cli/src/evidence/mod.rs                 public exports
crates/midori-cli/src/evidence/report.rs              JSON/status/exit policy
crates/midori-cli/src/evidence/profile_notes.rs       exact complete text/file gate
crates/midori-cli/src/main.rs                         new clap command and writer
crates/midori-cli/tests/profile_notes.rs              Rust self-tests
crates/midori-cli/tests/verify_profile_notes_cli.rs    real binary regression tests
crates/midori-cli/tests/fixtures/profile-notes-complete.md  synthetic test text
README.md                                           scope and usage
```

No new workspace crate, build script, Tauri dependency, generated domain schema,
FlatBuffers change, Python subprocess or runtime self-test command is needed.
`cargo test -p midori-cli --test profile_notes` is the native self-test interface.

### Task 1: Complete report and profile-text policy with native regression tests

**Files:** Create `src/lib.rs`, `src/evidence/{mod,report,profile_notes}.rs`,
`tests/profile_notes.rs`, `tests/fixtures/profile-notes-complete.md` under
`crates/midori-cli/`; modify its `Cargo.toml` and workspace `Cargo.lock`.

**Interfaces:**
- Consumes: exact profile constants and detail strings from Python source lines 34–93 and 3639–3685, quoted below.
- Produces: `CheckStatus`, `ReportStatus`, `Check`, `EvidenceReport::from_checks(Vec<Check>) -> EvidenceReport`, `EvidenceReport::exit_code(bool) -> i32`, `verify_profile_notes_text(&str, &str) -> EvidenceReport` exported by `midori_cli::evidence`.

- [ ] **Step 1: Add the literal positive fixture and failing tests.**

Create `tests/fixtures/profile-notes-complete.md` with this literal content. This
is a synthetic string-policy fixture, not an artifact to distribute as evidence:

```markdown
# Synthetic profile text for automated tests

## Evidence Artifacts

The synthetic report names are forest_floor_unity_import_report.json and
forest_floor_unreal_editor_report.json. The synthetic image names are
unity_forest_floor_import.png, unity_forest_floor_density.png,
unreal_forest_floor_import.png and unreal_forest_floor_foliage_settings.png.
This file tests string validation only and makes no claim that an editor was
executed or an image was captured. It must stay in the Rust test fixture folder.

## Unity

Unity Editor version: synthetic test value. The mobile profile mentions LOD,
Frame Debugger, instanced rendering, scale, density, material slot and wind.
Fields exercised by the text policy are detailPrototypesCreated,
detailPrototypesGeneratedFromGlb, detailPrototypeFailures,
detailPrototypeFallbackErrors, scatterBinaryRecordsRead and scatterChunkReports.
These names are deliberate input vocabulary for validating text requirements;
they are not measured values and cannot replace real import report checks.

## Unreal

Unreal Editor version: synthetic test value. The console profile mentions
RenderDoc, instanced rendering, scale, density, cull, material slot and wind.
Fields exercised by the text policy are foliage_type_count and
foliage_type_assets. The cull vocabulary contains 3500 and 7000. The production
gate must still independently check actual assets, checksums, images, recipes,
scatter buffers and engine reports. String completeness is only one component.

## Verdict

The strict verifier remains a separate full-engine acceptance gate. This
synthetic fixture passes only the profile text completeness policy and cannot
prove the existence, freshness or quality of any real profiling observation.
```

Create this initial `tests/profile_notes.rs` (later steps append tests):

```rust
use midori_cli::evidence::{
    Check, CheckStatus, EvidenceReport, ReportStatus, verify_profile_notes_text,
};

const GOOD: &str = include_str!("fixtures/profile-notes-complete.md");

fn check(name: &str, status: CheckStatus) -> Check {
    Check { name: name.into(), status, detail: "test".into() }
}

#[test]
fn report_precedence_and_exit_policy() {
    for (checks, status, strict, relaxed) in [
        (vec![check("a", CheckStatus::Passed)], ReportStatus::Passed, 0, 0),
        (vec![check("a", CheckStatus::Missing)], ReportStatus::Pending, 1, 0),
        (vec![check("a", CheckStatus::Failed)], ReportStatus::Failed, 1, 1),
        (vec![check("a", CheckStatus::Missing), check("b", CheckStatus::Failed)],
         ReportStatus::Failed, 1, 1),
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
    assert_eq!(report.checks.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), [
        "profile.notes.file", "profile.notes.length", "profile.notes.unresolved_markers",
        "profile.notes.sections", "profile.notes.required_terms", "profile.notes",
    ]);
    let value = serde_json::to_value(&report).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 2);
    assert_eq!(value["checks"][0], serde_json::json!({
        "name": "profile.notes.file", "status": "passed", "detail": "notes.md loaded"
    }));
    assert_eq!(value["checks"][5]["detail"], "notes.md contains Unity/Unreal profiling evidence");
}

#[test]
fn thin_notes_report_every_failure_without_final_success() {
    let report = verify_profile_notes_text("thin.md",
        "Unity Unreal Frame Debugger RenderDoc instanced scale density cull\n");
    assert_eq!(report.status, ReportStatus::Failed);
    assert_eq!(report.checks.len(), 5);
    assert_eq!(report.checks[1].status, CheckStatus::Failed);
    assert_eq!(report.checks[3].status, CheckStatus::Failed);
    assert_eq!(report.checks[4].status, CheckStatus::Failed);
}
```

- [ ] **Step 2: Run the focused test and record the intended failure.**

```powershell
$env:CARGO_TARGET_DIR = 'C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target'
cargo test -p midori-cli --test profile_notes
```

Expected: unresolved `midori_cli::evidence`/missing library, not an unrelated
toolchain or network failure. If dependency resolution is blocked, report it;
do not record that as a successful red test.

- [ ] **Step 3: Add dependencies and report implementation.**

Add to `crates/midori-cli/Cargo.toml` without changing existing entries:

```toml
# Under [dependencies]
serde = { version = "1.0", features = ["derive"] }
regex = "1.11"

[dev-dependencies]
tempfile = "3.10"
```

Create `src/lib.rs` with `pub mod evidence;`. Create `src/evidence/mod.rs`:

```rust
mod profile_notes;
mod report;
pub use profile_notes::verify_profile_notes_text;
pub use report::{Check, CheckStatus, EvidenceReport, ReportStatus};
```

Create `src/evidence/report.rs`:

```rust
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus { Passed, Failed, Missing }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportStatus { Passed, Failed, Pending }

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Check { pub name: String, pub status: CheckStatus, pub detail: String }

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EvidenceReport { pub status: ReportStatus, pub checks: Vec<Check> }

impl EvidenceReport {
    pub fn from_checks(checks: Vec<Check>) -> Self {
        let status = if checks.iter().any(|c| c.status == CheckStatus::Failed) {
            ReportStatus::Failed
        } else if checks.iter().any(|c| c.status == CheckStatus::Missing) {
            ReportStatus::Pending
        } else { ReportStatus::Passed };
        Self { status, checks }
    }
    pub fn exit_code(&self, allow_pending: bool) -> i32 {
        match self.status {
            ReportStatus::Passed => 0,
            ReportStatus::Pending if allow_pending => 0,
            _ => 1,
        }
    }
}
```

- [ ] **Step 4: Implement the complete text policy.**

Create `src/evidence/profile_notes.rs` using this code. The explicit Unicode
boundary is intentional: Rust regex `\b` would change Python marker matching.

```rust
use super::report::{Check, CheckStatus, EvidenceReport};
use regex::Regex;
use std::sync::LazyLock;

const TERMS: &[&str] = &[
    "Unity", "Unreal", "Unity Editor version", "Unreal Editor version",
    "mobile", "console", "LOD", "Frame Debugger", "RenderDoc", "instanced",
    "scale", "density", "cull", "material slot", "wind", "strict verifier",
    "forest_floor_unity_import_report.json", "forest_floor_unreal_editor_report.json",
    "unity_forest_floor_import.png", "unity_forest_floor_density.png",
    "unreal_forest_floor_import.png", "unreal_forest_floor_foliage_settings.png",
    "detailPrototypesCreated", "detailPrototypesGeneratedFromGlb",
    "detailPrototypeFailures", "detailPrototypeFallbackErrors",
    "scatterBinaryRecordsRead", "scatterChunkReports", "foliage_type_count",
    "foliage_type_assets", "3500", "7000",
];
const SECTIONS: &[&str] = &["## Evidence Artifacts", "## Unity", "## Unreal", "## Verdict"];
static MARKERS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    let bounded = |pattern: &str| Regex::new(&format!(
        r"(?i)(^|[^\p{{L}}\p{{N}}_])(?:{pattern})($|[^\p{{L}}\p{{N}}_])"
    )).expect("constant profile pattern");
    vec![
        ("TODO", bounded("TODO")), ("TBD", bounded("TBD")),
        ("placeholder", bounded("placeholder")), ("pending", bounded("pending")),
        ("not run", bounded(r"not[\s\x1c-\x1f]+run")),
        ("not captured", bounded(r"not[\s\x1c-\x1f]+captured")),
        ("missing evidence", bounded(r"missing[\s\x1c-\x1f]+(?:report|screenshot|notes?|evidence|artifact)s?")),
        ("fill marker", Regex::new(r"(?i)\[(?:fill|replace|todo)[^\]\n]*\]").expect("constant fill pattern")),
    ]
});

fn finding(name: &str, ok: bool, pass: String, fail: String) -> Check {
    Check { name: name.into(), status: if ok { CheckStatus::Passed } else { CheckStatus::Failed },
        detail: if ok { pass } else { fail } }
}

fn python_space(c: char) -> bool { c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c) }

pub fn verify_profile_notes_text(label: &str, text: &str) -> EvidenceReport {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n").replace('\r', "\n");
    let stripped = text.trim_matches(python_space);
    let lower = stripped.to_lowercase();
    // Python IGNORECASE additionally matches these characters against ASCII I/S/K.
    let marker_text: String = text.chars().map(|c| match c {
        '\u{130}' | '\u{131}' => 'i', '\u{17f}' => 's', '\u{212a}' => 'k', _ => c,
    }).collect();
    let forbidden: Vec<&str> = MARKERS.iter().filter_map(|(label, re)| re.is_match(&marker_text).then_some(*label)).collect();
    let missing_sections: Vec<&str> = SECTIONS.iter().copied().filter(|s| !lower.contains(&s.to_lowercase())).collect();
    let missing_terms: Vec<&str> = TERMS.iter().copied().filter(|s| !lower.contains(&s.to_lowercase())).collect();
    let mut checks = vec![
        Check { name: "profile.notes.file".into(), status: CheckStatus::Passed, detail: format!("{label} loaded") },
        finding("profile.notes.length", stripped.chars().count() >= 1000,
            format!("{label} is substantial enough for profile evidence"),
            format!("{label} is too short to be credible profile evidence")),
        finding("profile.notes.unresolved_markers", forbidden.is_empty(),
            format!("{label} contains no unresolved capture markers"),
            format!("{label} contains unresolved markers: {}", forbidden.join(", "))),
        finding("profile.notes.sections", missing_sections.is_empty(),
            format!("{label} contains required evidence sections"),
            format!("{label} is missing sections: {}", missing_sections.join(", "))),
        finding("profile.notes.required_terms", missing_terms.is_empty(),
            format!("{label} contains required engine evidence terms"),
            format!("{label} is missing required terms: {}", missing_terms.join(", "))),
    ];
    if checks.iter().all(|c| c.status == CheckStatus::Passed) {
        checks.push(Check { name: "profile.notes".into(), status: CheckStatus::Passed,
            detail: format!("{label} contains Unity/Unreal profiling evidence") });
    }
    EvidenceReport::from_checks(checks)
}
```

- [ ] **Step 5: Add exact boundary and vocabulary regressions.**

Append to `tests/profile_notes.rs`:

```rust
#[test]
fn markers_preserve_label_order_case_and_python_boundaries() {
    let all = format!("{GOOD}\n[fill value] missing notes not captured NOT\u{1c}RUN pending placeholder TBD TODO");
    let report = verify_profile_notes_text("m.md", &all);
    assert_eq!(report.checks[2].detail,
        "m.md contains unresolved markers: TODO, TBD, placeholder, pending, not run, not captured, missing evidence, fill marker");
    for marker in ["TODO", "tBd", "placeholder", "PENDİNG", "pendıng",
                   "not\nrun", "not captured", "miſſing artifacts", "[REPLACE value]",
                   "TODO\u{301}"] {
        let r = verify_profile_notes_text("m.md", &format!("{GOOD}\n{marker}"));
        assert_eq!(r.checks[2].status, CheckStatus::Failed, "{marker}");
    }
    for ordinary in ["TODO_item", "pendingly", "not runner", "[fill\nvalue]", "xTODO"] {
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
    for text in [format!("\u{feff}{}", GOOD.replace('\n', "\r\n")),
                 GOOD.replace('\n', "\r"), GOOD.to_uppercase()] {
        assert_eq!(verify_profile_notes_text("n.md", &text), baseline);
    }
}

#[test]
fn missing_section_lists_are_ordered() {
    let r = verify_profile_notes_text("n.md", &"x".repeat(1000));
    assert_eq!(r.checks[3].detail,
        "n.md is missing sections: ## Evidence Artifacts, ## Unity, ## Unreal, ## Verdict");
    assert_eq!(r.checks[4].detail,
        "n.md is missing required terms: Unity, Unreal, Unity Editor version, Unreal Editor version, mobile, console, LOD, Frame Debugger, RenderDoc, instanced, scale, density, cull, material slot, wind, strict verifier, forest_floor_unity_import_report.json, forest_floor_unreal_editor_report.json, unity_forest_floor_import.png, unity_forest_floor_density.png, unreal_forest_floor_import.png, unreal_forest_floor_foliage_settings.png, detailPrototypesCreated, detailPrototypesGeneratedFromGlb, detailPrototypeFailures, detailPrototypeFallbackErrors, scatterBinaryRecordsRead, scatterChunkReports, foliage_type_count, foliage_type_assets, 3500, 7000");
}

#[test]
fn removing_each_requirement_fails_the_corresponding_gate() {
    let terms = ["Unity", "Unreal", "Unity Editor version", "Unreal Editor version",
        "mobile", "console", "LOD", "Frame Debugger", "RenderDoc", "instanced", "scale",
        "density", "cull", "material slot", "wind", "strict verifier",
        "forest_floor_unity_import_report.json", "forest_floor_unreal_editor_report.json",
        "unity_forest_floor_import.png", "unity_forest_floor_density.png",
        "unreal_forest_floor_import.png", "unreal_forest_floor_foliage_settings.png",
        "detailPrototypesCreated", "detailPrototypesGeneratedFromGlb", "detailPrototypeFailures",
        "detailPrototypeFallbackErrors", "scatterBinaryRecordsRead", "scatterChunkReports",
        "foliage_type_count", "foliage_type_assets", "3500", "7000"];
    for term in terms {
        let changed = GOOD.to_lowercase().replace(&term.to_lowercase(), "removed");
        let r = verify_profile_notes_text("n.md", &changed);
        assert_eq!(r.checks[4].status, CheckStatus::Failed, "{term}");
    }
    for section in ["## Evidence Artifacts", "## Unity", "## Unreal", "## Verdict"] {
        let r = verify_profile_notes_text("n.md", &GOOD.replace(section, "## Removed"));
        assert_eq!(r.checks[3].status, CheckStatus::Failed, "{section}");
    }
}
```

- [ ] **Step 6: Verify and pause for task review.**

```powershell
$env:CARGO_TARGET_DIR = 'C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target'
cargo test -p midori-cli --test profile_notes
git diff --stat
```

Expected: every native text test passes; inspect lockfile additions and the exact
32-term/four-section/eight-marker contract. Record the real exit result. No
commit unless parent directs it.

### Task 2: File adapter and original profile self-test cases

**Files:** Modify `crates/midori-cli/src/evidence/profile_notes.rs`,
`src/evidence/mod.rs`, `tests/profile_notes.rs`.

**Interfaces:**
- Consumes: `verify_profile_notes_text(label: &str, text: &str) -> EvidenceReport`, report types from Task 1.
- Produces: `verify_profile_notes(path: &Path) -> EvidenceReport` in `midori_cli::evidence`.

- [ ] **Step 1: Append the failing filesystem/self-test cases.**

```rust
#[test]
fn profile_self_test_four_legacy_cases_and_file_errors() {
    use midori_cli::evidence::verify_profile_notes;
    let dir = tempfile::tempdir().unwrap();
    let absent = dir.path().join("absent.md");
    let r = verify_profile_notes(&absent);
    assert_eq!(r.status, ReportStatus::Pending);
    assert_eq!(r.checks.len(), 1);
    assert_eq!(r.checks[0].name, "profile.notes");
    assert_eq!(r.checks[0].detail, format!("{} is missing or empty", absent.display()));

    let copied = dir.path().join("copied-template.md");
    std::fs::write(&copied, include_str!("../../../docs/validation/midori-nature-engine-profile-notes.template.md")).unwrap();
    let r = verify_profile_notes(&copied);
    assert_eq!(r.status, ReportStatus::Failed);
    assert_eq!(r.checks[2].name, "profile.notes.unresolved_markers");
    assert_eq!(r.checks[2].status, CheckStatus::Failed);

    let thin = dir.path().join("thin.md");
    std::fs::write(&thin, "Unity Unreal Frame Debugger RenderDoc instanced scale density cull\n").unwrap();
    assert_eq!(verify_profile_notes(&thin).status, ReportStatus::Failed);
    let good = dir.path().join("good.md");
    std::fs::write(&good, format!("\u{feff}{}", GOOD.replace('\n', "\r\n"))).unwrap();
    assert_eq!(verify_profile_notes(&good).status, ReportStatus::Passed);

    let empty = dir.path().join("empty.md");
    std::fs::write(&empty, []).unwrap();
    assert_eq!(verify_profile_notes(&empty).status, ReportStatus::Pending);
    assert_eq!(verify_profile_notes(dir.path()).status, ReportStatus::Pending);
    let invalid = dir.path().join("invalid.md");
    std::fs::write(&invalid, [0xff]).unwrap();
    let r = verify_profile_notes(&invalid);
    assert_eq!(r.status, ReportStatus::Failed);
    assert_eq!(r.checks[0].name, "profile.notes");
    assert_eq!(r.exit_code(true), 1);
}
```

- [ ] **Step 2: Run the failing adapter test.**

```powershell
$env:CARGO_TARGET_DIR = 'C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target'
cargo test -p midori-cli --test profile_notes profile_self_test_four_legacy_cases_and_file_errors
```

Expected: missing `verify_profile_notes` export.

- [ ] **Step 3: Implement the file adapter and export it.**

Append this implementation to `profile_notes.rs`; in `mod.rs` re-export both
`verify_profile_notes` and `verify_profile_notes_text` from that module.

```rust
pub fn verify_profile_notes(path: &std::path::Path) -> EvidenceReport {
    let label = path.display().to_string();
    let single = |status, detail| EvidenceReport::from_checks(vec![Check {
        name: "profile.notes".into(), status, detail,
    }]);
    match std::fs::metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return single(CheckStatus::Missing, format!("{label} is missing or empty"));
        }
        Err(error) => return single(CheckStatus::Failed, format!("{label} cannot be read: {error}")),
        Ok(metadata) if !metadata.is_file() || metadata.len() == 0 => {
            return single(CheckStatus::Missing, format!("{label} is missing or empty"));
        }
        Ok(_) => {}
    }
    match std::fs::read_to_string(path) {
        Ok(text) => verify_profile_notes_text(&label, &text),
        Err(error) => single(CheckStatus::Failed, format!("{label} cannot be read as UTF-8: {error}")),
    }
}
```

- [ ] **Step 4: Run all native self-tests and review the exact scope.**

```powershell
$env:CARGO_TARGET_DIR = 'C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target'
cargo test -p midori-cli --test profile_notes
git diff --stat
```

Expected: all original four classifications and expanded tests pass. The source
template is read only through a Rust compile-time fixture inclusion; no Python
source is parsed/executed and no actual evidence file is created. Pause for
review, no commit unless parent directs it.

### Task 3: CLI command, output/error semantics and honest handoff

**Files:** Modify `crates/midori-cli/src/main.rs`, `README.md`; create
`crates/midori-cli/tests/verify_profile_notes_cli.rs`.

**Interfaces:**
- Consumes: `midori_cli::evidence::verify_profile_notes(&Path) -> EvidenceReport`, `EvidenceReport::exit_code(bool) -> i32`.
- Produces: clap `Commands::VerifyProfileNotes { profile_notes: PathBuf, output: Option<PathBuf>, allow_pending: bool }`; private `run_verify_profile_notes(&Path, Option<&Path>, bool) -> Result<i32, Box<dyn std::error::Error>>`; new command only, existing commands unchanged.

- [ ] **Step 1: Add failing actual-binary tests.**

Create `tests/verify_profile_notes_cli.rs`:

```rust
use std::process::{Command, Output};
use std::path::Path;
const GOOD: &str = include_str!("fixtures/profile-notes-complete.md");
fn run(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_midori")).current_dir(cwd)
        .args(args).output().unwrap()
}

#[test]
fn actual_binary_status_exit_matrix_and_output() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("good.md"), GOOD).unwrap();
    std::fs::write(dir.path().join("bad.md"), "TODO").unwrap();
    for (file, expected_status, strict, relaxed) in [
        ("good.md", "passed", 0, 0), ("absent.md", "pending", 1, 0),
        ("bad.md", "failed", 1, 1),
    ] {
        for allow in [false, true] {
            let mut args = vec!["verify-profile-notes", "--profile-notes", file,
                "--output", "reports/profile-only.json"];
            if allow { args.push("--allow-pending"); }
            let output = run(dir.path(), &args);
            assert_eq!(output.status.code(), Some(if allow { relaxed } else { strict }));
            let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(json["status"], expected_status);
            assert_eq!(std::fs::read(dir.path().join("reports/profile-only.json")).unwrap(), output.stdout);
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
    assert!(json["checks"][0]["detail"].as_str().unwrap().contains("midori-nature-engine-profile-notes.md"));
    let path = dir.path().join("docs/validation");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("midori-nature-engine-profile-notes.md"), GOOD).unwrap();
    assert_eq!(run(dir.path(), &["verify-profile-notes"]).status.code(), Some(0));
    std::fs::write(dir.path().join("blocked-parent"), "file").unwrap();
    let failed = run(dir.path(), &["verify-profile-notes", "--output",
        "blocked-parent/report.json", "--allow-pending"]);
    assert_eq!(failed.status.code(), Some(1));
    assert!(!failed.stderr.is_empty());
}

#[test]
fn unknown_flags_are_argument_errors_and_help_states_scope() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(run(dir.path(), &["verify-profile-notes", "--validation-root", "x"]).status.code(), Some(2));
    let help = run(dir.path(), &["verify-profile-notes", "--help"]);
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout).unwrap();
    assert!(text.contains("not full engine evidence"));
    assert!(text.contains("never failed"));
}
```

- [ ] **Step 2: Run tests and record the missing-command failure.**

```powershell
$env:CARGO_TARGET_DIR = 'C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target'
cargo test -p midori-cli --test verify_profile_notes_cli
```

Expected: unknown subcommand (exit 2), not the expected JSON/status assertions.

- [ ] **Step 3: Add the command and narrow dispatch branch.**

Add to `Commands`:

```rust
/// Verify profile-note completeness only; not full engine evidence
VerifyProfileNotes {
    #[arg(long, default_value = "docs/validation/midori-nature-engine-profile-notes.md")]
    profile_notes: PathBuf,
    /// Optional profile-only JSON report path
    #[arg(long)]
    output: Option<PathBuf>,
    /// Permit missing notes, never failed checks
    #[arg(long)]
    allow_pending: bool,
},
```

Keep the existing result-match and other arms intact. Add this match arm:

```rust
Commands::VerifyProfileNotes { profile_notes, output, allow_pending } => {
    match run_verify_profile_notes(&profile_notes, output.as_deref(), allow_pending) {
        Ok(code) => std::process::exit(code),
        Err(error) => Err(error),
    }
}
```

Add the private function (all writers finish before process exit):

```rust
fn run_verify_profile_notes(
    path: &std::path::Path,
    output: Option<&std::path::Path>,
    allow_pending: bool,
) -> Result<i32, Box<dyn std::error::Error>> {
    use std::io::Write;
    let report = midori_cli::evidence::verify_profile_notes(path);
    let mut json = serde_json::to_vec_pretty(&report)?;
    json.push(b'\n');
    if let Some(output) = output {
        if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(output, &json)?;
    }
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(&json)?;
    handle.flush()?;
    Ok(report.exit_code(allow_pending))
}
```

Do not route nonzero report exits through a generic `Err` that prints an extra
error after valid JSON; diagnostics are already in the report. Output failures
use the existing main `Error: ...` stderr path and exit 1.

- [ ] **Step 4: Add this exact README section after the existing engine-validation usage.**

````markdown
### Native profile-notes check (partial tooling migration)

```powershell
$env:CARGO_TARGET_DIR = 'C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target'
cargo run -p midori-cli -- verify-profile-notes --profile-notes docs/validation/midori-nature-engine-profile-notes.md --output C:/midori-build/profile-notes-only.json
cargo test -p midori-cli --test profile_notes
```

This native command checks the complete profile-note text policy only. It does
not verify images, GLBs, scatter buffers, recipes, checksums or real engine
execution, and its report is not a Phase 7 completion report. The full existing
Python evidence verifier and engine/handoff wrappers remain required until
their separate migrations pass review; this checkout is not yet Python-free.

Missing/empty notes produce `pending` and exit 1, or exit 0 with
`--allow-pending`. Failed checks always exit 1, even with that flag. This fixes
the legacy verifier's success-on-failed-report exit bug without changing its
JSON finding shape. The old full Python command itself is unchanged here.
````

- [ ] **Step 5: Run targeted and regression verification; record real evidence.**

```powershell
$env:CARGO_TARGET_DIR = 'C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target'
cargo test -p midori-cli --locked
cargo test -p midori-core --locked
cargo fmt --all -- --check
cargo run -p midori-cli --locked -- verify-profile-notes --help
git diff --check
git status --short
git diff --stat
```

Expected: native tests and prior CLI/core tests pass, format/diff checks pass,
help explicitly says profile-only and `never failed`. If formatting fails,
format only touched Rust files or revert unrelated formatter changes using
careful patches; preserve user edits. Do not run the legacy validation wrappers
or engines as part of this proof.

- [ ] **Step 6: Verify Python absence for the selected command/tests and hand off.**

Read the added Cargo/library/test code and lockfile for subprocess/build-script
dependencies. Run the already-built binary in a process with a minimal PATH:

```powershell
$profileSavedPath = $env:PATH
try {
    $env:PATH = "$env:SystemRoot/System32"
    Get-Command python, python3, py -ErrorAction SilentlyContinue
    & 'C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target/debug/midori.exe' verify-profile-notes --profile-notes 'crates/midori-cli/tests/fixtures/profile-notes-complete.md'
    $profileNativeExit = $LASTEXITCODE
} finally {
    $env:PATH = $profileSavedPath
}
if ($profileNativeExit -ne 0) { throw "Native profile check failed: $profileNativeExit" }
git diff --name-only
```

Expected: no Python executable found in this temporary PATH; selected command
returns passed JSON/0. This is runtime evidence plus source/dependency review,
not a claim that a whole clean checkout was built on a Python-less machine.
Record exact Cargo/Rust versions with `cargo --version` and `rustc --version`,
commands and exits in the parent handoff. Ask the parent for independent Astra
review. Do not commit, push or change wrappers unless directed.

## Self-review checklist and coverage map

- [ ] Task 1 covers exact report shape, status precedence, fixed exit matrix,
  all constants, failure accumulation, case/Unicode boundaries, character length,
  universal newlines, and precise ordered details.
- [ ] Task 2 covers original absent/copied-template/thin/complete cases, empty
  file/directory and invalid UTF-8, without executing Python.
- [ ] Task 3 covers the actual binary, output JSON equality/newline, default
  paths, strict/allow-pending exits including failure, argument/output errors,
  documentation and existing-command regressions.
- [ ] Every public type/function in later tasks is defined earlier; `EvidenceReport`
  derives `PartialEq/Eq` for the normalization parity tests.
- [ ] Synthetic test content stays in tests. No passing profile report is placed
  in the full-verifier conventional output path or used to complete Phase 7.
- [ ] The implementation diff contains no changes to the Python full verifier,
  self-test reference, wrappers, Unreal importer, scaffold or engine evidence.

## Retained migration work, not delivered here

The next separately designed unit must cover full engine report/package identity,
FNV checksums, PNG content and exact reported dimensions, GLB attributes/materials,
scatter records/parity, material/engine recipes, overlays, budgets, scaffolds and
summary semantics before naming a command `verify-engine-evidence`. Its release
proof still requires real Unity/Unreal execution. The Unreal C++ Editor plugin
and commandlet remain required, not a stub or removal of import functionality.
Neither the historical 1,610-passed/7-missing result nor a synthetic profile test
is new real-engine evidence.
