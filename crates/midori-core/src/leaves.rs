//! Leaf placement and geometry generation.
//!
//! This module provides leaf placement algorithms for distributing leaves
//! on tree branches, and geometry generation for rendering leaves with
//! different visual styles (polygon, cross_billboard, billboard).

use crate::{
    constants::*,
    math::*,
    mesh::{MaterialType, Mesh, Submesh, Vertex},
    rng::Rng,
    species::{
        CanopyParams, FoliageNormals, FoliageVertexColor, LeafDistribution, LeafGeometry,
        LeafShape, Species,
    },
    tree::{Leaf, Stem, Tree},
};
use glam::{Quat, Vec2, Vec3, Vec4};

/// Leaf generation configuration
#[derive(Debug, Clone)]
pub struct LeafConfig {
    /// Maximum leaves to generate
    pub max_leaves: u32,
    /// Leaf geometry type
    pub geometry: LeafGeometry,
    /// Leaf shape for polygon generation
    pub shape: LeafShape,
    /// Polygon resolution override (0 = use shape's recommended resolution)
    pub polygon_resolution: u32,
    /// Up influence factor (-1 to 1, how much leaves point upward)
    pub up_influence: f32,
    /// Enable Pivot Painter data
    pub pivot_painter: bool,
    /// Canopy shaping (normals, card orientation, vertex tint/AO)
    pub canopy: CanopyParams,
}

impl Default for LeafConfig {
    fn default() -> Self {
        Self {
            max_leaves: 3000,
            geometry: LeafGeometry::CrossBillboard,
            shape: LeafShape::default(),
            polygon_resolution: 0, // Use shape's recommended resolution
            up_influence: 0.25,
            pivot_painter: true,
            canopy: CanopyParams::default(),
        }
    }
}

impl LeafConfig {
    /// Create a LeafConfig from species parameters
    pub fn from_species(species: &Species) -> Self {
        Self {
            max_leaves: species.leaves.count,
            geometry: species.leaves.geometry,
            shape: species.leaves.shape,
            polygon_resolution: 0,
            up_influence: species.leaves.up_influence,
            pivot_painter: true,
            canopy: species.canopy.clone(),
        }
    }

    /// Get the effective polygon resolution (shape default or override)
    pub fn effective_resolution(&self) -> u32 {
        if self.polygon_resolution > 0 {
            self.polygon_resolution
        } else {
            self.shape.recommended_resolution()
        }
    }
}

// LeafShape is defined in species.rs and re-exported from there

/// Place leaves on the tree structure
///
/// Distributes leaves across candidate stems based on the species parameters.
/// Leaves can be placed at branch endpoints, along branches, or both.
///
/// # Arguments
///
/// * `tree` - The tree structure to place leaves on
/// * `species` - Species parameters controlling leaf placement
/// * `rng` - Random number generator for deterministic placement
///
/// # Returns
///
/// A vector of Leaf instances positioned in world space
pub fn place_leaves(tree: &Tree, species: &Species, rng: &mut Rng) -> Vec<Leaf> {
    let params = &species.leaves;
    let mut leaves = Vec::new();

    // Collect candidate stems for leaf placement
    let candidate_stems: Vec<&Stem> = tree
        .stems
        .iter()
        .filter(|s| s.level as u32 >= params.min_level)
        .collect();

    if candidate_stems.is_empty() {
        return leaves;
    }

    // Calculate leaves per candidate based on weights
    let total_weight: f32 = candidate_stems
        .iter()
        .map(|s| calculate_stem_weight(s, params.distribution))
        .sum();

    if total_weight <= 0.0 {
        return leaves;
    }

    for stem in &candidate_stems {
        if leaves.len() >= params.count as usize {
            break;
        }

        let stem_weight = calculate_stem_weight(stem, params.distribution);
        let stem_leaf_count = ((stem_weight / total_weight) * params.count as f32) as usize;

        for _ in 0..stem_leaf_count {
            if leaves.len() >= params.count as usize {
                break;
            }

            // Choose placement point
            let (position, direction) = choose_leaf_position(stem, params.distribution, rng);

            // Calculate leaf orientation
            let rotation = calculate_leaf_rotation(direction, params.up_influence, rng);

            // Random scale with variance
            let scale = params.size * rng.variance_mul(params.size_variance);

            leaves.push(Leaf {
                position,
                rotation,
                scale,
                stem_id: stem.id,
            });
        }
    }

    leaves
}

/// Calculate the weight of a stem for leaf distribution
fn calculate_stem_weight(stem: &Stem, distribution: LeafDistribution) -> f32 {
    match distribution {
        LeafDistribution::Endpoint => {
            // Only terminal branches get leaves
            if stem.child_ids.is_empty() { 1.0 } else { 0.0 }
        }
        LeafDistribution::AlongBranch => {
            // Weight by branch length
            stem.length().max(0.1)
        }
        LeafDistribution::Both => {
            // Combination of endpoint and length-based weighting
            let endpoint_weight = if stem.child_ids.is_empty() { 1.0 } else { 0.0 };
            let length_weight = stem.length().max(0.1) * 0.3;
            endpoint_weight + length_weight
        }
    }
}

/// Choose a position and direction for a leaf on a stem
fn choose_leaf_position(
    stem: &Stem,
    distribution: LeafDistribution,
    rng: &mut Rng,
) -> (Vec3, Vec3) {
    match distribution {
        LeafDistribution::Endpoint => {
            // Position at tip with small random offset
            let tip = stem.tip();
            let direction = stem.direction_at(1.0);
            let offset = random_in_hemisphere(direction, rng) * 0.1;
            (tip + offset, direction)
        }
        LeafDistribution::AlongBranch => {
            // Random position along branch (30% to 100%)
            let t = 0.3 + rng.next_f32() * 0.7;
            let position = stem.point_at(t);
            let direction = stem.direction_at(t);
            let offset = random_in_hemisphere(direction, rng) * 0.05;
            (position + offset, direction)
        }
        LeafDistribution::Both => {
            // Randomly choose between endpoint and along
            if rng.next_f32() < 0.6 {
                choose_leaf_position(stem, LeafDistribution::Endpoint, rng)
            } else {
                choose_leaf_position(stem, LeafDistribution::AlongBranch, rng)
            }
        }
    }
}

/// Generate a random point in a hemisphere oriented along the given direction
fn random_in_hemisphere(direction: Vec3, rng: &mut Rng) -> Vec3 {
    let theta = rng.next_f32() * TAU;
    let phi = rng.next_f32() * PI * 0.5;

    let (tangent, bitangent) = create_basis(direction);

    let x = phi.sin() * theta.cos();
    let y = phi.sin() * theta.sin();
    let z = phi.cos();

    tangent * x + bitangent * y + direction * z
}

/// Calculate leaf rotation based on branch direction and up influence
fn calculate_leaf_rotation(branch_direction: Vec3, up_influence: f32, rng: &mut Rng) -> Quat {
    // Blend between branch direction and up vector
    let target_dir = (branch_direction * (1.0 - up_influence.abs()) + Vec3::Y * up_influence)
        .normalize_or_zero();

    // If target is zero, default to pointing up
    let target_dir = if target_dir.length_squared() < 0.001 {
        Vec3::Y
    } else {
        target_dir
    };

    // Random rotation around the direction axis (roll)
    let roll = rng.range(-PI, PI);

    // Create rotation from Y-axis to target direction
    let base_rotation = Quat::from_rotation_arc(Vec3::Y, target_dir);
    let roll_rotation = Quat::from_axis_angle(target_dir, roll);

    roll_rotation * base_rotation
}

/// Generate leaf mesh geometry
///
/// Creates mesh geometry for all leaves based on the specified geometry type.
///
/// # Arguments
///
/// * `leaves` - The leaf instances to generate geometry for
/// * `config` - Configuration for leaf geometry generation
/// * `tree` - The tree for bounds information (used in Pivot Painter encoding)
///
/// # Returns
///
/// A Mesh containing all leaf geometry with proper UV and Pivot Painter data
pub fn generate_leaf_mesh(leaves: &[Leaf], config: &LeafConfig, tree: &Tree) -> Mesh {
    if config.geometry == LeafGeometry::None || leaves.is_empty() {
        return Mesh::new();
    }
    let canopy = CanopyContext::new(leaves, tree, &config.canopy);
    // Optionally pull leaves onto their clump's shell so cards form puffy
    // outer surfaces instead of filling crown interiors nobody sees.
    let shelled;
    let leaves = if config.canopy.shell > 0.0 {
        shelled = canopy.apply_shell(leaves);
        &shelled[..]
    } else {
        leaves
    };
    let mut mesh = match config.geometry {
        LeafGeometry::Polygon => generate_polygon_leaves(leaves, config, tree),
        LeafGeometry::CrossBillboard => generate_cross_billboards(leaves, config, tree),
        LeafGeometry::Billboard => generate_billboards(leaves, config, tree),
        LeafGeometry::CanopyCard => generate_canopy_cards(leaves, config, tree, &canopy),
        LeafGeometry::None => Mesh::new(),
    };
    let per_leaf = match config.geometry {
        LeafGeometry::Polygon => 1 + config.effective_resolution().max(6) as usize,
        LeafGeometry::CrossBillboard => 8,
        LeafGeometry::Billboard | LeafGeometry::CanopyCard => 4,
        LeafGeometry::None => 0,
    };
    canopy.shade(&mut mesh, leaves, per_leaf, config.geometry);
    mesh
}

/// Per-tree foliage analysis used for canopy normals, card orientation,
/// shells and baked tint/AO. Leaves are grouped into clumps by the stem
/// subtree they hang from (`canopy.clump_level`).
pub struct CanopyContext {
    params: CanopyParams,
    crown_center: Vec3,
    crown_radii: Vec3,
    crown_min_y: f32,
    crown_max_y: f32,
    leaf_clump: Vec<usize>,
    clump_center: Vec<Vec3>,
    clump_radius: Vec<f32>,
    clump_tint: Vec<Vec3>,
}

impl CanopyContext {
    /// Analyse `leaves` on `tree`.
    pub fn new(leaves: &[Leaf], tree: &Tree, params: &CanopyParams) -> Self {
        use std::collections::HashMap;
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for leaf in leaves {
            lo = lo.min(leaf.position);
            hi = hi.max(leaf.position);
        }
        if leaves.is_empty() {
            lo = Vec3::ZERO;
            hi = Vec3::ONE;
        }
        let crown_center = (lo + hi) * 0.5;
        let crown_radii = ((hi - lo) * 0.5).max(Vec3::splat(0.05));

        let mut key_to_clump: HashMap<u32, usize> = HashMap::new();
        let mut leaf_clump = Vec::with_capacity(leaves.len());
        let mut sums: Vec<(Vec3, u32)> = Vec::new();
        for leaf in leaves {
            let key = clump_key(tree, leaf.stem_id, params.clump_level);
            let next = key_to_clump.len();
            let c = *key_to_clump.entry(key).or_insert(next);
            if c == sums.len() {
                sums.push((Vec3::ZERO, 0));
            }
            sums[c].0 += leaf.position;
            sums[c].1 += 1;
            leaf_clump.push(c);
        }
        let clump_center: Vec<Vec3> = sums.iter().map(|(s, n)| *s / (*n).max(1) as f32).collect();
        let mut rad_acc = vec![(0.0f32, 0u32); clump_center.len()];
        for (leaf, &c) in leaves.iter().zip(&leaf_clump) {
            rad_acc[c].0 += (leaf.position - clump_center[c]).length_squared();
            rad_acc[c].1 += 1;
        }
        let clump_radius = rad_acc
            .iter()
            .map(|(s, n)| (s / (*n).max(1) as f32).sqrt().max(0.05) * 1.2)
            .collect();
        let clump_tint = (0..clump_center.len())
            .map(|i| {
                let mut rng =
                    Rng::from_seed(tree.seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
                let light = 1.0 - params.tint_variation * rng.next_f32();
                let hue = params.hue_variation * rng.range(-1.0, 1.0);
                // +hue = warmer/yellower, -hue = cooler/bluer.
                Vec3::new(
                    light * (1.0 + hue * 0.55),
                    light * (1.0 + hue * 0.12),
                    light * (1.0 - hue * 0.9),
                )
                .clamp(Vec3::ZERO, Vec3::ONE)
            })
            .collect();

        Self {
            params: params.clone(),
            crown_center,
            crown_radii,
            crown_min_y: lo.y,
            crown_max_y: hi.y,
            leaf_clump,
            clump_center,
            clump_radius,
            clump_tint,
        }
    }

    /// Number of foliage clumps found.
    pub fn clump_count(&self) -> usize {
        self.clump_center.len()
    }

    /// Card size multiplier for leaf `i` from `card_scale_by_clump`.
    pub fn card_scale(&self, i: usize) -> f32 {
        let k = self.params.card_scale_by_clump.clamp(0.0, 1.0);
        if k == 0.0 || self.clump_radius.is_empty() {
            return 1.0;
        }
        let mean = self.clump_radius.iter().sum::<f32>() / self.clump_radius.len() as f32;
        let c = self.leaf_clump.get(i).copied().unwrap_or(0);
        let rel = self.clump_radius.get(c).copied().unwrap_or(mean) / mean.max(1e-4);
        (1.0 + (rel - 1.0) * k).clamp(0.3, 1.6)
    }

    /// Outward direction of leaf `i` at `pos`: clump-out blended with
    /// crown-out plus an upward bias.
    pub fn outward(&self, i: usize, pos: Vec3) -> Vec3 {
        let c = self.leaf_clump.get(i).copied().unwrap_or(0);
        let cc = self
            .clump_center
            .get(c)
            .copied()
            .unwrap_or(self.crown_center);
        let w = self.params.clump_weight.clamp(0.0, 1.0);
        let n_clump = (pos - cc).try_normalize().unwrap_or(Vec3::Y);
        let n_crown = ((pos - self.crown_center) / self.crown_radii)
            .try_normalize()
            .unwrap_or(Vec3::Y);
        (n_clump * w + n_crown * (1.0 - w) + Vec3::Y * self.params.up_bias)
            .try_normalize()
            .unwrap_or(Vec3::Y)
    }

    fn apply_shell(&self, leaves: &[Leaf]) -> Vec<Leaf> {
        let t = self.params.shell.clamp(0.0, 1.0);
        leaves
            .iter()
            .enumerate()
            .map(|(i, leaf)| {
                let c = self.leaf_clump[i];
                let cc = self.clump_center[c];
                let d = leaf.position - cc;
                let len = d.length();
                let mut out = leaf.clone();
                if len > 1e-4 {
                    // Keep some depth jitter so the shell isn't a hard sphere.
                    let target = self.clump_radius[c] * (0.75 + 0.25 * (i as f32 * 0.618).fract());
                    out.position = cc + d / len * (len + (target - len) * t);
                }
                out
            })
            .collect()
    }

    /// Bend normals and/or write tint+AO colours for leaf vertices laid out
    /// as `per_leaf` consecutive vertices per leaf.
    fn shade(&self, mesh: &mut Mesh, leaves: &[Leaf], per_leaf: usize, geometry: LeafGeometry) {
        let bend = self.params.normals == FoliageNormals::Canopy;
        let tint = self.params.vertex_color == FoliageVertexColor::TintAo;
        if (!bend && !tint) || per_leaf == 0 {
            return;
        }
        let blend = self.params.normal_blend.clamp(0.0, 1.0);
        for (i, leaf) in leaves.iter().enumerate() {
            let start = i * per_leaf;
            let end = (start + per_leaf).min(mesh.vertices.len());
            if start >= end {
                break;
            }
            let out = self.outward(i, leaf.position);
            let color = tint.then(|| self.tint_ao(i, leaf.position));
            for v in &mut mesh.vertices[start..end] {
                if bend {
                    // Canopy cards already face `out`; bending each vertex
                    // toward its own offset rounds the card a little too.
                    let local = if geometry == LeafGeometry::CanopyCard {
                        (out + (v.position - leaf.position) * 0.6 / leaf.scale.max(1e-3))
                            .try_normalize()
                            .unwrap_or(out)
                    } else {
                        out
                    };
                    let geom = if v.normal.dot(local) < 0.0 {
                        -v.normal
                    } else {
                        v.normal
                    };
                    v.normal = geom.lerp(local, blend).try_normalize().unwrap_or(local);
                }
                if let Some(c) = color {
                    v.color = c;
                }
            }
        }
    }

    /// Linear-RGB tint x ambient occlusion for leaf `i` at `pos`.
    pub fn tint_ao(&self, i: usize, pos: Vec3) -> Vec4 {
        let c = self.leaf_clump.get(i).copied().unwrap_or(0);
        let crown_depth = 1.0
            - ((pos - self.crown_center) / self.crown_radii)
                .length()
                .clamp(0.0, 1.0);
        let clump_depth = self
            .clump_center
            .get(c)
            .map(|cc| 1.0 - ((pos - *cc).length() / self.clump_radius[c]).clamp(0.0, 1.0))
            .unwrap_or(0.0);
        let span = (self.crown_max_y - self.crown_min_y).max(1e-3);
        let low = 1.0 - ((pos.y - self.crown_min_y) / span).clamp(0.0, 1.0);
        let occl = (0.45 * crown_depth + 0.3 * clump_depth + 0.35 * low).clamp(0.0, 1.0);
        let ao = 1.0 - self.params.ao_strength.clamp(0.0, 1.0) * occl;
        // Small per-leaf jitter keeps clumps from looking flat-filled.
        let jitter = 1.0 - 0.08 * (i as f32 * 0.754_877_7).fract();
        let t = self.clump_tint.get(c).copied().unwrap_or(Vec3::ONE) * ao * jitter;
        Vec4::new(t.x, t.y, t.z, 1.0)
    }
}

/// Rotate `n` toward +Y so it is at most `max_angle` radians from up.
fn clamp_from_up(n: Vec3, max_angle: f32) -> Vec3 {
    let angle = n.dot(Vec3::Y).clamp(-1.0, 1.0).acos();
    if angle <= max_angle || max_angle >= PI {
        return n;
    }
    let axis = n.cross(Vec3::Y).try_normalize().unwrap_or(Vec3::X);
    Quat::from_axis_angle(axis, angle - max_angle) * n
}

/// Clump key for a leaf: the id of the ancestor stem at `clump_level`.
fn clump_key(tree: &Tree, stem_id: u32, clump_level: u32) -> u32 {
    let mut id = stem_id;
    for _ in 0..16 {
        let Some(stem) = tree.get_stem(id) else {
            return id;
        };
        if stem.level as u32 <= clump_level.max(1) {
            return id;
        }
        match stem.parent_id {
            Some(p) => id = p,
            None => return id,
        }
    }
    id
}

/// Outward-facing canopy cards: one quad per leaf, centred on the leaf and
/// oriented along the clump/crown outward direction with tilt and roll jitter.
fn generate_canopy_cards(
    leaves: &[Leaf],
    config: &LeafConfig,
    tree: &Tree,
    canopy: &CanopyContext,
) -> Mesh {
    let mut mesh = Mesh::new();
    let tilt_max = config.canopy.card_tilt.to_radians();
    for (i, leaf) in leaves.iter().enumerate() {
        let mut rng = Rng::from_seed(tree.seed ^ (i as u64).wrapping_mul(0xA24B_AED4_963E_E407));
        let out = canopy.outward(i, leaf.position);
        let (t0, _) = create_basis(out);
        let tilt_axis = Quat::from_axis_angle(out, rng.range(-PI, PI)) * t0;
        let normal = Quat::from_axis_angle(tilt_axis, rng.range(-tilt_max, tilt_max)) * out;
        let normal = clamp_from_up(normal, config.canopy.card_max_angle.to_radians());
        let (tan, bit) = create_basis(normal);
        let roll = rng.range(-PI, PI);
        let (sr, cr) = roll.sin_cos();
        let mut u = tan * cr + bit * sr;
        let v = bit * cr - tan * sr;
        // Make (u, v, normal) right-handed so CCW winding faces outward.
        if u.cross(v).dot(normal) < 0.0 {
            u = -u;
        }
        let h = leaf.scale * 0.5 * canopy.card_scale(i);
        let base = mesh.vertices.len() as u32;
        for (cx, cy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let pos = leaf.position + u * (cx * h) + v * (cy * h);
            let uv = Vec2::new(cx * 0.5 + 0.5, 0.5 - cy * 0.5);
            let mut vertex = Vertex::new(pos, normal, uv);
            if config.pivot_painter {
                encode_leaf_pivot_painter(&mut vertex, leaf, tree);
            }
            mesh.vertices.push(vertex);
        }
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        if !config.canopy.single_sided {
            mesh.indices
                .extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
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

/// Generate polygon leaf shapes (no alpha testing needed)
///
/// Creates actual polygonal leaf geometry using SDF-based outline generation.
/// This produces higher quality leaves but uses more triangles.
fn generate_polygon_leaves(leaves: &[Leaf], config: &LeafConfig, tree: &Tree) -> Mesh {
    let mut mesh = Mesh::new();
    let resolution = config.effective_resolution().max(6);

    // Generate leaf outline vertices using SDF for the configured shape
    let outline = generate_leaf_outline(config.shape, resolution);

    for leaf in leaves {
        let base_idx = mesh.vertices.len() as u32;

        // Add center vertex
        let center = leaf.position;
        let normal = leaf.rotation * Vec3::Y;

        let mut center_vertex = Vertex::new(center, normal, Vec2::new(0.5, 0.5));
        if config.pivot_painter {
            encode_leaf_pivot_painter(&mut center_vertex, leaf, tree);
        }
        mesh.vertices.push(center_vertex);

        // Add perimeter vertices
        for (i, &outline_pos) in outline.iter().enumerate() {
            // Transform to world space
            let local_pos = Vec3::new(outline_pos.x, 0.0, outline_pos.y) * leaf.scale;
            let world_pos = leaf.position + leaf.rotation * local_pos;

            // UV from outline position (map to 0-1 range)
            let uv = Vec2::new(outline_pos.x * 0.5 + 0.5, outline_pos.y * 0.5 + 0.5);

            let mut vertex = Vertex::new(world_pos, normal, uv);
            if config.pivot_painter {
                encode_leaf_pivot_painter(&mut vertex, leaf, tree);
            }
            mesh.vertices.push(vertex);

            // Add triangle (fan from center)
            let next_i = (i + 1) % outline.len();
            mesh.indices.push(base_idx); // center
            mesh.indices.push(base_idx + 1 + i as u32);
            mesh.indices.push(base_idx + 1 + next_i as u32);
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

/// Generate cross billboard leaves (two quads at 90 degrees)
///
/// Creates two intersecting quads for each leaf, providing better
/// 3D appearance than single billboards with moderate cost.
fn generate_cross_billboards(leaves: &[Leaf], config: &LeafConfig, tree: &Tree) -> Mesh {
    let mut mesh = Mesh::new();

    for leaf in leaves {
        // First quad (XY plane)
        add_billboard_quad(&mut mesh, leaf, Vec3::Z, config, tree);
        // Second quad (ZY plane, rotated 90 degrees)
        add_billboard_quad(&mut mesh, leaf, Vec3::X, config, tree);
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

/// Generate single billboard leaves
///
/// Creates a single quad for each leaf. Cheapest geometry option,
/// suitable for distant LODs or performance-critical scenarios.
fn generate_billboards(leaves: &[Leaf], config: &LeafConfig, tree: &Tree) -> Mesh {
    let mut mesh = Mesh::new();

    for leaf in leaves {
        add_billboard_quad(&mut mesh, leaf, Vec3::Z, config, tree);
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

/// Add a billboard quad for a leaf
fn add_billboard_quad(
    mesh: &mut Mesh,
    leaf: &Leaf,
    normal_dir: Vec3,
    config: &LeafConfig,
    tree: &Tree,
) {
    let base_idx = mesh.vertices.len() as u32;
    let half_size = leaf.scale * 0.5;

    // Quad corners in local space (centered at origin, bottom-center aligned)
    let corners = [
        Vec3::new(-half_size, 0.0, 0.0),        // bottom-left
        Vec3::new(half_size, 0.0, 0.0),         // bottom-right
        Vec3::new(half_size, leaf.scale, 0.0),  // top-right
        Vec3::new(-half_size, leaf.scale, 0.0), // top-left
    ];

    let uvs = [
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 1.0),
    ];

    // Rotate to align with normal direction, then apply leaf rotation
    let quad_rotation = Quat::from_rotation_arc(Vec3::Z, normal_dir);
    let final_rotation = leaf.rotation * quad_rotation;

    let normal = final_rotation * Vec3::Z;

    for i in 0..4 {
        let world_pos = leaf.position + final_rotation * corners[i];
        let mut vertex = Vertex::new(world_pos, normal, uvs[i]);

        if config.pivot_painter {
            encode_leaf_pivot_painter(&mut vertex, leaf, tree);
        }

        mesh.vertices.push(vertex);
    }

    // Front face triangles (CCW winding)
    mesh.indices.push(base_idx);
    mesh.indices.push(base_idx + 1);
    mesh.indices.push(base_idx + 2);

    mesh.indices.push(base_idx);
    mesh.indices.push(base_idx + 2);
    mesh.indices.push(base_idx + 3);

    if config.canopy.single_sided {
        // The double-sided foliage material renders the back; skip duplicates.
        return;
    }

    // Back face triangles (opposite winding for two-sided rendering)
    mesh.indices.push(base_idx);
    mesh.indices.push(base_idx + 2);
    mesh.indices.push(base_idx + 1);

    mesh.indices.push(base_idx);
    mesh.indices.push(base_idx + 3);
    mesh.indices.push(base_idx + 2);
}

/// Encode Pivot Painter data for a leaf vertex
fn encode_leaf_pivot_painter(vertex: &mut Vertex, leaf: &Leaf, tree: &Tree) {
    // UV2: depth=1.0 (leaves are deepest in hierarchy), phase from stem
    let phase = (leaf.stem_id as f32 * GOLDEN_RATIO).fract();
    vertex.uv2 = Vec2::new(1.0, phase);

    // Color: normalized pivot position, stiffness=0 (most flexible)
    let bounds = &tree.bounds;
    let size = bounds.size();
    let pivot_norm = if size.length() > 0.001 {
        (leaf.position - bounds.min) / size
    } else {
        Vec3::ZERO
    };

    vertex.color = Vec4::new(pivot_norm.x, pivot_norm.y, pivot_norm.z, 0.0);
}

/// Generate leaf outline using SDF marching
///
/// Creates a polygon outline for a leaf shape by marching around the
/// SDF boundary at regular angular intervals.
fn generate_leaf_outline(shape: LeafShape, resolution: u32) -> Vec<Vec2> {
    let mut outline = Vec::with_capacity(resolution as usize);

    for i in 0..resolution {
        let angle = (i as f32 / resolution as f32) * TAU;
        let dir = Vec2::new(angle.cos(), angle.sin());

        // Binary search for edge point along this direction
        let t = find_sdf_edge(dir, shape, 0.0, 1.0, 8);
        outline.push(dir * t);
    }

    outline
}

/// Find the SDF edge point using binary search
fn find_sdf_edge(dir: Vec2, shape: LeafShape, min: f32, max: f32, iterations: u32) -> f32 {
    let mut lo = min;
    let mut hi = max;

    for _ in 0..iterations {
        let mid = (lo + hi) * 0.5;
        let sdf = leaf_sdf(dir * mid, shape);

        if sdf < 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    (lo + hi) * 0.5
}

/// Signed distance function for leaf shapes
///
/// Returns negative values inside the shape, positive outside.
/// `pub(crate)` so the texture pipeline can stamp the same silhouettes.
pub(crate) fn leaf_sdf(p: Vec2, shape: LeafShape) -> f32 {
    match shape {
        LeafShape::Oval => {
            // Ellipse: wider in X, narrower in Y
            let scaled = Vec2::new(p.x * 1.0, p.y * 0.6);
            scaled.length() - 0.5
        }
        LeafShape::Pointed => {
            // Egg shape: radius decreases toward top
            let r = 0.5 - p.y * 0.3;
            p.length() - r.max(0.1)
        }
        LeafShape::Needle => {
            // Long thin ellipse for conifer needles
            let scaled = Vec2::new(p.x * 0.08, p.y * 1.0);
            scaled.length() - 0.5
        }
        LeafShape::OakLobed => {
            // Oak leaf with 7 rounded lobes
            let angle = p.y.atan2(p.x);
            let lobe_count = 7.0;
            // Create rounded lobes using smooth sine wave
            let lobe_depth = 0.12;
            let base_r = 0.38 + lobe_depth * (angle * lobe_count).sin().abs();
            // Taper toward the stem end (negative Y)
            let taper = 1.0 - ((-p.y) * 0.4).max(0.0);
            // Slight taper at tip too
            let tip_taper = 1.0 - (p.y * 0.2).max(0.0);
            p.length() - base_r * taper * tip_taper
        }
        LeafShape::Maple => {
            // 5-pointed maple leaf
            let angle = p.y.atan2(p.x);
            let lobe_count = 5.0;
            // Sharp pointed lobes
            let lobe_angle = (angle * lobe_count + PI * 0.5).rem_euclid(TAU) - PI;
            let lobe_sharpness = 1.0 - (lobe_angle.abs() / (PI / lobe_count)).min(1.0);
            let lobe_r = 0.25 + 0.25 * lobe_sharpness.powf(0.6);
            // Center depression between lobes
            let center_factor = 1.0 - p.length() * 0.3;
            p.length() - lobe_r * center_factor.max(0.5)
        }
        LeafShape::Serrated => {
            // Serrated oval (birch, elm) - oval with small teeth
            let angle = p.y.atan2(p.x);
            let tooth_count = 16.0;
            let tooth_depth = 0.04;
            // Base oval shape
            let scaled = Vec2::new(p.x * 0.9, p.y * 0.55);
            let base_dist = scaled.length() - 0.45;
            // Add serration
            let serration = tooth_depth * (angle * tooth_count).sin();
            base_dist - serration
        }
        LeafShape::Willow => {
            // Long narrow willow leaf (lanceolate)
            // Pointed at both ends, widest in middle
            let y_factor = 1.0 - (p.y * 2.0).abs().min(1.0);
            let width = 0.12 * y_factor.powf(0.5);
            let scaled = Vec2::new(p.x / width.max(0.02), p.y * 1.2);
            scaled.length() - 0.5
        }
        LeafShape::Heart => {
            // Heart-shaped leaf (cordate)
            // Two rounded lobes at top, pointed tip at bottom
            let px = p.x.abs(); // Mirror on X axis
            let py = p.y;

            // Upper lobes (two circles)
            if py > 0.0 {
                let lobe_center = Vec2::new(0.18, 0.1);
                let to_lobe = Vec2::new(px, py) - lobe_center;
                to_lobe.length() - 0.28
            } else {
                // Lower pointed section
                let tip_factor = 1.0 + py * 1.5; // Narrows toward bottom
                let width = 0.35 * tip_factor.max(0.0);
                if width < 0.01 {
                    p.length() - 0.01 // Point at very bottom
                } else {
                    px - width
                }
            }
        }
        LeafShape::Palmate => {
            // Compound palmate leaf (5-7 leaflets radiating from center)
            let angle = p.y.atan2(p.x);
            let leaflet_count = 7.0;

            // Each leaflet is an elongated oval
            let leaflet_angle = (angle * leaflet_count / TAU * PI).rem_euclid(PI) - PI * 0.5;
            let in_leaflet = leaflet_angle.abs() < PI * 0.35;

            if in_leaflet {
                // Inside a leaflet - elongated shape
                let dist_from_center = p.length();
                let leaflet_width = 0.08 * (1.0 - dist_from_center * 0.8).max(0.0);
                let side_dist = leaflet_angle.abs() * dist_from_center - leaflet_width;
                let tip_dist = dist_from_center - 0.5;
                side_dist.max(tip_dist)
            } else {
                // Between leaflets
                p.length() - 0.1
            }
        }
    }
}

/// Add leaves to tree structure
///
/// Generates leaves for the tree using species parameters and mutates
/// the tree to include them.
///
/// # Arguments
///
/// * `tree` - The tree to add leaves to (mutated)
/// * `species` - Species parameters for leaf generation
/// * `seed` - Random seed for deterministic generation
pub fn add_leaves_to_tree(tree: &mut Tree, species: &Species, seed: u64) {
    // Use a different seed offset for leaves to avoid correlation with branches
    let mut rng = Rng::from_seed(seed.wrapping_add(0xDEAD_1EAF));
    tree.leaves = place_leaves(tree, species, &mut rng);
    let fill = crown_fill_leaves(&tree.leaves, species, &mut rng);
    tree.leaves.extend(fill);
}

/// Extra leaves over the crown envelope (`canopy.crown_fill`). The envelope is
/// the axis-aligned ellipsoid of the placed leaves, shrunk slightly; points
/// are spread on a jittered Fibonacci sphere, skipping most of the underside.
/// Each fill leaf borrows the stem of its nearest placed leaf so clump
/// grouping, tint and normals treat it as part of that limb.
fn crown_fill_leaves(leaves: &[Leaf], species: &Species, rng: &mut Rng) -> Vec<Leaf> {
    let n = species.canopy.crown_fill as usize;
    if n == 0 || leaves.is_empty() {
        return Vec::new();
    }
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for leaf in leaves {
        lo = lo.min(leaf.position);
        hi = hi.max(leaf.position);
    }
    let center = (lo + hi) * 0.5;
    let radii = (hi - lo) * 0.5 * 0.82;
    let params = &species.leaves;
    let mut out = Vec::with_capacity(n);
    // Generate over a sphere but only keep y >= -0.35 (about 85% of it).
    let total = (n as f32 / 0.675).ceil() as usize;
    for i in 0..total {
        if out.len() >= n {
            break;
        }
        let y = 1.0 - 2.0 * (i as f32 + 0.5) / total as f32;
        if y < -0.35 {
            continue;
        }
        let r = (1.0 - y * y).max(0.0).sqrt();
        let phi = i as f32 * 2.399_963 + rng.range(-0.25, 0.25);
        let dir = Vec3::new(r * phi.cos(), y, r * phi.sin());
        let depth = rng.range(0.78, 1.0);
        let position = center + dir * radii * depth;
        let nearest = leaves
            .iter()
            .min_by(|a, b| {
                (a.position - position)
                    .length_squared()
                    .total_cmp(&(b.position - position).length_squared())
            })
            .map(|l| l.stem_id)
            .unwrap_or(0);
        out.push(Leaf {
            position,
            rotation: Quat::from_rotation_arc(Vec3::Y, dir),
            scale: params.size * rng.variance_mul(params.size_variance),
            stem_id: nearest,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Segment;

    fn create_test_tree() -> Tree {
        let mut tree = Tree::new("Test".to_string(), 42);

        // Add a simple trunk
        let mut trunk = Stem::new(0, 0);
        trunk.segments.push(Segment {
            start: Vec3::ZERO,
            end: Vec3::new(0.0, 5.0, 0.0),
            start_radius: 0.3,
            end_radius: 0.2,
            direction: Vec3::Y,
        });
        tree.add_stem(trunk);

        // Add a branch at level 2
        let mut branch = Stem::new(1, 2);
        branch.parent_id = Some(0);
        branch.parent_offset = 0.8;
        branch.segments.push(Segment {
            start: Vec3::new(0.0, 4.0, 0.0),
            end: Vec3::new(1.0, 4.5, 0.0),
            start_radius: 0.1,
            end_radius: 0.05,
            direction: Vec3::new(1.0, 0.5, 0.0).normalize(),
        });
        tree.add_stem(branch);

        tree.update_bounds();
        tree
    }

    fn create_test_species() -> Species {
        let toml = r#"
[species]
name = "Test Tree"

[trunk]
height = 5.0

[branches.level1]
count = 3
length = 2.0

[leaves]
count = 100
min_level = 2
size = 0.1
distribution = "both"
geometry = "cross_billboard"
"#;
        Species::from_toml(toml).unwrap()
    }

    #[test]
    fn test_place_leaves() {
        let tree = create_test_tree();
        let species = create_test_species();
        let mut rng = Rng::from_seed(42);

        let leaves = place_leaves(&tree, &species, &mut rng);

        // Should have placed some leaves
        assert!(!leaves.is_empty());

        // All leaves should have valid positions
        for leaf in &leaves {
            assert!(leaf.position.is_finite());
            assert!(leaf.scale > 0.0);
        }
    }

    #[test]
    fn test_place_leaves_deterministic() {
        let tree = create_test_tree();
        let species = create_test_species();

        let mut rng1 = Rng::from_seed(12345);
        let mut rng2 = Rng::from_seed(12345);

        let leaves1 = place_leaves(&tree, &species, &mut rng1);
        let leaves2 = place_leaves(&tree, &species, &mut rng2);

        assert_eq!(leaves1.len(), leaves2.len());

        for (l1, l2) in leaves1.iter().zip(leaves2.iter()) {
            assert!((l1.position - l2.position).length() < f32::EPSILON);
            assert!((l1.scale - l2.scale).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn test_generate_polygon_leaves() {
        let tree = create_test_tree();
        let leaves = vec![Leaf {
            position: Vec3::new(1.0, 4.5, 0.0),
            rotation: Quat::IDENTITY,
            scale: 0.1,
            stem_id: 1,
        }];

        let config = LeafConfig {
            geometry: LeafGeometry::Polygon,
            polygon_resolution: 8,
            ..Default::default()
        };

        let mesh = generate_polygon_leaves(&leaves, &config, &tree);

        // Should have vertices (center + 8 perimeter)
        assert_eq!(mesh.vertex_count(), 9);

        // Should have 8 triangles (one per perimeter segment)
        assert_eq!(mesh.triangle_count(), 8);

        // Should have one submesh with Leaves material
        assert_eq!(mesh.submeshes.len(), 1);
        assert_eq!(mesh.submeshes[0].material, MaterialType::Leaves);
    }

    #[test]
    fn test_generate_cross_billboards() {
        let tree = create_test_tree();
        let leaves = vec![Leaf {
            position: Vec3::new(1.0, 4.5, 0.0),
            rotation: Quat::IDENTITY,
            scale: 0.1,
            stem_id: 1,
        }];

        let config = LeafConfig {
            geometry: LeafGeometry::CrossBillboard,
            ..Default::default()
        };

        let mesh = generate_cross_billboards(&leaves, &config, &tree);

        // 2 quads * 4 vertices = 8 vertices
        assert_eq!(mesh.vertex_count(), 8);

        // 2 quads * 4 triangles (front + back) = 8 triangles
        assert_eq!(mesh.triangle_count(), 8);
    }

    #[test]
    fn test_generate_billboards() {
        let tree = create_test_tree();
        let leaves = vec![Leaf {
            position: Vec3::new(1.0, 4.5, 0.0),
            rotation: Quat::IDENTITY,
            scale: 0.1,
            stem_id: 1,
        }];

        let config = LeafConfig {
            geometry: LeafGeometry::Billboard,
            ..Default::default()
        };

        let mesh = generate_billboards(&leaves, &config, &tree);

        // 1 quad * 4 vertices = 4 vertices
        assert_eq!(mesh.vertex_count(), 4);

        // 1 quad * 4 triangles (front + back) = 4 triangles
        assert_eq!(mesh.triangle_count(), 4);
    }

    #[test]
    fn test_generate_leaf_mesh_none() {
        let tree = create_test_tree();
        let leaves = vec![Leaf {
            position: Vec3::new(1.0, 4.5, 0.0),
            rotation: Quat::IDENTITY,
            scale: 0.1,
            stem_id: 1,
        }];

        let config = LeafConfig {
            geometry: LeafGeometry::None,
            ..Default::default()
        };

        let mesh = generate_leaf_mesh(&leaves, &config, &tree);

        // Should be empty
        assert!(mesh.is_empty());
    }

    #[test]
    fn test_leaf_sdf_shapes() {
        // Test that SDF returns negative inside, positive outside
        let center = Vec2::ZERO;
        let far = Vec2::new(2.0, 2.0);

        // Test all shapes
        let shapes = [
            LeafShape::Oval,
            LeafShape::Pointed,
            LeafShape::Needle,
            LeafShape::OakLobed,
            LeafShape::Maple,
            LeafShape::Serrated,
            LeafShape::Willow,
            LeafShape::Heart,
            LeafShape::Palmate,
        ];

        for shape in shapes {
            // All shapes should have negative SDF at center (inside)
            assert!(
                leaf_sdf(center, shape) < 0.0,
                "{:?} should be negative at center",
                shape
            );

            // Points far from center should be outside
            assert!(
                leaf_sdf(far, shape) > 0.0,
                "{:?} should be positive far from center",
                shape
            );
        }
    }

    #[test]
    fn test_generate_leaf_outline() {
        let outline = generate_leaf_outline(LeafShape::Oval, 12);

        // Should have correct number of points
        assert_eq!(outline.len(), 12);

        // All points should be at similar distances from center (roughly)
        for point in &outline {
            assert!(point.length() > 0.1);
            assert!(point.length() < 1.0);
        }
    }

    #[test]
    fn test_add_leaves_to_tree() {
        let mut tree = create_test_tree();
        let species = create_test_species();

        assert!(tree.leaves.is_empty());

        add_leaves_to_tree(&mut tree, &species, 42);

        // Should have leaves now
        assert!(!tree.leaves.is_empty());
    }

    #[test]
    fn test_leaf_config_from_species() {
        let species = create_test_species();
        let config = LeafConfig::from_species(&species);

        assert_eq!(config.max_leaves, species.leaves.count);
        assert_eq!(config.geometry, species.leaves.geometry);
        assert_eq!(config.up_influence, species.leaves.up_influence);
    }

    #[test]
    fn test_calculate_stem_weight() {
        let mut stem = Stem::new(0, 2);
        stem.segments.push(Segment {
            start: Vec3::ZERO,
            end: Vec3::new(0.0, 2.0, 0.0),
            start_radius: 0.1,
            end_radius: 0.05,
            direction: Vec3::Y,
        });

        // Terminal stem (no children) should have weight for endpoint distribution
        let endpoint_weight = calculate_stem_weight(&stem, LeafDistribution::Endpoint);
        assert_eq!(endpoint_weight, 1.0);

        // Along branch should use length
        let along_weight = calculate_stem_weight(&stem, LeafDistribution::AlongBranch);
        assert!(along_weight > 0.0);

        // Add a child to make it non-terminal
        stem.child_ids.push(1);
        let endpoint_weight_with_child = calculate_stem_weight(&stem, LeafDistribution::Endpoint);
        assert_eq!(endpoint_weight_with_child, 0.0);
    }

    #[test]
    fn test_pivot_painter_encoding() {
        let tree = create_test_tree();
        // Place leaf within tree bounds (0,0,0) to (1,5,0)
        let leaf = Leaf {
            position: Vec3::new(0.5, 2.5, 0.0),
            rotation: Quat::IDENTITY,
            scale: 0.1,
            stem_id: 1,
        };

        let mut vertex = Vertex::new(leaf.position, Vec3::Y, Vec2::ZERO);
        encode_leaf_pivot_painter(&mut vertex, &leaf, &tree);

        // UV2.x should be 1.0 (leaf depth)
        assert!((vertex.uv2.x - 1.0).abs() < f32::EPSILON);

        // UV2.y should be a valid phase (0-1)
        assert!(vertex.uv2.y >= 0.0 && vertex.uv2.y < 1.0);

        // Color.w should be 0.0 (leaf stiffness)
        assert!((vertex.color.w - 0.0).abs() < f32::EPSILON);

        // Color xyz should be normalized position (values can be outside 0-1 if outside bounds)
        // Since we placed the leaf at (0.5, 2.5, 0.0) and bounds are (0,0,0) to (1,5,0):
        // x normalized: 0.5 / 1.0 = 0.5
        // y normalized: 2.5 / 5.0 = 0.5
        // z normalized: 0.0 / 0.0 = NaN, but we handle zero size, so it should be 0
        assert!(vertex.color.x >= 0.0 && vertex.color.x <= 1.0);
        assert!(vertex.color.y >= 0.0 && vertex.color.y <= 1.0);
        // Z bounds are 0 to 0, so z should be 0 (handled by zero-size check)
        assert!(vertex.color.z == 0.0 || (vertex.color.z >= 0.0 && vertex.color.z <= 1.0));
    }

    #[test]
    fn test_random_in_hemisphere() {
        let direction = Vec3::Y;
        let mut rng = Rng::from_seed(42);

        for _ in 0..100 {
            let point = random_in_hemisphere(direction, &mut rng);

            // Point should be normalized-ish (within reasonable bounds)
            assert!(point.length() > 0.0);
            assert!(point.length() <= 1.5);

            // Should generally point in the hemisphere direction
            // (dot product with direction should be >= 0 for hemisphere)
            assert!(point.dot(direction) >= -0.01);
        }
    }

    #[test]
    fn test_calculate_leaf_rotation() {
        let mut rng = Rng::from_seed(42);

        // With up_influence = 0, should align with branch direction
        let branch_dir = Vec3::new(1.0, 0.5, 0.0).normalize();
        let rotation = calculate_leaf_rotation(branch_dir, 0.0, &mut rng);

        // Rotation should be valid quaternion
        assert!((rotation.length() - 1.0).abs() < 0.001);

        // With up_influence = 1, should point mostly up
        let mut rng2 = Rng::from_seed(42);
        let rotation_up = calculate_leaf_rotation(branch_dir, 1.0, &mut rng2);

        // The rotated Y axis should be close to world Y
        let rotated_y = rotation_up * Vec3::Y;
        assert!(rotated_y.y > 0.5);
    }

    fn canopy_species(extra: &str) -> Species {
        Species::from_toml(&format!(
            r#"
[species]
name = "Canopy Test"

[trunk]
height = 2.0
radius = 0.1

[branches.level1]
count = 5
length = 0.8

[branches.level2]
count = 3
length = 0.3

[leaves]
count = 120
min_level = 1
size = 0.3
geometry = "canopy_card"

{extra}
"#
        ))
        .unwrap()
    }

    fn canopy_tree(species: &Species) -> Tree {
        crate::generation::generate_tree(species, 7).unwrap()
    }

    #[test]
    fn canopy_cards_single_sided_halves_triangles() {
        let double = canopy_species("");
        let single = canopy_species("[canopy]\nsingle_sided = true");
        let tree = canopy_tree(&double);
        let a = generate_leaf_mesh(&tree.leaves, &LeafConfig::from_species(&double), &tree);
        let b = generate_leaf_mesh(&tree.leaves, &LeafConfig::from_species(&single), &tree);
        assert_eq!(a.vertex_count(), tree.leaves.len() * 4);
        assert_eq!(b.vertex_count(), a.vertex_count());
        assert_eq!(a.triangle_count(), b.triangle_count() * 2);
    }

    #[test]
    fn canopy_cards_respect_max_angle_from_up() {
        let species = canopy_species("[canopy]\ncard_max_angle = 40.0\ncard_tilt = 60.0");
        let tree = canopy_tree(&species);
        let mesh = generate_leaf_mesh(&tree.leaves, &LeafConfig::from_species(&species), &tree);
        let limit = 40.0f32.to_radians().cos() - 1e-3;
        for v in &mesh.vertices {
            assert!(
                v.normal.y >= limit,
                "normal {:?} exceeds max angle",
                v.normal
            );
        }
    }

    #[test]
    fn tint_ao_writes_opaque_colours_in_unit_range() {
        let species = canopy_species(
            "[canopy]\nnormals = \"canopy\"\nvertex_color = \"tint_ao\"\nao_strength = 0.6",
        );
        let tree = canopy_tree(&species);
        let mesh = generate_leaf_mesh(&tree.leaves, &LeafConfig::from_species(&species), &tree);
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for v in &mesh.vertices {
            assert_eq!(v.color.w, 1.0);
            for c in [v.color.x, v.color.y, v.color.z] {
                assert!((0.0..=1.0).contains(&c));
                lo = lo.min(c);
                hi = hi.max(c);
            }
            assert!((v.normal.length() - 1.0).abs() < 1e-3);
        }
        assert!(hi - lo > 0.1, "expected visible tint/AO variation");
    }

    #[test]
    fn crown_fill_adds_requested_leaves() {
        let base = canopy_species("");
        let filled = canopy_species("[canopy]\ncrown_fill = 50");
        let a = canopy_tree(&base);
        let b = canopy_tree(&filled);
        assert_eq!(b.leaves.len(), a.leaves.len() + 50);
    }
}
