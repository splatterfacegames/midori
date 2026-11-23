//! World-space seamless tiled placement, volumetric Stand LOD 0/1/2 synthesis,
//! raster export, collision proxy export, and package writer for volumetric stands.

use crate::constants::TAU;
use crate::export::{ExportConfig, ExportMetadata, export_mesh};
use crate::mesh::{MaterialType, Mesh, Submesh, Vertex};
use crate::opaque_foliage::{
    FoliageRenderMode, append_opaque_rosette_tuft, append_sculpted_lobe, append_tapered_trunk_tube,
    species_bark_color, species_foliage_colors, srgb3_to_linear,
};
use crate::species::Species;
use crate::stand::{
    MoistureZone, NatureStand, STAND_MANIFEST_SCHEMA_VERSION, StandBudget, StandCanopyHeightfield,
    StandCollisionCapsule, StandCollisionFile, StandError, StandInstanceRecord, StandLibrary,
    StandManifest, StandMaterialSlot, StandPattern, StandTier, StandTileManifest,
    StandTileRastersManifest, StandTileStats, StandVariantProfile, StratumParams, StratumStats,
    UnderstoryTolerance, build_stand_library, encode_stand_instances_bin,
};
use glam::{Vec2, Vec3};
use image::{GrayImage, ImageBuffer, Luma, Rgb, RgbImage};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// World-space macro-cell size (metres) for deterministic tile-independent placement.
const WORLD_CELL_M: f32 = 32.0;
/// Clump-hub macro-cell size (metres) for `StandPattern::Clumped`.
const CLUMP_CELL_M: f32 = 64.0;
/// Border padding (metres) evaluated around every tile so conflict resolution and
/// canopy heightfields agree bit-for-bit across tile seams.
const TILE_PAD_M: f32 = 28.0;

/// Single-channel raster input in tile space (`[0, 1] x [0, 1]`).
#[derive(Debug, Clone)]
pub struct StandRaster {
    pub width: u32,
    pub height: u32,
    /// Normalized values in `0.0..=1.0` (or world metres for heightfields).
    pub data: Vec<f32>,
}

impl StandRaster {
    /// Load an 8-bit or 16-bit grayscale PNG into `[0.0, 1.0]`.
    pub fn from_png_normalized(path: &Path) -> Result<Self, StandError> {
        let img = image::open(path).map_err(|e| {
            StandError::Io(std::io::Error::other(format!(
                "failed reading raster {}: {e}",
                path.display()
            )))
        })?;
        let luma16 = img.to_luma16();
        let (width, height) = luma16.dimensions();
        let data = luma16
            .pixels()
            .map(|p| p.0[0] as f32 / 65535.0)
            .collect::<Vec<_>>();
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Load a 16-bit or 8-bit heightfield PNG mapped to `[min_m, max_m]`.
    pub fn from_png_height(path: &Path, min_m: f32, max_m: f32) -> Result<Self, StandError> {
        let mut r = Self::from_png_normalized(path)?;
        let span = max_m - min_m;
        for v in &mut r.data {
            *v = min_m + *v * span;
        }
        Ok(r)
    }

    /// Bilinearly sample at normalized `(u, v)` in `[0, 1]`.
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        if self.width == 0 || self.height == 0 || self.data.is_empty() {
            return 0.0;
        }
        let fx = u.clamp(0.0, 1.0) * (self.width - 1) as f32;
        let fy = v.clamp(0.0, 1.0) * (self.height - 1) as f32;
        let x0 = (fx.floor() as u32).min(self.width - 1);
        let y0 = (fy.floor() as u32).min(self.height - 1);
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;

        let idx = |x: u32, y: u32| self.data[(y * self.width + x) as usize];
        let a = idx(x0, y0) * (1.0 - tx) + idx(x1, y0) * tx;
        let b = idx(x0, y1) * (1.0 - tx) + idx(x1, y1) * tx;
        a * (1.0 - ty) + b * ty
    }

    /// Compute a fast 2D chamfer Euclidean distance field (in metres) from pixels where
    /// `predicate(value)` is true.
    pub fn distance_field_m<F: Fn(f32) -> bool>(&self, tile_size_m: f32, predicate: F) -> Self {
        let w = self.width as usize;
        let h = self.height as usize;
        if w == 0 || h == 0 {
            return Self {
                width: 0,
                height: 0,
                data: Vec::new(),
            };
        }
        let dx = tile_size_m / (w.max(1) as f32);
        let dy = tile_size_m / (h.max(1) as f32);
        let diag = (dx * dx + dy * dy).sqrt();
        let inf = tile_size_m * 4.0;

        let mut d = vec![inf; w * h];
        for (slot, &val) in d.iter_mut().zip(self.data.iter()) {
            if predicate(val) {
                *slot = 0.0;
            }
        }
        // Forward pass
        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let mut v = d[idx];
                if x > 0 {
                    v = v.min(d[idx - 1] + dx);
                }
                if y > 0 {
                    v = v.min(d[idx - w] + dy);
                    if x > 0 {
                        v = v.min(d[idx - w - 1] + diag);
                    }
                    if x + 1 < w {
                        v = v.min(d[idx - w + 1] + diag);
                    }
                }
                d[idx] = v;
            }
        }
        // Backward pass
        for y in (0..h).rev() {
            for x in (0..w).rev() {
                let idx = y * w + x;
                let mut v = d[idx];
                if x + 1 < w {
                    v = v.min(d[idx + 1] + dx);
                }
                if y + 1 < h {
                    v = v.min(d[idx + w] + dy);
                    if x + 1 < w {
                        v = v.min(d[idx + w + 1] + diag);
                    }
                    if x > 0 {
                        v = v.min(d[idx + w - 1] + diag);
                    }
                }
                d[idx] = v;
            }
        }
        Self {
            width: self.width,
            height: self.height,
            data: d,
        }
    }
}

/// Optional per-tile external raster inputs and density overrides.
#[derive(Debug, Clone, Default)]
pub struct StandTileInputs {
    /// 8-bit stand mask (`0` = outside stand, `1` = full density; soft values scale density).
    pub mask: Option<StandRaster>,
    /// Hard exclusion mask (`> 0.5` = road, track, building, or water).
    pub exclude: Option<StandRaster>,
    /// World terrain heightfield in metres.
    pub height: Option<StandRaster>,
    /// Per-stratum density multiplier (`stratum_name -> factor`).
    pub density_overrides: BTreeMap<String, f32>,
}

/// Precomputed tile-space evaluators for mask/exclude/height/edge distances.
struct TileFieldEvaluator {
    origin_x: f32,
    origin_z: f32,
    tile_size: f32,
    mask: Option<StandRaster>,
    mask_edge_dist: Option<StandRaster>,
    exclude: Option<StandRaster>,
    exclude_dist: Option<StandRaster>,
    height: Option<StandRaster>,
    min_height: f32,
    max_height: f32,
}

impl TileFieldEvaluator {
    fn new(origin_x: f32, origin_z: f32, tile_size: f32, inputs: &StandTileInputs) -> Self {
        let mask_edge_dist = inputs
            .mask
            .as_ref()
            .map(|m| m.distance_field_m(tile_size, |v| v < 0.5));
        let exclude_dist = inputs
            .exclude
            .as_ref()
            .map(|e| e.distance_field_m(tile_size, |v| v > 0.5));
        let (min_h, max_h) = if let Some(ref h) = inputs.height {
            let mut lo = f32::MAX;
            let mut hi = f32::MIN;
            for &v in &h.data {
                lo = lo.min(v);
                hi = hi.max(v);
            }
            if lo.is_finite() && hi.is_finite() {
                (lo, hi)
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        };
        Self {
            origin_x,
            origin_z,
            tile_size,
            mask: inputs.mask.clone(),
            mask_edge_dist,
            exclude: inputs.exclude.clone(),
            exclude_dist,
            height: inputs.height.clone(),
            min_height: min_h,
            max_height: max_h,
        }
    }

    #[inline]
    fn to_uv(&self, wx: f32, wz: f32) -> (f32, f32) {
        (
            (wx - self.origin_x) / self.tile_size,
            (wz - self.origin_z) / self.tile_size,
        )
    }

    #[inline]
    fn mask_at(&self, wx: f32, wz: f32) -> f32 {
        let Some(ref m) = self.mask else {
            return 1.0;
        };
        let (u, v) = self.to_uv(wx, wz);
        m.sample(u, v)
    }

    #[inline]
    fn excluded_at(&self, wx: f32, wz: f32) -> bool {
        let Some(ref e) = self.exclude else {
            return false;
        };
        let (u, v) = self.to_uv(wx, wz);
        e.sample(u, v) > 0.5
    }

    /// Distance in metres to the nearest stand boundary (mask < 0.5 or exclusion > 0.5).
    #[inline]
    fn edge_distance_at(&self, wx: f32, wz: f32) -> f32 {
        let (u, v) = self.to_uv(wx, wz);
        let mut d = f32::MAX;
        if let Some(ref md) = self.mask_edge_dist {
            d = d.min(md.sample(u, v));
        }
        if let Some(ref ed) = self.exclude_dist {
            d = d.min(ed.sample(u, v));
        }
        d
    }

    /// Distance in metres to the nearest hard exclusion pixel.
    #[inline]
    fn exclude_distance_at(&self, wx: f32, wz: f32) -> f32 {
        let Some(ref ed) = self.exclude_dist else {
            return f32::MAX;
        };
        let (u, v) = self.to_uv(wx, wz);
        ed.sample(u, v)
    }

    #[inline]
    fn height_at(&self, wx: f32, wz: f32) -> f32 {
        let Some(ref h) = self.height else {
            return 0.0;
        };
        let (u, v) = self.to_uv(wx, wz);
        h.sample(u, v)
    }

    fn slope_degrees_at(&self, wx: f32, wz: f32) -> f32 {
        if self.height.is_none() {
            return 0.0;
        }
        let eps = (self.tile_size / 64.0).max(0.5);
        let hx = (self.height_at(wx + eps, wz) - self.height_at(wx - eps, wz)) / (2.0 * eps);
        let hz = (self.height_at(wx, wz + eps) - self.height_at(wx, wz - eps)) / (2.0 * eps);
        (hx * hx + hz * hz).sqrt().atan().to_degrees()
    }

    /// Derived wetness index in `0..=1` (1 = low-lying depression / riparian edge).
    fn wetness_at(&self, wx: f32, wz: f32, seed: u64) -> f32 {
        let mut w = if self.height.is_some() && (self.max_height - self.min_height) > 0.25 {
            let h = self.height_at(wx, wz);
            let rel = ((h - self.min_height) / (self.max_height - self.min_height)).clamp(0.0, 1.0);
            1.0 - rel
        } else {
            // World-space smooth low-frequency wetness field when no height raster is supplied
            world_fbm2(wx * 0.015, wz * 0.015, seed ^ 0x57E7)
        };
        if self.exclude_dist.is_some() {
            let ed = self.exclude_distance_at(wx, wz);
            if ed < 14.0 {
                w = w.max(1.0 - ed / 14.0);
            }
        }
        w.clamp(0.0, 1.0)
    }
}

#[inline]
fn hash_u64(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^= x >> 33;
    x
}

#[inline]
fn hash_cell(seed: u64, stratum_idx: usize, cx: i32, cz: i32, local: u32) -> u64 {
    let mut h = seed ^ ((stratum_idx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    h ^= (cx as i64 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = hash_u64(h);
    h ^= (cz as i64 as u64).wrapping_mul(0x94D0_49BB_1331_11EB);
    h = hash_u64(h);
    h ^= (local as u64).wrapping_mul(0x517C_C1B7_2722_0A95);
    hash_u64(h)
}

#[inline]
fn u64_to_unit(v: u64) -> f32 {
    (v >> 40) as f32 / (1u32 << 24) as f32
}

/// Seamless world-space value noise (identical across tile seams).
fn world_noise2(x: f32, z: f32, seed: u64) -> f32 {
    let ix = x.floor() as i32;
    let iz = z.floor() as i32;
    let fx = x - ix as f32;
    let fz = z - iz as f32;
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uz = fz * fz * (3.0 - 2.0 * fz);
    let n00 = u64_to_unit(hash_cell(seed, 0, ix, iz, 0));
    let n10 = u64_to_unit(hash_cell(seed, 0, ix + 1, iz, 0));
    let n01 = u64_to_unit(hash_cell(seed, 0, ix, iz + 1, 0));
    let n11 = u64_to_unit(hash_cell(seed, 0, ix + 1, iz + 1, 0));
    let a = n00 + (n10 - n00) * ux;
    let b = n01 + (n11 - n01) * ux;
    a + (b - a) * uz
}

fn world_fbm2(x: f32, z: f32, seed: u64) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.55;
    let mut freq = 1.0;
    let mut norm = 0.0;
    for oct in 0..3 {
        sum += amp * world_noise2(x * freq, z * freq, seed ^ (oct as u64 * 0x1337));
        norm += amp;
        amp *= 0.5;
        freq *= 2.05;
    }
    (sum / norm).clamp(0.0, 1.0)
}

/// Internal placed stem before/after tiering (includes padded neighbors for seamless LODs).
#[derive(Debug, Clone)]
struct PlacedStem {
    priority: u64,
    stratum_idx: u16,
    variant_index: u32,
    wx: f32,
    wy: f32,
    wz: f32,
    yaw: f32,
    scale: f32,
    crown_radius: f32,
    crown_base_y: f32,
    crown_top_y: f32,
    height: f32,
    trunk_radius: f32,
    color_tint: [f32; 3],
    foliage_top_srgb: [f32; 3],
    foliage_bot_srgb: [f32; 3],
    bark_srgb: [f32; 3],
    organ_srgb: Option<[f32; 3]>,
    is_rosette: bool,
    is_dead: bool,
    near_edge: bool,
    cover_above: f32,
    in_tile: bool,
    tier: StandTier,
}

/// Fast 2D spatial hash grid for `O(1)` spacing & canopy-cover lookups.
struct SpatialGrid {
    cell_size: f32,
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl SpatialGrid {
    fn new(cell_size: f32) -> Self {
        Self {
            cell_size: cell_size.max(0.5),
            cells: HashMap::new(),
        }
    }

    #[inline]
    fn key(&self, x: f32, z: f32) -> (i32, i32) {
        (
            (x / self.cell_size).floor() as i32,
            (z / self.cell_size).floor() as i32,
        )
    }

    fn insert(&mut self, idx: usize, x: f32, z: f32) {
        self.cells.entry(self.key(x, z)).or_default().push(idx);
    }

    fn for_each_within<F: FnMut(usize)>(&self, x: f32, z: f32, radius: f32, mut f: F) {
        let min_c = self.key(x - radius, z - radius);
        let max_c = self.key(x + radius, z + radius);
        for cz in min_c.1..=max_c.1 {
            for cx in min_c.0..=max_c.0 {
                if let Some(bucket) = self.cells.get(&(cx, cz)) {
                    for &idx in bucket {
                        f(idx);
                    }
                }
            }
        }
    }
}

/// Evaluate projected foliage cover `0..=1` at `(wx, wz)` from a list of upper-stratum stems.
fn cover_at_point(wx: f32, wz: f32, stems: &[PlacedStem], grid: &SpatialGrid, max_r: f32) -> f32 {
    if stems.is_empty() || max_r <= 0.0 {
        return 0.0;
    }
    let mut sum = 0.0_f32;
    grid.for_each_within(wx, wz, max_r, |idx| {
        let s = &stems[idx];
        if s.is_dead {
            return;
        }
        let dx = wx - s.wx;
        let dz = wz - s.wz;
        let r = s.crown_radius.max(0.2);
        let d2 = dx * dx + dz * dz;
        if d2 < r * r {
            let u = 1.0 - (d2 / (r * r));
            sum += 0.82 * u.sqrt();
        }
    });
    1.0 - (-sum).exp()
}

/// Pick a variant from `library` for a stratum candidate deterministically.
#[allow(clippy::too_many_arguments)]
fn pick_variant_for_candidate<'a>(
    stratum: &StratumParams,
    stand: &NatureStand,
    library: &'a StandLibrary,
    h_species: f32,
    h_size: f32,
    h_dead: f32,
    h_var: u64,
) -> Option<&'a StandVariantProfile> {
    let share_sum: f32 = stratum.species.iter().map(|s| s.share.max(0.0)).sum();
    if share_sum <= 0.0 {
        return None;
    }
    let mut target = h_species * share_sum;
    let mut chosen_sp = &stratum.species[0];
    for sp in &stratum.species {
        target -= sp.share.max(0.0);
        if target <= 0.0 {
            chosen_sp = sp;
            break;
        }
    }

    let size_dist = chosen_sp.resolved_size_classes();
    let mut starget = h_size;
    let mut chosen_size = size_dist[0].0;
    for &(sc, w) in &size_dist {
        starget -= w;
        if starget <= 0.0 {
            chosen_size = sc;
            break;
        }
    }

    let want_dead = stratum.is_dead
        || (stand.stand.condition.dieback > 0.0
            && chosen_sp.is_dieback_susceptible()
            && h_dead < stand.stand.condition.dieback);

    let matching: Vec<&StandVariantProfile> = library
        .variants
        .iter()
        .filter(|v| {
            v.preset == chosen_sp.preset
                && v.size_class == chosen_size
                && v.is_dead == want_dead
                && (chosen_sp.state.is_none() || v.state == chosen_sp.state)
        })
        .collect();
    if !matching.is_empty() {
        return Some(matching[(h_var as usize) % matching.len()]);
    }
    // Fallback ignoring size_class/state
    let fallback: Vec<&StandVariantProfile> = library
        .variants
        .iter()
        .filter(|v| v.preset == chosen_sp.preset)
        .collect();
    if !fallback.is_empty() {
        Some(fallback[(h_var as usize) % fallback.len()])
    } else {
        library.variants.first()
    }
}

/// World-space seamless placement of all strata for tile `(tx, tz)` plus its padded border.
fn place_stand_stems(
    stand: &NatureStand,
    library: &StandLibrary,
    tile: [i32; 2],
    tile_size_m: f32,
    inputs: &StandTileInputs,
    eval: &TileFieldEvaluator,
) -> Vec<PlacedStem> {
    let origin_x = tile[0] as f32 * tile_size_m;
    let origin_z = tile[1] as f32 * tile_size_m;
    let min_x = origin_x - TILE_PAD_M;
    let max_x = origin_x + tile_size_m + TILE_PAD_M;
    let min_z = origin_z - TILE_PAD_M;
    let max_z = origin_z + tile_size_m + TILE_PAD_M;

    let mut all_kept: Vec<PlacedStem> = Vec::new();
    let mut upper_grid = SpatialGrid::new(8.0);
    let mut upper_max_crown = 0.0_f32;

    for (s_idx, stratum) in stand.strata.iter().enumerate() {
        let density_mul = inputs
            .density_overrides
            .get(&stratum.name)
            .copied()
            .unwrap_or(1.0)
            .max(0.0);

        // Compute mean crown radius and height of this stratum's variants for cover->density conversion
        let mut r_sum = 0.0_f32;
        let mut h_sum = 0.0_f32;
        let mut v_cnt = 0u32;
        for v in &library.variants {
            if v.stratum == stratum.name && !v.is_dead {
                r_sum += v.crown_radius_m;
                h_sum += v.height_m;
                v_cnt += 1;
            }
        }
        let mean_crown_r = if v_cnt > 0 {
            (r_sum / v_cnt as f32).max(0.35)
        } else {
            1.6
        };
        let mean_height = if v_cnt > 0 {
            h_sum / v_cnt as f32
        } else {
            stratum.height_range.map(|hr| hr[1]).unwrap_or(4.0)
        };

        let raw_stems_per_ha = if let Some(explicit_sph) = stratum.stems_per_ha {
            explicit_sph
        } else if stratum.cover <= 0.0 {
            0.0
        } else {
            let crown_area = std::f32::consts::PI * mean_crown_r * mean_crown_r;
            let target_cover = stratum.cover.clamp(0.01, 0.94);
            let poisson_sph = (-(1.0 - target_cover).ln() / crown_area.max(0.25)) * 10_000.0;
            let spacing_cap = 10_000.0 / (0.75 * stratum.min_spacing.max(0.5).powi(2));
            poisson_sph.min(spacing_cap).min(4_500.0)
        };
        let base_stems_per_ha = raw_stems_per_ha * density_mul;
        if base_stems_per_ha <= 0.0 {
            continue;
        }

        // Oversample candidates so Poisson spacing & understorey filtering hit target cover/stems
        let oversample = match stratum.pattern {
            StandPattern::Regular => 1.0,
            StandPattern::Random => 2.2,
            StandPattern::Clumped => 2.8,
        };
        let is_low_stratum = mean_height <= 3.5;

        // Collect deterministic world-space candidates
        let mut candidates: Vec<(u64, f32, f32, f32, u64)> = Vec::new();

        match stratum.pattern {
            StandPattern::Regular => {
                let rx = stratum.row_spacing.unwrap_or(stratum.min_spacing).max(0.5);
                let rz = stratum.min_spacing.max(0.5);
                let gx0 = (min_x / rx).floor() as i32;
                let gx1 = (max_x / rx).ceil() as i32;
                let gz0 = (min_z / rz).floor() as i32;
                let gz1 = (max_z / rz).ceil() as i32;
                let expected_per_ha = 10_000.0 / (rx * rz);
                let keep_prob = if stratum.stems_per_ha.is_some() {
                    (base_stems_per_ha / expected_per_ha).clamp(0.0, 1.0)
                } else {
                    ((stratum.cover / 0.80) * density_mul).clamp(0.0, 1.0)
                };
                for gz in gz0..=gz1 {
                    for gx in gx0..=gx1 {
                        let h = hash_cell(stand.stand.seed, s_idx, gx, gz, 0);
                        if u64_to_unit(h) > keep_prob {
                            continue;
                        }
                        let jx = (u64_to_unit(hash_u64(h ^ 0x11)) - 0.5) * rx * 0.14;
                        let jz = (u64_to_unit(hash_u64(h ^ 0x22)) - 0.5) * rz * 0.14;
                        let wx = gx as f32 * rx + jx;
                        let wz = gz as f32 * rz + jz;
                        if wx >= min_x && wx <= max_x && wz >= min_z && wz <= max_z {
                            candidates.push((hash_u64(h ^ 0x33), wx, wz, 1.0, h));
                        }
                    }
                }
            }
            StandPattern::Random | StandPattern::Clumped => {
                let cx0 = (min_x / WORLD_CELL_M).floor() as i32;
                let cx1 = (max_x / WORLD_CELL_M).ceil() as i32;
                let cz0 = (min_z / WORLD_CELL_M).floor() as i32;
                let cz1 = (max_z / WORLD_CELL_M).ceil() as i32;

                // Pre-generate deterministic world-space clump hubs if Clumped
                let mut clump_hubs: Vec<(f32, f32)> = Vec::new();
                if stratum.pattern == StandPattern::Clumped {
                    let hx0 = ((min_x - CLUMP_CELL_M) / CLUMP_CELL_M).floor() as i32;
                    let hx1 = ((max_x + CLUMP_CELL_M) / CLUMP_CELL_M).ceil() as i32;
                    let hz0 = ((min_z - CLUMP_CELL_M) / CLUMP_CELL_M).floor() as i32;
                    let hz1 = ((max_z + CLUMP_CELL_M) / CLUMP_CELL_M).ceil() as i32;
                    let hubs_per_cell = ((CLUMP_CELL_M * CLUMP_CELL_M)
                        / (stratum.clump_radius.max(3.0).powi(2) * 5.0))
                        .clamp(2.0, 14.0) as u32;
                    for hz in hz0..=hz1 {
                        for hx in hx0..=hx1 {
                            for k in 0..hubs_per_cell {
                                let hh = hash_cell(stand.stand.seed ^ 0xC10B, s_idx, hx, hz, k);
                                let ux = u64_to_unit(hh);
                                let uz = u64_to_unit(hash_u64(hh ^ 0x99));
                                clump_hubs.push((
                                    (hx as f32 + ux) * CLUMP_CELL_M,
                                    (hz as f32 + uz) * CLUMP_CELL_M,
                                ));
                            }
                        }
                    }
                }

                let cell_ha = (WORLD_CELL_M * WORLD_CELL_M) / 10_000.0;
                // Account for edge boost headroom when generating candidate pool
                let max_edge_boost = if is_low_stratum {
                    stand.stand.edge_density_multiplier * stand.stand.edge_shrub_boost
                } else {
                    stand.stand.edge_density_multiplier
                }
                .clamp(1.0, 3.0);

                for cz in cz0..=cz1 {
                    for cx in cx0..=cx1 {
                        let base_h = hash_cell(stand.stand.seed, s_idx, cx, cz, 0);
                        let expected = base_stems_per_ha * cell_ha * oversample * max_edge_boost;
                        let n_base = expected.floor() as u32;
                        let frac = expected - n_base as f32;
                        let n = n_base + u32::from(u64_to_unit(base_h) < frac);

                        for k in 0..n {
                            let h = hash_cell(stand.stand.seed, s_idx, cx, cz, k + 1);
                            let ux = u64_to_unit(h);
                            let uz = u64_to_unit(hash_u64(h ^ 0x45));
                            let mut wx = (cx as f32 + ux) * WORLD_CELL_M;
                            let mut wz = (cz as f32 + uz) * WORLD_CELL_M;

                            if stratum.pattern == StandPattern::Clumped && !clump_hubs.is_empty() {
                                // Pull candidate toward nearest world-space clump hub for organic bush clumps
                                let mut best_d2 = f32::MAX;
                                let mut best_hub = (wx, wz);
                                for &(hx, hz) in &clump_hubs {
                                    let d2 = (wx - hx) * (wx - hx) + (wz - hz) * (wz - hz);
                                    if d2 < best_d2 {
                                        best_d2 = d2;
                                        best_hub = (hx, hz);
                                    }
                                }
                                let pull = 0.32 + 0.30 * u64_to_unit(hash_u64(h ^ 0x77));
                                let r_clump = stratum.clump_radius.max(3.0);
                                let d = best_d2.sqrt();
                                if d > r_clump * 0.55 {
                                    let target_d = (d * (1.0 - pull)).min(
                                        r_clump * (0.55 + 0.85 * u64_to_unit(hash_u64(h ^ 0x88))),
                                    );
                                    let k_move = target_d / d.max(1e-4);
                                    wx = best_hub.0 + (wx - best_hub.0) * k_move;
                                    wz = best_hub.1 + (wz - best_hub.1) * k_move;
                                }
                            }

                            if wx < min_x || wx > max_x || wz < min_z || wz > max_z {
                                continue;
                            }
                            candidates.push((
                                hash_u64(h ^ 0xA5A5),
                                wx,
                                wz,
                                1.0 / max_edge_boost,
                                h,
                            ));
                        }
                    }
                }
            }
        }

        // Deterministic priority order (strictly world-space, tile-order independent!)
        candidates.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.total_cmp(&b.1))
                .then_with(|| a.2.total_cmp(&b.2))
        });

        let mut stratum_grid = SpatialGrid::new(stratum.min_spacing.max(1.5));
        let mut stratum_kept: Vec<PlacedStem> = Vec::new();
        let edge_w = stand.stand.edge_width;

        for (priority, wx, wz, pool_norm, h) in candidates {
            // 1. Hard exclusion check (with trunk clearance)
            if eval.excluded_at(wx, wz) {
                continue;
            }
            let excl_d = eval.exclude_distance_at(wx, wz);
            if excl_d < stand.stand.exclude_trunk_clearance {
                continue;
            }

            // 2. Mask density check
            let mask_v = eval.mask_at(wx, wz);
            if mask_v <= 0.02 {
                continue;
            }

            // 3. Edge response (within edge_width of mask boundary or exclusion)
            let edge_d = eval.edge_distance_at(wx, wz);
            let near_edge = edge_w > 0.0 && edge_d < edge_w;
            let edge_t = if near_edge {
                (1.0 - edge_d / edge_w).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let edge_density = 1.0 + (stand.stand.edge_density_multiplier - 1.0) * edge_t;
            let shrub_edge_boost = if is_low_stratum {
                1.0 + (stand.stand.edge_shrub_boost - 1.0) * edge_t
            } else {
                1.0
            };

            // 4. Strata interaction: read projected canopy cover from strata above
            let cover_above = cover_at_point(wx, wz, &all_kept, &upper_grid, upper_max_crown);
            let light_weight = match stratum.under_tolerance {
                UnderstoryTolerance::Avoid => (1.0 - 0.85 * cover_above).clamp(0.10, 1.0),
                UnderstoryTolerance::Neutral => (1.0 - 0.15 * cover_above).clamp(0.65, 1.0),
                UnderstoryTolerance::Prefer => (0.25 + 0.95 * cover_above).clamp(0.25, 1.2),
            };

            // 5. Optional slope & moisture zoning
            if let Some(max_slope) = stratum.slope_max_degrees
                && eval.slope_degrees_at(wx, wz) > max_slope
            {
                continue;
            }
            let wetness = eval.wetness_at(wx, wz, stand.stand.seed);
            let moisture_weight = match stratum.moisture {
                MoistureZone::Any => 1.0,
                MoistureZone::Dry => (1.15 - 0.85 * wetness).clamp(0.15, 1.0),
                MoistureZone::Wet => (0.15 + 1.05 * wetness).clamp(0.10, 1.1),
                MoistureZone::Riparian => {
                    if wetness < 0.42 {
                        0.0
                    } else {
                        ((wetness - 0.42) / 0.58).clamp(0.0, 1.0)
                    }
                }
            };

            let accept_prob = (mask_v
                * pool_norm
                * edge_density
                * shrub_edge_boost
                * light_weight
                * moisture_weight)
                .clamp(0.0, 1.0);
            if u64_to_unit(hash_u64(h ^ 0xBEEF)) > accept_prob {
                continue;
            }

            // 6. World-space Poisson conflict thinning against same stratum
            let min_sp = if near_edge {
                stratum.min_spacing * (1.0 - 0.25 * edge_t)
            } else {
                stratum.min_spacing
            };
            let min_sp2 = min_sp * min_sp;
            let mut blocked = false;
            stratum_grid.for_each_within(wx, wz, min_sp, |idx| {
                let other = &stratum_kept[idx];
                let d2 = (wx - other.wx) * (wx - other.wx) + (wz - other.wz) * (wz - other.wz);
                if d2 < min_sp2 {
                    blocked = true;
                }
            });
            if blocked {
                continue;
            }

            // 7. Avoid placing a lower stem directly inside a higher tree's trunk base
            let trunk_clear = 0.85;
            upper_grid.for_each_within(wx, wz, trunk_clear, |idx| {
                let other = &all_kept[idx];
                let r = (other.trunk_radius + 0.35).max(0.45);
                let d2 = (wx - other.wx) * (wx - other.wx) + (wz - other.wz) * (wz - other.wz);
                if d2 < r * r {
                    blocked = true;
                }
            });
            if blocked {
                continue;
            }

            // 8. Select species variant and apply height-range / edge-taper scale
            let Some(variant) = pick_variant_for_candidate(
                stratum,
                stand,
                library,
                u64_to_unit(hash_u64(h ^ 0x101)),
                u64_to_unit(hash_u64(h ^ 0x202)),
                u64_to_unit(hash_u64(h ^ 0x303)),
                hash_u64(h ^ 0x404),
            ) else {
                continue;
            };

            let base_scale = if let Some(hr) = stratum.height_range {
                let target_h = hr[0] + (hr[1] - hr[0]) * u64_to_unit(hash_u64(h ^ 0x505));
                (target_h / variant.height_m.max(0.25)).clamp(0.55, 1.55)
            } else {
                0.88 + 0.24 * u64_to_unit(hash_u64(h ^ 0x505))
            };
            let edge_h_mul = if near_edge && !is_low_stratum {
                1.0 - (1.0 - stand.stand.edge_height_scale) * edge_t
            } else {
                1.0
            };
            let mut scale = base_scale * edge_h_mul;

            // Clamp crown overhang into hard exclusions (roads/tracks)
            let mut crown_r = variant.crown_radius_m * scale;
            if excl_d < f32::MAX {
                let max_r = (excl_d + stand.stand.exclude_crown_overhang).max(0.35);
                if crown_r > max_r {
                    scale *= max_r / crown_r;
                    crown_r = max_r;
                }
            }

            let wy = eval.height_at(wx, wz);
            let yaw = u64_to_unit(hash_u64(h ^ 0x606)) * TAU;
            let tint_v = 0.88 + 0.24 * u64_to_unit(hash_u64(h ^ 0x707));
            let tint_w = (u64_to_unit(hash_u64(h ^ 0x808)) - 0.5) * 0.08;
            let color_tint = [
                (tint_v + tint_w).clamp(0.75, 1.22),
                tint_v.clamp(0.75, 1.22),
                (tint_v - tint_w).clamp(0.75, 1.22),
            ];

            let in_tile = wx >= origin_x
                && wx < origin_x + tile_size_m
                && wz >= origin_z
                && wz < origin_z + tile_size_m;

            let idx_in_stratum = stratum_kept.len();
            stratum_grid.insert(idx_in_stratum, wx, wz);
            stratum_kept.push(PlacedStem {
                priority,
                stratum_idx: s_idx as u16,
                variant_index: variant.variant_index,
                wx,
                wy,
                wz,
                yaw,
                scale,
                crown_radius: crown_r,
                crown_base_y: variant.crown_base_y_m * scale,
                crown_top_y: variant.crown_top_y_m * scale,
                height: variant.height_m * scale,
                trunk_radius: variant.trunk_radius_m * scale,
                color_tint,
                foliage_top_srgb: variant.foliage_top_srgb,
                foliage_bot_srgb: variant.foliage_bot_srgb,
                bark_srgb: variant.bark_srgb,
                organ_srgb: variant.organ_srgb,
                is_rosette: variant.is_rosette,
                is_dead: variant.is_dead || stratum.is_dead,
                near_edge,
                cover_above,
                in_tile,
                tier: StandTier::Merged,
            });
        }

        // Feed kept stems of this stratum into the upper-cover field for lower strata
        for stem in stratum_kept {
            let idx = all_kept.len();
            if !stratum.is_dead && stem.crown_radius > upper_max_crown {
                upper_max_crown = stem.crown_radius;
            }
            upper_grid.insert(idx, stem.wx, stem.wz);
            all_kept.push(stem);
        }
    }

    all_kept
}

/// Deterministically assign `StandTier::Instance` vs `StandTier::Merged` within a tile,
/// strictly meeting `budget.max_instances_per_tile`.
fn assign_tiers(stems: &mut [PlacedStem], max_instances_per_tile: usize) {
    let mut in_tile_indices: Vec<(f32, u64, usize)> = stems
        .iter()
        .enumerate()
        .filter(|(_, s)| s.in_tile)
        .map(|(i, s)| {
            // Strongly prioritise tall trees & shrubs along road/track/clearing edges so eye-level
            // vistas always see crisp Hero library plants rather than merged interior clumps.
            let mut score = s.height * 6.0 + s.crown_radius * 5.0;
            if s.near_edge {
                score += if s.height >= 2.0 {
                    260.0
                } else if s.height >= 0.9 {
                    140.0
                } else {
                    25.0
                };
            }
            if s.cover_above < 0.25 && s.height >= 1.5 {
                score += 55.0;
            }
            if s.trunk_radius >= 0.32 {
                score += 40.0;
            }
            (score, s.priority, i)
        })
        .collect();

    // Highest score first, deterministic tiebreak by world priority
    in_tile_indices.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    let keep_inst = max_instances_per_tile.min(in_tile_indices.len());
    for (rank, &(_, _, idx)) in in_tile_indices.iter().enumerate() {
        stems[idx].tier = if rank < keep_inst {
            StandTier::Instance
        } else {
            StandTier::Merged
        };
    }
}

/// Build Stand LOD 0 volumetric clump meshes (one mesh per stratum with merged stems),
/// strictly respecting `budget.max_triangles_lod0`.
fn build_lod0_stratum_meshes(
    stand: &NatureStand,
    stems: &[PlacedStem],
    origin_x: f32,
    origin_z: f32,
    budget: &StandBudget,
) -> BTreeMap<String, Mesh> {
    let mut by_stratum: BTreeMap<usize, Vec<&PlacedStem>> = BTreeMap::new();
    let mut mass_by_stratum: BTreeMap<usize, f32> = BTreeMap::new();
    let mut total_mass = 0.0_f32;
    for s in stems {
        if s.in_tile && s.tier == StandTier::Merged {
            by_stratum
                .entry(s.stratum_idx as usize)
                .or_default()
                .push(s);
            // Weight triangle budget by visual crown volume so tall trees get rich multi-tuft
            // crowns while tiny ground sedges stay compact.
            let m = (s.crown_radius.powf(1.5) * s.height.max(0.5).powf(0.45)).max(0.15);
            *mass_by_stratum.entry(s.stratum_idx as usize).or_insert(0.0) += m;
            total_mass += m;
        }
    }

    let mut out = BTreeMap::new();
    let max_tris = budget.max_triangles_lod0.max(200);
    let mut remaining_tris = max_tris;

    for (s_idx, stratum) in stand.strata.iter().enumerate() {
        let mut bark_mesh = Mesh::new();
        let mut foliage_mesh = Mesh::new();

        if let Some(list) = by_stratum.get(&s_idx)
            && !list.is_empty()
            && remaining_tris >= 20
        {
            let s_mass = mass_by_stratum.get(&s_idx).copied().unwrap_or(1.0);
            let share_budget = ((s_mass / total_mass.max(1e-4)) * max_tris as f32).ceil() as u32;
            let stratum_budget = share_budget.max(60).min(remaining_tris);
            let tris_per_stem = stratum_budget as f32 / list.len().max(1) as f32;

            // Choose multi-tuft detail dynamically; tall trees always keep detail >= 1 (20-tri organic
            // clusters, never pointy 6-vertex octahedrons) and stride gracefully when dense.
            let avg_h = list.iter().map(|s| s.height).sum::<f32>() / list.len().max(1) as f32;
            let (lobe_detail, lobes_per_tree, include_trunks) = if tris_per_stem >= 115.0 {
                (1u8, 5u32, true)
            } else if tris_per_stem >= 76.0 {
                (1u8, 3u32, true)
            } else if tris_per_stem >= 48.0 || avg_h >= 4.0 {
                (1u8, 2u32, true)
            } else if tris_per_stem >= 20.0 || avg_h >= 1.6 {
                (1u8, 1u32, false)
            } else {
                (0u8, 2u32, false)
            };

            let est_per = match (lobe_detail, lobes_per_tree, include_trunks) {
                (1, 5, true) => 116,
                (1, 3, true) => 76,
                (1, 2, true) => 52,
                (1, 1, true) => 32,
                (1, 1, false) => 20,
                _ => 16,
            };
            let max_stems = (stratum_budget / est_per).max(1) as usize;
            let stride = list.len().div_ceil(max_stems);

            let mut used_tris = 0u32;
            for (i, s) in list.iter().enumerate() {
                if i % stride != 0 {
                    continue;
                }
                if used_tris + est_per > stratum_budget {
                    break;
                }
                let lx = s.wx - origin_x;
                let lz = s.wz - origin_z;
                let base_pos = Vec3::new(lx, s.wy, lz);

                let top_col = [
                    (s.foliage_top_srgb[0] * s.color_tint[0]).clamp(0.0, 1.0),
                    (s.foliage_top_srgb[1] * s.color_tint[1]).clamp(0.0, 1.0),
                    (s.foliage_top_srgb[2] * s.color_tint[2]).clamp(0.0, 1.0),
                ];
                let bot_col = [
                    (s.foliage_bot_srgb[0] * s.color_tint[0]).clamp(0.0, 1.0),
                    (s.foliage_bot_srgb[1] * s.color_tint[1]).clamp(0.0, 1.0),
                    (s.foliage_bot_srgb[2] * s.color_tint[2]).clamp(0.0, 1.0),
                ];

                let show_trunk = s.is_dead
                    || (include_trunks
                        && (s.near_edge || s.cover_above < 0.45 || s.crown_base_y > 1.8));
                if show_trunk {
                    let trunk_h = if s.is_dead {
                        s.height * 0.85
                    } else {
                        (s.crown_base_y + (s.crown_top_y - s.crown_base_y) * 0.28).max(0.35)
                    };
                    let tip = Vec3::new(
                        lx + s.yaw.cos() * trunk_h * 0.04,
                        s.wy + trunk_h,
                        lz + s.yaw.sin() * trunk_h * 0.04,
                    );
                    let before = bark_mesh.indices.len() / 3;
                    append_tapered_trunk_tube(
                        &mut bark_mesh,
                        base_pos,
                        tip,
                        s.trunk_radius.max(0.04),
                        (s.trunk_radius * 0.45).max(0.025),
                        4,
                        s.bark_srgb,
                        stand.stand.condition.years_since_fire < 10.0,
                    );
                    used_tris += (bark_mesh.indices.len() / 3 - before) as u32;
                }

                if !s.is_dead {
                    let crown_h = (s.crown_top_y - s.crown_base_y).max(0.36);
                    let crown_mid_y = s.wy + s.crown_base_y + crown_h * 0.5;
                    // Never inflate single sub-lobe radii into giant smooth pillows or flat UFO discs
                    let r_xz =
                        (s.crown_radius * (if stride > 1 { 1.06 } else { 0.96 })).clamp(0.22, 4.4);
                    let r_y = (crown_h * 0.50).max(r_xz * 0.46).max(0.24);
                    let ao = (0.92 - 0.26 * s.cover_above).clamp(0.54, 1.0);

                    let rosette_blades: u32 = if tris_per_stem >= 90.0 {
                        9
                    } else if tris_per_stem >= 45.0 {
                        6
                    } else {
                        5
                    };
                    let rosette_tris = rosette_blades * 10;
                    if s.is_rosette && used_tris + rosette_tris <= stratum_budget {
                        // Crisp 3D spiky rosette tuft (grass tree, zamia, sedge, spinifex)
                        let before = foliage_mesh.indices.len() / 3;
                        append_opaque_rosette_tuft(
                            &mut foliage_mesh,
                            Vec3::new(lx, s.wy + s.crown_base_y.min(s.height * 0.35), lz),
                            r_xz.min(1.6),
                            crown_h.min(1.8),
                            rosette_blades,
                            s.priority,
                            top_col,
                            bot_col,
                            ao,
                        );
                        used_tris += (foliage_mesh.indices.len() / 3 - before) as u32;
                    } else if lobes_per_tree >= 2 || r_xz > 1.65 {
                        // Conifer detection: tall conical crown gets tapering stacked bough tiers + spire apex
                        let is_conical = s.height >= 6.0 && crown_h > r_xz * 1.45;
                        let n_lobes = if is_conical {
                            lobes_per_tree.clamp(2, 3)
                        } else if r_xz > 2.4 {
                            lobes_per_tree.max(3)
                        } else if s.height >= 4.5 {
                            lobes_per_tree.max(2)
                        } else {
                            lobes_per_tree.clamp(2, 3)
                        };
                        let sub_r_scale = match n_lobes {
                            5.. => 0.44,
                            3..=4 => 0.52,
                            _ => 0.58,
                        };
                        for l_i in 0..n_lobes {
                            if used_tris + 20 > stratum_budget {
                                break;
                            }
                            let is_apex = l_i == 0;
                            let (off, lr) = if is_conical {
                                let t = l_i as f32 / (n_lobes - 1).max(1) as f32;
                                let y_off = (0.42 - 0.72 * t) * r_y;
                                let tier_r = (r_xz * (0.36 + 0.48 * t)).min(2.1);
                                (
                                    Vec3::new(0.0, y_off, 0.0),
                                    Vec3::new(tier_r, (r_y * (0.46 - 0.10 * t)).min(2.2), tier_r),
                                )
                            } else {
                                let ang = s.yaw + (l_i as f32) * 2.399_963_2;
                                let ring = if is_apex { 0.16 } else { 0.50 };
                                let y_off = if is_apex {
                                    r_y * 0.30
                                } else {
                                    ((l_i % 2) as f32 * 0.34 - 0.20) * r_y
                                };
                                let rx = (r_xz
                                    * sub_r_scale
                                    * (0.84 + 0.28 * ((l_i as f32 * 1.7).sin() * 0.5 + 0.5)))
                                    .min(2.0);
                                let rz = (r_xz
                                    * sub_r_scale
                                    * (0.84 + 0.28 * ((l_i as f32 * 2.3).cos() * 0.5 + 0.5)))
                                    .min(2.0);
                                let ry = (r_y * (sub_r_scale * 0.88)).max(rx * 0.52).min(2.0);
                                (
                                    Vec3::new(
                                        ang.cos() * r_xz * ring,
                                        y_off,
                                        ang.sin() * r_xz * ring,
                                    ),
                                    Vec3::new(rx, ry, rz),
                                )
                            };
                            let c_top = if is_apex && let Some(org) = s.organ_srgb {
                                [
                                    top_col[0] * 0.68 + org[0] * 0.32,
                                    top_col[1] * 0.68 + org[1] * 0.32,
                                    top_col[2] * 0.68 + org[2] * 0.32,
                                ]
                            } else {
                                top_col
                            };
                            let before = foliage_mesh.indices.len() / 3;
                            append_sculpted_lobe(
                                &mut foliage_mesh,
                                Vec3::new(lx, crown_mid_y, lz) + off,
                                lr,
                                s.priority ^ ((l_i as u64 + 1).wrapping_mul(0x517C_C1B7)),
                                lobe_detail,
                                c_top,
                                bot_col,
                                ao * (if is_apex { 1.04 } else { 0.92 }),
                            );
                            used_tris += (foliage_mesh.indices.len() / 3 - before) as u32;
                        }
                    } else {
                        let c_top = if let Some(org) = s.organ_srgb {
                            [
                                top_col[0] * 0.72 + org[0] * 0.28,
                                top_col[1] * 0.72 + org[1] * 0.28,
                                top_col[2] * 0.72 + org[2] * 0.28,
                            ]
                        } else {
                            top_col
                        };
                        let before = foliage_mesh.indices.len() / 3;
                        append_sculpted_lobe(
                            &mut foliage_mesh,
                            Vec3::new(lx, crown_mid_y, lz),
                            Vec3::new(r_xz, r_y, r_xz),
                            s.priority,
                            lobe_detail,
                            c_top,
                            bot_col,
                            ao,
                        );
                        used_tris += (foliage_mesh.indices.len() / 3 - before) as u32;
                    }
                }
            }
            remaining_tris = remaining_tris.saturating_sub(used_tris);
        }

        // Combine bark + foliage submeshes into one stratum mesh
        let mut combined = Mesh::new();
        if !bark_mesh.indices.is_empty() {
            combined.merge(&bark_mesh, MaterialType::Bark);
        }
        if !foliage_mesh.indices.is_empty() {
            combined.merge(&foliage_mesh, MaterialType::Leaves);
        }
        if !combined.indices.is_empty() {
            out.insert(stratum.name.clone(), combined);
        }
    }

    out
}

/// Evaluated world-space canopy surface sample at `(wx, wz)`.
#[derive(Debug, Clone, Copy)]
struct CanopyFieldSample {
    ground_y: f32,
    canopy_top_y: f32,
    cover: f32,
    color_srgb: [f32; 3],
    bark_srgb: [f32; 3],
    excluded: bool,
}

/// Evaluate continuous world-space canopy height, cover, and colour at `(wx, wz)`.
/// Pure function of `(wx, wz)` + padded world stems → identical on shared tile boundaries!
fn sample_canopy_field(
    wx: f32,
    wz: f32,
    stems: &[PlacedStem],
    grid: &SpatialGrid,
    max_r: f32,
    eval: &TileFieldEvaluator,
    seed: u64,
) -> CanopyFieldSample {
    let ground_y = eval.height_at(wx, wz);
    let excluded = eval.excluded_at(wx, wz);
    if excluded || eval.mask_at(wx, wz) <= 0.04 {
        return CanopyFieldSample {
            ground_y,
            canopy_top_y: ground_y,
            cover: 0.0,
            color_srgb: [0.42, 0.40, 0.34],
            bark_srgb: [0.36, 0.32, 0.27],
            excluded,
        };
    }

    let mut max_top = ground_y;
    let mut cover_sum = 0.0_f32;
    let mut col_acc = [0.0_f32; 3];
    let mut bark_acc = [0.0_f32; 3];
    let mut col_w = 0.0_f32;

    grid.for_each_within(wx, wz, max_r, |idx| {
        let s = &stems[idx];
        if s.is_dead {
            return;
        }
        let dx = wx - s.wx;
        let dz = wz - s.wz;
        // Tight crown footprint keeps emergent trees crisp instead of inflating into solid gumdrop hills
        let r = (s.crown_radius * 0.92).max(0.35);
        let d2 = dx * dx + dz * dz;
        if d2 >= r * r {
            return;
        }
        let u = (1.0 - d2 / (r * r)).sqrt();
        let crown_h = (s.crown_top_y - s.crown_base_y).max(0.25);
        let top_y = s.wy + s.crown_base_y + crown_h * (0.14 + 0.86 * u.powf(1.25));
        if top_y > max_top {
            max_top = top_y;
        }
        let w = u * (0.35 + 0.65 * (s.height / 18.0).clamp(0.15, 1.2));
        cover_sum += 0.90 * u;
        // Blend sunlit crown top and shaded underside so Stand LOD 1/2 matches LOD 0 mean luminance
        let base_col = [
            s.foliage_top_srgb[0] * 0.54 + s.foliage_bot_srgb[0] * 0.46,
            s.foliage_top_srgb[1] * 0.54 + s.foliage_bot_srgb[1] * 0.46,
            s.foliage_top_srgb[2] * 0.54 + s.foliage_bot_srgb[2] * 0.46,
        ];
        let col = if let Some(org) = s.organ_srgb {
            [
                base_col[0] * 0.82 + org[0] * 0.18,
                base_col[1] * 0.82 + org[1] * 0.18,
                base_col[2] * 0.82 + org[2] * 0.18,
            ]
        } else {
            base_col
        };
        for c in 0..3 {
            col_acc[c] += (col[c] * s.color_tint[c]).clamp(0.0, 1.0) * w;
            bark_acc[c] += s.bark_srgb[c] * w;
        }
        col_w += w;
    });

    let cover = 1.0 - (-cover_sum).exp();
    let (color_srgb, bark_srgb) = if col_w > 1e-4 {
        (
            [col_acc[0] / col_w, col_acc[1] / col_w, col_acc[2] / col_w],
            [
                bark_acc[0] / col_w,
                bark_acc[1] / col_w,
                bark_acc[2] / col_w,
            ],
        )
    } else {
        ([0.34, 0.42, 0.29], [0.38, 0.33, 0.28])
    };

    // Organic world-space micro-undulation so upper canopy reads as living crowns, not flat facets
    let bump = if cover > 0.08 {
        let n = world_fbm2(wx * 0.14, wz * 0.14, seed ^ 0xCA01) - 0.5;
        n * ((max_top - ground_y) * 0.16).min(1.8)
    } else {
        0.0
    };

    CanopyFieldSample {
        ground_y,
        canopy_top_y: (max_top + bump).max(ground_y),
        cover,
        color_srgb,
        bark_srgb,
        excluded: false,
    }
}

/// Build a continuous displaced canopy mesh (Stand LOD 1 or Stand LOD 2) with real gaps,
/// closed skirts along mask/exclusion edges, and exact shared border vertices across tiles.
#[allow(clippy::too_many_arguments)]
fn build_canopy_surface_lod(
    grid_n: usize,
    origin_x: f32,
    origin_z: f32,
    tile_size: f32,
    stems: &[PlacedStem],
    grid: &SpatialGrid,
    max_r: f32,
    eval: &TileFieldEvaluator,
    seed: u64,
    max_triangles: u32,
    add_edge_trunks: bool,
) -> Mesh {
    let verts_per_side = grid_n + 1;
    let step = tile_size / grid_n as f32;

    let mut samples = Vec::with_capacity(verts_per_side * verts_per_side);
    for j in 0..verts_per_side {
        let wz = origin_z + j as f32 * step;
        for i in 0..verts_per_side {
            let wx = origin_x + i as f32 * step;
            samples.push(sample_canopy_field(wx, wz, stems, grid, max_r, eval, seed));
        }
    }

    // Cross-LOD seam constraint: on fine grids (LOD1, divisible by 4), linearly interpolate
    // intermediate tile-border vertex heights & cover between their stride-4 coarse anchor
    // vertices so mixed LOD1/LOD2 neighbors meet along exact collinear segments with zero cracks.
    if grid_n >= 16 && grid_n.is_multiple_of(4) {
        for coarse in (0..grid_n).step_by(4) {
            let c0 = coarse;
            let c1 = coarse + 4;
            for sub in 1..4 {
                let t = sub as f32 / 4.0;
                // Bottom (j=0) and top (j=grid_n) horizontal tile borders
                for j in [0, grid_n] {
                    let a = samples[j * verts_per_side + c0];
                    let b = samples[j * verts_per_side + c1];
                    let slot = &mut samples[j * verts_per_side + (c0 + sub)];
                    slot.canopy_top_y = a.canopy_top_y + (b.canopy_top_y - a.canopy_top_y) * t;
                    slot.ground_y = a.ground_y + (b.ground_y - a.ground_y) * t;
                    slot.cover = a.cover + (b.cover - a.cover) * t;
                }
                // Left (i=0) and right (i=grid_n) vertical tile borders
                for i in [0, grid_n] {
                    let a = samples[c0 * verts_per_side + i];
                    let b = samples[c1 * verts_per_side + i];
                    let slot = &mut samples[(c0 + sub) * verts_per_side + i];
                    slot.canopy_top_y = a.canopy_top_y + (b.canopy_top_y - a.canopy_top_y) * t;
                    slot.ground_y = a.ground_y + (b.ground_y - a.ground_y) * t;
                    slot.cover = a.cover + (b.cover - a.cover) * t;
                }
            }
        }
    }

    let s_at = |i: usize, j: usize| samples[j * verts_per_side + i];

    // Cell is active where any corner carries canopy/shrub cover and not all excluded
    let cover_thresh = 0.08;
    let mut cell_active = vec![false; grid_n * grid_n];
    for j in 0..grid_n {
        for i in 0..grid_n {
            let s00 = s_at(i, j);
            let s10 = s_at(i + 1, j);
            let s01 = s_at(i, j + 1);
            let s11 = s_at(i + 1, j + 1);
            let max_cov = s00.cover.max(s10.cover).max(s01.cover).max(s11.cover);
            let any_excl = s00.excluded || s10.excluded || s01.excluded || s11.excluded;
            let avg_h = (s00.canopy_top_y - s00.ground_y + s10.canopy_top_y - s10.ground_y
                + s01.canopy_top_y
                - s01.ground_y
                + s11.canopy_top_y
                - s11.ground_y)
                * 0.25;
            cell_active[j * grid_n + i] = !any_excl && max_cov >= cover_thresh && avg_h > 0.20;
        }
    }

    // On coarse LOD 2 (grid_n <= 8), fill 1-cell non-excluded pinholes to prevent jagged dark skirt slivers
    // while preserving all real tracks, roads, and clearings.
    if grid_n <= 8 {
        let orig = cell_active.clone();
        for j in 1..(grid_n.saturating_sub(1)) {
            for i in 1..(grid_n.saturating_sub(1)) {
                if !orig[j * grid_n + i] {
                    let s00 = s_at(i, j);
                    let s10 = s_at(i + 1, j);
                    let s01 = s_at(i, j + 1);
                    let s11 = s_at(i + 1, j + 1);
                    let any_excl = s00.excluded || s10.excluded || s01.excluded || s11.excluded;
                    if !any_excl
                        && orig[j * grid_n + (i - 1)]
                        && orig[j * grid_n + (i + 1)]
                        && orig[(j - 1) * grid_n + i]
                        && orig[(j + 1) * grid_n + i]
                    {
                        cell_active[j * grid_n + i] = true;
                    }
                }
            }
        }
    }

    let mut foliage_mesh = Mesh::new();
    let mut bark_mesh = Mesh::new();

    // Emit shared-vertex top surface grid
    let mut top_vert_idx: Vec<Option<u32>> = vec![None; verts_per_side * verts_per_side];
    let mut get_or_push_top = |i: usize, j: usize, m: &mut Mesh| -> u32 {
        let k = j * verts_per_side + i;
        if let Some(id) = top_vert_idx[k] {
            return id;
        }
        let s = s_at(i, j);
        let lx = i as f32 * step;
        let lz = j as f32 * step;
        // Finite-difference normal from world-space canopy samples (seamless across borders!)
        let sx_l = if i > 0 {
            s_at(i - 1, j).canopy_top_y
        } else {
            sample_canopy_field(
                origin_x - step,
                origin_z + lz,
                stems,
                grid,
                max_r,
                eval,
                seed,
            )
            .canopy_top_y
        };
        let sx_r = if i + 1 < verts_per_side {
            s_at(i + 1, j).canopy_top_y
        } else {
            sample_canopy_field(
                origin_x + tile_size + step,
                origin_z + lz,
                stems,
                grid,
                max_r,
                eval,
                seed,
            )
            .canopy_top_y
        };
        let sz_d = if j > 0 {
            s_at(i, j - 1).canopy_top_y
        } else {
            sample_canopy_field(
                origin_x + lx,
                origin_z - step,
                stems,
                grid,
                max_r,
                eval,
                seed,
            )
            .canopy_top_y
        };
        let sz_u = if j + 1 < verts_per_side {
            s_at(i, j + 1).canopy_top_y
        } else {
            sample_canopy_field(
                origin_x + lx,
                origin_z + tile_size + step,
                stems,
                grid,
                max_r,
                eval,
                seed,
            )
            .canopy_top_y
        };
        let normal = Vec3::new(sx_l - sx_r, 2.0 * step, sz_d - sz_u).normalize_or_zero();
        let lin = srgb3_to_linear(s.color_srgb);
        let shade = (0.58 + 0.34 * normal.y.clamp(0.0, 1.0)) * (0.76 + 0.24 * s.cover);
        let id = m.vertices.len() as u32;
        m.vertices.push(Vertex {
            position: Vec3::new(lx, s.canopy_top_y, lz),
            normal,
            uv: Vec2::new(i as f32 / grid_n as f32, j as f32 / grid_n as f32),
            uv2: Vec2::ZERO,
            color: [lin[0] * shade, lin[1] * shade, lin[2] * shade, 1.0].into(),
        });
        top_vert_idx[k] = Some(id);
        id
    };

    let mut tri_count = 0u32;
    for j in 0..grid_n {
        for i in 0..grid_n {
            if !cell_active[j * grid_n + i] {
                continue;
            }
            if tri_count + 2 > max_triangles {
                break;
            }
            let v00 = get_or_push_top(i, j, &mut foliage_mesh);
            let v10 = get_or_push_top(i + 1, j, &mut foliage_mesh);
            let v01 = get_or_push_top(i, j + 1, &mut foliage_mesh);
            let v11 = get_or_push_top(i + 1, j + 1, &mut foliage_mesh);
            // Counter-clockwise upward-facing winding (+Y normal)
            foliage_mesh
                .indices
                .extend_from_slice(&[v00, v01, v11, v00, v11, v10]);
            tri_count += 2;
        }
    }

    // Helper to append a closed vertical skirt quad along an active-cell rim facing a gap/boundary
    let mut emit_skirt_edge = |i0: usize, j0: usize, i1: usize, j1: usize, out_dir: Vec3| {
        if tri_count + 2 > max_triangles {
            return;
        }
        let s0 = s_at(i0, j0);
        let s1 = s_at(i1, j1);
        let p0_top = Vec3::new(i0 as f32 * step, s0.canopy_top_y, j0 as f32 * step);
        let p1_top = Vec3::new(i1 as f32 * step, s1.canopy_top_y, j1 as f32 * step);
        let p0_bot = Vec3::new(i0 as f32 * step, s0.ground_y - 0.15, j0 as f32 * step);
        let p1_bot = Vec3::new(i1 as f32 * step, s1.ground_y - 0.15, j1 as f32 * step);

        let top_c0 = srgb3_to_linear(s0.color_srgb);
        let top_c1 = srgb3_to_linear(s1.color_srgb);
        let bot_c0 = srgb3_to_linear([
            s0.color_srgb[0] * 0.45 + s0.bark_srgb[0] * 0.35,
            s0.color_srgb[1] * 0.45 + s0.bark_srgb[1] * 0.35,
            s0.color_srgb[2] * 0.45 + s0.bark_srgb[2] * 0.35,
        ]);
        let bot_c1 = srgb3_to_linear([
            s1.color_srgb[0] * 0.45 + s1.bark_srgb[0] * 0.35,
            s1.color_srgb[1] * 0.45 + s1.bark_srgb[1] * 0.35,
            s1.color_srgb[2] * 0.45 + s1.bark_srgb[2] * 0.35,
        ]);

        let b = foliage_mesh.vertices.len() as u32;
        for (pos, col, uv) in [
            (
                p0_top,
                [top_c0[0] * 0.85, top_c0[1] * 0.85, top_c0[2] * 0.85, 1.0],
                Vec2::new(0.0, 1.0),
            ),
            (
                p1_top,
                [top_c1[0] * 0.85, top_c1[1] * 0.85, top_c1[2] * 0.85, 1.0],
                Vec2::new(1.0, 1.0),
            ),
            (
                p1_bot,
                [bot_c1[0] * 0.65, bot_c1[1] * 0.65, bot_c1[2] * 0.65, 1.0],
                Vec2::new(1.0, 0.0),
            ),
            (
                p0_bot,
                [bot_c0[0] * 0.65, bot_c0[1] * 0.65, bot_c0[2] * 0.65, 1.0],
                Vec2::new(0.0, 0.0),
            ),
        ] {
            foliage_mesh.vertices.push(Vertex {
                position: pos,
                normal: out_dir,
                uv,
                uv2: Vec2::ZERO,
                color: col.into(),
            });
        }
        // Guarantee outward-facing triangle winding so single-sided shaders never render black backfaces!
        let geom_n = (p1_top - p0_top).cross(p1_bot - p0_top);
        if geom_n.dot(out_dir) >= 0.0 {
            foliage_mesh
                .indices
                .extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        } else {
            foliage_mesh
                .indices
                .extend_from_slice(&[b, b + 2, b + 1, b, b + 3, b + 2]);
        }
        tri_count += 2;
    };

    // Emit skirts wherever an active cell meets an inactive gap/exclusion/boundary cell
    for j in 0..grid_n {
        for i in 0..grid_n {
            if !cell_active[j * grid_n + i] {
                continue;
            }
            // -Z edge
            if j > 0 && !cell_active[(j - 1) * grid_n + i] {
                emit_skirt_edge(i + 1, j, i, j, -Vec3::Z);
            } else if j == 0 && (eval.mask.is_some() || eval.exclude.is_some()) {
                let s0 = s_at(i, 0);
                if s0.cover < 0.25 {
                    emit_skirt_edge(i + 1, 0, i, 0, -Vec3::Z);
                }
            }
            // +Z edge
            if j + 1 < grid_n && !cell_active[(j + 1) * grid_n + i] {
                emit_skirt_edge(i, j + 1, i + 1, j + 1, Vec3::Z);
            } else if j + 1 == grid_n && (eval.mask.is_some() || eval.exclude.is_some()) {
                let s0 = s_at(i, grid_n);
                if s0.cover < 0.25 {
                    emit_skirt_edge(i, grid_n, i + 1, grid_n, Vec3::Z);
                }
            }
            // -X edge
            if i > 0 && !cell_active[j * grid_n + (i - 1)] {
                emit_skirt_edge(i, j, i, j + 1, -Vec3::X);
            } else if i == 0 && (eval.mask.is_some() || eval.exclude.is_some()) {
                let s0 = s_at(0, j);
                if s0.cover < 0.25 {
                    emit_skirt_edge(0, j, 0, j + 1, -Vec3::X);
                }
            }
            // +X edge
            if i + 1 < grid_n && !cell_active[j * grid_n + (i + 1)] {
                emit_skirt_edge(i + 1, j + 1, i + 1, j, Vec3::X);
            } else if i + 1 == grid_n && (eval.mask.is_some() || eval.exclude.is_some()) {
                let s0 = s_at(grid_n, j);
                if s0.cover < 0.25 {
                    emit_skirt_edge(grid_n, j + 1, grid_n, j, Vec3::X);
                }
            }
        }
    }

    // For Stand LOD 1, add visible low-poly bark trunk pillars along road/track/mask edges
    if add_edge_trunks {
        let mut edge_stems: Vec<&PlacedStem> = stems
            .iter()
            .filter(|s| s.in_tile && (s.near_edge || s.cover_above < 0.20) && s.height >= 2.5)
            .collect();
        edge_stems.sort_by_key(|s| s.priority);
        for s in edge_stems.into_iter().take(120) {
            if tri_count + 12 > max_triangles {
                break;
            }
            let lx = s.wx - origin_x;
            let lz = s.wz - origin_z;
            let h = (s.crown_base_y + (s.crown_top_y - s.crown_base_y) * 0.35).max(1.2);
            let before = bark_mesh.indices.len() / 3;
            append_tapered_trunk_tube(
                &mut bark_mesh,
                Vec3::new(lx, s.wy, lz),
                Vec3::new(lx, s.wy + h, lz),
                s.trunk_radius.max(0.08),
                (s.trunk_radius * 0.55).max(0.05),
                4,
                s.bark_srgb,
                false,
            );
            tri_count += (bark_mesh.indices.len() / 3 - before) as u32;
        }
    }

    let mut combined = Mesh::new();
    if !bark_mesh.indices.is_empty() {
        combined.merge(&bark_mesh, MaterialType::Bark);
    }
    if !foliage_mesh.indices.is_empty() {
        combined.merge(&foliage_mesh, MaterialType::Leaves);
    }
    // Guarantee non-empty valid mesh even if tile is 100% masked out (1 tiny degenerate-free ground triangle)
    if combined.indices.is_empty() {
        append_sculpted_lobe(
            &mut combined,
            Vec3::new(tile_size * 0.5, 0.05, tile_size * 0.5),
            Vec3::splat(0.05),
            seed,
            0,
            [0.35, 0.40, 0.28],
            [0.25, 0.30, 0.20],
            0.8,
        );
        combined.submeshes.push(Submesh {
            index_start: 0,
            index_count: combined.indices.len() as u32,
            material: MaterialType::Leaves,
        });
    }
    combined
}

/// Write per-tile 4 analytical rasters (`canopy_height.png`, `canopy_cover.png`,
/// `stem_density.png`, `dominant_color.png`) and return `(manifest, achieved_total_cover, canopy_heightfield)`.
#[allow(clippy::too_many_arguments)]
fn write_tile_rasters(
    rasters_dir: &Path,
    tile_rel_prefix: &str,
    res: u32,
    origin_x: f32,
    origin_z: f32,
    tile_size: f32,
    stems: &[PlacedStem],
    grid: &SpatialGrid,
    max_r: f32,
    eval: &TileFieldEvaluator,
    seed: u64,
    include_heightfield: bool,
) -> Result<
    (
        StandTileRastersManifest,
        f32,
        Option<StandCanopyHeightfield>,
    ),
    StandError,
> {
    std::fs::create_dir_all(rasters_dir)?;
    let res = res.max(16);

    let mut heights = vec![0.0_f32; (res * res) as usize];
    let mut covers = vec![0.0_f32; (res * res) as usize];
    let mut densities = vec![0.0_f32; (res * res) as usize];
    let mut colors = vec![[0u8; 3]; (res * res) as usize];

    let mut max_h = 1.0_f32;
    let mut max_dens = 100.0_f32;
    let mut cover_acc = 0.0_f64;
    let mut valid_pixels = 0usize;

    // Bin stem count into raster cells for stem_density
    let cell_ha = ((tile_size / res as f32) * (tile_size / res as f32)) / 10_000.0;
    let mut raw_counts = vec![0u32; (res * res) as usize];
    for s in stems {
        if !s.in_tile {
            continue;
        }
        let u = ((s.wx - origin_x) / tile_size).clamp(0.0, 0.9999);
        let v = ((s.wz - origin_z) / tile_size).clamp(0.0, 0.9999);
        let px = (u * res as f32) as u32;
        let py = (v * res as f32) as u32;
        raw_counts[(py * res + px) as usize] += 1;
    }

    for y in 0..res {
        let wz = origin_z + (y as f32 + 0.5) / res as f32 * tile_size;
        for x in 0..res {
            let wx = origin_x + (x as f32 + 0.5) / res as f32 * tile_size;
            let idx = (y * res + x) as usize;
            let sample = sample_canopy_field(wx, wz, stems, grid, max_r, eval, seed);
            let h_above = (sample.canopy_top_y - sample.ground_y).max(0.0);
            heights[idx] = sample.canopy_top_y;
            max_h = max_h.max(h_above);
            covers[idx] = sample.cover;
            if !sample.excluded && eval.mask_at(wx, wz) > 0.5 {
                cover_acc += sample.cover as f64;
                valid_pixels += 1;
            }
            // 3x3 smoothed stem density in stems/ha
            let mut sum_c = 0u32;
            let mut cnt_n = 0u32;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && nx < res as i32 && ny >= 0 && ny < res as i32 {
                        sum_c += raw_counts[(ny as u32 * res + nx as u32) as usize];
                        cnt_n += 1;
                    }
                }
            }
            let dens = (sum_c as f32 / (cnt_n as f32 * cell_ha.max(1e-6))).max(0.0);
            densities[idx] = dens;
            max_dens = max_dens.max(dens);
            colors[idx] = [
                (sample.color_srgb[0].clamp(0.0, 1.0) * 255.0) as u8,
                (sample.color_srgb[1].clamp(0.0, 1.0) * 255.0) as u8,
                (sample.color_srgb[2].clamp(0.0, 1.0) * 255.0) as u8,
            ];
        }
    }

    // Write 16-bit canopy_height.png
    let mut h_img: ImageBuffer<Luma<u16>, Vec<u16>> = ImageBuffer::new(res, res);
    for y in 0..res {
        for x in 0..res {
            let idx = (y * res + x) as usize;
            let g = eval.height_at(
                origin_x + (x as f32 + 0.5) / res as f32 * tile_size,
                origin_z + (y as f32 + 0.5) / res as f32 * tile_size,
            );
            let rel = ((heights[idx] - g).max(0.0) / max_h).clamp(0.0, 1.0);
            h_img.put_pixel(x, y, Luma([(rel * 65535.0) as u16]));
        }
    }
    h_img
        .save(rasters_dir.join("canopy_height.png"))
        .map_err(|e| StandError::Export(format!("canopy_height.png: {e}")))?;

    // Write 8-bit canopy_cover.png
    let mut c_img = GrayImage::new(res, res);
    for y in 0..res {
        for x in 0..res {
            let v = (covers[(y * res + x) as usize].clamp(0.0, 1.0) * 255.0) as u8;
            c_img.put_pixel(x, y, Luma([v]));
        }
    }
    c_img
        .save(rasters_dir.join("canopy_cover.png"))
        .map_err(|e| StandError::Export(format!("canopy_cover.png: {e}")))?;

    // Write 8-bit stem_density.png
    let mut d_img = GrayImage::new(res, res);
    for y in 0..res {
        for x in 0..res {
            let v = ((densities[(y * res + x) as usize] / max_dens).clamp(0.0, 1.0) * 255.0) as u8;
            d_img.put_pixel(x, y, Luma([v]));
        }
    }
    d_img
        .save(rasters_dir.join("stem_density.png"))
        .map_err(|e| StandError::Export(format!("stem_density.png: {e}")))?;

    // Write 8-bit RGB dominant_color.png
    let mut col_img = RgbImage::new(res, res);
    for y in 0..res {
        for x in 0..res {
            col_img.put_pixel(x, y, Rgb(colors[(y * res + x) as usize]));
        }
    }
    col_img
        .save(rasters_dir.join("dominant_color.png"))
        .map_err(|e| StandError::Export(format!("dominant_color.png: {e}")))?;

    let achieved_cover = if valid_pixels > 0 {
        (cover_acc / valid_pixels as f64) as f32
    } else {
        0.0
    };

    let hf = include_heightfield.then(|| {
        let hf_res = 16u32;
        let mut hf_vals = Vec::with_capacity((hf_res * hf_res) as usize);
        for y in 0..hf_res {
            let sy = ((y as f32 / (hf_res - 1) as f32) * (res - 1) as f32).round() as u32;
            for x in 0..hf_res {
                let sx = ((x as f32 / (hf_res - 1) as f32) * (res - 1) as f32).round() as u32;
                hf_vals.push(heights[(sy * res + sx) as usize]);
            }
        }
        StandCanopyHeightfield {
            resolution: hf_res,
            tile_size_m: tile_size,
            origin_xz: [origin_x, origin_z],
            heights_m: hf_vals,
        }
    });

    Ok((
        StandTileRastersManifest {
            resolution: res,
            canopy_height: format!("{tile_rel_prefix}/rasters/canopy_height.png"),
            canopy_height_max_m: max_h,
            canopy_cover: format!("{tile_rel_prefix}/rasters/canopy_cover.png"),
            stem_density: format!("{tile_rel_prefix}/rasters/stem_density.png"),
            stem_density_max_per_ha: max_dens,
            dominant_color: format!("{tile_rel_prefix}/rasters/dominant_color.png"),
        },
        achieved_cover,
        hf,
    ))
}

/// Configuration for exporting one tile or a region of tiles.
#[derive(Debug, Clone)]
pub struct StandExportOptions {
    pub tile_size_m: f32,
    pub foliage_mode: FoliageRenderMode,
    pub budget: StandBudget,
    pub write_library: bool,
    pub raster_resolution: u32,
}

impl Default for StandExportOptions {
    fn default() -> Self {
        Self {
            tile_size_m: 128.0,
            foliage_mode: FoliageRenderMode::Cutout,
            budget: StandBudget::default(),
            write_library: true,
            raster_resolution: 64,
        }
    }
}

/// Export one or more tiles into `out_dir` and write `midori_stand.json`.
pub fn generate_stand_tiles(
    stand: &NatureStand,
    base_dir: &Path,
    out_dir: &Path,
    tiles: &[[i32; 2]],
    inputs: &StandTileInputs,
    options: &StandExportOptions,
) -> Result<(StandManifest, StandLibrary, Vec<StandTileStats>), StandError> {
    std::fs::create_dir_all(out_dir)?;
    let library = build_stand_library(
        stand,
        base_dir,
        Some(out_dir),
        options.foliage_mode,
        options.write_library,
    )?;

    let mut tile_manifests = Vec::with_capacity(tiles.len());
    let mut tile_stats = Vec::with_capacity(tiles.len());

    // Sort tiles deterministically in the manifest so region generation order never alters manifest bytes
    let mut sorted_tiles = tiles.to_vec();
    sorted_tiles.sort();
    sorted_tiles.dedup();

    for &tile in &sorted_tiles {
        let (tm, st) = write_single_stand_tile(stand, &library, out_dir, tile, inputs, options)?;
        tile_manifests.push(tm);
        tile_stats.push(st);
    }

    let alpha_mode = match options.foliage_mode {
        FoliageRenderMode::Opaque => "OPAQUE",
        FoliageRenderMode::Cutout => "MASK",
    };
    let shared_materials = vec![
        StandMaterialSlot {
            name: "stand_ground".to_string(),
            alpha_mode: "OPAQUE".to_string(),
            role: "floor_albedo_normal".to_string(),
        },
        StandMaterialSlot {
            name: "stand_bark".to_string(),
            alpha_mode: "OPAQUE".to_string(),
            role: "shared_bark_atlas".to_string(),
        },
        StandMaterialSlot {
            name: "stand_foliage".to_string(),
            alpha_mode: alpha_mode.to_string(),
            role: if options.foliage_mode == FoliageRenderMode::Opaque {
                "opaque_vertex_color_canopy".to_string()
            } else {
                "shared_foliage_atlas".to_string()
            },
        },
    ];

    let manifest = StandManifest {
        schema_version: STAND_MANIFEST_SCHEMA_VERSION,
        kind: "nature_stand".to_string(),
        name: stand.asset.name.clone(),
        biome: stand.stand.biome.clone(),
        seed: stand.stand.seed,
        tile_size_m: options.tile_size_m,
        foliage_mode: options.foliage_mode,
        floor_preset: stand.stand.floor.clone(),
        cross_lod_stitch_rule:
            "LOD1 (32x32) and LOD2 (8x8) share exact stride-4 world-space border vertices; adjacent same-LOD tiles match bit-for-bit with zero cracks."
                .to_string(),
        shared_materials,
        library: library.variants.clone(),
        tiles: tile_manifests,
    };

    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| StandError::Export(format!("midori_stand.json: {e}")))?;
    std::fs::write(out_dir.join("midori_stand.json"), manifest_json)?;

    Ok((manifest, library, tile_stats))
}

fn write_single_stand_tile(
    stand: &NatureStand,
    library: &StandLibrary,
    out_dir: &Path,
    tile: [i32; 2],
    inputs: &StandTileInputs,
    options: &StandExportOptions,
) -> Result<(StandTileManifest, StandTileStats), StandError> {
    #[cfg(not(target_arch = "wasm32"))]
    let t_start = std::time::Instant::now();
    let tile_size = options.tile_size_m;
    let origin_x = tile[0] as f32 * tile_size;
    let origin_z = tile[1] as f32 * tile_size;

    let tile_rel = format!("tiles/t_{}_{}", tile[0], tile[1]);
    let tile_dir = out_dir.join(&tile_rel);
    std::fs::create_dir_all(&tile_dir)?;

    let eval = TileFieldEvaluator::new(origin_x, origin_z, tile_size, inputs);
    let mut stems = place_stand_stems(stand, library, tile, tile_size, inputs, &eval);
    assign_tiers(&mut stems, options.budget.max_instances_per_tile);

    // Build spatial grid over all padded stems for canopy surface & rasters
    let mut all_grid = SpatialGrid::new(8.0);
    let mut max_r = 2.0_f32;
    for (idx, s) in stems.iter().enumerate() {
        let r = s.crown_radius * 1.15;
        if r > max_r {
            max_r = r;
        }
        all_grid.insert(idx, s.wx, s.wz);
    }

    // Export instances.json & instances.bin (strictly ordered for determinism)
    let mut instances: Vec<StandInstanceRecord> = stems
        .iter()
        .filter(|s| s.in_tile)
        .map(|s| StandInstanceRecord {
            variant_id: library.variants[s.variant_index as usize]
                .variant_id
                .clone(),
            variant_index: s.variant_index,
            stratum: stand.strata[s.stratum_idx as usize].name.clone(),
            stratum_index: s.stratum_idx,
            tier: s.tier,
            position: [s.wx, s.wy, s.wz],
            yaw: s.yaw,
            scale: s.scale,
            color_tint: s.color_tint,
            near_edge: s.near_edge,
        })
        .collect();
    instances.sort_by(|a, b| {
        a.stratum_index
            .cmp(&b.stratum_index)
            .then_with(|| a.position[0].total_cmp(&b.position[0]))
            .then_with(|| a.position[2].total_cmp(&b.position[2]))
            .then_with(|| a.variant_index.cmp(&b.variant_index))
    });

    let inst_json_rel = format!("{tile_rel}/instances.json");
    let inst_bin_rel = format!("{tile_rel}/instances.bin");
    std::fs::write(
        out_dir.join(&inst_json_rel),
        serde_json::to_string_pretty(&instances)
            .map_err(|e| StandError::Export(format!("instances.json: {e}")))?,
    )?;
    std::fs::write(
        out_dir.join(&inst_bin_rel),
        encode_stand_instances_bin(&instances),
    )?;

    // Export Stand LOD 0 meshes (one per stratum with merged stems)
    let stand_export_cfg = ExportConfig {
        opaque_foliage: true, // Volumetric stand LOD meshes are always closed opaque geometry
        leaf_roughness: Some(0.82),
        metadata: Some(ExportMetadata {
            species_name: format!("{} t_{}_{}", stand.asset.name, tile[0], tile[1]),
            scientific_name: stand.stand.biome.clone(),
            seed: Some(stand.stand.seed),
            lod_screen_heights: vec![],
        }),
        ..ExportConfig::default()
    };

    let lod0_map = build_lod0_stratum_meshes(stand, &stems, origin_x, origin_z, &options.budget);
    let mut lod0_manifest = BTreeMap::new();
    let mut lod0_tris_by_stratum: BTreeMap<String, u32> = BTreeMap::new();
    let mut total_lod0_tris = 0u32;
    for (stratum_name, mesh) in &lod0_map {
        let rel = format!("{tile_rel}/stand_lod0_{stratum_name}.glb");
        crate::stand::validate_stand_relative_path(&rel, "tile.lod0_meshes")?;
        export_mesh(mesh, &out_dir.join(&rel), &stand_export_cfg)
            .map_err(|e| StandError::Export(format!("{rel}: {e}")))?;
        let tris = (mesh.indices.len() / 3) as u32;
        total_lod0_tris += tris;
        lod0_tris_by_stratum.insert(stratum_name.clone(), tris);
        lod0_manifest.insert(stratum_name.clone(), rel);
    }

    // Export Stand LOD 1 & LOD 2 as strictly nested stride-4 grids (32/8, 24/6, or 16/4)
    // so adjacent mixed-LOD tiles always match along collinear border segments with zero cracks.
    let (lod1_grid, lod2_grid) = if options.budget.max_triangles_lod1 >= 3_200
        && options.budget.max_triangles_lod2 >= 180
    {
        (32usize, 8usize)
    } else if options.budget.max_triangles_lod1 >= 1_800 && options.budget.max_triangles_lod2 >= 120
    {
        (24usize, 6usize)
    } else {
        (16usize, 4usize)
    };
    let lod1_mesh = build_canopy_surface_lod(
        lod1_grid,
        origin_x,
        origin_z,
        tile_size,
        &stems,
        &all_grid,
        max_r,
        &eval,
        stand.stand.seed,
        options.budget.max_triangles_lod1,
        true,
    );
    let lod1_rel = format!("{tile_rel}/stand_lod1_canopy.glb");
    export_mesh(&lod1_mesh, &out_dir.join(&lod1_rel), &stand_export_cfg)
        .map_err(|e| StandError::Export(format!("{lod1_rel}: {e}")))?;
    let total_lod1_tris = (lod1_mesh.indices.len() / 3) as u32;

    let lod2_mesh = build_canopy_surface_lod(
        lod2_grid,
        origin_x,
        origin_z,
        tile_size,
        &stems,
        &all_grid,
        max_r,
        &eval,
        stand.stand.seed,
        options.budget.max_triangles_lod2,
        false,
    );
    let lod2_rel = format!("{tile_rel}/stand_lod2_far.glb");
    export_mesh(&lod2_mesh, &out_dir.join(&lod2_rel), &stand_export_cfg)
        .map_err(|e| StandError::Export(format!("{lod2_rel}: {e}")))?;
    let total_lod2_tris = (lod2_mesh.indices.len() / 3) as u32;

    // Write tile rasters
    let (rasters_manifest, achieved_total_cover, canopy_hf) = write_tile_rasters(
        &tile_dir.join("rasters"),
        &tile_rel,
        options.raster_resolution,
        origin_x,
        origin_z,
        tile_size,
        &stems,
        &all_grid,
        max_r,
        &eval,
        stand.stand.seed,
        stand.stand.include_canopy_heightfield,
    )?;

    // Write collision.json
    let min_r = stand.stand.collision_min_diameter * 0.5;
    let mut capsules: Vec<StandCollisionCapsule> = stems
        .iter()
        .filter(|s| s.in_tile && s.trunk_radius >= min_r)
        .map(|s| StandCollisionCapsule {
            position: [s.wx, s.wy, s.wz],
            radius: s.trunk_radius,
            height: s.height,
            stratum: stand.strata[s.stratum_idx as usize].name.clone(),
            variant_id: library.variants[s.variant_index as usize]
                .variant_id
                .clone(),
        })
        .collect();
    capsules.sort_by(|a, b| {
        a.position[0]
            .total_cmp(&b.position[0])
            .then_with(|| a.position[2].total_cmp(&b.position[2]))
    });
    let collision = StandCollisionFile {
        tile,
        min_diameter_m: stand.stand.collision_min_diameter,
        capsules,
        canopy_heightfield: canopy_hf,
    };
    let col_rel = format!("{tile_rel}/collision.json");
    std::fs::write(
        out_dir.join(&col_rel),
        serde_json::to_string_pretty(&collision)
            .map_err(|e| StandError::Export(format!("collision.json: {e}")))?,
    )?;

    // Compute per-stratum stats (deterministic — note generation_ms is stored in stats,
    // wait: keep stand_stats.json byte-identical across shuffled runs by setting generation_ms = 0
    // on disk and returning live elapsed in memory? Let's keep generation_ms deterministic = 0 in JSON
    // OR check Seam test: "a 4 x 4 block of tiles generated in shuffled order equals the same block
    // generated in order, byte for byte" — YES! Never put wall-clock timing into hashed tile files!)
    let tile_area = (tile_size * tile_size).max(1.0);
    let mut strata_stats = Vec::with_capacity(stand.strata.len());
    let mut total_instanced = 0u32;
    let mut total_merged = 0u32;
    let mut req_cover_prod = 1.0_f32;

    for (s_idx, stratum) in stand.strata.iter().enumerate() {
        let mut s_stems = 0u32;
        let mut s_inst = 0u32;
        let mut s_merged = 0u32;
        let mut crown_area_sum = 0.0_f32;
        for s in &stems {
            if s.in_tile && s.stratum_idx as usize == s_idx {
                s_stems += 1;
                if s.tier == StandTier::Instance {
                    s_inst += 1;
                } else {
                    s_merged += 1;
                }
                crown_area_sum += std::f32::consts::PI * s.crown_radius * s.crown_radius;
            }
        }
        total_instanced += s_inst;
        total_merged += s_merged;
        req_cover_prod *= 1.0 - stratum.cover.clamp(0.0, 0.99);
        let achieved_s = 1.0 - (-(crown_area_sum / tile_area)).exp();
        strata_stats.push(StratumStats {
            name: stratum.name.clone(),
            stems: s_stems,
            instanced: s_inst,
            merged: s_merged,
            requested_cover: stratum.cover,
            achieved_cover: achieved_s,
            lod0_triangles: lod0_tris_by_stratum
                .get(&stratum.name)
                .copied()
                .unwrap_or(0),
        });
    }

    let requested_total_cover = 1.0 - req_cover_prod;
    #[cfg(not(target_arch = "wasm32"))]
    let elapsed_ms = t_start.elapsed().as_millis() as u64;
    #[cfg(target_arch = "wasm32")]
    let elapsed_ms = 0u64;

    // Enforce strict budgets
    if total_instanced as usize > options.budget.max_instances_per_tile {
        return Err(StandError::Budget(format!(
            "tile {:?} instanced count {} exceeds --max-instances-per-tile {}",
            tile, total_instanced, options.budget.max_instances_per_tile
        )));
    }
    if total_lod0_tris > options.budget.max_triangles_lod0 {
        return Err(StandError::Budget(format!(
            "tile {:?} LOD0 triangles {} exceeds budget {}",
            tile, total_lod0_tris, options.budget.max_triangles_lod0
        )));
    }
    if total_lod1_tris > options.budget.max_triangles_lod1 {
        return Err(StandError::Budget(format!(
            "tile {:?} LOD1 triangles {} exceeds budget {}",
            tile, total_lod1_tris, options.budget.max_triangles_lod1
        )));
    }
    if total_lod2_tris > options.budget.max_triangles_lod2 {
        return Err(StandError::Budget(format!(
            "tile {:?} LOD2 triangles {} exceeds budget {}",
            tile, total_lod2_tris, options.budget.max_triangles_lod2
        )));
    }

    // Write deterministic stand_stats.json (generation_ms = 0 on disk so byte-for-byte
    // determinism holds across machines and shuffled runs; caller receives real elapsed_ms).
    let disk_stats = StandTileStats {
        tile,
        tile_size_m: tile_size,
        total_stems: total_instanced + total_merged,
        instanced_count: total_instanced,
        merged_count: total_merged,
        requested_total_cover,
        achieved_total_cover,
        triangles_lod0: total_lod0_tris,
        triangles_lod1: total_lod1_tris,
        triangles_lod2: total_lod2_tris,
        material_count: 3,
        foliage_mode: options.foliage_mode,
        generation_ms: 0,
        strata: strata_stats.clone(),
    };
    let stats_rel = format!("{tile_rel}/stand_stats.json");
    std::fs::write(
        out_dir.join(&stats_rel),
        serde_json::to_string_pretty(&disk_stats)
            .map_err(|e| StandError::Export(format!("stand_stats.json: {e}")))?,
    )?;

    let mut live_stats = disk_stats;
    live_stats.generation_ms = elapsed_ms;

    Ok((
        StandTileManifest {
            tile,
            origin_xz: [origin_x, origin_z],
            instances_json: inst_json_rel,
            instances_bin: inst_bin_rel,
            lod0_meshes: lod0_manifest,
            lod1_mesh: lod1_rel,
            lod2_mesh: lod2_rel,
            rasters: rasters_manifest,
            collision: col_rel,
            stats: stats_rel,
        },
        live_stats,
    ))
}

/// Fast in-memory volumetric stand preview for the interactive WASM workbench (`/demo/`).
/// Builds a self-contained tile (e.g. `48 m`) with optional synthetic winding sandy track
/// exclusion mask so users can inspect Stand LOD 0, LOD 1, LOD 2, and bush edges live in WebGL!
#[derive(Debug, Clone)]
pub struct StandPreviewData {
    pub name: String,
    pub biome: String,
    pub tile_size_m: f32,
    pub ground_mesh: Mesh,
    pub lod0_mesh: Mesh,
    pub lod1_mesh: Mesh,
    pub lod2_mesh: Mesh,
    pub stats: StandTileStats,
}

/// Build an in-memory volumetric stand preview using pre-loaded species profiles (no filesystem I/O,
/// 100% WASM-compatible!).
pub fn generate_stand_preview_in_memory(
    stand: &NatureStand,
    species_map: &BTreeMap<(String, Option<String>), Species>,
    tile_size_m: f32,
    include_demo_track: bool,
    ground_srgb: [f32; 3],
) -> Result<StandPreviewData, StandError> {
    #[cfg(not(target_arch = "wasm32"))]
    let start = std::time::Instant::now();
    // Build lightweight analytical variant profiles directly from parsed species (instant in WASM!)
    let mut variants = Vec::new();
    let v_count = stand.stand.variants_per_species_size.clamp(1, 3);
    for (s_idx, stratum) in stand.strata.iter().enumerate() {
        for sp_entry in &stratum.species {
            let key = (sp_entry.preset.clone(), sp_entry.state.clone());
            let Some(raw) = species_map.get(&key) else {
                return Err(StandError::Species(format!(
                    "missing preloaded species {:?} (state={:?})",
                    sp_entry.preset, sp_entry.state
                )));
            };
            let mut dead_modes = vec![stratum.is_dead];
            if !stratum.is_dead
                && stand.stand.condition.dieback > 0.0
                && sp_entry.is_dieback_susceptible()
            {
                dead_modes.push(true);
            }
            for is_dead in dead_modes {
                for (sc, _) in sp_entry.resolved_size_classes() {
                    let scaled = crate::stand::scale_species_for_size_class(
                        raw,
                        sc,
                        &stand.stand.condition,
                        is_dead,
                    );
                    let (fol_top, fol_bot) = species_foliage_colors(&scaled);
                    let bark_col = species_bark_color(&scaled);
                    let organ_srgb = scaled
                        .inflorescence
                        .as_ref()
                        .filter(|inf| inf.count >= 40)
                        .map(|inf| inf.color);

                    let is_ros =
                        scaled.generator.family == crate::species::GeneratorFamily::Rosette;
                    let height_m = if is_ros {
                        let b = scaled
                            .rosette
                            .as_ref()
                            .map(|r| r.blade_length)
                            .unwrap_or(1.0);
                        (scaled.trunk.height + b * 1.1).max(0.5)
                    } else {
                        (scaled.trunk.height * 1.18).max(0.6)
                    };
                    let crown_r = if is_ros {
                        scaled
                            .rosette
                            .as_ref()
                            .map(|r| r.blade_length * 0.95)
                            .unwrap_or(0.9)
                    } else {
                        (height_m * 0.34 * scaled.crown.width_ratio).clamp(0.4, 14.0)
                    };
                    let crown_base_y = if is_ros {
                        (scaled.trunk.height * 0.45).max(0.05)
                    } else {
                        (height_m * scaled.crown.offset.clamp(0.15, 0.72)).max(0.15)
                    };

                    for v_i in 0..v_count {
                        let v_idx = variants.len() as u32;
                        variants.push(StandVariantProfile {
                            variant_id: format!("v_{s_idx}_{v_idx}"),
                            variant_index: v_idx,
                            stratum: stratum.name.clone(),
                            preset: sp_entry.preset.clone(),
                            state: sp_entry.state.clone(),
                            size_class: sc,
                            is_dead,
                            seed: stand.stand.seed ^ (v_idx as u64 + 1) ^ (v_i as u64 * 997),
                            glb_file: String::new(),
                            height_m,
                            crown_radius_m: crown_r,
                            crown_base_y_m: crown_base_y,
                            crown_top_y_m: height_m,
                            trunk_radius_m: scaled.trunk.radius,
                            foliage_top_srgb: fol_top,
                            foliage_bot_srgb: fol_bot,
                            bark_srgb: bark_col,
                            organ_srgb,
                            is_rosette: is_ros,
                            lod0_triangles: 0,
                        });
                    }
                }
            }
        }
    }
    let library = StandLibrary {
        variants,
        build_ms: 0,
    };

    // Optional synthetic winding bush track + clearing for interactive edge demonstration
    let mut inputs = StandTileInputs::default();
    if include_demo_track {
        let res = 64u32;
        let mut excl = vec![0.0_f32; (res * res) as usize];
        for y in 0..res {
            let v = y as f32 / (res - 1) as f32;
            let track_u = 0.5 + 0.16 * (v * TAU * 0.85).sin();
            for x in 0..res {
                let u = x as f32 / (res - 1) as f32;
                let dist_track = (u - track_u).abs();
                let dist_clear = ((u - 0.25).powi(2) + (v - 0.62).powi(2)).sqrt();
                if dist_track < 0.055 || dist_clear < 0.12 {
                    excl[(y * res + x) as usize] = 1.0;
                }
            }
        }
        inputs.exclude = Some(StandRaster {
            width: res,
            height: res,
            data: excl,
        });
    }

    let eval = TileFieldEvaluator::new(0.0, 0.0, tile_size_m, &inputs);
    let mut stems = place_stand_stems(stand, &library, [0, 0], tile_size_m, &inputs, &eval);
    // In interactive stand preview, merge all stems into volumetric LOD0 so the whole bush is visible at once!
    for s in &mut stems {
        if s.in_tile {
            s.tier = StandTier::Merged;
        }
    }

    let budget = StandBudget {
        max_instances_per_tile: 0,
        max_triangles_lod0: 28_000,
        max_triangles_lod1: 4_000,
        max_triangles_lod2: 200,
        max_materials: 4,
    };

    let mut all_grid = SpatialGrid::new(8.0);
    let mut max_r = 2.0_f32;
    for (idx, s) in stems.iter().enumerate() {
        let r = s.crown_radius * 1.15;
        if r > max_r {
            max_r = r;
        }
        all_grid.insert(idx, s.wx, s.wz);
    }

    let lod0_strata = build_lod0_stratum_meshes(stand, &stems, 0.0, 0.0, &budget);
    let mut lod0_mesh = Mesh::new();
    for m in lod0_strata.values() {
        for sub in &m.submeshes {
            let mut sub_m = Mesh::new();
            let mut remap = vec![u32::MAX; m.vertices.len()];
            let slice =
                &m.indices[sub.index_start as usize..(sub.index_start + sub.index_count) as usize];
            for &idx in slice {
                let slot = &mut remap[idx as usize];
                if *slot == u32::MAX {
                    *slot = sub_m.vertices.len() as u32;
                    sub_m.vertices.push(m.vertices[idx as usize]);
                }
                sub_m.indices.push(*slot);
            }
            lod0_mesh.merge(&sub_m, sub.material);
        }
    }

    let mut lod1_mesh = build_canopy_surface_lod(
        32,
        0.0,
        0.0,
        tile_size_m,
        &stems,
        &all_grid,
        max_r,
        &eval,
        stand.stand.seed,
        budget.max_triangles_lod1,
        true,
    );
    let mut lod2_mesh = build_canopy_surface_lod(
        8,
        0.0,
        0.0,
        tile_size_m,
        &stems,
        &all_grid,
        max_r,
        &eval,
        stand.stand.seed,
        budget.max_triangles_lod2,
        false,
    );

    // Build ground tile mesh coloured with floor soil & subtle sandy track lightening
    let mut ground_mesh = Mesh::new();
    let gn = 24usize;
    let step = tile_size_m / gn as f32;
    for j in 0..=gn {
        for i in 0..=gn {
            let lx = i as f32 * step;
            let lz = j as f32 * step;
            let excl = eval.excluded_at(lx, lz);
            let n = world_fbm2(lx * 0.18, lz * 0.18, stand.stand.seed ^ 0xF100);
            let col = if excl {
                srgb3_to_linear([
                    (ground_srgb[0] * 1.18 + 0.06).clamp(0.0, 1.0),
                    (ground_srgb[1] * 1.15 + 0.05).clamp(0.0, 1.0),
                    (ground_srgb[2] * 1.10 + 0.04).clamp(0.0, 1.0),
                ])
            } else {
                let k = 0.84 + 0.28 * n;
                srgb3_to_linear([
                    (ground_srgb[0] * k).clamp(0.0, 1.0),
                    (ground_srgb[1] * k).clamp(0.0, 1.0),
                    (ground_srgb[2] * k).clamp(0.0, 1.0),
                ])
            };
            ground_mesh.vertices.push(Vertex {
                position: Vec3::new(
                    lx - tile_size_m * 0.5,
                    eval.height_at(lx, lz),
                    lz - tile_size_m * 0.5,
                ),
                normal: Vec3::Y,
                uv: Vec2::new(i as f32 / gn as f32, j as f32 / gn as f32),
                uv2: Vec2::ZERO,
                color: [col[0], col[1], col[2], 1.0].into(),
            });
        }
    }
    for j in 0..gn {
        for i in 0..gn {
            let v00 = (j * (gn + 1) + i) as u32;
            let v10 = v00 + 1;
            let v01 = ((j + 1) * (gn + 1) + i) as u32;
            let v11 = v01 + 1;
            ground_mesh
                .indices
                .extend_from_slice(&[v00, v01, v11, v00, v11, v10]);
        }
    }
    ground_mesh.submeshes.push(Submesh {
        index_start: 0,
        index_count: ground_mesh.indices.len() as u32,
        material: MaterialType::Bark,
    });

    // Center LOD 0/1/2 around origin to match ground_mesh & NaturePanel camera
    let half = tile_size_m * 0.5;
    for m in [&mut lod0_mesh, &mut lod1_mesh, &mut lod2_mesh] {
        for v in &mut m.vertices {
            v.position.x -= half;
            v.position.z -= half;
        }
    }

    let in_tile_count = stems.iter().filter(|s| s.in_tile).count() as u32;
    let tile_area = (tile_size_m * tile_size_m).max(1.0);
    let mut strata_stats = Vec::with_capacity(stand.strata.len());
    let mut req_prod = 1.0_f32;
    let mut total_crown_area = 0.0_f32;
    for (s_idx, stratum) in stand.strata.iter().enumerate() {
        let mut cnt = 0u32;
        let mut area = 0.0_f32;
        for s in &stems {
            if s.in_tile && s.stratum_idx as usize == s_idx {
                cnt += 1;
                area += std::f32::consts::PI * s.crown_radius * s.crown_radius;
            }
        }
        total_crown_area += area;
        req_prod *= 1.0 - stratum.cover.clamp(0.0, 0.99);
        strata_stats.push(StratumStats {
            name: stratum.name.clone(),
            stems: cnt,
            instanced: 0,
            merged: cnt,
            requested_cover: stratum.cover,
            achieved_cover: 1.0 - (-(area / tile_area)).exp(),
            lod0_triangles: lod0_strata
                .get(&stratum.name)
                .map(|m| (m.indices.len() / 3) as u32)
                .unwrap_or(0),
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    let generation_ms = start.elapsed().as_millis() as u64;
    #[cfg(target_arch = "wasm32")]
    let generation_ms = 0u64;

    let stats = StandTileStats {
        tile: [0, 0],
        tile_size_m,
        total_stems: in_tile_count,
        instanced_count: 0,
        merged_count: in_tile_count,
        requested_total_cover: 1.0 - req_prod,
        achieved_total_cover: 1.0 - (-(total_crown_area / tile_area)).exp(),
        triangles_lod0: (lod0_mesh.indices.len() / 3) as u32,
        triangles_lod1: (lod1_mesh.indices.len() / 3) as u32,
        triangles_lod2: (lod2_mesh.indices.len() / 3) as u32,
        material_count: 3,
        foliage_mode: FoliageRenderMode::Opaque,
        generation_ms,
        strata: strata_stats,
    };

    Ok(StandPreviewData {
        name: stand.asset.name.clone(),
        biome: stand.stand.biome.clone(),
        tile_size_m,
        ground_mesh,
        lod0_mesh,
        lod1_mesh,
        lod2_mesh,
        stats,
    })
}
