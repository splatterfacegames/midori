//! Closed low-polygon opaque foliage synthesis (`--foliage opaque`).
//!
//! Used by both single-tree exports (`midori generate --foliage opaque`) and
//! volumetric nature stands (`midori stand --foliage opaque`) for renderers that
//! avoid alpha-tested or alpha-blended materials on their fastest path.

use crate::constants::TAU;
use crate::mesh::{Mesh, Vertex};
use crate::organs::{OrganKind, reduce_organs};
use crate::species::{GeneratorFamily, Species};
use crate::tree::{Leaf, Tree};
use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Foliage rendering mode for plant and stand exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoliageRenderMode {
    /// Default alpha-tested botanical cards (`ALPHA_MODE = MASK`).
    #[default]
    Cutout,
    /// Closed low-polygon opaque geometry shaped from actual leaf clusters (`ALPHA_MODE = OPAQUE`).
    Opaque,
}

/// Exact sRGB to linear transfer for glTF 2.0 `COLOR_0`.
#[inline]
pub fn srgb_to_linear(c: f32) -> f32 {
    let x = c.clamp(0.0, 1.0);
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

/// Convert an sRGB triplet to linear RGB for `COLOR_0`.
#[inline]
pub fn srgb3_to_linear(rgb: [f32; 3]) -> [f32; 3] {
    [
        srgb_to_linear(rgb[0]),
        srgb_to_linear(rgb[1]),
        srgb_to_linear(rgb[2]),
    ]
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

/// Base icosahedron vertices (normalized) and outward-wound triangle indices.
fn icosahedron() -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let phi = (1.0 + 5.0_f32.sqrt()) * 0.5;
    let raw = [
        Vec3::new(-1.0, phi, 0.0),
        Vec3::new(1.0, phi, 0.0),
        Vec3::new(-1.0, -phi, 0.0),
        Vec3::new(1.0, -phi, 0.0),
        Vec3::new(0.0, -1.0, phi),
        Vec3::new(0.0, 1.0, phi),
        Vec3::new(0.0, -1.0, -phi),
        Vec3::new(0.0, 1.0, -phi),
        Vec3::new(phi, 0.0, -1.0),
        Vec3::new(phi, 0.0, 1.0),
        Vec3::new(-phi, 0.0, -1.0),
        Vec3::new(-phi, 0.0, 1.0),
    ];
    let verts = raw.into_iter().map(|v| v.normalize()).collect();
    let tris = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    (verts, tris)
}

/// Base octahedron vertices (6 verts, 8 tris) for ultra-low-poly distant/merged lobes.
fn octahedron() -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let verts = vec![Vec3::Y, Vec3::X, Vec3::Z, -Vec3::X, -Vec3::Z, -Vec3::Y];
    let tris = vec![
        [0, 2, 1],
        [0, 3, 2],
        [0, 4, 3],
        [0, 1, 4],
        [5, 1, 2],
        [5, 2, 3],
        [5, 3, 4],
        [5, 4, 1],
    ];
    (verts, tris)
}

fn subdivide_unit_sphere(verts: &[Vec3], tris: &[[u32; 3]]) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let mut out_v = verts.to_vec();
    let mut out_t = Vec::with_capacity(tris.len() * 4);
    let mut mid_cache: BTreeMap<(u32, u32), u32> = BTreeMap::new();

    let mut midpoint = |a: u32, b: u32, v: &mut Vec<Vec3>| -> u32 {
        let key = if a < b { (a, b) } else { (b, a) };
        if let Some(&idx) = mid_cache.get(&key) {
            return idx;
        }
        let m = (v[a as usize] + v[b as usize]).normalize();
        let idx = v.len() as u32;
        v.push(m);
        mid_cache.insert(key, idx);
        idx
    };

    for &[a, b, c] in tris {
        let ab = midpoint(a, b, &mut out_v);
        let bc = midpoint(b, c, &mut out_v);
        let ca = midpoint(c, a, &mut out_v);
        out_t.push([a, ab, ca]);
        out_t.push([b, bc, ab]);
        out_t.push([c, ca, bc]);
        out_t.push([ab, bc, ca]);
    }
    (out_v, out_t)
}

/// Append a closed, organically sculpted low-polygon foliage lobe to `mesh`.
///
/// - `detail == 0`: 8 triangles (octahedron)
/// - `detail == 1`: 20 triangles (icosahedron)
/// - `detail >= 2`: 80 triangles (1-subdivided icosahedron)
#[allow(clippy::too_many_arguments)]
pub fn append_sculpted_lobe(
    mesh: &mut Mesh,
    center: Vec3,
    radii: Vec3,
    seed: u64,
    detail: u8,
    color_top_srgb: [f32; 3],
    color_bot_srgb: [f32; 3],
    ao: f32,
) {
    let (unit_verts, tris) = match detail {
        0 => octahedron(),
        1 => icosahedron(),
        _ => {
            let (v, t) = icosahedron();
            subdivide_unit_sphere(&v, &t)
        }
    };

    let base_idx = mesh.vertices.len() as u32;
    let p1 = hash_unit(seed ^ 0x11) * TAU;
    let p2 = hash_unit(seed ^ 0x22) * TAU;
    let p3 = hash_unit(seed ^ 0x33) * TAU;

    let top_lin = srgb3_to_linear(color_top_srgb);
    let bot_lin = srgb3_to_linear(color_bot_srgb);
    let ao_clamped = ao.clamp(0.22, 1.0);

    let mut positions = Vec::with_capacity(unit_verts.len());
    for (vi, &dir) in unit_verts.iter().enumerate() {
        // Organic multi-frequency botanical crinkle so lobes read as tufted foliage clusters, not geometric polyhedra.
        let wobble = 1.0
            + 0.20 * (dir.x * 3.1 + p1).sin() * (dir.z * 2.9 + p2).cos()
            + 0.14 * (dir.y * 3.7 + p3).sin() * (dir.x * 2.1 - dir.z * 2.4 + p1).cos()
            + 0.10 * (hash_unit(seed ^ (vi as u64).wrapping_mul(0x9E37_79B9)) - 0.5);
        // Flatten underside gently for authentic eucalypt/banksia umbrella crown tiers.
        let y_squash = if dir.y < 0.0 { 0.78 } else { 1.02 };
        let local = Vec3::new(
            dir.x * radii.x * wobble,
            dir.y * radii.y * wobble * y_squash,
            dir.z * radii.z * wobble,
        );
        positions.push(center + local);
    }

    // Accumulate smooth outward normals from displaced triangles.
    let mut normals = vec![Vec3::ZERO; positions.len()];
    for &[a, b, c] in &tris {
        let pa = positions[a as usize];
        let pb = positions[b as usize];
        let pc = positions[c as usize];
        let n = (pb - pa).cross(pc - pa);
        normals[a as usize] += n;
        normals[b as usize] += n;
        normals[c as usize] += n;
    }

    for (vi, (&pos, &dir)) in positions.iter().zip(unit_verts.iter()).enumerate() {
        let normal = normals[vi].try_normalize().unwrap_or(dir);
        let up_t = (normal.y * 0.5 + 0.5).clamp(0.0, 1.0);
        // High-contrast dappled foliage variation breaks up smooth shading on low-poly volumes
        let v_jit = 0.78 + 0.34 * hash_unit(seed ^ (vi as u64).wrapping_mul(0x517C_C1B7));
        let shade = ao_clamped * (0.58 + 0.44 * up_t) * v_jit;
        let col = [
            (bot_lin[0] + (top_lin[0] - bot_lin[0]) * up_t) * shade,
            (bot_lin[1] + (top_lin[1] - bot_lin[1]) * up_t) * shade,
            (bot_lin[2] + (top_lin[2] - bot_lin[2]) * up_t) * shade,
            1.0,
        ];
        let u = (dir.x.atan2(dir.z) / TAU + 0.5).clamp(0.0, 1.0);
        let v = (0.5 - dir.y * 0.5).clamp(0.0, 1.0);

        mesh.vertices.push(Vertex {
            position: pos,
            normal,
            uv: Vec2::new(u, v),
            uv2: Vec2::new(0.35, hash_unit(seed ^ 0x77)),
            color: col.into(),
        });
    }

    for &[a, b, c] in &tris {
        mesh.indices.push(base_idx + a);
        mesh.indices.push(base_idx + b);
        mesh.indices.push(base_idx + c);
    }
}

/// Append a closed, tapered low-poly bark tube (used for stand-level trunks and edge pillars).
#[allow(clippy::too_many_arguments)]
pub fn append_tapered_trunk_tube(
    mesh: &mut Mesh,
    base: Vec3,
    tip: Vec3,
    r_base: f32,
    r_tip: f32,
    sides: u32,
    bark_srgb: [f32; 3],
    charred_base: bool,
) {
    let sides = sides.max(3);
    let axis = (tip - base).try_normalize().unwrap_or(Vec3::Y);
    let ref_v = if axis.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let t = axis.cross(ref_v).normalize();
    let b = axis.cross(t).normalize();

    let bark_lin = srgb3_to_linear(bark_srgb);
    let base_lin = if charred_base {
        [bark_lin[0] * 0.16, bark_lin[1] * 0.15, bark_lin[2] * 0.14]
    } else {
        [bark_lin[0] * 0.80, bark_lin[1] * 0.80, bark_lin[2] * 0.80]
    };

    let v0 = mesh.vertices.len() as u32;
    for ring in 0..2 {
        let (center, rad, col_rgb, uv_v) = if ring == 0 {
            (base, r_base.max(0.015), base_lin, 0.0)
        } else {
            (tip, r_tip.max(0.01), bark_lin, 1.0)
        };
        for s in 0..sides {
            let ang = (s as f32 / sides as f32) * TAU;
            let n = (t * ang.cos() + b * ang.sin()).normalize();
            let pos = center + n * rad;
            mesh.vertices.push(Vertex {
                position: pos,
                normal: n,
                uv: Vec2::new(s as f32 / sides as f32, uv_v),
                uv2: Vec2::ZERO,
                color: [col_rgb[0], col_rgb[1], col_rgb[2], 1.0].into(),
            });
        }
    }

    for s in 0..sides {
        let next = (s + 1) % sides;
        let b0 = v0 + s;
        let b1 = v0 + next;
        let t0 = v0 + sides + s;
        let t1 = v0 + sides + next;
        mesh.indices.extend_from_slice(&[b0, b1, t1, b0, t1, t0]);
    }
    // Cap top so tube is closed
    let cap_center = mesh.vertices.len() as u32;
    mesh.vertices.push(Vertex {
        position: tip,
        normal: axis,
        uv: Vec2::splat(0.5),
        uv2: Vec2::ZERO,
        color: [bark_lin[0], bark_lin[1], bark_lin[2], 1.0].into(),
    });
    for s in 0..sides {
        let next = (s + 1) % sides;
        mesh.indices
            .extend_from_slice(&[cap_center, v0 + sides + s, v0 + sides + next]);
    }
}

/// Append a closed 3D opaque spiky rosette tuft (grass tree, zamia, sword sedge, spinifex)
/// built from tapered 3-sided arched blade wedges — zero alpha, unmistakable spiky silhouette!
#[allow(clippy::too_many_arguments)]
pub fn append_opaque_rosette_tuft(
    mesh: &mut Mesh,
    center: Vec3,
    radius_xz: f32,
    height_y: f32,
    blade_count: u32,
    seed: u64,
    tip_srgb: [f32; 3],
    base_srgb: [f32; 3],
    ao: f32,
) {
    let n = blade_count.clamp(5, 84);
    let top_lin = srgb3_to_linear(tip_srgb);
    let bot_lin = srgb3_to_linear(base_srgb);
    let ao_c = ao.clamp(0.35, 1.0);
    let golden_angle = 2.399_963_2_f32;

    for i in 0..n {
        let frac = (i as f32 + 0.5) / n as f32;
        let h = hash_unit(seed ^ (i as u64).wrapping_mul(0x9E37_79B9));
        let az = i as f32 * golden_angle + (h - 0.5) * 0.28;
        // Elevation spans from lower drooping skirt needles to upright central spears
        let elev = (-0.12 + 1.06 * (1.0 - frac)).clamp(-0.15, 0.94) * (std::f32::consts::FRAC_PI_2);
        let dir = Vec3::new(az.cos() * elev.cos(), elev.sin(), az.sin() * elev.cos()).normalize();
        let right = Vec3::new(-az.sin(), 0.0, az.cos()).normalize();
        let up = right.cross(dir).normalize_or_zero();

        let len = (radius_xz * (0.76 + 0.36 * elev.cos()) + height_y * 0.42 * elev.sin().max(0.0))
            * (0.86 + 0.28 * h);
        // Fine needle-like blade width so Xanthorrhoea grass trees never read as wide agave daggers
        let half_w = (radius_xz * 0.052).clamp(0.014, 0.085);
        let thick = half_w * 0.60;

        let p_base = center + dir * (radius_xz * 0.06);
        let p_mid = center + dir * (len * 0.55) + Vec3::Y * (len * 0.10 * elev.cos());
        let p_tip = center + dir * len - Vec3::Y * (len * 0.18 * elev.cos());

        let b0 = p_base - right * half_w;
        let b1 = p_base + right * half_w;
        let b2 = p_base + up * thick;
        let m0 = p_mid - right * (half_w * 0.62);
        let m1 = p_mid + right * (half_w * 0.62);
        let m2 = p_mid + up * (thick * 0.65);

        let v_jit = 0.88 + 0.24 * hash_unit(seed ^ ((i as u64 + 17).wrapping_mul(0xBF58_476D)));
        let c_base = [
            bot_lin[0] * ao_c * 0.62 * v_jit,
            bot_lin[1] * ao_c * 0.62 * v_jit,
            bot_lin[2] * ao_c * 0.62 * v_jit,
            1.0,
        ];
        let c_mid = [
            (bot_lin[0] * 0.45 + top_lin[0] * 0.55) * ao_c * 0.88 * v_jit,
            (bot_lin[1] * 0.45 + top_lin[1] * 0.55) * ao_c * 0.88 * v_jit,
            (bot_lin[2] * 0.45 + top_lin[2] * 0.55) * ao_c * 0.88 * v_jit,
            1.0,
        ];
        let c_tip = [
            top_lin[0] * ao_c * 1.05 * v_jit,
            top_lin[1] * ao_c * 1.05 * v_jit,
            top_lin[2] * ao_c * 1.05 * v_jit,
            1.0,
        ];

        let base_idx = mesh.vertices.len() as u32;
        for (pos, norm, col, uv) in [
            (
                b0,
                (-right - up * 0.4).normalize(),
                c_base,
                Vec2::new(0.0, 0.0),
            ),
            (
                b1,
                (right - up * 0.4).normalize(),
                c_base,
                Vec2::new(1.0, 0.0),
            ),
            (b2, up, c_base, Vec2::new(0.5, 0.0)),
            (
                m0,
                (-right + up * 0.3).normalize(),
                c_mid,
                Vec2::new(0.1, 0.55),
            ),
            (
                m1,
                (right + up * 0.3).normalize(),
                c_mid,
                Vec2::new(0.9, 0.55),
            ),
            (m2, up, c_mid, Vec2::new(0.5, 0.55)),
            (
                p_tip,
                (dir + Vec3::Y * 0.4).normalize(),
                c_tip,
                Vec2::new(0.5, 1.0),
            ),
        ] {
            mesh.vertices.push(Vertex {
                position: pos,
                normal: norm,
                uv,
                uv2: Vec2::new(0.5, h),
                color: col.into(),
            });
        }

        // Closed 3-sided prism (base -> mid) + 3-sided pyramid (mid -> tip) = 9 tris per blade
        mesh.indices.extend_from_slice(&[
            base_idx,
            base_idx + 1,
            base_idx + 4,
            base_idx,
            base_idx + 4,
            base_idx + 3,
            base_idx + 1,
            base_idx + 2,
            base_idx + 5,
            base_idx + 1,
            base_idx + 5,
            base_idx + 4,
            base_idx + 2,
            base_idx,
            base_idx + 3,
            base_idx + 2,
            base_idx + 3,
            base_idx + 5,
            base_idx + 3,
            base_idx + 4,
            base_idx + 6,
            base_idx + 4,
            base_idx + 5,
            base_idx + 6,
            base_idx + 5,
            base_idx + 3,
            base_idx + 6,
        ]);
    }
}

/// Extract species foliage palette `(top_srgb, bottom_srgb)` for opaque geometry.
pub fn species_foliage_colors(species: &Species) -> ([f32; 3], [f32; 3]) {
    if species.generator.family == GeneratorFamily::Rosette
        && let Some(ref ros) = species.rosette
    {
        let base = ros.color_base.unwrap_or([0.22, 0.30, 0.18]);
        let tip = ros.color_tip.unwrap_or([0.38, 0.46, 0.28]);
        return (tip, [base[0] * 0.72, base[1] * 0.74, base[2] * 0.70]);
    }
    let top = species.textures.leaf_color.unwrap_or([0.30, 0.40, 0.24]);
    // Shaded underside of opaque foliage clusters should be deeper/cooler than sunlit tips,
    // never brighter pale underside colour (which washes out crown depth).
    let bot = [top[0] * 0.50, top[1] * 0.56, top[2] * 0.48];
    (top, bot)
}

/// Extract species bark sRGB colour.
pub fn species_bark_color(species: &Species) -> [f32; 3] {
    species.textures.bark_color.unwrap_or([0.42, 0.37, 0.32])
}

/// Generate closed low-polygon opaque foliage (and optional separate organ mesh) for a plant LOD.
pub fn generate_opaque_foliage_mesh(
    tree: &Tree,
    species: &Species,
    leaf_reduction: f32,
    is_impostor_lod: bool,
) -> (Mesh, Option<Mesh>) {
    let mut foliage = Mesh::new();
    let mut organ_mesh_opt: Option<Mesh> = None;

    let (top_col, bot_col) = species_foliage_colors(species);
    let new_col = species.textures.leaf_new_growth_color;
    let new_frac = species.textures.leaf_new_growth_fraction;

    let crown_center = tree.bounds.center();
    let crown_half = (tree.bounds.size() * 0.5).max(Vec3::splat(0.25));

    // Group leaves into volumetric clusters by ancestor clump stem (or single crown for impostor LOD)
    if !tree.leaves.is_empty() {
        if is_impostor_lod {
            // Single compact closed crown hull matching actual leaf bounding box
            let (mut min_p, mut max_p) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for leaf in &tree.leaves {
                let pad = Vec3::splat(leaf.scale * 0.45);
                min_p = min_p.min(leaf.position - pad);
                max_p = max_p.max(leaf.position + pad);
            }
            let c = (min_p + max_p) * 0.5;
            let r = ((max_p - min_p) * 0.5).max(Vec3::splat(0.18));
            append_sculpted_lobe(
                &mut foliage,
                c,
                r,
                tree.seed ^ 0x55AA,
                1,
                top_col,
                bot_col,
                0.92,
            );
        } else if species.generator.family == GeneratorFamily::Rosette {
            // Rosette crown: crisp closed 3D spiky arched blade darts (grass tree, zamia, sedge, palm)
            let (mut min_p, mut max_p) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for leaf in &tree.leaves {
                let tip = leaf.position + leaf.rotation * (Vec3::Y * leaf.scale * 0.85);
                min_p = min_p.min(leaf.position).min(tip);
                max_p = max_p.max(leaf.position).max(tip);
            }
            let c_y = species
                .trunk
                .height
                .max(min_p.y + (max_p.y - min_p.y) * 0.22);
            let center = Vec3::new((min_p.x + max_p.x) * 0.5, c_y, (min_p.z + max_p.z) * 0.5);
            let r_xz = ((max_p.x - min_p.x).max(max_p.z - min_p.z) * 0.52).max(0.28);
            let h_y = ((max_p.y - min_p.y) * 0.75).max(0.28);
            let blade_n = if leaf_reduction >= 0.75 { 72 } else { 32 };
            append_opaque_rosette_tuft(
                &mut foliage,
                center,
                r_xz,
                h_y,
                blade_n,
                tree.seed ^ 0x51A7,
                top_col,
                bot_col,
                0.94,
            );
        } else {
            // Branching tree: cluster leaves by fine branch stems (level 3 for LOD0, level 2 for LOD1+)
            // and subdivide any oversized cluster into organic overlapping sub-tufts so no giant blobs form.
            let target_level = if leaf_reduction >= 0.75 { 3 } else { 2 };
            let stem_lookup: BTreeMap<u32, (u8, Option<u32>)> = tree
                .stems
                .iter()
                .map(|s| (s.id, (s.level, s.parent_id)))
                .collect();

            let ancestor_at = |mut id: u32| -> u32 {
                for _ in 0..8 {
                    if let Some(&(lvl, parent)) = stem_lookup.get(&id) {
                        if lvl <= target_level {
                            return id;
                        }
                        if let Some(pid) = parent {
                            id = pid;
                        } else {
                            return id;
                        }
                    } else {
                        break;
                    }
                }
                id
            };

            let mut clusters: BTreeMap<u32, Vec<&Leaf>> = BTreeMap::new();
            for leaf in &tree.leaves {
                let cid = ancestor_at(leaf.stem_id);
                clusters.entry(cid).or_default().push(leaf);
            }

            let detail = if leaf_reduction >= 0.75 { 2 } else { 1 };
            for (cid, leaves) in clusters {
                if leaves.is_empty() {
                    continue;
                }
                let mut sum = Vec3::ZERO;
                let mut min_p = Vec3::splat(f32::MAX);
                let mut max_p = Vec3::splat(f32::MIN);
                for leaf in &leaves {
                    let pad = Vec3::splat(leaf.scale * 0.36);
                    sum += leaf.position;
                    min_p = min_p.min(leaf.position - pad);
                    max_p = max_p.max(leaf.position + pad);
                }
                let c = (sum / leaves.len() as f32).lerp((min_p + max_p) * 0.5, 0.42);
                let r = ((max_p - min_p) * 0.48).max(Vec3::splat(0.14));

                // Crown interior AO: outer/upper clusters brighter, lower interior clusters darker
                let rel = (c - crown_center) / crown_half;
                let radial = rel.length().clamp(0.0, 1.4);
                let ao = (0.58 + 0.36 * radial.min(1.0) + 0.12 * rel.y.clamp(-1.0, 1.0))
                    .clamp(0.42, 1.0);

                let cluster_seed = tree.seed ^ (cid as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
                let use_new = new_col.is_some() && hash_unit(cluster_seed ^ 0x44) < new_frac;
                let c_top = if let (true, Some(nc)) = (use_new, new_col) {
                    [
                        top_col[0] * 0.45 + nc[0] * 0.55,
                        top_col[1] * 0.45 + nc[1] * 0.55,
                        top_col[2] * 0.45 + nc[2] * 0.55,
                    ]
                } else {
                    top_col
                };

                // If a branch cluster spans > 1.3 m radius, split into 3 overlapping organic puffs
                let max_r = r.x.max(r.y).max(r.z);
                if max_r > 1.35 {
                    let sub_detail = 1u8;
                    for sub_i in 0..3u64 {
                        let a = hash_unit(cluster_seed ^ (sub_i * 0x1234)) * TAU;
                        let off = Vec3::new(
                            a.cos() * r.x * 0.42,
                            (sub_i as f32 - 1.0) * r.y * 0.28,
                            a.sin() * r.z * 0.42,
                        );
                        append_sculpted_lobe(
                            &mut foliage,
                            c + off,
                            r * 0.62,
                            cluster_seed ^ (sub_i * 0x7F4A_7C15),
                            sub_detail,
                            c_top,
                            bot_col,
                            ao,
                        );
                    }
                } else {
                    append_sculpted_lobe(
                        &mut foliage,
                        c,
                        r,
                        cluster_seed,
                        detail,
                        c_top,
                        bot_col,
                        ao,
                    );
                }
            }
        }
    }

    // Closed opaque organs (spikes/nuts/cones as tubes/lobes, flower clusters as blossom puffs)
    if !tree.organs.is_empty() && !is_impostor_lod {
        let reduced = reduce_organs(&tree.organs, leaf_reduction.max(0.35));
        let mut org_mesh = Mesh::new();
        let inf_col = species
            .inflorescence
            .as_ref()
            .map(|i| i.color)
            .unwrap_or([0.88, 0.72, 0.24]);
        let inf_tip = species
            .inflorescence
            .as_ref()
            .and_then(|i| i.color_tip)
            .unwrap_or(inf_col);
        let skirt_col = species
            .rosette
            .as_ref()
            .map(|r| r.skirt_color)
            .unwrap_or([0.46, 0.36, 0.22]);

        // Cap opaque cluster puffs so flowering trees stay within tight budgets
        let max_puffs = if leaf_reduction >= 0.75 { 240 } else { 90 };
        let stride = (reduced.len() / max_puffs).max(1);

        for (i, organ) in reduced.iter().enumerate() {
            let is_tall_spike = organ.kind == OrganKind::Cylinder && organ.length >= 0.45;
            if !is_tall_spike && i % stride != 0 {
                continue;
            }
            let axis = organ.rotation * Vec3::Y;
            let tip = organ.position + axis * organ.length;
            let seed = tree.seed ^ (i as u64).wrapping_mul(0x94D0_49BB_1331_11EB);
            match organ.kind {
                OrganKind::Cylinder => {
                    let c = if is_tall_spike {
                        species
                            .rosette
                            .as_ref()
                            .and_then(|r| r.spike.as_ref())
                            .map(|s| s.color)
                            .unwrap_or([0.36, 0.26, 0.16])
                    } else {
                        inf_col
                    };
                    append_tapered_trunk_tube(
                        &mut org_mesh,
                        organ.position,
                        tip,
                        (organ.width * 0.5).max(0.015),
                        (organ.width * 0.35).max(0.01),
                        5,
                        c,
                        false,
                    );
                }
                OrganKind::Urn | OrganKind::Ovoid => {
                    let c_pos = (organ.position + tip) * 0.5;
                    let r = Vec3::new(organ.width * 0.5, organ.length * 0.5, organ.width * 0.5)
                        .max(Vec3::splat(0.025));
                    append_sculpted_lobe(&mut org_mesh, c_pos, r, seed, 0, inf_col, inf_tip, 0.90);
                }
                OrganKind::Card => {
                    let c_pos = organ.position + axis * (organ.length * 0.35);
                    let r = Vec3::splat((organ.length * 0.34).max(0.05));
                    append_sculpted_lobe(&mut org_mesh, c_pos, r, seed, 1, inf_tip, inf_col, 0.96);
                }
                OrganKind::Blade => {
                    // Dead skirt blades: compact closed opaque hanging prism/lobe
                    let c_pos = (organ.position + tip) * 0.5;
                    let r = Vec3::new(
                        (organ.width * 0.6).max(0.04),
                        (organ.length * 0.48).max(0.08),
                        (organ.width * 0.6).max(0.04),
                    );
                    append_sculpted_lobe(
                        &mut org_mesh,
                        c_pos,
                        r,
                        seed,
                        0,
                        skirt_col,
                        [
                            skirt_col[0] * 0.65,
                            skirt_col[1] * 0.65,
                            skirt_col[2] * 0.65,
                        ],
                        0.78,
                    );
                }
            }
        }
        if !org_mesh.vertices.is_empty() {
            if species.uses_separate_organ_material() {
                organ_mesh_opt = Some(org_mesh);
            } else {
                // Merge directly into opaque foliage submesh
                let base = foliage.vertices.len() as u32;
                foliage.vertices.extend(org_mesh.vertices);
                foliage
                    .indices
                    .extend(org_mesh.indices.into_iter().map(|idx| idx + base));
            }
        }
    }

    (foliage, organ_mesh_opt)
}
