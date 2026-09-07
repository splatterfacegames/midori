//! Fixed common/v1 selector. Never validate a retained envelope through current schema fields.
use serde_json::Value;
#[allow(unused_imports)] mod control_generated;
mod types {pub use super::control_generated::app::control::v1::*;}
#[allow(dead_code)] mod projection_generated;
pub fn metadata()->&'static Value {
 static META:std::sync::OnceLock<Value>=std::sync::OnceLock::new();
 META.get_or_init(||{
  let mut value:Value=serde_json::from_str(include_str!("../../../../../schemas/releases/common-v1/projection.json")).expect("frozen common descriptor");
  value["objects"]["DispatchRequest"]=value["objects"]["OperationRequest"].clone();
  value["identifiers"]["DispatchRequest"]=value["identifiers"]["OperationRequest"].clone();value
 })
}
fn decode(root:&str,bytes:&[u8])->Result<Value,String>{projection_generated::decode_raw(if root=="DispatchRequest" {"OperationRequest"} else {root},bytes)}
fn read_only(_: &str,_:&Value)->Result<Vec<u8>,String>{Err("Catalog selector is read-only".into())}
pub fn inspect(root:&str,bytes:&[u8])->Result<Value,String>{
 let name=match root {"OperationRequest"=>"DispatchRequest","OperationReceipt"=>"OperationReceipt",_=>return Err("PROTOCOL_INCOMPATIBLE: unsupported envelope root".into())};
 let codec=super::v1::Codec{metadata:metadata(),encode_raw:read_only,decode_raw:decode};
 let value=codec.decode_json(name,bytes)?;
 if root=="OperationRequest" && (value["schema"]!="app.operation-request/v1" || value["protocol_version"]!="1") {return Err("PROTOCOL_INCOMPATIBLE".into())}
 Ok(value)
}
