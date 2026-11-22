# Botanical generators: rosettes, inflorescences, leaf shapes, bark layers, states

Design note for the generator features added with the south-west Western
Australian flora (`docs/wa-flora.md`). It was written before the code and is
kept as the reference for what each parameter means. The human schema summary
stays in [`species-schema.md`](species-schema.md); the JSON Schema is
`schemas/species.schema.json`.

## Why

The Weber-Penn and dichotomous families grow a branching woody skeleton and
hang leaf cards on it. Several plants that define the Perth bush are not that:

| Plant | What it is | Needed |
|-------|------------|--------|
| Grass tree (*Xanthorrhoea*) | blackened trunk, dead-leaf skirt, sphere of long needle leaves, flower spike | rosette generator |
| Zamia (*Macrozamia*) | trunkless cycad: a rosette of pinnate fronds, cones in the centre | rosette generator, pinnate blade |
| Kangaroo paw, sedges, spinifex | clump of strap leaves, flower scapes | rosette generator |
| Canary Island date palm | thick trunk, dense crown of pinnate fronds, dead-frond skirt | rosette generator, pinnate blade |
| Banksias | erect cylindrical flower spikes and persistent old cones | inflorescence primitive, serrated leaf |
| Marri | large urn-shaped "honky nuts" | fruit primitive |
| WA Christmas tree, jacaranda | flowers dense enough to colour the crown | flower-cluster primitive, seasonal state |
| Eucalypts | pendulous sickle leaves; stringy, tessellated, patch-shedding barks | leaf shape, bark styles |
| Sheoaks | drooping jointed branchlets instead of leaves | leaf shape |
| Paperbarks | papery layered bark | bark style |

## Rules every feature follows

- **Opt-in.** Each feature is reached only through a new enum value or a new
  optional section. A species document that uses none of them takes exactly
  the code path it took before, and every preset that existed before this
  change exports byte-identical files (checked by hashing, see
  `docs/wa-flora.md`).
- **Same pipeline.** Everything ends up as the shared `Tree` (stems, leaves,
  and the new `organs` list), so LOD generation, impostor baking, budgets,
  glTF export with split bark/foliage materials, metadata, WASM and FFI are
  unchanged. No new material slot is introduced: importers that bind one bark
  and one foliage material keep working.
- **Deterministic.** Seeded from the tree seed like leaves are; no clocks, no
  hash maps in iteration order.
- **Budgeted.** Blades and organs count against `--max-leaves`; the estimate
  refuses a document before generation when it cannot fit.
- **Validated.** Each new section has field-level checks in
  `Species::validate()` with dotted paths (`rosette.blade_length`, ...).

## The foliage atlas

The exporter binds one foliage texture. A species that needs more than a leaf
card (a rosette, or any species with `[inflorescence]`) gets that texture as a
**2 x 2 atlas** of equal cells at the species' `textures.resolution`:

| Cell | u, v origin | Branching tree with `[inflorescence]` | Rosette |
|------|-------------|----------------------------------------|---------|
| 0 | 0, 0 | the ordinary leaf card | live blade |
| 1 | 0.5, 0 | fresh inflorescence / fruit | flower spike (scape below, spike above) |
| 2 | 0, 0.5 | aged inflorescence (old cone, spent flowers, pods) | dead skirt blade |
| 3 | 0.5, 0.5 | unused (transparent) | raceme flower |

Leaf UVs are remapped into cell 0 with a small inset against bleeding. Each
cell is painted at half the texture resolution, so a species that wants the
old leaf sharpness sets `resolution = 2048`. Species without an atlas are
untouched.

## `generator.family = "rosette"` and `[rosette]`

A rosette is an optional trunk carrying one or more crowns of long blades.

```toml
[generator]
family = "rosette"

[trunk]                 # as for every family; a tiny height gives a trunkless plant
height = 1.6
radius = 0.16

[rosette]
crowns = 1              # heads; more than one forks the trunk into that many arms
fork_height = 0.55      # 0..1 of trunk height where the arms leave (crowns > 1)
fork_angle = 32.0       # degrees between an arm and the trunk axis
blades = 320            # live blades per crown
blade_length = 1.0      # metres
blade_length_variance = 0.15
blade_width = 0.03      # metres; width of the ribbon (a tuft card holds several leaves)
blade_card = "tuft"     # single | tuft | pinnate
spread_min = 5.0        # degrees from the crown axis for the youngest, central blades
spread_max = 150.0      # degrees for the oldest, outer blades; above 90 they hang
droop = 0.35            # 0 = straight .. 1 = strongly arching under gravity
stiffness = 0.7         # 0 = limp .. 1 = rigid; less waver, and the Pivot Painter stiffness
keel = 0.2              # 0 = flat ribbon .. 1 = deep V fold along the midrib
color_base = [0.20, 0.33, 0.14]   # optional; blade colour at the base (sRGB)
color_tip = [0.42, 0.52, 0.26]    # optional; blade colour at the tip
skirt = 0.6             # 0..1 of the stem below each crown clothed in dead blades
skirt_blades = 160      # dead blades per crown (0 = none)
skirt_color = [0.40, 0.31, 0.20]

[rosette.spike]
count = 1               # spikes attempted per crown, each subject to `chance`
chance = 0.6            # probability that a spike appears (decided by the seed)
style = "cylinder"      # cylinder | raceme | cone
scape_length = 0.9      # bare stalk, metres
scape_radius = 0.012
length = 1.3            # flowering part, metres
radius = 0.025
lean = 8.0              # maximum lean from the crown axis, degrees
color = [0.36, 0.26, 0.16]
tip_color = [0.80, 0.76, 0.58]
scape_color = [0.30, 0.24, 0.16]
flowers = 0             # raceme only: tubular flowers at the top of the scape
flower_length = 0.08
```

- **Blades** are `Leaf` instances drawn as curved ribbons (not cards): the
  launch direction comes from the blade's rank (young and upright in the
  centre, old and spread at the rim, laid out on a golden-angle spiral), and
  the ribbon then bends under `droop`. `blade_card` picks what is painted on
  the ribbon: `single` one strap leaf with a midrib (kangaroo paw, sedge
  straps), `tuft` several fine needle leaves side by side so a few hundred
  ribbons read as thousands of leaves (grass tree, rushes, spinifex),
  `pinnate` a rachis with paired leaflets (zamia, date palm).
- **Skirt** blades are dead blades hanging down the stem below the crown,
  painted in `skirt_color`. They are organs, so they do not dilute the leaf
  count and thin out with the LOD's `leaf_reduction`.
- **Spike** styles: `cylinder` is a scape with a long cylindrical flower spike
  (grass tree); `raceme` a scape topped by a one-sided fan of tubular flowers
  (kangaroo paw); `cone` a stout ovoid sitting in the crown (cycad cones).
- **Forked trunks** (`crowns > 1`) put each crown on its own arm; old grass
  trees fork after flowering.
- **LODs.** Ribbon rows drop with the LOD's leaf geometry (`polygon` and
  `cross_billboard` 6 rows, `billboard` and `canopy_card` 3), blades are
  thinned by `leaf_reduction` and widened to hold coverage, and impostor LODs
  bake the real blades.
- `leaves.count` is ignored by this family; `leaves.size`, `shape` and
  `geometry` are unused. `[canopy]` tint/AO and normals apply to blades.

## `[inflorescence]`: spikes, nuts, flower clusters, berries

Optional on the branching families (`weber_penn`, `dichotomous`).

```toml
[inflorescence]
kind = "spike"          # spike | nut | cluster | berries
count = 40              # organs on the whole tree
min_level = 2           # only stems at or beyond this branch level carry them
placement = "endpoint"  # endpoint | along_branch
orientation = "up"      # up | branch | hang
length = 0.20           # metres (spike/nut length, cluster/berry card size)
width = 0.08            # metres (spike/nut diameter; ignored by cards)
size_variance = 0.2
offset = 0.0            # metres between the attachment and the organ base, along its axis;
                        # lifts terminal spikes clear of the leaf spray around the shoot tip
color = [0.86, 0.72, 0.20]       # fresh colour
color_tip = [0.80, 0.30, 0.22]   # colour toward the tip / centre (unopened florets, throat)
aged_fraction = 0.35             # share of organs drawn as aged
aged_color = [0.36, 0.31, 0.27]  # old cones, spent flowers, pods
```

- `spike`: a capped cylinder painted with spiralled florets — banksia flower
  spikes; `aged` ones are the grey cones with follicle lips.
- `nut`: an urn-shaped lathe (marri "honky nuts", large gumnuts).
- `cluster`: crossed cards painted with a panicle of small flowers, meant to
  be used in the hundreds so the crown takes the flower colour (WA Christmas
  tree, jacaranda, eucalypt blossom).
- `berries`: crossed cards painted with hanging round fruit on fine stalks
  (cape lilac in winter).
- Organs thin with the LOD's `leaf_reduction`, disappear with the leaves at
  `leaf_geometry = "none"`, and are baked into crown impostors, so a
  flowering tree keeps its colour at distance.

## New leaf shapes (`leaves.shape`, `textures.leaf_shape`)

| Value | Shape | For |
|-------|-------|-----|
| `sickle` | long falcate lanceolate blade, curved to one side | adult eucalypt leaves |
| `banksia` | long narrow strap with saw-toothed margins and a blunt tip | banksias |
| `bipinnate` | twice-divided fern-like leaf (painted as a frond of tiny leaflets) | jacaranda, wattles |
| `branchlet` | drooping tuft of jointed wire-like branchlets | sheoaks |

All four work with every leaf geometry and card layout; `polygon` geometry
uses the outline of the whole leaf.

## New bark styles (`textures.bark_style`) and bark layers

| Value | Relief and colour | For |
|-------|-------------------|-----|
| `stringy` | long vertical fibrous strands with shallow interlaced furrows | jarrah, peppermint |
| `tessellated` | small squarish plates separated by cracks | marri, tuart ("box" bark) |
| `patchy` | smooth bark shed in large flakes, exposing differently coloured patches | karri, wandoo, plane, lemon-scented gum |
| `papery` | thin overlapping torn sheets | paperbarks |

Two optional `[textures]` parameters layer on any style:

```toml
bark_patch_colors = [[0.80, 0.74, 0.62], [0.78, 0.52, 0.32]]  # up to 4 extra patch colours (patchy), sheet-edge colours (papery)
bark_kino = 0.5      # 0..1 amount of dark red gum (kino) bleeding down from cracks
```

Fire-blackened bases are a vertex-colour layer, because the bark texture
tiles along the trunk and cannot know its height:

```toml
[trunk]
char_height = 1.8    # metres of trunk blackened from the ground (0 = off)
char_strength = 0.85 # 0..1 how black
```

It multiplies the bark `COLOR_0` and therefore needs
`canopy.vertex_color = "tint_ao"` (in Pivot Painter mode `COLOR_0` carries
wind data and the setting is ignored).

## Trunk base: `trunk.buttress`

The mesher gives every substantial trunk a flared base with five root flutes,
and exposed surface roots when the trunk is thick and tapered. That suits an
oak and is wrong for a palm or a grass tree, and too timid for a fig.

```toml
[trunk]
buttress = 1.0   # 0 = plain column, no surface roots; 1 = the built-in look; up to 3 = heavy buttresses
```

It scales the base flare, the flute depth and switches the surface roots off
at 0. The default leaves existing trees exactly as they were.

## `[states.<name>]`: seasonal and condition variants

A species document may carry named overlays:

```toml
[states.flowering]
leaves.count = 300
inflorescence.count = 1400

[states.bare]
leaves.count = 0
```

`midori generate --state flowering` (and `maps`, `info`) deep-merges the
overlay onto the document before validation, so one file holds a species and
its states. Tables merge key by key; any other value replaces. A state may
set any parameter, including ones in `[inflorescence]`, `[rosette]` or
`[textures]`. Library: `Species::from_toml_with_state()`,
`Species::from_file_with_state()`, `Species::state_names()`.

Two things to know when writing states:

- Node and scene names in the export come from `species.name`, so a state
  that should be told apart in an engine sets `species.name` too
  (`species.name = "Jacaranda (flowering)"`).
- The procedural texture seed is derived from `species.name` unless
  `textures.seed` is set. A file whose states rename the species should set
  `textures.seed` so every state shares one bark and one foliage atlas.

## What is still not expressible

See "Generator gaps" in `docs/wa-flora.md` for the list kept against real
species (fan palms, aerial roots, pollard knuckles, grafted multi-trunk
mallees, per-organ materials).
