# Native Full Engine-Evidence Verifier Design

## Scope and authority

This subproject replaces the implementation of Midori's full engine-evidence
verification policy with a native Rust library and CLI command. The approved
parent design at
`B:/lab/worktrees/megumi-docs-import-status/docs/superpowers/specs/2026-09-06-python-free-lab-design.md`
is authoritative: application verification must not require Python, and removal
of Python must not weaken evidence requirements.

This unit does not cut over PowerShell wrappers, remove Python files, launch an
editor, qualify a GPU, or claim Phase 7 engine acceptance. Existing Python
scripts, handoff wrappers, generated evidence, and the completed native
profile-notes work remain unchanged. Wrapper cutover is a separate reviewed
change after native parity is demonstrated.

## Compatibility contract

The new `midori verify-engine-evidence` command accepts the legacy full
verifier's path surface: `--validation-root`, `--midori-report`, `--summary`,
`--unity-report`, `--unreal-dry-run-report`, `--unreal-report`, the four
screenshot flags, `--profile-notes`, `--output`, and `--allow-pending`.
Defaults remain current-working-directory-relative and retain the legacy
`forest_floor` filenames and canonical documentation paths.

The JSON result remains exactly a top-level `status` and ordered `checks`
array. Each check retains `name`, lowercase `status` (`passed`, `failed`, or
`missing`), and human-readable `detail`. Failed checks dominate overall status;
otherwise missing checks produce `pending`; otherwise status is `passed`.
Strict mode exits 1 for both failed and pending. `--allow-pending` changes only
pending to exit 0: it never waives a failed check, malformed input, unreadable
input, or output-write error. Pretty JSON with one terminal newline is written
to stdout and, when requested, byte-for-byte to `--output`.

## Verification architecture

The existing `evidence::report` and `evidence::profile_notes` modules remain
the common status/serialization policy. A focused `evidence::full` module owns
orchestration and public options. Its internal modules separate JSON/value and
identity helpers, PNG decoding, Midori package/report policy, and summary plus
Unity/Unreal policy. Production code uses `serde_json::Value` at the imported
JSON boundary so camelCase and snake_case legacy reports can be checked without
inventing a competing schema.

The public interface is:

```rust
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

impl FullEvidenceOptions {
    pub fn from_validation_root(validation_root: PathBuf) -> Self;
}

pub fn verify_engine_evidence(options: &FullEvidenceOptions) -> EvidenceReport;
```

Path overrides replace only their named default. The package directory remains
`validation_root/forest_floor`, matching the legacy verifier.

## Required legacy semantics

The Rust verifier ports every checker and helper in
`scripts/verify_engine_evidence.py`, preserving check ordering and all current
policy constants. This includes:

- report status precedence, missing-vs-failed classification, numeric equality
  and absolute-tolerance comparisons, vector object/array shapes, UTF-8 BOM
  JSON/text handling, FNV-1a 64-bit file checksums, sorted source-file identity,
  manifest checksum and source-file XOR identity;
- screenshot PNG signature/IHDR/IDAT/palette parsing, zlib decompression, PNG
  filters 0 through 4, supported 8-bit color types, sampled luminance, minimum
  256x256 size, and minimum luminance range 8;
- Midori schema/name/counts, map formats/ranges/relationships, prototype GLB
  summaries and attributes, LOD/material/instance budgets, memory footprint,
  JSON/binary scatter parity and ranges/checksums;
- surface overlays and prototype targets, material parameters and engine-native
  names, material recipes and their required texture/vertex-stream evidence,
  engine import recipes/profiles/systems and report projections;
- generated Unity/Unreal project scaffold evidence, Unity compile-stub report
  details, Unreal fake-editor report details and synthetic screenshot metrics;
- Unity editor report package identity, terrain/profile/material/wind/detail
  prototype/scatter/screenshot evidence;
- Unreal dry-run and real editor report package identity, import task files and
  destinations, profile/material/wind/foliage/cull/scatter/screenshot evidence;
- four external screenshot artifacts and the already approved native
  profile-notes verifier.

Missing top-level artifacts are `missing`. Present but malformed/unreadable JSON,
PNG, text, directories where files are required, missing nested objects, invalid
numeric/vector shapes, and missing referenced package files are `failed` checks,
never panics. A missing Midori report still skips dependent engine-report checks,
matching the legacy control flow, while summary/screenshots/profile notes remain
independently checked.

## Native self-tests

Tests create synthetic fixtures only beneath a `tempfile::TempDir` owned by the
test. They do not read or write the repository's real `target/` evidence, invoke
Python, launch Unity/Unreal, contact a provider, or use a GPU. The complete
fixture contains the exact legacy cardinalities (five maps, 22 prototype LODs,
26 nonempty JSON and binary scatter chunks totaling 222 records, two budget
profiles, two material recipes, two import recipes, eight foliage prototypes,
four valid nonblank PNGs, scaffolds, compile-stub/fake-editor reports, Unity and
Unreal reports, and completed profile notes).

The passing fixture proves all check statuses are passed and that every required
checker family emits checks. Mutation cases independently break identity,
checksum, PNG dimensions/content/filter/decompression, GLB-summary attributes,
map relationships, scatter parity/ranges, recipes/overlays/targets, budgets and
footprints, scaffold/compile-stub/fake-editor data, Unity fields, Unreal import
tasks/foliage/cull, screenshots, and notes. Each mutation asserts the named check
fails. Removing editor reports/screenshots/notes proves pending behavior without
creating false failures.

The CLI integration test runs the actual compiled binary in its own temp
directory and proves the passed/pending/failed exit matrix, override/default path
behavior, stdout/output byte identity, argument errors, and output errors. These
are verifier unit/integration fixtures, explicitly not real-engine acceptance.

## Preservation and completion boundary

Only `midori-cli` Rust sources, Cargo metadata, and native Rust tests are changed.
No Python or PowerShell file is modified or deleted. No existing local commit,
completed profile plan/spec, SDD ledger/report, real evidence, original
`B:/lab/Midori`, shared repository, remote branch, or PR is changed.

Completion means the native full-verifier library, synthetic self-tests, and CLI
adapter are independently reviewed and fresh Rust checks pass using
`CARGO_TARGET_DIR=C:/Users/jetha/AppData/Local/Temp/lab-midori-cargo-target` with
at most two Cargo jobs. Actual editor qualification and wrapper cutover remain
explicit gates.
