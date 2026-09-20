//! The Midori tree-compute wire contract.
//!
//! This crate owns nothing but the generated FlatBuffers bindings for
//! `schemas/midori/tree.fbs` and a small, engine-free encoding surface over
//! them. It deliberately does not depend on `midori-core` or
//! `midori-ui-domain`: the contract is the boundary, so a change to the engine
//! cannot quietly change the wire shape, and this crate stays cheap to build
//! and test on its own.
//!
//! Two invariants the rest of the application relies on:
//!
//! - **Large payloads stay out of the envelope.** [`TreeComputeResult`] refers
//!   to the exported GLB by digest and byte count through [`ArtifactRef`]. The
//!   bytes travel over a separate, bounded, digest-checked path.
//! - **Identity is exact.** The seed is a `u64` on the wire; JavaScript sees it
//!   as a `bigint` through the generated bindings, never as a lossy `number`.
//!   `source_sha256` digests the exact imported UTF-8 bytes.
//!
//! Everything under `src/generated/` is machine-generated and carries an
//! ownership manifest. Do not hand-edit it; change the `.fbs` and regenerate.

mod generated {
    #[allow(clippy::all, unused_imports, dead_code)]
    pub mod tree_generated;

    #[allow(clippy::all, unused_imports, dead_code)]
    pub mod projection_generated;
}

// `projection_generated.rs` is generated with `use crate::*;`, so the table
// types have to live at the crate root.
pub use generated::tree_generated::midori::tree::v1::*;

use flatbuffers::FlatBufferBuilder;

/// SHA-256 of `schemas/generated/tree.bfbs`, the binary descriptor the bindings
/// in `src/generated/` were produced from. `tests::descriptor_sha256_matches_generated`
/// fails if the checked-in descriptor moves without this constant following it.
pub const DESCRIPTOR_SHA256: &str =
    "b5fddc6191b5b10c5e22dd21f60e628d65c8864ee6f77a60fb598e2e58f59235";

/// The `schema` field carried by every request and result. Binding the
/// namespace to the descriptor digest means a peer built against a different
/// `.fbs` is rejected on the first message rather than misreading a field.
pub const SCHEMA_TAG: &str =
    "Midori.Tree.V1:b5fddc6191b5b10c5e22dd21f60e628d65c8864ee6f77a60fb598e2e58f59235";

/// FlatBuffers file identifier of the root table, from the `.fbs`.
pub const FILE_IDENTIFIER: &str = "MTR1";

/// `ArtifactRef::kind` for the exported binary glTF.
pub const ARTIFACT_KIND_GLB: &str = "model/gltf-binary";

/// Upper bound on an accepted envelope. Control messages are metadata; a
/// multi-megabyte one means a caller tried to smuggle a payload through.
pub const MAX_ENVELOPE_BYTES: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    /// The buffer is larger than [`MAX_ENVELOPE_BYTES`].
    TooLarge { bytes: usize, limit: usize },
    /// The buffer is too short to contain a root offset and file identifier.
    /// Checked before anything else: `flatbuffers::buffer_has_identifier`
    /// asserts on a buffer shorter than eight bytes, which would abort the
    /// process rather than reject the message.
    Truncated { bytes: usize },
    /// The buffer does not carry the [`FILE_IDENTIFIER`] of the root table.
    Identifier,
    /// FlatBuffers verification failed: truncated, malformed, or missing a
    /// field the schema marks required.
    Verify(String),
    /// The peer speaks a different descriptor.
    Schema { expected: String, found: String },
    /// The descriptor-driven JSON projection rejected the value.
    Projection(String),
}

impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge { bytes, limit } => {
                write!(f, "envelope is {bytes} bytes, over the {limit} byte limit")
            }
            Self::Truncated { bytes } => write!(f, "envelope is only {bytes} bytes"),
            Self::Identifier => write!(f, "buffer is not a {FILE_IDENTIFIER} message"),
            Self::Verify(error) => write!(f, "FlatBuffers verification failed: {error}"),
            Self::Schema { expected, found } => {
                write!(f, "schema mismatch: expected {expected}, found {found}")
            }
            Self::Projection(error) => write!(f, "JSON projection: {error}"),
        }
    }
}

impl std::error::Error for ContractError {}

/// One contiguous index range of a LOD mesh drawn with one core material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmeshRangeInput {
    pub material: MaterialRole,
    pub index_offset: u32,
    pub index_count: u32,
}

/// One balanced-LOD level, as counts and material ranges. Vertex and index data
/// stay out of the envelope by design.
#[derive(Debug, Clone, PartialEq)]
pub struct LodLevelInput {
    pub index: u32,
    pub name: String,
    pub screen_height: f32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub submeshes: Vec<SubmeshRangeInput>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreeSummaryInput {
    pub species_name: String,
    pub scientific_name: String,
    pub stem_count: u64,
    pub branch_count: u64,
    pub leaf_count: u64,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
}

/// Caller-supplied build provenance. The contract carries this value; it does
/// not attest to its truth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineIdentityInput {
    pub source_revision: String,
    pub build_id: String,
}

/// A bounded reference to bytes held outside the envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRefInput {
    pub kind: String,
    pub sha256: String,
    pub byte_count: u64,
}

/// Plain-data form of [`TreeComputeResult`]. The adapter that owns the engine
/// fills this in; this crate owns turning it into bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeComputeResultInput {
    pub source_sha256: String,
    pub seed: u64,
    pub engine: EngineIdentityInput,
    pub summary: TreeSummaryInput,
    pub lods: Vec<LodLevelInput>,
    pub glb: ArtifactRefInput,
}

/// Plain-data form of [`TreeComputeRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeComputeRequestInput {
    /// The exact imported UTF-8 species TOML. Not reparsed or normalized here.
    pub source: Vec<u8>,
    pub seed: u64,
    /// UI provenance only; never part of document identity.
    pub display_name: Option<String>,
}

/// Encode a result. The buffer is finished with [`FILE_IDENTIFIER`].
#[must_use]
pub fn encode_result(input: &TreeComputeResultInput) -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();

    let schema = fbb.create_string(SCHEMA_TAG);
    let source_sha256 = fbb.create_string(&input.source_sha256);

    let source_revision = fbb.create_string(&input.engine.source_revision);
    let build_id = fbb.create_string(&input.engine.build_id);
    let engine = EngineIdentity::create(
        &mut fbb,
        &EngineIdentityArgs {
            source_revision: Some(source_revision),
            build_id: Some(build_id),
        },
    );

    let species_name = fbb.create_string(&input.summary.species_name);
    let scientific_name = fbb.create_string(&input.summary.scientific_name);
    let bounds_min = fbb.create_vector(&input.summary.bounds_min);
    let bounds_max = fbb.create_vector(&input.summary.bounds_max);
    let summary = TreeSummary::create(
        &mut fbb,
        &TreeSummaryArgs {
            species_name: Some(species_name),
            scientific_name: Some(scientific_name),
            stem_count: input.summary.stem_count,
            branch_count: input.summary.branch_count,
            leaf_count: input.summary.leaf_count,
            bounds_min: Some(bounds_min),
            bounds_max: Some(bounds_max),
        },
    );

    let lods: Vec<_> = input
        .lods
        .iter()
        .map(|lod| {
            let name = fbb.create_string(&lod.name);
            let submeshes: Vec<_> = lod
                .submeshes
                .iter()
                .map(|submesh| {
                    SubmeshRange::create(
                        &mut fbb,
                        &SubmeshRangeArgs {
                            material: submesh.material,
                            index_offset: submesh.index_offset,
                            index_count: submesh.index_count,
                        },
                    )
                })
                .collect();
            let submeshes = fbb.create_vector(&submeshes);
            LodLevel::create(
                &mut fbb,
                &LodLevelArgs {
                    index: lod.index,
                    name: Some(name),
                    screen_height: lod.screen_height,
                    vertex_count: lod.vertex_count,
                    index_count: lod.index_count,
                    submeshes: Some(submeshes),
                },
            )
        })
        .collect();
    let lods = fbb.create_vector(&lods);

    let glb_kind = fbb.create_string(&input.glb.kind);
    let glb_sha256 = fbb.create_string(&input.glb.sha256);
    let glb = ArtifactRef::create(
        &mut fbb,
        &ArtifactRefArgs {
            kind: Some(glb_kind),
            sha256: Some(glb_sha256),
            byte_count: input.glb.byte_count,
        },
    );

    let root = TreeComputeResult::create(
        &mut fbb,
        &TreeComputeResultArgs {
            schema: Some(schema),
            source_sha256: Some(source_sha256),
            seed: input.seed,
            engine: Some(engine),
            summary: Some(summary),
            lods: Some(lods),
            glb: Some(glb),
        },
    );
    finish_tree_compute_result_buffer(&mut fbb, root);
    fbb.finished_data().to_vec()
}

/// Encode a request. `TreeComputeRequest` is not the schema's root type, so the
/// buffer carries no file identifier.
#[must_use]
pub fn encode_request(input: &TreeComputeRequestInput) -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();
    let schema = fbb.create_string(SCHEMA_TAG);
    let source = fbb.create_vector(&input.source);
    let display_name = input.display_name.as_deref().map(|n| fbb.create_string(n));
    let root = TreeComputeRequest::create(
        &mut fbb,
        &TreeComputeRequestArgs {
            schema: Some(schema),
            source: Some(source),
            seed: input.seed,
            display_name,
        },
    );
    fbb.finish(root, None);
    fbb.finished_data().to_vec()
}

/// Smallest buffer that can carry a root offset and a file identifier.
const MIN_ENVELOPE_BYTES: usize = 8;

fn check_size(bytes: &[u8]) -> Result<(), ContractError> {
    if bytes.len() > MAX_ENVELOPE_BYTES {
        return Err(ContractError::TooLarge {
            bytes: bytes.len(),
            limit: MAX_ENVELOPE_BYTES,
        });
    }
    if bytes.len() < MIN_ENVELOPE_BYTES {
        return Err(ContractError::Truncated { bytes: bytes.len() });
    }
    Ok(())
}

fn check_schema(found: &str) -> Result<(), ContractError> {
    if found == SCHEMA_TAG {
        Ok(())
    } else {
        Err(ContractError::Schema {
            expected: SCHEMA_TAG.to_owned(),
            found: found.to_owned(),
        })
    }
}

/// Verify and read a result. Rejects an oversized buffer, a buffer without the
/// root file identifier, a buffer that fails FlatBuffers verification, and a
/// buffer built against a different descriptor.
///
/// # Errors
///
/// See [`ContractError`].
pub fn decode_result(bytes: &[u8]) -> Result<TreeComputeResult<'_>, ContractError> {
    check_size(bytes)?;
    if !tree_compute_result_buffer_has_identifier(bytes) {
        return Err(ContractError::Identifier);
    }
    let result = flatbuffers::root::<TreeComputeResult>(bytes)
        .map_err(|error| ContractError::Verify(error.to_string()))?;
    check_schema(result.schema())?;
    Ok(result)
}

/// Verify and read a request.
///
/// # Errors
///
/// See [`ContractError`]. A request buffer carries no file identifier, so
/// [`ContractError::Identifier`] is never returned here.
pub fn decode_request(bytes: &[u8]) -> Result<TreeComputeRequest<'_>, ContractError> {
    check_size(bytes)?;
    let request = flatbuffers::root::<TreeComputeRequest>(bytes)
        .map_err(|error| ContractError::Verify(error.to_string()))?;
    check_schema(request.schema())?;
    Ok(request)
}

/// Descriptor-driven JSON projection, from the generated `projection_generated.rs`.
///
/// This exists so tests can cross-check a hand-written encoder against the
/// generator's own view of the descriptor. **It is not a transport**, and it is
/// not safe on untrusted input: the generated parser is written with `unwrap()`
/// on every field, so a malformed value panics rather than returning an error.
/// The wire format is FlatBuffers, and [`decode_result`] is the only accepted
/// entry point for bytes from another process.
///
/// The projection is also *not* a symmetric round trip for a table containing an
/// enum. The serializer flatc emits writes `MaterialRole` as its name
/// (`"Leaves"`), while the generated parser reads it with `as_u64()`. Feeding
/// the output of this function back into [`project_from_json`] therefore panics
/// for any table reachable from a `MaterialRole` field.
///
/// **This asymmetry is pre-existing in the Python generator, not something the
/// Node port introduced.** The same `as_u64().unwrap()` against the same
/// `serialize_unit_variant(..., variant_name())` appears in the checked-in
/// Python-generated projection for the shared control schema
/// (`tools-frontend-contracts/src/generated/projection_generated.rs`, the
/// `OperationStatus` field of `parse_OperationReceipt`, against
/// `control_generated.rs`'s `impl Serialize for OperationStatus`). Reproducing
/// it is what byte-identity with the oracle requires, and byte-identity is the
/// generator's entire acceptance basis.
///
/// So it **must not be "fixed" in the generator**: doing so would break
/// byte-identity against every checked-in bundle. Changing it is a
/// contract-tooling decision with that cost attached, not a local bug fix.
/// `tests::the_projection_is_not_symmetric_for_enums` pins the behaviour so
/// nothing in this application is built on the assumption that the round trip
/// closes, and so a silent change upstream is noticed.
///
/// # Errors
///
/// Returns [`ContractError::Projection`] for an unknown root.
pub fn project_to_json(root: &str, bytes: &[u8]) -> Result<serde_json::Value, ContractError> {
    generated::projection_generated::decode_raw(root, bytes).map_err(ContractError::Projection)
}

/// Nominal inverse of [`project_to_json`]. Test-only, trusted input only: see
/// that function's caveats about `unwrap()` and enum asymmetry.
///
/// # Errors
///
/// Returns [`ContractError::Projection`] for an unknown root.
///
/// # Panics
///
/// The generated parser panics on a value whose shape does not match the
/// descriptor, including an enum written as a name rather than an integer.
pub fn project_from_json(root: &str, value: &serde_json::Value) -> Result<Vec<u8>, ContractError> {
    generated::projection_generated::encode_raw(root, value).map_err(ContractError::Projection)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TreeComputeResultInput {
        TreeComputeResultInput {
            source_sha256: "a".repeat(64),
            // 2^64 - 1: survives only if the whole path is 64-bit.
            seed: u64::MAX,
            engine: EngineIdentityInput {
                source_revision: "d355c03bdac759f2952b2724c932ade38661ee4d".into(),
                build_id: "test".into(),
            },
            summary: TreeSummaryInput {
                species_name: "Test Oak".into(),
                scientific_name: "Quercus testus".into(),
                stem_count: 41,
                branch_count: 40,
                leaf_count: 900,
                bounds_min: [-1.5, 0.0, -1.5],
                bounds_max: [1.5, 7.25, 1.5],
            },
            lods: vec![
                LodLevelInput {
                    index: 0,
                    name: "LOD0".into(),
                    screen_height: 1.0,
                    vertex_count: 5000,
                    index_count: 12000,
                    submeshes: vec![
                        SubmeshRangeInput {
                            material: MaterialRole::Bark,
                            index_offset: 0,
                            index_count: 9000,
                        },
                        SubmeshRangeInput {
                            material: MaterialRole::Leaves,
                            index_offset: 9000,
                            index_count: 3000,
                        },
                    ],
                },
                LodLevelInput {
                    index: 1,
                    name: "LOD1".into(),
                    screen_height: 0.5,
                    vertex_count: 1200,
                    index_count: 2400,
                    submeshes: vec![SubmeshRangeInput {
                        material: MaterialRole::Bark,
                        index_offset: 0,
                        index_count: 2400,
                    }],
                },
            ],
            glb: ArtifactRefInput {
                kind: ARTIFACT_KIND_GLB.into(),
                sha256: "b".repeat(64),
                byte_count: 1_234_567,
            },
        }
    }

    #[test]
    fn descriptor_sha256_matches_generated() {
        // Guards the constant against a regenerated schema landing without the
        // encoder's schema tag following it.
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let projection = manifest.join("../../schemas/generated/projection.json");
        let text = std::fs::read_to_string(&projection)
            .unwrap_or_else(|e| panic!("read {}: {e}", projection.display()));
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            value["descriptor_sha256"].as_str(),
            Some(DESCRIPTOR_SHA256),
            "DESCRIPTOR_SHA256 is stale relative to schemas/generated/projection.json"
        );
        assert!(SCHEMA_TAG.ends_with(DESCRIPTOR_SHA256));

        // And the digest really is over the checked-in binary descriptor.
        use sha2::{Digest, Sha256};
        let bfbs = std::fs::read(manifest.join("../../schemas/generated/tree.bfbs")).unwrap();
        let hex = Sha256::digest(&bfbs)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(hex, DESCRIPTOR_SHA256);
    }

    #[test]
    fn result_round_trips_through_the_generated_bindings() {
        let input = sample();
        let bytes = encode_result(&input);
        let result = decode_result(&bytes).expect("decode");

        assert_eq!(result.schema(), SCHEMA_TAG);
        assert_eq!(result.source_sha256(), input.source_sha256);
        // The point of the ulong: this value is not representable as an f64.
        assert_eq!(result.seed(), u64::MAX);
        assert_eq!(result.engine().build_id(), "test");
        assert_eq!(result.summary().species_name(), "Test Oak");
        assert_eq!(result.summary().leaf_count(), 900);
        assert_eq!(
            result.summary().bounds_max().iter().collect::<Vec<_>>(),
            vec![1.5, 7.25, 1.5]
        );

        assert_eq!(result.lods().len(), 2);
        let lod0 = result.lods().get(0);
        assert_eq!(lod0.name(), "LOD0");
        assert_eq!(lod0.index_count(), 12000);
        assert_eq!(lod0.submeshes().len(), 2);
        assert_eq!(lod0.submeshes().get(0).material(), MaterialRole::Bark);
        assert_eq!(lod0.submeshes().get(1).material(), MaterialRole::Leaves);
        assert_eq!(lod0.submeshes().get(1).index_offset(), 9000);

        assert_eq!(result.glb().kind(), ARTIFACT_KIND_GLB);
        assert_eq!(result.glb().byte_count(), 1_234_567);
        // The envelope refers to the GLB; it does not carry it.
        assert!(bytes.len() < 1024, "envelope grew to {} bytes", bytes.len());
    }

    #[test]
    fn request_round_trips_and_keeps_source_bytes_exact() {
        // Not valid UTF-8-normalizable content: a BOM and CRLF must survive
        // byte-for-byte, because the digest is over exactly these bytes.
        let source = b"\xef\xbb\xbf[species]\r\nname = \"Oak\"\r\n".to_vec();
        let input = TreeComputeRequestInput {
            source: source.clone(),
            seed: 18_446_744_073_709_551_615,
            display_name: Some("oak.toml".into()),
        };
        let bytes = encode_request(&input);
        let request = decode_request(&bytes).expect("decode");
        assert_eq!(request.source().bytes(), source.as_slice());
        assert_eq!(request.seed(), u64::MAX);
        assert_eq!(request.display_name(), Some("oak.toml"));
    }

    #[test]
    fn a_request_without_a_display_name_is_the_same_computation() {
        let a = encode_request(&TreeComputeRequestInput {
            source: b"x".to_vec(),
            seed: 7,
            display_name: Some("a.toml".into()),
        });
        let b = encode_request(&TreeComputeRequestInput {
            source: b"x".to_vec(),
            seed: 7,
            display_name: None,
        });
        let (a, b) = (decode_request(&a).unwrap(), decode_request(&b).unwrap());
        assert_eq!(a.source().bytes(), b.source().bytes());
        assert_eq!(a.seed(), b.seed());
        assert_eq!(b.display_name(), None);
    }

    #[test]
    fn a_truncated_buffer_is_rejected_and_never_aborts() {
        // `flatbuffers::buffer_has_identifier` asserts on a buffer shorter than
        // eight bytes. Since these bytes arrive from the frontend, reaching that
        // assertion would abort the process instead of rejecting the message.
        let bytes = encode_result(&sample());
        assert!(decode_result(&bytes).is_ok());
        assert_eq!(
            decode_result(&[]),
            Err(ContractError::Truncated { bytes: 0 })
        );
        assert_eq!(
            decode_result(&bytes[..7]),
            Err(ContractError::Truncated { bytes: 7 })
        );
        for cut in [8usize, 9, 32, bytes.len() / 2, bytes.len() - 1] {
            assert!(
                decode_result(&bytes[..cut]).is_err(),
                "a {cut}-byte prefix decoded"
            );
        }
        // The same for the request path, which has no identifier to check.
        let request = encode_request(&TreeComputeRequestInput {
            source: b"[species]\nname = \"Oak\"\n".to_vec(),
            seed: 3,
            display_name: None,
        });
        assert!(decode_request(&request).is_ok());
        for cut in 0..request.len() {
            assert!(
                decode_request(&request[..cut]).is_err(),
                "a {cut}-byte request prefix decoded"
            );
        }
    }

    #[test]
    fn a_result_buffer_is_not_accepted_as_a_request() {
        // Both roots are tables of strings and scalars, so the rejection has to
        // come from verification, not from luck about layout.
        let bytes = encode_result(&sample());
        assert!(decode_request(&bytes).is_err());
    }

    #[test]
    fn a_buffer_without_the_file_identifier_is_rejected() {
        let mut fbb = FlatBufferBuilder::new();
        let schema = fbb.create_string(SCHEMA_TAG);
        let source = fbb.create_vector(b"x");
        let root = TreeComputeRequest::create(
            &mut fbb,
            &TreeComputeRequestArgs {
                schema: Some(schema),
                source: Some(source),
                seed: 1,
                display_name: None,
            },
        );
        fbb.finish(root, None);
        assert_eq!(
            decode_result(fbb.finished_data()),
            Err(ContractError::Identifier)
        );
    }

    #[test]
    fn a_foreign_schema_tag_is_rejected() {
        let mut fbb = FlatBufferBuilder::new();
        let schema = fbb.create_string(
            "Midori.Tree.V1:0000000000000000000000000000000000000000000000000000000000000000",
        );
        let source = fbb.create_vector(b"x");
        let root = TreeComputeRequest::create(
            &mut fbb,
            &TreeComputeRequestArgs {
                schema: Some(schema),
                source: Some(source),
                seed: 1,
                display_name: None,
            },
        );
        fbb.finish(root, None);
        assert!(matches!(
            decode_request(fbb.finished_data()),
            Err(ContractError::Schema { .. })
        ));
    }

    #[test]
    fn an_oversized_envelope_is_rejected_before_verification() {
        let bytes = vec![0u8; MAX_ENVELOPE_BYTES + 1];
        assert_eq!(
            decode_result(&bytes),
            Err(ContractError::TooLarge {
                bytes: MAX_ENVELOPE_BYTES + 1,
                limit: MAX_ENVELOPE_BYTES,
            })
        );
    }

    #[test]
    fn the_hand_written_encoder_agrees_with_the_generated_projection() {
        // Cross-check: the descriptor-driven projection from the generator reads
        // back exactly what `encode_result` wrote. If the two disagree, one of
        // them is not speaking the checked-in descriptor.
        let input = sample();
        let bytes = encode_result(&input);
        let json = project_to_json("TreeComputeResult", &bytes).expect("project");

        assert_eq!(json["schema"], serde_json::json!(SCHEMA_TAG));
        assert_eq!(
            json["source_sha256"],
            serde_json::json!(input.source_sha256)
        );
        assert_eq!(json["seed"], serde_json::json!(u64::MAX));
        assert_eq!(
            json["summary"]["species_name"],
            serde_json::json!("Test Oak")
        );
        assert_eq!(json["lods"].as_array().unwrap().len(), 2);
        assert_eq!(json["lods"][0]["name"], serde_json::json!("LOD0"));
        assert_eq!(
            json["lods"][0]["submeshes"][1]["index_offset"],
            serde_json::json!(9000)
        );
        // The generated serializer writes enums by name, not by ordinal.
        assert_eq!(
            json["lods"][0]["submeshes"][0]["material"],
            serde_json::json!("Bark")
        );
        assert_eq!(
            json["lods"][0]["submeshes"][1]["material"],
            serde_json::json!("Leaves")
        );
        assert_eq!(json["glb"]["byte_count"], serde_json::json!(1_234_567u64));
        assert_eq!(json["glb"]["kind"], serde_json::json!(ARTIFACT_KIND_GLB));
    }

    #[test]
    fn the_projection_round_trips_a_table_with_no_enum_in_it() {
        // `TreeSummary` reaches no `MaterialRole`, so the generated parser and
        // the generated serializer agree on it and the round trip closes.
        let bytes = encode_result(&sample());
        let json = project_to_json("TreeComputeResult", &bytes).unwrap();
        let summary = project_from_json("TreeSummary", &json["summary"]).expect("encode summary");
        let summary = flatbuffers::root::<TreeSummary>(&summary).expect("verify summary");
        assert_eq!(summary.species_name(), "Test Oak");
        assert_eq!(summary.leaf_count(), 900);
        assert_eq!(
            summary.bounds_max().iter().collect::<Vec<_>>(),
            vec![1.5, 7.25, 1.5]
        );
    }

    #[test]
    fn the_projection_is_not_symmetric_for_enums() {
        // Pins the behaviour documented on `project_from_json`: the serializer
        // emits `"Leaves"`, the parser calls `as_u64()` on it and unwraps
        // `None`.
        //
        // This is upstream, not ours. The Python generator produces the same
        // asymmetry for the shared control schema, and a byte-identical port is
        // obliged to reproduce it - so this test exists to hold the behaviour
        // still, NOT as a bug report against the Node generator. If it ever
        // starts passing the wrong way, the generator's byte-identity against
        // the checked-in bundles has been broken and that is the thing to
        // investigate first.
        let json = serde_json::json!({
            "material": "Leaves",
            "index_offset": 0,
            "index_count": 3,
        });
        let panicked = std::panic::catch_unwind(|| {
            let _ = project_from_json("SubmeshRange", &json);
        });
        assert!(
            panicked.is_err(),
            "the projection parser accepted a named enum; \
             re-check whether project_from_json is now symmetric"
        );
        // The integer form is what the generated parser actually wants.
        let bytes = project_from_json(
            "SubmeshRange",
            &serde_json::json!({ "material": 1, "index_offset": 0, "index_count": 3 }),
        )
        .expect("encode");
        let range = flatbuffers::root::<SubmeshRange>(&bytes).expect("verify");
        assert_eq!(range.material(), MaterialRole::Leaves);
    }
}
