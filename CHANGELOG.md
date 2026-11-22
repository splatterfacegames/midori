# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
See [docs/releasing.md](docs/releasing.md) for the versioning and
compatibility policy.

## [Unreleased]

### Added

- **Rosette generator** (`generator.family = "rosette"`, `[rosette]`, `midori-core::rosette`): crowns of
  curved blade ribbons on an optional trunk that may fork into arms, a skirt of dead blades, optional
  `crown_boss` ("pineapple" palm crownshaft), creeping `runners` with daughter tufts, and
  `cylinder` / `raceme` / `cone` / `plume` / `panicle` flower spikes; blade cards `single`, `tuft`,
  `pinnate` (cleanly sky-gapped leaflets), and `fan` (palmate fan palm). Covers grass trees, cycads,
  kangaroo paw, sedge and rush clumps, spinifex, and feather & fan palms.
- **Inflorescences, dual organs & dedicated organ material** (`[inflorescence]`, `[secondary_inflorescence]`,
  `midori-core::organs`): `spike`, `nut`, `cluster`, and `berries` placed at shoot tips or along branches,
  thinned per LOD (retaining tall structural flower spears at LOD1) and baked into crown impostors.
  Optional `separate_material = true` routes organs onto `MaterialType::Organ` with dedicated roughness,
  and `[secondary_inflorescence]` supports simultaneous flowers + fruit (e.g. `wa_marri:flowering`).
- **Branching, multi-stem, half-bark & pruning extensions**:
  - Multi-stem / mallee trunks (`stems`, `stem_spread`, `stem_offset`, `stem_height_variation`), 4th branch
    level (`[branches.level4]`), araucaria whorl spacing variance & upturned tips (`tip_upturn`,
    `whorl_spacing_variance`), and true whorled leaf placement (`leaves.distribution = "whorl"`).
  - Half-barked trunks (`upper_bark_style`, `upper_bark_color`, `upper_bark_patch_colors`,
    `bark_transition_height`, `bark_transition_span`) routed to `MaterialType::UpperBark`.
  - Pruning & fire-recovery operators (`[pruning]`: `lopped`, `pollard`, `crown_lift`, `epicormic_count`).
  - Hero trunk geometry: `trunk.buttress`, `buttress_height`, `surface_root_reach`, organic curved
    `aerial_roots`, arched basal fire-hollow (`hollow_height`, `hollow_width`), and `char_height` / `char_strength`.
  - Opt-in LOD1 billboard area compensation (`canopy.billboard_lod_compensation`).
- **Leaf shapes & palettes**: `sickle`, `banksia`, `bull_banksia`, `bipinnate`, `branchlet`, `paperbark`,
  `broad_eucalypt`, and `trifoliate`, plus discolorous undersides (`leaf_underside_color`), new-growth
  flushes (`leaf_new_growth_color` / `leaf_new_growth_fraction`), and dead leaf accents (`leaf_dead_color` /
  `leaf_dead_fraction`).
- **Bark styles**: `stringy`, `tessellated`, `patchy`, `papery`, and `palm_diamond`, with optional
  `bark_patch_colors` and `bark_kino`.
- **States & interactive web workbench**: `[states.<name>]` overlays applied via `--state` in CLI and
  selectable live via the state switcher HUD in `apps/desktop` and the public `/demo/` web workbench;
  Three.js viewport updated for `UpperBark` and `Organ` materials; public site (`site/`) updated with
  golden-hour WA/Perth showcase (`site/wa-flora.jpg`) and full 68-species / 11-patch counts.
- **South-west Western Australian & 1980s–90s Perth flora (68 species · 11 nature patches)**:
  - All 29 native WA species (`presets/species/wa_*.toml`) tuned to authentic muted WA sclerophyll colours
    (guarded by automated HSV test `wa_palette.rs`), including `:street`, `:post_fire`, `:flowering`,
    `:out_of_flower`, `:coastal`, `:forest`, `:forked`, and `:burnt` states.
  - All 20 planted 1980s–90s Perth street, park and garden species (`presets/species/perth_*.toml`),
    including 8 new presets (`perth_cotton_palm`, `perth_queen_palm`, `perth_bottlebrush`,
    `perth_coral_tree`, `perth_illawarra_flame_tree`, `perth_sugar_gum`, `perth_oleander`,
    `perth_bougainvillea`) and `:pollarded` London plane / `:lopped` Queensland box states.
  - All 6 WA/Perth nature patches (`wa_banksia_woodland_floor`, `wa_jarrah_forest_floor`,
    `wa_coastal_dune`, `wa_paperbark_wetland_edge`, `perth_jacaranda_carpet`, and new
    `perth_verge_lawn`), exporting procedural `maps/ground_albedo.png` and validated by Unity + Unreal
    importer tests (`--from-manifest`).
  - Botany, sources, independent blind visual verification, and per-preset verdicts in `docs/wa-flora.md`;
    generator reference in `docs/botanical-generators.md`.

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
