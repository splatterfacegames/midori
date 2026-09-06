# Native profile-notes verification: first Midori migration unit

## Authority and scope

This refines the approved Python-free lab design at
`B:/lab/worktrees/megumi-docs-import-status/docs/superpowers/specs/2026-09-06-python-free-lab-design.md`
and the source findings in `B:/lab/frontend-migration/python-free-delta.md`.
Planning baseline: `a24e4fe93b865330cff724ded9466108690b22b8`, branch
`agent/python-free-core`, worktree `B:/lab/worktrees/midori-python-free`.
Preserve the eight local commits ahead of the original remote.

The parent explicitly authorized the smallest coherent complete subcommand if
the full verifier was too large. Inspection found 3,785 lines in
`scripts/verify_engine_evidence.py`, including package, material, recipe, GLB,
scatter, scaffold, engine-report and screenshot gates. This unit implements
**all of the profile-notes gate only**, plus its native tests. It does not port
the full verifier or its dependent wrappers.

New invocation:

```text
midori verify-profile-notes [--profile-notes PATH] [--output PATH] [--allow-pending]
```

The default notes path remains
`docs/validation/midori-nature-engine-profile-notes.md`, relative to the process
working directory. There is no default output path, validation-root option,
engine launch, network request or Python invocation.

## Alternatives and decision

1. Port the full verifier in one task: too broad for an independently reviewable
   first slice, especially its 1,500-line summary/scaffold gate.
2. Expose a partial command named `verify-engine-evidence`: rejected because its
   success would look like strict release acceptance while omitting most checks.
3. Implement a plainly named, complete profile gate: selected. Its reusable Rust
   library entry can later be composed into the full Rust evidence verifier.

Keep the library inside `midori-cli` for this slice rather than creating a new
workspace crate. It has no dependency on clap or process state. A later verifier
can consume the library or move it without changing the check contract.

## Inherited constraints

- Application business logic, persistence, local services, CLI entrypoints and desktop command handlers run in Rust; presentation runs in React/TypeScript.
- Historical Python implementations remain migration references until parity is verified. Retire them from active app/build/test entrypoints at cutover, keeping recoverable Git history. Do not delete user data, environments or model caches.
- Production acceptance retains the full audited capabilities and unfinished release blockers. No app is declared complete based on a shell, disabled control, untextured preview, unsupported-format stub or artificially reduced scope.
- Keep build caches on C: while B: is low on space; do not spend on GPU workers or copy large models merely for architecture work.

This slice's selected command and tests must run without Python; it does not
assert that the entire checkout is Python-free. Planning makes only Markdown
edits, no builds, installs, commits, engine runs or provider calls. Execution
requires parent self-review followed by the approved Luna implementation and
independent Astra review workflow.

## Report and status contract

Preserve the existing JSON finding shape and order:

```json
{
  "status": "pending",
  "checks": [
    { "name": "profile.notes", "status": "missing", "detail": "absent.md is missing or empty" }
  ]
}
```

No additional top-level scope or schema fields. Scope is explicit in command
name, help, docs and the output path the caller selects. Do not write this report
to the conventional full-verifier report by default or claim it proves engine
execution. JSON serialization is pretty-printed with a final newline; key
spelling and check order are contractual, ASCII escaping and OS path separators
are not. The same serialized document goes to stdout and optional output.

Report status precedence is `failed` over `pending` over `passed`; pending means
at least one `missing` finding and no failed finding. Findings use `passed`,
`failed`, `missing`, never `pending` as a finding status.

| Report status | Default exit | With `--allow-pending` |
| --- | --- | --- |
| passed | 0 | 0 |
| pending | 1 | 0 |
| failed | 1 | 1 |

This deliberately fixes the legacy Python bug at lines 3778–3781: its flag
currently returns 0 even for failed reports. Parent review explicitly ruled that
failed checks must never become successful exits. A caller relying on that bug
will now fail correctly. The old full Python verifier and wrappers are unchanged
in this unit. Argument errors use clap's existing exit 2; output I/O errors use
exit 1, never waived by `--allow-pending`.

## Exact profile policy

Reference: `PROFILE_NOTE_REQUIRED_TERMS`, `PROFILE_NOTE_REQUIRED_SECTIONS`,
`PROFILE_NOTE_FORBIDDEN_PATTERNS`, and `check_profile_notes` at lines 34–93 and
3639–3685 of the Python verifier.

1. A missing path, non-file path or zero-byte file emits only
   `profile.notes: missing` with `<path> is missing or empty`.
2. Decode nonempty files as strict UTF-8, remove one leading UTF-8 BOM and perform
   Python universal-newline conversion (`CRLF` and lone `CR` to `LF`). Read or
   decoding failure emits `profile.notes: failed` and a useful error detail;
   never silently treat unreadable or malformed data as absent. This structured
   error replaces Python's uncaught I/O/Unicode exception while retaining exit 1.
3. Emit `profile.notes.file: passed`, `<path> loaded`.
4. Trim Python whitespace and count Unicode scalar values, not UTF-8 bytes. The
   length gate is 1,000 characters inclusive. Emit `profile.notes.length`.
5. Search the original decoded text, case-insensitively, for all eight forbidden
   patterns. Emit `profile.notes.unresolved_markers`, with matching labels joined
   in original declaration order. Evaluate all patterns, not just the first.
6. Required section names and terms are case-insensitive substring checks in the
   lowercased, stripped text. Preserve all four sections, all 32 terms and their
   original order in missing lists. Emit `profile.notes.sections`, then
   `profile.notes.required_terms`.
7. Do not short-circuit length/marker/section/term failures. Emit all five checks
   for a readable nonempty failure; append final `profile.notes: passed` only
   when all four policy checks pass (six checks total).

Use the exact source detail strings for each policy result. Python whitespace
includes U+001C–U+001F in addition to Rust's Unicode White_Space. Regex word
boundaries must use Python's letter/number/underscore definition, not the Rust
regex crate's broader combining-mark word class. Preserve Python's special
case-insensitive matches for ASCII I/S/K (`İ`, `ı`, `ſ`, `K`) in marker detection.
This is a text completeness gate, not natural-language proof of observations:
synthetic passing test notes do not prove actual profiling happened.

## Files and interfaces

| File | Responsibility |
| --- | --- |
| `crates/midori-cli/src/lib.rs` | Expose `pub mod evidence` without process effects |
| `crates/midori-cli/src/evidence/mod.rs` | Public re-exports |
| `crates/midori-cli/src/evidence/report.rs` | Serializable status/check/report, precedence and strict exit policy |
| `crates/midori-cli/src/evidence/profile_notes.rs` | Immutable constants, text policy, filesystem adapter |
| `crates/midori-cli/tests/profile_notes.rs` | Original four self-test cases and expanded boundary/error regressions |
| `crates/midori-cli/tests/verify_profile_notes_cli.rs` | Real CLI JSON, exit and output behavior |
| `crates/midori-cli/tests/fixtures/profile-notes-complete.md` | Clearly synthetic positive fixture, never engine evidence |
| `crates/midori-cli/src/main.rs` | Add clap command and thin report writer |
| `crates/midori-cli/Cargo.toml`, `Cargo.lock` | Add serde derive, regex, tempfile dev dependency |
| `README.md` | Document scoped native command and remaining Python migration |

Public library API:

```rust
pub enum CheckStatus { Passed, Failed, Missing }
pub enum ReportStatus { Passed, Failed, Pending }
pub struct Check { pub name: String, pub status: CheckStatus, pub detail: String }
pub struct EvidenceReport { pub status: ReportStatus, pub checks: Vec<Check> }
impl EvidenceReport {
    pub fn from_checks(checks: Vec<Check>) -> Self;
    pub fn exit_code(&self, allow_pending: bool) -> i32;
}
pub fn verify_profile_notes_text(label: &str, text: &str) -> EvidenceReport;
pub fn verify_profile_notes(path: &std::path::Path) -> EvidenceReport;
```

`verify_profile_notes_text` consumes decoded text and performs BOM/newline
normalization itself, so its tests cover the same policy as the file adapter.
It treats the empty string as readable text (failed length and missing
requirements); the filesystem adapter alone classifies zero bytes as missing.

## Acceptance and explicitly retained blockers

Native tests reproduce absent notes => pending, copied current template =>
failed, thin notes => failed, synthetic complete notes => passed. Extended tests
pin exact ordered findings/details, every required term and section, all marker
labels, case handling, BOM/newlines, Unicode character thresholds, missing/empty/
directory, invalid UTF-8, stdout/output equality and the exit table including
failed + allow-pending. No test runs Python or an editor.

The full Python verifier remains authoritative until its separate complete
replacement is approved. Checksums/FNV64, PNG decoding/luminance/dimensions,
GLB attributes, material slots, overlays/recipes, scatter header/record ranges/
parity, package identity, profile budgets, scaffold checks and real engine report
validation all remain unported by this slice. Current fixture expectations of
28 Unreal import assets, eight LOD0 foliage types, 26 chunks and 222 records are
not weakened. The native Unreal C++ plugin/commandlet remains future required
work, retaining real asset import, foliage settings, reports and screenshots;
do not substitute a stub or claim a Landscape implementation exists.

Do not change `scripts/verify_engine_evidence.py`,
`scripts/test_profile_notes_verifier.py`, engine scripts, PowerShell wrappers,
handoff bundles, templates, actual profile notes or historical evidence reports.
Do not use a profile-only passing report to set `phase_7_complete`.

## Review notes

This document records inspected source, not new test results. Parent must review
the intentional exit compatibility fix and narrow scope before execution. No
toolchain/library-version claims depend on a live lookup: implementation resolves
declared Cargo dependency ranges and records the resulting lockfile and versions.
