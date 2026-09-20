# GrassSystemThreeJS Audit For Midori

Date: 2026-07-05

Inspected repo: https://github.com/achrefelouafi/GrassSystemThreeJS

Inspected commit: `b236b2a38d9f35daa2ddc7b0152544b10e635d0c`

Local verification: `npm ci` and `npm run build` both passed in the cloned repo. The production bundle built successfully with one large-chunk warning.

## Mission Implication

Midori should expand from "tree generator" to "procedural nature asset generator for real-time worlds."

The clean scope is not "everything outdoors." It is deterministic, exportable, engine-ready nature assets:

- Woody plants: trees, shrubs, cacti, yucca, branching plants.
- Groundcover: grass, moss, small plants, repeated low vegetation.
- Soil and terrain surfaces: sculptable ground tiles, height fields, density masks, material masks.
- Biome recipes: seedable combinations of plants, soil, moisture, moss, cracks, wind response, and LOD policy.

The texture/PBR generation pipeline remains parked. This report discusses texture and material contracts because soil assets require them, but it does not recommend building image-generation, PBR derivation, or texture curation now.

## What The Repo Actually Is

Despite the repository name, the project is closer to "Soil Studio" than a narrow grass renderer:

- `src/main.js` owns the scene, GUI, soil shader injection, texture loading, procedural height/mask functions, wind controls, and render loop.
- `src/grass.js` builds a single `InstancedBufferGeometry` for a grass field and moves blades entirely in the vertex shader.
- `src/model.js` loads arbitrary GLB files and injects upward-face moss accumulation into their materials.
- `src/clouds.js` and `src/postfx.js` are cinematic demo features, not asset-generation fundamentals.
- `public/` includes ambientCG-style soil/moss texture sets and two sample vehicle GLBs.

The repo is MIT licensed. The README credits ambientCG for the soil/moss texture sets; ambientCG documents its downloadable assets as CC0. The bundled vehicle GLB provenance is not documented in the README, so Midori should not reuse those vehicle assets.

## Most Useful Ideas To Crib

### 1. One Shared Terrain Field

The strongest idea is the shared height/mask source of truth. In `main.js`, `HEIGHT_FUNCTIONS` defines `groundHeightAt(worldXZ)`, and both the soil surface and the grass shader consume that same function. Moss height is folded into the same height field, so grass rides over moss rather than clipping through it.

Midori should adapt this as a Rust-side `TerrainField` contract:

- deterministic seed
- tile size and world transform
- height function or baked height map
- analytic/sampleable normal
- named masks: grass density, moss, wetness, cracks, exposed soil
- export metadata that records all scalar parameters used to produce the field

For Unity and Unreal, the practical output should usually be baked maps and instance data, not a browser-only GLSL function.

### 2. Grass As Prototype Mesh Plus Instance Attributes

`grass.js` uses a simple subdivided vertical strip as the blade prototype. CPU data supplies compact per-instance attributes: position, yaw, height, width, phase, curl variation, and color variation. The shader handles terrain snap, curl, wind, color gradient, base darkening, and translucency.

Midori should keep that data model, but turn it into engine-facing assets:

- one or more grass clump meshes, not millions of exported individual blades
- vertex attributes for bend stiffness, phase, blade height, and color variation
- deterministic instance placement per tile
- chunked instance buffers or density maps
- LOD meshes for clumps and billboards/cards for far ranges

The repo changes density by editing `geometry.instanceCount`; Midori should expose density as a deterministic scatter parameter and an engine adapter setting.

### 3. Procedural Masks As Authoring Controls

The GUI exposes separate controls for mounds, tone variation, moisture, cracks, moss, grass coverage, and wind. That maps well to Midori's species/control-groups direction, but at a patch/biome level rather than a single tree.

Recommended Midori concepts:

- `soil.surface`: mounds, fine relief, edge taper, cracks
- `soil.material_masks`: dry/rich variation, wetness, moss, exposed dirt
- `groundcover.layers[]`: grass, moss tufts, small flowers, dead litter
- `wind`: shared direction/strength/speed/gust fields
- `biome.tags`: temperate, arid, meadow, forest_floor, roadside, alpine

### 4. Material Coherence Across Ground, Grass, And Objects

The model moss system reuses the ground moss maps and master moss enable uniform. That is valuable as a concept: ecological overlays should be consistent across asset families.

For Midori, this becomes a "surface overlay" layer:

- moss can affect soil, rocks, logs, and trunk bases
- wetness can affect soil, rocks, and bark bases
- snow or leaf litter can become future overlay types

This should be data-driven and exportable as masks/material slots, not implemented as Three.js `onBeforeCompile` patches.

### 5. Performance Defaults

The repo makes useful demo choices:

- grass is one instanced draw call
- grass shadows are disabled
- expensive effects are off by default
- volumetric clouds render at reduced resolution
- density is a direct quality knob

For Midori's asset pipeline, the engine equivalent is:

- chunk vegetation by tile for culling
- keep grass collision off by default
- provide shadow-casting variants separately
- provide LODs and density scaling metadata
- keep material slot counts low, especially for foliage

## What Not To Copy

- Do not copy the Three.js `onBeforeCompile` architecture as Midori's core abstraction. It is useful for a web demo but not portable to Unity or Unreal.
- Do not export one giant 200k-blade mesh. Use prototypes, clumps, instance transforms, density maps, and engine-native foliage systems.
- Do not disable frustum culling globally the way the demo does for one field. Midori should chunk tiles and provide conservative bounds per tile.
- Do not rely on `Math.random()` for generated assets. Midori needs fixed-seed deterministic scatter.
- Do not make clouds, depth of field, bloom, or film grade part of the asset mission.
- Do not reuse undocumented sample GLBs.
- Do not unpark the texture/PBR generation pipeline. Use placeholders or externally supplied maps until a separate material-pipeline goal exists.

## Engine-Facing Output Strategy

Midori should generate a package, not just a mesh.

Recommended package contents for a soil-and-grass tile:

- `tile.glb`: optional preview mesh or baked terrain tile mesh.
- `grass_clump_LOD0.glb`, `grass_clump_LOD1.glb`, `grass_clump_LOD2.glb`: reusable prototypes.
- `height.png` or `height.exr`: terrain height.
- `normal_unity_yplus.png`: Unity/OpenGL-style normal map.
- `normal_directx_yminus.png`: Unreal-friendly normal variant or documented green-channel flip.
- `grass_density.png`: density/weight mask.
- `moss_mask.png`, `wetness_mask.png`, `crack_mask.png`: material/overlay masks.
- `instances.bin` or `instances.json`: optional deterministic instance transforms when not relying on engine scatter.
- `midori_nature.json`: manifest with seed, units, tile size, material slots, LODs, wind channels, coordinate conventions, and intended engine adapter.

For glTF/GLB, use it for mesh prototypes and preview geometry. Do not assume glTF alone can express every engine foliage or landscape feature. Unreal's glTF docs explicitly warn that extensions are not universally implemented, and engine foliage systems have their own native asset types.

## Unity Notes

Unity Terrain details can render grass and small objects as textured quads or full meshes, and Unity recommends instanced detail meshes for most arbitrary-mesh detail placement. Its docs note that GPU-instanced terrain details use the prefab material/shader, allow shader customization, and are rendered in batches of 1,023 or fewer instances.

Midori's Unity target should therefore produce:

- Terrain heightmap and Terrain Layer maps for soil.
- Detail Mesh prefabs for grass clumps, moss tufts, small plants, and debris.
- Density maps that can be consumed by an editor importer.
- Shader Graph or URP/HDRP material templates that read Midori vertex colors and masks.
- Y+ normal maps. Unity's current manual identifies Unity normal maps as Y+, also known as OpenGL format, which matches the repo's `NormalGL` naming.

Avoid depending on Unity's non-instanced grass detail mode for high-quality custom shading; Unity's own docs say GPU instancing is the path that uses the material and shader from the prefab.

## Unreal Notes

Unreal has two relevant paths:

- Landscape Grass Type driven by Landscape materials and Landscape Grass Output for terrain-bound grass.
- Static Mesh Foliage for painted or procedural groundcover on landscapes and other surfaces.

Epic's foliage docs state that Static Mesh Foliage is batched with hardware instancing, while Actor Foliage has the cost of normal Actors. That makes Static Mesh Foliage the default Midori target for performance. The same docs also call out cluster culling, per-instance fade through `PerInstanceFadeAmount`, foliage density scaling, and LOD caveats.

Midori's Unreal target should produce:

- Static Mesh grass clumps with LODs and a low material slot count.
- Foliage Type or Landscape Grass Type configuration metadata.
- Landscape weightmaps/density maps for grass and material layers.
- Material functions for wind, bend stiffness, color variation, and fade.
- DirectX/Y- normal maps or clear import guidance for OpenGL/Y+ maps.
- FBX export as an optional adapter for Unreal static meshes because Unreal's FBX static mesh pipeline supports materials, multiple UV sets, smoothing groups, vertex colors, LODs, and custom collision. The docs also note that UE's FBX pipeline uses FBX 2020.2.

Important Unreal foliage caveats:

- Prefer one material element for foliage LOD0 when possible.
- Keep LODs aligned with the same pivot and compatible UV/lightmap assumptions.
- Avoid high-density Actor Foliage.
- Use start/end cull distance and material fading to avoid abrupt cluster disappearance.

## Data Model Proposal

Midori can add a nature patch schema without disrupting tree species:

```toml
[asset]
kind = "nature_patch"
name = "Temperate Forest Floor"
units = "meters"

[patch]
size = 16.0
seed = 42
biome = "forest_floor"
tags = ["temperate", "moss", "grass", "damp_soil"]

[soil]
profile = "loam"
mound_scale = 0.12
mound_height = 0.55
relief_scale = 0.7
relief_strength = 0.6

[soil.cracks]
enabled = false
plate_density = 0.9
channel_width = 0.06
depth = 0.7

[[groundcover.layers]]
kind = "grass"
name = "fine meadow grass"
density = 0.13
coverage = 0.62
patch_scale = 0.15
height = 1.5
width = 0.049
curl = 1.14

[[groundcover.layers]]
kind = "moss"
coverage = 0.55
patch_scale = 0.14
height = 0.14

[wind]
strength = 0.5
speed = 1.8
direction_degrees = 20
gust_scale = 0.35
flutter = 0.6
```

This should compile to a shared `NaturePatch` output:

- terrain bounds
- height and normal samples
- material masks
- groundcover prototypes
- optional instance transforms
- export manifest

## Implementation Roadmap

### Phase A: Mission And Schema

- Update Midori language from tree-only to nature asset generation.
- Add `NaturePatch` schema docs.
- Keep `Species` for plants and add `Patch` for soil/groundcover composition.
- Add deterministic fixture tests for patch masks and scatter counts.

### Phase B: Soil Tile Prototype

- Implement Rust-side deterministic height/mask sampling for a square tile.
- Export a preview terrain mesh with vertex colors for masks.
- Export map images only as simple generated masks at first; do not build a PBR derivation pipeline.
- Preview the same tile in the web editor.

### Phase C: Grass/Groundcover Prototype

- Add clump mesh generation: blade strips grouped into small reusable clusters.
- Add vertex colors or custom attributes for bend stiffness, phase, and color variation.
- Add deterministic scatter over the tile, with density and exclusion masks.
- Add low LOD cards or simplified clumps.

### Phase D: Engine Package Manifests

- Add `midori_nature.json` sidecar output.
- Add Unity profile metadata: terrain layer names, detail prefab mappings, normal map convention, density map names.
- Add Unreal profile metadata: foliage type defaults, cull distances, LOD names, material parameter names, normal map convention.

### Phase E: Validation

- Core tests: fixed seed mask checksums, instance counts, bounds, no NaN attributes, LOD counts.
- Export tests: GLB prototype validity, manifest schema validity, image dimensions/ranges.
- Web smoke: load a soil+grass tile, force LODs, verify nonblank preview.
- Manual engine smoke initially: import into Unity and Unreal, confirm scale, normal orientation, LODs, density masks, culling, and wind material hookup.

## Open Questions

- Should Midori generate engine-importer helper scripts, or stay engine-neutral with manifests?
- Should grass instance transforms be exported, or should the engine importer regenerate scatter from seed and masks?
- Do we want "soil" to mean terrain-tile assets only, or also material overlays for tree bases and rocks?
- Should moss be a material overlay first, a mesh groundcover first, or both?
- How strict should Midori be about target budgets per engine profile?

## Recommendation

Expand Midori's mission, but keep it asset-centric:

> Midori generates deterministic, engine-ready nature assets: plants, groundcover, soil surfaces, and biome patches for real-time worlds.

The GrassSystemThreeJS repo is valuable as a design reference for shared fields, masks, instanced grass attributes, and coherent moss/soil/grass controls. It should not become the implementation model wholesale. Midori should move the ideas into Rust-side deterministic data, exportable maps, mesh prototypes, and engine-specific manifests that Unity and Unreal can consume through their native terrain and foliage systems.

## Sources

- GrassSystemThreeJS repository: https://github.com/achrefelouafi/GrassSystemThreeJS
- Inspected grass implementation: https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/grass.js
- Inspected soil/moss implementation: https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/main.js
- Inspected model moss implementation: https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/model.js
- MIT license in repo: https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/LICENSE
- Unity Terrain grass/details docs: https://docs.unity3d.com/Manual/terrain-Grass.html
- Unity normal map docs: https://docs.unity3d.com/Manual/StandardShaderMaterialParameterNormalMap.html
- Unity glTFast docs: https://docs.unity3d.com/Packages/com.unity.cloud.gltfast@5.2/manual/index.html
- Unreal Grass Quick Start: https://dev.epicgames.com/documentation/en-us/unreal-engine/grass-quick-start-in-unreal-engine
- Unreal Foliage Mode docs: https://dev.epicgames.com/documentation/en-us/unreal-engine/foliage-mode-in-unreal-engine
- Unreal glTF support docs: https://dev.epicgames.com/documentation/unreal-engine/gltf-file-format-support-in-unreal-engine
- Unreal FBX Static Mesh Pipeline docs: https://dev.epicgames.com/documentation/en-us/unreal-engine/fbx-static-mesh-pipeline-in-unreal-engine
- ambientCG license docs: https://docs.ambientcg.com/license/
