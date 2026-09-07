use serde_json::{json, Value};
use tools_frontend_contracts::{Codec, DecodeBudget, DecodeLimits};
fn unused_encode(_: &str, _: &Value) -> Result<Vec<u8>, String> {
    unreachable!()
}
fn normalize_bad_scalar(_: &str, _: &[u8]) -> Result<Value, String> {
    Ok(json!({"value":null}))
}
fn forbidden_decode(_: &str, _: &[u8]) -> Result<Value, String> {
    panic!("generated serde accessor ran before rejection")
}
fn metadata(base: &str) -> Value {
    json!({"objects":{"External":{"attributes":{},"fields":[{"name":"value","id":0,"optional":true,"required":false,"type":{"base_type":base,"index":-1}}]}},"enums":{},"identifiers":{},"catalog":{}})
}
#[test]
fn optional_nonfinite_wire_scalar_is_rejected_before_json_null_elision() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut builder = flatbuffers::FlatBufferBuilder::new();
        let start = builder.start_table();
        builder.push_slot_always::<f64>(4, value);
        let root = builder.end_table(start);
        builder.finish(root, None);
        let metadata = metadata("Double");
        let codec = Codec {
            metadata: &metadata,
            encode_raw: unused_encode,
            decode_raw: normalize_bad_scalar,
        };
        assert!(
            codec
                .decode_json("External", builder.finished_data())
                .is_err(),
            "nonfinite optional value was erased by serde_json"
        );
    }
}
#[test]
fn invalid_physical_bool_is_rejected_before_accessor_normalization() {
    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let start = builder.start_table();
    builder.push_slot_always::<u8>(4, 2);
    let root = builder.end_table(start);
    builder.finish(root, None);
    let metadata = metadata("Bool");
    let codec = Codec {
        metadata: &metadata,
        encode_raw: unused_encode,
        decode_raw: forbidden_decode,
    };
    assert!(codec
        .decode_json("External", builder.finished_data())
        .unwrap_err()
        .contains("bool"));
}
fn nested_metadata() -> Value {
    json!({"objects":{
 "External":{"attributes":{},"fields":[{"name":"child","id":0,"required":true,"type":{"base_type":"Obj","object":"Child"}}]},
 "Child":{"attributes":{},"fields":[{"name":"values","id":0,"required":true,"type":{"base_type":"Vector","element":"Float","index":-1}}]}
},"enums":{},"identifiers":{},"catalog":{}})
}
fn nested_bytes(value: f32) -> Vec<u8> {
    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let vector = builder.create_vector(&[value]);
    let start = builder.start_table();
    builder.push_slot_always(4, vector);
    let child = builder.end_table(start);
    let start = builder.start_table();
    builder.push_slot_always(4, child);
    let root = builder.end_table(start);
    builder.finish(root, None);
    builder.finished_data().to_vec()
}
fn decode_nested(_: &str, _: &[u8]) -> Result<Value, String> {
    Ok(json!({"child":{"values":[0.0]}}))
}
#[test]
fn nested_float_vector_nonfinite_values_are_rejected_before_serde() {
    let metadata = nested_metadata();
    let codec = Codec {
        metadata: &metadata,
        encode_raw: unused_encode,
        decode_raw: forbidden_decode,
    };
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(codec
            .decode_json("External", &nested_bytes(value))
            .unwrap_err()
            .contains("finite"));
    }
}
#[test]
fn table_depth_and_apparent_work_accumulate_across_roots() {
    let metadata = nested_metadata();
    let codec = Codec {
        metadata: &metadata,
        encode_raw: unused_encode,
        decode_raw: decode_nested,
    };
    let bytes = nested_bytes(0.0);
    let mut budget = DecodeBudget::default();
    let limits = DecodeLimits::default();
    codec
        .decode_json_with_budget("External", &bytes, &limits, &mut budget, 0)
        .unwrap();
    assert_eq!(budget.tables, 2);
    let one_root = budget.apparent_bytes;
    assert!(one_root > 0);
    let limits = DecodeLimits {
        max_apparent_bytes: one_root * 2 - 1,
        ..limits
    };
    assert!(codec
        .decode_json_with_budget("External", &bytes, &limits, &mut budget, 0)
        .unwrap_err()
        .contains("Apparent"));
    for (limits, depth, expected) in [
        (
            DecodeLimits {
                max_depth: 1,
                ..DecodeLimits::default()
            },
            0,
            "depth",
        ),
        (
            DecodeLimits {
                max_depth: 2,
                ..DecodeLimits::default()
            },
            1,
            "depth",
        ),
        (
            DecodeLimits {
                max_tables: 1,
                ..DecodeLimits::default()
            },
            0,
            "Table",
        ),
        (
            DecodeLimits {
                max_bytes: bytes.len() - 1,
                ..DecodeLimits::default()
            },
            0,
            "size",
        ),
    ] {
        assert!(codec
            .decode_json_with_budget(
                "External",
                &bytes,
                &limits,
                &mut DecodeBudget::default(),
                depth
            )
            .unwrap_err()
            .contains(expected));
    }
    codec
        .decode_json_with_budget(
            "External",
            &bytes,
            &DecodeLimits {
                max_depth: 2,
                ..DecodeLimits::default()
            },
            &mut DecodeBudget::default(),
            0,
        )
        .unwrap();
}
#[test]
fn utf8_byte_string_caps_and_detail_override_apply_before_json_allocation() {
    let codec = tools_frontend_contracts::common_codec();
    let limits = DecodeLimits {
        max_string_bytes: 1024,
        max_detail_bytes: 4096,
        ..DecodeLimits::default()
    };
    let problem =
        json!({"schema":"app.problem/v1","code":"x","title":"x","detail":"é".repeat(1500)});
    let bytes = codec.encode_json("Problem", &problem).unwrap();
    codec
        .decode_json_with_budget("Problem", &bytes, &limits, &mut DecodeBudget::default(), 0)
        .unwrap();
    let mut bad = problem.clone();
    bad["title"] = json!("é".repeat(513));
    let bytes = codec.encode_json("Problem", &bad).unwrap();
    assert!(codec
        .decode_json_with_budget("Problem", &bytes, &limits, &mut DecodeBudget::default(), 0)
        .unwrap_err()
        .contains("String"));
    let mut bad = problem;
    bad["detail"] = json!("a".repeat(4097));
    let bytes = codec.encode_json("Problem", &bad).unwrap();
    assert!(codec
        .decode_json_with_budget("Problem", &bytes, &limits, &mut DecodeBudget::default(), 0)
        .unwrap_err()
        .contains("String"));
    let metadata = json!({"objects":{"External":{"fields":[{"name":"capabilities","id":0,"required":true,"type":{"base_type":"Vector","element":"String"}}]}}});
    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let string = builder.create_string(&"a".repeat(1025));
    let vector = builder.create_vector(&[string]);
    let start = builder.start_table();
    builder.push_slot_always(4, vector);
    let root = builder.end_table(start);
    builder.finish(root, None);
    let codec = Codec {
        metadata: &metadata,
        encode_raw: unused_encode,
        decode_raw: forbidden_decode,
    };
    assert!(codec
        .decode_json_with_budget(
            "External",
            builder.finished_data(),
            &limits,
            &mut DecodeBudget::default(),
            0
        )
        .unwrap_err()
        .contains("String"));
}
#[test]
fn aliased_strings_consume_apparent_work_for_every_traversal() {
    let metadata = json!({"objects":{"External":{"fields":[{"name":"values","id":0,"required":true,"type":{"base_type":"Vector","element":"String"}}]}}});
    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let string = builder.create_string(&"a".repeat(1024));
    let vector = builder.create_vector(&vec![string; 1000]);
    let start = builder.start_table();
    builder.push_slot_always(4, vector);
    let root = builder.end_table(start);
    builder.finish(root, None);
    assert!(builder.finished_data().len() < 6000);
    let codec = Codec {
        metadata: &metadata,
        encode_raw: unused_encode,
        decode_raw: forbidden_decode,
    };
    let limits = DecodeLimits {
        max_apparent_bytes: 100_000,
        ..DecodeLimits::default()
    };
    assert!(codec
        .decode_json_with_budget(
            "External",
            builder.finished_data(),
            &limits,
            &mut DecodeBudget::default(),
            0
        )
        .unwrap_err()
        .contains("Apparent"));
}

#[test]
fn hostile_configurations_cannot_disable_preflight_safety_caps() {
    let metadata = nested_metadata();
    let codec = Codec {
        metadata: &metadata,
        encode_raw: unused_encode,
        decode_raw: forbidden_decode,
    };
    for limits in [
        DecodeLimits {
            max_depth: usize::MAX,
            ..DecodeLimits::default()
        },
        DecodeLimits {
            max_depth: 0,
            ..DecodeLimits::default()
        },
        DecodeLimits {
            max_bytes: usize::MAX,
            ..DecodeLimits::default()
        },
        DecodeLimits {
            max_bytes: 7,
            ..DecodeLimits::default()
        },
        DecodeLimits {
            max_tables: usize::MAX,
            ..DecodeLimits::default()
        },
        DecodeLimits {
            max_tables: 0,
            ..DecodeLimits::default()
        },
        DecodeLimits {
            max_apparent_bytes: usize::MAX,
            ..DecodeLimits::default()
        },
        DecodeLimits {
            max_apparent_bytes: 0,
            ..DecodeLimits::default()
        },
        DecodeLimits {
            max_string_bytes: usize::MAX,
            ..DecodeLimits::default()
        },
        DecodeLimits {
            max_string_bytes: 0,
            ..DecodeLimits::default()
        },
        DecodeLimits {
            max_detail_bytes: usize::MAX,
            ..DecodeLimits::default()
        },
        DecodeLimits {
            max_detail_bytes: 0,
            ..DecodeLimits::default()
        },
    ] {
        assert!(codec
            .decode_json_with_budget(
                "External",
                &nested_bytes(0.),
                &limits,
                &mut DecodeBudget::default(),
                0
            )
            .unwrap_err()
            .contains("limits"));
    }
}
