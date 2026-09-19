# Architecture

Midori is a procedural nature asset generator (Weber–Penn model plus the
nature patch pipeline) with a Rust engine, a CLI, a desktop workbench, and a
C FFI surface for engine plugins.

```
midori/
├── crates/
│   ├── midori-core/      # Generation engine: species/nature TOML -> tree/terrain -> LOD meshes -> glTF
│   ├── midori-cli/       # `midori` binary: generate / info / maps / nature / validate-nature
│   ├── midori-wasm/      # wasm-bindgen bindings used by the workbench webview
│   ├── midori-ffi/       # C API for engine plugins (cdylib/staticlib)
│   └── midori-desktop/   # Tauri 2 host: windowing + bounded file commands
│                        #   (standalone crate — excluded from the workspace so the
│                        #    engine builds without the private jethaforge dep)
├── apps/desktop/        # React 19 + Vite workbench UI (jethaforge stack)
│   └── src/wasm/        # Committed wasm-pack build of midori-wasm
├── presets/species/     # Authoritative species TOML documents
├── presets/nature/      # NaturePatch TOML documents (soil, groundcover, scatter)
├── integrations/        # Unity/Unreal nature package importer helpers
├── docs/                # architecture.md, workbench.md, species-format.md, validation/
└── scripts/             # build-wasm.mjs, desktop.mjs, make-icon.mjs, nature/evidence tooling
```

## The one engine rule

There is exactly one generation engine: `midori-core`. Every surface consumes
the same code:

- `midori-cli` links it natively.
- `midori-wasm` compiles it to WebAssembly for the webview.
- `midori-ffi` exposes it over C ABI.
- `midori-desktop` deliberately does **not** embed the engine — the desktop app
  runs the same `midori-wasm` build inside the Tauri webview, so preview and
  export are byte-identical between browser dev and the packaged app.

## Document flow

`species.toml` is the authoritative document. In the workbench:

```
params panel ──edit──> species JSON ──fromJson──> engine validates
source panel ──edit──> TOML text  ──fromToml──> engine validates
        ▲                                          │
        └────────── toJson() / toToml() ◄──────────┘
                         then regenerate + re-render
```

Edits in either view are validated by the engine before being committed; the
other view is re-synced from the engine output. There is no JS-side TOML
parser and no duplicated parameter list outside `apps/desktop/src/species.ts`
(which only describes *how to render* fields, not their meaning).

## Data flow (generation)

1. A species TOML file is parsed into `Species`.
2. `generate_tree` dispatches by `species.generator.family`, then creates
   deterministic stems, leaves, bounds, and seed metadata through the
   selected family.
3. `generate_lod_meshes` converts the generated tree into one mesh per LOD.
4. `midori-wasm` serializes LOD mesh arrays for the editor preview.
5. Export calls build glTF/GLB from the same LOD mesh set.

## Determinism

Generation is seed-driven. Fixed-seed stem counts, leaf counts, bounds, LOD
vertex counts, and LOD triangle counts are compatibility signals, guarded by
`crates/midori-core/tests/fixtures.rs`. If an intentional generator change
updates those values, the fixture tests are updated in the same patch with a
clear reason.

## Generator boundary

Generator families own morphology-specific skeleton construction, but they
must return the shared `Tree` output. Mesh building, LOD generation, WASM
serialization, preview rendering, and export do not branch on plant family
unless a future feature has a concrete downstream contract.

Shared radius and taper semantics belong at the species/generation boundary.
Generator families may choose where branches attach and how many siblings
they create (`sibling_count` feeds `branch_base_radius`), but branch radius
models and taper profiles stay schema-driven so Weber-Penn, dichotomous, and
future families do not drift into incompatible controls.

## Mesh boundary

The mesh builder owns family-agnostic surface quality: transported ring
frames, exact seam duplicates, coincident side-surface normal averaging, base
flare and branch-base swell, parent attachment rings at visible child
offsets, cap normals, UVs, submesh assignment, and Pivot Painter attributes.
Generator families emit stable stems and foliage attachment data; they do
not duplicate mesh-level seam, normal, or export concerns.

LOD filtering applies before attachment-ring generation. If a child branch is
excluded from a lower LOD, the parent does not keep hidden junction rings for
that child, and the parent tip is capped unless an included child actually
continues from the tip.

Cap center vertices are excluded from coincident normal averaging so terminal
cuts keep axial normals while side-surface seam vertices can be smoothed
together.

## LOD previews

`MidoriGenerator.generate(seed)` produces one `LodMesh` per level in the
species' `[lod]` config (preset or custom levels). Each LOD carries flat
vertex arrays plus `submeshes` material ranges. The viewport splits each LOD
into one `MeshDescriptor` per material (bark / leaves / impostor) sharing the
same vertex buffers — cheap switching, no duplicated geometry.

Levels flagged `crown_impostor` replace per-leaf geometry with two crossed
quads sampling a baked front+side atlas (`impostor.rs` — an in-engine
z-buffered rasterizer over the same leaf cards and cut branches the nearer
LODs draw). The atlas rides on the `LodMesh` as a `GeneratedMap`
(`impostor_atlas` = `png` + `rgba`): the workbench inspects the PNG, the
exporter embeds it, and the RGBA8 copy uploads straight to the viewport.

The viewport preview binds the generated maps on each material's
`MeshDescriptor` (`uvs` + RGBA8 `map`, `alphaTest` cutout for leaves and
impostors — the `rgba` half of each `GeneratedMap`). Bark V coordinates are
metres along the stem, so bark binds `mapWrap: 'repeat'`; all bound maps
use `mapFilter: 'linear'` for trilinear minification. When maps are
absent the viewport falls back to flat per-material colors, and the maps
remain inspectable in the Objects panel materials strip.

## Material maps

`textures.rs` generates deterministic bark albedo, bark normal (OpenGL +Y),
and leaf albedo+alpha card PNGs from `[textures]` params — no external art
dependency. `TextureSet::resolve(species, dir)` lets native hosts substitute
file-slot paths (relative to the species document); WASM always generates.
`generateMaps()` bakes the `TextureSet` once and returns all three maps as
`GeneratedMap`s — PNG bytes for inspection plus raw RGBA8 for direct GPU
upload; exports embed the PNGs when requested.

## Nature pipeline

`nature.rs` owns the NaturePatch path: `NaturePatch` TOML (from
`presets/nature/`) describes soil height fields, grass/moss/flower/weeds/
litter layers, wind, and mobile/console profile metadata. The engine produces
deterministic terrain fields and masks, groundcover prototype LODs, chunked
scatter data (JSON + `midori.scatter.bin.v1` binary), and a
`midori_nature.json` manifest.

- `midori nature` writes a package directory; `midori validate-nature` runs
  the package conformance gate against the manifest.
- `MidoriNatureGenerator.preview()` and `generate_nature_preview_from_toml()`
  expose terrain, prototype, scatter, and stats data to the workbench.
- `integrations/` carries Unity (`MidoriNaturePackageImporter.cs`) and Unreal
  (`midori_nature_importer.py`) import helpers.

The texture/PBR asset generation pipeline for nature assets is intentionally
parked; current outputs focus on geometry, masks, density, normal
conventions, scatter data, and import metadata.

## Engine validation and evidence tooling

`scripts/` holds the nature evidence pipeline: `test_*.py` smoke checks run
under plain `python3`, and `verify_engine_evidence.py` scores a validation
root produced by the PowerShell editor-validation bundle
(`validate_engine_imports.ps1`, `export_engine_validation_handoff.ps1`,
`import_engine_validation_handoff.ps1`). The strict verifier still awaits
real licensed-editor evidence; see `docs/session-handoff.md` and
`docs/validation/` for status.

## Export flow

- **glb**: `export_glb(seed, embed_textures)` → single binary file.
- **gltf**: `exportGltf(seed, binName, embed_textures)` → `.gltf` JSON +
  `.bin` pair (images ride inside the `.bin` via bufferViews).
- Each submesh becomes its own primitive with the right material — bark,
  leaves (alpha-masked), impostor (alpha-masked, atlas-textured). Node and
  mesh names follow `<species>_LODn`; glTF extras carry Midori export
  metadata (species, scientific name, seed, LOD screen heights).
- Browser host: files download via anchor.
- Tauri host: `plugin-dialog` picks the destination, then the `save_export`
  command writes raw bytes (`[u32 path_len][path][payload]` frame).

Export tests validate both builder-level JSON and real GLB bytes. The
byte-level path parses the GLB header, chunks, embedded JSON, materials, LOD
node names, primitive attributes, accessors, buffer views, scene nodes, and
Midori extras so exporter regressions are caught at the same boundary used
by CLI and WASM downloads.

## Host boundaries

The Tauri shell exposes two commands only:

- `read_text_file(path)` — bounded to 1 MiB, UTF-8 (species import).
- `save_export(raw frame)` — bounded to 512 MiB (glTF export).

Panel detachment to real native windows is provided by the stack's
`tools-frontend-host-tauri` plugin; the browser host falls back to
`createBrowserWindowHost`.
