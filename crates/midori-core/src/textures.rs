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
        let height = bark_heightfield(params.bark_style, seed, res);
        Self {
            bark_albedo: bark_albedo_from_height(&height, res, params.bark_color, seed),
            bark_normal: normal_from_height(&height, res),
            leaf_card: leaf_card(
                params.leaf_shape,
                params.leaf_card,
                params.leaf_color,
                seed,
                res,
            ),
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

/// Hash a lattice cell to [0, 1). Deterministic across platforms.
fn lattice(seed: u64, ix: i64, iy: i64) -> f32 {
    let mut z = seed
        .wrapping_add((ix as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add((iy as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    ((z >> 40) as f32) / (1u64 << 24) as f32
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Value noise on a wrapping lattice. Sampling `u*px`/`v*py` over u,v in
/// [0,1) produces a texture that tiles seamlessly.
fn value_noise(seed: u64, x: f32, y: f32, px: i64, py: i64) -> f32 {
    let ix = x.floor() as i64;
    let iy = y.floor() as i64;
    let fx = smooth(x - ix as f32);
    let fy = smooth(y - iy as f32);
    let v00 = lattice(seed, ix.rem_euclid(px), iy.rem_euclid(py));
    let v10 = lattice(seed, (ix + 1).rem_euclid(px), iy.rem_euclid(py));
    let v01 = lattice(seed, ix.rem_euclid(px), (iy + 1).rem_euclid(py));
    let v11 = lattice(seed, (ix + 1).rem_euclid(px), (iy + 1).rem_euclid(py));
    lerp(lerp(v00, v10, fx), lerp(v01, v11, fx), fy)
}

/// Fractional Brownian motion; periods double with frequency so the result
/// stays periodic over [0, 1).
fn fbm(seed: u64, x: f32, y: f32, px: i64, py: i64, octaves: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut norm = 0.0;
    let mut f: i64 = 1;
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

/// Periodic Worley F1 distance (scaled by `px`/`py` cells per side).
/// Returns (distance to nearest feature point, cell hash of that point).
fn voronoi(seed: u64, x: f32, y: f32, px: i64, py: i64) -> (f32, f32) {
    let ix = x.floor() as i64;
    let iy = y.floor() as i64;
    let fx = x - ix as f32;
    let fy = y - iy as f32;
    let mut best = f32::MAX;
    let mut cell = 0.0;
    for dy in -1..=1_i64 {
        for dx in -1..=1_i64 {
            let cx = ix + dx;
            let cy = iy + dy;
            let wx = cx.rem_euclid(px);
            let wy = cy.rem_euclid(py);
            let jx = lattice(seed, wx, wy);
            let jy = lattice(seed ^ 0x5EED_5EED, wx, wy);
            let ox = dx as f32 + jx - fx;
            let oy = dy as f32 + jy - fy;
            let d2 = ox * ox + oy * oy;
            if d2 < best {
                best = d2;
                cell = lattice(seed ^ 0xCE11_5EED, wx, wy);
            }
        }
    }
    (best.sqrt(), cell)
}

// ---------------------------------------------------------------------------
// Bark
// ---------------------------------------------------------------------------

/// Tileable bark heightfield in [0,1], evaluated per texel.
fn bark_height_at(style: BarkStyle, u: f32, v: f32, seed: u64) -> f32 {
    match style {
        BarkStyle::Furrowed => {
            // Braided anastomosing oak/willow fissures with organic meander
            let meander = fbm(seed ^ 0xB1, u * 4.0, v * 3.0, 4, 3, 3) - 0.5;
            let uu = u + meander * 0.085;
            // Primary deep vertical ridges + secondary finer sub-ridges
            let n1 = fbm(seed ^ 0xB2, uu * 18.0, v * 3.0, 18, 3, 4);
            let n2 = fbm(seed ^ 0xBA, uu * 36.0, v * 8.0, 36, 8, 3);
            let ridge1 = (1.0 - (2.0 * n1 - 1.0).abs()).powf(1.35);
            let ridge2 = (1.0 - (2.0 * n2 - 1.0).abs()).powf(1.6);
            let mut h = ridge1 * 0.76 + ridge2 * 0.24;
            // Diagonal/transverse bark block cracks
            let crack = fbm(seed ^ 0xB3, uu * 18.0, v * 12.0, 18, 12, 2);
            if crack < 0.36 {
                h *= 1.0 - (0.36 - crack) * 1.65;
            }
            let fiber = fbm(seed ^ 0xBB, u * 64.0, v * 16.0, 64, 16, 2);
            (h * (0.86 + 0.24 * fiber)).clamp(0.0, 1.0)
        }
        BarkStyle::Plated => {
            // Vertically elongated scaly pine plates (4.5:1 vertical aspect) with flat flaky
            // plate faces separated by dark anastomosing vertical fissures.
            let warp = (fbm(seed ^ 0xBC, u * 6.0, v * 3.0, 6, 3, 2) - 0.5) * 0.045;
            let (d1, cell1) = voronoi(seed ^ 0xB4, (u + warp) * 18.0, v * 4.0, 18, 4);
            let (d2, cell2) = voronoi(seed ^ 0xBD, (u - warp) * 32.0, v * 8.0, 32, 8);
            // Flat-topped terraced bark scale rather than rounded polka-dot pillows
            let edge1 = ((1.0 - d1 * 1.42) / 0.36).clamp(0.0, 1.0);
            let plate1 = edge1 * edge1 * (3.0 - 2.0 * edge1);
            let edge2 = ((1.0 - d2 * 1.50) / 0.42).clamp(0.0, 1.0);
            let plate2 = edge2 * edge2 * (3.0 - 2.0 * edge2);
            let flake = fbm(seed ^ 0xB5, u * 48.0, v * 16.0, 48, 16, 3);
            let combined =
                plate1 * (0.64 + 0.36 * cell1) * 0.76 + plate2 * (0.55 + 0.45 * cell2) * 0.24;
            (combined * (0.84 + 0.32 * flake)).clamp(0.0, 1.0)
        }
        BarkStyle::Smooth => {
            // Smooth palm/beech bark with organic ring leaf-scars and fine vertical fiber
            let base = fbm(seed ^ 0xB6, u * 6.0, v * 4.0, 6, 4, 4);
            let ring_warp = (fbm(seed ^ 0xBE, u * 4.0, v * 8.0, 4, 8, 3) - 0.5) * 0.32;
            let ring_wave = ((v * 18.0 + ring_warp) * crate::constants::TAU).cos();
            let ring_groove = (1.0 - ring_wave.abs()).powf(3.0);
            let fiber = fbm(seed ^ 0xBF, u * 64.0, v * 10.0, 64, 10, 3);
            let lenticel = fbm(seed ^ 0xB7, u * 16.0, v * 64.0, 16, 64, 2);
            let mut h = 0.40 + 0.32 * base - 0.16 * ring_groove + 0.16 * fiber;
            if lenticel > 0.74 {
                h -= (lenticel - 0.74) * 0.45;
            }
            h.clamp(0.0, 1.0)
        }
    }
}

/// Evaluate the bark heightfield over the whole texture.
fn bark_heightfield(style: BarkStyle, seed: u64, res: u32) -> Vec<f32> {
    let mut h = vec![0.0; (res * res) as usize];
    for y in 0..res {
        for x in 0..res {
            let u = (x as f32 + 0.5) / res as f32;
            let v = (y as f32 + 0.5) / res as f32;
            h[(y * res + x) as usize] = bark_height_at(style, u, v, seed);
        }
    }
    h
}

/// Albedo: rich multi-stop palette ramp driven by height, bark grain, and subtle
/// warm inner-bark crevices / cool weathered ridge highlights.
fn bark_albedo_from_height(
    height: &[f32],
    res: u32,
    color: Option<[f32; 3]>,
    seed: u64,
) -> RgbaTexture {
    let base = color.unwrap_or([0.36, 0.28, 0.20]);
    let crevice = [base[0] * 0.34, base[1] * 0.30, base[2] * 0.28];
    let mid = [(base[0] * 1.04).min(1.0), base[1] * 0.96, base[2] * 0.88];
    let ridge = [
        (base[0] * 1.28 + 0.03).min(1.0),
        (base[1] * 1.26 + 0.03).min(1.0),
        (base[2] * 1.22 + 0.02).min(1.0),
    ];
    let mut tex = RgbaTexture::new(res, res);
    for y in 0..res {
        for x in 0..res {
            let i = (y * res + x) as usize;
            let u = (x as f32 + 0.5) / res as f32;
            let v = (y as f32 + 0.5) / res as f32;
            let grain = fbm(seed ^ 0xA9, u * 64.0, v * 20.0, 64, 20, 3);
            let patch = fbm(seed ^ 0xAA, u * 6.0, v * 6.0, 6, 6, 3);
            let t = (height[i] * 0.78 + grain * 0.22).clamp(0.0, 1.0);
            let mut rgb = if t < 0.5 {
                let k = t * 2.0;
                [
                    lerp(crevice[0], mid[0], k),
                    lerp(crevice[1], mid[1], k),
                    lerp(crevice[2], mid[2], k),
                ]
            } else {
                let k = (t - 0.5) * 2.0;
                [
                    lerp(mid[0], ridge[0], k),
                    lerp(mid[1], ridge[1], k),
                    lerp(mid[2], ridge[2], k),
                ]
            };
            // Subtle weathered grey-green lichen/moss tint on occasional high bark patches
            if patch > 0.70 && height[i] > 0.58 {
                let m = ((patch - 0.70) / 0.30 * 0.28).clamp(0.0, 0.25);
                rgb[0] = lerp(rgb[0], 0.42, m);
                rgb[1] = lerp(rgb[1], 0.47, m);
                rgb[2] = lerp(rgb[2], 0.36, m);
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

/// Tangent-space normal map from a heightfield (Sobel, OpenGL +Y — the glTF
/// convention, matching what glTF viewers sample).
fn normal_from_height(height: &[f32], res: u32) -> RgbaTexture {
    let mut tex = RgbaTexture::new(res, res);
    let n = res as i64;
    // Height deltas shrink with resolution; scale so relief stays visible.
    let strength = res as f32 / 128.0 * 2.0;
    for y in 0..res {
        for x in 0..res {
            let xl = (((x as i64 - 1).rem_euclid(n)) + y as i64 * n) as usize;
            let xr = (((x as i64 + 1).rem_euclid(n)) + y as i64 * n) as usize;
            let yu = ((x as i64) + (y as i64 - 1).rem_euclid(n) * n) as usize;
            let yd = ((x as i64) + (y as i64 + 1).rem_euclid(n) * n) as usize;
            let dx = (height[xr] - height[xl]) * 0.5 * strength;
            let dy = (height[yd] - height[yu]) * 0.5 * strength;
            // OpenGL +Y: n = normalize(-dh/du, +dh/dv, 1).
            let nrm = Vec3::new(-dx, dy, 1.0).normalize();
            tex.set(
                x,
                y,
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
            // Curved main woody twig + connected sub-petioles so zero leaves float in mid-air
            let ctrl_x = 0.5 + rng.range(-0.08, 0.08);
            let top_x = 0.5 + rng.range(-0.06, 0.06);
            let p0 = Vec2::new(0.5, 0.985);
            let p1 = Vec2::new(ctrl_x, 0.56);
            let p2 = Vec2::new(top_x, 0.22);

            let mut twigs = Vec::new();
            let segs = 8;
            for s in 0..segs {
                let t0 = s as f32 / segs as f32;
                let t1 = (s + 1) as f32 / segs as f32;
                twigs.push(TwigStroke {
                    a: quad_bezier(p0, p1, p2, t0),
                    b: quad_bezier(p0, p1, p2, t1),
                    w_a: lerp(0.011, 0.004, t0),
                    w_b: lerp(0.011, 0.004, t1),
                });
            }

            let count = match shape {
                LeafShape::Willow => 16 + rng.index(4),
                _ => 11 + rng.index(4),
            };
            let mut leaves = Vec::with_capacity(count + 3);

            for i in 0..count {
                let t = i as f32 / (count.saturating_sub(1).max(1)) as f32;
                let side = if i % 2 == 0 { 1.0 } else { -1.0 };
                let along = (0.10 + t * 0.84 + rng.range(-0.02, 0.02)).clamp(0.06, 0.96);
                let node = quad_bezier(p0, p1, p2, along);
                let tangent = (quad_bezier(p0, p1, p2, (along + 0.03).min(1.0))
                    - quad_bezier(p0, p1, p2, (along - 0.03).max(0.0)))
                .try_normalize()
                .unwrap_or(Vec2::new(0.0, -1.0));
                let normal = Vec2::new(-tangent.y * side, tangent.x * side);

                // Sweep forward and broadly outward so sprays fill a natural botanical fan
                let sweep = lerp(1.14, 0.58, t) + rng.range(-0.14, 0.14);
                let dir = (tangent * sweep.cos() + normal * sweep.sin()).normalize();
                let leaf_scale = match shape {
                    LeafShape::Willow => lerp(0.38, 0.25, t) * rng.variance_mul(0.12),
                    _ => lerp(0.35, 0.22, t) * rng.variance_mul(0.14),
                };
                let petiole_len = lerp(0.085, 0.035, t) * rng.variance_mul(0.15);
                let leaf_base = node + dir * petiole_len;
                let center = leaf_base + dir * (leaf_scale * 0.44);

                twigs.push(TwigStroke {
                    a: node,
                    b: leaf_base + dir * (leaf_scale * 0.12),
                    w_a: lerp(0.0055, 0.0035, t),
                    w_b: 0.0025,
                });

                let tip_angle = dir.y.atan2(dir.x);
                leaves.push(CardLeaf {
                    center,
                    rotation: crate::constants::PI / 2.0 - tip_angle,
                    scale: leaf_scale,
                    jitter: lerp(0.78, 1.12, t) * rng.range(0.90, 1.08),
                    hue: lerp(-0.35, 0.45, t) + rng.range(-0.25, 0.25),
                });
            }

            // Terminal rosette of 3 overlapping apical leaves crowning the twig tip
            let apex = p2;
            let apex_tan = (p2 - p1).try_normalize().unwrap_or(Vec2::new(0.0, -1.0));
            for fan in [-0.46f32, 0.44, 0.0] {
                let ang = apex_tan.y.atan2(apex_tan.x) + fan + rng.range(-0.08, 0.08);
                let dir = Vec2::new(ang.cos(), ang.sin());
                let leaf_scale =
                    (if fan.abs() < 0.1 { 0.25 } else { 0.22 }) * rng.variance_mul(0.1);
                let petiole = 0.025;
                let leaf_base = apex + dir * petiole;
                twigs.push(TwigStroke {
                    a: apex,
                    b: leaf_base + dir * 0.03,
                    w_a: 0.004,
                    w_b: 0.0025,
                });
                leaves.push(CardLeaf {
                    center: leaf_base + dir * (leaf_scale * 0.44),
                    rotation: crate::constants::PI / 2.0 - ang,
                    scale: leaf_scale,
                    jitter: rng.range(0.98, 1.14),
                    hue: rng.range(0.05, 0.55),
                });
            }
            (leaves, twigs)
        }
        LeafCardLayout::Puff => {
            // Irregular multi-clump broadleaf bouquet: 5 offset sub-clusters with scalloped
            // perimeter and directional sun-top / cool-shadow-base shading (NO center bullseye!).
            let sub_clumps: Vec<(Vec2, f32, f32)> = {
                let mut v = Vec::with_capacity(6);
                let n = 5;
                for k in 0..n {
                    let a = k as f32 / n as f32 * crate::constants::TAU + rng.range(-0.35, 0.35);
                    let dist = rng.range(0.11, 0.18);
                    let c = Vec2::new(0.5 + dist * a.cos(), 0.52 + dist * a.sin() * 0.92);
                    // Sunlit bias toward upper sub-clumps (-y in UV is top)
                    let sun = lerp(1.06, 0.84, ((c.y - 0.30) / 0.45).clamp(0.0, 1.0));
                    v.push((c, rng.range(0.22, 0.27), sun));
                }
                v.push((Vec2::new(0.5, 0.50), 0.23, 0.96));
                v
            };

            let count = 150 + rng.index(25);
            let mut leaves = Vec::with_capacity(count);
            for i in 0..count {
                let layer = i as f32 / (count - 1) as f32;
                let (sc, sr, sun) = sub_clumps[rng.index(sub_clumps.len())];
                let r = sr * lerp(1.0, 0.55, layer) * rng.next_f32().sqrt();
                let a = rng.range(-crate::constants::PI, crate::constants::PI);
                let center = (sc + Vec2::new(r * a.cos(), r * a.sin()))
                    .clamp(Vec2::splat(0.11), Vec2::splat(0.89));
                let out = (center - sc)
                    .try_normalize()
                    .unwrap_or(Vec2::new(0.0, -1.0));
                let tip_angle = out.y.atan2(out.x) + rng.range(-0.75, 0.75);
                leaves.push(CardLeaf {
                    center,
                    rotation: crate::constants::PI / 2.0 - tip_angle,
                    scale: lerp(0.16, 0.105, layer) * rng.variance_mul(0.22),
                    jitter: sun * lerp(0.76, 1.06, layer) * rng.range(0.90, 1.08),
                    hue: lerp(-0.35, 0.35, layer) + rng.range(-0.35, 0.35),
                });
            }
            (leaves, Vec::new())
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
        LeafCardLayout::Frond => (Vec::new(), Vec::new()),
    }
}

/// Render the leaf card: woody twigs + petioles underleaf, then leaves back-to-front
/// with botanical venation, soft anti-aliased SDF edges, and RGB edge dilation.
fn leaf_card(
    shape: LeafShape,
    layout: LeafCardLayout,
    color: Option<[f32; 3]>,
    seed: u64,
    res: u32,
) -> RgbaTexture {
    let mut rng = Rng::from_seed(seed ^ 0x1EAF_CA4D);
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
    let stem_rgb = [0.31, 0.23, 0.14];
    let detailed_veins = matches!(layout, LeafCardLayout::Single | LeafCardLayout::Cluster);

    // Precompute per-leaf bounding radius in UV space so every layout skips distant pixels fast
    let leaf_bounds: Vec<(f32, f32, f32)> = leaves
        .iter()
        .map(|leaf| {
            let (s, c) = leaf.rotation.sin_cos();
            (s, c, (leaf.scale * 0.72).powi(2))
        })
        .collect();

    let mut tex = RgbaTexture::new(res, res);
    let px = 1.0 / res as f32;
    for y in 0..res {
        for x in 0..res {
            let u = (x as f32 + 0.5) * px;
            let v = (y as f32 + 0.5) * px;
            let q = Vec2::new(u, v);
            let mut acc = [0.0f32; 4];

            // Woody twig & sub-petioles underneath leaf blades
            for stroke in &twigs {
                let w_max = stroke.w_a.max(stroke.w_b) + px;
                let lo = stroke.a.min(stroke.b) - Vec2::splat(w_max);
                let hi = stroke.a.max(stroke.b) + Vec2::splat(w_max);
                if q.x < lo.x || q.y < lo.y || q.x > hi.x || q.y > hi.y {
                    continue;
                }
                let ab = stroke.b - stroke.a;
                let t = ((q - stroke.a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
                let w = lerp(stroke.w_a, stroke.w_b, t);
                let d = (q - (stroke.a + ab * t)).length();
                let a = ((w - d) / px + 0.5).clamp(0.0, 1.0);
                if a > 0.0 {
                    let cyl = 0.80 + 0.35 * (1.0 - (d / w.max(1e-4)).clamp(0.0, 1.0));
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

            let mottle = fbm(seed ^ 0xC0, u * 8.0, v * 8.0, 8, 8, 2);
            for (leaf, &(sin, cos, max_r2)) in leaves.iter().zip(&leaf_bounds) {
                let p = q - leaf.center;
                if p.length_squared() > max_r2 {
                    continue;
                }
                // Leaf-local space: +y is tip, -y is petiole base
                let local = Vec2::new(p.x * cos - p.y * sin, -(p.x * sin + p.y * cos)) / leaf.scale;
                let sdf = leaf_sdf(local, shape);
                let edge = (1.6 / (res as f32 * leaf.scale.max(0.02))).max(0.008);
                let alpha = (0.5 - sdf / edge).clamp(0.0, 1.0);
                if alpha <= 0.0 {
                    continue;
                }

                // Natural lamina shading: base-to-tip translucency + gentle lateral convexity
                let along = (local.y + 0.5).clamp(0.0, 1.0);
                let mut shade = leaf.jitter * (0.84 + 0.24 * along) * (0.92 + 0.16 * mottle);
                let mut vein_tint = [1.0f32, 1.0, 1.0];

                if detailed_veins {
                    // Tapered pale yellow-green midrib + herringbone secondary veins
                    let midrib_w = lerp(0.015, 0.003, along) * (1.0 - along * 0.25);
                    if local.y < 0.44 && local.x.abs() < midrib_w {
                        let k = 1.0 - local.x.abs() / midrib_w.max(1e-4);
                        shade *= 1.0 + 0.14 * k;
                        vein_tint = [1.0 + 0.12 * k, 1.0 + 0.08 * k, 1.0 - 0.10 * k];
                    } else if shape != LeafShape::Needle {
                        let sec = ((local.y - local.x.abs() * 0.58) * 11.0).fract();
                        let sec_line = (1.0 - (sec - 0.5).abs() * 9.0).clamp(0.0, 1.0)
                            * (1.0 - (local.x.abs() * 2.8).clamp(0.0, 1.0));
                        shade *= 1.0 + 0.06 * sec_line;
                    }
                    // Subtle sunlit rim translucency near blade margin
                    let rim = (1.0 + sdf / 0.06).clamp(0.0, 1.0);
                    shade *= 1.0 + 0.07 * rim;
                }

                let warm = [
                    (1.0 + 0.11 * leaf.hue) * vein_tint[0],
                    (1.0 + 0.04 * leaf.hue) * vein_tint[1],
                    (1.0 - 0.15 * leaf.hue) * vein_tint[2],
                ];
                let c = [
                    (base[0] * shade * warm[0]).clamp(0.0, 1.0),
                    (base[1] * shade * warm[1]).clamp(0.0, 1.0),
                    (base[2] * shade * warm[2]).clamp(0.0, 1.0),
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
    tex.dilate_alpha_rgb(6);
    tex
}

/// Pine / Joshua needle fascicle shoot card: scaly woody shoot with 2-3 lateral forks
/// clothed in radiating bundles of slender tapered needles.
fn needle_cluster_card(color: Option<[f32; 3]>, rng: &mut Rng, res: u32) -> RgbaTexture {
    let base = color.unwrap_or([0.16, 0.34, 0.16]);
    let twig_rgb = [0.34, 0.22, 0.13];
    // (a, b, w_a, w_b, shade, hue, is_needle)
    let mut twigs: Vec<(Vec2, Vec2, f32, f32)> = Vec::new();
    let mut needles: Vec<(Vec2, Vec2, f32, f32, f32, f32)> = Vec::new();

    let p0 = Vec2::new(0.5, 0.97);
    let p1 = Vec2::new(0.5 + rng.range(-0.05, 0.05), 0.56);
    let p2 = Vec2::new(0.5 + rng.range(-0.04, 0.04), 0.24);
    let mut shoots: Vec<(Vec2, Vec2, f32)> = Vec::new();
    for s in 0..5 {
        let t0 = s as f32 / 5.0;
        let t1 = (s + 1) as f32 / 5.0;
        let a = quad_bezier(p0, p1, p2, t0);
        let b = quad_bezier(p0, p1, p2, t1);
        twigs.push((a, b, lerp(0.015, 0.008, t0), lerp(0.015, 0.008, t1)));
        shoots.push((a, b, t0));
    }
    // Two angled side shoots for a rich 3-forked pine branchlet
    for &(t_attach, side, len) in &[(0.34f32, -1.0f32, 0.24f32), (0.48, 1.0, 0.22)] {
        let a = quad_bezier(p0, p1, p2, t_attach);
        let ang = -crate::constants::PI * 0.5 + side * rng.range(0.58, 0.74);
        let b = a + Vec2::new(ang.cos(), ang.sin()) * len;
        twigs.push((a, b, 0.010, 0.006));
        shoots.push((a, b, t_attach));
    }

    // Radiate paired/tripled needle fascicles along every shoot, layered back-to-front
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
                    // 2 needles per fascicle
                    for n_i in 0..2 {
                        let spread = rng.range(0.30, 0.98) + (n_i as f32) * 0.14;
                        let nd = (dir * spread.cos() + perp * side * spread.sin()).normalize();
                        let n_len = rng.range(0.16, 0.24) * lerp(1.05, 0.82, t_base);
                        let end = (origin + nd * n_len).clamp(Vec2::splat(0.04), Vec2::splat(0.96));
                        let shade = if is_top_layer {
                            rng.range(0.96, 1.24)
                        } else {
                            rng.range(0.62, 0.88)
                        };
                        let hue = if is_top_layer {
                            rng.range(0.05, 0.45)
                        } else {
                            rng.range(-0.35, 0.05)
                        };
                        needles.push((origin, end, 0.0075, 0.0022, shade, hue));
                    }
                }
            }
        }
    }
    // Terminal apical needle tuft crowning the main shoot tip
    for k in 0..14 {
        let frac = k as f32 / 13.0 - 0.5;
        let ang = -crate::constants::PI * 0.5 + frac * 1.15 + rng.range(-0.08, 0.08);
        let nd = Vec2::new(ang.cos(), ang.sin());
        let end = (p2 + nd * rng.range(0.17, 0.22)).clamp(Vec2::splat(0.03), Vec2::splat(0.97));
        needles.push((
            p2,
            end,
            0.0075,
            0.0022,
            rng.range(1.0, 1.26),
            rng.range(0.1, 0.5),
        ));
    }

    rasterize_twig_and_needle_strokes(&twigs, &needles, base, twig_rgb, res)
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

    // Two-layer dense spruce/fir needles: dark shadowed interior + bright sunlit top needles
    for layer in 0..2 {
        let top = layer == 1;
        for &(a, b, branch_t) in &branches {
            let len = (b - a).length();
            let dir = (b - a) / len.max(1e-4);
            let perp = Vec2::new(-dir.y, dir.x);
            let steps = ((len / 0.015).ceil() as usize).max(3);
            for i in 0..=steps {
                let t = i as f32 / steps as f32;
                let p = a.lerp(b, t);
                let needle_len = rng.range(0.058, 0.088) * lerp(1.05, 0.72, t);
                for side in [-1.0f32, 1.0] {
                    let sweep = if top {
                        rng.range(0.30, 0.68)
                    } else {
                        rng.range(0.52, 0.98)
                    };
                    let nd = (dir * sweep.cos() + perp * side * sweep.sin()).normalize();
                    let end = (p + nd * needle_len).clamp(Vec2::splat(0.03), Vec2::splat(0.97));
                    let tip_bright = lerp(0.74, 1.25, (t * 0.60 + branch_t * 0.40).clamp(0.0, 1.0));
                    let shade = tip_bright
                        * if top {
                            rng.range(0.95, 1.15)
                        } else {
                            rng.range(0.62, 0.85)
                        };
                    let hue = if top {
                        lerp(-0.10, 0.45, t)
                    } else {
                        lerp(-0.35, -0.05, t)
                    };
                    let w0 = if top { 0.0082 } else { 0.0102 };
                    needles.push((p, end, w0, 0.0028, shade, hue));
                }
            }
            // Terminal needle fan capping each branchlet tip
            for fan in [-0.30f32, 0.0, 0.30] {
                let ang = dir.y.atan2(dir.x) + fan;
                let nd = Vec2::new(ang.cos(), ang.sin());
                let end = (b + nd * 0.055).clamp(Vec2::splat(0.03), Vec2::splat(0.97));
                needles.push((
                    b - dir * 0.015,
                    end,
                    0.008,
                    0.0026,
                    rng.range(1.05, 1.26),
                    0.35,
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
    tex.dilate_alpha_rgb(6);
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
}
