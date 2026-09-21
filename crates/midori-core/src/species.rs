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
    /// Generator family and family-specific routing metadata
    #[serde(default)]
    pub generator: GeneratorConfig,
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
    /// Material placeholder names and notes.
    ///
    /// This is intentionally metadata only. Texture/PBR asset generation is a
    /// separate parked workflow.
    #[serde(default)]
    pub materials: MaterialPlaceholders,
    /// Optional editor control groups for mapping friendly UI controls to raw parameters.
    #[serde(default)]
    pub control_groups: Vec<ControlGroup>,
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
    /// Latin/binomial name alias for workflows that prefer `latin`.
    #[serde(default)]
    pub latin: String,
    /// Broad biome label such as `temperate`, `desert`, or `tropical`.
    #[serde(default)]
    pub biome: String,
    /// Search/classification tags for editor filtering and future preset tooling.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Generator family selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneratorFamily {
    /// Current branch/trunk/leaf tree path.
    WeberPenn,
    /// Planned fork grammar path for rosette plants and other dichotomous forms.
    Dichotomous,
    /// Reserved for explicit cactus/ribbed-column implementations.
    Cactus,
    /// Reserved for pad/segment chain plants.
    PadChain,
    /// Explicit custom family marker for external tooling.
    Custom,
}

/// Generator routing metadata.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GeneratorConfig {
    /// Family of generator used for this species.
    #[serde(default = "default_generator_family")]
    pub family: GeneratorFamily,
    /// Optional note explaining why this family was selected.
    #[serde(default)]
    pub notes: String,
}

/// Placeholder material identifiers.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MaterialPlaceholders {
    /// Bark/stem material placeholder name.
    #[serde(default)]
    pub bark: String,
    /// Leaf/foliage material placeholder name.
    #[serde(default)]
    pub foliage: String,
    /// Human notes for future material work. No texture/PBR pipeline behavior.
    #[serde(default)]
    pub notes: String,
}

/// Editor control group metadata.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ControlGroup {
    /// Stable group key.
    #[serde(default)]
    pub key: String,
    /// User-facing group label.
    #[serde(default)]
    pub label: String,
    /// Controls in this group.
    #[serde(default)]
    pub controls: Vec<ControlSpec>,
}

/// Editor control metadata mapping a friendly control to a raw parameter path.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ControlSpec {
    /// Stable control key.
    #[serde(default)]
    pub key: String,
    /// User-facing label.
    #[serde(default)]
    pub label: String,
    /// Dotted path to the underlying species parameter.
    #[serde(default)]
    pub parameter: String,
    /// Minimum numeric value when applicable.
    #[serde(default)]
    pub min: Option<f32>,
    /// Maximum numeric value when applicable.
    #[serde(default)]
    pub max: Option<f32>,
    /// Step size when applicable.
    #[serde(default)]
    pub step: Option<f32>,
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
    /// Profile used to apply taper along trunk height.
    #[serde(default = "default_trunk_taper_profile")]
    pub taper_profile: TaperProfile,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level1: Option<BranchParams>,
    /// Secondary branches
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level2: Option<BranchParams>,
    /// Tertiary branches (twigs)
    #[serde(skip_serializing_if = "Option::is_none")]
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
    /// Child radius model: direct ratio or pipe-model sibling split.
    #[serde(default)]
    pub radius_model: BranchRadiusModel,
    /// Exponent used by pipe-model radius splitting.
    #[serde(default = "default_pipe_exponent")]
    pub pipe_exponent: f32,
    /// Branch taper factor (0.0 = cylinder, 1.0 = taper to minimum radius)
    #[serde(default = "default_branch_taper")]
    pub taper: f32,
    /// Profile used to apply taper along branch length.
    #[serde(default = "default_branch_taper_profile")]
    pub taper_profile: TaperProfile,
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

/// Child branch radius calculation model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchRadiusModel {
    /// Child radius is parent radius multiplied by `radius_ratio`.
    #[default]
    Ratio,
    /// Child radii are split across siblings using a pipe-model exponent.
    Pipe,
}

/// Radius taper curve along a trunk or branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaperProfile {
    /// Interpolate directly from base to tip radius.
    #[default]
    Linear,
    /// Smoothstep curve, retaining more thickness near the base and tip.
    Smooth,
    /// Exponential curve for fast early narrowing with a controlled tip ratio.
    Exponential,
    /// Legacy per-segment multiplicative taper, kept for branch compatibility.
    Compound,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeafGeometry {
    /// Full polygon mesh leaves
    Polygon,
    /// Two crossed billboard quads
    #[default]
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
    #[serde(alias = "lobed")]
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
    /// Influence of upward direction on leaf orientation (-1.0 droop - 1.0 upright)
    #[serde(default)]
    pub up_influence: f32,
}

/// Bark relief style for generated bark maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BarkStyle {
    /// Deep vertical furrows and ridges (oak, ash)
    #[default]
    Furrowed,
    /// Scaly plated bark (pine, spruce)
    Plated,
    /// Smooth bark with lenticel dashes (beech, birch, palm)
    Smooth,
}

/// Leaf card layout — how leaf silhouettes are arranged on the card texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeafCardLayout {
    /// A single leaf filling the card. Best for `polygon` leaf geometry.
    Single,
    /// A spray of several leaves — gives billboard quads foliage volume.
    #[default]
    Cluster,
}

/// Texture material slots and generation parameters.
///
/// Midori can procedurally generate bark albedo+normal and a leaf albedo+alpha
/// card from these parameters (deterministic, license-clean), or consume
/// host-provided image files via the slot paths — the same slots the project
/// sidecar fills when maps arrive over the studio tool bus.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TextureParams {
    /// Prompt for bark texture generation (authoring hint for art pipelines)
    #[serde(default)]
    pub bark_prompt: String,
    /// Prompt for leaf texture generation
    #[serde(default)]
    pub leaf_prompt: String,
    /// Generated map resolution in pixels (square), default 512
    #[serde(default = "default_texture_resolution")]
    pub resolution: u32,
    /// Seed for procedural maps; when unset, derived from the species name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Bark relief style for generated maps
    #[serde(default)]
    pub bark_style: BarkStyle,
    /// Leaf silhouette stamped into generated cards
    #[serde(default)]
    pub leaf_shape: LeafShape,
    /// Leaf card layout: single leaf or cluster spray
    #[serde(default)]
    pub leaf_card: LeafCardLayout,
    /// Base bark color as sRGB 0-1 triple; default is a mid brown
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bark_color: Option<[f32; 3]>,
    /// Base leaf color as sRGB 0-1 triple; default is a mid green
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leaf_color: Option<[f32; 3]>,
    /// Optional bark albedo+alpha override, resolved relative to the species
    /// document by native hosts (empty = generate procedurally)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bark_albedo: String,
    /// Optional bark normal override (OpenGL +Y, glTF convention)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bark_normal: String,
    /// Optional leaf albedo+alpha card override
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub leaf_albedo_alpha: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
fn default_trunk_taper_profile() -> TaperProfile {
    TaperProfile::Linear
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
fn default_pipe_exponent() -> f32 {
    2.0
}
fn default_branch_taper() -> f32 {
    0.7
}
fn default_branch_taper_profile() -> TaperProfile {
    TaperProfile::Compound
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
fn default_generator_family() -> GeneratorFamily {
    GeneratorFamily::WeberPenn
}

fn default_texture_resolution() -> u32 {
    512
}

impl Default for TextureParams {
    /// Matches the per-field serde defaults so an absent `[textures]` section
    /// resolves to the documented values (512px maps, procedural slots).
    fn default() -> Self {
        Self {
            bark_prompt: String::new(),
            leaf_prompt: String::new(),
            resolution: default_texture_resolution(),
            seed: None,
            bark_style: BarkStyle::default(),
            leaf_shape: LeafShape::default(),
            leaf_card: LeafCardLayout::default(),
            bark_color: None,
            leaf_color: None,
            bark_albedo: String::new(),
            bark_normal: String::new(),
            leaf_albedo_alpha: String::new(),
        }
    }
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

impl Default for GeneratorFamily {
    fn default() -> Self {
        default_generator_family()
    }
}

/// A single field-level validation failure on a species document.
///
/// `field` is the dotted TOML path (e.g. `trunk.radius`,
/// `branches.level2.length`) so UIs and structured error consumers can map
/// the failure back to the offending control.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SpeciesFieldError {
    /// Dotted path to the offending parameter.
    pub field: String,
    /// Human-readable description of what is wrong.
    pub message: String,
}

/// Error type for species loading operations.
#[derive(Debug)]
pub enum SpeciesError {
    /// IO error reading file
    Io(std::io::Error),
    /// TOML parsing error
    Parse(toml::de::Error),
    /// Semantically invalid values (non-finite floats, out-of-range
    /// parameters, non-positive dimensions). Carries every violation found.
    Validation(Vec<SpeciesFieldError>),
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
            SpeciesError::Io(e) => write!(f, "IO error: {e}"),
            SpeciesError::Parse(e) => write!(f, "Parse error: {e}"),
            SpeciesError::Validation(errors) => {
                writeln!(f, "Invalid species ({} problem(s)):", errors.len())?;
                for err in errors {
                    writeln!(f, "  {}: {}", err.field, err.message)?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for SpeciesError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SpeciesError::Io(e) => Some(e),
            SpeciesError::Parse(e) => Some(e),
            SpeciesError::Validation(_) => None,
        }
    }
}

impl Species {
    /// Load species from TOML string.
    ///
    /// # Example
    ///
    /// ```
    /// use midori_core::species::Species;
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
    ///
    /// Runs [`Species::validate`] after parsing, so semantically invalid
    /// values (non-positive dimensions, out-of-range variances, NaN/inf)
    /// are rejected here rather than producing degenerate geometry later.
    pub fn from_toml(toml_str: &str) -> Result<Self, SpeciesError> {
        let species: Self = toml::from_str(toml_str)?;
        species.validate()?;
        Ok(species)
    }

    /// Validate the species' semantic constraints.
    ///
    /// TOML parsing only enforces types; this pass rejects values that would
    /// silently produce degenerate output or runaway generation: non-positive
    /// heights/radii/lengths, zero segments, variance factors outside their
    /// documented 0..=1 range, crown offset/gravity outside their ranges, and
    /// NaN or infinite floats. Collects every violation before returning.
    pub fn validate(&self) -> Result<(), SpeciesError> {
        let mut errors = Vec::new();
        let mut check = |field: &str, rule: &str, ok: bool| {
            if !ok {
                errors.push(SpeciesFieldError {
                    field: field.to_string(),
                    message: rule.to_string(),
                });
            }
        };
        let finite = |v: f32| v.is_finite();
        let positive = |v: f32| v.is_finite() && v > 0.0;
        let nonnegative = |v: f32| v.is_finite() && v >= 0.0;
        let unit = |v: f32| v.is_finite() && (0.0..=1.0).contains(&v);
        let signed_unit = |v: f32| v.is_finite() && (-1.0..=1.0).contains(&v);

        check(
            "species.name",
            "must not be empty",
            !self.species.name.trim().is_empty(),
        );

        check(
            "trunk.height",
            "must be positive and finite",
            positive(self.trunk.height),
        );
        check(
            "trunk.height_variance",
            "must be in 0..=1",
            unit(self.trunk.height_variance),
        );
        check(
            "trunk.radius",
            "must be positive and finite",
            positive(self.trunk.radius),
        );
        check(
            "trunk.taper",
            "must be non-negative and finite",
            nonnegative(self.trunk.taper),
        );
        check("trunk.curve", "must be finite", finite(self.trunk.curve));
        check(
            "trunk.curve_variance",
            "must be finite",
            finite(self.trunk.curve_variance),
        );
        check(
            "trunk.curve_back",
            "must be finite",
            finite(self.trunk.curve_back),
        );
        check(
            "trunk.segments",
            "must be at least 1",
            self.trunk.segments > 0,
        );

        for (level, params) in [
            (1u32, self.branches.level1.as_ref()),
            (2, self.branches.level2.as_ref()),
            (3, self.branches.level3.as_ref()),
        ] {
            let Some(p) = params else { continue };
            let prefix = format!("branches.level{level}");
            check(
                &format!("{prefix}.length"),
                "must be positive and finite",
                positive(p.length),
            );
            check(
                &format!("{prefix}.length_variance"),
                "must be in 0..=1",
                unit(p.length_variance),
            );
            check(
                &format!("{prefix}.radius_ratio"),
                "must be positive and finite",
                positive(p.radius_ratio),
            );
            check(
                &format!("{prefix}.pipe_exponent"),
                "must be positive and finite",
                positive(p.pipe_exponent),
            );
            check(
                &format!("{prefix}.taper"),
                "must be non-negative and finite",
                nonnegative(p.taper),
            );
            check(
                &format!("{prefix}.angle"),
                "must be finite",
                finite(p.angle),
            );
            check(
                &format!("{prefix}.angle_variance"),
                "must be finite",
                finite(p.angle_variance),
            );
            check(
                &format!("{prefix}.rotation"),
                "must be finite",
                finite(p.rotation),
            );
            check(
                &format!("{prefix}.gravity"),
                "must be in -1..=1",
                signed_unit(p.gravity),
            );
            check(
                &format!("{prefix}.curve"),
                "must be finite",
                finite(p.curve),
            );
            check(
                &format!("{prefix}.curve_variance"),
                "must be finite",
                finite(p.curve_variance),
            );
            check(
                &format!("{prefix}.count"),
                "level with count 0 contributes nothing; remove it or raise the count",
                p.count > 0 || p.count_variance > 0,
            );
            check(
                &format!("{prefix}.segments"),
                "must be at least 1",
                p.segments > 0,
            );
        }

        check("crown.offset", "must be in 0..=1", unit(self.crown.offset));
        check(
            "crown.density",
            "must be positive and finite",
            positive(self.crown.density),
        );
        check(
            "crown.width_ratio",
            "must be positive and finite",
            positive(self.crown.width_ratio),
        );

        check(
            "leaves.size",
            "must be positive and finite",
            positive(self.leaves.size),
        );
        check(
            "leaves.size_variance",
            "must be in 0..=1",
            unit(self.leaves.size_variance),
        );
        check(
            "leaves.up_influence",
            "must be in -1..=1",
            signed_unit(self.leaves.up_influence),
        );
        check(
            "leaves.min_level",
            "out of supported range",
            self.leaves.min_level <= crate::constants::MAX_BRANCH_LEVELS,
        );

        check(
            "textures.resolution",
            "must be in 1..=8192",
            (1..=8192).contains(&self.textures.resolution),
        );
        for (field, color) in [
            ("textures.bark_color", self.textures.bark_color),
            ("textures.leaf_color", self.textures.leaf_color),
        ] {
            if let Some(c) = color {
                for (i, v) in c.iter().enumerate() {
                    check(&format!("{field}[{i}]"), "must be in 0..=1", unit(*v));
                }
            }
        }

        if let Some(count) = self.lod.count {
            check("lod.count", "must be at least 1", count > 0);
        }
        for (i, level) in self.lod.levels.iter().enumerate() {
            let prefix = format!("lod.levels[{i}]");
            check(
                &format!("{prefix}.leaf_reduction"),
                "must be in 0..=1",
                unit(level.leaf_reduction),
            );
            check(
                &format!("{prefix}.screen_height"),
                "must be non-negative and finite",
                nonnegative(level.screen_height),
            );
            check(
                &format!("{prefix}.branch_levels"),
                "must be at least 1",
                level.branch_levels > 0,
            );
            if let Some(ring) = level.ring_resolution {
                check(
                    &format!("{prefix}.ring_resolution"),
                    "entries must be at least 1",
                    ring.iter().all(|r| *r > 0),
                );
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(SpeciesError::Validation(errors))
        }
    }

    /// Load species from file path.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use midori_core::species::Species;
    /// use std::path::Path;
    ///
    /// let species = Species::from_file(Path::new("species/oak.toml")).unwrap();
    /// ```
    pub fn from_file(path: &std::path::Path) -> Result<Self, SpeciesError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml(&content)
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

    /// Get the effective Latin/binomial name.
    ///
    /// `species.scientific` is the original field. `species.latin` is an alias
    /// added for richer metadata workflows.
    pub fn latin_name(&self) -> &str {
        if self.species.latin.is_empty() {
            &self.species.scientific
        } else {
            &self.species.latin
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
latin = "Quercus robur"
biome = "temperate"
tags = ["broadleaf", "deciduous"]

[generator]
family = "weber_penn"
notes = "Broadleaf tree path."

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
radius_model = "pipe"
pipe_exponent = 2.0
taper = 0.6
taper_profile = "smooth"
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

[materials]
bark = "oak_bark"
foliage = "oak_leaf"
notes = "Placeholders only; texture/PBR pipeline is parked."

[[control_groups]]
key = "shape"
label = "Shape"

[[control_groups.controls]]
key = "height"
label = "Height"
parameter = "trunk.height"
min = 3.0
max = 20.0
step = 0.1

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
        assert_eq!(species.species.latin, "Quercus robur");
        assert_eq!(species.species.biome, "temperate");
        assert_eq!(species.species.tags, vec!["broadleaf", "deciduous"]);
        assert_eq!(species.latin_name(), "Quercus robur");
        assert_eq!(species.generator.family, GeneratorFamily::WeberPenn);

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
        assert_eq!(level1.radius_model, BranchRadiusModel::Pipe);
        assert_eq!(level1.pipe_exponent, 2.0);
        assert_eq!(level1.taper, 0.6);
        assert_eq!(level1.taper_profile, TaperProfile::Smooth);

        let level2 = species.get_branch_level(2).unwrap();
        assert_eq!(level2.count, 4);
        assert_eq!(level2.radius_model, BranchRadiusModel::Ratio);
        assert_eq!(level2.taper_profile, TaperProfile::Compound);

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

        // Material placeholders and controls
        assert_eq!(species.materials.bark, "oak_bark");
        assert_eq!(species.materials.foliage, "oak_leaf");
        assert_eq!(species.control_groups.len(), 1);
        assert_eq!(
            species.control_groups[0].controls[0].parameter,
            "trunk.height"
        );

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
shape = "{shape}"
"#
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
            Species::from_toml(&toml("hemispherical"))
                .unwrap()
                .crown
                .shape,
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
distribution = "{dist}"
"#
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
geometry = "{geom}"
"#
            )
        };

        assert_eq!(
            Species::from_toml(&toml("polygon"))
                .unwrap()
                .leaves
                .geometry,
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
preset = "{preset}"
"#
            )
        };

        assert_eq!(
            Species::from_toml(&toml("ultra")).unwrap().lod.preset,
            LodPreset::Ultra
        );
        assert_eq!(
            Species::from_toml(&toml("high_quality"))
                .unwrap()
                .lod
                .preset,
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
    fn test_generator_family_metadata() {
        let toml = r#"
[species]
name = "Joshua Prototype"
latin = "Yucca brevifolia"
biome = "desert"
tags = ["rosette", "desert"]

[generator]
family = "dichotomous"

[trunk]
"#;

        let species = Species::from_toml(toml).unwrap();
        assert_eq!(species.generator.family, GeneratorFamily::Dichotomous);
        assert_eq!(species.latin_name(), "Yucca brevifolia");
        assert_eq!(species.species.biome, "desert");
    }

    #[test]
    fn test_invalid_generator_family_errors() {
        let toml = r#"
[species]
name = "Invalid"

[generator]
family = "space_magic"

[trunk]
"#;

        let err = Species::from_toml(toml).unwrap_err().to_string();
        assert!(err.contains("space_magic"));
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
target = "{target}"
"#
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
        assert_eq!(species.trunk.taper_profile, TaperProfile::Linear);
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

        let species_err = Species::from_toml("invalid").unwrap_err();
        assert!(matches!(species_err, SpeciesError::Parse(_)));
        assert!(species_err.to_string().contains("Parse error"));
    }

    #[test]
    fn test_validate_rejects_nonpositive_dimensions() {
        let toml = r#"
[species]
name = "Neg2"

[trunk]
height = 0.0
radius = -1.0
segments = 3
"#;
        let err = Species::from_toml(toml).unwrap_err();
        let SpeciesError::Validation(fields) = err else {
            panic!("expected validation error, got {err:?}");
        };
        let paths: Vec<&str> = fields.iter().map(|f| f.field.as_str()).collect();
        assert!(paths.contains(&"trunk.height"));
        assert!(paths.contains(&"trunk.radius"));
    }

    #[test]
    fn test_validate_rejects_out_of_range_and_nonfinite() {
        let toml = r#"
[species]
name = "Bad Ranges"

[trunk]
height = 5.0
height_variance = 1.5
radius = 0.3

[crown]
offset = 2.0

[leaves]
size = nan
up_influence = -1.5
"#;
        let err = Species::from_toml(toml).unwrap_err();
        let SpeciesError::Validation(fields) = err else {
            panic!("expected validation error, got {err:?}");
        };
        let paths: Vec<&str> = fields.iter().map(|f| f.field.as_str()).collect();
        for expected in [
            "trunk.height_variance",
            "crown.offset",
            "leaves.size",
            "leaves.up_influence",
        ] {
            assert!(paths.contains(&expected), "missing {expected} in {paths:?}");
        }
    }

    #[test]
    fn test_validate_rejects_zero_segments_and_lod_violations() {
        let toml = r#"
[species]
name = "ZeroSeg"

[trunk]
height = 5.0
radius = 0.3
segments = 0

[branches.level1]
count = 0
length = 2.0
segments = 4

[[lod.levels]]
index = 0
target_triangles = 1000
leaf_reduction = 1.5
"#;
        let err = Species::from_toml(toml).unwrap_err();
        let SpeciesError::Validation(fields) = err else {
            panic!("expected validation error, got {err:?}");
        };
        let paths: Vec<&str> = fields.iter().map(|f| f.field.as_str()).collect();
        for expected in [
            "trunk.segments",
            "branches.level1.count",
            "lod.levels[0].leaf_reduction",
        ] {
            assert!(paths.contains(&expected), "missing {expected} in {paths:?}");
        }
    }
}
