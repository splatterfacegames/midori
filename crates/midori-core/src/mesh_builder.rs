//! Mesh builder for generating branch geometry.

use crate::{
    constants::{GOLDEN_RATIO, MAX_BRANCH_LEVELS, TAU},
    math::create_basis,
    mesh::{MaterialType, Mesh, Submesh, Vertex},
    tree::{Stem, Tree},
};
use glam::{Vec2, Vec3, Vec4};
use std::collections::HashMap;

const RING_T_EPSILON: f32 = 0.0001;
const NORMAL_SMOOTH_POSITION_SCALE: f32 = 100_000.0;

/// Configuration for mesh generation
#[derive(Debug, Clone)]
pub struct MeshConfig {
    /// Ring resolution per branch level [trunk, level1, level2, level3]
    pub ring_resolution: [u32; 4],
    /// Texture V scale (repeats per meter)
    pub texture_v_scale: f32,
    /// Enable Pivot Painter data encoding
    pub pivot_painter: bool,
    /// Branch collar swelling factor (1.0 = no swelling, 1.5 = 50% larger at joints)
    pub branch_collar_swell: f32,
    /// How far the swelling extends (0.0-1.0, as fraction of branch length)
    pub collar_falloff: f32,
    /// Trunk base flare multiplier at ground level (1.0 = no flare)
    pub trunk_base_flare: f32,
    /// How far trunk flare extends (0.0-1.0, as fraction of trunk length)
    pub trunk_base_flare_height: f32,
    /// Branch base swell multiplier at parent attachment (1.0 = no swell)
    pub branch_base_swell: f32,
    /// How far branch base swell extends (0.0-1.0, as fraction of branch length)
    pub branch_base_falloff: f32,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            // Higher resolution for production-quality trees
            // Trunk needs 24+ segments for smooth appearance at close range
            ring_resolution: [24, 16, 10, 6],
            texture_v_scale: 1.0,
            pivot_painter: true,
            branch_collar_swell: 1.35,
            collar_falloff: 0.15,
            trunk_base_flare: 1.18,
            trunk_base_flare_height: 0.12,
            branch_base_swell: 1.2,
            branch_base_falloff: 0.14,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RingFrame {
    tangent: Vec3,
    bitangent: Vec3,
}

#[derive(Debug, Clone, Copy)]
struct RingSample {
    position: Vec3,
    radius: f32,
    direction: Vec3,
    v_coord: f32,
}

impl RingFrame {
    fn from_direction(direction: Vec3) -> Self {
        let direction = normalized_or_up(direction);
        let (tangent, bitangent) = create_basis(direction);
        Self {
            tangent,
            bitangent: bitangent.normalize_or_zero(),
        }
    }

    fn transported(self, direction: Vec3) -> Self {
        let direction = normalized_or_up(direction);
        let projected = self.tangent - direction * self.tangent.dot(direction);
        let tangent = if projected.length_squared() > 0.0001 {
            projected.normalize()
        } else {
            create_basis(direction).0
        };
        let bitangent = direction.cross(tangent).normalize_or_zero();

        Self { tangent, bitangent }
    }
}

/// Build mesh from generated tree
pub struct MeshBuilder<'a> {
    tree: &'a Tree,
    config: MeshConfig,
    mesh: Mesh,
    max_branch_level: Option<u32>,
    cap_center_indices: Vec<u32>,
}

impl<'a> MeshBuilder<'a> {
    /// Create a new mesh builder for the given tree
    pub fn new(tree: &'a Tree, config: MeshConfig) -> Self {
        Self {
            tree,
            config,
            mesh: Mesh::new(),
            max_branch_level: None,
            cap_center_indices: Vec::new(),
        }
    }

    /// Build complete branch mesh
    pub fn build_branches(mut self) -> Mesh {
        for stem in &self.tree.stems {
            self.build_stem(stem);
        }

        self.smooth_coincident_normals();

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
        self.max_branch_level = Some(max_level);

        for stem in &self.tree.stems {
            if stem.level as u32 <= max_level {
                self.build_stem(stem);
            }
        }

        self.smooth_coincident_normals();

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

    /// Build stems at or below min_level depth — the branches a LOD cut
    /// away. The impostor baker rasterizes these into the crown atlas so
    /// the baked card still shows the twiggy interior.
    ///
    /// # Arguments
    ///
    /// * `min_level` - Minimum branch level to include
    pub fn build_stems_from_level(mut self, min_level: u32) -> Mesh {
        for stem in &self.tree.stems {
            if stem.level as u32 >= min_level {
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
        let avg_radius = stem
            .segments
            .iter()
            .map(|s| (s.start_radius + s.end_radius) / 2.0)
            .sum::<f32>()
            / stem.segments.len() as f32;

        // Use adaptive resolution based on branch thickness
        let ring_res = self.adaptive_ring_resolution(stem.level, avg_radius);
        if ring_res < 3 {
            return; // Too few vertices to make a cylinder
        }

        // Calculate total stem length for position calculations
        let total_length: f32 = stem
            .segments
            .iter()
            .map(|s| (s.end - s.start).length())
            .sum();

        let child_offsets = self.included_child_offsets(stem);
        let samples = self.ring_samples(stem, &child_offsets, total_length);
        if samples.len() < 2 {
            return;
        }

        // Generate ring vertices for each segment joint
        let mut rings: Vec<Vec<u32>> = Vec::new();
        let mut frame = RingFrame::from_direction(samples[0].direction);

        for (sample_idx, sample) in samples.iter().enumerate() {
            if sample_idx > 0 {
                frame = frame.transported(sample.direction);
            }
            let ring = self.create_ring(
                sample.position,
                frame,
                sample.radius,
                ring_res,
                sample.v_coord,
                stem,
            );
            rings.push(ring);
        }

        // Connect adjacent rings with faces
        for i in 0..rings.len() - 1 {
            self.connect_rings(&rings[i], &rings[i + 1]);
        }

        // Cap the end unless an included child actually continues from the tip.
        if !has_terminal_child(&child_offsets) && !rings.is_empty() {
            let tip_normal = normalized_or_up(stem.segments.last().unwrap().direction);
            self.cap_ring(rings.last().unwrap(), tip_normal, stem);
        }
    }

    fn included_child_offsets(&self, stem: &Stem) -> Vec<f32> {
        let mut offsets: Vec<f32> = stem
            .child_ids
            .iter()
            .filter_map(|&child_id| self.tree.stems.iter().find(|s| s.id == child_id))
            .filter(|child| self.includes_stem(child))
            .map(|child| child.parent_offset.clamp(0.0, 1.0))
            .collect();

        offsets.sort_by(|a, b| a.total_cmp(b));
        offsets.dedup_by(|a, b| (*a - *b).abs() < RING_T_EPSILON);
        offsets
    }

    fn includes_stem(&self, stem: &Stem) -> bool {
        self.max_branch_level
            .map(|max_level| stem.level as u32 <= max_level)
            .unwrap_or(true)
    }

    fn ring_samples(
        &self,
        stem: &Stem,
        child_offsets: &[f32],
        total_length: f32,
    ) -> Vec<RingSample> {
        let mut t_values = Vec::with_capacity(stem.segments.len() + child_offsets.len() + 1);
        t_values.push(0.0);

        let mut accumulated_length = 0.0;
        for seg in &stem.segments {
            accumulated_length += (seg.end - seg.start).length();
            let t = if total_length > f32::EPSILON {
                accumulated_length / total_length
            } else {
                1.0
            };
            t_values.push(t.clamp(0.0, 1.0));
        }

        t_values.extend(child_offsets.iter().copied());
        t_values.sort_by(|a, b| a.total_cmp(b));
        t_values.dedup_by(|a, b| (*a - *b).abs() < RING_T_EPSILON);

        t_values
            .into_iter()
            .map(|t| RingSample {
                position: stem.point_at(t),
                radius: stem.radius_at(t)
                    * self.calculate_radius_scale(stem.level, t, child_offsets),
                direction: self.direction_at_ring(stem, t, total_length),
                v_coord: t * total_length * self.config.texture_v_scale,
            })
            .collect()
    }

    fn direction_at_ring(&self, stem: &Stem, t: f32, total_length: f32) -> Vec3 {
        if stem.segments.is_empty() {
            return Vec3::Y;
        }

        if t <= RING_T_EPSILON {
            return normalized_or_up(stem.segments[0].direction);
        }

        if t >= 1.0 - RING_T_EPSILON || total_length <= f32::EPSILON {
            return normalized_or_up(stem.segments.last().unwrap().direction);
        }

        let target_length = t.clamp(0.0, 1.0) * total_length;
        let mut accumulated = 0.0;

        for (seg_idx, seg) in stem.segments.iter().enumerate() {
            let seg_length = (seg.end - seg.start).length();
            let segment_end = accumulated + seg_length;

            if (target_length - segment_end).abs() <= total_length * RING_T_EPSILON
                && seg_idx + 1 < stem.segments.len()
            {
                let next = &stem.segments[seg_idx + 1];
                return normalized_or(seg.direction + next.direction, seg.direction);
            }

            if target_length <= segment_end {
                return normalized_or_up(seg.direction);
            }

            accumulated = segment_end;
        }

        normalized_or_up(stem.segments.last().unwrap().direction)
    }

    fn calculate_radius_scale(&self, level: u8, t: f32, child_offsets: &[f32]) -> f32 {
        self.calculate_collar_swell(t, child_offsets) * self.calculate_base_swell(level, t)
    }

    /// Calculate branch collar swelling at a position along the stem
    fn calculate_collar_swell(&self, t: f32, child_offsets: &[f32]) -> f32 {
        if child_offsets.is_empty() || self.config.branch_collar_swell <= 1.0 {
            return 1.0;
        }

        let falloff = self.config.collar_falloff;
        let max_swell = self.config.branch_collar_swell;

        // Find the maximum swelling contribution from all child attachments
        let mut swell = 1.0f32;
        for &child_t in child_offsets {
            let dist = (t - child_t).abs();
            if dist < falloff {
                // Smooth falloff using cosine interpolation
                let factor = (1.0 - dist / falloff).powi(2);
                let local_swell = 1.0 + (max_swell - 1.0) * factor;
                swell = swell.max(local_swell);
            }
        }

        swell
    }

    fn calculate_base_swell(&self, level: u8, t: f32) -> f32 {
        let (max_swell, falloff) = if level == 0 {
            (
                self.config.trunk_base_flare,
                self.config.trunk_base_flare_height,
            )
        } else {
            (
                self.config.branch_base_swell,
                self.config.branch_base_falloff,
            )
        };

        if max_swell <= 1.0 || falloff <= 0.0 || t >= falloff {
            return 1.0;
        }

        let x = (1.0 - t / falloff).clamp(0.0, 1.0);
        let smooth = x * x * (3.0 - 2.0 * x);
        1.0 + (max_swell - 1.0) * smooth
    }

    fn create_ring(
        &mut self,
        center: Vec3,
        frame: RingFrame,
        radius: f32,
        resolution: u32,
        v_coord: f32,
        stem: &Stem,
    ) -> Vec<u32> {
        // +1 for seam vertex at U=1.0 to avoid texture seam artifacts
        let mut ring_indices = Vec::with_capacity(resolution as usize + 1);

        // Generate resolution+1 vertices (0 to resolution inclusive)
        // Last vertex duplicates position of first but has U=1.0 for proper UV wrap
        for i in 0..=resolution {
            let angle = if i == resolution {
                0.0
            } else {
                (i as f32 / resolution as f32) * TAU
            };
            let offset = frame.tangent * angle.cos() + frame.bitangent * angle.sin();

            let position = center + offset * radius;
            let normal = offset.normalize_or_zero(); // Points outward
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

    fn cap_ring(&mut self, ring: &[u32], normal: Vec3, stem: &Stem) {
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

        // Add center vertex
        let uv = Vec2::new(0.5, 0.5);
        let mut vertex = Vertex::new(center, normal, uv);
        if self.config.pivot_painter {
            self.encode_pivot_painter(&mut vertex, stem);
        }
        let center_idx = self.mesh.vertices.len() as u32;
        self.mesh.vertices.push(vertex);
        self.cap_center_indices.push(center_idx);

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

    fn smooth_coincident_normals(&mut self) {
        if self.mesh.vertices.is_empty() {
            return;
        }

        let mut is_cap_center = vec![false; self.mesh.vertices.len()];
        for &index in &self.cap_center_indices {
            if let Some(is_cap) = is_cap_center.get_mut(index as usize) {
                *is_cap = true;
            }
        }

        let mut groups: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();
        for (index, vertex) in self.mesh.vertices.iter().enumerate() {
            if is_cap_center[index] {
                continue;
            }
            groups
                .entry(position_key(vertex.position))
                .or_default()
                .push(index);
        }

        for indices in groups.values() {
            if indices.len() < 2 {
                continue;
            }

            let normal_sum = indices.iter().fold(Vec3::ZERO, |sum, &index| {
                sum + self.mesh.vertices[index].normal
            });
            let smoothed = normal_sum.normalize_or_zero();
            if smoothed.length_squared() <= f32::EPSILON {
                continue;
            }

            for &index in indices {
                self.mesh.vertices[index].normal = smoothed;
            }
        }
    }
}

fn normalized_or_up(value: Vec3) -> Vec3 {
    normalized_or(value, Vec3::Y)
}

fn normalized_or(value: Vec3, fallback: Vec3) -> Vec3 {
    let normalized = value.normalize_or_zero();
    if normalized.length_squared() > 0.0 {
        normalized
    } else {
        fallback.normalize_or_zero()
    }
}

fn has_terminal_child(child_offsets: &[f32]) -> bool {
    child_offsets
        .iter()
        .any(|&offset| offset >= 1.0 - RING_T_EPSILON)
}

fn position_key(position: Vec3) -> (i32, i32, i32) {
    (
        (position.x * NORMAL_SMOOTH_POSITION_SCALE).round() as i32,
        (position.y * NORMAL_SMOOTH_POSITION_SCALE).round() as i32,
        (position.z * NORMAL_SMOOTH_POSITION_SCALE).round() as i32,
    )
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
        assert!(config.trunk_base_flare > 1.0);
        assert!(config.branch_base_swell > 1.0);
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
    fn test_trunk_base_flare_expands_base_ring() {
        let tree = create_simple_tree();
        let config = MeshConfig {
            ring_resolution: [8, 6, 4, 3],
            trunk_base_flare: 1.5,
            trunk_base_flare_height: 0.25,
            branch_base_swell: 1.0,
            branch_collar_swell: 1.0,
            ..Default::default()
        };
        let mesh = build_mesh_with_config(&tree, config);

        let ring_len = 9;
        let base_radius = max_ring_distance(&mesh, 0..ring_len, Vec3::ZERO);
        let next_radius =
            max_ring_distance(&mesh, ring_len..ring_len * 2, Vec3::new(0.0, 2.0, 0.0));

        assert!(base_radius > 0.70, "base ring should be visibly flared");
        assert!(
            next_radius < 0.45,
            "flare should fall off by the next trunk ring"
        );
    }

    #[test]
    fn test_branch_base_swell_expands_child_base_ring() {
        let tree = create_tree_with_branch();
        let config = MeshConfig {
            ring_resolution: [8, 8, 4, 3],
            trunk_base_flare: 1.0,
            branch_base_swell: 1.6,
            branch_base_falloff: 0.4,
            branch_collar_swell: 1.0,
            ..Default::default()
        };
        let mesh = build_mesh_with_config(&tree, config);

        let branch_start = Vec3::new(0.0, 1.5, 0.0);
        let branch_base_radius = max_depth_ring_distance(&mesh, branch_start, 0.25);

        assert!(
            branch_base_radius > 0.22,
            "branch base should be larger than the raw 0.15m radius"
        );
    }

    #[test]
    fn test_child_attachment_ring_adds_parent_collar_geometry() {
        let tree = create_tree_with_branch();
        let config = MeshConfig {
            ring_resolution: [8, 8, 4, 3],
            trunk_base_flare: 1.0,
            branch_base_swell: 1.0,
            branch_collar_swell: 1.5,
            collar_falloff: 0.25,
            ..Default::default()
        };
        let mesh = build_mesh_with_config(&tree, config);

        let ring_len = 9;
        let attachment_center = Vec3::new(0.0, 1.5, 0.0);
        let collar_radius = max_ring_distance(&mesh, ring_len..ring_len * 2, attachment_center);

        assert!(
            collar_radius > 0.55,
            "parent collar ring should be inserted and swollen at the child offset"
        );
    }

    #[test]
    fn test_parent_tip_is_capped_when_children_are_side_branches() {
        let tree = create_tree_with_branch();
        let config = MeshConfig {
            ring_resolution: [8, 8, 4, 3],
            trunk_base_flare: 1.0,
            branch_base_swell: 1.0,
            branch_collar_swell: 1.0,
            ..Default::default()
        };
        let mesh = build_mesh_with_config(&tree, config);

        let trunk_tip = Vec3::new(0.0, 3.0, 0.0);
        let cap_center = mesh
            .vertices
            .iter()
            .find(|vertex| {
                (vertex.position - trunk_tip).length() < 0.001
                    && vertex.normal.dot(Vec3::Y) > 0.99
                    && (vertex.uv2.x - 0.0).abs() < f32::EPSILON
            })
            .expect("side-branched parent tip should have a cap center");

        assert!((cap_center.color.w - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_lod_filtering_skips_excluded_child_attachment_ring() {
        let tree = create_tree_with_branch();
        let config = MeshConfig {
            ring_resolution: [8, 8, 4, 3],
            trunk_base_flare: 1.0,
            branch_base_swell: 1.0,
            branch_collar_swell: 1.5,
            ..Default::default()
        };

        let mesh = MeshBuilder::new(&tree, config).build_branches_to_level(0);

        // Trunk-only LOD keeps the start/end rings and a tip cap, but does not
        // add a hidden attachment ring for the excluded level-1 branch.
        assert_eq!(mesh.vertex_count(), 19);
    }

    #[test]
    fn test_seam_vertices_are_exact_duplicates_with_shared_normals() {
        let tree = create_simple_tree();
        let config = MeshConfig {
            ring_resolution: [8, 6, 4, 3],
            trunk_base_flare: 1.0,
            branch_base_swell: 1.0,
            branch_collar_swell: 1.0,
            ..Default::default()
        };
        let mesh = build_mesh_with_config(&tree, config);

        let ring_len = 9;
        for ring_index in 0..3 {
            let first = ring_index * ring_len;
            let seam = first + ring_len - 1;

            assert_eq!(mesh.vertices[first].position, mesh.vertices[seam].position);
            assert_eq!(mesh.vertices[first].normal, mesh.vertices[seam].normal);
        }
    }

    #[test]
    fn test_coincident_side_normals_are_averaged() {
        let tree = Tree::new("Normals".to_string(), 0);
        let mut builder = MeshBuilder::new(&tree, MeshConfig::default());
        builder
            .mesh
            .vertices
            .push(Vertex::new(Vec3::new(1.0, 2.0, 3.0), Vec3::X, Vec2::ZERO));
        builder.mesh.vertices.push(Vertex::new(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::Y,
            Vec2::new(1.0, 0.0),
        ));

        builder.smooth_coincident_normals();

        let expected = (Vec3::X + Vec3::Y).normalize();
        assert!((builder.mesh.vertices[0].normal - expected).length() < 0.001);
        assert!((builder.mesh.vertices[1].normal - expected).length() < 0.001);
    }

    #[test]
    fn test_cap_center_normals_are_not_smoothed_with_side_vertices() {
        let tree = Tree::new("Normals".to_string(), 0);
        let mut builder = MeshBuilder::new(&tree, MeshConfig::default());
        builder
            .mesh
            .vertices
            .push(Vertex::new(Vec3::new(1.0, 2.0, 3.0), Vec3::X, Vec2::ZERO));
        builder.mesh.vertices.push(Vertex::new(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::Y,
            Vec2::new(0.5, 0.5),
        ));
        builder.cap_center_indices.push(1);

        builder.smooth_coincident_normals();

        assert_eq!(builder.mesh.vertices[0].normal, Vec3::X);
        assert_eq!(builder.mesh.vertices[1].normal, Vec3::Y);
    }

    #[test]
    fn test_mesh_normals_point_outward() {
        let tree = create_simple_tree();
        let mesh = build_mesh(&tree);

        // Check that normals point outward from the trunk center axis
        for vertex in &mesh.vertices {
            if vertex.normal.length() > 0.5 {
                if vertex.normal.dot(Vec3::Y).abs() > 0.95 {
                    continue; // terminal cap center normal follows the stem axis
                }
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
    fn test_terminal_cap_center_uses_axial_normal_and_pivot_data() {
        let tree = create_simple_tree();
        let mesh = build_mesh(&tree);
        let cap_center = mesh.vertices.last().expect("terminal cap center vertex");

        assert!(
            cap_center.normal.dot(Vec3::Y) > 0.99,
            "cap center normal should follow the terminal stem direction"
        );
        assert!((cap_center.uv2.x - 0.0).abs() < f32::EPSILON);
        assert!((cap_center.color.w - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_mesh_attributes_are_finite_and_normalized() {
        let tree = create_tree_with_multiple_levels();
        let mesh = build_mesh(&tree);

        for (index, vertex) in mesh.vertices.iter().enumerate() {
            assert!(
                vertex.position.is_finite(),
                "position {} is not finite",
                index
            );
            assert!(vertex.normal.is_finite(), "normal {} is not finite", index);
            assert!(vertex.uv.is_finite(), "uv {} is not finite", index);
            assert!(vertex.uv2.is_finite(), "uv2 {} is not finite", index);
            assert!(vertex.color.is_finite(), "color {} is not finite", index);
            assert!(
                (vertex.normal.length() - 1.0).abs() < 0.001,
                "normal {} should be unit length: {:?}",
                index,
                vertex.normal
            );
            assert!(
                vertex.uv.x >= 0.0 && vertex.uv.x <= 1.0,
                "u coordinate {} out of range: {}",
                index,
                vertex.uv.x
            );
            assert!(
                vertex.color.w >= 0.0 && vertex.color.w <= 1.0,
                "stiffness {} out of range: {}",
                index,
                vertex.color.w
            );
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
        let max_v1 = mesh1
            .vertices
            .iter()
            .map(|v| v.uv.y)
            .fold(0.0_f32, f32::max);
        let max_v2 = mesh2
            .vertices
            .iter()
            .map(|v| v.uv.y)
            .fold(0.0_f32, f32::max);

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

    fn max_ring_distance(mesh: &Mesh, range: std::ops::Range<usize>, center: Vec3) -> f32 {
        range
            .map(|index| (mesh.vertices[index].position - center).length())
            .fold(0.0_f32, f32::max)
    }

    fn max_depth_ring_distance(mesh: &Mesh, center: Vec3, depth: f32) -> f32 {
        mesh.vertices
            .iter()
            .filter(|vertex| (vertex.uv2.x - depth).abs() < 0.001)
            .filter(|vertex| (vertex.position - center).length() < 1.0)
            .map(|vertex| (vertex.position - center).length())
            .fold(0.0_f32, f32::max)
    }
}
