//! Volumetric nature stands (`[asset] kind = "nature_stand"`).
//!
//! A volumetric stand combines an existing ground patch (`presets/nature/*.toml`)
//! with interacting vertical strata drawn from species presets (`presets/species/*.toml`),
//! a bounded reusable plant variant library, seamless world-space tiled placement,
//! volumetric Stand LOD 0/1/2 meshes, rasters, collision capsules, and package validation.

use crate::export::{ExportConfig, ExportMetadata, export_lod_meshes};
use crate::generation::generate_tree;
use crate::lod::{LodGenerationConfig, generate_lod_meshes_with_foliage_mode};
use crate::opaque_foliage::{FoliageRenderMode, species_bark_color, species_foliage_colors};
use crate::species::{GeneratorFamily, Species};
use crate::textures::TextureSet;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// Magic bytes for binary stand instance buffers (`midori.stand.instances.v1`).
pub const STAND_INSTANCES_MAGIC: &[u8; 4] = b"MDSI";
/// Binary stand instance record format version.
pub const STAND_INSTANCES_VERSION: u32 = 1;
/// Byte size of one binary stand instance record (4+2+1+1+12+4+4+12 = 40 bytes).
pub const STAND_INSTANCE_RECORD_BYTES: usize = 40;
/// Current stand manifest schema version.
pub const STAND_MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Error returned when parsing, generating, or validating a volumetric stand.
#[derive(Debug)]
pub enum StandError {
    Io(std::io::Error),
    Parse(String),
    Validation(String),
    Species(String),
    Budget(String),
    Export(String),
}

impl std::fmt::Display for StandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "stand I/O error: {e}"),
            Self::Parse(m) => write!(f, "stand parse error: {m}"),
            Self::Validation(m) => write!(f, "stand validation error: {m}"),
            Self::Species(m) => write!(f, "stand species error: {m}"),
            Self::Budget(m) => write!(f, "stand budget exceeded: {m}"),
            Self::Export(m) => write!(f, "stand export error: {m}"),
        }
    }
}

impl std::error::Error for StandError {}

impl From<std::io::Error> for StandError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Top-level volumetric nature stand TOML document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatureStand {
    pub asset: StandAssetInfo,
    pub stand: StandParams,
    #[serde(default)]
    pub strata: Vec<StratumParams>,
}

/// Asset header (`[asset]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandAssetInfo {
    pub kind: String,
    pub name: String,
    #[serde(default = "default_units")]
    pub units: String,
}

fn default_units() -> String {
    "meters".to_string()
}

/// Core stand configuration (`[stand]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandParams {
    /// Optional path to a `presets/nature/*.toml` ground patch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor: Option<String>,
    /// World seed for deterministic stand generation.
    #[serde(default = "default_stand_seed")]
    pub seed: u64,
    /// Biome tag for metadata and floor fallback.
    #[serde(default = "default_biome")]
    pub biome: String,
    /// Maximum distinct generated plant variants per `(species, state, size_class)`.
    #[serde(default = "default_variants_per_species_size")]
    pub variants_per_species_size: u32,
    /// Distance inside a mask or exclusion boundary where bush-edge response applies (metres).
    #[serde(default = "default_edge_width")]
    pub edge_width: f32,
    /// Density multiplier near a stand edge.
    #[serde(default = "default_edge_density_multiplier")]
    pub edge_density_multiplier: f32,
    /// Height multiplier at the outer rim of a stand edge (interpolates to 1.0 at `edge_width`).
    #[serde(default = "default_edge_height_scale")]
    pub edge_height_scale: f32,
    /// Additional shrub/low-layer density boost near edges.
    #[serde(default = "default_edge_shrub_boost")]
    pub edge_shrub_boost: f32,
    /// Minimum trunk base clearance from hard exclusions (metres).
    #[serde(
        default = "default_exclude_trunk_clearance",
        alias = "exclusion_clearance"
    )]
    pub exclude_trunk_clearance: f32,
    /// Maximum crown overhang allowed into hard exclusions (metres).
    #[serde(default = "default_exclude_crown_overhang")]
    pub exclude_crown_overhang: f32,
    /// Minimum trunk diameter (DBH, metres) emitted into `collision.json` capsules.
    #[serde(default = "default_collision_min_diameter")]
    pub collision_min_diameter: f32,
    /// Include coarse canopy heightfield in `collision.json`.
    #[serde(default = "default_true")]
    pub include_canopy_heightfield: bool,
    /// Static ecological condition (fire age, dieback).
    #[serde(default)]
    pub condition: StandCondition,
}

fn default_stand_seed() -> u64 {
    7001
}
fn default_biome() -> String {
    "bushland".to_string()
}
fn default_variants_per_species_size() -> u32 {
    3
}
fn default_edge_width() -> f32 {
    8.0
}
fn default_edge_density_multiplier() -> f32 {
    1.55
}
fn default_edge_height_scale() -> f32 {
    0.72
}
fn default_edge_shrub_boost() -> f32 {
    1.75
}
fn default_exclude_trunk_clearance() -> f32 {
    1.2
}
fn default_exclude_crown_overhang() -> f32 {
    2.5
}
fn default_collision_min_diameter() -> f32 {
    0.12
}
fn default_true() -> bool {
    true
}

/// Static ecological condition (`[stand.condition]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandCondition {
    /// Years since last bushfire (drives grass-tree skirts/burnt state, charred bases,
    /// epicormic regrowth, and shrub height).
    #[serde(default = "default_years_since_fire")]
    pub years_since_fire: f32,
    /// Fraction `0..=1` of dieback-susceptible species (banksias, jarrah) converted to
    /// dead/thinning stags.
    #[serde(default)]
    pub dieback: f32,
}

fn default_years_since_fire() -> f32 {
    10.0
}

impl Default for StandCondition {
    fn default() -> Self {
        Self {
            years_since_fire: default_years_since_fire(),
            dieback: 0.0,
        }
    }
}

/// Spatial arrangement pattern for a stratum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandPattern {
    #[default]
    Random,
    Regular,
    Clumped,
}

/// Response of a lower stratum to projected canopy cover from strata above it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnderstoryTolerance {
    /// Shade-intolerant: favours gaps and open edges (`weight = 1 - 0.85 * cover_above`).
    Avoid,
    /// Broadly indifferent (`weight = 1 - 0.15 * cover_above`).
    #[default]
    Neutral,
    /// Shade-preferring understorey (`weight = 0.25 + 0.95 * cover_above`).
    Prefer,
}

/// Optional topographic wetness / riparian zoning for a stratum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoistureZone {
    #[default]
    Any,
    Dry,
    Wet,
    Riparian,
}

/// Age/size class mapped onto species preset botanical parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeClass {
    Sapling,
    Pole,
    Mature,
    Veteran,
}

impl SizeClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sapling => "sapling",
            Self::Pole => "pole",
            Self::Mature => "mature",
            Self::Veteran => "veteran",
        }
    }
}

/// One vertical layer (`[[strata]]`) within a volumetric stand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StratumParams {
    pub name: String,
    /// Optional target height range override `[min_m, max_m]`. When omitted,
    /// each species variant keeps its true botanical size-class height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_range: Option<[f32; 2]>,
    /// Requested projected foliage cover (`0..=1`).
    #[serde(default = "default_cover")]
    pub cover: f32,
    /// Target stem density per hectare (`10,000 m²`). When omitted, derived automatically
    /// from `cover` and the mean crown area of the stratum's plant variants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stems_per_ha: Option<f32>,
    #[serde(default)]
    pub pattern: StandPattern,
    /// Clump radius in metres when `pattern = "clumped"`.
    #[serde(default = "default_clump_radius", alias = "clump_scale")]
    pub clump_radius: f32,
    /// Row spacing in metres when `pattern = "regular"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_spacing: Option<f32>,
    /// Minimum centre-to-centre stem spacing in metres.
    #[serde(default = "default_min_spacing")]
    pub min_spacing: f32,
    /// Optional name of a higher stratum (or `"canopy"`) this stratum responds to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub under: Option<String>,
    /// Tolerance to cover from higher strata (`avoid | neutral | prefer`).
    #[serde(default, alias = "understory_tolerance")]
    pub under_tolerance: UnderstoryTolerance,
    /// Optional terrain wetness/riparian preference.
    #[serde(default)]
    pub moisture: MoistureZone,
    /// Optional maximum terrain slope in degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slope_max_degrees: Option<f32>,
    /// Mark stratum as dead timber / standing stags (bare foliage).
    #[serde(default)]
    pub is_dead: bool,
    /// Species mix in this stratum.
    #[serde(default)]
    pub species: Vec<StandSpeciesEntry>,
}

fn default_cover() -> f32 {
    0.35
}
fn default_clump_radius() -> f32 {
    8.0
}
fn default_min_spacing() -> f32 {
    1.5
}

/// Species entry inside a stratum (`[[strata.species]]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandSpeciesEntry {
    /// Path to a species TOML preset (`presets/species/*.toml`).
    pub preset: String,
    /// Optional `[states.<name>]` overlay to apply (e.g. `"flowering"`, `"forest"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// Relative share weight inside the stratum.
    #[serde(default = "default_share", alias = "weight")]
    pub share: f32,
    /// Whether Phytophthora dieback affects this species (inferred true for banksia/jarrah if unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub susceptible_to_dieback: Option<bool>,
    /// Distribution over age/size classes (defaults to `{ mature = 1.0 }`).
    #[serde(default)]
    pub size_classes: BTreeMap<SizeClass, f32>,
}

fn default_share() -> f32 {
    1.0
}

impl StandSpeciesEntry {
    /// Whether this species entry is susceptible to `stand.condition.dieback`.
    pub fn is_dieback_susceptible(&self) -> bool {
        if let Some(v) = self.susceptible_to_dieback {
            return v;
        }
        let lower = self.preset.to_ascii_lowercase();
        lower.contains("banksia") || lower.contains("jarrah") || lower.contains("grass_tree")
    }

    /// Resolved normalized size-class distribution (falls back to 100% `Mature`).
    pub fn resolved_size_classes(&self) -> Vec<(SizeClass, f32)> {
        if self.size_classes.is_empty() {
            return vec![(SizeClass::Mature, 1.0)];
        }
        let sum: f32 = self
            .size_classes
            .values()
            .copied()
            .filter(|v| *v > 0.0)
            .sum();
        if sum <= 0.0 {
            return vec![(SizeClass::Mature, 1.0)];
        }
        self.size_classes
            .iter()
            .filter(|&(_, &w)| w > 0.0)
            .map(|(&c, &w)| (c, w / sum))
            .collect()
    }
}

impl NatureStand {
    /// Parse a volumetric stand TOML string and validate it.
    pub fn from_toml(toml_str: &str) -> Result<Self, StandError> {
        // Support shorthand `under = "avoid" | "neutral" | "prefer"` in addition to stratum name!
        let mut stand: Self =
            toml::from_str(toml_str).map_err(|e| StandError::Parse(e.to_string()))?;
        for stratum in &mut stand.strata {
            if let Some(ref u) = stratum.under {
                match u.to_ascii_lowercase().as_str() {
                    "avoid" => {
                        stratum.under_tolerance = UnderstoryTolerance::Avoid;
                    }
                    "neutral" => {
                        stratum.under_tolerance = UnderstoryTolerance::Neutral;
                    }
                    "prefer" => {
                        stratum.under_tolerance = UnderstoryTolerance::Prefer;
                    }
                    _ => {}
                }
            }
        }
        stand.validate()?;
        Ok(stand)
    }

    /// Load and validate a volumetric stand from a TOML file.
    pub fn from_file(path: &Path) -> Result<Self, StandError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_toml(&text)
    }

    /// Validate all structural fields and ranges.
    pub fn validate(&self) -> Result<(), StandError> {
        if self.asset.kind != "nature_stand" {
            return Err(StandError::Validation(format!(
                "asset.kind must be \"nature_stand\", got {:?}",
                self.asset.kind
            )));
        }
        if self.asset.name.trim().is_empty() {
            return Err(StandError::Validation(
                "asset.name must not be empty".to_string(),
            ));
        }
        if self.stand.variants_per_species_size == 0 || self.stand.variants_per_species_size > 16 {
            return Err(StandError::Validation(
                "stand.variants_per_species_size must be in 1..=16".to_string(),
            ));
        }
        if !self.stand.edge_width.is_finite() || self.stand.edge_width < 0.0 {
            return Err(StandError::Validation(
                "stand.edge_width must be non-negative and finite".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&self.stand.condition.dieback) {
            return Err(StandError::Validation(
                "stand.condition.dieback must be in 0.0..=1.0".to_string(),
            ));
        }
        if !self.stand.condition.years_since_fire.is_finite()
            || self.stand.condition.years_since_fire < 0.0
        {
            return Err(StandError::Validation(
                "stand.condition.years_since_fire must be non-negative".to_string(),
            ));
        }
        if self.strata.is_empty() {
            return Err(StandError::Validation(
                "stand must declare at least one [[strata]] entry".to_string(),
            ));
        }
        let mut seen_names = std::collections::BTreeSet::new();
        for (i, stratum) in self.strata.iter().enumerate() {
            if stratum.name.trim().is_empty()
                || !stratum
                    .name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            {
                return Err(StandError::Validation(format!(
                    "strata[{i}].name {:?} must contain only ASCII letters, digits, '_' or '-'",
                    stratum.name
                )));
            }
            if !seen_names.insert(stratum.name.clone()) {
                return Err(StandError::Validation(format!(
                    "duplicate stratum name {:?}",
                    stratum.name
                )));
            }
            if let Some(hr) = stratum.height_range
                && (hr[0] <= 0.0 || hr[1] < hr[0] || !hr[1].is_finite())
            {
                return Err(StandError::Validation(format!(
                    "strata[{i}].height_range must be positive and ordered"
                )));
            }
            if !(0.0..=1.0).contains(&stratum.cover) {
                return Err(StandError::Validation(format!(
                    "strata[{i}].cover must be in 0.0..=1.0"
                )));
            }
            if let Some(sph) = stratum.stems_per_ha
                && (!sph.is_finite() || sph < 0.0)
            {
                return Err(StandError::Validation(format!(
                    "strata[{i}].stems_per_ha must be non-negative"
                )));
            }
            if !stratum.min_spacing.is_finite() || stratum.min_spacing <= 0.0 {
                return Err(StandError::Validation(format!(
                    "strata[{i}].min_spacing must be positive"
                )));
            }
            if stratum.species.is_empty() {
                return Err(StandError::Validation(format!(
                    "strata[{i}] ({}) must declare at least one [[strata.species]]",
                    stratum.name
                )));
            }
            let share_sum: f32 = stratum.species.iter().map(|s| s.share).sum();
            if share_sum <= 0.0 {
                return Err(StandError::Validation(format!(
                    "strata[{i}] ({}) species shares must sum > 0",
                    stratum.name
                )));
            }
        }
        Ok(())
    }
}

/// Apply documented non-uniform botanical parameter scaling for a `SizeClass`
/// plus static `StandCondition` (`years_since_fire`, dead stag flag) onto a `Species`.
pub fn scale_species_for_size_class(
    base: &Species,
    size_class: SizeClass,
    condition: &StandCondition,
    is_dead: bool,
) -> Species {
    let mut sp = base.clone();

    // Non-uniform architectural scaling (not uniform XYZ scale alone!)
    let (h_mul, r_mul, crown_off_mul, br_len_mul, buttress_mul) = match size_class {
        SizeClass::Sapling => (0.38, 0.34, 0.65, 0.46, 0.55),
        SizeClass::Pole => (0.70, 0.64, 0.86, 0.74, 0.82),
        SizeClass::Mature => (1.00, 1.00, 1.00, 1.00, 1.00),
        SizeClass::Veteran => (1.16, 1.38, 1.10, 1.14, 1.25),
    };

    sp.trunk.height = (sp.trunk.height * h_mul).max(0.05);
    sp.trunk.radius = (sp.trunk.radius * r_mul).max(0.01);
    sp.trunk.buttress = (sp.trunk.buttress * buttress_mul).clamp(0.0, 3.0);
    sp.crown.offset = (sp.crown.offset * crown_off_mul).clamp(0.05, 0.85);

    for lvl in [
        &mut sp.branches.level1,
        &mut sp.branches.level2,
        &mut sp.branches.level3,
        &mut sp.branches.level4,
    ]
    .into_iter()
    .flatten()
    {
        lvl.length = (lvl.length * br_len_mul).max(0.05);
    }

    if let Some(ref mut ros) = sp.rosette {
        let blade_mul = match size_class {
            SizeClass::Sapling => 0.52,
            SizeClass::Pole => 0.78,
            SizeClass::Mature => 1.0,
            SizeClass::Veteran => 1.15,
        };
        ros.blade_length = (ros.blade_length * blade_mul).max(0.08);
        // Fire condition modulates grass-tree dead-leaf skirts
        if condition.years_since_fire < 3.0 {
            ros.skirt_blades = 0;
            ros.skirt = 0.0;
        } else if condition.years_since_fire < 7.0 {
            let k = (condition.years_since_fire - 3.0) / 4.0;
            ros.skirt_blades = ((ros.skirt_blades as f32) * (0.25 + 0.75 * k)) as u32;
        }
    }

    // Fire age blackens lower bark on woody trees and reduces young shrub stature
    if sp.generator.family != GeneratorFamily::Rosette && condition.years_since_fire < 12.0 {
        let fire_k = (1.0 - condition.years_since_fire / 12.0).clamp(0.0, 1.0);
        if sp.trunk.height >= 2.5 {
            sp.trunk.char_height = sp
                .trunk
                .char_height
                .max((1.8 * fire_k).min(sp.trunk.height * 0.45));
            sp.trunk.char_strength = sp.trunk.char_strength.max(0.85 * fire_k);
        } else if condition.years_since_fire < 5.0 {
            let shrub_mul = (0.45 + 0.11 * condition.years_since_fire).clamp(0.45, 1.0);
            sp.trunk.height = (sp.trunk.height * shrub_mul).max(0.08);
        }
    }

    // Dead stag / dieback conversion: strip foliage & flowers, silver weathered bark
    if is_dead {
        sp.leaves.count = 0;
        if let Some(ref mut inf) = sp.inflorescence {
            inf.count = 0;
        }
        if let Some(ref mut sec) = sp.secondary_inflorescence {
            sec.count = 0;
        }
        if let Some(ref mut ros) = sp.rosette {
            ros.blades = (ros.blades / 4).max(4);
            ros.color_base = Some([0.38, 0.34, 0.28]);
            ros.color_tip = Some([0.48, 0.44, 0.37]);
        }
        sp.textures.bark_color = Some([0.46, 0.44, 0.41]);
        sp.species.name = format!("{} (dead)", sp.species.name);
    }

    sp
}

/// Measured botanical envelope of one generated plant variant, used to build
/// accurate volumetric Stand LOD 0/1/2 meshes and cover calculations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandVariantProfile {
    pub variant_id: String,
    pub variant_index: u32,
    pub stratum: String,
    pub preset: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    pub size_class: SizeClass,
    pub is_dead: bool,
    pub seed: u64,
    /// Relative path inside the stand package (`library/<variant_id>.glb`).
    pub glb_file: String,
    pub height_m: f32,
    pub crown_radius_m: f32,
    pub crown_base_y_m: f32,
    pub crown_top_y_m: f32,
    pub trunk_radius_m: f32,
    pub foliage_top_srgb: [f32; 3],
    pub foliage_bot_srgb: [f32; 3],
    pub bark_srgb: [f32; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organ_srgb: Option<[f32; 3]>,
    pub is_rosette: bool,
    pub lod0_triangles: u32,
}

/// Reusable bounded plant library built once per stand.
#[derive(Debug, Clone)]
pub struct StandLibrary {
    pub variants: Vec<StandVariantProfile>,
    pub build_ms: u64,
}

/// Resolve a species or floor path relative to `stand_dir` or workspace root.
pub fn resolve_asset_path(rel: &str, base_dir: &Path) -> PathBuf {
    let direct = Path::new(rel);
    if direct.is_file() {
        return direct.to_path_buf();
    }
    let from_base = base_dir.join(rel);
    if from_base.is_file() {
        return from_base;
    }
    // Walk up from base_dir to find repo root containing `presets/`
    let mut cur = Some(base_dir);
    while let Some(dir) = cur {
        let cand = dir.join(rel);
        if cand.is_file() {
            return cand;
        }
        cur = dir.parent();
    }
    direct.to_path_buf()
}

fn sanitize_id(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

/// Build the bounded plant library for a stand (at most `variants_per_species_size`
/// distinct variants per `(species, state, size_class)`), optionally writing GLBs into `out_dir/library/`.
pub fn build_stand_library(
    stand: &NatureStand,
    base_dir: &Path,
    out_dir: Option<&Path>,
    foliage_mode: FoliageRenderMode,
    write_glbs: bool,
) -> Result<StandLibrary, StandError> {
    #[cfg(not(target_arch = "wasm32"))]
    let start = std::time::Instant::now();
    if let Some(dir) = out_dir
        && write_glbs
    {
        std::fs::create_dir_all(dir.join("library"))?;
    }

    let mut variants = Vec::new();
    let mut seen_keys: BTreeMap<(String, Option<String>, SizeClass, bool), Vec<u32>> =
        BTreeMap::new();
    let mut used_variant_ids: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    let v_count = stand.stand.variants_per_species_size.max(1);

    for (s_idx, stratum) in stand.strata.iter().enumerate() {
        for sp_entry in &stratum.species {
            let preset_path = resolve_asset_path(&sp_entry.preset, base_dir);
            let stem_name = preset_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("species");

            // Condition-driven state override (e.g., recently burnt grass tree or post-fire jarrah)
            let effective_state: Option<String> = if let Some(ref explicit) = sp_entry.state {
                Some(explicit.clone())
            } else if stand.stand.condition.years_since_fire < 2.5
                && stem_name.contains("grass_tree")
            {
                Some("burnt".to_string())
            } else if stand.stand.condition.years_since_fire < 3.5 && stem_name.contains("jarrah") {
                Some("post_fire".to_string())
            } else {
                None
            };

            let raw_species =
                Species::from_file_with_state(&preset_path, effective_state.as_deref()).map_err(
                    |e| {
                        StandError::Species(format!(
                            "failed loading {} (state={:?}): {e}",
                            preset_path.display(),
                            effective_state
                        ))
                    },
                )?;

            // Build variants for both live and (if dieback > 0 & susceptible) dead stags
            let mut dead_modes = vec![stratum.is_dead];
            if !stratum.is_dead
                && stand.stand.condition.dieback > 0.0
                && sp_entry.is_dieback_susceptible()
            {
                dead_modes.push(true);
            }

            for is_dead in dead_modes {
                for (size_class, _) in sp_entry.resolved_size_classes() {
                    let key = (
                        sp_entry.preset.clone(),
                        effective_state.clone(),
                        size_class,
                        is_dead,
                    );
                    if seen_keys.contains_key(&key) {
                        continue;
                    }

                    let scaled = scale_species_for_size_class(
                        &raw_species,
                        size_class,
                        &stand.stand.condition,
                        is_dead,
                    );

                    // Compact LOD chain for stand library variants
                    let lod_cfg = LodGenerationConfig::balanced();
                    let textures = if foliage_mode == FoliageRenderMode::Cutout && write_glbs {
                        let mut tex_sp = scaled.clone();
                        tex_sp.textures.resolution = tex_sp.textures.resolution.min(512);
                        Some(TextureSet::generate(&tex_sp))
                    } else {
                        None
                    };

                    let mut ids = Vec::new();
                    for v_i in 0..v_count {
                        let v_idx = variants.len() as u32;
                        let v_seed = stand.stand.seed
                            ^ ((s_idx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
                            ^ ((v_idx as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9))
                            ^ (v_i as u64 + 1);

                        let tree = generate_tree(&scaled, v_seed).map_err(|e| {
                            StandError::Species(format!(
                                "failed generating variant {stem_name}: {e}"
                            ))
                        })?;

                        let bmin = tree.bounds.min;
                        let bmax = tree.bounds.max;
                        let height_m = (bmax.y - bmin.y.min(0.0)).max(0.25);
                        let crown_rx = (bmax.x - bmin.x) * 0.5;
                        let crown_rz = (bmax.z - bmin.z) * 0.5;
                        let crown_radius_m = crown_rx.max(crown_rz).max(0.25);

                        let (crown_base_y, crown_top_y) = if !tree.leaves.is_empty() {
                            let mut lo = f32::MAX;
                            let mut hi = f32::MIN;
                            for leaf in &tree.leaves {
                                lo = lo.min(leaf.position.y - leaf.scale * 0.35);
                                hi = hi.max(leaf.position.y + leaf.scale * 0.35);
                            }
                            (lo.max(0.05), hi.max(lo + 0.25))
                        } else {
                            (height_m * 0.35, height_m)
                        };

                        let (fol_top, fol_bot) = species_foliage_colors(&scaled);
                        let bark_col = species_bark_color(&scaled);
                        let organ_srgb = scaled
                            .inflorescence
                            .as_ref()
                            .filter(|inf| inf.count >= 40)
                            .map(|inf| inf.color);

                        let state_slug = effective_state
                            .as_deref()
                            .map(|s| format!("_{}", sanitize_id(s)))
                            .unwrap_or_default();
                        let dead_slug = if is_dead { "_dead" } else { "" };
                        let base_id = format!(
                            "{}{}{}_{}_v{}",
                            sanitize_id(stem_name),
                            state_slug,
                            dead_slug,
                            size_class.as_str(),
                            v_i
                        );
                        // Disambiguate deterministically if two different directories share a basename
                        let variant_id = if used_variant_ids.contains(&base_id) {
                            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
                            for b in sp_entry.preset.as_bytes() {
                                h ^= u64::from(*b);
                                h = h.wrapping_mul(0x0100_0000_01b3);
                            }
                            let disambig = format!(
                                "{}_{:06x}{}{}_{}_v{}",
                                sanitize_id(stem_name),
                                (h & 0x00ff_ffff) as u32,
                                state_slug,
                                dead_slug,
                                size_class.as_str(),
                                v_i
                            );
                            used_variant_ids.insert(disambig.clone());
                            disambig
                        } else {
                            used_variant_ids.insert(base_id.clone());
                            base_id
                        };
                        let glb_file = format!("library/{variant_id}.glb");

                        let mut lod0_tris = 0u32;
                        if let Some(dir) = out_dir
                            && write_glbs
                        {
                            let lods = generate_lod_meshes_with_foliage_mode(
                                &tree,
                                &scaled,
                                &lod_cfg,
                                foliage_mode,
                            );
                            lod0_tris = lods
                                .meshes
                                .first()
                                .map(|m| m.stats.triangle_count)
                                .unwrap_or(0);
                            let export_cfg = ExportConfig {
                                textures: textures.clone(),
                                leaf_roughness: scaled.canopy.roughness,
                                organ_roughness: scaled
                                    .inflorescence
                                    .as_ref()
                                    .and_then(|i| i.roughness),
                                opaque_foliage: foliage_mode == FoliageRenderMode::Opaque,
                                metadata: Some(ExportMetadata {
                                    species_name: scaled.species.name.clone(),
                                    scientific_name: scaled.latin_name().to_string(),
                                    seed: Some(v_seed),
                                    lod_screen_heights: lods
                                        .meshes
                                        .iter()
                                        .map(|l| l.screen_height)
                                        .collect(),
                                }),
                                ..ExportConfig::default()
                            };
                            export_lod_meshes(&lods, &dir.join(&glb_file), &export_cfg)
                                .map_err(|e| StandError::Export(format!("{glb_file}: {e}")))?;
                        }

                        variants.push(StandVariantProfile {
                            variant_id,
                            variant_index: v_idx,
                            stratum: stratum.name.clone(),
                            preset: sp_entry.preset.clone(),
                            state: effective_state.clone(),
                            size_class,
                            is_dead,
                            seed: v_seed,
                            glb_file,
                            height_m,
                            crown_radius_m,
                            crown_base_y_m: crown_base_y,
                            crown_top_y_m: crown_top_y,
                            trunk_radius_m: scaled.trunk.radius,
                            foliage_top_srgb: fol_top,
                            foliage_bot_srgb: fol_bot,
                            bark_srgb: bark_col,
                            organ_srgb,
                            is_rosette: scaled.generator.family == GeneratorFamily::Rosette,
                            lod0_triangles: lod0_tris,
                        });
                        ids.push(v_idx);
                    }
                    seen_keys.insert(key, ids);
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    let build_ms = start.elapsed().as_millis() as u64;
    #[cfg(target_arch = "wasm32")]
    let build_ms = 0u64;

    Ok(StandLibrary { variants, build_ms })
}

/// Instance placement tier within a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandTier {
    /// Exported as a full individual plant instance (edges, gaps, emergents).
    Instance,
    /// Merged into Stand LOD 0 volumetric clump meshes.
    Merged,
}

/// One placed stem record in a tile (`instances.json` / `instances.bin`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandInstanceRecord {
    pub variant_id: String,
    pub variant_index: u32,
    pub stratum: String,
    pub stratum_index: u16,
    pub tier: StandTier,
    pub position: [f32; 3],
    pub yaw: f32,
    pub scale: f32,
    pub color_tint: [f32; 3],
    pub near_edge: bool,
}

/// One trunk collision capsule (`collision.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandCollisionCapsule {
    pub position: [f32; 3],
    pub radius: f32,
    pub height: f32,
    pub stratum: String,
    pub variant_id: String,
}

/// Optional coarse canopy heightfield stored in `collision.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandCanopyHeightfield {
    pub resolution: u32,
    pub tile_size_m: f32,
    pub origin_xz: [f32; 2],
    pub heights_m: Vec<f32>,
}

/// Per-tile collision proxy document (`collision.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandCollisionFile {
    pub tile: [i32; 2],
    pub min_diameter_m: f32,
    pub capsules: Vec<StandCollisionCapsule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canopy_heightfield: Option<StandCanopyHeightfield>,
}

/// Per-stratum stats inside `stand_stats.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StratumStats {
    pub name: String,
    pub stems: u32,
    pub instanced: u32,
    pub merged: u32,
    pub requested_cover: f32,
    pub achieved_cover: f32,
    pub lod0_triangles: u32,
}

/// Per-tile statistics (`stand_stats.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandTileStats {
    pub tile: [i32; 2],
    pub tile_size_m: f32,
    pub total_stems: u32,
    pub instanced_count: u32,
    pub merged_count: u32,
    pub requested_total_cover: f32,
    pub achieved_total_cover: f32,
    pub triangles_lod0: u32,
    pub triangles_lod1: u32,
    pub triangles_lod2: u32,
    pub material_count: u32,
    pub foliage_mode: FoliageRenderMode,
    pub generation_ms: u64,
    pub strata: Vec<StratumStats>,
}

/// Raster files exported for a single tile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandTileRastersManifest {
    pub resolution: u32,
    pub canopy_height: String,
    pub canopy_height_max_m: f32,
    pub canopy_cover: String,
    pub stem_density: String,
    pub stem_density_max_per_ha: f32,
    pub dominant_color: String,
}

/// Tile entry inside `midori_stand.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandTileManifest {
    pub tile: [i32; 2],
    pub origin_xz: [f32; 2],
    pub instances_json: String,
    pub instances_bin: String,
    pub lod0_meshes: BTreeMap<String, String>,
    pub lod1_mesh: String,
    pub lod2_mesh: String,
    pub rasters: StandTileRastersManifest,
    pub collision: String,
    pub stats: String,
}

/// Shared material slot specification in `midori_stand.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandMaterialSlot {
    pub name: String,
    pub alpha_mode: String,
    pub role: String,
}

/// Top-level volumetric stand package manifest (`midori_stand.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandManifest {
    pub schema_version: u32,
    pub kind: String,
    pub name: String,
    pub biome: String,
    pub seed: u64,
    pub tile_size_m: f32,
    pub foliage_mode: FoliageRenderMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor_preset: Option<String>,
    pub cross_lod_stitch_rule: String,
    pub shared_materials: Vec<StandMaterialSlot>,
    pub library: Vec<StandVariantProfile>,
    pub tiles: Vec<StandTileManifest>,
}

/// Budget constraints for stand generation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StandBudget {
    pub max_instances_per_tile: usize,
    pub max_triangles_lod0: u32,
    pub max_triangles_lod1: u32,
    pub max_triangles_lod2: u32,
    pub max_materials: u32,
}

impl Default for StandBudget {
    fn default() -> Self {
        Self {
            max_instances_per_tile: 300,
            max_triangles_lod0: 40_000,
            max_triangles_lod1: 4_000,
            max_triangles_lod2: 200,
            max_materials: 4,
        }
    }
}

/// Validate a package-relative path (rejects absolute paths and `..` traversal).
pub fn validate_stand_relative_path(rel: &str, label: &str) -> Result<PathBuf, StandError> {
    let p = Path::new(rel);
    if p.is_absolute() || rel.is_empty() {
        return Err(StandError::Validation(format!(
            "{label} must be a non-empty relative path, got {rel:?}"
        )));
    }
    for comp in p.components() {
        if matches!(
            comp,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err(StandError::Validation(format!(
                "{label} escapes package directory: {rel:?}"
            )));
        }
    }
    Ok(p.to_path_buf())
}

/// Encode binary stand instances (`midori.stand.instances.v1`).
pub fn encode_stand_instances_bin(instances: &[StandInstanceRecord]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(12 + instances.len() * STAND_INSTANCE_RECORD_BYTES);
    buf.extend_from_slice(STAND_INSTANCES_MAGIC);
    buf.extend_from_slice(&STAND_INSTANCES_VERSION.to_le_bytes());
    buf.extend_from_slice(&(instances.len() as u32).to_le_bytes());
    for inst in instances {
        buf.extend_from_slice(&inst.variant_index.to_le_bytes());
        buf.extend_from_slice(&inst.stratum_index.to_le_bytes());
        buf.push(match inst.tier {
            StandTier::Instance => 0,
            StandTier::Merged => 1,
        });
        buf.push(u8::from(inst.near_edge));
        for v in inst.position {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        buf.extend_from_slice(&inst.yaw.to_le_bytes());
        buf.extend_from_slice(&inst.scale.to_le_bytes());
        for c in inst.color_tint {
            buf.extend_from_slice(&c.to_le_bytes());
        }
    }
    buf
}

/// Summary returned by `validate_stand_package`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandValidationReport {
    pub name: String,
    pub tile_count: usize,
    pub library_variants: usize,
    pub total_instances: usize,
    pub max_materials_seen: usize,
    pub opaque_verified: bool,
    pub ok: bool,
}

/// Parse GLB JSON chunk and return `(triangle_count, material_count, has_alpha_mask_or_blend)`.
pub fn inspect_glb_summary(path: &Path) -> Result<(u32, usize, bool), StandError> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < 20 || &bytes[0..4] != b"glTF" {
        return Err(StandError::Validation(format!(
            "invalid GLB header: {}",
            path.display()
        )));
    }
    let json_len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    if &bytes[16..20] != b"JSON" || 20 + json_len > bytes.len() {
        return Err(StandError::Validation(format!(
            "corrupt GLB JSON chunk: {}",
            path.display()
        )));
    }
    let root: serde_json::Value = serde_json::from_slice(&bytes[20..20 + json_len])
        .map_err(|e| StandError::Validation(format!("GLB JSON parse {}: {e}", path.display())))?;

    let accessors = root["accessors"].as_array();
    let mut tris = 0u32;
    if let Some(meshes) = root["meshes"].as_array() {
        for mesh in meshes {
            if let Some(prims) = mesh["primitives"].as_array() {
                for prim in prims {
                    if let Some(idx_acc) = prim["indices"].as_u64()
                        && let Some(accs) = accessors
                        && let Some(acc) = accs.get(idx_acc as usize)
                        && let Some(cnt) = acc["count"].as_u64()
                    {
                        tris += (cnt / 3) as u32;
                    }
                }
            }
        }
    }

    let materials = root["materials"].as_array();
    let mat_count = materials.map(|m| m.len()).unwrap_or(0);
    let has_alpha = materials
        .map(|mats| {
            mats.iter().any(|m| {
                matches!(
                    m.get("alphaMode").and_then(|v| v.as_str()),
                    Some("MASK") | Some("BLEND")
                )
            })
        })
        .unwrap_or(false);

    Ok((tris, mat_count, has_alpha))
}

/// Validate a complete volumetric stand package directory (`midori validate-stand`).
pub fn validate_stand_package(directory: &Path) -> Result<StandValidationReport, StandError> {
    let manifest_path = directory.join("midori_stand.json");
    if !manifest_path.is_file() {
        return Err(StandError::Validation(format!(
            "missing midori_stand.json in {}",
            directory.display()
        )));
    }
    let manifest: StandManifest =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path)?)
            .map_err(|e| StandError::Validation(format!("invalid midori_stand.json: {e}")))?;

    if manifest.schema_version != STAND_MANIFEST_SCHEMA_VERSION {
        return Err(StandError::Validation(format!(
            "unsupported stand schema_version {}",
            manifest.schema_version
        )));
    }
    if manifest.kind != "nature_stand" {
        return Err(StandError::Validation(format!(
            "unexpected manifest.kind {:?}",
            manifest.kind
        )));
    }
    if manifest.shared_materials.len() > 4 {
        return Err(StandError::Validation(format!(
            "shared_materials exceeds limit 4 (got {})",
            manifest.shared_materials.len()
        )));
    }

    let require_opaque = manifest.foliage_mode == FoliageRenderMode::Opaque;
    let mut max_mats = manifest.shared_materials.len();

    // Validate plant library GLBs if present on disk
    for variant in &manifest.library {
        let rel = validate_stand_relative_path(&variant.glb_file, "library.glb_file")?;
        let full = directory.join(&rel);
        if full.is_file() {
            let (_, mats, has_alpha) = inspect_glb_summary(&full)?;
            max_mats = max_mats.max(mats);
            if require_opaque && has_alpha {
                return Err(StandError::Validation(format!(
                    "opaque mode violated: {} contains MASK/BLEND material",
                    variant.glb_file
                )));
            }
        }
    }

    let mut total_instances = 0usize;
    for tile in &manifest.tiles {
        let inst_json_rel =
            validate_stand_relative_path(&tile.instances_json, "tile.instances_json")?;
        let inst_bin_rel = validate_stand_relative_path(&tile.instances_bin, "tile.instances_bin")?;
        let lod1_rel = validate_stand_relative_path(&tile.lod1_mesh, "tile.lod1_mesh")?;
        let lod2_rel = validate_stand_relative_path(&tile.lod2_mesh, "tile.lod2_mesh")?;
        let col_rel = validate_stand_relative_path(&tile.collision, "tile.collision")?;
        let stats_rel = validate_stand_relative_path(&tile.stats, "tile.stats")?;

        let instances: Vec<StandInstanceRecord> =
            serde_json::from_str(&std::fs::read_to_string(directory.join(&inst_json_rel))?)
                .map_err(|e| StandError::Validation(format!("instances.json: {e}")))?;
        total_instances += instances.len();

        let bin = std::fs::read(directory.join(&inst_bin_rel))?;
        if bin.len() < 12 || &bin[0..4] != STAND_INSTANCES_MAGIC {
            return Err(StandError::Validation(format!(
                "bad instances.bin magic in {}",
                tile.instances_bin
            )));
        }
        let ver = u32::from_le_bytes([bin[4], bin[5], bin[6], bin[7]]);
        let cnt = u32::from_le_bytes([bin[8], bin[9], bin[10], bin[11]]) as usize;
        if ver != STAND_INSTANCES_VERSION
            || cnt != instances.len()
            || bin.len() != 12 + cnt * STAND_INSTANCE_RECORD_BYTES
        {
            return Err(StandError::Validation(format!(
                "instances.bin record mismatch in {} (json={}, bin={cnt})",
                tile.instances_bin,
                instances.len()
            )));
        }

        // Validate Stand LOD 0/1/2 GLBs
        let mut lod0_tris = 0u32;
        for (stratum, rel_s) in &tile.lod0_meshes {
            let rel = validate_stand_relative_path(rel_s, "tile.lod0_meshes")?;
            let (t, mats, has_alpha) = inspect_glb_summary(&directory.join(&rel))?;
            lod0_tris += t;
            max_mats = max_mats.max(mats);
            if require_opaque && has_alpha {
                return Err(StandError::Validation(format!(
                    "opaque mode violated in LOD0 stratum {stratum}: {rel_s}"
                )));
            }
        }
        let (lod1_tris, m1, a1) = inspect_glb_summary(&directory.join(&lod1_rel))?;
        let (lod2_tris, m2, a2) = inspect_glb_summary(&directory.join(&lod2_rel))?;
        max_mats = max_mats.max(m1).max(m2);
        if require_opaque && (a1 || a2) {
            return Err(StandError::Validation(
                "opaque mode violated in Stand LOD1/LOD2".to_string(),
            ));
        }

        // Validate rasters
        for (label, rel_s) in [
            ("canopy_height", &tile.rasters.canopy_height),
            ("canopy_cover", &tile.rasters.canopy_cover),
            ("stem_density", &tile.rasters.stem_density),
            ("dominant_color", &tile.rasters.dominant_color),
        ] {
            let rel = validate_stand_relative_path(rel_s, label)?;
            let img = image::open(directory.join(&rel))
                .map_err(|e| StandError::Validation(format!("raster {label} ({rel_s}): {e}")))?;
            if img.width() == 0 || img.height() == 0 {
                return Err(StandError::Validation(format!(
                    "raster {label} has zero size"
                )));
            }
        }

        // Validate collision.json & stand_stats.json
        let _col: StandCollisionFile =
            serde_json::from_str(&std::fs::read_to_string(directory.join(&col_rel))?)
                .map_err(|e| StandError::Validation(format!("collision.json: {e}")))?;
        let stats: StandTileStats =
            serde_json::from_str(&std::fs::read_to_string(directory.join(&stats_rel))?)
                .map_err(|e| StandError::Validation(format!("stand_stats.json: {e}")))?;

        if stats.triangles_lod0 != lod0_tris
            || stats.triangles_lod1 != lod1_tris
            || stats.triangles_lod2 != lod2_tris
        {
            return Err(StandError::Validation(format!(
                "stand_stats.json triangle counts do not match GLBs (stats={}/{}/{}, glb={lod0_tris}/{lod1_tris}/{lod2_tris})",
                stats.triangles_lod0, stats.triangles_lod1, stats.triangles_lod2
            )));
        }
    }

    Ok(StandValidationReport {
        name: manifest.name,
        tile_count: manifest.tiles.len(),
        library_variants: manifest.library.len(),
        total_instances,
        max_materials_seen: max_mats,
        opaque_verified: require_opaque,
        ok: true,
    })
}
