# Midori

A procedural nature asset generator for real-time game engines. Midori generates trees today, and is expanding into soil tiles, grass, moss, and low groundcover for mobile and console workflows.

## Features

- **Weber-Penn branching algorithm** - Biologically-inspired recursive branching with configurable parameters
- **Multiple LOD levels** - Automatic generation of level-of-detail meshes with configurable triangle budgets
- **Leaf systems** - Polygon, cross-billboard, and billboard leaf geometries
- **Pivot Painter 2.0** - Wind animation vertex data compatible with Unreal Engine 5
- **glTF 2.0 export** - Binary (.glb) or JSON (.gltf) output with split bark/foliage material slots and Midori metadata
- **Nature patches (WIP)** - Deterministic soil height fields, grass density, moss/wetness/crack masks, and low-cost groundcover prototype LODs
- **Species presets** - Oak, Pine, Palm, Willow, and a Joshua tree prototype included

## Installation

### From source

```bash
git clone https://github.com/jethac/midori.git
cd midori
cargo build --release
```

The binary will be at `target/release/midori`.

## Usage

### Generate a tree

```bash
midori generate -s presets/species/oak.toml -o tree.glb
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

### Show species information

```bash
midori info -s presets/species/willow.toml
```

### Tree Options

| Option | Description | Default |
|--------|-------------|---------|
| `-s, --species <FILE>` | Species TOML file | required |
| `-o, --output <PATH>` | Output path | `tree.glb` |
| `-n, --count <N>` | Number of variants | `1` |
| `--seed <N>` | Random seed | random |
| `--lod <all\|0\|1\|2\|3>` | LOD level(s) to export | `all` |
| `--format <glb\|gltf>` | Output format | `glb` |
| `--lod-preset <PRESET>` | LOD quality preset | `balanced` |
| `-v, --verbose` | Verbose output | off |

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

### LOD Presets

| Preset | Levels | Max Triangles |
|--------|--------|---------------|
| `ultra` | 5 | 50,000 |
| `high_quality` | 4 | 30,000 |
| `balanced` | 3 | 8,000 |
| `mobile` | 3 | 3,000 |
| `minimal` | 2 | 1,500 |

## Species Files

Trees are defined in TOML files. Example:

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
```

See `presets/species/` for complete examples.

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

- `spherical` - Round, spreading crown (oak, maple)
- `conical` - Triangular profile (pine, spruce)
- `hemispherical` - Dome-shaped (palm)
- `flame` - Pointed oval (cypress)
- `columnar` - Tall and narrow (poplar)

### Leaf Geometries

- `polygon` - Actual leaf-shaped mesh, no alpha testing needed
- `cross_billboard` - Two quads at 90°, good balance of quality/performance
- `billboard` - Single quad, cheapest option
- `none` - No leaves (used for lowest LOD)

## Output Format

Midori exports glTF 2.0 with:

- Multiple meshes (one per LOD level)
- Predictable node and mesh names such as `<species>_LOD0`
- Separate bark and foliage material primitives when both submeshes are present
- Vertex attributes: `POSITION`, `NORMAL`, `TEXCOORD_0`, `TEXCOORD_1`, `COLOR_0`
- Midori export metadata in glTF extras, including species, seed, and LOD thresholds
- Pivot Painter data encoded in `TEXCOORD_1` and `COLOR_0`
- Geometry-only far LOD crown cards when `crown_impostor` is enabled

### Pivot Painter Data

For wind animation in game engines:

| Attribute | Channel | Data |
|-----------|---------|------|
| `TEXCOORD_1.x` | U | Branch depth (0=trunk, 1=leaf) |
| `TEXCOORD_1.y` | V | Phase offset |
| `COLOR_0.xyz` | RGB | Pivot position (normalized) |
| `COLOR_0.w` | A | Stiffness (0=flexible, 1=rigid) |

## Project Structure

```
midori/
├── crates/
│   ├── midori-core/    # Core generation library
│   ├── midori-cli/     # Command-line interface
│   ├── midori-wasm/    # WebAssembly bindings (WIP)
│   └── midori-ffi/     # C FFI for engine plugins (WIP)
└── presets/
    ├── species/        # Species TOML files
    └── nature/         # Nature patch TOML files
```

## Development Checks

Core checks:

```bash
cargo fmt --check
cargo test
```

Web checks:

```bash
cd web
npm run build
```

The browser smoke test requires the editor to be running, then validates Oak and Joshua Prototype previews through Chrome or Edge:

```bash
npm run dev
npm run test:browser
```

Set `MIDORI_VISUAL_URL` if the editor is running on a different URL.

## Library Usage

```rust
use midori_core::{Species, generate_tree, generate_lod_meshes, export_lod_meshes, ExportConfig};
use std::path::Path;

let species = Species::from_file(Path::new("oak.toml")).unwrap();
let tree = generate_tree(&species, 12345);
let lods = generate_lod_meshes(&tree, &species);

export_lod_meshes(&lods, Path::new("tree.glb"), &ExportConfig::default()).unwrap();
```

## License

MIT
