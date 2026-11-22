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
use crate::math::lerp;
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
    /// Silky Miscanthus sinensis (Susuki Japanese pampas grass) plumes.
    Pampas,
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
            CardSpecies::Pampas => "pampas",
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
        let sink = Vec3::Y * (-0.018 * h);
        let start = mesh.vertices.len() as u32;
        for (p, uv, t) in [
            (centre - along * w + sink, Vec2::new(u0, v1), 0.0),
            (centre + along * w + sink, Vec2::new(u1, v1), 0.0),
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

pub(crate) fn parse_hex(s: &str) -> Option<[f32; 3]> {
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

    fn disc(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, inner: [f32; 3], outer: [f32; 3]) {
        if rx < 0.4 || ry < 0.4 {
            return;
        }
        let x_min = (cx - rx - 1.0).floor().max(0.0) as i64;
        let x_max = (cx + rx + 1.0).ceil().min(self.w as f32 - 1.0) as i64;
        let y_min = (cy - ry - 1.0).floor().max(0.0) as i64;
        let y_max = (cy + ry + 1.0).ceil().min(self.h as f32 - 1.0) as i64;
        for y in y_min..=y_max {
            let dy = (y as f32 + 0.5 - cy) / ry;
            for x in x_min..=x_max {
                let dx = (x as f32 + 0.5 - cx) / rx;
                let r2 = dx * dx + dy * dy;
                if r2 <= 1.0 {
                    let c = mix(inner, outer, r2.sqrt());
                    self.px[(y as u32 * self.w + x as u32) as usize] = [
                        c[0].clamp(0.0, 1.0),
                        c[1].clamp(0.0, 1.0),
                        c[2].clamp(0.0, 1.0),
                        1.0,
                    ];
                }
            }
        }
    }

    /// Stamp a rotated, tapered botanical leaf or flower petal with base-to-tip gradient
    /// and subtle lateral convexity shading.
    #[allow(clippy::too_many_arguments)]
    fn stamp_lamina(
        &mut self,
        base_pt: (f32, f32),
        angle: f32,
        length: f32,
        half_width: f32,
        col_base: [f32; 3],
        col_tip: [f32; 3],
        broadness: f32,
    ) {
        if length < 0.8 || half_width < 0.4 {
            return;
        }
        let dir = (angle.cos(), angle.sin());
        let nrm = (-dir.1, dir.0);
        let tip = (base_pt.0 + dir.0 * length, base_pt.1 + dir.1 * length);
        let pad = half_width + 1.5;
        let x_min = base_pt
            .0
            .min(tip.0 - pad)
            .min(base_pt.0 - pad)
            .floor()
            .max(0.0) as i64;
        let x_max = base_pt
            .0
            .max(tip.0 + pad)
            .max(base_pt.0 + pad)
            .ceil()
            .min(self.w as f32 - 1.0) as i64;
        let y_min = base_pt
            .1
            .min(tip.1 - pad)
            .min(base_pt.1 - pad)
            .floor()
            .max(0.0) as i64;
        let y_max = base_pt
            .1
            .max(tip.1 + pad)
            .max(base_pt.1 + pad)
            .ceil()
            .min(self.h as f32 - 1.0) as i64;

        for y in y_min..=y_max {
            let py = y as f32 + 0.5 - base_pt.1;
            for x in x_min..=x_max {
                let px = x as f32 + 0.5 - base_pt.0;
                let along = (px * dir.0 + py * dir.1) / length;
                if !(0.0..=1.0).contains(&along) {
                    continue;
                }
                let across = (px * nrm.0 + py * nrm.1).abs();
                let profile = (along * std::f32::consts::PI)
                    .sin()
                    .max(0.0)
                    .powf(broadness);
                let w_here = half_width * profile;
                if across <= w_here {
                    let edge_norm = across / w_here.max(1e-4);
                    // Subtle midrib crease + convex lamina highlight
                    let camber =
                        0.90 + 0.16 * (1.0 - (edge_norm - 0.38).abs() * 1.3).clamp(0.0, 1.0);
                    let c = scale(mix(col_base, col_tip, along.powf(0.85)), camber);
                    self.px[(y as u32 * self.w + x as u32) as usize] = [
                        c[0].clamp(0.0, 1.0),
                        c[1].clamp(0.0, 1.0),
                        c[2].clamp(0.0, 1.0),
                        1.0,
                    ];
                }
            }
        }
    }

    /// Smoothly shaded quadratic Bezier ribbon stroke from `p0` through `p1` to `p2`.
    #[allow(clippy::too_many_arguments)]
    fn ribbon(
        &mut self,
        p0: (f32, f32),
        p1: (f32, f32),
        p2: (f32, f32),
        w0: f32,
        w1: f32,
        c0: [f32; 3],
        c1: [f32; 3],
        steps: u32,
    ) {
        let steps = steps.max(4);
        let eval = |t: f32| -> (f32, f32) {
            let u = 1.0 - t;
            (
                u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
                u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
            )
        };
        let mut prev = eval(0.0);
        for i in 1..=steps {
            let t0 = (i - 1) as f32 / steps as f32;
            let t1 = i as f32 / steps as f32;
            let curr = eval(t1);
            let (dx, dy) = (curr.0 - prev.0, curr.1 - prev.1);
            let len = (dx * dx + dy * dy).sqrt().max(1e-4);
            let (nx, ny) = (-dy / len, dx / len);
            let wa = (w0 + (w1 - w0) * t0.powf(0.85)).max(0.45);
            let wb = (w0 + (w1 - w0) * t1.powf(0.85)).max(0.35);
            let tm = (t0 + t1) * 0.5;
            let col = mix(c0, c1, tm.powf(0.78));
            let hi = scale(col, 1.06);
            let lo = scale(col, 0.91);
            // Two longitudinal halves give subtle midrib depth instead of flat tape
            self.polygon(
                &[
                    (prev.0 - nx * wa, prev.1 - ny * wa),
                    (prev.0, prev.1),
                    (curr.0, curr.1),
                    (curr.0 - nx * wb, curr.1 - ny * wb),
                ],
                hi,
            );
            self.polygon(
                &[
                    (prev.0, prev.1),
                    (prev.0 + nx * wa, prev.1 + ny * wa),
                    (curr.0 + nx * wb, curr.1 + ny * wb),
                    (curr.0, curr.1),
                ],
                lo,
            );
            prev = curr;
        }
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
        // colour bleed: 16 passes over 8-connected neighbours so mips and bilinear filtering never fringe dark
        let mut filled: Vec<bool> = out.iter().map(|p| p[3] > 0.0).collect();
        for _ in 0..16 {
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
                    for (dx, dy) in [
                        (-1i64, 0i64),
                        (1, 0),
                        (0, -1),
                        (0, 1),
                        (-1, -1),
                        (1, -1),
                        (-1, 1),
                        (1, 1),
                    ] {
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

    /// Organic fountain tuft of smoothly curved Bezier grass blades rising from an irregular
    /// root crown (back shadow layer -> mid layer -> sunlit foreground blades) so card bottoms
    /// blend naturally into terrain without a flat scissor-cut comb.
    fn blades(&mut self, count: u32, tips: &[[f32; 3]], hmin: f32, hmax: f32) {
        let c = self.c;
        for i in 0..count {
            let depth = i as f32 / count.max(1) as f32; // 0 = back/shadow, 1 = front/sunlit
            let spread = (self.r() - 0.5) + (self.r() - 0.5); // triangular around tuft centre
            let x0 = self.ox + c * (0.50 + spread * 0.34).clamp(0.14, 0.86);
            let y0 = self.oy + c * (0.945 + 0.045 * self.r());
            let hgt = c * (hmin + (hmax - hmin) * self.r()) * (1.0 - spread.abs() * 0.16);
            // Blades splay outward gracefully from the tuft crown with natural droop near tips
            let lean = (spread * 0.34 + (self.r() - 0.5) * 0.24) * c;
            let p1 = (
                (x0 + lean * 0.38).clamp(self.ox + c * 0.05, self.ox + c * 0.95),
                y0 - hgt * 0.58,
            );
            let p2 = (
                (x0 + lean).clamp(self.ox + c * 0.04, self.ox + c * 0.96),
                (y0 - hgt + lean.abs() * 0.14).max(self.oy + c * 0.04),
            );
            let w0 = c * (0.0085 + 0.0095 * self.r());
            let shade = 0.68 + 0.44 * depth + 0.14 * self.r();
            let root_col = scale(self.base, 0.58 + 0.30 * depth);
            let tip_col = scale(tips[self.rng.index(tips.len())], shade);
            self.canvas
                .ribbon((x0, y0), p1, p2, w0, c * 0.0014, root_col, tip_col, 18);
        }
    }

    /// Curved botanical stem from irregular tuft root to `(fx, fy)` with optional cauline leaves.
    fn stem_to(&mut self, fx: f32, fy: f32, width: f32, leaf_pairs: u32, tips: &[[f32; 3]]) {
        let c = self.c;
        let root_x =
            (self.ox + c * 0.50 + (fx - (self.ox + c * 0.50)) * 0.58 + (self.r() - 0.5) * c * 0.08)
                .clamp(self.ox + c * 0.16, self.ox + c * 0.84);
        let root_y = self.oy + c * (0.95 + 0.04 * self.r());
        let mid_x = (root_x + fx) * 0.5 + (self.r() - 0.5) * c * 0.07;
        let mid_y = (root_y + fy) * 0.52;
        let stem_base = scale(self.base, 0.72);
        let stem_top = mix(scale(self.base, 1.22), tips[0], 0.35);
        self.canvas.ribbon(
            (root_x, root_y),
            (mid_x, mid_y),
            (fx, fy),
            width * 1.25,
            width * 0.72,
            stem_base,
            stem_top,
            14,
        );
        for k in 0..leaf_pairs {
            let t = 0.25 + 0.48 * (k as f32 + 0.4 * self.r()) / leaf_pairs.max(1) as f32;
            let u = 1.0 - t;
            let lx = u * u * root_x + 2.0 * u * t * mid_x + t * t * fx;
            let ly = u * u * root_y + 2.0 * u * t * mid_y + t * t * fy;
            for side in [-1.0f32, 1.0] {
                let ang = -std::f32::consts::FRAC_PI_2 + side * (0.68 + 0.25 * self.r());
                let llen = c * (0.055 + 0.035 * (1.0 - t));
                self.canvas.stamp_lamina(
                    (lx, ly),
                    ang,
                    llen,
                    llen * 0.26,
                    scale(self.base, 0.88),
                    mix(self.base, tips[0], 0.55),
                    0.75,
                );
            }
        }
    }

    fn paint(&mut self, species: CardSpecies, tips: &[[f32; 3]]) {
        let c = self.c;
        let (ox, oy) = (self.ox, self.oy);
        let golden = [[0.84, 0.74, 0.38], [0.76, 0.65, 0.31], [0.90, 0.82, 0.48]];
        match species {
            CardSpecies::Grass => {
                self.blades(86, tips, 0.42, 0.92);
                // Occasional fine meadow-grass seed heads
                for _ in 0..6 {
                    let fx = ox + c * (0.20 + 0.60 * self.r());
                    let fy = oy + c * (0.10 + 0.22 * self.r());
                    self.stem_to(fx, fy + c * 0.07, c * 0.0035, 0, tips);
                    for k in 0..7 {
                        let t = k as f32 / 6.0;
                        let py = fy + c * 0.07 * (1.0 - t);
                        let side = if k % 2 == 0 { -1.0 } else { 1.0 };
                        let ang = -std::f32::consts::FRAC_PI_2 + side * 0.45;
                        self.canvas.stamp_lamina(
                            (fx, py),
                            ang,
                            c * 0.024,
                            c * 0.0055,
                            mix(self.base, golden[0], 0.55),
                            golden[2],
                            0.75,
                        );
                    }
                }
            }
            CardSpecies::GoldenGrass => {
                self.blades(72, &golden, 0.46, 0.94);
                for _ in 0..8 {
                    let fx = ox + c * (0.18 + 0.64 * self.r());
                    let fy = oy + c * (0.09 + 0.22 * self.r());
                    self.stem_to(fx, fy + c * 0.08, c * 0.0035, 0, &golden);
                    for k in 0..8 {
                        let t = k as f32 / 7.0;
                        let py = fy + c * 0.08 * (1.0 - t);
                        let side = if k % 2 == 0 { -1.0 } else { 1.0 };
                        self.canvas.stamp_lamina(
                            (fx, py),
                            -std::f32::consts::FRAC_PI_2 + side * 0.38,
                            c * 0.028 * (1.0 - t * 0.35),
                            c * 0.006,
                            golden[1],
                            golden[2],
                            0.78,
                        );
                    }
                }
            }
            CardSpecies::OatSeeds => {
                self.blades(44, tips, 0.36, 0.72);
                // Arching golden culms with nodding wild-oat panicle spikelets + awns
                for _ in 0..13 {
                    let fx = ox + c * (0.16 + 0.68 * self.r());
                    let fy = oy + c * (0.10 + 0.28 * self.r());
                    self.stem_to(fx, fy + c * 0.14, c * 0.0042, 1, &golden);
                    let arch = (self.r() - 0.5) * c * 0.09;
                    self.canvas.ribbon(
                        (fx, fy + c * 0.14),
                        (fx + arch * 0.5, fy + c * 0.05),
                        (fx + arch, fy),
                        c * 0.0038,
                        c * 0.0018,
                        golden[1],
                        golden[0],
                        10,
                    );
                    for k in 0..6 {
                        let t = k as f32 / 5.0;
                        let px = fx + arch * t;
                        let py = fy + c * 0.13 * (1.0 - t);
                        let side = if k % 2 == 0 { -1.0f32 } else { 1.0 };
                        let sx = px + side * c * (0.018 + 0.010 * (1.0 - t));
                        let sy = py + c * 0.016;
                        self.canvas.line((px, py), (sx, sy), c * 0.0022, golden[1]);
                        // Paired papery oat glumes hanging downward
                        for g in [-0.18f32, 0.18] {
                            let gang = std::f32::consts::FRAC_PI_2 + side * 0.28 + g;
                            let glen = c * (0.032 - 0.008 * t);
                            self.canvas.stamp_lamina(
                                (sx, sy),
                                gang,
                                glen,
                                c * 0.0068,
                                golden[1],
                                golden[2],
                                0.72,
                            );
                            // Fine bristle awn
                            self.canvas.line(
                                (sx + gang.cos() * glen * 0.7, sy + gang.sin() * glen * 0.7),
                                (sx + gang.cos() * glen * 1.45, sy + gang.sin() * glen * 1.45),
                                c * 0.0016,
                                golden[0],
                            );
                        }
                    }
                }
            }
            CardSpecies::Daisy => {
                self.blades(42, tips, 0.34, 0.74);
                // Collect blossoms bottom-to-top / back-to-front so foreground heads overlap cleanly
                for i in 0..14 {
                    let fx = ox + c * (0.15 + 0.70 * self.r());
                    let fy = oy + c * (0.13 + 0.44 * self.r());
                    self.stem_to(fx, fy, c * 0.005, 2, tips);
                    let r = c * (0.042 + 0.022 * self.r()) * (0.88 + 0.18 * (i as f32 / 14.0));
                    let tilt = 0.62 + 0.38 * self.r(); // natural 3/4 perspective foreshortening
                    let ray_n = 15 + (i % 4);
                    let rot0 = self.r() * std::f32::consts::TAU;
                    for p in 0..ray_n {
                        let a = rot0 + p as f32 * std::f32::consts::TAU / ray_n as f32;
                        let dx = a.cos();
                        let dy = a.sin() * tilt;
                        let ang = dy.atan2(dx);
                        let plen = r * (dx * dx + dy * dy).sqrt().max(0.35);
                        self.canvas.stamp_lamina(
                            (fx + dx * r * 0.18, fy + dy * r * 0.18),
                            ang,
                            plen * 0.92,
                            r * 0.14,
                            [0.86, 0.88, 0.84],
                            [0.99, 0.99, 0.96],
                            0.68,
                        );
                    }
                    self.canvas.disc(
                        fx,
                        fy,
                        r * 0.30,
                        r * 0.30 * tilt,
                        [1.0, 0.86, 0.22],
                        [0.86, 0.56, 0.08],
                    );
                }
            }
            CardSpecies::Buttercup => {
                self.blades(42, tips, 0.35, 0.76);
                for _ in 0..16 {
                    let fx = ox + c * (0.15 + 0.70 * self.r());
                    let fy = oy + c * (0.13 + 0.45 * self.r());
                    self.stem_to(fx, fy, c * 0.0045, 2, tips);
                    let r = c * (0.032 + 0.018 * self.r());
                    let tilt = 0.68 + 0.32 * self.r();
                    let rot0 = self.r() * std::f32::consts::TAU;
                    for p in 0..5 {
                        let a = rot0 + p as f32 * std::f32::consts::TAU / 5.0;
                        let dx = a.cos();
                        let dy = a.sin() * tilt;
                        let ang = dy.atan2(dx);
                        let plen = r * (dx * dx + dy * dy).sqrt().max(0.45);
                        // Wide glossy cupped buttercup petal
                        self.canvas.stamp_lamina(
                            (fx + dx * r * 0.08, fy + dy * r * 0.08),
                            ang,
                            plen,
                            r * 0.46,
                            [0.94, 0.68, 0.06],
                            [1.0, 0.90, 0.22],
                            0.52,
                        );
                    }
                    self.canvas.disc(
                        fx,
                        fy,
                        r * 0.24,
                        r * 0.24 * tilt,
                        [0.74, 0.78, 0.16],
                        [0.88, 0.58, 0.06],
                    );
                }
            }
            CardSpecies::Clover => {
                self.blades(28, tips, 0.28, 0.62);
                // Rich mat of trifoliate clover leaves with pale V-chevrons
                for _ in 0..22 {
                    let lx = ox + c * (0.14 + 0.72 * self.r());
                    let ly = oy + c * (0.48 + 0.40 * self.r());
                    self.stem_to(lx, ly, c * 0.004, 0, tips);
                    let rot0 = self.r() * std::f32::consts::TAU;
                    let lr = c * (0.030 + 0.014 * self.r());
                    for k in 0..3 {
                        let a = rot0 + k as f32 * std::f32::consts::TAU / 3.0;
                        self.canvas.stamp_lamina(
                            (lx, ly),
                            a,
                            lr,
                            lr * 0.52,
                            scale(self.base, 0.90),
                            mix(self.base, tips[0], 0.52),
                            0.55,
                        );
                        // Characteristic pale clover leaflet chevron
                        let cx = lx + a.cos() * lr * 0.42;
                        let cy = ly + a.sin() * lr * 0.42;
                        self.canvas.disc(
                            cx,
                            cy,
                            lr * 0.16,
                            lr * 0.12,
                            [0.74, 0.84, 0.58],
                            mix(self.base, tips[0], 0.55),
                        );
                    }
                }
                // Rounded red-clover globe heads built from ~30 layered tubular florets (zero donut hole!)
                for _ in 0..13 {
                    let fx = ox + c * (0.16 + 0.68 * self.r());
                    let fy = oy + c * (0.20 + 0.40 * self.r());
                    self.stem_to(fx, fy + c * 0.02, c * 0.0048, 1, tips);
                    let r = c * (0.030 + 0.015 * self.r());
                    let hue_alt = self.r();
                    let deep = if hue_alt < 0.6 {
                        [0.58, 0.18, 0.42]
                    } else {
                        [0.68, 0.24, 0.50]
                    };
                    let bright = if hue_alt < 0.6 {
                        [0.92, 0.54, 0.78]
                    } else {
                        [0.96, 0.68, 0.86]
                    };
                    // Solid shadowed core + radiating tubular florets from bottom of globe to sunlit top
                    self.canvas.disc(fx, fy, r * 0.72, r * 0.82, bright, deep);
                    for k in 0..28 {
                        let kt = k as f32 / 27.0;
                        let row_y = fy + r * (0.65 - 1.35 * kt);
                        let row_w = (1.0 - (1.0 - 2.0 * kt).powi(2)).max(0.15).sqrt() * r * 0.85;
                        let ox_f = (self.r() - 0.5) * 2.0 * row_w;
                        let ang = -std::f32::consts::FRAC_PI_2 + (ox_f / r.max(1.0)) * 0.85;
                        self.canvas.stamp_lamina(
                            (fx + ox_f * 0.55, row_y + r * 0.15),
                            ang,
                            r * 0.48,
                            r * 0.13,
                            mix(deep, bright, kt * 0.5),
                            mix(deep, bright, 0.45 + kt * 0.55),
                            0.65,
                        );
                    }
                }
            }
            CardSpecies::Bellflower => {
                self.blades(38, tips, 0.36, 0.78);
                // Slender arching harebell stems with nodding violet-blue bell corollas
                for _ in 0..14 {
                    let sx = ox + c * (0.18 + 0.64 * self.r());
                    let sy = oy + c * (0.14 + 0.40 * self.r());
                    self.stem_to(sx, sy, c * 0.0040, 1, tips);
                    let side = if self.r() < 0.5 { -1.0f32 } else { 1.0 };
                    let bx = sx + side * c * (0.028 + 0.018 * self.r());
                    let by = sy + c * 0.018;
                    // Delicate curved pedicel + green calyx
                    self.canvas.ribbon(
                        (sx, sy),
                        (sx + side * c * 0.018, sy - c * 0.016),
                        (bx, by),
                        c * 0.0034,
                        c * 0.0024,
                        scale(self.base, 1.05),
                        scale(self.base, 1.25),
                        8,
                    );
                    let r = c * (0.034 + 0.015 * self.r());
                    let hang = std::f32::consts::FRAC_PI_2 - side * (0.25 + 0.25 * self.r());
                    let deep_violet = [0.30, 0.36, 0.82];
                    let light_periwinkle = [0.64, 0.72, 0.99];
                    // Flared 5-lobed nodding bell
                    for lobe in -2..=2 {
                        let la = hang + lobe as f32 * 0.22;
                        let shade_k = 1.0 - (lobe as f32).abs() * 0.10;
                        self.canvas.stamp_lamina(
                            (bx, by),
                            la,
                            r * (1.05 - (lobe as f32).abs() * 0.06),
                            r * 0.28,
                            scale(deep_violet, shade_k),
                            scale(light_periwinkle, shade_k),
                            0.55,
                        );
                    }
                    // Green 5-toothed sepal cap at base of bell
                    self.canvas.disc(
                        bx,
                        by,
                        r * 0.16,
                        r * 0.14,
                        mix(self.base, tips[0], 0.4),
                        self.base,
                    );
                }
            }
            CardSpecies::Yarrow => {
                self.blades(28, tips, 0.30, 0.65);
                // Upright stems with feathery bipinnate leaves & flat-topped white/blush corymb umbels
                for _ in 0..9 {
                    let fx = ox + c * (0.16 + 0.68 * self.r());
                    let fy = oy + c * (0.12 + 0.32 * self.r());
                    let rx = (ox + c * 0.50 + (fx - (ox + c * 0.50)) * 0.62)
                        .clamp(ox + c * 0.16, ox + c * 0.84);
                    let ry = oy + c * (0.95 + 0.035 * self.r());
                    self.canvas.ribbon(
                        (rx, ry),
                        ((rx + fx) * 0.5, (ry + fy) * 0.5),
                        (fx, fy + c * 0.045),
                        c * 0.006,
                        c * 0.004,
                        scale(self.base, 0.75),
                        scale(self.base, 1.25),
                        12,
                    );
                    // Feathery fern-like cauline yarrow leaves along stem
                    for lk in 0..4 {
                        let lt = 0.22 + 0.52 * lk as f32 / 3.0;
                        let lx = rx + (fx - rx) * lt;
                        let ly = ry + (fy - ry) * lt;
                        for side in [-1.0f32, 1.0] {
                            let la = -std::f32::consts::FRAC_PI_2 + side * 0.92;
                            let llen = c * (0.075 - 0.030 * lt);
                            let tip_l = (lx + la.cos() * llen, ly + la.sin() * llen);
                            self.canvas
                                .line((lx, ly), tip_l, c * 0.003, scale(self.base, 1.05));
                            for pk in 1..=5 {
                                let pt = pk as f32 / 5.5;
                                let px = lx + (tip_l.0 - lx) * pt;
                                let py = ly + (tip_l.1 - ly) * pt;
                                for ps in [-1.0f32, 1.0] {
                                    self.canvas.stamp_lamina(
                                        (px, py),
                                        la + ps * 0.85,
                                        llen * 0.28 * (1.0 - pt * 0.4),
                                        c * 0.004,
                                        self.base,
                                        mix(self.base, tips[0], 0.55),
                                        0.7,
                                    );
                                }
                            }
                        }
                    }
                    // Branching pedicels into flat-topped compound corymb of tiny 5-rayed florets
                    let umbel_w = c * (0.060 + 0.032 * self.r());
                    let blush = self.r() < 0.28;
                    let petal_col = if blush {
                        [0.97, 0.84, 0.88]
                    } else {
                        [0.97, 0.96, 0.90]
                    };
                    for u in 0..18 {
                        let ux = fx
                            + ((u as f32 / 17.0) - 0.5) * umbel_w * 1.8
                            + (self.r() - 0.5) * c * 0.018;
                        let arch = ((ux - fx) / umbel_w.max(1.0)).powi(2) * c * 0.014;
                        let uy = fy + arch + (self.r() - 0.5) * c * 0.018;
                        if u % 3 == 0 {
                            self.canvas.line(
                                (fx, fy + c * 0.048),
                                (ux, uy),
                                c * 0.0024,
                                scale(self.base, 1.2),
                            );
                        }
                        let fr = c * (0.010 + 0.004 * self.r());
                        for p in 0..5 {
                            let pa = p as f32 * std::f32::consts::TAU / 5.0 + u as f32 * 0.4;
                            self.canvas.disc(
                                ux + pa.cos() * fr * 0.65,
                                uy + pa.sin() * fr * 0.52,
                                fr * 0.58,
                                fr * 0.48,
                                petal_col,
                                scale(petal_col, 0.90),
                            );
                        }
                        self.canvas.disc(
                            ux,
                            uy,
                            fr * 0.34,
                            fr * 0.28,
                            [0.86, 0.78, 0.46],
                            [0.72, 0.64, 0.36],
                        );
                    }
                }
            }
            CardSpecies::Poppy => {
                self.blades(36, tips, 0.32, 0.72);
                // Sinuous stems + silky crinkled 4-petal scarlet poppies with dark basal blotches
                for _ in 0..10 {
                    let fx = ox + c * (0.17 + 0.66 * self.r());
                    let fy = oy + c * (0.14 + 0.38 * self.r());
                    self.stem_to(fx, fy, c * 0.0052, 2, tips);
                    let r = c * (0.048 + 0.024 * self.r());
                    let tilt = 0.62 + 0.34 * self.r();
                    let rot0 = self.r() * std::f32::consts::TAU;
                    let scarlet = if self.r() < 0.5 {
                        [0.92, 0.16, 0.10]
                    } else {
                        [0.96, 0.24, 0.12]
                    };
                    // 4 wide overlapping silky petals (each composed of 2 slightly fanned sub-lobes)
                    for p in 0..4 {
                        let base_a = rot0 + p as f32 * std::f32::consts::FRAC_PI_2;
                        for sub in [-0.22f32, 0.22] {
                            let a = base_a + sub;
                            let dx = a.cos();
                            let dy = a.sin() * tilt;
                            let ang = dy.atan2(dx);
                            let plen = r * (dx * dx + dy * dy).sqrt().max(0.48);
                            let pshade = if p % 2 == 0 { 0.92 } else { 1.04 };
                            self.canvas.stamp_lamina(
                                (fx, fy),
                                ang,
                                plen,
                                r * 0.56,
                                [0.46, 0.06, 0.06],
                                scale(scarlet, pshade),
                                0.44,
                            );
                        }
                    }
                    // Characteristic dark purple-black basal blotch + pale green central seed capsule
                    self.canvas.disc(
                        fx,
                        fy,
                        r * 0.24,
                        r * 0.22 * tilt,
                        [0.10, 0.06, 0.08],
                        [0.24, 0.08, 0.10],
                    );
                    self.canvas.disc(
                        fx,
                        fy,
                        r * 0.10,
                        r * 0.09 * tilt,
                        [0.54, 0.66, 0.34],
                        [0.26, 0.34, 0.18],
                    );
                }
            }
            CardSpecies::Lupin => {
                self.blades(24, tips, 0.28, 0.56);
                // Basal palmate lupin leaves (7-8 radiating lance leaflets!)
                for _ in 0..10 {
                    let lx = ox + c * (0.16 + 0.68 * self.r());
                    let ly = oy + c * (0.58 + 0.28 * self.r());
                    self.stem_to(lx, ly, c * 0.0042, 0, tips);
                    let lr = c * (0.044 + 0.018 * self.r());
                    for k in 0..7 {
                        let a = k as f32 * std::f32::consts::TAU / 7.0 + 0.2;
                        self.canvas.stamp_lamina(
                            (lx, ly),
                            a,
                            lr,
                            lr * 0.22,
                            scale(self.base, 0.85),
                            mix(self.base, tips[0], 0.55),
                            0.72,
                        );
                    }
                }
                // Tall tapered conical racemes of whorled two-tone violet/rose & cream pea-florets
                for _ in 0..8 {
                    let fx = ox + c * (0.16 + 0.68 * self.r());
                    let top_y = oy + c * (0.07 + 0.18 * self.r());
                    let spike_h = c * (0.28 + 0.12 * self.r());
                    let bot_y = top_y + spike_h;
                    let lean = (self.r() - 0.5) * c * 0.05;
                    self.stem_to(fx, bot_y, c * 0.0065, 1, tips);
                    self.canvas.line(
                        (fx, bot_y),
                        (fx + lean, top_y),
                        c * 0.005,
                        scale(self.base, 1.15),
                    );
                    let variant = self.r();
                    let (wing_col, banner_col) = if variant < 0.55 {
                        ([0.44, 0.26, 0.78], [0.84, 0.78, 0.98]) // classic indigo-violet + cream banner
                    } else if variant < 0.82 {
                        ([0.82, 0.34, 0.64], [0.98, 0.82, 0.92]) // rose-magenta + blush banner
                    } else {
                        ([0.56, 0.40, 0.88], [0.94, 0.88, 0.99]) // lavender-blue
                    };
                    let whorls = 14;
                    for w in 0..whorls {
                        // Paint from bottom whorl up to tight apical buds
                        let t = w as f32 / (whorls - 1) as f32;
                        let wy = bot_y - spike_h * t;
                        let wx = fx + lean * t;
                        let cone_r = c * 0.034 * (1.0 - t * 0.78);
                        let n_florets = if t > 0.78 { 3 } else { 5 };
                        for f in 0..n_florets {
                            let u = (f as f32 / (n_florets - 1).max(1) as f32) * 2.0 - 1.0;
                            let px = wx + u * cone_r * 0.72;
                            let py = wy + (1.0 - u.abs()) * c * 0.006;
                            let ang = -std::f32::consts::FRAC_PI_2 + u * 1.05;
                            let flen = cone_r * (0.95 - 0.25 * t) + c * 0.008;
                            self.canvas.stamp_lamina(
                                (px, py),
                                ang,
                                flen,
                                flen * 0.48,
                                scale(wing_col, 0.82 + 0.22 * (1.0 - u.abs())),
                                mix(wing_col, banner_col, 0.45 + 0.45 * t),
                                0.58,
                            );
                        }
                    }
                }
            }
            CardSpecies::Fern => {
                // 7 arching woodland fern fronds unfurling from a shaded crown with 18 pairs of tapered pinnae
                for f_idx in 0..7 {
                    let depth = f_idx as f32 / 6.0;
                    let spread = (f_idx as f32 / 6.0 - 0.5) * 1.5 + (self.r() - 0.5) * 0.35;
                    let rx = (ox + c * (0.50 + spread * 0.14)).clamp(ox + c * 0.24, ox + c * 0.76);
                    let ry = oy + c * (0.95 + 0.035 * self.r());
                    let hgt = c * (0.58 + 0.30 * self.r()) * (1.0 - spread.abs() * 0.12);
                    let lean = spread * c * 0.26;
                    let p1 = (rx + lean * 0.42, ry - hgt * 0.58);
                    let p2 = (
                        (rx + lean).clamp(ox + c * 0.07, ox + c * 0.93),
                        (ry - hgt + lean.abs() * 0.18).max(oy + c * 0.06),
                    );
                    let shade = 0.68 + 0.42 * depth;
                    let rachis_base = [0.24 * shade, 0.26 * shade, 0.13 * shade];
                    let frond_tip = scale(tips[0], shade * 1.05);
                    self.canvas.ribbon(
                        (rx, ry),
                        p1,
                        p2,
                        c * 0.0075,
                        c * 0.0022,
                        rachis_base,
                        frond_tip,
                        18,
                    );
                    let pairs = 18;
                    for k in 2..pairs {
                        let t = k as f32 / pairs as f32;
                        let u = 1.0 - t;
                        let px = u * u * rx + 2.0 * u * t * p1.0 + t * t * p2.0;
                        let py = u * u * ry + 2.0 * u * t * p1.1 + t * t * p2.1;
                        let tx = 2.0 * u * (p1.0 - rx) + 2.0 * t * (p2.0 - p1.0);
                        let ty = 2.0 * u * (p1.1 - ry) + 2.0 * t * (p2.1 - p1.1);
                        let tang = ty.atan2(tx);
                        // Botanical fern pinna envelope: slightly shorter at very base, broadest at t~0.28, tapering to tip
                        let env = (t * std::f32::consts::PI * 0.96).sin().max(0.0).powf(0.62)
                            * (1.0 - t * 0.35);
                        let plen = c * 0.125 * env;
                        let col_in = scale(mix(self.base, tips[0], 0.18 + 0.48 * t), shade * 0.92);
                        let col_out = scale(mix(self.base, tips[0], 0.42 + 0.55 * t), shade * 1.08);
                        for side in [-1.0f32, 1.0] {
                            let pa = tang + side * (1.12 - 0.24 * t);
                            self.canvas.stamp_lamina(
                                (px, py),
                                pa,
                                plen,
                                (plen * 0.22).max(c * 0.006),
                                col_in,
                                col_out,
                                0.66,
                            );
                        }
                    }
                }
            }
            CardSpecies::Pampas => {
                // Miscanthus sinensis (Susuki Japanese pampas grass): fountain tussock of
                // arching green-gold blades + tall slender culms crowned by silky, wind-swept
                // silvery-champagne & sunlit ivory panicle plumes.
                let straw_tip = [0.74, 0.68, 0.42];
                for k in 0..22 {
                    let depth = k as f32 / 21.0;
                    let shade = 0.58 + 0.48 * depth;
                    let spread = (k as f32 / 21.0 - 0.5) * 2.0;
                    let x0 = self.ox + c * (0.50 + spread * 0.14 + self.rng.range(-0.02, 0.02));
                    let y0 = self.oy + c * 0.992;
                    let h = c * self.rng.range(0.36, 0.58);
                    let arch = c * (spread * 0.26 + self.rng.range(-0.06, 0.06));
                    let p1 = (x0 + arch * 0.44, y0 - h * 0.72);
                    let p2 = (
                        (x0 + arch).clamp(self.ox + c * 0.05, self.ox + c * 0.95),
                        y0 - h * self.rng.range(0.82, 0.98),
                    );
                    self.canvas.ribbon(
                        (x0, y0),
                        p1,
                        p2,
                        c * 0.010,
                        c * 0.0018,
                        scale(self.base, shade * 0.85),
                        scale(mix(tips[0], straw_tip, 0.65), shade * 1.05),
                        14,
                    );
                }

                // 4 tall culms with lush feathery panicle plumes
                let culm_specs = [
                    (0.43_f32, 0.34_f32, 0.11_f32, -1.0_f32, 0.84_f32),
                    (0.57, 0.66, 0.13, 1.0, 0.90),
                    (0.48, 0.47, 0.06, -0.6, 0.98),
                    (0.53, 0.60, 0.08, 0.8, 1.04),
                ];
                let culm_base = [0.46, 0.45, 0.26];
                let culm_top = [0.76, 0.68, 0.46];
                let plume_shadow = [0.68, 0.60, 0.45];
                let plume_gold = [0.90, 0.83, 0.66];
                let plume_silk = [0.99, 0.96, 0.89];

                for &(bx_u, tx_u, apex_v, wind_bias, lum) in &culm_specs {
                    let x0 = self.ox + c * (bx_u + self.rng.range(-0.015, 0.015));
                    let y0 = self.oy + c * 0.99;
                    let neck_x =
                        self.ox + c * (lerp(bx_u, tx_u, 0.68) + self.rng.range(-0.015, 0.015));
                    let neck_y = self.oy + c * (apex_v + 0.33 + self.rng.range(-0.02, 0.02));
                    let tip_x =
                        self.ox + c * (tx_u + wind_bias * 0.05 + self.rng.range(-0.02, 0.02));
                    let tip_y = self.oy + c * (apex_v + self.rng.range(-0.01, 0.02));

                    // Slender woody-golden culm from root to panicle neck
                    self.canvas.ribbon(
                        (x0, y0),
                        ((x0 + neck_x) * 0.5, lerp(y0, neck_y, 0.52)),
                        (neck_x, neck_y),
                        c * 0.0085,
                        c * 0.0042,
                        scale(culm_base, lum * 0.9),
                        scale(culm_top, lum),
                        16,
                    );

                    // Central panicle rachis arching softly toward tip
                    let r_ctrl = (
                        lerp(neck_x, tip_x, 0.45) - wind_bias * c * 0.015,
                        lerp(neck_y, tip_y, 0.55),
                    );
                    self.canvas.ribbon(
                        (neck_x, neck_y),
                        r_ctrl,
                        (tip_x, tip_y),
                        c * 0.0045,
                        c * 0.0016,
                        scale(culm_top, lum),
                        scale(plume_gold, lum),
                        14,
                    );

                    // Two-pass gossamer-fine feathery racemes along the rachis: warm taupe-gold core + sunlit ivory silk
                    for pass in 0..2 {
                        let is_silk = pass == 1;
                        let raceme_nodes = if is_silk { 34 } else { 26 };
                        for n_i in 0..raceme_nodes {
                            let t = n_i as f32 / (raceme_nodes - 1) as f32;
                            let u = 1.0 - t;
                            let rx = u * u * neck_x + 2.0 * u * t * r_ctrl.0 + t * t * tip_x;
                            let ry = u * u * neck_y + 2.0 * u * t * r_ctrl.1 + t * t * tip_y;
                            // Plume envelope: widest in lower-middle third, tapering to a soft plume spear
                            let env = (t * std::f32::consts::PI * 0.96).sin().max(0.06).powf(0.56)
                                * (1.06 - 0.44 * t);
                            for side in [-1.0_f32, 1.0] {
                                let sweep = side * self.rng.range(0.42, 0.92) + wind_bias * 0.24;
                                let len = c * self.rng.range(0.080, 0.142) * env;
                                let ctrl = (
                                    rx + sweep * len * 0.50,
                                    ry - len * self.rng.range(0.34, 0.56),
                                );
                                // Silky racemes arch outward and weep gently at their feathery tips
                                let end = (
                                    (rx + sweep * len)
                                        .clamp(self.ox + c * 0.04, self.ox + c * 0.96),
                                    (ry - len * self.rng.range(0.06, 0.36)
                                        + len * 0.20 * sweep.abs())
                                    .clamp(self.oy + c * 0.03, self.oy + c * 0.96),
                                );
                                let (c_in, c_out, w0) = if is_silk {
                                    (
                                        scale(plume_gold, lum * self.rng.range(0.95, 1.06)),
                                        scale(plume_silk, (lum * 1.06).min(1.0)),
                                        c * 0.0021,
                                    )
                                } else {
                                    (
                                        scale(plume_shadow, lum * 0.88),
                                        scale(plume_gold, lum * 0.95),
                                        c * 0.0031,
                                    )
                                };
                                self.canvas.ribbon(
                                    (rx, ry),
                                    ctrl,
                                    end,
                                    w0,
                                    c * 0.0007,
                                    c_in,
                                    c_out,
                                    12,
                                );
                                // Fine silky spikelet awns radiating off each raceme branch
                                if is_silk && n_i % 2 == 0 {
                                    for sub in [0.42_f32, 0.72] {
                                        let su = 1.0 - sub;
                                        let sx = su * su * rx
                                            + 2.0 * su * sub * ctrl.0
                                            + sub * sub * end.0;
                                        let sy = su * su * ry
                                            + 2.0 * su * sub * ctrl.1
                                            + sub * sub * end.1;
                                        let hx = (sx + sweep * c * 0.018)
                                            .clamp(self.ox + c * 0.03, self.ox + c * 0.97);
                                        let hy = (sy - c * self.rng.range(0.010, 0.024))
                                            .max(self.oy + c * 0.02);
                                        self.canvas.line(
                                            (sx, sy),
                                            (hx, hy),
                                            c * 0.0011,
                                            scale(plume_silk, (lum * 1.04).min(1.0)),
                                        );
                                    }
                                }
                            }
                        }
                    }
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
