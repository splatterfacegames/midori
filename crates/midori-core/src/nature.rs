//! Nature patch definitions and deterministic terrain/groundcover sampling.
//!
//! This module is the first core slice of Midori's broader nature mission. It
//! keeps dynamic shader tricks out of the exported asset contract by making soil
//! height, material masks, and groundcover density sampleable and bakeable from
//! Rust.

use crate::Rng;
use crate::export::{ExportConfig, ExportError, ExportFormat, export_mesh};
use crate::math::lerp;
use crate::mesh::{MaterialType, Mesh, Submesh, Vertex};
use glam::{Vec2, Vec3, Vec4};
use image::{ColorType, GrayImage, ImageBuffer, ImageError, Luma, RgbImage, RgbaImage};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

const SCATTER_BINARY_MAGIC: &[u8; 4] = b"MDSI";
const SCATTER_BINARY_VERSION: u32 = 1;
const SCATTER_BINARY_HEADER_BYTES: u32 = 16;
const SCATTER_BINARY_RECORD_STRIDE_BYTES: u32 = 32;
const PACKAGE_VALIDATION_EPSILON: f32 = 0.001;

/// Full deterministic nature patch definition loaded from TOML.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NaturePatch {
    /// Asset-level identification and kind.
    pub asset: NatureAssetInfo,
    /// Tile dimensions, seed, and biome metadata.
    pub patch: PatchParams,
    /// Soil height, wetness, and crack parameters.
    #[serde(default)]
    pub soil: SoilParams,
    /// Grass, moss, and other low groundcover layers.
    #[serde(default)]
    pub groundcover: GroundcoverConfig,
    /// Shared lightweight wind metadata for exported materials.
    #[serde(default)]
    pub wind: WindParams,
    /// Mobile and console budget/profile metadata.
    #[serde(default)]
    pub profiles: NatureProfiles,
}

/// Asset-level metadata.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NatureAssetInfo {
    /// Must be `nature_patch` for this schema.
    pub kind: String,
    /// User-facing asset name.
    pub name: String,
    /// Unit label for dimensions.
    #[serde(default = "default_units")]
    pub units: String,
}

/// Patch-level metadata.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PatchParams {
    /// Square tile side length in world units.
    #[serde(default = "default_patch_size")]
    pub size: f32,
    /// Deterministic seed for all patch fields.
    #[serde(default)]
    pub seed: u64,
    /// Broad biome label.
    #[serde(default)]
    pub biome: String,
    /// Search/classification tags.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Soil and material-mask controls.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SoilParams {
    /// Soil profile label such as `loam`, `sand`, or `clay`.
    #[serde(default = "default_soil_profile")]
    pub profile: String,
    /// Broad mound frequency.
    #[serde(default = "default_mound_scale")]
    pub mound_scale: f32,
    /// Maximum mound height.
    #[serde(default = "default_mound_height")]
    pub mound_height: f32,
    /// Fraction of the tile affected by mound height.
    #[serde(default = "default_one")]
    pub mound_coverage: f32,
    /// Softness of the mound coverage threshold.
    #[serde(default = "default_patch_softness")]
    pub mound_edge: f32,
    /// Fine relief frequency.
    #[serde(default = "default_relief_scale")]
    pub relief_scale: f32,
    /// Fine relief strength.
    #[serde(default = "default_relief_strength")]
    pub relief_strength: f32,
    /// Wetness mask coverage.
    #[serde(default)]
    pub wetness_coverage: f32,
    /// Wetness patch frequency.
    #[serde(default = "default_wetness_scale")]
    pub wetness_scale: f32,
    /// Wetness patch edge softness.
    #[serde(default = "default_wetness_edge")]
    pub wetness_edge: f32,
    /// Dry crack controls.
    #[serde(default)]
    pub cracks: SoilCracks,
}

/// Dry crack mask controls.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SoilCracks {
    /// Enables crack mask generation.
    #[serde(default)]
    pub enabled: bool,
    /// Overall crack mask multiplier.
    #[serde(default = "default_crack_amount")]
    pub amount: f32,
    /// Cellular plate density.
    #[serde(default = "default_crack_plate_density")]
    pub plate_density: f32,
    /// Channel width between plates.
    #[serde(default = "default_crack_width")]
    pub channel_width: f32,
    /// Warping amount for less regular cells.
    #[serde(default)]
    pub warp: f32,
    /// Visual crack depth metadata; also adds a small height depression.
    #[serde(default = "default_crack_depth")]
    pub depth: f32,
}

/// Groundcover layer list.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GroundcoverConfig {
    /// Ordered low vegetation/material overlay layers.
    #[serde(default)]
    pub layers: Vec<GroundcoverLayer>,
}

/// Supported groundcover layer families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroundcoverKind {
    /// Blade or clump grass.
    Grass,
    /// Moss mask and optional tuft meshes.
    Moss,
    /// Small flower layer.
    Flower,
    /// Weed or herbaceous layer.
    Weed,
    /// Dead leaves, twigs, or organic litter.
    Litter,
    /// Small woody shrub clumps.
    Shrub,
    /// Faceted small stones or rocks.
    Rock,
    /// Fallen low logs or branches.
    Log,
}

/// One grass/moss/low vegetation layer.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GroundcoverLayer {
    /// Layer family.
    pub kind: GroundcoverKind,
    /// Optional user-facing layer name.
    #[serde(default)]
    pub name: String,
    /// Scatter density multiplier.
    #[serde(default = "default_groundcover_density")]
    pub density: f32,
    /// Coverage mask amount.
    #[serde(default = "default_groundcover_coverage")]
    pub coverage: f32,
    /// Patch noise frequency.
    #[serde(default = "default_groundcover_patch_scale")]
    pub patch_scale: f32,
    /// Coverage edge softness.
    #[serde(default = "default_groundcover_patch_softness")]
    pub patch_softness: f32,
    /// Optional per-layer seed offset.
    #[serde(default)]
    pub seed_offset: f32,
    /// Prototype/blade height or moss layer thickness.
    #[serde(default = "default_groundcover_height")]
    pub height: f32,
    /// Prototype/blade width.
    #[serde(default = "default_groundcover_width")]
    pub width: f32,
    /// Resting grass curl in radians.
    #[serde(default)]
    pub curl: f32,
    /// Moss/tuft relief frequency.
    #[serde(default = "default_moss_relief_scale")]
    pub relief_scale: f32,
    /// Moss/tuft relief strength.
    #[serde(default = "default_moss_relief_strength")]
    pub relief_strength: f32,
    /// Base color placeholder for preview/import metadata.
    #[serde(default = "default_grass_base_color")]
    pub color_base: String,
    /// Tip color placeholder for preview/import metadata.
    #[serde(default = "default_grass_tip_color")]
    pub color_tip: String,
}

/// Shared wind metadata.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WindParams {
    /// Direction in degrees, measured around world up.
    #[serde(default)]
    pub direction_degrees: f32,
    /// Bend strength.
    #[serde(default = "default_wind_strength")]
    pub strength: f32,
    /// Wind animation speed metadata.
    #[serde(default = "default_wind_speed")]
    pub speed: f32,
    /// Gust spatial scale metadata.
    #[serde(default = "default_gust_scale")]
    pub gust_scale: f32,
    /// Fine flutter amount.
    #[serde(default = "default_flutter")]
    pub flutter: f32,
}

/// Export budget profiles.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NatureProfiles {
    /// Mobile/handheld budget.
    #[serde(default = "default_mobile_profile")]
    pub mobile: NatureProfile,
    /// Console budget.
    #[serde(default = "default_console_profile")]
    pub console: NatureProfile,
}

/// One platform export profile.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NatureProfile {
    /// Tile side length expected by this profile.
    pub tile_size: f32,
    /// Suggested baked map/preview resolution.
    pub terrain_resolution: u32,
    /// LOD0 triangle budget for grass clumps.
    pub lod0_max_triangles: u32,
    /// LOD1 triangle budget for grass clumps.
    pub lod1_max_triangles: u32,
    /// LOD2 triangle budget for grass cards/clumps.
    pub lod2_max_triangles: u32,
    /// End distance for LOD0 before switching to LOD1.
    pub lod0_max_distance: f32,
    /// End distance for LOD1 before switching to LOD2.
    pub lod1_max_distance: f32,
    /// End distance for LOD2 before culling or terrain-only fallback.
    pub lod2_max_distance: f32,
    /// Maximum material slot count for groundcover prototypes.
    pub material_slots: u32,
    /// Maximum authored scatter instances per exported tile.
    pub max_instances_per_tile: u32,
    /// Maximum authored scatter instances per culling chunk.
    pub max_instances_per_chunk: u32,
    /// Default density multiplier for this profile.
    pub density_scale: f32,
    /// Default start fade distance.
    pub cull_start: f32,
    /// Default end cull distance.
    pub cull_end: f32,
    /// Whether grass shadows are intended by default.
    pub shadows: bool,
    /// Whether grass instances should create collision by default.
    #[serde(default)]
    pub grass_collision: bool,
    /// Whether moss instances should create collision by default.
    #[serde(default)]
    pub moss_collision: bool,
}

/// Per-point material and density masks.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MaskSample {
    /// Grass density after layer density multiplier.
    pub grass_density: f32,
    /// Moss coverage.
    pub moss: f32,
    /// Wetness coverage.
    pub wetness: f32,
    /// Crack coverage.
    pub crack: f32,
}

/// Complete terrain sample at one world-space position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainSample {
    /// World X.
    pub x: f32,
    /// World Z.
    pub z: f32,
    /// Height above the tile base.
    pub height: f32,
    /// Analytic/sample normal in world space.
    pub normal: Vec3,
    /// Material and density masks.
    pub masks: MaskSample,
}

/// Deterministic terrain sampler for a nature patch.
#[derive(Debug, Clone)]
pub struct TerrainField {
    patch: NaturePatch,
}

/// Baked map buffers generated from a terrain field.
#[derive(Debug, Clone)]
pub struct NatureMapSet {
    /// Width/height in pixels.
    pub resolution: u32,
    /// Minimum sampled height used to normalize `height_u16`.
    pub min_height: f32,
    /// Maximum sampled height used to normalize `height_u16`.
    pub max_height: f32,
    /// Height map normalized across the sampled tile.
    pub height_u16: Vec<u16>,
    /// Unity/OpenGL-style terrain normal encoding.
    pub normal_yplus: Vec<[u8; 3]>,
    /// Unreal/DirectX-style green-channel-flipped normal encoding.
    pub normal_yminus: Vec<[u8; 3]>,
    /// RGBA masks: R grass, G moss, B wetness, A cracks.
    pub masks_rgba: Vec<[u8; 4]>,
    /// Grass density as a single-channel convenience map.
    pub grass_density: Vec<u8>,
}

/// Engine-facing manifest for a Midori nature package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatureExportManifest {
    /// Manifest schema version.
    pub schema_version: u32,
    /// Generator identity.
    pub generator: String,
    /// Generator package version.
    pub generator_version: String,
    /// Asset name.
    pub asset_name: String,
    /// Source seed.
    pub seed: u64,
    /// Units label.
    pub units: String,
    /// Tile side length.
    pub tile_size: f32,
    /// Coordinate and scale conventions.
    pub axis: AxisConventions,
    /// Baked map resolution.
    pub map_resolution: u32,
    /// Terrain map paths, bounds, and height range.
    pub terrain: TerrainManifest,
    /// Map channel descriptions.
    pub map_channels: Vec<MapChannel>,
    /// Normal map convention outputs.
    pub normal_conventions: NormalConventions,
    /// Material slot contract.
    pub material_slots: Vec<MaterialSlotManifest>,
    /// Engine-native material parameter contract for static Unity/Unreal materials.
    pub material_parameters: Vec<MaterialParameterSetManifest>,
    /// Static material recipe files for engine-native Unity/Unreal material setup.
    pub material_recipes: Vec<MaterialRecipeManifest>,
    /// Static engine import recipe files for Unity/Unreal package setup.
    pub engine_import_recipes: Vec<EngineImportRecipeManifest>,
    /// Exported surface overlay mask contract for terrain and static prototypes.
    pub surface_overlays: Vec<SurfaceOverlayManifest>,
    /// Groundcover layer metadata.
    pub groundcover_layers: Vec<GroundcoverLayer>,
    /// Generated prototype LOD metadata.
    pub prototypes: Vec<GroundcoverPrototypeManifest>,
    /// Scatter output metadata.
    pub scatter: ScatterManifest,
    /// Packed wind/variation attribute contract.
    pub wind_packing: WindPackingManifest,
    /// Predictable package memory and payload footprint.
    pub memory_footprint: MemoryFootprintManifest,
    /// Mobile profile metadata.
    pub mobile: NatureProfile,
    /// Console profile metadata.
    pub console: NatureProfile,
    /// Unity importer hints.
    pub unity: UnityImportHints,
    /// Unreal importer hints.
    pub unreal: UnrealImportHints,
    /// Shader/runtime policy.
    pub shader_policy: String,
    /// Texture/PBR pipeline state.
    pub texture_pipeline: String,
}

/// Coordinate and scale conventions for exported package assets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisConventions {
    /// Up axis for generated meshes and scatter positions.
    pub up_axis: String,
    /// Forward axis for generated prototype meshes.
    pub forward_axis: String,
    /// Coordinate handedness.
    pub handedness: String,
    /// World units represented by one package unit.
    pub unit_scale: f32,
}

/// Terrain metadata needed by engine importers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainManifest {
    /// Relative normalized heightmap path.
    pub heightmap_file: String,
    /// Relative packed mask path.
    pub masks_file: String,
    /// Relative grass density path.
    pub grass_density_file: String,
    /// Minimum sampled world-space height baked into the heightmap.
    pub height_min: f32,
    /// Maximum sampled world-space height baked into the heightmap.
    pub height_max: f32,
    /// Conservative terrain bounds minimum.
    pub bounds_min: [f32; 3],
    /// Conservative terrain bounds maximum.
    pub bounds_max: [f32; 3],
}

/// Normal map convention outputs for common engines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalConventions {
    /// Unity/OpenGL-style normal map file.
    pub unity_yplus_file: String,
    /// Unreal/DirectX-style normal map file.
    pub unreal_yminus_file: String,
}

/// Material slot metadata for package importers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialSlotManifest {
    /// Stable slot name.
    pub name: String,
    /// Slot purpose.
    pub purpose: String,
    /// Suggested alpha mode.
    pub alpha_mode: String,
    /// Whether generated geometry expects two-sided shading.
    pub double_sided: bool,
    /// Whether shadows are enabled by default.
    pub shadows: bool,
}

/// Material parameter set metadata for engine-native material setup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialParameterSetManifest {
    /// Material slot this parameter set configures.
    pub material_slot: String,
    /// Stable parameter set identifier.
    pub parameter_set: String,
    /// Runtime policy for the default mobile/console package.
    pub runtime_policy: String,
    /// Named material parameter bindings.
    pub parameters: Vec<MaterialParameterBindingManifest>,
}

/// One engine material parameter binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialParameterBindingManifest {
    /// Stable Unity Shader Graph / Unreal material function parameter name.
    pub name: String,
    /// Cross-engine semantic for validators and tools.
    pub semantic: String,
    /// Parameter value type.
    pub value_type: String,
    /// Source data or profile field.
    pub source: String,
    /// Default value encoded for lightweight importer reports.
    pub default_value: String,
}

/// Static material recipe file metadata for engine importers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialRecipeManifest {
    /// Material slot configured by this recipe.
    pub material_slot: String,
    /// Relative recipe JSON file path.
    pub file: String,
    /// Runtime policy for the default mobile/console package.
    pub runtime_policy: String,
    /// Engine systems this recipe is intended to help configure.
    pub engine_targets: Vec<String>,
}

/// File-backed static material recipe consumed by engine importers/tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialRecipeFile {
    /// Recipe schema identifier.
    pub schema: String,
    /// Material slot configured by this recipe.
    pub material_slot: String,
    /// Stable parameter set identifier.
    pub parameter_set: String,
    /// Runtime policy for the default mobile/console package.
    pub runtime_policy: String,
    /// Shader/runtime policy inherited from the package.
    pub shader_policy: String,
    /// Texture/PBR pipeline state inherited from the package.
    pub texture_pipeline: String,
    /// Engine systems this recipe is intended to help configure.
    pub engine_targets: Vec<String>,
    /// Named material parameter bindings copied from the manifest contract.
    pub parameters: Vec<MaterialParameterBindingManifest>,
    /// Package textures required by this static recipe.
    pub required_textures: Vec<String>,
    /// Mesh vertex streams required by this static recipe.
    pub required_vertex_streams: Vec<String>,
    /// Surface overlays consumed by this recipe.
    pub surface_overlays: Vec<String>,
    /// Human-readable implementation notes for engine material authors.
    pub notes: Vec<String>,
}

/// Static engine import recipe file metadata for engine importers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineImportRecipeManifest {
    /// Engine configured by this recipe.
    pub engine: String,
    /// Relative recipe JSON file path.
    pub file: String,
    /// Export profile this recipe should use by default.
    pub profile: String,
    /// Runtime policy for the default mobile/console package.
    pub runtime_policy: String,
    /// Engine-native systems this recipe configures.
    pub expected_systems: Vec<String>,
}

/// File-backed static engine import recipe consumed by importers/tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineImportRecipeFile {
    /// Recipe schema identifier.
    pub schema: String,
    /// Engine configured by this recipe.
    pub engine: String,
    /// Export profile this recipe should use by default.
    pub profile: String,
    /// Runtime policy for the default mobile/console package.
    pub runtime_policy: String,
    /// Engine-native systems this recipe configures.
    pub expected_systems: Vec<String>,
    /// Complete package sources the importer should fingerprint for this engine.
    pub source_files: Vec<String>,
    /// Static material recipes consumed by this engine import recipe.
    pub material_recipe_files: Vec<String>,
    /// Terrain/landscape import inputs.
    pub terrain: EngineTerrainImportRecipe,
    /// Groundcover/detail/foliage import inputs.
    pub groundcover: EngineGroundcoverImportRecipe,
    /// Scatter import inputs.
    pub scatter: EngineScatterImportRecipe,
    /// Mobile/console profile fields used by the import recipe.
    pub profile_settings: EngineProfileImportRecipe,
    /// Human-readable implementation notes for engine import authors.
    pub notes: Vec<String>,
}

/// Terrain/landscape import inputs for one engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineTerrainImportRecipe {
    /// Engine-native target system.
    pub target_system: String,
    /// Relative normalized heightmap path.
    pub heightmap_file: String,
    /// Relative packed mask/weightmap path.
    pub mask_file: String,
    /// Relative density map path.
    pub density_map_file: String,
    /// Relative normal map path for the target engine convention.
    pub normal_map_file: String,
    /// Tile side length in meters.
    pub tile_size_meters: f32,
    /// Minimum sampled world-space height.
    pub height_min: f32,
    /// Maximum sampled world-space height.
    pub height_max: f32,
}

/// Groundcover/detail/foliage import inputs for one engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineGroundcoverImportRecipe {
    /// Engine-native target system.
    pub target_system: String,
    /// Prototype source policy.
    pub prototype_source: String,
    /// Groundcover material slot used by generated prototypes.
    pub material_slot: String,
    /// Static material recipe file for groundcover.
    pub material_recipe_file: String,
    /// Number of prototype families declared by the manifest.
    pub prototype_family_count: usize,
    /// Number of LOD0 prototype assets expected for foliage/detail setup.
    pub lod0_prototype_count: usize,
    /// Whether default groundcover shading is two-sided.
    pub double_sided: bool,
    /// Alpha mode expected by the groundcover material.
    pub alpha_mode: String,
}

/// Scatter import inputs for one engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineScatterImportRecipe {
    /// Import placement source.
    pub placement_source: String,
    /// Optional debug scatter JSON file.
    pub scatter_json_file: String,
    /// Number of binary culling chunks expected.
    pub binary_chunk_count: usize,
    /// Number of authored scatter instances expected.
    pub instance_count: usize,
    /// Scatter chunk size in meters.
    pub chunk_size_meters: f32,
}

/// Profile fields used by an engine import recipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineProfileImportRecipe {
    /// Profile name.
    pub name: String,
    /// Default density multiplier.
    pub density_scale: f32,
    /// LOD0 end distance in meters.
    pub lod0_max_distance_meters: f32,
    /// LOD1 end distance in meters.
    pub lod1_max_distance_meters: f32,
    /// LOD2 end distance in meters.
    pub lod2_max_distance_meters: f32,
    /// Fade/cull start distance in meters.
    pub cull_start_meters: f32,
    /// Fade/cull end distance in meters.
    pub cull_end_meters: f32,
    /// Whether grass shadows are enabled by default.
    pub shadows: bool,
    /// Maximum material slots for generated groundcover.
    pub material_slots: u32,
    /// Maximum authored instances per tile.
    pub max_instances_per_tile: u32,
    /// Maximum authored instances per culling chunk.
    pub max_instances_per_chunk: u32,
    /// Whether grass collision is enabled by default.
    pub grass_collision: bool,
    /// Whether moss collision is enabled by default.
    pub moss_collision: bool,
}

/// Surface overlay channel metadata for engine importers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceOverlayManifest {
    /// Stable overlay name.
    pub name: String,
    /// Relative mask file containing the overlay channel.
    pub source_file: String,
    /// Channel in `source_file` used by this overlay.
    pub channel: String,
    /// Import targets that should consume this overlay.
    pub targets: Vec<String>,
    /// How engines should apply the overlay by default.
    pub application: String,
    /// Runtime policy for the default mobile/console package.
    pub runtime_policy: String,
}

/// Manifest metadata for one generated groundcover prototype.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundcoverPrototypeManifest {
    /// Prototype display/import name.
    pub name: String,
    /// Source groundcover kind.
    pub kind: GroundcoverKind,
    /// Material slot name.
    pub material_slot: String,
    /// Stable surface target labels used by static overlays and engine importers.
    pub surface_targets: Vec<String>,
    /// LOD metadata.
    pub lods: Vec<GroundcoverPrototypeLodManifest>,
}

/// Manifest metadata for one generated prototype LOD.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundcoverPrototypeLodManifest {
    /// LOD index.
    pub index: u32,
    /// Relative GLB path.
    pub file: String,
    /// Vertex count.
    pub vertex_count: usize,
    /// Triangle count.
    pub triangle_count: usize,
    /// Local-space bounds minimum.
    pub bounds_min: [f32; 3],
    /// Local-space bounds maximum.
    pub bounds_max: [f32; 3],
}

/// Scatter output metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScatterManifest {
    /// Whether scatter output is written.
    pub enabled: bool,
    /// Relative JSON scatter file path for debugging and tools.
    pub file: Option<String>,
    /// JSON scatter file format.
    pub format: String,
    /// Relative per-chunk binary scatter files for engine importers.
    pub binary_files: Vec<ScatterBinaryFileManifest>,
    /// Binary scatter file format.
    pub binary_format: ScatterBinaryFormatManifest,
    /// Chunk size in world units.
    pub chunk_size: f32,
    /// Instance field order.
    pub fields: Vec<String>,
}

/// One binary scatter file emitted for a source layer/chunk pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScatterBinaryFileManifest {
    /// Source layer index.
    pub layer_index: usize,
    /// Source layer name.
    pub layer_name: String,
    /// Groundcover kind.
    pub kind: GroundcoverKind,
    /// Chunk coordinate on X.
    pub chunk_x: i32,
    /// Chunk coordinate on Z.
    pub chunk_z: i32,
    /// Relative binary file path.
    pub file: String,
    /// Number of records in the binary file.
    pub instance_count: usize,
    /// Conservative chunk bounds.
    pub bounds_min: [f32; 3],
    /// Conservative chunk bounds.
    pub bounds_max: [f32; 3],
}

/// Binary scatter buffer layout metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScatterBinaryFormatManifest {
    /// Format identifier.
    pub format: String,
    /// Header byte size.
    pub header_bytes: u32,
    /// Record stride in bytes.
    pub record_stride_bytes: u32,
    /// Endianness.
    pub endian: String,
    /// Header field order.
    pub header: Vec<String>,
    /// Record field order.
    pub record: Vec<String>,
}

/// Packed vertex attribute contract for wind and per-instance variation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindPackingManifest {
    /// Phase channel.
    pub phase: String,
    /// Stiffness channel.
    pub stiffness: String,
    /// Height channel.
    pub height: String,
    /// Color variation channel.
    pub color_variation: String,
    /// Normalized blade/card progress channel.
    pub normalized_progress: String,
}

/// Byte footprint metadata for mobile/console budget inspection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryFootprintManifest {
    /// Baked map pixel count per map.
    pub map_pixel_count: u64,
    /// Estimated decoded CPU/GPU bytes for canonical map buffers.
    pub decoded_map_bytes: u64,
    /// Encoded PNG bytes written under `maps/`.
    pub encoded_map_bytes: u64,
    /// Static material recipe JSON bytes written under `materials/`.
    pub material_recipe_bytes: u64,
    /// Static engine import recipe JSON bytes written under `engines/`.
    pub engine_import_recipe_bytes: u64,
    /// Encoded preview mesh GLB bytes.
    pub preview_mesh_bytes: u64,
    /// Encoded prototype GLB bytes.
    pub prototype_mesh_bytes: u64,
    /// Pretty JSON scatter debug bytes.
    pub scatter_json_bytes: u64,
    /// Binary scatter buffer bytes.
    pub scatter_binary_bytes: u64,
    /// Binary scatter header bytes.
    pub scatter_binary_header_bytes: u64,
    /// Binary scatter record payload bytes.
    pub scatter_binary_record_bytes: u64,
    /// Total generated payload bytes, excluding `midori_nature.json` itself.
    pub total_payload_bytes: u64,
}

/// Unity import hints for the default package path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnityImportHints {
    /// Suggested terrain heightmap file.
    pub terrain_heightmap: String,
    /// Suggested terrain/detail density map.
    pub detail_density_map: String,
    /// Suggested normal map file.
    pub normal_map: String,
    /// Suggested detail rendering path.
    pub detail_mode: String,
    /// Maximum instances per instanced batch noted by Unity terrain details.
    pub detail_batch_max_instances: u32,
}

/// Unreal import hints for the default package path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnrealImportHints {
    /// Suggested landscape heightmap file.
    pub landscape_heightmap: String,
    /// Suggested landscape weightmap file.
    pub landscape_weightmap: String,
    /// Suggested normal map file.
    pub normal_map: String,
    /// Suggested foliage import path.
    pub foliage_mode: String,
    /// Suggested static mesh import path.
    pub static_mesh_pipeline: String,
}

/// Controls for writing an initial Midori nature package directory.
#[derive(Debug, Clone)]
pub struct NaturePackageConfig {
    /// Resolution for baked map outputs.
    pub map_resolution: u32,
    /// Resolution for the preview terrain mesh.
    pub preview_resolution: u32,
    /// World-space chunk size used for scatter output.
    pub scatter_chunk_size: f32,
    /// Write `preview_tile.glb`.
    pub write_preview_mesh: bool,
    /// Write prototype LOD meshes into `prototypes/`.
    pub write_prototype_meshes: bool,
    /// Write chunked scatter records into `instances/scatter.json`.
    pub write_scatter_json: bool,
    /// Write per-chunk binary scatter buffers into `instances/binary/`.
    pub write_scatter_binary: bool,
}

/// Summary returned after writing a nature package.
#[derive(Debug, Clone)]
pub struct NaturePackageSummary {
    /// Path to `midori_nature.json`.
    pub manifest_path: PathBuf,
    /// Path to the `maps/` directory.
    pub maps_directory: PathBuf,
    /// Optional preview mesh path.
    pub preview_mesh_path: Option<PathBuf>,
    /// Prototype mesh paths written to disk.
    pub prototype_meshes: Vec<PathBuf>,
    /// Optional scatter JSON path.
    pub scatter_path: Option<PathBuf>,
    /// Binary scatter buffer paths written to disk.
    pub scatter_binary_paths: Vec<PathBuf>,
    /// Static material recipe JSON paths written to disk.
    pub material_recipe_paths: Vec<PathBuf>,
    /// Static engine import recipe JSON paths written to disk.
    pub engine_import_recipe_paths: Vec<PathBuf>,
    /// Stable checksum of the baked map set.
    pub map_checksum: u64,
    /// Total scatter instances written.
    pub scatter_instance_count: usize,
}

/// Report returned after validating an exported nature package from disk.
#[derive(Debug, Clone, Serialize)]
pub struct NaturePackageValidationReport {
    /// Parsed and semantically validated manifest.
    pub manifest: NatureExportManifest,
    /// Path to the validated manifest file.
    pub manifest_path: PathBuf,
    /// Baked map files validated from `maps/`.
    pub map_files: Vec<PathBuf>,
    /// Baked map dimensions, color format, channel ranges, and file checksum.
    pub map_summaries: Vec<MapValidationSummary>,
    /// Cross-map semantic checks for engine import conventions.
    pub map_relationships: MapRelationshipSummary,
    /// Static material recipe files referenced by the manifest.
    pub material_recipe_files: Vec<PathBuf>,
    /// Parsed material recipe summaries for engine material setup.
    pub material_recipe_summaries: Vec<MaterialRecipeValidationSummary>,
    /// Static engine import recipe files referenced by the manifest.
    pub engine_import_recipe_files: Vec<PathBuf>,
    /// Parsed engine import recipe summaries for engine setup.
    pub engine_import_recipe_summaries: Vec<EngineImportRecipeValidationSummary>,
    /// Prototype mesh files referenced by the manifest.
    pub prototype_files: Vec<PathBuf>,
    /// Decoded GLB summaries for prototype files referenced by the manifest.
    pub prototype_summaries: Vec<PrototypeValidationSummary>,
    /// Mobile/console profile budget summaries for exported prototypes.
    pub profile_budget_summaries: Vec<ProfileBudgetValidationSummary>,
    /// Optional debug/tooling scatter JSON file.
    pub scatter_json_path: Option<PathBuf>,
    /// Instance count read from scatter JSON.
    pub scatter_json_instances: usize,
    /// Per-chunk summaries read from scatter JSON.
    pub scatter_json_chunks: Vec<ScatterChunkValidationSummary>,
    /// Binary scatter files referenced by the manifest.
    pub scatter_binary_files: Vec<PathBuf>,
    /// Instance count read from binary scatter headers.
    pub scatter_binary_instances: usize,
    /// Per-chunk summaries read from binary scatter buffers.
    pub scatter_binary_summaries: Vec<ScatterChunkValidationSummary>,
    /// JSON/binary scatter parity summary.
    pub scatter_parity: ScatterParitySummary,
    /// File-backed memory footprint summary, matched against the manifest.
    pub memory_footprint: MemoryFootprintManifest,
}

/// Validation summary for one baked map artifact.
#[derive(Debug, Clone, Serialize)]
pub struct MapValidationSummary {
    /// Validated map file path.
    pub file: PathBuf,
    /// Decoded pixel width.
    pub width: u32,
    /// Decoded pixel height.
    pub height: u32,
    /// Decoded image color type.
    pub color_type: String,
    /// Number of decoded channels.
    pub channels: usize,
    /// Minimum decoded value per channel.
    pub channel_min: Vec<u32>,
    /// Maximum decoded value per channel.
    pub channel_max: Vec<u32>,
    /// FNV-1a checksum of the encoded PNG file bytes.
    pub file_checksum: u64,
}

/// Semantic validation summary across baked map artifacts.
#[derive(Debug, Clone, Serialize)]
pub struct MapRelationshipSummary {
    /// Pixels compared between Unity Y+ and Unreal Y- normal maps.
    pub normal_pair_pixels: usize,
    /// Count of pixels whose normal red/blue channels differ between conventions.
    pub normal_red_blue_mismatches: usize,
    /// Count of pixels whose green channels are not inverse Y within tolerance.
    pub normal_green_flip_mismatches: usize,
    /// Largest absolute error from `normal_yplus.g + normal_yminus.g == 255`.
    pub normal_green_flip_max_error: u8,
    /// Pixels compared between packed mask R and grass density map.
    pub grass_density_pixels: usize,
    /// Count of pixels where `grass_density.png` differs from `masks_rgba.png` R.
    pub grass_density_mask_r_mismatches: usize,
}

/// Validation summary for one static material recipe JSON artifact.
#[derive(Debug, Clone, Serialize)]
pub struct MaterialRecipeValidationSummary {
    /// Validated recipe file path.
    pub file: PathBuf,
    /// Material slot configured by this recipe.
    pub material_slot: String,
    /// Stable parameter set identifier.
    pub parameter_set: String,
    /// Runtime policy for the default mobile/console package.
    pub runtime_policy: String,
    /// Shader/runtime policy declared by the recipe.
    pub shader_policy: String,
    /// Texture/PBR pipeline state declared by the recipe.
    pub texture_pipeline: String,
    /// Count of material parameters bound by the recipe.
    pub parameter_count: usize,
    /// Engine systems this recipe targets.
    pub engine_targets: Vec<String>,
    /// Required package textures.
    pub required_textures: Vec<String>,
    /// Required mesh vertex streams.
    pub required_vertex_streams: Vec<String>,
    /// Surface overlays consumed by this recipe.
    pub surface_overlays: Vec<String>,
    /// FNV-1a checksum of the encoded recipe file bytes.
    pub file_checksum: u64,
}

/// Validation summary for one static engine import recipe JSON artifact.
#[derive(Debug, Clone, Serialize)]
pub struct EngineImportRecipeValidationSummary {
    /// Validated recipe file path.
    pub file: PathBuf,
    /// Engine configured by this recipe.
    pub engine: String,
    /// Profile configured by this recipe.
    pub profile: String,
    /// Runtime policy for the default mobile/console package.
    pub runtime_policy: String,
    /// Number of source files fingerprinted by this recipe.
    pub source_file_count: usize,
    /// Static material recipes consumed by this recipe.
    pub material_recipe_files: Vec<String>,
    /// Engine-native systems configured by this recipe.
    pub expected_systems: Vec<String>,
    /// Scatter instances expected by this recipe.
    pub scatter_instances: usize,
    /// Scatter binary chunks expected by this recipe.
    pub scatter_binary_chunks: usize,
    /// FNV-1a checksum of the encoded recipe file bytes.
    pub file_checksum: u64,
}

/// Validation summary for one exported prototype GLB.
#[derive(Debug, Clone, Serialize)]
pub struct PrototypeValidationSummary {
    /// Validated prototype file path.
    pub file: PathBuf,
    /// LOD index declared in the manifest.
    pub lod_index: u32,
    /// Decoded mesh count.
    pub mesh_count: usize,
    /// Decoded primitive count.
    pub primitive_count: usize,
    /// Decoded POSITION vertex count.
    pub vertex_count: usize,
    /// Decoded indexed triangle count.
    pub triangle_count: usize,
    /// Count of materials actually referenced by primitives.
    pub used_material_count: usize,
    /// Referenced material indices.
    pub used_material_indices: Vec<usize>,
    /// Whether every primitive has POSITION.
    pub has_positions: bool,
    /// Whether every primitive has NORMAL.
    pub has_normals: bool,
    /// Whether every primitive has TANGENT.
    pub has_tangents: bool,
    /// Whether every decoded NORMAL is finite and unit length.
    pub normals_are_valid: bool,
    /// Whether every decoded TANGENT is finite, unit length, and compatible with NORMAL.
    pub tangents_are_valid: bool,
    /// Whether every primitive has TEXCOORD_0.
    pub has_texcoord0: bool,
    /// Whether every primitive has TEXCOORD_1 for wind/variation data.
    pub has_texcoord1: bool,
    /// Whether every primitive has COLOR_0 for wind/variation data.
    pub has_color0: bool,
    /// Decoded local-space bounds minimum.
    pub bounds_min: [f32; 3],
    /// Decoded local-space bounds maximum.
    pub bounds_max: [f32; 3],
    /// FNV-1a checksum of the encoded GLB file bytes.
    pub file_checksum: u64,
}

/// Validation summary proving exported prototype LODs fit one platform profile.
#[derive(Debug, Clone, Serialize)]
pub struct ProfileBudgetValidationSummary {
    /// Profile name, such as `mobile` or `console`.
    pub profile: String,
    /// Count of decoded prototype LODs checked against this profile.
    pub prototype_lod_count: usize,
    /// Observed maximum LOD0 triangle count.
    pub max_lod0_triangles: usize,
    /// Observed maximum LOD1 triangle count.
    pub max_lod1_triangles: usize,
    /// Observed maximum LOD2-or-farther triangle count.
    pub max_lod2_triangles: usize,
    /// Profile LOD0 triangle budget.
    pub lod0_triangle_budget: u32,
    /// Profile LOD1 triangle budget.
    pub lod1_triangle_budget: u32,
    /// Profile LOD2-or-farther triangle budget.
    pub lod2_triangle_budget: u32,
    /// Observed maximum used material count per prototype GLB.
    pub max_used_material_count: usize,
    /// Profile material slot budget.
    pub material_slot_budget: u32,
    /// Observed authored scatter instances in the package tile.
    pub scatter_instance_count: usize,
    /// Observed maximum authored scatter instances in one culling chunk.
    pub max_chunk_instance_count: usize,
    /// Profile authored scatter instance budget per tile.
    pub max_instances_per_tile_budget: u32,
    /// Profile authored scatter instance budget per culling chunk.
    pub max_instances_per_chunk_budget: u32,
    /// Count of prototype LODs exceeding their triangle budget.
    pub triangle_budget_violation_count: usize,
    /// Count of prototype LODs exceeding their material-slot budget.
    pub material_slot_violation_count: usize,
    /// Count of scatter instance budget violations.
    pub instance_budget_violation_count: usize,
    /// Whether all budget checks passed.
    pub passed: bool,
}

/// Validation summary for one scatter chunk.
#[derive(Debug, Clone, Serialize)]
pub struct ScatterChunkValidationSummary {
    /// Source kind, either `json` or `binary`.
    pub source: String,
    /// Source file path.
    pub file: PathBuf,
    /// Source layer index.
    pub layer_index: usize,
    /// Source layer name.
    pub layer_name: String,
    /// Groundcover kind.
    pub kind: GroundcoverKind,
    /// Chunk coordinate on X.
    pub chunk_x: i32,
    /// Chunk coordinate on Z.
    pub chunk_z: i32,
    /// Decoded instance count.
    pub instance_count: usize,
    /// Conservative chunk bounds minimum.
    pub bounds_min: [f32; 3],
    /// Conservative chunk bounds maximum.
    pub bounds_max: [f32; 3],
    /// Minimum decoded position per axis.
    pub position_min: [f32; 3],
    /// Maximum decoded position per axis.
    pub position_max: [f32; 3],
    /// Minimum yaw.
    pub yaw_min: f32,
    /// Maximum yaw.
    pub yaw_max: f32,
    /// Minimum height multiplier.
    pub height_min: f32,
    /// Maximum height multiplier.
    pub height_max: f32,
    /// Minimum width multiplier.
    pub width_min: f32,
    /// Maximum width multiplier.
    pub width_max: f32,
    /// Minimum phase.
    pub phase_min: f32,
    /// Maximum phase.
    pub phase_max: f32,
    /// Minimum color variation.
    pub color_variation_min: f32,
    /// Maximum color variation.
    pub color_variation_max: f32,
    /// FNV-1a checksum over decoded logical scatter records.
    pub record_checksum: u64,
    /// FNV-1a checksum of the encoded source file bytes.
    pub file_checksum: u64,
}

/// Summary proving JSON debug scatter and binary scatter buffers agree.
#[derive(Debug, Clone, Serialize)]
pub struct ScatterParitySummary {
    /// JSON chunk count.
    pub json_chunk_count: usize,
    /// Binary chunk count.
    pub binary_chunk_count: usize,
    /// Matching JSON/binary chunks.
    pub matching_chunk_count: usize,
    /// JSON chunks missing from binary output.
    pub missing_binary_chunk_count: usize,
    /// Binary chunks missing from JSON output.
    pub extra_binary_chunk_count: usize,
    /// Chunks with mismatched instance counts.
    pub instance_count_mismatch_count: usize,
    /// Chunks with mismatched bounds.
    pub bounds_mismatch_count: usize,
    /// Chunks with mismatched logical record checksums.
    pub record_checksum_mismatch_count: usize,
}

/// Generated reusable groundcover prototype with a small LOD chain.
#[derive(Debug, Clone)]
pub struct GroundcoverPrototype {
    /// Prototype display/import name.
    pub name: String,
    /// Source groundcover kind.
    pub kind: GroundcoverKind,
    /// Mesh LODs ordered nearest to farthest.
    pub lods: Vec<GroundcoverPrototypeLod>,
}

/// One reusable mesh LOD for a generated groundcover prototype.
#[derive(Debug, Clone)]
pub struct GroundcoverPrototypeLod {
    /// LOD index.
    pub index: u32,
    /// LOD display/import name.
    pub name: String,
    /// Generated mesh data.
    pub mesh: Mesh,
}

/// Deterministic placement data for one groundcover layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScatterSet {
    /// Source layer index.
    pub layer_index: usize,
    /// Source layer name, or a generated kind label.
    pub layer_name: String,
    /// Source groundcover kind.
    pub kind: GroundcoverKind,
    /// Chunked instances for streaming and culling.
    pub chunks: Vec<ScatterChunk>,
}

/// Scatter instances grouped by world-space tile chunk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScatterChunk {
    /// Chunk coordinate on X.
    pub chunk_x: i32,
    /// Chunk coordinate on Z.
    pub chunk_z: i32,
    /// Conservative chunk bounds.
    pub bounds_min: [f32; 3],
    /// Conservative chunk bounds.
    pub bounds_max: [f32; 3],
    /// Instances contained in this chunk.
    pub instances: Vec<ScatterInstance>,
}

/// One groundcover placement record.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScatterInstance {
    /// World-space position.
    pub position: [f32; 3],
    /// Yaw in radians.
    pub yaw: f32,
    /// Per-instance height multiplier.
    pub height: f32,
    /// Per-instance width multiplier.
    pub width: f32,
    /// Wind/animation phase.
    pub phase: f32,
    /// Stable color variation scalar.
    pub color_variation: f32,
}

/// One exported map channel description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapChannel {
    /// File name.
    pub file: String,
    /// Channel meaning.
    pub channels: String,
}

/// Error type for nature patch parsing, validation, and map export.
#[derive(Debug)]
pub enum NaturePatchError {
    /// IO error reading/writing files.
    Io(std::io::Error),
    /// TOML parse error.
    Parse(toml::de::Error),
    /// JSON encoding error.
    Json(serde_json::Error),
    /// Image encoding error.
    Image(ImageError),
    /// Mesh export error.
    Export(ExportError),
    /// Semantic validation error.
    Validation(String),
}

impl From<std::io::Error> for NaturePatchError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<toml::de::Error> for NaturePatchError {
    fn from(err: toml::de::Error) -> Self {
        Self::Parse(err)
    }
}

impl From<serde_json::Error> for NaturePatchError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

impl From<ImageError> for NaturePatchError {
    fn from(err: ImageError) -> Self {
        Self::Image(err)
    }
}

impl From<ExportError> for NaturePatchError {
    fn from(err: ExportError) -> Self {
        Self::Export(err)
    }
}

impl std::fmt::Display for NaturePatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NaturePatchError::Io(err) => write!(f, "IO error: {err}"),
            NaturePatchError::Parse(err) => write!(f, "Parse error: {err}"),
            NaturePatchError::Json(err) => write!(f, "JSON error: {err}"),
            NaturePatchError::Image(err) => write!(f, "Image error: {err}"),
            NaturePatchError::Export(err) => write!(f, "Export error: {err}"),
            NaturePatchError::Validation(err) => write!(f, "Validation error: {err}"),
        }
    }
}

impl std::error::Error for NaturePatchError {}

/// Validate an exported Midori nature package against the importer contract.
pub fn validate_nature_package(
    directory: &Path,
) -> Result<NaturePackageValidationReport, NaturePatchError> {
    if !directory.is_dir() {
        return Err(NaturePatchError::Validation(format!(
            "nature package directory `{}` does not exist",
            directory.display()
        )));
    }

    let manifest_path = directory.join("midori_nature.json");
    let manifest: NatureExportManifest =
        serde_json::from_reader(std::fs::File::open(&manifest_path)?)?;
    manifest.validate()?;

    let mut map_files = Vec::new();
    let mut map_summaries = Vec::new();
    for channel in &manifest.map_channels {
        let path = require_package_subfile(
            directory,
            "maps",
            &channel.file,
            "manifest.map_channels[].file",
        )?;
        let summary = validate_png_map(
            &path,
            manifest.map_resolution,
            &channel.file,
            "manifest.map_channels[].file",
        )?;
        map_files.push(path);
        map_summaries.push(summary);
    }

    require_package_file(
        directory,
        &manifest.terrain.heightmap_file,
        "manifest.terrain.heightmap_file",
    )?;
    require_package_file(
        directory,
        &manifest.terrain.masks_file,
        "manifest.terrain.masks_file",
    )?;
    require_package_file(
        directory,
        &manifest.terrain.grass_density_file,
        "manifest.terrain.grass_density_file",
    )?;
    require_package_file(
        directory,
        &manifest.normal_conventions.unity_yplus_file,
        "manifest.normal_conventions.unity_yplus_file",
    )?;
    require_package_file(
        directory,
        &manifest.normal_conventions.unreal_yminus_file,
        "manifest.normal_conventions.unreal_yminus_file",
    )?;
    require_package_file(
        directory,
        &manifest.unity.terrain_heightmap,
        "manifest.unity.terrain_heightmap",
    )?;
    require_package_file(
        directory,
        &manifest.unity.detail_density_map,
        "manifest.unity.detail_density_map",
    )?;
    require_package_file(
        directory,
        &manifest.unity.normal_map,
        "manifest.unity.normal_map",
    )?;
    require_package_file(
        directory,
        &manifest.unreal.landscape_heightmap,
        "manifest.unreal.landscape_heightmap",
    )?;
    require_package_file(
        directory,
        &manifest.unreal.landscape_weightmap,
        "manifest.unreal.landscape_weightmap",
    )?;
    require_package_file(
        directory,
        &manifest.unreal.normal_map,
        "manifest.unreal.normal_map",
    )?;
    let map_relationships = validate_map_relationships(directory, &manifest)?;

    let mut material_recipe_files = Vec::new();
    let mut material_recipe_summaries = Vec::new();
    for recipe in &manifest.material_recipes {
        let path = require_package_file(
            directory,
            recipe.file.as_str(),
            "manifest.material_recipes[].file",
        )?;
        let summary = validate_material_recipe_file(&path, recipe, &manifest, directory)?;
        material_recipe_files.push(path);
        material_recipe_summaries.push(summary);
    }

    let mut engine_import_recipe_files = Vec::new();
    let mut engine_import_recipe_summaries = Vec::new();
    for recipe in &manifest.engine_import_recipes {
        let path = require_package_file(
            directory,
            recipe.file.as_str(),
            "manifest.engine_import_recipes[].file",
        )?;
        let summary = validate_engine_import_recipe_file(&path, recipe, &manifest, directory)?;
        engine_import_recipe_files.push(path);
        engine_import_recipe_summaries.push(summary);
    }

    let material_slot_limit = manifest
        .mobile
        .material_slots
        .min(manifest.console.material_slots) as usize;
    let mut prototype_files = Vec::new();
    let mut prototype_summaries = Vec::new();
    for prototype in &manifest.prototypes {
        for lod in &prototype.lods {
            let path =
                require_package_file(directory, &lod.file, "manifest.prototypes[].lods[].file")?;
            let summary = validate_prototype_glb(&path, lod, material_slot_limit)?;
            prototype_files.push(path);
            prototype_summaries.push(summary);
        }
    }
    let (scatter_json_path, scatter_json_instances, scatter_json_chunks) =
        if let Some(file) = manifest.scatter.file.as_deref() {
            let path = require_package_file(directory, file, "manifest.scatter.file")?;
            let (instance_count, chunks) = validate_scatter_json_file(&path)?;
            (Some(path), instance_count, chunks)
        } else {
            (None, 0, Vec::new())
        };

    let mut scatter_binary_files = Vec::new();
    let mut scatter_binary_summaries = Vec::new();
    let mut scatter_binary_instances = 0usize;
    for binary in &manifest.scatter.binary_files {
        let path = require_package_file(
            directory,
            &binary.file,
            "manifest.scatter.binary_files[].file",
        )?;
        let summary = validate_scatter_binary_file(&path, binary)?;
        let instance_count = summary.instance_count;
        scatter_binary_instances = scatter_binary_instances
            .checked_add(instance_count)
            .ok_or_else(|| {
                NaturePatchError::Validation(
                    "scatter binary instance count overflowed usize".to_string(),
                )
            })?;
        scatter_binary_files.push(path);
        scatter_binary_summaries.push(summary);
    }

    if scatter_json_path.is_some()
        && !scatter_binary_files.is_empty()
        && scatter_json_instances != scatter_binary_instances
    {
        return Err(NaturePatchError::Validation(format!(
            "scatter JSON contains {scatter_json_instances} instances but binary buffers contain {scatter_binary_instances}"
        )));
    }
    let scatter_parity = validate_scatter_parity(&scatter_json_chunks, &scatter_binary_summaries)?;
    let (budget_scatter_instances, budget_scatter_chunks) = if scatter_binary_summaries.is_empty() {
        (scatter_json_instances, scatter_json_chunks.as_slice())
    } else {
        (
            scatter_binary_instances,
            scatter_binary_summaries.as_slice(),
        )
    };
    let profile_budget_summaries = validate_profile_budgets(
        &manifest,
        &prototype_summaries,
        budget_scatter_instances,
        budget_scatter_chunks,
    )?;
    let preview_mesh_path = optional_existing_package_file(directory, "preview_tile.glb")?;
    let memory_config = NaturePackageConfig {
        map_resolution: manifest.map_resolution,
        ..NaturePackageConfig::default()
    };
    let memory_footprint = package_memory_footprint(
        directory,
        &memory_config,
        preview_mesh_path.as_deref(),
        &material_recipe_files,
        &engine_import_recipe_files,
        &prototype_files,
        scatter_json_path.as_deref(),
        &scatter_binary_files,
    )?;
    if memory_footprint != manifest.memory_footprint {
        return Err(NaturePatchError::Validation(format!(
            "manifest.memory_footprint does not match package files: declared {:?}, actual {:?}",
            manifest.memory_footprint, memory_footprint
        )));
    }

    Ok(NaturePackageValidationReport {
        manifest,
        manifest_path,
        map_files,
        map_summaries,
        map_relationships,
        material_recipe_files,
        material_recipe_summaries,
        engine_import_recipe_files,
        engine_import_recipe_summaries,
        prototype_files,
        prototype_summaries,
        profile_budget_summaries,
        scatter_json_path,
        scatter_json_instances,
        scatter_json_chunks,
        scatter_binary_files,
        scatter_binary_instances,
        scatter_binary_summaries,
        scatter_parity,
        memory_footprint,
    })
}

impl NatureExportManifest {
    /// Validate the package manifest contract before writing or importing it.
    pub fn validate(&self) -> Result<(), NaturePatchError> {
        if self.schema_version != 3 {
            return Err(NaturePatchError::Validation(format!(
                "manifest.schema_version must be 3, got {}",
                self.schema_version
            )));
        }
        if self.generator.trim().is_empty() {
            return Err(NaturePatchError::Validation(
                "manifest.generator must not be empty".to_string(),
            ));
        }
        if self.asset_name.trim().is_empty() {
            return Err(NaturePatchError::Validation(
                "manifest.asset_name must not be empty".to_string(),
            ));
        }
        validate_positive("manifest.tile_size", self.tile_size)?;
        if self.map_resolution < 2 {
            return Err(NaturePatchError::Validation(
                "manifest.map_resolution must be at least 2".to_string(),
            ));
        }
        self.validate_memory_footprint()?;
        validate_non_empty_file(
            "manifest.terrain.heightmap_file",
            &self.terrain.heightmap_file,
        )?;
        validate_non_empty_file("manifest.terrain.masks_file", &self.terrain.masks_file)?;
        validate_non_empty_file(
            "manifest.terrain.grass_density_file",
            &self.terrain.grass_density_file,
        )?;
        validate_bounds(
            &self.terrain.bounds_min,
            &self.terrain.bounds_max,
            "manifest.terrain.bounds",
        )?;
        if !self.terrain.height_min.is_finite()
            || !self.terrain.height_max.is_finite()
            || self.terrain.height_min > self.terrain.height_max
            || self.terrain.bounds_min[1] > self.terrain.height_min
            || self.terrain.bounds_max[1] < self.terrain.height_max
        {
            return Err(NaturePatchError::Validation(
                "manifest.terrain height range must be finite, ordered, and inside bounds"
                    .to_string(),
            ));
        }
        validate_positive("manifest.axis.unit_scale", self.axis.unit_scale)?;
        if self.axis.up_axis != "Y" {
            return Err(NaturePatchError::Validation(
                "manifest.axis.up_axis must be Y".to_string(),
            ));
        }
        if !self.has_map_file("height_u16.png")
            || !self.has_map_file("normal_yplus.png")
            || !self.has_map_file("normal_yminus.png")
            || !self.has_map_file("masks_rgba.png")
            || !self.has_map_file("grass_density.png")
        {
            return Err(NaturePatchError::Validation(
                "manifest.map_channels must include canonical Midori map files".to_string(),
            ));
        }
        validate_non_empty_file(
            "manifest.normal_conventions.unity_yplus_file",
            &self.normal_conventions.unity_yplus_file,
        )?;
        validate_non_empty_file(
            "manifest.normal_conventions.unreal_yminus_file",
            &self.normal_conventions.unreal_yminus_file,
        )?;
        if !self
            .material_slots
            .iter()
            .any(|slot| slot.name == "groundcover_foliage")
        {
            return Err(NaturePatchError::Validation(
                "manifest.material_slots must include groundcover_foliage".to_string(),
            ));
        }
        self.validate_material_parameters()?;
        self.validate_material_recipes()?;
        self.validate_engine_import_recipes()?;
        self.validate_surface_overlays()?;
        for prototype in &self.prototypes {
            if prototype.name.trim().is_empty() {
                return Err(NaturePatchError::Validation(
                    "manifest.prototypes[].name must not be empty".to_string(),
                ));
            }
            if prototype.surface_targets.is_empty()
                || prototype
                    .surface_targets
                    .iter()
                    .any(|target| target.trim().is_empty())
            {
                return Err(NaturePatchError::Validation(format!(
                    "manifest prototype `{}` must declare non-empty surface targets",
                    prototype.name
                )));
            }
            for required in required_surface_targets_for_kind(prototype.kind) {
                if !prototype
                    .surface_targets
                    .iter()
                    .any(|target| target == required)
                {
                    return Err(NaturePatchError::Validation(format!(
                        "manifest prototype `{}` kind `{:?}` must include surface target `{required}`",
                        prototype.name, prototype.kind
                    )));
                }
            }
            if prototype.lods.is_empty() {
                return Err(NaturePatchError::Validation(format!(
                    "manifest prototype `{}` must contain at least one LOD",
                    prototype.name
                )));
            }
            for lod in &prototype.lods {
                validate_non_empty_file("manifest.prototypes[].lods[].file", &lod.file)?;
                if lod.vertex_count == 0 || lod.triangle_count == 0 {
                    return Err(NaturePatchError::Validation(format!(
                        "manifest prototype `{}` LOD{} must have nonzero geometry",
                        prototype.name, lod.index
                    )));
                }
                for axis in 0..3 {
                    if lod.bounds_min[axis] > lod.bounds_max[axis] {
                        return Err(NaturePatchError::Validation(format!(
                            "manifest prototype `{}` LOD{} has invalid bounds",
                            prototype.name, lod.index
                        )));
                    }
                }
            }
        }
        if self.scatter.enabled {
            validate_positive("manifest.scatter.chunk_size", self.scatter.chunk_size)?;
            if self.scatter.file.is_none() && self.scatter.binary_files.is_empty() {
                return Err(NaturePatchError::Validation(
                    "manifest.scatter must include JSON or binary outputs when enabled".to_string(),
                ));
            }
            if !self.scatter.binary_files.is_empty() {
                if self.scatter.binary_format.format != "midori.scatter.bin.v1" {
                    return Err(NaturePatchError::Validation(
                        "manifest.scatter.binary_format.format must be midori.scatter.bin.v1"
                            .to_string(),
                    ));
                }
                if self.scatter.binary_format.header_bytes != SCATTER_BINARY_HEADER_BYTES
                    || self.scatter.binary_format.record_stride_bytes
                        != SCATTER_BINARY_RECORD_STRIDE_BYTES
                {
                    return Err(NaturePatchError::Validation(
                        "manifest.scatter.binary_format byte sizes do not match Midori v1"
                            .to_string(),
                    ));
                }
                if self.scatter.binary_format.endian != "little" {
                    return Err(NaturePatchError::Validation(
                        "manifest.scatter.binary_format.endian must be little".to_string(),
                    ));
                }
                let expected_format = scatter_binary_format_manifest();
                if self.scatter.binary_format.header != expected_format.header
                    || self.scatter.binary_format.record != expected_format.record
                {
                    return Err(NaturePatchError::Validation(
                        "manifest.scatter.binary_format field layout does not match Midori v1"
                            .to_string(),
                    ));
                }
            }
            for file in &self.scatter.binary_files {
                validate_non_empty_file("manifest.scatter.binary_files[].file", &file.file)?;
                if file.instance_count == 0 {
                    return Err(NaturePatchError::Validation(format!(
                        "manifest scatter binary `{}` must contain at least one instance",
                        file.file
                    )));
                }
                validate_bounds(
                    &file.bounds_min,
                    &file.bounds_max,
                    &format!("manifest scatter binary `{}` bounds", file.file),
                )?;
            }
        }
        if self.shader_policy != "preview_only" {
            return Err(NaturePatchError::Validation(
                "manifest.shader_policy must be preview_only".to_string(),
            ));
        }
        if self.texture_pipeline != "parked" {
            return Err(NaturePatchError::Validation(
                "manifest.texture_pipeline must be parked".to_string(),
            ));
        }
        validate_non_empty_file("manifest.unity.normal_map", &self.unity.normal_map)?;
        validate_non_empty_file("manifest.unreal.normal_map", &self.unreal.normal_map)?;
        Ok(())
    }

    fn validate_material_parameters(&self) -> Result<(), NaturePatchError> {
        let material_slots = self
            .material_slots
            .iter()
            .map(|slot| slot.name.as_str())
            .collect::<BTreeSet<_>>();
        let mut semantics_by_slot: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();

        for set in &self.material_parameters {
            if set.material_slot.trim().is_empty()
                || set.parameter_set.trim().is_empty()
                || set.runtime_policy.trim().is_empty()
            {
                return Err(NaturePatchError::Validation(
                    "manifest.material_parameters must include material_slot, parameter_set, and runtime_policy".to_string(),
                ));
            }
            if !material_slots.contains(set.material_slot.as_str()) {
                return Err(NaturePatchError::Validation(format!(
                    "manifest material parameter set `{}` references unknown material slot `{}`",
                    set.parameter_set, set.material_slot
                )));
            }
            if set.runtime_policy != "engine_native_static" {
                return Err(NaturePatchError::Validation(format!(
                    "manifest material parameter set `{}` must use engine_native_static runtime policy",
                    set.parameter_set
                )));
            }
            if set.parameters.is_empty() {
                return Err(NaturePatchError::Validation(format!(
                    "manifest material parameter set `{}` must include parameters",
                    set.parameter_set
                )));
            }
            let semantics = semantics_by_slot
                .entry(set.material_slot.as_str())
                .or_default();
            for parameter in &set.parameters {
                if parameter.name.trim().is_empty()
                    || parameter.semantic.trim().is_empty()
                    || parameter.value_type.trim().is_empty()
                    || parameter.source.trim().is_empty()
                    || parameter.default_value.trim().is_empty()
                {
                    return Err(NaturePatchError::Validation(format!(
                        "manifest material parameter set `{}` has an incomplete parameter",
                        set.parameter_set
                    )));
                }
                semantics.insert(parameter.semantic.as_str());
            }
        }

        let required = [
            (
                "terrain_surface",
                [
                    "overlay_mask_texture",
                    "moss_mask_channel",
                    "wetness_mask_channel",
                    "crack_mask_channel",
                ]
                .as_slice(),
            ),
            (
                "groundcover_foliage",
                [
                    "alpha_cutoff",
                    "wind_strength",
                    "wind_speed",
                    "wind_direction_degrees",
                    "wind_gust_scale",
                    "fade_start_meters",
                    "fade_end_meters",
                    "color_variation_scale",
                ]
                .as_slice(),
            ),
        ];

        for (slot, required_semantics) in required {
            let Some(actual_semantics) = semantics_by_slot.get(slot) else {
                return Err(NaturePatchError::Validation(format!(
                    "manifest.material_parameters must include {slot}"
                )));
            };
            for semantic in required_semantics {
                if !actual_semantics.contains(semantic) {
                    return Err(NaturePatchError::Validation(format!(
                        "manifest.material_parameters `{slot}` must include semantic `{semantic}`"
                    )));
                }
            }
        }

        Ok(())
    }

    fn validate_material_recipes(&self) -> Result<(), NaturePatchError> {
        let material_slots = self
            .material_slots
            .iter()
            .map(|slot| slot.name.as_str())
            .collect::<BTreeSet<_>>();
        let parameter_slots = self
            .material_parameters
            .iter()
            .map(|set| set.material_slot.as_str())
            .collect::<BTreeSet<_>>();
        let mut recipe_slots = BTreeSet::new();

        for recipe in &self.material_recipes {
            if recipe.material_slot.trim().is_empty()
                || recipe.file.trim().is_empty()
                || recipe.runtime_policy.trim().is_empty()
            {
                return Err(NaturePatchError::Validation(
                    "manifest.material_recipes must include material_slot, file, and runtime_policy"
                        .to_string(),
                ));
            }
            if !material_slots.contains(recipe.material_slot.as_str()) {
                return Err(NaturePatchError::Validation(format!(
                    "manifest material recipe `{}` references unknown material slot `{}`",
                    recipe.file, recipe.material_slot
                )));
            }
            if !parameter_slots.contains(recipe.material_slot.as_str()) {
                return Err(NaturePatchError::Validation(format!(
                    "manifest material recipe `{}` has no matching material parameter set",
                    recipe.file
                )));
            }
            if recipe.runtime_policy != "engine_native_static" {
                return Err(NaturePatchError::Validation(format!(
                    "manifest material recipe `{}` must use engine_native_static runtime policy",
                    recipe.file
                )));
            }
            if !recipe.file.starts_with("materials/") || !recipe.file.ends_with(".recipe.json") {
                return Err(NaturePatchError::Validation(format!(
                    "manifest material recipe `{}` must live under materials/*.recipe.json",
                    recipe.file
                )));
            }
            if recipe.engine_targets.is_empty()
                || recipe
                    .engine_targets
                    .iter()
                    .any(|target| target.trim().is_empty())
            {
                return Err(NaturePatchError::Validation(format!(
                    "manifest material recipe `{}` must include engine targets",
                    recipe.file
                )));
            }
            let targets = recipe
                .engine_targets
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            if !targets.iter().any(|target| target.starts_with("unity_"))
                || !targets.iter().any(|target| target.starts_with("unreal_"))
            {
                return Err(NaturePatchError::Validation(format!(
                    "manifest material recipe `{}` must target Unity and Unreal engine-native systems",
                    recipe.file
                )));
            }
            recipe_slots.insert(recipe.material_slot.as_str());
        }

        for slot in ["terrain_surface", "groundcover_foliage"] {
            if !recipe_slots.contains(slot) {
                return Err(NaturePatchError::Validation(format!(
                    "manifest.material_recipes must include {slot}"
                )));
            }
        }

        Ok(())
    }

    fn validate_engine_import_recipes(&self) -> Result<(), NaturePatchError> {
        let mut recipes_by_engine = BTreeMap::new();
        for recipe in &self.engine_import_recipes {
            if recipe.engine.trim().is_empty()
                || recipe.file.trim().is_empty()
                || recipe.profile.trim().is_empty()
                || recipe.runtime_policy.trim().is_empty()
            {
                return Err(NaturePatchError::Validation(
                    "manifest.engine_import_recipes must include engine, file, profile, and runtime_policy"
                        .to_string(),
                ));
            }
            if !recipe.file.starts_with("engines/") || !recipe.file.ends_with(".recipe.json") {
                return Err(NaturePatchError::Validation(format!(
                    "manifest engine import recipe `{}` must live under engines/*.recipe.json",
                    recipe.file
                )));
            }
            if recipe.runtime_policy != "engine_native_static" {
                return Err(NaturePatchError::Validation(format!(
                    "manifest engine import recipe `{}` must use engine_native_static runtime policy",
                    recipe.file
                )));
            }
            if recipe.expected_systems.is_empty()
                || recipe
                    .expected_systems
                    .iter()
                    .any(|system| system.trim().is_empty())
            {
                return Err(NaturePatchError::Validation(format!(
                    "manifest engine import recipe `{}` must include expected engine systems",
                    recipe.file
                )));
            }
            recipes_by_engine.insert(recipe.engine.as_str(), recipe);
        }

        for (engine, profile) in [("unity", "mobile"), ("unreal", "console")] {
            let recipe = recipes_by_engine.get(engine).ok_or_else(|| {
                NaturePatchError::Validation(format!(
                    "manifest.engine_import_recipes must include {engine}"
                ))
            })?;
            if recipe.profile != profile {
                return Err(NaturePatchError::Validation(format!(
                    "manifest engine import recipe `{}` must target {profile} profile",
                    recipe.file
                )));
            }
        }

        Ok(())
    }

    fn validate_surface_overlays(&self) -> Result<(), NaturePatchError> {
        for required in ["moss", "wetness", "cracks"] {
            if !self
                .surface_overlays
                .iter()
                .any(|overlay| overlay.name == required)
            {
                return Err(NaturePatchError::Validation(format!(
                    "manifest.surface_overlays must include {required}"
                )));
            }
        }

        for overlay in &self.surface_overlays {
            if overlay.name.trim().is_empty() {
                return Err(NaturePatchError::Validation(
                    "manifest.surface_overlays[].name must not be empty".to_string(),
                ));
            }
            if overlay.source_file != "maps/masks_rgba.png" {
                return Err(NaturePatchError::Validation(format!(
                    "manifest surface overlay `{}` must use maps/masks_rgba.png",
                    overlay.name
                )));
            }
            if !matches!(overlay.channel.as_str(), "R" | "G" | "B" | "A") {
                return Err(NaturePatchError::Validation(format!(
                    "manifest surface overlay `{}` must declare an RGBA channel",
                    overlay.name
                )));
            }
            if overlay.targets.is_empty()
                || overlay
                    .targets
                    .iter()
                    .any(|target| target.trim().is_empty())
            {
                return Err(NaturePatchError::Validation(format!(
                    "manifest surface overlay `{}` must include non-empty targets",
                    overlay.name
                )));
            }
            if overlay.application.trim().is_empty() || overlay.runtime_policy.trim().is_empty() {
                return Err(NaturePatchError::Validation(format!(
                    "manifest surface overlay `{}` must include application and runtime policy",
                    overlay.name
                )));
            }
            if !matches!(
                overlay.runtime_policy.as_str(),
                "baked_static" | "baked_optional_runtime"
            ) {
                return Err(NaturePatchError::Validation(format!(
                    "manifest surface overlay `{}` has unsupported runtime policy",
                    overlay.name
                )));
            }
        }

        let moss = self
            .surface_overlays
            .iter()
            .find(|overlay| overlay.name == "moss")
            .expect("moss overlay checked above");
        for target in ["terrain_surface", "rock", "log"] {
            if !moss.targets.iter().any(|item| item == target) {
                return Err(NaturePatchError::Validation(format!(
                    "manifest moss surface overlay must target {target}"
                )));
            }
        }

        Ok(())
    }

    fn validate_memory_footprint(&self) -> Result<(), NaturePatchError> {
        let footprint = &self.memory_footprint;
        if footprint.map_pixel_count != map_pixel_count(self.map_resolution) {
            return Err(NaturePatchError::Validation(format!(
                "manifest.memory_footprint.map_pixel_count must match map_resolution^2, got {}",
                footprint.map_pixel_count
            )));
        }
        if footprint.decoded_map_bytes != decoded_map_bytes(footprint.map_pixel_count) {
            return Err(NaturePatchError::Validation(format!(
                "manifest.memory_footprint.decoded_map_bytes must be {}",
                decoded_map_bytes(footprint.map_pixel_count)
            )));
        }
        let expected_scatter_header_bytes =
            self.scatter.binary_files.len() as u64 * SCATTER_BINARY_HEADER_BYTES as u64;
        if footprint.scatter_binary_header_bytes != expected_scatter_header_bytes {
            return Err(NaturePatchError::Validation(format!(
                "manifest.memory_footprint.scatter_binary_header_bytes must be {expected_scatter_header_bytes}"
            )));
        }
        let expected_scatter_record_bytes = self
            .scatter
            .binary_files
            .iter()
            .map(|file| file.instance_count as u64 * SCATTER_BINARY_RECORD_STRIDE_BYTES as u64)
            .sum::<u64>();
        if footprint.scatter_binary_record_bytes != expected_scatter_record_bytes {
            return Err(NaturePatchError::Validation(format!(
                "manifest.memory_footprint.scatter_binary_record_bytes must be {expected_scatter_record_bytes}"
            )));
        }
        if footprint.scatter_binary_bytes
            != footprint.scatter_binary_header_bytes + footprint.scatter_binary_record_bytes
        {
            return Err(NaturePatchError::Validation(
                "manifest.memory_footprint.scatter_binary_bytes must equal header plus record bytes"
                    .to_string(),
            ));
        }
        let expected_total = footprint
            .encoded_map_bytes
            .checked_add(footprint.material_recipe_bytes)
            .and_then(|value| value.checked_add(footprint.engine_import_recipe_bytes))
            .and_then(|value| value.checked_add(footprint.preview_mesh_bytes))
            .and_then(|value| value.checked_add(footprint.prototype_mesh_bytes))
            .and_then(|value| value.checked_add(footprint.scatter_json_bytes))
            .and_then(|value| value.checked_add(footprint.scatter_binary_bytes))
            .ok_or_else(|| {
                NaturePatchError::Validation(
                    "manifest.memory_footprint total byte count overflowed u64".to_string(),
                )
            })?;
        if footprint.total_payload_bytes != expected_total {
            return Err(NaturePatchError::Validation(format!(
                "manifest.memory_footprint.total_payload_bytes must be {expected_total}"
            )));
        }
        Ok(())
    }

    fn has_map_file(&self, file: &str) -> bool {
        self.map_channels.iter().any(|channel| channel.file == file)
    }
}

impl NaturePatch {
    /// Load a nature patch from TOML text and validate semantic constraints.
    pub fn from_toml(toml_str: &str) -> Result<Self, NaturePatchError> {
        let patch: Self = toml::from_str(toml_str)?;
        patch.validate()?;
        Ok(patch)
    }

    /// Load a nature patch from disk.
    pub fn from_file(path: &Path) -> Result<Self, NaturePatchError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml(&content)
    }

    /// Validate constraints that serde cannot express.
    pub fn validate(&self) -> Result<(), NaturePatchError> {
        if self.asset.kind != "nature_patch" {
            return Err(NaturePatchError::Validation(format!(
                "asset.kind must be `nature_patch`, got `{}`",
                self.asset.kind
            )));
        }
        if self.asset.name.trim().is_empty() {
            return Err(NaturePatchError::Validation(
                "asset.name must not be empty".to_string(),
            ));
        }
        validate_positive("patch.size", self.patch.size)?;
        validate_non_negative("soil.mound_scale", self.soil.mound_scale)?;
        validate_non_negative("soil.mound_height", self.soil.mound_height)?;
        validate_unit("soil.mound_coverage", self.soil.mound_coverage)?;
        validate_non_negative("soil.relief_scale", self.soil.relief_scale)?;
        validate_non_negative("soil.relief_strength", self.soil.relief_strength)?;
        validate_unit("soil.wetness_coverage", self.soil.wetness_coverage)?;
        validate_non_negative("soil.wetness_scale", self.soil.wetness_scale)?;
        validate_non_negative("soil.cracks.amount", self.soil.cracks.amount)?;
        validate_non_negative("soil.cracks.plate_density", self.soil.cracks.plate_density)?;
        validate_non_negative("soil.cracks.channel_width", self.soil.cracks.channel_width)?;
        validate_non_negative("soil.cracks.depth", self.soil.cracks.depth)?;

        for (index, layer) in self.groundcover.layers.iter().enumerate() {
            let prefix = format!("groundcover.layers[{index}]");
            validate_non_negative(&format!("{prefix}.density"), layer.density)?;
            validate_unit(&format!("{prefix}.coverage"), layer.coverage)?;
            validate_positive(&format!("{prefix}.patch_scale"), layer.patch_scale)?;
            validate_non_negative(&format!("{prefix}.patch_softness"), layer.patch_softness)?;
            validate_non_negative(&format!("{prefix}.height"), layer.height)?;
            validate_non_negative(&format!("{prefix}.width"), layer.width)?;
            validate_non_negative(&format!("{prefix}.relief_scale"), layer.relief_scale)?;
            validate_non_negative(&format!("{prefix}.relief_strength"), layer.relief_strength)?;
        }

        self.profiles.mobile.validate("profiles.mobile")?;
        self.profiles.console.validate("profiles.console")?;
        Ok(())
    }

    /// Create a deterministic terrain sampler from this patch.
    pub fn terrain_field(&self) -> TerrainField {
        TerrainField::new(self.clone())
    }

    /// Build a manifest for the default nature package settings.
    pub fn export_manifest(&self, map_resolution: u32) -> NatureExportManifest {
        let config = NaturePackageConfig {
            map_resolution,
            ..NaturePackageConfig::default()
        };
        let scatter_sets = self.scatter_sets_for_package(&config);
        let maps = self.terrain_field().bake_maps(config.map_resolution);
        self.export_manifest_for_package(&config, &scatter_sets, &maps)
    }

    fn export_manifest_for_package(
        &self,
        config: &NaturePackageConfig,
        scatter_sets: &[ScatterSet],
        maps: &NatureMapSet,
    ) -> NatureExportManifest {
        let half_tile = self.patch.size * 0.5;
        NatureExportManifest {
            schema_version: 3,
            generator: "midori".to_string(),
            generator_version: env!("CARGO_PKG_VERSION").to_string(),
            asset_name: self.asset.name.clone(),
            seed: self.patch.seed,
            units: self.asset.units.clone(),
            tile_size: self.patch.size,
            axis: AxisConventions {
                up_axis: "Y".to_string(),
                forward_axis: "Z".to_string(),
                handedness: "right".to_string(),
                unit_scale: 1.0,
            },
            map_resolution: config.map_resolution,
            terrain: TerrainManifest {
                heightmap_file: "maps/height_u16.png".to_string(),
                masks_file: "maps/masks_rgba.png".to_string(),
                grass_density_file: "maps/grass_density.png".to_string(),
                height_min: maps.min_height,
                height_max: maps.max_height,
                bounds_min: [-half_tile, maps.min_height, -half_tile],
                bounds_max: [half_tile, maps.max_height, half_tile],
            },
            map_channels: nature_map_channels(),
            normal_conventions: NormalConventions {
                unity_yplus_file: "maps/normal_yplus.png".to_string(),
                unreal_yminus_file: "maps/normal_yminus.png".to_string(),
            },
            material_slots: nature_material_slots(&self.profiles),
            material_parameters: nature_material_parameters(&self.wind, &self.profiles),
            material_recipes: nature_material_recipes(),
            engine_import_recipes: nature_engine_import_recipes(),
            surface_overlays: nature_surface_overlays(),
            groundcover_layers: self.groundcover.layers.clone(),
            prototypes: if config.write_prototype_meshes {
                prototype_manifests(&self.generate_groundcover_prototypes())
            } else {
                Vec::new()
            },
            scatter: ScatterManifest {
                enabled: config.write_scatter_json || config.write_scatter_binary,
                file: config
                    .write_scatter_json
                    .then(|| "instances/scatter.json".to_string()),
                format: "json.chunked_instances.v1".to_string(),
                binary_files: if config.write_scatter_binary {
                    scatter_binary_file_manifests(scatter_sets)
                } else {
                    Vec::new()
                },
                binary_format: scatter_binary_format_manifest(),
                chunk_size: config.scatter_chunk_size,
                fields: vec![
                    "position.xyz".to_string(),
                    "yaw_radians".to_string(),
                    "height_multiplier".to_string(),
                    "width_multiplier".to_string(),
                    "phase_radians".to_string(),
                    "color_variation".to_string(),
                ],
            },
            wind_packing: WindPackingManifest {
                phase: "TEXCOORD_1.x".to_string(),
                stiffness: "TEXCOORD_1.y and COLOR_0.w".to_string(),
                height: "COLOR_0.x".to_string(),
                color_variation: "COLOR_0.y".to_string(),
                normalized_progress: "COLOR_0.z".to_string(),
            },
            memory_footprint: estimated_memory_footprint(config, scatter_sets),
            mobile: self.profiles.mobile.clone(),
            console: self.profiles.console.clone(),
            unity: UnityImportHints {
                terrain_heightmap: "maps/height_u16.png".to_string(),
                detail_density_map: "maps/grass_density.png".to_string(),
                normal_map: "maps/normal_yplus.png".to_string(),
                detail_mode: "GPU-instanced terrain detail mesh prefabs".to_string(),
                detail_batch_max_instances: 1023,
            },
            unreal: UnrealImportHints {
                landscape_heightmap: "maps/height_u16.png".to_string(),
                landscape_weightmap: "maps/masks_rgba.png".to_string(),
                normal_map: "maps/normal_yminus.png".to_string(),
                foliage_mode: "Static Mesh Foliage or Landscape Grass Type".to_string(),
                static_mesh_pipeline:
                    "GLB prototypes first; FBX adapter only where pipeline requires it".to_string(),
            },
            shader_policy: "preview_only".to_string(),
            texture_pipeline: "parked".to_string(),
        }
    }

    /// Write an initial engine-friendly nature package directory.
    pub fn write_package(
        &self,
        directory: &Path,
        config: &NaturePackageConfig,
    ) -> Result<NaturePackageSummary, NaturePatchError> {
        std::fs::create_dir_all(directory)?;

        let maps_directory = directory.join("maps");
        let field = self.terrain_field();
        let maps = field.write_maps(&maps_directory, config.map_resolution)?;
        let scatter_sets = self.scatter_sets_for_package(config);

        let preview_mesh_path = if config.write_preview_mesh {
            let path = directory.join("preview_tile.glb");
            let mesh = field.build_preview_mesh(config.preview_resolution);
            export_mesh(&mesh, &path, &nature_mesh_export_config())?;
            Some(path)
        } else {
            None
        };

        let mut prototype_meshes = Vec::new();
        if config.write_prototype_meshes {
            let prototype_directory = directory.join("prototypes");
            std::fs::create_dir_all(&prototype_directory)?;
            for prototype in self.generate_groundcover_prototypes() {
                for lod in prototype.lods {
                    let path = prototype_directory
                        .join(prototype_lod_filename(&prototype.name, lod.index));
                    export_mesh(&lod.mesh, &path, &nature_mesh_export_config())?;
                    prototype_meshes.push(path);
                }
            }
        }

        let mut scatter_instance_count = 0;
        let scatter_path = if config.write_scatter_json {
            scatter_instance_count = scatter_sets
                .iter()
                .flat_map(|set| &set.chunks)
                .map(|chunk| chunk.instances.len())
                .sum();
            let instance_directory = directory.join("instances");
            std::fs::create_dir_all(&instance_directory)?;
            let path = instance_directory.join("scatter.json");
            let scatter_file = std::fs::File::create(&path)?;
            serde_json::to_writer_pretty(scatter_file, &scatter_sets)?;
            Some(path)
        } else {
            None
        };

        let scatter_binary_paths = if config.write_scatter_binary {
            scatter_instance_count = scatter_sets
                .iter()
                .flat_map(|set| &set.chunks)
                .map(|chunk| chunk.instances.len())
                .sum();
            write_scatter_binary_files(directory, &scatter_sets)?
        } else {
            Vec::new()
        };

        let manifest_path = directory.join("midori_nature.json");
        let mut manifest = self.export_manifest_for_package(config, &scatter_sets, &maps);
        let material_recipe_paths = write_material_recipe_files(directory, &manifest)?;
        let engine_import_recipe_paths = write_engine_import_recipe_files(directory, &manifest)?;
        manifest.memory_footprint = package_memory_footprint(
            directory,
            config,
            preview_mesh_path.as_deref(),
            &material_recipe_paths,
            &engine_import_recipe_paths,
            &prototype_meshes,
            scatter_path.as_deref(),
            &scatter_binary_paths,
        )?;
        manifest.validate()?;
        let manifest_file = std::fs::File::create(&manifest_path)?;
        serde_json::to_writer_pretty(manifest_file, &manifest)?;

        Ok(NaturePackageSummary {
            manifest_path,
            maps_directory,
            preview_mesh_path,
            prototype_meshes,
            scatter_path,
            scatter_binary_paths,
            material_recipe_paths,
            engine_import_recipe_paths,
            map_checksum: maps.checksum(),
            scatter_instance_count,
        })
    }

    /// Generate reusable groundcover prototype meshes from supported layers.
    pub fn generate_groundcover_prototypes(&self) -> Vec<GroundcoverPrototype> {
        self.groundcover
            .layers
            .iter()
            .filter_map(|layer| match layer.kind {
                GroundcoverKind::Grass => Some(generate_grass_prototype(layer)),
                GroundcoverKind::Moss => Some(generate_moss_prototype(layer)),
                GroundcoverKind::Flower => Some(generate_flower_prototype(layer)),
                GroundcoverKind::Weed => Some(generate_weed_prototype(layer)),
                GroundcoverKind::Litter => Some(generate_litter_prototype(layer)),
                GroundcoverKind::Shrub => Some(generate_shrub_prototype(layer)),
                GroundcoverKind::Rock => Some(generate_rock_prototype(layer)),
                GroundcoverKind::Log => Some(generate_log_prototype(layer)),
            })
            .collect()
    }

    /// Generate deterministic chunked scatter sets for each groundcover layer.
    pub fn generate_scatter_sets(&self, chunk_size: f32) -> Vec<ScatterSet> {
        let field = self.terrain_field();
        self.groundcover
            .layers
            .iter()
            .enumerate()
            .map(|(index, layer)| field.scatter_layer(index, layer, chunk_size))
            .collect()
    }

    fn scatter_sets_for_package(&self, config: &NaturePackageConfig) -> Vec<ScatterSet> {
        if config.write_scatter_json || config.write_scatter_binary {
            self.generate_scatter_sets(config.scatter_chunk_size)
        } else {
            Vec::new()
        }
    }
}

impl NatureProfile {
    fn validate(&self, prefix: &str) -> Result<(), NaturePatchError> {
        validate_positive(&format!("{prefix}.tile_size"), self.tile_size)?;
        if self.terrain_resolution < 2 {
            return Err(NaturePatchError::Validation(format!(
                "{prefix}.terrain_resolution must be at least 2"
            )));
        }
        if self.lod0_max_triangles == 0
            || self.lod1_max_triangles == 0
            || self.lod2_max_triangles == 0
        {
            return Err(NaturePatchError::Validation(format!(
                "{prefix} LOD triangle budgets must be positive"
            )));
        }
        validate_positive(
            &format!("{prefix}.lod0_max_distance"),
            self.lod0_max_distance,
        )?;
        validate_positive(
            &format!("{prefix}.lod1_max_distance"),
            self.lod1_max_distance,
        )?;
        validate_positive(
            &format!("{prefix}.lod2_max_distance"),
            self.lod2_max_distance,
        )?;
        if self.lod0_max_distance > self.lod1_max_distance
            || self.lod1_max_distance > self.lod2_max_distance
        {
            return Err(NaturePatchError::Validation(format!(
                "{prefix} LOD distances must be ordered lod0 <= lod1 <= lod2"
            )));
        }
        if self.material_slots == 0 {
            return Err(NaturePatchError::Validation(format!(
                "{prefix}.material_slots must be positive"
            )));
        }
        if self.max_instances_per_tile == 0 || self.max_instances_per_chunk == 0 {
            return Err(NaturePatchError::Validation(format!(
                "{prefix} instance budgets must be positive"
            )));
        }
        validate_non_negative(&format!("{prefix}.density_scale"), self.density_scale)?;
        validate_non_negative(&format!("{prefix}.cull_start"), self.cull_start)?;
        validate_non_negative(&format!("{prefix}.cull_end"), self.cull_end)?;
        if self.cull_end < self.cull_start {
            return Err(NaturePatchError::Validation(format!(
                "{prefix}.cull_end must be >= cull_start"
            )));
        }
        if self.lod2_max_distance > self.cull_end {
            return Err(NaturePatchError::Validation(format!(
                "{prefix}.lod2_max_distance must be <= cull_end"
            )));
        }
        Ok(())
    }
}

impl TerrainField {
    /// Create a terrain field from a validated patch.
    pub fn new(patch: NaturePatch) -> Self {
        Self { patch }
    }

    /// Source patch.
    pub fn patch(&self) -> &NaturePatch {
        &self.patch
    }

    /// Sample terrain height, normal, and masks at world-space X/Z.
    pub fn sample(&self, x: f32, z: f32) -> TerrainSample {
        let height = self.height_at(x, z);
        TerrainSample {
            x,
            z,
            height,
            normal: self.normal_at(x, z),
            masks: self.masks_at(x, z),
        }
    }

    /// Sample terrain height at world-space X/Z.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let soil = &self.patch.soil;
        let base = fbm(
            x * soil.mound_scale,
            z * soil.mound_scale,
            self.patch.patch.seed ^ 0x11,
        );
        let drift = fbm(
            x * soil.relief_scale + 17.0,
            z * soil.relief_scale - 31.0,
            self.patch.patch.seed ^ 0x23,
        );
        let relief = 1.0 - 0.4 * soil.relief_strength + 0.4 * soil.relief_strength * drift;
        let threshold = lerp(1.0 + soil.mound_edge, -soil.mound_edge, soil.mound_coverage);
        let coverage = smoothstep(
            threshold - soil.mound_edge,
            threshold + soil.mound_edge,
            base,
        );
        let edge = self.edge_taper(x, z);
        let crack = self.crack_mask_at(x, z);

        soil.mound_height * base * relief * coverage * edge + self.moss_height_at(x, z)
            - crack * soil.cracks.depth * 0.08
    }

    /// Sample terrain normal from the same height field.
    pub fn normal_at(&self, x: f32, z: f32) -> Vec3 {
        let eps = (self.patch.patch.size / 512.0).max(0.01);
        let h0 = self.height_at(x, z);
        let hx = self.height_at(x + eps, z);
        let hz = self.height_at(x, z + eps);
        Vec3::new(-(hx - h0) / eps, 1.0, -(hz - h0) / eps).normalize()
    }

    /// Sample terrain masks at world-space X/Z.
    pub fn masks_at(&self, x: f32, z: f32) -> MaskSample {
        MaskSample {
            grass_density: self.grass_density_at(x, z),
            moss: self.moss_mask_at(x, z),
            wetness: self.wetness_mask_at(x, z),
            crack: self.crack_mask_at(x, z),
        }
    }

    /// Grass density from the first grass layer.
    pub fn grass_density_at(&self, x: f32, z: f32) -> f32 {
        self.layer_mask_at(GroundcoverKind::Grass, x, z)
            .map(|(layer, mask)| (layer.density * mask).clamp(0.0, 1.0))
            .unwrap_or(0.0)
    }

    /// Moss coverage from the first moss layer.
    pub fn moss_mask_at(&self, x: f32, z: f32) -> f32 {
        self.layer_mask_at(GroundcoverKind::Moss, x, z)
            .map(|(_, mask)| mask)
            .unwrap_or(0.0)
    }

    /// Wetness coverage mask.
    pub fn wetness_mask_at(&self, x: f32, z: f32) -> f32 {
        let soil = &self.patch.soil;
        if soil.wetness_coverage <= 0.0 {
            return 0.0;
        }
        let n = fbm(
            x * soil.wetness_scale + 29.0,
            z * soil.wetness_scale - 11.0,
            self.patch.patch.seed ^ 0x31,
        );
        let threshold = lerp(
            1.0 + soil.wetness_edge,
            -soil.wetness_edge,
            soil.wetness_coverage,
        );
        smoothstep(
            threshold - soil.wetness_edge,
            threshold + soil.wetness_edge,
            n,
        )
    }

    /// Crack coverage mask.
    pub fn crack_mask_at(&self, x: f32, z: f32) -> f32 {
        let cracks = &self.patch.soil.cracks;
        if !cracks.enabled || cracks.amount <= 0.0 {
            return 0.0;
        }

        let warp_x = fbm(
            x * cracks.plate_density * 0.5 + 3.1,
            z * cracks.plate_density * 0.5 + 7.7,
            self.patch.patch.seed ^ 0x41,
        ) - 0.5;
        let warp_z = fbm(
            x * cracks.plate_density * 0.5 - 5.3,
            z * cracks.plate_density * 0.5 + 9.9,
            self.patch.patch.seed ^ 0x43,
        ) - 0.5;
        let px = x * cracks.plate_density + warp_x * cracks.warp;
        let pz = z * cracks.plate_density + warp_z * cracks.warp;
        let (f1, f2) = worley_f1_f2(px, pz, self.patch.patch.seed ^ 0x47);
        let width = cracks.channel_width.max(0.001);
        let primary = 1.0 - smoothstep(0.0, width, f2 - f1);
        let (s1, s2) = worley_f1_f2(
            px * 2.7 + 13.0,
            pz * 2.7 - 4.0,
            self.patch.patch.seed ^ 0x53,
        );
        let secondary = (1.0 - smoothstep(0.0, width * 1.6, s2 - s1)) * 0.5;
        primary.max(secondary).clamp(0.0, 1.0) * cracks.amount
    }

    /// Bake height, normal, and mask maps into memory.
    pub fn bake_maps(&self, resolution: u32) -> NatureMapSet {
        assert!(resolution >= 2, "resolution must be at least 2");

        let mut heights = Vec::with_capacity((resolution * resolution) as usize);
        let mut normals = Vec::with_capacity((resolution * resolution) as usize);
        let mut masks = Vec::with_capacity((resolution * resolution) as usize);

        let mut min_height = f32::MAX;
        let mut max_height = f32::MIN;
        for z in 0..resolution {
            for x in 0..resolution {
                let (wx, wz) = self.grid_position(x, z, resolution);
                let sample = self.sample(wx, wz);
                min_height = min_height.min(sample.height);
                max_height = max_height.max(sample.height);
                heights.push(sample.height);
                normals.push(sample.normal);
                masks.push(sample.masks);
            }
        }

        let height_range = (max_height - min_height).max(0.0001);
        let mut height_u16 = Vec::with_capacity(heights.len());
        let mut normal_yplus = Vec::with_capacity(normals.len());
        let mut normal_yminus = Vec::with_capacity(normals.len());
        let mut masks_rgba = Vec::with_capacity(masks.len());
        let mut grass_density = Vec::with_capacity(masks.len());

        for ((height, normal), mask) in heights.into_iter().zip(normals).zip(masks) {
            let h = ((height - min_height) / height_range).clamp(0.0, 1.0);
            height_u16.push((h * u16::MAX as f32).round() as u16);
            normal_yplus.push(encode_terrain_normal(normal, false));
            normal_yminus.push(encode_terrain_normal(normal, true));

            let grass = to_u8(mask.grass_density);
            grass_density.push(grass);
            masks_rgba.push([
                grass,
                to_u8(mask.moss),
                to_u8(mask.wetness),
                to_u8(mask.crack),
            ]);
        }

        NatureMapSet {
            resolution,
            min_height,
            max_height,
            height_u16,
            normal_yplus,
            normal_yminus,
            masks_rgba,
            grass_density,
        }
    }

    /// Write baked maps to a directory.
    pub fn write_maps(
        &self,
        directory: &Path,
        resolution: u32,
    ) -> Result<NatureMapSet, NaturePatchError> {
        std::fs::create_dir_all(directory)?;
        let maps = self.bake_maps(resolution);
        maps.write_to_directory(directory)?;
        Ok(maps)
    }

    /// Build a preview terrain mesh from the deterministic field.
    pub fn build_preview_mesh(&self, resolution: u32) -> Mesh {
        assert!(resolution >= 2, "resolution must be at least 2");

        let mut mesh = Mesh::new();
        mesh.vertices.reserve((resolution * resolution) as usize);
        mesh.indices
            .reserve(((resolution - 1) * (resolution - 1) * 6) as usize);

        for z in 0..resolution {
            for x in 0..resolution {
                let (wx, wz) = self.grid_position(x, z, resolution);
                let sample = self.sample(wx, wz);
                let u = x as f32 / (resolution - 1) as f32;
                let v = z as f32 / (resolution - 1) as f32;
                mesh.vertices.push(Vertex {
                    position: Vec3::new(wx, sample.height, wz),
                    normal: sample.normal,
                    uv: Vec2::new(u, v),
                    uv2: Vec2::new(sample.masks.grass_density, sample.masks.moss),
                    color: Vec4::new(
                        sample.masks.grass_density,
                        sample.masks.moss,
                        sample.masks.wetness,
                        sample.masks.crack,
                    ),
                });
            }
        }

        for z in 0..(resolution - 1) {
            for x in 0..(resolution - 1) {
                let i0 = z * resolution + x;
                let i1 = i0 + 1;
                let i2 = i0 + resolution;
                let i3 = i2 + 1;
                mesh.indices.extend_from_slice(&[i0, i2, i1, i1, i2, i3]);
            }
        }

        mesh.submeshes.push(Submesh {
            index_start: 0,
            index_count: mesh.indices.len() as u32,
            material: MaterialType::Bark,
        });
        mesh
    }

    /// Generate deterministic chunked placements for one groundcover layer.
    pub fn scatter_layer(
        &self,
        layer_index: usize,
        layer: &GroundcoverLayer,
        chunk_size: f32,
    ) -> ScatterSet {
        assert!(chunk_size > 0.0, "chunk_size must be positive");

        let size = self.patch.patch.size;
        let half = size * 0.5;
        let candidates = ((size * size * layer.density.max(0.0) * 25.0).round() as u32).max(1);
        let mut rng = Rng::from_seed(
            self.patch.patch.seed
                ^ layer_seed(layer.kind)
                ^ ((layer_index as u64 + 1) * 0x9E37_79B9),
        );
        let mut chunks: BTreeMap<(i32, i32), Vec<ScatterInstance>> = BTreeMap::new();

        for _ in 0..candidates {
            let x = rng.range(-half, half);
            let z = rng.range(-half, half);
            let mask = match layer.kind {
                GroundcoverKind::Grass => self.grass_density_at(x, z),
                GroundcoverKind::Moss => self.moss_mask_at(x, z) * layer.density.clamp(0.0, 1.0),
                GroundcoverKind::Flower
                | GroundcoverKind::Weed
                | GroundcoverKind::Litter
                | GroundcoverKind::Shrub
                | GroundcoverKind::Rock
                | GroundcoverKind::Log => self
                    .layer_mask_at(layer.kind, x, z)
                    .map(|(_, mask)| mask * layer.density.clamp(0.0, 1.0))
                    .unwrap_or(0.0),
            };

            if rng.next_f32() > mask {
                continue;
            }

            let y = self.height_at(x, z);
            let instance = ScatterInstance {
                position: [x, y, z],
                yaw: rng.range(0.0, crate::constants::TAU),
                height: rng.range(0.8, 1.25),
                width: rng.range(0.75, 1.2),
                phase: rng.range(0.0, crate::constants::TAU),
                color_variation: rng.next_f32(),
            };
            let chunk_x = ((x + half) / chunk_size).floor() as i32;
            let chunk_z = ((z + half) / chunk_size).floor() as i32;
            chunks.entry((chunk_x, chunk_z)).or_default().push(instance);
        }

        let chunks = chunks
            .into_iter()
            .map(|((chunk_x, chunk_z), instances)| {
                let mut bounds_min = [f32::MAX; 3];
                let mut bounds_max = [f32::MIN; 3];
                let radius = scatter_radius_for_layer(layer);
                let height = scatter_height_for_layer(layer);
                for instance in &instances {
                    let radius = radius * instance.width;
                    let height = height * instance.height;
                    bounds_min[0] = bounds_min[0].min(instance.position[0] - radius);
                    bounds_min[1] = bounds_min[1].min(instance.position[1]);
                    bounds_min[2] = bounds_min[2].min(instance.position[2] - radius);
                    bounds_max[0] = bounds_max[0].max(instance.position[0] + radius);
                    bounds_max[1] = bounds_max[1].max(instance.position[1] + height);
                    bounds_max[2] = bounds_max[2].max(instance.position[2] + radius);
                }
                ScatterChunk {
                    chunk_x,
                    chunk_z,
                    bounds_min,
                    bounds_max,
                    instances,
                }
            })
            .collect();

        ScatterSet {
            layer_index,
            layer_name: if layer.name.is_empty() {
                format!("{:?}", layer.kind).to_lowercase()
            } else {
                layer.name.clone()
            },
            kind: layer.kind,
            chunks,
        }
    }

    fn layer_mask_at(
        &self,
        kind: GroundcoverKind,
        x: f32,
        z: f32,
    ) -> Option<(&GroundcoverLayer, f32)> {
        let layer = self
            .patch
            .groundcover
            .layers
            .iter()
            .find(|layer| layer.kind == kind)?;
        let n = fbm(
            x * layer.patch_scale + layer.seed_offset,
            z * layer.patch_scale - layer.seed_offset,
            self.patch.patch.seed ^ layer_seed(kind),
        );
        let edge = layer.patch_softness.max(0.001);
        let threshold = lerp(1.0 + edge, -edge, layer.coverage);
        let mask = smoothstep(threshold - edge, threshold + edge, n) * self.edge_taper(x, z);
        Some((layer, mask.clamp(0.0, 1.0)))
    }

    fn moss_height_at(&self, x: f32, z: f32) -> f32 {
        let Some((layer, mask)) = self.layer_mask_at(GroundcoverKind::Moss, x, z) else {
            return 0.0;
        };
        let drift = fbm(
            x * layer.relief_scale + 31.7,
            z * layer.relief_scale - 18.0,
            self.patch.patch.seed ^ 0x61,
        );
        let relief = 1.0 - 0.4 * layer.relief_strength + 0.4 * layer.relief_strength * drift;
        layer.height * mask * relief * self.edge_taper(x, z)
    }

    fn edge_taper(&self, x: f32, z: f32) -> f32 {
        let half = self.patch.patch.size * 0.5;
        let inner = half * 0.8;
        let fade_width = (half - inner).max(0.0001);
        let tx = ((half - x.abs()) / fade_width).clamp(0.0, 1.0);
        let tz = ((half - z.abs()) / fade_width).clamp(0.0, 1.0);
        smoothstep(0.0, 1.0, tx) * smoothstep(0.0, 1.0, tz)
    }

    fn grid_position(&self, x: u32, z: u32, resolution: u32) -> (f32, f32) {
        let size = self.patch.patch.size;
        let half = size * 0.5;
        let u = x as f32 / (resolution - 1) as f32;
        let v = z as f32 / (resolution - 1) as f32;
        (lerp(-half, half, u), lerp(-half, half, v))
    }
}

impl NatureMapSet {
    /// Write all baked maps to a directory with the canonical Midori names.
    pub fn write_to_directory(&self, directory: &Path) -> Result<(), NaturePatchError> {
        std::fs::create_dir_all(directory)?;
        let res = self.resolution;

        let height_img: ImageBuffer<Luma<u16>, Vec<u16>> =
            ImageBuffer::from_vec(res, res, self.height_u16.clone()).expect("valid height map");
        height_img.save(directory.join("height_u16.png"))?;

        let normal_yplus = flatten_rgb(&self.normal_yplus);
        let img = RgbImage::from_vec(res, res, normal_yplus).expect("valid normal map");
        img.save(directory.join("normal_yplus.png"))?;

        let normal_yminus = flatten_rgb(&self.normal_yminus);
        let img = RgbImage::from_vec(res, res, normal_yminus).expect("valid normal map");
        img.save(directory.join("normal_yminus.png"))?;

        let masks = flatten_rgba(&self.masks_rgba);
        let img = RgbaImage::from_vec(res, res, masks).expect("valid masks map");
        img.save(directory.join("masks_rgba.png"))?;

        let img =
            GrayImage::from_vec(res, res, self.grass_density.clone()).expect("valid density map");
        img.save(directory.join("grass_density.png"))?;
        Ok(())
    }

    /// Stable FNV-1a checksum over baked map data for fixtures.
    pub fn checksum(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        for value in &self.height_u16 {
            for byte in value.to_le_bytes() {
                hash = fnv1a(hash, byte);
            }
        }
        for normal in &self.normal_yplus {
            for byte in normal {
                hash = fnv1a(hash, *byte);
            }
        }
        for mask in &self.masks_rgba {
            for byte in mask {
                hash = fnv1a(hash, *byte);
            }
        }
        hash
    }
}

impl Default for NaturePackageConfig {
    fn default() -> Self {
        Self {
            map_resolution: 64,
            preview_resolution: 32,
            scatter_chunk_size: 8.0,
            write_preview_mesh: true,
            write_prototype_meshes: true,
            write_scatter_json: true,
            write_scatter_binary: true,
        }
    }
}

impl Default for SoilParams {
    fn default() -> Self {
        Self {
            profile: default_soil_profile(),
            mound_scale: default_mound_scale(),
            mound_height: default_mound_height(),
            mound_coverage: default_one(),
            mound_edge: default_patch_softness(),
            relief_scale: default_relief_scale(),
            relief_strength: default_relief_strength(),
            wetness_coverage: 0.0,
            wetness_scale: default_wetness_scale(),
            wetness_edge: default_wetness_edge(),
            cracks: SoilCracks::default(),
        }
    }
}

impl Default for SoilCracks {
    fn default() -> Self {
        Self {
            enabled: false,
            amount: default_crack_amount(),
            plate_density: default_crack_plate_density(),
            channel_width: default_crack_width(),
            warp: 0.0,
            depth: default_crack_depth(),
        }
    }
}

impl Default for WindParams {
    fn default() -> Self {
        Self {
            direction_degrees: 0.0,
            strength: default_wind_strength(),
            speed: default_wind_speed(),
            gust_scale: default_gust_scale(),
            flutter: default_flutter(),
        }
    }
}

impl Default for NatureProfiles {
    fn default() -> Self {
        Self {
            mobile: default_mobile_profile(),
            console: default_console_profile(),
        }
    }
}

fn validate_positive(name: &str, value: f32) -> Result<(), NaturePatchError> {
    if value > 0.0 && value.is_finite() {
        Ok(())
    } else {
        Err(NaturePatchError::Validation(format!(
            "{name} must be positive"
        )))
    }
}

fn validate_non_negative(name: &str, value: f32) -> Result<(), NaturePatchError> {
    if value >= 0.0 && value.is_finite() {
        Ok(())
    } else {
        Err(NaturePatchError::Validation(format!(
            "{name} must be non-negative"
        )))
    }
}

fn validate_unit(name: &str, value: f32) -> Result<(), NaturePatchError> {
    if (0.0..=1.0).contains(&value) && value.is_finite() {
        Ok(())
    } else {
        Err(NaturePatchError::Validation(format!(
            "{name} must be in 0..=1"
        )))
    }
}

fn validate_non_empty_file(name: &str, value: &str) -> Result<(), NaturePatchError> {
    if value.trim().is_empty() {
        Err(NaturePatchError::Validation(format!(
            "{name} must not be empty"
        )))
    } else {
        Ok(())
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if (edge1 - edge0).abs() < 0.000001 {
        return if x >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn fbm(x: f32, z: f32, seed: u64) -> f32 {
    let mut value = 0.0;
    let mut amplitude = 0.5;
    let mut total = 0.0;
    let mut frequency = 1.0;
    for octave in 0..5 {
        value += amplitude * value_noise(x * frequency, z * frequency, seed ^ octave);
        total += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    (value / total).clamp(0.0, 1.0)
}

fn value_noise(x: f32, z: f32, seed: u64) -> f32 {
    let xi = x.floor() as i32;
    let zi = z.floor() as i32;
    let xf = x - xi as f32;
    let zf = z - zi as f32;
    let u = smoothstep(0.0, 1.0, xf);
    let v = smoothstep(0.0, 1.0, zf);

    let a = hash01(xi, zi, seed);
    let b = hash01(xi + 1, zi, seed);
    let c = hash01(xi, zi + 1, seed);
    let d = hash01(xi + 1, zi + 1, seed);
    lerp(lerp(a, b, u), lerp(c, d, u), v)
}

fn worley_f1_f2(x: f32, z: f32, seed: u64) -> (f32, f32) {
    let xi = x.floor() as i32;
    let zi = z.floor() as i32;
    let xf = x - xi as f32;
    let zf = z - zi as f32;
    let mut f1 = f32::MAX;
    let mut f2 = f32::MAX;

    for dz in -1..=1 {
        for dx in -1..=1 {
            let cell_x = xi + dx;
            let cell_z = zi + dz;
            let ox = hash01(cell_x, cell_z, seed ^ 0x71);
            let oz = hash01(cell_x, cell_z, seed ^ 0x73);
            let px = dx as f32 + ox - xf;
            let pz = dz as f32 + oz - zf;
            let distance = (px * px + pz * pz).sqrt();
            if distance < f1 {
                f2 = f1;
                f1 = distance;
            } else if distance < f2 {
                f2 = distance;
            }
        }
    }

    (f1, f2)
}

fn hash01(x: i32, z: i32, seed: u64) -> f32 {
    let mut h = seed;
    h ^= (x as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= (z as i64 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = splitmix64(h);
    ((h >> 40) as f32) / ((1u64 << 24) as f32)
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

fn layer_seed(kind: GroundcoverKind) -> u64 {
    match kind {
        GroundcoverKind::Grass => 0x100,
        GroundcoverKind::Moss => 0x200,
        GroundcoverKind::Flower => 0x300,
        GroundcoverKind::Weed => 0x400,
        GroundcoverKind::Litter => 0x500,
        GroundcoverKind::Shrub => 0x600,
        GroundcoverKind::Rock => 0x700,
        GroundcoverKind::Log => 0x800,
    }
}

fn scatter_radius_for_layer(layer: &GroundcoverLayer) -> f32 {
    match layer.kind {
        GroundcoverKind::Log => layer.height.max(layer.width).max(0.2) * 0.55,
        GroundcoverKind::Shrub => layer.width.max(0.08) * 2.4,
        GroundcoverKind::Rock => layer.width.max(0.08),
        GroundcoverKind::Grass
        | GroundcoverKind::Moss
        | GroundcoverKind::Flower
        | GroundcoverKind::Weed
        | GroundcoverKind::Litter => layer.width.max(0.05),
    }
}

fn scatter_height_for_layer(layer: &GroundcoverLayer) -> f32 {
    match layer.kind {
        GroundcoverKind::Log => layer.width.max(0.05) * 2.4,
        GroundcoverKind::Rock => layer.height.max(0.05),
        GroundcoverKind::Grass
        | GroundcoverKind::Moss
        | GroundcoverKind::Flower
        | GroundcoverKind::Weed
        | GroundcoverKind::Litter
        | GroundcoverKind::Shrub => layer.height.max(0.05),
    }
}

fn nature_map_channels() -> Vec<MapChannel> {
    vec![
        MapChannel {
            file: "height_u16.png".to_string(),
            channels: "16-bit normalized terrain height".to_string(),
        },
        MapChannel {
            file: "normal_yplus.png".to_string(),
            channels: "RGB terrain normal, green channel Y+".to_string(),
        },
        MapChannel {
            file: "normal_yminus.png".to_string(),
            channels: "RGB terrain normal, green channel Y-".to_string(),
        },
        MapChannel {
            file: "masks_rgba.png".to_string(),
            channels: "R grass density, G moss, B wetness, A cracks".to_string(),
        },
        MapChannel {
            file: "grass_density.png".to_string(),
            channels: "single-channel grass density".to_string(),
        },
    ]
}

fn estimated_memory_footprint(
    config: &NaturePackageConfig,
    scatter_sets: &[ScatterSet],
) -> MemoryFootprintManifest {
    let map_pixel_count = map_pixel_count(config.map_resolution);
    let scatter_binary_header_bytes = if config.write_scatter_binary {
        scatter_binary_chunk_count(scatter_sets) * SCATTER_BINARY_HEADER_BYTES as u64
    } else {
        0
    };
    let scatter_binary_record_bytes = if config.write_scatter_binary {
        scatter_instance_count(scatter_sets) * SCATTER_BINARY_RECORD_STRIDE_BYTES as u64
    } else {
        0
    };
    let scatter_binary_bytes = scatter_binary_header_bytes + scatter_binary_record_bytes;

    MemoryFootprintManifest {
        map_pixel_count,
        decoded_map_bytes: decoded_map_bytes(map_pixel_count),
        encoded_map_bytes: 0,
        material_recipe_bytes: 0,
        engine_import_recipe_bytes: 0,
        preview_mesh_bytes: 0,
        prototype_mesh_bytes: 0,
        scatter_json_bytes: 0,
        scatter_binary_bytes,
        scatter_binary_header_bytes,
        scatter_binary_record_bytes,
        total_payload_bytes: scatter_binary_bytes,
    }
}

fn package_memory_footprint(
    package_directory: &Path,
    config: &NaturePackageConfig,
    preview_mesh_path: Option<&Path>,
    material_recipe_paths: &[PathBuf],
    engine_import_recipe_paths: &[PathBuf],
    prototype_meshes: &[PathBuf],
    scatter_path: Option<&Path>,
    scatter_binary_paths: &[PathBuf],
) -> Result<MemoryFootprintManifest, NaturePatchError> {
    let map_files = [
        package_directory.join("maps/height_u16.png"),
        package_directory.join("maps/normal_yplus.png"),
        package_directory.join("maps/normal_yminus.png"),
        package_directory.join("maps/masks_rgba.png"),
        package_directory.join("maps/grass_density.png"),
    ];
    let encoded_map_bytes = sum_file_lengths(&map_files)?;
    let material_recipe_bytes = sum_file_lengths(material_recipe_paths)?;
    let engine_import_recipe_bytes = sum_file_lengths(engine_import_recipe_paths)?;
    let preview_mesh_bytes = optional_file_length(preview_mesh_path)?;
    let prototype_mesh_bytes = sum_file_lengths(prototype_meshes)?;
    let scatter_json_bytes = optional_file_length(scatter_path)?;
    let scatter_binary_bytes = sum_file_lengths(scatter_binary_paths)?;
    let scatter_binary_header_bytes =
        scatter_binary_paths.len() as u64 * SCATTER_BINARY_HEADER_BYTES as u64;
    let scatter_binary_record_bytes = scatter_binary_bytes
        .checked_sub(scatter_binary_header_bytes)
        .ok_or_else(|| {
            NaturePatchError::Validation(
                "scatter binary bytes are smaller than their declared headers".to_string(),
            )
        })?;
    let map_pixel_count = map_pixel_count(config.map_resolution);
    let total_payload_bytes = encoded_map_bytes
        .checked_add(material_recipe_bytes)
        .and_then(|value| value.checked_add(engine_import_recipe_bytes))
        .and_then(|value| value.checked_add(preview_mesh_bytes))
        .and_then(|value| value.checked_add(prototype_mesh_bytes))
        .and_then(|value| value.checked_add(scatter_json_bytes))
        .and_then(|value| value.checked_add(scatter_binary_bytes))
        .ok_or_else(|| {
            NaturePatchError::Validation("package payload byte count overflowed u64".to_string())
        })?;

    Ok(MemoryFootprintManifest {
        map_pixel_count,
        decoded_map_bytes: decoded_map_bytes(map_pixel_count),
        encoded_map_bytes,
        material_recipe_bytes,
        engine_import_recipe_bytes,
        preview_mesh_bytes,
        prototype_mesh_bytes,
        scatter_json_bytes,
        scatter_binary_bytes,
        scatter_binary_header_bytes,
        scatter_binary_record_bytes,
        total_payload_bytes,
    })
}

fn map_pixel_count(map_resolution: u32) -> u64 {
    let resolution = map_resolution as u64;
    resolution * resolution
}

fn decoded_map_bytes(map_pixel_count: u64) -> u64 {
    // height L16 + two RGB8 normals + RGBA8 masks + L8 grass density = 13 bytes/pixel.
    map_pixel_count * 13
}

fn scatter_instance_count(scatter_sets: &[ScatterSet]) -> u64 {
    scatter_sets
        .iter()
        .flat_map(|set| &set.chunks)
        .map(|chunk| chunk.instances.len() as u64)
        .sum()
}

fn scatter_binary_chunk_count(scatter_sets: &[ScatterSet]) -> u64 {
    scatter_sets.iter().map(|set| set.chunks.len() as u64).sum()
}

fn sum_file_lengths(paths: &[impl AsRef<Path>]) -> Result<u64, NaturePatchError> {
    paths.iter().try_fold(0u64, |total, path| {
        let length = file_length(path.as_ref())?;
        total.checked_add(length).ok_or_else(|| {
            NaturePatchError::Validation("package file byte count overflowed u64".to_string())
        })
    })
}

fn optional_file_length(path: Option<&Path>) -> Result<u64, NaturePatchError> {
    match path {
        Some(path) => file_length(path),
        None => Ok(0),
    }
}

fn file_length(path: &Path) -> Result<u64, NaturePatchError> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(NaturePatchError::Validation(format!(
            "package footprint path `{}` must be a file",
            path.display()
        )));
    }
    let length = metadata.len();
    if length == 0 {
        return Err(NaturePatchError::Validation(format!(
            "package footprint path `{}` must not be empty",
            path.display()
        )));
    }
    Ok(length)
}

fn scatter_binary_format_manifest() -> ScatterBinaryFormatManifest {
    ScatterBinaryFormatManifest {
        format: "midori.scatter.bin.v1".to_string(),
        header_bytes: SCATTER_BINARY_HEADER_BYTES,
        record_stride_bytes: SCATTER_BINARY_RECORD_STRIDE_BYTES,
        endian: "little".to_string(),
        header: vec![
            "magic:u8[4]=MDSI".to_string(),
            "version:u32".to_string(),
            "record_stride_bytes:u32".to_string(),
            "instance_count:u32".to_string(),
        ],
        record: scatter_instance_fields(),
    }
}

fn scatter_instance_fields() -> Vec<String> {
    vec![
        "position.x:f32".to_string(),
        "position.y:f32".to_string(),
        "position.z:f32".to_string(),
        "yaw_radians:f32".to_string(),
        "height_multiplier:f32".to_string(),
        "width_multiplier:f32".to_string(),
        "phase_radians:f32".to_string(),
        "color_variation:f32".to_string(),
    ]
}

fn scatter_binary_file_manifests(scatter_sets: &[ScatterSet]) -> Vec<ScatterBinaryFileManifest> {
    scatter_sets
        .iter()
        .flat_map(|set| {
            set.chunks.iter().map(|chunk| ScatterBinaryFileManifest {
                layer_index: set.layer_index,
                layer_name: set.layer_name.clone(),
                kind: set.kind,
                chunk_x: chunk.chunk_x,
                chunk_z: chunk.chunk_z,
                file: scatter_binary_relative_path(set, chunk),
                instance_count: chunk.instances.len(),
                bounds_min: chunk.bounds_min,
                bounds_max: chunk.bounds_max,
            })
        })
        .collect()
}

fn scatter_binary_relative_path(set: &ScatterSet, chunk: &ScatterChunk) -> String {
    format!(
        "instances/binary/{}_{}_{}_{}.bin",
        sanitize_filename(&set.layer_name),
        groundcover_kind_filename(set.kind),
        chunk.chunk_x,
        chunk.chunk_z
    )
}

fn write_material_recipe_files(
    package_directory: &Path,
    manifest: &NatureExportManifest,
) -> Result<Vec<PathBuf>, NaturePatchError> {
    let mut paths = Vec::new();
    for recipe in &manifest.material_recipes {
        let recipe_file = material_recipe_file(manifest, recipe)?;
        let path = package_directory.join(recipe.file.as_str());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(&path)?;
        serde_json::to_writer_pretty(file, &recipe_file)?;
        paths.push(path);
    }
    Ok(paths)
}

fn write_engine_import_recipe_files(
    package_directory: &Path,
    manifest: &NatureExportManifest,
) -> Result<Vec<PathBuf>, NaturePatchError> {
    let mut paths = Vec::new();
    for recipe in &manifest.engine_import_recipes {
        let recipe_file = engine_import_recipe_file(manifest, recipe)?;
        let path = package_directory.join(recipe.file.as_str());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(&path)?;
        serde_json::to_writer_pretty(file, &recipe_file)?;
        paths.push(path);
    }
    Ok(paths)
}

fn groundcover_kind_filename(kind: GroundcoverKind) -> &'static str {
    match kind {
        GroundcoverKind::Grass => "grass",
        GroundcoverKind::Moss => "moss",
        GroundcoverKind::Flower => "flower",
        GroundcoverKind::Weed => "weed",
        GroundcoverKind::Litter => "litter",
        GroundcoverKind::Shrub => "shrub",
        GroundcoverKind::Rock => "rock",
        GroundcoverKind::Log => "log",
    }
}

fn write_scatter_binary_files(
    package_directory: &Path,
    scatter_sets: &[ScatterSet],
) -> Result<Vec<PathBuf>, NaturePatchError> {
    let mut paths = Vec::new();
    for set in scatter_sets {
        for chunk in &set.chunks {
            let relative = scatter_binary_relative_path(set, chunk);
            let path = package_directory.join(&relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut file = std::fs::File::create(&path)?;
            write_scatter_binary_chunk(&mut file, chunk)?;
            paths.push(path);
        }
    }
    Ok(paths)
}

fn write_scatter_binary_chunk(
    writer: &mut impl Write,
    chunk: &ScatterChunk,
) -> Result<(), NaturePatchError> {
    writer.write_all(SCATTER_BINARY_MAGIC)?;
    writer.write_all(&SCATTER_BINARY_VERSION.to_le_bytes())?;
    writer.write_all(&SCATTER_BINARY_RECORD_STRIDE_BYTES.to_le_bytes())?;
    writer.write_all(&(chunk.instances.len() as u32).to_le_bytes())?;
    for instance in &chunk.instances {
        for value in [
            instance.position[0],
            instance.position[1],
            instance.position[2],
            instance.yaw,
            instance.height,
            instance.width,
            instance.phase,
            instance.color_variation,
        ] {
            writer.write_all(&value.to_le_bytes())?;
        }
    }
    Ok(())
}

fn require_package_subfile(
    package_directory: &Path,
    subdirectory: &str,
    relative: &str,
    label: &str,
) -> Result<PathBuf, NaturePatchError> {
    let relative_path = Path::new(relative);
    validate_package_relative_path(relative_path, label)?;
    require_package_file(
        package_directory,
        Path::new(subdirectory).join(relative_path),
        label,
    )
}

fn optional_existing_package_file(
    package_directory: &Path,
    relative: &str,
) -> Result<Option<PathBuf>, NaturePatchError> {
    validate_package_relative_path(Path::new(relative), "optional package file")?;
    let path = package_directory.join(relative);
    if path.exists() {
        file_length(&path)?;
        Ok(Some(path))
    } else {
        Ok(None)
    }
}

fn require_package_file(
    package_directory: &Path,
    relative: impl AsRef<Path>,
    label: &str,
) -> Result<PathBuf, NaturePatchError> {
    let relative = relative.as_ref();
    validate_package_relative_path(relative, label)?;
    let path = package_directory.join(relative);
    let metadata = std::fs::metadata(&path).map_err(|err| {
        NaturePatchError::Validation(format!(
            "{label} `{}` could not be read: {err}",
            relative.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(NaturePatchError::Validation(format!(
            "{label} `{}` must resolve to a file",
            relative.display()
        )));
    }
    if metadata.len() == 0 {
        return Err(NaturePatchError::Validation(format!(
            "{label} `{}` must not be empty",
            relative.display()
        )));
    }
    Ok(path)
}

fn validate_package_relative_path(relative: &Path, label: &str) -> Result<(), NaturePatchError> {
    if relative.as_os_str().is_empty() {
        return Err(NaturePatchError::Validation(format!(
            "{label} must not be empty"
        )));
    }
    if relative.is_absolute() {
        return Err(NaturePatchError::Validation(format!(
            "{label} `{}` must be package-relative",
            relative.display()
        )));
    }
    for component in relative.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(NaturePatchError::Validation(format!(
                    "{label} `{}` must stay inside the package directory",
                    relative.display()
                )));
            }
        }
    }
    Ok(())
}

fn validate_png_map(
    path: &Path,
    expected_resolution: u32,
    manifest_file: &str,
    label: &str,
) -> Result<MapValidationSummary, NaturePatchError> {
    let image = image::open(path)?;
    if image.width() != expected_resolution || image.height() != expected_resolution {
        return Err(NaturePatchError::Validation(format!(
            "{label} `{}` must be {}x{}, got {}x{}",
            path.display(),
            expected_resolution,
            expected_resolution,
            image.width(),
            image.height()
        )));
    }

    if let Some(expected) = expected_map_color_type(manifest_file) {
        let actual = image.color();
        if actual != expected {
            return Err(NaturePatchError::Validation(format!(
                "{label} `{}` must decode as {}, got {}",
                path.display(),
                color_type_label(expected),
                color_type_label(actual)
            )));
        }
    }

    let (channels, channel_min, channel_max) = decoded_channel_ranges(&image)?;
    Ok(MapValidationSummary {
        file: path.to_path_buf(),
        width: image.width(),
        height: image.height(),
        color_type: color_type_label(image.color()).to_string(),
        channels,
        channel_min,
        channel_max,
        file_checksum: fnv1a_file_checksum(path)?,
    })
}

fn expected_map_color_type(manifest_file: &str) -> Option<ColorType> {
    let file_name = Path::new(manifest_file).file_name()?.to_str()?;
    match file_name {
        "height_u16.png" => Some(ColorType::L16),
        "normal_yplus.png" | "normal_yminus.png" => Some(ColorType::Rgb8),
        "masks_rgba.png" => Some(ColorType::Rgba8),
        "grass_density.png" => Some(ColorType::L8),
        _ => None,
    }
}

fn color_type_label(color_type: ColorType) -> &'static str {
    match color_type {
        ColorType::L8 => "L8",
        ColorType::La8 => "LA8",
        ColorType::Rgb8 => "RGB8",
        ColorType::Rgba8 => "RGBA8",
        ColorType::L16 => "L16",
        ColorType::La16 => "LA16",
        ColorType::Rgb16 => "RGB16",
        ColorType::Rgba16 => "RGBA16",
        ColorType::Rgb32F => "RGB32F",
        ColorType::Rgba32F => "RGBA32F",
        _ => "unsupported",
    }
}

fn decoded_channel_ranges(
    image: &image::DynamicImage,
) -> Result<(usize, Vec<u32>, Vec<u32>), NaturePatchError> {
    match image.color() {
        ColorType::L8 => {
            let buffer = image.to_luma8();
            let mut min = vec![u32::MAX; 1];
            let mut max = vec![0; 1];
            for pixel in buffer.pixels() {
                update_channel_range(&mut min, &mut max, &[pixel.0[0] as u32]);
            }
            Ok((1, min, max))
        }
        ColorType::L16 => {
            let buffer = image.to_luma16();
            let mut min = vec![u32::MAX; 1];
            let mut max = vec![0; 1];
            for pixel in buffer.pixels() {
                update_channel_range(&mut min, &mut max, &[pixel.0[0] as u32]);
            }
            Ok((1, min, max))
        }
        ColorType::Rgb8 => {
            let buffer = image.to_rgb8();
            let mut min = vec![u32::MAX; 3];
            let mut max = vec![0; 3];
            for pixel in buffer.pixels() {
                update_channel_range(
                    &mut min,
                    &mut max,
                    &[pixel.0[0] as u32, pixel.0[1] as u32, pixel.0[2] as u32],
                );
            }
            Ok((3, min, max))
        }
        ColorType::Rgba8 => {
            let buffer = image.to_rgba8();
            let mut min = vec![u32::MAX; 4];
            let mut max = vec![0; 4];
            for pixel in buffer.pixels() {
                update_channel_range(
                    &mut min,
                    &mut max,
                    &[
                        pixel.0[0] as u32,
                        pixel.0[1] as u32,
                        pixel.0[2] as u32,
                        pixel.0[3] as u32,
                    ],
                );
            }
            Ok((4, min, max))
        }
        other => Err(NaturePatchError::Validation(format!(
            "unsupported PNG color type {} for Midori map validation",
            color_type_label(other)
        ))),
    }
}

fn update_channel_range(min: &mut [u32], max: &mut [u32], values: &[u32]) {
    for (index, value) in values.iter().enumerate() {
        min[index] = min[index].min(*value);
        max[index] = max[index].max(*value);
    }
}

fn fnv1a_file_checksum(path: &Path) -> Result<u64, NaturePatchError> {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in std::fs::read(path)? {
        hash = fnv1a(hash, byte);
    }
    Ok(hash)
}

fn validate_map_relationships(
    directory: &Path,
    manifest: &NatureExportManifest,
) -> Result<MapRelationshipSummary, NaturePatchError> {
    let normal_yplus_path = require_package_file(
        directory,
        &manifest.normal_conventions.unity_yplus_file,
        "manifest.normal_conventions.unity_yplus_file",
    )?;
    let normal_yminus_path = require_package_file(
        directory,
        &manifest.normal_conventions.unreal_yminus_file,
        "manifest.normal_conventions.unreal_yminus_file",
    )?;
    let masks_path = require_package_file(
        directory,
        &manifest.terrain.masks_file,
        "manifest.terrain.masks_file",
    )?;
    let grass_density_path = require_package_file(
        directory,
        &manifest.terrain.grass_density_file,
        "manifest.terrain.grass_density_file",
    )?;

    let normal_yplus = image::open(&normal_yplus_path)?.to_rgb8();
    let normal_yminus = image::open(&normal_yminus_path)?.to_rgb8();
    if normal_yplus.dimensions() != normal_yminus.dimensions() {
        return Err(NaturePatchError::Validation(format!(
            "normal convention maps must have matching dimensions, got {}x{} and {}x{}",
            normal_yplus.width(),
            normal_yplus.height(),
            normal_yminus.width(),
            normal_yminus.height()
        )));
    }

    let mut normal_red_blue_mismatches = 0usize;
    let mut normal_green_flip_mismatches = 0usize;
    let mut normal_green_flip_max_error = 0u8;
    for (plus, minus) in normal_yplus.pixels().zip(normal_yminus.pixels()) {
        if plus.0[0] != minus.0[0] || plus.0[2] != minus.0[2] {
            normal_red_blue_mismatches += 1;
        }
        let green_error = ((plus.0[1] as i16 + minus.0[1] as i16) - u8::MAX as i16)
            .unsigned_abs()
            .min(u8::MAX as u16) as u8;
        normal_green_flip_max_error = normal_green_flip_max_error.max(green_error);
        if green_error > 1 {
            normal_green_flip_mismatches += 1;
        }
    }
    if normal_red_blue_mismatches > 0 || normal_green_flip_mismatches > 0 {
        return Err(NaturePatchError::Validation(format!(
            "normal convention maps must be paired Y+/Y- encodings; red/blue mismatches={normal_red_blue_mismatches}, green flip mismatches={normal_green_flip_mismatches}, max green error={normal_green_flip_max_error}"
        )));
    }

    let masks = image::open(&masks_path)?.to_rgba8();
    let grass_density = image::open(&grass_density_path)?.to_luma8();
    if masks.dimensions() != grass_density.dimensions() {
        return Err(NaturePatchError::Validation(format!(
            "grass density map must match mask dimensions, got {}x{} and {}x{}",
            grass_density.width(),
            grass_density.height(),
            masks.width(),
            masks.height()
        )));
    }

    let mut grass_density_mask_r_mismatches = 0usize;
    for (mask, density) in masks.pixels().zip(grass_density.pixels()) {
        if mask.0[0] != density.0[0] {
            grass_density_mask_r_mismatches += 1;
        }
    }
    if grass_density_mask_r_mismatches > 0 {
        return Err(NaturePatchError::Validation(format!(
            "grass density map must match masks_rgba R channel; mismatches={grass_density_mask_r_mismatches}"
        )));
    }

    Ok(MapRelationshipSummary {
        normal_pair_pixels: normal_yplus.width() as usize * normal_yplus.height() as usize,
        normal_red_blue_mismatches,
        normal_green_flip_mismatches,
        normal_green_flip_max_error,
        grass_density_pixels: grass_density.width() as usize * grass_density.height() as usize,
        grass_density_mask_r_mismatches,
    })
}

fn validate_material_recipe_file(
    path: &Path,
    recipe_manifest: &MaterialRecipeManifest,
    manifest: &NatureExportManifest,
    package_directory: &Path,
) -> Result<MaterialRecipeValidationSummary, NaturePatchError> {
    let recipe: MaterialRecipeFile = serde_json::from_reader(std::fs::File::open(path)?)?;
    if recipe.schema != "midori.material_recipe.v1" {
        return Err(NaturePatchError::Validation(format!(
            "material recipe `{}` has unsupported schema `{}`",
            path.display(),
            recipe.schema
        )));
    }
    if recipe.material_slot != recipe_manifest.material_slot {
        return Err(NaturePatchError::Validation(format!(
            "material recipe `{}` material_slot `{}` does not match manifest `{}`",
            path.display(),
            recipe.material_slot,
            recipe_manifest.material_slot
        )));
    }
    if recipe.runtime_policy != recipe_manifest.runtime_policy
        || recipe.runtime_policy != "engine_native_static"
    {
        return Err(NaturePatchError::Validation(format!(
            "material recipe `{}` must use engine_native_static runtime policy",
            path.display()
        )));
    }
    if recipe.shader_policy != "preview_only" {
        return Err(NaturePatchError::Validation(format!(
            "material recipe `{}` shader_policy must be preview_only",
            path.display()
        )));
    }
    if recipe.texture_pipeline != "parked" {
        return Err(NaturePatchError::Validation(format!(
            "material recipe `{}` texture_pipeline must be parked",
            path.display()
        )));
    }
    if recipe.engine_targets != recipe_manifest.engine_targets {
        return Err(NaturePatchError::Validation(format!(
            "material recipe `{}` engine targets do not match manifest",
            path.display()
        )));
    }
    let parameter_set = manifest
        .material_parameters
        .iter()
        .find(|set| set.material_slot == recipe.material_slot)
        .ok_or_else(|| {
            NaturePatchError::Validation(format!(
                "material recipe `{}` has no manifest material parameter set",
                path.display()
            ))
        })?;
    if recipe.parameter_set != parameter_set.parameter_set {
        return Err(NaturePatchError::Validation(format!(
            "material recipe `{}` parameter_set `{}` does not match manifest `{}`",
            path.display(),
            recipe.parameter_set,
            parameter_set.parameter_set
        )));
    }
    let manifest_parameter_names = parameter_set
        .parameters
        .iter()
        .map(|parameter| parameter.name.as_str())
        .collect::<BTreeSet<_>>();
    let recipe_parameter_names = recipe
        .parameters
        .iter()
        .map(|parameter| parameter.name.as_str())
        .collect::<BTreeSet<_>>();
    if recipe_parameter_names != manifest_parameter_names {
        return Err(NaturePatchError::Validation(format!(
            "material recipe `{}` parameter names do not match manifest",
            path.display()
        )));
    }
    for texture in &recipe.required_textures {
        require_package_file(
            package_directory,
            texture.as_str(),
            "material_recipe.required_textures[]",
        )?;
    }
    match recipe.material_slot.as_str() {
        "terrain_surface" => {
            if !recipe
                .required_textures
                .iter()
                .any(|texture| texture == "maps/masks_rgba.png")
            {
                return Err(NaturePatchError::Validation(format!(
                    "material recipe `{}` must require maps/masks_rgba.png",
                    path.display()
                )));
            }
            for overlay in ["moss", "wetness", "cracks"] {
                if !recipe
                    .surface_overlays
                    .iter()
                    .any(|actual| actual == overlay)
                {
                    return Err(NaturePatchError::Validation(format!(
                        "material recipe `{}` must include surface overlay `{overlay}`",
                        path.display()
                    )));
                }
            }
        }
        "groundcover_foliage" => {
            for stream in ["TEXCOORD_1.x", "COLOR_0.y"] {
                if !recipe
                    .required_vertex_streams
                    .iter()
                    .any(|actual| actual.starts_with(stream))
                {
                    return Err(NaturePatchError::Validation(format!(
                        "material recipe `{}` must require vertex stream `{stream}`",
                        path.display()
                    )));
                }
            }
        }
        _ => {}
    }

    Ok(MaterialRecipeValidationSummary {
        file: path.to_path_buf(),
        material_slot: recipe.material_slot,
        parameter_set: recipe.parameter_set,
        runtime_policy: recipe.runtime_policy,
        shader_policy: recipe.shader_policy,
        texture_pipeline: recipe.texture_pipeline,
        parameter_count: recipe.parameters.len(),
        engine_targets: recipe.engine_targets,
        required_textures: recipe.required_textures,
        required_vertex_streams: recipe.required_vertex_streams,
        surface_overlays: recipe.surface_overlays,
        file_checksum: fnv1a_file_checksum(path)?,
    })
}

fn validate_engine_import_recipe_file(
    path: &Path,
    recipe_manifest: &EngineImportRecipeManifest,
    manifest: &NatureExportManifest,
    package_directory: &Path,
) -> Result<EngineImportRecipeValidationSummary, NaturePatchError> {
    let recipe: EngineImportRecipeFile = serde_json::from_reader(std::fs::File::open(path)?)?;
    if recipe.schema != "midori.engine_import_recipe.v1" {
        return Err(NaturePatchError::Validation(format!(
            "engine import recipe `{}` has unsupported schema `{}`",
            path.display(),
            recipe.schema
        )));
    }
    if recipe.engine != recipe_manifest.engine {
        return Err(NaturePatchError::Validation(format!(
            "engine import recipe `{}` engine `{}` does not match manifest `{}`",
            path.display(),
            recipe.engine,
            recipe_manifest.engine
        )));
    }
    if recipe.profile != recipe_manifest.profile {
        return Err(NaturePatchError::Validation(format!(
            "engine import recipe `{}` profile `{}` does not match manifest `{}`",
            path.display(),
            recipe.profile,
            recipe_manifest.profile
        )));
    }
    if recipe.runtime_policy != recipe_manifest.runtime_policy
        || recipe.runtime_policy != "engine_native_static"
    {
        return Err(NaturePatchError::Validation(format!(
            "engine import recipe `{}` must use engine_native_static runtime policy",
            path.display()
        )));
    }
    if recipe.expected_systems != recipe_manifest.expected_systems {
        return Err(NaturePatchError::Validation(format!(
            "engine import recipe `{}` expected systems do not match manifest",
            path.display()
        )));
    }
    let expected_sources = expected_engine_import_source_files(manifest, &recipe.engine);
    if recipe.source_files != expected_sources {
        return Err(NaturePatchError::Validation(format!(
            "engine import recipe `{}` source files do not match manifest",
            path.display()
        )));
    }
    for source in &recipe.source_files {
        require_package_file(
            package_directory,
            source.as_str(),
            "engine_import_recipe.source_files[]",
        )?;
    }
    let expected_material_recipe_files = material_recipe_files(manifest);
    if recipe.material_recipe_files != expected_material_recipe_files {
        return Err(NaturePatchError::Validation(format!(
            "engine import recipe `{}` material recipes do not match manifest",
            path.display()
        )));
    }

    let expected_profile = match recipe.engine.as_str() {
        "unity" => profile_import_recipe("mobile", &manifest.mobile),
        "unreal" => profile_import_recipe("console", &manifest.console),
        _ => {
            return Err(NaturePatchError::Validation(format!(
                "engine import recipe `{}` has unsupported engine `{}`",
                path.display(),
                recipe.engine
            )));
        }
    };
    validate_engine_profile_import_recipe(path, &recipe.profile_settings, &expected_profile)?;

    let expected_normal = if recipe.engine == "unreal" {
        manifest.normal_conventions.unreal_yminus_file.as_str()
    } else {
        manifest.normal_conventions.unity_yplus_file.as_str()
    };
    if recipe.terrain.heightmap_file != manifest.terrain.heightmap_file
        || recipe.terrain.mask_file != manifest.terrain.masks_file
        || recipe.terrain.normal_map_file != expected_normal
    {
        return Err(NaturePatchError::Validation(format!(
            "engine import recipe `{}` terrain files do not match manifest",
            path.display()
        )));
    }
    if (recipe.terrain.tile_size_meters - manifest.tile_size).abs() > PACKAGE_VALIDATION_EPSILON
        || (recipe.terrain.height_min - manifest.terrain.height_min).abs()
            > PACKAGE_VALIDATION_EPSILON
        || (recipe.terrain.height_max - manifest.terrain.height_max).abs()
            > PACKAGE_VALIDATION_EPSILON
    {
        return Err(NaturePatchError::Validation(format!(
            "engine import recipe `{}` terrain scale/height range does not match manifest",
            path.display()
        )));
    }

    let groundcover_recipe_file =
        material_recipe_file_for_slot(manifest, "groundcover_foliage").unwrap_or_default();
    if recipe.groundcover.material_slot != "groundcover_foliage"
        || recipe.groundcover.material_recipe_file != groundcover_recipe_file
        || recipe.groundcover.prototype_family_count != manifest.prototypes.len()
        || recipe.groundcover.lod0_prototype_count != lod0_prototype_count(manifest)
    {
        return Err(NaturePatchError::Validation(format!(
            "engine import recipe `{}` groundcover settings do not match manifest",
            path.display()
        )));
    }
    if recipe.scatter.binary_chunk_count != manifest.scatter.binary_files.len()
        || recipe.scatter.instance_count != manifest_scatter_instance_count(manifest)
        || (recipe.scatter.chunk_size_meters - manifest.scatter.chunk_size).abs()
            > PACKAGE_VALIDATION_EPSILON
    {
        return Err(NaturePatchError::Validation(format!(
            "engine import recipe `{}` scatter settings do not match manifest",
            path.display()
        )));
    }

    Ok(EngineImportRecipeValidationSummary {
        file: path.to_path_buf(),
        engine: recipe.engine,
        profile: recipe.profile,
        runtime_policy: recipe.runtime_policy,
        source_file_count: recipe.source_files.len(),
        material_recipe_files: recipe.material_recipe_files,
        expected_systems: recipe.expected_systems,
        scatter_instances: recipe.scatter.instance_count,
        scatter_binary_chunks: recipe.scatter.binary_chunk_count,
        file_checksum: fnv1a_file_checksum(path)?,
    })
}

fn validate_engine_profile_import_recipe(
    path: &Path,
    actual: &EngineProfileImportRecipe,
    expected: &EngineProfileImportRecipe,
) -> Result<(), NaturePatchError> {
    let matches = actual.name == expected.name
        && (actual.density_scale - expected.density_scale).abs() <= PACKAGE_VALIDATION_EPSILON
        && (actual.lod0_max_distance_meters - expected.lod0_max_distance_meters).abs()
            <= PACKAGE_VALIDATION_EPSILON
        && (actual.lod1_max_distance_meters - expected.lod1_max_distance_meters).abs()
            <= PACKAGE_VALIDATION_EPSILON
        && (actual.lod2_max_distance_meters - expected.lod2_max_distance_meters).abs()
            <= PACKAGE_VALIDATION_EPSILON
        && (actual.cull_start_meters - expected.cull_start_meters).abs()
            <= PACKAGE_VALIDATION_EPSILON
        && (actual.cull_end_meters - expected.cull_end_meters).abs() <= PACKAGE_VALIDATION_EPSILON
        && actual.shadows == expected.shadows
        && actual.material_slots == expected.material_slots
        && actual.max_instances_per_tile == expected.max_instances_per_tile
        && actual.max_instances_per_chunk == expected.max_instances_per_chunk
        && actual.grass_collision == expected.grass_collision
        && actual.moss_collision == expected.moss_collision;
    if !matches {
        return Err(NaturePatchError::Validation(format!(
            "engine import recipe `{}` profile settings do not match manifest",
            path.display()
        )));
    }
    Ok(())
}

fn validate_prototype_glb(
    path: &Path,
    lod: &GroundcoverPrototypeLodManifest,
    material_slot_limit: usize,
) -> Result<PrototypeValidationSummary, NaturePatchError> {
    let (document, buffers, _) = gltf::import(path).map_err(|err| {
        NaturePatchError::Validation(format!(
            "prototype GLB `{}` failed to parse: {err}",
            path.display()
        ))
    })?;
    let buffer_data = buffers
        .iter()
        .map(|buffer| buffer.0.as_slice())
        .collect::<Vec<_>>();

    let mesh_count = document.meshes().count();
    let mut primitive_count = 0usize;
    let mut vertex_count: Option<usize> = None;
    let mut triangle_count = 0usize;
    let mut used_materials = BTreeSet::new();
    let mut has_positions = true;
    let mut has_normals = true;
    let mut has_tangents = true;
    let mut normals_are_valid = true;
    let mut tangents_are_valid = true;
    let mut has_texcoord0 = true;
    let mut has_texcoord1 = true;
    let mut has_color0 = true;
    let mut bounds_min = [f32::MAX; 3];
    let mut bounds_max = [f32::MIN; 3];

    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            primitive_count += 1;
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                return Err(NaturePatchError::Validation(format!(
                    "prototype GLB `{}` primitive must use TRIANGLES mode",
                    path.display()
                )));
            }

            let material_index = primitive.material().index().ok_or_else(|| {
                NaturePatchError::Validation(format!(
                    "prototype GLB `{}` primitive must reference a material",
                    path.display()
                ))
            })?;
            used_materials.insert(material_index);

            let reader = primitive.reader(|buffer| Some(buffer_data[buffer.index()]));
            let positions = reader.read_positions().ok_or_else(|| {
                NaturePatchError::Validation(format!(
                    "prototype GLB `{}` primitive is missing POSITION",
                    path.display()
                ))
            })?;
            let positions = positions.collect::<Vec<_>>();
            if positions.is_empty() {
                return Err(NaturePatchError::Validation(format!(
                    "prototype GLB `{}` primitive has no POSITION vertices",
                    path.display()
                )));
            }

            if let Some(existing) = vertex_count {
                if existing != positions.len() {
                    return Err(NaturePatchError::Validation(format!(
                        "prototype GLB `{}` primitives disagree on vertex count: {existing} vs {}",
                        path.display(),
                        positions.len()
                    )));
                }
            } else {
                vertex_count = Some(positions.len());
            }

            for position in &positions {
                for axis in 0..3 {
                    if !position[axis].is_finite() {
                        return Err(NaturePatchError::Validation(format!(
                            "prototype GLB `{}` contains a non-finite POSITION",
                            path.display()
                        )));
                    }
                    bounds_min[axis] = bounds_min[axis].min(position[axis]);
                    bounds_max[axis] = bounds_max[axis].max(position[axis]);
                }
            }

            let position_count = positions.len();
            let normals = reader
                .read_normals()
                .map(|values| values.collect::<Vec<_>>());
            has_normals = has_normals
                && normals
                    .as_ref()
                    .map(|values| values.len() == position_count)
                    .unwrap_or(false);
            normals_are_valid = normals_are_valid
                && normals
                    .as_ref()
                    .map(|values| normal_samples_are_valid(values))
                    .unwrap_or(false);
            let tangents = reader
                .read_tangents()
                .map(|values| values.collect::<Vec<_>>());
            has_tangents = has_tangents
                && tangents
                    .as_ref()
                    .map(|values| values.len() == position_count)
                    .unwrap_or(false);
            tangents_are_valid = tangents_are_valid
                && match (normals.as_ref(), tangents.as_ref()) {
                    (Some(normals), Some(tangents)) => tangent_samples_are_valid(normals, tangents),
                    _ => false,
                };
            let texcoord0_count = reader
                .read_tex_coords(0)
                .map(|values| values.into_f32().count());
            has_texcoord0 = has_texcoord0 && texcoord0_count == Some(position_count);
            let texcoord1_count = reader
                .read_tex_coords(1)
                .map(|values| values.into_f32().count());
            has_texcoord1 = has_texcoord1 && texcoord1_count == Some(position_count);
            let color0_count = reader
                .read_colors(0)
                .map(|values| values.into_rgba_f32().count());
            has_color0 = has_color0 && color0_count == Some(position_count);

            let index_count = reader
                .read_indices()
                .ok_or_else(|| {
                    NaturePatchError::Validation(format!(
                        "prototype GLB `{}` primitive is missing indices",
                        path.display()
                    ))
                })?
                .into_u32()
                .count();
            if index_count == 0 || index_count % 3 != 0 {
                return Err(NaturePatchError::Validation(format!(
                    "prototype GLB `{}` primitive index count must be a nonzero multiple of 3, got {index_count}",
                    path.display()
                )));
            }
            triangle_count = triangle_count.checked_add(index_count / 3).ok_or_else(|| {
                NaturePatchError::Validation(format!(
                    "prototype GLB `{}` triangle count overflowed usize",
                    path.display()
                ))
            })?;
        }
    }

    has_positions = primitive_count > 0 && vertex_count.is_some() && has_positions;
    if !has_positions
        || !has_normals
        || !has_tangents
        || !normals_are_valid
        || !tangents_are_valid
        || !has_texcoord0
        || !has_texcoord1
        || !has_color0
    {
        return Err(NaturePatchError::Validation(format!(
            "prototype GLB `{}` must contain valid POSITION, NORMAL, TANGENT, TEXCOORD_0, TEXCOORD_1, and COLOR_0 on every primitive",
            path.display()
        )));
    }

    let vertex_count = vertex_count.unwrap_or(0);
    if vertex_count != lod.vertex_count {
        return Err(NaturePatchError::Validation(format!(
            "prototype GLB `{}` has {vertex_count} vertices but manifest declares {}",
            path.display(),
            lod.vertex_count
        )));
    }
    if triangle_count != lod.triangle_count {
        return Err(NaturePatchError::Validation(format!(
            "prototype GLB `{}` has {triangle_count} triangles but manifest declares {}",
            path.display(),
            lod.triangle_count
        )));
    }
    validate_bounds_close(path, "bounds_min", &bounds_min, &lod.bounds_min)?;
    validate_bounds_close(path, "bounds_max", &bounds_max, &lod.bounds_max)?;

    if used_materials.is_empty() {
        return Err(NaturePatchError::Validation(format!(
            "prototype GLB `{}` must reference at least one material",
            path.display()
        )));
    }
    if used_materials.len() > material_slot_limit {
        return Err(NaturePatchError::Validation(format!(
            "prototype GLB `{}` uses {} materials, above profile material slot limit {}",
            path.display(),
            used_materials.len(),
            material_slot_limit
        )));
    }

    Ok(PrototypeValidationSummary {
        file: path.to_path_buf(),
        lod_index: lod.index,
        mesh_count,
        primitive_count,
        vertex_count,
        triangle_count,
        used_material_count: used_materials.len(),
        used_material_indices: used_materials.into_iter().collect(),
        has_positions,
        has_normals,
        has_tangents,
        normals_are_valid,
        tangents_are_valid,
        has_texcoord0,
        has_texcoord1,
        has_color0,
        bounds_min,
        bounds_max,
        file_checksum: fnv1a_file_checksum(path)?,
    })
}

fn normal_samples_are_valid(normals: &[[f32; 3]]) -> bool {
    normals.iter().all(|normal| {
        let normal = Vec3::from_array(*normal);
        normal.is_finite() && (normal.length() - 1.0).abs() <= 0.01
    })
}

fn tangent_samples_are_valid(normals: &[[f32; 3]], tangents: &[[f32; 4]]) -> bool {
    normals.len() == tangents.len()
        && normals.iter().zip(tangents).all(|(normal, tangent)| {
            let normal = Vec3::from_array(*normal);
            let tangent_xyz = Vec3::new(tangent[0], tangent[1], tangent[2]);
            let handedness = tangent[3];
            normal.is_finite()
                && tangent_xyz.is_finite()
                && handedness.is_finite()
                && (normal.length() - 1.0).abs() <= 0.01
                && (tangent_xyz.length() - 1.0).abs() <= 0.01
                && normal.dot(tangent_xyz).abs() <= 0.05
                && (handedness.abs() - 1.0).abs() <= 0.001
        })
}

fn validate_profile_budgets(
    manifest: &NatureExportManifest,
    prototype_summaries: &[PrototypeValidationSummary],
    scatter_instance_count: usize,
    scatter_chunks: &[ScatterChunkValidationSummary],
) -> Result<Vec<ProfileBudgetValidationSummary>, NaturePatchError> {
    let summaries = vec![
        profile_budget_summary(
            "mobile",
            &manifest.mobile,
            prototype_summaries,
            scatter_instance_count,
            scatter_chunks,
        ),
        profile_budget_summary(
            "console",
            &manifest.console,
            prototype_summaries,
            scatter_instance_count,
            scatter_chunks,
        ),
    ];

    if let Some(failed) = summaries.iter().find(|summary| !summary.passed) {
        return Err(NaturePatchError::Validation(format!(
            "{} profile budget exceeded: {} triangle violations, {} material-slot violations, {} instance violations",
            failed.profile,
            failed.triangle_budget_violation_count,
            failed.material_slot_violation_count,
            failed.instance_budget_violation_count
        )));
    }

    Ok(summaries)
}

fn profile_budget_summary(
    profile_name: &str,
    profile: &NatureProfile,
    prototype_summaries: &[PrototypeValidationSummary],
    scatter_instance_count: usize,
    scatter_chunks: &[ScatterChunkValidationSummary],
) -> ProfileBudgetValidationSummary {
    let mut max_lod0_triangles = 0usize;
    let mut max_lod1_triangles = 0usize;
    let mut max_lod2_triangles = 0usize;
    let mut max_used_material_count = 0usize;
    let mut triangle_budget_violation_count = 0usize;
    let mut material_slot_violation_count = 0usize;
    let mut instance_budget_violation_count = 0usize;
    let max_chunk_instance_count = scatter_chunks
        .iter()
        .map(|chunk| chunk.instance_count)
        .max()
        .unwrap_or(0);

    for summary in prototype_summaries {
        let budget = lod_triangle_budget(profile, summary.lod_index);
        if summary.triangle_count > budget as usize {
            triangle_budget_violation_count += 1;
        }
        match summary.lod_index {
            0 => max_lod0_triangles = max_lod0_triangles.max(summary.triangle_count),
            1 => max_lod1_triangles = max_lod1_triangles.max(summary.triangle_count),
            _ => max_lod2_triangles = max_lod2_triangles.max(summary.triangle_count),
        }

        max_used_material_count = max_used_material_count.max(summary.used_material_count);
        if summary.used_material_count > profile.material_slots as usize {
            material_slot_violation_count += 1;
        }
    }

    if scatter_instance_count > profile.max_instances_per_tile as usize {
        instance_budget_violation_count += 1;
    }
    if max_chunk_instance_count > profile.max_instances_per_chunk as usize {
        instance_budget_violation_count += 1;
    }

    ProfileBudgetValidationSummary {
        profile: profile_name.to_string(),
        prototype_lod_count: prototype_summaries.len(),
        max_lod0_triangles,
        max_lod1_triangles,
        max_lod2_triangles,
        lod0_triangle_budget: profile.lod0_max_triangles,
        lod1_triangle_budget: profile.lod1_max_triangles,
        lod2_triangle_budget: profile.lod2_max_triangles,
        max_used_material_count,
        material_slot_budget: profile.material_slots,
        scatter_instance_count,
        max_chunk_instance_count,
        max_instances_per_tile_budget: profile.max_instances_per_tile,
        max_instances_per_chunk_budget: profile.max_instances_per_chunk,
        triangle_budget_violation_count,
        material_slot_violation_count,
        instance_budget_violation_count,
        passed: triangle_budget_violation_count == 0
            && material_slot_violation_count == 0
            && instance_budget_violation_count == 0,
    }
}

fn lod_triangle_budget(profile: &NatureProfile, lod_index: u32) -> u32 {
    match lod_index {
        0 => profile.lod0_max_triangles,
        1 => profile.lod1_max_triangles,
        _ => profile.lod2_max_triangles,
    }
}

fn validate_bounds_close(
    path: &Path,
    field: &str,
    actual: &[f32; 3],
    expected: &[f32; 3],
) -> Result<(), NaturePatchError> {
    for axis in 0..3 {
        if (actual[axis] - expected[axis]).abs() > PACKAGE_VALIDATION_EPSILON {
            return Err(NaturePatchError::Validation(format!(
                "prototype GLB `{}` {field}[{axis}] is {}, expected {}",
                path.display(),
                actual[axis],
                expected[axis]
            )));
        }
    }
    Ok(())
}

fn validate_scatter_json_file(
    path: &Path,
) -> Result<(usize, Vec<ScatterChunkValidationSummary>), NaturePatchError> {
    let scatter_sets: Vec<ScatterSet> = serde_json::from_reader(std::fs::File::open(path)?)?;
    let mut instance_count = 0usize;
    let mut summaries = Vec::new();
    let file_checksum = fnv1a_file_checksum(path)?;
    for set in &scatter_sets {
        if set.layer_name.trim().is_empty() {
            return Err(NaturePatchError::Validation(format!(
                "scatter JSON `{}` contains a layer with an empty name",
                path.display()
            )));
        }
        for chunk in &set.chunks {
            validate_bounds(
                &chunk.bounds_min,
                &chunk.bounds_max,
                "scatter JSON chunk bounds",
            )?;
            if chunk.instances.is_empty() {
                return Err(NaturePatchError::Validation(format!(
                    "scatter JSON `{}` contains an empty chunk {},{}",
                    path.display(),
                    chunk.chunk_x,
                    chunk.chunk_z
                )));
            }
            for instance in &chunk.instances {
                validate_scatter_instance(
                    instance,
                    &chunk.bounds_min,
                    &chunk.bounds_max,
                    "scatter JSON instance",
                )?;
            }
            instance_count = instance_count
                .checked_add(chunk.instances.len())
                .ok_or_else(|| {
                    NaturePatchError::Validation(
                        "scatter JSON instance count overflowed usize".to_string(),
                    )
                })?;
            summaries.push(summarize_scatter_chunk(
                "json",
                path,
                file_checksum,
                set.layer_index,
                &set.layer_name,
                set.kind,
                chunk.chunk_x,
                chunk.chunk_z,
                &chunk.bounds_min,
                &chunk.bounds_max,
                &chunk.instances,
            )?);
        }
    }
    Ok((instance_count, summaries))
}

fn validate_scatter_binary_file(
    path: &Path,
    manifest: &ScatterBinaryFileManifest,
) -> Result<ScatterChunkValidationSummary, NaturePatchError> {
    validate_bounds(
        &manifest.bounds_min,
        &manifest.bounds_max,
        "manifest.scatter.binary_files[].bounds",
    )?;
    if manifest.instance_count > u32::MAX as usize {
        return Err(NaturePatchError::Validation(format!(
            "scatter binary `{}` has too many instances for midori.scatter.bin.v1",
            manifest.file
        )));
    }

    let bytes = std::fs::read(path)?;
    if bytes.len() < SCATTER_BINARY_HEADER_BYTES as usize {
        return Err(NaturePatchError::Validation(format!(
            "scatter binary `{}` is shorter than the v1 header",
            manifest.file
        )));
    }
    if &bytes[0..4] != SCATTER_BINARY_MAGIC {
        return Err(NaturePatchError::Validation(format!(
            "scatter binary `{}` has invalid magic",
            manifest.file
        )));
    }
    let version = read_u32_le(&bytes, 4);
    if version != SCATTER_BINARY_VERSION {
        return Err(NaturePatchError::Validation(format!(
            "scatter binary `{}` version must be {}, got {}",
            manifest.file, SCATTER_BINARY_VERSION, version
        )));
    }
    let stride = read_u32_le(&bytes, 8);
    if stride != SCATTER_BINARY_RECORD_STRIDE_BYTES {
        return Err(NaturePatchError::Validation(format!(
            "scatter binary `{}` stride must be {}, got {}",
            manifest.file, SCATTER_BINARY_RECORD_STRIDE_BYTES, stride
        )));
    }
    let instance_count = read_u32_le(&bytes, 12) as usize;
    if instance_count != manifest.instance_count {
        return Err(NaturePatchError::Validation(format!(
            "scatter binary `{}` header count {} does not match manifest count {}",
            manifest.file, instance_count, manifest.instance_count
        )));
    }

    let expected_len = (SCATTER_BINARY_HEADER_BYTES as usize)
        .checked_add(
            instance_count
                .checked_mul(SCATTER_BINARY_RECORD_STRIDE_BYTES as usize)
                .ok_or_else(|| {
                    NaturePatchError::Validation(format!(
                        "scatter binary `{}` byte length overflowed usize",
                        manifest.file
                    ))
                })?,
        )
        .ok_or_else(|| {
            NaturePatchError::Validation(format!(
                "scatter binary `{}` byte length overflowed usize",
                manifest.file
            ))
        })?;
    if bytes.len() != expected_len {
        return Err(NaturePatchError::Validation(format!(
            "scatter binary `{}` length must be {}, got {}",
            manifest.file,
            expected_len,
            bytes.len()
        )));
    }

    let mut instances = Vec::with_capacity(instance_count);
    for index in 0..instance_count {
        let offset = SCATTER_BINARY_HEADER_BYTES as usize
            + index * SCATTER_BINARY_RECORD_STRIDE_BYTES as usize;
        let instance = ScatterInstance {
            position: [
                read_f32_le(&bytes, offset),
                read_f32_le(&bytes, offset + 4),
                read_f32_le(&bytes, offset + 8),
            ],
            yaw: read_f32_le(&bytes, offset + 12),
            height: read_f32_le(&bytes, offset + 16),
            width: read_f32_le(&bytes, offset + 20),
            phase: read_f32_le(&bytes, offset + 24),
            color_variation: read_f32_le(&bytes, offset + 28),
        };
        validate_scatter_instance(
            &instance,
            &manifest.bounds_min,
            &manifest.bounds_max,
            "scatter binary instance",
        )?;
        instances.push(instance);
    }

    summarize_scatter_chunk(
        "binary",
        path,
        fnv1a_file_checksum(path)?,
        manifest.layer_index,
        &manifest.layer_name,
        manifest.kind,
        manifest.chunk_x,
        manifest.chunk_z,
        &manifest.bounds_min,
        &manifest.bounds_max,
        &instances,
    )
}

fn summarize_scatter_chunk(
    source: &str,
    path: &Path,
    file_checksum: u64,
    layer_index: usize,
    layer_name: &str,
    kind: GroundcoverKind,
    chunk_x: i32,
    chunk_z: i32,
    bounds_min: &[f32; 3],
    bounds_max: &[f32; 3],
    instances: &[ScatterInstance],
) -> Result<ScatterChunkValidationSummary, NaturePatchError> {
    if instances.is_empty() {
        return Err(NaturePatchError::Validation(format!(
            "scatter {source} chunk {chunk_x},{chunk_z} must not be empty"
        )));
    }

    let mut position_min = [f32::MAX; 3];
    let mut position_max = [f32::MIN; 3];
    let mut yaw_min = f32::MAX;
    let mut yaw_max = f32::MIN;
    let mut height_min = f32::MAX;
    let mut height_max = f32::MIN;
    let mut width_min = f32::MAX;
    let mut width_max = f32::MIN;
    let mut phase_min = f32::MAX;
    let mut phase_max = f32::MIN;
    let mut color_variation_min = f32::MAX;
    let mut color_variation_max = f32::MIN;

    for instance in instances {
        for axis in 0..3 {
            position_min[axis] = position_min[axis].min(instance.position[axis]);
            position_max[axis] = position_max[axis].max(instance.position[axis]);
        }
        yaw_min = yaw_min.min(instance.yaw);
        yaw_max = yaw_max.max(instance.yaw);
        height_min = height_min.min(instance.height);
        height_max = height_max.max(instance.height);
        width_min = width_min.min(instance.width);
        width_max = width_max.max(instance.width);
        phase_min = phase_min.min(instance.phase);
        phase_max = phase_max.max(instance.phase);
        color_variation_min = color_variation_min.min(instance.color_variation);
        color_variation_max = color_variation_max.max(instance.color_variation);
    }

    Ok(ScatterChunkValidationSummary {
        source: source.to_string(),
        file: path.to_path_buf(),
        layer_index,
        layer_name: layer_name.to_string(),
        kind,
        chunk_x,
        chunk_z,
        instance_count: instances.len(),
        bounds_min: *bounds_min,
        bounds_max: *bounds_max,
        position_min,
        position_max,
        yaw_min,
        yaw_max,
        height_min,
        height_max,
        width_min,
        width_max,
        phase_min,
        phase_max,
        color_variation_min,
        color_variation_max,
        record_checksum: fnv1a_scatter_records(instances),
        file_checksum,
    })
}

fn validate_scatter_parity(
    json_chunks: &[ScatterChunkValidationSummary],
    binary_chunks: &[ScatterChunkValidationSummary],
) -> Result<ScatterParitySummary, NaturePatchError> {
    let json_by_key = json_chunks
        .iter()
        .map(|chunk| (scatter_summary_key(chunk), chunk))
        .collect::<BTreeMap<_, _>>();
    let binary_by_key = binary_chunks
        .iter()
        .map(|chunk| (scatter_summary_key(chunk), chunk))
        .collect::<BTreeMap<_, _>>();

    let mut summary = ScatterParitySummary {
        json_chunk_count: json_chunks.len(),
        binary_chunk_count: binary_chunks.len(),
        matching_chunk_count: 0,
        missing_binary_chunk_count: 0,
        extra_binary_chunk_count: 0,
        instance_count_mismatch_count: 0,
        bounds_mismatch_count: 0,
        record_checksum_mismatch_count: 0,
    };

    for (key, json) in &json_by_key {
        let Some(binary) = binary_by_key.get(key) else {
            summary.missing_binary_chunk_count += 1;
            continue;
        };
        summary.matching_chunk_count += 1;
        if json.instance_count != binary.instance_count {
            summary.instance_count_mismatch_count += 1;
        }
        if !bounds_equal(&json.bounds_min, &binary.bounds_min)
            || !bounds_equal(&json.bounds_max, &binary.bounds_max)
        {
            summary.bounds_mismatch_count += 1;
        }
        if json.record_checksum != binary.record_checksum {
            summary.record_checksum_mismatch_count += 1;
        }
    }

    for key in binary_by_key.keys() {
        if !json_by_key.contains_key(key) {
            summary.extra_binary_chunk_count += 1;
        }
    }

    if summary.missing_binary_chunk_count > 0
        || summary.extra_binary_chunk_count > 0
        || summary.instance_count_mismatch_count > 0
        || summary.bounds_mismatch_count > 0
        || summary.record_checksum_mismatch_count > 0
    {
        return Err(NaturePatchError::Validation(format!(
            "scatter JSON/binary parity failed: missing={}, extra={}, count mismatches={}, bounds mismatches={}, record checksum mismatches={}",
            summary.missing_binary_chunk_count,
            summary.extra_binary_chunk_count,
            summary.instance_count_mismatch_count,
            summary.bounds_mismatch_count,
            summary.record_checksum_mismatch_count
        )));
    }

    Ok(summary)
}

fn scatter_summary_key(summary: &ScatterChunkValidationSummary) -> String {
    format!(
        "{}:{:?}:{}:{}",
        summary.layer_index, summary.kind, summary.chunk_x, summary.chunk_z
    )
}

fn bounds_equal(left: &[f32; 3], right: &[f32; 3]) -> bool {
    (0..3).all(|axis| (left[axis] - right[axis]).abs() <= PACKAGE_VALIDATION_EPSILON)
}

fn fnv1a_scatter_records(instances: &[ScatterInstance]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for instance in instances {
        for value in [
            instance.position[0],
            instance.position[1],
            instance.position[2],
            instance.yaw,
            instance.height,
            instance.width,
            instance.phase,
            instance.color_variation,
        ] {
            for byte in value.to_le_bytes() {
                hash = fnv1a(hash, byte);
            }
        }
    }
    hash
}

fn validate_bounds(
    bounds_min: &[f32; 3],
    bounds_max: &[f32; 3],
    label: &str,
) -> Result<(), NaturePatchError> {
    for axis in 0..3 {
        if !bounds_min[axis].is_finite()
            || !bounds_max[axis].is_finite()
            || bounds_min[axis] > bounds_max[axis]
        {
            return Err(NaturePatchError::Validation(format!(
                "{label} must be finite and ordered"
            )));
        }
    }
    Ok(())
}

fn validate_scatter_instance(
    instance: &ScatterInstance,
    bounds_min: &[f32; 3],
    bounds_max: &[f32; 3],
    label: &str,
) -> Result<(), NaturePatchError> {
    for axis in 0..3 {
        let value = instance.position[axis];
        if !value.is_finite()
            || value < bounds_min[axis] - PACKAGE_VALIDATION_EPSILON
            || value > bounds_max[axis] + PACKAGE_VALIDATION_EPSILON
        {
            return Err(NaturePatchError::Validation(format!(
                "{label} position must be finite and inside chunk bounds"
            )));
        }
    }
    if !(0.0..=crate::constants::TAU).contains(&instance.yaw) || !instance.yaw.is_finite() {
        return Err(NaturePatchError::Validation(format!(
            "{label} yaw must be in 0..=TAU"
        )));
    }
    if instance.height <= 0.0 || !instance.height.is_finite() {
        return Err(NaturePatchError::Validation(format!(
            "{label} height multiplier must be positive"
        )));
    }
    if instance.width <= 0.0 || !instance.width.is_finite() {
        return Err(NaturePatchError::Validation(format!(
            "{label} width multiplier must be positive"
        )));
    }
    if !(0.0..=crate::constants::TAU).contains(&instance.phase) || !instance.phase.is_finite() {
        return Err(NaturePatchError::Validation(format!(
            "{label} phase must be in 0..=TAU"
        )));
    }
    if !(0.0..=1.0).contains(&instance.color_variation) || !instance.color_variation.is_finite() {
        return Err(NaturePatchError::Validation(format!(
            "{label} color variation must be in 0..=1"
        )));
    }
    Ok(())
}

fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn read_f32_le(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn nature_material_slots(profiles: &NatureProfiles) -> Vec<MaterialSlotManifest> {
    vec![
        MaterialSlotManifest {
            name: "terrain_surface".to_string(),
            purpose: "preview terrain and baked soil surface".to_string(),
            alpha_mode: "opaque".to_string(),
            double_sided: false,
            shadows: true,
        },
        MaterialSlotManifest {
            name: "groundcover_foliage".to_string(),
            purpose: "grass, moss, and low groundcover prototype meshes".to_string(),
            alpha_mode: "masked".to_string(),
            double_sided: true,
            shadows: profiles.mobile.shadows || profiles.console.shadows,
        },
    ]
}

fn nature_material_parameters(
    wind: &WindParams,
    profiles: &NatureProfiles,
) -> Vec<MaterialParameterSetManifest> {
    vec![
        MaterialParameterSetManifest {
            material_slot: "terrain_surface".to_string(),
            parameter_set: "midori_terrain_static_v1".to_string(),
            runtime_policy: "engine_native_static".to_string(),
            parameters: vec![
                material_parameter(
                    "Midori_MaskTexture",
                    "overlay_mask_texture",
                    "texture2d",
                    "maps/masks_rgba.png",
                    "maps/masks_rgba.png",
                ),
                material_parameter(
                    "Midori_MossMaskChannel",
                    "moss_mask_channel",
                    "channel",
                    "surface_overlays.moss.channel",
                    "G",
                ),
                material_parameter(
                    "Midori_WetnessMaskChannel",
                    "wetness_mask_channel",
                    "channel",
                    "surface_overlays.wetness.channel",
                    "B",
                ),
                material_parameter(
                    "Midori_CrackMaskChannel",
                    "crack_mask_channel",
                    "channel",
                    "surface_overlays.cracks.channel",
                    "A",
                ),
            ],
        },
        MaterialParameterSetManifest {
            material_slot: "groundcover_foliage".to_string(),
            parameter_set: "midori_groundcover_foliage_static_v1".to_string(),
            runtime_policy: "engine_native_static".to_string(),
            parameters: vec![
                material_parameter(
                    "Midori_AlphaCutoff",
                    "alpha_cutoff",
                    "float",
                    "material_slots.groundcover_foliage.alpha_mode",
                    "0.5",
                ),
                material_parameter(
                    "Midori_WindStrength",
                    "wind_strength",
                    "float",
                    "wind.strength",
                    &format_float(wind.strength),
                ),
                material_parameter(
                    "Midori_WindSpeed",
                    "wind_speed",
                    "float",
                    "wind.speed",
                    &format_float(wind.speed),
                ),
                material_parameter(
                    "Midori_WindDirectionDegrees",
                    "wind_direction_degrees",
                    "float",
                    "wind.direction_degrees",
                    &format_float(wind.direction_degrees),
                ),
                material_parameter(
                    "Midori_WindGustScale",
                    "wind_gust_scale",
                    "float",
                    "wind.gust_scale",
                    &format_float(wind.gust_scale),
                ),
                material_parameter(
                    "Midori_FadeStartMeters",
                    "fade_start_meters",
                    "float",
                    "profiles.mobile.cull_start",
                    &format_float(profiles.mobile.cull_start),
                ),
                material_parameter(
                    "Midori_FadeEndMeters",
                    "fade_end_meters",
                    "float",
                    "profiles.mobile.cull_end",
                    &format_float(profiles.mobile.cull_end),
                ),
                material_parameter(
                    "Midori_ColorVariationScale",
                    "color_variation_scale",
                    "float",
                    "COLOR_0.y",
                    "1.0",
                ),
            ],
        },
    ]
}

fn material_parameter(
    name: &str,
    semantic: &str,
    value_type: &str,
    source: &str,
    default_value: &str,
) -> MaterialParameterBindingManifest {
    MaterialParameterBindingManifest {
        name: name.to_string(),
        semantic: semantic.to_string(),
        value_type: value_type.to_string(),
        source: source.to_string(),
        default_value: default_value.to_string(),
    }
}

fn nature_material_recipes() -> Vec<MaterialRecipeManifest> {
    vec![
        MaterialRecipeManifest {
            material_slot: "terrain_surface".to_string(),
            file: "materials/terrain_surface.recipe.json".to_string(),
            runtime_policy: "engine_native_static".to_string(),
            engine_targets: vec![
                "unity_terrain_material".to_string(),
                "unreal_landscape_material".to_string(),
            ],
        },
        MaterialRecipeManifest {
            material_slot: "groundcover_foliage".to_string(),
            file: "materials/groundcover_foliage.recipe.json".to_string(),
            runtime_policy: "engine_native_static".to_string(),
            engine_targets: vec![
                "unity_detail_mesh_material".to_string(),
                "unreal_static_mesh_foliage_material".to_string(),
            ],
        },
    ]
}

fn nature_engine_import_recipes() -> Vec<EngineImportRecipeManifest> {
    vec![
        EngineImportRecipeManifest {
            engine: "unity".to_string(),
            file: "engines/unity_import.recipe.json".to_string(),
            profile: "mobile".to_string(),
            runtime_policy: "engine_native_static".to_string(),
            expected_systems: vec![
                "Unity TerrainData".to_string(),
                "GPU-instanced terrain detail mesh prefabs".to_string(),
                "Midori scatter ScriptableObject".to_string(),
            ],
        },
        EngineImportRecipeManifest {
            engine: "unreal".to_string(),
            file: "engines/unreal_import.recipe.json".to_string(),
            profile: "console".to_string(),
            runtime_policy: "engine_native_static".to_string(),
            expected_systems: vec![
                "Unreal Landscape".to_string(),
                "Static Mesh Foliage".to_string(),
                "Midori binary scatter importer".to_string(),
            ],
        },
    ]
}

fn material_recipe_file(
    manifest: &NatureExportManifest,
    recipe: &MaterialRecipeManifest,
) -> Result<MaterialRecipeFile, NaturePatchError> {
    let parameter_set = manifest
        .material_parameters
        .iter()
        .find(|set| set.material_slot == recipe.material_slot)
        .ok_or_else(|| {
            NaturePatchError::Validation(format!(
                "material recipe `{}` has no matching parameter set",
                recipe.file
            ))
        })?;

    let (required_textures, required_vertex_streams, surface_overlays, notes) =
        match recipe.material_slot.as_str() {
            "terrain_surface" => (
                vec!["maps/masks_rgba.png".to_string()],
                Vec::new(),
                manifest
                    .surface_overlays
                    .iter()
                    .map(|overlay| overlay.name.clone())
                    .collect(),
                vec![
                    "Use the packed mask texture channels for static moss, wetness, and crack overlays.".to_string(),
                    "Create engine-native terrain or landscape materials; do not inject runtime shaders.".to_string(),
                ],
            ),
            "groundcover_foliage" => (
                Vec::new(),
                vec![
                    "TEXCOORD_1.x phase_radians".to_string(),
                    "TEXCOORD_1.y bend_stiffness".to_string(),
                    "COLOR_0.x normalized_height".to_string(),
                    "COLOR_0.y color_variation".to_string(),
                    "COLOR_0.z normalized_progress".to_string(),
                    "COLOR_0.w bend_stiffness".to_string(),
                ],
                Vec::new(),
                vec![
                    "Use masked alpha and engine-native foliage/detail instancing.".to_string(),
                    "Drive lightweight vertex-stage wind from packed TEXCOORD_1 and COLOR_0 channels.".to_string(),
                    "Use cull/fade parameters from the active mobile or console profile.".to_string(),
                ],
            ),
            _ => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
        };

    Ok(MaterialRecipeFile {
        schema: "midori.material_recipe.v1".to_string(),
        material_slot: recipe.material_slot.clone(),
        parameter_set: parameter_set.parameter_set.clone(),
        runtime_policy: recipe.runtime_policy.clone(),
        shader_policy: manifest.shader_policy.clone(),
        texture_pipeline: manifest.texture_pipeline.clone(),
        engine_targets: recipe.engine_targets.clone(),
        parameters: parameter_set.parameters.clone(),
        required_textures,
        required_vertex_streams,
        surface_overlays,
        notes,
    })
}

fn engine_import_recipe_file(
    manifest: &NatureExportManifest,
    recipe: &EngineImportRecipeManifest,
) -> Result<EngineImportRecipeFile, NaturePatchError> {
    let groundcover_slot = manifest
        .material_slots
        .iter()
        .find(|slot| slot.name == "groundcover_foliage")
        .ok_or_else(|| {
            NaturePatchError::Validation(
                "engine import recipe requires groundcover_foliage material slot".to_string(),
            )
        })?;
    let groundcover_material_recipe_file =
        material_recipe_file_for_slot(manifest, "groundcover_foliage").ok_or_else(|| {
            NaturePatchError::Validation(
                "engine import recipe requires groundcover_foliage material recipe".to_string(),
            )
        })?;

    let scatter = EngineScatterImportRecipe {
        placement_source: match recipe.engine.as_str() {
            "unity" => {
                "grass_density/masks plus midori.scatter.bin.v1 ScriptableObject".to_string()
            }
            "unreal" => "midori.scatter.bin.v1 foliage placement buffers".to_string(),
            _ => "midori.scatter.bin.v1".to_string(),
        },
        scatter_json_file: manifest.scatter.file.clone().unwrap_or_default(),
        binary_chunk_count: manifest.scatter.binary_files.len(),
        instance_count: manifest_scatter_instance_count(manifest),
        chunk_size_meters: manifest.scatter.chunk_size,
    };
    let groundcover = EngineGroundcoverImportRecipe {
        target_system: match recipe.engine.as_str() {
            "unity" => "Unity Terrain detail mesh prefabs".to_string(),
            "unreal" => "Unreal Static Mesh Foliage".to_string(),
            _ => "Engine-native foliage/detail system".to_string(),
        },
        prototype_source: match recipe.engine.as_str() {
            "unity" => "LOD0 GLB detail mesh prefabs with native GLB fallback".to_string(),
            "unreal" => "LOD0 GLB Static Mesh assets".to_string(),
            _ => "LOD0 GLB prototypes".to_string(),
        },
        material_slot: groundcover_slot.name.clone(),
        material_recipe_file: groundcover_material_recipe_file,
        prototype_family_count: manifest.prototypes.len(),
        lod0_prototype_count: lod0_prototype_count(manifest),
        double_sided: groundcover_slot.double_sided,
        alpha_mode: groundcover_slot.alpha_mode.clone(),
    };

    let (terrain, profile_settings, notes) = match recipe.engine.as_str() {
        "unity" => (
            EngineTerrainImportRecipe {
                target_system: "Unity TerrainData".to_string(),
                heightmap_file: manifest.unity.terrain_heightmap.clone(),
                mask_file: manifest.terrain.masks_file.clone(),
                density_map_file: manifest.unity.detail_density_map.clone(),
                normal_map_file: manifest.unity.normal_map.clone(),
                tile_size_meters: manifest.tile_size,
                height_min: manifest.terrain.height_min,
                height_max: manifest.terrain.height_max,
            },
            profile_import_recipe("mobile", &manifest.mobile),
            vec![
                "Use Unity TerrainData plus GPU-instanced detail mesh prefabs; no custom runtime renderer is required.".to_string(),
                "Validate mobile density, cull range, material slot count, and scatter source fingerprints in the Unity import report.".to_string(),
            ],
        ),
        "unreal" => (
            EngineTerrainImportRecipe {
                target_system: "Unreal Landscape".to_string(),
                heightmap_file: manifest.unreal.landscape_heightmap.clone(),
                mask_file: manifest.unreal.landscape_weightmap.clone(),
                density_map_file: manifest.terrain.grass_density_file.clone(),
                normal_map_file: manifest.unreal.normal_map.clone(),
                tile_size_meters: manifest.tile_size,
                height_min: manifest.terrain.height_min,
                height_max: manifest.terrain.height_max,
            },
            profile_import_recipe("console", &manifest.console),
            vec![
                "Use Unreal Landscape plus Static Mesh Foliage; no custom runtime renderer is required.".to_string(),
                "Convert console cull distances from meters to centimeters when creating foliage type assets.".to_string(),
            ],
        ),
        other => {
            return Err(NaturePatchError::Validation(format!(
                "unsupported engine import recipe `{other}`"
            )));
        }
    };

    Ok(EngineImportRecipeFile {
        schema: "midori.engine_import_recipe.v1".to_string(),
        engine: recipe.engine.clone(),
        profile: recipe.profile.clone(),
        runtime_policy: recipe.runtime_policy.clone(),
        expected_systems: recipe.expected_systems.clone(),
        source_files: expected_engine_import_source_files(manifest, &recipe.engine),
        material_recipe_files: material_recipe_files(manifest),
        terrain,
        groundcover,
        scatter,
        profile_settings,
        notes,
    })
}

fn expected_engine_import_source_files(
    manifest: &NatureExportManifest,
    engine: &str,
) -> Vec<String> {
    let normal_map = if engine == "unreal" {
        manifest.normal_conventions.unreal_yminus_file.as_str()
    } else {
        manifest.normal_conventions.unity_yplus_file.as_str()
    };
    let mut files = BTreeSet::from([
        "midori_nature.json".to_string(),
        "preview_tile.glb".to_string(),
        manifest.terrain.heightmap_file.clone(),
        manifest.terrain.masks_file.clone(),
        manifest.terrain.grass_density_file.clone(),
        normal_map.to_string(),
    ]);
    if let Some(scatter_file) = &manifest.scatter.file {
        files.insert(scatter_file.clone());
    }
    for binary in &manifest.scatter.binary_files {
        files.insert(binary.file.clone());
    }
    for prototype in &manifest.prototypes {
        for lod in &prototype.lods {
            files.insert(lod.file.clone());
        }
    }
    for recipe in &manifest.material_recipes {
        files.insert(recipe.file.clone());
    }
    for recipe in &manifest.engine_import_recipes {
        files.insert(recipe.file.clone());
    }
    files.into_iter().collect()
}

fn material_recipe_files(manifest: &NatureExportManifest) -> Vec<String> {
    manifest
        .material_recipes
        .iter()
        .map(|recipe| recipe.file.clone())
        .collect()
}

fn material_recipe_file_for_slot(manifest: &NatureExportManifest, slot: &str) -> Option<String> {
    manifest
        .material_recipes
        .iter()
        .find(|recipe| recipe.material_slot == slot)
        .map(|recipe| recipe.file.clone())
}

fn manifest_scatter_instance_count(manifest: &NatureExportManifest) -> usize {
    manifest
        .scatter
        .binary_files
        .iter()
        .map(|binary| binary.instance_count)
        .sum()
}

fn lod0_prototype_count(manifest: &NatureExportManifest) -> usize {
    manifest
        .prototypes
        .iter()
        .filter(|prototype| prototype.lods.iter().any(|lod| lod.index == 0))
        .count()
}

fn profile_import_recipe(name: &str, profile: &NatureProfile) -> EngineProfileImportRecipe {
    EngineProfileImportRecipe {
        name: name.to_string(),
        density_scale: profile.density_scale,
        lod0_max_distance_meters: profile.lod0_max_distance,
        lod1_max_distance_meters: profile.lod1_max_distance,
        lod2_max_distance_meters: profile.lod2_max_distance,
        cull_start_meters: profile.cull_start,
        cull_end_meters: profile.cull_end,
        shadows: profile.shadows,
        material_slots: profile.material_slots,
        max_instances_per_tile: profile.max_instances_per_tile,
        max_instances_per_chunk: profile.max_instances_per_chunk,
        grass_collision: profile.grass_collision,
        moss_collision: profile.moss_collision,
    }
}

fn format_float(value: f32) -> String {
    format!("{value:.3}")
}

fn nature_surface_overlays() -> Vec<SurfaceOverlayManifest> {
    vec![
        SurfaceOverlayManifest {
            name: "moss".to_string(),
            source_file: "maps/masks_rgba.png".to_string(),
            channel: "G".to_string(),
            targets: vec![
                "terrain_surface".to_string(),
                "rock".to_string(),
                "log".to_string(),
                "shrub_base".to_string(),
                "trunk_base".to_string(),
                "roots".to_string(),
            ],
            application:
                "static material overlay mask; authored prototypes may include geometry moss cards"
                    .to_string(),
            runtime_policy: "baked_static".to_string(),
        },
        SurfaceOverlayManifest {
            name: "wetness".to_string(),
            source_file: "maps/masks_rgba.png".to_string(),
            channel: "B".to_string(),
            targets: vec![
                "terrain_surface".to_string(),
                "rock".to_string(),
                "log".to_string(),
            ],
            application:
                "static material overlay mask for externally supplied roughness/darkening inputs"
                    .to_string(),
            runtime_policy: "baked_static".to_string(),
        },
        SurfaceOverlayManifest {
            name: "cracks".to_string(),
            source_file: "maps/masks_rgba.png".to_string(),
            channel: "A".to_string(),
            targets: vec![
                "terrain_surface".to_string(),
                "dry_soil".to_string(),
                "scatter_exclusion".to_string(),
            ],
            application: "static soil crack and optional scatter-exclusion mask".to_string(),
            runtime_policy: "baked_static".to_string(),
        },
    ]
}

fn prototype_manifests(prototypes: &[GroundcoverPrototype]) -> Vec<GroundcoverPrototypeManifest> {
    prototypes
        .iter()
        .map(|prototype| GroundcoverPrototypeManifest {
            name: prototype.name.clone(),
            kind: prototype.kind,
            material_slot: "groundcover_foliage".to_string(),
            surface_targets: prototype_surface_targets(prototype.kind),
            lods: prototype
                .lods
                .iter()
                .map(|lod| {
                    let (bounds_min, bounds_max) = mesh_bounds(&lod.mesh);
                    GroundcoverPrototypeLodManifest {
                        index: lod.index,
                        file: format!(
                            "prototypes/{}",
                            prototype_lod_filename(&prototype.name, lod.index)
                        ),
                        vertex_count: lod.mesh.vertex_count(),
                        triangle_count: lod.mesh.triangle_count(),
                        bounds_min,
                        bounds_max,
                    }
                })
                .collect(),
        })
        .collect()
}

fn prototype_surface_targets(kind: GroundcoverKind) -> Vec<String> {
    required_surface_targets_for_kind(kind)
        .iter()
        .map(|target| (*target).to_string())
        .collect()
}

fn required_surface_targets_for_kind(kind: GroundcoverKind) -> &'static [&'static str] {
    match kind {
        GroundcoverKind::Grass => &["groundcover_foliage"],
        GroundcoverKind::Moss => &["groundcover_foliage", "moss_tuft"],
        GroundcoverKind::Flower | GroundcoverKind::Weed | GroundcoverKind::Litter => {
            &["groundcover_foliage"]
        }
        GroundcoverKind::Shrub => &["groundcover_foliage", "shrub_base"],
        GroundcoverKind::Rock => &["static_surface", "rock"],
        GroundcoverKind::Log => &["static_surface", "log"],
    }
}

fn nature_mesh_export_config() -> ExportConfig {
    ExportConfig {
        format: ExportFormat::Glb,
        draco: false,
        embed_textures: false,
        pivot_painter_extras: true,
        metadata: None,
    }
}

fn prototype_lod_filename(name: &str, index: u32) -> String {
    format!("{}_lod{index}.glb", sanitize_filename(name))
}

fn sanitize_filename(name: &str) -> String {
    let mut out = String::new();
    let mut last_was_separator = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator {
            out.push('_');
            last_was_separator = true;
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "asset".to_string()
    } else {
        trimmed.to_string()
    }
}

fn mesh_bounds(mesh: &Mesh) -> ([f32; 3], [f32; 3]) {
    if mesh.vertices.is_empty() {
        return ([0.0; 3], [0.0; 3]);
    }

    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for vertex in &mesh.vertices {
        min = min.min(vertex.position);
        max = max.max(vertex.position);
    }
    ([min.x, min.y, min.z], [max.x, max.y, max.z])
}

fn generate_grass_prototype(layer: &GroundcoverLayer) -> GroundcoverPrototype {
    GroundcoverPrototype {
        name: if layer.name.is_empty() {
            "grass".to_string()
        } else {
            layer.name.clone()
        },
        kind: GroundcoverKind::Grass,
        lods: vec![
            GroundcoverPrototypeLod {
                index: 0,
                name: "LOD0".to_string(),
                mesh: build_grass_clump_mesh(layer, 6, 3, 0),
            },
            GroundcoverPrototypeLod {
                index: 1,
                name: "LOD1".to_string(),
                mesh: build_grass_clump_mesh(layer, 4, 1, 1),
            },
            GroundcoverPrototypeLod {
                index: 2,
                name: "LOD2".to_string(),
                mesh: build_grass_clump_mesh(layer, 2, 1, 2),
            },
        ],
    }
}

fn build_grass_clump_mesh(
    layer: &GroundcoverLayer,
    blade_count: u32,
    segments: u32,
    lod_index: u32,
) -> Mesh {
    let mut rng = Rng::from_seed(0x6772_6173_7300u64 ^ ((lod_index as u64 + 1) << 8));
    let mut mesh = Mesh::new();
    let base_width = layer.width.max(0.01);
    let base_height = layer.height.max(0.05);

    for blade in 0..blade_count {
        let angle =
            (blade as f32 / blade_count as f32) * crate::constants::TAU + rng.range(-0.35, 0.35);
        let radius = rng.range(0.0, base_width * 1.8);
        let base = Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
        let yaw = angle + rng.range(-0.5, 0.5);
        let height = base_height * rng.range(0.75, 1.15);
        let width = base_width * rng.range(0.7, 1.2);
        let curl = layer.curl * rng.range(0.7, 1.3);
        let phase = rng.range(0.0, crate::constants::TAU);
        let color_variation = rng.next_f32();

        append_blade_strip(
            &mut mesh,
            base,
            yaw,
            height,
            width,
            curl,
            phase,
            color_variation,
            segments,
        );
    }

    mesh.submeshes.push(Submesh {
        index_start: 0,
        index_count: mesh.indices.len() as u32,
        material: MaterialType::Leaves,
    });
    mesh.recalculate_normals();
    mesh
}

#[allow(clippy::too_many_arguments)]
fn append_blade_strip(
    mesh: &mut Mesh,
    base: Vec3,
    yaw: f32,
    height: f32,
    width: f32,
    curl: f32,
    phase: f32,
    color_variation: f32,
    segments: u32,
) {
    let start = mesh.vertices.len() as u32;
    let forward = Vec3::new(yaw.sin(), 0.0, yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());

    for segment in 0..=segments {
        let t = segment as f32 / segments.max(1) as f32;
        let stiffness = 1.0 - t;
        let center = base + Vec3::Y * height * t + forward * curl.sin() * height * t * t * 0.22;
        let width_at_t = width * (1.0 - t * 0.82).max(0.12);
        let left = center - right * width_at_t * 0.5;
        let right_pos = center + right * width_at_t * 0.5;

        mesh.vertices.push(Vertex {
            position: left,
            normal: forward,
            uv: Vec2::new(0.0, t),
            uv2: Vec2::new(phase, stiffness),
            color: Vec4::new(height, color_variation, t, stiffness),
        });
        mesh.vertices.push(Vertex {
            position: right_pos,
            normal: forward,
            uv: Vec2::new(1.0, t),
            uv2: Vec2::new(phase, stiffness),
            color: Vec4::new(height, color_variation, t, stiffness),
        });
    }

    for segment in 0..segments {
        let a = start + segment * 2;
        mesh.indices
            .extend_from_slice(&[a, a + 1, a + 3, a, a + 3, a + 2]);
    }
}

fn generate_moss_prototype(layer: &GroundcoverLayer) -> GroundcoverPrototype {
    GroundcoverPrototype {
        name: if layer.name.is_empty() {
            "moss".to_string()
        } else {
            layer.name.clone()
        },
        kind: GroundcoverKind::Moss,
        lods: vec![
            GroundcoverPrototypeLod {
                index: 0,
                name: "LOD0".to_string(),
                mesh: build_moss_tuft_mesh(layer, 6, 0),
            },
            GroundcoverPrototypeLod {
                index: 1,
                name: "LOD1".to_string(),
                mesh: build_moss_tuft_mesh(layer, 3, 1),
            },
        ],
    }
}

fn build_moss_tuft_mesh(layer: &GroundcoverLayer, card_count: u32, lod_index: u32) -> Mesh {
    let mut rng = Rng::from_seed(0x6d6f_7373_0000u64 ^ ((lod_index as u64 + 1) << 8));
    let mut mesh = Mesh::new();
    let radius = layer.width.max(0.04) * 1.8;
    let height = layer.height.max(0.03);

    for card in 0..card_count {
        let yaw = (card as f32 / card_count as f32) * crate::constants::TAU + rng.range(-0.2, 0.2);
        let offset_angle = rng.range(0.0, crate::constants::TAU);
        let offset_radius = rng.range(0.0, radius * 0.45);
        let center = Vec3::new(
            offset_angle.cos() * offset_radius,
            0.0,
            offset_angle.sin() * offset_radius,
        );
        append_moss_card(
            &mut mesh,
            center,
            yaw,
            radius * rng.range(0.65, 1.05),
            height * rng.range(0.65, 1.15),
            rng.next_f32(),
        );
    }

    mesh.submeshes.push(Submesh {
        index_start: 0,
        index_count: mesh.indices.len() as u32,
        material: MaterialType::Leaves,
    });
    mesh.recalculate_normals();
    mesh
}

fn append_moss_card(
    mesh: &mut Mesh,
    center: Vec3,
    yaw: f32,
    radius: f32,
    height: f32,
    color_variation: f32,
) {
    let start = mesh.vertices.len() as u32;
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let normal = Vec3::new(yaw.sin(), 0.0, yaw.cos());
    let bottom_left = center - right * radius * 0.5;
    let bottom_right = center + right * radius * 0.5;
    let top_left = bottom_left + Vec3::Y * height;
    let top_right = bottom_right + Vec3::Y * height;

    for (position, uv, t) in [
        (bottom_left, Vec2::new(0.0, 0.0), 0.0),
        (bottom_right, Vec2::new(1.0, 0.0), 0.0),
        (top_right, Vec2::new(1.0, 1.0), 1.0),
        (top_left, Vec2::new(0.0, 1.0), 1.0),
    ] {
        mesh.vertices.push(Vertex {
            position,
            normal,
            uv,
            uv2: Vec2::new(color_variation, 1.0 - t),
            color: Vec4::new(height, color_variation, t, 1.0 - t),
        });
    }
    mesh.indices
        .extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
}

fn generate_flower_prototype(layer: &GroundcoverLayer) -> GroundcoverPrototype {
    GroundcoverPrototype {
        name: if layer.name.is_empty() {
            "flower".to_string()
        } else {
            layer.name.clone()
        },
        kind: GroundcoverKind::Flower,
        lods: vec![
            GroundcoverPrototypeLod {
                index: 0,
                name: "LOD0".to_string(),
                mesh: build_flower_clump_mesh(layer, 3, 5, 0),
            },
            GroundcoverPrototypeLod {
                index: 1,
                name: "LOD1".to_string(),
                mesh: build_flower_clump_mesh(layer, 2, 4, 1),
            },
            GroundcoverPrototypeLod {
                index: 2,
                name: "LOD2".to_string(),
                mesh: build_flower_clump_mesh(layer, 1, 3, 2),
            },
        ],
    }
}

fn build_flower_clump_mesh(
    layer: &GroundcoverLayer,
    flower_count: u32,
    petal_count: u32,
    lod_index: u32,
) -> Mesh {
    let mut rng = Rng::from_seed(0x666c_6f77_6572_0000u64 ^ ((lod_index as u64 + 1) << 8));
    let mut mesh = Mesh::new();
    let base_width = layer.width.max(0.025);
    let base_height = layer.height.clamp(0.08, 0.8);

    for flower in 0..flower_count {
        let angle = (flower as f32 / flower_count.max(1) as f32) * crate::constants::TAU
            + rng.range(-0.5, 0.5);
        let radius = rng.range(0.0, base_width * 2.5);
        let base = Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
        let yaw = angle + rng.range(-0.6, 0.6);
        let height = base_height * rng.range(0.75, 1.15);
        let phase = rng.range(0.0, crate::constants::TAU);
        let variation = rng.next_f32();

        append_blade_strip(
            &mut mesh,
            base,
            yaw,
            height * 0.82,
            base_width * 0.28,
            layer.curl.max(0.08),
            phase,
            variation,
            1,
        );

        if lod_index < 2 {
            append_blade_strip(
                &mut mesh,
                base,
                yaw + 1.6,
                height * 0.35,
                base_width * 1.8,
                0.25,
                phase,
                variation,
                1,
            );
        }

        append_flower_head(
            &mut mesh,
            base + Vec3::Y * height,
            yaw,
            base_width * 2.2,
            petal_count,
            phase,
            variation,
        );
    }

    mesh.submeshes.push(Submesh {
        index_start: 0,
        index_count: mesh.indices.len() as u32,
        material: MaterialType::Leaves,
    });
    mesh.recalculate_normals();
    mesh
}

fn append_flower_head(
    mesh: &mut Mesh,
    center: Vec3,
    yaw: f32,
    radius: f32,
    petal_count: u32,
    phase: f32,
    color_variation: f32,
) {
    for petal in 0..petal_count.max(1) {
        let angle = yaw + (petal as f32 / petal_count.max(1) as f32) * crate::constants::TAU;
        let forward = Vec3::new(angle.sin(), 0.08, angle.cos()).normalize_or_zero();
        let right = Vec3::new(angle.cos(), 0.0, -angle.sin());
        let start = mesh.vertices.len() as u32;
        let inner = center + forward * radius * 0.15;
        let outer = center + forward * radius;
        let width = radius * 0.32;

        for (position, uv, t) in [
            (inner - right * width * 0.35, Vec2::new(0.15, 0.0), 0.0),
            (inner + right * width * 0.35, Vec2::new(0.85, 0.0), 0.0),
            (outer + right * width, Vec2::new(1.0, 1.0), 1.0),
            (outer - right * width, Vec2::new(0.0, 1.0), 1.0),
        ] {
            mesh.vertices.push(Vertex {
                position,
                normal: Vec3::Y,
                uv,
                uv2: Vec2::new(phase, 1.0 - t),
                color: Vec4::new(radius, color_variation, t, 1.0 - t),
            });
        }
        mesh.indices
            .extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
    }
}

fn generate_weed_prototype(layer: &GroundcoverLayer) -> GroundcoverPrototype {
    GroundcoverPrototype {
        name: if layer.name.is_empty() {
            "weed".to_string()
        } else {
            layer.name.clone()
        },
        kind: GroundcoverKind::Weed,
        lods: vec![
            GroundcoverPrototypeLod {
                index: 0,
                name: "LOD0".to_string(),
                mesh: build_weed_clump_mesh(layer, 5, 2, 0),
            },
            GroundcoverPrototypeLod {
                index: 1,
                name: "LOD1".to_string(),
                mesh: build_weed_clump_mesh(layer, 3, 1, 1),
            },
            GroundcoverPrototypeLod {
                index: 2,
                name: "LOD2".to_string(),
                mesh: build_weed_clump_mesh(layer, 1, 1, 2),
            },
        ],
    }
}

fn build_weed_clump_mesh(
    layer: &GroundcoverLayer,
    leaf_count: u32,
    segments: u32,
    lod_index: u32,
) -> Mesh {
    let mut rng = Rng::from_seed(0x7765_6564_0000u64 ^ ((lod_index as u64 + 1) << 8));
    let mut mesh = Mesh::new();
    let base_width = layer.width.max(0.035);
    let base_height = layer.height.clamp(0.12, 1.1);

    for leaf in 0..leaf_count {
        let angle =
            (leaf as f32 / leaf_count.max(1) as f32) * crate::constants::TAU + rng.range(-0.4, 0.4);
        let radius = rng.range(0.0, base_width * 1.6);
        let base = Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
        let height = base_height * rng.range(0.65, 1.15);
        let width = base_width * rng.range(1.4, 2.4);
        append_blade_strip(
            &mut mesh,
            base,
            angle + rng.range(-0.45, 0.45),
            height,
            width,
            layer.curl.max(0.18),
            rng.range(0.0, crate::constants::TAU),
            rng.next_f32(),
            segments,
        );
    }

    mesh.submeshes.push(Submesh {
        index_start: 0,
        index_count: mesh.indices.len() as u32,
        material: MaterialType::Leaves,
    });
    mesh.recalculate_normals();
    mesh
}

fn generate_litter_prototype(layer: &GroundcoverLayer) -> GroundcoverPrototype {
    GroundcoverPrototype {
        name: if layer.name.is_empty() {
            "litter".to_string()
        } else {
            layer.name.clone()
        },
        kind: GroundcoverKind::Litter,
        lods: vec![
            GroundcoverPrototypeLod {
                index: 0,
                name: "LOD0".to_string(),
                mesh: build_litter_patch_mesh(layer, 6, 0),
            },
            GroundcoverPrototypeLod {
                index: 1,
                name: "LOD1".to_string(),
                mesh: build_litter_patch_mesh(layer, 3, 1),
            },
        ],
    }
}

fn build_litter_patch_mesh(layer: &GroundcoverLayer, card_count: u32, lod_index: u32) -> Mesh {
    let mut rng = Rng::from_seed(0x6c69_7474_6572_0000u64 ^ ((lod_index as u64 + 1) << 8));
    let mut mesh = Mesh::new();
    let base_width = layer.width.max(0.06);
    let base_length = layer.height.clamp(0.08, 0.5);

    for card in 0..card_count {
        let angle = rng.range(0.0, crate::constants::TAU);
        let radius = rng.range(0.0, base_width * 2.4 + card as f32 * 0.01);
        let center = Vec3::new(angle.cos() * radius, 0.003, angle.sin() * radius);
        append_litter_card(
            &mut mesh,
            center,
            rng.range(0.0, crate::constants::TAU),
            base_width * rng.range(0.7, 1.3),
            base_length * rng.range(0.65, 1.15),
            rng.range(0.0, crate::constants::TAU),
            rng.next_f32(),
        );
    }

    mesh.submeshes.push(Submesh {
        index_start: 0,
        index_count: mesh.indices.len() as u32,
        material: MaterialType::Leaves,
    });
    mesh.recalculate_normals();
    mesh
}

fn append_litter_card(
    mesh: &mut Mesh,
    center: Vec3,
    yaw: f32,
    width: f32,
    length: f32,
    phase: f32,
    color_variation: f32,
) {
    let start = mesh.vertices.len() as u32;
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let forward = Vec3::new(yaw.sin(), 0.0, yaw.cos());
    let p0 = center - right * width * 0.5 - forward * length * 0.5;
    let p1 = center + right * width * 0.5 - forward * length * 0.5;
    let p2 = center + right * width * 0.5 + forward * length * 0.5;
    let p3 = center - right * width * 0.5 + forward * length * 0.5;

    for (position, uv, t) in [
        (p0, Vec2::new(0.0, 0.0), 0.0),
        (p1, Vec2::new(1.0, 0.0), 0.0),
        (p2, Vec2::new(1.0, 1.0), 1.0),
        (p3, Vec2::new(0.0, 1.0), 1.0),
    ] {
        mesh.vertices.push(Vertex {
            position,
            normal: Vec3::Y,
            uv,
            uv2: Vec2::new(phase, 1.0),
            color: Vec4::new(length, color_variation, t, 1.0),
        });
    }
    mesh.indices
        .extend_from_slice(&[start, start + 2, start + 1, start, start + 3, start + 2]);
}

fn generate_shrub_prototype(layer: &GroundcoverLayer) -> GroundcoverPrototype {
    GroundcoverPrototype {
        name: if layer.name.is_empty() {
            "shrub".to_string()
        } else {
            layer.name.clone()
        },
        kind: GroundcoverKind::Shrub,
        lods: vec![
            GroundcoverPrototypeLod {
                index: 0,
                name: "LOD0".to_string(),
                mesh: build_shrub_clump_mesh(layer, 5, 2, 2, 0),
            },
            GroundcoverPrototypeLod {
                index: 1,
                name: "LOD1".to_string(),
                mesh: build_shrub_clump_mesh(layer, 3, 1, 1, 1),
            },
            GroundcoverPrototypeLod {
                index: 2,
                name: "LOD2".to_string(),
                mesh: build_shrub_card_mesh(layer, 2),
            },
        ],
    }
}

fn build_shrub_clump_mesh(
    layer: &GroundcoverLayer,
    stem_count: u32,
    leaf_cards_per_stem: u32,
    segments: u32,
    lod_index: u32,
) -> Mesh {
    let mut rng = Rng::from_seed(0x7368_7275_6200u64 ^ ((lod_index as u64 + 1) << 8));
    let mut mesh = Mesh::new();
    let base_width = layer.width.max(0.05);
    let base_height = layer.height.clamp(0.2, 1.6);

    for stem in 0..stem_count {
        let angle = (stem as f32 / stem_count.max(1) as f32) * crate::constants::TAU
            + rng.range(-0.45, 0.45);
        let base_radius = rng.range(0.0, base_width * 1.8);
        let base = Vec3::new(angle.cos() * base_radius, 0.0, angle.sin() * base_radius);
        let height = base_height * rng.range(0.65, 1.15);
        let yaw = angle + rng.range(-0.55, 0.55);
        let phase = rng.range(0.0, crate::constants::TAU);
        let variation = rng.next_f32();

        append_blade_strip(
            &mut mesh,
            base,
            yaw,
            height * 0.78,
            base_width * 0.38,
            layer.curl.max(0.18),
            phase,
            variation,
            segments,
        );

        for card in 0..leaf_cards_per_stem {
            append_moss_card(
                &mut mesh,
                base + Vec3::Y * height * rng.range(0.28, 0.58),
                yaw + (card as f32 / leaf_cards_per_stem.max(1) as f32) * crate::constants::TAU,
                base_width * rng.range(2.4, 3.8),
                height * rng.range(0.25, 0.42),
                variation,
            );
        }
    }

    mesh.submeshes.push(Submesh {
        index_start: 0,
        index_count: mesh.indices.len() as u32,
        material: MaterialType::Leaves,
    });
    mesh.recalculate_normals();
    mesh
}

fn build_shrub_card_mesh(layer: &GroundcoverLayer, card_count: u32) -> Mesh {
    let mut mesh = Mesh::new();
    let width = layer.width.max(0.06) * 4.2;
    let height = layer.height.clamp(0.2, 1.6) * 0.9;
    for card in 0..card_count.max(1) {
        append_moss_card(
            &mut mesh,
            Vec3::ZERO,
            (card as f32 / card_count.max(1) as f32) * crate::constants::TAU,
            width,
            height,
            card as f32 / card_count.max(1) as f32,
        );
    }
    mesh.submeshes.push(Submesh {
        index_start: 0,
        index_count: mesh.indices.len() as u32,
        material: MaterialType::Leaves,
    });
    mesh.recalculate_normals();
    mesh
}

fn generate_rock_prototype(layer: &GroundcoverLayer) -> GroundcoverPrototype {
    GroundcoverPrototype {
        name: if layer.name.is_empty() {
            "rock".to_string()
        } else {
            layer.name.clone()
        },
        kind: GroundcoverKind::Rock,
        lods: vec![
            GroundcoverPrototypeLod {
                index: 0,
                name: "LOD0".to_string(),
                mesh: build_rock_mesh(layer, 8, 3, 0),
            },
            GroundcoverPrototypeLod {
                index: 1,
                name: "LOD1".to_string(),
                mesh: build_rock_mesh(layer, 6, 2, 1),
            },
            GroundcoverPrototypeLod {
                index: 2,
                name: "LOD2".to_string(),
                mesh: build_rock_mesh(layer, 4, 1, 2),
            },
        ],
    }
}

fn build_rock_mesh(
    layer: &GroundcoverLayer,
    radial_segments: u32,
    rings: u32,
    lod_index: u32,
) -> Mesh {
    let mut rng = Rng::from_seed(0x726f_636b_0000u64 ^ ((lod_index as u64 + 1) << 8));
    let mut mesh = Mesh::new();
    let radial_segments = radial_segments.max(3);
    let rings = rings.max(1);
    let radius_x = layer.width.max(0.08);
    let radius_z = radius_x * 0.72;
    let height = layer.height.max(0.05);
    let radial_scales: Vec<f32> = (0..radial_segments)
        .map(|_| rng.range(0.78, 1.18))
        .collect();
    let mut ring_indices: Vec<Vec<u32>> = Vec::new();

    for ring in 0..rings {
        let t = ring as f32 / rings as f32;
        let theta = t * std::f32::consts::FRAC_PI_2;
        let ring_radius = theta.cos().max(0.05);
        let y = theta.sin() * height;
        let mut indices = Vec::with_capacity(radial_segments as usize);
        for segment in 0..radial_segments {
            let angle = (segment as f32 / radial_segments as f32) * crate::constants::TAU;
            let scale = radial_scales[segment as usize];
            let position = Vec3::new(
                angle.cos() * radius_x * ring_radius * scale,
                y * rng.range(0.92, 1.08),
                angle.sin() * radius_z * ring_radius * scale,
            );
            indices.push(mesh.vertices.len() as u32);
            mesh.vertices.push(Vertex {
                position,
                normal: Vec3::Y,
                uv: Vec2::new(segment as f32 / radial_segments as f32, t),
                uv2: Vec2::new(lod_index as f32, scale),
                color: Vec4::new(height, scale, t, 1.0),
            });
        }
        ring_indices.push(indices);
    }

    let top_index = mesh.vertices.len() as u32;
    mesh.vertices.push(Vertex {
        position: Vec3::new(0.0, height, 0.0),
        normal: Vec3::Y,
        uv: Vec2::new(0.5, 1.0),
        uv2: Vec2::new(lod_index as f32, 1.0),
        color: Vec4::new(height, 1.0, 1.0, 1.0),
    });

    let bottom_index = mesh.vertices.len() as u32;
    mesh.vertices.push(Vertex {
        position: Vec3::ZERO,
        normal: -Vec3::Y,
        uv: Vec2::new(0.5, 0.5),
        uv2: Vec2::new(lod_index as f32, 0.0),
        color: Vec4::new(height, 1.0, 0.0, 1.0),
    });

    for ring in 0..rings.saturating_sub(1) {
        let current = &ring_indices[ring as usize];
        let next = &ring_indices[(ring + 1) as usize];
        for segment in 0..radial_segments {
            let a = current[segment as usize];
            let b = current[((segment + 1) % radial_segments) as usize];
            let c = next[((segment + 1) % radial_segments) as usize];
            let d = next[segment as usize];
            mesh.indices.extend_from_slice(&[a, b, c, a, c, d]);
        }
    }

    let top_ring = &ring_indices[(rings - 1) as usize];
    let bottom_ring = &ring_indices[0];
    for segment in 0..radial_segments {
        let next = ((segment + 1) % radial_segments) as usize;
        let current = segment as usize;
        mesh.indices
            .extend_from_slice(&[top_ring[current], top_ring[next], top_index]);
        mesh.indices
            .extend_from_slice(&[bottom_index, bottom_ring[next], bottom_ring[current]]);
    }

    mesh.submeshes.push(Submesh {
        index_start: 0,
        index_count: mesh.indices.len() as u32,
        material: MaterialType::Bark,
    });
    mesh.recalculate_normals();
    mesh
}

fn generate_log_prototype(layer: &GroundcoverLayer) -> GroundcoverPrototype {
    GroundcoverPrototype {
        name: if layer.name.is_empty() {
            "log".to_string()
        } else {
            layer.name.clone()
        },
        kind: GroundcoverKind::Log,
        lods: vec![
            GroundcoverPrototypeLod {
                index: 0,
                name: "LOD0".to_string(),
                mesh: build_log_cylinder_mesh(layer, 8, 2, 0),
            },
            GroundcoverPrototypeLod {
                index: 1,
                name: "LOD1".to_string(),
                mesh: build_log_cylinder_mesh(layer, 6, 1, 1),
            },
            GroundcoverPrototypeLod {
                index: 2,
                name: "LOD2".to_string(),
                mesh: build_log_card_mesh(layer),
            },
        ],
    }
}

fn build_log_cylinder_mesh(
    layer: &GroundcoverLayer,
    radial_segments: u32,
    length_segments: u32,
    lod_index: u32,
) -> Mesh {
    let mut rng = Rng::from_seed(0x6c6f_6700_0000u64 ^ ((lod_index as u64 + 1) << 8));
    let mut mesh = Mesh::new();
    let radial_segments = radial_segments.max(4);
    let length_segments = length_segments.max(1);
    let length = layer.height.max(0.25);
    let radius = layer.width.max(0.04);
    let mut ring_indices: Vec<Vec<u32>> = Vec::new();

    for x_segment in 0..=length_segments {
        let t = x_segment as f32 / length_segments as f32;
        let x = lerp(-length * 0.5, length * 0.5, t);
        let radius_scale = rng.range(0.88, 1.12);
        let mut indices = Vec::with_capacity(radial_segments as usize);
        for segment in 0..radial_segments {
            let angle = (segment as f32 / radial_segments as f32) * crate::constants::TAU;
            let y = radius + angle.sin() * radius * 0.86 * radius_scale;
            let z = angle.cos() * radius * radius_scale;
            indices.push(mesh.vertices.len() as u32);
            mesh.vertices.push(Vertex {
                position: Vec3::new(x, y, z),
                normal: Vec3::new(0.0, angle.sin(), angle.cos()).normalize_or_zero(),
                uv: Vec2::new(t, segment as f32 / radial_segments as f32),
                uv2: Vec2::new(lod_index as f32, radius_scale),
                color: Vec4::new(length, radius_scale, t, 1.0),
            });
        }
        ring_indices.push(indices);
    }

    for x_segment in 0..length_segments {
        let current = &ring_indices[x_segment as usize];
        let next = &ring_indices[(x_segment + 1) as usize];
        for segment in 0..radial_segments {
            let a = current[segment as usize];
            let b = current[((segment + 1) % radial_segments) as usize];
            let c = next[((segment + 1) % radial_segments) as usize];
            let d = next[segment as usize];
            mesh.indices.extend_from_slice(&[a, d, c, a, c, b]);
        }
    }

    for (x_segment, normal) in [(0usize, -Vec3::X), (length_segments as usize, Vec3::X)] {
        let center_index = mesh.vertices.len() as u32;
        let x = if x_segment == 0 {
            -length * 0.5
        } else {
            length * 0.5
        };
        mesh.vertices.push(Vertex {
            position: Vec3::new(x, radius, 0.0),
            normal,
            uv: Vec2::new(0.5, 0.5),
            uv2: Vec2::new(lod_index as f32, 1.0),
            color: Vec4::new(length, 1.0, 0.0, 1.0),
        });
        let ring = &ring_indices[x_segment];
        for segment in 0..radial_segments {
            let current = ring[segment as usize];
            let next = ring[((segment + 1) % radial_segments) as usize];
            if x_segment == 0 {
                mesh.indices
                    .extend_from_slice(&[center_index, current, next]);
            } else {
                mesh.indices
                    .extend_from_slice(&[center_index, next, current]);
            }
        }
    }

    mesh.submeshes.push(Submesh {
        index_start: 0,
        index_count: mesh.indices.len() as u32,
        material: MaterialType::Bark,
    });
    mesh.recalculate_normals();
    mesh
}

fn build_log_card_mesh(layer: &GroundcoverLayer) -> Mesh {
    let mut mesh = Mesh::new();
    let length = layer.height.max(0.25);
    let radius = layer.width.max(0.04);
    append_moss_card(&mut mesh, Vec3::ZERO, 0.0, length, radius * 2.0, 0.0);
    append_litter_card(
        &mut mesh,
        Vec3::new(0.0, radius * 1.8, 0.0),
        0.0,
        length,
        radius * 2.2,
        0.0,
        0.0,
    );
    mesh.submeshes.push(Submesh {
        index_start: 0,
        index_count: mesh.indices.len() as u32,
        material: MaterialType::Bark,
    });
    mesh.recalculate_normals();
    mesh
}

fn encode_terrain_normal(normal: Vec3, flip_green: bool) -> [u8; 3] {
    let tangent_space = Vec3::new(normal.x, normal.z, normal.y).normalize_or_zero();
    let green = if flip_green {
        -tangent_space.y
    } else {
        tangent_space.y
    };
    [
        to_u8(tangent_space.x * 0.5 + 0.5),
        to_u8(green * 0.5 + 0.5),
        to_u8(tangent_space.z * 0.5 + 0.5),
    ]
}

fn to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn flatten_rgb(values: &[[u8; 3]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 3);
    for value in values {
        out.extend_from_slice(value);
    }
    out
}

fn flatten_rgba(values: &[[u8; 4]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 4);
    for value in values {
        out.extend_from_slice(value);
    }
    out
}

fn fnv1a(hash: u64, byte: u8) -> u64 {
    (hash ^ byte as u64).wrapping_mul(0x100000001b3)
}

fn default_units() -> String {
    "meters".to_string()
}
fn default_patch_size() -> f32 {
    16.0
}
fn default_soil_profile() -> String {
    "loam".to_string()
}
fn default_mound_scale() -> f32 {
    0.12
}
fn default_mound_height() -> f32 {
    0.55
}
fn default_one() -> f32 {
    1.0
}
fn default_patch_softness() -> f32 {
    0.15
}
fn default_relief_scale() -> f32 {
    0.7
}
fn default_relief_strength() -> f32 {
    0.6
}
fn default_wetness_scale() -> f32 {
    0.18
}
fn default_wetness_edge() -> f32 {
    0.12
}
fn default_crack_amount() -> f32 {
    0.75
}
fn default_crack_plate_density() -> f32 {
    0.9
}
fn default_crack_width() -> f32 {
    0.06
}
fn default_crack_depth() -> f32 {
    0.7
}
fn default_groundcover_density() -> f32 {
    0.13
}
fn default_groundcover_coverage() -> f32 {
    0.62
}
fn default_groundcover_patch_scale() -> f32 {
    0.15
}
fn default_groundcover_patch_softness() -> f32 {
    0.251
}
fn default_groundcover_height() -> f32 {
    1.5
}
fn default_groundcover_width() -> f32 {
    0.049
}
fn default_moss_relief_scale() -> f32 {
    0.9
}
fn default_moss_relief_strength() -> f32 {
    0.7
}
fn default_grass_base_color() -> String {
    "#33421b".to_string()
}
fn default_grass_tip_color() -> String {
    "#9bc24a".to_string()
}
fn default_wind_strength() -> f32 {
    0.5
}
fn default_wind_speed() -> f32 {
    1.8
}
fn default_gust_scale() -> f32 {
    0.35
}
fn default_flutter() -> f32 {
    0.6
}

fn default_mobile_profile() -> NatureProfile {
    NatureProfile {
        tile_size: 16.0,
        terrain_resolution: 64,
        lod0_max_triangles: 120,
        lod1_max_triangles: 40,
        lod2_max_triangles: 8,
        lod0_max_distance: 6.0,
        lod1_max_distance: 18.0,
        lod2_max_distance: 32.0,
        material_slots: 1,
        max_instances_per_tile: 512,
        max_instances_per_chunk: 96,
        density_scale: 0.65,
        cull_start: 18.0,
        cull_end: 32.0,
        shadows: false,
        grass_collision: false,
        moss_collision: false,
    }
}

fn default_console_profile() -> NatureProfile {
    NatureProfile {
        tile_size: 32.0,
        terrain_resolution: 128,
        lod0_max_triangles: 300,
        lod1_max_triangles: 100,
        lod2_max_triangles: 16,
        lod0_max_distance: 12.0,
        lod1_max_distance: 35.0,
        lod2_max_distance: 70.0,
        material_slots: 1,
        max_instances_per_tile: 2048,
        max_instances_per_chunk: 256,
        density_scale: 1.0,
        cull_start: 35.0,
        cull_end: 70.0,
        shadows: false,
        grass_collision: false,
        moss_collision: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const PATCH_TOML: &str = r##"
[asset]
kind = "nature_patch"
name = "Temperate Forest Floor"

[patch]
size = 16.0
seed = 42
biome = "forest_floor"
tags = ["temperate", "grass", "moss", "shrub", "rock", "log", "damp_soil"]

[soil]
profile = "loam"
mound_scale = 0.12
mound_height = 0.55
mound_coverage = 1.0
relief_scale = 0.7
relief_strength = 0.6
wetness_coverage = 0.35

[soil.cracks]
enabled = true
amount = 0.75
plate_density = 0.9
channel_width = 0.06
warp = 0.2
depth = 0.7

[[groundcover.layers]]
kind = "grass"
name = "fine meadow grass"
density = 0.13
coverage = 0.62
patch_scale = 0.15
patch_softness = 0.251
height = 1.5
width = 0.049
curl = 1.14
color_base = "#33421b"
color_tip = "#9bc24a"

[[groundcover.layers]]
kind = "moss"
coverage = 0.55
patch_scale = 0.14
height = 0.14
relief_scale = 0.9
relief_strength = 0.7

[[groundcover.layers]]
kind = "flower"
name = "woodland violet"
density = 0.14
coverage = 0.35
patch_scale = 0.21
patch_softness = 0.28
height = 0.28
width = 0.045
curl = 0.1

[[groundcover.layers]]
kind = "weed"
name = "broadleaf weed"
density = 0.14
coverage = 0.45
patch_scale = 0.17
patch_softness = 0.24
height = 0.45
width = 0.06
curl = 0.45

[[groundcover.layers]]
kind = "litter"
name = "fallen leaf litter"
density = 0.06
coverage = 0.38
patch_scale = 0.1
patch_softness = 0.2
height = 0.18
width = 0.09

[[groundcover.layers]]
kind = "shrub"
name = "young hazel shrub"
density = 0.055
coverage = 0.3
patch_scale = 0.09
patch_softness = 0.25
height = 0.9
width = 0.16
curl = 0.35
color_base = "#263a1f"
color_tip = "#6f9a4b"

[[groundcover.layers]]
kind = "rock"
name = "mossy field stones"
density = 0.08
coverage = 0.28
patch_scale = 0.08
patch_softness = 0.22
height = 0.22
width = 0.32
color_base = "#4f5148"
color_tip = "#8a8d7d"

[[groundcover.layers]]
kind = "log"
name = "fallen branch log"
density = 0.065
coverage = 0.24
patch_scale = 0.07
patch_softness = 0.2
height = 1.2
width = 0.12
curl = 0.0
color_base = "#4a2f1d"
color_tip = "#8a5b34"

[wind]
strength = 0.5
speed = 1.8
direction_degrees = 20
gust_scale = 0.35
flutter = 0.6
"##;

    fn assert_mobile_lod_budget(patch: &NaturePatch, lod: &GroundcoverPrototypeLod) {
        let budget = lod_triangle_budget(&patch.profiles.mobile, lod.index);
        assert!(
            lod.mesh.triangle_count() as u32 <= budget,
            "LOD{} has {} triangles but mobile budget is {budget}",
            lod.index,
            lod.mesh.triangle_count()
        );
    }

    #[test]
    fn parse_representative_nature_patch() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        assert_eq!(patch.asset.kind, "nature_patch");
        assert_eq!(patch.asset.name, "Temperate Forest Floor");
        assert_eq!(patch.patch.size, 16.0);
        assert_eq!(patch.patch.seed, 42);
        assert_eq!(patch.groundcover.layers.len(), 8);
        assert_eq!(patch.groundcover.layers[0].kind, GroundcoverKind::Grass);
        assert_eq!(patch.groundcover.layers[1].kind, GroundcoverKind::Moss);
        assert_eq!(patch.groundcover.layers[2].kind, GroundcoverKind::Flower);
        assert_eq!(patch.groundcover.layers[3].kind, GroundcoverKind::Weed);
        assert_eq!(patch.groundcover.layers[4].kind, GroundcoverKind::Litter);
        assert_eq!(patch.groundcover.layers[5].kind, GroundcoverKind::Shrub);
        assert_eq!(patch.groundcover.layers[6].kind, GroundcoverKind::Rock);
        assert_eq!(patch.groundcover.layers[7].kind, GroundcoverKind::Log);
        assert_eq!(patch.profiles.mobile.material_slots, 1);
        assert_eq!(patch.profiles.mobile.lod0_max_distance, 6.0);
        assert_eq!(patch.profiles.mobile.lod1_max_distance, 18.0);
        assert_eq!(patch.profiles.mobile.lod2_max_distance, 32.0);
        assert!(!patch.profiles.mobile.grass_collision);
        assert!(!patch.profiles.mobile.moss_collision);
        assert_eq!(patch.profiles.console.terrain_resolution, 128);
        assert_eq!(patch.profiles.console.lod0_max_distance, 12.0);
        assert_eq!(patch.profiles.console.lod1_max_distance, 35.0);
        assert_eq!(patch.profiles.console.lod2_max_distance, 70.0);
        assert!(!patch.profiles.console.grass_collision);
        assert!(!patch.profiles.console.moss_collision);
    }

    #[test]
    fn invalid_nature_patch_kind_fails() {
        let err = NaturePatch::from_toml(
            r#"
[asset]
kind = "tree"
name = "Wrong"

[patch]
size = 16.0
"#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("asset.kind"));
    }

    #[test]
    fn negative_patch_size_fails() {
        let err = NaturePatch::from_toml(
            r#"
[asset]
kind = "nature_patch"
name = "Bad"

[patch]
size = -1.0
"#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("patch.size"));
    }

    #[test]
    fn unordered_profile_lod_distances_fail() {
        let err = NaturePatch::from_toml(
            r#"
[asset]
kind = "nature_patch"
name = "Bad"

[patch]
size = 16.0

[profiles.mobile]
tile_size = 16.0
terrain_resolution = 64
lod0_max_triangles = 120
lod1_max_triangles = 40
lod2_max_triangles = 8
lod0_max_distance = 20.0
lod1_max_distance = 10.0
lod2_max_distance = 32.0
material_slots = 1
max_instances_per_tile = 512
max_instances_per_chunk = 96
density_scale = 0.65
cull_start = 18.0
cull_end = 32.0
shadows = false
"#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("LOD distances"), "{err}");
    }

    #[test]
    fn unknown_groundcover_kind_fails_at_parse() {
        let err = NaturePatch::from_toml(
            r#"
[asset]
kind = "nature_patch"
name = "Bad"

[patch]
size = 16.0

[[groundcover.layers]]
kind = "laser_grass"
"#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("laser_grass"));
    }

    #[test]
    fn terrain_field_is_deterministic_and_range_checked() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let field = patch.terrain_field();
        let a = field.sample(1.25, -2.5);
        let b = field.sample(1.25, -2.5);

        assert_eq!(a, b);
        assert!(a.height.is_finite());
        assert!((a.normal.length() - 1.0).abs() < 0.001);
        assert!((0.0..=1.0).contains(&a.masks.grass_density));
        assert!((0.0..=1.0).contains(&a.masks.moss));
        assert!((0.0..=1.0).contains(&a.masks.wetness));
        assert!((0.0..=1.0).contains(&a.masks.crack));
    }

    #[test]
    fn baked_maps_have_stable_size_and_checksum() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let maps = patch.terrain_field().bake_maps(16);
        assert_eq!(maps.height_u16.len(), 256);
        assert_eq!(maps.normal_yplus.len(), 256);
        assert_eq!(maps.normal_yminus.len(), 256);
        assert_eq!(maps.masks_rgba.len(), 256);
        assert_eq!(maps.grass_density.len(), 256);
        assert!(maps.max_height > maps.min_height);
        assert_eq!(maps.checksum(), 0xb2fe875410604e0b);
    }

    #[test]
    fn writes_canonical_maps() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let dir = tempdir().unwrap();
        patch.terrain_field().write_maps(dir.path(), 8).unwrap();

        for name in [
            "height_u16.png",
            "normal_yplus.png",
            "normal_yminus.png",
            "masks_rgba.png",
            "grass_density.png",
        ] {
            assert!(dir.path().join(name).exists(), "{name} should be written");
        }
    }

    #[test]
    fn preview_mesh_uses_same_field() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let field = patch.terrain_field();
        let mesh = field.build_preview_mesh(8);

        assert_eq!(mesh.vertex_count(), 64);
        assert_eq!(mesh.triangle_count(), 98);
        assert_eq!(mesh.submeshes.len(), 1);
        assert!(!mesh.is_empty());
        for vertex in &mesh.vertices {
            assert!(vertex.position.y.is_finite());
            assert!((vertex.normal.length() - 1.0).abs() < 0.001);
            assert!((0.0..=1.0).contains(&vertex.color.x));
            assert!((0.0..=1.0).contains(&vertex.color.y));
            assert!((0.0..=1.0).contains(&vertex.color.z));
            assert!((0.0..=1.0).contains(&vertex.color.w));
        }
    }

    #[test]
    fn generates_groundcover_prototypes_with_mobile_budgets() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let prototypes = patch.generate_groundcover_prototypes();
        assert_eq!(prototypes.len(), 8);

        let grass = prototypes
            .iter()
            .find(|prototype| prototype.kind == GroundcoverKind::Grass)
            .unwrap();
        assert_eq!(grass.lods.len(), 3);
        assert!(
            grass.lods[0].mesh.triangle_count() > grass.lods[1].mesh.triangle_count(),
            "grass LODs should reduce triangle cost"
        );
        assert!(
            grass.lods[1].mesh.triangle_count() > grass.lods[2].mesh.triangle_count(),
            "grass LODs should reduce triangle cost"
        );

        for lod in &grass.lods {
            assert!(!lod.mesh.is_empty());
            assert_eq!(lod.mesh.submeshes[0].material, MaterialType::Leaves);
            assert_mobile_lod_budget(&patch, lod);
            for vertex in &lod.mesh.vertices {
                assert!(vertex.position.x.is_finite());
                assert!(vertex.position.y.is_finite());
                assert!(vertex.position.z.is_finite());
                assert!(vertex.normal.x.is_finite());
                assert!(vertex.normal.y.is_finite());
                assert!(vertex.normal.z.is_finite());
                assert!(vertex.color.x.is_finite());
                assert!(vertex.color.y.is_finite());
                assert!(vertex.color.z.is_finite());
                assert!(vertex.color.w.is_finite());
            }
        }

        let moss = prototypes
            .iter()
            .find(|prototype| prototype.kind == GroundcoverKind::Moss)
            .unwrap();
        assert_eq!(moss.lods.len(), 2);
        assert!(
            moss.lods[0].mesh.triangle_count() > moss.lods[1].mesh.triangle_count(),
            "moss LODs should reduce triangle cost"
        );
        for lod in &moss.lods {
            assert!(!lod.mesh.is_empty());
            assert_mobile_lod_budget(&patch, lod);
        }

        for (kind, expected_lods) in [
            (GroundcoverKind::Flower, 3usize),
            (GroundcoverKind::Weed, 3usize),
            (GroundcoverKind::Litter, 2usize),
            (GroundcoverKind::Shrub, 3usize),
            (GroundcoverKind::Rock, 3usize),
            (GroundcoverKind::Log, 3usize),
        ] {
            let prototype = prototypes
                .iter()
                .find(|prototype| prototype.kind == kind)
                .unwrap_or_else(|| panic!("missing {kind:?} prototype"));
            assert_eq!(prototype.lods.len(), expected_lods);
            assert!(
                prototype.lods[0].mesh.triangle_count() > prototype.lods[1].mesh.triangle_count(),
                "{kind:?} LODs should reduce triangle cost"
            );
            for lod in &prototype.lods {
                assert!(!lod.mesh.is_empty());
                let expected_material = match kind {
                    GroundcoverKind::Rock | GroundcoverKind::Log => MaterialType::Bark,
                    _ => MaterialType::Leaves,
                };
                assert_eq!(lod.mesh.submeshes[0].material, expected_material);
                assert_mobile_lod_budget(&patch, lod);
                for vertex in &lod.mesh.vertices {
                    assert!(vertex.position.x.is_finite());
                    assert!(vertex.position.y.is_finite());
                    assert!(vertex.position.z.is_finite());
                    assert!(vertex.normal.x.is_finite());
                    assert!(vertex.normal.y.is_finite());
                    assert!(vertex.normal.z.is_finite());
                    assert!(vertex.color.x.is_finite());
                    assert!(vertex.color.y.is_finite());
                    assert!(vertex.color.z.is_finite());
                    assert!(vertex.color.w.is_finite());
                }
            }
        }
    }

    #[test]
    fn scatter_sets_are_deterministic_and_chunked() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let a = patch.generate_scatter_sets(8.0);
        let b = patch.generate_scatter_sets(8.0);

        assert_eq!(a, b);
        assert_eq!(a.len(), 8);
        let total_instances: usize = a
            .iter()
            .flat_map(|set| &set.chunks)
            .map(|chunk| chunk.instances.len())
            .sum();
        assert!(total_instances > 0);

        for set in a {
            assert!(set.layer_index < patch.groundcover.layers.len());
            let set_instances: usize = set.chunks.iter().map(|chunk| chunk.instances.len()).sum();
            assert!(
                set_instances > 0,
                "{:?} should produce representative scatter instances",
                set.kind
            );
            for chunk in set.chunks {
                assert!(!chunk.instances.is_empty());
                assert!(chunk.bounds_min[0] <= chunk.bounds_max[0]);
                assert!(chunk.bounds_min[1] <= chunk.bounds_max[1]);
                assert!(chunk.bounds_min[2] <= chunk.bounds_max[2]);

                for instance in chunk.instances {
                    assert!(instance.position[0].is_finite());
                    assert!(instance.position[1].is_finite());
                    assert!(instance.position[2].is_finite());
                    assert!(instance.yaw >= 0.0);
                    assert!(instance.yaw <= crate::constants::TAU);
                    assert!(instance.height > 0.0);
                    assert!(instance.width > 0.0);
                    assert!((0.0..=1.0).contains(&instance.color_variation));
                }
            }
        }
    }

    #[test]
    fn workspace_nature_presets_export_and_validate() {
        let presets = [
            (
                "temperate_forest_floor",
                include_str!("../../../presets/nature/temperate_forest_floor.toml"),
            ),
            (
                "flowering_meadow",
                include_str!("../../../presets/nature/flowering_meadow.toml"),
            ),
            (
                "arid_scrub",
                include_str!("../../../presets/nature/arid_scrub.toml"),
            ),
        ];
        let config = NaturePackageConfig {
            map_resolution: 8,
            preview_resolution: 8,
            scatter_chunk_size: 8.0,
            ..NaturePackageConfig::default()
        };
        let dir = tempdir().unwrap();

        for (name, toml) in presets {
            let patch = NaturePatch::from_toml(toml).unwrap();
            assert!(
                patch.groundcover.layers.len() >= 5,
                "{name} should exercise multiple nature families"
            );
            assert_eq!(
                patch.generate_groundcover_prototypes().len(),
                patch.groundcover.layers.len(),
                "{name} should generate one prototype family for every layer"
            );

            let scatter_sets = patch.generate_scatter_sets(config.scatter_chunk_size);
            assert_eq!(scatter_sets.len(), patch.groundcover.layers.len());
            for set in &scatter_sets {
                let instances: usize = set.chunks.iter().map(|chunk| chunk.instances.len()).sum();
                assert!(
                    instances > 0,
                    "{name} {:?} scatter should be nonzero",
                    set.kind
                );
            }

            let package_dir = dir.path().join(name);
            let summary = patch.write_package(&package_dir, &config).unwrap();
            let report = validate_nature_package(&package_dir).unwrap();
            assert_eq!(report.manifest_path, summary.manifest_path);
            assert_eq!(
                report.manifest.groundcover_layers.len(),
                patch.groundcover.layers.len()
            );
            assert_eq!(
                report.scatter_json_instances,
                summary.scatter_instance_count
            );
            assert_eq!(
                report.scatter_binary_instances,
                summary.scatter_instance_count
            );
            assert_eq!(report.map_summaries.len(), 5);
            assert_eq!(
                report.prototype_summaries.len(),
                report.prototype_files.len()
            );
        }
    }

    fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn writes_initial_nature_package_layout() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let dir = tempdir().unwrap();
        let config = NaturePackageConfig {
            map_resolution: 8,
            preview_resolution: 8,
            scatter_chunk_size: 8.0,
            ..NaturePackageConfig::default()
        };

        let summary = patch.write_package(dir.path(), &config).unwrap();

        assert!(summary.manifest_path.exists());
        assert!(summary.maps_directory.join("height_u16.png").exists());
        assert!(summary.maps_directory.join("normal_yplus.png").exists());
        assert!(summary.maps_directory.join("normal_yminus.png").exists());
        assert!(summary.maps_directory.join("masks_rgba.png").exists());
        assert!(summary.maps_directory.join("grass_density.png").exists());
        assert!(summary.preview_mesh_path.unwrap().exists());
        assert_eq!(summary.material_recipe_paths.len(), 2);
        for path in &summary.material_recipe_paths {
            assert!(path.exists(), "{} should be written", path.display());
        }
        assert_eq!(summary.engine_import_recipe_paths.len(), 2);
        for path in &summary.engine_import_recipe_paths {
            assert!(path.exists(), "{} should be written", path.display());
        }
        assert_eq!(summary.prototype_meshes.len(), 22);
        for path in &summary.prototype_meshes {
            assert!(path.exists(), "{} should be written", path.display());
        }
        assert!(summary.scatter_instance_count > 0);
        let scatter_path = summary.scatter_path.unwrap();
        assert!(scatter_path.exists());
        assert!(!summary.scatter_binary_paths.is_empty());

        let manifest: NatureExportManifest = serde_json::from_reader(
            std::fs::File::open(dir.path().join("midori_nature.json")).unwrap(),
        )
        .unwrap();
        manifest.validate().unwrap();
        assert_eq!(manifest.schema_version, 3);
        assert_eq!(manifest.map_resolution, 8);
        assert_eq!(manifest.terrain.heightmap_file, "maps/height_u16.png");
        assert_eq!(manifest.terrain.masks_file, "maps/masks_rgba.png");
        assert_eq!(
            manifest.terrain.grass_density_file,
            "maps/grass_density.png"
        );
        assert!(manifest.terrain.height_min <= manifest.terrain.height_max);
        assert!(manifest.terrain.bounds_min[0] < manifest.terrain.bounds_max[0]);
        assert!(manifest.terrain.bounds_min[2] < manifest.terrain.bounds_max[2]);
        assert_eq!(manifest.shader_policy, "preview_only");
        assert_eq!(manifest.texture_pipeline, "parked");
        assert_eq!(manifest.material_recipes.len(), 2);
        assert!(
            manifest
                .material_recipes
                .iter()
                .any(|recipe| recipe.file == "materials/terrain_surface.recipe.json")
        );
        assert!(
            manifest
                .material_recipes
                .iter()
                .any(|recipe| recipe.file == "materials/groundcover_foliage.recipe.json")
        );
        assert_eq!(manifest.engine_import_recipes.len(), 2);
        assert!(
            manifest
                .engine_import_recipes
                .iter()
                .any(|recipe| recipe.file == "engines/unity_import.recipe.json"
                    && recipe.profile == "mobile")
        );
        assert!(
            manifest
                .engine_import_recipes
                .iter()
                .any(|recipe| recipe.file == "engines/unreal_import.recipe.json"
                    && recipe.profile == "console")
        );
        assert_eq!(manifest.prototypes.len(), 8);
        assert!(manifest.scatter.enabled);
        assert_eq!(
            manifest.scatter.file.as_deref(),
            Some("instances/scatter.json")
        );
        assert_eq!(
            manifest.scatter.binary_format.format,
            "midori.scatter.bin.v1"
        );
        assert_eq!(
            manifest.scatter.binary_format.record_stride_bytes,
            SCATTER_BINARY_RECORD_STRIDE_BYTES
        );
        assert_eq!(
            manifest.scatter.binary_files.len(),
            summary.scatter_binary_paths.len()
        );
        assert_eq!(manifest.memory_footprint.map_pixel_count, 64);
        assert_eq!(manifest.memory_footprint.decoded_map_bytes, 64 * 13);
        assert!(manifest.memory_footprint.encoded_map_bytes > 0);
        assert!(manifest.memory_footprint.material_recipe_bytes > 0);
        assert!(manifest.memory_footprint.engine_import_recipe_bytes > 0);
        assert!(manifest.memory_footprint.preview_mesh_bytes > 0);
        assert!(manifest.memory_footprint.prototype_mesh_bytes > 0);
        assert!(manifest.memory_footprint.scatter_json_bytes > 0);
        assert_eq!(
            manifest.memory_footprint.scatter_binary_header_bytes,
            manifest.scatter.binary_files.len() as u64 * SCATTER_BINARY_HEADER_BYTES as u64
        );
        assert_eq!(
            manifest.memory_footprint.scatter_binary_record_bytes,
            summary.scatter_instance_count as u64 * SCATTER_BINARY_RECORD_STRIDE_BYTES as u64
        );
        assert_eq!(
            manifest.memory_footprint.scatter_binary_bytes,
            manifest.memory_footprint.scatter_binary_header_bytes
                + manifest.memory_footprint.scatter_binary_record_bytes
        );
        assert_eq!(
            manifest.memory_footprint.total_payload_bytes,
            manifest.memory_footprint.encoded_map_bytes
                + manifest.memory_footprint.material_recipe_bytes
                + manifest.memory_footprint.engine_import_recipe_bytes
                + manifest.memory_footprint.preview_mesh_bytes
                + manifest.memory_footprint.prototype_mesh_bytes
                + manifest.memory_footprint.scatter_json_bytes
                + manifest.memory_footprint.scatter_binary_bytes
        );
        let first_binary = &manifest.scatter.binary_files[0];
        let first_binary_path = dir.path().join(&first_binary.file);
        assert!(first_binary_path.exists());
        let binary = std::fs::read(first_binary_path).unwrap();
        assert_eq!(&binary[0..4], SCATTER_BINARY_MAGIC);
        assert_eq!(read_u32_le(&binary, 4), SCATTER_BINARY_VERSION);
        assert_eq!(read_u32_le(&binary, 8), SCATTER_BINARY_RECORD_STRIDE_BYTES);
        assert_eq!(read_u32_le(&binary, 12), first_binary.instance_count as u32);
        assert_eq!(
            binary.len(),
            SCATTER_BINARY_HEADER_BYTES as usize
                + first_binary.instance_count * SCATTER_BINARY_RECORD_STRIDE_BYTES as usize
        );
        assert_eq!(manifest.unity.normal_map, "maps/normal_yplus.png");
        assert_eq!(manifest.unreal.normal_map, "maps/normal_yminus.png");

        let scatter_sets: Vec<ScatterSet> =
            serde_json::from_reader(std::fs::File::open(scatter_path).unwrap()).unwrap();
        assert_eq!(scatter_sets.len(), 8);
    }

    #[test]
    fn validates_written_nature_package_conformance() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let dir = tempdir().unwrap();
        let config = NaturePackageConfig {
            map_resolution: 8,
            preview_resolution: 8,
            scatter_chunk_size: 8.0,
            ..NaturePackageConfig::default()
        };

        let summary = patch.write_package(dir.path(), &config).unwrap();
        let report = validate_nature_package(dir.path()).unwrap();

        assert_eq!(report.manifest.asset_name, "Temperate Forest Floor");
        assert_eq!(report.manifest.schema_version, 3);
        assert_eq!(report.manifest_path, summary.manifest_path);
        assert_eq!(report.memory_footprint, report.manifest.memory_footprint);
        assert_eq!(report.memory_footprint.map_pixel_count, 64);
        assert_eq!(report.memory_footprint.decoded_map_bytes, 64 * 13);
        assert!(report.memory_footprint.material_recipe_bytes > 0);
        assert!(report.memory_footprint.engine_import_recipe_bytes > 0);
        assert!(report.memory_footprint.total_payload_bytes > 0);
        assert_eq!(report.map_files.len(), 5);
        assert_eq!(report.map_summaries.len(), 5);
        assert_eq!(report.map_relationships.normal_pair_pixels, 64);
        assert_eq!(report.map_relationships.normal_red_blue_mismatches, 0);
        assert_eq!(report.map_relationships.normal_green_flip_mismatches, 0);
        assert!(report.map_relationships.normal_green_flip_max_error <= 1);
        assert_eq!(report.map_relationships.grass_density_pixels, 64);
        assert_eq!(report.map_relationships.grass_density_mask_r_mismatches, 0);
        let find_map = |name: &str| {
            report
                .map_summaries
                .iter()
                .find(|map| map.file.file_name().and_then(|file| file.to_str()) == Some(name))
                .unwrap_or_else(|| panic!("missing map summary for {name}"))
        };
        let height = find_map("height_u16.png");
        assert_eq!(height.width, 8);
        assert_eq!(height.height, 8);
        assert_eq!(height.color_type, "L16");
        assert_eq!(height.channels, 1);
        assert!(height.channel_max[0] > height.channel_min[0]);

        let normal = find_map("normal_yplus.png");
        assert_eq!(normal.color_type, "RGB8");
        assert_eq!(normal.channels, 3);
        assert!(
            normal
                .channel_min
                .iter()
                .zip(&normal.channel_max)
                .any(|(min, max)| max > min)
        );

        let masks = find_map("masks_rgba.png");
        assert_eq!(masks.color_type, "RGBA8");
        assert_eq!(masks.channels, 4);
        assert!(masks.channel_max[0] > masks.channel_min[0]);
        assert!(masks.channel_max[1] > masks.channel_min[1]);

        let density = find_map("grass_density.png");
        assert_eq!(density.color_type, "L8");
        assert_eq!(density.channels, 1);
        assert!(density.channel_max[0] > density.channel_min[0]);
        assert_eq!(report.material_recipe_files.len(), 2);
        assert_eq!(report.material_recipe_summaries.len(), 2);
        let terrain_recipe = report
            .material_recipe_summaries
            .iter()
            .find(|recipe| recipe.material_slot == "terrain_surface")
            .expect("terrain material recipe should be summarized");
        assert_eq!(terrain_recipe.runtime_policy, "engine_native_static");
        assert_eq!(terrain_recipe.shader_policy, "preview_only");
        assert_eq!(terrain_recipe.texture_pipeline, "parked");
        assert_eq!(terrain_recipe.parameter_count, 4);
        assert!(
            terrain_recipe
                .required_textures
                .iter()
                .any(|texture| texture == "maps/masks_rgba.png")
        );
        assert!(
            terrain_recipe
                .surface_overlays
                .iter()
                .any(|overlay| overlay == "moss")
        );
        assert_ne!(terrain_recipe.file_checksum, 0);
        let groundcover_recipe = report
            .material_recipe_summaries
            .iter()
            .find(|recipe| recipe.material_slot == "groundcover_foliage")
            .expect("groundcover material recipe should be summarized");
        assert_eq!(groundcover_recipe.parameter_count, 8);
        assert!(
            groundcover_recipe
                .required_vertex_streams
                .iter()
                .any(|stream| stream.starts_with("TEXCOORD_1.x"))
        );
        assert!(
            groundcover_recipe
                .engine_targets
                .iter()
                .any(|target| target == "unreal_static_mesh_foliage_material")
        );
        assert_eq!(report.engine_import_recipe_files.len(), 2);
        assert_eq!(report.engine_import_recipe_summaries.len(), 2);
        let unity_import_recipe = report
            .engine_import_recipe_summaries
            .iter()
            .find(|recipe| recipe.engine == "unity")
            .expect("Unity import recipe should be summarized");
        assert_eq!(unity_import_recipe.profile, "mobile");
        assert_eq!(unity_import_recipe.runtime_policy, "engine_native_static");
        assert_eq!(unity_import_recipe.scatter_instances, 222);
        assert_eq!(unity_import_recipe.scatter_binary_chunks, 26);
        assert!(
            unity_import_recipe
                .material_recipe_files
                .iter()
                .any(|file| file == "materials/groundcover_foliage.recipe.json")
        );
        assert_ne!(unity_import_recipe.file_checksum, 0);
        let unreal_import_recipe = report
            .engine_import_recipe_summaries
            .iter()
            .find(|recipe| recipe.engine == "unreal")
            .expect("Unreal import recipe should be summarized");
        assert_eq!(unreal_import_recipe.profile, "console");
        assert!(
            unreal_import_recipe
                .expected_systems
                .iter()
                .any(|system| system == "Static Mesh Foliage")
        );
        assert_eq!(report.prototype_files.len(), 22);
        assert_eq!(report.prototype_summaries.len(), 22);
        let grass_lod0 = report
            .prototype_summaries
            .iter()
            .find(|prototype| {
                prototype.file.file_name().and_then(|file| file.to_str())
                    == Some("fine_meadow_grass_lod0.glb")
            })
            .expect("fine meadow grass LOD0 should be summarized");
        assert_eq!(grass_lod0.lod_index, 0);
        assert_eq!(grass_lod0.mesh_count, 1);
        assert_eq!(grass_lod0.primitive_count, 1);
        assert_eq!(grass_lod0.used_material_count, 1);
        assert!(grass_lod0.has_positions);
        assert!(grass_lod0.has_normals);
        assert!(grass_lod0.has_tangents);
        assert!(grass_lod0.normals_are_valid);
        assert!(grass_lod0.tangents_are_valid);
        assert!(grass_lod0.has_texcoord0);
        assert!(grass_lod0.has_texcoord1);
        assert!(grass_lod0.has_color0);
        assert!(grass_lod0.vertex_count > 0);
        assert!(grass_lod0.triangle_count > 0);
        assert_ne!(grass_lod0.file_checksum, 0);
        assert_eq!(report.profile_budget_summaries.len(), 2);
        let find_budget = |profile: &str| {
            report
                .profile_budget_summaries
                .iter()
                .find(|summary| summary.profile == profile)
                .unwrap_or_else(|| panic!("missing profile budget summary for {profile}"))
        };
        let mobile_budget = find_budget("mobile");
        assert!(mobile_budget.passed);
        assert_eq!(mobile_budget.prototype_lod_count, 22);
        assert_eq!(mobile_budget.max_lod0_triangles, 48);
        assert_eq!(mobile_budget.max_lod1_triangles, 24);
        assert_eq!(mobile_budget.max_lod2_triangles, 8);
        assert_eq!(mobile_budget.lod0_triangle_budget, 120);
        assert_eq!(mobile_budget.lod1_triangle_budget, 40);
        assert_eq!(mobile_budget.lod2_triangle_budget, 8);
        assert_eq!(mobile_budget.max_used_material_count, 1);
        assert_eq!(mobile_budget.material_slot_budget, 1);
        assert_eq!(mobile_budget.scatter_instance_count, 222);
        assert_eq!(mobile_budget.max_chunk_instance_count, 29);
        assert_eq!(mobile_budget.max_instances_per_tile_budget, 512);
        assert_eq!(mobile_budget.max_instances_per_chunk_budget, 96);
        assert_eq!(mobile_budget.triangle_budget_violation_count, 0);
        assert_eq!(mobile_budget.material_slot_violation_count, 0);
        assert_eq!(mobile_budget.instance_budget_violation_count, 0);
        let console_budget = find_budget("console");
        assert!(console_budget.passed);
        assert_eq!(console_budget.prototype_lod_count, 22);
        assert_eq!(console_budget.max_lod0_triangles, 48);
        assert_eq!(console_budget.max_lod1_triangles, 24);
        assert_eq!(console_budget.max_lod2_triangles, 8);
        assert_eq!(console_budget.lod0_triangle_budget, 300);
        assert_eq!(console_budget.lod1_triangle_budget, 100);
        assert_eq!(console_budget.lod2_triangle_budget, 16);
        assert_eq!(console_budget.max_used_material_count, 1);
        assert_eq!(console_budget.material_slot_budget, 1);
        assert_eq!(console_budget.scatter_instance_count, 222);
        assert_eq!(console_budget.max_chunk_instance_count, 29);
        assert_eq!(console_budget.max_instances_per_tile_budget, 2048);
        assert_eq!(console_budget.max_instances_per_chunk_budget, 256);
        assert_eq!(console_budget.triangle_budget_violation_count, 0);
        assert_eq!(console_budget.material_slot_violation_count, 0);
        assert_eq!(console_budget.instance_budget_violation_count, 0);
        assert_eq!(
            report.scatter_json_path.as_deref(),
            summary.scatter_path.as_deref()
        );
        assert_eq!(
            report.scatter_json_instances,
            summary.scatter_instance_count
        );
        assert_eq!(report.scatter_json_chunks.len(), 26);
        assert_eq!(report.scatter_binary_summaries.len(), 26);
        assert_eq!(report.scatter_parity.json_chunk_count, 26);
        assert_eq!(report.scatter_parity.binary_chunk_count, 26);
        assert_eq!(report.scatter_parity.matching_chunk_count, 26);
        assert_eq!(report.scatter_parity.missing_binary_chunk_count, 0);
        assert_eq!(report.scatter_parity.extra_binary_chunk_count, 0);
        assert_eq!(report.scatter_parity.instance_count_mismatch_count, 0);
        assert_eq!(report.scatter_parity.bounds_mismatch_count, 0);
        assert_eq!(report.scatter_parity.record_checksum_mismatch_count, 0);
        let first_scatter = report.scatter_binary_summaries.first().unwrap();
        assert_eq!(first_scatter.source, "binary");
        assert!(first_scatter.instance_count > 0);
        assert_ne!(first_scatter.record_checksum, 0);
        assert_ne!(first_scatter.file_checksum, 0);
        assert!(first_scatter.height_min > 0.0);
        assert!(first_scatter.width_min > 0.0);
        assert!((0.0..=crate::constants::TAU).contains(&first_scatter.yaw_min));
        assert!((0.0..=crate::constants::TAU).contains(&first_scatter.yaw_max));
        assert!((0.0..=crate::constants::TAU).contains(&first_scatter.phase_min));
        assert!((0.0..=crate::constants::TAU).contains(&first_scatter.phase_max));
        assert!((0.0..=1.0).contains(&first_scatter.color_variation_min));
        assert!((0.0..=1.0).contains(&first_scatter.color_variation_max));
        assert_eq!(
            report.scatter_binary_files.len(),
            summary.scatter_binary_paths.len()
        );
        assert_eq!(
            report.scatter_binary_instances,
            summary.scatter_instance_count
        );
    }

    #[test]
    fn package_validation_rejects_profile_triangle_budget_violation() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let dir = tempdir().unwrap();
        let config = NaturePackageConfig {
            map_resolution: 8,
            preview_resolution: 8,
            scatter_chunk_size: 8.0,
            ..NaturePackageConfig::default()
        };

        patch.write_package(dir.path(), &config).unwrap();
        let manifest_path = dir.path().join("midori_nature.json");
        let mut manifest: serde_json::Value =
            serde_json::from_reader(std::fs::File::open(&manifest_path).unwrap()).unwrap();
        manifest["mobile"]["lod2_max_triangles"] = serde_json::json!(1);
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let err = validate_nature_package(dir.path()).unwrap_err().to_string();
        assert!(err.contains("mobile profile budget exceeded"), "{err}");
        assert!(err.contains("triangle violations"), "{err}");
    }

    #[test]
    fn package_validation_rejects_profile_instance_budget_violation() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let dir = tempdir().unwrap();
        let config = NaturePackageConfig {
            map_resolution: 8,
            preview_resolution: 8,
            scatter_chunk_size: 8.0,
            ..NaturePackageConfig::default()
        };

        patch.write_package(dir.path(), &config).unwrap();
        let manifest_path = dir.path().join("midori_nature.json");
        let mut manifest: serde_json::Value =
            serde_json::from_reader(std::fs::File::open(&manifest_path).unwrap()).unwrap();
        manifest["mobile"]["max_instances_per_tile"] = serde_json::json!(100);
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let unity_recipe_path = dir.path().join("engines/unity_import.recipe.json");
        let mut unity_recipe: serde_json::Value =
            serde_json::from_reader(std::fs::File::open(&unity_recipe_path).unwrap()).unwrap();
        unity_recipe["profile_settings"]["max_instances_per_tile"] = serde_json::json!(100);
        std::fs::write(
            &unity_recipe_path,
            serde_json::to_vec_pretty(&unity_recipe).unwrap(),
        )
        .unwrap();

        let err = validate_nature_package(dir.path()).unwrap_err().to_string();
        assert!(err.contains("mobile profile budget exceeded"), "{err}");
        assert!(err.contains("instance violations"), "{err}");
    }

    #[test]
    fn package_validation_rejects_memory_footprint_file_mismatch() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let dir = tempdir().unwrap();
        let config = NaturePackageConfig {
            map_resolution: 8,
            preview_resolution: 8,
            scatter_chunk_size: 8.0,
            ..NaturePackageConfig::default()
        };

        patch.write_package(dir.path(), &config).unwrap();
        let manifest_path = dir.path().join("midori_nature.json");
        let mut manifest: serde_json::Value =
            serde_json::from_reader(std::fs::File::open(&manifest_path).unwrap()).unwrap();
        manifest["memory_footprint"]["encoded_map_bytes"] = serde_json::json!(
            manifest["memory_footprint"]["encoded_map_bytes"]
                .as_u64()
                .unwrap()
                + 1
        );
        manifest["memory_footprint"]["total_payload_bytes"] = serde_json::json!(
            manifest["memory_footprint"]["total_payload_bytes"]
                .as_u64()
                .unwrap()
                + 1
        );
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let err = validate_nature_package(dir.path()).unwrap_err().to_string();
        assert!(err.contains("memory_footprint"), "{err}");
        assert!(err.contains("does not match package files"), "{err}");
    }

    #[test]
    fn package_validation_rejects_corrupt_binary_scatter() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let dir = tempdir().unwrap();
        let config = NaturePackageConfig {
            map_resolution: 8,
            preview_resolution: 8,
            scatter_chunk_size: 8.0,
            ..NaturePackageConfig::default()
        };

        let summary = patch.write_package(dir.path(), &config).unwrap();
        let binary_path = summary.scatter_binary_paths.first().unwrap();
        let mut bytes = std::fs::read(binary_path).unwrap();
        bytes[0] = b'X';
        std::fs::write(binary_path, bytes).unwrap();

        let err = validate_nature_package(dir.path()).unwrap_err().to_string();
        assert!(err.contains("invalid magic"), "{err}");
    }

    #[test]
    fn package_validation_rejects_scatter_binary_json_record_mismatch() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let dir = tempdir().unwrap();
        let config = NaturePackageConfig {
            map_resolution: 8,
            preview_resolution: 8,
            scatter_chunk_size: 8.0,
            ..NaturePackageConfig::default()
        };

        let summary = patch.write_package(dir.path(), &config).unwrap();
        let binary_path = summary.scatter_binary_paths.first().unwrap();
        let mut bytes = std::fs::read(binary_path).unwrap();
        let height_offset = SCATTER_BINARY_HEADER_BYTES as usize + 16;
        bytes[height_offset..height_offset + 4].copy_from_slice(&1.2345f32.to_le_bytes());
        std::fs::write(binary_path, bytes).unwrap();

        let err = validate_nature_package(dir.path()).unwrap_err().to_string();
        assert!(err.contains("record checksum mismatches"), "{err}");
    }

    #[test]
    fn package_validation_rejects_corrupt_prototype_glb() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let dir = tempdir().unwrap();
        let config = NaturePackageConfig {
            map_resolution: 8,
            preview_resolution: 8,
            scatter_chunk_size: 8.0,
            ..NaturePackageConfig::default()
        };

        patch.write_package(dir.path(), &config).unwrap();
        std::fs::write(
            dir.path().join("prototypes/fine_meadow_grass_lod0.glb"),
            b"not a glb",
        )
        .unwrap();

        let err = validate_nature_package(dir.path()).unwrap_err().to_string();
        assert!(err.contains("failed to parse"), "{err}");
    }

    #[test]
    fn package_validation_rejects_wrong_map_color_type() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let dir = tempdir().unwrap();
        let config = NaturePackageConfig {
            map_resolution: 8,
            preview_resolution: 8,
            scatter_chunk_size: 8.0,
            ..NaturePackageConfig::default()
        };

        patch.write_package(dir.path(), &config).unwrap();
        GrayImage::from_pixel(8, 8, Luma([128u8]))
            .save(dir.path().join("maps/height_u16.png"))
            .unwrap();

        let err = validate_nature_package(dir.path()).unwrap_err().to_string();
        assert!(err.contains("must decode as L16"), "{err}");
    }

    #[test]
    fn package_validation_rejects_mismatched_normal_convention_maps() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let dir = tempdir().unwrap();
        let config = NaturePackageConfig {
            map_resolution: 8,
            preview_resolution: 8,
            scatter_chunk_size: 8.0,
            ..NaturePackageConfig::default()
        };

        patch.write_package(dir.path(), &config).unwrap();
        std::fs::copy(
            dir.path().join("maps/normal_yplus.png"),
            dir.path().join("maps/normal_yminus.png"),
        )
        .unwrap();

        let err = validate_nature_package(dir.path()).unwrap_err().to_string();
        assert!(err.contains("paired Y+/Y- encodings"), "{err}");
    }

    #[test]
    fn package_validation_rejects_mismatched_grass_density_map() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let dir = tempdir().unwrap();
        let config = NaturePackageConfig {
            map_resolution: 8,
            preview_resolution: 8,
            scatter_chunk_size: 8.0,
            ..NaturePackageConfig::default()
        };

        patch.write_package(dir.path(), &config).unwrap();
        GrayImage::from_pixel(8, 8, Luma([0u8]))
            .save(dir.path().join("maps/grass_density.png"))
            .unwrap();

        let err = validate_nature_package(dir.path()).unwrap_err().to_string();
        assert!(err.contains("masks_rgba R channel"), "{err}");
    }

    #[test]
    fn manifest_records_preview_only_shader_policy() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let manifest = patch.export_manifest(64);
        manifest.validate().unwrap();
        assert_eq!(manifest.schema_version, 3);
        assert_eq!(manifest.generator, "midori");
        assert_eq!(manifest.axis.up_axis, "Y");
        assert_eq!(manifest.axis.handedness, "right");
        assert_eq!(manifest.terrain.heightmap_file, "maps/height_u16.png");
        assert!(manifest.terrain.height_min <= manifest.terrain.height_max);
        assert!(manifest.terrain.bounds_min[1] <= manifest.terrain.height_min);
        assert!(manifest.terrain.bounds_max[1] >= manifest.terrain.height_max);
        assert_eq!(
            manifest.normal_conventions.unity_yplus_file,
            "maps/normal_yplus.png"
        );
        assert_eq!(
            manifest.normal_conventions.unreal_yminus_file,
            "maps/normal_yminus.png"
        );
        assert_eq!(manifest.shader_policy, "preview_only");
        assert_eq!(manifest.texture_pipeline, "parked");
        assert!(
            manifest
                .map_channels
                .iter()
                .any(|channel| channel.file == "masks_rgba.png")
        );
        assert!(
            manifest
                .material_slots
                .iter()
                .any(|slot| slot.name == "groundcover_foliage" && slot.alpha_mode == "masked")
        );
        let terrain_parameters = manifest
            .material_parameters
            .iter()
            .find(|set| set.material_slot == "terrain_surface")
            .unwrap();
        assert_eq!(terrain_parameters.runtime_policy, "engine_native_static");
        assert!(
            terrain_parameters
                .parameters
                .iter()
                .any(|parameter| parameter.name == "Midori_MaskTexture"
                    && parameter.default_value == "maps/masks_rgba.png")
        );
        let groundcover_parameters = manifest
            .material_parameters
            .iter()
            .find(|set| set.material_slot == "groundcover_foliage")
            .unwrap();
        assert!(
            groundcover_parameters
                .parameters
                .iter()
                .any(|parameter| parameter.semantic == "wind_strength"
                    && parameter.name == "Midori_WindStrength")
        );
        assert!(
            groundcover_parameters
                .parameters
                .iter()
                .any(|parameter| parameter.semantic == "fade_end_meters"
                    && parameter.default_value == "32.000")
        );
        let terrain_recipe = manifest
            .material_recipes
            .iter()
            .find(|recipe| recipe.material_slot == "terrain_surface")
            .unwrap();
        assert_eq!(terrain_recipe.file, "materials/terrain_surface.recipe.json");
        assert_eq!(terrain_recipe.runtime_policy, "engine_native_static");
        assert!(
            terrain_recipe
                .engine_targets
                .iter()
                .any(|target| target == "unity_terrain_material")
        );
        assert!(
            terrain_recipe
                .engine_targets
                .iter()
                .any(|target| target == "unreal_landscape_material")
        );
        let groundcover_recipe = manifest
            .material_recipes
            .iter()
            .find(|recipe| recipe.material_slot == "groundcover_foliage")
            .unwrap();
        assert_eq!(
            groundcover_recipe.file,
            "materials/groundcover_foliage.recipe.json"
        );
        assert!(
            groundcover_recipe
                .engine_targets
                .iter()
                .any(|target| target == "unity_detail_mesh_material")
        );
        assert!(
            groundcover_recipe
                .engine_targets
                .iter()
                .any(|target| target == "unreal_static_mesh_foliage_material")
        );
        let unity_import_recipe = manifest
            .engine_import_recipes
            .iter()
            .find(|recipe| recipe.engine == "unity")
            .unwrap();
        assert_eq!(unity_import_recipe.file, "engines/unity_import.recipe.json");
        assert_eq!(unity_import_recipe.profile, "mobile");
        assert_eq!(unity_import_recipe.runtime_policy, "engine_native_static");
        assert!(
            unity_import_recipe
                .expected_systems
                .iter()
                .any(|system| system == "Unity TerrainData")
        );
        let unreal_import_recipe = manifest
            .engine_import_recipes
            .iter()
            .find(|recipe| recipe.engine == "unreal")
            .unwrap();
        assert_eq!(
            unreal_import_recipe.file,
            "engines/unreal_import.recipe.json"
        );
        assert_eq!(unreal_import_recipe.profile, "console");
        assert!(
            unreal_import_recipe
                .expected_systems
                .iter()
                .any(|system| system == "Static Mesh Foliage")
        );
        assert_eq!(manifest.surface_overlays.len(), 3);
        let moss_overlay = manifest
            .surface_overlays
            .iter()
            .find(|overlay| overlay.name == "moss")
            .unwrap();
        assert_eq!(moss_overlay.source_file, "maps/masks_rgba.png");
        assert_eq!(moss_overlay.channel, "G");
        assert!(moss_overlay.targets.iter().any(|target| target == "rock"));
        assert!(moss_overlay.targets.iter().any(|target| target == "log"));
        assert_eq!(moss_overlay.runtime_policy, "baked_static");
        let wetness_overlay = manifest
            .surface_overlays
            .iter()
            .find(|overlay| overlay.name == "wetness")
            .unwrap();
        assert_eq!(wetness_overlay.channel, "B");
        assert!(
            wetness_overlay
                .targets
                .iter()
                .any(|target| target == "terrain_surface")
        );
        let cracks_overlay = manifest
            .surface_overlays
            .iter()
            .find(|overlay| overlay.name == "cracks")
            .unwrap();
        assert_eq!(cracks_overlay.channel, "A");
        assert!(
            cracks_overlay
                .targets
                .iter()
                .any(|target| target == "scatter_exclusion")
        );
        let grass = manifest
            .prototypes
            .iter()
            .find(|prototype| prototype.kind == GroundcoverKind::Grass)
            .unwrap();
        assert_eq!(grass.lods.len(), 3);
        assert_eq!(grass.lods[0].file, "prototypes/fine_meadow_grass_lod0.glb");
        assert!(grass.lods[0].triangle_count > grass.lods[1].triangle_count);
        assert!(grass.lods[0].bounds_min[0] <= grass.lods[0].bounds_max[0]);
        assert!(
            grass
                .surface_targets
                .iter()
                .any(|target| target == "groundcover_foliage")
        );
        let rock = manifest
            .prototypes
            .iter()
            .find(|prototype| prototype.kind == GroundcoverKind::Rock)
            .unwrap();
        assert!(rock.surface_targets.iter().any(|target| target == "rock"));
        assert!(
            rock.surface_targets
                .iter()
                .any(|target| target == "static_surface")
        );
        let log = manifest
            .prototypes
            .iter()
            .find(|prototype| prototype.kind == GroundcoverKind::Log)
            .unwrap();
        assert!(log.surface_targets.iter().any(|target| target == "log"));
        assert!(manifest.scatter.enabled);
        assert_eq!(manifest.scatter.chunk_size, 8.0);
        assert!(!manifest.scatter.binary_files.is_empty());
        assert_eq!(
            manifest.scatter.binary_format.record,
            scatter_instance_fields()
        );
        assert_eq!(manifest.wind_packing.phase, "TEXCOORD_1.x");
        assert_eq!(manifest.unity.detail_batch_max_instances, 1023);
        assert_eq!(manifest.unity.normal_map, "maps/normal_yplus.png");
        assert_eq!(manifest.unreal.normal_map, "maps/normal_yminus.png");
        assert_eq!(manifest.mobile.material_slots, 1);
        assert_eq!(manifest.console.material_slots, 1);

        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains("Temperate Forest Floor"));
        assert!(json.contains("preview_only"));
        assert!(json.contains("Static Mesh Foliage"));
        assert!(json.contains("material_parameters"));
        assert!(json.contains("material_recipes"));
        assert!(json.contains("engine_import_recipes"));
        assert!(json.contains("surface_overlays"));
    }

    #[test]
    fn manifest_validation_rejects_missing_required_map_channel() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let mut manifest = patch.export_manifest(64);
        manifest
            .map_channels
            .retain(|channel| channel.file != "masks_rgba.png");

        let err = manifest.validate().unwrap_err().to_string();
        assert!(err.contains("canonical Midori map files"));
    }

    #[test]
    fn manifest_validation_rejects_missing_surface_overlay() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let mut manifest = patch.export_manifest(64);
        manifest
            .surface_overlays
            .retain(|overlay| overlay.name != "moss");

        let err = manifest.validate().unwrap_err().to_string();
        assert!(err.contains("surface_overlays must include moss"), "{err}");
    }

    #[test]
    fn manifest_validation_rejects_missing_material_parameter() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let mut manifest = patch.export_manifest(64);
        let groundcover = manifest
            .material_parameters
            .iter_mut()
            .find(|set| set.material_slot == "groundcover_foliage")
            .unwrap();
        groundcover
            .parameters
            .retain(|parameter| parameter.semantic != "wind_strength");

        let err = manifest.validate().unwrap_err().to_string();
        assert!(
            err.contains("groundcover_foliage") && err.contains("wind_strength"),
            "{err}"
        );
    }

    #[test]
    fn manifest_validation_rejects_missing_material_recipe() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let mut manifest = patch.export_manifest(64);
        manifest
            .material_recipes
            .retain(|recipe| recipe.material_slot != "groundcover_foliage");

        let err = manifest.validate().unwrap_err().to_string();
        assert!(
            err.contains("material_recipes must include groundcover_foliage"),
            "{err}"
        );
    }

    #[test]
    fn manifest_validation_rejects_missing_engine_import_recipe() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let mut manifest = patch.export_manifest(64);
        manifest
            .engine_import_recipes
            .retain(|recipe| recipe.engine != "unreal");

        let err = manifest.validate().unwrap_err().to_string();
        assert!(
            err.contains("engine_import_recipes must include unreal"),
            "{err}"
        );
    }

    #[test]
    fn manifest_validation_rejects_missing_prototype_surface_target() {
        let patch = NaturePatch::from_toml(PATCH_TOML).unwrap();
        let mut manifest = patch.export_manifest(64);
        let rock = manifest
            .prototypes
            .iter_mut()
            .find(|prototype| prototype.kind == GroundcoverKind::Rock)
            .unwrap();
        rock.surface_targets.retain(|target| target != "rock");

        let err = manifest.validate().unwrap_err().to_string();
        assert!(err.contains("surface target `rock`"), "{err}");
    }
}
