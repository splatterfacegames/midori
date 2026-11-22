# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
See [docs/releasing.md](docs/releasing.md) for the versioning and
compatibility policy.

## [Unreleased]

### Added

- **Rosette generator** (`generator.family = "rosette"`, `[rosette]`, `midori-core::rosette`): crowns of
  curved blade ribbons on an optional trunk that may fork into arms, a skirt of dead blades, and
  `cylinder` / `raceme` / `cone` flower spikes; blade cards `single`, `tuft` and `pinnate`. Covers grass
  trees, cycads, kangaroo paw, sedge and rush clumps, spinifex and feather palms.
- **Inflorescence and fruit organs** (`[inflorescence]`, `midori-core::organs`): `spike` (banksia flower
  spikes and old cones), `nut` (marri "honky nuts"), `cluster` (crown-colouring flower panicles) and
  `berries`, placed at shoot tips or along branches, thinned per LOD and baked into crown impostors.
  They draw from a 2x2 **foliage atlas** in the existing foliage material, so exports keep one bark and
  one foliage slot.
- **Leaf shapes** `sickle` (adult eucalypt), `banksia` (saw-toothed strap), `bipinnate` (fern-like,
  jacaranda and wattles) and `branchlet` (sheoak).
- **Bark styles** `stringy`, `tessellated`, `patchy` and `papery`, with optional `bark_patch_colors`
  (exposed patches, sheet-edge underlayers) and `bark_kino` (red gum streaks).
- **Trunk layers**: `trunk.buttress` (0 = plain column, 1 = unchanged, up to 3 = heavy fig/tingle
  buttresses) and `trunk.char_height` / `char_strength` (fire-blackened base, a `tint_ao` vertex-colour
  layer).
- **States**: `[states.<name>]` overlays in a species file, applied with `--state` on `generate`, `maps`
  and `info` (`Species::from_toml_with_state`), for seasonal, burnt and site-form variants of one species.
- **South-west Western Australian flora**: 29 native species presets (`presets/species/wa_*.toml`) and 12
  planted Perth street and park trees (`presets/species/perth_*.toml`), 11 of the 41 with states, plus five
  nature patches (`presets/nature/wa_*.toml`, `perth_jacaranda_carpet.toml`). Botany, sources and per-preset
  verdicts in `docs/wa-flora.md`; generator design in `docs/botanical-generators.md`.
- Nature patches: optional `soil.color` (`#rrggbb`) for the preview surface albedo.

### Compatibility

- Every feature above is opt-in. All 19 presets shipped in 0.2.0 and the five 0.2.0 nature patches export
  byte-identical files (428 files hashed before and after; see `docs/wa-flora.md`).
- `Tree` gains a public `organs` field, `LeafConfig` gains `blade_ribbon` and `foliage_atlas`, and
  `MeshConfig` gains `trunk_buttress`: code that builds these structs with literal syntax must add the
  new fields. Organs count against the leaf budget (`--max-leaves`).

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
