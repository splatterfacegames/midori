//! The tree slice's command boundary.
//!
//! Everything crossing to the frontend that describes a computation crosses as
//! the generated FlatBuffers contract: a `TreeComputeRequest` in, a
//! `TreeComputeResult` out. There is no second hand-maintained JSON protocol.
//!
//! The GLB itself never enters that envelope. `compute` keeps the bytes in a
//! bounded in-process store keyed by their SHA-256 and puts an `ArtifactRef`
//! (digest + byte count) in the result; `artifact` hands the bytes back only to
//! a caller that names the right digest, and re-checks the digest before doing
//! so.
//!
//! This is pure in-process computation. There is no durable project, no
//! committed revision, no job queue and no cancellation. Ordering across
//! concurrent requests is the caller's problem, and the frontend solves it by
//! matching `source_sha256` and `seed` against the input it currently holds.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use midori_contracts::{
    ARTIFACT_KIND_GLB, ArtifactRefInput, EngineIdentityInput, LodLevelInput, MaterialRole,
    SubmeshRangeInput, TreeComputeResultInput, TreeSummaryInput, decode_request, encode_result,
};
use midori_ui_domain::{EngineIdentity, TreeResult, compose_tree};
use sha2::{Digest, Sha256};

/// Species TOML shipped with the application, embedded so the running binary
/// serves the exact bytes that were built into it rather than whatever happens
/// to be on disk next to it.
pub const PRESETS: &[(&str, &[u8])] = &[
    ("oak", include_bytes!("../../../presets/species/oak.toml")),
    ("pine", include_bytes!("../../../presets/species/pine.toml")),
    ("palm", include_bytes!("../../../presets/species/palm.toml")),
    (
        "willow",
        include_bytes!("../../../presets/species/willow.toml"),
    ),
    (
        "joshua_prototype",
        include_bytes!("../../../presets/species/joshua_prototype.toml"),
    ),
];

/// How many generated GLBs stay resident. A tree is a few megabytes, and a user
/// scrubbing a seed produces one per keystroke-with-generate; without a bound
/// the store is an unbounded allocation driven by UI input.
const MAX_RESIDENT_ARTIFACTS: usize = 6;

/// Refuse to read back anything implausible as a tree export.
const MAX_VERIFIED_FILE_BYTES: u64 = 256 * 1024 * 1024;

/// Largest species TOML the compute path accepts. Generation cost rises with
/// the source, and this is an untrusted input from the renderer.
const MAX_SOURCE_BYTES: usize = 1024 * 1024;

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// One generated GLB, held only long enough for the frontend to fetch or export
/// it. The LOD record is kept so a reopened file can be checked against what
/// the engine said it produced, rather than merely parsed.
///
/// A LOD has two distinct names and they are not interchangeable. `lod_names`
/// holds the engine's own threshold names — `High`, `Medium`, `Low` — which are
/// what the contract's `LodLevel::name` carries. The exported GLB instead names
/// its nodes `<sanitized species>_LOD<index>`. Verifying a reopened file against
/// the engine names would always fail; the index is the identity the two share.
struct Artifact {
    sha256: String,
    glb: Vec<u8>,
    species_name: String,
    seed: u64,
    lod_indices: Vec<u32>,
    lod_names: Vec<String>,
}

#[derive(Default)]
struct ArtifactStore {
    entries: VecDeque<Artifact>,
}

impl ArtifactStore {
    fn insert(&mut self, artifact: Artifact) {
        self.entries.retain(|entry| entry.sha256 != artifact.sha256);
        self.entries.push_back(artifact);
        while self.entries.len() > MAX_RESIDENT_ARTIFACTS {
            self.entries.pop_front();
        }
    }

    fn get(&self, sha256: &str) -> Option<&Artifact> {
        self.entries.iter().find(|entry| entry.sha256 == sha256)
    }
}

pub struct TreeState {
    engine: EngineIdentity,
    artifacts: Mutex<ArtifactStore>,
}

impl TreeState {
    #[must_use]
    pub fn new(engine: EngineIdentity) -> Self {
        Self {
            engine,
            artifacts: Mutex::new(ArtifactStore::default()),
        }
    }
}

/// Turn a composed result into the plain-data form of the wire contract.
///
/// Every number here is read off the same generation that produced the GLB the
/// `ArtifactRef` points at, so a consumer that trusts `source_sha256` can trust
/// the counts beside it.
pub fn result_input(result: &TreeResult, engine: &EngineIdentity) -> TreeComputeResultInput {
    let summary = result.summary();
    TreeComputeResultInput {
        source_sha256: hex(result.source_sha256()),
        seed: result.seed(),
        engine: EngineIdentityInput {
            source_revision: engine.source_revision.clone(),
            build_id: engine.build_id.clone(),
        },
        summary: TreeSummaryInput {
            species_name: summary.species_name.clone(),
            scientific_name: summary.scientific_name.clone(),
            stem_count: summary.stem_count as u64,
            branch_count: summary.branch_count as u64,
            leaf_count: summary.leaf_count as u64,
            bounds_min: summary.bounds_min,
            bounds_max: summary.bounds_max,
        },
        lods: result
            .lods()
            .meshes
            .iter()
            .map(|lod| LodLevelInput {
                index: lod.index,
                name: lod.name.clone(),
                screen_height: lod.screen_height,
                vertex_count: lod.mesh.vertices.len() as u32,
                index_count: lod.mesh.indices.len() as u32,
                submeshes: lod
                    .mesh
                    .submeshes
                    .iter()
                    .map(|submesh| SubmeshRangeInput {
                        // Bark and Leaves are the only roles public
                        // `midori_core::MaterialType` has. The legacy frontend's
                        // Twig and Frond labels are not engine variants, so
                        // there is nothing here to map them onto.
                        material: match submesh.material {
                            midori_core::MaterialType::Bark => MaterialRole::Bark,
                            midori_core::MaterialType::Leaves => MaterialRole::Leaves,
                        },
                        index_offset: submesh.index_start,
                        index_count: submesh.index_count,
                    })
                    .collect(),
            })
            .collect(),
        glb: ArtifactRefInput {
            kind: ARTIFACT_KIND_GLB.into(),
            sha256: hex(result.glb_sha256()),
            byte_count: result.glb_byte_count(),
        },
    }
}

/// Decode a request, generate, store the GLB, and encode the result.
///
/// Kept free of `tauri` so it can be tested without a running application.
///
/// # Errors
///
/// Returns a display string for a malformed envelope, an oversized source, a
/// non-canonical seed, invalid species TOML or a failed GLB export. No partial
/// result is stored: a failure leaves the artifact store exactly as it was.
pub fn compute(state: &TreeState, request: &[u8]) -> Result<Vec<u8>, String> {
    let request = decode_request(request).map_err(|error| error.to_string())?;
    let source = request.source().bytes();
    if source.len() > MAX_SOURCE_BYTES {
        return Err(format!(
            "source is {} bytes, over the {MAX_SOURCE_BYTES} byte limit",
            source.len()
        ));
    }

    // The seed is a `ulong` on the wire and reaches `compose_tree` as the
    // canonical decimal string it insists on, so no value is lost or widened
    // anywhere along the path.
    let seed_decimal = request.seed().to_string();
    let result = compose_tree(source, &seed_decimal, state.engine.clone())
        .map_err(|error| error.to_string())?;

    let input = result_input(&result, &state.engine);
    state
        .artifacts
        .lock()
        .map_err(|_| "artifact store is poisoned".to_string())?
        .insert(Artifact {
            sha256: input.glb.sha256.clone(),
            glb: result.glb().to_vec(),
            species_name: input.summary.species_name.clone(),
            seed: input.seed,
            lod_indices: input.lods.iter().map(|lod| lod.index).collect(),
            lod_names: input.lods.iter().map(|lod| lod.name.clone()).collect(),
        });

    Ok(encode_result(&input))
}

/// Hand back GLB bytes to a caller that names their digest.
///
/// The digest is recomputed before returning, so a corrupted store entry is
/// caught here rather than becoming a corrupted export.
///
/// # Errors
///
/// Returns a display string when no resident artifact has that digest, or when
/// the resident bytes no longer hash to it.
pub fn artifact(state: &TreeState, sha256: &str) -> Result<Vec<u8>, String> {
    let store = state
        .artifacts
        .lock()
        .map_err(|_| "artifact store is poisoned".to_string())?;
    let artifact = store
        .get(sha256)
        .ok_or_else(|| format!("no resident artifact with digest {sha256}"))?;
    let actual = hex(&Sha256::digest(&artifact.glb));
    if actual != sha256 {
        return Err(format!(
            "resident artifact hashes to {actual}, not {sha256}"
        ));
    }
    Ok(artifact.glb.clone())
}

/// Sanitize a user-supplied export name down to one path component.
fn export_file_name(requested: &str) -> String {
    let stem: String = requested
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let stem = stem.trim_matches('_');
    let stem = if stem.is_empty() { "tree" } else { stem };
    format!("{}.glb", &stem[..stem.len().min(96)])
}

/// Write the artifact to `directory` and return the absolute path written.
///
/// # Errors
///
/// Returns a display string when the artifact is not resident, the directory
/// cannot be created, or the write fails.
pub fn export(
    state: &TreeState,
    sha256: &str,
    requested_name: &str,
    directory: &Path,
) -> Result<PathBuf, String> {
    let bytes = artifact(state, sha256)?;
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("create {}: {error}", directory.display()))?;
    let path = directory.join(export_file_name(requested_name));
    std::fs::write(&path, &bytes).map_err(|error| format!("write {}: {error}", path.display()))?;
    Ok(path)
}

/// What reopening an exported file actually found in it.
pub struct Inspection {
    pub path: String,
    pub byte_count: u64,
    pub sha256: String,
    pub digest_matches: bool,
    pub generator: String,
    pub scene_name: String,
    /// One entry per node: name, primitive count, per-primitive material names,
    /// vertex and index totals.
    pub nodes: Vec<InspectedNode>,
    pub materials: Vec<String>,
    /// `extras.midori` from the glTF root, if present.
    pub extras_midori: Option<serde_json::Value>,
    pub uses_msft_lod: bool,
    /// LOD indices the engine reported that no node in the file carries.
    pub missing_lod_nodes: Vec<String>,
    /// The engine's own LOD names, for the report. These are threshold names
    /// (`High`, `Medium`, `Low`), not the node names in the file.
    pub engine_lod_names: Vec<String>,
}

pub struct InspectedNode {
    pub name: String,
    pub primitive_count: usize,
    pub materials: Vec<String>,
    pub vertex_count: usize,
    pub index_count: usize,
    pub attributes: Vec<String>,
}

/// Reopen a file from disk and report what is in it.
///
/// This is the "and verify its contents" half of the workflow, and it
/// deliberately does not consult the in-memory result for the parse: it reads
/// the bytes back off the filesystem, re-digests them, and parses the GLB. The
/// in-memory record is used only to say which LOD node names *should* be there.
///
/// # Errors
///
/// Returns a display string when the path cannot be read, is implausibly large,
/// or does not parse as a GLB.
pub fn inspect(state: &TreeState, sha256: &str, path: &Path) -> Result<Inspection, String> {
    let metadata =
        std::fs::metadata(path).map_err(|error| format!("stat {}: {error}", path.display()))?;
    if metadata.len() > MAX_VERIFIED_FILE_BYTES {
        return Err(format!(
            "{} is {} bytes, over the {MAX_VERIFIED_FILE_BYTES} byte limit",
            path.display(),
            metadata.len()
        ));
    }
    let bytes =
        std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let actual = hex(&Sha256::digest(&bytes));

    let (expected_lod_indices, engine_lod_names) = state
        .artifacts
        .lock()
        .map_err(|_| "artifact store is poisoned".to_string())?
        .get(sha256)
        .map(|artifact| (artifact.lod_indices.clone(), artifact.lod_names.clone()))
        .unwrap_or_default();

    let gltf = gltf::Gltf::from_slice(&bytes)
        .map_err(|error| format!("{} is not a readable GLB: {error}", path.display()))?;
    let document = &gltf.document;

    let materials: Vec<String> = document
        .materials()
        .map(|material| material.name().unwrap_or("<unnamed>").to_owned())
        .collect();

    let mut nodes = Vec::new();
    for node in document.nodes() {
        let Some(mesh) = node.mesh() else { continue };
        let mut node_materials = Vec::new();
        let mut vertex_count = 0usize;
        let mut index_count = 0usize;
        let mut attributes: Vec<String> = Vec::new();
        let mut primitive_count = 0usize;
        for primitive in mesh.primitives() {
            primitive_count += 1;
            node_materials.push(
                primitive
                    .material()
                    .name()
                    .unwrap_or("<default>")
                    .to_owned(),
            );
            for (semantic, accessor) in primitive.attributes() {
                let name = format!("{semantic:?}");
                if !attributes.contains(&name) {
                    attributes.push(name);
                }
                if semantic == gltf::Semantic::Positions {
                    vertex_count = vertex_count.max(accessor.count());
                }
            }
            if let Some(indices) = primitive.indices() {
                index_count += indices.count();
            }
        }
        nodes.push(InspectedNode {
            name: node.name().unwrap_or("<unnamed>").to_owned(),
            primitive_count,
            materials: node_materials,
            vertex_count,
            index_count,
            attributes,
        });
    }

    // The exporter names nodes `<sanitized species>_LOD<index>`, so the index is
    // the identity a reopened file and the in-memory result actually share.
    let missing_lod_nodes = expected_lod_indices
        .iter()
        .map(|index| format!("_LOD{index}"))
        .filter(|suffix| !nodes.iter().any(|node| node.name.ends_with(suffix)))
        .collect();

    // The exporter names `<Species>_LOD<n>` nodes and records thresholds in root
    // extras. It does not emit an MSFT_lod hierarchy, and reporting one would be
    // fabricating a capability.
    let uses_msft_lod = document
        .extensions_used()
        .any(|extension| extension == "MSFT_lod");

    let root: serde_json::Value = serde_json::from_slice(&glb_json_chunk(&bytes)?)
        .map_err(|error| format!("glTF JSON chunk: {error}"))?;

    Ok(Inspection {
        path: path.display().to_string(),
        byte_count: metadata.len(),
        digest_matches: actual == sha256,
        sha256: actual,
        generator: root["asset"]["generator"]
            .as_str()
            .unwrap_or("<none>")
            .to_owned(),
        scene_name: document
            .default_scene()
            .and_then(|scene| scene.name().map(str::to_owned))
            .unwrap_or_else(|| "<unnamed>".into()),
        nodes,
        materials,
        extras_midori: root
            .get("extras")
            .and_then(|extras| extras.get("midori"))
            .cloned(),
        uses_msft_lod,
        missing_lod_nodes,
        engine_lod_names,
    })
}

/// Pull the JSON chunk out of a binary glTF container.
///
/// `gltf::Gltf` exposes the typed document but drops unrecognized root extras,
/// and the exporter's LOD thresholds live exactly there.
fn glb_json_chunk(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() < 20 || &bytes[0..4] != b"glTF" {
        return Err("not a binary glTF container".into());
    }
    let chunk_length = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    if &bytes[16..20] != b"JSON" {
        return Err("first GLB chunk is not JSON".into());
    }
    bytes
        .get(20..20 + chunk_length)
        .map(<[u8]>::to_vec)
        .ok_or_else(|| "GLB JSON chunk runs past the end of the file".into())
}

impl Inspection {
    /// A human-readable report. The frontend renders this verbatim rather than
    /// re-deriving anything: inventing a second structured protocol for an
    /// inspection would be exactly the duplication the contract rules forbid.
    #[must_use]
    pub fn report(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("reopened {}\n", self.path));
        out.push_str(&format!("  {} bytes on disk\n", self.byte_count));
        out.push_str(&format!(
            "  sha256 {} ({})\n",
            self.sha256,
            if self.digest_matches {
                "matches the ArtifactRef in the result"
            } else {
                "DOES NOT MATCH the ArtifactRef in the result"
            }
        ));
        out.push_str(&format!("  asset.generator {}\n", self.generator));
        out.push_str(&format!("  scene {}\n", self.scene_name));
        out.push_str(&format!("  materials {}\n", self.materials.join(", ")));
        out.push_str(&format!(
            "  LOD representation: named nodes{}\n",
            if self.uses_msft_lod {
                " plus an MSFT_lod hierarchy"
            } else {
                ", no MSFT_lod extension (as the exporter intends)"
            }
        ));
        for node in &self.nodes {
            out.push_str(&format!(
                "  node {}: {} primitive(s) [{}], {} vertices, {} indices, attributes {}\n",
                node.name,
                node.primitive_count,
                node.materials.join(" + "),
                node.vertex_count,
                node.index_count,
                node.attributes.join(" ")
            ));
        }
        match &self.extras_midori {
            Some(extras) => out.push_str(&format!("  extras.midori {extras}\n")),
            None => out.push_str("  extras.midori absent\n"),
        }
        if self.missing_lod_nodes.is_empty() {
            out.push_str("  every LOD the engine reported is present in the file\n");
        } else {
            out.push_str(&format!(
                "  MISSING LOD nodes: {}\n",
                self.missing_lod_nodes.join(", ")
            ));
        }
        out.push_str(&format!(
            "  note: the engine's LOD names are {} - the file names its nodes\n         \
             <species>_LOD<index> instead, so index is the shared identity\n",
            self.engine_lod_names.join(", ")
        ));
        out
    }
}

/// The default file name offered for an export, from what the engine actually
/// generated rather than from whatever the source file happened to be called.
#[must_use]
pub fn suggested_name(state: &TreeState, sha256: &str) -> String {
    let store = match state.artifacts.lock() {
        Ok(store) => store,
        Err(_) => return "tree".into(),
    };
    match store.get(sha256) {
        Some(artifact) => format!("{}_{}", artifact.species_name, artifact.seed),
        None => "tree".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use midori_contracts::{TreeComputeRequestInput, decode_result, encode_request};

    fn state() -> TreeState {
        TreeState::new(EngineIdentity {
            source_revision: "test".into(),
            build_id: "test".into(),
        })
    }

    fn preset(id: &str) -> &'static [u8] {
        PRESETS
            .iter()
            .find(|(name, _)| *name == id)
            .map(|(_, bytes)| *bytes)
            .expect("preset")
    }

    fn request(source: &[u8], seed: u64) -> Vec<u8> {
        encode_request(&TreeComputeRequestInput {
            source: source.to_vec(),
            seed,
            display_name: None,
        })
    }

    #[test]
    fn every_embedded_preset_generates() {
        let state = state();
        for (id, source) in PRESETS {
            let bytes = compute(&state, &request(source, 42))
                .unwrap_or_else(|error| panic!("{id}: {error}"));
            let result = decode_result(&bytes).unwrap();
            assert_eq!(result.seed(), 42);
            assert!(
                !result.summary().species_name().is_empty(),
                "{id} has no species name"
            );
            assert!(result.lods().len() > 0, "{id} produced no LODs");
            assert!(result.glb().byte_count() > 0, "{id} exported no bytes");
            assert_eq!(result.glb().kind(), ARTIFACT_KIND_GLB);
        }
    }

    #[test]
    fn the_result_agrees_with_a_direct_call_to_the_engine() {
        // The contract must describe the same generation the adapter composed,
        // not a plausible-looking summary of it.
        let state = state();
        let source = preset("oak");
        let bytes = compute(&state, &request(source, 12345)).unwrap();
        let result = decode_result(&bytes).unwrap();

        let direct = compose_tree(
            source,
            "12345",
            EngineIdentity {
                source_revision: "test".into(),
                build_id: "test".into(),
            },
        )
        .unwrap();

        assert_eq!(result.source_sha256(), hex(direct.source_sha256()));
        assert_eq!(result.glb().sha256(), hex(direct.glb_sha256()));
        assert_eq!(result.glb().byte_count(), direct.glb_byte_count());
        assert_eq!(
            result.summary().species_name(),
            direct.summary().species_name
        );
        assert_eq!(result.summary().leaf_count(), direct.summary().leaf_count as u64);
        assert_eq!(result.lods().len(), direct.lods().meshes.len());
        for (index, lod) in direct.lods().meshes.iter().enumerate() {
            let wire = result.lods().get(index);
            assert_eq!(wire.index(), lod.index);
            assert_eq!(wire.name(), lod.name);
            assert_eq!(wire.screen_height(), lod.screen_height);
            assert_eq!(wire.vertex_count() as usize, lod.mesh.vertices.len());
            assert_eq!(wire.index_count() as usize, lod.mesh.indices.len());
            assert_eq!(wire.submeshes().len(), lod.mesh.submeshes.len());
            for (slot, submesh) in lod.mesh.submeshes.iter().enumerate() {
                let range = wire.submeshes().get(slot);
                assert_eq!(range.index_offset(), submesh.index_start);
                assert_eq!(range.index_count(), submesh.index_count);
                assert_eq!(
                    range.material(),
                    match submesh.material {
                        midori_core::MaterialType::Bark => MaterialRole::Bark,
                        midori_core::MaterialType::Leaves => MaterialRole::Leaves,
                    }
                );
                // Ranges must actually address the mesh they describe.
                assert!(
                    (range.index_offset() + range.index_count()) as usize
                        <= lod.mesh.indices.len(),
                    "submesh range runs past the index buffer"
                );
            }
        }
    }

    #[test]
    fn only_bark_and_leaves_appear_as_material_roles() {
        let state = state();
        for (_, source) in PRESETS {
            let bytes = compute(&state, &request(source, 7)).unwrap();
            let result = decode_result(&bytes).unwrap();
            for lod in 0..result.lods().len() {
                let lod = result.lods().get(lod);
                for slot in 0..lod.submeshes().len() {
                    let material = lod.submeshes().get(slot).material();
                    assert!(
                        material == MaterialRole::Bark || material == MaterialRole::Leaves,
                        "unexpected material role {material:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_seed_survives_the_full_width_of_a_u64() {
        let state = state();
        let bytes = compute(&state, &request(preset("pine"), u64::MAX)).unwrap();
        let result = decode_result(&bytes).unwrap();
        assert_eq!(result.seed(), u64::MAX);
        // 2^64 - 1 is not representable as an f64; a lossy path would land on
        // 18446744073709551616 instead.
        assert_eq!(result.seed().to_string(), "18446744073709551615");
    }

    #[test]
    fn different_seeds_give_different_artifacts_and_the_same_seed_repeats() {
        let state = state();
        let a = decode_result(&compute(&state, &request(preset("oak"), 1)).unwrap())
            .unwrap()
            .glb()
            .sha256()
            .to_owned();
        let b = decode_result(&compute(&state, &request(preset("oak"), 2)).unwrap())
            .unwrap()
            .glb()
            .sha256()
            .to_owned();
        let a_again = decode_result(&compute(&state, &request(preset("oak"), 1)).unwrap())
            .unwrap()
            .glb()
            .sha256()
            .to_owned();
        assert_ne!(a, b, "two seeds produced the same GLB");
        assert_eq!(a, a_again, "the same seed did not reproduce");
    }

    #[test]
    fn invalid_input_is_an_error_and_stores_nothing() {
        let state = state();
        let good = decode_result(&compute(&state, &request(preset("oak"), 5)).unwrap())
            .unwrap()
            .glb()
            .sha256()
            .to_owned();

        let error = compute(&state, &request(b"this is not species TOML", 5)).unwrap_err();
        assert!(error.contains("species TOML"), "unexpected error: {error}");

        // The previous artifact is untouched, but nothing new was admitted; an
        // error must not be able to present the last good result as its own.
        assert!(artifact(&state, &good).is_ok());
        let store = state.artifacts.lock().unwrap();
        assert_eq!(store.entries.len(), 1);
    }

    #[test]
    fn a_malformed_envelope_is_rejected_rather_than_guessed_at() {
        let state = state();
        for bad in [
            vec![],
            vec![0u8; 4],
            b"not a flatbuffer at all".to_vec(),
            vec![0xffu8; 64],
        ] {
            assert!(compute(&state, &bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn artifact_bytes_come_back_only_for_the_right_digest() {
        let state = state();
        let bytes = compute(&state, &request(preset("oak"), 99)).unwrap();
        let result = decode_result(&bytes).unwrap();
        let digest = result.glb().sha256();

        let glb = artifact(&state, digest).unwrap();
        assert_eq!(glb.len() as u64, result.glb().byte_count());
        assert_eq!(hex(&Sha256::digest(&glb)), digest);
        assert_eq!(&glb[0..4], b"glTF");

        assert!(artifact(&state, &"0".repeat(64)).is_err());
        // The control envelope stayed a control envelope.
        assert!(
            bytes.len() < 4096,
            "the result envelope is {} bytes; the GLB is {}",
            bytes.len(),
            glb.len()
        );
    }

    #[test]
    fn the_store_stays_bounded_and_evicts_the_oldest() {
        let state = state();
        let mut digests = Vec::new();
        for seed in 0..(MAX_RESIDENT_ARTIFACTS as u64 + 3) {
            let bytes = compute(&state, &request(preset("oak"), seed)).unwrap();
            digests.push(decode_result(&bytes).unwrap().glb().sha256().to_owned());
        }
        assert_eq!(
            state.artifacts.lock().unwrap().entries.len(),
            MAX_RESIDENT_ARTIFACTS
        );
        assert!(artifact(&state, &digests[0]).is_err(), "oldest survived");
        assert!(artifact(&state, digests.last().unwrap()).is_ok());
    }

    #[test]
    fn an_exported_file_reopens_with_the_lods_and_materials_the_result_promised() {
        let state = state();
        let bytes = compute(&state, &request(preset("oak"), 4242)).unwrap();
        let result = decode_result(&bytes).unwrap();
        let digest = result.glb().sha256();

        let dir = std::env::temp_dir().join("midori-app-export-test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = export(&state, digest, "Oak 4242", &dir).unwrap();
        assert_eq!(path.file_name().unwrap(), "Oak_4242.glb");

        let inspection = inspect(&state, digest, &path).unwrap();
        // Shown with `-- --nocapture`; this is the same text the Output panel
        // renders after an export.
        println!("{}", inspection.report());
        assert!(inspection.digest_matches, "{}", inspection.report());
        assert_eq!(inspection.byte_count, result.glb().byte_count());
        assert_eq!(inspection.generator, "midori");
        assert!(inspection.missing_lod_nodes.is_empty(), "{:?}", inspection.missing_lod_nodes);
        assert!(
            !inspection.uses_msft_lod,
            "the exporter does not emit MSFT_lod; something changed"
        );

        // Materials in the file are the two core roles, named as the exporter
        // names them.
        assert_eq!(inspection.materials, vec!["bark", "leaves"]);

        // One node per LOD, named `<Species>_LOD<n>`, carrying real geometry.
        // The engine's own LOD names are threshold names and are deliberately
        // NOT what the file uses; conflating them is how a verifier ends up
        // proving nothing.
        assert_eq!(
            inspection.engine_lod_names,
            vec!["High", "Medium", "Low"],
            "the balanced LOD names changed"
        );
        assert_eq!(inspection.nodes.len(), result.lods().len());
        for index in 0..result.lods().len() {
            let lod = result.lods().get(index);
            let node = &inspection.nodes[index];
            assert!(
                node.name.ends_with(&format!("_LOD{}", lod.index())),
                "node {} does not name LOD {}",
                node.name,
                lod.index()
            );
            assert_eq!(
                node.name,
                format!("{}_LOD{}", result.summary().species_name(), lod.index()),
                "the node is not named after the species and LOD index"
            );
            assert_ne!(
                node.name,
                lod.name(),
                "the file used the engine's LOD name; the two schemes converged"
            );
            assert_eq!(
                node.primitive_count,
                lod.submeshes().len(),
                "{} primitive count",
                node.name
            );
            assert_eq!(node.index_count as u32, lod.index_count());
            // Attributes the frontend's wind and shading depend on must survive
            // the export, not just positions.
            for required in ["Positions", "Normals", "TexCoords(0)", "TexCoords(1)", "Colors(0)"] {
                assert!(
                    node.attributes.iter().any(|a| a == required),
                    "{} is missing {required}; has {:?}",
                    node.name,
                    node.attributes
                );
            }
        }

        // Root extras carry the LOD thresholds and the source-derived metadata.
        let extras = inspection.extras_midori.as_ref().expect("extras.midori");
        assert_eq!(extras["seed"], serde_json::json!(4242));
        assert_eq!(
            extras["species_name"],
            serde_json::json!(result.summary().species_name())
        );
        let thresholds = extras["lod_screen_heights"].as_array().unwrap();
        assert_eq!(thresholds.len(), result.lods().len());
        for index in 0..result.lods().len() {
            let recorded = thresholds[index].as_f64().unwrap() as f32;
            assert_eq!(recorded, result.lods().get(index).screen_height());
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tampered_export_is_caught_on_reopen() {
        let state = state();
        let bytes = compute(&state, &request(preset("oak"), 8)).unwrap();
        let digest = decode_result(&bytes).unwrap().glb().sha256().to_owned();

        let dir = std::env::temp_dir().join("midori-app-tamper-test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = export(&state, &digest, "tamper", &dir).unwrap();

        let mut file = std::fs::read(&path).unwrap();
        let last = file.len() - 1;
        file[last] ^= 0xff;
        std::fs::write(&path, &file).unwrap();

        let inspection = inspect(&state, &digest, &path).unwrap();
        assert!(
            !inspection.digest_matches,
            "a modified file reported as matching"
        );
        assert!(inspection.report().contains("DOES NOT MATCH"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_names_cannot_escape_the_directory() {
        assert_eq!(export_file_name("../../etc/passwd"), "etc_passwd.glb");
        assert_eq!(export_file_name("C:\\Windows\\system32"), "C__Windows_system32.glb");
        assert_eq!(export_file_name(""), "tree.glb");
        assert_eq!(export_file_name("..."), "tree.glb");
        assert!(!export_file_name(&"x".repeat(400)).contains('/'));
        assert!(export_file_name(&"x".repeat(400)).len() <= 100);
    }

    /// Emits the cross-language fixture the TypeScript tests decode.
    ///
    /// Run with `MIDORI_WRITE_FIXTURES=1` to regenerate. Otherwise it asserts
    /// that the checked-in fixture is still exactly what this Rust encoder
    /// produces, so the TS side is decoding real engine output rather than
    /// something hand-written to match its own reader.
    #[test]
    fn the_typescript_fixture_is_this_encoder_s_own_output() {
        let state = state();
        let bytes = compute(&state, &request(preset("oak"), 1_000_003)).unwrap();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/tree-compute-result.bin");
        if std::env::var("MIDORI_WRITE_FIXTURES").is_ok() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &bytes).unwrap();
        }
        let checked_in = std::fs::read(&path).unwrap_or_else(|error| {
            panic!(
                "{}: {error}. Re-run with MIDORI_WRITE_FIXTURES=1 to create it.",
                path.display()
            )
        });
        assert_eq!(
            checked_in, bytes,
            "the checked-in fixture no longer matches this encoder"
        );
    }
}
