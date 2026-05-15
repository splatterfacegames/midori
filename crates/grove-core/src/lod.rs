//! Level of Detail (LOD) generation system.
//!
//! This module provides LOD mesh generation for trees, creating multiple
//! quality levels for efficient rendering at different distances.

use crate::{
    leaves::{generate_leaf_mesh, LeafConfig},
    mesh::{MaterialType, Mesh},
    mesh_builder::{MeshBuilder, MeshConfig},
    species::{LeafGeometry, LodLevel, LodPreset, Species},
    tree::Tree,
};

/// Configuration for LOD generation
#[derive(Debug, Clone)]
pub struct LodGenerationConfig {
    /// LOD levels to generate
    pub levels: Vec<LodLevelConfig>,
}

/// Configuration for a single LOD level
#[derive(Debug, Clone)]
pub struct LodLevelConfig {
    /// LOD index (0 = highest quality)
    pub index: u32,
    /// Name of this LOD level
    pub name: String,
    /// Target triangle count (soft limit)
    pub target_triangles: u32,
    /// Maximum triangle count (hard limit)
    pub max_triangles: u32,
    /// Maximum branch depth to include (0-3)
    pub branch_levels: u32,
    /// Leaf geometry type for this LOD
    pub leaf_geometry: LeafGeometry,
    /// Leaf count multiplier (0.0 - 1.0)
    pub leaf_reduction: f32,
    /// Ring resolution per branch level [trunk, level1, level2, level3]
    pub ring_resolution: [u32; 4],
    /// Screen height threshold for LOD transition
    pub screen_height: f32,
    /// Use crown impostor instead of individual leaves
    pub crown_impostor: bool,
}

impl Default for LodLevelConfig {
    fn default() -> Self {
        Self {
            index: 0,
            name: "LOD0".to_string(),
            target_triangles: 8000,
            max_triangles: 10000,
            branch_levels: 4,
            leaf_geometry: LeafGeometry::CrossBillboard,
            leaf_reduction: 1.0,
            ring_resolution: [16, 12, 8, 5], // Production-quality defaults
            screen_height: 0.5,
            crown_impostor: false,
        }
    }
}

impl LodGenerationConfig {
    /// Create config from species LOD settings
    pub fn from_species(species: &Species) -> Self {
        let lod_levels = species.get_lod_levels();

        Self {
            levels: lod_levels
                .into_iter()
                .map(|level| LodLevelConfig::from_lod_level(&level))
                .collect(),
        }
    }

    /// Create config from LOD preset
    pub fn from_preset(preset: LodPreset) -> Self {
        match preset {
            LodPreset::Ultra => Self::ultra(),
            LodPreset::HighQuality => Self::high_quality(),
            LodPreset::Balanced => Self::balanced(),
            LodPreset::Mobile => Self::mobile(),
            LodPreset::Minimal => Self::minimal(),
            LodPreset::OpenWorld => Self::open_world(),
            LodPreset::HeroTree => Self::hero_tree(),
            LodPreset::Custom => Self::balanced(), // Default to balanced for custom
        }
    }

    /// Generate ultra quality LOD chain (5 levels)
    /// Production-quality for hero trees and close-up views
    pub fn ultra() -> Self {
        Self {
            levels: vec![
                LodLevelConfig {
                    index: 0,
                    name: "Ultra".to_string(),
                    target_triangles: 50000,
                    max_triangles: 75000,
                    branch_levels: 4,
                    leaf_geometry: LeafGeometry::Polygon,
                    leaf_reduction: 1.0,
                    ring_resolution: [32, 24, 16, 10], // Smooth cylinders at close range
                    screen_height: 0.5,
                    crown_impostor: false,
                },
                LodLevelConfig {
                    index: 1,
                    name: "High".to_string(),
                    target_triangles: 25000,
                    max_triangles: 35000,
                    branch_levels: 4,
                    leaf_geometry: LeafGeometry::CrossBillboard,
                    leaf_reduction: 0.8,
                    ring_resolution: [24, 16, 10, 6],
                    screen_height: 0.25,
                    crown_impostor: false,
                },
                LodLevelConfig {
                    index: 2,
                    name: "Medium".to_string(),
                    target_triangles: 10000,
                    max_triangles: 15000,
                    branch_levels: 3,
                    leaf_geometry: LeafGeometry::CrossBillboard,
                    leaf_reduction: 0.5,
                    ring_resolution: [16, 10, 6, 4],
                    screen_height: 0.1,
                    crown_impostor: false,
                },
                LodLevelConfig {
                    index: 3,
                    name: "Low".to_string(),
                    target_triangles: 3000,
                    max_triangles: 5000,
                    branch_levels: 2,
                    leaf_geometry: LeafGeometry::Billboard,
                    leaf_reduction: 0.25,
                    ring_resolution: [10, 6, 4, 3],
                    screen_height: 0.05,
                    crown_impostor: true,
                },
                LodLevelConfig {
                    index: 4,
                    name: "Impostor".to_string(),
                    target_triangles: 500,
                    max_triangles: 1000,
                    branch_levels: 1,
                    leaf_geometry: LeafGeometry::None,
                    leaf_reduction: 0.0,
                    ring_resolution: [6, 4, 3, 3],
                    screen_height: 0.02,
                    crown_impostor: true,
                },
            ],
        }
    }

    /// Generate high quality LOD chain (4 levels)
    /// Good for primary gameplay trees
    pub fn high_quality() -> Self {
        Self {
            levels: vec![
                LodLevelConfig {
                    index: 0,
                    name: "High".to_string(),
                    target_triangles: 30000,
                    max_triangles: 45000,
                    branch_levels: 4,
                    leaf_geometry: LeafGeometry::CrossBillboard,
                    leaf_reduction: 1.0,
                    ring_resolution: [24, 16, 10, 6], // Smooth at medium range
                    screen_height: 0.4,
                    crown_impostor: false,
                },
                LodLevelConfig {
                    index: 1,
                    name: "Medium".to_string(),
                    target_triangles: 12000,
                    max_triangles: 18000,
                    branch_levels: 3,
                    leaf_geometry: LeafGeometry::CrossBillboard,
                    leaf_reduction: 0.6,
                    ring_resolution: [16, 10, 6, 4],
                    screen_height: 0.15,
                    crown_impostor: false,
                },
                LodLevelConfig {
                    index: 2,
                    name: "Low".to_string(),
                    target_triangles: 4000,
                    max_triangles: 6000,
                    branch_levels: 2,
                    leaf_geometry: LeafGeometry::Billboard,
                    leaf_reduction: 0.3,
                    ring_resolution: [10, 6, 4, 3],
                    screen_height: 0.05,
                    crown_impostor: true,
                },
                LodLevelConfig {
                    index: 3,
                    name: "Impostor".to_string(),
                    target_triangles: 500,
                    max_triangles: 1000,
                    branch_levels: 1,
                    leaf_geometry: LeafGeometry::None,
                    leaf_reduction: 0.0,
                    ring_resolution: [4, 3, 3, 3],
                    screen_height: 0.02,
                    crown_impostor: true,
                },
            ],
        }
    }

    /// Generate balanced LOD chain (3 levels)
    /// Default for most game scenarios
    pub fn balanced() -> Self {
        Self {
            levels: vec![
                LodLevelConfig {
                    index: 0,
                    name: "High".to_string(),
                    target_triangles: 8000,
                    max_triangles: 10000,
                    branch_levels: 4,
                    leaf_geometry: LeafGeometry::CrossBillboard,
                    leaf_reduction: 1.0,
                    ring_resolution: [16, 12, 8, 5], // Good quality at typical game distances
                    screen_height: 0.5,
                    crown_impostor: false,
                },
                LodLevelConfig {
                    index: 1,
                    name: "Medium".to_string(),
                    target_triangles: 2500,
                    max_triangles: 3500,
                    branch_levels: 3,
                    leaf_geometry: LeafGeometry::Billboard,
                    leaf_reduction: 0.4,
                    ring_resolution: [10, 8, 5, 4],
                    screen_height: 0.2,
                    crown_impostor: false,
                },
                LodLevelConfig {
                    index: 2,
                    name: "Low".to_string(),
                    target_triangles: 500,
                    max_triangles: 800,
                    branch_levels: 2,
                    leaf_geometry: LeafGeometry::None,
                    leaf_reduction: 0.0,
                    ring_resolution: [8, 5, 4, 3],
                    screen_height: 0.05,
                    crown_impostor: true,
                },
            ],
        }
    }

    /// Generate mobile-optimized LOD chain (3 levels)
    pub fn mobile() -> Self {
        Self {
            levels: vec![
                LodLevelConfig {
                    index: 0,
                    name: "High".to_string(),
                    target_triangles: 3000,
                    max_triangles: 4000,
                    branch_levels: 3,
                    leaf_geometry: LeafGeometry::Billboard,
                    leaf_reduction: 0.6,
                    ring_resolution: [8, 6, 4, 3],
                    screen_height: 0.4,
                    crown_impostor: false,
                },
                LodLevelConfig {
                    index: 1,
                    name: "Medium".to_string(),
                    target_triangles: 1000,
                    max_triangles: 1500,
                    branch_levels: 2,
                    leaf_geometry: LeafGeometry::Billboard,
                    leaf_reduction: 0.25,
                    ring_resolution: [6, 4, 3, 3],
                    screen_height: 0.15,
                    crown_impostor: false,
                },
                LodLevelConfig {
                    index: 2,
                    name: "Low".to_string(),
                    target_triangles: 200,
                    max_triangles: 400,
                    branch_levels: 1,
                    leaf_geometry: LeafGeometry::None,
                    leaf_reduction: 0.0,
                    ring_resolution: [4, 3, 3, 3],
                    screen_height: 0.03,
                    crown_impostor: true,
                },
            ],
        }
    }

    /// Generate minimal LOD chain (2 levels)
    pub fn minimal() -> Self {
        Self {
            levels: vec![
                LodLevelConfig {
                    index: 0,
                    name: "Main".to_string(),
                    target_triangles: 4000,
                    max_triangles: 6000,
                    branch_levels: 2,
                    leaf_geometry: LeafGeometry::Billboard,
                    leaf_reduction: 0.5,
                    ring_resolution: [4, 3, 3, 3],
                    screen_height: 0.15,
                    crown_impostor: true,
                },
                LodLevelConfig {
                    index: 1,
                    name: "Low".to_string(),
                    target_triangles: 500,
                    max_triangles: 1000,
                    branch_levels: 1,
                    leaf_geometry: LeafGeometry::None,
                    leaf_reduction: 0.0,
                    ring_resolution: [3, 3, 3, 3],
                    screen_height: 0.02,
                    crown_impostor: true,
                },
            ],
        }
    }

    /// Generate open-world forest optimized LOD chain (4 levels)
    /// Designed for 10-50 trees on screen simultaneously at 60fps
    pub fn open_world() -> Self {
        Self {
            levels: vec![
                LodLevelConfig {
                    index: 0,
                    name: "Near".to_string(),
                    target_triangles: 8000,
                    max_triangles: 10000,
                    branch_levels: 4,
                    leaf_geometry: LeafGeometry::CrossBillboard,
                    leaf_reduction: 1.0,
                    ring_resolution: [20, 14, 8, 5],
                    screen_height: 0.25,
                    crown_impostor: false,
                },
                LodLevelConfig {
                    index: 1,
                    name: "Medium".to_string(),
                    target_triangles: 3000,
                    max_triangles: 4000,
                    branch_levels: 3,
                    leaf_geometry: LeafGeometry::Billboard,
                    leaf_reduction: 0.5,
                    ring_resolution: [12, 8, 5, 4],
                    screen_height: 0.08,
                    crown_impostor: false,
                },
                LodLevelConfig {
                    index: 2,
                    name: "Far".to_string(),
                    target_triangles: 800,
                    max_triangles: 1200,
                    branch_levels: 2,
                    leaf_geometry: LeafGeometry::Billboard,
                    leaf_reduction: 0.2,
                    ring_resolution: [8, 5, 4, 3],
                    screen_height: 0.03,
                    crown_impostor: false,
                },
                LodLevelConfig {
                    index: 3,
                    name: "Distant".to_string(),
                    target_triangles: 200,
                    max_triangles: 400,
                    branch_levels: 1,
                    leaf_geometry: LeafGeometry::None,
                    leaf_reduction: 0.0,
                    ring_resolution: [4, 3, 3, 3],
                    screen_height: 0.01,
                    crown_impostor: true,
                },
            ],
        }
    }

    /// Generate hero tree LOD chain (2 levels)
    /// Maximum detail for single prominent trees (e.g., quest markers, landmarks)
    pub fn hero_tree() -> Self {
        Self {
            levels: vec![
                LodLevelConfig {
                    index: 0,
                    name: "Hero".to_string(),
                    target_triangles: 25000,
                    max_triangles: 35000,
                    branch_levels: 4,
                    leaf_geometry: LeafGeometry::Polygon,
                    leaf_reduction: 1.0,
                    ring_resolution: [32, 24, 16, 10],
                    screen_height: 0.4,
                    crown_impostor: false,
                },
                LodLevelConfig {
                    index: 1,
                    name: "Medium".to_string(),
                    target_triangles: 12000,
                    max_triangles: 16000,
                    branch_levels: 4,
                    leaf_geometry: LeafGeometry::CrossBillboard,
                    leaf_reduction: 0.8,
                    ring_resolution: [24, 16, 10, 6],
                    screen_height: 0.15,
                    crown_impostor: false,
                },
            ],
        }
    }

    /// Get the number of LOD levels
    pub fn level_count(&self) -> usize {
        self.levels.len()
    }
}

impl LodLevelConfig {
    /// Create from a species LodLevel definition
    pub fn from_lod_level(level: &LodLevel) -> Self {
        Self {
            index: level.index,
            name: if level.name.is_empty() {
                format!("LOD{}", level.index)
            } else {
                level.name.clone()
            },
            target_triangles: level.target_triangles,
            max_triangles: level
                .max_triangles
                .unwrap_or((level.target_triangles as f32 * 1.3) as u32),
            branch_levels: level.branch_levels,
            leaf_geometry: level.leaf_geometry,
            leaf_reduction: level.leaf_reduction,
            ring_resolution: level.ring_resolution.unwrap_or([12, 8, 5, 4]),
            screen_height: level.screen_height,
            crown_impostor: level.crown_impostor,
        }
    }
}

/// Complete set of LOD meshes for a tree
#[derive(Debug, Clone)]
pub struct LodMeshSet {
    /// Meshes for each LOD level
    pub meshes: Vec<LodMesh>,
}

/// A single LOD level mesh with metadata
#[derive(Debug, Clone)]
pub struct LodMesh {
    /// LOD index (0 = highest quality)
    pub index: u32,
    /// Name of this LOD level
    pub name: String,
    /// The generated mesh
    pub mesh: Mesh,
    /// Screen height threshold for LOD transition
    pub screen_height: f32,
    /// Statistics about this LOD
    pub stats: LodStats,
}

/// Statistics for a LOD level
#[derive(Debug, Clone, Copy, Default)]
pub struct LodStats {
    /// Number of vertices in the mesh
    pub vertex_count: u32,
    /// Number of triangles in the mesh
    pub triangle_count: u32,
    /// Number of branches included
    pub branch_count: u32,
    /// Number of leaves included
    pub leaf_count: u32,
}

/// Generate all LOD meshes for a tree using species settings
pub fn generate_lod_meshes(tree: &Tree, species: &Species) -> LodMeshSet {
    let config = LodGenerationConfig::from_species(species);
    generate_lod_meshes_with_config(tree, species, &config)
}

/// Generate LOD meshes with custom configuration
pub fn generate_lod_meshes_with_config(
    tree: &Tree,
    species: &Species,
    config: &LodGenerationConfig,
) -> LodMeshSet {
    let mut meshes = Vec::with_capacity(config.levels.len());

    for level_config in &config.levels {
        let lod_mesh = generate_single_lod(tree, species, level_config);
        meshes.push(lod_mesh);
    }

    LodMeshSet { meshes }
}

/// Generate a single LOD level mesh
fn generate_single_lod(tree: &Tree, species: &Species, level: &LodLevelConfig) -> LodMesh {
    // Count branches that will be included
    let branch_count = tree
        .stems
        .iter()
        .filter(|s| s.level as u32 <= level.branch_levels)
        .count() as u32;

    // Build branch mesh with LOD-appropriate ring resolution
    let mesh_config = MeshConfig {
        ring_resolution: level.ring_resolution,
        texture_v_scale: 1.0,
        pivot_painter: true,
        branch_collar_swell: 1.35,
        collar_falloff: 0.15,
    };

    // Generate branch mesh with level filtering
    let mut branch_mesh =
        MeshBuilder::new(tree, mesh_config).build_branches_to_level(level.branch_levels);

    // Generate leaf mesh based on LOD settings
    let leaf_count;
    let leaf_mesh = if level.crown_impostor {
        // TODO: Generate crown impostor billboard
        leaf_count = 0;
        Mesh::new()
    } else if level.leaf_geometry != LeafGeometry::None && level.leaf_reduction > 0.0 {
        // Filter leaves based on reduction factor
        let max_leaves = (tree.leaves.len() as f32 * level.leaf_reduction) as usize;
        let filtered_leaves: Vec<_> = tree.leaves.iter().take(max_leaves).cloned().collect();

        leaf_count = filtered_leaves.len() as u32;

        let leaf_config = LeafConfig {
            max_leaves: (species.leaves.count as f32 * level.leaf_reduction) as u32,
            geometry: level.leaf_geometry,
            shape: species.leaves.shape,
            polygon_resolution: 0, // Use shape's recommended resolution
            up_influence: species.leaves.up_influence,
            pivot_painter: true,
        };

        generate_leaf_mesh(&filtered_leaves, &leaf_config, tree)
    } else {
        leaf_count = 0;
        Mesh::new()
    };

    // Add bark submesh entry for existing branch geometry
    if !branch_mesh.indices.is_empty() {
        branch_mesh.submeshes.push(crate::mesh::Submesh {
            index_start: 0,
            index_count: branch_mesh.indices.len() as u32,
            material: MaterialType::Bark,
        });
    }

    // Merge branch and leaf meshes
    if !leaf_mesh.vertices.is_empty() {
        branch_mesh.merge(&leaf_mesh, MaterialType::Leaves);
    }

    let stats = LodStats {
        vertex_count: branch_mesh.vertices.len() as u32,
        triangle_count: (branch_mesh.indices.len() / 3) as u32,
        branch_count,
        leaf_count,
    };

    LodMesh {
        index: level.index,
        name: level.name.clone(),
        mesh: branch_mesh,
        screen_height: level.screen_height,
        stats,
    }
}

impl LodMeshSet {
    /// Get mesh for specific LOD level by index
    pub fn get(&self, level: u32) -> Option<&LodMesh> {
        self.meshes.iter().find(|m| m.index == level)
    }

    /// Get highest quality mesh (LOD0)
    pub fn best(&self) -> Option<&LodMesh> {
        self.meshes.first()
    }

    /// Get lowest quality mesh
    pub fn lowest(&self) -> Option<&LodMesh> {
        self.meshes.last()
    }

    /// Get appropriate LOD for screen height
    ///
    /// Returns the highest quality LOD whose screen_height threshold
    /// is less than or equal to the given height.
    pub fn for_screen_height(&self, height: f32) -> Option<&LodMesh> {
        for mesh in &self.meshes {
            if height >= mesh.screen_height {
                return Some(mesh);
            }
        }
        self.meshes.last()
    }

    /// Total number of LOD levels
    pub fn level_count(&self) -> usize {
        self.meshes.len()
    }

    /// Check if the set is empty
    pub fn is_empty(&self) -> bool {
        self.meshes.is_empty()
    }

    /// Get total statistics across all LOD levels
    pub fn total_stats(&self) -> LodStats {
        let mut total = LodStats::default();
        for mesh in &self.meshes {
            total.vertex_count += mesh.stats.vertex_count;
            total.triangle_count += mesh.stats.triangle_count;
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{Segment, Stem};
    use glam::Vec3;

    fn create_test_species() -> Species {
        let toml = r#"
[species]
name = "Test Tree"

[trunk]
height = 5.0
radius = 0.3

[branches.level1]
count = 4
length = 2.0

[leaves]
count = 100
min_level = 1
size = 0.1
distribution = "both"
geometry = "cross_billboard"

[lod]
preset = "balanced"
"#;
        Species::from_toml(toml).unwrap()
    }

    fn create_test_tree() -> Tree {
        let mut tree = Tree::new("Test".to_string(), 42);

        // Add trunk
        let mut trunk = Stem::new(0, 0);
        trunk.segments.push(Segment {
            start: Vec3::ZERO,
            end: Vec3::new(0.0, 5.0, 0.0),
            start_radius: 0.3,
            end_radius: 0.2,
            direction: Vec3::Y,
        });
        trunk.child_ids.push(1);
        tree.add_stem(trunk);

        // Add level 1 branch
        let mut branch1 = Stem::new(1, 1);
        branch1.parent_id = Some(0);
        branch1.parent_offset = 0.6;
        branch1.segments.push(Segment {
            start: Vec3::new(0.0, 3.0, 0.0),
            end: Vec3::new(1.5, 3.5, 0.0),
            start_radius: 0.1,
            end_radius: 0.05,
            direction: Vec3::new(1.0, 0.3, 0.0).normalize(),
        });
        branch1.child_ids.push(2);
        tree.add_stem(branch1);

        // Add level 2 branch
        let mut branch2 = Stem::new(2, 2);
        branch2.parent_id = Some(1);
        branch2.parent_offset = 0.8;
        branch2.segments.push(Segment {
            start: Vec3::new(1.2, 3.4, 0.0),
            end: Vec3::new(1.8, 3.8, 0.3),
            start_radius: 0.03,
            end_radius: 0.02,
            direction: Vec3::new(0.6, 0.4, 0.3).normalize(),
        });
        tree.add_stem(branch2);

        // Add level 3 branch
        let mut branch3 = Stem::new(3, 3);
        branch3.parent_id = Some(2);
        branch3.parent_offset = 0.9;
        branch3.segments.push(Segment {
            start: Vec3::new(1.7, 3.75, 0.27),
            end: Vec3::new(2.0, 4.0, 0.5),
            start_radius: 0.015,
            end_radius: 0.01,
            direction: Vec3::new(0.3, 0.25, 0.23).normalize(),
        });
        tree.add_stem(branch3);

        tree.update_bounds();
        tree
    }

    #[test]
    fn test_lod_level_config_default() {
        let config = LodLevelConfig::default();
        assert_eq!(config.index, 0);
        assert_eq!(config.branch_levels, 4);
        assert_eq!(config.leaf_geometry, LeafGeometry::CrossBillboard);
        assert!((config.leaf_reduction - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_lod_generation_config_balanced() {
        let config = LodGenerationConfig::balanced();
        assert_eq!(config.level_count(), 3);
        assert_eq!(config.levels[0].name, "High");
        assert_eq!(config.levels[1].name, "Medium");
        assert_eq!(config.levels[2].name, "Low");
    }

    #[test]
    fn test_lod_generation_config_high_quality() {
        let config = LodGenerationConfig::high_quality();
        assert_eq!(config.level_count(), 4);

        // Triangle counts should decrease
        for i in 1..config.levels.len() {
            assert!(
                config.levels[i].target_triangles < config.levels[i - 1].target_triangles,
                "LOD{} should have fewer triangles than LOD{}",
                i,
                i - 1
            );
        }
    }

    #[test]
    fn test_lod_generation_config_mobile() {
        let config = LodGenerationConfig::mobile();
        assert_eq!(config.level_count(), 3);

        // Mobile should have lower triangle counts
        assert!(config.levels[0].target_triangles <= 4000);
    }

    #[test]
    fn test_lod_generation_config_minimal() {
        let config = LodGenerationConfig::minimal();
        assert_eq!(config.level_count(), 2);
    }

    #[test]
    fn test_lod_generation_config_ultra() {
        let config = LodGenerationConfig::ultra();
        assert_eq!(config.level_count(), 5);

        // Ultra should have high triangle counts
        assert!(config.levels[0].target_triangles >= 50000);
    }

    #[test]
    fn test_lod_generation_config_from_preset() {
        let balanced = LodGenerationConfig::from_preset(LodPreset::Balanced);
        assert_eq!(balanced.level_count(), 3);

        let mobile = LodGenerationConfig::from_preset(LodPreset::Mobile);
        assert_eq!(mobile.level_count(), 3);
    }

    #[test]
    fn test_lod_generation_config_from_species() {
        let species = create_test_species();
        let config = LodGenerationConfig::from_species(&species);

        // Should get balanced preset levels
        assert_eq!(config.level_count(), 3);
    }

    #[test]
    fn test_generate_lod_meshes() {
        let tree = create_test_tree();
        let species = create_test_species();

        let lod_set = generate_lod_meshes(&tree, &species);

        // Should have multiple LOD levels
        assert!(!lod_set.is_empty());
        assert!(lod_set.level_count() >= 2);

        // Each LOD should have geometry
        for lod in &lod_set.meshes {
            assert!(!lod.mesh.is_empty(), "LOD {} should have geometry", lod.index);
        }
    }

    #[test]
    fn test_lod_mesh_set_get() {
        let tree = create_test_tree();
        let species = create_test_species();
        let lod_set = generate_lod_meshes(&tree, &species);

        // Should be able to get LOD0
        let lod0 = lod_set.get(0);
        assert!(lod0.is_some());
        assert_eq!(lod0.unwrap().index, 0);

        // Non-existent LOD should return None
        let lod_bad = lod_set.get(999);
        assert!(lod_bad.is_none());
    }

    #[test]
    fn test_lod_mesh_set_best_and_lowest() {
        let tree = create_test_tree();
        let species = create_test_species();
        let lod_set = generate_lod_meshes(&tree, &species);

        let best = lod_set.best().unwrap();
        let lowest = lod_set.lowest().unwrap();

        assert_eq!(best.index, 0);
        assert!(lowest.index > best.index);
    }

    #[test]
    fn test_lod_mesh_set_for_screen_height() {
        let tree = create_test_tree();
        let species = create_test_species();
        let lod_set = generate_lod_meshes(&tree, &species);

        // Large screen height should return highest quality
        let large = lod_set.for_screen_height(0.8).unwrap();
        assert_eq!(large.index, 0);

        // Small screen height should return lower quality
        let small = lod_set.for_screen_height(0.01).unwrap();
        assert!(small.index > 0);
    }

    #[test]
    fn test_lod_stats() {
        let tree = create_test_tree();
        let species = create_test_species();
        let lod_set = generate_lod_meshes(&tree, &species);

        for lod in &lod_set.meshes {
            assert_eq!(
                lod.stats.vertex_count,
                lod.mesh.vertices.len() as u32,
                "Stats vertex count should match mesh"
            );
            assert_eq!(
                lod.stats.triangle_count,
                (lod.mesh.indices.len() / 3) as u32,
                "Stats triangle count should match mesh"
            );
        }
    }

    #[test]
    fn test_lod_branch_level_filtering() {
        let tree = create_test_tree();
        let species = create_test_species();

        let config = LodGenerationConfig {
            levels: vec![
                LodLevelConfig {
                    index: 0,
                    name: "All".to_string(),
                    branch_levels: 4, // Include all levels
                    leaf_geometry: LeafGeometry::None,
                    leaf_reduction: 0.0,
                    ..Default::default()
                },
                LodLevelConfig {
                    index: 1,
                    name: "TrunkOnly".to_string(),
                    branch_levels: 0, // Trunk only
                    leaf_geometry: LeafGeometry::None,
                    leaf_reduction: 0.0,
                    ..Default::default()
                },
            ],
        };

        let lod_set = generate_lod_meshes_with_config(&tree, &species, &config);

        // LOD with all branches should have more geometry
        let all_lod = lod_set.get(0).unwrap();
        let trunk_lod = lod_set.get(1).unwrap();

        assert!(
            all_lod.stats.branch_count > trunk_lod.stats.branch_count,
            "All branches LOD should have more branches than trunk-only"
        );
    }

    #[test]
    fn test_lod_leaf_reduction() {
        let mut tree = create_test_tree();
        // Add some leaves
        use crate::tree::Leaf;
        use glam::Quat;
        for i in 0..100 {
            tree.leaves.push(Leaf {
                position: Vec3::new(1.0 + (i as f32 * 0.01), 3.5, 0.0),
                rotation: Quat::IDENTITY,
                scale: 0.1,
                stem_id: 1,
            });
        }

        let species = create_test_species();

        let config = LodGenerationConfig {
            levels: vec![
                LodLevelConfig {
                    index: 0,
                    name: "Full".to_string(),
                    leaf_geometry: LeafGeometry::Billboard,
                    leaf_reduction: 1.0, // 100% leaves
                    ..Default::default()
                },
                LodLevelConfig {
                    index: 1,
                    name: "Half".to_string(),
                    leaf_geometry: LeafGeometry::Billboard,
                    leaf_reduction: 0.5, // 50% leaves
                    ..Default::default()
                },
            ],
        };

        let lod_set = generate_lod_meshes_with_config(&tree, &species, &config);

        let full_lod = lod_set.get(0).unwrap();
        let half_lod = lod_set.get(1).unwrap();

        assert!(
            full_lod.stats.leaf_count > half_lod.stats.leaf_count,
            "Full LOD ({}) should have more leaves than half LOD ({})",
            full_lod.stats.leaf_count,
            half_lod.stats.leaf_count
        );
    }

    #[test]
    fn test_lod_level_config_from_lod_level() {
        let lod_level = LodLevel {
            index: 2,
            name: "Custom".to_string(),
            target_triangles: 5000,
            max_triangles: Some(7000),
            branch_levels: 2,
            leaf_geometry: LeafGeometry::Billboard,
            leaf_reduction: 0.5,
            ring_resolution: Some([8, 6, 4, 3]),
            screen_height: 0.15,
            crown_impostor: true,
        };

        let config = LodLevelConfig::from_lod_level(&lod_level);

        assert_eq!(config.index, 2);
        assert_eq!(config.name, "Custom");
        assert_eq!(config.target_triangles, 5000);
        assert_eq!(config.max_triangles, 7000);
        assert_eq!(config.branch_levels, 2);
        assert_eq!(config.leaf_geometry, LeafGeometry::Billboard);
        assert!((config.leaf_reduction - 0.5).abs() < f32::EPSILON);
        assert_eq!(config.ring_resolution, [8, 6, 4, 3]);
        assert!((config.screen_height - 0.15).abs() < f32::EPSILON);
        assert!(config.crown_impostor);
    }

    #[test]
    fn test_lod_level_config_default_name() {
        let lod_level = LodLevel {
            index: 3,
            name: String::new(), // Empty name
            target_triangles: 1000,
            max_triangles: None,
            branch_levels: 1,
            leaf_geometry: LeafGeometry::None,
            leaf_reduction: 0.0,
            ring_resolution: None,
            screen_height: 0.05,
            crown_impostor: true,
        };

        let config = LodLevelConfig::from_lod_level(&lod_level);

        assert_eq!(config.name, "LOD3");
        assert_eq!(config.max_triangles, 1300); // 1000 * 1.3
        assert_eq!(config.ring_resolution, [12, 8, 5, 4]); // default
    }

    #[test]
    fn test_total_stats() {
        let tree = create_test_tree();
        let species = create_test_species();
        let lod_set = generate_lod_meshes(&tree, &species);

        let total = lod_set.total_stats();

        let expected_verts: u32 = lod_set.meshes.iter().map(|m| m.stats.vertex_count).sum();
        let expected_tris: u32 = lod_set.meshes.iter().map(|m| m.stats.triangle_count).sum();

        assert_eq!(total.vertex_count, expected_verts);
        assert_eq!(total.triangle_count, expected_tris);
    }
}
