//! Keeps `schemas/species.schema.json` in sync with the serde model: every
//! field name a serialized `Species` emits must exist in the schema's
//! `properties` (or be allowed by a non-false `additionalProperties`), and
//! every bundled preset must parse.

use std::path::{Path, PathBuf};

use midori_core::Species;
use serde_json::Value;

fn schema() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/species.schema.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn assert_fields_covered(schema_obj: &Value, value: &Value, path: &str) {
    let (Some(obj), Some(fields)) = (schema_obj.as_object(), value.as_object()) else {
        return;
    };
    for (key, val) in fields {
        let next = format!("{path}.{key}");
        let properties = obj.get("properties").and_then(Value::as_object);
        let additional = obj.get("additionalProperties");
        let sub_schema = properties.and_then(|p| p.get(key));
        match (sub_schema, additional) {
            (Some(sub), _) => {
                let sub = if sub.get("$ref").is_some() {
                    resolve_ref(&schema(), sub["$ref"].as_str().unwrap())
                } else {
                    sub.clone()
                };
                assert_fields_covered(&sub, val, &next);
            }
            (None, Some(a)) if a == &Value::Bool(true) => {}
            (None, None) => {
                panic!("field `{next}` is not declared in schemas/species.schema.json")
            }
            (None, Some(a)) if a != &Value::Bool(false) => {}
            (None, _) => panic!("field `{next}` is not declared in schemas/species.schema.json"),
        }
    }
}

fn resolve_ref(root: &Value, r: &str) -> Value {
    let mut cur = root.clone();
    for part in r.trim_start_matches('#').trim_start_matches('/').split('/') {
        cur = cur.get(part).cloned().expect("unresolvable $ref");
    }
    cur
}

#[test]
fn every_serialized_field_is_in_the_schema() {
    let preset = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets/species/oak.toml");
    let species = Species::from_file(&preset).unwrap();
    let value = serde_json::to_value(&species).unwrap();
    assert_fields_covered(&schema(), &value, "species");
}

#[test]
fn rosette_inflorescence_and_state_fields_are_in_the_schema() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets/species");
    for (preset, state) in [
        ("wa_grass_tree.toml", Some("forked")),
        ("wa_kangaroo_paw.toml", None),
        ("wa_marri.toml", None),
        ("wa_karri.toml", None),
        ("perth_jacaranda.toml", Some("flowering")),
        ("wa_firewood_banksia.toml", None),
    ] {
        let species = Species::from_file_with_state(&dir.join(preset), state).unwrap();
        let value = serde_json::to_value(&species).unwrap();
        assert_fields_covered(&schema(), &value, "species");
    }
}

/// Every bundled preset, in every state it declares, must validate, fit the
/// default generation budget, and generate a tree with valid bounds.
#[test]
fn every_state_of_every_preset_generates_within_the_default_budget() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets/species");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.map(|e| e.path()).ok())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    files.sort();
    let budget = midori_core::GenerationBudget::default();
    let mut states_seen = 0;
    for path in files {
        let base = Species::from_file(&path).unwrap();
        let mut variants: Vec<Option<String>> = vec![None];
        variants.extend(base.state_names().into_iter().map(|s| Some(s.to_string())));
        states_seen += variants.len() - 1;
        for state in variants {
            let label = format!(
                "{} [{}]",
                path.display(),
                state.as_deref().unwrap_or("default")
            );
            let species = Species::from_file_with_state(&path, state.as_deref())
                .unwrap_or_else(|e| panic!("{label} failed to load: {e}"));
            budget
                .check_estimate(&midori_core::estimate_tree(&species))
                .unwrap_or_else(|e| panic!("{label}: {e}"));
            let tree = midori_core::generate_tree(&species, 42)
                .unwrap_or_else(|e| panic!("{label} failed to generate: {e}"));
            assert!(tree.bounds.is_valid(), "{label}: invalid bounds");
            assert!(tree.stem_count() >= 1, "{label}: no stems");
        }
    }
    assert!(
        states_seen >= 10,
        "expected the bundled presets to declare states"
    );
}

#[test]
fn all_presets_parse_and_serialize() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets/species");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.map(|e| e.path()).ok())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no species presets found");
    for path in files {
        let species = Species::from_file(&path)
            .unwrap_or_else(|e| panic!("{} failed to parse: {e}", path.display()));
        serde_json::to_value(&species)
            .unwrap_or_else(|e| panic!("{} failed to serialize: {e}", path.display()));
    }
}
