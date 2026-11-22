//! Procedural texture-map generation.
//!
//! Midori generates deterministic, license-clean maps in-engine so exported
//! trees carry real materials without external art: a tileable bark
//! albedo + tangent-space normal pair, and a leaf albedo+alpha card stamped
//! from the same SDF silhouettes the polygon leaf geometry uses.
//!
//! Host-provided maps replace any slot without touching geometry or export
//! code — species `[textures]` file paths today, studio-bus CAS artifacts
//! once Midori has a project authority. Generated maps are also what the
//! crown-impostor baker samples, so far LODs share the same materials.

use crate::math::lerp;
use crate::rng::Rng;
use crate::species::{BarkStyle, LeafCardLayout, Species};
use crate::{leaves::leaf_sdf, species::LeafShape};
use glam::{Vec2, Vec3};

/// Minimum generated-map resolution.
pub const MIN_TEXTURE_RESOLUTION: u32 = 16;
/// Maximum generated-map resolution.
pub const MAX_TEXTURE_RESOLUTION: u32 = 4096;

/// An RGBA8 texture, row-major, four bytes per texel.
///
/// Kept as a plain buffer so `image` stays an implementation detail and the
/// type is cheap to clone across the engine/FFI boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaTexture {
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// RGBA8 pixels, `width * height * 4` bytes.
    pub pixels: Vec<u8>,
}

/// Errors from texture encode/decode and slot resolution.
#[derive(Debug)]
pub enum TextureError {
    /// An image failed to decode.
    Decode(String),
    /// An image failed to encode.
    Encode(String),
    /// A slot path could not be read.
    Io(std::io::Error),
    /// A slot path was set but empty or non-UTF-8.
    BadPath(String),
}

impl From<std::io::Error> for TextureError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl std::fmt::Display for TextureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decode(e) => write!(f, "image decode error: {e}"),
            Self::Encode(e) => write!(f, "image encode error: {e}"),
            Self::Io(e) => write!(f, "texture IO error: {e}"),
            Self::BadPath(p) => write!(f, "invalid texture path: {p}"),
        }
    }
}

impl std::error::Error for TextureError {}

impl RgbaTexture {
    /// A zeroed texture.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height * 4) as usize],
        }
    }

    /// Texel at integer coordinates (clamped).
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        let x = x.min(self.width.saturating_sub(1));
        let y = y.min(self.height.saturating_sub(1));
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }

    /// Write a texel.
    pub fn set(&mut self, x: u32, y: u32, c: [u8; 4]) {
        let i = ((y * self.width + x) * 4) as usize;
        self.pixels[i..i + 4].copy_from_slice(&c);
    }

    /// Bilinear sample with wrap addressing. `u`/`v` are texture space
    /// (v = 0 at the top row, matching glTF UV origin).
    pub fn sample_wrap(&self, u: f32, v: f32) -> [f32; 4] {
        let w = self.width as f32;
        let h = self.height as f32;
        let x = (u.rem_euclid(1.0)) * w - 0.5;
        let y = (v.rem_euclid(1.0)) * h - 0.5;
        let x0 = x.floor();
        let y0 = y.floor();
        let fx = x - x0;
        let fy = y - y0;
        let wrap_x = |x: f32| x.rem_euclid(w) as u32;
        let wrap_y = |y: f32| y.rem_euclid(h) as u32;
        let c00 = self.get(wrap_x(x0), wrap_y(y0));
        let c10 = self.get(wrap_x(x0 + 1.0), wrap_y(y0));
        let c01 = self.get(wrap_x(x0), wrap_y(y0 + 1.0));
        let c11 = self.get(wrap_x(x0 + 1.0), wrap_y(y0 + 1.0));
        let mut out = [0.0f32; 4];
        for ch in 0..4 {
            let a = lerp(c00[ch] as f32, c10[ch] as f32, fx);
            let b = lerp(c01[ch] as f32, c11[ch] as f32, fx);
            out[ch] = lerp(a, b, fy) / 255.0;
        }
        out
    }

    /// Bilinear sample with clamp-to-edge addressing (for non-tiling cards/atlases).
    pub fn sample_clamp(&self, u: f32, v: f32) -> [f32; 4] {
        let w = self.width as f32;
        let h = self.height as f32;
        let x = u.clamp(0.0, 1.0) * w - 0.5;
        let y = v.clamp(0.0, 1.0) * h - 0.5;
        let x0 = x.floor();
        let y0 = y.floor();
        let fx = x - x0;
        let fy = y - y0;
        let clamp_x = |x: f32| x.clamp(0.0, (w - 1.0).max(0.0)) as u32;
        let clamp_y = |y: f32| y.clamp(0.0, (h - 1.0).max(0.0)) as u32;
        let c00 = self.get(clamp_x(x0), clamp_y(y0));
        let c10 = self.get(clamp_x(x0 + 1.0), clamp_y(y0));
        let c01 = self.get(clamp_x(x0), clamp_y(y0 + 1.0));
        let c11 = self.get(clamp_x(x0 + 1.0), clamp_y(y0 + 1.0));
        let mut out = [0.0f32; 4];
        for ch in 0..4 {
            let a = lerp(c00[ch] as f32, c10[ch] as f32, fx);
            let b = lerp(c01[ch] as f32, c11[ch] as f32, fx);
            out[ch] = lerp(a, b, fy) / 255.0;
        }
        out
    }

    /// Bleed RGB colours from opaque texels outward into transparent texels while
    /// keeping alpha untouched, eliminating dark mipmap/cutout fringes.
    pub fn dilate_alpha_rgb(&mut self, passes: u32) {
        if self.width == 0 || self.height == 0 {
            return;
        }
        let w = self.width as i32;
        let h = self.height as i32;
        let mut seeded = vec![false; (self.width * self.height) as usize];
        for (i, px) in self.pixels.as_chunks::<4>().0.iter().enumerate() {
            seeded[i] = px[3] >= 24;
        }
        for _ in 0..passes {
            let prev = self.pixels.clone();
            let prev_seeded = seeded.clone();
            for y in 0..h {
                for x in 0..w {
                    let idx = (y * w + x) as usize;
                    if prev_seeded[idx] {
                        continue;
                    }
                    let mut r_sum = 0.0f32;
                    let mut g_sum = 0.0f32;
                    let mut b_sum = 0.0f32;
                    let mut w_sum = 0.0f32;
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            if dx == 0 && dy == 0 {
                                continue;
                            }
                            let nx = x + dx;
                            let ny = y + dy;
                            if nx < 0 || ny < 0 || nx >= w || ny >= h {
                                continue;
                            }
                            let nidx = (ny * w + nx) as usize;
                            if !prev_seeded[nidx] {
                                continue;
                            }
                            let weight = if dx == 0 || dy == 0 { 1.0 } else { 0.7 };
                            let p = &prev[nidx * 4..nidx * 4 + 4];
                            r_sum += p[0] as f32 * weight;
                            g_sum += p[1] as f32 * weight;
                            b_sum += p[2] as f32 * weight;
                            w_sum += weight;
                        }
                    }
                    if w_sum > 0.0 {
                        self.pixels[idx * 4] = (r_sum / w_sum).round() as u8;
                        self.pixels[idx * 4 + 1] = (g_sum / w_sum).round() as u8;
                        self.pixels[idx * 4 + 2] = (b_sum / w_sum).round() as u8;
                        seeded[idx] = true;
                    }
                }
            }
        }
    }

    /// Fraction of texels with alpha >= 128.
    pub fn alpha_coverage(&self) -> f32 {
        if self.pixels.is_empty() {
            return 0.0;
        }
        let covered = self
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|px| px[3] >= 128)
            .count();
        covered as f32 / (self.pixels.len() / 4) as f32
    }

    /// Encode as PNG bytes (deterministic for identical pixels).
    pub fn to_png(&self) -> Result<Vec<u8>, TextureError> {
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        use image::ImageEncoder;
        encoder
            .write_image(
                &self.pixels,
                self.width,
                self.height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| TextureError::Encode(e.to_string()))?;
        Ok(buf)
    }

    /// Decode any image format `image` understands into RGBA8.
    pub fn from_image_bytes(bytes: &[u8]) -> Result<Self, TextureError> {
        let img = image::load_from_memory(bytes)
            .map_err(|e| TextureError::Decode(e.to_string()))?
            .to_rgba8();
        let (width, height) = img.dimensions();
        Ok(Self {
            width,
            height,
            pixels: img.into_raw(),
        })
    }
}

/// The material maps a species ships with.
#[derive(Debug, Clone)]
pub struct TextureSet {
    /// Tiled bark albedo (opaque).
    pub bark_albedo: RgbaTexture,
    /// Tiled bark normal map, OpenGL +Y (glTF) convention.
    pub bark_normal: RgbaTexture,
    /// Leaf card albedo+alpha.
    pub leaf_card: RgbaTexture,
}

impl TextureSet {
    /// Generate all three maps procedurally from the species' `[textures]`
    /// parameters.
    pub fn generate(species: &Species) -> Self {
        let params = &species.textures;
        let res = params
            .resolution
            .clamp(MIN_TEXTURE_RESOLUTION, MAX_TEXTURE_RESOLUTION);
        let seed = species_texture_seed(species);
        let is_palm_ringed = params.bark_style == BarkStyle::Smooth
            && params.leaf_card == LeafCardLayout::Frond
            && species.branches.level2.is_none();
        let is_arid = species.generator.family == crate::species::GeneratorFamily::Dichotomous
            || species.species.biome.eq_ignore_ascii_case("desert")
            || is_palm_ringed;
        let height = if is_palm_ringed {
            palm_bark_heightfield(seed, res)
        } else {
            bark_heightfield(params.bark_style, seed, res)
        };
        let mut bark_albedo = bark_albedo_from_height(
            &height,
            res,
            params.bark_style,
            is_arid,
            is_palm_ringed,
            params.bark_color,
            seed,
        );
        // Optional patch-colour / kino layers (no-op unless the species sets them).
        crate::bark_layers::apply_layers(&mut bark_albedo, &height, params, seed);
        let leaf_card = if species.uses_foliage_atlas() {
            crate::botanical_cards::foliage_atlas(species, seed, res)
        } else {
            leaf_card(
                params.leaf_shape,
                params.leaf_card,
                params.leaf_color,
                seed,
                res,
            )
        };
        Self {
            bark_albedo,
            bark_normal: normal_from_height(&height, res),
            leaf_card,
        }
    }

    /// Resolve the effective map set: file slots load relative to `dir`
    /// (typically the species document's directory); empty slots generate
    /// procedurally. An explicit slot that fails to load is an error — a
    /// silent procedural fallback would hide a broken art reference.
    pub fn resolve(species: &Species, dir: &std::path::Path) -> Result<Self, TextureError> {
        let params = &species.textures;
        let mut set = Self::generate(species);
        let _ = params; // used above via generate; slots below
        for (slot, target) in [
            (&species.textures.bark_albedo, Slot::BarkAlbedo),
            (&species.textures.bark_normal, Slot::BarkNormal),
            (&species.textures.leaf_albedo_alpha, Slot::LeafCard),
        ] {
            if slot.is_empty() {
                continue;
            }
            let path = dir.join(slot);
            let bytes = std::fs::read(&path)?;
            let tex = RgbaTexture::from_image_bytes(&bytes)
                .map_err(|e| TextureError::Decode(format!("{}: {}", path.display(), e)))?;
            match target {
                Slot::BarkAlbedo => set.bark_albedo = tex,
                Slot::BarkNormal => set.bark_normal = tex,
                Slot::LeafCard => set.leaf_card = tex,
            }
        }
        Ok(set)
    }
}

enum Slot {
    BarkAlbedo,
    BarkNormal,
    LeafCard,
}

/// Deterministic per-species texture seed: `textures.seed` when set, else a
/// stable hash of the species name so each species gets distinct maps.
pub fn species_texture_seed(species: &Species) -> u64 {
    if let Some(seed) = species.textures.seed {
        return seed;
    }
    // FNV-1a over the species name.
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in species.species.name.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

// ---------------------------------------------------------------------------
// Periodic noise (tileable, position-independent, no std::hash)
// ---------------------------------------------------------------------------

/// Hash a 2D integer lattice cell to [0, 1) using fast 32-bit Murmur-style avalanche
/// (compiles to single-cycle native multiplies in WASM32, deterministic across platforms).
#[inline]
fn lattice(seed: u64, ix: i32, iy: i32) -> f32 {
    let s0 = (seed as u32) ^ ((seed >> 32) as u32).wrapping_mul(0x85EB_CA6B);
    let mut h = s0
        .wrapping_add((ix as u32).wrapping_mul(0x9E37_79B1))
        .wrapping_add((iy as u32).wrapping_mul(0xC2B2_AE3D));
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    (h >> 8) as f32 * (1.0 / 16_777_216.0)
}

#[inline]
fn wrap_period(i: i32, p: i32) -> i32 {
    if p > 0 && (p & (p - 1)) == 0 {
        i & (p - 1)
    } else {
        i.rem_euclid(p)
    }
}

#[inline]
pub(crate) fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Value noise on a wrapping lattice. Sampling `u*px`/`v*py` over u,v in
/// [0,1) produces a texture that tiles seamlessly.
#[inline]
pub(crate) fn value_noise(seed: u64, x: f32, y: f32, px: i32, py: i32) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = smooth(x - ix as f32);
    let fy = smooth(y - iy as f32);
    let x0 = wrap_period(ix, px);
    let x1 = wrap_period(ix + 1, px);
    let y0 = wrap_period(iy, py);
    let y1 = wrap_period(iy + 1, py);
    let v00 = lattice(seed, x0, y0);
    let v10 = lattice(seed, x1, y0);
    let v01 = lattice(seed, x0, y1);
    let v11 = lattice(seed, x1, y1);
    lerp(lerp(v00, v10, fx), lerp(v01, v11, fx), fy)
}

/// Fractional Brownian motion; periods double with frequency so the result
/// stays periodic over [0, 1).
pub(crate) fn fbm(seed: u64, x: f32, y: f32, px: i32, py: i32, octaves: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut norm = 0.0;
    let mut f: i32 = 1;
    for octave in 0..octaves {
        sum += amp
            * value_noise(
                seed.wrapping_add((octave as u64).wrapping_mul(0xA24B_AED4_963E_E407)),
                x * f as f32,
                y * f as f32,
                px * f,
                py * f,
            );
        norm += amp;
        amp *= 0.5;
        f *= 2;
    }
    sum / norm
}

/// Periodic Worley F1 & F2 distances (scaled by `px`/`py` cells per side).
/// Returns `(f1_dist, f2_dist, nearest_cell_hash)` so `f2 - f1` yields crisp
/// polygonal bark-plate fissures rather than circular polka-dot cones.
pub(crate) fn voronoi_f1_f2(seed: u64, x: f32, y: f32, px: i32, py: i32) -> (f32, f32, f32) {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x - ix as f32;
    let fy = y - iy as f32;
    let mut best1 = f32::MAX;
    let mut best2 = f32::MAX;
    let mut cell = 0.0;
    for dy in -1..=1_i32 {
        let wy = wrap_period(iy + dy, py);
        for dx in -1..=1_i32 {
            let wx = wrap_period(ix + dx, px);
            let jx = 0.14 + 0.72 * lattice(seed, wx, wy);
            let jy = 0.14 + 0.72 * lattice(seed ^ 0x5EED_5EED, wx, wy);
            let ox = dx as f32 + jx - fx;
            let oy = dy as f32 + jy - fy;
            let d2 = ox * ox + oy * oy;
            if d2 < best1 {
                best2 = best1;
                best1 = d2;
                cell = lattice(seed ^ 0xCE11_5EED, wx, wy);
            } else if d2 < best2 {
                best2 = d2;
            }
        }
    }
    (best1.sqrt(), best2.sqrt(), cell)
}

// ---------------------------------------------------------------------------
// Bark
// ---------------------------------------------------------------------------

/// Tileable bark heightfield in [0,1], evaluated per texel.
fn bark_height_at(style: BarkStyle, u: f32, v: f32, seed: u64) -> f32 {
    use std::f32::consts::TAU;
    let u = u.rem_euclid(1.0);
    let v = v.rem_euclid(1.0);
    match style {
        BarkStyle::Furrowed => {
            // High-definition diamond-interlocking / anastomosing rough cork bark (Quercus / Ginkgo / Salix / Yucca):
            // 26 primary braided cork ridges (~7cm wide on mature trunks) separated by deep V-shaped diamond canyons.
            let warp_macro = (fbm(seed ^ 0x11, u * 6.0, v * 3.0, 6, 3, 4) - 0.5) * 0.085;
            let warp_med = (fbm(seed ^ 0x12, u * 16.0, v * 6.0, 16, 6, 4) - 0.5) * 0.038;
            // High-frequency fibrous edge splintering up to 256px
            let tear_u = (fbm(seed ^ 0x13, u * 80.0, v * 32.0, 80, 32, 3) - 0.5) * 0.016
                + (value_noise(seed ^ 0x14, u * 256.0, v * 96.0, 256, 96) - 0.5) * 0.007;

            // Occasional callused branch-scar eye deflecting bark ridges organically
            let (knot_d, _, knot_c) = voronoi_f1_f2(seed ^ 0x15, u * 5.0, v * 5.0, 5, 5);
            let knot_push = if knot_c > 0.86 && knot_d < 0.30 {
                ((0.30 - knot_d) / 0.30).powi(2) * 0.038
            } else {
                0.0
            };

            let uw1 = u + warp_macro + warp_med + tear_u + knot_push;
            let uw2 = u + warp_macro * 0.65 - warp_med * 0.75 + tear_u - knot_push;

            // Two opposing diagonal-braided 26-strand ridged-cork networks (exact integer 26x4 periodic shear!)
            // Combining lerp with c1.min(c2) carves deep anastomosing V-canyons (h -> 0) between rough cork plateaus.
            let c1 = ((uw1 * 26.0 + v * 4.0) * TAU).cos().abs();
            let c2 = ((uw2 * 26.0 - v * 4.0) * TAU).cos().abs();
            let braid_mix = fbm(seed ^ 0x17, u * 8.0, v * 4.0, 8, 4, 3);
            let diamond_net = lerp(c1, c2, braid_mix) * 0.56 + c1.min(c2) * 0.44;

            let mut h = diamond_net.clamp(0.0, 1.0).powf(0.74);

            // Secondary 52-strand longitudinal splits dividing wide cork ridges into craggy bark strips
            let sub_split = (((uw1 * 52.0
                + (fbm(seed ^ 0x18, u * 20.0, v * 8.0, 20, 8, 2) - 0.5) * 0.42)
                * TAU)
                .cos()
                .abs())
            .powf(0.50);
            h *= 0.80 + 0.20 * sub_split;

            // Rough, bumpy cork cragginess + ultra-crisp 192/384px longitudinal wood-fiber striae
            let cork_crag = fbm(seed ^ 0x19, u * 96.0, v * 48.0, 96, 48, 3);
            let fiber_a = fbm(
                seed ^ 0x1A,
                (u + warp_med * 0.4) * 192.0,
                v * 32.0,
                192,
                32,
                3,
            );
            let groove_a = (1.0 - (2.0 * fiber_a - 1.0).abs()).powf(1.55);
            let fiber_b = value_noise(seed ^ 0x1B, u * 384.0, v * 96.0, 384, 96);
            h = h * (0.74 + 0.15 * cork_crag + 0.17 * groove_a) + (fiber_b - 0.5) * 0.068;

            // Subtle short horizontal bark-stress cracks across individual high cork ridges
            if h > 0.46 {
                let check_n = fbm(
                    seed ^ 0x1C,
                    (u + warp_macro * 0.5) * 32.0,
                    v * 48.0,
                    32,
                    48,
                    3,
                );
                let check_line = (check_n - 0.50).abs();
                if check_line < 0.026 {
                    let cut = (1.0 - check_line / 0.026).powi(2);
                    h *= 1.0 - 0.34 * cut;
                }
            }

            if knot_c > 0.86 && knot_d < 0.24 {
                let r_norm = knot_d / 0.24;
                let callus = (-(r_norm - 0.60).powi(2) / 0.045).exp() * 0.18;
                let pit = (1.0 - (r_norm / 0.40).clamp(0.0, 1.0)).powi(2) * 0.38;
                h = h + callus - pit;
            }

            h.clamp(0.0, 1.0)
        }
        BarkStyle::Plated => {
            // High-definition Akamatsu / Ponderosa exfoliating puzzle-flake bark:
            // 24x12 primary elongated scales + 48x24 sub-flakes with crisp ragged papery edges!
            let macro_u = (fbm(seed ^ 0x21, u * 8.0, v * 6.0, 8, 6, 3) - 0.5) * 0.042;
            let macro_v = (fbm(seed ^ 0x22, u * 8.0, v * 6.0, 8, 6, 2) - 0.5) * 0.028;
            let jag_u = (fbm(seed ^ 0x23, u * 64.0, v * 48.0, 64, 48, 3) - 0.5) * 0.018
                + (value_noise(seed ^ 0x24, u * 256.0, v * 192.0, 256, 192) - 0.5) * 0.007;
            let jag_v = (fbm(seed ^ 0x25, u * 64.0, v * 48.0, 64, 48, 2) - 0.5) * 0.014;

            let uw = u + macro_u + jag_u;
            let vw = v + macro_v + jag_v;

            let (f1_a, f2_a, cell_a) = voronoi_f1_f2(seed ^ 0x26, uw * 24.0, vw * 11.0, 24, 11);
            let (f1_b, f2_b, cell_b) = voronoi_f1_f2(
                seed ^ 0x27,
                (u - macro_u * 0.4 + jag_u * 1.1) * 48.0,
                (v + macro_v * 0.5 + jag_v) * 22.0,
                48,
                22,
            );

            let rift_mod = 0.075 + 0.115 * fbm(seed ^ 0x28, u * 16.0, v * 4.0, 16, 4, 3);
            let border_a = smooth(((f2_a - f1_a) / rift_mod).clamp(0.0, 1.0));
            let border_b = smooth(((f2_b - f1_b) / 0.105).clamp(0.0, 1.0));

            // Subtle papery exfoliating laminae + rich high-frequency flaky grain inside every scale
            let lamina_field = fbm(seed ^ 0x29, uw * 36.0, vw * 18.0, 36, 18, 4) * 0.68
                + (1.0 - f1_a.clamp(0.0, 0.85)) * 0.38;
            let laminae_steps = (lamina_field * 4.0).floor() / 4.0;
            let plate_tilt = 0.62 + 0.28 * cell_a + 0.12 * cell_b;

            let cork_pit = value_noise(seed ^ 0x2A, u * 256.0, v * 128.0, 256, 128);
            let fine_grain = fbm(seed ^ 0x2B, u * 192.0, v * 64.0, 192, 64, 3);

            let interior = lerp(lamina_field, laminae_steps, 0.24);
            let mut h = border_a * plate_tilt * (0.72 + 0.28 * border_b) * (0.60 + 0.44 * interior);
            h = h * (0.82 + 0.22 * fine_grain) + (cork_pit - 0.5) * 0.068;
            h.clamp(0.0, 1.0)
        }
        BarkStyle::Smooth => {
            // Silky Momiji Japanese Maple / Beech living bark (ZERO synthetic horizontal lathe rings!):
            // Subtle muscular sapwood flutes, meandering vertical snake-bark striations, scattered
            // horizontal cork lenticel dashes with callus lips, and healed twig-scar eyes.
            let muscle_flute = fbm(seed ^ 0x31, u * 6.0, v * 2.0, 6, 2, 3);
            let patch_undul = fbm(seed ^ 0x32, u * 14.0, v * 8.0, 14, 8, 3);
            let warp_u = (fbm(seed ^ 0x33, u * 8.0, v * 4.0, 8, 4, 3) - 0.5) * 0.035;

            // Delicate meandering vertical snake-bark striations & fine epidermis grain
            let stria_a = fbm(seed ^ 0x34, (u + warp_u) * 80.0, v * 10.0, 80, 10, 3);
            let vein_a = (1.0 - (2.0 * stria_a - 1.0).abs()).powf(2.0);
            let stria_b = value_noise(seed ^ 0x35, (u - warp_u * 0.5) * 192.0, v * 28.0, 192, 28);
            let epidermis = value_noise(seed ^ 0x36, u * 256.0, v * 128.0, 256, 128);

            // Scattered horizontal cork lenticel dashes (aperiodic 2-scale distribution)
            let (lf1, _, lcell1) = voronoi_f1_f2(seed ^ 0x37, u * 28.0, v * 144.0, 28, 144);
            let lenticel_major = if lcell1 > 0.83 && lf1 < 0.34 {
                let r = lf1 / 0.34;
                let rim = (-(r - 0.55).powi(2) / 0.08).exp() * 0.12;
                let cleft = (1.0 - (r / 0.38).clamp(0.0, 1.0)) * 0.14;
                rim - cleft
            } else {
                0.0
            };
            let (lf2, _, lcell2) = voronoi_f1_f2(seed ^ 0x38, u * 56.0, v * 256.0, 56, 256);
            let lenticel_minor = if lcell2 > 0.80 && lf2 < 0.30 {
                (0.30 - lf2) * 0.32
            } else {
                0.0
            };

            // Occasional healed self-pruned twig collar ("bark eye" chevron)
            let (kf1, _, kcell) = voronoi_f1_f2(seed ^ 0x39, u * 4.0, v * 8.0, 4, 8);
            let eye_scar = if kcell > 0.87 && kf1 < 0.30 {
                let kr = kf1 / 0.30;
                (-(kr - 0.62).powi(2) / 0.05).exp() * 0.14
                    - (1.0 - (kr / 0.35).clamp(0.0, 1.0)) * 0.18
            } else {
                0.0
            };

            let h = 0.44
                + 0.15 * (muscle_flute - 0.5)
                + 0.11 * (patch_undul - 0.5)
                + 0.16 * vein_a
                + 0.07 * (stria_b - 0.5)
                + 0.06 * (epidermis - 0.5)
                + lenticel_major
                + lenticel_minor
                + eye_scar;
            h.clamp(0.0, 1.0)
        }
        BarkStyle::Stringy | BarkStyle::Tessellated | BarkStyle::Patchy | BarkStyle::Papery => {
            crate::bark_layers::height_at(style, u, v, seed)
        }
    }
}

/// Dedicated tropical ringed trunk heightfield for pinnate palms (irregular wavy frond-base
/// annular scars + frayed vertical vascular coir fibers).
fn palm_bark_heightfield(seed: u64, res: u32) -> Vec<f32> {
    let mut h = vec![0.0; (res * res) as usize];
    let inv = 1.0 / res as f32;
    for y in 0..res {
        let v = (y as f32 + 0.5) * inv;
        let row = (y * res) as usize;
        for x in 0..res {
            let u = (x as f32 + 0.5) * inv;
            let ring_warp = (fbm(seed ^ 0x41, u * 8.0, v * 8.0, 8, 8, 3) - 0.5) * 0.42
                + (value_noise(seed ^ 0x42, u * 32.0, v * 16.0, 32, 16) - 0.5) * 0.11;
            let phase = (v * 14.0 + ring_warp).rem_euclid(1.0);
            let ring_amp = 0.55 + 0.45 * fbm(seed ^ 0x43, u * 4.0, v * 14.0, 4, 14, 2);
            let scar_cleft = (-(phase - 0.18).powi(2) / 0.0045).exp() * ring_amp;
            let scar_lip = (-(phase - 0.28).powi(2) / 0.009).exp() * ring_amp;

            // Dense frayed vertical coconut coir fibers & weathered pitting
            let coir = fbm(seed ^ 0x44, u * 192.0, v * 24.0, 192, 24, 3);
            let fiber = (1.0 - (2.0 * coir - 1.0).abs()).powf(1.7);
            let pit = value_noise(seed ^ 0x45, u * 128.0, v * 128.0, 128, 128);
            let undul = fbm(seed ^ 0x46, u * 8.0, v * 6.0, 8, 6, 3);

            let val = 0.46 + 0.14 * (undul - 0.5) - 0.16 * scar_cleft
                + 0.13 * scar_lip
                + 0.18 * fiber
                + 0.08 * (pit - 0.5);
            h[row + x as usize] = val.clamp(0.0, 1.0);
        }
    }
    h
}

/// Evaluate the bark heightfield over the whole texture.
fn bark_heightfield(style: BarkStyle, seed: u64, res: u32) -> Vec<f32> {
    let mut h = vec![0.0; (res * res) as usize];
    let inv = 1.0 / res as f32;
    for y in 0..res {
        let v = (y as f32 + 0.5) * inv;
        let row = (y * res) as usize;
        for x in 0..res {
            let u = (x as f32 + 0.5) * inv;
            h[row + x as usize] = bark_height_at(style, u, v, seed);
        }
    }
    h
}

/// Albedo: multi-zone botanical bark synthesis with deep cavity AO, warm active-crack phloem,
/// independent 2D weathering/rain-streak variation, longitudinal wood-fiber streaks, cork pores,
/// and scalloped crustose lichen + bryophyte dappling on temperate trees.
#[allow(clippy::too_many_arguments)]
fn bark_albedo_from_height(
    height: &[f32],
    res: u32,
    style: BarkStyle,
    is_arid: bool,
    is_palm_ringed: bool,
    color: Option<[f32; 3]>,
    seed: u64,
) -> RgbaTexture {
    let base = color.unwrap_or([0.42, 0.34, 0.26]);
    let crevice = match style {
        BarkStyle::Furrowed | BarkStyle::Stringy | BarkStyle::Tessellated => {
            [base[0] * 0.16, base[1] * 0.15, base[2] * 0.14]
        }
        _ => [base[0] * 0.24, base[1] * 0.22, base[2] * 0.20],
    };
    let phloem = match style {
        BarkStyle::Plated => [
            (base[0] * 0.98 + 0.03).min(1.0),
            (base[1] * 0.72 + 0.01).min(1.0),
            (base[2] * 0.54).min(1.0),
        ],
        BarkStyle::Smooth | BarkStyle::Patchy | BarkStyle::Papery => [
            (base[0] * 0.76).min(1.0),
            (base[1] * 0.74).min(1.0),
            (base[2] * 0.70).min(1.0),
        ],
        BarkStyle::Furrowed | BarkStyle::Stringy | BarkStyle::Tessellated => [
            (base[0] * 0.76 + 0.02).min(1.0),
            (base[1] * 0.66 + 0.01).min(1.0),
            (base[2] * 0.54).min(1.0),
        ],
    };
    let mid_warm = [
        (base[0] * 1.02 + 0.01).min(1.0),
        (base[1] * 0.99 + 0.01).min(1.0),
        (base[2] * 0.94).min(1.0),
    ];
    let mid_cool = [
        (base[0] * 0.93).min(1.0),
        (base[1] * 0.95).min(1.0),
        (base[2] * 0.97 + 0.015).min(1.0),
    ];
    let crown_ash = match style {
        BarkStyle::Plated => [
            (base[0] * 1.10 + 0.03).min(1.0),
            (base[1] * 1.08 + 0.035).min(1.0),
            (base[2] * 1.08 + 0.04).min(1.0),
        ],
        _ => [
            (base[0] * 1.18 + 0.035).min(1.0),
            (base[1] * 1.20 + 0.04).min(1.0),
            (base[2] * 1.20 + 0.045).min(1.0),
        ],
    };

    let mut tex = RgbaTexture::new(res, res);
    let inv = 1.0 / res as f32;
    for y in 0..res {
        let v = (y as f32 + 0.5) * inv;
        let row = (y * res) as usize;
        for x in 0..res {
            let i = row + x as usize;
            let u = (x as f32 + 0.5) * inv;

            // Independent 2D weathering zones, vertical rain-streak runoff, and fine wood-fiber grain
            let weather_zone = fbm(seed ^ 0xA1, u * 6.0, v * 4.0, 6, 4, 3);
            let rain_streak = fbm(seed ^ 0xA2, u * 18.0, v * 2.0, 18, 2, 3);
            let fiber_streak = fbm(seed ^ 0xA3, u * 160.0, v * 20.0, 160, 20, 3);
            let cork_pore = value_noise(seed ^ 0xA4, u * 256.0, v * 128.0, 256, 128);

            let h = height[i];
            let mid = [
                lerp(mid_warm[0], mid_cool[0], weather_zone),
                lerp(mid_warm[1], mid_cool[1], weather_zone),
                lerp(mid_warm[2], mid_cool[2], weather_zone),
            ];

            let t = (h * 0.84 + fiber_streak * 0.11 + cork_pore * 0.05).clamp(0.0, 1.0);
            let mut rgb = if t < 0.24 {
                let k = t / 0.24;
                [
                    lerp(crevice[0], phloem[0], k),
                    lerp(crevice[1], phloem[1], k),
                    lerp(crevice[2], phloem[2], k),
                ]
            } else if t < 0.64 {
                let k = (t - 0.24) / 0.40;
                [
                    lerp(phloem[0], mid[0], k),
                    lerp(phloem[1], mid[1], k),
                    lerp(phloem[2], mid[2], k),
                ]
            } else {
                let k = ((t - 0.64) / 0.36) * (0.55 + 0.45 * weather_zone);
                [
                    lerp(mid[0], crown_ash[0], k),
                    lerp(mid[1], crown_ash[1], k),
                    lerp(mid[2], crown_ash[2], k),
                ]
            };

            // Vertical dark moisture/tannin runoff streaks & fine wood-fiber contrast
            let streak_mul = 0.86 + 0.24 * rain_streak;
            let grain_mul = 0.90 + 0.20 * fiber_streak - (0.5 - cork_pore).max(0.0) * 0.12;
            rgb[0] *= streak_mul * grain_mul;
            rgb[1] *= streak_mul * grain_mul;
            rgb[2] *= streak_mul * grain_mul;

            // Russet cork lenticel pigment on smooth bark
            if style == BarkStyle::Smooth && !is_palm_ringed && cork_pore > 0.82 && h < 0.46 {
                rgb[0] *= 0.68;
                rgb[1] *= 0.60;
                rgb[2] *= 0.54;
            }

            // Temperate powdery micro-lichen stippling & damp crevice bryophyte dappling (zero round polka dots!)
            if !is_arid {
                let lic_macro = fbm(seed ^ 0xA5, u * 10.0, v * 10.0, 10, 10, 4);
                let lic_micro = fbm(seed ^ 0xA6, u * 96.0, v * 96.0, 96, 96, 3);
                let lic_speck = value_noise(seed ^ 0xA7, u * 256.0, v * 256.0, 256, 256);
                let lic_mask = ((lic_macro - 0.62) / 0.30).clamp(0.0, 1.0)
                    * ((lic_micro - 0.46) / 0.42).clamp(0.0, 1.0)
                    * (0.45 + 0.55 * lic_speck);
                if lic_mask > 0.02 && h > 0.48 {
                    let blend = (lic_mask * 0.46).clamp(0.0, 0.34);
                    rgb[0] = lerp(rgb[0], 0.48, blend);
                    rgb[1] = lerp(rgb[1], 0.53, blend);
                    rgb[2] = lerp(rgb[2], 0.45, blend);
                } else if style != BarkStyle::Plated && weather_zone > 0.64 && rain_streak < 0.42 {
                    let moss_t = (((weather_zone - 0.64) / 0.36)
                        * ((0.42 - rain_streak) / 0.42)
                        * cork_pore
                        * 0.50)
                        .clamp(0.0, 0.30);
                    rgb[0] = lerp(rgb[0], 0.18, moss_t);
                    rgb[1] = lerp(rgb[1], 0.27, moss_t);
                    rgb[2] = lerp(rgb[2], 0.11, moss_t);
                }
            }

            let px = [
                (rgb[0].clamp(0.0, 1.0) * 255.0) as u8,
                (rgb[1].clamp(0.0, 1.0) * 255.0) as u8,
                (rgb[2].clamp(0.0, 1.0) * 255.0) as u8,
                255,
            ];
            tex.pixels[i * 4..i * 4 + 4].copy_from_slice(&px);
        }
    }
    tex
}

/// Tangent-space normal map from a heightfield (3-scale Sobel + mid/macro canyon bevels,
/// OpenGL +Y — the glTF convention).
fn normal_from_height(height: &[f32], res: u32) -> RgbaTexture {
    let mut tex = RgbaTexture::new(res, res);
    let n = res as i32;
    let scale = (res as f32 / 256.0).clamp(0.5, 4.5);
    let fine_strength = 2.8 * scale;
    let mid_step = 2_i32;
    let mid_strength = 2.4 * (scale * 0.5).clamp(0.5, 2.2);
    let macro_step = (res / 200).max(4) as i32;
    let macro_strength = 1.6;
    let sample = |x: i32, y: i32| -> f32 {
        let wx = wrap_period(x, n) as usize;
        let wy = wrap_period(y, n) as usize;
        height[wy * (res as usize) + wx]
    };

    for y in 0..res as i32 {
        for x in 0..res as i32 {
            // 1-texel 3x3 Sobel micro-fiber & cork-pore gradient (primary tack-sharp detail!)
            let tl = sample(x - 1, y - 1);
            let tc = sample(x, y - 1);
            let tr = sample(x + 1, y - 1);
            let ml = sample(x - 1, y);
            let mr = sample(x + 1, y);
            let bl = sample(x - 1, y + 1);
            let bc = sample(x, y + 1);
            let br = sample(x + 1, y + 1);

            let gx_fine = ((tr + 2.0 * mr + br) - (tl + 2.0 * ml + bl)) * 0.125 * fine_strength;
            let gy_fine = ((bl + 2.0 * bc + br) - (tl + 2.0 * tc + tr)) * 0.125 * fine_strength;

            // Crisp 2-texel bark-fissure & flake-edge bevel
            let gx_mid = (sample(x + mid_step, y) - sample(x - mid_step, y)) * 0.5 * mid_strength;
            let gy_mid = (sample(x, y + mid_step) - sample(x, y - mid_step)) * 0.5 * mid_strength;

            // Subtle broad bark-ridge contour bevel
            let gx_macro =
                (sample(x + macro_step, y) - sample(x - macro_step, y)) * 0.5 * macro_strength;
            let gy_macro =
                (sample(x, y + macro_step) - sample(x, y - macro_step)) * 0.5 * macro_strength;

            let mut dx = gx_fine * 0.66 + gx_mid * 0.24 + gx_macro * 0.10;
            let mut dy = gy_fine * 0.66 + gy_mid * 0.24 + gy_macro * 0.10;
            let slope_len = (dx * dx + dy * dy).sqrt();
            if slope_len > 1.42 {
                let inv_s = 1.42 / slope_len;
                dx *= inv_s;
                dy *= inv_s;
            }
            let nrm = Vec3::new(-dx, dy, 1.0).normalize();
            tex.set(
                x as u32,
                y as u32,
                [
                    ((nrm.x * 0.5 + 0.5) * 255.0) as u8,
                    ((nrm.y * 0.5 + 0.5) * 255.0) as u8,
                    ((nrm.z * 0.5 + 0.5) * 255.0) as u8,
                    255,
                ],
            );
        }
    }
    tex
}

// ---------------------------------------------------------------------------
// Leaf card
// ---------------------------------------------------------------------------

struct CardLeaf {
    /// Center in card space [0,1], v down.
    center: Vec2,
    /// Radians; rotates leaf-local +y (tip) to this direction.
    rotation: f32,
    /// Leaf extent in card space.
    scale: f32,
    /// Per-leaf lightness jitter.
    jitter: f32,
    /// Per-leaf warm (+) / cool (-) hue shift.
    hue: f32,
}

struct TwigStroke {
    a: Vec2,
    b: Vec2,
    w_a: f32,
    w_b: f32,
}

/// Evaluate quadratic Bezier curve at `t` in `[0, 1]`.
fn quad_bezier(p0: Vec2, p1: Vec2, p2: Vec2, t: f32) -> Vec2 {
    let u = 1.0 - t;
    p0 * (u * u) + p1 * (2.0 * u * t) + p2 * (t * t)
}

/// Compose the spray of leaf instances and woody twig strokes for a card layout.
fn card_leaves_and_twigs(
    layout: LeafCardLayout,
    shape: LeafShape,
    rng: &mut Rng,
) -> (Vec<CardLeaf>, Vec<TwigStroke>) {
    match layout {
        LeafCardLayout::Single => {
            let twigs = vec![TwigStroke {
                a: Vec2::new(0.5, 0.985),
                b: Vec2::new(0.5, 0.88),
                w_a: 0.009,
                w_b: 0.005,
            }];
            let leaves = vec![CardLeaf {
                center: Vec2::new(0.5, 0.50),
                rotation: 0.0,
                scale: 0.78,
                jitter: 1.0,
                hue: 0.0,
            }];
            (leaves, twigs)
        }
        LeafCardLayout::Cluster => {
            // High-definition airy botanical branchlet fan:
            // Graceful 3-forked woody shoot carrying 12-16 generously scaled, crisply separated
            // leaves with real sky-gaps (fenestration) so every Momiji star, Ginkgo fan, Oak lobe,
            // and Willow lance reads razor-sharp both up close and from hero framing.
            let ctrl_x = 0.5 + rng.range(-0.045, 0.045);
            let top_x = 0.5 + rng.range(-0.04, 0.04);
            let p0 = Vec2::new(0.5, 0.985);
            let p1 = Vec2::new(ctrl_x, 0.56);
            let p2 = Vec2::new(top_x, 0.24);

            let mut twigs = Vec::new();
            let segs = 8;
            for s in 0..segs {
                let t0 = s as f32 / segs as f32;
                let t1 = (s + 1) as f32 / segs as f32;
                twigs.push(TwigStroke {
                    a: quad_bezier(p0, p1, p2, t0),
                    b: quad_bezier(p0, p1, p2, t1),
                    w_a: lerp(0.010, 0.0034, t0),
                    w_b: lerp(0.010, 0.0034, t1),
                });
            }

            let fork_specs = [
                (
                    0.24_f32,
                    -1.0_f32,
                    Vec2::new(0.20 + rng.range(-0.02, 0.02), 0.45 + rng.range(-0.03, 0.03)),
                ),
                (
                    0.34_f32,
                    1.0_f32,
                    Vec2::new(0.80 + rng.range(-0.02, 0.02), 0.41 + rng.range(-0.03, 0.03)),
                ),
            ];

            let mut leaves = Vec::with_capacity(20);

            for &(t_fork, fork_side, fork_tip) in &fork_specs {
                let f0 = quad_bezier(p0, p1, p2, t_fork);
                let f1 = f0.lerp(fork_tip, 0.52) + Vec2::new(fork_side * 0.04, 0.025);
                for s in 0..5 {
                    let t0 = s as f32 / 5.0;
                    let t1 = (s + 1) as f32 / 5.0;
                    twigs.push(TwigStroke {
                        a: quad_bezier(f0, f1, fork_tip, t0),
                        b: quad_bezier(f0, f1, fork_tip, t1),
                        w_a: lerp(0.0062, 0.0026, t0),
                        w_b: lerp(0.0062, 0.0026, t1),
                    });
                }

                let sub_count = match shape {
                    LeafShape::Willow | LeafShape::Sickle | LeafShape::Banksia => 4,
                    _ => 2,
                };
                for i in 0..sub_count {
                    let st = (i as f32 + 0.42) / (sub_count as f32 + 0.15);
                    let side = if i % 2 == 0 { fork_side } else { -fork_side };
                    let node = quad_bezier(f0, f1, fork_tip, st);
                    let tan = (quad_bezier(f0, f1, fork_tip, (st + 0.05).min(1.0))
                        - quad_bezier(f0, f1, fork_tip, (st - 0.05).max(0.0)))
                    .try_normalize()
                    .unwrap_or(Vec2::new(fork_side * 0.6, -0.8).normalize());
                    let nrm = Vec2::new(-tan.y * side, tan.x * side);
                    let sweep = lerp(0.96, 0.62, st) + rng.range(-0.08, 0.08);
                    let dir = (tan * sweep.cos() + nrm * sweep.sin()).normalize();
                    let leaf_scale = match shape {
                        LeafShape::Willow | LeafShape::Sickle | LeafShape::Banksia => {
                            lerp(0.40, 0.31, st) * rng.variance_mul(0.08)
                        }
                        LeafShape::Ginkgo => lerp(0.28, 0.23, st) * rng.variance_mul(0.08),
                        _ => lerp(0.32, 0.26, st) * rng.variance_mul(0.08),
                    };
                    let petiole_len = lerp(0.048, 0.028, st);
                    let leaf_base = node + dir * petiole_len;
                    let center = (leaf_base + dir * (leaf_scale * 0.44))
                        .clamp(Vec2::new(0.11, 0.12), Vec2::new(0.89, 0.89));
                    twigs.push(TwigStroke {
                        a: node,
                        b: leaf_base + dir * (leaf_scale * 0.10),
                        w_a: 0.0034,
                        w_b: 0.0018,
                    });
                    let tip_angle = dir.y.atan2(dir.x);
                    leaves.push(CardLeaf {
                        center,
                        rotation: crate::constants::PI / 2.0 - tip_angle,
                        scale: leaf_scale,
                        jitter: lerp(0.76, 0.98, st) * rng.range(0.92, 1.05),
                        hue: lerp(-0.34, 0.18, st) + rng.range(-0.18, 0.18),
                    });
                }

                // Single crisp terminal blade on each lateral sub-twig
                let f_tan = (fork_tip - f1)
                    .try_normalize()
                    .unwrap_or(Vec2::new(fork_side * 0.55, -0.83).normalize());
                let ang = f_tan.y.atan2(f_tan.x) + fork_side * 0.12 + rng.range(-0.06, 0.06);
                let dir = Vec2::new(ang.cos(), ang.sin());
                let leaf_scale = match shape {
                    LeafShape::Willow | LeafShape::Sickle | LeafShape::Banksia => {
                        0.34 * rng.variance_mul(0.08)
                    }
                    LeafShape::Ginkgo => 0.25 * rng.variance_mul(0.08),
                    _ => 0.28 * rng.variance_mul(0.08),
                };
                let leaf_base = fork_tip + dir * 0.020;
                let center = (leaf_base + dir * (leaf_scale * 0.44))
                    .clamp(Vec2::new(0.11, 0.12), Vec2::new(0.89, 0.89));
                twigs.push(TwigStroke {
                    a: fork_tip,
                    b: leaf_base + dir * 0.025,
                    w_a: 0.0030,
                    w_b: 0.0016,
                });
                leaves.push(CardLeaf {
                    center,
                    rotation: crate::constants::PI / 2.0 - ang,
                    scale: leaf_scale,
                    jitter: rng.range(0.96, 1.10),
                    hue: rng.range(0.0, 0.32),
                });
            }

            let count: usize = match shape {
                LeafShape::Willow | LeafShape::Sickle | LeafShape::Banksia => 8,
                _ => 5,
            };

            for i in 0..count {
                let t = i as f32 / (count.saturating_sub(1).max(1)) as f32;
                let side = if i % 2 == 0 { 1.0 } else { -1.0 };
                let along = (0.16 + t * 0.74 + rng.range(-0.02, 0.02)).clamp(0.12, 0.94);
                let node = quad_bezier(p0, p1, p2, along);
                let tangent = (quad_bezier(p0, p1, p2, (along + 0.03).min(1.0))
                    - quad_bezier(p0, p1, p2, (along - 0.03).max(0.0)))
                .try_normalize()
                .unwrap_or(Vec2::new(0.0, -1.0));
                let normal = Vec2::new(-tangent.y * side, tangent.x * side);

                // Wide alternating splay so adjacent blades never pile into a solid blob
                let sweep = lerp(1.12, 0.68, t) + rng.range(-0.08, 0.08);
                let dir = (tangent * sweep.cos() + normal * sweep.sin()).normalize();
                let leaf_scale = match shape {
                    LeafShape::Willow | LeafShape::Sickle | LeafShape::Banksia => {
                        lerp(0.44, 0.32, t) * rng.variance_mul(0.08)
                    }
                    LeafShape::Ginkgo => lerp(0.31, 0.25, t) * rng.variance_mul(0.08),
                    _ => lerp(0.36, 0.28, t) * rng.variance_mul(0.08),
                };
                let petiole_len = lerp(0.068, 0.034, t) * rng.variance_mul(0.10);
                let leaf_base = node + dir * petiole_len;
                let center = (leaf_base + dir * (leaf_scale * 0.45))
                    .clamp(Vec2::new(0.12, 0.12), Vec2::new(0.88, 0.89));

                twigs.push(TwigStroke {
                    a: node,
                    b: leaf_base + dir * (leaf_scale * 0.10),
                    w_a: lerp(0.0044, 0.0028, t),
                    w_b: 0.0018,
                });

                let tip_angle = dir.y.atan2(dir.x);
                // Give Parisian Linden (Heart) sprays 2 warm late-summer golden-ochre leaves
                let hue = if shape == LeafShape::Heart && (i == 1 || i == 3) {
                    rng.range(2.65, 3.60)
                } else {
                    lerp(-0.24, 0.44, t) + rng.range(-0.18, 0.18)
                };
                leaves.push(CardLeaf {
                    center,
                    rotation: crate::constants::PI / 2.0 - tip_angle,
                    scale: leaf_scale,
                    jitter: lerp(0.86, 1.14, t) * rng.range(0.94, 1.06),
                    hue,
                });
            }

            // Crisp 2-leaf apical pair crowning the main shoot tip without crowding
            let apex = p2;
            let apex_tan = (p2 - p1).try_normalize().unwrap_or(Vec2::new(0.0, -1.0));
            for fan in [-0.36_f32, 0.34] {
                let ang = apex_tan.y.atan2(apex_tan.x) + fan + rng.range(-0.05, 0.05);
                let dir = Vec2::new(ang.cos(), ang.sin());
                let leaf_scale = match shape {
                    LeafShape::Willow | LeafShape::Sickle | LeafShape::Banksia => {
                        0.34 * rng.variance_mul(0.08)
                    }
                    LeafShape::Ginkgo => 0.26 * rng.variance_mul(0.08),
                    _ => 0.29 * rng.variance_mul(0.08),
                };
                let petiole = 0.026;
                let leaf_base = apex + dir * petiole;
                twigs.push(TwigStroke {
                    a: apex,
                    b: leaf_base + dir * 0.028,
                    w_a: 0.0034,
                    w_b: 0.0018,
                });
                leaves.push(CardLeaf {
                    center: (leaf_base + dir * (leaf_scale * 0.45))
                        .clamp(Vec2::new(0.12, 0.11), Vec2::new(0.88, 0.88)),
                    rotation: crate::constants::PI / 2.0 - ang,
                    scale: leaf_scale,
                    jitter: rng.range(1.02, 1.16),
                    hue: rng.range(0.10, 0.50),
                });
            }
            (leaves, twigs)
        }
        LeafCardLayout::Puff => {
            // Crisp multi-lobe broadleaf canopy bouquet: woody interior fork twigs supporting
            // 5 scalloped sub-clusters of veined botanical leaves (eliminating mushy blob sponges).
            let hub = Vec2::new(0.50, 0.62);
            let mut twigs = Vec::with_capacity(6);
            let sub_clumps: Vec<(Vec2, f32, f32)> = {
                let mut v = Vec::with_capacity(6);
                let n = 5;
                for k in 0..n {
                    let a = k as f32 / n as f32 * crate::constants::TAU + rng.range(-0.28, 0.28);
                    let dist = rng.range(0.14, 0.21);
                    let c = Vec2::new(0.5 + dist * a.cos(), 0.50 + dist * a.sin() * 0.90);
                    twigs.push(TwigStroke {
                        a: hub,
                        b: hub.lerp(c, 0.88),
                        w_a: 0.0095,
                        w_b: 0.0040,
                    });
                    // Sunlit bias toward upper sub-clumps (-y in UV is top)
                    let sun = lerp(1.10, 0.82, ((c.y - 0.28) / 0.46).clamp(0.0, 1.0));
                    v.push((c, rng.range(0.20, 0.25), sun));
                }
                v.push((Vec2::new(0.5, 0.48), 0.21, 1.02));
                v
            };

            let count = 92 + rng.index(18);
            let mut leaves = Vec::with_capacity(count);
            for i in 0..count {
                let layer = i as f32 / (count - 1) as f32;
                let (sc, sr, sun) = sub_clumps[rng.index(sub_clumps.len())];
                let r = sr * lerp(1.0, 0.48, layer) * rng.next_f32().sqrt();
                let a = rng.range(-crate::constants::PI, crate::constants::PI);
                let center = (sc + Vec2::new(r * a.cos(), r * a.sin()))
                    .clamp(Vec2::splat(0.12), Vec2::splat(0.88));
                let out = (center - sc)
                    .try_normalize()
                    .unwrap_or(Vec2::new(0.0, -1.0));
                let tip_angle = out.y.atan2(out.x) + rng.range(-0.55, 0.55);
                leaves.push(CardLeaf {
                    center,
                    rotation: crate::constants::PI / 2.0 - tip_angle,
                    scale: lerp(0.215, 0.140, layer) * rng.variance_mul(0.18),
                    jitter: sun * lerp(0.74, 1.08, layer) * rng.range(0.92, 1.08),
                    hue: lerp(-0.32, 0.38, layer) + rng.range(-0.28, 0.28),
                });
            }
            (leaves, twigs)
        }
        LeafCardLayout::Dense => {
            // Overlapping asymmetric bough lobes with soft sun-angle shading rather than
            // glowing concentric bullseye centers.
            let lobes: Vec<(Vec2, f32, f32)> = {
                let n = 6 + rng.index(2);
                let mut v = Vec::with_capacity(n + 1);
                for k in 0..n {
                    let a = k as f32 / n as f32 * crate::constants::TAU + rng.range(-0.35, 0.35);
                    let d = rng.range(0.14, 0.22);
                    let c = Vec2::new(0.5 + d * a.cos(), 0.5 + d * a.sin());
                    let sun = lerp(1.06, 0.84, ((c.y - 0.28) / 0.46).clamp(0.0, 1.0));
                    v.push((c, rng.range(0.18, 0.24), sun));
                }
                v.push((Vec2::new(0.50 + rng.range(-0.04, 0.04), 0.48), 0.22, 0.98));
                v
            };
            let count = 950 + rng.index(150);
            let mut heaped: Vec<(f32, CardLeaf)> = Vec::with_capacity(count);
            for _ in 0..count {
                let (c, lr, sun) = lobes[rng.index(lobes.len())];
                let r = lr * rng.next_f32().sqrt();
                let a = rng.range(-crate::constants::PI, crate::constants::PI);
                let center = c + Vec2::new(r * a.cos(), r * a.sin());
                let height = 1.0 - (r / lr).powf(1.8);
                let out = (center - c).try_normalize().unwrap_or(Vec2::new(0.0, -1.0));
                let tip_angle = out.y.atan2(out.x) + rng.range(-1.0, 1.0);
                heaped.push((
                    height + rng.range(-0.12, 0.12),
                    CardLeaf {
                        center,
                        rotation: crate::constants::PI / 2.0 - tip_angle,
                        scale: lerp(0.068, 0.046, height) * rng.variance_mul(0.24),
                        jitter: sun * lerp(0.82, 1.04, height) * rng.range(0.88, 1.08),
                        hue: lerp(-0.30, 0.35, height) + rng.range(-0.35, 0.35),
                    },
                ));
            }
            heaped.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            (heaped.into_iter().map(|(_, l)| l).collect(), Vec::new())
        }
        LeafCardLayout::Frond
        | LeafCardLayout::Buxus
        | LeafCardLayout::ScaleCypress
        | LeafCardLayout::Floral => (Vec::new(), Vec::new()),
    }
}

/// Render the leaf card: woody twigs + petioles underleaf, then leaves back-to-front
/// with botanical venation, inter-leaf contact AO, razor-sharp anti-aliased SDF edges,
/// 16x16 tile spatial binning (for fast 1024x1024 WASM generation), and RGB edge dilation.
pub(crate) fn leaf_card(
    shape: LeafShape,
    layout: LeafCardLayout,
    color: Option<[f32; 3]>,
    seed: u64,
    res: u32,
) -> RgbaTexture {
    let mut rng = Rng::from_seed(seed ^ 0x1EAF_CA4D);
    if layout == LeafCardLayout::Buxus {
        return crate::bistro_cards::buxus_sprig_card(color, &mut rng, seed, res);
    }
    if layout == LeafCardLayout::ScaleCypress {
        return crate::bistro_cards::scale_cypress_card(color, &mut rng, seed, res);
    }
    if layout == LeafCardLayout::Floral {
        return crate::bistro_cards::floral_pansy_atlas_card(color, &mut rng, seed, res);
    }
    if shape == LeafShape::Paddle {
        return crate::bistro_cards::paddle_leaf_card(color, &mut rng, seed, res);
    }
    if matches!(shape, LeafShape::Bipinnate | LeafShape::Branchlet) {
        return crate::botanical_cards::stroke_leaf_card(shape, layout, color, &mut rng, res);
    }
    if shape == LeafShape::Willow && layout == LeafCardLayout::Frond {
        return palm_pinnae_card(color, &mut rng, res);
    }
    if layout == LeafCardLayout::Frond {
        return frond_card(color, &mut rng, res);
    }
    if shape == LeafShape::Needle && layout == LeafCardLayout::Cluster {
        return needle_cluster_card(color, &mut rng, res);
    }
    if shape == LeafShape::Needle && layout == LeafCardLayout::Puff {
        return yucca_rosette_card(color, &mut rng, res);
    }

    let (leaves, twigs) = card_leaves_and_twigs(layout, shape, &mut rng);
    let base = color.unwrap_or([0.27, 0.44, 0.17]);
    let stem_rgb = [0.30, 0.22, 0.14];
    let detailed_veins = matches!(
        layout,
        LeafCardLayout::Single | LeafCardLayout::Cluster | LeafCardLayout::Puff
    );

    let px = 1.0 / res as f32;
    const TILES: usize = 16;
    let tile_of = |c: f32| ((c.clamp(0.0, 0.9999) * TILES as f32) as usize).min(TILES - 1);

    let mut twig_bins: Vec<Vec<usize>> = vec![Vec::new(); TILES * TILES];
    for (idx, stroke) in twigs.iter().enumerate() {
        let pad = stroke.w_a.max(stroke.w_b) + px * 2.0;
        let x0 = tile_of((stroke.a.x.min(stroke.b.x) - pad).max(0.0));
        let x1 = tile_of((stroke.a.x.max(stroke.b.x) + pad).min(1.0));
        let y0 = tile_of((stroke.a.y.min(stroke.b.y) - pad).max(0.0));
        let y1 = tile_of((stroke.a.y.max(stroke.b.y) + pad).min(1.0));
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                twig_bins[ty * TILES + tx].push(idx);
            }
        }
    }

    // Precompute per-leaf sin/cos & bounding radius + bin into 16x16 tiles
    let mut leaf_bounds: Vec<(f32, f32, f32)> = Vec::with_capacity(leaves.len());
    let mut leaf_bins: Vec<Vec<usize>> = vec![Vec::new(); TILES * TILES];
    for (idx, leaf) in leaves.iter().enumerate() {
        let (s, c) = leaf.rotation.sin_cos();
        let rad = leaf.scale * 0.72 + px * 2.0;
        leaf_bounds.push((s, c, rad * rad));
        let x0 = tile_of((leaf.center.x - rad).max(0.0));
        let x1 = tile_of((leaf.center.x + rad).min(1.0));
        let y0 = tile_of((leaf.center.y - rad).max(0.0));
        let y1 = tile_of((leaf.center.y + rad).min(1.0));
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                leaf_bins[ty * TILES + tx].push(idx);
            }
        }
    }

    let mut tex = RgbaTexture::new(res, res);
    for y in 0..res {
        let v = (y as f32 + 0.5) * px;
        let ty = tile_of(v);
        for x in 0..res {
            let u = (x as f32 + 0.5) * px;
            let tile_idx = ty * TILES + tile_of(u);
            let t_bin = &twig_bins[tile_idx];
            let l_bin = &leaf_bins[tile_idx];
            if t_bin.is_empty() && l_bin.is_empty() {
                continue;
            }

            let q = Vec2::new(u, v);
            let mut acc = [0.0f32; 4];

            // Woody twig & sub-petioles underneath leaf blades
            for &idx in t_bin {
                let stroke = &twigs[idx];
                let ab = stroke.b - stroke.a;
                let t = ((q - stroke.a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
                let w = lerp(stroke.w_a, stroke.w_b, t);
                let d = (q - (stroke.a + ab * t)).length();
                let a = ((w - d) / px + 0.5).clamp(0.0, 1.0);
                if a > 0.0 {
                    let cyl = 0.76 + 0.42 * (1.0 - (d / w.max(1e-4)).clamp(0.0, 1.0));
                    over(
                        &mut acc,
                        [
                            (stem_rgb[0] * cyl).min(1.0),
                            (stem_rgb[1] * cyl).min(1.0),
                            (stem_rgb[2] * cyl).min(1.0),
                            a,
                        ],
                    );
                }
            }

            if !l_bin.is_empty() {
                let mottle = value_noise(seed ^ 0xC0, u * 16.0, v * 16.0, 16, 16);
                for &idx in l_bin {
                    let leaf = &leaves[idx];
                    let (sin, cos, max_r2) = leaf_bounds[idx];
                    let p = q - leaf.center;
                    if p.length_squared() > max_r2 {
                        continue;
                    }
                    // Leaf-local space: +y is tip, -y is petiole base
                    let local =
                        Vec2::new(p.x * cos - p.y * sin, -(p.x * sin + p.y * cos)) / leaf.scale;
                    let sdf = leaf_sdf(local, shape);
                    let edge = (0.85 / (res as f32 * leaf.scale.max(0.02))).max(0.0012);
                    let alpha = (0.5 - sdf / edge).clamp(0.0, 1.0);
                    if alpha <= 0.0 {
                        continue;
                    }

                    // Soft contact ambient occlusion cast onto underlying blades
                    if acc[3] > 0.15 && alpha > 0.25 {
                        let occ = 1.0 - 0.22 * alpha;
                        acc[0] *= occ;
                        acc[1] *= occ;
                        acc[2] *= occ;
                    }

                    // Natural lamina shading: base-to-tip translucency + subtle bilateral V-keel fold
                    let along = (local.y + 0.5).clamp(0.0, 1.0);
                    let keel = 1.0 + 0.07 * local.x.clamp(-0.35, 0.35)
                        - 0.10 * (local.x.abs() * 1.9).clamp(0.0, 0.35);
                    let mut shade =
                        leaf.jitter * (0.84 + 0.24 * along) * keel * (0.93 + 0.14 * mottle);
                    let mut vein_tint = [1.0f32, 1.0, 1.0];

                    if detailed_veins {
                        match shape {
                            LeafShape::Ginkgo => {
                                // Dichotomous parallel-radiating fan striation of Ginkgo biloba
                                let fan = Vec2::new(local.x, local.y + 0.42);
                                let angle = fan.x.atan2(fan.y.max(1e-4));
                                let stria = (angle * 48.0).sin().abs();
                                let k = (1.0 - stria).powi(4);
                                shade *= 1.0 + 0.14 * k;
                                vein_tint = [1.0 + 0.08 * k, 1.0 + 0.06 * k, 1.0 - 0.08 * k];
                            }
                            LeafShape::Ivy => {
                                // Hedera helix: high-contrast cream-chartreuse 5-rib palmate venation
                                // + bright reticulate network on dark glossy forest-emerald lamina.
                                let hub = Vec2::new(local.x, local.y + 0.14);
                                let r = hub.length();
                                let a = hub.x.abs().atan2(hub.y);
                                let nearest_rib = [0.0_f32, 1.10, 2.08]
                                    .into_iter()
                                    .map(|ra| (a - ra).abs())
                                    .fold(f32::MAX, f32::min);
                                let rib_d = r * nearest_rib.sin();
                                let rib_w = lerp(0.015, 0.0030, (r / 0.62).clamp(0.0, 1.0));
                                let cell = value_noise(
                                    seed ^ 0x1711,
                                    (local.x + 1.0) * 44.0,
                                    (local.y + 1.0) * 44.0,
                                    128,
                                    128,
                                );
                                let net = (1.0 - (cell - 0.5).abs() * 11.0).clamp(0.0, 1.0);
                                if r < 0.64 && rib_d < rib_w {
                                    let k = (1.0 - rib_d / rib_w.max(1e-4)).powf(0.75);
                                    shade *= 1.0 + 0.52 * k;
                                    vein_tint = [1.0 + 0.55 * k, 1.0 + 0.42 * k, 1.0 + 0.18 * k];
                                } else {
                                    let sec = ((r - rib_d * 0.55) * 18.0).fract();
                                    let sec_k = (1.0 - (sec - 0.5).abs() * 9.5).clamp(0.0, 1.0)
                                        * (1.0 - (rib_d * 4.2).clamp(0.0, 1.0));
                                    let v_net = (0.65 * sec_k + 0.38 * net).clamp(0.0, 1.0);
                                    shade *= 1.0 + 0.28 * v_net;
                                    vein_tint = [
                                        1.0 + 0.34 * v_net,
                                        1.0 + 0.26 * v_net,
                                        1.0 + 0.08 * v_net,
                                    ];
                                }
                            }
                            LeafShape::Maple | LeafShape::Palmate => {
                                // 7-rib palmate primary venation + lateral secondary ribs
                                let hub = Vec2::new(local.x, local.y + 0.16);
                                let r = hub.length();
                                let a = hub.x.abs().atan2(hub.y);
                                let nearest_rib = [0.0_f32, 0.68, 1.36, 2.02]
                                    .into_iter()
                                    .map(|ra| (a - ra).abs())
                                    .fold(f32::MAX, f32::min);
                                let rib_d = r * nearest_rib.sin();
                                let rib_w = lerp(0.011, 0.0022, (r / 0.58).clamp(0.0, 1.0));
                                if r < 0.56 && rib_d < rib_w {
                                    let k = 1.0 - rib_d / rib_w.max(1e-4);
                                    shade *= 1.0 + 0.17 * k;
                                    vein_tint = [1.0 + 0.12 * k, 1.0 + 0.08 * k, 1.0 - 0.10 * k];
                                } else {
                                    let sec = ((r - rib_d * 0.7) * 26.0).fract();
                                    let sec_k = (1.0 - (sec - 0.5).abs() * 10.0).clamp(0.0, 1.0)
                                        * (1.0 - (rib_d * 6.0).clamp(0.0, 1.0));
                                    shade *= 1.0 + 0.06 * sec_k;
                                }
                            }
                            _ => {
                                // Razor-crisp tapered midrib + herringbone secondary + reticulate net
                                let midrib_w = lerp(0.011, 0.0022, along) * (1.0 - along * 0.22);
                                if local.y < 0.45 && local.x.abs() < midrib_w {
                                    let k = 1.0 - local.x.abs() / midrib_w.max(1e-4);
                                    shade *= 1.0 + 0.18 * k;
                                    vein_tint = [1.0 + 0.14 * k, 1.0 + 0.09 * k, 1.0 - 0.12 * k];
                                } else if shape != LeafShape::Needle {
                                    let sec = ((local.y - local.x.abs() * 0.62) * 14.0).fract();
                                    let sec_line = (1.0 - (sec - 0.5).abs() * 11.0).clamp(0.0, 1.0)
                                        * (1.0 - (local.x.abs() * 2.5).clamp(0.0, 1.0));
                                    let cell = value_noise(
                                        seed ^ 0xCE11,
                                        (local.x + 1.0) * 64.0,
                                        (local.y + 1.0) * 64.0,
                                        128,
                                        128,
                                    );
                                    let retic = (1.0 - (cell - 0.5).abs() * 9.0).clamp(0.0, 1.0);
                                    shade *= 1.0 + 0.09 * sec_line + 0.035 * retic;
                                    vein_tint[0] += 0.05 * sec_line;
                                    vein_tint[1] += 0.04 * sec_line;
                                }
                            }
                        }
                        // Crisp sunlit rim translucency right inside the margin
                        let rim = (1.0 + sdf / 0.042).clamp(0.0, 1.0);
                        shade *= 1.0 + 0.09 * rim;
                    }

                    let leaf_base_rgb = if shape == LeafShape::Heart && leaf.hue > 1.5 {
                        // Warm late-summer Parisian Linden golden-amber / ochre accent blade
                        let amber_t = ((leaf.hue - 1.5) / 2.0).clamp(0.45, 0.92);
                        [
                            lerp(base[0], 0.68, amber_t),
                            lerp(base[1], 0.44, amber_t),
                            lerp(base[2], 0.10, amber_t),
                        ]
                    } else {
                        base
                    };
                    let eff_hue = if shape == LeafShape::Heart && leaf.hue > 1.5 {
                        0.25
                    } else {
                        leaf.hue
                    };
                    let warm = [
                        (1.0 + 0.12 * eff_hue) * vein_tint[0],
                        (1.0 + 0.04 * eff_hue) * vein_tint[1],
                        (1.0 - 0.15 * eff_hue) * vein_tint[2],
                    ];
                    let c = [
                        (leaf_base_rgb[0] * shade * warm[0]).clamp(0.0, 1.0),
                        (leaf_base_rgb[1] * shade * warm[1]).clamp(0.0, 1.0),
                        (leaf_base_rgb[2] * shade * warm[2]).clamp(0.0, 1.0),
                    ];
                    over(&mut acc, [c[0], c[1], c[2], alpha]);
                }
            }

            tex.set(
                x,
                y,
                [
                    (acc[0].clamp(0.0, 1.0) * 255.0) as u8,
                    (acc[1].clamp(0.0, 1.0) * 255.0) as u8,
                    (acc[2].clamp(0.0, 1.0) * 255.0) as u8,
                    (acc[3].clamp(0.0, 1.0) * 255.0) as u8,
                ],
            );
        }
    }
    tex.dilate_alpha_rgb(12);
    tex
}

/// Japanese Red Pine (*Akamatsu*) / conifer needle fascicle shoot card:
/// scaly woody 5-shoot branchlet with dark papery fascicle sheaths and 260+ razor-fine needles.
fn needle_cluster_card(color: Option<[f32; 3]>, rng: &mut Rng, res: u32) -> RgbaTexture {
    let base = color.unwrap_or([0.16, 0.34, 0.16]);
    let twig_rgb = [0.33, 0.20, 0.12];
    let mut twigs: Vec<(Vec2, Vec2, f32, f32)> = Vec::new();
    let mut needles: Vec<(Vec2, Vec2, f32, f32, f32, f32)> = Vec::new();

    let p0 = Vec2::new(0.50, 0.97);
    let p1 = Vec2::new(0.50 + rng.range(-0.04, 0.04), 0.56);
    let p2 = Vec2::new(0.50 + rng.range(-0.03, 0.03), 0.22);
    let mut shoots: Vec<(Vec2, Vec2, f32)> = Vec::new();
    for s in 0..6 {
        let t0 = s as f32 / 6.0;
        let t1 = (s + 1) as f32 / 6.0;
        let a = quad_bezier(p0, p1, p2, t0);
        let b = quad_bezier(p0, p1, p2, t1);
        twigs.push((a, b, lerp(0.012, 0.0055, t0), lerp(0.012, 0.0055, t1)));
        shoots.push((a, b, t0));
    }
    // Four angled lateral shoots forming a dense, sculptural Akamatsu matsuba fan
    for &(t_attach, side, spread, len) in &[
        (0.24f32, -1.0f32, 0.78f32, 0.28f32),
        (0.28, 1.0, 0.75, 0.27),
        (0.54, -1.0, 0.62, 0.22),
        (0.58, 1.0, 0.60, 0.21),
    ] {
        let a = quad_bezier(p0, p1, p2, t_attach);
        let ang = -crate::constants::PI * 0.5 + side * (spread + rng.range(-0.06, 0.06));
        let b =
            (a + Vec2::new(ang.cos(), ang.sin()) * len).clamp(Vec2::splat(0.10), Vec2::splat(0.90));
        twigs.push((a, b, 0.0085, 0.0042));
        shoots.push((a, b, t_attach));
    }

    // Radiate paired/tripled razor-fine needle fascicles with short basal sheaths
    for pass in 0..2 {
        let is_top_layer = pass == 1;
        for &(sa, sb, t_base) in &shoots {
            let len = (sb - sa).length();
            let dir = (sb - sa) / len.max(1e-4);
            let perp = Vec2::new(-dir.y, dir.x);
            let bundles = ((len / 0.032).ceil() as usize).max(3);
            for b_idx in 0..=bundles {
                let bt = b_idx as f32 / bundles as f32;
                let origin = sa.lerp(sb, bt);
                for side in [-1.0f32, 1.0] {
                    let sheath_ang = rng.range(0.38, 0.92);
                    let sheath_dir =
                        (dir * sheath_ang.cos() + perp * side * sheath_ang.sin()).normalize();
                    let tip_sheath = origin + sheath_dir * 0.015;
                    if !is_top_layer {
                        twigs.push((origin, tip_sheath, 0.0040, 0.0032));
                    }
                    for n_i in 0..2 {
                        let spread =
                            sheath_ang + (n_i as f32 - 0.5) * 0.18 + rng.range(-0.06, 0.06);
                        let nd = (dir * spread.cos() + perp * side * spread.sin()).normalize();
                        let n_len = rng.range(0.18, 0.26) * lerp(1.05, 0.82, t_base);
                        let end =
                            (tip_sheath + nd * n_len).clamp(Vec2::splat(0.025), Vec2::splat(0.975));
                        let shade = if is_top_layer {
                            rng.range(1.02, 1.30)
                        } else {
                            rng.range(0.64, 0.90)
                        };
                        let hue = if is_top_layer {
                            rng.range(0.08, 0.46)
                        } else {
                            rng.range(-0.32, 0.06)
                        };
                        let w0 = if is_top_layer { 0.0034 } else { 0.0039 };
                        needles.push((tip_sheath, end, w0, 0.0010, shade, hue));
                    }
                }
            }
        }
    }
    // Terminal apical needle burst crowning every shoot tip
    for &(sa, sb, _) in &shoots {
        let dir = (sb - sa).try_normalize().unwrap_or(Vec2::new(0.0, -1.0));
        let base_ang = dir.y.atan2(dir.x);
        for k in 0..12 {
            let frac = k as f32 / 11.0 - 0.5;
            let ang = base_ang + frac * 1.18 + rng.range(-0.07, 0.07);
            let nd = Vec2::new(ang.cos(), ang.sin());
            let end = (sb + nd * rng.range(0.15, 0.22)).clamp(Vec2::splat(0.03), Vec2::splat(0.97));
            needles.push((
                sb,
                end,
                0.0026,
                0.0008,
                rng.range(1.02, 1.28),
                rng.range(0.10, 0.48),
            ));
        }
    }

    rasterize_twig_and_needle_strokes(&twigs, &needles, base, twig_rgb, res)
}

/// Tropical Pinnate Palm Frond Card (`leaf_shape = "willow", leaf_card = "frond"`):
/// tapering golden-green palm rachis spine clothed in 42 pairs of broad, glossy, V-pleated
/// arching sword pinnae fanning cleanly across the full width of the frond ribbon.
fn palm_pinnae_card(color: Option<[f32; 3]>, rng: &mut Rng, res: u32) -> RgbaTexture {
    let base = color.unwrap_or([0.22, 0.46, 0.16]);
    let rachis_rgb = [0.42, 0.48, 0.17];
    let mut twigs: Vec<(Vec2, Vec2, f32, f32)> = Vec::new();
    let mut pinnae: Vec<(Vec2, Vec2, f32, f32, f32, f32)> = Vec::new();

    let p0 = Vec2::new(0.50, 0.99);
    let p1 = Vec2::new(0.50 + rng.range(-0.008, 0.008), 0.50);
    let p2 = Vec2::new(0.50 + rng.range(-0.010, 0.010), 0.05);
    for s in 0..12 {
        let t0 = s as f32 / 12.0;
        let t1 = (s + 1) as f32 / 12.0;
        let a = quad_bezier(p0, p1, p2, t0);
        let b = quad_bezier(p0, p1, p2, t1);
        twigs.push((a, b, lerp(0.013, 0.0022, t0), lerp(0.013, 0.0022, t1)));
    }

    // 36 pairs of distinct, sky-gapped V-pleated glossy sword pinnae tapering to a plumose spear apex
    let pairs = 36usize;
    for pass in 0..2 {
        let top = pass == 1;
        for i in 0..pairs {
            if top && i % 2 == 0 {
                continue;
            }
            if !top && i % 2 == 1 {
                continue;
            }
            let t = 0.04 + 0.93 * (i as f32 / (pairs - 1) as f32);
            let origin = quad_bezier(p0, p1, p2, t);
            // Graceful spear-feathered coconut frond silhouette: broad mid-body, pointed plumose apex
            let base_taper = ((t - 0.01) / 0.16).clamp(0.24, 1.0).sqrt();
            let tip_taper = ((0.99 - t) / 0.40).clamp(0.05, 1.0).powf(0.74);
            let env = (base_taper * tip_taper).clamp(0.05, 1.0);
            let pinna_len = lerp(0.06, 0.475, env) * rng.variance_mul(0.05);

            for side in [-1.0f32, 1.0] {
                // Pinnae sweep gracefully forward toward the frond apex with crisp sky separation
                let launch_ang = -crate::constants::PI * 0.5
                    + side * lerp(1.18, 0.26, t)
                    + rng.range(-0.028, 0.028);
                let droop_ang = launch_ang + side * lerp(0.22, 0.08, t);
                let mid_pt =
                    origin + Vec2::new(launch_ang.cos(), launch_ang.sin()) * (pinna_len * 0.55);
                let tip_pt = (mid_pt
                    + Vec2::new(droop_ang.cos(), droop_ang.sin()) * (pinna_len * 0.47))
                    .clamp(Vec2::splat(0.012), Vec2::splat(0.988));

                let shade = lerp(0.86, 1.16, t)
                    * if top {
                        rng.range(1.00, 1.18)
                    } else {
                        rng.range(0.76, 0.94)
                    };
                let hue = lerp(-0.14, 0.34, t) + rng.range(-0.08, 0.08);
                // Calibrated sword-leaflet width so clean sky gaps sparkle between every leaflet pair
                let w_base = lerp(0.0052, 0.0086, env);
                let w_mid = w_base * 0.88;

                for seg in 0..3 {
                    let st0 = seg as f32 / 3.0;
                    let st1 = (seg + 1) as f32 / 3.0;
                    let sa = quad_bezier(origin, mid_pt, tip_pt, st0);
                    let sb = quad_bezier(origin, mid_pt, tip_pt, st1);
                    let sw0 = if seg == 0 {
                        w_base * 0.72
                    } else if seg == 1 {
                        w_mid
                    } else {
                        w_mid * 0.56
                    };
                    let sw1 = if seg == 0 {
                        w_mid
                    } else if seg == 1 {
                        w_mid * 0.56
                    } else {
                        0.0014
                    };
                    pinnae.push((sa, sb, sw0, sw1, shade * lerp(0.94, 1.10, st0), hue));
                    // Crisp golden-green central leaflet midrib highlight along basal 2/3 of each pinna
                    if top && seg < 2 {
                        pinnae.push((sa, sb, sw0 * 0.18, sw1 * 0.16, shade * 1.24, hue + 0.32));
                    }
                }
            }
        }
    }

    // Terminal V-pair of apical sword pinnae crowning the tip of the rachis cleanly
    for side in [-1.0f32, 1.0] {
        let ang = -crate::constants::PI * 0.5 + side * 0.18;
        let tip = (p2 + Vec2::new(ang.cos(), ang.sin()) * 0.05)
            .clamp(Vec2::splat(0.01), Vec2::splat(0.99));
        pinnae.push((p2 + Vec2::new(0.0, 0.04), tip, 0.011, 0.0012, 1.22, 0.34));
    }

    rasterize_twig_and_needle_strokes(&twigs, &pinnae, base, rachis_rgb, res)
}

/// Mojave Joshua tree (*Yucca brevifolia*) spiky bayonet rosette card:
/// reflexed tan fibrous thatch at the collar crowned by 360-deg stiff dagger blades.
fn yucca_rosette_card(color: Option<[f32; 3]>, rng: &mut Rng, res: u32) -> RgbaTexture {
    let base = color.unwrap_or([0.36, 0.44, 0.23]);
    let thatch_rgb = [0.50, 0.41, 0.26];
    let mut twigs: Vec<(Vec2, Vec2, f32, f32)> = Vec::new();
    let mut blades: Vec<(Vec2, Vec2, f32, f32, f32, f32)> = Vec::new();

    let hub = Vec2::new(0.5, 0.58);
    // Short fibrous stem + reflexed dead lower blades (thatch skirt)
    twigs.push((Vec2::new(0.5, 0.96), hub, 0.028, 0.022));
    for k in 0..18 {
        let frac = k as f32 / 17.0 - 0.5;
        let ang = crate::constants::PI * 0.5 + frac * 1.65 + rng.range(-0.08, 0.08);
        let origin = hub + Vec2::new(rng.range(-0.02, 0.02), rng.range(0.02, 0.12));
        let tip = (origin + Vec2::new(ang.cos(), ang.sin()) * rng.range(0.20, 0.30))
            .clamp(Vec2::splat(0.04), Vec2::splat(0.96));
        twigs.push((origin, tip, 0.015, 0.003));
    }

    // Back, middle, and sunlit core rings of stiff radiating bayonet daggers
    for (ring_count, len_min, len_max, shade_min, shade_max, hue_shift) in [
        (34usize, 0.32f32, 0.43f32, 0.64f32, 0.88f32, -0.20f32),
        (28, 0.24, 0.36, 0.86, 1.08, 0.08),
        (18, 0.14, 0.25, 1.04, 1.26, 0.32),
    ] {
        for k in 0..ring_count {
            let base_a = (k as f32 / ring_count as f32) * crate::constants::TAU;
            let a = base_a + rng.range(-0.11, 0.11);
            // Bias slightly fuller upward (-Y in UV) while keeping 360-deg spiky crown
            let dir = Vec2::new(a.cos(), a.sin() * 0.90 - 0.14).normalize();
            let origin = hub + dir * rng.range(0.0, 0.045);
            let len = rng.range(len_min, len_max);
            let tip = (origin + dir * len).clamp(Vec2::splat(0.03), Vec2::splat(0.97));
            let sun = lerp(1.08, 0.86, ((tip.y - 0.15) / 0.70).clamp(0.0, 1.0));
            let shade = rng.range(shade_min, shade_max) * sun;
            let hue = hue_shift + rng.range(-0.18, 0.18);
            blades.push((origin, tip, 0.016, 0.0026, shade, hue));
        }
    }

    rasterize_twig_and_needle_strokes(&twigs, &blades, base, thatch_rgb, res)
}

/// Conifer bough spray card: a multi-forked spruce/fir bough fan with main shoot,
/// diverging secondary shoots, and tertiary side branchlets densely clothed in layered
/// evergreen needles so top-down and 3/4 views read as natural feathered boughs.
fn frond_card(color: Option<[f32; 3]>, rng: &mut Rng, res: u32) -> RgbaTexture {
    let base = color.unwrap_or([0.16, 0.30, 0.20]);
    let twig_rgb = [0.26, 0.18, 0.12];
    let mut twigs: Vec<(Vec2, Vec2, f32, f32)> = Vec::new();
    let mut needles: Vec<(Vec2, Vec2, f32, f32, f32, f32)> = Vec::new();

    let root = Vec2::new(0.5, 0.96);
    let apex = Vec2::new(0.5 + rng.range(-0.02, 0.02), 0.09);
    twigs.push((root, root.lerp(apex, 0.86), 0.013, 0.005));

    // Main central shoot + 4 diverging secondary bough forks (forming a natural spruce fan)
    let mut branches = vec![(root, apex, 0.0f32)];
    for &(fork_t, side, spread, len_scale) in &[
        (0.18f32, -1.0f32, 0.72f32, 0.56f32),
        (0.20, 1.0, 0.70, 0.55),
        (0.46, -1.0, 0.56, 0.42),
        (0.48, 1.0, 0.54, 0.41),
    ] {
        let p = root.lerp(apex, fork_t);
        let ang = side * spread + rng.range(-0.05, 0.05);
        let dir = Vec2::new(ang.sin(), -ang.cos());
        let tip = (p + dir * len_scale).clamp(Vec2::splat(0.06), Vec2::splat(0.94));
        twigs.push((p, p.lerp(tip, 0.82), 0.009, 0.004));
        branches.push((p, tip, fork_t * 0.6));

        // 3 pairs of short tertiary branchlets along each secondary bough fork
        for sub_k in 0..3 {
            let st = 0.26 + 0.48 * (sub_k as f32 / 2.0);
            let sp = p.lerp(tip, st);
            let slen = lerp(0.16, 0.09, st) * rng.variance_mul(0.10);
            for sside in [-1.0f32, 1.0] {
                let sang = ang + sside * 0.68 + rng.range(-0.06, 0.06);
                let sdir = Vec2::new(sang.sin(), -sang.cos());
                let stip = (sp + sdir * slen).clamp(Vec2::splat(0.05), Vec2::splat(0.95));
                twigs.push((sp, sp.lerp(stip, 0.72), 0.005, 0.0025));
                branches.push((sp, stip, (fork_t + st * 0.4).min(1.0)));
            }
        }
    }

    // Short side branchlets along the upper half of the central leader
    for k in 0..4 {
        let t = 0.56 + 0.28 * (k as f32 / 3.0);
        let p = root.lerp(apex, t);
        let len = lerp(0.20, 0.09, (t - 0.56) / 0.28) * rng.variance_mul(0.10);
        for side in [-1.0f32, 1.0] {
            let ang = side * 0.65 + rng.range(-0.05, 0.05);
            let dir = Vec2::new(ang.sin(), -ang.cos());
            let tip = (p + dir * len).clamp(Vec2::splat(0.05), Vec2::splat(0.95));
            twigs.push((p, p.lerp(tip, 0.74), 0.005, 0.0025));
            branches.push((p, tip, t));
        }
    }

    // Two-layer dense spruce/fir needles: dark shadowed interior + bright sunlit new-growth tips
    for layer in 0..2 {
        let top = layer == 1;
        for &(a, b, branch_t) in &branches {
            let len = (b - a).length();
            let dir = (b - a) / len.max(1e-4);
            let perp = Vec2::new(-dir.y, dir.x);
            let steps = ((len / 0.0115).ceil() as usize).max(4);
            for i in 0..=steps {
                let t = i as f32 / steps as f32;
                let p = a.lerp(b, t);
                let needle_len = rng.range(0.062, 0.094) * lerp(1.06, 0.68, t);
                for side in [-1.0f32, 1.0] {
                    let sweep = if top {
                        rng.range(0.28, 0.64)
                    } else {
                        rng.range(0.50, 0.96)
                    };
                    let nd = (dir * sweep.cos() + perp * side * sweep.sin()).normalize();
                    let end = (p + nd * needle_len).clamp(Vec2::splat(0.025), Vec2::splat(0.975));
                    let growth_prog = (t * 0.62 + branch_t * 0.38).clamp(0.0, 1.0);
                    let tip_bright = lerp(0.72, 1.32, growth_prog.powf(0.85));
                    let shade = tip_bright
                        * if top {
                            rng.range(0.98, 1.18)
                        } else {
                            rng.range(0.58, 0.82)
                        };
                    let hue = if top {
                        lerp(-0.08, 0.52, growth_prog)
                    } else {
                        lerp(-0.36, -0.04, growth_prog)
                    };
                    let w0 = if top { 0.0054 } else { 0.0070 };
                    needles.push((p, end, w0, 0.0015, shade, hue));
                }
            }
            // Sunlit spring-bud needle rosette capping each spruce branchlet tip
            for fan in [-0.36f32, -0.18, 0.0, 0.18, 0.36] {
                let ang = dir.y.atan2(dir.x) + fan;
                let nd = Vec2::new(ang.cos(), ang.sin());
                let end = (b + nd * 0.060).clamp(Vec2::splat(0.025), Vec2::splat(0.975));
                needles.push((
                    b - dir * 0.014,
                    end,
                    0.0052,
                    0.0014,
                    rng.range(1.12, 1.34),
                    0.46,
                ));
            }
        }
    }

    rasterize_twig_and_needle_strokes(&twigs, &needles, base, twig_rgb, res)
}

fn rasterize_twig_and_needle_strokes(
    twigs: &[(Vec2, Vec2, f32, f32)],
    needles: &[(Vec2, Vec2, f32, f32, f32, f32)],
    base: [f32; 3],
    twig_rgb: [f32; 3],
    res: u32,
) -> RgbaTexture {
    let mut tex = RgbaTexture::new(res, res);
    let px = 1.0 / res as f32;

    // Bin strokes by horizontal row bands (16 bands) so 512x512 rasterization stays sub-10ms!
    const BANDS: usize = 16;
    let band_of = |y: f32| ((y.clamp(0.0, 0.9999) * BANDS as f32) as usize).min(BANDS - 1);
    let mut needle_bins: [Vec<usize>; BANDS] = std::array::from_fn(|_| Vec::new());
    for (idx, &(a, b, w0, w1, _, _)) in needles.iter().enumerate() {
        let pad = w0.max(w1) + px * 1.5;
        let b_lo = band_of((a.y.min(b.y) - pad).max(0.0));
        let b_hi = band_of((a.y.max(b.y) + pad).min(1.0));
        for bin in &mut needle_bins[b_lo..=b_hi] {
            bin.push(idx);
        }
    }

    for y in 0..res {
        let v = (y as f32 + 0.5) * px;
        let bin = &needle_bins[band_of(v)];
        for x in 0..res {
            let q = Vec2::new((x as f32 + 0.5) * px, v);
            let mut acc = [0.0f32; 4];

            for &(a, b, w0, w1) in twigs {
                let w_max = w0.max(w1) + px;
                let lo = a.min(b) - Vec2::splat(w_max);
                let hi = a.max(b) + Vec2::splat(w_max);
                if q.x < lo.x || q.y < lo.y || q.x > hi.x || q.y > hi.y {
                    continue;
                }
                let ab = b - a;
                let t = ((q - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
                let w = lerp(w0, w1, t);
                let d = (q - (a + ab * t)).length();
                let alpha = ((w - d) / px + 0.5).clamp(0.0, 1.0);
                if alpha > 0.0 {
                    over(&mut acc, [twig_rgb[0], twig_rgb[1], twig_rgb[2], alpha]);
                }
            }

            for &idx in bin {
                let (a, b, w0, w1, shade, hue) = needles[idx];
                let w_max = w0.max(w1) + px;
                let lo_x = a.x.min(b.x) - w_max;
                let hi_x = a.x.max(b.x) + w_max;
                if q.x < lo_x || q.x > hi_x {
                    continue;
                }
                let ab = b - a;
                let t = ((q - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
                let w = lerp(w0, w1, t);
                let d = (q - (a + ab * t)).length();
                let alpha = ((w - d) / px + 0.5).clamp(0.0, 1.0);
                if alpha <= 0.0 {
                    continue;
                }
                let ridge = 0.90 + 0.16 * (1.0 - (d / w.max(1e-4)).clamp(0.0, 1.0));
                let s = shade * ridge * lerp(0.84, 1.12, t);
                let warm = [1.0 + 0.12 * hue, 1.0 + 0.05 * hue, 1.0 - 0.14 * hue];
                let c = [
                    (base[0] * s * warm[0]).clamp(0.0, 1.0),
                    (base[1] * s * warm[1]).clamp(0.0, 1.0),
                    (base[2] * s * warm[2]).clamp(0.0, 1.0),
                ];
                over(&mut acc, [c[0], c[1], c[2], alpha]);
            }

            tex.set(
                x,
                y,
                [
                    (acc[0].clamp(0.0, 1.0) * 255.0) as u8,
                    (acc[1].clamp(0.0, 1.0) * 255.0) as u8,
                    (acc[2].clamp(0.0, 1.0) * 255.0) as u8,
                    (acc[3].clamp(0.0, 1.0) * 255.0) as u8,
                ],
            );
        }
    }
    tex.dilate_alpha_rgb(12);
    tex
}

/// Alpha-over composite: `dst` = `src` over `dst` (all channels 0..1).
fn over(dst: &mut [f32; 4], src: [f32; 4]) {
    let sa = src[3];
    let da = dst[3];
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        return;
    }
    for ch in 0..3 {
        dst[ch] = (src[ch] * sa + dst[ch] * da * (1.0 - sa)) / out_a;
    }
    dst[3] = out_a;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::{LeafCardLayout, TextureParams};

    fn test_species(toml_extra: &str) -> Species {
        let toml = format!(
            r#"
[species]
name = "Texture Test"

[trunk]
height = 5.0
radius = 0.3

{toml_extra}
"#
        );
        Species::from_toml(&toml).unwrap()
    }

    #[test]
    fn generated_maps_are_deterministic() {
        let species = test_species("");
        let a = TextureSet::generate(&species);
        let b = TextureSet::generate(&species);
        assert_eq!(a.bark_albedo, b.bark_albedo);
        assert_eq!(a.bark_normal, b.bark_normal);
        assert_eq!(a.leaf_card, b.leaf_card);
        // PNG encoding is deterministic too.
        assert_eq!(a.leaf_card.to_png().unwrap(), b.leaf_card.to_png().unwrap());
    }

    #[test]
    fn different_species_names_get_different_maps() {
        let oak = test_species("");
        let other = test_species("").clone();
        let mut renamed = other;
        renamed.species.name = "Other".to_string();
        assert_ne!(
            TextureSet::generate(&oak).leaf_card,
            TextureSet::generate(&renamed).leaf_card
        );
    }

    #[test]
    fn leaf_card_has_alpha_coverage() {
        let species = test_species("");
        let set = TextureSet::generate(&species);
        let coverage = set.leaf_card.alpha_coverage();
        assert!(
            (0.08..0.85).contains(&coverage),
            "cluster card coverage {coverage} outside expected range"
        );
        // Transparent border and opaque interior both exist.
        assert_eq!(set.leaf_card.get(0, 0)[3], 0);
    }

    #[test]
    fn single_layout_is_sparser_than_cluster() {
        let single = test_species(
            r#"
[textures]
leaf_card = "single"
"#,
        );
        let cluster = test_species("");
        let a = TextureSet::generate(&single).leaf_card.alpha_coverage();
        let b = TextureSet::generate(&cluster).leaf_card.alpha_coverage();
        // One leaf silhouette is sparser than a spray of many.
        assert!(a < b, "single {a} should be sparser than cluster {b}");
        assert!(a > 0.03, "single {a} should still be visible");
    }

    #[test]
    fn bark_height_tiles_horizontally() {
        // h(u=0) must equal h(u=1): periodic lattice makes the wrap seamless.
        for &style in &[BarkStyle::Furrowed, BarkStyle::Plated, BarkStyle::Smooth] {
            for i in 0..16 {
                let v = i as f32 / 16.0;
                let a = bark_height_at(style, 0.0, v, 42);
                let b = bark_height_at(style, 1.0, v, 42);
                assert!(
                    (a - b).abs() < 1e-5,
                    "{style:?} does not tile at v={v}: {a} vs {b}"
                );
            }
        }
    }

    #[test]
    fn bark_height_tiles_vertically() {
        for &style in &[BarkStyle::Furrowed, BarkStyle::Plated, BarkStyle::Smooth] {
            for i in 0..16 {
                let u = i as f32 / 16.0;
                let a = bark_height_at(style, u, 0.0, 42);
                let b = bark_height_at(style, u, 1.0, 42);
                assert!(
                    (a - b).abs() < 1e-5,
                    "{style:?} does not tile vertically at u={u}"
                );
            }
        }
    }

    #[test]
    fn normal_map_is_normalized_and_opengl() {
        let species = test_species("");
        let set = TextureSet::generate(&species);
        // Center of a 512 map: decode and check the normal is unit-ish and z > 0.
        let px = set.bark_normal.get(256, 256);
        let n = Vec3::new(
            px[0] as f32 / 255.0 * 2.0 - 1.0,
            px[1] as f32 / 255.0 * 2.0 - 1.0,
            px[2] as f32 / 255.0 * 2.0 - 1.0,
        );
        assert!((n.length() - 1.0).abs() < 0.05);
        assert!(n.z > 0.5);
    }

    #[test]
    fn png_roundtrip_preserves_pixels() {
        let species = test_species("");
        let set = TextureSet::generate(&species);
        let png = set.leaf_card.to_png().unwrap();
        let decoded = RgbaTexture::from_image_bytes(&png).unwrap();
        assert_eq!(decoded, set.leaf_card);
    }

    #[test]
    fn texture_seed_prefers_explicit_value() {
        let with_seed = test_species(
            r#"
[textures]
seed = 99
"#,
        );
        assert_eq!(species_texture_seed(&with_seed), 99);
        // Name-derived seeds differ across names.
        let a = test_species("");
        let mut b = test_species("");
        b.species.name = "B".to_string();
        assert_ne!(species_texture_seed(&a), species_texture_seed(&b));
    }

    #[test]
    fn texture_params_parse_extended_fields() {
        let species = test_species(
            r#"
[textures]
resolution = 256
seed = 7
bark_style = "plated"
leaf_shape = "lobed"
leaf_card = "single"
bark_color = [0.4, 0.3, 0.2]
leaf_color = [0.3, 0.5, 0.2]
bark_albedo = "maps/bark.png"
"#,
        );
        let t: &TextureParams = &species.textures;
        assert_eq!(t.resolution, 256);
        assert_eq!(t.seed, Some(7));
        assert_eq!(t.bark_style, BarkStyle::Plated);
        assert_eq!(t.leaf_shape, LeafShape::OakLobed);
        assert_eq!(t.leaf_card, LeafCardLayout::Single);
        assert_eq!(t.bark_color, Some([0.4, 0.3, 0.2]));
        assert_eq!(t.bark_albedo, "maps/bark.png");
    }

    #[test]
    fn layered_bark_styles_generate_distinct_tileable_maps() {
        let mut seen = Vec::new();
        for style in ["stringy", "tessellated", "patchy", "papery"] {
            let species = test_species(&format!(
                "[textures]\nresolution = 64\nbark_style = \"{style}\"\nbark_color = [0.5, 0.45, 0.4]\n"
            ));
            let set = TextureSet::generate(&species);
            assert_eq!(set.bark_albedo.width, 64);
            assert!(set.bark_albedo.pixels.chunks(4).all(|p| p[3] == 255));
            assert!(
                !seen.contains(&set.bark_albedo.pixels),
                "{style} repeats another style"
            );
            seen.push(set.bark_albedo.pixels.clone());
        }
    }

    #[test]
    fn patch_colours_and_kino_recolour_the_bark() {
        let base =
            "[textures]\nresolution = 64\nbark_style = \"patchy\"\nbark_color = [0.8, 0.8, 0.8]\n";
        let plain = TextureSet::generate(&test_species(base));
        let patched = TextureSet::generate(&test_species(&format!(
            "{base}bark_patch_colors = [[0.9, 0.4, 0.1]]\n"
        )));
        let kino = TextureSet::generate(&test_species(&format!("{base}bark_kino = 1.0\n")));
        assert_ne!(plain.bark_albedo.pixels, patched.bark_albedo.pixels);
        assert_ne!(plain.bark_albedo.pixels, kino.bark_albedo.pixels);
        // The relief (and so the normal map) is untouched by colour layers.
        assert_eq!(plain.bark_normal.pixels, patched.bark_normal.pixels);
        assert_eq!(plain.bark_normal.pixels, kino.bark_normal.pixels);
        // Orange patches push red above blue on a meaningful share of texels.
        let warm = patched
            .bark_albedo
            .pixels
            .chunks(4)
            .filter(|p| p[0] as i32 - p[2] as i32 > 60)
            .count();
        assert!(warm > 64 * 64 / 20, "only {warm} orange texels");
        // Patch colours are ignored by styles that have no patches.
        let furrowed = "[textures]\nresolution = 64\nbark_style = \"furrowed\"\n";
        let a = TextureSet::generate(&test_species(furrowed));
        let b = TextureSet::generate(&test_species(&format!(
            "{furrowed}bark_patch_colors = [[0.9, 0.4, 0.1]]\n"
        )));
        assert_eq!(a.bark_albedo.pixels, b.bark_albedo.pixels);
    }

    #[test]
    fn foliage_atlas_is_used_only_when_a_species_needs_it() {
        let plain = test_species("[textures]\nresolution = 64\n");
        assert!(!plain.uses_foliage_atlas());
        let with_organs =
            test_species("[textures]\nresolution = 64\n[inflorescence]\nkind = \"spike\"\n");
        assert!(with_organs.uses_foliage_atlas());
        let a = TextureSet::generate(&plain);
        let b = TextureSet::generate(&with_organs);
        assert_eq!((b.leaf_card.width, b.leaf_card.height), (64, 64));
        assert_ne!(a.leaf_card.pixels, b.leaf_card.pixels);
        // The organ cell (top right) is an opaque wrap; the plain card is not opaque there.
        assert_eq!(b.leaf_card.get(48, 16)[3], 255);
    }
}
