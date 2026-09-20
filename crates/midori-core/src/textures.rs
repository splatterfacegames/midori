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
            Self::Decode(e) => write!(f, "image decode error: {}", e),
            Self::Encode(e) => write!(f, "image encode error: {}", e),
            Self::Io(e) => write!(f, "texture IO error: {}", e),
            Self::BadPath(p) => write!(f, "invalid texture path: {}", p),
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
            // Slow waviness keeps furrows from being ruler-straight.
            let meander = fbm(seed ^ 0xB1, u * 3.0, v * 2.0, 3, 2, 3) - 0.5;
            let uu = u + meander * 0.10;
            // Anisotropic ridged noise → vertical furrows.
            let n = fbm(seed ^ 0xB2, uu * 24.0, v * 4.0, 24, 4, 4);
            let ridge = 1.0 - (2.0 * n - 1.0).abs();
            let mut h = ridge.powf(1.4);
            // Cross-cracks break the ridges into blocks.
            let crack = fbm(seed ^ 0xB3, uu * 24.0, v * 10.0, 24, 10, 2);
            if crack < 0.38 {
                h *= 1.0 - (0.38 - crack) * 1.5;
            }
            h.clamp(0.0, 1.0)
        }
        BarkStyle::Plated => {
            // Wrapped Worley cells: raised plates, dark seams.
            let (d, cell) = voronoi(seed ^ 0xB4, u * 6.0, v * 8.0, 6, 8);
            let plate = (1.0 - d * 1.7).clamp(0.0, 1.0).powf(0.7);
            let detail = fbm(seed ^ 0xB5, u * 24.0, v * 32.0, 24, 32, 2);
            (plate * (0.55 + 0.45 * cell) * (0.8 + 0.4 * detail)).clamp(0.0, 1.0)
        }
        BarkStyle::Smooth => {
            let base = fbm(seed ^ 0xB6, u * 5.0, v * 4.0, 5, 4, 4);
            let mut h = 0.35 + 0.5 * base;
            // Horizontal lenticel dashes (beech/birch/palm look).
            let rows = 36.0;
            let band = (v * rows).fract();
            if (0.30..0.62).contains(&band) {
                let row = (v * rows).floor() as i64;
                let col = (u * 20.0).floor() as i64;
                let on = lattice(seed ^ 0xB7, col.rem_euclid(20), row);
                let along = (u * 20.0).fract();
                if on > 0.66 && (0.15..0.85).contains(&along) {
                    let dip = 1.0 - ((band - 0.46).abs() * 4.0).max(0.0);
                    h -= 0.22 * dip;
                }
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

/// Albedo: palette ramp driven by height, plus fine grain.
fn bark_albedo_from_height(
    height: &[f32],
    res: u32,
    color: Option<[f32; 3]>,
    seed: u64,
) -> RgbaTexture {
    let base = color.unwrap_or([0.36, 0.28, 0.20]);
    let dark = base.map(|c| c * 0.45);
    let light = base.map(|c| (c * 1.3).min(1.0));
    let mut tex = RgbaTexture::new(res, res);
    for y in 0..res {
        for x in 0..res {
            let i = (y * res + x) as usize;
            let u = (x as f32 + 0.5) / res as f32;
            let v = (y as f32 + 0.5) / res as f32;
            let grain = fbm(seed ^ 0xA9, u * 48.0, v * 48.0, 48, 48, 2);
            let t = (height[i] * 0.75 + grain * 0.25).clamp(0.0, 1.0);
            let px = [
                (lerp(dark[0], light[0], t) * 255.0) as u8,
                (lerp(dark[1], light[1], t) * 255.0) as u8,
                (lerp(dark[2], light[2], t) * 255.0) as u8,
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
}

/// Compose the spray of leaf instances for a card layout.
fn card_leaves(layout: LeafCardLayout, shape: LeafShape, rng: &mut Rng) -> Vec<CardLeaf> {
    match layout {
        LeafCardLayout::Single => vec![CardLeaf {
            center: Vec2::new(0.5, 0.52),
            rotation: 0.0,
            scale: 0.42,
            jitter: 1.0,
        }],
        LeafCardLayout::Cluster => {
            // A pinnate spray: leaflets alternate along a curved rachis,
            // wider at the base like a compound leaf or small twig.
            let count = match shape {
                LeafShape::Needle => 13 + rng.index(5),
                _ => 9 + rng.index(4),
            };
            let mut leaves = Vec::with_capacity(count + 1);
            let top_x = 0.5 + rng.range(-0.06, 0.06);
            for i in 0..count {
                let t = i as f32 / (count.saturating_sub(1).max(1)) as f32;
                let side = if i % 2 == 0 { 1.0 } else { -1.0 };
                let along = 0.10 + t * 0.72;
                let base = Vec2::new(lerp(0.5, top_x, along), 0.97 - along * 0.85);
                let spread = lerp(0.30, 0.12, t) * rng.variance_mul(0.15);
                // Outward and slightly upward, mirrored by side.
                let dir = Vec2::new(side, -0.4).normalize();
                let center = base + dir * spread;
                let tip_angle = dir.y.atan2(dir.x);
                leaves.push(CardLeaf {
                    center,
                    rotation: crate::constants::PI / 2.0 - tip_angle,
                    scale: lerp(0.34, 0.17, t) * rng.variance_mul(0.15),
                    jitter: rng.range(0.85, 1.12),
                });
            }
            // Terminal leaf at the top of the rachis.
            leaves.push(CardLeaf {
                center: Vec2::new(top_x, 0.16),
                rotation: rng.range(-0.15, 0.15),
                scale: 0.24 * rng.variance_mul(0.1),
                jitter: rng.range(0.9, 1.1),
            });
            leaves
        }
    }
}

/// Render the leaf card: rachis underleaf, then leaflets back-to-front with
/// alpha-over compositing and a soft SDF edge.
fn leaf_card(
    shape: LeafShape,
    layout: LeafCardLayout,
    color: Option<[f32; 3]>,
    seed: u64,
    res: u32,
) -> RgbaTexture {
    let mut rng = Rng::from_seed(seed ^ 0x1EAF_CA4D);
    let leaves = card_leaves(layout, shape, &mut rng);
    let base = color.unwrap_or([0.27, 0.44, 0.17]);
    let stem_rgb = [0.30, 0.22, 0.13];
    let top_x = leaves.last().map(|l| l.center.x).unwrap_or(0.5);

    let mut tex = RgbaTexture::new(res, res);
    for y in 0..res {
        for x in 0..res {
            let u = (x as f32 + 0.5) / res as f32;
            let v = (y as f32 + 0.5) / res as f32;
            let mut acc = [0.0f32; 4];

            // Rachis / petiole: a thin stroke from bottom center to the spray top.
            if layout == LeafCardLayout::Cluster {
                let d = dist_to_segment(
                    Vec2::new(u, v),
                    Vec2::new(0.5, 0.99),
                    Vec2::new(top_x, 0.10),
                );
                let w = 0.006;
                if d < w {
                    let a = (1.0 - d / w) * 0.9;
                    over(&mut acc, [stem_rgb[0], stem_rgb[1], stem_rgb[2], a]);
                }
            }

            for leaf in &leaves {
                let p = Vec2::new(u, v) - leaf.center;
                let (sin, cos) = leaf.rotation.sin_cos();
                // Leaf-local space: +y is the tip direction.
                let local = Vec2::new(p.x * cos - p.y * sin, p.x * sin + p.y * cos) / leaf.scale;
                // Leaf SDF is defined tip-at-+y; card v grows downward, so a
                // leaf at rotation 0 points its tip toward the top of the card.
                let local = Vec2::new(local.x, -local.y);
                let sdf = leaf_sdf(local, shape);
                let edge = 2.0 * leaf.scale / res as f32;
                let alpha = (0.5 - sdf / edge).clamp(0.0, 1.0);
                if alpha <= 0.0 {
                    continue;
                }
                // Shading: darker toward the petiole base, mottled, midrib line.
                let mottle = fbm(seed ^ 0xC0, u * 8.0, v * 8.0, 8, 8, 2);
                let mut shade =
                    leaf.jitter * (0.88 + 0.24 * (local.y + 0.5)) * (0.9 + 0.2 * mottle);
                if local.x.abs() < 0.02 {
                    shade *= 0.8;
                }
                let c = base.map(|ch| (ch * shade).clamp(0.0, 1.0));
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
    tex
}

/// Distance from point to segment (card space).
fn dist_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
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

{}
"#,
            toml_extra
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
            "cluster card coverage {} outside expected range",
            coverage
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
        assert!(a < b, "single {} should be sparser than cluster {}", a, b);
        assert!(a > 0.03, "single {} should still be visible", a);
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
                    "{:?} does not tile at v={}: {} vs {}",
                    style,
                    v,
                    a,
                    b
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
                    "{:?} does not tile vertically at u={}",
                    style,
                    u
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
