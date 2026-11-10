//! Alpha-card groundcover for nature patches: a procedurally painted species atlas,
//! card-clump prototypes that sample it, and an optional external scatter mask.
//!
//! Blade prototypes (`nature.rs`) read well from above and at mid range; a meadow seen
//! from a low camera needs silhouettes (seed heads, flower umbels, lupin spikes, fern
//! fronds) that cheap geometry can't carry. A groundcover layer with a `[cards]` block
//! is built as a clump of alpha-tested cards instead, each card one species cell of a
//! shared RGBA atlas (`textures/groundcover_cards.png`) that the package writes. Cards
//! keep the blade vertex packing (wind phase/stiffness, height/variation/t), so the
//! same foliage material and wind contract apply.
//!
//! The scatter mask lets a host drive density from its own data (a road and verge mask,
//! clearings round props): a grayscale PNG mapped over an X/Z extent multiplies every
//! layer's density, and `clearings` are discs with no instances at all.

use crate::Rng;
use crate::mesh::{MaterialType, Mesh, Submesh, Vertex};
use glam::{Vec2, Vec3, Vec4};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Painted species available as card atlas cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardSpecies {
    /// Fine green meadow grass.
    Grass,
    /// Dry, golden grass.
    GoldenGrass,
    /// Grass with oat-like seed heads.
    OatSeeds,
    /// White oxeye daisies.
    Daisy,
    /// Yellow buttercups.
    Buttercup,
    /// Pink clover heads.
    Clover,
    /// Blue bellflowers.
    Bellflower,
    /// Purple/pink lupin spikes.
    Lupin,
    /// Fern frond.
    Fern,
    /// White yarrow umbels.
    Yarrow,
    /// Red poppies.
    Poppy,
}

impl CardSpecies {
    /// Snake-case label used in manifests.
    pub fn label(self) -> &'static str {
        match self {
            CardSpecies::Grass => "grass",
            CardSpecies::GoldenGrass => "golden_grass",
            CardSpecies::OatSeeds => "oat_seeds",
            CardSpecies::Daisy => "daisy",
            CardSpecies::Buttercup => "buttercup",
            CardSpecies::Clover => "clover",
            CardSpecies::Bellflower => "bellflower",
            CardSpecies::Lupin => "lupin",
            CardSpecies::Fern => "fern",
            CardSpecies::Yarrow => "yarrow",
            CardSpecies::Poppy => "poppy",
        }
    }
}

/// `[groundcover.layers.cards]`: build this layer's prototype as alpha cards.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CardParams {
    /// Species the clump's cards are drawn from (each card picks one).
    pub species: Vec<CardSpecies>,
    /// Cards per clump at LOD0 (LOD1 has half, LOD2 one).
    #[serde(default = "default_card_count")]
    pub count: u32,
    /// Card width in world units (the layer's `height` is the card height).
    #[serde(default = "default_card_width")]
    pub card_width: f32,
    /// Per-card height range, as multipliers of the layer height.
    #[serde(default = "default_card_height_range")]
    pub height_range: [f32; 2],
    /// Lean off upright per card, degrees [min, max] (either way, across the card).
    #[serde(default = "default_card_lean")]
    pub lean_degrees: [f32; 2],
    /// Cards stand apart by up to this many units (0: all crossed through the clump centre).
    #[serde(default = "default_card_loose")]
    pub loose: f32,
    /// Normals blended towards +Y (0: card face normals, 1: straight up), so cards light
    /// like the ground they stand on rather than like walls.
    #[serde(default = "default_card_normal_up")]
    pub normal_up: f32,
}

/// `[groundcover.card_atlas]`: how the shared species atlas is painted.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CardAtlasParams {
    /// Pixels per species cell in the written atlas.
    #[serde(default = "default_atlas_cell")]
    pub cell: u32,
    /// Grass colour at the root (sRGB hex).
    #[serde(default = "default_atlas_base")]
    pub base: String,
    /// Grass tip colours (sRGB hex), picked per blade.
    #[serde(default = "default_atlas_tips")]
    pub tips: Vec<String>,
    /// Seed offset for the painter.
    #[serde(default)]
    pub seed: u64,
}

impl Default for CardAtlasParams {
    fn default() -> Self {
        Self {
            cell: default_atlas_cell(),
            base: default_atlas_base(),
            tips: default_atlas_tips(),
            seed: 0,
        }
    }
}

/// `[scatter_mask]`: an external density mask over the patch.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScatterMaskConfig {
    /// Grayscale (or RGB, red used) PNG, relative to the patch TOML.
    #[serde(default)]
    pub path: String,
    /// World X/Z rectangle the image covers: [x_min, z_min, x_max, z_max]. Image row 0
    /// is `z_max` (a map seen from above, +Z up the page), column 0 is `x_min`.
    #[serde(default)]
    pub extent: [f32; 4],
    /// Discs with no instances: [x, z, radius].
    #[serde(default)]
    pub clearings: Vec<[f32; 3]>,
}

/// A loaded scatter mask.
#[derive(Debug, Clone, Default)]
pub struct ScatterMaskData {
    width: u32,
    height: u32,
    values: Vec<f32>,
    extent: [f32; 4],
    clearings: Vec<[f32; 3]>,
    loaded: bool,
}

impl ScatterMaskData {
    /// Load the mask image (if any) named by `config`, relative to `base_dir`.
    pub fn load(config: &ScatterMaskConfig, base_dir: &Path) -> Result<Self, String> {
        let mut data = ScatterMaskData {
            extent: config.extent,
            clearings: config.clearings.clone(),
            ..Default::default()
        };
        if !config.path.is_empty() {
            let path = base_dir.join(&config.path);
            let image = image::open(&path)
                .map_err(|err| format!("scatter_mask.path {}: {err}", path.display()))?
                .to_luma8();
            data.width = image.width();
            data.height = image.height();
            data.values = image.pixels().map(|p| p.0[0] as f32 / 255.0).collect();
            data.loaded = true;
        }
        Ok(data)
    }

    /// Build a mask from raw values (row 0 = `z_max`), for hosts that have no file.
    pub fn from_values(
        width: u32,
        height: u32,
        values: Vec<f32>,
        extent: [f32; 4],
        clearings: Vec<[f32; 3]>,
    ) -> Self {
        assert_eq!(values.len(), (width * height) as usize);
        ScatterMaskData {
            width,
            height,
            values,
            extent,
            clearings,
            loaded: true,
        }
    }

    /// Density multiplier at world X/Z: bilinear in the image (0 outside its extent), 0
    /// inside a clearing.
    pub fn sample(&self, x: f32, z: f32) -> f32 {
        for c in &self.clearings {
            let (dx, dz) = (x - c[0], z - c[1]);
            if dx * dx + dz * dz < c[2] * c[2] {
                return 0.0;
            }
        }
        if !self.loaded {
            return 1.0;
        }
        let [x0, z0, x1, z1] = self.extent;
        if x1 <= x0 || z1 <= z0 || x < x0 || x > x1 || z < z0 || z > z1 {
            return 0.0;
        }
        let fx = (x - x0) / (x1 - x0) * self.width as f32 - 0.5;
        let fy = (z1 - z) / (z1 - z0) * self.height as f32 - 0.5;
        let ix = fx.floor();
        let iy = fy.floor();
        let (ax, ay) = (fx - ix, fy - iy);
        let at = |i: f32, j: f32| {
            let i = (i as i64).clamp(0, self.width as i64 - 1) as usize;
            let j = (j as i64).clamp(0, self.height as i64 - 1) as usize;
            self.values[j * self.width as usize + i]
        };
        let top = at(ix, iy) * (1.0 - ax) + at(ix + 1.0, iy) * ax;
        let bottom = at(ix, iy + 1.0) * (1.0 - ax) + at(ix + 1.0, iy + 1.0) * ax;
        top * (1.0 - ay) + bottom * ay
    }
}

/// Where each species sits in the atlas: (u0, v0, cell size in UV), V up (glTF: v = 0 at
/// the top of the image, so cells are addressed top-down).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardAtlasCell {
    /// Species painted in this cell.
    pub species: CardSpecies,
    /// Cell column.
    pub column: u32,
    /// Cell row (0 = top of the image).
    pub row: u32,
}

/// The painted atlas and its layout.
#[derive(Debug, Clone)]
pub struct CardAtlas {
    /// Cells per side.
    pub grid: u32,
    /// Species cells in painting order.
    pub cells: Vec<CardAtlasCell>,
    /// RGBA image (sRGB colour, straight alpha, colour bled into transparent texels).
    pub image: RgbaImage,
}

impl CardAtlas {
    /// UV rectangle (u0, v0, u1, v1) of a species' cell, glTF convention (v down the image).
    pub fn uv_rect(&self, species: CardSpecies) -> [f32; 4] {
        let cell = self
            .cells
            .iter()
            .find(|c| c.species == species)
            .expect("species is in the atlas");
        let s = 1.0 / self.grid as f32;
        let e = s * 0.012;
        [
            cell.column as f32 * s + e,
            cell.row as f32 * s + e,
            (cell.column + 1) as f32 * s - e,
            (cell.row + 1) as f32 * s - e,
        ]
    }
}

/// Paint the atlas for every species used by the card layers, in a stable order.
pub fn bake_card_atlas(
    species: &[CardSpecies],
    params: &CardAtlasParams,
    patch_seed: u64,
) -> CardAtlas {
    let mut list: Vec<CardSpecies> = species.to_vec();
    list.sort();
    list.dedup();
    let grid = (list.len() as f32).sqrt().ceil().max(1.0) as u32;
    let cell = params.cell.max(32);
    let ss = 2; // supersampling
    let size = grid * cell * ss;
    let mut canvas = Canvas::new(size, size);
    let base = parse_hex(&params.base).unwrap_or([0.3, 0.4, 0.16]);
    let tips: Vec<[f32; 3]> = params.tips.iter().filter_map(|t| parse_hex(t)).collect();
    let tips = if tips.is_empty() {
        vec![[0.66, 0.74, 0.3]]
    } else {
        tips
    };
    let mut cells = Vec::new();
    for (i, sp) in list.iter().enumerate() {
        let column = i as u32 % grid;
        let row = i as u32 / grid;
        let c = (cell * ss) as f32;
        let mut rng =
            Rng::from_seed(patch_seed ^ params.seed ^ 0xCA5D_0000 ^ ((i as u64 + 1) * 0x9E37_79B9));
        let mut painter = Painter {
            canvas: &mut canvas,
            ox: column as f32 * c,
            oy: row as f32 * c,
            c,
            base,
            rng: &mut rng,
        };
        painter.paint(*sp, &tips);
        cells.push(CardAtlasCell {
            species: *sp,
            column,
            row,
        });
    }
    let image = canvas.downsample(ss);
    CardAtlas { grid, cells, image }
}

/// The species used by every card layer of a patch's groundcover.
pub fn card_species_of(layers: &[crate::nature::GroundcoverLayer]) -> Vec<CardSpecies> {
    let mut all: Vec<CardSpecies> = layers
        .iter()
        .filter_map(|l| l.cards.as_ref())
        .flat_map(|c| c.species.iter().copied())
        .collect();
    all.sort();
    all.dedup();
    all
}

/// One card-clump LOD: `count` cards, each a species cell, leaning and spread per `cards`.
pub fn build_card_clump_mesh(
    height: f32,
    cards: &CardParams,
    atlas: &CardAtlas,
    count: u32,
    lod_index: u32,
    seed: u64,
) -> Mesh {
    let mut rng = Rng::from_seed(seed ^ 0x6361_7264_0000 ^ ((lod_index as u64 + 1) << 8));
    let mut mesh = Mesh::new();
    let species = if cards.species.is_empty() {
        vec![CardSpecies::Grass]
    } else {
        cards.species.clone()
    };
    let base_height = height.max(0.05);
    let half_w = cards.card_width.max(0.02) * 0.5;
    let a0 = rng.range(0.0, std::f32::consts::PI);
    let up = cards.normal_up.clamp(0.0, 1.0);
    for card in 0..count.max(1) {
        let sp = species[rng.index(species.len())];
        let [u0, v0, u1, v1] = atlas.uv_rect(sp);
        let h = base_height
            * rng.range(
                cards.height_range[0],
                cards.height_range[1].max(cards.height_range[0]),
            );
        let w = half_w * rng.range(0.8, 1.2);
        let yaw = if cards.loose > 0.0 {
            rng.range(0.0, std::f32::consts::PI)
        } else {
            a0 + card as f32 * std::f32::consts::PI / count.max(1) as f32 + rng.range(-0.2, 0.2)
        };
        let lean = rng
            .range(
                cards.lean_degrees[0],
                cards.lean_degrees[1].max(cards.lean_degrees[0]),
            )
            .to_radians()
            * if rng.next_f32() < 0.5 { -1.0 } else { 1.0 };
        let centre = Vec3::new(
            rng.range(-1.0, 1.0) * cards.loose,
            0.0,
            rng.range(-1.0, 1.0) * cards.loose,
        );
        let along = Vec3::new(yaw.cos(), 0.0, yaw.sin());
        let across = Vec3::new(-yaw.sin(), 0.0, yaw.cos());
        let top = Vec3::Y * (h * lean.cos()) + across * (h * lean.sin());
        let phase = rng.range(0.0, std::f32::consts::TAU);
        let variation = rng.next_f32();
        // face normal, blended towards +Y
        let face = across * lean.cos() - Vec3::Y * lean.sin();
        let n = (face * (1.0 - up) + Vec3::Y * up).normalize_or_zero();
        let start = mesh.vertices.len() as u32;
        for (p, uv, t) in [
            (centre - along * w, Vec2::new(u0, v1), 0.0),
            (centre + along * w, Vec2::new(u1, v1), 0.0),
            (centre + along * w + top, Vec2::new(u1, v0), 1.0),
            (centre - along * w + top, Vec2::new(u0, v0), 1.0),
        ] {
            mesh.vertices.push(Vertex {
                position: p,
                normal: n,
                uv,
                uv2: Vec2::new(phase, 1.0 - t),
                color: Vec4::new(h, variation, t, 1.0 - t),
            });
        }
        mesh.indices
            .extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
    }
    mesh.submeshes.push(Submesh {
        index_start: 0,
        index_count: mesh.indices.len() as u32,
        material: MaterialType::Leaves,
    });
    mesh
}

fn default_card_count() -> u32 {
    4
}
fn default_card_width() -> f32 {
    0.4
}
fn default_card_height_range() -> [f32; 2] {
    [0.65, 1.35]
}
fn default_card_lean() -> [f32; 2] {
    [4.0, 28.0]
}
fn default_card_loose() -> f32 {
    0.1
}
fn default_card_normal_up() -> f32 {
    0.75
}
fn default_atlas_cell() -> u32 {
    256
}
fn default_atlas_base() -> String {
    "#57702b".to_string()
}
fn default_atlas_tips() -> Vec<String> {
    vec![
        "#a8bd4d".to_string(),
        "#ccc25c".to_string(),
        "#8fae42".to_string(),
    ]
}

fn parse_hex(s: &str) -> Option<[f32; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([
        ((v >> 16) & 0xff) as f32 / 255.0,
        ((v >> 8) & 0xff) as f32 / 255.0,
        (v & 0xff) as f32 / 255.0,
    ])
}

// ---------------------------------------------------------------------------- painting

/// A straight-alpha RGBA float canvas with polygon fill.
struct Canvas {
    w: u32,
    h: u32,
    px: Vec<[f32; 4]>,
}

impl Canvas {
    fn new(w: u32, h: u32) -> Self {
        Canvas {
            w,
            h,
            px: vec![[0.0; 4]; (w * h) as usize],
        }
    }

    /// Fill a polygon (even-odd, sampled at pixel centres).
    fn polygon(&mut self, pts: &[(f32, f32)], c: [f32; 3]) {
        if pts.len() < 3 {
            return;
        }
        let y_min = pts
            .iter()
            .map(|p| p.1)
            .fold(f32::MAX, f32::min)
            .floor()
            .max(0.0) as i64;
        let y_max = pts
            .iter()
            .map(|p| p.1)
            .fold(f32::MIN, f32::max)
            .ceil()
            .min(self.h as f32 - 1.0) as i64;
        let mut xs: Vec<f32> = Vec::with_capacity(8);
        for y in y_min..=y_max {
            let fy = y as f32 + 0.5;
            xs.clear();
            for i in 0..pts.len() {
                let (x0, y0) = pts[i];
                let (x1, y1) = pts[(i + 1) % pts.len()];
                if (y0 <= fy && y1 > fy) || (y1 <= fy && y0 > fy) {
                    xs.push(x0 + (fy - y0) / (y1 - y0) * (x1 - x0));
                }
            }
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for pair in xs.chunks(2) {
                if pair.len() < 2 {
                    break;
                }
                let a = (pair[0] - 0.5).ceil().max(0.0) as i64;
                let b = (pair[1] - 0.5).floor().min(self.w as f32 - 1.0) as i64;
                for x in a..=b {
                    self.px[(y as u32 * self.w + x as u32) as usize] = [c[0], c[1], c[2], 1.0];
                }
            }
        }
    }

    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, c: [f32; 3]) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let l = (dx * dx + dy * dy).sqrt().max(1e-4);
        let (nx, ny) = (-dy / l * width * 0.5, dx / l * width * 0.5);
        self.polygon(
            &[
                (a.0 + nx, a.1 + ny),
                (b.0 + nx, b.1 + ny),
                (b.0 - nx, b.1 - ny),
                (a.0 - nx, a.1 - ny),
            ],
            c,
        );
    }

    fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, c: [f32; 3]) {
        let pts: Vec<(f32, f32)> = (0..14)
            .map(|i| {
                let a = i as f32 / 14.0 * std::f32::consts::TAU;
                (cx + a.cos() * rx, cy + a.sin() * ry)
            })
            .collect();
        self.polygon(&pts, c);
    }

    /// Box-filter down by `f`, then bleed colour into transparent texels (so mips and
    /// filtering don't fringe dark), as 8-bit sRGB RGBA.
    fn downsample(&self, f: u32) -> RgbaImage {
        let (w, h) = (self.w / f, self.h / f);
        let mut out = vec![[0.0f32; 4]; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0.0f32; 4];
                for j in 0..f {
                    for i in 0..f {
                        let p = self.px[((y * f + j) * self.w + x * f + i) as usize];
                        acc[0] += p[0] * p[3];
                        acc[1] += p[1] * p[3];
                        acc[2] += p[2] * p[3];
                        acc[3] += p[3];
                    }
                }
                let n = (f * f) as f32;
                let a = acc[3] / n;
                out[(y * w + x) as usize] = if acc[3] > 0.0 {
                    [acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3], a]
                } else {
                    [0.0, 0.0, 0.0, 0.0]
                };
            }
        }
        // colour bleed: a few passes of averaging opaque neighbours into empty texels
        let mut filled: Vec<bool> = out.iter().map(|p| p[3] > 0.0).collect();
        for _ in 0..6 {
            let prev = out.clone();
            let prev_filled = filled.clone();
            for y in 0..h as i64 {
                for x in 0..w as i64 {
                    let idx = (y * w as i64 + x) as usize;
                    if prev_filled[idx] {
                        continue;
                    }
                    let mut acc = [0.0f32; 3];
                    let mut n = 0.0;
                    for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                        let (nx, ny) = (x + dx, y + dy);
                        if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                            continue;
                        }
                        let j = (ny * w as i64 + nx) as usize;
                        if prev_filled[j] {
                            acc[0] += prev[j][0];
                            acc[1] += prev[j][1];
                            acc[2] += prev[j][2];
                            n += 1.0;
                        }
                    }
                    if n > 0.0 {
                        out[idx] = [acc[0] / n, acc[1] / n, acc[2] / n, 0.0];
                        filled[idx] = true;
                    }
                }
            }
        }
        let mut img = RgbaImage::new(w, h);
        for (i, p) in out.iter().enumerate() {
            let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            img.put_pixel(
                i as u32 % w,
                i as u32 / w,
                Rgba([q(p[0]), q(p[1]), q(p[2]), q(p[3])]),
            );
        }
        img
    }
}

struct Painter<'a> {
    canvas: &'a mut Canvas,
    ox: f32,
    oy: f32,
    c: f32,
    base: [f32; 3],
    rng: &'a mut Rng,
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn scale(a: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}

impl Painter<'_> {
    fn r(&mut self) -> f32 {
        self.rng.next_f32()
    }

    /// Tapered, leaning blades rising from the cell's bottom edge, dark at the root.
    fn blades(&mut self, count: u32, tips: &[[f32; 3]], hmin: f32, hmax: f32) {
        let c = self.c;
        for _ in 0..count {
            let x0 = self.ox + c * (0.12 + 0.76 * self.r());
            let hgt = c * (hmin + (hmax - hmin) * self.r());
            let lean = (self.r() - 0.5) * c * 0.55;
            let w = c * (0.011 + 0.012 * self.r());
            let tip = scale(tips[self.rng.index(tips.len())], 0.85 + 0.3 * self.r());
            for k in 0..6 {
                let (t0, t1) = (k as f32 / 6.0, (k + 1) as f32 / 6.0);
                let col = mix(self.base, tip, ((t0 + t1) * 0.5).powf(0.8));
                let (xa, xb) = (x0 + lean * t0 * t0, x0 + lean * t1 * t1);
                let (ya, yb) = (self.oy + c - hgt * t0, self.oy + c - hgt * t1);
                let (wa, wb) = (w * (1.0 - t0 * 0.85), w * (1.0 - t1 * 0.85));
                self.canvas.polygon(
                    &[(xa - wa, ya), (xa + wa, ya), (xb + wb, yb), (xb - wb, yb)],
                    col,
                );
            }
        }
    }

    /// Flower heads on stems: `petals` round a centre (none: a plain head).
    #[allow(clippy::too_many_arguments)]
    fn heads(
        &mut self,
        count: u32,
        colours: &[[f32; 3]],
        rad: f32,
        ymin: f32,
        ymax: f32,
        petals: u32,
        centre: Option<[f32; 3]>,
    ) {
        let c = self.c;
        for _ in 0..count {
            let fx = self.ox + c * (0.15 + 0.7 * self.r());
            let fy = self.oy + c * (ymin + (ymax - ymin) * self.r());
            let stem_x = fx + (self.r() - 0.5) * c * 0.03;
            self.canvas.line(
                (fx, fy),
                (stem_x, self.oy + c),
                c * 0.008,
                scale(self.base, 1.3),
            );
            let col = colours[self.rng.index(colours.len())];
            let r = c * rad * (0.7 + 0.6 * self.r());
            if petals == 0 {
                self.canvas.ellipse(fx, fy, r, r, col);
            } else {
                for p in 0..petals {
                    let a = p as f32 * std::f32::consts::TAU / petals as f32;
                    self.canvas
                        .ellipse(fx + a.cos() * r, fy + a.sin() * r, r * 0.7, r * 0.7, col);
                }
            }
            if let Some(cc) = centre {
                self.canvas.ellipse(fx, fy, r * 0.6, r * 0.6, cc);
            }
        }
    }

    fn paint(&mut self, species: CardSpecies, tips: &[[f32; 3]]) {
        let c = self.c;
        let (ox, oy) = (self.ox, self.oy);
        let golden = [[0.82, 0.74, 0.42], [0.74, 0.66, 0.36]];
        match species {
            CardSpecies::Grass => self.blades(75, tips, 0.45, 0.95),
            CardSpecies::GoldenGrass => self.blades(60, &golden, 0.5, 1.0),
            CardSpecies::OatSeeds => {
                self.blades(45, tips, 0.4, 0.8);
                for _ in 0..16 {
                    let fx = ox + c * (0.15 + 0.7 * self.r());
                    let fy = oy + c * (0.08 + 0.3 * self.r());
                    let sx = fx + (self.r() - 0.5) * c * 0.02;
                    self.canvas
                        .line((fx, fy), (sx, oy + c), c * 0.006, scale(golden[0], 0.9));
                    for k in 0..6 {
                        let kx = fx + if k % 2 == 0 { -0.006 } else { 0.006 } * c * 1.5;
                        self.canvas.ellipse(
                            kx,
                            fy + k as f32 * c * 0.024,
                            c * 0.009,
                            c * 0.014,
                            golden[k % 2],
                        );
                    }
                }
            }
            CardSpecies::Daisy => {
                self.blades(45, tips, 0.45, 0.95);
                self.heads(
                    18,
                    &[[0.97, 0.96, 0.9]],
                    0.02,
                    0.12,
                    0.55,
                    6,
                    Some([1.0, 0.8, 0.2]),
                );
            }
            CardSpecies::Buttercup => {
                self.blades(45, tips, 0.45, 0.95);
                self.heads(
                    18,
                    &[[1.0, 0.84, 0.18]],
                    0.018,
                    0.12,
                    0.55,
                    5,
                    Some([0.95, 0.7, 0.12]),
                );
            }
            CardSpecies::Clover => {
                self.blades(45, tips, 0.4, 0.8);
                self.heads(
                    16,
                    &[[0.88, 0.52, 0.72], [0.78, 0.42, 0.66]],
                    0.018,
                    0.25,
                    0.6,
                    8,
                    None,
                );
            }
            CardSpecies::Bellflower => {
                self.blades(45, tips, 0.45, 0.95);
                self.heads(
                    16,
                    &[[0.5, 0.6, 0.95], [0.42, 0.5, 0.88]],
                    0.017,
                    0.12,
                    0.5,
                    5,
                    Some([0.9, 0.9, 1.0]),
                );
            }
            CardSpecies::Yarrow => {
                self.blades(40, tips, 0.4, 0.85);
                for _ in 0..8 {
                    let fx = ox + c * (0.15 + 0.7 * self.r());
                    let fy = oy + c * (0.1 + 0.3 * self.r());
                    self.canvas
                        .line((fx, fy), (fx, oy + c), c * 0.007, scale(self.base, 1.3));
                    for _ in 0..14 {
                        let dx = (self.r() - 0.5) * c * 0.09;
                        let dy = (self.r() - 0.5) * c * 0.03;
                        let col = if self.r() < 0.8 {
                            [0.96, 0.95, 0.88]
                        } else {
                            [0.95, 0.85, 0.85]
                        };
                        self.canvas
                            .ellipse(fx + dx, fy + dy, c * 0.013, c * 0.013, col);
                    }
                }
            }
            CardSpecies::Poppy => {
                self.blades(40, tips, 0.4, 0.85);
                self.heads(
                    10,
                    &[[0.88, 0.2, 0.12], [0.94, 0.3, 0.14]],
                    0.024,
                    0.15,
                    0.5,
                    4,
                    Some([0.15, 0.1, 0.08]),
                );
            }
            CardSpecies::Lupin => {
                self.blades(40, tips, 0.35, 0.7);
                for _ in 0..9 {
                    let fx = ox + c * (0.15 + 0.7 * self.r());
                    let fy = oy + c * (0.08 + 0.25 * self.r());
                    self.canvas
                        .line((fx, fy), (fx, oy + c), c * 0.009, scale(self.base, 1.3));
                    let lc = if self.r() < 0.7 {
                        [0.62, 0.42, 0.82]
                    } else {
                        [0.92, 0.6, 0.78]
                    };
                    for k in 0..9 {
                        let rr = c * 0.022 * (1.0 - k as f32 / 12.0);
                        self.canvas.ellipse(
                            fx,
                            fy + k as f32 * c * 0.03,
                            rr,
                            rr,
                            scale(lc, 0.85 + 0.03 * k as f32),
                        );
                    }
                }
            }
            CardSpecies::Fern => {
                for _ in 0..7 {
                    let fx = ox + c * (0.2 + 0.6 * self.r());
                    let hgt = c * (0.5 + 0.4 * self.r());
                    let lean = (self.r() - 0.5) * c * 0.4;
                    for k in 0..14 {
                        let t = k as f32 / 14.0;
                        let (px, py) = (fx + lean * t * t, oy + c - hgt * t);
                        let ln = c * 0.09 * (1.0 - t);
                        let col = mix(self.base, tips[0], 0.3 + 0.7 * t);
                        self.canvas
                            .line((px, py), (px - ln, py - ln * 0.4), c * 0.01, col);
                        self.canvas
                            .line((px, py), (px + ln, py - ln * 0.4), c * 0.01, col);
                    }
                    self.canvas.line(
                        (fx, oy + c),
                        (fx + lean, oy + c - hgt),
                        c * 0.008,
                        scale(self.base, 1.2),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nature::NaturePatch;

    fn meadow() -> NaturePatch {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets/nature/alpine_meadow.toml");
        NaturePatch::from_file(&path).expect("alpine meadow preset parses")
    }

    #[test]
    fn card_atlas_is_deterministic_and_lays_out_every_species() {
        let patch = meadow();
        let a = patch.card_atlas().expect("card layers make an atlas");
        let b = patch.card_atlas().unwrap();
        assert_eq!(a.image.as_raw(), b.image.as_raw());
        assert_eq!(a.cells.len(), 11);
        assert_eq!(a.grid, 4);
        assert_eq!(a.image.width(), 4 * 256);
        // something is painted in every cell
        for cell in &a.cells {
            let (x0, y0) = (cell.column * 256, cell.row * 256);
            let opaque = (0..256)
                .flat_map(|y| (0..256).map(move |x| (x, y)))
                .filter(|&(x, y)| a.image.get_pixel(x0 + x, y0 + y).0[3] > 127)
                .count();
            assert!(opaque > 500, "{:?} cell is nearly empty", cell.species);
        }
    }

    #[test]
    fn card_prototypes_sample_their_species_cells() {
        let patch = meadow();
        let atlas = patch.card_atlas().unwrap();
        let prototypes = patch.generate_groundcover_prototypes();
        assert_eq!(prototypes.len(), patch.groundcover.layers.len());
        for (layer, proto) in patch.groundcover.layers.iter().zip(&prototypes) {
            let cards = layer.cards.as_ref().unwrap();
            assert_eq!(proto.lods.len(), 3);
            assert_eq!(proto.lods[0].mesh.indices.len() as u32, cards.count * 6);
            assert_eq!(proto.lods[2].mesh.indices.len(), 6);
            for v in &proto.lods[0].mesh.vertices {
                let inside = cards.species.iter().any(|sp| {
                    let [u0, v0, u1, v1] = atlas.uv_rect(*sp);
                    v.uv.x >= u0 - 1e-5
                        && v.uv.x <= u1 + 1e-5
                        && v.uv.y >= v0 - 1e-5
                        && v.uv.y <= v1 + 1e-5
                });
                assert!(inside, "uv {:?} outside the layer's species cells", v.uv);
                assert!(v.normal.y > 0.5, "card normals lean up");
            }
        }
    }

    #[test]
    fn scatter_mask_scales_density_and_clearings_empty() {
        let mut patch = meadow();
        let full = patch.generate_scatter_sets(8.0);
        let count = |sets: &[crate::nature::ScatterSet]| -> usize {
            sets.iter()
                .flat_map(|s| &s.chunks)
                .map(|c| c.instances.len())
                .sum()
        };
        let n_full = count(&full);
        assert!(n_full > 0);
        // left half of the patch only, and a clearing in the middle of the right half (empty anyway)
        let half = patch.patch.size * 0.5;
        let (w, h) = (16u32, 16u32);
        let values: Vec<f32> = (0..h)
            .flat_map(|_| (0..w).map(move |x| if x < w / 2 { 1.0 } else { 0.0 }))
            .collect();
        patch.scatter_mask_data = Some(std::sync::Arc::new(ScatterMaskData::from_values(
            w,
            h,
            values,
            [-half, -half, half, half],
            vec![[-half * 0.5, 0.0, half * 0.3]],
        )));
        let masked = patch.generate_scatter_sets(8.0);
        let n_masked = count(&masked);
        assert!(
            n_masked > 0 && n_masked < n_full * 6 / 10,
            "{n_masked} of {n_full}"
        );
        for inst in masked
            .iter()
            .flat_map(|s| &s.chunks)
            .flat_map(|c| &c.instances)
        {
            // (bilinear: the mask fades over one pixel at the edge)
            assert!(
                inst.position[0] < patch.patch.size / w as f32,
                "instance in the masked-out half"
            );
            let (dx, dz) = (inst.position[0] + half * 0.5, inst.position[2]);
            assert!(
                dx * dx + dz * dz >= (half * 0.3) * (half * 0.3),
                "instance in the clearing"
            );
        }
        // deterministic
        assert_eq!(masked, patch.generate_scatter_sets(8.0));
    }
}
