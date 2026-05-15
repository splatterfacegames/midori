//! Mesh builder for generating branch geometry.

use crate::{
    constants::{GOLDEN_RATIO, MAX_BRANCH_LEVELS, TAU},
    math::create_basis,
    mesh::{MaterialType, Mesh, Submesh, Vertex},
    tree::{Stem, Tree},
};
use glam::{Vec2, Vec3, Vec4};

/// Configuration for mesh generation
#[derive(Debug, Clone)]
pub struct MeshConfig {
    /// Ring resolution per branch level [trunk, level1, level2, level3]
    pub ring_resolution: [u32; 4],
    /// Texture V scale (repeats per meter)
    pub texture_v_scale: f32,
    /// Enable Pivot Painter data encoding
    pub pivot_painter: bool,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            // Higher resolution for production-quality trees
            // Trunk needs 24+ segments for smooth appearance at close range
            ring_resolution: [24, 16, 10, 6],
            texture_v_scale: 1.0,
            pivot_painter: true,
        }
    }
}

/// Build mesh from generated tree
pub struct MeshBuilder<'a> {
    tree: &'a Tree,
    config: MeshConfig,
    mesh: Mesh,
}

impl<'a> MeshBuilder<'a> {
    /// Create a new mesh builder for the given tree
    pub fn new(tree: &'a Tree, config: MeshConfig) -> Self {
        Self {
            tree,
            config,
            mesh: Mesh::new(),
        }
    }

    /// Build complete branch mesh
    pub fn build_branches(mut self) -> Mesh {
        for stem in &self.tree.stems {
            self.build_stem(stem);
        }

        // Add bark submesh for all branch geometry
        if !self.mesh.indices.is_empty() {
            self.mesh.submeshes.push(Submesh {
                index_start: 0,
                index_count: self.mesh.indices.len() as u32,
                material: MaterialType::Bark,
            });
        }

        self.mesh
    }


    /// Build branches up to max_level depth
    ///
    /// This is useful for LOD generation where lower LOD levels
    /// should exclude smaller branches.
    ///
    /// # Arguments
    ///
    /// * `max_level` - Maximum branch level to include (0 = trunk only, 1 = trunk + primary branches, etc.)
    pub fn build_branches_to_level(mut self, max_level: u32) -> Mesh {
        for stem in &self.tree.stems {
            if stem.level as u32 <= max_level {
                self.build_stem(stem);
            }
        }

        // Add bark submesh for all branch geometry
        if !self.mesh.indices.is_empty() {
            self.mesh.submeshes.push(Submesh {
                index_start: 0,
                index_count: self.mesh.indices.len() as u32,
                material: MaterialType::Bark,
            });
        }

        self.mesh
    }

    fn build_stem(&mut self, stem: &Stem) {
        if stem.segments.is_empty() {
            return;
        }

        // Get average radius for adaptive resolution calculation
        let avg_radius = stem.segments.iter()
            .map(|s| (s.start_radius + s.end_radius) / 2.0)
            .sum::<f32>() / stem.segments.len() as f32;

        // Use adaptive resolution based on branch thickness
        let ring_res = self.adaptive_ring_resolution(stem.level, avg_radius);
        if ring_res < 3 {
            return; // Too few vertices to make a cylinder
        }

        // Generate ring vertices for each segment joint
        let mut rings: Vec<Vec<u32>> = Vec::new();
        let mut v_coord = 0.0; // Accumulated V coordinate

        for (seg_idx, seg) in stem.segments.iter().enumerate() {
            // Create ring at segment start
            if seg_idx == 0 {
                let ring = self.create_ring(
                    seg.start,
                    seg.direction,
                    seg.start_radius,
                    ring_res,
                    v_coord,
                    stem,
                );
                rings.push(ring);
            }

            // Advance V coordinate by segment length
            v_coord += (seg.end - seg.start).length() * self.config.texture_v_scale;

            // Create ring at segment end
            let direction = if seg_idx + 1 < stem.segments.len() {
                // Average with next segment direction for smooth transition
                let next = &stem.segments[seg_idx + 1];
                ((seg.direction + next.direction) * 0.5).normalize()
            } else {
                seg.direction
            };

            let ring =
                self.create_ring(seg.end, direction, seg.end_radius, ring_res, v_coord, stem);
            rings.push(ring);
        }

        // Connect adjacent rings with faces
        for i in 0..rings.len() - 1 {
            self.connect_rings(&rings[i], &rings[i + 1]);
        }

        // Cap the end if it's a terminal branch
        if stem.child_ids.is_empty() && !rings.is_empty() {
            self.cap_ring(rings.last().unwrap());
        }
    }

    fn create_ring(
        &mut self,
        center: Vec3,
        direction: Vec3,
        radius: f32,
        resolution: u32,
        v_coord: f32,
        stem: &Stem,
    ) -> Vec<u32> {
        let (tangent, bitangent) = create_basis(direction);
        // +1 for seam vertex at U=1.0 to avoid texture seam artifacts
        let mut ring_indices = Vec::with_capacity(resolution as usize + 1);

        // Generate resolution+1 vertices (0 to resolution inclusive)
        // Last vertex duplicates position of first but has U=1.0 for proper UV wrap
        for i in 0..=resolution {
            let angle = (i as f32 / resolution as f32) * TAU;
            let offset = tangent * angle.cos() + bitangent * angle.sin();

            let position = center + offset * radius;
            let normal = offset; // Points outward
            // U goes from 0.0 to 1.0 (inclusive) for seamless texture wrap
            let uv = Vec2::new(i as f32 / resolution as f32, v_coord);

            let mut vertex = Vertex::new(position, normal, uv);

            // Add Pivot Painter data
            if self.config.pivot_painter {
                self.encode_pivot_painter(&mut vertex, stem);
            }

            let idx = self.mesh.vertices.len() as u32;
            self.mesh.vertices.push(vertex);
            ring_indices.push(idx);
        }

        ring_indices
    }

    fn connect_rings(&mut self, ring_a: &[u32], ring_b: &[u32]) {
        // Ring has resolution+1 vertices (last is seam duplicate)
        // Connect resolution quads (not wrapping, since last vertex handles the seam)
        let n = ring_a.len() - 1; // -1 because last vertex is seam duplicate

        for i in 0..n {
            let a0 = ring_a[i];
            let a1 = ring_a[i + 1];
            let b0 = ring_b[i];
            let b1 = ring_b[i + 1];

            // Two triangles per quad (CCW winding)
            // First triangle
            self.mesh.indices.push(a0);
            self.mesh.indices.push(b0);
            self.mesh.indices.push(b1);

            // Second triangle
            self.mesh.indices.push(a0);
            self.mesh.indices.push(b1);
            self.mesh.indices.push(a1);
        }
    }

    fn cap_ring(&mut self, ring: &[u32]) {
        // Ring has resolution+1 vertices, last is seam duplicate
        // Use only the unique vertices for the cap (exclude last seam vertex)
        let actual_count = ring.len() - 1;
        if actual_count < 3 {
            return;
        }

        // Calculate center position (exclude seam vertex)
        let center: Vec3 = ring[..actual_count]
            .iter()
            .map(|&i| self.mesh.vertices[i as usize].position)
            .sum::<Vec3>()
            / actual_count as f32;

        // Get normal from first vertex's direction (average of ring normals would be better
        // but this is simpler and works for our use case)
        let normal = self.mesh.vertices[ring[0] as usize].normal;

        // Add center vertex
        let uv = Vec2::new(0.5, 0.5);
        let vertex = Vertex::new(center, normal, uv);
        let center_idx = self.mesh.vertices.len() as u32;
        self.mesh.vertices.push(vertex);

        // Fan triangulation (use actual_count, not ring.len())
        for i in 0..actual_count {
            let next_i = (i + 1) % actual_count;
            self.mesh.indices.push(ring[i]);
            self.mesh.indices.push(ring[next_i]);
            self.mesh.indices.push(center_idx);
        }
    }

    fn encode_pivot_painter(&self, vertex: &mut Vertex, stem: &Stem) {
        // UV2: x = branch depth (normalized), y = phase offset (for animation variation)
        let depth = stem.level as f32 / MAX_BRANCH_LEVELS as f32;
        let phase = (stem.id as f32 * GOLDEN_RATIO).fract(); // Pseudo-random phase
        vertex.uv2 = Vec2::new(depth, phase);

        // Color: RGB = pivot position (normalized to bounding box), A = stiffness
        let pivot = if let Some(parent_id) = stem.parent_id {
            // Pivot is parent attachment point
            if let Some(parent) = self.tree.get_stem(parent_id) {
                parent.point_at(stem.parent_offset)
            } else {
                stem.base()
            }
        } else {
            // Trunk pivots at ground
            Vec3::ZERO
        };

        // Normalize pivot to bounding box
        let bounds = &self.tree.bounds;
        let size = bounds.size();
        let pivot_norm = if size.length() > 0.001 {
            (pivot - bounds.min) / size
        } else {
            Vec3::ZERO
        };

        // Stiffness: trunk is stiff (1.0), deeper branches are more flexible (lower values)
        let stiffness = 1.0 - depth;

        vertex.color = Vec4::new(pivot_norm.x, pivot_norm.y, pivot_norm.z, stiffness);
    }

    fn ring_resolution_for_level(&self, level: u8) -> u32 {
        self.config
            .ring_resolution
            .get(level as usize)
            .copied()
            .unwrap_or(4)
            .max(3)
    }

    /// Adaptive ring resolution based on branch radius.
    /// Thin branches don't need as many segments - reduces polygon count
    /// without visible quality loss.
    fn adaptive_ring_resolution(&self, level: u8, radius: f32) -> u32 {
        let base = self.ring_resolution_for_level(level);

        // Very thin branches (< 2cm) use half resolution
        if radius < 0.02 {
            (base / 2).max(3)
        }
        // Thin branches (< 5cm) use 2/3 resolution
        else if radius < 0.05 {
            (base * 2 / 3).max(4)
        }
        // Normal branches use full resolution
        else {
            base
        }
    }
}

/// Generate mesh from tree with default configuration
pub fn build_mesh(tree: &Tree) -> Mesh {
    MeshBuilder::new(tree, MeshConfig::default()).build_branches()
}

/// Generate mesh from tree with custom configuration
pub fn build_mesh_with_config(tree: &Tree, config: MeshConfig) -> Mesh {
    MeshBuilder::new(tree, config).build_branches()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Segment;

    fn create_simple_tree() -> Tree {
        let mut tree = Tree::new("Test".to_string(), 42);

        // Create a simple trunk
        let mut trunk = Stem::new(0, 0);
        trunk.segments.push(Segment {
            start: Vec3::ZERO,
            end: Vec3::new(0.0, 2.0, 0.0),
            start_radius: 0.5,
            end_radius: 0.4,
            direction: Vec3::Y,
        });
        trunk.segments.push(Segment {
            start: Vec3::new(0.0, 2.0, 0.0),
            end: Vec3::new(0.0, 4.0, 0.0),
            start_radius: 0.4,
            end_radius: 0.3,
            direction: Vec3::Y,
        });
        tree.add_stem(trunk);

        tree.update_bounds();
        tree
    }

    fn create_tree_with_branch() -> Tree {
        let mut tree = Tree::new("Test".to_string(), 42);

        // Create trunk
        let mut trunk = Stem::new(0, 0);
        trunk.segments.push(Segment {
            start: Vec3::ZERO,
            end: Vec3::new(0.0, 3.0, 0.0),
            start_radius: 0.5,
            end_radius: 0.3,
            direction: Vec3::Y,
        });
        trunk.child_ids.push(1);
        tree.add_stem(trunk);

        // Create branch
        let mut branch = Stem::new(1, 1);
        branch.parent_id = Some(0);
        branch.parent_offset = 0.5;
        branch.segments.push(Segment {
            start: Vec3::new(0.0, 1.5, 0.0),
            end: Vec3::new(1.0, 2.0, 0.0),
            start_radius: 0.15,
            end_radius: 0.1,
            direction: Vec3::new(1.0, 0.5, 0.0).normalize(),
        });
        tree.add_stem(branch);

        tree.update_bounds();
        tree
    }

    #[test]
    fn test_mesh_config_default() {
        let config = MeshConfig::default();
        assert_eq!(config.ring_resolution, [24, 16, 10, 6]);
        assert!((config.texture_v_scale - 1.0).abs() < 0.001);
        assert!(config.pivot_painter);
    }

    #[test]
    fn test_build_simple_mesh() {
        let tree = create_simple_tree();
        let mesh = build_mesh(&tree);

        // Should have vertices and triangles
        assert!(mesh.vertex_count() > 0);
        assert!(mesh.triangle_count() > 0);

        // Should have one submesh (bark)
        assert_eq!(mesh.submeshes.len(), 1);
        assert_eq!(mesh.submeshes[0].material, MaterialType::Bark);
    }

    #[test]
    fn test_mesh_has_correct_topology() {
        let tree = create_simple_tree();
        let config = MeshConfig {
            ring_resolution: [8, 6, 4, 3],
            ..Default::default()
        };
        let mesh = build_mesh_with_config(&tree, config);

        // With 8 vertices per ring, 3 rings (2 segments), plus cap center
        // Vertices: 8 * 3 + 1 = 25
        // Triangles: 2 quads per column * 8 columns * 2 segments + 8 cap triangles
        //          = 2 * 8 * 2 + 8 = 32 + 8 = 40 (but actually 2 rings between segments)

        // All indices should be valid
        for &idx in &mesh.indices {
            assert!(
                (idx as usize) < mesh.vertex_count(),
                "Invalid index {} >= {}",
                idx,
                mesh.vertex_count()
            );
        }

        // Indices should be multiple of 3 (triangles)
        assert_eq!(mesh.indices.len() % 3, 0);
    }

    #[test]
    fn test_mesh_normals_point_outward() {
        let tree = create_simple_tree();
        let mesh = build_mesh(&tree);

        // Check that normals point outward from the trunk center axis
        for vertex in &mesh.vertices {
            if vertex.normal.length() > 0.5 {
                // Skip cap center vertex
                // For a trunk along Y axis, X and Z components of normal should be non-zero
                // and Y component should be small for side vertices
                let horizontal_mag = (vertex.normal.x.powi(2) + vertex.normal.z.powi(2)).sqrt();
                assert!(
                    horizontal_mag > 0.5,
                    "Normal should point outward: {:?}",
                    vertex.normal
                );
            }
        }
    }

    #[test]
    fn test_mesh_uvs_are_valid() {
        let tree = create_simple_tree();
        let mesh = build_mesh(&tree);

        for vertex in &mesh.vertices {
            // U should be in [0, 1)
            assert!(
                vertex.uv.x >= 0.0 && vertex.uv.x <= 1.0,
                "Invalid U coordinate: {}",
                vertex.uv.x
            );
            // V should be non-negative (increases along stem)
            assert!(vertex.uv.y >= 0.0, "Invalid V coordinate: {}", vertex.uv.y);
        }
    }

    #[test]
    fn test_pivot_painter_encoding() {
        let tree = create_tree_with_branch();
        let config = MeshConfig {
            pivot_painter: true,
            ..Default::default()
        };
        let mesh = build_mesh_with_config(&tree, config);

        // Check that pivot painter data is encoded
        for vertex in &mesh.vertices {
            // UV2.x (depth) should be in [0, 1]
            assert!(
                vertex.uv2.x >= 0.0 && vertex.uv2.x <= 1.0,
                "Invalid depth: {}",
                vertex.uv2.x
            );
            // UV2.y (phase) should be in [0, 1)
            assert!(
                vertex.uv2.y >= 0.0 && vertex.uv2.y < 1.0,
                "Invalid phase: {}",
                vertex.uv2.y
            );
            // Color.w (stiffness) should be in [0, 1]
            assert!(
                vertex.color.w >= 0.0 && vertex.color.w <= 1.0,
                "Invalid stiffness: {}",
                vertex.color.w
            );
        }
    }

    #[test]
    fn test_pivot_painter_disabled() {
        let tree = create_simple_tree();
        let config = MeshConfig {
            pivot_painter: false,
            ..Default::default()
        };
        let mesh = build_mesh_with_config(&tree, config);

        // Check that pivot painter data is zero
        for vertex in &mesh.vertices {
            assert_eq!(vertex.uv2, Vec2::ZERO);
            assert_eq!(vertex.color, Vec4::ZERO);
        }
    }

    #[test]
    fn test_branch_mesh_generation() {
        let tree = create_tree_with_branch();
        let mesh = build_mesh(&tree);

        // Should generate geometry for both trunk and branch
        assert!(mesh.vertex_count() > 0);
        assert!(mesh.triangle_count() > 0);

        // The trunk has a child, so it should NOT have a cap
        // The branch has no children, so it SHOULD have a cap
    }

    #[test]
    fn test_empty_tree() {
        let tree = Tree::new("Empty".to_string(), 0);
        let mesh = build_mesh(&tree);

        assert!(mesh.is_empty());
        assert!(mesh.submeshes.is_empty());
    }

    #[test]
    fn test_empty_stem() {
        let mut tree = Tree::new("EmptyStem".to_string(), 0);
        let stem = Stem::new(0, 0); // No segments
        tree.add_stem(stem);

        let mesh = build_mesh(&tree);

        // Should not crash, should produce no geometry
        assert!(mesh.is_empty());
    }

    #[test]
    fn test_ring_resolution_per_level() {
        let config = MeshConfig {
            ring_resolution: [12, 8, 5, 3],
            ..Default::default()
        };
        let tree = Tree::new("Test".to_string(), 0);
        let builder = MeshBuilder::new(&tree, config.clone());

        assert_eq!(builder.ring_resolution_for_level(0), 12);
        assert_eq!(builder.ring_resolution_for_level(1), 8);
        assert_eq!(builder.ring_resolution_for_level(2), 5);
        assert_eq!(builder.ring_resolution_for_level(3), 3);
        // Out of bounds should return default minimum
        assert_eq!(builder.ring_resolution_for_level(10), 4);
    }

    #[test]
    fn test_texture_v_scale() {
        let tree = create_simple_tree();

        let config1 = MeshConfig {
            texture_v_scale: 1.0,
            ..Default::default()
        };
        let mesh1 = build_mesh_with_config(&tree, config1);

        let config2 = MeshConfig {
            texture_v_scale: 2.0,
            ..Default::default()
        };
        let mesh2 = build_mesh_with_config(&tree, config2);

        // Find max V coordinate in each mesh
        let max_v1 = mesh1.vertices.iter().map(|v| v.uv.y).fold(0.0_f32, f32::max);
        let max_v2 = mesh2.vertices.iter().map(|v| v.uv.y).fold(0.0_f32, f32::max);

        // V coordinates should scale proportionally
        assert!(
            (max_v2 / max_v1 - 2.0).abs() < 0.01,
            "V scale not applied correctly: {} vs {}",
            max_v1,
            max_v2
        );
    }

    fn create_tree_with_multiple_levels() -> Tree {
        let mut tree = Tree::new("Test".to_string(), 42);

        // Create trunk (level 0)
        let mut trunk = Stem::new(0, 0);
        trunk.segments.push(Segment {
            start: Vec3::ZERO,
            end: Vec3::new(0.0, 4.0, 0.0),
            start_radius: 0.5,
            end_radius: 0.3,
            direction: Vec3::Y,
        });
        trunk.child_ids.push(1);
        tree.add_stem(trunk);

        // Create level 1 branch
        let mut branch1 = Stem::new(1, 1);
        branch1.parent_id = Some(0);
        branch1.parent_offset = 0.5;
        branch1.segments.push(Segment {
            start: Vec3::new(0.0, 2.0, 0.0),
            end: Vec3::new(1.5, 2.5, 0.0),
            start_radius: 0.15,
            end_radius: 0.1,
            direction: Vec3::new(1.0, 0.3, 0.0).normalize(),
        });
        branch1.child_ids.push(2);
        tree.add_stem(branch1);

        // Create level 2 branch
        let mut branch2 = Stem::new(2, 2);
        branch2.parent_id = Some(1);
        branch2.parent_offset = 0.7;
        branch2.segments.push(Segment {
            start: Vec3::new(1.0, 2.35, 0.0),
            end: Vec3::new(1.5, 2.8, 0.3),
            start_radius: 0.05,
            end_radius: 0.03,
            direction: Vec3::new(0.5, 0.5, 0.3).normalize(),
        });
        tree.add_stem(branch2);

        tree.update_bounds();
        tree
    }

    #[test]
    fn test_build_branches_to_level() {
        let tree = create_tree_with_multiple_levels();

        // Build with all levels
        let config = MeshConfig::default();
        let mesh_all = MeshBuilder::new(&tree, config.clone()).build_branches();

        // Build with trunk only (level 0)
        let mesh_trunk = MeshBuilder::new(&tree, config.clone()).build_branches_to_level(0);

        // Build with trunk + level 1
        let mesh_level1 = MeshBuilder::new(&tree, config.clone()).build_branches_to_level(1);

        // Trunk only should have fewer vertices than trunk + level 1
        assert!(
            mesh_trunk.vertex_count() < mesh_level1.vertex_count(),
            "Trunk only ({}) should have fewer vertices than trunk + level 1 ({})",
            mesh_trunk.vertex_count(),
            mesh_level1.vertex_count()
        );

        // Level 1 should have fewer vertices than all levels
        assert!(
            mesh_level1.vertex_count() < mesh_all.vertex_count(),
            "Trunk + level 1 ({}) should have fewer vertices than all ({})",
            mesh_level1.vertex_count(),
            mesh_all.vertex_count()
        );
    }

    #[test]
    fn test_build_branches_to_level_high_limit() {
        let tree = create_tree_with_multiple_levels();
        let config = MeshConfig::default();

        // Build with max_level higher than any branch level in tree
        let mesh_high = MeshBuilder::new(&tree, config.clone()).build_branches_to_level(10);

        // Should be the same as building all branches
        let mesh_all = MeshBuilder::new(&tree, config).build_branches();

        assert_eq!(
            mesh_high.vertex_count(),
            mesh_all.vertex_count(),
            "High limit should include all branches"
        );
    }
}
