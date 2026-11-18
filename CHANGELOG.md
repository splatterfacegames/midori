# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
See [docs/releasing.md](docs/releasing.md) for the versioning and
compatibility policy.

## [0.2.0] - 2026-11-18

### Added

- **Bistro & ornamental vegetation suite** (`midori-core::ornamental`, `midori-core::bistro_cards`):
  - `topiary` generator family with signed-distance sheared box, curved-arc, sphere, cylinder, and cone hulls (`bistro_box_hedge`, `bistro_curved_hedge`, `bistro_topiary_ball`).
  - `floral_shrub` generator family (`paris_flower_mound`, `paris_hanging_cascade`) with 2x2 high-resolution single-organ Pansy/*Viola* atlases, 3D-cupped zygomorphic corollas on curved gooseneck peduncles, and shingled herringbone *Vinca* cascades.
  - `climbing_vine` (`english_ivy`) and `tropical_clump` (`bird_of_paradise`) generator families, plus `linden` and `italian_cypress` street/courtyard trees (19 built-in species presets total).
- **Tiled Nature Patch biome packages** (`midori nature`, `midori validate-nature`):
  - Seamless 16-bit terrain heightfields, soil/moss/wetness/crack masks, multi-species groundcover LOD prototypes, and chunked `midori.scatter.bin.v1` instances across 5 presets (`pampas_meadow`, `alpine_meadow`, `flowering_meadow`, `temperate_forest_floor`, `arid_scrub`).
- **Procedural PBR textures & 3D botanical organs**:
  - Deterministic 1024px bark height/albedo/normal synthesis, RGB alpha-dilated leaf/frond/sprig cards, 3D gullwing palm frond ribbons, 3D tropical paddle ribbons, reaction-wood trunk bow, root buttress flares, and dedicated ARPG/RTS top-down presets (`topdown_broadleaf`, `topdown_conifer`, `topdown_shrub`).
- **Desktop installers & public site**:
  - Cross-platform Tauri 2 desktop workbench installers (Windows NSIS `.exe`, macOS `.dmg`, Linux `.deb`) and pre-bundled `presets/` in all `midori` CLI archives on GitHub Releases.
  - Public product site and live browser workbench at `https://midori.splatterfacegames.com`.

## [0.1.0] - 2026-09-21

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
