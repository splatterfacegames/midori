# Midori Architecture

Midori has four main ownership boundaries:

- `midori-core`: parses species TOML, generates deterministic tree data, builds meshes, generates LOD sets, and exports glTF/GLB.
- `midori-cli`: loads species files from disk, chooses generation/export options, and writes assets for command-line workflows.
- `midori-wasm`: wraps `midori-core` for the workbench, returning JS-friendly mesh arrays, stats, metadata, and GLB bytes.
- `midori-contracts` / `midori-ui-domain`: FlatBuffers tree-compute wire contract and source-bound composition helpers for UI adapters (mined from the ui-domain line; not yet consumed by the workbench).
- `apps/desktop`: React + Tauri workbench on the private jethaforge frontend stack. Loads species presets or TOML, calls the WASM package, previews generated LOD meshes and nature-patch scatter, and downloads GLB exports. `crates/midori-desktop` is the Tauri host crate, excluded from the workspace.

## Data Flow

1. A species TOML file is parsed into `Species`.
2. `generate_tree` dispatches by `species.generator.family`, then creates deterministic stems, leaves, bounds, and seed metadata through the selected family.
3. `generate_lod_meshes` converts the generated tree into one mesh per LOD.
4. `midori-wasm` serializes LOD mesh arrays for the editor preview.
5. The Three preview builds renderable geometry/materials from WASM mesh data.
6. Export calls build glTF/GLB from the same LOD mesh set.

## Determinism

Generation is seed-driven. Tests should treat fixed-seed stem counts, leaf counts, bounds, LOD vertex counts, and LOD triangle counts as compatibility signals. If an intentional generator change updates those values, the fixture tests should be updated in the same patch with a clear reason.

## Generator Boundary

Generator families own morphology-specific skeleton construction, but they must return the shared `Tree` output. Mesh building, LOD generation, WASM serialization, preview rendering, and export should not branch on plant family unless a future feature has a concrete downstream contract.

Shared radius and taper semantics belong at the species/generation boundary. Generator families may choose where branches attach and how many siblings they create, but branch radius models and taper profiles should stay schema-driven so Weber-Penn, dichotomous, and future families do not drift into incompatible controls.

## Mesh Boundary

The mesh builder owns family-agnostic surface quality: transported ring frames, exact seam duplicates, coincident side-surface normal averaging, base flare and branch-base swell, parent attachment rings at visible child offsets, cap normals, UVs, submesh assignment, and Pivot Painter attributes. Generator families should emit stable stems and foliage attachment data; they should not duplicate mesh-level seam, normal, or export concerns.

LOD filtering applies before attachment-ring generation. If a child branch is excluded from a lower LOD, the parent does not keep hidden junction rings for that child, and the parent tip is capped unless an included child actually continues from the tip.

Cap center vertices are excluded from coincident normal averaging so terminal cuts keep axial normals while side-surface seam vertices can be smoothed together.

## Export Boundary

Export tests should validate both builder-level JSON and real GLB bytes. The byte-level path must parse the GLB header, chunks, embedded JSON, materials, LOD node names, primitive attributes, accessors, buffer views, scene nodes, and Midori extras so exporter regressions are caught at the same boundary used by CLI and WASM downloads.

## Workbench Validation Boundary

`apps/desktop` is covered by `npm test` (vitest) for model/engine logic and `npm run build` for the Vite bundle; the wasm bundle in `apps/desktop/src/wasm/` is regenerated with `node scripts/build-wasm.mjs`. There is no browser smoke test on this line; visual checks of PreviewPanel/NaturePanel are manual.

## Parked Work

Texture/PBR asset generation is not part of the active architecture work. Species and material metadata may carry placeholders, but image generation, alpha extraction, and derived PBR/translucency maps remain parked.
