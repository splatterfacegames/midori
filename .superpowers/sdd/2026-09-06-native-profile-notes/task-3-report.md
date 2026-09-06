# Task 3 report: native profile-notes CLI

Date: 2026-09-07 (Asia/Tokyo)
Worktree: `B:/lab/worktrees/midori-python-free`
Base: `39c2beff3354dda544da32eaf369791b331a0705`

## Delivered files

- Modified `crates/midori-cli/src/main.rs`: added the `verify-profile-notes`
  clap command, narrow dispatch arm, and JSON stdout/optional-output writer.
- Modified `README.md`: added the partial native profile-notes command and its
  intentionally limited scope and exit semantics.
- Added `crates/midori-cli/tests/verify_profile_notes_cli.rs`: actual binary
  regression coverage for statuses, exit policy, output equality/newline,
  defaults, output errors, unknown flags, and help scope.
- Added this report.

No Cargo manifest or lockfile changes were needed. Task 1/2 evidence APIs and
tests were consumed unchanged. No Python verifier, wrapper, engine, fixture,
actual profile note, or historical evidence was changed.

## TDD evidence

Command before the CLI implementation:

```powershell
$env:CARGO_TARGET_DIR = 'C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target'
cargo test -p midori-cli --test verify_profile_notes_cli
```

Observed result: the test binary compiled, then all three tests failed because
the subcommand was not present. The first failure was `left: Some(2)` and
`right: Some(1)`; the status-matrix test similarly saw `left: Some(2)` vs
`right: Some(0)`; the help test failed `help.status.success()`.

After the implementation:

```powershell
$env:CARGO_TARGET_DIR = 'C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target'
cargo test -p midori-cli --test verify_profile_notes_cli
```

```text
running 3 tests
test unknown_flags_are_argument_errors_and_help_states_scope ... ok
test defaults_are_cwd_relative_and_output_failures_are_not_waived ... ok
test actual_binary_status_exit_matrix_and_output ... ok
test result: ok. 3 passed; 0 failed
```

## Verification commands and exact result summaries

All Cargo commands below used:

```powershell
$env:CARGO_TARGET_DIR = 'C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target'
$env:CARGO_BUILD_JOBS = '2'
```

```powershell
cargo test -p midori-cli --locked --quiet
```

```text
midori-cli: 0 + 2 + 9 + 3 + 0 tests; all test result: ok
```

The full non-quiet run also completed with the same five groups: 2 main unit
tests, 9 profile self-tests, 3 CLI tests, and 0 doc tests (library unit group
also had 0 tests).

```powershell
cargo test -p midori-core --locked --quiet
```

```text
185 unit tests passed; 2 integration tests passed; 4 doctests passed; 0 failed
```

```powershell
cargo fmt --all -- --check
```

Exit: `1`. This reports pre-existing Task 1 formatting differences only in
`crates/midori-cli/src/evidence/profile_notes.rs` and
`crates/midori-cli/tests/profile_notes.rs`. Those files were intentionally not
modified in this Task 3-only change. The touched files were checked narrowly:

```powershell
rustfmt --edition 2024 --check crates/midori-cli/src/main.rs crates/midori-cli/tests/verify_profile_notes_cli.rs
```

Exit: `0`.

```powershell
git diff --check
```

Exit: `0` (only Git's LF/CRLF working-copy warnings were printed for the two
modified tracked text files).

```powershell
cargo run -p midori-cli --locked -- verify-profile-notes --help
```

Exit: `0`; help output included:

```text
Verify profile-note completeness only; not full engine evidence
--allow-pending                  Permit missing notes, never failed checks
```

Toolchain identity:

```powershell
cargo --version
rustc --version
```

```text
cargo 1.98.0 (797e8a9bc 2026-08-05)
rustc 1.98.0 (88d9e12ae 2026-08-18)
```

## Reduced-PATH runtime proof

The already-built binary was run in a child process with the temporary PATH
set to `$env:SystemRoot/System32`, and PATH was restored in `finally`:

```powershell
$profileSavedPath = $env:PATH
$profileNativeExit = $null
try {
    $env:PATH = "$env:SystemRoot/System32"
    Get-Command python, python3, py -ErrorAction SilentlyContinue
    & 'C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target/debug/midori.exe' verify-profile-notes --profile-notes 'crates/midori-cli/tests/fixtures/profile-notes-complete.md'
    $profileNativeExit = $LASTEXITCODE
} finally {
    $env:PATH = $profileSavedPath
}
if ($profileNativeExit -ne 0) { throw "Native profile check failed: $profileNativeExit" }
Write-Output "selected command exit: $profileNativeExit"
Write-Output "PATH restored: $($env:PATH -eq $profileSavedPath)"
```

No Python executable was found (the `Get-Command` line emitted no output). The
binary emitted a passed JSON report with six passed checks, then:

```text
selected command exit: 0
PATH restored: True
```

This is selected-command local child-process runtime evidence plus source and
dependency review. It is not a claim that the full checkout or all migrations
build without Python.

## Source/dependency review

`crates/midori-cli/Cargo.toml` and `Cargo.lock` contain no Python, build-script,
or runtime subprocess dependency. The only process use in the new CLI path is
Rust's `std::process::exit`; the new test uses Rust `std::process::Command` to
launch the actual binary. The profile policy remains a native Rust library.

## Concerns and handoff

- Full-workspace format check remains nonzero because of inherited Task 1
  formatting; touched-file rustfmt and diff checks are clean.
- The command verifies profile-note text completeness only. It does not verify
  images, GLBs, scatter buffers, recipes, checksums, editor reports, or real
  Unity/Unreal execution. The checkout remains not Python-free overall.
- Independent Astra review is required before treating this slice as accepted.
