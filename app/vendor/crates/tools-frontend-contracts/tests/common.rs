use serde_json::json;
use tools_frontend_contracts::common_codec;
#[test]
fn common_bulk_codec_is_independent_of_any_application() {
    let codec = common_codec();
    let value = json!({"schema":"frame/v1","source_revision":"r","mesh_revision":"m","sequence":"18446744073709551615","positions":[0,1,2],"producer":"shared-library-test"});
    let bytes = codec.encode_json("Frame", &value).unwrap();
    let result = codec.decode_json("Frame", &bytes).unwrap();
    assert_eq!(result["sequence"], value["sequence"]);
    assert!(codec.catalog().as_object().unwrap().is_empty());
}
