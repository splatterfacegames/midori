# Midori Goal Plan

Goal: evolve Midori from a working tree mesh generator into a production-oriented procedural tree and plant generator, using the SeedThree audit as guidance while keeping Midori's Rust core, Svelte editor, WASM boundary, and glTF export path coherent.

## Parked Explicitly

Texture/PBR asset pipeline work is parked.

Do not spend this goal on image-generation prompts, alpha extraction, PBR derivation scripts, translucency map generation, bark/leaf texture naming conventions beyond metadata placeholders, or generated asset curation. Those can become a later goal after the generator, LOD, export, and validation architecture are stronger.

## Outcomes

- Midori supports more than one morphology family without forcing every plant through the same trunk/branch/leaf schema.
- Species definitions carry enough structured metadata to drive generation, editor controls, LOD behavior, export naming, and future materials.
- Far LODs become real exported assets, not TODO flags.
- glTF export represents Midori assets cleanly: multiple materials, predictable LOD names, useful metadata, and valid engine-facing structure.
- Branch geometry, normals, wind data, and junctions improve without destabilizing deterministic generation.
- The editor becomes a practical validation harness for shape, LOD, wind, export, and scale.

## Progress So Far

Status as of 2026-07-05:

- The crate rename to Midori is complete across Rust crates, CLI binary, WASM package, generated bindings, UI labels, package metadata, and docs.
- Phase 0 is partly landed: `docs/architecture.md` documents ownership and data flow, fixture tests cover a representative oak preset plus a minimal species with fixed-seed structure, LOD counts, and bounds, and export validation now parses real GLB bytes to check the embedded JSON and binary buffer contract.
- Phase 1 is partly landed: species TOML now accepts `generator.family`, `species.latin`, `species.biome`, `species.tags`, material placeholders, and `control_groups`; CLI and WASM expose the metadata; invalid generator families are tested.
- Phase 2 is partly landed: `generate_tree` dispatches by `generator.family`, the `dichotomous` family emits the shared `Tree` output, and `presets/species/joshua_prototype.toml` plus the web preset exercise a terminal fork prototype through LOD, WASM, CLI, and GLB export.
- Phase 3 has multiple mesh-quality slices landed: the mesh builder now applies family-agnostic trunk base flare, branch-base swell, transported ring frames, exact seam duplicate vertices, coincident side-surface normal averaging, explicit parent attachment rings for visible child offsets, side-branch parent tip caps, axial terminal cap normals, and cap Pivot Painter data; species generation also supports opt-in pipe-model branch radius splitting plus configurable trunk/branch taper profiles. Tests cover radius effects, seam duplication, coincident normal smoothing, attachment ring insertion, LOD filtering for hidden child rings, pipe-model splitting, taper profile behavior, and finite/normalized mesh attributes.
- Phase 4 is partly landed: glTF export splits Midori submeshes into bark/foliage primitives, names LOD nodes and meshes predictably, writes Midori export metadata into glTF extras, and has byte-level GLB tests for LOD nodes, materials, primitive attributes, accessors, buffer views, scene nodes, and extras.
- Phase 5 has its first Rust-side slice landed: `crown_impostor = true` now emits a geometry-only crossed-card crown placeholder for far LODs, with no texture/PBR bake.
- Phase 6 is partly landed: the editor has manual/auto LOD preview controls, direct LOD buttons, per-LOD vertex/triangle/branch/leaf stats, a scale reference toggle, wind strength/speed/direction controls, sun azimuth/elevation/intensity controls, and export status with size, seed, and LOD chain. `web/scripts/browser-smoke.mjs` now runs a headless Chrome/Edge smoke test against the live editor, loads Oak and Joshua Prototype, forces LODs, toggles the scale reference, checks the export panel, and captures nonblank canvas screenshots.
- Verification run after the latest browser-validation slice: `npm run test:browser` passed and wrote `web/target/browser-smoke/oak-high.png` and `web/target/browser-smoke/joshua-high.png`; `npm run build` passed. The latest Rust verification also passed with `cargo fmt --check` and `cargo test`, including parsed-GLB validation. The latest WASM/CLI verification passed with `wasm-pack build crates\midori-wasm --target web --out-dir ..\..\web\src\lib\wasm`, direct Node/WASM smoke against `web/static/presets/oak.toml` and `web/static/presets/joshua_prototype.toml`, and CLI `generate --species presets\species\joshua_prototype.toml --seed 42`. The Svelte build still reports pre-existing warnings about unused exports/SvelteKit dependency exports and large chunks.

Still open:

- Deeper Phase 3 branch geometry work: visual validation of the new seam/junction behavior, optional lobes/ribs, and any further surface blending needed after browser inspection.
- Further Phase 6 UX refinement after manual inspection, plus the optional instanced forest/repeated-tree ring.
- Deeper Phase 2 botanical work for rosette-specific foliage and future cactus/pad-chain families.
- Broader Phase 7 morphology notes beyond the first oak note.
- Broader export polish such as optional `MSFT_lod` evaluation remains open, but core GLB/JSON structure is now parsed from exported bytes in tests.

## Phase 0: Baseline And Guardrails

Purpose: make sure future work has a stable target and catches regressions quickly.

- Keep the Midori rename complete across crates, CLI, WASM, web UI, docs, lockfiles, and generated wasm glue.
- Add a short architecture note describing the current data flow: TOML species -> Rust generation -> LOD meshes -> WASM -> Three preview -> GLB export.
- Add fixture-based tests for one representative species and one minimal species.
- Add snapshot-style checks for deterministic generation counts: stems, leaves, vertices, triangles, and bounds for fixed seeds.
- Add a small export validation test that parses generated GLB/JSON and checks materials, node names, accessors, and extras.

Acceptance:

- `cargo test` and `npm run build` stay green.
- A fixed seed produces stable structural counts unless a change intentionally updates fixtures.
- The architecture note names ownership boundaries between core, wasm, web, and export.

## Phase 1: Richer Species Schema

Purpose: let species describe morphology and editor behavior without hardcoding assumptions in UI components.

- Extend TOML schema with optional metadata:
  - `generator.family`: `weber_penn`, `dichotomous`, later `cactus`, `pad_chain`, or `custom`.
  - `species.latin`, `species.biome`, `species.tags`.
  - `materials.bark`, `materials.foliage`, and `materials.notes` as placeholders only. No texture pipeline implementation in this goal.
  - `controls` or `control_groups` that map friendly editor sliders to raw generator parameters.
- Keep backward compatibility for current oak, pine, palm, and willow presets.
- Add validation errors for unsupported generator families and incompatible sections.
- Update README examples and docs to describe the new schema incrementally.

Acceptance:

- Existing presets load unchanged.
- New optional metadata round-trips through parsing and can be queried by CLI/WASM.
- Invalid generator family or incompatible sections produce clear errors.

## Phase 2: Generator Family Boundary

Purpose: make the core ready for plants that are not ordinary branching trees.

- Introduce a generator abstraction in `midori-core`, such as a `GeneratorFamily` enum plus dispatch from `generate_tree`.
- Keep the existing Weber-Penn-style path as the default implementation.
- Separate shared output types from family-specific internals:
  - skeleton/stem data usable by mesh builder
  - foliage attachment points
  - bounds
  - wind/pivot attributes
- Define a second-family interface before implementing a full plant:
  - stochastic fork grammar
  - tube sweep parameters
  - terminal rosette attachment points
  - support for same-family LOD reduction
- Add a small "prototype plant" preset that exercises the new family with simple geometry before botanical detail.

Acceptance:

- Existing tree generation is behaviorally unchanged for current presets.
- A non-Weber family can generate a valid mesh, LOD set, WASM preview, and GLB.
- Family-specific code is isolated enough that adding cactus or rosette behavior does not add special cases throughout the editor.

## Phase 3: Branch Geometry And Normals

Purpose: improve visual quality where the current output most obviously looks procedural.

- Upgrade branch radius rules:
  - pipe-model child radius option
  - base flare on trunks
  - configurable taper profiles
  - optional lobes/ribs for future succulent/cactus geometry
- Improve ring frame continuity:
  - rotation-minimizing or transported frames along curved stems
  - stable seam vertex handling
  - less twist at high curvature
- Improve junction appearance:
  - collar swell tuning
  - child-base flare options
  - normal averaging for coincident seam and junction vertices
- Keep Pivot Painter data coherent across stems and branches.
- Add geometry tests for valid indices, bounds, UV range, normal sanity, and deterministic vertex/index counts.

Acceptance:

- Trunks and branches have fewer visible seam and junction artifacts in preview.
- No invalid indices or NaN vertex attributes are produced.
- Wind metadata remains present and normalized.

## Phase 4: glTF Export Upgrade

Purpose: make exported files clean enough for engine and DCC workflows.

- Split glTF primitives by Midori submesh instead of exporting everything as one material.
- Preserve bark and leaf material assignment across all LODs.
- Rename exported nodes and meshes predictably:
  - `<species>_LOD0`
  - `<species>_LOD1`
  - `<species>_LOD2`
  - `<species>_LOD3` when present
- Add export metadata:
  - Midori version
  - species name and scientific name when available
  - seed
  - LOD screen thresholds
  - Pivot Painter metadata
- Evaluate adding `MSFT_lod` after clean LOD node naming lands. Prefer simple, valid glTF first.
- Keep export tests focused on generated JSON structure and binary validity.

Acceptance:

- Bark and leaves export as separate material primitives.
- LOD nodes can be discovered by suffix convention.
- Export tests prove the generated GLB/GLTF is structurally valid.

## Phase 5: Real Far LODs

Purpose: replace `crown_impostor` TODO behavior with practical low-cost geometry.

- First implement a simple geometry-only far LOD in Rust:
  - trunk or major branches only
  - coarse crown/card placeholder generated from bounds
  - no texture/PBR bake
- Then add editor-side billboard/impostor baking as a separate capability:
  - bake from current LOD into crossed cards
  - store albedo-like flat color and alpha only at first
  - no derived PBR maps in this goal
- Add UI controls for LOD preview and switch thresholds.
- Make export include the far LOD when generated.

Acceptance:

- `crown_impostor = true` produces non-empty LOD geometry.
- The editor can force-preview each LOD.
- Export includes the full LOD chain with predictable names.

## Phase 6: Editor Validation Harness

Purpose: use the app to catch generator, wind, LOD, and export problems quickly.

- Add scale reference toggle.
- Add sun direction/intensity controls.
- Add wind strength/speed controls wired to existing shader uniforms.
- Show per-LOD stats: vertices, triangles, branch count, leaf count.
- Add forced LOD preview and auto LOD mode.
- Add export status: size, selected seed, selected LOD chain.
- Add a headless browser smoke test that validates nonblank Oak and Joshua previews plus core LOD/export controls.
- Add a small instanced forest or repeated-tree ring only after single-tree LOD preview is stable.

Acceptance:

- A user can inspect one tree at each LOD, with wind on/off, before export.
- Stats update when seed, species, parameters, or LOD changes.
- Browser smoke validation proves that representative species render nonblank preview canvases.
- The UI remains focused on generation work, not marketing content.

## Phase 7: Morphology Notes As Source Data

Purpose: make species creation repeatable and grounded without building the parked texture pipeline.

- Add `docs/morphology/` with one markdown note per target species or plant family.
- Each note should capture:
  - crown silhouette
  - trunk/bark structural traits
  - branching habit
  - foliage placement and geometry
  - height/spread ranges
  - generator family recommendation
  - parameters to tune first
- Link presets back to their morphology note.
- Use notes to guide generator parameters and editor controls.

Acceptance:

- Adding a species starts with a morphology note, then a preset.
- Notes do not include texture-generation work beyond descriptive placeholders.
- At least one existing preset gets a morphology note and cross-reference.

## Sequence

1. Phase 0: baseline tests and architecture note.
2. Phase 1: schema extensions with backward compatibility.
3. Phase 4: glTF material splitting and LOD naming, because export correctness is a current product surface.
4. Phase 3: branch geometry and normal quality improvements.
5. Phase 5: real far LODs, starting with geometry-only placeholders.
6. Phase 6: editor validation controls.
7. Phase 2: second generator family boundary and prototype.
8. Phase 7: morphology notes, then use them to drive more species.

Phase 2 can move earlier if the next target species is Joshua tree, saguaro, yucca, or another non-tree morphology. For ordinary trees, export and LOD work should land first.

## Risks

- Renaming crates and wasm artifacts can leave stale generated files; keep a case-insensitive leftover-name search as a standard check.
- glTF LOD extensions vary by engine. Prefer clear node names and valid core glTF before adding optional extensions.
- Editor-side impostor baking can become browser-specific. Keep the first far LOD simple and geometry-only before attempting richer bakes.
- New generator families can leak special cases into shared code. Keep family dispatch explicit and outputs shared.
- Determinism can break if new features consume RNG in existing traversal paths. Use stable fixture tests before broad generator changes.

## Definition Of Done For This Goal

- Current tree presets still work.
- At least one richer species schema example exists.
- Export has correct material primitives and LOD naming.
- `crown_impostor` produces real geometry or a documented editor-baked card path.
- The editor can preview LODs and basic environmental validation controls.
- A second generator family can generate a minimal valid plant mesh, even if botanical polish comes later.
- Texture/PBR asset pipeline remains parked and has not absorbed implementation time.
