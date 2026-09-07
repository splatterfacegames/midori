// Generated from BFBS; do not edit.
use crate::*;
use serde_json::Value;
#[allow(non_snake_case)]
fn parse_ArtifactRef(v: &Value) -> ArtifactRefT { let mut result = ArtifactRefT::default();
if let Some(value) = v.get("id").filter(|v| !v.is_null()) { result.id = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("kind").filter(|v| !v.is_null()) { result.kind = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("sha256").filter(|v| !v.is_null()) { result.sha256 = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("byte_count").filter(|v| !v.is_null()) { result.byte_count = value.as_u64().unwrap() as u64; }
if let Some(value) = v.get("source_revision").filter(|v| !v.is_null()) { result.source_revision = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("available").filter(|v| !v.is_null()) { result.available = value.as_bool().unwrap() as bool; }
result }
#[allow(non_snake_case)]
fn parse_ChangeEvent(v: &Value) -> ChangeEventT { let mut result = ChangeEventT::default();
if let Some(value) = v.get("cursor").filter(|v| !v.is_null()) { result.cursor = value.as_u64().unwrap() as u64; }
if let Some(value) = v.get("target").filter(|v| !v.is_null()) { result.target = Box::new(parse_ResourceRef(value)); }
if let Some(value) = v.get("revision").filter(|v| !v.is_null()) { result.revision = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("operation_id").filter(|v| !v.is_null()) { result.operation_id = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("resource_sequence").filter(|v| !v.is_null()) { result.resource_sequence = value.as_u64().unwrap() as u64; }
result }
#[allow(non_snake_case)]
fn parse_FieldError(v: &Value) -> FieldErrorT { let mut result = FieldErrorT::default();
if let Some(value) = v.get("pointer").filter(|v| !v.is_null()) { result.pointer = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("detail").filter(|v| !v.is_null()) { result.detail = value.as_str().unwrap().to_owned(); }
result }
#[allow(non_snake_case)]
fn parse_Frame(v: &Value) -> FrameT { let mut result = FrameT::default();
if let Some(value) = v.get("schema").filter(|v| !v.is_null()) { result.schema = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("source_revision").filter(|v| !v.is_null()) { result.source_revision = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("mesh_revision").filter(|v| !v.is_null()) { result.mesh_revision = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("sequence").filter(|v| !v.is_null()) { result.sequence = value.as_u64().unwrap() as u64; }
if let Some(value) = v.get("time_s").filter(|v| !v.is_null()) { result.time_s = value.as_f64().unwrap() as f64; }
if let Some(value) = v.get("positions").filter(|v| !v.is_null()) { result.positions = value.as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect(); }
if let Some(value) = v.get("indices").filter(|v| !v.is_null()) { result.indices = Some(value.as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect()); }
if let Some(value) = v.get("entity_ids").filter(|v| !v.is_null()) { result.entity_ids = Some(value.as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_owned()).collect()); }
if let Some(value) = v.get("producer").filter(|v| !v.is_null()) { result.producer = value.as_str().unwrap().to_owned(); }
result }
#[allow(non_snake_case)]
fn parse_OperationReceipt(v: &Value) -> OperationReceiptT { let mut result = OperationReceiptT::default();
if let Some(value) = v.get("operation_id").filter(|v| !v.is_null()) { result.operation_id = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("status").filter(|v| !v.is_null()) { result.status = OperationStatus(value.as_u64().unwrap() as u8); }
if let Some(value) = v.get("kind").filter(|v| !v.is_null()) { result.kind = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("target").filter(|v| !v.is_null()) { result.target = Box::new(parse_ResourceRef(value)); }
if let Some(value) = v.get("input_revision").filter(|v| !v.is_null()) { result.input_revision = Some(value.as_str().unwrap().to_owned()); }
if let Some(value) = v.get("output_revision").filter(|v| !v.is_null()) { result.output_revision = Some(value.as_str().unwrap().to_owned()); }
if let Some(value) = v.get("resource_sequence").filter(|v| !v.is_null()) { result.resource_sequence = value.as_u64().unwrap() as u64; }
if let Some(value) = v.get("result").filter(|v| !v.is_null()) { result.result = Some(value.as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect()); }
if let Some(value) = v.get("error").filter(|v| !v.is_null()) { result.error = Some(Box::new(parse_Problem(value))); }
if let Some(value) = v.get("event_cursor").filter(|v| !v.is_null()) { result.event_cursor = value.as_u64().unwrap() as u64; }
if let Some(value) = v.get("catalog_revision").filter(|v| !v.is_null()) { result.catalog_revision = value.as_str().unwrap().to_owned(); }
result }
#[allow(non_snake_case)]
fn parse_OperationRequest(v: &Value) -> OperationRequestT { let mut result = OperationRequestT::default();
if let Some(value) = v.get("schema").filter(|v| !v.is_null()) { result.schema = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("protocol_version").filter(|v| !v.is_null()) { result.protocol_version = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("operation_id").filter(|v| !v.is_null()) { result.operation_id = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("target").filter(|v| !v.is_null()) { result.target = Box::new(parse_ResourceRef(value)); }
if let Some(value) = v.get("input_revision").filter(|v| !v.is_null()) { result.input_revision = Some(value.as_str().unwrap().to_owned()); }
if let Some(value) = v.get("kind").filter(|v| !v.is_null()) { result.kind = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("parameters").filter(|v| !v.is_null()) { result.parameters = value.as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u8).collect(); }
if let Some(value) = v.get("catalog_revision").filter(|v| !v.is_null()) { result.catalog_revision = value.as_str().unwrap().to_owned(); }
result }
#[allow(non_snake_case)]
fn parse_Problem(v: &Value) -> ProblemT { let mut result = ProblemT::default();
if let Some(value) = v.get("schema").filter(|v| !v.is_null()) { result.schema = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("code").filter(|v| !v.is_null()) { result.code = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("title").filter(|v| !v.is_null()) { result.title = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("detail").filter(|v| !v.is_null()) { result.detail = Some(value.as_str().unwrap().to_owned()); }
if let Some(value) = v.get("request_id").filter(|v| !v.is_null()) { result.request_id = Some(value.as_str().unwrap().to_owned()); }
if let Some(value) = v.get("operation_id").filter(|v| !v.is_null()) { result.operation_id = Some(value.as_str().unwrap().to_owned()); }
if let Some(value) = v.get("retryable").filter(|v| !v.is_null()) { result.retryable = value.as_bool().unwrap() as bool; }
if let Some(value) = v.get("current_revision").filter(|v| !v.is_null()) { result.current_revision = Some(value.as_str().unwrap().to_owned()); }
if let Some(value) = v.get("field_errors").filter(|v| !v.is_null()) { result.field_errors = Some(value.as_array().unwrap().iter().map(|v| parse_FieldError(v)).collect()); }
result }
#[allow(non_snake_case)]
fn parse_ResourceRef(v: &Value) -> ResourceRefT { let mut result = ResourceRefT::default();
if let Some(value) = v.get("type").filter(|v| !v.is_null()) { result.type_ = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("id").filter(|v| !v.is_null()) { result.id = value.as_str().unwrap().to_owned(); }
result }
pub(crate) fn encode_raw(name:&str,v:&Value)->Result<Vec<u8>,String>{ let mut b=flatbuffers::FlatBufferBuilder::new(); match name {
"ArtifactRef"=>{let value=parse_ArtifactRef(v); let root=value.pack(&mut b); b.finish(root, None);},
"ChangeEvent"=>{let value=parse_ChangeEvent(v); let root=value.pack(&mut b); b.finish(root, None);},
"FieldError"=>{let value=parse_FieldError(v); let root=value.pack(&mut b); b.finish(root, None);},
"Frame"=>{let value=parse_Frame(v); let root=value.pack(&mut b); b.finish(root, None);},
"OperationReceipt"=>{let value=parse_OperationReceipt(v); let root=value.pack(&mut b); b.finish(root, None);},
"OperationRequest"=>{let value=parse_OperationRequest(v); let root=value.pack(&mut b); b.finish(root, Some("AOP1"));},
"Problem"=>{let value=parse_Problem(v); let root=value.pack(&mut b); b.finish(root, None);},
"ResourceRef"=>{let value=parse_ResourceRef(v); let root=value.pack(&mut b); b.finish(root, None);},
 _=>return Err("Unknown root".into()) } Ok(b.finished_data().to_vec()) }
pub(crate) fn decode_raw(name:&str,b:&[u8])->Result<Value,String>{let options=flatbuffers::VerifierOptions {max_depth:64,max_tables:100000,max_apparent_size:64*1024*1024,..Default::default()}; match name {
"ArtifactRef"=>serde_json::to_value(flatbuffers::root_with_opts::<ArtifactRef>(&options,b).map_err(|e|e.to_string())?).map_err(|e|e.to_string()),
"ChangeEvent"=>serde_json::to_value(flatbuffers::root_with_opts::<ChangeEvent>(&options,b).map_err(|e|e.to_string())?).map_err(|e|e.to_string()),
"FieldError"=>serde_json::to_value(flatbuffers::root_with_opts::<FieldError>(&options,b).map_err(|e|e.to_string())?).map_err(|e|e.to_string()),
"Frame"=>serde_json::to_value(flatbuffers::root_with_opts::<Frame>(&options,b).map_err(|e|e.to_string())?).map_err(|e|e.to_string()),
"OperationReceipt"=>serde_json::to_value(flatbuffers::root_with_opts::<OperationReceipt>(&options,b).map_err(|e|e.to_string())?).map_err(|e|e.to_string()),
"OperationRequest"=>serde_json::to_value(flatbuffers::root_with_opts::<OperationRequest>(&options,b).map_err(|e|e.to_string())?).map_err(|e|e.to_string()),
"Problem"=>serde_json::to_value(flatbuffers::root_with_opts::<Problem>(&options,b).map_err(|e|e.to_string())?).map_err(|e|e.to_string()),
"ResourceRef"=>serde_json::to_value(flatbuffers::root_with_opts::<ResourceRef>(&options,b).map_err(|e|e.to_string())?).map_err(|e|e.to_string()),
 _=>Err("Unknown root".into()) }}
