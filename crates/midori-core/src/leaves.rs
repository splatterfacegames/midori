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
    species::{LeafDistribution, LeafGeometry, Species},
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
pub use crate::species::LeafShape;

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
    match config.geometry {
        LeafGeometry::Polygon => generate_polygon_leaves(leaves, config, tree),
        LeafGeometry::CrossBillboard => generate_cross_billboards(leaves, config, tree),
        LeafGeometry::Billboard => generate_billboards(leaves, config, tree),
        LeafGeometry::None => Mesh::new(),
    }
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
}
