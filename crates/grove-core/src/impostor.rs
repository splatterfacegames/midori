//! Software-baked crown impostors for far LODs.
//!
//! A crown impostor replaces per-leaf geometry with two crossed quads
//! sampling a baked atlas: the left half is the crown seen from +Z, the
//! right half from +X. The bake is done in-engine (a small z-buffered
//! rasterizer over the same leaf cross-billboards and cut branches the
//! nearer LODs draw), so the impostor matches the actual tree — including
//! its generated material maps — rather than a generic billboard.
//!
//! Impostors always bake from the species' *procedural* texture set: host
//! file-slot overrides are resolved by the host at export time, after
//! geometry generation, so they cannot be sampled here.

use crate::{
    leaves::{LeafConfig, generate_leaf_mesh},
    mesh::{MaterialType, Mesh, Submesh, Vertex},
    mesh_builder::{MeshBuilder, MeshConfig},
    species::{LeafGeometry, Species},
    textures::{RgbaTexture, TextureSet},
    tree::Tree,
};
use glam::{Vec2, Vec3};

/// Baked crown impostor: crossed-quad geometry plus the atlas it samples.
#[derive(Debug, Clone)]
pub struct Impostor {
    /// Two crossed quads (front + back faces), one `Impostor` submesh.
    pub mesh: Mesh,
    /// RGBA atlas: left half = view from +Z, right half = view from +X.
    pub atlas: RgbaTexture,
}

/// Bake a crown impostor for `tree` at a LOD that keeps branches up to
/// `kept_branch_level`. Crown content is every leaf (as cross billboards)
/// plus all stems deeper than the kept level.
///
/// Returns `None` when the crown is empty (no leaves and no cut stems), so
/// degenerate species still get a valid bare-branch LOD.
pub fn bake_impostor(
    tree: &Tree,
    species: &Species,
    kept_branch_level: u32,
    textures: &TextureSet,
) -> Option<Impostor> {
    // Assemble crown content: leaves as cross billboards + cut branches.
    let leaf_config = LeafConfig {
        max_leaves: u32::MAX,
        geometry: LeafGeometry::CrossBillboard,
        polygon_resolution: 10,
        up_influence: species.leaves.up_influence,
        pivot_painter: false,
    };
    let leaf_mesh = generate_leaf_mesh(&tree.leaves, &leaf_config, tree);

    let stem_config = MeshConfig {
        ring_resolution: [6, 5, 4, 3],
        texture_v_scale: 1.0,
        pivot_painter: false,
    };
    let stem_mesh =
        MeshBuilder::new(tree, stem_config).build_stems_from_level(kept_branch_level + 1);

    let mut content = Mesh::new();
    if !stem_mesh.is_empty() {
        content.merge(&stem_mesh, MaterialType::Bark);
    }
    if !leaf_mesh.is_empty() {
        content.merge(&leaf_mesh, MaterialType::Leaves);
    }
    if content.is_empty() {
        return None;
    }

    // Crown bounds with a small pad so alpha edges don't clip the quads.
    let (mut min, mut max) = content_bounds(&content);
    let span = (max - min).max(Vec3::splat(0.01));
    min -= span * 0.02;
    max += span * 0.02;
    let span = max - min;

    // Atlas: two square views side by side.
    let view_res = textures
        .bark_albedo
        .width
        .max(textures.leaf_card.width)
        .clamp(64, 1024);
    let front = bake_view(&content, View::Front, min, span, view_res, textures);
    let side = bake_view(&content, View::Side, min, span, view_res, textures);
    let mut atlas = RgbaTexture::new(view_res * 2, view_res);
    for y in 0..view_res {
        for x in 0..view_res {
            atlas.set(x, y, front.get(x, y));
            atlas.set(view_res + x, y, side.get(x, y));
        }
    }

    Some(Impostor {
        mesh: impostor_quads(min, max),
        atlas,
    })
}

/// Orthographic view direction for the bake.
#[derive(Debug, Clone, Copy)]
enum View {
    /// Viewer on +Z looking toward -Z; screen u = +x, v = +y.
    Front,
    /// Viewer on +X looking toward -X; screen u = -z, v = +y.
    Side,
}

impl View {
    /// World → (u, v, depth); nearer = larger depth.
    fn project(self, p: Vec3, min: Vec3, span: Vec3) -> (f32, f32, f32) {
        match self {
            View::Front => (
                (p.x - min.x) / span.x.max(1e-6),
                (p.y - min.y) / span.y.max(1e-6),
                p.z,
            ),
            View::Side => (
                (min.z + span.z - p.z) / span.z.max(1e-6),
                (p.y - min.y) / span.y.max(1e-6),
                p.x,
            ),
        }
    }
}

/// Material of every triangle, derived from submesh index ranges.
fn tri_materials(mesh: &Mesh) -> Vec<MaterialType> {
    let mut out = vec![MaterialType::Bark; mesh.triangle_count()];
    for sub in &mesh.submeshes {
        let end = (sub.index_start + sub.index_count) as usize;
        for i in (sub.index_start as usize..end).step_by(3) {
            if i / 3 < out.len() {
                out[i / 3] = sub.material;
            }
        }
    }
    out
}

/// Rasterize the crown content into a `res`×`res` RGBA view.
///
/// 2× supersampling turns per-texel alpha tests into coverage values, which
/// keeps leaf-card edges soft instead of blocky.
fn bake_view(
    mesh: &Mesh,
    view: View,
    min: Vec3,
    span: Vec3,
    res: u32,
    textures: &TextureSet,
) -> RgbaTexture {
    const SS: u32 = 2;
    let ss = res * SS;
    let mut color = vec![[0.0f32; 4]; (ss * ss) as usize];
    let mut depth = vec![f32::NEG_INFINITY; (ss * ss) as usize];

    let materials = tri_materials(mesh);
    for (tri_i, tri) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
        let [i0, i1, i2] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        let (v0, v1, v2) = (&mesh.vertices[i0], &mesh.vertices[i1], &mesh.vertices[i2]);

        let p0 = view.project(v0.position, min, span);
        let p1 = view.project(v1.position, min, span);
        let p2 = view.project(v2.position, min, span);

        // Pixel space: u → x, v → y with v=0 at the top row.
        let s = |p: (f32, f32, f32)| (p.0 * ss as f32, (1.0 - p.1) * ss as f32, p.2);
        let (x0, y0, d0) = s(p0);
        let (x1, y1, d1) = s(p1);
        let (x2, y2, d2) = s(p2);

        let min_x = x0.min(x1).min(x2).floor().max(0.0) as u32;
        let max_x = x0.max(x1).max(x2).ceil().min(ss as f32) as u32;
        let min_y = y0.min(y1).min(y2).floor().max(0.0) as u32;
        let max_y = y0.max(y1).max(y2).ceil().min(ss as f32) as u32;
        if min_x >= max_x || min_y >= max_y {
            continue;
        }

        let edge = |ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32| {
            (px - ax) * (by - ay) - (py - ay) * (bx - ax)
        };
        let area = edge(x0, y0, x1, y1, x2, y2);
        if area.abs() < 1e-9 {
            continue;
        }

        let material = materials.get(tri_i).copied().unwrap_or(MaterialType::Bark);
        for py in min_y..max_y {
            for px in min_x..max_x {
                let cx = px as f32 + 0.5;
                let cy = py as f32 + 0.5;
                let w0 = edge(x1, y1, x2, y2, cx, cy);
                let w1 = edge(x2, y2, x0, y0, cx, cy);
                let w2 = edge(x0, y0, x1, y1, cx, cy);
                let inside =
                    (w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0) || (w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0);
                if !inside {
                    continue;
                }
                let idx = (py * ss + px) as usize;
                let d = (w0 * d0 + w1 * d1 + w2 * d2) / area;
                if d <= depth[idx] {
                    continue;
                }
                let u = (w0 * v0.uv.x + w1 * v1.uv.x + w2 * v2.uv.x) / area;
                let v = (w0 * v0.uv.y + w1 * v1.uv.y + w2 * v2.uv.y) / area;
                let texel = match material {
                    MaterialType::Leaves | MaterialType::Impostor => {
                        textures.leaf_card.sample_wrap(u, v)
                    }
                    MaterialType::Bark => textures.bark_albedo.sample_wrap(u, v),
                };
                // Low threshold: coverage accumulates at downsample.
                if texel[3] > 0.15 {
                    depth[idx] = d;
                    color[idx] = texel;
                }
            }
        }
    }

    // Downsample 2×2 → alpha becomes coverage; un-premultiply RGB.
    let mut out = RgbaTexture::new(res, res);
    for y in 0..res {
        for x in 0..res {
            let mut a_sum = 0.0;
            let mut rgb = [0.0f32; 3];
            for dy in 0..SS {
                for dx in 0..SS {
                    let c = color[((y * SS + dy) * ss + x * SS + dx) as usize];
                    a_sum += c[3];
                    rgb[0] += c[0] * c[3];
                    rgb[1] += c[1] * c[3];
                    rgb[2] += c[2] * c[3];
                }
            }
            let a = a_sum / 4.0;
            let px = if a_sum > 0.0 {
                [
                    (rgb[0] / a_sum * 255.0) as u8,
                    (rgb[1] / a_sum * 255.0) as u8,
                    (rgb[2] / a_sum * 255.0) as u8,
                    (a * 255.0) as u8,
                ]
            } else {
                [0, 0, 0, 0]
            };
            out.set(x, y, px);
        }
    }
    out
}

/// Min/max bounds of a mesh's vertices.
fn content_bounds(mesh: &Mesh) -> (Vec3, Vec3) {
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for v in &mesh.vertices {
        min = min.min(v.position);
        max = max.max(v.position);
    }
    (min, max)
}

/// Two crossed quads in crown space: an XY quad at the crown's z-center
/// (normal +Z) and a ZY quad at its x-center (normal +X). Both get front
/// and back faces so the impostor reads from every direction without
/// relying on a double-sided material flag.
fn impostor_quads(min: Vec3, max: Vec3) -> Mesh {
    let c = (min + max) * 0.5;
    let mut mesh = Mesh::new();

    // Front quad (XY plane): atlas left half.
    let quad = |mesh: &mut Mesh, corners: [(Vec3, Vec2); 4]| {
        let base = mesh.vertices.len() as u32;
        for (position, uv) in corners {
            mesh.vertices.push(Vertex::new(position, Vec3::Z, uv));
        }
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        // Back faces.
        mesh.indices
            .extend_from_slice(&[base + 2, base + 1, base, base + 3, base + 2, base]);
    };

    quad(
        &mut mesh,
        [
            (Vec3::new(min.x, min.y, c.z), Vec2::new(0.0, 1.0)),
            (Vec3::new(max.x, min.y, c.z), Vec2::new(0.5, 1.0)),
            (Vec3::new(max.x, max.y, c.z), Vec2::new(0.5, 0.0)),
            (Vec3::new(min.x, max.y, c.z), Vec2::new(0.0, 0.0)),
        ],
    );

    // Side quad (ZY plane): atlas right half. u runs 0.5→1.0 from
    // z=max to z=min, matching the -z view axis.
    quad(
        &mut mesh,
        [
            (Vec3::new(c.x, min.y, max.z), Vec2::new(0.5, 1.0)),
            (Vec3::new(c.x, min.y, min.z), Vec2::new(1.0, 1.0)),
            (Vec3::new(c.x, max.y, min.z), Vec2::new(1.0, 0.0)),
            (Vec3::new(c.x, max.y, max.z), Vec2::new(0.5, 0.0)),
        ],
    );
    // Correct normals for the second quad (+X).
    for v in &mut mesh.vertices[4..] {
        v.normal = Vec3::X;
    }

    mesh.submeshes.push(Submesh {
        index_start: 0,
        index_count: mesh.indices.len() as u32,
        material: MaterialType::Impostor,
    });
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{generation::generate_tree, textures::TextureSet};

    fn crown_species() -> Species {
        Species::from_toml(
            r#"
[species]
name = "Impostor Test"

[trunk]
height = 5.0
radius = 0.3
segments = 4

[branches.level1]
count = 4
length = 2.0
segments = 3

[leaves]
count = 200
min_level = 1
size = 0.2
distribution = "both"
geometry = "cross_billboard"

[textures]
resolution = 64
"#,
        )
        .unwrap()
    }

    #[test]
    fn impostor_bakes_quads_and_atlas() {
        let species = crown_species();
        let tree = generate_tree(&species, 7);
        let textures = TextureSet::generate(&species);

        let imp = bake_impostor(&tree, &species, 0, &textures).expect("crown should bake");
        // Two quads, front+back → 8 triangles, one Impostor submesh.
        assert_eq!(imp.mesh.triangle_count(), 8);
        assert_eq!(imp.mesh.submeshes.len(), 1);
        assert_eq!(imp.mesh.submeshes[0].material, MaterialType::Impostor);
        // Atlas is 2:1 (front | side).
        assert_eq!(imp.atlas.width, imp.atlas.height * 2);
        // The bake should have real coverage, not an empty card.
        let coverage = imp.atlas.alpha_coverage();
        assert!(coverage > 0.02, "atlas coverage {coverage} too low");
        // UVs stay inside each view half.
        for v in &imp.mesh.vertices {
            assert!(v.uv.x >= 0.0 && v.uv.x <= 1.0);
            assert!(v.uv.y >= 0.0 && v.uv.y <= 1.0);
        }
    }

    #[test]
    fn empty_crown_returns_none() {
        let species = crown_species();
        let mut tree = Tree::new("Bare".to_string(), 1);
        tree.leaves.clear();
        let textures = TextureSet::generate(&species);
        // No stems above level 0, no leaves → no impostor.
        let mut trunk = crate::tree::Stem::new(0, 0);
        trunk.segments.push(crate::tree::Segment {
            start: Vec3::ZERO,
            end: Vec3::Y,
            start_radius: 0.1,
            end_radius: 0.05,
            direction: Vec3::Y,
        });
        tree.add_stem(trunk);
        assert!(bake_impostor(&tree, &species, 0, &textures).is_none());
    }
}
