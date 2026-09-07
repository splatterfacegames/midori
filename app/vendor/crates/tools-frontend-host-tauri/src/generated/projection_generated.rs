// Generated from BFBS; do not edit.
use crate::*;
use serde_json::Value;
#[allow(non_snake_case)]
fn parse_HostMessage(v: &Value) -> HostMessageT { let mut result = HostMessageT::default();
if let Some(value) = v.get("operation").filter(|v| !v.is_null()) { result.operation = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("app_id").filter(|v| !v.is_null()) { result.app_id = value.as_str().unwrap().to_owned(); }
if let Some(value) = v.get("project_id").filter(|v| !v.is_null()) { result.project_id = Some(value.as_str().unwrap().to_owned()); }
if let Some(value) = v.get("session_id").filter(|v| !v.is_null()) { result.session_id = Some(value.as_str().unwrap().to_owned()); }
if let Some(value) = v.get("window_id").filter(|v| !v.is_null()) { result.window_id = Some(value.as_str().unwrap().to_owned()); }
if let Some(value) = v.get("generation").filter(|v| !v.is_null()) { result.generation = value.as_u64().unwrap() as u32; }
if let Some(value) = v.get("transfer_id").filter(|v| !v.is_null()) { result.transfer_id = Some(value.as_str().unwrap().to_owned()); }
if let Some(value) = v.get("source_id").filter(|v| !v.is_null()) { result.source_id = Some(value.as_str().unwrap().to_owned()); }
if let Some(value) = v.get("destination_id").filter(|v| !v.is_null()) { result.destination_id = Some(value.as_str().unwrap().to_owned()); }
if let Some(value) = v.get("panel_ids").filter(|v| !v.is_null()) { result.panel_ids = Some(value.as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_owned()).collect()); }
if let Some(value) = v.get("layout").filter(|v| !v.is_null()) { result.layout = Some(value.as_str().unwrap().to_owned()); }
if let Some(value) = v.get("status").filter(|v| !v.is_null()) { result.status = Some(value.as_str().unwrap().to_owned()); }
if let Some(value) = v.get("owned_panel_ids").filter(|v| !v.is_null()) { result.owned_panel_ids = Some(value.as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_owned()).collect()); }
if let Some(value) = v.get("ownership_revision").filter(|v| !v.is_null()) { result.ownership_revision = value.as_u64().unwrap() as u64; }
result }
pub(crate) fn encode_raw(name:&str,v:&Value)->Result<Vec<u8>,String>{ let mut b=flatbuffers::FlatBufferBuilder::new(); match name {
"HostMessage"=>{let value=parse_HostMessage(v); let root=value.pack(&mut b); b.finish(root, Some("TFWH"));},
 _=>return Err("Unknown root".into()) } Ok(b.finished_data().to_vec()) }
pub(crate) fn decode_raw(name:&str,b:&[u8])->Result<Value,String>{let options=flatbuffers::VerifierOptions {max_depth:64,max_tables:100000,max_apparent_size:64*1024*1024,..Default::default()}; match name {
"HostMessage"=>serde_json::to_value(flatbuffers::root_with_opts::<HostMessage>(&options,b).map_err(|e|e.to_string())?).map_err(|e|e.to_string()),
 _=>Err("Unknown root".into()) }}
