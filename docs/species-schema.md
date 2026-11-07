# Species TOML Schema

Midori species are TOML files deserializing into `midori_core::Species`
(`crates/midori-core/src/species.rs`). A machine-readable JSON Schema lives at
[`schemas/species.schema.json`](../schemas/species.schema.json); the
`species_schema` test keeps it in sync with the serde model, so this page is
the human reference.

Unknown keys are accepted at the top level (forward compatibility), but every
documented section is closed: a typo inside `[species]`, `[trunk]`,
`[branches]`, `[crown]`, `[leaves]`, `[textures]`, `[materials]`, `[lod]`, or
`[platform]` will be ignored by serde — rely on the schema for linting.

## Top level

| Section | Required | Purpose |
|---------|----------|---------|
| `[species]` | yes | identity: `name` (required), `scientific`, `latin`, `biome`, `tags` |
| `[trunk]` | yes | trunk geometry |
| `[branches]` | no | per-level branch params (`level1`/`level2`/`level3`) |
| `[generator]` | no | `family` + `notes`; default `weber_penn` |
| `[crown]` | no | crown shape, offset, density, width ratio |
| `[leaves]` | no | leaf count, size, distribution, geometry, shape |
| `[textures]` | no | procedural-map parameters + host file overrides |
| `[materials]` | no | metadata-only material placeholders (see below) |
| `[[control_groups]]` | no | editor control → parameter-path mapping |
| `[canopy]` | no | top-down/distant canopy shaping (see below) |
| `[lod]` | no | LOD preset or explicit `levels` list |
| `[platform]` | no | target platform (`modern_pc`, `mobile`, `switch`, `quest`, `web`, `universal`) |

## Enums

| Field | Values |
|-------|--------|
| `generator.family` | `weber_penn`, `dichotomous`, `cactus`, `pad_chain`, `custom` |
| `branches.*.radius_model` | `ratio` (default), `pipe` |
| `*.taper_profile` | `linear`, `smooth`, `exponential`, `compound` |
| `crown.shape` | `spherical` (default), `conical`, `hemispherical`, `flame`, `columnar` |
| `leaves.distribution` | `endpoint`, `along_branch`, `both` (default) |
| `leaves.geometry`, `lod.levels.*.leaf_geometry` | `polygon`, `cross_billboard` (default), `billboard`, `canopy_card`, `none` |
| `leaves.shape`, `textures.leaf_shape` | `oval`, `pointed` (default), `needle`, `oak_lobed` (legacy alias `lobed`), `maple`, `serrated`, `willow`, `heart`, `palmate` |
| `textures.bark_style` | `furrowed` (default), `plated`, `smooth` |
| `textures.leaf_card` | `single`, `cluster` (default), `puff`, `frond` |
| `canopy.normals` | `geometric` (default), `canopy` |
| `canopy.card_roll` | `random` (default), `radial` |
| `canopy.vertex_color` | `pivot_painter` (default), `tint_ao` |
| `lod.preset` | `ultra`, `high_quality`, `balanced` (default), `mobile`, `minimal`, `open_world`, `hero_tree`, `custom` |
| `platform.target` | `modern_pc` (default), `mobile`, `switch`, `quest`, `web`, `universal` |

## Canopy shaping (`[canopy]`)

For foliage seen from above or far away (RTS / top-down cameras), where the
crown's silhouette and shading carry the read. All defaults leave output
unchanged; see `presets/species/topdown_*.toml`.

- Leaves are grouped into **clumps** by the `clump_level` stem they hang from
  (1 = one clump per main limb).
- `leaves.geometry = "canopy_card"` emits one quad per leaf centred on it and
  facing out of its clump/crown (`card_tilt` jitter, `card_max_angle` limit
  from straight up, `card_scale_by_clump` to shrink cards on short limbs,
  `card_roll = "radial"` + `card_offset` to fan frond cards outward from the
  trunk for star-shaped conifer outlines).
- `normals = "canopy"` bends foliage normals toward clump-out
  (`clump_weight`) / crown-out plus `up_bias`, blended by `normal_blend`, so
  card foliage shades as rounded lumps.
- `crown_fill` adds cards over the crown envelope to close holes between
  clumps; `shell` pulls leaves toward their clump surface.
- `vertex_color = "tint_ao"` writes per-clump tint (`tint_variation`,
  `hue_variation`) x baked crown/clump AO (`ao_strength`) into `COLOR_0`
  instead of Pivot Painter data (bark gets white). Godot's glTF import
  multiplies albedo by `COLOR_0` automatically.
- `single_sided` drops duplicated back faces (the leaf material is already
  double-sided); `roughness` overrides the exported leaf roughness;
  `lod_scale_compensation` grows reduced-LOD canopy cards to keep coverage.
- `lod.levels.*.branch_levels = 0` exports trunk-only bark.
- `midori generate --lod-preset species` uses the species' own `[lod]`.

## Materials and textures (#17)

Procedural textures are the baseline: `midori generate --textures` /
`midori maps` and `export_glb`/`export_config(embed_textures)` bake a bark
albedo+normal pair and a leaf albedo+alpha card per species, deterministic
from `[textures]` (`bark_style`, `leaf_shape`, `leaf_card`, `bark_color`,
`leaf_color`, `resolution`, `seed`) or an explicit `seed`.

`bark_albedo`, `bark_normal`, `leaf_albedo_alpha` are file-override slots that
native hosts resolve relative to the species document — the same slots the
project sidecar fills when maps arrive over the tool bus.

`[materials]` (`bark`, `foliage`, `notes`) is metadata only — names for
external pipelines — not a texture contract.

What remains parked: host-side PBR asset derivation (roughness/metallic/
translucency maps from real-world sources) is not implemented; the procedural
maps cover albedo, normal, and alpha only.

## LOD

`[lod] preset` selects a built-in profile; `count` overrides the level count;
`levels` supplies explicit per-level configs (`index`, `target_triangles`,
`max_triangles`, `branch_levels`, `leaf_geometry`, `leaf_reduction`,
`ring_resolution` as `[trunk,l1,l2,l3]`, `screen_height`, `crown_impostor`).
`crown_impostor = true` bakes a front/side impostor atlas (from the species'
`[textures]` parameters) onto crossed cards for that LOD.

See `presets/species/` for complete examples.
