# Species TOML format

A species document is the authoritative input to every Grove surface (CLI,
WASM workbench, C FFI). This file documents the schema; `presets/species/`
contains complete examples.

```toml
[species]
name = "Oak"                       # required
scientific = "Quercus robur"       # optional

[trunk]
height = 6.0                       # metres, default 8.0
height_variance = 0.15             # 0..1 randomisation
radius = 0.45                      # metres, default 0.45
taper = 0.75                       # 0..1, default 0.7
curve = 8.0                        # degrees
curve_variance = 4.0               # degrees
curve_back = 3.0                   # degrees, corrective counter-curve
segments = 8                       # ring resolution, default 8

# Up to three recursive branch levels. Omit a section to disable the level.
[branches.level1]
count = 6                          # children per parent
count_variance = 2                 # ± randomisation
length = 4.5                       # metres
length_variance = 0.25             # 0..1
radius_ratio = 0.55                # relative to parent radius
angle = 55.0                       # degrees from parent axis
angle_variance = 20.0
rotation = 137.5                   # degrees around parent (golden angle default)
gravity = -0.15                    # -1..1, negative pulls upward
curve = 25.0                       # degrees along the branch
curve_variance = 10.0
segments = 6

[branches.level2]  # ...           # same fields, typically thinner/shorter
[branches.level3]  # ...

[crown]
shape = "spherical"                # spherical | conical | hemispherical |
                                   # flame | columnar
offset = 0.35                      # 0..1 up the trunk where the crown starts
density = 1.0                      # multiplier on leaf count
width_ratio = 1.2                  # width relative to height

[leaves]
count = 3000                       # target count at LOD0
min_level = 2                      # branches below this level get no leaves
size = 0.12                        # metres
size_variance = 0.25               # 0..1
distribution = "both"              # endpoint | along_branch | both
geometry = "cross_billboard"       # polygon | cross_billboard | billboard | none
up_influence = 0.25                # 0..1, how strongly leaves tilt upward

[textures]                         # material maps: generated or file slots
bark_prompt = "..."                # authoring hint for art pipelines/tool bus
leaf_prompt = "..."
resolution = 512                   # generated map size, 16..4096 px square
seed = 12345                       # optional; omit to derive from species name
bark_style = "furrowed"            # furrowed | plated | smooth
leaf_shape = "oval"                # oval | pointed | lobed | needle
leaf_card = "cluster"              # cluster | single
bark_color = [0.36, 0.30, 0.25]    # optional sRGB 0-1 base color
leaf_color = [0.17, 0.35, 0.12]    # optional sRGB 0-1 base color
bark_albedo = "maps/bark.png"      # optional file overrides, resolved relative
bark_normal = "maps/bark_n.png"    #   to this document by native hosts
leaf_albedo_alpha = "maps/leaf.png"

[lod]
preset = "balanced"                # ultra | high_quality | balanced | mobile |
                                   # minimal | custom
count = 3                          # optional override of preset level count

# With preset = "custom", declare levels explicitly:
# [[lod.levels]]
# index = 0
# name = "high"
# target_triangles = 8000
# branch_levels = 3
# leaf_geometry = "cross_billboard"
# leaf_reduction = 0.0
# ring_resolution = [16, 10, 6, 4] # per-level ring segments
# screen_height = 1.0              # switch threshold (fraction of screen)
# crown_impostor = false           # bake the crown to crossed impostor cards

[platform]
target = "modern_pc"               # modern_pc | mobile | switch | quest |
                                   # web | universal
```

## Material maps

Grove generates the three maps a tree needs — bark albedo, bark normal, and a
leaf albedo+alpha card — deterministically from `[textures]`, with no external
art dependency. Generated maps are what glTF exports embed (when textures are
requested) and what the crown-impostor baker samples.

- `resolution` applies to all generated maps. The impostor atlas is two square
  views side by side — front (+Z) left, side (+X) right — rendered at the
  larger of the map resolutions, clamped to 64–1024 px per view.
- `seed` is optional: leave it out and the map seed derives from the species
  name, so renaming a species re-rolls its bark.
- File slots (`bark_albedo`, `bark_normal`, `leaf_albedo_alpha`) override the
  corresponding generated map. Native hosts (CLI, FFI) resolve them relative
  to the species document; WASM hosts always generate procedurally because the
  browser bundle has no filesystem.
- Normal maps are tangent-space **OpenGL +Y** (the glTF convention). Hand a
  DirectX-style map in through `bark_normal` and shading will be inverted.
- Leaf/impostor materials export with `alphaMode: MASK` — the alpha channel
  of the leaf card or impostor atlas is the cutout.
- `bark_prompt`/`leaf_prompt` are authoring hints only. They travel with the
  document for art pipelines (e.g. the studio tool bus proposal) but do not
  affect generated maps.

## Notes

- All floats are f32; counts are u32. `*_variance` values are ± ranges applied
  per seed, so the same document + seed is fully deterministic. Generated maps
  are likewise deterministic in the document.
- `leaf_reduction` in custom LOD levels scales `leaves.count`
  (1.0 = remove none, 0.0 = remove all).
- `crown_impostor` on a LOD level replaces per-leaf geometry with two crossed
  quads textured by a baked front+side atlas; the stem hierarchy still renders
  normally down to `branch_levels`.
- Unknown sections or keys are rejected — the document must stay strictly
  inside this schema (that is what makes a bad edit surface in the workbench
  source panel instead of silently no-oping).
