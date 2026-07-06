//! Mesh data structures for tree geometry.

use glam::{Vec2, Vec3, Vec4};

/// Interleaved vertex layout (56 bytes per vertex)
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Vertex {
    /// Position in 3D space (12 bytes)
    pub position: Vec3,
    /// Surface normal vector (12 bytes)
    pub normal: Vec3,
    /// Standard texture coordinates (8 bytes)
    pub uv: Vec2,
    /// Pivot painter data: x = depth, y = phase (8 bytes)
    pub uv2: Vec2,
    /// Pivot painter data: xyz = pivot position, w = stiffness (16 bytes)
    pub color: Vec4,
}

/// Complete mesh data
#[derive(Debug, Clone, Default)]
pub struct Mesh {
    /// All vertices in the mesh
    pub vertices: Vec<Vertex>,
    /// Triangle indices (3 per triangle)
    pub indices: Vec<u32>,
    /// Submesh ranges with material assignments
    pub submeshes: Vec<Submesh>,
}

/// Submesh defines a range of indices with material type
#[derive(Debug, Clone, Copy)]
pub struct Submesh {
    /// Starting index in the indices array
    pub index_start: u32,
    /// Number of indices in this submesh
    pub index_count: u32,
    /// Material type for this submesh
    pub material: MaterialType,
}

/// Material types for tree rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialType {
    /// Bark material for trunk and branches
    Bark,
    /// Leaf material for foliage
    Leaves,
}

impl Mesh {
    /// Create a new empty mesh
    pub fn new() -> Self {
        Default::default()
    }

    /// Get the number of vertices in the mesh
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Get the number of triangles in the mesh
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Merge another mesh into this one with the specified material
    pub fn merge(&mut self, other: &Mesh, material: MaterialType) {
        let vertex_offset = self.vertices.len() as u32;
        let index_start = self.indices.len() as u32;

        self.vertices.extend_from_slice(&other.vertices);

        for &idx in &other.indices {
            self.indices.push(idx + vertex_offset);
        }

        self.submeshes.push(Submesh {
            index_start,
            index_count: other.indices.len() as u32,
            material,
        });
    }

    /// Recalculate smooth vertex normals from face geometry
    pub fn recalculate_normals(&mut self) {
        // Zero out normals
        for v in &mut self.vertices {
            v.normal = Vec3::ZERO;
        }

        // Accumulate face normals (area-weighted)
        for tri in self.indices.chunks(3) {
            if tri.len() < 3 {
                continue;
            }
            let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);

            let v0 = self.vertices[i0].position;
            let v1 = self.vertices[i1].position;
            let v2 = self.vertices[i2].position;

            // Cross product gives area-weighted normal
            let face_normal = (v1 - v0).cross(v2 - v0);

            self.vertices[i0].normal += face_normal;
            self.vertices[i1].normal += face_normal;
            self.vertices[i2].normal += face_normal;
        }

        // Normalize all vertex normals
        for v in &mut self.vertices {
            v.normal = v.normal.normalize_or_zero();
        }
    }

    /// Check if the mesh is empty (no vertices or indices)
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty() || self.indices.is_empty()
    }

    /// Clear all mesh data
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.submeshes.clear();
    }
}

impl Vertex {
    /// Create a new vertex with basic attributes
    pub fn new(position: Vec3, normal: Vec3, uv: Vec2) -> Self {
        Self {
            position,
            normal,
            uv,
            uv2: Vec2::ZERO,
            color: Vec4::ZERO,
        }
    }

    /// Create a vertex with all attributes
    pub fn with_pivot_painter(
        position: Vec3,
        normal: Vec3,
        uv: Vec2,
        uv2: Vec2,
        color: Vec4,
    ) -> Self {
        Self {
            position,
            normal,
            uv,
            uv2,
            color,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vertex_new() {
        let v = Vertex::new(Vec3::new(1.0, 2.0, 3.0), Vec3::Y, Vec2::new(0.5, 0.5));
        assert_eq!(v.position, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(v.normal, Vec3::Y);
        assert_eq!(v.uv, Vec2::new(0.5, 0.5));
        assert_eq!(v.uv2, Vec2::ZERO);
        assert_eq!(v.color, Vec4::ZERO);
    }

    #[test]
    fn test_vertex_with_pivot_painter() {
        let v = Vertex::with_pivot_painter(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::Y,
            Vec2::new(0.5, 0.5),
            Vec2::new(0.1, 0.2),
            Vec4::new(0.3, 0.4, 0.5, 0.6),
        );
        assert_eq!(v.uv2, Vec2::new(0.1, 0.2));
        assert_eq!(v.color, Vec4::new(0.3, 0.4, 0.5, 0.6));
    }

    #[test]
    fn test_mesh_new() {
        let mesh = Mesh::new();
        assert!(mesh.vertices.is_empty());
        assert!(mesh.indices.is_empty());
        assert!(mesh.submeshes.is_empty());
        assert!(mesh.is_empty());
    }

    #[test]
    fn test_mesh_counts() {
        let mut mesh = Mesh::new();
        mesh.vertices.push(Vertex::default());
        mesh.vertices.push(Vertex::default());
        mesh.vertices.push(Vertex::default());
        mesh.indices.extend_from_slice(&[0, 1, 2]);

        assert_eq!(mesh.vertex_count(), 3);
        assert_eq!(mesh.triangle_count(), 1);
        assert!(!mesh.is_empty());
    }

    #[test]
    fn test_mesh_merge() {
        let mut mesh1 = Mesh::new();
        mesh1
            .vertices
            .push(Vertex::new(Vec3::ZERO, Vec3::Y, Vec2::ZERO));
        mesh1
            .vertices
            .push(Vertex::new(Vec3::X, Vec3::Y, Vec2::ZERO));
        mesh1
            .vertices
            .push(Vertex::new(Vec3::Y, Vec3::Y, Vec2::ZERO));
        mesh1.indices.extend_from_slice(&[0, 1, 2]);

        let mut mesh2 = Mesh::new();
        mesh2
            .vertices
            .push(Vertex::new(Vec3::Z, Vec3::Y, Vec2::ZERO));
        mesh2
            .vertices
            .push(Vertex::new(Vec3::ONE, Vec3::Y, Vec2::ZERO));
        mesh2
            .vertices
            .push(Vertex::new(Vec3::new(1.0, 1.0, 0.0), Vec3::Y, Vec2::ZERO));
        mesh2.indices.extend_from_slice(&[0, 1, 2]);

        mesh1.merge(&mesh2, MaterialType::Bark);

        assert_eq!(mesh1.vertex_count(), 6);
        assert_eq!(mesh1.triangle_count(), 2);
        assert_eq!(mesh1.submeshes.len(), 1);

        // Check that indices were offset correctly
        assert_eq!(mesh1.indices[3], 3);
        assert_eq!(mesh1.indices[4], 4);
        assert_eq!(mesh1.indices[5], 5);
    }

    #[test]
    fn test_mesh_recalculate_normals() {
        let mut mesh = Mesh::new();

        // Create a simple triangle in the XY plane (facing +Z)
        mesh.vertices.push(Vertex::new(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::ZERO,
            Vec2::ZERO,
        ));
        mesh.vertices.push(Vertex::new(
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::ZERO,
            Vec2::ZERO,
        ));
        mesh.vertices.push(Vertex::new(
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::ZERO,
            Vec2::ZERO,
        ));
        mesh.indices.extend_from_slice(&[0, 1, 2]);

        mesh.recalculate_normals();

        // All vertices should have normals pointing in +Z direction
        for v in &mesh.vertices {
            assert!((v.normal.z - 1.0).abs() < 0.001);
            assert!(v.normal.x.abs() < 0.001);
            assert!(v.normal.y.abs() < 0.001);
        }
    }

    #[test]
    fn test_mesh_clear() {
        let mut mesh = Mesh::new();
        mesh.vertices.push(Vertex::default());
        mesh.indices.push(0);
        mesh.submeshes.push(Submesh {
            index_start: 0,
            index_count: 1,
            material: MaterialType::Bark,
        });

        mesh.clear();

        assert!(mesh.vertices.is_empty());
        assert!(mesh.indices.is_empty());
        assert!(mesh.submeshes.is_empty());
    }

    #[test]
    fn test_material_type_equality() {
        assert_eq!(MaterialType::Bark, MaterialType::Bark);
        assert_eq!(MaterialType::Leaves, MaterialType::Leaves);
        assert_ne!(MaterialType::Bark, MaterialType::Leaves);
    }
}
