use serde_json::{json,Value};
use tools_frontend_contracts::{CatalogDispatch,RegisteredCodec};
fn unused_encode(_: &str,_:&Value)->Result<Vec<u8>,String>{panic!("must not dispatch")}
fn unused_decode(_: &str,_:&[u8])->Result<Value,String>{panic!("must not dispatch")}
fn metadata(revision:&str,digest:&str)->Value{json!({"descriptor_sha256":digest,"catalog":{format!("{revision}|edit/v1"):"Payload"},"objects":{"Payload":{"namespace":"Fixture.V1"}}})}
#[test]
fn retained_registration_wins_over_a_changed_current_aggregate(){
 let old=metadata("old/v1",&"a".repeat(64));let mut latest=metadata("old/v1",&"b".repeat(64));latest["catalog"]["new/v2|edit/v1"]=json!("Payload");
 let retained=RegisteredCodec{metadata:&old,encode_json:unused_encode,decode_json:unused_decode};
 let current=RegisteredCodec{metadata:&latest,encode_json:unused_encode,decode_json:unused_decode};
 let dispatch=CatalogDispatch::new(current,&[retained]).unwrap();
 assert_eq!(dispatch.operation_registration("old/v1","edit/v1").unwrap()["descriptor_sha256"],"a".repeat(64));
 assert_eq!(dispatch.operation_registration("new/v2","edit/v1").unwrap()["descriptor_sha256"],"b".repeat(64));
 assert_eq!(dispatch.catalog().as_array().unwrap().len(),2);
 assert!(dispatch.operation_registration("unknown","edit/v1").is_err());
}

#[test]fn common_selector_is_explicitly_bound_to_the_published_descriptor(){
 let metadata=tools_frontend_contracts::releases::common_v1::metadata();
 assert_eq!(metadata["descriptor_sha256"],"4f4807cc1b02213b331af209e477bf47b1bdb50cb32cba91faa616fc25680304");
 assert_eq!(metadata["objects"]["OperationReceipt"]["fields"].as_array().unwrap().len(),11);
 assert!(tools_frontend_contracts::releases::common_v1::inspect("OperationRequest/v2",&[]).unwrap_err().contains("PROTOCOL_INCOMPATIBLE"));
}
