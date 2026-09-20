# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
See [docs/releasing.md](docs/releasing.md) for the versioning and
compatibility policy.

## [0.1.0] - unreleased

First tagged release of the unified `midori-*` engine.

### Added

- `midori-core`: procedural tree/nature generation engine producing glTF 2.0
  (Weber–Penn morphology, LOD chain generation with `MSFT_lod` markers,
  material descriptors, impostor cards, deterministic seeded generation).
- `midori-cli`: `midori` binary — generate/export trees from species TOML
  presets, `--json`/`--quiet` modes, generation budgets
  (`--max-stems/--max-leaves/--max-vertices/--max-triangles`, `0` = unbounded).
- `midori-wasm`: WASM binding (`wasm-pack --target web`) powering the
  workbench viewport; committed bundle for toolchain-free `npm ci && npm run build`.
- `midori-contracts`, `midori-ffi`, `midori-ui-domain`: generated contracts,
  C ABI surface, and shared UI-domain types.
- `apps/desktop` workbench (React + Tauri) — species/params/source/activity
  panels, PreviewPanel and NaturePanel 3D viewports.
- Species TOML schema: `docs/species-schema.md` +
  `schemas/species.schema.json` (JSON Schema for validation).
- Engine validation tooling: `scripts/*.ps1` (pwsh ≥7.4, editorless with
  `-SkipUnity -SkipUnrealEditor`), `verify_engine_evidence.py` evidence
  verifier, `validate_engine_imports.ps1` orchestrator.
- CI: workspace test matrix (self-hosted + GitHub-hosted Linux/Windows/macOS),
  MSRV leg (rustc 1.88), wasm reproducibility check, engine validation leg,
  browser smoke test of the built workbench.
- Release process: this changelog, `docs/releasing.md`, tag-driven
  `.github/workflows/release.yml` producing `midori` binaries
  (linux/windows/macos), the `@midori/wasm` npm tarball, and the workbench
  bundle on GitHub Releases.
