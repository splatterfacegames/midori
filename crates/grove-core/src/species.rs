//! Species parameter definitions for procedural tree generation.
//!
//! This module provides Serde-deserializable structures for loading tree species
//! definitions from TOML files. Species files define all parameters needed to
//! generate a specific type of tree, including trunk, branches, crown, leaves,
//! textures, LOD configuration, and platform targeting.

use serde::{Deserialize, Serialize};

/// Complete species definition loaded from TOML.
///
/// A species defines all parameters needed to procedurally generate a specific
/// type of tree. This includes physical structure (trunk, branches, crown),
/// visual elements (leaves, textures), and optimization settings (LOD, platform).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Species {
    /// Basic species identification
    pub species: SpeciesInfo,
    /// Trunk geometry parameters
    pub trunk: TrunkParams,
    /// Branch parameters for each level
    #[serde(default)]
    pub branches: BranchLevels,
    /// Crown shape and density
    #[serde(default)]
    pub crown: CrownParams,
    /// Leaf rendering parameters
    #[serde(default)]
    pub leaves: LeafParams,
    /// AI texture generation prompts
    #[serde(default)]
    pub textures: TextureParams,
    /// Level of detail configuration
    #[serde(default)]
    pub lod: LodConfig,
    /// Target platform optimization
    #[serde(default)]
    pub platform: PlatformConfig,
}

/// Basic species identification information.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpeciesInfo {
    /// Common name of the tree species (e.g., "Oak")
    pub name: String,
    /// Scientific name (e.g., "Quercus robur")
    #[serde(default)]
    pub scientific: String,
}

/// Trunk geometry parameters.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TrunkParams {
    /// Base height of the trunk in meters
    #[serde(default = "default_trunk_height")]
    pub height: f32,
    /// Random variance in height (0.0 - 1.0)
    #[serde(default)]
    pub height_variance: f32,
    /// Base radius of the trunk at ground level in meters
    #[serde(default = "default_trunk_radius")]
    pub radius: f32,
    /// Taper factor (0.0 = cylinder, 1.0 = cone)
    #[serde(default = "default_taper")]
    pub taper: f32,
    /// Curvature of the trunk in degrees
    #[serde(default)]
    pub curve: f32,
    /// Random variance in curvature
    #[serde(default)]
    pub curve_variance: f32,
    /// Back-curve amount (S-curve effect)
    #[serde(default)]
    pub curve_back: f32,
    /// Number of segments along trunk height
    #[serde(default = "default_segments")]
    pub segments: u32,
}

/// Container for branch parameters at different levels.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct BranchLevels {
    /// Primary branches off the trunk
    pub level1: Option<BranchParams>,
    /// Secondary branches
    pub level2: Option<BranchParams>,
    /// Tertiary branches (twigs)
    pub level3: Option<BranchParams>,
}

/// Branch geometry and distribution parameters.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BranchParams {
    /// Number of branches at this level
    #[serde(default = "default_branch_count")]
    pub count: u32,
    /// Random variance in branch count
    #[serde(default)]
    pub count_variance: u32,
    /// Length of branches in meters
    #[serde(default = "default_branch_length")]
    pub length: f32,
    /// Random variance in length (0.0 - 1.0)
    #[serde(default)]
    pub length_variance: f32,
    /// Ratio of branch radius to parent radius
    #[serde(default = "default_radius_ratio")]
    pub radius_ratio: f32,
    /// Angle from parent branch in degrees
    #[serde(default = "default_branch_angle")]
    pub angle: f32,
    /// Random variance in angle
    #[serde(default)]
    pub angle_variance: f32,
    /// Rotation around parent (golden angle = 137.5)
    #[serde(default = "default_rotation")]
    pub rotation: f32,
    /// Gravity influence (-1.0 to 1.0, negative = droop)
    #[serde(default)]
    pub gravity: f32,
    /// Curvature of the branch in degrees
    #[serde(default)]
    pub curve: f32,
    /// Random variance in curvature
    #[serde(default)]
    pub curve_variance: f32,
    /// Number of segments along branch length
    #[serde(default = "default_branch_segments")]
    pub segments: u32,
}

/// Crown shape enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrownShape {
    /// Round, ball-shaped crown (oak, maple)
    Spherical,
    /// Cone-shaped crown (pine, spruce)
    Conical,
    /// Half-sphere crown
    Hemispherical,
    /// Flame or teardrop shape (cypress, poplar)
    Flame,
    /// Tall, narrow column shape
    Columnar,
}

/// Crown shape and density parameters.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CrownParams {
    /// Overall crown shape
    #[serde(default = "default_crown_shape")]
    pub shape: CrownShape,
    /// Vertical offset of crown center (0.0 - 1.0 of tree height)
    #[serde(default = "default_crown_offset")]
    pub offset: f32,
    /// Leaf/branch density multiplier
    #[serde(default = "default_one")]
    pub density: f32,
    /// Width to height ratio of crown
    #[serde(default = "default_one")]
    pub width_ratio: f32,
}

/// Leaf distribution pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeafDistribution {
    /// Leaves only at branch endpoints
    Endpoint,
    /// Leaves distributed along branches
    AlongBranch,
    /// Both endpoint and along branches
    Both,
}

/// Leaf rendering geometry type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeafGeometry {
    /// Full polygon mesh leaves
    Polygon,
    /// Two crossed billboard quads
    CrossBillboard,
    /// Single camera-facing billboard
    Billboard,
    /// No leaves rendered
    None,
}

/// Leaf shape for polygon generation using SDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeafShape {
    /// Elliptical leaf shape (common in many trees)
    Oval,
    /// Egg-shaped leaf, narrower at tip (default)
    #[default]
    Pointed,
    /// Long thin needle (pine, spruce, fir)
    Needle,
    /// Oak-style with 7 rounded lobes
    OakLobed,
    /// 5-point maple leaf
    Maple,
    /// Serrated oval (birch, elm)
    Serrated,
    /// Long narrow willow-style leaf
    Willow,
    /// Heart-shaped leaf (linden, redbud)
    Heart,
    /// Compound palmate (horse chestnut style)
    Palmate,
}

impl LeafShape {
    /// Get the recommended polygon resolution for this shape
    pub fn recommended_resolution(&self) -> u32 {
        match self {
            LeafShape::Oval | LeafShape::Pointed | LeafShape::Heart => 12,
            LeafShape::Needle | LeafShape::Willow => 8,
            LeafShape::Serrated => 24,
            LeafShape::OakLobed => 20,
            LeafShape::Maple => 30,
            LeafShape::Palmate => 36,
        }
    }
}

/// Leaf rendering parameters.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LeafParams {
    /// Total number of leaves
    #[serde(default = "default_leaf_count")]
    pub count: u32,
    /// Minimum branch level for leaves (0 = trunk)
    #[serde(default = "default_min_level")]
    pub min_level: u32,
    /// Base size of leaves in meters
    #[serde(default = "default_leaf_size")]
    pub size: f32,
    /// Random variance in size (0.0 - 1.0)
    #[serde(default)]
    pub size_variance: f32,
    /// Where leaves are placed
    #[serde(default = "default_distribution")]
    pub distribution: LeafDistribution,
    /// Rendering geometry type
    #[serde(default = "default_leaf_geometry")]
    pub geometry: LeafGeometry,
    /// Leaf shape for polygon generation
    #[serde(default)]
    pub shape: LeafShape,
    /// Influence of upward direction on leaf orientation (0.0 - 1.0)
    #[serde(default)]
    pub up_influence: f32,
}

/// AI texture generation parameters.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TextureParams {
    /// Prompt for bark texture generation
    #[serde(default)]
    pub bark_prompt: String,
    /// Prompt for leaf texture generation
    #[serde(default)]
    pub leaf_prompt: String,
}

/// LOD preset enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LodPreset {
    /// Maximum quality, many LOD levels
    Ultra,
    /// High quality for modern PCs
    HighQuality,
    /// Balanced quality and performance
    Balanced,
    /// Optimized for mobile devices
    Mobile,
    /// Minimal LOD for low-end devices
    Minimal,
    /// Optimized for open-world forests (10-50 trees on screen)
    OpenWorld,
    /// High detail for single prominent/hero trees
    HeroTree,
    /// Custom LOD configuration
    Custom,
}

/// Level of detail configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LodConfig {
    /// LOD preset to use
    #[serde(default = "default_lod_preset")]
    pub preset: LodPreset,
    /// Override number of LOD levels
    #[serde(default)]
    pub count: Option<u32>,
    /// Custom LOD level definitions
    #[serde(default)]
    pub levels: Vec<LodLevel>,
}

/// Individual LOD level definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LodLevel {
    /// LOD index (0 = highest quality)
    pub index: u32,
    /// Optional name for this level
    #[serde(default)]
    pub name: String,
    /// Target triangle count
    pub target_triangles: u32,
    /// Maximum allowed triangles
    #[serde(default)]
    pub max_triangles: Option<u32>,
    /// Number of branch levels to include
    #[serde(default = "default_branch_levels")]
    pub branch_levels: u32,
    /// Leaf rendering mode for this LOD
    #[serde(default = "default_leaf_geometry")]
    pub leaf_geometry: LeafGeometry,
    /// Leaf count reduction factor (0.0 - 1.0)
    #[serde(default = "default_one")]
    pub leaf_reduction: f32,
    /// Ring resolution for trunk/branches [trunk, level1, level2, level3]
    #[serde(default)]
    pub ring_resolution: Option<[u32; 4]>,
    /// Screen height threshold for LOD switching
    #[serde(default)]
    pub screen_height: f32,
    /// Whether to use crown impostor at this LOD
    #[serde(default)]
    pub crown_impostor: bool,
}

/// Target platform for optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformTarget {
    /// Modern PC with high-end GPU
    ModernPc,
    /// Mobile phones and tablets
    Mobile,
    /// Nintendo Switch
    Switch,
    /// Meta Quest VR headsets
    Quest,
    /// WebGL/WebGPU browsers
    Web,
    /// Universal compatibility
    Universal,
}

/// Platform-specific optimization configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlatformConfig {
    /// Target platform
    #[serde(default = "default_platform")]
    pub target: PlatformTarget,
}

// Default value functions
fn default_trunk_height() -> f32 {
    8.0
}
fn default_trunk_radius() -> f32 {
    0.45
}
fn default_taper() -> f32 {
    0.75
}
fn default_segments() -> u32 {
    8
}
fn default_branch_count() -> u32 {
    5
}
fn default_branch_length() -> f32 {
    3.0
}
fn default_radius_ratio() -> f32 {
    0.5
}
fn default_branch_angle() -> f32 {
    45.0
}
fn default_rotation() -> f32 {
    137.5 // Golden angle
}
fn default_branch_segments() -> u32 {
    4
}
fn default_crown_shape() -> CrownShape {
    CrownShape::Spherical
}
fn default_crown_offset() -> f32 {
    0.35
}
fn default_one() -> f32 {
    1.0
}
fn default_leaf_count() -> u32 {
    2000
}
fn default_min_level() -> u32 {
    2
}
fn default_leaf_size() -> f32 {
    0.12
}
fn default_distribution() -> LeafDistribution {
    LeafDistribution::Both
}
fn default_leaf_geometry() -> LeafGeometry {
    LeafGeometry::CrossBillboard
}
fn default_lod_preset() -> LodPreset {
    LodPreset::Balanced
}
fn default_branch_levels() -> u32 {
    4
}
fn default_platform() -> PlatformTarget {
    PlatformTarget::ModernPc
}

// Default implementations
impl Default for CrownParams {
    fn default() -> Self {
        Self {
            shape: default_crown_shape(),
            offset: default_crown_offset(),
            density: default_one(),
            width_ratio: default_one(),
        }
    }
}

impl Default for LeafParams {
    fn default() -> Self {
        Self {
            count: default_leaf_count(),
            min_level: default_min_level(),
            size: default_leaf_size(),
            size_variance: 0.0,
            distribution: default_distribution(),
            geometry: default_leaf_geometry(),
            shape: LeafShape::default(),
            up_influence: 0.0,
        }
    }
}

impl Default for LodConfig {
    fn default() -> Self {
        Self {
            preset: default_lod_preset(),
            count: None,
            levels: Vec::new(),
        }
    }
}

impl Default for PlatformConfig {
    fn default() -> Self {
        Self {
            target: default_platform(),
        }
    }
}

impl Default for LeafGeometry {
    fn default() -> Self {
        LeafGeometry::CrossBillboard
    }
}

/// Error type for species loading operations.
#[derive(Debug)]
pub enum SpeciesError {
    /// IO error reading file
    Io(std::io::Error),
    /// TOML parsing error
    Parse(toml::de::Error),
}

impl From<std::io::Error> for SpeciesError {
    fn from(err: std::io::Error) -> Self {
        SpeciesError::Io(err)
    }
}

impl From<toml::de::Error> for SpeciesError {
    fn from(err: toml::de::Error) -> Self {
        SpeciesError::Parse(err)
    }
}

impl std::fmt::Display for SpeciesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpeciesError::Io(e) => write!(f, "IO error: {}", e),
            SpeciesError::Parse(e) => write!(f, "Parse error: {}", e),
        }
    }
}

impl std::error::Error for SpeciesError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SpeciesError::Io(e) => Some(e),
            SpeciesError::Parse(e) => Some(e),
        }
    }
}

impl Species {
    /// Load species from TOML string.
    ///
    /// # Example
    ///
    /// ```
    /// use grove_core::species::Species;
    ///
    /// let toml = r#"
    /// [species]
    /// name = "Oak"
    /// scientific = "Quercus robur"
    ///
    /// [trunk]
    /// height = 6.0
    /// radius = 0.45
    /// "#;
    ///
    /// let species = Species::from_toml(toml).unwrap();
    /// assert_eq!(species.species.name, "Oak");
    /// ```
    pub fn from_toml(toml_str: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(toml_str)
    }

    /// Load species from file path.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use grove_core::species::Species;
    /// use std::path::Path;
    ///
    /// let species = Species::from_file(Path::new("species/oak.toml")).unwrap();
    /// ```
    pub fn from_file(path: &std::path::Path) -> Result<Self, SpeciesError> {
        let content = std::fs::read_to_string(path)?;
        Ok(Self::from_toml(&content)?)
    }

    /// Get branch params for a level (0 = trunk, 1-3 = branches).
    ///
    /// Returns `None` for level 0 (trunk) or if the level is not defined.
    pub fn get_branch_level(&self, level: u32) -> Option<&BranchParams> {
        match level {
            1 => self.branches.level1.as_ref(),
            2 => self.branches.level2.as_ref(),
            3 => self.branches.level3.as_ref(),
            _ => None,
        }
    }

    /// Get effective LOD configuration based on preset or custom levels.
    ///
    /// If custom levels are defined, returns those. Otherwise generates
    /// appropriate levels based on the selected preset.
    pub fn get_lod_levels(&self) -> Vec<LodLevel> {
        if !self.lod.levels.is_empty() {
            return self.lod.levels.clone();
        }
        // Generate from preset
        match self.lod.preset {
            LodPreset::Ultra => generate_ultra_lods(),
            LodPreset::HighQuality => generate_high_quality_lods(),
            LodPreset::Balanced => generate_balanced_lods(),
            LodPreset::Mobile => generate_mobile_lods(),
            LodPreset::Minimal => generate_minimal_lods(),
            LodPreset::OpenWorld => generate_open_world_lods(),
            LodPreset::HeroTree => generate_hero_tree_lods(),
            LodPreset::Custom => Vec::new(), // Custom but no levels defined
        }
    }
}

/// Generate ultra quality LOD levels.
fn generate_ultra_lods() -> Vec<LodLevel> {
    vec![
        LodLevel {
            index: 0,
            name: "Ultra".to_string(),
            target_triangles: 50000,
            max_triangles: Some(75000),
            branch_levels: 4,
            leaf_geometry: LeafGeometry::Polygon,
            leaf_reduction: 1.0,
            ring_resolution: Some([16, 12, 8, 6]),
            screen_height: 0.5,
            crown_impostor: false,
        },
        LodLevel {
            index: 1,
            name: "High".to_string(),
            target_triangles: 25000,
            max_triangles: Some(35000),
            branch_levels: 4,
            leaf_geometry: LeafGeometry::CrossBillboard,
            leaf_reduction: 0.8,
            ring_resolution: Some([12, 8, 6, 4]),
            screen_height: 0.25,
            crown_impostor: false,
        },
        LodLevel {
            index: 2,
            name: "Medium".to_string(),
            target_triangles: 10000,
            max_triangles: Some(15000),
            branch_levels: 3,
            leaf_geometry: LeafGeometry::CrossBillboard,
            leaf_reduction: 0.5,
            ring_resolution: Some([8, 6, 4, 3]),
            screen_height: 0.1,
            crown_impostor: false,
        },
        LodLevel {
            index: 3,
            name: "Low".to_string(),
            target_triangles: 3000,
            max_triangles: Some(5000),
            branch_levels: 2,
            leaf_geometry: LeafGeometry::Billboard,
            leaf_reduction: 0.25,
            ring_resolution: Some([6, 4, 3, 3]),
            screen_height: 0.05,
            crown_impostor: true,
        },
        LodLevel {
            index: 4,
            name: "Impostor".to_string(),
            target_triangles: 500,
            max_triangles: Some(1000),
            branch_levels: 1,
            leaf_geometry: LeafGeometry::None,
            leaf_reduction: 0.0,
            ring_resolution: Some([4, 3, 3, 3]),
            screen_height: 0.02,
            crown_impostor: true,
        },
    ]
}

/// Generate high quality LOD levels.
fn generate_high_quality_lods() -> Vec<LodLevel> {
    vec![
        LodLevel {
            index: 0,
            name: "High".to_string(),
            target_triangles: 30000,
            max_triangles: Some(45000),
            branch_levels: 4,
            leaf_geometry: LeafGeometry::CrossBillboard,
            leaf_reduction: 1.0,
            ring_resolution: Some([12, 8, 6, 4]),
            screen_height: 0.4,
            crown_impostor: false,
        },
        LodLevel {
            index: 1,
            name: "Medium".to_string(),
            target_triangles: 12000,
            max_triangles: Some(18000),
            branch_levels: 3,
            leaf_geometry: LeafGeometry::CrossBillboard,
            leaf_reduction: 0.6,
            ring_resolution: Some([8, 6, 4, 3]),
            screen_height: 0.15,
            crown_impostor: false,
        },
        LodLevel {
            index: 2,
            name: "Low".to_string(),
            target_triangles: 4000,
            max_triangles: Some(6000),
            branch_levels: 2,
            leaf_geometry: LeafGeometry::Billboard,
            leaf_reduction: 0.3,
            ring_resolution: Some([6, 4, 3, 3]),
            screen_height: 0.05,
            crown_impostor: true,
        },
        LodLevel {
            index: 3,
            name: "Impostor".to_string(),
            target_triangles: 500,
            max_triangles: Some(1000),
            branch_levels: 1,
            leaf_geometry: LeafGeometry::None,
            leaf_reduction: 0.0,
            ring_resolution: Some([4, 3, 3, 3]),
            screen_height: 0.02,
            crown_impostor: true,
        },
    ]
}

/// Generate balanced LOD levels.
fn generate_balanced_lods() -> Vec<LodLevel> {
    vec![
        LodLevel {
            index: 0,
            name: "High".to_string(),
            target_triangles: 20000,
            max_triangles: Some(30000),
            branch_levels: 3,
            leaf_geometry: LeafGeometry::CrossBillboard,
            leaf_reduction: 1.0,
            ring_resolution: Some([8, 6, 4, 3]),
            screen_height: 0.3,
            crown_impostor: false,
        },
        LodLevel {
            index: 1,
            name: "Medium".to_string(),
            target_triangles: 8000,
            max_triangles: Some(12000),
            branch_levels: 2,
            leaf_geometry: LeafGeometry::CrossBillboard,
            leaf_reduction: 0.5,
            ring_resolution: Some([6, 4, 3, 3]),
            screen_height: 0.1,
            crown_impostor: false,
        },
        LodLevel {
            index: 2,
            name: "Low".to_string(),
            target_triangles: 2000,
            max_triangles: Some(4000),
            branch_levels: 1,
            leaf_geometry: LeafGeometry::Billboard,
            leaf_reduction: 0.2,
            ring_resolution: Some([4, 3, 3, 3]),
            screen_height: 0.03,
            crown_impostor: true,
        },
    ]
}

/// Generate mobile-optimized LOD levels.
fn generate_mobile_lods() -> Vec<LodLevel> {
    vec![
        LodLevel {
            index: 0,
            name: "High".to_string(),
            target_triangles: 8000,
            max_triangles: Some(12000),
            branch_levels: 2,
            leaf_geometry: LeafGeometry::CrossBillboard,
            leaf_reduction: 0.7,
            ring_resolution: Some([6, 4, 3, 3]),
            screen_height: 0.25,
            crown_impostor: false,
        },
        LodLevel {
            index: 1,
            name: "Medium".to_string(),
            target_triangles: 3000,
            max_triangles: Some(5000),
            branch_levels: 2,
            leaf_geometry: LeafGeometry::Billboard,
            leaf_reduction: 0.4,
            ring_resolution: Some([4, 3, 3, 3]),
            screen_height: 0.08,
            crown_impostor: true,
        },
        LodLevel {
            index: 2,
            name: "Low".to_string(),
            target_triangles: 800,
            max_triangles: Some(1500),
            branch_levels: 1,
            leaf_geometry: LeafGeometry::None,
            leaf_reduction: 0.0,
            ring_resolution: Some([3, 3, 3, 3]),
            screen_height: 0.02,
            crown_impostor: true,
        },
    ]
}

/// Generate minimal LOD levels.
fn generate_minimal_lods() -> Vec<LodLevel> {
    vec![
        LodLevel {
            index: 0,
            name: "Main".to_string(),
            target_triangles: 4000,
            max_triangles: Some(6000),
            branch_levels: 2,
            leaf_geometry: LeafGeometry::Billboard,
            leaf_reduction: 0.5,
            ring_resolution: Some([4, 3, 3, 3]),
            screen_height: 0.15,
            crown_impostor: true,
        },
        LodLevel {
            index: 1,
            name: "Low".to_string(),
            target_triangles: 500,
            max_triangles: Some(1000),
            branch_levels: 1,
            leaf_geometry: LeafGeometry::None,
            leaf_reduction: 0.0,
            ring_resolution: Some([3, 3, 3, 3]),
            screen_height: 0.02,
            crown_impostor: true,
        },
    ]
}

/// Generate open-world forest optimized LOD levels.
fn generate_open_world_lods() -> Vec<LodLevel> {
    vec![
        LodLevel {
            index: 0,
            name: "Near".to_string(),
            target_triangles: 8000,
            max_triangles: Some(10000),
            branch_levels: 4,
            leaf_geometry: LeafGeometry::CrossBillboard,
            leaf_reduction: 1.0,
            ring_resolution: Some([20, 14, 8, 5]),
            screen_height: 0.25,
            crown_impostor: false,
        },
        LodLevel {
            index: 1,
            name: "Medium".to_string(),
            target_triangles: 3000,
            max_triangles: Some(4000),
            branch_levels: 3,
            leaf_geometry: LeafGeometry::Billboard,
            leaf_reduction: 0.5,
            ring_resolution: Some([12, 8, 5, 4]),
            screen_height: 0.08,
            crown_impostor: false,
        },
        LodLevel {
            index: 2,
            name: "Far".to_string(),
            target_triangles: 800,
            max_triangles: Some(1200),
            branch_levels: 2,
            leaf_geometry: LeafGeometry::Billboard,
            leaf_reduction: 0.2,
            ring_resolution: Some([8, 5, 4, 3]),
            screen_height: 0.03,
            crown_impostor: false,
        },
        LodLevel {
            index: 3,
            name: "Distant".to_string(),
            target_triangles: 200,
            max_triangles: Some(400),
            branch_levels: 1,
            leaf_geometry: LeafGeometry::None,
            leaf_reduction: 0.0,
            ring_resolution: Some([4, 3, 3, 3]),
            screen_height: 0.01,
            crown_impostor: true,
        },
    ]
}

/// Generate hero tree LOD levels for prominent single trees.
fn generate_hero_tree_lods() -> Vec<LodLevel> {
    vec![
        LodLevel {
            index: 0,
            name: "Hero".to_string(),
            target_triangles: 25000,
            max_triangles: Some(35000),
            branch_levels: 4,
            leaf_geometry: LeafGeometry::Polygon,
            leaf_reduction: 1.0,
            ring_resolution: Some([32, 24, 16, 10]),
            screen_height: 0.4,
            crown_impostor: false,
        },
        LodLevel {
            index: 1,
            name: "Medium".to_string(),
            target_triangles: 12000,
            max_triangles: Some(16000),
            branch_levels: 4,
            leaf_geometry: LeafGeometry::CrossBillboard,
            leaf_reduction: 0.8,
            ring_resolution: Some([24, 16, 10, 6]),
            screen_height: 0.15,
            crown_impostor: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_TOML: &str = r#"
[species]
name = "Test Tree"

[trunk]
height = 5.0
"#;

    const FULL_OAK_TOML: &str = r#"
[species]
name = "Oak"
scientific = "Quercus robur"

[trunk]
height = 6.0
height_variance = 0.15
radius = 0.45
taper = 0.75
curve = 8.0
curve_variance = 4.0
curve_back = 3.0
segments = 8

[branches.level1]
count = 6
count_variance = 2
length = 4.5
length_variance = 0.25
radius_ratio = 0.55
angle = 45.0
angle_variance = 20.0
rotation = 137.5
gravity = -0.15
curve = 25.0
curve_variance = 10.0
segments = 6

[branches.level2]
count = 4
count_variance = 1
length = 2.5
length_variance = 0.3
radius_ratio = 0.5
angle = 50.0
angle_variance = 15.0
rotation = 137.5
gravity = -0.1
curve = 15.0
curve_variance = 8.0
segments = 4

[branches.level3]
count = 3
count_variance = 1
length = 1.0
length_variance = 0.4
radius_ratio = 0.4
angle = 55.0
angle_variance = 10.0
rotation = 137.5
gravity = -0.05
curve = 10.0
curve_variance = 5.0
segments = 2

[crown]
shape = "spherical"
offset = 0.35
density = 1.0
width_ratio = 1.2

[leaves]
count = 3000
min_level = 2
size = 0.12
size_variance = 0.25
distribution = "both"
geometry = "cross_billboard"

[textures]
bark_prompt = "Rough oak bark with deep fissures"
leaf_prompt = "Oak leaf with rounded lobes"

[lod]
preset = "balanced"

[platform]
target = "modern_pc"
"#;

    #[test]
    fn test_parse_minimal_toml() {
        let species = Species::from_toml(MINIMAL_TOML).unwrap();
        assert_eq!(species.species.name, "Test Tree");
        assert_eq!(species.trunk.height, 5.0);
        // Check defaults
        assert_eq!(species.trunk.radius, default_trunk_radius());
        assert_eq!(species.crown.shape, CrownShape::Spherical);
        assert_eq!(species.leaves.count, default_leaf_count());
    }

    #[test]
    fn test_parse_full_oak_toml() {
        let species = Species::from_toml(FULL_OAK_TOML).unwrap();

        // Species info
        assert_eq!(species.species.name, "Oak");
        assert_eq!(species.species.scientific, "Quercus robur");

        // Trunk
        assert_eq!(species.trunk.height, 6.0);
        assert_eq!(species.trunk.height_variance, 0.15);
        assert_eq!(species.trunk.radius, 0.45);
        assert_eq!(species.trunk.segments, 8);

        // Branches
        let level1 = species.get_branch_level(1).unwrap();
        assert_eq!(level1.count, 6);
        assert_eq!(level1.length, 4.5);
        assert_eq!(level1.rotation, 137.5);

        let level2 = species.get_branch_level(2).unwrap();
        assert_eq!(level2.count, 4);

        let level3 = species.get_branch_level(3).unwrap();
        assert_eq!(level3.count, 3);

        // Crown
        assert_eq!(species.crown.shape, CrownShape::Spherical);
        assert_eq!(species.crown.offset, 0.35);
        assert_eq!(species.crown.width_ratio, 1.2);

        // Leaves
        assert_eq!(species.leaves.count, 3000);
        assert_eq!(species.leaves.distribution, LeafDistribution::Both);
        assert_eq!(species.leaves.geometry, LeafGeometry::CrossBillboard);

        // Textures
        assert!(species.textures.bark_prompt.contains("oak bark"));

        // LOD
        assert_eq!(species.lod.preset, LodPreset::Balanced);

        // Platform
        assert_eq!(species.platform.target, PlatformTarget::ModernPc);
    }

    #[test]
    fn test_crown_shapes() {
        let toml = |shape| {
            format!(
                r#"
[species]
name = "Test"
[trunk]
[crown]
shape = "{}"
"#,
                shape
            )
        };

        assert_eq!(
            Species::from_toml(&toml("spherical")).unwrap().crown.shape,
            CrownShape::Spherical
        );
        assert_eq!(
            Species::from_toml(&toml("conical")).unwrap().crown.shape,
            CrownShape::Conical
        );
        assert_eq!(
            Species::from_toml(&toml("hemispherical")).unwrap().crown.shape,
            CrownShape::Hemispherical
        );
        assert_eq!(
            Species::from_toml(&toml("flame")).unwrap().crown.shape,
            CrownShape::Flame
        );
        assert_eq!(
            Species::from_toml(&toml("columnar")).unwrap().crown.shape,
            CrownShape::Columnar
        );
    }

    #[test]
    fn test_leaf_distributions() {
        let toml = |dist| {
            format!(
                r#"
[species]
name = "Test"
[trunk]
[leaves]
distribution = "{}"
"#,
                dist
            )
        };

        assert_eq!(
            Species::from_toml(&toml("endpoint"))
                .unwrap()
                .leaves
                .distribution,
            LeafDistribution::Endpoint
        );
        assert_eq!(
            Species::from_toml(&toml("along_branch"))
                .unwrap()
                .leaves
                .distribution,
            LeafDistribution::AlongBranch
        );
        assert_eq!(
            Species::from_toml(&toml("both"))
                .unwrap()
                .leaves
                .distribution,
            LeafDistribution::Both
        );
    }

    #[test]
    fn test_leaf_geometries() {
        let toml = |geom| {
            format!(
                r#"
[species]
name = "Test"
[trunk]
[leaves]
geometry = "{}"
"#,
                geom
            )
        };

        assert_eq!(
            Species::from_toml(&toml("polygon")).unwrap().leaves.geometry,
            LeafGeometry::Polygon
        );
        assert_eq!(
            Species::from_toml(&toml("cross_billboard"))
                .unwrap()
                .leaves
                .geometry,
            LeafGeometry::CrossBillboard
        );
        assert_eq!(
            Species::from_toml(&toml("billboard"))
                .unwrap()
                .leaves
                .geometry,
            LeafGeometry::Billboard
        );
        assert_eq!(
            Species::from_toml(&toml("none")).unwrap().leaves.geometry,
            LeafGeometry::None
        );
    }

    #[test]
    fn test_lod_presets() {
        let toml = |preset| {
            format!(
                r#"
[species]
name = "Test"
[trunk]
[lod]
preset = "{}"
"#,
                preset
            )
        };

        assert_eq!(
            Species::from_toml(&toml("ultra")).unwrap().lod.preset,
            LodPreset::Ultra
        );
        assert_eq!(
            Species::from_toml(&toml("high_quality")).unwrap().lod.preset,
            LodPreset::HighQuality
        );
        assert_eq!(
            Species::from_toml(&toml("balanced")).unwrap().lod.preset,
            LodPreset::Balanced
        );
        assert_eq!(
            Species::from_toml(&toml("mobile")).unwrap().lod.preset,
            LodPreset::Mobile
        );
        assert_eq!(
            Species::from_toml(&toml("minimal")).unwrap().lod.preset,
            LodPreset::Minimal
        );
    }

    #[test]
    fn test_platform_targets() {
        let toml = |target| {
            format!(
                r#"
[species]
name = "Test"
[trunk]
[platform]
target = "{}"
"#,
                target
            )
        };

        assert_eq!(
            Species::from_toml(&toml("modern_pc"))
                .unwrap()
                .platform
                .target,
            PlatformTarget::ModernPc
        );
        assert_eq!(
            Species::from_toml(&toml("mobile")).unwrap().platform.target,
            PlatformTarget::Mobile
        );
        assert_eq!(
            Species::from_toml(&toml("switch")).unwrap().platform.target,
            PlatformTarget::Switch
        );
        assert_eq!(
            Species::from_toml(&toml("quest")).unwrap().platform.target,
            PlatformTarget::Quest
        );
        assert_eq!(
            Species::from_toml(&toml("web")).unwrap().platform.target,
            PlatformTarget::Web
        );
        assert_eq!(
            Species::from_toml(&toml("universal"))
                .unwrap()
                .platform
                .target,
            PlatformTarget::Universal
        );
    }

    #[test]
    fn test_get_branch_level() {
        let species = Species::from_toml(FULL_OAK_TOML).unwrap();

        // Level 0 (trunk) returns None
        assert!(species.get_branch_level(0).is_none());

        // Levels 1-3 return Some
        assert!(species.get_branch_level(1).is_some());
        assert!(species.get_branch_level(2).is_some());
        assert!(species.get_branch_level(3).is_some());

        // Level 4+ returns None
        assert!(species.get_branch_level(4).is_none());
    }

    #[test]
    fn test_get_lod_levels_from_preset() {
        let species = Species::from_toml(MINIMAL_TOML).unwrap();
        let lods = species.get_lod_levels();

        // Balanced preset should have 3 levels
        assert_eq!(lods.len(), 3);
        assert_eq!(lods[0].index, 0);
        assert_eq!(lods[1].index, 1);
        assert_eq!(lods[2].index, 2);
    }

    #[test]
    fn test_custom_lod_levels() {
        let toml = r#"
[species]
name = "Test"

[trunk]

[lod]
preset = "custom"

[[lod.levels]]
index = 0
name = "Custom High"
target_triangles = 15000
branch_levels = 3

[[lod.levels]]
index = 1
name = "Custom Low"
target_triangles = 2000
branch_levels = 1
crown_impostor = true
"#;

        let species = Species::from_toml(toml).unwrap();
        let lods = species.get_lod_levels();

        assert_eq!(lods.len(), 2);
        assert_eq!(lods[0].name, "Custom High");
        assert_eq!(lods[0].target_triangles, 15000);
        assert_eq!(lods[1].name, "Custom Low");
        assert!(lods[1].crown_impostor);
    }

    #[test]
    fn test_default_values() {
        let toml = r#"
[species]
name = "Defaults Test"

[trunk]
"#;

        let species = Species::from_toml(toml).unwrap();

        // Trunk defaults
        assert_eq!(species.trunk.height, 8.0);
        assert_eq!(species.trunk.radius, 0.45);
        assert_eq!(species.trunk.taper, 0.75);
        assert_eq!(species.trunk.segments, 8);

        // Crown defaults
        assert_eq!(species.crown.shape, CrownShape::Spherical);
        assert_eq!(species.crown.offset, 0.35);
        assert_eq!(species.crown.density, 1.0);

        // Leaf defaults
        assert_eq!(species.leaves.count, 2000);
        assert_eq!(species.leaves.min_level, 2);
        assert_eq!(species.leaves.size, 0.12);
        assert_eq!(species.leaves.geometry, LeafGeometry::CrossBillboard);

        // Platform defaults
        assert_eq!(species.platform.target, PlatformTarget::ModernPc);
    }

    #[test]
    fn test_parse_error_invalid_toml() {
        let invalid = "this is not valid toml {{{{";
        assert!(Species::from_toml(invalid).is_err());
    }

    #[test]
    fn test_parse_error_missing_required() {
        // Missing [species] section
        let missing_species = r#"
[trunk]
height = 5.0
"#;
        assert!(Species::from_toml(missing_species).is_err());

        // Missing name in species
        let missing_name = r#"
[species]
scientific = "Test"
[trunk]
"#;
        assert!(Species::from_toml(missing_name).is_err());
    }

    #[test]
    fn test_species_error_display() {
        let io_err = SpeciesError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file not found",
        ));
        assert!(io_err.to_string().contains("IO error"));

        let parse_err = Species::from_toml("invalid").unwrap_err();
        let species_err = SpeciesError::Parse(parse_err);
        assert!(species_err.to_string().contains("Parse error"));
    }
}
