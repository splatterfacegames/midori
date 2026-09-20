//! In-process source-bound composition, not a serialized application protocol.
use midori_core::{
    ExportConfig, ExportMetadata, LodGenerationConfig, LodMeshSet, Species,
    export_lod_meshes_to_bytes, generate_lod_meshes_with_config, generate_tree,
};
use sha2::{Digest, Sha256};

/// Trusted host/build provenance; this value does not attest to its own truth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineIdentity {
    pub source_revision: String,
    pub build_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreeSummary {
    pub species_name: String,
    pub scientific_name: String,
    pub stem_count: usize,
    pub branch_count: usize,
    pub leaf_count: usize,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
}

#[derive(Debug)]
pub enum TreeError {
    Seed,
    Utf8(std::str::Utf8Error),
    Species(String),
    Generation(midori_core::GenerationError),
    Export(midori_core::ExportError),
}

impl std::fmt::Display for TreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Seed => f.write_str("seed must be a canonical unsigned decimal u64"),
            Self::Utf8(error) => write!(f, "source is not UTF-8: {error}"),
            Self::Species(error) => write!(f, "species TOML: {error}"),
            Self::Generation(error) => write!(f, "generation budget: {error}"),
            Self::Export(error) => write!(f, "GLB export: {error}"),
        }
    }
}

impl std::error::Error for TreeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Utf8(error) => Some(error),
            Self::Generation(error) => Some(error),
            Self::Export(error) => Some(error),
            Self::Seed | Self::Species(_) => None,
        }
    }
}

/// Fields are private so consumers cannot detach exported bytes from their inputs.
#[derive(Debug)]
pub struct TreeResult {
    source: Vec<u8>,
    source_sha256: [u8; 32],
    seed: u64,
    engine: EngineIdentity,
    species: Species,
    lod_config: LodGenerationConfig,
    summary: TreeSummary,
    lods: LodMeshSet,
    glb: Vec<u8>,
    glb_sha256: [u8; 32],
}

impl TreeResult {
    pub fn source(&self) -> &[u8] {
        &self.source
    }
    pub fn source_sha256(&self) -> &[u8; 32] {
        &self.source_sha256
    }
    pub fn seed(&self) -> u64 {
        self.seed
    }
    pub fn seed_decimal(&self) -> String {
        self.seed.to_string()
    }
    pub fn engine(&self) -> &EngineIdentity {
        &self.engine
    }
    pub fn species(&self) -> &Species {
        &self.species
    }
    pub fn lod_config(&self) -> &LodGenerationConfig {
        &self.lod_config
    }
    pub fn summary(&self) -> &TreeSummary {
        &self.summary
    }
    pub fn lods(&self) -> &LodMeshSet {
        &self.lods
    }
    pub fn glb(&self) -> &[u8] {
        &self.glb
    }
    pub fn glb_sha256(&self) -> &[u8; 32] {
        &self.glb_sha256
    }
    pub fn glb_byte_count(&self) -> u64 {
        self.glb.len() as u64
    }
}

/// Compose using the same balanced defaults as the existing WASM tree workflow.
///
/// No filesystem, network, persistence, background execution or cancellation.
/// Callers own untrusted-input resource admission before invoking core generation.
///
/// # Errors
///
/// Returns [`TreeError::Seed`] unless `seed_decimal` is the canonical decimal
/// form of a `u64`, [`TreeError::Utf8`] when `source` is not UTF-8,
/// [`TreeError::Species`] when the source is not valid species TOML and
/// [`TreeError::Export`] when GLB export fails. No partial result is built.
pub fn compose_tree(
    source: &[u8],
    seed_decimal: &str,
    engine: EngineIdentity,
) -> Result<TreeResult, TreeError> {
    let seed = seed_decimal.parse::<u64>().map_err(|_| TreeError::Seed)?;
    if seed.to_string() != seed_decimal {
        return Err(TreeError::Seed);
    }
    let text = std::str::from_utf8(source).map_err(TreeError::Utf8)?;
    let species = Species::from_toml(text).map_err(|e| TreeError::Species(e.to_string()))?;
    let tree = generate_tree(&species, seed).map_err(TreeError::Generation)?;
    let lod_config = LodGenerationConfig::balanced();
    let lods = generate_lod_meshes_with_config(&tree, &species, &lod_config);
    let summary = TreeSummary {
        species_name: species.species.name.clone(),
        scientific_name: species.latin_name().into(),
        stem_count: tree.stems.len(),
        branch_count: tree.stems.iter().filter(|stem| stem.level > 0).count(),
        leaf_count: tree.leaves.len(),
        bounds_min: tree.bounds.min.to_array(),
        bounds_max: tree.bounds.max.to_array(),
    };
    let export = ExportConfig {
        metadata: Some(ExportMetadata {
            species_name: summary.species_name.clone(),
            scientific_name: summary.scientific_name.clone(),
            seed: Some(seed),
            lod_screen_heights: lods.meshes.iter().map(|lod| lod.screen_height).collect(),
        }),
        ..ExportConfig::default()
    };
    let glb = export_lod_meshes_to_bytes(&lods, &export).map_err(TreeError::Export)?;
    Ok(TreeResult {
        source: source.to_vec(),
        source_sha256: Sha256::digest(source).into(),
        seed,
        engine,
        species,
        lod_config,
        summary,
        lods,
        glb_sha256: Sha256::digest(&glb).into(),
        glb,
    })
}
