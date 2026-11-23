# Volumetric Nature Stands (`midori stand`)

Real Australian bushland does not read as isolated trees dropped onto bare terrain: canopies overlap into continuous uneven domes, shrubs thicken along edges and clearings, understorey responds to shade above it, and distant stands merge into a textured volumetric mass rather than a forest of alpha-tested billboards.

**Volumetric nature stands** (`[asset] kind = "nature_stand"`) combine an existing ground patch (`presets/nature/*.toml`) with interacting vertical strata drawn from `presets/species/*.toml` and their `[states.*]` overlays. A single command builds a bounded, reusable plant library once and then streams deterministic, seamless world-space tiles of any size.

---

## Quickstart

```bash
# Generate a 128 m tile of WA Banksia Woodland in opaque foliage mode
midori stand \
  -p presets/stands/wa_banksia_woodland.toml \
  -o out/banksia_woodland \
  --tile-size 128 \
  --tile 0,0 \
  --foliage opaque \
  --max-instances-per-tile 300 \
  --max-triangles-lod0 40000 \
  --max-triangles-lod1 4000 \
  --max-triangles-lod2 200

# Validate the exported stand package (manifest, binary records, GLB budgets, opaque check)
midori validate-stand -i out/banksia_woodland

# Generate a seamless 4x4 region of tiles
midori stand \
  -p presets/stands/wa_jarrah_marri_forest.toml \
  -o out/jarrah_region \
  --tile-size 128 \
  --region -1,-1,2,2 \
  --foliage opaque

# Mask-driven bushland around a road, track, and clearing
midori stand \
  -p presets/stands/wa_banksia_woodland.toml \
  -o out/banksia_masked \
  --tile-size 128 \
  --tile 0,0 \
  --mask road_mask.png \
  --exclude road_exclude.png \
  --height terrain_height.png \
  --height-min 0 --height-max 35 \
  --foliage opaque

# Single-tree opaque foliage mode (zero alpha cutouts, closed low-poly leaf clusters)
midori generate -s presets/species/wa_jarrah.toml -o jarrah_opaque.glb --seed 42 --foliage opaque
```

---

## Asset Format (`nature_stand` TOML)

A stand preset is a TOML file declaring `[asset]`, `[stand]`, optional `[stand.condition]`, and one or more `[[strata]]` ordered from highest to lowest:

```toml
[asset]
kind = "nature_stand"
name = "WA Banksia Woodland"

[stand]
biome = "wa_banksia_woodland"
floor = "presets/nature/wa_banksia_woodland_floor.toml"
seed = 4001
edge_width = 7.0
exclude_trunk_clearance = 1.4
variants_per_species_size = 3
collision_min_diameter = 0.16

[stand.condition]
years_since_fire = 10.0
dieback = 0.08

[[strata]]
name = "canopy"
cover = 0.50
min_spacing = 3.2
clump_radius = 14.0
pattern = "clumped"

  [[strata.species]]
  preset = "presets/species/wa_slender_banksia.toml"
  share = 0.48
  size_classes = { sapling = 0.15, pole = 0.30, mature = 0.45, veteran = 0.10 }
```

### Strata Interaction, Size Classes & Condition

- **Vertical shade interaction:** Strata are placed top-down. Every lower stratum evaluates the projected crown cover above `(wx, wz)` and applies `under_tolerance`:
  - `avoid`: shade-intolerant pioneers favour gaps, tracks, and edges (`weight = 1 - 0.85 * cover_above`).
  - `neutral`: broadly tolerant sclerophyll shrubs (`weight = 1 - 0.15 * cover_above`).
  - `prefer`: shade-preferring understorey & sedges (`weight = 0.25 + 0.95 * cover_above`).
- **Botanical size classes (`sapling`, `pole`, `mature`, `veteran`):** Rather than uniformly scaling meshes, `scale_species_for_size_class` adjusts trunk height, slenderness/radius, crown offset, branch counts, rosette blade lengths, and leaf/bark parameters before generating the bounded plant library.
- **Static ecological condition:**
  - `years_since_fire`: activates `burnt` grass-tree state (`< 2.5 yr`) and `post_fire` jarrah epicormic regrowth (`< 3.5 yr`), clears/regrows grass-tree dead-leaf skirts, chars lower woody trunks (`< 12 yr`), and reduces young shrub stature (`< 5 yr`).
  - `dieback` (`0..=1`): converts a fraction of *Phytophthora*-susceptible species (banksias, jarrah, grass trees) into weathered silver-grey dead stags.
- **Bounded plant library:** At most `variants_per_species_size` (default `3`) distinct GLBs are generated per `(species, state, size_class, is_dead)` tuple and reused across all tiles.

---

## World-Space Seamless Tiling & Cross-LOD Stitching

Every tile `(tx, tz)` is evaluated strictly in **world space**:

1. Candidate positions are hashed from world-space macro-cells (`32 m`, with `64 m` clump hubs for `pattern = "clumped"`) across the tile plus a `28 m` border pad (`TILE_PAD_M = 28.0`).
2. Conflict resolution (`min_spacing`) runs in deterministic world-space priority order over the padded region, so stems near tile borders make identical accept/reject decisions regardless of tile generation order.
3. **Cross-LOD Stitching Rule:**
   - **Stand LOD 1** (`stand_lod1_canopy.glb`) evaluates a continuous world-space canopy heightfield on a `32 × 32` grid with real openings where canopy cover drops below threshold, closed vertical skirts along edges/exclusions/gaps, and low-poly border trunk pillars.
   - **Stand LOD 2** (`stand_lod2_far.glb`) evaluates the exact same continuous canopy field on an `8 × 8` grid (stride 4 against LOD 1) with closed boundary skirts and `<= 200` triangles per tile.
   - Adjacent same-LOD tiles share exact world-space border vertices bit-for-bit; adjacent LOD 1 / LOD 2 tiles coincide at every stride-4 vertex along shared edges with zero cracks.

---

## Masks, Exclusions & Bush Edges

Optional per-tile PNG inputs (`--mask`, `--exclude`, `--height`) drive realistic stand boundaries:

- **`--mask` (0..255):** Soft stand boundary; density scales smoothly with mask value and stems vanish where mask is 0.
- **`--exclude` (0..255, `> 0.5` excluded):** Hard exclusions for roads, tracks, buildings, and water. Zero stems are placed inside exclusions or within `exclude_trunk_clearance` metres; crowns overhanging an exclusion are clamped to `exclude_crown_overhang`.
- **Edge response (`edge_width`):** Within `edge_width` metres of a mask boundary or exclusion, canopy height tapers (`edge_height_scale`), shrub density boosts (`edge_shrub_boost`), and plants near the edge are prioritised into `tier = "instance"` so eye-level views along roads and tracks show individual Hero trees and shrubs backed by volumetric interior clumps.

---

## Opaque Foliage Mode (`--foliage opaque`)

Both `midori stand` and `midori generate` support `--foliage opaque`:

- **Zero alpha cutout or blending:** Every exported glTF material is strictly `OPAQUE` (`doubleSided: false`, no `alphaMode: "MASK"`, no `alphaMode: "BLEND"`), eliminating overdraw and alpha-testing shimmer on mobile, VR, and forward renderers.
- **Closed low-poly foliage volumes:** Foliage clusters are sculpted directly from the plant's actual leaf positions and bounding envelopes with per-vertex linear colour gradients, interior crown AO, and species-accurate bark/char colours.
- **Material cap:** Stands stay within `<= 4` shared material slots (`stand_ground`, `stand_bark`, `stand_foliage`).

---

## Package Layout & Binary Format

```text
<output>/
├── midori_stand.json              # Package manifest (docs/schema/midori_stand.schema.json)
├── library/
│   └── <species>_<size>_v<n>.glb  # Bounded reusable multi-LOD plant library
└── tiles/
    └── t_<tx>_<tz>/
        ├── instances.json         # Human-readable stem records (instance + merged tiers)
        ├── instances.bin          # Binary buffer (MDSI v1, 40 bytes/record)
        ├── stand_lod0_<stratum>.glb # Per-stratum merged clump volumes
        ├── stand_lod1_canopy.glb  # Mid-distance continuous irregular canopy + skirts + edge trunks
        ├── stand_lod2_far.glb     # Far canopy volume (<= 200 tris)
        ├── collision.json         # Trunk collision capsules (>= collision_min_diameter) + canopy heightfield
        ├── stand_stats.json       # Stem counts, requested vs achieved cover, triangle & material budgets
        └── rasters/
            ├── canopy_height.png  # 16-bit normalised canopy height above ground
            ├── canopy_cover.png   # 8-bit projected foliage cover (0..255)
            ├── stem_density.png   # 8-bit local stem density
            └── dominant_color.png # 8-bit RGB dominant canopy albedo
```

### Binary Instance Buffer (`instances.bin`, `MDSI` v1)

- **Header (12 bytes, little-endian):** `b"MDSI"` (`[u8; 4]`), `version = 1` (`u32`), `count` (`u32`).
- **Record (40 bytes, little-endian):** `variant_index` (`u32`), `stratum_index` (`u16`), `tier` (`u8`: `0 = instance`, `1 = merged`), `near_edge` (`u8`), `position` (`[f32; 3]`), `yaw` (`f32`), `scale` (`f32`), `color_tint` (`[f32; 3]`).

---

## Built-in Perth & South-West WA Stand Presets

Ecological canopy composition and structural targets are grounded in WA conservation literature; game-scale clump radii, species shares, and spacing parameters are explicitly marked as practical estimates.

| Preset | Floor Patch | Strata | Ecological Basis (Cited vs. Estimated) |
| --- | --- | --- | --- |
| `wa_banksia_woodland.toml` | `wa_banksia_woodland_floor.toml` | `emergent`, `canopy`, `shrub`, `ground` | **Cited:** *Banksia attenuata* / *B. menziesii* low open woodland (4–8 m, 30–60% cover) with emergent Jarrah/Marri/Sheoak & *Phytophthora* dieback (EPBC Conservation Advice 2016; DBCA). **Estimated:** clump radii, shrub shares, edge width. |
| `wa_jarrah_marri_forest.toml` | `wa_jarrah_forest_floor.toml` | `canopy`, `midstory`, `shrub`, `ground` | **Cited:** Northern Jarrah–Marri dry sclerophyll open forest (20–32 m, 50–70% canopy) over Bull Banksia & Sheoak midstorey, 8-yr prescribed burn cycle (Dell, Havel & Malajczuk 1989; Beard 1990). **Estimated:** size-class splits & clump scales. |
| `wa_tuart_woodland.toml` | `wa_banksia_woodland_floor.toml` | `canopy`, `midstory`, `shrub`, `ground` | **Cited:** Swan Coastal Plain *Eucalyptus gomphocephala* woodland over weeping *Agonis flexuosa* & *Banksia* midstorey (EPBC Tuart Conservation Advice 2019). **Estimated:** species shares & shrub density. |
| `wa_paperbark_swamp.toml` | `wa_paperbark_wetland_edge.toml` | `canopy`, `midstory`, `rush_sedge` | **Cited:** Seasonal dampland/sumpland *Melaleuca preissiana* / *M. rhaphiophylla* / *Banksia littoralis* groves over *Baumea articulata* rush beds (Keighery 1994). **Estimated:** clump radii & spacing. |
| `wa_flooded_gum_fringe.toml` | `wa_paperbark_wetland_edge.toml` | `canopy`, `subcanopy`, `bank_sedge` | **Cited:** Swan–Canning riparian *Eucalyptus rudis* & *Melaleuca rhaphiophylla* fringing woodland with *Casuarina obesa* and *Baumea* banks (Swan River Trust / DBCA). **Estimated:** density & subcanopy shares. |
| `wa_coastal_heath.toml` | `wa_coastal_dune.toml` | `tall_shrub`, `low_shrub`, `dune_grass` | **Cited:** Quindalup coastal fore-dune & heath (**zero tree canopy stratum**): *Olearia axillaris*, *Scaevola crassifolia*, *Lepidosperma gladiatum*, *Spinifex longifolius* (Beard 1990; Trudgen 1991). **Estimated:** clump spacing. |
| `perth_pine_plantation.toml` | `temperate_forest_floor.toml` | `canopy` (`regular`), `edge_scrub` (`avoid`) | **Cited:** Gnangara / Pinjar *Pinus pinaster* forestry rows (~3.6 m × 3.0 m spacing, FPC WA silviculture) with shade-suppressed interior. **Estimated:** edge pioneer scrub mix. |

---

## Engine Integration Notes

> **Important:** Volumetric stand import is **separate** from Midori's single-patch groundcover importers (`integrations/unity/Editor/MidoriNaturePackageImporter.cs` and `integrations/unreal/midori_nature_importer.py`, which consume `midori_nature.json`).

To stream a `midori_stand.json` package into Godot 4, Unity, or Unreal Engine 5:

1. **Import `library/*.glb` once** as shared mesh/prefab prototypes (each GLB carries its own `MSFT_lod` chain).
2. **Near tiles (`0–120 m`):** Read `tiles/t_<x>_<z>/instances.bin` (`MDSI` v1) and spawn `tier == 0` (`instance`) records via `MultiMeshInstance3D` (Godot), `Graphics.DrawMeshInstanced` / `TerrainData` (Unity), or `UHierarchicalInstancedStaticMeshComponent` (UE5), combined with `stand_lod0_<stratum>.glb` for interior merged clumps.
3. **Mid tiles (`120–350 m`):** Swap the tile to the single mesh `stand_lod1_canopy.glb` (`<= 4,000` tris).
4. **Far tiles (`350 m+`):** Swap to `stand_lod2_far.glb` (`<= 200` tris).
5. **Physics & AI:** Load `collision.json` trunk capsules for vehicle/character collision and optional `canopy_heightfield` + `rasters/canopy_cover.png` for AI sightlines and mini-map shading.

---

## Measured Desktop CPU Performance

Measured with the release binary (`cargo build --release --bin midori`) on a desktop Windows workstation (x86_64) generating `wa_banksia_woodland.toml` (`--tile-size 128 --foliage opaque --max-instances-per-tile 300 --max-triangles-lod0 40000 --max-triangles-lod1 4000 --max-triangles-lod2 200`):

| Operation | Output / Scope | Measured Wall Time |
| --- | --- | --- |
| Build plant library (`library/*.glb`) | 141 multi-LOD opaque plant GLBs (13 species × size classes × 3 variants + dieback stags) | **3,558 ms** (3.56 s) |
| Generate one 128 m tile (`tiles/t_0_0/`) | 2,400 stems (300 instanced, 2,100 merged), 56% achieved cover, LOD 0 = 27,804 tris, LOD 1 = 2,692 tris, LOD 2 = 72 tris, 4 PNG rasters, `instances.bin`, `collision.json` | **164 ms** (0.16 s) |
| Subsequent 128 m tile (`--skip-library`) | 2,357 stems (300 instanced, 2,057 merged), LOD 0 = 27,484 tris, LOD 1 = 2,656 tris, LOD 2 = 72 tris | **196 ms** (0.20 s) |
| Package validation (`midori validate-stand`) | Full manifest + 141 library GLBs + tile GLBs + binary instance count + opaque verification | **199 ms** (0.20 s) |
