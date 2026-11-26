//! Level of Detail (LOD) generation system.
//!
//! This module provides LOD mesh generation for trees, creating multiple
//! quality levels for efficient rendering at different distances.

use crate::{
    impostor::bake_impostor,
    leaves::{LeafConfig, generate_leaf_mesh},
    mesh::{MaterialType, Mesh, Submesh, Vertex},
    mesh_builder::{MeshBuilder, MeshConfig},
    species::{LeafGeometry, LodLevel, LodPreset, Species},
    textures::{RgbaTexture, TextureSet},
    tree::{BoundingBox, Tree},
};
use glam::{Vec2, Vec3, Vec4};

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
                    target_triangles: 70000,
                    max_triangles: 135000,
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
                    target_triangles: 28000,
                    max_triangles: 42000,
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
                    target_triangles: 65000,
                    max_triangles: 125000,
                    branch_levels: 4,
                    leaf_geometry: LeafGeometry::CrossBillboard,
                    leaf_reduction: 1.0,
                    ring_resolution: [32, 22, 14, 8], // Smooth close-up cylinders across trunk & limbs
                    screen_height: 0.4,
                    crown_impostor: false,
                },
                LodLevelConfig {
                    index: 1,
                    name: "Medium".to_string(),
                    target_triangles: 20000,
                    max_triangles: 32000,
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

    /// Generate 2-LOD Nanite/meshlet-ready 100% opaque 3D geometric flora chain:
    /// - LOD0 ("Hero3D", near < 85 m): 80k–110k tris packing 10k–18k+ 100% opaque 3D curved leaves,
    ///   petite 3D blossoms, twigs & 3D displaced bark/branching root claws.
    /// - LOD1 ("Mid3D", mid-field 85–420 m): 20k–32k tris packing 3.5k–6k+ 100% opaque 3D leaf clusters & limbs.
    pub fn nanite_2lod() -> Self {
        Self {
            levels: vec![
                LodLevelConfig {
                    index: 0,
                    name: "Hero3D".to_string(),
                    target_triangles: 94_000,
                    max_triangles: 110_000,
                    branch_levels: 4,
                    leaf_geometry: LeafGeometry::Polygon,
                    leaf_reduction: 1.0,
                    ring_resolution: [22, 12, 6, 4],
                    screen_height: 0.16,
                    crown_impostor: false,
                },
                LodLevelConfig {
                    index: 1,
                    name: "Mid3D".to_string(),
                    target_triangles: 26_000,
                    max_triangles: 32_000,
                    branch_levels: 2,
                    leaf_geometry: LeafGeometry::Polygon,
                    leaf_reduction: 0.38,
                    ring_resolution: [12, 7, 4, 3],
                    screen_height: 0.03,
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
    /// Baked crown-impostor atlas when this level uses `crown_impostor`
    /// (left half = +Z view, right half = +X view).
    pub impostor_atlas: Option<RgbaTexture>,
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

/// Generate LOD meshes with custom configuration (default alpha-tested cutout foliage).
pub fn generate_lod_meshes_with_config(
    tree: &Tree,
    species: &Species,
    config: &LodGenerationConfig,
) -> LodMeshSet {
    generate_lod_meshes_with_foliage_mode(
        tree,
        species,
        config,
        crate::opaque_foliage::FoliageRenderMode::Cutout,
    )
}

/// Generate LOD meshes with custom configuration and explicit foliage rendering mode
/// (`Cutout` = default alpha-tested cards; `Opaque` = closed low-poly cluster volumes).
pub fn generate_lod_meshes_with_foliage_mode(
    tree: &Tree,
    species: &Species,
    config: &LodGenerationConfig,
    foliage_mode: crate::opaque_foliage::FoliageRenderMode,
) -> LodMeshSet {
    let is_opaque = foliage_mode.is_opaque();
    // Impostor bakes rasterize into a compact distant atlas; cap temporary texture
    // resolution at 256 so 1024x1024 hero species stay instantaneous during mesh generation.
    let impostor_textures = (!is_opaque && config.levels.iter().any(|level| level.crown_impostor))
        .then(|| {
            let mut imp_species = species.clone();
            imp_species.textures.resolution = imp_species.textures.resolution.min(256);
            TextureSet::generate(&imp_species)
        });

    let mut meshes = Vec::with_capacity(config.levels.len());
    for level_config in &config.levels {
        let lod_mesh = generate_single_lod(
            tree,
            species,
            level_config,
            impostor_textures.as_ref(),
            foliage_mode,
        );
        meshes.push(lod_mesh);
    }

    LodMeshSet { meshes }
}

/// Generate a single LOD level mesh
fn generate_single_lod(
    tree: &Tree,
    species: &Species,
    level: &LodLevelConfig,
    impostor_textures: Option<&TextureSet>,
    foliage_mode: crate::opaque_foliage::FoliageRenderMode,
) -> LodMesh {
    let is_geo3d = foliage_mode == crate::opaque_foliage::FoliageRenderMode::Geo3d;
    // Adapt branch depth & ring counts for dense multi-hundred-stem species (e.g. Norfolk Island Pine)
    // in geo3d mode so LOD0 stays within 25k-55k tris and LOD1 stays within 6k-14k tris.
    let effective_branch_levels = if is_geo3d && level.index > 0 && tree.stems.len() > 400 {
        level.branch_levels.min(1)
    } else if is_geo3d && level.index > 0 && tree.stems.len() > 260 {
        level.branch_levels.min(2)
    } else {
        level.branch_levels
    };
    let effective_ring_res = if is_geo3d && tree.stems.len() > 260 {
        if level.index == 0 {
            [20, 10, 4, 3]
        } else {
            [12, 5, 3, 3]
        }
    } else if is_geo3d && level.index == 0 {
        [20, 11, 5, 3]
    } else if is_geo3d && level.index > 0 {
        [
            level.ring_resolution[0].min(12),
            level.ring_resolution[1].min(6),
            level.ring_resolution[2].min(4),
            level.ring_resolution[3].min(3),
        ]
    } else {
        level.ring_resolution
    };

    // Count branches that will be included
    let branch_count = tree
        .stems
        .iter()
        .filter(|s| s.level as u32 <= effective_branch_levels)
        .count() as u32;

    // Build branch mesh with LOD-appropriate ring resolution
    let default_mesh_config = MeshConfig::default();
    let flare_height = if species.trunk.buttress_height > 0.0 {
        (species.trunk.buttress_height / species.trunk.height.max(0.5)).clamp(0.05, 0.65)
    } else {
        default_mesh_config.trunk_base_flare_height
    };
    let mesh_config = MeshConfig {
        ring_resolution: effective_ring_res,
        texture_v_scale: 1.0,
        pivot_painter: true,
        branch_collar_swell: 1.35,
        collar_falloff: 0.15,
        // `trunk.buttress` scales the base flare and flutes; 1.0 is the built-in look.
        trunk_base_flare: if species.trunk.buttress == 1.0 {
            default_mesh_config.trunk_base_flare
        } else {
            1.0 + (default_mesh_config.trunk_base_flare - 1.0) * species.trunk.buttress
        },
        trunk_base_flare_height: flare_height,
        branch_base_swell: default_mesh_config.branch_base_swell,
        branch_base_falloff: default_mesh_config.branch_base_falloff,
        trunk_buttress: species.trunk.buttress,
        surface_root_reach: species.trunk.surface_root_reach,
        aerial_roots: if is_geo3d && level.index > 0 {
            0
        } else if effective_branch_levels >= 2 {
            species.trunk.aerial_roots
        } else {
            0
        },
        hollow_height: species.trunk.hollow_height,
        hollow_width: species.trunk.hollow_width,
        crown_boss: species
            .rosette
            .as_ref()
            .map(|r| r.crown_boss)
            .unwrap_or(0.0),
        upper_bark_transition: species.uses_upper_bark().then_some((
            species.textures.bark_transition_height,
            species.textures.bark_transition_span,
        )),
        world_scale_uv: is_geo3d,
        bark_relief_strength: if is_geo3d { 1.50 } else { 1.0 },
    };

    // Generate branch mesh with level filtering
    let mut branch_mesh =
        MeshBuilder::new(tree, mesh_config).build_branches_to_level(effective_branch_levels);

    // Generate leaf mesh based on LOD settings
    let leaf_count;
    let mut impostor_atlas = None;
    let mut geo3d_twigs: Option<Mesh> = None;
    let is_opaque = foliage_mode.is_opaque();
    let (leaf_mesh, separate_organ_mesh) = if is_geo3d {
        if level.crown_impostor
            || (level.leaf_geometry != LeafGeometry::None && level.leaf_reduction > 0.0)
        {
            let branch_tris = (branch_mesh.indices.len() / 3) as u32;
            let min_foliage = if level.leaf_reduction >= 0.60 {
                48_000
            } else {
                12_000
            };
            let desired = level.target_triangles.saturating_sub(branch_tris);
            let cap = level.max_triangles.saturating_sub(branch_tris);
            let foliage_budget = desired.clamp(min_foliage.min(cap.max(4_000)), cap.max(4_000));

            let res = crate::geo3d_foliage::generate_geo3d_foliage_mesh(
                tree,
                species,
                level.leaf_reduction.max(0.25),
                foliage_budget,
            );
            leaf_count = res.unit_count;
            if !res.twigs.vertices.is_empty() {
                geo3d_twigs = Some(res.twigs);
            }
            (res.foliage, res.organs)
        } else {
            leaf_count = 0;
            (Mesh::new(), None)
        }
    } else if is_opaque {
        if level.crown_impostor
            || (level.leaf_geometry != LeafGeometry::None && level.leaf_reduction > 0.0)
        {
            let (fmesh, omesh) = crate::opaque_foliage::generate_opaque_foliage_mesh(
                tree,
                species,
                level.leaf_reduction.max(0.25),
                level.crown_impostor,
            );
            leaf_count = if fmesh.is_empty() {
                0
            } else {
                (fmesh.indices.len() / 3) as u32
            };
            (fmesh, omesh)
        } else {
            leaf_count = 0;
            (Mesh::new(), None)
        }
    } else if level.crown_impostor {
        // Bake the crown (leaves + cut branches) into a two-view atlas when the
        // species ships a [textures] section; otherwise fall back to the
        // geometric two-quad crown impostor.
        let mesh = impostor_textures
            .and_then(|textures| bake_impostor(tree, species, level.branch_levels, textures))
            .map(|impostor| {
                impostor_atlas = Some(impostor.atlas);
                impostor.mesh
            })
            .unwrap_or_else(|| generate_crown_impostor_mesh(tree, species));
        leaf_count = if mesh.is_empty() { 0 } else { 1 };
        (mesh, None)
    } else if level.leaf_geometry != LeafGeometry::None && level.leaf_reduction > 0.0 {
        // Filter leaves based on reduction factor. Leaves are stored stem by
        // stem, so subsample evenly across the whole list rather than taking
        // a prefix (a prefix strips entire limbs off one side of the crown).
        let total = tree.leaves.len();
        let max_leaves = ((total as f32 * level.leaf_reduction) as usize).min(total);
        let mut filtered_leaves: Vec<_> = if max_leaves >= total {
            tree.leaves.clone()
        } else {
            (0..max_leaves)
                .map(|i| tree.leaves[i * total / max_leaves.max(1)])
                .collect()
        };
        // Canopy cards (or opt-in billboard LOD compensation): grow surviving cards so crown coverage holds.
        let compensate = (species.canopy.lod_scale_compensation
            && level.leaf_geometry == LeafGeometry::CanopyCard)
            || (species.canopy.billboard_lod_compensation
                && matches!(
                    level.leaf_geometry,
                    LeafGeometry::Billboard | LeafGeometry::CrossBillboard
                ));
        if compensate && max_leaves > 0 && max_leaves < total {
            let max_grow = if level.leaf_geometry == LeafGeometry::CanopyCard {
                3.0
            } else {
                1.75
            };
            let grow = (total as f32 / max_leaves as f32).sqrt().min(max_grow);
            for leaf in &mut filtered_leaves {
                leaf.scale *= grow;
            }
        }

        leaf_count = filtered_leaves.len() as u32;

        let leaf_config = LeafConfig {
            max_leaves: (species.leaves.count as f32 * level.leaf_reduction) as u32,
            geometry: level.leaf_geometry,
            shape: species.leaves.shape,
            polygon_resolution: 0, // Use shape's recommended resolution
            up_influence: species.leaves.up_influence,
            pivot_painter: true,
            palm_frond_ribbon: crate::leaves::is_pinnate_palm_species(species),
            paddle_leaf_ribbon: crate::leaves::is_paddle_ribbon_species(species),
            floral_quad_atlas: crate::leaves::is_floral_atlas_species(species),
            blade_ribbon: crate::rosette::BladeRibbon::from_species(species).map(|mut ribbon| {
                // Far LODs: fewer rows, and wider ribbons to hold the crown's coverage.
                if matches!(
                    level.leaf_geometry,
                    LeafGeometry::Billboard | LeafGeometry::CanopyCard
                ) {
                    ribbon.rows = 3;
                }
                if max_leaves > 0 && max_leaves < total {
                    ribbon.widen = (total as f32 / max_leaves as f32).sqrt().min(2.5);
                }
                ribbon
            }),
            foliage_atlas: species.uses_foliage_atlas(),
            canopy: species.canopy.clone(),
        };

        let mut mesh = generate_leaf_mesh(&filtered_leaves, &leaf_config, tree);
        // Inflorescences, fruit, skirt blades and spikes thin with the foliage.
        let mut separate_organ_mesh: Option<Mesh> = None;
        if !tree.organs.is_empty() {
            let organs = crate::organs::reduce_organs(&tree.organs, level.leaf_reduction);
            let detail = match level.leaf_geometry {
                LeafGeometry::Polygon | LeafGeometry::CrossBillboard => {
                    if level.leaf_reduction >= 0.75 { 0 } else { 1 }
                }
                _ => 2,
            };
            let organ_mesh = crate::organs::generate_organ_mesh(
                &organs,
                tree,
                species.canopy.vertex_color,
                true,
                detail,
            );
            if species.uses_separate_organ_material() {
                separate_organ_mesh = Some(organ_mesh);
            } else {
                crate::organs::append_foliage(&mut mesh, &organ_mesh);
            }
        }
        (mesh, separate_organ_mesh)
    } else {
        leaf_count = 0;
        (Mesh::new(), None)
    };

    // Add bark submesh entry for existing branch geometry
    if !branch_mesh.indices.is_empty() && branch_mesh.submeshes.is_empty() {
        branch_mesh.submeshes.push(crate::mesh::Submesh {
            index_start: 0,
            index_count: branch_mesh.indices.len() as u32,
            material: MaterialType::Bark,
        });
    }

    // Tint+AO vertex colour mode: bake anatomical branch-crotch contact AO,
    // root-buttress soil occlusion, interior canopy limb self-shadowing, and
    // subtle young-shoot bark warmth into COLOR_0 so glTF / engine PBR shaders
    // ground branch junctions and wood hierarchy automatically.
    if species.canopy.vertex_color == crate::species::FoliageVertexColor::TintAo {
        let bounds_min = tree.bounds.min;
        let bounds_size = tree.bounds.size();
        let has_bounds = bounds_size.length_squared() > 1e-6;
        let (crown_center, crown_inv_r) = if !tree.leaves.is_empty() {
            let mut sum = Vec3::ZERO;
            for leaf in &tree.leaves {
                sum += leaf.position;
            }
            let c = sum / tree.leaves.len() as f32;
            let mut max_r2 = 0.25_f32;
            for leaf in &tree.leaves {
                max_r2 = max_r2.max((leaf.position - c).length_squared());
            }
            (c, 1.0 / max_r2.sqrt())
        } else {
            (Vec3::ZERO, 0.0)
        };
        let trunk_base_r = tree.stems.first().map(|s| s.base_radius()).unwrap_or(0.30);
        let ao_scale = species.canopy.ao_strength.clamp(0.0, 1.0);
        let smooth01 = |e0: f32, e1: f32, x: f32| -> f32 {
            let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };

        for v in &mut branch_mesh.vertices {
            let depth = v.uv2.x.clamp(0.0, 1.0); // 0.0 = trunk, 0.25 = L1, 0.50 = L2, 0.75 = L3

            // 1. Anatomical branch-crotch contact AO reaching past parent radius onto visible collar
            let crotch_ao = if depth > 0.05 && has_bounds {
                let pivot = bounds_min + Vec3::new(v.color.x, v.color.y, v.color.z) * bounds_size;
                let collar_dist = (v.position - pivot).length();
                let parent_r_est = trunk_base_r * (1.0 - depth * 1.15).clamp(0.12, 0.78);
                let inner = parent_r_est * 0.72;
                let outer = parent_r_est * 1.15 + (0.30 - 0.16 * depth).max(0.10);
                let base_ao = 0.68 + 0.32 * smooth01(inner, outer, collar_dist);
                let under = (0.22 - v.normal.y).clamp(0.0, 0.55)
                    * (1.0 - smooth01(inner, outer, collar_dist));
                (base_ao - 0.10 * under).clamp(0.60, 1.0)
            } else {
                1.0
            };

            // 2. Root-collar & concave root-flute soil contact AO
            let h_ground = v.position.y - bounds_min.y.min(0.0);
            let root_ao = if h_ground < 0.62 {
                let radial_xz = Vec2::new(v.position.x, v.position.z);
                let norm_xz = Vec2::new(v.normal.x, v.normal.z);
                let flute_crevice =
                    if radial_xz.length_squared() > 1e-4 && norm_xz.length_squared() > 1e-4 {
                        (1.0 - radial_xz.normalize().dot(norm_xz.normalize()).abs()).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                let base = 0.72 + 0.28 * smooth01(-0.04, 0.56, h_ground);
                (base - 0.10 * flute_crevice * (1.0 - smooth01(0.0, 0.45, h_ground)))
                    .clamp(0.64, 1.0)
            } else {
                1.0
            };

            // 3. Gentle canopy interior self-shadowing on inner crown limbs
            let canopy_ao = if crown_inv_r > 0.0 && depth >= 0.20 {
                let d_crown = (v.position - crown_center).length() * crown_inv_r;
                let raw = 0.80 + 0.20 * smooth01(0.18, 0.88, d_crown);
                1.0 - (1.0 - raw) * ao_scale
            } else {
                1.0
            };

            let ao = (crotch_ao * root_ao * canopy_ao).clamp(0.54, 1.0);

            // 4. Subtle living young-shoot bark tint on outer branches & twigs
            let young_t = smooth01(0.32, 0.78, depth);
            let tint = Vec3::ONE.lerp(Vec3::new(1.04, 0.99, 0.91), young_t);
            let mut rgb = (tint * ao).clamp(Vec3::ZERO, Vec3::ONE);

            // 5. Fire-blackened base (`trunk.char_height`) + fire-hollow interior soot:
            // a ragged scorch line that reaches higher on some sides of the trunk than others.
            if species.trunk.char_height > 0.0 {
                let az = v.position.z.atan2(v.position.x);
                let ragged = 0.80
                    + 0.20 * (az * 3.0 + tree.seed as f32 * 0.37).sin()
                    + 0.10 * (az * 7.0 - tree.seed as f32 * 0.11).sin();
                let top = species.trunk.char_height * ragged;
                let scorch = 1.0 - smooth01(top * 0.55, top, h_ground);
                let k = (scorch * species.trunk.char_strength).clamp(0.0, 1.0);
                rgb = rgb.lerp(rgb * Vec3::new(0.10, 0.095, 0.09), k);
            }
            if species.trunk.hollow_height > 0.0 && h_ground < species.trunk.hollow_height {
                // Darken recessed inward-facing fire-hollow cavern walls deeply
                let horiz = Vec2::new(v.position.x, v.position.z);
                let n_horiz = Vec2::new(v.normal.x, v.normal.z);
                if horiz.length_squared() > 1e-4 && n_horiz.dot(horiz) < 0.0 {
                    rgb *= Vec3::new(0.06, 0.055, 0.05);
                }
            }
            v.color = Vec4::new(rgb.x, rgb.y, rgb.z, 1.0);
        }
    }

    // In opaque foliage mode, bake linear species bark colour (and half-bark
    // transitions) into branch vertex colours AFTER TintAo so crotch/root AO
    // and fire-char multiply cleanly onto the true species bark albedo.
    if is_opaque {
        let bark_lin = crate::opaque_foliage::srgb3_to_linear(
            crate::opaque_foliage::species_bark_color(species),
        );
        let upper_lin = species
            .textures
            .upper_bark_color
            .map(crate::opaque_foliage::srgb3_to_linear)
            .unwrap_or(bark_lin);
        let has_tint_ao = species.canopy.vertex_color == crate::species::FoliageVertexColor::TintAo;
        let char_h = species.trunk.char_height;
        let char_s = species.trunk.char_strength;
        let trans_h = species.textures.bark_transition_height;
        let trans_span = species.textures.bark_transition_span.max(0.25);
        for v in &mut branch_mesh.vertices {
            let mut c = if species.uses_upper_bark() && v.position.y > trans_h {
                let t = ((v.position.y - trans_h) / trans_span).clamp(0.0, 1.0);
                [
                    bark_lin[0] + (upper_lin[0] - bark_lin[0]) * t,
                    bark_lin[1] + (upper_lin[1] - bark_lin[1]) * t,
                    bark_lin[2] + (upper_lin[2] - bark_lin[2]) * t,
                ]
            } else {
                bark_lin
            };
            if has_tint_ao {
                c = [c[0] * v.color.x, c[1] * v.color.y, c[2] * v.color.z];
            } else {
                if char_h > 0.0 && char_s > 0.0 && v.position.y < char_h {
                    let ck = (1.0 - (v.position.y / char_h).clamp(0.0, 1.0)) * char_s;
                    let mult = (1.0 - 0.84 * ck).clamp(0.12, 1.0);
                    c = [c[0] * mult, c[1] * mult, c[2] * mult];
                }
                let h_ao =
                    0.78 + 0.22 * (v.position.y / species.trunk.height.max(1.0)).clamp(0.0, 1.0);
                c = [c[0] * h_ao, c[1] * h_ao, c[2] * h_ao];
            }
            v.color = Vec4::new(c[0], c[1], c[2], 1.0);
        }

        // In geo3d mode, bake `TEXCOORD_1 = (bark_ao, height_norm)` onto bark vertices so
        // `nanite_bark_4k.gdshader` drives root-collar soil contact darkening (`UV2.y < 0.08`)
        // and dual-zone rough-stocking -> smooth upper eucalyptus bark transitions directly!
        if is_geo3d {
            let min_y = tree.bounds.min.y.min(0.0);
            let span_y = (tree.bounds.max.y - min_y).max(0.5);
            for v in &mut branch_mesh.vertices {
                let height_norm = ((v.position.y - min_y) / span_y).clamp(0.0, 1.0);
                // Recover scalar AO luminance stored on bark color relative to unoccluded bark
                let depth_ao = (1.0 - 0.28 * v.uv2.x.clamp(0.0, 1.0)).clamp(0.55, 1.0);
                let ground_ao = if v.position.y < 0.55 {
                    0.58 + 0.42 * (v.position.y / 0.55).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let bark_ao = (depth_ao * ground_ao).clamp(0.32, 1.0);
                v.uv2 = Vec2::new(bark_ao, height_norm);
            }
            if let Some(twigs) = geo3d_twigs
                && !twigs.vertices.is_empty()
            {
                let twig_mat = if species.uses_upper_bark() {
                    MaterialType::UpperBark
                } else {
                    MaterialType::Bark
                };
                branch_mesh.merge(&twigs, twig_mat);
            }
        }
    }

    // Merge leaf/impostor geometry with its material.
    let merge_material = if level.crown_impostor && !is_opaque {
        MaterialType::Impostor
    } else {
        MaterialType::Leaves
    };
    if !leaf_mesh.vertices.is_empty() {
        branch_mesh.merge(&leaf_mesh, merge_material);
    }
    if let Some(organ_mesh) = separate_organ_mesh
        && !organ_mesh.vertices.is_empty()
    {
        branch_mesh.merge(&organ_mesh, MaterialType::Organ);
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
        impostor_atlas,
        stats,
    }
}

fn generate_crown_impostor_mesh(tree: &Tree, species: &Species) -> Mesh {
    if !tree.bounds.is_valid() {
        return Mesh::new();
    }

    let bounds = tree.bounds;
    let size = bounds.size();
    let center = bounds.center();
    let crown_offset = species.crown.offset.clamp(0.0, 0.9);
    let crown_center_y = bounds.min.y + size.y * (crown_offset + (1.0 - crown_offset) * 0.5);
    let crown_height = (size.y * (1.0 - crown_offset)).max(0.75);
    let crown_width = size
        .x
        .max(size.z)
        .max(size.y * 0.35)
        .max(species.leaves.size * 8.0)
        .max(0.75)
        * species.crown.width_ratio.max(0.2);
    let crown_center = Vec3::new(center.x, crown_center_y, center.z);

    let mut mesh = Mesh::new();
    add_impostor_card(
        &mut mesh,
        crown_center,
        Vec3::X * crown_width,
        Vec3::Y * crown_height,
        Vec3::Z,
        &bounds,
    );
    add_impostor_card(
        &mut mesh,
        crown_center,
        Vec3::Z * crown_width,
        Vec3::Y * crown_height,
        Vec3::X,
        &bounds,
    );

    if !mesh.indices.is_empty() {
        mesh.submeshes.push(Submesh {
            index_start: 0,
            index_count: mesh.indices.len() as u32,
            material: MaterialType::Leaves,
        });
    }

    mesh
}

fn add_impostor_card(
    mesh: &mut Mesh,
    center: Vec3,
    horizontal: Vec3,
    vertical: Vec3,
    normal: Vec3,
    bounds: &BoundingBox,
) {
    let base = mesh.vertices.len() as u32;
    let half_h = horizontal * 0.5;
    let half_v = vertical * 0.5;
    let corners = [
        center - half_h - half_v,
        center + half_h - half_v,
        center + half_h + half_v,
        center - half_h + half_v,
    ];
    let uvs = [
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 1.0),
    ];

    for (position, uv) in corners.into_iter().zip(uvs) {
        let mut vertex = Vertex::new(position, normal, uv);
        vertex.uv2 = Vec2::new(1.0, 0.0);
        vertex.color = Vec4::new(
            normalize_axis(position.x, bounds.min.x, bounds.max.x),
            normalize_axis(position.y, bounds.min.y, bounds.max.y),
            normalize_axis(position.z, bounds.min.z, bounds.max.z),
            0.2,
        );
        mesh.vertices.push(vertex);
    }

    mesh.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    mesh.indices
        .extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
}

fn normalize_axis(value: f32, min: f32, max: f32) -> f32 {
    let span = max - min;
    if span.abs() < 0.0001 {
        0.0
    } else {
        ((value - min) / span).clamp(0.0, 1.0)
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
            assert!(
                !lod.mesh.is_empty(),
                "LOD {} should have geometry",
                lod.index
            );
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

    use crate::generation::generate_tree;

    fn layered_trunk_species(trunk_extra: &str) -> Species {
        Species::from_toml(&format!(
            r#"
[species]
name = "Trunk Layers"

[trunk]
height = 8.0
radius = 0.5
taper = 0.7
segments = 12
{trunk_extra}

[branches.level1]
count = 5
length = 2.5

[leaves]
count = 200

[canopy]
vertex_color = "tint_ao"
"#
        ))
        .unwrap()
    }

    #[test]
    fn char_height_blackens_only_the_base_of_the_trunk() {
        let mean_low_and_high = |species: &Species| {
            let tree = generate_tree(species, 5).unwrap();
            let lods = generate_lod_meshes(&tree, species);
            let mesh = &lods.meshes[0].mesh;
            let bark_end = mesh
                .submeshes
                .iter()
                .find(|s| s.material == MaterialType::Bark)
                .map(|s| (s.index_start + s.index_count) as usize)
                .unwrap();
            let (mut low, mut low_n, mut high, mut high_n) = (0.0f32, 0u32, 0.0f32, 0u32);
            for &i in &mesh.indices[..bark_end] {
                let v = &mesh.vertices[i as usize];
                if v.uv2.x > 0.05 {
                    continue; // trunk only
                }
                if v.position.y > 0.2 && v.position.y < 1.0 {
                    low += v.color.x;
                    low_n += 1;
                } else if v.position.y > 4.0 {
                    high += v.color.x;
                    high_n += 1;
                }
            }
            (low / low_n.max(1) as f32, high / high_n.max(1) as f32)
        };
        let plain = mean_low_and_high(&layered_trunk_species(""));
        let charred = mean_low_and_high(&layered_trunk_species(
            "char_height = 2.0\nchar_strength = 0.9",
        ));
        assert!(
            charred.0 < plain.0 * 0.7,
            "base should be scorched: {} vs {}",
            charred.0,
            plain.0
        );
        assert!(
            (charred.1 - plain.1).abs() < 1e-4,
            "bark above the scorch line must be untouched"
        );
    }

    #[test]
    fn buttress_scales_the_trunk_base() {
        let base_radius = |species: &Species| {
            let tree = generate_tree(species, 5).unwrap();
            let lods = generate_lod_meshes(&tree, species);
            lods.meshes[0]
                .mesh
                .vertices
                .iter()
                .filter(|v| v.uv2.x < 0.05 && v.position.y < 0.15)
                .map(|v| (v.position.x.powi(2) + v.position.z.powi(2)).sqrt())
                .fold(0.0f32, f32::max)
        };
        let column = base_radius(&layered_trunk_species("buttress = 0.0"));
        let normal = base_radius(&layered_trunk_species(""));
        let heavy = base_radius(&layered_trunk_species("buttress = 2.5"));
        assert!(
            column < normal && normal < heavy,
            "{column} < {normal} < {heavy}"
        );
        assert!(
            column < 0.5 * 1.08,
            "buttress = 0 should leave a plain column: {column}"
        );
    }
}
