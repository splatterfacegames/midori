# Midori

A procedural nature asset generator for real-time game engines. Midori generates trees today, and is expanding into soil tiles, grass, moss, and low groundcover for mobile and console workflows. Ships as a Rust library, a CLI, a browser/desktop workbench, and a C FFI for engine plugins.

## Features

- **Weber–Penn branching algorithm** — biologically-inspired recursive branching with configurable parameters
- **Multiple LOD levels** — configurable triangle budgets per level
- **Leaf systems** — polygon, cross-billboard, and billboard leaf geometries
- **Procedural material maps** — deterministic bark albedo+normal and leaf albedo+alpha cards, generated in-engine or overridden via file slots
- **Crown impostors** — far LODs bake the crown to crossed cards sampled from a front+side atlas
- **Pivot Painter 2.0** — wind animation vertex data compatible with Unreal Engine 5
- **glTF 2.0 export** — binary `.glb` or `.gltf` + `.bin` with per-submesh PBR materials, embedded PNG maps, and Midori metadata
- **Nature patches (WIP)** — deterministic soil height fields, grass density, moss/wetness/crack masks, and low-cost groundcover prototype LODs
- **Workbench app** — React/Tauri editor for species parameters, LOD previews, and export
- **Species presets** — Oak, Pine, Palm, Willow, and a Joshua tree prototype included

## Quick start

### Requirements

- Rust stable (edition 2024), `wasm32-unknown-unknown` target for engine changes
- Node ≥ 24, wasm-pack (only to rebuild `apps/desktop/src/wasm`)
- Tauri 2 prerequisites for `npm run desktop` (WebView2 on Windows)

### Workbench (interactive editor)

```bash
npm ci                  # installs deps, incl. the pinned jethaforge stack build
npm run dev             # http://127.0.0.1:1420
npm run desktop         # native Tauri window (dev)
npm run desktop:build   # packaged installer
```

The workbench edits species parameters, previews every LOD in 3D, manages
seeded variants, and exports `.glb`/`.gltf`. See [docs/workbench.md](docs/workbench.md).

```bash
cargo build --release
```

The binary will be at `target/release/midori`.

### CLI

```bash
cargo build --release -p midori-cli
midori generate -s presets/species/oak.toml -o tree.glb --seed 42 --textures
midori maps -s presets/species/oak.toml -o maps/
midori info -s presets/species/willow.toml
```

### Generate multiple variants

```bash
midori generate -s presets/species/pine.toml -n 10 --seed 42 -o forest/pine.glb
```

Output files will be named `pine_0.glb`, `pine_1.glb`, etc.

### Generate a nature package

```bash
midori nature -p presets/nature/temperate_forest_floor.toml -o forest_floor_midori
```

This writes `midori_nature.json`, baked maps, a preview tile GLB, prototype groundcover GLBs, and scatter data into the output directory.

### Tree Options

| Option | Description | Default |
|--------|-------------|---------|
| `-s, --species <FILE>` | Species TOML file | required |
| `-o, --output <PATH>` | Output path | `tree.glb` |
| `-n, --count <N>` | Number of variants | `1` |
| `--seed <N>` | Random seed | random |
| `--lod <all\|0\|1\|2\|3>` | LOD level(s) to export | `all` |
| `--format <glb\|gltf>` | Output format | `glb` |
| `--lod-preset <PRESET>` | LOD quality preset | species `[lod]` |
| `--textures` | Embed material maps in the export | off |
| `-v, --verbose` | Verbose output | off |

`midori maps` writes the species' material maps (`*_bark_albedo.png`,
`*_bark_normal.png`, `*_leaf_card.png`) without generating a tree — useful for
inspecting `[textures]` output or handing maps to an art pipeline.

### Nature Package Options

| Option | Description | Default |
|--------|-------------|---------|
| `-p, --patch <FILE>` | NaturePatch TOML file | required |
| `-o, --output <PATH>` | Output package directory | `nature_midori` |
| `--map-resolution <N>` | Baked map resolution | `64` |
| `--preview-resolution <N>` | Preview terrain mesh resolution | `32` |
| `--scatter-chunk-size <N>` | Scatter chunk size in world units | `8.0` |
| `--no-preview` | Skip `preview_tile.glb` | off |
| `--no-prototypes` | Skip prototype GLBs | off |
| `--no-scatter` | Skip all scatter outputs | off |
| `--no-scatter-binary` | Skip binary scatter buffers | off |
| `-v, --verbose` | Verbose output | off |

### Nature Package Validation

Use `midori validate-nature -i <package_dir>` to validate an exported package before handing it to Unity or Unreal import tooling. The validator reads `midori_nature.json`, verifies referenced maps and prototype meshes, checks scatter JSON, and validates every `midori.scatter.bin.v1` binary chunk header and record against the manifest.

Unity and Unreal helper importers live under `integrations/`:

- `integrations/unity/Editor/MidoriNaturePackageImporter.cs`
- `integrations/unreal/midori_nature_importer.py`

### Engine Validation Status

The current nature package and editorless import checks are automated through:

```bash
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/validate_engine_imports.ps1
python scripts/verify_engine_evidence.py --validation-root target/midori_engine_validation --allow-pending
```

The latest local evidence report is `pending` with 1610 passing checks, 7 missing editor-only artifacts, and 0 failed checks. Local Unity is installed but license-blocked for batch import, and Unreal Editor is not installed on this host.

To finish Phase 7, run the handoff bundle on a machine with licensed Unity and installed Unreal:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/export_engine_validation_handoff.ps1
cd target/midori_engine_validation_handoff
.\run_editor_validation.ps1 -UnityExe "C:\Program Files\Unity\Hub\Editor\<version>\Editor\Unity.exe" -UnrealEditorExe "C:\Program Files\Epic Games\UE_<version>\Engine\Binaries\Win64\UnrealEditor.exe" -AllowPending
```

Then complete `docs/validation/midori-nature-engine-profile-notes.md`, rerun `.\run_editor_validation.ps1 -VerifyOnly`, and ingest the returned bundle with `scripts/import_engine_validation_handoff.ps1`.

For a cold-start resume guide with current blockers, key files, and verification commands, see `docs/session-handoff.md`.

### Library

```rust
use midori_core::{Species, generate_tree, generate_lod_meshes, export_lod_meshes, ExportConfig};
use std::path::Path;

let species = Species::from_file(Path::new("oak.toml"))?;
let tree = generate_tree(&species, 12345);
let lods = generate_lod_meshes(&tree, &species);
export_lod_meshes(&lods, Path::new("tree.glb"), &ExportConfig::default())?;
```

## Project structure

```
midori/
├── crates/
│   ├── midori-core/     # Generation engine: species TOML -> tree -> LODs -> glTF
│   ├── midori-cli/      # `midori` command-line binary
│   ├── midori-wasm/     # wasm-bindgen bindings for the workbench webview
│   ├── midori-ffi/      # C ABI for engine plugins (parse/generate/export)
│   └── midori-desktop/  # Tauri 2 host (windowing + bounded file commands)
├── apps/desktop/       # React 19 + Vite workbench on the jethaforge stack
│   └── src/wasm/       # Committed wasm-pack build (`npm run wasm` to rebuild)
├── presets/species/    # Authoritative species TOML documents
├── presets/nature/     # Nature patch TOML documents
├── integrations/       # Unity/Unreal nature package importer helpers
├── docs/               # architecture.md, workbench.md, species-format.md
└── scripts/            # build-wasm.mjs, desktop.mjs, nature/evidence tooling
```

Architecture details live in [docs/architecture.md](docs/architecture.md).
The shared UI stack is
[`splatterfacegames/jethaforge`](https://github.com/splatterfacegames/jethaforge)
(pinned by git revision in `package.json`); the desktop app is Python-free —
build, test, package, and launch need only Rust, Node, and the Tauri toolchain.

## Species files

Trees are defined in TOML. The same documents drive the CLI, the WASM engine,
and the FFI surface:

```toml
[species]
name = "Oak"
scientific = "Quercus robur"
latin = "Quercus robur"
biome = "temperate"
tags = ["broadleaf", "deciduous", "spreading_crown"]

[generator]
family = "weber_penn"
notes = "Broadleaf tree using the current trunk/branch/leaf path."

[trunk]
height = 6.0
radius = 0.45
taper = 0.75
curve = 8.0
segments = 8

[branches.level1]
count = 6
length = 4.5
radius_ratio = 0.55
angle = 55.0
rotation = 137.5
gravity = -0.15

[crown]
shape = "spherical"
offset = 0.35

[leaves]
count = 3000
size = 0.12
geometry = "cross_billboard"

[materials]
bark = "oak_bark"
foliage = "oak_leaf"
notes = "Metadata placeholders only; texture/PBR asset pipeline is parked."

[[control_groups]]
key = "shape"
label = "Shape"

[[control_groups.controls]]
key = "height"
label = "Height"
parameter = "trunk.height"
min = 3.0
max = 18.0
step = 0.1

[lod]
preset = "balanced"

[platform]
target = "modern_pc"
```

See `presets/species/` for complete examples and
[docs/species-format.md](docs/species-format.md) for the full reference.

## Nature Patches

Midori's nature pipeline is in active development. The first slice adds `NaturePatch` TOML for soil, grass, moss, flowers, weeds, litter, wind, and mobile/console profile metadata, with deterministic Rust-side height/mask sampling, prototype groundcover meshes, chunked scatter data, and an initial package writer.

The texture/PBR asset generation pipeline is intentionally parked. Current nature outputs focus on geometry, masks, density, normal conventions, JSON and binary scatter data, and import metadata that can be consumed by Unity and Unreal pipelines.

Use `midori nature -p <patch.toml> -o <package_dir>` to write the current package layout, then `midori validate-nature -i <package_dir>` to run the package conformance gate.
In the web editor, choose `Forest Floor` from Presets to inspect the representative nature patch with Rust/WASM terrain, prototype, and scatter data.

See `docs/midori-nature-mobile-console-goal.md` and `presets/nature/temperate_forest_floor.toml`.

### Branch Radius And Taper

Branches default to direct ratio sizing for compatibility:

```toml
radius_ratio = 0.55
radius_model = "ratio"
taper = 0.7
taper_profile = "compound"
```

For terminal fork species, `radius_model = "pipe"` splits child radii across siblings using `pipe_exponent` so fork bases stay more physically coherent. Taper profiles are `linear`, `smooth`, `exponential`, and `compound`.

### Crown Shapes

`spherical` (oak, maple) · `conical` (pine, spruce) · `hemispherical` (palm) ·
`flame` (cypress) · `columnar` (poplar)

### Leaf geometries

- `polygon` — leaf-shaped mesh, no alpha test needed
- `cross_billboard` — two quads at 90°, quality/performance balance
- `billboard` — single quad, cheapest
- `none` — no leaves (lowest LOD)

### LOD presets

| Preset | Levels | Max triangles |
|--------|--------|---------------|
| `ultra` | 5 | 50,000 |
| `high_quality` | 4 | 30,000 |
| `balanced` | 3 | 8,000 |
| `mobile` | 3 | 3,000 |
| `minimal` | 2 | 1,500 |

`preset = "custom"` reads explicit `[[lod.levels]]` entries instead.

## Output format

Midori exports glTF 2.0 with one mesh per LOD level and attributes
`POSITION`, `NORMAL`, `TEXCOORD_0`, `TEXCOORD_1`, `COLOR_0`. Each submesh is
its own primitive: bark (albedo + normal), leaves (alpha-masked card), and
impostor (alpha-masked baked atlas) get separate PBR materials. Material maps
embed as PNGs when textures are enabled; impostor atlases always embed when a
LOD bakes one. Normal maps use the glTF OpenGL +Y convention.

### Pivot Painter data

| Attribute | Channel | Data |
|-----------|---------|------|
| `TEXCOORD_1.x` | U | Branch depth (0 = trunk, 1 = leaf) |
| `TEXCOORD_1.y` | V | Phase offset |
| `COLOR_0.xyz` | RGB | Pivot position (normalized) |
| `COLOR_0.w` | A | Stiffness (0 = flexible, 1 = rigid) |

## Development

```bash
cargo fmt --all -- --check                # formatting
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                    # engine, CLI, WASM, FFI
cargo test --manifest-path crates/midori-desktop/Cargo.toml  # desktop shell tests
npm run typecheck                         # tsc --noEmit
npm test                                  # vitest (model + mesh tests run the real WASM engine)
npm run build                             # production web build -> dist/
npm run wasm                              # rebuild committed WASM bundle
npm run desktop:build                     # Tauri package (NSIS on Windows)
```

Nature evidence tooling:

```bash
midori nature --patch presets/nature/temperate_forest_floor.toml --output target/nature_smoke
midori validate-nature --input target/nature_smoke --report target/nature_smoke/report.json
for t in scripts/test_*.py; do python3 "$t" || exit 1; done
```

`midori-desktop` is a standalone crate (excluded from the workspace) because it
consumes the private `jethaforge` git dependency; the engine workspace resolves
and builds with no private credentials required.

CI (`.github/workflows/ci.yml`) runs the Rust suite, a wasm32 compile check,
and the app typecheck/test/build on every PR.

### Engine plugin FFI

`midori-ffi` builds a `cdylib`/`staticlib` C API:
`midori_species_parse` → `midori_tree_generate` → `midori_tree_export_glb` →
`midori_tree_free` / `midori_species_free`. Handles are opaque pointers; strings
are `(ptr, len)` UTF-8 pairs.

## License

MIT OR Apache-2.0
