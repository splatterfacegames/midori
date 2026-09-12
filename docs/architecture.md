# Architecture

Grove is a procedural tree generator (Weber–Penn model) with a Rust engine,
a CLI, a desktop workbench, and a C FFI surface for engine plugins.

```
midori/
├── crates/
│   ├── grove-core/      # Generation engine: species TOML -> tree -> LOD meshes -> glTF
│   ├── grove-cli/       # `grove` binary: generate / info
│   ├── grove-wasm/      # wasm-bindgen bindings used by the workbench webview
│   ├── grove-ffi/       # C API for engine plugins (cdylib/staticlib)
│   └── grove-desktop/   # Tauri 2 host: windowing + bounded file commands
│                        #   (standalone crate — excluded from the workspace so the
│                        #    engine builds without the private jethaforge dep)
├── apps/desktop/        # React 19 + Vite workbench UI (jethaforge stack)
│   └── src/wasm/        # Committed wasm-pack build of grove-wasm
├── presets/species/     # Authoritative species TOML documents
└── scripts/             # build-wasm.mjs, desktop.mjs, make-icon.mjs
```

## The one engine rule

There is exactly one generation engine: `grove-core`. Every surface consumes
the same code:

- `grove-cli` links it natively.
- `grove-wasm` compiles it to WebAssembly for the webview.
- `grove-ffi` exposes it over C ABI.
- `grove-desktop` deliberately does **not** embed the engine — the desktop app
  runs the same `grove-wasm` build inside the Tauri webview, so preview and
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

## LOD previews

`GroveGenerator.generate(seed)` produces one `LodMesh` per level in the
species' `[lod]` config (preset or custom levels). Each LOD carries flat
vertex arrays plus `submeshes` material ranges. The viewport splits each LOD
into one `MeshDescriptor` per material (bark / leaves / impostor) sharing the
same vertex buffers — cheap switching, no duplicated geometry.

Levels flagged `crown_impostor` replace per-leaf geometry with two crossed
quads sampling a baked front+side atlas (`impostor.rs` — an in-engine
z-buffered rasterizer over the same leaf cards and cut branches the nearer
LODs draw). The atlas PNG rides on the `LodMesh` (`impostor_atlas`) so the
workbench can inspect it and the exporter embeds it.

The stack `MeshDescriptor` has no UV/texture channel yet, so the viewport
renders flat colors; generated maps are inspectable in the Objects panel
materials strip. Textured preview lands with the jethaforge descriptor
extension.

## Material maps

`textures.rs` generates deterministic bark albedo, bark normal (OpenGL +Y),
and leaf albedo+alpha card PNGs from `[textures]` params — no external art
dependency. `TextureSet::resolve(species, dir)` lets native hosts substitute
file-slot paths (relative to the species document); WASM always generates.
`generateMaps()` returns the three PNGs for inspection; exports embed them
when requested.

## Export flow

- **glb**: `export_glb(seed, embed_textures)` → single binary file.
- **gltf**: `exportGltf(seed, binName, embed_textures)` → `.gltf` JSON +
  `.bin` pair (images ride inside the `.bin` via bufferViews).
- Each submesh becomes its own primitive with the right material — bark,
  leaves (alpha-masked), impostor (alpha-masked, atlas-textured).
- Browser host: files download via anchor.
- Tauri host: `plugin-dialog` picks the destination, then the `save_export`
  command writes raw bytes (`[u32 path_len][path][payload]` frame).

## Host boundaries

The Tauri shell exposes two commands only:

- `read_text_file(path)` — bounded to 1 MiB, UTF-8 (species import).
- `save_export(raw frame)` — bounded to 512 MiB (glTF export).

Panel detachment to real native windows is provided by the stack's
`tools-frontend-host-tauri` plugin; the browser host falls back to
`createBrowserWindowHost`.
