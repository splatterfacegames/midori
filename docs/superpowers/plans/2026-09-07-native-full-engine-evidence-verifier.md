# Native Full Engine-Evidence Verifier Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a complete native Rust replacement for Midori's full Python engine-evidence verifier, native synthetic self-tests, and a non-cutover CLI adapter.

**Architecture:** Reuse the approved native evidence report and profile-notes library. Add a `full` verification module split by value/file identity, PNG decoding, package policy, and Unity/Unreal summary policy, then expose it through `midori verify-engine-evidence` without changing legacy wrappers.

**Tech Stack:** Rust 2024, serde/serde_json, regex, flate2, clap, tempfile, Cargo tests on Windows PowerShell.

**Spec:** `docs/superpowers/specs/2026-09-07-native-full-engine-evidence-verifier-design.md`

## Global Constraints

- Work only in `B:/lab/worktrees/midori-python-free` on `agent/python-free-core`, starting from `4a3ac1c07ee8e81b5166014fba2f0a603ae87d9e`.
- Preserve all eight local baseline commits, original `B:/lab/Midori`, completed profile-notes plan/spec/ledger/report, real evidence, Python/PowerShell scripts, shared repositories, remotes, and PR state.
- Port all 3,785 lines of actual verifier policy; never remove a check or create a fake-passed full command.
- Do not execute Python, Unity, Unreal, an engine/provider/GPU process, or real-engine acceptance in this subproject.
- Native tests use synthetic data only in their exact owned `tempfile::TempDir` trees and identify themselves as not real-engine acceptance.
- Strict exit is 1 for failed or pending; `--allow-pending` may change pending only to 0 and never waives failed or I/O errors.
- Keep the old Python full command and wrappers unchanged until a separately reviewed cutover.
- Set `CARGO_TARGET_DIR=C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target` and `CARGO_BUILD_JOBS=2`; do not reinstall broadly or clean caches.
- Scoped local commits are authorized. Never force-add ignored SDD reports or unrelated/untracked files.

---

### Task 1: Complete native verifier library and synthetic self-tests

**Files:**
- Modify: `crates/midori-cli/Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/midori-cli/src/evidence/mod.rs`
- Create: `crates/midori-cli/src/evidence/full/mod.rs`
- Create: `crates/midori-cli/src/evidence/full/common.rs`
- Create: `crates/midori-cli/src/evidence/full/png.rs`
- Create: `crates/midori-cli/src/evidence/full/package.rs`
- Create: `crates/midori-cli/src/evidence/full/engines.rs`
- Create: `crates/midori-cli/tests/support/mod.rs`
- Create: `crates/midori-cli/tests/support/full_evidence_fixture.rs`
- Create: `crates/midori-cli/tests/full_engine_evidence.rs`

**Interfaces:**
- Consumes: `evidence::{Check, CheckStatus, EvidenceReport, ReportStatus, verify_profile_notes}` and every helper/checker contract in `scripts/verify_engine_evidence.py`.
- Produces: `FullEvidenceOptions::from_validation_root(PathBuf) -> FullEvidenceOptions` and `verify_engine_evidence(&FullEvidenceOptions) -> EvidenceReport`, re-exported from `evidence`.

- [ ] **Step 1: Write the failing public-contract and complete-fixture tests**

```rust
let fixture = SyntheticFullEvidence::create();
let report = verify_engine_evidence(&fixture.options());
assert_eq!(report.status, ReportStatus::Passed);
assert!(report.checks.iter().all(|check| check.status == CheckStatus::Passed));
for required in REQUIRED_CHECK_FAMILIES {
    assert!(report.checks.iter().any(|check| check.name.starts_with(required)), "{required}");
}
```

The independent fixture builder writes all spec cardinalities beneath its owned
`TempDir`. Add table-driven mutations with literal expected failing check names
for every family listed in the spec, plus missing-only and malformed-input cases.

- [ ] **Step 2: Run the focused tests and capture the expected RED**

Run:

```powershell
$env:CARGO_TARGET_DIR='C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target'
$env:CARGO_BUILD_JOBS='2'
cargo test -p midori-cli --test full_engine_evidence
```

Expected: compilation fails because `FullEvidenceOptions` and
`verify_engine_evidence` do not exist. Record the command, exit code, and relevant
failure in the task report before production implementation.

- [ ] **Step 3: Implement the complete verifier library**

```rust
#[derive(Clone, Debug)]
pub struct FullEvidenceOptions {
    pub validation_root: PathBuf,
    pub midori_report: PathBuf,
    pub summary: PathBuf,
    pub unity_report: PathBuf,
    pub unreal_dry_run_report: PathBuf,
    pub unreal_report: PathBuf,
    pub unity_import_screenshot: PathBuf,
    pub unity_density_screenshot: PathBuf,
    pub unreal_import_screenshot: PathBuf,
    pub unreal_foliage_settings_screenshot: PathBuf,
    pub profile_notes: PathBuf,
}

pub fn verify_engine_evidence(options: &FullEvidenceOptions) -> EvidenceReport {
    let mut verifier = Verifier::default();
    // Preserve legacy order: Midori, summary, dependent Unreal dry-run,
    // Unity and Unreal reports, four PNG artifacts, native profile notes.
    verify_all(&mut verifier, options);
    EvidenceReport::from_checks(verifier.into_checks())
}
```

Port every legacy helper/checker into its planned responsibility. Preserve
constants, check names/order, JSON casing, exact cardinalities, comparisons and
path resolution. Convert all Python panic/exception surfaces into failed checks
without weakening missing classification. Add `flate2 = "1"` to dependencies and
`png = "0.17"` to dev-dependencies; both already have compatible locked versions.

- [ ] **Step 4: Run RED/GREEN cycles until the focused suite passes**

Run the Step 2 command after each narrow implementation slice. For any defect
found after green, first add a mutation that fails for the correct reason, record
RED, implement the smallest correction, and record GREEN.

- [ ] **Step 5: Run package tests and static preservation checks**

```powershell
$env:CARGO_TARGET_DIR='C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target'
$env:CARGO_BUILD_JOBS='2'
cargo test -p midori-cli --all-targets
cargo fmt --all -- --check
git diff --exit-code 4a3ac1c07ee8e81b5166014fba2f0a603ae87d9e -- scripts integrations docs/validation
```

Expected: all Rust tests pass; formatting passes; preservation diff is empty.

- [ ] **Step 6: Commit the library and tests**

```powershell
git add Cargo.lock crates/midori-cli/Cargo.toml crates/midori-cli/src/evidence crates/midori-cli/tests/full_engine_evidence.rs crates/midori-cli/tests/support
git commit -m "feat(midori): add native full evidence verifier"
```

Do not add either plan/spec or `.superpowers` files.

### Task 2: Native CLI adapter and end-to-end exit contract

**Files:**
- Modify: `crates/midori-cli/src/main.rs`
- Create: `crates/midori-cli/tests/verify_engine_evidence_cli.rs`
- Reuse: `crates/midori-cli/tests/support/full_evidence_fixture.rs`

**Interfaces:**
- Consumes: Task 1's `FullEvidenceOptions` and `verify_engine_evidence`.
- Produces: `midori verify-engine-evidence` with all legacy path flags and exact strict/pending/output behavior.

- [ ] **Step 1: Write actual-binary failing tests**

```rust
let output = Command::new(env!("CARGO_BIN_EXE_midori"))
    .current_dir(fixture.root())
    .args(["verify-engine-evidence", "--validation-root", fixture.validation_root_arg()])
    .output()?;
assert_eq!(output.status.code(), Some(0));
assert!(output.stderr.is_empty());
assert_eq!(serde_json::from_slice::<Value>(&output.stdout)?["status"], "passed");
```

Add literal tests for strict pending exit 1, pending with `--allow-pending` exit
0, failed with `--allow-pending` exit 1, every path override, cwd-relative
defaults, stdout/output byte identity with newline, blocked output path exit 1,
unknown option exit 2, and help that says verification does not launch or prove
real engines.

- [ ] **Step 2: Run the CLI test and capture the expected RED**

```powershell
$env:CARGO_TARGET_DIR='C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target'
$env:CARGO_BUILD_JOBS='2'
cargo test -p midori-cli --test verify_engine_evidence_cli
```

Expected: assertions fail because `verify-engine-evidence` is not yet a command.

- [ ] **Step 3: Implement the CLI adapter**

```rust
Commands::VerifyEngineEvidence {
    validation_root, midori_report, summary, unity_report,
    unreal_dry_run_report, unreal_report,
    unity_import_screenshot, unity_density_screenshot,
    unreal_import_screenshot, unreal_foliage_settings_screenshot,
    profile_notes, output, allow_pending,
} => match run_verify_engine_evidence(
    validation_root, midori_report, summary, unity_report,
    unreal_dry_run_report, unreal_report,
    unity_import_screenshot, unity_density_screenshot,
    unreal_import_screenshot, unreal_foliage_settings_screenshot,
    profile_notes, output.as_deref(), allow_pending,
) {
        Ok(code) => std::process::exit(code),
        Err(error) => Err(error),
    },
```

Build defaults from `FullEvidenceOptions::from_validation_root`, replace only
provided overrides, serialize one pretty JSON value plus newline, write requested
output before stdout, and return `EvidenceReport::exit_code(allow_pending)`.

- [ ] **Step 4: Run focused and complete native verification**

```powershell
$env:CARGO_TARGET_DIR='C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target'
$env:CARGO_BUILD_JOBS='2'
cargo test -p midori-cli --test verify_engine_evidence_cli
cargo test -p midori-cli --all-targets
cargo fmt --all -- --check
cargo clippy -p midori-cli --all-targets -- -D warnings
git diff --exit-code 4a3ac1c07ee8e81b5166014fba2f0a603ae87d9e -- scripts integrations docs/validation
```

Expected: focused and package tests pass; fmt/clippy pass; preservation diff is
empty. Tests must state and prove their temp fixtures are not real-engine
acceptance.

- [ ] **Step 5: Commit the CLI adapter**

```powershell
git add crates/midori-cli/src/main.rs crates/midori-cli/tests/verify_engine_evidence_cli.rs crates/midori-cli/tests/support
git commit -m "feat(midori): expose native evidence verification CLI"
```

Do not add legacy wrappers, plan/spec scratch, SDD reports, or real evidence.
