# Workbench guide

The workbench is the interactive front-end for the Midori engine: edit
Weber–Penn parameters, preview LODs in 3D, and export glTF.

## Run it

```bash
npm ci                # installs deps incl. the pinned jethaforge stack build
npm run dev           # browser dev server at http://127.0.0.1:1420
npm run desktop       # Tauri dev build (native window)
npm run desktop:build # packaged app (NSIS installer on Windows)
```

Regenerate the WASM bundle after changing `crates/midori-wasm` or
`crates/midori-core`:

```bash
npm run wasm          # wasm-pack build -> apps/desktop/src/wasm
```

## Panels

| Panel | Contents |
| --- | --- |
| Species (left) | Built-in presets (oak/pine/palm/willow from `presets/species/`), TOML import |
| LOD preview | 3D viewport; toolbar switches LOD levels, Fit recenters |
| Species source | TOML editor; Apply validates through the engine, Revert restores |
| Tree (right) | LOD list with triangle budgets, bark/leaves layer toggles, generated material-map strip, variant seeds |
| Parameters (right) | Weber–Penn fields: trunk, per-level branches, crown, leaves, textures, LOD preset, platform |
| Activity (bottom) | Generation stats, bounds, export log |

The Tree panel's **Materials** strip shows the species' generated maps (bark
albedo, bark normal, leaf card) plus the baked impostor atlas for LODs that
use `crown_impostor`. The 3D preview binds the same maps — bark albedo,
alpha-tested leaf cards, and the baked impostor atlas on far LODs — so what
renders matches the glTF export's materials.

Drag panel headers to re-dock; tap Space over a viewport to maximize; hold
Space for the hotbox; Cmd/Ctrl+K opens the action palette. Layouts persist
per window. In the desktop build, panels can be detached into real native
windows.

## Nature panel

The Nature panel previews the scatter presets bundled from
`presets/nature/*.toml` (Temperate Forest Floor, Flowering Meadow, Arid
Scrub). Selecting a preset renders its terrain tile plus a bounded,
deterministic sample of scatter instances as plain meshes with the instance
transform baked in — the default cap is 400 instances. The LOD selector
picks which prototype LOD is drawn.

Full-density scatter and GPU instancing are not yet supported by the
viewport; that work is tracked separately. The same TOML file feeds the
`midori nature` CLI subcommand, so what the panel samples is what the
pipeline builds.

## Export

**File → Export glTF…** writes every LOD level for the chosen variants:

- `.glb` — one binary file per variant.
- `.gltf` — JSON document + matching `.bin` buffer.

Variants export as `name_0.glb`, `name_1.glb`, … from the seed list in the
Tree panel. Vertex data includes `TEXCOORD_1`/`COLOR_0` Pivot Painter channels
for wind animation in engines that support them.

**Embed material maps** (on by default) packs the species' generated bark
albedo+normal and leaf card PNGs into the export. Leaf and impostor
primitives get `alphaMode: MASK` cutout materials; baked impostor atlases
embed automatically whenever a LOD uses `crown_impostor`.

## Browser vs desktop

Both hosts run the identical WASM engine. The browser host downloads files;
the desktop host writes to a picked path and supports detached panel windows.
