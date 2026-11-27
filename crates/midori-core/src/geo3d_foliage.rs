//! 2026/2027 Nanite-era 100% OPAQUE 3D geometric botanical foliage (`--foliage geo3d`).
//!
//! Replaces 2012 alpha-scissor billboard cards and low-poly blob lobes with actual
//! 3D opaque triangle geometry:
//! - Curved 3D V-creased sickle/lanceolate/elliptic/serrated leaf blades (4–8 tris each)
//!   clustered along visible 3D woody petioles and crooked quaternary twiglets.
//! - Perth Jacaranda: fine dark-brown 3D woody twigs threading through 3D curved bipinnate
//!   leaflet fronds and distinct 3D tubular lavender-violet trumpet-bell blossom clusters.
//! - Norfolk Island Pine: upturned 3D feathery Araucaria chevron needle-shoot branchlets.
//! - Slender Banksia: 3D serrated lance leaves + 3D ribbed golden-yellow cylindrical candles.
//! - Groundcover rosettes: true 3D Kangaroo Paw (curved strap leaves, forked velvety stems,
//!   and 3D tubular red-and-lime-green 6-clawed paw flowers), 3D arched Sword Sedge blades,
//!   and 3D Zamia pinnate cycad fronds.
//!
//! Vertex attributes (all 100% `OPAQUE`, zero alpha-scissor):
//! - `TEXCOORD_0`: clean normalized botanical `(0..1, 0..1)` UV on every 3D leaf, leaflet,
//!   trumpet-bell petal, and Araucaria needle shoot (`v=0` petiole base -> `v=1` tip,
//!   `u=0` left margin -> `u=0.5` midrib -> `u=1` right margin).
//! - `NORMAL`: ~60% smooth spherical bent normal + ~40% curved 3D leaf normal at LOD0,
//!   stepping up to ~80% spherical bent normal at LOD1 (`Mid3D`) for zero mid-field shimmer.
//! - `TEXCOORD_1`: `(crown_ao, height_norm)` with strong interior crown occlusion (`0.22..1.0`).
//! - `COLOR_0`: `(linear_r, linear_g, linear_b, thickness)` with per-leaf hue/value variation,
//!   interior dark-olive occlusion vs sunlit outer tips, and ~12% dry ochre/rust-bronze WA leaves.

use crate::constants::{GOLDEN_ANGLE, PI, TAU};
use crate::math::create_basis;
use crate::mesh::{Mesh, Vertex};
use crate::opaque_foliage::{srgb3_to_linear, species_bark_color, species_foliage_colors};
use crate::organs::{Organ, OrganKind, reduce_organs, ribbon_spine, ribbon_tangent};
use crate::species::{
    BladeCard, GeneratorFamily, InflorescenceKind, LeafShape, Species, SpikeStyle,
};
use crate::tree::{Leaf, Stem, Tree};
use glam::{Vec2, Vec3, Vec4};
use std::collections::BTreeMap;

/// Output of 3D geometric foliage generation for one LOD level.
pub struct Geo3dResult {
    /// 100% opaque 3D geometric leaves/needles/fronds (`MaterialType::Leaves`).
    pub foliage: Mesh,
    /// Fine 3D woody petioles and crooked quaternary twigs (`MaterialType::Bark` / `UpperBark`).
    pub twigs: Mesh,
    /// Dedicated 100% opaque 3D flowers/blossoms/candles/nuts (`MaterialType::Organ`), when enabled.
    pub organs: Option<Mesh>,
    /// Number of individual 3D botanical leaf/blossom units emitted.
    pub unit_count: u32,
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
fn hash_unit(seed: u64) -> f32 {
    (hash_u64(seed) >> 40) as f32 / (1u32 << 24) as f32
}

#[inline]
fn hash_signed(seed: u64) -> f32 {
    hash_unit(seed) * 2.0 - 1.0
}

#[inline]
fn lerp_rgb(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

#[inline]
fn mul_rgb(c: [f32; 3], k: f32) -> [f32; 3] {
    [
        (c[0] * k).clamp(0.0, 1.0),
        (c[1] * k).clamp(0.0, 1.0),
        (c[2] * k).clamp(0.0, 1.0),
    ]
}

/// Crown & sub-cluster spatial context for smooth bent normals and strong interior AO (`0.22..1.0`).
struct CrownField {
    crown_center: Vec3,
    crown_radii: Vec3,
    crown_min_y: f32,
    crown_max_y: f32,
    tree_min_y: f32,
    tree_span_y: f32,
    clump_centers: BTreeMap<u32, (Vec3, f32)>,
    stem_to_clump: BTreeMap<u32, u32>,
    bent_weight: f32,
    up_bias: f32,
}

impl CrownField {
    fn from_tree(tree: &Tree, species: &Species, high_detail: bool) -> Self {
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for l in &tree.leaves {
            lo = lo.min(l.position);
            hi = hi.max(l.position);
        }
        for o in &tree.organs {
            lo = lo.min(o.position);
            hi = hi.max(o.position);
        }
        if lo.x > hi.x {
            lo = tree.bounds.min;
            hi = tree.bounds.max;
        }
        if lo.x > hi.x {
            lo = Vec3::ZERO;
            hi = Vec3::ONE;
        }

        let crown_center = (lo + hi) * 0.5;
        let crown_radii = ((hi - lo) * 0.5).max(Vec3::splat(0.25));
        let tree_min_y = if tree.bounds.is_valid() {
            tree.bounds.min.y.min(0.0)
        } else {
            lo.y.min(0.0)
        };
        let tree_max_y = if tree.bounds.is_valid() {
            tree.bounds.max.y.max(hi.y)
        } else {
            hi.y
        };
        let tree_span_y = (tree_max_y - tree_min_y).max(0.35);

        let stem_lookup: BTreeMap<u32, (u8, Option<u32>)> = tree
            .stems
            .iter()
            .map(|s| (s.id, (s.level, s.parent_id)))
            .collect();

        let clump_level = species.canopy.clump_level.clamp(1, 2) as u8;
        let mut stem_to_clump = BTreeMap::new();
        for s in &tree.stems {
            let mut id = s.id;
            for _ in 0..8 {
                if let Some(&(lvl, parent)) = stem_lookup.get(&id) {
                    if lvl <= clump_level {
                        break;
                    }
                    if let Some(pid) = parent {
                        id = pid;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            stem_to_clump.insert(s.id, id);
        }

        let mut sums: BTreeMap<u32, (Vec3, u32)> = BTreeMap::new();
        for l in &tree.leaves {
            let cid = stem_to_clump.get(&l.stem_id).copied().unwrap_or(l.stem_id);
            let e = sums.entry(cid).or_insert((Vec3::ZERO, 0));
            e.0 += l.position;
            e.1 += 1;
        }
        for o in &tree.organs {
            let cid = stem_to_clump.get(&o.stem_id).copied().unwrap_or(o.stem_id);
            let e = sums.entry(cid).or_insert((Vec3::ZERO, 0));
            e.0 += o.position;
            e.1 += 1;
        }
        let centers: BTreeMap<u32, Vec3> = sums
            .into_iter()
            .map(|(k, (s, n))| (k, s / n.max(1) as f32))
            .collect();

        let mut rad_acc: BTreeMap<u32, (f32, u32)> = BTreeMap::new();
        for l in &tree.leaves {
            let cid = stem_to_clump.get(&l.stem_id).copied().unwrap_or(l.stem_id);
            if let Some(&cc) = centers.get(&cid) {
                let e = rad_acc.entry(cid).or_insert((0.0, 0));
                e.0 += (l.position - cc).length_squared();
                e.1 += 1;
            }
        }
        for o in &tree.organs {
            let cid = stem_to_clump.get(&o.stem_id).copied().unwrap_or(o.stem_id);
            if let Some(&cc) = centers.get(&cid) {
                let e = rad_acc.entry(cid).or_insert((0.0, 0));
                e.0 += (o.position - cc).length_squared();
                e.1 += 1;
            }
        }

        let clump_centers = centers
            .into_iter()
            .map(|(k, c)| {
                let r = rad_acc
                    .get(&k)
                    .map(|&(sq, n)| (sq / n.max(1) as f32).sqrt().max(0.20) * 1.35)
                    .unwrap_or(1.0);
                (k, (c, r))
            })
            .collect();

        // LOD0 (~60% spherical canopy + ~40% curved 3D leaf normal) preserves crisp macro leaf
        // modeling & specular highlights; LOD1 (~80% spherical canopy + ~20% geometric normal)
        // dramatically suppresses mid-field (65–450 m) normal-derivative shimmer!
        let bent_weight = if species.generator.family == GeneratorFamily::Rosette {
            if high_detail { 0.35 } else { 0.58 }
        } else if high_detail {
            species.canopy.normal_blend.clamp(0.56, 0.62)
        } else {
            0.80
        };

        Self {
            crown_center,
            crown_radii,
            crown_min_y: lo.y,
            crown_max_y: hi.y,
            tree_min_y,
            tree_span_y,
            clump_centers,
            stem_to_clump,
            bent_weight,
            up_bias: species.canopy.up_bias.clamp(0.18, 0.36),
        }
    }

    #[inline]
    fn height_norm(&self, pos: Vec3) -> f32 {
        ((pos.y - self.tree_min_y) / self.tree_span_y).clamp(0.0, 1.0)
    }

    /// Strong interior crown self-occlusion in `[0.22, 1.0]`.
    #[inline]
    fn crown_ao(&self, stem_id: u32, pos: Vec3) -> f32 {
        let rel = (pos - self.crown_center) / self.crown_radii;
        let crown_depth = 1.0 - rel.length().clamp(0.0, 1.0);
        let cid = self.stem_to_clump.get(&stem_id).copied().unwrap_or(stem_id);
        let clump_depth = self
            .clump_centers
            .get(&cid)
            .map(|&(cc, cr)| 1.0 - ((pos - cc).length() / cr).clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let span = (self.crown_max_y - self.crown_min_y).max(0.25);
        let h_crown = ((pos.y - self.crown_min_y) / span).clamp(0.0, 1.0);
        let occl = (0.48 * crown_depth + 0.30 * clump_depth + 0.32 * (1.0 - h_crown)).clamp(0.0, 1.0);
        (1.0 - 0.78 * occl).clamp(0.22, 1.0)
    }

    /// Smooth outward spherical bent normal for sub-cluster volume lighting.
    #[inline]
    fn bent_normal(&self, stem_id: u32, pos: Vec3) -> Vec3 {
        let cid = self.stem_to_clump.get(&stem_id).copied().unwrap_or(stem_id);
        let n_crown = ((pos - self.crown_center) / self.crown_radii)
            .try_normalize()
            .unwrap_or(Vec3::Y);
        let n_clump = self
            .clump_centers
            .get(&cid)
            .and_then(|&(cc, _)| (pos - cc).try_normalize())
            .unwrap_or(n_crown);
        (n_clump * 0.62 + n_crown * 0.38 + Vec3::Y * self.up_bias)
            .try_normalize()
            .unwrap_or(Vec3::Y)
    }

    /// Blend actual 3D curved geometric normal with smooth spherical bent normal.
    #[inline]
    fn blend_normal(&self, stem_id: u32, pos: Vec3, geom_normal: Vec3) -> Vec3 {
        let bent = self.bent_normal(stem_id, pos);
        let aligned_geom = if geom_normal.dot(bent) < 0.0 {
            -geom_normal
        } else {
            geom_normal
        };
        aligned_geom
            .lerp(bent, self.bent_weight)
            .try_normalize()
            .unwrap_or(bent)
    }
}

/// Parameters shaping an individual 3D opaque curved leaf blade.
#[derive(Clone, Copy)]
struct BladeShapeSpec {
    length: f32,
    max_width: f32,
    /// Lateral sickle/falcate sweep (positive = curves to +side, characteristic of eucalypts).
    sickle: f32,
    /// Longitudinal downward arch/droop along blade length (negative = upturned conifer shoot).
    arch: f32,
    /// Central V-crease midrib depth relative to half-width.
    keel: f32,
    /// Upper-blade shoulder fullness at `v = 0.70` (`0.66..0.88`; fuller = softer rounded silhouette).
    upper_shoulder: f32,
    /// Slight notched tooth offset on Banksia leaves.
    serrated: bool,
    /// Linear base & tip RGB colours.
    col_base: [f32; 3],
    col_tip: [f32; 3],
    /// Leaf translucency thickness encoded in `COLOR_0.a` (`0.25..0.85`).
    thickness: f32,
    /// `true` = 6-tri curved botanical blade with rounded shoulders (LOD0), `false` = 4-tri blade (LOD1).
    high_detail: bool,
}

/// Emit one 100% opaque 3D curved leaf blade with clean normalized botanical `TEXCOORD_0`
/// `(u=0 left margin -> u=0.5 midrib -> u=1 right margin, v=0 petiole base -> v=1 pointed tip)`,
/// V-creased midrib, falcate/sickle bend, and blended bent+geometric normals.
fn append_curved_3d_leaf(
    mesh: &mut Mesh,
    origin: Vec3,
    dir: Vec3,
    up_hint: Vec3,
    stem_id: u32,
    field: &CrownField,
    spec: &BladeShapeSpec,
) {
    let fwd = dir.try_normalize().unwrap_or(Vec3::Y);
    let side = fwd
        .cross(up_hint)
        .try_normalize()
        .unwrap_or_else(|| create_basis(fwd).0);
    let normal_up = side.cross(fwd).try_normalize().unwrap_or(Vec3::Y);

    let len = spec.length.max(0.03);
    let hw = (spec.max_width * 0.5).max(0.008);
    let v_base = mesh.vertices.len() as u32;

    let make_vert = |u_along: f32, x_lat: f32, w_scale: f32, uv: Vec2| -> Vertex {
        let sickle_off = spec.sickle * len * (u_along * u_along);
        // Gentle upward lift near base then graceful gravity arch toward tip (or upturned curve if arch < 0)
        let arch_off = len * (spec.arch * (0.35 * u_along - 1.15 * u_along * u_along));
        // V-crease: midrib (x_lat=0) dips slightly, margins lift
        let keel_off = spec.keel * hw * w_scale * x_lat.abs();
        let pos = origin
            + fwd * (len * u_along)
            + side * (sickle_off + x_lat * hw * w_scale)
            + normal_up * (arch_off + keel_off);

        // Analytical curved 3D blade surface normal (captures longitudinal arch + V-crease!)
        let d_arch_du = spec.arch * (0.35 - 2.30 * u_along);
        let d_keel_dx = if x_lat.abs() > 0.01 {
            spec.keel * x_lat.signum() * 0.85
        } else {
            0.0
        };
        let geom_n = (normal_up - fwd * (d_arch_du * 0.65) - side * d_keel_dx)
            .try_normalize()
            .unwrap_or(normal_up);
        let final_n = field.blend_normal(stem_id, pos, geom_n);

        let crown_ao = field.crown_ao(stem_id, pos);
        let height_norm = field.height_norm(pos);
        // Subtle midrib crease darkening + rich dark-olive interior occlusion vs warm sunlit outer tips
        let midrib_shade = 0.91 + 0.09 * x_lat.abs();
        let interior_depth = 0.78 + 0.24 * crown_ao;
        let sunlit_tip = ((crown_ao - 0.68) * 2.5).clamp(0.0, 1.0) * u_along;
        let mut rgb = mul_rgb(
            lerp_rgb(spec.col_base, spec.col_tip, u_along),
            midrib_shade * interior_depth,
        );
        if sunlit_tip > 0.01 {
            rgb = [
                (rgb[0] * (1.0 + 0.07 * sunlit_tip)).clamp(0.0, 1.0),
                (rgb[1] * (1.0 + 0.05 * sunlit_tip)).clamp(0.0, 1.0),
                (rgb[2] * (1.0 - 0.03 * sunlit_tip)).clamp(0.0, 1.0),
            ];
        }

        Vertex {
            position: pos,
            normal: final_n,
            uv,
            uv2: Vec2::new(crown_ao, height_norm),
            color: Vec4::new(rgb[0], rgb[1], rgb[2], spec.thickness),
        }
    };

    if spec.high_detail {
        // 7 vertices, 6 triangles: pointed petiole base (v=0) -> lower-mid V-creased shoulder (v=0.32)
        // -> upper rounded botanical shoulder (v=0.70) -> pointed apex tip (v=1.0).
        // Zero harsh geometric diamond corners even at 0.5 m macro!
        let w1 = if spec.serrated { 0.96 } else { 1.0 };
        let w2 = spec.upper_shoulder.clamp(0.62, 0.90);
        let s1 = if spec.serrated { 1.12 } else { 1.0 };
        let s2 = if spec.serrated { 0.86 } else { 1.0 };

        mesh.vertices.push(make_vert(0.0, 0.0, 0.0, Vec2::new(0.5, 0.0)));
        mesh.vertices.push(make_vert(0.32, -1.0, w1 * s1, Vec2::new(0.0, 0.32)));
        mesh.vertices.push(make_vert(0.32, 0.0, w1, Vec2::new(0.5, 0.32)));
        mesh.vertices.push(make_vert(0.32, 1.0, w1 * s2, Vec2::new(1.0, 0.32)));
        mesh.vertices.push(make_vert(0.70, -1.0, w2 * s2, Vec2::new(0.0, 0.70)));
        mesh.vertices.push(make_vert(0.70, 1.0, w2 * s1, Vec2::new(1.0, 0.70)));
        mesh.vertices.push(make_vert(1.0, 0.0, 0.0, Vec2::new(0.5, 1.0)));

        mesh.indices.extend_from_slice(&[
            // Base fan (2 tris)
            v_base,
            v_base + 1,
            v_base + 2,
            v_base,
            v_base + 2,
            v_base + 3,
            // Upper-mid V-creased wings (2 tris)
            v_base + 1,
            v_base + 4,
            v_base + 2,
            v_base + 2,
            v_base + 5,
            v_base + 3,
            // Arching pointed apex tip (2 tris)
            v_base + 4,
            v_base + 6,
            v_base + 2,
            v_base + 2,
            v_base + 6,
            v_base + 5,
        ]);
    } else {
        // 5 vertices, 4 triangles: pointed base -> mid V-crease row (L,M,R) -> pointed tip (LOD1)
        mesh.vertices.push(make_vert(0.0, 0.0, 0.0, Vec2::new(0.5, 0.0)));
        mesh.vertices.push(make_vert(0.45, -1.0, 1.0, Vec2::new(0.0, 0.45)));
        mesh.vertices.push(make_vert(0.45, 0.0, 1.0, Vec2::new(0.5, 0.45)));
        mesh.vertices.push(make_vert(0.45, 1.0, 1.0, Vec2::new(1.0, 0.45)));
        mesh.vertices.push(make_vert(1.0, 0.0, 0.0, Vec2::new(0.5, 1.0)));

        mesh.indices.extend_from_slice(&[
            v_base,
            v_base + 1,
            v_base + 2,
            v_base,
            v_base + 2,
            v_base + 3,
            v_base + 1,
            v_base + 4,
            v_base + 2,
            v_base + 2,
            v_base + 4,
            v_base + 3,
        ]);
    }
}

/// Emit a slender tapered 3D woody petiole or crooked quaternary twiglet (6 tris, 3-sided prism).
#[allow(clippy::too_many_arguments)]
fn append_woody_twiglet(
    mesh: &mut Mesh,
    start: Vec3,
    end: Vec3,
    r_start: f32,
    r_end: f32,
    bark_lin: [f32; 3],
    stem_id: u32,
    field: &CrownField,
) {
    let delta = end - start;
    let len = delta.length();
    if len < 0.015 {
        return;
    }
    let axis = delta / len;
    let (tan, bit) = create_basis(axis);
    let r0 = r_start.max(0.004);
    let r1 = r_end.max(0.0025);
    let v0 = mesh.vertices.len() as u32;

    let ao0 = field.crown_ao(stem_id, start);
    let ao1 = field.crown_ao(stem_id, end);
    let h0 = field.height_norm(start);
    let h1 = field.height_norm(end);
    let v_repeat = (len / 1.25).clamp(0.08, 1.0);

    for k in 0..3u32 {
        let ang = (k as f32 / 3.0) * TAU;
        let n = (tan * ang.cos() + bit * ang.sin()).normalize();
        let u = k as f32 / 3.0;
        let c0 = mul_rgb(bark_lin, 0.72 + 0.28 * ao0);
        let c1 = mul_rgb(bark_lin, 0.72 + 0.28 * ao1);
        mesh.vertices.push(Vertex {
            position: start + n * r0,
            normal: n,
            uv: Vec2::new(u, 0.0),
            uv2: Vec2::new(ao0, h0),
            color: Vec4::new(c0[0], c0[1], c0[2], 1.0),
        });
        mesh.vertices.push(Vertex {
            position: end + n * r1,
            normal: n,
            uv: Vec2::new(u, v_repeat),
            uv2: Vec2::new(ao1, h1),
            color: Vec4::new(c1[0], c1[1], c1[2], 1.0),
        });
    }

    for k in 0..3u32 {
        let next = (k + 1) % 3;
        let b0 = v0 + k * 2;
        let t0 = b0 + 1;
        let b1 = v0 + next * 2;
        let t1 = b1 + 1;
        mesh.indices.extend_from_slice(&[b0, b1, t1, b0, t1, t0]);
    }
}

/// Emit a crooked 2-segment dark-brown woody quaternary twig fork (used inside Jacaranda & eucalypt shoots).
#[allow(clippy::too_many_arguments)]
fn append_crooked_twig_fork(
    twigs: &mut Mesh,
    origin: Vec3,
    dir: Vec3,
    length: f32,
    radius: f32,
    seed: u64,
    bark_lin: [f32; 3],
    stem_id: u32,
    field: &CrownField,
) -> (Vec3, Vec3) {
    let fwd = dir.try_normalize().unwrap_or(Vec3::Y);
    let (tan, bit) = create_basis(fwd);
    let kink_ang = hash_unit(seed ^ 0x101) * TAU;
    let kink_dir = tan * kink_ang.cos() + bit * kink_ang.sin();
    let mid = origin + fwd * (length * 0.52) + kink_dir * (length * 0.14);
    let tip_dir = (fwd + kink_dir * 0.35 + Vec3::Y * (0.12 * hash_signed(seed ^ 0x202)))
        .try_normalize()
        .unwrap_or(fwd);
    let tip = mid + tip_dir * (length * 0.54);

    append_woody_twiglet(twigs, origin, mid, radius, radius * 0.68, bark_lin, stem_id, field);
    append_woody_twiglet(twigs, mid, tip, radius * 0.68, radius * 0.36, bark_lin, stem_id, field);

    // Side forklet emerging from the mid-kink for true crooked tertiary/quaternary twig density
    let side_dir = (fwd * 0.65 - kink_dir * 0.65 + Vec3::Y * 0.18)
        .try_normalize()
        .unwrap_or(fwd);
    let side_tip = mid + side_dir * (length * 0.48);
    append_woody_twiglet(
        twigs,
        mid,
        side_tip,
        radius * 0.56,
        radius * 0.28,
        bark_lin,
        stem_id,
        field,
    );
    (tip, side_tip)
}

/// Species-specific foliage colour palette in linear RGB, including ~12% dry ochre / bronze / copper variation.
struct LeafPalette {
    base_lin: [f32; 3],
    tip_lin: [f32; 3],
    under_lin: [f32; 3],
    new_lin: Option<[f32; 3]>,
    new_frac: f32,
    dead_lin: [f32; 3],
    dead_frac: f32,
    thickness: f32,
}

impl LeafPalette {
    fn from_species(species: &Species) -> Self {
        let (top_srgb, _) = species_foliage_colors(species);
        let base_srgb = [top_srgb[0] * 0.78, top_srgb[1] * 0.82, top_srgb[2] * 0.76];
        let tip_srgb = [
            (top_srgb[0] * 1.14).min(1.0),
            (top_srgb[1] * 1.12).min(1.0),
            (top_srgb[2] * 1.06).min(1.0),
        ];
        let under_srgb = species
            .textures
            .leaf_underside_color
            .unwrap_or([top_srgb[0] * 1.10, top_srgb[1] * 1.12, top_srgb[2] * 0.96]);

        let dead_srgb = species
            .textures
            .leaf_dead_color
            .unwrap_or([0.58, 0.44, 0.23]);
        // Ensure authentic WA dry-ochre/rust-bronze foliage mix (~11-14%) on eucalypts & natives
        let is_eucalypt_or_wa = matches!(
            species.leaves.shape,
            LeafShape::Sickle
                | LeafShape::BroadEucalypt
                | LeafShape::Willow
                | LeafShape::Banksia
                | LeafShape::BullBanksia
                | LeafShape::Paperbark
        );
        let dead_frac = if species.leaves.shape == LeafShape::Needle {
            0.02
        } else if species.textures.leaf_dead_fraction > 0.0 {
            species.textures.leaf_dead_fraction.max(0.11)
        } else if is_eucalypt_or_wa {
            0.12
        } else {
            0.05
        };

        let thickness = match species.leaves.shape {
            LeafShape::Bipinnate => 0.30,
            LeafShape::Willow | LeafShape::Paperbark => 0.44,
            LeafShape::Sickle | LeafShape::Needle => 0.54,
            LeafShape::Banksia | LeafShape::BullBanksia => 0.62,
            LeafShape::BroadEucalypt | LeafShape::Pointed => 0.66,
            LeafShape::Oval => 0.78,
            _ => 0.55,
        };

        Self {
            base_lin: srgb3_to_linear(base_srgb),
            tip_lin: srgb3_to_linear(tip_srgb),
            under_lin: srgb3_to_linear(under_srgb),
            new_lin: species.textures.leaf_new_growth_color.map(srgb3_to_linear),
            new_frac: species.textures.leaf_new_growth_fraction.clamp(0.0, 0.45),
            dead_lin: srgb3_to_linear(dead_srgb),
            dead_frac,
            thickness,
        }
    }

    fn pick_colors(&self, seed: u64, is_moreton_fig: bool) -> ([f32; 3], [f32; 3], f32) {
        let roll = hash_unit(seed ^ 0xA11CE);
        let val_jit = 0.84 + 0.30 * hash_unit(seed ^ 0xBEEF);
        let warm_jit = hash_signed(seed ^ 0xCAFE) * 0.11;

        let apply_var = |c: [f32; 3]| -> [f32; 3] {
            [
                (c[0] * val_jit * (1.0 + warm_jit * 0.85)).clamp(0.0, 1.0),
                (c[1] * val_jit * (1.0 + warm_jit * 0.25)).clamp(0.0, 1.0),
                (c[2] * val_jit * (1.0 - warm_jit * 0.75)).clamp(0.0, 1.0),
            ]
        };

        if roll < self.dead_frac {
            // Dry WA ochre / rust-bronze leaf
            let d_base = apply_var(mul_rgb(self.dead_lin, 0.82));
            let d_tip = apply_var(mul_rgb(self.dead_lin, 1.12));
            return (d_base, d_tip, (self.thickness + 0.12).min(0.88));
        }
        if let Some(new_c) = self.new_lin
            && roll < self.dead_frac + self.new_frac
        {
            // Copper-bronze / lime new-growth shoot
            let n_base = apply_var(lerp_rgb(self.base_lin, new_c, 0.65));
            let n_tip = apply_var(new_c);
            return (n_base, n_tip, (self.thickness * 0.72).max(0.28));
        }
        if is_moreton_fig && roll > 0.78 {
            // Occasional upturned fig leaf showing its warm rusty-bronze underside
            let u_base = apply_var(mul_rgb(self.under_lin, 0.84));
            let u_tip = apply_var(self.under_lin);
            return (u_base, u_tip, self.thickness);
        }
        (apply_var(self.base_lin), apply_var(self.tip_lin), self.thickness)
    }
}

/// Emit a 3D curved bipinnate Jacaranda fern-frond shoot: curved 3D rachis + dense paired narrow pointed 3D leaflets.
#[allow(clippy::too_many_arguments)]
fn append_jacaranda_bipinnate_frond(
    foliage: &mut Mesh,
    twigs: &mut Mesh,
    leaf: &Leaf,
    seed: u64,
    palette: &LeafPalette,
    bark_lin: [f32; 3],
    field: &CrownField,
    high_detail: bool,
) -> u32 {
    let rachis_len = (leaf.scale * 0.72).clamp(0.38, 0.72);
    let raw_fwd = (leaf.rotation * Vec3::Y).try_normalize().unwrap_or(Vec3::Y);
    // Keep bipinnate fronds arching gracefully outward/horizontally like real fern feathers
    let fwd = (Vec3::new(raw_fwd.x, raw_fwd.y * 0.45 + 0.10, raw_fwd.z))
        .try_normalize()
        .unwrap_or(raw_fwd);
    let side_base = fwd
        .cross(Vec3::Y)
        .try_normalize()
        .unwrap_or_else(|| create_basis(fwd).0);
    let up_base = side_base.cross(fwd).try_normalize().unwrap_or(Vec3::Y);

    let tip_pos = leaf.position + fwd * rachis_len - Vec3::Y * (rachis_len * 0.22);
    if high_detail {
        append_woody_twiglet(
            twigs,
            leaf.position,
            tip_pos,
            0.0045,
            0.002,
            bark_lin,
            leaf.stem_id,
            field,
        );
    }

    let pairs: usize = if high_detail { 6 } else { 3 };
    let lod_w_boost = if high_detail { 1.0 } else { 1.52 };
    let lod_l_boost = if high_detail { 1.0 } else { 1.32 };
    let mut emitted = 0u32;
    for p in 0..pairs {
        let u = (p as f32 + 0.55) / (pairs as f32 + 0.35);
        let anchor = leaf.position + fwd * (rachis_len * u) - Vec3::Y * (rachis_len * 0.22 * u * u);
        let env = (u * PI).sin().max(0.30).powf(0.45);
        let pinna_len = rachis_len * 0.48 * env * lod_l_boost;
        let pinna_w = (pinna_len * 0.38 * lod_w_boost).clamp(0.036, 0.115);

        for sign in [-1.0_f32, 1.0_f32] {
            let p_seed = seed ^ ((p as u64) * 31 + if sign > 0.0 { 7 } else { 19 });
            let (col_base, col_tip, thick) = palette.pick_colors(p_seed, false);
            let p_dir = (side_base * (sign * 0.88) + fwd * 0.42 + up_base * 0.14)
                .try_normalize()
                .unwrap_or(side_base * sign);
            let spec = BladeShapeSpec {
                length: pinna_len,
                max_width: pinna_w,
                sickle: 0.04 * sign,
                arch: 0.20,
                keel: 0.28,
                upper_shoulder: 0.76,
                serrated: false,
                col_base,
                col_tip,
                thickness: thick,
                high_detail, // 6-tri curved rounded leaflet at LOD0, 4-tri broader leaflet at LOD1
            };
            append_curved_3d_leaf(foliage, anchor, p_dir, Vec3::Y, leaf.stem_id, field, &spec);
            emitted += 1;
        }
    }
    emitted
}

/// Emit one 3D tubular flared Jacaranda lavender-violet trumpet-bell floret with clean seam-free
/// `(0..1, 0..1)` petal UVs (`v=0` royal-violet throat base -> `v=1` luminous lilac petal rim).
#[allow(clippy::too_many_arguments)]
fn append_jacaranda_trumpet_bell(
    mesh: &mut Mesh,
    origin: Vec3,
    axis: Vec3,
    length: f32,
    mouth_radius: f32,
    seed: u64,
    throat_lin: [f32; 3],
    rim_lin: [f32; 3],
    stem_id: u32,
    field: &CrownField,
    high_detail: bool,
) {
    let fwd = axis.try_normalize().unwrap_or(Vec3::Y);
    let (tan, bit) = create_basis(fwd);
    let spin = hash_unit(seed ^ 0x51A) * TAU;
    let sides: u32 = if high_detail { 4 } else { 3 };
    let v0 = mesh.vertices.len() as u32;

    // Gentle trumpet droop along the tubular bell
    let bend_vec = (Vec3::NEG_Y - fwd * Vec3::NEG_Y.dot(fwd))
        .try_normalize()
        .unwrap_or(tan);
    let mouth_pos = origin + fwd * length + bend_vec * (length * 0.20);
    let r_rim = mouth_radius.max(0.018);

    let crown_ao = field.crown_ao(stem_id, mouth_pos);
    let h_norm = field.height_norm(mouth_pos);
    let hue_jit = 0.86 + 0.26 * hash_unit(seed ^ 0x771);
    let warm_shift = hash_signed(seed ^ 0x883) * 0.08;
    let c_throat = mul_rgb(throat_lin, hue_jit * (0.64 + 0.36 * crown_ao));
    let c_rim_raw = mul_rgb(rim_lin, hue_jit * (0.86 + 0.22 * crown_ao));
    let c_rim = [
        (c_rim_raw[0] * (1.0 + warm_shift)).clamp(0.0, 1.0),
        (c_rim_raw[1] * (1.0 - warm_shift * 0.35)).clamp(0.0, 1.0),
        (c_rim_raw[2] * (1.0 + warm_shift * 0.25)).clamp(0.0, 1.0),
    ];

    // Closed-apex flared 4-sided (LOD0) or 3-sided (LOD1) trumpet bell with duplicated seam vertex
    // so `TEXCOORD_0.x` sweeps cleanly `0.0..=1.0` without wrap-back distortion!
    let n_apex = field.blend_normal(stem_id, origin, -fwd);
    mesh.vertices.push(Vertex {
        position: origin,
        normal: n_apex,
        uv: Vec2::new(0.5, 0.0),
        uv2: Vec2::new((crown_ao * 0.80).clamp(0.22, 1.0), h_norm),
        color: Vec4::new(c_throat[0], c_throat[1], c_throat[2], 0.34),
    });

    for k in 0..=sides {
        let wrap_k = k % sides;
        let a = spin + (wrap_k as f32 / sides as f32) * TAU;
        let rad = tan * a.cos() + bit * a.sin();
        let scallop = 1.0 + 0.16 * (wrap_k as f32 * 1.9 + spin).sin();
        let p_rim = mouth_pos + rad * (r_rim * scallop);
        let n_rim = field.blend_normal(
            stem_id,
            p_rim,
            (rad * 0.72 + fwd * 0.68).normalize_or_zero(),
        );
        mesh.vertices.push(Vertex {
            position: p_rim,
            normal: n_rim,
            uv: Vec2::new(k as f32 / sides as f32, 1.0),
            uv2: Vec2::new(crown_ao, h_norm),
            color: Vec4::new(c_rim[0], c_rim[1], c_rim[2], 0.24),
        });
    }

    for k in 0..sides {
        mesh.indices
            .extend_from_slice(&[v0, v0 + 1 + k, v0 + 2 + k]);
    }
}

/// Emit soft feathery coastal Araucaria (Norfolk Island Pine) layered curved 3D needle-shoot tufts
/// clothing each sweeping coastal branchlet — lush, overlapping, upturned rounded-shoulder pine plumes
/// (rich dark coastal emerald volume, ZERO giant flat paper origami diamonds even at 0.5 m macro!).
#[allow(clippy::too_many_arguments)]
fn append_araucaria_needle_shoot(
    foliage: &mut Mesh,
    _twigs: &mut Mesh,
    leaf: &Leaf,
    seed: u64,
    palette: &LeafPalette,
    _bark_lin: [f32; 3],
    field: &CrownField,
    high_detail: bool,
) -> u32 {
    let raw_dir = (leaf.rotation * Vec3::Y).try_normalize().unwrap_or(Vec3::X);
    // Real Cottesloe / Riverside Dr Araucaria boughs sweep horizontally outward with lush upturned feathery plumes
    let horiz_dir = Vec3::new(raw_dir.x, raw_dir.y * 0.24 + 0.10, raw_dir.z)
        .try_normalize()
        .unwrap_or(Vec3::X);
    let side_dir = horiz_dir
        .cross(Vec3::Y)
        .try_normalize()
        .unwrap_or_else(|| create_basis(horiz_dir).0);
    let up_dir = side_dir.cross(horiz_dir).try_normalize().unwrap_or(Vec3::Y);

    let frond_len = (leaf.scale * 0.90).clamp(0.46, 0.92);
    // Deep rich coastal emerald interior -> lush sunlit sea-pine emerald tip gradient
    let emerald_base = srgb3_to_linear([0.09, 0.22, 0.10]);
    let pine_tip = srgb3_to_linear([0.24, 0.44, 0.19]);

    // Dense layered feathery upturned curved needle-plume tufts clothing the branchlet from base to tip:
    // - LOD0: 3 paired sweeping plumes + 2 upturned dorsal crest plumes (8 × 6 = 48 tris/shoot)
    // - LOD1: 2 paired broadened plumes + 1 dorsal crest plume (5 × 4 = 20 tris/shoot)
    let pairs: usize = if high_detail { 3 } else { 2 };
    let lod_len_boost = if high_detail { 1.0 } else { 1.42 };
    let lod_w_boost = if high_detail { 1.0 } else { 1.56 };
    let mut emitted = 0u32;

    for p in 0..pairs {
        // Start slightly behind the limb junction (`u = -0.10..0.92`) so feathery plumes wrap branches densely!
        let u = -0.10 + 1.02 * ((p as f32 + 0.25) / (pairs as f32 + 0.10));
        let u_pos = u.clamp(0.0, 1.0);
        let anchor = leaf.position
            + horiz_dir * (frond_len * u)
            + up_dir * (frond_len * 0.20 * u_pos * u_pos);
        let env = (1.0 - 0.20 * u_pos).clamp(0.74, 1.04);
        // Lush compact feathery plume dimensions (24–32 cm long × 8.8–12.2 cm wide at LOD0;
        // every 6-tri blade edge stays <= 0.23 m — soft rounded feathery tufts, zero sharp origami diamonds!)
        let cord_len = (0.24 + 0.08 * env)
            * (0.90 + 0.20 * hash_unit(seed ^ (p as u64 * 29)))
            * lod_len_boost;
        let cord_w = (cord_len * 0.365 * (lod_w_boost / lod_len_boost))
            .clamp(0.088, if high_detail { 0.122 } else { 0.192 });

        for sign in [-1.0_f32, 1.0_f32] {
            let n_seed = seed ^ ((p as u64) * 97 + if sign > 0.0 { 13 } else { 37 });
            let (p_base, p_tip, thick) = palette.pick_colors(n_seed, false);
            let col_base = lerp_rgb(p_base, emerald_base, 0.65);
            let col_tip = lerp_rgb(p_tip, pine_tip, 0.65);
            let jit_up = 0.22 + 0.30 * u_pos + 0.10 * hash_signed(n_seed ^ 0x41);
            // Swept forward & gently upturned in lush overlapping coastal herringbone plumes
            let n_dir = (horiz_dir * 0.72 + side_dir * (sign * 0.58) + up_dir * jit_up)
                .try_normalize()
                .unwrap_or(horiz_dir);
            let roll_up = (up_dir + side_dir * (sign * 0.28))
                .try_normalize()
                .unwrap_or(up_dir);
            let spec = BladeShapeSpec {
                length: cord_len,
                max_width: cord_w,
                sickle: 0.03 * sign,
                arch: -0.25, // iconic upturned Araucaria cord curve
                keel: 0.52,  // plush rounded 3D tubular-V needle plume depth
                upper_shoulder: 0.86, // soft rounded feathery silhouette (zero sharp origami corners!)
                serrated: false,
                col_base,
                col_tip,
                thickness: thick,
                high_detail, // 6-tri curved rounded plume at LOD0, 4-tri stable plume at LOD1
            };
            append_curved_3d_leaf(foliage, anchor, n_dir, roll_up, leaf.stem_id, field, &spec);
            emitted += 1;
        }

        // Upturned dorsal feathery crest plume filling the top & inner depth of each whorl tier
        if (high_detail && p < 2) || (!high_detail && p == 0) {
            let c_seed = seed ^ ((p as u64) * 131 + 71);
            let (p_base, p_tip, thick) = palette.pick_colors(c_seed, false);
            let col_base = lerp_rgb(p_base, emerald_base, 0.55);
            let col_tip = lerp_rgb(p_tip, pine_tip, 0.70);
            let crest_dir = (horiz_dir * 0.60
                + up_dir * 0.78
                + side_dir * (0.18 * hash_signed(c_seed ^ 0x17)))
                .try_normalize()
                .unwrap_or(Vec3::Y);
            let spec = BladeShapeSpec {
                length: cord_len * 0.96,
                max_width: cord_w * 1.02,
                sickle: 0.0,
                arch: -0.22,
                keel: 0.54,
                upper_shoulder: 0.86,
                serrated: false,
                col_base,
                col_tip,
                thickness: thick,
                high_detail,
            };
            append_curved_3d_leaf(foliage, anchor, crest_dir, side_dir, leaf.stem_id, field, &spec);
            emitted += 1;
        }
    }
    emitted
}

/// Clothe every Norfolk Island Pine primary whorl bough (L1) from inner 16% near the trunk
/// out to the sweeping upturned outer tip with heavy, lush, layered dark coastal emerald
/// Araucaria needle-frond tiers so no see-through sky gaps or naked inner boughs remain!
#[allow(clippy::too_many_arguments)]
fn append_araucaria_whorl_tier_plumes(
    foliage: &mut Mesh,
    tree: &Tree,
    palette: &LeafPalette,
    field: &CrownField,
    budget_tris: u32,
    high_detail: bool,
) -> u32 {
    let l1_stems: Vec<&Stem> = tree
        .stems
        .iter()
        .filter(|s| s.level == 1 && !s.segments.is_empty())
        .collect();
    if l1_stems.is_empty() || budget_tris == 0 {
        return 0;
    }

    let emerald_base = srgb3_to_linear([0.09, 0.22, 0.10]);
    let pine_tip = srgb3_to_linear([0.24, 0.44, 0.19]);

    let plumes_per_station: usize = if high_detail { 6 } else { 3 };
    let tris_per_plume: u32 = if high_detail { 6 } else { 4 };
    let tris_per_station = plumes_per_station as u32 * tris_per_plume;
    let total_stations = (budget_tris / tris_per_station.max(1)).max(l1_stems.len() as u32) as usize;
    let stations_per_bough = (total_stations / l1_stems.len()).clamp(4, if high_detail { 15 } else { 10 });

    let lod_len_boost = if high_detail { 1.0 } else { 1.44 };
    let lod_w_boost = if high_detail { 1.0 } else { 1.58 };
    let mut emitted = 0u32;

    for (b_i, stem) in l1_stems.iter().enumerate() {
        let b_seed = tree.seed ^ ((stem.id as u64 + 1).wrapping_mul(0xA076_1D64_78BD_642F));
        for st in 0..stations_per_bough {
            // Clothe every whorl from inner 16% near the trunk all the way to the outer upturned tip!
            let frac = (st as f32 + 0.35) / stations_per_bough as f32;
            let t = 0.16 + 0.83 * frac;
            let base = stem.point_at(t);
            let raw_dir = stem.direction_at(t).try_normalize().unwrap_or(Vec3::X);
            let horiz = Vec3::new(raw_dir.x, raw_dir.y * 0.28 + 0.08, raw_dir.z)
                .try_normalize()
                .unwrap_or(raw_dir);
            let side = horiz
                .cross(Vec3::Y)
                .try_normalize()
                .unwrap_or_else(|| create_basis(horiz).0);
            let up = side.cross(horiz).try_normalize().unwrap_or(Vec3::Y);
            let st_seed = b_seed ^ ((st as u64 + 1).wrapping_mul(0xE703_7ED1_A0B4_28DB));

            // Slightly fuller coastal emerald collar near inner/mid bough, tapering naturally at outer tip
            let env = (1.06 - 0.18 * frac).clamp(0.82, 1.08);
            let cord_len = (0.25 + 0.07 * env)
                * (0.90 + 0.20 * hash_unit(st_seed ^ 0x11))
                * lod_len_boost;
            let cord_w = (cord_len * 0.38 * (lod_w_boost / lod_len_boost))
                .clamp(0.094, if high_detail { 0.126 } else { 0.200 });

            // Emit lush overlapping left/right sweeping horizontal flanks + dorsal upturned plumes
            // + slight lower-flank skirt cushions so each whorl tier reads as a dense, rich plume!
            for p_k in 0..plumes_per_station {
                let p_seed = st_seed ^ ((p_k as u64 + 1).wrapping_mul(0x9E37_79B9));
                let (p_base, p_tip, thick) = palette.pick_colors(p_seed, false);
                let inner_dark = 0.84 + 0.18 * frac;
                let col_base = mul_rgb(lerp_rgb(p_base, emerald_base, 0.70), inner_dark);
                let col_tip = mul_rgb(lerp_rgb(p_tip, pine_tip, 0.65), inner_dark);

                let (dir, roll_up, off) = match p_k {
                    // Sweeping left & right horizontal-to-upturned coastal herringbone fans
                    0 => (
                        (horiz * 0.62 + side * 0.70 + up * (0.26 + 0.22 * frac)),
                        up + side * 0.24,
                        side * 0.06 + up * 0.02,
                    ),
                    1 => (
                        (horiz * 0.62 - side * 0.70 + up * (0.26 + 0.22 * frac)),
                        up - side * 0.24,
                        -side * 0.06 + up * 0.02,
                    ),
                    // Dorsal upturned plume crest crowning the bough
                    2 => (
                        (horiz * 0.56 + up * 0.80 + side * (0.20 * hash_signed(p_seed ^ 0x33))),
                        side,
                        up * 0.05,
                    ),
                    // Forward-swept diagonal sub-plumes bridging between consecutive whorl stations
                    3 => (
                        (horiz * 0.80 + side * 0.44 + up * (0.36 + 0.18 * frac)),
                        up + side * 0.18,
                        horiz * 0.08 + side * 0.05 + up * 0.04,
                    ),
                    4 => (
                        (horiz * 0.80 - side * 0.44 + up * (0.36 + 0.18 * frac)),
                        up - side * 0.18,
                        horiz * 0.08 - side * 0.05 + up * 0.04,
                    ),
                    // Soft lower-flank emerald cushion closing gaps beneath the whorl bough
                    _ => {
                        let s_sign = if (st + b_i) % 2 == 0 { 1.0 } else { -1.0 };
                        (
                            (horiz * 0.68 + side * (s_sign * 0.62) - up * 0.12),
                            up + side * (s_sign * 0.32),
                            side * (s_sign * 0.05) - up * 0.03,
                        )
                    }
                };
                let n_dir = dir.try_normalize().unwrap_or(horiz);
                let up_h = roll_up.try_normalize().unwrap_or(up);
                let spec = BladeShapeSpec {
                    length: cord_len * (0.92 + 0.16 * hash_unit(p_seed ^ 0x55)),
                    max_width: cord_w * (0.94 + 0.12 * hash_unit(p_seed ^ 0x66)),
                    sickle: 0.03 * hash_signed(p_seed ^ 0x77),
                    arch: -0.24,
                    keel: 0.54,
                    upper_shoulder: 0.86,
                    serrated: false,
                    col_base,
                    col_tip,
                    thickness: thick,
                    high_detail,
                };
                append_curved_3d_leaf(foliage, base + off, n_dir, up_h, stem.id, field, &spec);
                emitted += 1;
            }
        }
    }
    emitted
}

/// Emit a 3D ribbed/fuzzy golden-yellow cylindrical Banksia flower candle (or weathered woody cone).
#[allow(clippy::too_many_arguments)]
fn append_banksia_candle(
    mesh: &mut Mesh,
    organ: &Organ,
    seed: u64,
    fresh_col_srgb: [f32; 3],
    tip_col_srgb: [f32; 3],
    aged_col_srgb: [f32; 3],
    field: &CrownField,
    high_detail: bool,
) {
    let is_aged = organ.cell == crate::botanical_cards::CELL_ORGAN
        || organ.cell == crate::botanical_cards::CELL_AGED;
    let base_lin = srgb3_to_linear(if is_aged { aged_col_srgb } else { fresh_col_srgb });
    let tip_lin = srgb3_to_linear(if is_aged {
        [
            aged_col_srgb[0] * 0.82,
            aged_col_srgb[1] * 0.80,
            aged_col_srgb[2] * 0.78,
        ]
    } else {
        tip_col_srgb
    });

    let axis = (organ.rotation * Vec3::Y).try_normalize().unwrap_or(Vec3::Y);
    let tan = (organ.rotation * Vec3::X)
        .try_normalize()
        .unwrap_or_else(|| create_basis(axis).0);
    let bit = axis.cross(tan).try_normalize().unwrap_or(Vec3::Z);

    let sides: u32 = if high_detail { 8 } else { 5 };
    let cols = sides + 1;
    let rings: usize = if high_detail { 9 } else { 5 };
    let half_w = (organ.width * 0.5).max(0.022);
    let v0 = mesh.vertices.len() as u32;
    let jit = 0.90 + 0.20 * hash_unit(seed ^ 0xBA41);

    for r_idx in 0..rings {
        let t = r_idx as f32 / (rings - 1) as f32;
        // Rounded bottle-brush candle envelope
        let env = if t < 0.12 {
            0.55 + 0.45 * (t / 0.12).sqrt()
        } else if t > 0.86 {
            let k = (t - 0.86) / 0.14;
            (1.0 - 0.65 * k * k).max(0.24)
        } else {
            1.0
        };
        // Alternating protruding floret ridges give real 3D fuzzy bottle-brush silhouette!
        let ridge_mod = if high_detail && r_idx > 0 && r_idx + 1 < rings {
            if r_idx % 2 == 1 { 1.16 } else { 0.86 }
        } else {
            1.0
        };
        let center = organ.position + axis * (organ.length * t);
        let base_c = lerp_rgb(base_lin, tip_lin, t);

        for s in 0..cols {
            let wrap_s = s % sides;
            let spiral = (r_idx as f32) * 0.45;
            let a = (wrap_s as f32 / sides as f32) * TAU + spiral;
            let radial = tan * a.cos() + bit * a.sin();
            // Subtle spiral floret bump around circumference
            let floret_bump = if high_detail {
                1.0 + 0.10 * ((wrap_s as f32 * 2.0 + r_idx as f32) * 1.5).sin()
            } else {
                1.0
            };
            let rad_here = half_w * env * ridge_mod * floret_bump;
            let pos = center + radial * rad_here;
            let ao = field.crown_ao(organ.stem_id, pos);
            let h_norm = field.height_norm(pos);
            // Recessed groove between floret rings is warmer/deeper golden-amber
            let groove_shade = if ridge_mod < 0.95 { 0.72 } else { 1.06 };
            let col = mul_rgb(base_c, jit * groove_shade * (0.78 + 0.24 * ao));

            mesh.vertices.push(Vertex {
                position: pos,
                normal: radial,
                uv: Vec2::new(s as f32 / sides as f32, t),
                uv2: Vec2::new(ao, h_norm),
                color: Vec4::new(col[0], col[1], col[2], if is_aged { 0.92 } else { 0.42 }),
            });
        }
    }

    for r_idx in 0..(rings as u32 - 1) {
        for s in 0..sides {
            let i00 = v0 + r_idx * cols + s;
            let i10 = i00 + 1;
            let i01 = i00 + cols;
            let i11 = i01 + 1;
            mesh.indices
                .extend_from_slice(&[i00, i01, i11, i00, i11, i10]);
        }
    }

    // Rounded candle cap
    let tip = organ.position + axis * organ.length;
    let cap_idx = mesh.vertices.len() as u32;
    let tip_ao = field.crown_ao(organ.stem_id, tip);
    let tip_c = mul_rgb(tip_lin, jit);
    mesh.vertices.push(Vertex {
        position: tip,
        normal: axis,
        uv: Vec2::new(0.5, 1.0),
        uv2: Vec2::new(tip_ao, field.height_norm(tip)),
        color: Vec4::new(tip_c[0], tip_c[1], tip_c[2], 0.45),
    });
    let top_row = v0 + (rings as u32 - 1) * cols;
    for s in 0..sides {
        mesh.indices
            .extend_from_slice(&[cap_idx, top_row + s + 1, top_row + s]);
    }
}

/// Emit a 3D woody urn (Marri honky nut) or plated cycad cone (`OrganKind::Urn` / `OrganKind::Ovoid`).
#[allow(clippy::too_many_arguments)]
fn append_3d_nut_or_cone(
    mesh: &mut Mesh,
    organ: &Organ,
    base_srgb: [f32; 3],
    tip_srgb: [f32; 3],
    field: &CrownField,
    high_detail: bool,
) {
    let base_lin = srgb3_to_linear(base_srgb);
    let tip_lin = srgb3_to_linear(tip_srgb);
    let axis = (organ.rotation * Vec3::Y).try_normalize().unwrap_or(Vec3::Y);
    let (tan, bit) = create_basis(axis);
    let sides: u32 = if high_detail { 7 } else { 4 };
    let cols = sides + 1;
    let rings: &[f32] = if high_detail {
        &[0.0, 0.22, 0.52, 0.80, 1.0]
    } else {
        &[0.0, 0.5, 1.0]
    };
    let half_w = (organ.width * 0.5).max(0.018);
    let v0 = mesh.vertices.len() as u32;

    for (ri, &t) in rings.iter().enumerate() {
        let profile = match organ.kind {
            OrganKind::Urn => {
                if t < 0.5 {
                    0.25 + 0.75 * (t * PI).sin()
                } else if t < 0.82 {
                    1.0 - 0.32 * ((t - 0.5) / 0.32)
                } else {
                    0.72
                }
            }
            _ => (t * PI).sin().max(0.0).powf(0.60).max(0.18),
        };
        // Cycad cones have diamond scale ridges
        let scale_bump = if organ.kind == OrganKind::Ovoid && high_detail && ri % 2 == 1 {
            1.12
        } else {
            1.0
        };
        let center = organ.position + axis * (organ.length * t);
        let col = lerp_rgb(base_lin, tip_lin, t);
        for s in 0..cols {
            let wrap_s = s % sides;
            let a = (wrap_s as f32 / sides as f32) * TAU + (ri as f32 * 0.4);
            let rad = tan * a.cos() + bit * a.sin();
            let pos = center + rad * (half_w * profile * scale_bump);
            let ao = field.crown_ao(organ.stem_id, pos);
            let c = mul_rgb(col, 0.75 + 0.25 * ao);
            mesh.vertices.push(Vertex {
                position: pos,
                normal: rad,
                uv: Vec2::new(s as f32 / sides as f32, t),
                uv2: Vec2::new(ao, field.height_norm(pos)),
                color: Vec4::new(c[0], c[1], c[2], 0.90),
            });
        }
    }

    for ri in 0..(rings.len() as u32 - 1) {
        for s in 0..sides {
            let i00 = v0 + ri * cols + s;
            let i10 = i00 + 1;
            let i01 = i00 + cols;
            let i11 = i01 + 1;
            mesh.indices
                .extend_from_slice(&[i00, i01, i11, i00, i11, i10]);
        }
    }
    let tip = organ.position + axis * organ.length;
    let cap_idx = mesh.vertices.len() as u32;
    mesh.vertices.push(Vertex {
        position: tip,
        normal: axis,
        uv: Vec2::new(0.5, 1.0),
        uv2: Vec2::new(field.crown_ao(organ.stem_id, tip), field.height_norm(tip)),
        color: Vec4::new(tip_lin[0], tip_lin[1], tip_lin[2], 0.90),
    });
    let top_row = v0 + (rings.len() as u32 - 1) * cols;
    for s in 0..sides {
        mesh.indices
            .extend_from_slice(&[cap_idx, top_row + s + 1, top_row + s]);
    }
}

/// Emit one unmistakable WA Red-and-Green Kangaroo Paw (`Anigozanthos manglesii`) 3D curved
/// velvety tubular flower with swollen scarlet ovary base, vivid lime-emerald tube, and
/// splayed claw-like curled petal tips ("the paw"!).
#[allow(clippy::too_many_arguments)]
fn append_kangaroo_paw_flower(
    mesh: &mut Mesh,
    origin: Vec3,
    launch_dir: Vec3,
    length: f32,
    red_lin: [f32; 3],
    green_lin: [f32; 3],
    stem_id: u32,
    field: &CrownField,
    high_detail: bool,
) {
    let fwd = launch_dir.try_normalize().unwrap_or(Vec3::Y);
    let side = fwd
        .cross(Vec3::Y)
        .try_normalize()
        .unwrap_or_else(|| create_basis(fwd).0);
    let up = side.cross(fwd).try_normalize().unwrap_or(Vec3::Y);

    let sides: u32 = if high_detail { 6 } else { 4 };
    let cols = sides + 1;
    let rings: usize = if high_detail { 5 } else { 3 };
    let tube_r = (length * 0.145).clamp(0.008, 0.018);
    let v0 = mesh.vertices.len() as u32;

    let mut last_center = origin;
    let mut last_dir = fwd;

    for ri in 0..rings {
        let t = ri as f32 / (rings - 1) as f32;
        // Graceful downward curve typical of Anigozanthos tubular paws
        let curve_drop = -length * 0.24 * t * t;
        let center = origin + fwd * (length * t) + up * curve_drop;
        let local_fwd = (fwd + up * (-0.48 * t)).try_normalize().unwrap_or(fwd);
        let local_up = side.cross(local_fwd).try_normalize().unwrap_or(up);
        last_center = center;
        last_dir = local_fwd;

        // Swollen bulbous velvety red ovary at base -> slender curved emerald tube -> flared paw throat
        let r_profile = if t < 0.28 {
            0.78 + 0.52 * (t / 0.28 * PI * 0.8).sin()
        } else if t < 0.78 {
            0.85 - 0.10 * ((t - 0.28) / 0.50)
        } else {
            0.75 + 0.35 * ((t - 0.78) / 0.22)
        };
        let r = tube_r * r_profile;

        // Crisp scarlet-red ovary base transitioning sharply into vivid lime-emerald velvety tube!
        let color_t = ((t - 0.24) / 0.22).clamp(0.0, 1.0);
        let col = lerp_rgb(red_lin, green_lin, color_t);

        for s in 0..cols {
            let wrap_s = s % sides;
            let a = (wrap_s as f32 / sides as f32) * TAU;
            let rad = side * a.cos() + local_up * a.sin();
            let pos = center + rad * r;
            let ao = field.crown_ao(stem_id, pos);
            let h_norm = field.height_norm(pos);
            let velvety = 0.86 + 0.18 * (rad.y * 0.5 + 0.5);
            let c = mul_rgb(col, velvety);
            mesh.vertices.push(Vertex {
                position: pos,
                normal: rad,
                uv: Vec2::new(s as f32 / sides as f32, t),
                uv2: Vec2::new(ao, h_norm),
                color: Vec4::new(c[0], c[1], c[2], 0.38),
            });
        }
    }

    for ri in 0..(rings as u32 - 1) {
        for s in 0..sides {
            let i00 = v0 + ri * cols + s;
            let i10 = i00 + 1;
            let i01 = i00 + cols;
            let i11 = i01 + 1;
            mesh.indices
                .extend_from_slice(&[i00, i01, i11, i00, i11, i10]);
        }
    }

    // 5-6 splayed recurved claw-like petal lobes at the mouth — the iconic "kangaroo paw" silhouette!
    let claw_count: usize = if high_detail { 6 } else { 3 };
    let claw_len = length * 0.28;
    let claw_w = tube_r * 0.85;
    let local_up = side.cross(last_dir).try_normalize().unwrap_or(up);
    let tip_col = mul_rgb(green_lin, 1.18);

    for k in 0..claw_count {
        let a = (k as f32 / claw_count as f32) * TAU;
        let rad = side * a.cos() + local_up * a.sin();
        let claw_dir = (last_dir * 0.55 + rad * 0.85).try_normalize().unwrap_or(rad);
        let spec = BladeShapeSpec {
            length: claw_len,
            max_width: claw_w,
            sickle: 0.0,
            arch: 0.35, // curled outward claw tip
            keel: 0.25,
            upper_shoulder: 0.72,
            serrated: false,
            col_base: green_lin,
            col_tip: tip_col,
            thickness: 0.30,
            high_detail: false,
        };
        append_curved_3d_leaf(
            mesh,
            last_center + rad * (tube_r * 0.85),
            claw_dir,
            last_dir,
            stem_id,
            field,
            &spec,
        );
    }
}

/// Generate 100% opaque 3D geometric foliage for rosette species:
/// `wa_grass_tree`, `wa_kangaroo_paw`, `wa_sword_sedge`, `wa_zamia`.
fn generate_geo3d_rosette(
    tree: &Tree,
    species: &Species,
    leaf_reduction: f32,
    field: &CrownField,
    high_detail: bool,
) -> Geo3dResult {
    let mut foliage = Mesh::new();
    let mut twigs = Mesh::new();
    let mut organs = Mesh::new();
    let mut unit_count = 0u32;

    let ros = species.rosette.clone().unwrap_or_default();
    let base_lin = srgb3_to_linear(ros.color_base.unwrap_or([0.32, 0.42, 0.28]));
    let tip_lin = srgb3_to_linear(ros.color_tip.unwrap_or([0.46, 0.54, 0.36]));
    let skirt_lin = srgb3_to_linear(ros.skirt_color);
    let bark_lin = srgb3_to_linear(species_bark_color(species));

    let total = tree.leaves.len();
    let keep = ((total as f32 * leaf_reduction.clamp(0.25, 1.0)).ceil() as usize).clamp(1, total.max(1));

    // 1. Live rosette blades / pinnate cycad fronds
    if ros.blade_card == BladeCard::Pinnate {
        // ZAMIA CYCAD (`wa_zamia`): thick arched 3D central woody rachis + crisp paired pointed 3D leaflets!
        let max_fronds = if high_detail { keep.min(34) } else { keep.min(18) };
        let rachis_segs = if high_detail { 6usize } else { 4usize };
        let leaflet_pairs = if high_detail { 15usize } else { 8usize };

        for i in 0..max_fronds {
            let leaf = &tree.leaves[i * total / max_fronds.max(1)];
            let seed = tree.seed ^ ((i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let bend = ros.droop * (0.90 + 0.25 * hash_unit(seed ^ 0x11));
            let twist = 0.22 * hash_signed(seed ^ 0x22);
            let (pts, sides) = ribbon_spine(
                leaf.position,
                leaf.rotation,
                leaf.scale,
                bend,
                twist,
                rachis_segs,
            );

            // Thick arched 3D central rachis stem (olive-green woody petiole)
            let rachis_col = lerp_rgb(bark_lin, base_lin, 0.65);
            for s in 0..rachis_segs {
                let u0 = s as f32 / rachis_segs as f32;
                let u1 = (s + 1) as f32 / rachis_segs as f32;
                append_woody_twiglet(
                    &mut twigs,
                    pts[s],
                    pts[s + 1],
                    0.015 * (1.0 - 0.65 * u0),
                    0.015 * (1.0 - 0.65 * u1),
                    rachis_col,
                    leaf.stem_id,
                    field,
                );
            }

            // Individual stiff glossy pointed 3D cycad leaflets angled in a crisp V along the rachis!
            let half_frond_w = (ros.blade_width * 0.5).max(0.18);
            for p in 0..leaflet_pairs {
                let u = 0.14 + 0.83 * ((p as f32 + 0.5) / leaflet_pairs as f32);
                let f_idx = u * rachis_segs as f32;
                let idx0 = (f_idx.floor() as usize).min(rachis_segs - 1);
                let frac = f_idx - idx0 as f32;
                let anchor = pts[idx0].lerp(pts[idx0 + 1], frac);
                let side_v = sides[idx0]
                    .lerp(sides[idx0 + 1], frac)
                    .try_normalize()
                    .unwrap_or(sides[idx0]);
                let tang = (pts[idx0 + 1] - pts[idx0]).try_normalize().unwrap_or(Vec3::Y);
                let up_v = side_v.cross(tang).try_normalize().unwrap_or(Vec3::Y);

                let env = (u * PI).sin().max(0.24).powf(0.45);
                let lf_len = half_frond_w * env * (0.94 + 0.12 * hash_unit(seed ^ (p as u64 * 17)));
                let lf_w = (lf_len * 0.145).clamp(0.018, 0.036);

                for sign in [-1.0_f32, 1.0_f32] {
                    let l_seed = seed ^ ((p as u64) * 53 + if sign > 0.0 { 3 } else { 11 });
                    let jit = 0.88 + 0.24 * hash_unit(l_seed);
                    let lf_dir = (side_v * (sign * 0.82) + tang * 0.44 + up_v * 0.34)
                        .try_normalize()
                        .unwrap_or(side_v * sign);
                    let spec = BladeShapeSpec {
                        length: lf_len,
                        max_width: lf_w,
                        sickle: 0.02 * sign,
                        arch: 0.10,
                        keel: 0.42,
                        upper_shoulder: 0.72,
                        serrated: false,
                        col_base: mul_rgb(base_lin, jit),
                        col_tip: mul_rgb(tip_lin, jit),
                        thickness: 0.72,
                        high_detail: false, // 4-tri crisp stiff pointed cycad leaflet
                    };
                    append_curved_3d_leaf(
                        &mut foliage,
                        anchor,
                        lf_dir,
                        up_v,
                        leaf.stem_id,
                        field,
                        &spec,
                    );
                    unit_count += 1;
                }
            }
        }
    } else {
        // GRASS TREE (`wa_grass_tree`), SWORD SEDGE (`wa_sword_sedge`), KANGAROO PAW (`wa_kangaroo_paw`):
        // Gracefully arched 3D V-keeled/triangular pointed blades!
        let is_grass_tree = ros.blade_card == BladeCard::Tuft;
        let max_blades = if is_grass_tree {
            if high_detail { keep.min(540) } else { keep.min(220) }
        } else if high_detail {
            keep
        } else {
            (keep * 3 / 4).max(36).min(total)
        };
        let segs: usize = if is_grass_tree {
            if high_detail { 4 } else { 3 }
        } else if high_detail {
            5
        } else {
            3
        };
        let base_half_w = if is_grass_tree {
            (ros.blade_width * 0.26).clamp(0.008, 0.018) // slender wiry grass-tree needles!
        } else {
            (ros.blade_width * 0.56).clamp(0.014, 0.042)
        };
        let keel = ros.keel.max(0.28);

        for i in 0..max_blades {
            let leaf = &tree.leaves[i * total / max_blades.max(1)];
            let seed = tree.seed ^ ((i as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9));
            let jit = 0.84 + 0.30 * hash_unit(seed ^ 0x41);
            let dry = hash_unit(seed ^ 0x82) < 0.09;
            let c_base = mul_rgb(if dry { skirt_lin } else { base_lin }, jit);
            let c_tip = mul_rgb(if dry { mul_rgb(skirt_lin, 1.15) } else { tip_lin }, jit);

            let limp = 1.0 - ros.stiffness.clamp(0.0, 1.0);
            let bend = (ros.droop + 0.14) * (0.85 + 0.40 * hash_unit(seed ^ 0x19));
            let twist = (0.30 + 0.85 * limp) * hash_signed(seed ^ 0x29);
            let (pts, sides) = ribbon_spine(leaf.position, leaf.rotation, leaf.scale, bend, twist, segs);

            let v_start = foliage.vertices.len() as u32;
            // Rows 0..segs have 3 vertices (L, midrib, R); final tip collapses to a sharp 1-vertex sword point!
            for r in 0..segs {
                let u = r as f32 / segs as f32;
                let tang = ribbon_tangent(&pts, r);
                let up = sides[r].cross(tang).try_normalize().unwrap_or(Vec3::Y);
                // Smooth sword taper along blade length
                let w_here = base_half_w * (1.0 - 0.65 * u * u);
                let rgb = lerp_rgb(c_base, c_tip, u);
                for &xn in &[-1.0_f32, 0.0, 1.0] {
                    let lift = keel * 0.72 * w_here * xn.abs();
                    let pos = pts[r] + sides[r] * (xn * w_here) + up * lift;
                    let geom_n = (up - sides[r] * (xn * keel * 0.75))
                        .try_normalize()
                        .unwrap_or(up);
                    let final_n = field.blend_normal(leaf.stem_id, pos, geom_n);
                    let ao = field.crown_ao(leaf.stem_id, pos);
                    let h_norm = field.height_norm(pos);
                    foliage.vertices.push(Vertex {
                        position: pos,
                        normal: final_n,
                        uv: Vec2::new(xn * 0.5 + 0.5, u),
                        uv2: Vec2::new(ao, h_norm),
                        color: Vec4::new(rgb[0], rgb[1], rgb[2], 0.52),
                    });
                }
            }
            // Pointed blade tip vertex
            let tip_pos = pts[segs];
            let tip_tang = ribbon_tangent(&pts, segs);
            let tip_up = sides[segs].cross(tip_tang).try_normalize().unwrap_or(Vec3::Y);
            let tip_n = field.blend_normal(leaf.stem_id, tip_pos, tip_up);
            let tip_idx = foliage.vertices.len() as u32;
            foliage.vertices.push(Vertex {
                position: tip_pos,
                normal: tip_n,
                uv: Vec2::new(0.5, 1.0),
                uv2: Vec2::new(field.crown_ao(leaf.stem_id, tip_pos), field.height_norm(tip_pos)),
                color: Vec4::new(c_tip[0], c_tip[1], c_tip[2], 0.45),
            });

            // Connect V-creased blade rows
            for r in 0..(segs as u32 - 1) {
                for c in 0..2u32 {
                    let i00 = v_start + r * 3 + c;
                    let i10 = i00 + 1;
                    let i01 = i00 + 3;
                    let i11 = i01 + 1;
                    foliage
                        .indices
                        .extend_from_slice(&[i00, i10, i11, i00, i11, i01]);
                }
            }
            // Connect last row to pointed tip (2 tris)
            let last_row = v_start + (segs as u32 - 1) * 3;
            foliage.indices.extend_from_slice(&[
                last_row,
                last_row + 1,
                tip_idx,
                last_row + 1,
                last_row + 2,
                tip_idx,
            ]);
            unit_count += 1;
        }
    }

    // 2. Rosette organs: dead straw skirts, Grass Tree flowering spears, Zamia cones,
    //    Sword Sedge seed plumes, and Kangaroo Paw forked velvety scarlet stems + 3D tubular paw flowers!
    if !tree.organs.is_empty() {
        let spike_cfg = ros.spike.as_ref();
        let spike_style = spike_cfg.map(|s| s.style);
        let spike_col = spike_cfg.map(|s| s.color).unwrap_or([0.45, 0.30, 0.18]);
        let spike_tip = spike_cfg.map(|s| s.tip_color).unwrap_or([0.85, 0.80, 0.62]);
        let scape_col = spike_cfg.map(|s| s.scape_color).unwrap_or([0.38, 0.33, 0.21]);

        if spike_style == Some(SpikeStyle::Raceme) {
            // KANGAROO PAW: Upgrade from bare red needle sticks to forked velvety scarlet scapes
            // carrying rich 3D curved tubular red-and-lime-green 6-clawed paw blossoms!
            let red_lin = srgb3_to_linear(spike_col);
            let green_lin = srgb3_to_linear(spike_tip);
            let scape_lin = srgb3_to_linear(scape_col);
            let scapes: Vec<&Organ> = tree
                .organs
                .iter()
                .filter(|o| o.kind == OrganKind::Cylinder)
                .collect();

            for (s_i, scape) in scapes.iter().enumerate() {
                if !high_detail && s_i >= 4 {
                    break;
                }
                let seed = tree.seed ^ ((s_i as u64 + 1).wrapping_mul(0x517C_C1B7_2722_0A95));
                let axis = (scape.rotation * Vec3::Y).try_normalize().unwrap_or(Vec3::Y);
                let (tan, bit) = create_basis(axis);
                let fork_t = 0.72;
                let fork_pt = scape.position + axis * (scape.length * fork_t);

                // Slightly arching velvety scarlet main scape
                let mid_pt = scape.position
                    + axis * (scape.length * 0.38)
                    + (tan * hash_signed(seed ^ 1) + bit * hash_signed(seed ^ 2)) * 0.035;
                let r_scape = (scape.width * 0.55).max(0.008);
                append_woody_twiglet(
                    &mut organs,
                    scape.position,
                    mid_pt,
                    r_scape * 1.15,
                    r_scape,
                    scape_lin,
                    scape.stem_id,
                    field,
                );
                append_woody_twiglet(
                    &mut organs,
                    mid_pt,
                    fork_pt,
                    r_scape,
                    r_scape * 0.82,
                    scape_lin,
                    scape.stem_id,
                    field,
                );

                // Fork into 2 velvety arched panicle arms topped with clustered 3D tubular paw blooms!
                let fork_az = hash_unit(seed ^ 0x33) * TAU;
                let fork_dir = tan * fork_az.cos() + bit * fork_az.sin();
                let arm_len = (scape.length * 0.30).clamp(0.14, 0.28);

                for arm_sign in [-1.0_f32, 1.0_f32] {
                    let arm_axis = (axis * 0.84 + fork_dir * (arm_sign * 0.52))
                        .try_normalize()
                        .unwrap_or(axis);
                    let arm_tip = fork_pt + arm_axis * arm_len;
                    append_woody_twiglet(
                        &mut organs,
                        fork_pt,
                        arm_tip,
                        r_scape * 0.78,
                        r_scape * 0.48,
                        red_lin,
                        scape.stem_id,
                        field,
                    );

                    // 4-5 distinct 3D red-and-lime-green tubular paw flowers arranged in a one-sided fan along each fork!
                    let flowers_per_arm = if high_detail { 5usize } else { 3usize };
                    let face_dir = (fork_dir * (arm_sign * 0.75) + (axis.cross(fork_dir)) * 0.45)
                        .try_normalize()
                        .unwrap_or(fork_dir * arm_sign);

                    for f_i in 0..flowers_per_arm {
                        let fu = (f_i as f32 + 0.35) / flowers_per_arm as f32;
                        let f_origin = fork_pt.lerp(arm_tip, 0.25 + 0.75 * fu);
                        let f_angle = 0.25 + 0.75 * (1.0 - fu);
                        let f_dir = (arm_axis * (1.0 - f_angle * 0.55) + face_dir * (0.65 + 0.35 * f_angle))
                            .try_normalize()
                            .unwrap_or(face_dir);
                        let f_len = spike_cfg
                            .map(|s| s.flower_length)
                            .unwrap_or(0.088)
                            * (0.92 + 0.24 * (1.0 - fu));
                        append_kangaroo_paw_flower(
                            &mut organs,
                            f_origin,
                            f_dir,
                            f_len,
                            red_lin,
                            green_lin,
                            scape.stem_id,
                            field,
                            high_detail,
                        );
                        unit_count += 1;
                    }
                }
            }
        } else {
            let reduced = reduce_organs(&tree.organs, leaf_reduction.max(0.35));
            for (idx, organ) in reduced.iter().enumerate() {
                let seed = tree.seed ^ ((idx as u64).wrapping_mul(0x94D0_49BB_1331_11EB));
                match organ.kind {
                    OrganKind::Cylinder => {
                        let is_scape = organ.v_range[1] > 0.5;
                        let col = if is_scape { scape_col } else { spike_col };
                        let tip_c = if is_scape { scape_col } else { spike_tip };
                        if !is_scape && organ.length >= 0.6 {
                            // Grass Tree tall ribbed flowering spear!
                            append_banksia_candle(
                                &mut organs,
                                organ,
                                seed,
                                col,
                                tip_c,
                                col,
                                field,
                                high_detail,
                            );
                        } else {
                            let axis = (organ.rotation * Vec3::Y).try_normalize().unwrap_or(Vec3::Y);
                            let tip = organ.position + axis * organ.length;
                            append_woody_twiglet(
                                &mut organs,
                                organ.position,
                                tip,
                                (organ.width * 0.5).max(0.006),
                                (organ.width * 0.35).max(0.004),
                                srgb3_to_linear(col),
                                organ.stem_id,
                                field,
                            );
                        }
                        unit_count += 1;
                    }
                    OrganKind::Ovoid | OrganKind::Urn => {
                        // Zamia plated cycad cones
                        append_3d_nut_or_cone(
                            &mut organs,
                            organ,
                            spike_col,
                            spike_tip,
                            field,
                            high_detail,
                        );
                        unit_count += 1;
                    }
                    OrganKind::Card => {
                        // Sword Sedge feathery rusty-brown 3D seed-spikelet panicle tufts
                        let axis = (organ.rotation * Vec3::Y).try_normalize().unwrap_or(Vec3::Y);
                        let (tan, bit) = create_basis(axis);
                        let spikelets = if high_detail { 4usize } else { 2usize };
                        let c_base = srgb3_to_linear(spike_col);
                        let c_tip = srgb3_to_linear(spike_tip);
                        for k in 0..spikelets {
                            let a = (k as f32 / spikelets as f32) * TAU + hash_unit(seed ^ k as u64);
                            let d = (axis * 0.72 + (tan * a.cos() + bit * a.sin()) * 0.68)
                                .try_normalize()
                                .unwrap_or(axis);
                            let spec = BladeShapeSpec {
                                length: (organ.length * 0.85).max(0.045),
                                max_width: (organ.width * 0.32).max(0.015),
                                sickle: 0.0,
                                arch: 0.25,
                                keel: 0.35,
                                upper_shoulder: 0.70,
                                serrated: false,
                                col_base: c_base,
                                col_tip: c_tip,
                                thickness: 0.55,
                                high_detail: false,
                            };
                            append_curved_3d_leaf(
                                &mut organs,
                                organ.position,
                                d,
                                Vec3::Y,
                                organ.stem_id,
                                field,
                                &spec,
                            );
                            unit_count += 1;
                        }
                    }
                    OrganKind::Blade => {
                        // Dead hanging straw skirt blades (Grass Tree, Sword Sedge)
                        if !high_detail && idx % 2 != 0 {
                            continue;
                        }
                        let dir = (organ.rotation * Vec3::Y).try_normalize().unwrap_or(Vec3::NEG_Y);
                        let jit = 0.82 + 0.30 * hash_unit(seed ^ 0x55);
                        let spec = BladeShapeSpec {
                            length: organ.length.clamp(0.12, 0.95),
                            max_width: (organ.width * 0.65).clamp(0.018, 0.055),
                            sickle: 0.04 * hash_signed(seed ^ 0x66),
                            arch: 0.20,
                            keel: 0.28,
                            upper_shoulder: 0.70,
                            serrated: false,
                            col_base: mul_rgb(skirt_lin, jit * 0.75),
                            col_tip: mul_rgb(skirt_lin, jit * 1.05),
                            thickness: 0.75,
                            high_detail: false,
                        };
                        append_curved_3d_leaf(
                            &mut foliage,
                            organ.position,
                            dir,
                            Vec3::Y,
                            organ.stem_id,
                            field,
                            &spec,
                        );
                        unit_count += 1;
                    }
                }
            }
        }
    }

    // Merge rosette organs into foliage mesh (ros species share Leaves material slot unless separate)
    let organ_opt = if !organs.vertices.is_empty() {
        if species.uses_separate_organ_material() {
            Some(organs)
        } else {
            let base = foliage.vertices.len() as u32;
            foliage.vertices.extend(organs.vertices);
            foliage
                .indices
                .extend(organs.indices.into_iter().map(|idx| idx + base));
            None
        }
    } else {
        None
    };

    Geo3dResult {
        foliage,
        twigs,
        organs: organ_opt,
        unit_count,
    }
}

/// Generate 100% opaque 3D geometric foliage, woody twiglets, and 3D blossoms/organs for a tree LOD.
///
/// `foliage_tri_budget` controls the target triangle count for foliage + twiglets + organs so
/// LOD0 lands cleanly in `25k–55k` total tris and LOD1 lands cleanly in `6k–14k` total tris.
pub fn generate_geo3d_foliage_mesh(
    tree: &Tree,
    species: &Species,
    leaf_reduction: f32,
    foliage_tri_budget: u32,
) -> Geo3dResult {
    let high_detail = leaf_reduction >= 0.60;
    let field = CrownField::from_tree(tree, species, high_detail);

    if species.generator.family == GeneratorFamily::Rosette {
        return generate_geo3d_rosette(tree, species, leaf_reduction, &field, high_detail);
    }

    let mut foliage = Mesh::new();
    let mut twigs = Mesh::new();
    let mut organ_mesh = Mesh::new();
    let mut unit_count = 0u32;

    let palette = LeafPalette::from_species(species);
    let bark_lin = srgb3_to_linear(species_bark_color(species));
    let upper_bark_lin = species
        .textures
        .upper_bark_color
        .map(srgb3_to_linear)
        .unwrap_or(bark_lin);
    let twig_bark_lin = if species.uses_upper_bark() {
        lerp_rgb(bark_lin, upper_bark_lin, 0.72)
    } else {
        mul_rgb(bark_lin, 0.80)
    };

    let is_jacaranda = species.leaves.shape == LeafShape::Bipinnate;
    let is_araucaria = species.leaves.shape == LeafShape::Needle
        && species.textures.leaf_card == crate::species::LeafCardLayout::Frond;
    let is_banksia = matches!(
        species.leaves.shape,
        LeafShape::Banksia | LeafShape::BullBanksia
    );
    let is_paperbark = species.leaves.shape == LeafShape::Paperbark;
    let is_peppermint = species.leaves.shape == LeafShape::Willow;
    let is_moreton_fig = species.leaves.shape == LeafShape::Oval
        && species.trunk.aerial_roots > 0;
    let is_qbox = species.leaves.shape == LeafShape::Pointed;
    let is_flooded_gum = species.leaves.shape == LeafShape::Sickle && species.uses_upper_bark();

    // Split triangle budget between foliage leaves/twigs and dedicated floral/fruiting organs
    let has_blossom_clusters = species
        .inflorescence
        .as_ref()
        .is_some_and(|i| i.kind == InflorescenceKind::Cluster && !tree.organs.is_empty());
    let has_banksia_candles = species
        .inflorescence
        .as_ref()
        .is_some_and(|i| i.kind == InflorescenceKind::Spike && !tree.organs.is_empty());

    let (leaf_tri_budget, organ_tri_budget) = if has_blossom_clusters {
        // Flowering Jacaranda / Marri: lush billowing 3D trumpet-bell domes + soft-green bipinnate sprays tucked beneath!
        let leaf_share = if is_jacaranda { 0.22 } else { 0.60 };
        let lb = (foliage_tri_budget as f32 * leaf_share) as u32;
        (lb, foliage_tri_budget.saturating_sub(lb))
    } else if has_banksia_candles {
        let ob = (foliage_tri_budget as f32 * 0.24).min(9_000.0) as u32;
        (foliage_tri_budget.saturating_sub(ob), ob)
    } else {
        (foliage_tri_budget, 0)
    };

    // -------------------------------------------------------------------------
    // 1. 100% OPAQUE 3D GEOMETRIC LEAVES & WOODY PETIOLES / TWIGLETS
    // -------------------------------------------------------------------------
    if !tree.leaves.is_empty() && leaf_tri_budget > 0 {
        let total_leaves = tree.leaves.len();

        if is_jacaranda {
            // Lush curved 3D bipinnate fern-frond sprays tucked beneath & inside the billowing purple canopy:
            // emit paired twin-spray fern fronds per leaf anchor so soft green foliage peeks warmly under
            // the November lavender cloud!
            let tris_per_spray = if high_detail { 156u32 } else { 48u32 };
            let target_fronds = ((leaf_tri_budget / tris_per_spray).max(90) as usize).min(total_leaves);
            for i in 0..target_fronds {
                let leaf = &tree.leaves[i * total_leaves / target_fronds.max(1)];
                let seed = tree.seed ^ ((i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
                unit_count += append_jacaranda_bipinnate_frond(
                    &mut foliage,
                    &mut twigs,
                    leaf,
                    seed,
                    &palette,
                    twig_bark_lin,
                    &field,
                    high_detail,
                );
                // Paired sister frond fanned ~55° sideways and slightly tucked below for lush layered green fill
                let sister_rot = glam::Quat::from_rotation_y(
                    if i % 2 == 0 { 0.95 } else { -0.95 } + 0.25 * hash_signed(seed ^ 0x41B),
                ) * leaf.rotation;
                let sister_leaf = Leaf {
                    position: leaf.position - Vec3::Y * 0.08,
                    rotation: sister_rot,
                    scale: leaf.scale * (0.86 + 0.22 * hash_unit(seed ^ 0x71E)),
                    stem_id: leaf.stem_id,
                };
                unit_count += append_jacaranda_bipinnate_frond(
                    &mut foliage,
                    &mut twigs,
                    &sister_leaf,
                    seed ^ 0xD1B5_4A32,
                    &palette,
                    twig_bark_lin,
                    &field,
                    high_detail,
                );
            }
        } else if is_araucaria {
            // Norfolk Island Pine: heavy, lush, layered dark coastal emerald needle-frond tiers!
            // Dedicate ~36% of foliage budget to clothing every primary L1 whorl bough from inner
            // 16% near the trunk to outer tips + ~64% to dense overlapping L2 feathery branchlet plumes.
            let l1_whorl_budget = (leaf_tri_budget as f32 * 0.36) as u32;
            let l2_shoot_budget = leaf_tri_budget.saturating_sub(l1_whorl_budget);
            unit_count += append_araucaria_whorl_tier_plumes(
                &mut foliage,
                tree,
                &palette,
                &field,
                l1_whorl_budget,
                high_detail,
            );
            let tris_per_shoot = if high_detail { 48u32 } else { 20u32 };
            let target_shoots = ((l2_shoot_budget / tris_per_shoot).max(480) as usize).min(total_leaves);
            for i in 0..target_shoots {
                let leaf = &tree.leaves[i * total_leaves / target_shoots.max(1)];
                let seed = tree.seed ^ ((i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
                unit_count += append_araucaria_needle_shoot(
                    &mut foliage,
                    &mut twigs,
                    leaf,
                    seed,
                    &palette,
                    twig_bark_lin,
                    &field,
                    high_detail,
                );
            }
        } else {
            // Eucalypts (Flooded Gum, Jarrah, Marri, Tuart), WA Peppermint, Slender Banksia,
            // Swamp Paperbark, Queensland Box, Moreton Bay Fig:
            // - LOD0: 10k-14k rounded-shoulder 6-tri curved botanical leaves (zero flat diamond corners!)
            // - LOD1: broader, temporally-stable clustered 4-tri shoots (~1.48x width, ~80% spherical normals)
            //   killing mid-field (65-450 m) sub-pixel shimmer while preserving lush crown silhouettes!
            let leaves_per_shoot: usize = if high_detail {
                if is_paperbark {
                    11
                } else if is_peppermint {
                    10
                } else if is_moreton_fig {
                    8
                } else {
                    9
                }
            } else if is_paperbark {
                5
            } else {
                4
            };
            let tris_per_leaf: u32 = if high_detail { 6 } else { 4 };
            let avg_twig_tris = if high_detail { 3u32 } else { 0u32 };
            let tris_per_shoot = avg_twig_tris + leaves_per_shoot as u32 * tris_per_leaf;
            let target_shoots = ((leaf_tri_budget / tris_per_shoot.max(1)).max(180) as usize)
                .min(total_leaves);

            // Species-tuned lush 3D leaf dimensions & botanical shoulder curvature
            let crown_scale_boost = (species.leaves.size / 0.85).clamp(0.94, 1.55);
            let (base_len, aspect_w, sickle, arch, keel, hang_pull, upper_shoulder) = if is_peppermint {
                // Lush weeping drapes of Agonis flexuosa willow-lance leaves
                (0.46_f32, 0.28_f32, 0.08_f32, 0.42_f32, 0.24_f32, 0.82_f32, 0.72_f32)
            } else if is_paperbark {
                // Soft billowing Melaleuca grey-sage foliage pillows
                (0.36, 0.33, 0.04, 0.24, 0.24, 0.18, 0.78)
            } else if is_banksia {
                // Dense radial whorls of serrated Banksia attenuata lance leaves
                (0.36, 0.30, 0.03, 0.19, 0.32, 0.06, 0.74)
            } else if is_moreton_fig {
                // Large glossy leathery elliptic Ficus macrophylla leaves
                (0.66, 0.54, 0.02, 0.28, 0.34, 0.22, 0.84)
            } else if is_qbox {
                // Dense glossy whorled Lophostemon confertus cohesive dome leaves
                (0.44, 0.52, 0.02, 0.24, 0.30, 0.14, 0.84)
            } else if species.leaves.shape == LeafShape::BroadEucalypt {
                // Marri (Corymbia calophylla): lush broad glossy eucalypt foliage
                (0.53 * crown_scale_boost.min(1.26), 0.45, 0.11, 0.28, 0.28, 0.36, 0.80)
            } else if is_flooded_gum {
                // Flooded Gum (Eucalyptus rudis): soft drooping silvery-olive sickle drapes
                (0.51 * crown_scale_boost.min(1.30), 0.36, 0.16, 0.35, 0.28, 0.58, 0.74)
            } else {
                // WA Eucalypts (Jarrah, Tuart): fuller cohesive curved falcate sickle crowns
                (0.51 * crown_scale_boost.min(1.32), 0.35, 0.16, 0.32, 0.28, 0.48, 0.74)
            };

            // Temporal-stability area compensation at LOD1: broaden & slightly lengthen shoots
            // (~1.42x len, ~1.52x width) with fewer tiny sub-pixel shards to kill mid-field glitter!
            let lod_len_boost = if high_detail { 1.0 } else { 1.42 };
            let lod_width_boost = if high_detail { 1.0 } else { 1.52 };
            let scale_ref = species.leaves.size.max(0.45);

            for s_idx in 0..target_shoots {
                let anchor_leaf = &tree.leaves[s_idx * total_leaves / target_shoots.max(1)];
                let shoot_seed =
                    tree.seed ^ ((s_idx as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9));
                let shoot_dir = (anchor_leaf.rotation * Vec3::Y)
                    .try_normalize()
                    .unwrap_or(Vec3::Y);
                let size_ratio = (anchor_leaf.scale / scale_ref).clamp(0.80, 1.38);

                let twig_len = (base_len * 0.96 * size_ratio).clamp(0.22, 0.56);
                let twig_r = if is_moreton_fig { 0.009 } else { 0.0055 };
                let (twig_tip, side_tip) = if high_detail && s_idx % 4 == 0 {
                    append_crooked_twig_fork(
                        &mut twigs,
                        anchor_leaf.position,
                        shoot_dir,
                        twig_len,
                        twig_r,
                        shoot_seed,
                        twig_bark_lin,
                        anchor_leaf.stem_id,
                        &field,
                    )
                } else if high_detail && s_idx % 2 == 0 {
                    let tip = anchor_leaf.position + shoot_dir * twig_len;
                    append_woody_twiglet(
                        &mut twigs,
                        anchor_leaf.position,
                        tip,
                        twig_r,
                        twig_r * 0.45,
                        twig_bark_lin,
                        anchor_leaf.stem_id,
                        &field,
                    );
                    (tip, anchor_leaf.position.lerp(tip, 0.55))
                } else {
                    let tip = anchor_leaf.position + shoot_dir * twig_len;
                    let (t_b, _) = create_basis(shoot_dir);
                    (tip, anchor_leaf.position + shoot_dir * (twig_len * 0.55) + t_b * (twig_len * 0.28))
                };

                let (tan, bit) = create_basis(shoot_dir);
                let az0 = hash_unit(shoot_seed ^ 0x17) * TAU;

                for k in 0..leaves_per_shoot {
                    let l_seed = shoot_seed ^ ((k as u64 + 1).wrapping_mul(0x94D0_49BB_1331_11EB));
                    let frac = (k as f32 + 0.5) / leaves_per_shoot as f32;
                    let jitter_r = (0.04 + 0.24 * hash_unit(l_seed ^ 0x19)) * size_ratio;
                    let az = az0 + (k as f32) * GOLDEN_ANGLE + hash_signed(l_seed ^ 0x2) * 0.34;
                    let radial = tan * az.cos() + bit * az.sin();
                    let attach = if k % 3 == 2 {
                        anchor_leaf.position.lerp(side_tip, 0.35 + 0.60 * frac) + radial * jitter_r
                    } else {
                        anchor_leaf.position.lerp(twig_tip, 0.12 + 0.86 * frac) + radial * (jitter_r * 0.65)
                    };

                    let spread = 0.55 + 0.45 * hash_unit(l_seed ^ 0x3);
                    let raw_dir = shoot_dir * (1.0 - spread * 0.42) + radial * spread
                        - Vec3::Y * (hang_pull * (0.60 + 0.55 * hash_unit(l_seed ^ 0x4)));
                    let blade_dir = raw_dir.try_normalize().unwrap_or(radial);

                    let len = base_len
                        * size_ratio
                        * lod_len_boost
                        * (0.80 + 0.42 * hash_unit(l_seed ^ 0x5));
                    let width = len
                        * aspect_w
                        * (lod_width_boost / lod_len_boost)
                        * (0.90 + 0.26 * hash_unit(l_seed ^ 0x6));
                    let sickle_sign = if (k & 1) == 0 { 1.0 } else { -1.0 };
                    let (col_base, col_tip, thickness) =
                        palette.pick_colors(l_seed, is_moreton_fig);

                    let up_hint = (Vec3::Y + radial * 0.35).try_normalize().unwrap_or(Vec3::Y);
                    let spec = BladeShapeSpec {
                        length: len,
                        max_width: width,
                        sickle: sickle * sickle_sign * (0.75 + 0.5 * hash_unit(l_seed ^ 0x7)),
                        arch: arch * (0.80 + 0.45 * hash_unit(l_seed ^ 0x8)),
                        keel,
                        upper_shoulder,
                        serrated: is_banksia,
                        col_base,
                        col_tip,
                        thickness,
                        high_detail, // 6-tri curved botanical leaf at LOD0, 4-tri broader cluster blade at LOD1
                    };
                    append_curved_3d_leaf(
                        &mut foliage,
                        attach,
                        blade_dir,
                        up_hint,
                        anchor_leaf.stem_id,
                        &field,
                        &spec,
                    );
                    unit_count += 1;
                }
            }
        }
    }

    // -------------------------------------------------------------------------
    // 2. 100% OPAQUE 3D BLOSSOMS, BANKSIA CANDLES & HONKY NUTS (`tree.organs`)
    // -------------------------------------------------------------------------
    if !tree.organs.is_empty() {
        let inf = species.inflorescence.as_ref();
        let inf_col = inf.map(|i| i.color).unwrap_or([0.58, 0.42, 0.90]);
        let inf_tip = inf.and_then(|i| i.color_tip).unwrap_or(inf_col);
        let aged_col = inf.map(|i| i.aged_color).unwrap_or([0.42, 0.36, 0.28]);

        if has_blossom_clusters && organ_tri_budget > 0 {
            // PERTH JACARANDA (and flowering Marri):
            // ~950-1,100 wide overlapping billowing dome panicles × 15-16 lush 3D flared lavender-violet
            // trumpet bells (`8.2-12.8 cm` long) = 14,000-17,000+ 3D bells providing ~2.4x canopy fill
            // so weathered dark-brown limbs peek organically through dense billowing purple domes!
            let throat_srgb = if is_jacaranda {
                // Deep rich royal violet-indigo inside tubular Jacaranda bell throat
                [0.24, 0.13, 0.58]
            } else {
                [inf_col[0] * 0.80, inf_col[1] * 0.76, inf_col[2] * 0.65]
            };
            let rim_srgb = if is_jacaranda {
                // Luminous sunlit lilac-lavender flared bell rim
                [0.76, 0.60, 0.99]
            } else {
                inf_tip
            };
            let throat_lin = srgb3_to_linear(throat_srgb);
            let rim_lin = srgb3_to_linear(rim_srgb);
            let dark_twig_lin = mul_rgb(bark_lin, 0.56);

            let bells_per_panicle: usize = if high_detail { 15 } else { 7 };
            let tris_per_bell: u32 = if high_detail { 4 } else { 3 };
            let twig_tris_per_panicle: u32 = if high_detail { 4 } else { 0 };
            let tris_per_panicle = twig_tris_per_panicle + bells_per_panicle as u32 * tris_per_bell;
            let total_organs = tree.organs.len();
            let total_target_panicles = ((organ_tri_budget / tris_per_panicle.max(1)).max(220) as usize)
                .min(total_organs);

            // On flowering Perth Jacaranda, reserve ~22% of panicles to drape billowing dome puffs
            // across upper L1/L2 inner-crown limb spans so crooked dark limbs peek organically
            // through a lush continuous lavender dome (ZERO bare-stick sky gaps!).
            let dome_fill_panicles = if is_jacaranda {
                (total_target_panicles * 22 / 100).max(40)
            } else {
                0
            };
            let target_panicles = total_target_panicles.saturating_sub(dome_fill_panicles);

            let bell_size_boost = if high_detail { 1.0 } else { 1.56 };

            let mut emit_panicle = |pos: Vec3,
                                    panicle_axis: Vec3,
                                    organ_len: f32,
                                    stem_id: u32,
                                    p_seed: u64,
                                    emit_twig: bool| {
                // Wider, fuller billowing panicle dome (`0.46..0.72 m` span, ~2.5x coverage vs v2!)
                let panicle_len = (organ_len * 0.72).clamp(0.46, 0.72);

                let (twig_tip, side_tip) = if emit_twig {
                    append_crooked_twig_fork(
                        &mut twigs,
                        pos - panicle_axis * 0.10,
                        panicle_axis,
                        panicle_len * 0.68,
                        0.0055,
                        p_seed,
                        dark_twig_lin,
                        stem_id,
                        &field,
                    )
                } else {
                    let tip = pos + panicle_axis * panicle_len;
                    let (t_b, _) = create_basis(panicle_axis);
                    (
                        tip,
                        pos + panicle_axis * (panicle_len * 0.5) + t_b * (panicle_len * 0.38),
                    )
                };

                let (tan, bit) = create_basis(panicle_axis);
                let az0 = hash_unit(p_seed ^ 0x55) * TAU;
                let panicle_r = panicle_len * 0.82;

                for b_i in 0..bells_per_panicle {
                    let b_seed = p_seed ^ ((b_i as u64 + 1).wrapping_mul(0x517C_C1B7));
                    let u = (b_i as f32 + 0.5) / bells_per_panicle as f32;
                    // Full billowing dome-panicle envelope overlapping neighbouring branches
                    let dome_profile = (1.0 - 0.42 * u * u).clamp(0.48, 1.0);
                    let cone_r =
                        panicle_r * dome_profile * (0.42 + 0.74 * hash_unit(b_seed ^ 0x19));
                    let az = az0 + b_i as f32 * GOLDEN_ANGLE + hash_signed(b_seed ^ 0x11) * 0.28;
                    let radial = tan * az.cos() + bit * az.sin();
                    let spine_pt = if b_i % 3 == 2 {
                        pos.lerp(side_tip, 0.18 + 0.78 * u)
                    } else {
                        pos.lerp(twig_tip, 0.06 + 0.92 * u)
                    };
                    let base_pt = spine_pt + radial * cone_r;

                    let spread = 0.48 + 0.52 * (1.0 - u * 0.45);
                    let bell_axis = (panicle_axis * (0.72 - 0.32 * spread) + radial * spread)
                        .try_normalize()
                        .unwrap_or(radial);

                    // Lush, temporally-stable 3D Jacaranda trumpet bells (`8.8-13.6 cm` long at LOD0,
                    // broadened at LOD1 for continuous billowing purple domes with zero sub-pixel shimmer!)
                    let bell_len = (0.088 + 0.048 * hash_unit(b_seed ^ 0x33)) * bell_size_boost;
                    let bell_rad = bell_len * (0.52 + 0.12 * hash_unit(b_seed ^ 0x44));
                    append_jacaranda_trumpet_bell(
                        &mut organ_mesh,
                        base_pt,
                        bell_axis,
                        bell_len,
                        bell_rad,
                        b_seed,
                        throat_lin,
                        rim_lin,
                        stem_id,
                        &field,
                        high_detail,
                    );
                    unit_count += 1;
                }
            };

            for p_i in 0..target_panicles {
                let organ = &tree.organs[p_i * total_organs / target_panicles.max(1)];
                if organ.kind != OrganKind::Card {
                    continue;
                }
                let p_seed = tree.seed ^ ((p_i as u64).wrapping_mul(0x94D0_49BB_1331_11EB));
                let panicle_axis = (organ.rotation * Vec3::Y)
                    .try_normalize()
                    .unwrap_or(Vec3::Y);
                emit_panicle(
                    organ.position,
                    panicle_axis,
                    organ.length,
                    organ.stem_id,
                    p_seed,
                    high_detail && p_i % 4 == 0,
                );
            }

            if dome_fill_panicles > 0 {
                let dome_stems: Vec<&Stem> = tree
                    .stems
                    .iter()
                    .filter(|s| (s.level == 1 || s.level == 2) && !s.segments.is_empty())
                    .collect();
                if !dome_stems.is_empty() {
                    for d_i in 0..dome_fill_panicles {
                        let d_seed =
                            tree.seed ^ ((d_i as u64 + 1).wrapping_mul(0xD6E8_FEB8_6659_FD93));
                        let stem = dome_stems[d_i % dome_stems.len()];
                        let t = 0.42 + 0.56 * hash_unit(d_seed ^ 0x21);
                        let base = stem.point_at(t);
                        let s_dir = stem.direction_at(t).try_normalize().unwrap_or(Vec3::Y);
                        let (tan, bit) = create_basis(s_dir);
                        let az = hash_unit(d_seed ^ 0x37) * TAU;
                        let side_off = (tan * az.cos() + bit * az.sin())
                            * (0.28 + 0.46 * hash_unit(d_seed ^ 0x49));
                        let lift = Vec3::Y * (0.22 + 0.34 * hash_unit(d_seed ^ 0x5B));
                        let axis = (Vec3::Y * 0.72 + side_off.normalize_or_zero() * 0.42)
                            .try_normalize()
                            .unwrap_or(Vec3::Y);
                        emit_panicle(
                            base + side_off + lift,
                            axis,
                            0.92,
                            stem.id,
                            d_seed,
                            high_detail && d_i % 3 == 0,
                        );
                    }
                }
            }
        } else {
            // Banksia golden candles, Marri hanging woody honky nuts, etc.
            let reduced = reduce_organs(&tree.organs, leaf_reduction.max(0.35));
            let max_organs = if has_banksia_candles && organ_tri_budget > 0 {
                let tris_per_candle = if high_detail { 136u32 } else { 45u32 };
                ((organ_tri_budget / tris_per_candle).max(32)) as usize
            } else if high_detail {
                120usize
            } else {
                48usize
            };
            let stride = (reduced.len() / max_organs.max(1)).max(1);

            for (idx, organ) in reduced.iter().enumerate() {
                if idx % stride != 0 {
                    continue;
                }
                let seed = tree.seed ^ ((idx as u64).wrapping_mul(0x94D0_49BB_1331_11EB));
                match organ.kind {
                    OrganKind::Cylinder => {
                        append_banksia_candle(
                            &mut organ_mesh,
                            organ,
                            seed,
                            inf_col,
                            inf_tip,
                            aged_col,
                            &field,
                            high_detail,
                        );
                        unit_count += 1;
                    }
                    OrganKind::Urn | OrganKind::Ovoid => {
                        append_3d_nut_or_cone(
                            &mut organ_mesh,
                            organ,
                            inf_col,
                            aged_col,
                            &field,
                            high_detail,
                        );
                        unit_count += 1;
                    }
                    OrganKind::Card | OrganKind::Blade => {
                        let dir = (organ.rotation * Vec3::Y).try_normalize().unwrap_or(Vec3::Y);
                        let spec = BladeShapeSpec {
                            length: organ.length.clamp(0.08, 0.45),
                            max_width: organ.width.clamp(0.04, 0.24),
                            sickle: 0.0,
                            arch: 0.22,
                            keel: 0.28,
                            upper_shoulder: 0.76,
                            serrated: false,
                            col_base: srgb3_to_linear(inf_col),
                            col_tip: srgb3_to_linear(inf_tip),
                            thickness: 0.35,
                            high_detail: false,
                        };
                        append_curved_3d_leaf(
                            &mut organ_mesh,
                            organ.position,
                            dir,
                            Vec3::Y,
                            organ.stem_id,
                            &field,
                            &spec,
                        );
                        unit_count += 1;
                    }
                }
            }
        }
    }

    // Also add extra fine crooked woody L4 twigs on terminal L3 stems that lack L4 children
    // so canopies viewed from underneath show dense dark branching structure!
    if high_detail && !is_araucaria && foliage_tri_budget >= 20_000 {
        let terminal_l3: Vec<&Stem> = tree
            .stems
            .iter()
            .filter(|s| s.level >= 2 && s.child_ids.is_empty() && !s.segments.is_empty())
            .collect();
        let max_extra_twigs = 96usize;
        let step = (terminal_l3.len() / max_extra_twigs).max(1);
        for (i, stem) in terminal_l3.iter().enumerate().step_by(step) {
            let seed = tree.seed ^ ((stem.id as u64).wrapping_mul(0xC6A4_A793_5BD1_E995));
            let tip = stem.tip();
            let dir = stem.direction_at(1.0);
            let r = stem.radius_at(1.0).clamp(0.007, 0.018);
            let len = (stem.length() * 0.32).clamp(0.18, 0.48);
            let _ = append_crooked_twig_fork(
                &mut twigs,
                tip,
                dir,
                len,
                r,
                seed ^ i as u64,
                twig_bark_lin,
                stem.id,
                &field,
            );
        }
    }

    let organ_opt = if !organ_mesh.vertices.is_empty() {
        if species.uses_separate_organ_material() {
            Some(organ_mesh)
        } else {
            let base = foliage.vertices.len() as u32;
            foliage.vertices.extend(organ_mesh.vertices);
            foliage
                .indices
                .extend(organ_mesh.indices.into_iter().map(|idx| idx + base));
            None
        }
    } else {
        None
    };

    Geo3dResult {
        foliage,
        twigs,
        organs: organ_opt,
        unit_count,
    }
}
