# Midori nature packages in UsagiGP

UsagiGP (the Usagi engine's diorama racer) builds its course meadows from Midori nature
packages: knee-high grass and wildflowers as alpha-card clumps, scattered over a mask that
the game draws from its course (the road, the verges, water and prop clearings), and
baked into chunked static models on the game side.

This file describes the parts of the package UsagiGP reads and the patch features it relies
on. They are general Midori features; nothing here is UsagiGP-specific except the importer,
which lives in the game repository (`tools/tracks/midori_meadow.py` and
`tools/tracks/blender_midori.py`).

## Patch features used

### Alpha-card layers (`[groundcover.layers.cards]`)

Any groundcover layer can be built as a clump of alpha-tested cards instead of geometry
blades. Each card samples one species cell of a shared, procedurally painted atlas.

```toml
[groundcover.card_atlas]          # one atlas for every card layer in the patch
cell = 256                        # pixels per species cell
base = "#4f6426"                  # grass colour at the root (sRGB)
tips = ["#a3b04a", "#c4ba5c"]     # grass tip colours, picked per blade
seed = 0

[[groundcover.layers]]
kind = "flower"
name = "meadow flowers"
density = 0.55
height = 0.42                     # card height (world units)

[groundcover.layers.cards]
species = ["daisy", "buttercup", "clover", "bellflower", "yarrow"]
count = 3                         # cards per clump at LOD0 (LOD1: half, LOD2: one)
card_width = 0.36
height_range = [0.7, 1.25]        # per-card height, x layer height
lean_degrees = [2.0, 20.0]        # per-card lean off upright, either way
loose = 0.1                       # cards stand apart by up to this (0: crossed at the centre)
normal_up = 0.7                   # normals blended towards +Y (cards light like the ground)
```

Species: `grass`, `golden_grass`, `oat_seeds`, `daisy`, `buttercup`, `clover`,
`bellflower`, `lupin`, `fern`, `yarrow`, `poppy`.

Card vertices keep the blade packing used by every Midori groundcover prototype:
`COLOR_0 = (height, colour variation, t along the card, stiffness)` and
`TEXCOORD_1 = (wind phase, stiffness)`, so a single foliage material and wind setup covers
blades and cards.

### Scatter mask (`[scatter_mask]`)

```toml
[patch]
size = 150.0
edge_fade = 0.0                   # no soil/groundcover fade at the patch edges

[scatter_mask]
path = "mask.png"                 # grayscale PNG, relative to the patch TOML
extent = [-75.0, -42.0, 75.0, 42.0]   # world X/Z rectangle [x_min, z_min, x_max, z_max]
clearings = [[24.2, -10.6, 3.6]]  # discs with no instances: [x, z, radius]
```

The mask multiplies every layer's acceptance. Image row 0 is `z_max` (a map seen from
above with +Z up the page) and column 0 is `x_min`. Outside the extent the density is 0.
`NaturePatch::from_file` loads the image; hosts building a patch in code can set
`scatter_mask_data` with `ScatterMaskData::from_values`.

## Package outputs read

| Path | Use |
|---|---|
| `midori_nature.json` | `prototypes[i]` (one per groundcover layer, in layer order) with `lods[].file`; `card_atlas` (file, grid, cell layout); `scatter_mask` (echo of the mask used). |
| `prototypes/<layer>_lod<N>.glb` | Card clump meshes: `POSITION`, `NORMAL`, `TEXCOORD_0` (atlas UVs, glTF convention: v down the image), `COLOR_0`, `TEXCOORD_1`. Y up, clump base at the origin. |
| `textures/groundcover_cards.png` | The RGBA species atlas: sRGB colour, straight alpha (alpha-test at 0.5), colour bled into the transparent texels so mips don't fringe. |
| `instances/scatter.json` | Per layer: `layer_index`, chunks of instances with `position` (patch X/Y/Z), `yaw` (radians about +Y), `height` and `width` multipliers, `phase`, `color_variation`. |

The binary scatter files (`instances/binary/*.bin`, `midori.scatter.bin.v1`) carry the same
records for importers that prefer them.

## How UsagiGP consumes it

1. **Mask.** `midori_meadow.py` draws the mask over the course board from the ground
   painter's turf density map (zero on the road, verges, water and paved areas). It
   multiplies that by a road band: full density within `near` units of the road edge and
   `far` beyond. It writes the patch TOML from a preset (default `alpine_meadow`) with:
   - the board's size and extent;
   - flat soil (the game has its own terrain);
   - per-layer density multipliers;
   - the course's clearings.
2. **Generate.** `midori nature -p meadow.toml -o <package>`.
3. **Bake.** `blender_midori.py` stands every instance on the course terrain, using LOD0
   within `lod_near` of the road and LOD1 beyond. It merges the copies into one static
   model per 24-unit chunk, with one alpha-tested material using the atlas and the engine's
   per-material foliage flag (no SSAO or contact shadow, light through the cards). It casts
   no shadows and has a draw distance. Chunks cull independently.
