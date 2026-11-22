//! Organs: inflorescences, fruit, dead skirt blades and flower spikes.
//!
//! An [`Organ`] is anything a plant carries besides stems and leaves. Organs
//! are drawn into the foliage material from the cells of the 2x2 foliage
//! atlas (`botanical_cards`), so they need no extra material slot. They are
//! placed deterministically from the tree seed, thinned by each LOD's
//! `leaf_reduction`, and baked into crown impostors.
//!
//! See `docs/botanical-generators.md`.

use crate::{
    botanical_cards::{CELL_AGED, CELL_ORGAN, atlas_uv},
    constants::*,
    math::*,
    mesh::{MaterialType, Mesh, Submesh, Vertex},
    rng::Rng,
    species::{
        FoliageVertexColor, InflorescenceKind, InflorescenceParams, OrganOrientation,
        OrganPlacement, Species,
    },
    tree::{Leaf, Stem, Tree},
};
use glam::{Quat, Vec2, Vec3, Vec4};

/// Geometry an organ is drawn with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganKind {
    /// Capped cylinder (flower spikes, scapes).
    Cylinder,
    /// Urn-shaped lathe, attached at its base (woody nuts).
    Urn,
    /// Ovoid lathe (cones).
    Ovoid,
    /// Two crossed cards (flower clusters, berries).
    Card,
    /// Curved ribbon (dead skirt blades, tubular flowers).
    Blade,
}

/// One organ instance.
#[derive(Debug, Clone, Copy)]
pub struct Organ {
    /// Attachment point in world space.
    pub position: Vec3,
    /// Orientation: local +Y is the organ axis (away from the attachment),
    /// local +Z the face normal of cards and blades.
    pub rotation: Quat,
    /// Length along the axis in metres.
    pub length: f32,
    /// Diameter (lathes) or width (cards, blades) in metres.
    pub width: f32,
    /// Geometry.
    pub kind: OrganKind,
    /// Foliage-atlas cell the organ samples.
    pub cell: u8,
    /// Cell-local v at the attachment and at the far end (v runs down the
    /// cell, so `[1.0, 0.0]` maps the whole cell base-to-tip).
    pub v_range: [f32; 2],
    /// Gravity bend of `Blade` organs (`0` straight .. `1` strongly arching).
    pub bend: f32,
    /// Stem the organ is attached to (wind phase, clump grouping).
    pub stem_id: u32,
}

/// Place the species' `[inflorescence]` organs on a generated tree.
///
/// Returns an empty list when the species has no inflorescence section or no
/// stem qualifies.
pub fn place_inflorescence(tree: &Tree, species: &Species, seed: u64) -> Vec<Organ> {
    let Some(inf) = &species.inflorescence else {
        return Vec::new();
    };
    if inf.count == 0 {
        return Vec::new();
    }
    let mut rng = Rng::from_seed(seed.wrapping_add(0x0F10_4E55));

    // Qualifying stems; fall back to the deepest level the tree actually has.
    let deepest = tree.stems.iter().map(|s| s.level as u32).max().unwrap_or(0);
    let min_level = inf.min_level.min(deepest);
    let mut stems: Vec<&Stem> = tree
        .stems
        .iter()
        .filter(|s| s.level as u32 >= min_level && !s.segments.is_empty())
        .collect();
    if inf.placement == OrganPlacement::Endpoint {
        let terminal: Vec<&Stem> = stems
            .iter()
            .copied()
            .filter(|s| s.child_ids.is_empty())
            .collect();
        if !terminal.is_empty() {
            stems = terminal;
        }
    }
    if stems.is_empty() {
        return Vec::new();
    }

    // Visit stems in a seeded shuffle so a count below the stem count picks
    // tips from all over the crown instead of the first limbs generated.
    let mut order: Vec<usize> = (0..stems.len()).collect();
    for i in (1..order.len()).rev() {
        order.swap(i, rng.index(i + 1));
    }
    let total_len: f32 = stems.iter().map(|s| s.length().max(0.05)).sum();

    let mut organs = Vec::with_capacity(inf.count as usize);
    for n in 0..inf.count as usize {
        let (stem, t) = match inf.placement {
            OrganPlacement::Endpoint => {
                let stem = stems[order[n % order.len()]];
                // First pass sits on the tip; later passes step back along the shoot.
                let pass = n / order.len();
                let t = if pass == 0 {
                    1.0
                } else {
                    (1.0 - rng.range(0.05, 0.45)).clamp(0.3, 1.0)
                };
                (stem, t)
            }
            OrganPlacement::AlongBranch => {
                let mut pick = rng.next_f32() * total_len;
                let mut chosen = stems[0];
                for s in &stems {
                    let l = s.length().max(0.05);
                    if pick <= l {
                        chosen = s;
                        break;
                    }
                    pick -= l;
                }
                (chosen, rng.range(0.25, 1.0))
            }
        };
        let position = stem.point_at(t);
        let stem_dir = stem.direction_at(t).try_normalize().unwrap_or(Vec3::Y);
        let axis = organ_axis(inf.orientation, stem_dir, &mut rng);
        let rotation = Quat::from_axis_angle(axis, rng.range(0.0, TAU))
            * Quat::from_rotation_arc(Vec3::Y, axis);
        let size = rng.variance_mul(inf.size_variance);
        let aged = rng.next_f32() < inf.aged_fraction;
        organs.push(Organ {
            position: position + axis * inf.offset,
            rotation,
            length: inf.length * size,
            width: inf.width * size,
            kind: organ_kind(inf),
            cell: if aged { CELL_AGED } else { CELL_ORGAN },
            v_range: [1.0, 0.0],
            bend: 0.0,
            stem_id: stem.id,
        });
    }
    organs
}

fn organ_kind(inf: &InflorescenceParams) -> OrganKind {
    match inf.kind {
        InflorescenceKind::Spike => OrganKind::Cylinder,
        InflorescenceKind::Nut => OrganKind::Urn,
        InflorescenceKind::Cluster | InflorescenceKind::Berries => OrganKind::Card,
    }
}

fn organ_axis(orientation: OrganOrientation, stem_dir: Vec3, rng: &mut Rng) -> Vec3 {
    let wobble = |v: Vec3, max: f32, rng: &mut Rng| {
        let (t, b) = create_basis(v);
        let a = rng.range(0.0, TAU);
        let tilt = rng.next_f32().sqrt() * max;
        (v * tilt.cos() + (t * a.cos() + b * a.sin()) * tilt.sin())
            .try_normalize()
            .unwrap_or(v)
    };
    match orientation {
        OrganOrientation::Up => wobble(Vec3::Y, 0.26, rng),
        OrganOrientation::Hang => wobble(Vec3::NEG_Y, 0.35, rng),
        OrganOrientation::Branch => {
            let lifted = (stem_dir + Vec3::Y * 0.25)
                .try_normalize()
                .unwrap_or(stem_dir);
            wobble(lifted, 0.45, rng)
        }
    }
}

/// Organs kept at a LOD that keeps `reduction` (`0..=1`) of the foliage:
/// evenly subsampled so no side of the crown is stripped.
pub fn reduce_organs(organs: &[Organ], reduction: f32) -> Vec<Organ> {
    let total = organs.len();
    let keep = ((total as f32 * reduction.clamp(0.0, 1.0)).ceil() as usize).min(total);
    if keep >= total {
        return organs.to_vec();
    }
    (0..keep).map(|i| organs[i * total / keep.max(1)]).collect()
}

/// Radius profile of a lathe organ along its axis (`t` in `0..=1`), as a
/// fraction of the organ's half-width.
fn lathe_radius(kind: OrganKind, t: f32) -> f32 {
    match kind {
        OrganKind::Cylinder => {
            if t < 0.08 {
                0.55 + 0.45 * (t / 0.08).sqrt()
            } else if t > 0.88 {
                let k = (t - 0.88) / 0.12;
                (1.0 - 0.55 * k * k).max(0.3)
            } else {
                1.0
            }
        }
        // Bulbous body from the stalk, a narrower neck, a slightly flared rim.
        OrganKind::Urn => {
            if t < 0.5 {
                0.22 + 0.78 * (t / 0.5 * PI * 0.5).sin()
            } else if t < 0.82 {
                lerp(1.0, 0.66, ((t - 0.5) / 0.32).powf(1.4))
            } else {
                lerp(0.66, 0.74, (t - 0.82) / 0.18)
            }
        }
        _ => (t * PI).sin().max(0.0).powf(0.62).max(0.12),
    }
}

/// Build the mesh for `organs`.
///
/// `detail` is 0 for the nearest LOD and rises with distance: it lowers lathe
/// side counts, ribbon rows, and drops the second card of crossed cards.
/// `stiffness` is written as the Pivot Painter stiffness of blade organs.
pub fn generate_organ_mesh(
    organs: &[Organ],
    tree: &Tree,
    vertex_color: FoliageVertexColor,
    pivot_painter: bool,
    detail: u32,
) -> Mesh {
    let mut mesh = Mesh::new();
    let tint = vertex_color == FoliageVertexColor::TintAo;
    for (idx, organ) in organs.iter().enumerate() {
        let start = mesh.vertices.len();
        match organ.kind {
            OrganKind::Cylinder | OrganKind::Urn | OrganKind::Ovoid => {
                add_lathe(&mut mesh, organ, detail);
            }
            OrganKind::Card => add_cards(&mut mesh, organ, idx, detail),
            OrganKind::Blade => add_blade(&mut mesh, organ, idx, detail),
        }
        // Per-organ shade keeps a crown of flowers from looking flat-filled.
        let jitter = 1.0 - 0.14 * (idx as f32 * 0.754_877_7).fract();
        let leaf = Leaf {
            position: organ.position,
            rotation: organ.rotation,
            scale: organ.length,
            stem_id: organ.stem_id,
        };
        for v in &mut mesh.vertices[start..] {
            if tint {
                v.color = Vec4::new(jitter, jitter, jitter, 1.0);
            } else if pivot_painter {
                crate::leaves::encode_leaf_pivot_painter(v, &leaf, tree);
                // Woody organs are stiff; ribbons and cards flutter like leaves.
                if !matches!(organ.kind, OrganKind::Card | OrganKind::Blade) {
                    v.color.w = 0.8;
                }
            }
        }
    }
    if !mesh.indices.is_empty() {
        mesh.submeshes.push(Submesh {
            index_start: 0,
            index_count: mesh.indices.len() as u32,
            material: MaterialType::Leaves,
        });
    }
    mesh
}

fn add_lathe(mesh: &mut Mesh, organ: &Organ, detail: u32) {
    let sides: u32 = match detail {
        0 => 8,
        1 => 6,
        _ => 4,
    };
    let rings: &[f32] = match (organ.kind, detail) {
        (OrganKind::Cylinder, 0) => &[0.0, 0.08, 0.5, 0.88, 1.0],
        (OrganKind::Cylinder, _) => &[0.0, 0.5, 1.0],
        (OrganKind::Urn, 0) => &[0.0, 0.2, 0.5, 0.82, 1.0],
        (OrganKind::Urn, _) => &[0.0, 0.5, 1.0],
        (_, 0) => &[0.03, 0.25, 0.5, 0.75, 0.97],
        _ => &[0.05, 0.5, 0.95],
    };
    let axis = organ.rotation * Vec3::Y;
    let (tan, bit) = (organ.rotation * Vec3::X, organ.rotation * Vec3::Z);
    let half_w = organ.width * 0.5;
    let base = mesh.vertices.len() as u32;
    let cols = sides + 1; // seam column duplicated for the wrap UV
    for &t in rings {
        let r = half_w * lathe_radius(organ.kind, t);
        let center = organ.position + axis * (organ.length * t);
        let v = lerp(organ.v_range[0], organ.v_range[1], t);
        for k in 0..cols {
            let a = k as f32 / sides as f32 * TAU;
            let radial = tan * a.cos() + bit * a.sin();
            let uv = atlas_uv(organ.cell, Vec2::new(k as f32 / sides as f32, v));
            mesh.vertices
                .push(Vertex::new(center + radial * r, radial, uv));
        }
    }
    for ring in 0..(rings.len() as u32 - 1) {
        for k in 0..sides {
            let i00 = base + ring * cols + k;
            let i10 = i00 + 1;
            let i01 = i00 + cols;
            let i11 = i01 + 1;
            mesh.indices
                .extend_from_slice(&[i00, i01, i11, i00, i11, i10]);
        }
    }
    // Cap the far end with a fan (the foliage material is double-sided, so
    // the open base against the stem needs none).
    let last = *rings.last().unwrap_or(&1.0);
    let tip = organ.position + axis * (organ.length * last);
    let cap_uv = atlas_uv(organ.cell, Vec2::new(0.5, organ.v_range[1]));
    let cap = mesh.vertices.len() as u32;
    mesh.vertices.push(Vertex::new(tip, axis, cap_uv));
    let top = base + (rings.len() as u32 - 1) * cols;
    for k in 0..sides {
        mesh.indices.extend_from_slice(&[cap, top + k + 1, top + k]);
    }
}

fn add_cards(mesh: &mut Mesh, organ: &Organ, idx: usize, detail: u32) {
    let axis = organ.rotation * Vec3::Y;
    let half = organ.length * 0.5;
    let planes: &[f32] = if detail >= 2 {
        &[0.0]
    } else {
        &[0.0, PI * 0.5]
    };
    // Per-organ splay so crossed cards never line up into a visible grid.
    let splay = ((idx as f32 * 0.618_034).fract() - 0.5) * 0.5;
    for &turn in planes {
        let q = Quat::from_axis_angle(axis, turn + splay) * organ.rotation;
        let side = q * Vec3::X;
        let normal = q * Vec3::Z;
        let base = mesh.vertices.len() as u32;
        for (cx, cy) in [(-1.0f32, 0.0f32), (1.0, 0.0), (1.0, 1.0), (-1.0, 1.0)] {
            let pos = organ.position + side * (cx * half) + axis * (cy * organ.length);
            let v = lerp(organ.v_range[0], organ.v_range[1], cy);
            let uv = atlas_uv(organ.cell, Vec2::new(cx * 0.5 + 0.5, v));
            mesh.vertices.push(Vertex::new(pos, normal, uv));
        }
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

fn add_blade(mesh: &mut Mesh, organ: &Organ, idx: usize, detail: u32) {
    let rows: usize = match detail {
        0 => 5,
        1 => 3,
        _ => 2,
    };
    let hash = (idx as f32 * 0.618_034).fract();
    let (points, sides) = ribbon_spine(
        organ.position,
        organ.rotation,
        organ.length,
        organ.bend,
        0.35 * (hash - 0.5),
        rows,
    );
    let base = mesh.vertices.len() as u32;
    let half_w = organ.width * 0.5;
    for r in 0..=rows {
        let u = r as f32 / rows as f32;
        let tang = ribbon_tangent(&points, r);
        let normal = sides[r].cross(tang).try_normalize().unwrap_or(Vec3::Y);
        let v = lerp(organ.v_range[0], organ.v_range[1], u);
        for xn in [-1.0f32, 1.0] {
            let pos = points[r] + sides[r] * (xn * half_w);
            let uv = atlas_uv(organ.cell, Vec2::new(xn * 0.5 + 0.5, v));
            mesh.vertices.push(Vertex::new(pos, normal, uv));
        }
    }
    for r in 0..rows as u32 {
        let i00 = base + r * 2;
        mesh.indices
            .extend_from_slice(&[i00, i00 + 1, i00 + 3, i00, i00 + 3, i00 + 2]);
    }
}

/// Spine points and parallel-transported side vectors of a ribbon launched
/// from `position` along `rotation * Y` that bends under gravity. `twist`
/// is the total roll in radians from base to tip.
pub(crate) fn ribbon_spine(
    position: Vec3,
    rotation: Quat,
    length: f32,
    bend: f32,
    twist: f32,
    rows: usize,
) -> (Vec<Vec3>, Vec<Vec3>) {
    let rows = rows.max(1);
    let mut dir = (rotation * Vec3::Y).try_normalize().unwrap_or(Vec3::Y);
    let mut side = (rotation * Vec3::X).try_normalize().unwrap_or(Vec3::X);
    let step = length / rows as f32;
    let mut cur = position;
    let mut points = Vec::with_capacity(rows + 1);
    let mut sides = Vec::with_capacity(rows + 1);
    points.push(cur);
    sides.push(side);
    for r in 0..rows {
        let u = (r as f32 + 0.5) / rows as f32;
        // The pull grows toward the tip, where the blade has the least support.
        let pull = bend.clamp(0.0, 1.0) * 2.2 * (0.25 + 0.95 * u) / rows as f32;
        dir = (dir - Vec3::Y * pull).try_normalize().unwrap_or(dir);
        cur += dir * step;
        let projected = side - dir * side.dot(dir);
        side = projected.try_normalize().unwrap_or(side);
        if twist != 0.0 {
            side = rotate_around_axis(side, dir, twist / rows as f32);
        }
        points.push(cur);
        sides.push(side);
    }
    (points, sides)
}

/// Central-difference tangent of a ribbon spine at row `r`.
pub(crate) fn ribbon_tangent(points: &[Vec3], r: usize) -> Vec3 {
    let last = points.len() - 1;
    let d = if r == 0 {
        points[1] - points[0]
    } else if r == last {
        points[last] - points[last - 1]
    } else {
        points[r + 1] - points[r - 1]
    };
    d.try_normalize().unwrap_or(Vec3::Y)
}

/// Append `extra` (a foliage mesh) to `mesh` as part of one foliage submesh.
pub(crate) fn append_foliage(mesh: &mut Mesh, extra: &Mesh) {
    if extra.indices.is_empty() {
        return;
    }
    let offset = mesh.vertices.len() as u32;
    mesh.vertices.extend_from_slice(&extra.vertices);
    mesh.indices
        .extend(extra.indices.iter().map(|i| i + offset));
    let count = mesh.indices.len() as u32;
    mesh.submeshes.clear();
    mesh.submeshes.push(Submesh {
        index_start: 0,
        index_count: count,
        material: MaterialType::Leaves,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::generate_tree;

    fn tree_with(kind: &str, extra: &str) -> (Species, Tree) {
        let species = Species::from_toml(&format!(
            r#"
[species]
name = "Organ Fixture"

[trunk]
height = 5.0
radius = 0.3
segments = 6

[branches.level1]
count = 6
length = 2.5

[branches.level2]
count = 4
length = 1.2

[leaves]
count = 300

[inflorescence]
kind = "{kind}"
count = 30
{extra}
"#
        ))
        .unwrap();
        let tree = generate_tree(&species, 11).unwrap();
        (species, tree)
    }

    #[test]
    fn inflorescence_places_requested_count_deterministically() {
        let (species, tree) = tree_with("spike", "aged_fraction = 0.5");
        assert_eq!(tree.organs.len(), 30);
        let again = generate_tree(&species, 11).unwrap();
        for (a, b) in tree.organs.iter().zip(&again.organs) {
            assert_eq!(a.position, b.position);
            assert_eq!(a.cell, b.cell);
        }
        let aged = tree.organs.iter().filter(|o| o.cell == CELL_AGED).count();
        assert!(aged > 3 && aged < 27, "aged share off: {aged}/30");
    }

    #[test]
    fn spikes_stand_up_and_nuts_hang() {
        let (_, up) = tree_with("spike", "orientation = \"up\"");
        assert!(up.organs.iter().all(|o| (o.rotation * Vec3::Y).y > 0.9));
        let (_, down) = tree_with("nut", "orientation = \"hang\"");
        assert!(down.organs.iter().all(|o| (o.rotation * Vec3::Y).y < -0.85));
    }

    #[test]
    fn organ_meshes_are_valid_for_every_kind_and_detail() {
        for kind in ["spike", "nut", "cluster", "berries"] {
            let (_, tree) = tree_with(kind, "");
            let mut previous = usize::MAX;
            for detail in 0..3 {
                let mesh = generate_organ_mesh(
                    &tree.organs,
                    &tree,
                    FoliageVertexColor::PivotPainter,
                    true,
                    detail,
                );
                assert!(!mesh.is_empty(), "{kind} detail {detail} is empty");
                assert_eq!(mesh.indices.len() % 3, 0);
                let n = mesh.vertices.len() as u32;
                assert!(mesh.indices.iter().all(|i| *i < n));
                for v in &mesh.vertices {
                    assert!(v.position.is_finite() && v.normal.is_finite());
                    // Organs never sample the leaf-card cell.
                    assert!(v.uv.x > 0.5 || v.uv.y > 0.5, "{kind}: uv in leaf cell");
                }
                assert!(
                    mesh.triangle_count() <= previous,
                    "{kind}: detail must not add triangles"
                );
                previous = mesh.triangle_count();
            }
        }
    }

    #[test]
    fn reduce_organs_keeps_an_even_share() {
        let (_, tree) = tree_with("cluster", "");
        assert_eq!(reduce_organs(&tree.organs, 1.0).len(), 30);
        assert_eq!(reduce_organs(&tree.organs, 0.5).len(), 15);
        assert_eq!(reduce_organs(&tree.organs, 0.0).len(), 0);
    }

    #[test]
    fn species_without_inflorescence_has_no_organs() {
        let species = Species::from_toml(
            "[species]\nname = \"Plain\"\n[trunk]\nheight = 4.0\n[branches.level1]\ncount = 4\n",
        )
        .unwrap();
        assert!(generate_tree(&species, 3).unwrap().organs.is_empty());
    }
}
