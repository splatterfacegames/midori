# Release process

How Midori versions, tags, and ships artifacts.

## Distribution channels

| Artifact | Channel | Produced by |
|---|---|---|
| `midori` CLI binary (x86_64 linux, x86_64/aarch64 macOS, x86_64 windows) | GitHub Releases | `.github/workflows/release.yml` `cli` job |
| `@midori/wasm` npm tarball (wasm-opt `-O2`, size recorded) | GitHub Releases / npm | `wasm-package` job |
| Workbench static bundle | GitHub Releases | `workbench` job |
| `midori-core`, `midori-contracts`, `midori-ui-domain` crates | crates.io (manual) | `crates-dry-run` job + `cargo publish` |

## Versioning

- SemVer `MAJOR.MINOR.PATCH`, declared once in `[workspace.package].version`
  (Cargo.toml) and mirrored in the root `package.json`. All member crates
  inherit `version.workspace = true`.
- **Pre-1.0**: minor bumps may break public APIs; patch bumps do not.
- A git tag `vX.Y.Z` must equal the workspace version — the `version-check`
  job hard-fails otherwise (`node scripts/check-version.mjs vX.Y.Z` runs the
  same check locally).

## Compatibility guarantees

Three versioned surfaces get explicit guarantees; bump the **format version**,
not just the package version, on any breaking change:

- **Species TOML schema** (`species/*.toml`, `schemas/species.schema.json`):
  additive fields are minor bumps; renaming or changing field semantics is a
  schema-version bump and must be noted in the changelog.
- **`midori.scatter.bin.v1` binary format**: the `v1` in the name is the
  contract — readers must accept all `v1` payloads; a breaking change lands as
  `v2` alongside continued `v1` reads for at least one major version.
- **Nature package manifest** (`docs/schema/midori_nature.schema.json`):
  manifests carry a `schemaVersion`; unknown future fields are ignored by
  readers (forward compatibility), removals/renames require a version bump.

## Release profile

`Cargo.toml` `[profile.release]` sets `lto = true`, `codegen-units = 1`,
`opt-level = 3` — maximum optimization plus deterministic wasm output (CI's
`wasm-repro` leg rebuilds and byte-compares the committed bundle). The
release workflow runs `wasm-opt -O2` on the published npm tarball and reports
before/after size; the committed dev bundle is intentionally left unoptimized
so the reproducibility check stays a pure wasm-pack comparison.

## Cutting a release

1. Bump `[workspace.package].version` and root `package.json` `version`.
2. Add a `## [X.Y.Z] - YYYY-MM-DD` section to CHANGELOG.md (the workflow uses
   that section as the GitHub Release notes).
3. Merge to `master`, wait for green CI.
4. Tag the merge commit: `git tag -a vX.Y.Z -m "Midori vX.Y.Z" && git push origin vX.Y.Z`.
5. `release.yml` runs: version check → binary matrix → wasm tarball →
   workbench bundle → GitHub Release with all assets attached.
6. crates.io publish is manual once a token is set (`CARGO_REGISTRY_TOKEN`
   secret; the workflow runs `cargo publish --dry-run` when it is):
   `cargo publish -p midori-contracts && cargo publish -p midori-core && cargo publish -p midori-ui-domain`
   (publish order: contracts → core → ui-domain, due to dependencies).
