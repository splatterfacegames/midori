// Generated from BFBS; do not edit.
use crate::*;
use serde_json::Value;
#[allow(non_snake_case)]
fn parse_ArtifactRef(v: &Value) -> ArtifactRefT {
    let mut result = ArtifactRefT::default();
    if let Some(value) = v.get("kind").filter(|v| !v.is_null()) {
        result.kind = value.as_str().unwrap().to_owned();
    }
    if let Some(value) = v.get("sha256").filter(|v| !v.is_null()) {
        result.sha256 = value.as_str().unwrap().to_owned();
    }
    if let Some(value) = v.get("byte_count").filter(|v| !v.is_null()) {
        result.byte_count = value.as_u64().unwrap() as u64;
    }
    result
}
#[allow(non_snake_case)]
fn parse_EngineIdentity(v: &Value) -> EngineIdentityT {
    let mut result = EngineIdentityT::default();
    if let Some(value) = v.get("source_revision").filter(|v| !v.is_null()) {
        result.source_revision = value.as_str().unwrap().to_owned();
    }
    if let Some(value) = v.get("build_id").filter(|v| !v.is_null()) {
        result.build_id = value.as_str().unwrap().to_owned();
    }
    result
}
#[allow(non_snake_case)]
fn parse_LodLevel(v: &Value) -> LodLevelT {
    let mut result = LodLevelT::default();
    if let Some(value) = v.get("index").filter(|v| !v.is_null()) {
        result.index = value.as_u64().unwrap() as u32;
    }
    if let Some(value) = v.get("name").filter(|v| !v.is_null()) {
        result.name = value.as_str().unwrap().to_owned();
    }
    if let Some(value) = v.get("screen_height").filter(|v| !v.is_null()) {
        result.screen_height = value.as_f64().unwrap() as f32;
    }
    if let Some(value) = v.get("vertex_count").filter(|v| !v.is_null()) {
        result.vertex_count = value.as_u64().unwrap() as u32;
    }
    if let Some(value) = v.get("index_count").filter(|v| !v.is_null()) {
        result.index_count = value.as_u64().unwrap() as u32;
    }
    if let Some(value) = v.get("submeshes").filter(|v| !v.is_null()) {
        result.submeshes = value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| parse_SubmeshRange(v))
            .collect();
    }
    result
}
#[allow(non_snake_case)]
fn parse_SubmeshRange(v: &Value) -> SubmeshRangeT {
    let mut result = SubmeshRangeT::default();
    if let Some(value) = v.get("material").filter(|v| !v.is_null()) {
        result.material = MaterialRole(value.as_u64().unwrap() as u8);
    }
    if let Some(value) = v.get("index_offset").filter(|v| !v.is_null()) {
        result.index_offset = value.as_u64().unwrap() as u32;
    }
    if let Some(value) = v.get("index_count").filter(|v| !v.is_null()) {
        result.index_count = value.as_u64().unwrap() as u32;
    }
    result
}
#[allow(non_snake_case)]
fn parse_TreeComputeRequest(v: &Value) -> TreeComputeRequestT {
    let mut result = TreeComputeRequestT::default();
    if let Some(value) = v.get("schema").filter(|v| !v.is_null()) {
        result.schema = value.as_str().unwrap().to_owned();
    }
    if let Some(value) = v.get("source").filter(|v| !v.is_null()) {
        result.source = value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect();
    }
    if let Some(value) = v.get("seed").filter(|v| !v.is_null()) {
        result.seed = value.as_u64().unwrap() as u64;
    }
    if let Some(value) = v.get("display_name").filter(|v| !v.is_null()) {
        result.display_name = Some(value.as_str().unwrap().to_owned());
    }
    result
}
#[allow(non_snake_case)]
fn parse_TreeComputeResult(v: &Value) -> TreeComputeResultT {
    let mut result = TreeComputeResultT::default();
    if let Some(value) = v.get("schema").filter(|v| !v.is_null()) {
        result.schema = value.as_str().unwrap().to_owned();
    }
    if let Some(value) = v.get("source_sha256").filter(|v| !v.is_null()) {
        result.source_sha256 = value.as_str().unwrap().to_owned();
    }
    if let Some(value) = v.get("seed").filter(|v| !v.is_null()) {
        result.seed = value.as_u64().unwrap() as u64;
    }
    if let Some(value) = v.get("engine").filter(|v| !v.is_null()) {
        result.engine = Box::new(parse_EngineIdentity(value));
    }
    if let Some(value) = v.get("summary").filter(|v| !v.is_null()) {
        result.summary = Box::new(parse_TreeSummary(value));
    }
    if let Some(value) = v.get("lods").filter(|v| !v.is_null()) {
        result.lods = value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| parse_LodLevel(v))
            .collect();
    }
    if let Some(value) = v.get("glb").filter(|v| !v.is_null()) {
        result.glb = Box::new(parse_ArtifactRef(value));
    }
    result
}
#[allow(non_snake_case)]
fn parse_TreeSummary(v: &Value) -> TreeSummaryT {
    let mut result = TreeSummaryT::default();
    if let Some(value) = v.get("species_name").filter(|v| !v.is_null()) {
        result.species_name = value.as_str().unwrap().to_owned();
    }
    if let Some(value) = v.get("scientific_name").filter(|v| !v.is_null()) {
        result.scientific_name = value.as_str().unwrap().to_owned();
    }
    if let Some(value) = v.get("stem_count").filter(|v| !v.is_null()) {
        result.stem_count = value.as_u64().unwrap() as u64;
    }
    if let Some(value) = v.get("branch_count").filter(|v| !v.is_null()) {
        result.branch_count = value.as_u64().unwrap() as u64;
    }
    if let Some(value) = v.get("leaf_count").filter(|v| !v.is_null()) {
        result.leaf_count = value.as_u64().unwrap() as u64;
    }
    if let Some(value) = v.get("bounds_min").filter(|v| !v.is_null()) {
        result.bounds_min = value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect();
    }
    if let Some(value) = v.get("bounds_max").filter(|v| !v.is_null()) {
        result.bounds_max = value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect();
    }
    result
}
pub(crate) fn encode_raw(name: &str, v: &Value) -> Result<Vec<u8>, String> {
    let mut b = flatbuffers::FlatBufferBuilder::new();
    match name {
        "ArtifactRef" => {
            let value = parse_ArtifactRef(v);
            let root = value.pack(&mut b);
            b.finish(root, None);
        }
        "EngineIdentity" => {
            let value = parse_EngineIdentity(v);
            let root = value.pack(&mut b);
            b.finish(root, None);
        }
        "LodLevel" => {
            let value = parse_LodLevel(v);
            let root = value.pack(&mut b);
            b.finish(root, None);
        }
        "SubmeshRange" => {
            let value = parse_SubmeshRange(v);
            let root = value.pack(&mut b);
            b.finish(root, None);
        }
        "TreeComputeRequest" => {
            let value = parse_TreeComputeRequest(v);
            let root = value.pack(&mut b);
            b.finish(root, None);
        }
        "TreeComputeResult" => {
            let value = parse_TreeComputeResult(v);
            let root = value.pack(&mut b);
            b.finish(root, Some("MTR1"));
        }
        "TreeSummary" => {
            let value = parse_TreeSummary(v);
            let root = value.pack(&mut b);
            b.finish(root, None);
        }
        _ => return Err("Unknown root".into()),
    }
    Ok(b.finished_data().to_vec())
}
pub(crate) fn decode_raw(name: &str, b: &[u8]) -> Result<Value, String> {
    let options = flatbuffers::VerifierOptions {
        max_depth: 64,
        max_tables: 100000,
        max_apparent_size: 64 * 1024 * 1024,
        ..Default::default()
    };
    match name {
        "ArtifactRef" => serde_json::to_value(
            flatbuffers::root_with_opts::<ArtifactRef>(&options, b).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string()),
        "EngineIdentity" => serde_json::to_value(
            flatbuffers::root_with_opts::<EngineIdentity>(&options, b)
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string()),
        "LodLevel" => serde_json::to_value(
            flatbuffers::root_with_opts::<LodLevel>(&options, b).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string()),
        "SubmeshRange" => serde_json::to_value(
            flatbuffers::root_with_opts::<SubmeshRange>(&options, b).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string()),
        "TreeComputeRequest" => serde_json::to_value(
            flatbuffers::root_with_opts::<TreeComputeRequest>(&options, b)
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string()),
        "TreeComputeResult" => serde_json::to_value(
            flatbuffers::root_with_opts::<TreeComputeResult>(&options, b)
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string()),
        "TreeSummary" => serde_json::to_value(
            flatbuffers::root_with_opts::<TreeSummary>(&options, b).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string()),
        _ => Err("Unknown root".into()),
    }
}
