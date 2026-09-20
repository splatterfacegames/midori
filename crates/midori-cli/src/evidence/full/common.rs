use super::super::report::{Check, CheckStatus};
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;

pub const MIN_SCREENSHOT_SIZE: u32 = 256;
pub const MIN_SCREENSHOT_LUMINANCE_RANGE: u32 = 8;
pub const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
pub const FNV_PRIME: u64 = 0x100000001b3;

pub type ExpectedMapSummary<'a> = (&'a str, &'a str, usize, &'a [usize], &'a [usize]);

pub const EXPECTED_MAP_SUMMARIES: &[ExpectedMapSummary<'static>] = &[
    ("height_u16.png", "L16", 1, &[0], &[]),
    ("normal_yplus.png", "RGB8", 3, &[], &[0, 1, 2]),
    ("normal_yminus.png", "RGB8", 3, &[], &[0, 1, 2]),
    ("masks_rgba.png", "RGBA8", 4, &[0, 1], &[]),
    ("grass_density.png", "L8", 1, &[0], &[]),
];

#[derive(Default)]
pub struct Verifier {
    checks: Vec<Check>,
}

impl Verifier {
    pub fn pass(&mut self, name: impl Into<String>, detail: impl Into<String>) {
        self.checks.push(Check {
            name: name.into(),
            status: CheckStatus::Passed,
            detail: detail.into(),
        });
    }

    pub fn fail(&mut self, name: impl Into<String>, detail: impl Into<String>) {
        self.checks.push(Check {
            name: name.into(),
            status: CheckStatus::Failed,
            detail: detail.into(),
        });
    }

    pub fn missing(&mut self, name: impl Into<String>, detail: impl Into<String>) {
        self.checks.push(Check {
            name: name.into(),
            status: CheckStatus::Missing,
            detail: detail.into(),
        });
    }

    pub fn require(
        &mut self,
        name: impl Into<String>,
        condition: bool,
        pass: impl Into<String>,
        failure: impl Into<String>,
    ) {
        let name = name.into();
        if condition {
            self.pass(name, pass);
        } else {
            self.fail(name, failure);
        }
    }

    pub fn require_equal(
        &mut self,
        name: impl Into<String>,
        actual: Option<&Value>,
        expected: Value,
    ) {
        let actual_value = actual.cloned().unwrap_or(Value::Null);
        // A missing field is malformed evidence, even when another missing
        // field would otherwise make both sides serialize as JSON null.
        let ok = actual.is_some() && !expected.is_null() && values_equal(&actual_value, &expected);
        self.require(
            name,
            ok,
            format!("{} == {}", py_repr(&actual_value), py_repr(&expected)),
            format!("{} != {}", py_repr(&actual_value), py_repr(&expected)),
        );
    }

    pub fn require_close(
        &mut self,
        name: impl Into<String>,
        actual: Option<&Value>,
        expected: f64,
        tolerance: f64,
    ) {
        let name = name.into();
        let actual_float = actual.and_then(as_f64);
        let ok = actual_float.is_some_and(|value| {
            let difference = (value - expected).abs();
            value.is_finite()
                && expected.is_finite()
                && difference.is_finite()
                && difference <= tolerance.max(1e-9 * value.abs().max(expected.abs()))
        });
        let actual_text = actual_float
            .map(|value| value.to_string())
            .unwrap_or_else(|| py_repr(actual.unwrap_or(&Value::Null)));
        self.require(
            name,
            ok,
            format!("{actual_text} ~= {expected}"),
            format!("{actual_text} not within {tolerance} of {expected}"),
        );
    }

    pub fn into_checks(self) -> Vec<Check> {
        self.checks
    }
}

#[derive(Debug)]
pub enum JsonLoad {
    Missing,
    Invalid(String),
    Value(Value),
}

pub fn load_json(path: &Path) -> JsonLoad {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return JsonLoad::Missing,
        Err(error) => return JsonLoad::Invalid(format!("{path:?} cannot be inspected: {error}")),
    };
    if !metadata.is_file() {
        return JsonLoad::Invalid(format!("{path:?} is not a regular file"));
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return JsonLoad::Invalid(format!("{path:?} cannot be read: {error}")),
    };
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text.strip_prefix('\u{feff}').unwrap_or(text),
        Err(error) => return JsonLoad::Invalid(format!("{path:?} is not UTF-8: {error}")),
    };
    match serde_json::from_str(text) {
        Ok(value) => JsonLoad::Value(value),
        Err(error) => JsonLoad::Invalid(format!("{path:?} contains invalid JSON: {error}")),
    }
}

pub fn file_nonempty(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
}

pub fn value_as_array(value: Option<&Value>) -> Option<&Vec<Value>> {
    value.and_then(Value::as_array)
}

pub fn field<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    value.as_object()?.get(name)
}

pub fn field_path<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Value> {
    let mut current = Some(value);
    for name in names {
        current = current.and_then(|item| field(item, name));
    }
    current
}

pub fn string_field(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str)
}

pub fn as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse::<f64>().ok(),
        Value::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

pub fn int_or_default(value: Option<&Value>, default: i64) -> i64 {
    let Some(value) = value else { return default };
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| {
                number
                    .as_u64()
                    .map(|value| value.min(i64::MAX as u64) as i64)
            })
            .or_else(|| number.as_f64().map(|value| value as i64))
            .unwrap_or(default),
        Value::String(text) => text
            .parse::<i64>()
            .ok()
            .or_else(|| {
                text.parse::<u64>()
                    .ok()
                    .map(|value| value.min(i64::MAX as u64) as i64)
            })
            .unwrap_or(default),
        Value::Bool(value) => i64::from(*value),
        Value::Null | Value::Array(_) | Value::Object(_) => default,
    }
}

pub fn integer_value(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| {
                number
                    .as_u64()
                    .map(|value| value.min(i64::MAX as u64) as i64)
            })
            .or_else(|| number.as_f64().map(|value| value as i64)),
        Value::String(text) => text.parse::<i64>().ok().or_else(|| {
            text.parse::<u64>()
                .ok()
                .map(|value| value.min(i64::MAX as u64) as i64)
        }),
        Value::Bool(value) => Some(i64::from(*value)),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

pub fn float_or_default(value: Option<&Value>, default: f64) -> f64 {
    value.and_then(as_f64).unwrap_or(default)
}

pub fn is_nonzero_hex_checksum(value: Option<&Value>) -> bool {
    let Some(text) = value.and_then(Value::as_str) else {
        return false;
    };
    let Some(digits) = text.strip_prefix("0x") else {
        return false;
    };
    !digits.is_empty()
        && digits
            .chars()
            .all(|character| character.is_ascii_hexdigit())
        && digits.chars().any(|character| character != '0')
}

pub fn fnv1a(data: &[u8], seed: u64) -> u64 {
    let mut value = seed;
    for byte in data {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(FNV_PRIME);
    }
    value
}

pub fn hex_u64(value: u64) -> String {
    format!("0x{value:016x}")
}

pub fn file_checksum_hex(path: &Path) -> io::Result<String> {
    Ok(hex_u64(fnv1a(&fs::read(path)?, FNV_OFFSET_BASIS)))
}

pub fn expected_source_files(manifest: &Value, normal_key: &str) -> Vec<String> {
    let mut files = std::collections::BTreeSet::new();
    files.insert("midori_nature.json".to_string());
    files.insert("preview_tile.glb".to_string());
    if let Some(terrain) = field(manifest, "terrain") {
        for key in ["heightmap_file", "masks_file", "grass_density_file"] {
            if let Some(text) = string_field(field(terrain, key)) {
                files.insert(text.to_string());
            }
        }
    }
    if let Some(conventions) = field(manifest, "normal_conventions")
        && let Some(text) = string_field(field(conventions, normal_key))
    {
        files.insert(text.to_string());
    }
    if let Some(scatter) = field(manifest, "scatter") {
        if let Some(text) = string_field(field(scatter, "file"))
            && !text.is_empty()
        {
            files.insert(text.to_string());
        }
        if let Some(binary_files) = value_as_array(field(scatter, "binary_files")) {
            for item in binary_files {
                if let Some(text) = string_field(field(item, "file"))
                    && !text.is_empty()
                {
                    files.insert(text.to_string());
                }
            }
        }
    }
    for collection in ["prototypes", "material_recipes", "engine_import_recipes"] {
        if let Some(items) = value_as_array(field(manifest, collection)) {
            for item in items {
                if collection == "prototypes" {
                    if let Some(lods) = value_as_array(field(item, "lods")) {
                        for lod in lods {
                            if let Some(text) = string_field(field(lod, "file"))
                                && !text.is_empty()
                            {
                                files.insert(text.to_string());
                            }
                        }
                    }
                } else if let Some(text) = string_field(field(item, "file"))
                    && !text.is_empty()
                {
                    files.insert(text.to_string());
                }
            }
        }
    }
    files.into_iter().collect()
}

pub fn expected_unreal_import_files(manifest: &Value) -> Vec<String> {
    let mut files = std::collections::BTreeSet::new();
    files.insert("midori_nature.json".to_string());
    files.insert("preview_tile.glb".to_string());
    if let Some(terrain) = field(manifest, "terrain") {
        for key in ["heightmap_file", "masks_file", "grass_density_file"] {
            if let Some(text) = string_field(field(terrain, key)) {
                files.insert(text.to_string());
            }
        }
    }
    if let Some(conventions) = field(manifest, "normal_conventions")
        && let Some(text) = string_field(field(conventions, "unreal_yminus_file"))
    {
        files.insert(text.to_string());
    }
    for lod in expected_prototype_lod_files(manifest) {
        files.insert(lod);
    }
    files.into_iter().collect()
}

pub fn expected_unreal_import_destinations(destination: &str, files: &[String]) -> Vec<String> {
    files
        .iter()
        .map(|relative| {
            let normalized = relative.replace('\\', "/");
            let mut components = Vec::new();
            for component in normalized.split('/') {
                match component {
                    "" | "." => {}
                    ".." => components.push(".."),
                    value => components.push(value),
                }
            }
            if components.len() <= 1 {
                destination.to_string()
            } else {
                components.pop();
                format!("{destination}/{}", components.join("/"))
            }
        })
        .collect()
}

pub fn expected_unreal_import_map_files(manifest: &Value) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(terrain) = field(manifest, "terrain") {
        for key in ["heightmap_file", "masks_file", "grass_density_file"] {
            if let Some(text) = string_field(field(terrain, key)) {
                result.push(text.to_string());
            }
        }
    }
    if let Some(conventions) = field(manifest, "normal_conventions")
        && let Some(text) = string_field(field(conventions, "unreal_yminus_file"))
    {
        result.push(text.to_string());
    }
    result
}

pub fn expected_prototype_lod_files(manifest: &Value) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(prototypes) = value_as_array(field(manifest, "prototypes")) {
        for prototype in prototypes {
            if let Some(lods) = value_as_array(field(prototype, "lods")) {
                for lod in lods {
                    if let Some(text) = string_field(field(lod, "file"))
                        && !text.is_empty()
                    {
                        result.push(text.to_string());
                    }
                }
            }
        }
    }
    result.sort();
    result
}

pub fn expected_lod0_prototype_files(manifest: &Value) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(prototypes) = value_as_array(field(manifest, "prototypes")) {
        for prototype in prototypes {
            if let Some(lods) = value_as_array(field(prototype, "lods")) {
                for lod in lods {
                    if field(lod, "index").is_some_and(is_numeric_zero)
                        && let Some(text) = string_field(field(lod, "file"))
                        && !text.is_empty()
                    {
                        result.push(text.to_string());
                    }
                }
            }
        }
    }
    result.sort();
    result
}

fn is_numeric_zero(value: &Value) -> bool {
    match value {
        Value::Number(number) => number.as_f64().is_some_and(|value| value == 0.0),
        Value::Bool(value) => !value,
        _ => false,
    }
}

pub fn source_file_checksum_xor(package_dir: &Path, source_files: &[String]) -> io::Result<String> {
    let mut checksum = 0;
    for relative in source_files {
        checksum ^= fnv1a(&fs::read(package_dir.join(relative))?, FNV_OFFSET_BASIS);
    }
    Ok(hex_u64(checksum))
}

pub fn check_package_identity(
    verifier: &mut Verifier,
    prefix: &str,
    report: &Value,
    manifest: &Value,
    package_dir: &Path,
    normal_key: &str,
    camel_case: bool,
) {
    let source_files = expected_source_files(manifest, normal_key);
    let expected_manifest_checksum =
        match file_checksum_hex(&package_dir.join("midori_nature.json")) {
            Ok(value) => value,
            Err(error) => {
                verifier.fail(
                    format!("{prefix}.package_identity"),
                    format!("could not read package source files: {error}"),
                );
                return;
            }
        };
    let expected_source_checksum = match source_file_checksum_xor(package_dir, &source_files) {
        Ok(value) => value,
        Err(error) => {
            verifier.fail(
                format!("{prefix}.package_identity"),
                format!("could not read package source files: {error}"),
            );
            return;
        }
    };
    let manifest_field = if camel_case {
        "manifestFileChecksum"
    } else {
        "manifest_file_checksum"
    };
    let count_field = if camel_case {
        "sourceFileCount"
    } else {
        "source_file_count"
    };
    let checksum_field = if camel_case {
        "sourceFileChecksumXor"
    } else {
        "source_file_checksum_xor"
    };
    let files_field = if camel_case {
        "sourceFiles"
    } else {
        "source_files"
    };
    verifier.require_equal(
        format!("{prefix}.manifest_file_checksum"),
        field(report, manifest_field),
        json!(expected_manifest_checksum),
    );
    verifier.require_equal(
        format!("{prefix}.source_file_count"),
        field(report, count_field),
        json!(source_files.len()),
    );
    verifier.require_equal(
        format!("{prefix}.source_file_checksum_xor"),
        field(report, checksum_field),
        json!(expected_source_checksum),
    );
    verifier.require_equal(
        format!("{prefix}.source_files"),
        field(report, files_field),
        json!(source_files),
    );
}

pub fn check_artifact(verifier: &mut Verifier, name: &str, path: &Path) {
    match fs::metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            verifier.missing(name, format!("{path:?} is missing or empty"))
        }
        Err(error) => verifier.fail(name, format!("{path:?} cannot be read: {error}")),
        Ok(metadata) if !metadata.is_file() => {
            verifier.fail(name, format!("{path:?} is not a regular file"))
        }
        Ok(metadata) if metadata.len() == 0 => {
            verifier.missing(name, format!("{path:?} is missing or empty"))
        }
        Ok(_) => match fs::read(path) {
            Ok(_) => verifier.pass(name, format!("{path:?} exists")),
            Err(error) => verifier.fail(name, format!("{path:?} cannot be read: {error}")),
        },
    }
}

pub fn value_len(value: Option<&Value>) -> usize {
    value_as_array(value).map_or(0, Vec::len)
}

pub fn array_strings(value: Option<&Value>) -> Vec<String> {
    value_as_array(value)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub fn expected_material_parameter_semantics(manifest: &Value) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(sets) = value_as_array(field(manifest, "material_parameters")) {
        for set in sets {
            let slot = string_field(field(set, "material_slot")).unwrap_or("");
            if let Some(parameters) = value_as_array(field(set, "parameters")) {
                for parameter in parameters {
                    if parameter.is_object() {
                        let semantic = field(parameter, "semantic")
                            .map(py_string)
                            .unwrap_or_default();
                        result.push(format!("{slot}:{semantic}"));
                    }
                }
            }
        }
    }
    result
}

pub fn expected_material_parameter_names_for_slot(manifest: &Value, slot: &str) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    if let Some(sets) = value_as_array(field(manifest, "material_parameters")) {
        for set in sets {
            if string_field(field(set, "material_slot")) != Some(slot) {
                continue;
            }
            if let Some(parameters) = value_as_array(field(set, "parameters")) {
                for parameter in parameters {
                    if let Some(name) = string_field(field(parameter, "name"))
                        && !name.is_empty()
                    {
                        names.insert(name.to_string());
                    }
                }
            }
        }
    }
    names.into_iter().collect()
}

pub fn expected_material_recipe_files(manifest: &Value) -> Vec<String> {
    array_object_field_strings(manifest, "material_recipes", "file")
}

pub fn expected_material_recipe_engine_targets(manifest: &Value) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(recipes) = value_as_array(field(manifest, "material_recipes")) {
        for recipe in recipes {
            let slot = string_field(field(recipe, "material_slot")).unwrap_or("");
            if let Some(targets) = value_as_array(field(recipe, "engine_targets")) {
                for target in targets {
                    result.push(format!("{slot}:{}", py_string(target)));
                }
            }
        }
    }
    result
}

pub fn material_recipe_file_for_slot(manifest: &Value, slot: &str) -> String {
    value_as_array(field(manifest, "material_recipes"))
        .and_then(|recipes| {
            recipes.iter().find_map(|recipe| {
                (string_field(field(recipe, "material_slot")) == Some(slot)).then(|| {
                    string_field(field(recipe, "file"))
                        .unwrap_or("")
                        .to_string()
                })
            })
        })
        .unwrap_or_default()
}

pub fn array_object_field_strings(manifest: &Value, collection: &str, key: &str) -> Vec<String> {
    value_as_array(field(manifest, collection))
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.as_object()
                        .map(|_| string_field(field(item, key)).unwrap_or("").to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn expected_engine_import_recipe_files(manifest: &Value) -> Vec<String> {
    array_object_field_strings(manifest, "engine_import_recipes", "file")
}

pub fn expected_engine_import_recipe_profiles(manifest: &Value) -> Vec<String> {
    value_as_array(field(manifest, "engine_import_recipes"))
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.as_object().map(|_| {
                        format!(
                            "{}:{}",
                            string_field(field(item, "engine")).unwrap_or(""),
                            string_field(field(item, "profile")).unwrap_or("")
                        )
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn expected_engine_import_recipe_systems(manifest: &Value) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(recipes) = value_as_array(field(manifest, "engine_import_recipes")) {
        for recipe in recipes {
            let engine = string_field(field(recipe, "engine")).unwrap_or("");
            if let Some(systems) = value_as_array(field(recipe, "expected_systems")) {
                for system in systems {
                    result.push(format!("{engine}:{}", py_string(system)));
                }
            }
        }
    }
    result
}

pub fn engine_import_recipe_file_for_engine(manifest: &Value, engine: &str) -> String {
    value_as_array(field(manifest, "engine_import_recipes"))
        .and_then(|recipes| {
            recipes.iter().find_map(|recipe| {
                (string_field(field(recipe, "engine")) == Some(engine)).then(|| {
                    string_field(field(recipe, "file"))
                        .unwrap_or("")
                        .to_string()
                })
            })
        })
        .unwrap_or_default()
}

pub fn expected_prototype_surface_target_entries(manifest: &Value) -> Vec<String> {
    value_as_array(field(manifest, "prototypes"))
        .map(|items| {
            items
                .iter()
                .filter_map(|prototype| {
                    prototype.as_object().map(|_| {
                        let name = string_field(field(prototype, "name")).unwrap_or("");
                        let targets = array_strings(field(prototype, "surface_targets"));
                        format!("{name}:{}", targets.join(","))
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn required_surface_targets_for_kind(kind: &str) -> &'static [&'static str] {
    match kind {
        "grass" | "flower" | "weed" | "litter" => &["groundcover_foliage"],
        "moss" => &["groundcover_foliage", "moss_tuft"],
        "shrub" => &["groundcover_foliage", "shrub_base"],
        "rock" => &["static_surface", "rock"],
        "log" => &["static_surface", "log"],
        _ => &[],
    }
}

pub fn py_string(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(_) | Value::Object(_) => py_repr(value),
    }
}

pub fn py_repr(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(value) => if *value { "True" } else { "False" }.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => format!("'{value}'"),
        Value::Array(values) => {
            let mut result = String::from("[");
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    result.push_str(", ");
                }
                result.push_str(&py_repr(value));
            }
            result.push(']');
            result
        }
        Value::Object(values) => {
            let mut result = String::from("{");
            for (index, (key, value)) in values.iter().enumerate() {
                if index > 0 {
                    result.push_str(", ");
                }
                let _ = write!(result, "'{key}': {}", py_repr(value));
            }
            result.push('}');
            result
        }
    }
}

fn numbers_equal(actual: &serde_json::Number, expected: &serde_json::Number) -> bool {
    if actual.is_f64() != expected.is_f64() {
        let (integer, floating) = if actual.is_f64() {
            (expected, actual)
        } else {
            (actual, expected)
        };
        return integer_float_equal(integer, floating);
    }
    if !actual.is_f64() && !expected.is_f64() {
        if let (Some(a), Some(b)) = (actual.as_i64(), expected.as_i64()) {
            return a == b;
        }
        if let (Some(a), Some(b)) = (actual.as_u64(), expected.as_u64()) {
            return a == b;
        }
        if let (Some(a), Some(b)) = (actual.as_i64(), expected.as_u64()) {
            return a >= 0 && a as u64 == b;
        }
        if let (Some(a), Some(b)) = (actual.as_u64(), expected.as_i64()) {
            return b >= 0 && a == b as u64;
        }
        return false;
    }
    actual.as_f64() == expected.as_f64()
}

fn integer_float_equal(integer: &serde_json::Number, floating: &serde_json::Number) -> bool {
    let Some(floating) = floating.as_f64().filter(|value| value.is_finite()) else {
        return false;
    };
    if let Some(integer) = integer.as_i64() {
        // Rust's float-to-integer cast saturates at the signed bounds. Reject
        // the positive saturation boundary, where i64::MAX rounds to 2^63.
        if integer >= 0 && floating >= 2f64.powi(63) {
            return false;
        }
        let candidate = floating as i64;
        candidate == integer && candidate as f64 == floating
    } else if let Some(integer) = integer.as_u64() {
        // Reject the unsigned saturation boundary, where u64::MAX rounds to
        // 2^64. Every lower exactly representable integer round-trips.
        if floating >= 2f64.powi(64) {
            return false;
        }
        let candidate = floating as u64;
        candidate == integer && candidate as f64 == floating
    } else {
        false
    }
}

pub fn values_equal(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Number(actual), Value::Number(expected)) => numbers_equal(actual, expected),
        (Value::Bool(_), Value::Number(_)) | (Value::Number(_), Value::Bool(_)) => {
            as_f64(actual) == as_f64(expected)
        }
        (Value::Array(actual), Value::Array(expected)) => {
            actual.len() == expected.len()
                && actual
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| values_equal(actual, expected))
        }
        (Value::Object(actual), Value::Object(expected)) => {
            actual.len() == expected.len()
                && actual.iter().all(|(key, value)| {
                    expected
                        .get(key)
                        .is_some_and(|other| values_equal(value, other))
                })
        }
        _ => actual == expected,
    }
}

pub fn vector_component(value: Option<&Value>, component: &str) -> Result<f64, String> {
    let Some(value) = value else {
        return Err("missing vector".to_string());
    };
    match value {
        Value::Object(values) => values
            .get(component)
            .and_then(as_f64)
            .ok_or_else(|| format!("missing vector component {component}")),
        Value::Array(values) => {
            let index = match component {
                "x" => 0,
                "y" => 1,
                "z" => 2,
                _ => return Err(format!("unsupported vector component {component}")),
            };
            values
                .get(index)
                .and_then(as_f64)
                .ok_or_else(|| format!("missing vector component {component}"))
        }
        _ => Err(format!("unsupported vector shape: {}", py_repr(value))),
    }
}

pub fn require_vector_close(
    verifier: &mut Verifier,
    name: &str,
    value: Option<&Value>,
    component: &str,
    expected: f64,
) {
    match vector_component(value, component) {
        Ok(actual) => verifier.require_close(name, Some(&json!(actual)), expected, 0.001),
        Err(error) => verifier.fail(name, error),
    }
}
