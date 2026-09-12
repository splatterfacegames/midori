# Grove

A procedural tree generator for real-time game engines. Generates 3D tree
meshes with LOD support and Pivot Painter wind data, and exports to glTF 2.0.
Ships as a Rust library, a CLI, a browser/desktop workbench, and a C FFI for
engine plugins.

## Features

- **Weber–Penn branching algorithm** — biologically-inspired recursive branching with configurable parameters
- **Multiple LOD levels** — configurable triangle budgets per level
- **Leaf systems** — polygon, cross-billboard, and billboard leaf geometries
- **Procedural material maps** — deterministic bark albedo+normal and leaf albedo+alpha cards, generated in-engine or overridden via file slots
- **Crown impostors** — far LODs bake the crown to crossed cards sampled from a front+side atlas
- **Pivot Painter 2.0** — wind animation vertex data compatible with Unreal Engine 5
- **glTF 2.0 export** — binary `.glb` or `.gltf` + `.bin` with per-submesh PBR materials and embedded PNG maps
- **Workbench app** — React/Tauri editor for species parameters, LOD previews, and export
- **Species presets** — Oak, Pine, Palm, and Willow included

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

### CLI

```bash
cargo build --release -p grove-cli
grove generate -s presets/species/oak.toml -o tree.glb --seed 42 --textures
grove maps -s presets/species/oak.toml -o maps/
grove info -s presets/species/willow.toml
```

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

`grove maps` writes the species' material maps (`*_bark_albedo.png`,
`*_bark_normal.png`, `*_leaf_card.png`) without generating a tree — useful for
inspecting `[textures]` output or handing maps to an art pipeline.

### Library

```rust
use grove_core::{Species, generate_tree, generate_lod_meshes, export_lod_meshes, ExportConfig};
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
│   ├── grove-core/     # Generation engine: species TOML -> tree -> LODs -> glTF
│   ├── grove-cli/      # `grove` command-line binary
│   ├── grove-wasm/     # wasm-bindgen bindings for the workbench webview
│   ├── grove-ffi/      # C ABI for engine plugins (parse/generate/export)
│   └── grove-desktop/  # Tauri 2 host (windowing + bounded file commands)
├── apps/desktop/       # React 19 + Vite workbench on the jethaforge stack
│   └── src/wasm/       # Committed wasm-pack build (`npm run wasm` to rebuild)
├── presets/species/    # Authoritative species TOML documents
├── docs/               # architecture.md, workbench.md, species-format.md
└── scripts/            # build-wasm.mjs, desktop.mjs, make-icon.mjs
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

[trunk]
height = 6.0
radius = 0.45
taper = 0.75
curve = 8.0
segments = 8

[branches.level1]
count = 6
length = 4.5
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

[lod]
preset = "balanced"

[platform]
target = "modern_pc"
```

See `presets/species/` for complete examples and
[docs/species-format.md](docs/species-format.md) for the full reference.

### Crown shapes

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

Grove exports glTF 2.0 with one mesh per LOD level and attributes
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
cargo test --workspace                    # 140+ tests (engine, CLI, WASM, FFI)
cargo test --manifest-path crates/grove-desktop/Cargo.toml  # desktop shell tests
npm run typecheck                         # tsc --noEmit
npm test                                  # vitest (model + mesh tests run the real WASM engine)
npm run build                             # production web build -> dist/
npm run wasm                              # rebuild committed WASM bundle
npm run desktop:build                     # Tauri package (NSIS on Windows)
```

`grove-desktop` is a standalone crate (excluded from the workspace) because it
consumes the private `jethaforge` git dependency; the engine workspace resolves
and builds with no private credentials required.

CI (`.github/workflows/ci.yml`) runs the Rust suite, a wasm32 compile check,
and the app typecheck/test/build on every PR.

### Engine plugin FFI

`grove-ffi` builds a `cdylib`/`staticlib` C API:
`grove_species_parse` → `grove_tree_generate` → `grove_tree_export_glb` →
`grove_tree_free` / `grove_species_free`. Handles are opaque pointers; strings
are `(ptr, len)` UTF-8 pairs.

## License

MIT OR Apache-2.0
