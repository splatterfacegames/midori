//! Generic bridge policy over generated reflection metadata; no request schema lives here.
use crate::{DecodeBudget, DecodeLimits};
use serde_json::{json, Map, Value};
const MAX_BYTES: usize = 16 * 1024 * 1024;
pub struct Codec<'a> {
    pub metadata: &'a Value,
    pub encode_raw: fn(&str, &Value) -> Result<Vec<u8>, String>,
    pub decode_raw: fn(&str, &[u8]) -> Result<Value, String>,
}
impl Codec<'_> {
    pub fn catalog(&self) -> &Value {
        &self.metadata["catalog"]
    }
    fn binding(&self, value: &Value) -> Result<String, String> {
        let key = format!(
            "{}|{}",
            value["catalog_revision"].as_str().unwrap_or(""),
            value["kind"].as_str().unwrap_or("")
        );
        self.catalog()[key]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "SCHEMA_INCOMPATIBLE: unknown catalog/kind registration".into())
    }
    /// Encode schema-derived JSON (SI numbers, decimal-string int64s, flattened wrappers).
    pub fn encode_json(&self, root: &str, value: &Value) -> Result<Vec<u8>, String> {
        let mut value = value.clone();
        if root == "OperationRequest" {
            self.check_envelope(&value)?;
            let payload = self.encode_json(&self.binding(&value)?, &value["parameters"])?;
            value["parameters"] = json!(payload);
        }
        let raw = self.transform(root, &value, false, 0)?;
        let bytes = (self.encode_raw)(root, &raw)?;
        if bytes.len() > MAX_BYTES {
            return Err("LIMIT_EXCEEDED".into());
        }
        Ok(bytes)
    }
    /// Verify the FlatBuffer before accessor traversal, then project descriptor-defined fields.
    pub fn decode_json(&self, root: &str, bytes: &[u8]) -> Result<Value, String> {
        self.decode_json_with_budget(
            root,
            bytes,
            &DecodeLimits::default(),
            &mut DecodeBudget::default(),
            0,
        )
    }
    /// Bounded envelope inspection, without interpreting the registered nested payload.
    pub fn inspect_operation(&self, bytes: &[u8]) -> Result<Value, String> {
        self.decode_internal(
            "OperationRequest",
            bytes,
            false,
            &DecodeLimits::default(),
            &mut DecodeBudget::default(),
            0,
        )
    }
    /// One message can share traversal limits across independently registered payload codecs.
    pub fn decode_json_with_budget(
        &self,
        root: &str,
        bytes: &[u8],
        limits: &DecodeLimits,
        budget: &mut DecodeBudget,
        depth: usize,
    ) -> Result<Value, String> {
        self.decode_internal(root, bytes, true, limits, budget, depth)
    }
    fn decode_internal(
        &self,
        root: &str,
        bytes: &[u8],
        expand: bool,
        limits: &DecodeLimits,
        budget: &mut DecodeBudget,
        depth: usize,
    ) -> Result<Value, String> {
        if bytes.len() < 4 || bytes.len() > MAX_BYTES {
            return Err("Invalid buffer size".into());
        }
        if let Some(identifier) = self.metadata["identifiers"][root].as_str() {
            if bytes.len() < 8 || &bytes[4..8] != identifier.as_bytes() {
                return Err("Invalid root identifier".into());
            }
        }
        crate::preflight::verify(self.metadata, root, bytes, limits, budget, depth)?;
        let raw = (self.decode_raw)(root, bytes)?;
        let mut value = self.transform(root, &raw, true, 0)?;
        if root == "OperationRequest" {
            self.check_envelope(&value)?;
            if !expand {
                return Ok(value);
            }
            let payload: Vec<u8> =
                serde_json::from_value(value["parameters"].clone()).map_err(|e| e.to_string())?;
            value["parameters"] = self.decode_json_with_budget(
                &self.binding(&value)?,
                &payload,
                limits,
                budget,
                depth + 1,
            )?;
        }
        Ok(value)
    }
    pub fn validate_operation(&self, bytes: &[u8]) -> Result<Value, String> {
        self.decode_json("OperationRequest", bytes)
    }
    fn check_envelope(&self, v: &Value) -> Result<(), String> {
        if v["schema"] != "app.operation-request/v1" || v["protocol_version"] != "1" {
            return Err("PROTOCOL_INCOMPATIBLE".into());
        }
        Ok(())
    }
    fn transform(
        &self,
        name: &str,
        value: &Value,
        decode: bool,
        depth: usize,
    ) -> Result<Value, String> {
        if depth > 64 {
            return Err("Maximum nesting depth exceeded".into());
        }
        let definition = &self.metadata["objects"][name];
        let fields = definition["fields"].as_array().ok_or("Unknown root")?;
        if let Some(wrapper) = definition["attributes"]["json_wrapper"].as_str() {
            return if decode {
                self.scalar(
                    &json!({"base_type":"Double","index":-1}),
                    &value[wrapper],
                    true,
                )
            } else {
                Ok(
                    json!({wrapper:self.scalar(&json!({"base_type":"Double","index":-1}),value,false)?}),
                )
            };
        }
        let mut input = value
            .as_object()
            .ok_or_else(|| format!("{name}: expected object"))?
            .clone();
        if decode {
            for field in fields {
                let key = field["name"].as_str().unwrap();
                if let Some(value) = input.remove(&format!("{key}_")) {
                    input.insert(key.into(), value);
                }
            }
        }
        for key in input.keys() {
            if !fields
                .iter()
                .any(|f| f["deprecated"] != true && f["name"] == *key)
            {
                return Err(format!("{name}.{key}: unknown field"));
            }
        }
        let mut output = Map::new();
        for field in fields {
            if field["deprecated"] == true {
                continue;
            }
            let key = field["name"].as_str().unwrap();
            let t = &field["type"];
            let base = t["base_type"].as_str().unwrap();
            let item = input.get(key).unwrap_or(&Value::Null);
            if item.is_null() {
                if field["required"] == true {
                    return Err(format!("{name}.{key}: required field absent"));
                }
                if field["optional"] == true || ["String", "Obj", "Vector"].contains(&base) {
                    continue;
                }
                if decode {
                    return Err(format!("{name}.{key}: non-finite or invalid scalar"));
                }
                let default = if base == "Bool" {
                    json!(field["default_integer"].as_i64().unwrap_or(0) != 0)
                } else if ["Float", "Double"].contains(&base) {
                    field["default_real"].clone()
                } else {
                    field["default_integer"].clone()
                };
                output.insert(
                    key.into(),
                    if ["Long", "ULong"].contains(&base) {
                        self.scalar(t, &default, false)?
                    } else {
                        default
                    },
                );
                continue;
            }
            let converted = match base {
                "Obj" => self.transform(t["object"].as_str().unwrap(), item, decode, depth + 1)?,
                "Vector" => {
                    let array = item
                        .as_array()
                        .ok_or_else(|| format!("{name}.{key}: expected array"))?;
                    if array.len() > 1_000_000 {
                        return Err("Vector limit exceeded".into());
                    }
                    let mut element = t.clone();
                    element["base_type"] = t["element"].clone();
                    Value::Array(
                        array
                            .iter()
                            .map(|v| {
                                if element["base_type"] == "Obj" {
                                    self.transform(
                                        element["object"].as_str().unwrap(),
                                        v,
                                        decode,
                                        depth + 1,
                                    )
                                } else {
                                    self.scalar(&element, v, decode)
                                }
                            })
                            .collect::<Result<_, _>>()?,
                    )
                }
                _ => self.scalar(t, item, decode)?,
            };
            output.insert(key.into(), converted);
        }
        Ok(Value::Object(output))
    }
    fn scalar(&self, t: &Value, value: &Value, decode: bool) -> Result<Value, String> {
        let base = t["base_type"].as_str().unwrap();
        if base == "String" {
            return value
                .as_str()
                .map(|v| json!(v))
                .ok_or("Expected string".into());
        }
        if base == "Bool" {
            return value
                .as_bool()
                .map(|v| json!(v))
                .ok_or("Expected bool".into());
        }
        if ["Double", "Float"].contains(&base) {
            let n = value
                .as_f64()
                .filter(|n| n.is_finite() && (base != "Float" || (*n as f32).is_finite()))
                .ok_or("Expected finite physical quantity")?;
            return Ok(json!(if n == 0.0 { 0.0 } else { n }));
        }
        if ["Long", "ULong"].contains(&base) {
            if decode {
                return if (base == "ULong" && value.as_u64().is_some())
                    || (base == "Long" && value.as_i64().is_some())
                {
                    Ok(json!(value.to_string()))
                } else {
                    Err("Invalid int64".into())
                };
            }
            let s = value.as_str().ok_or("int64 must be a decimal string")?;
            if base == "ULong" {
                let n = s.parse::<u64>().map_err(|_| "Invalid uint64")?;
                if n.to_string() != s {
                    return Err("Noncanonical uint64".into());
                }
                return Ok(json!(n));
            }
            let n = s.parse::<i64>().map_err(|_| "Invalid int64")?;
            if n.to_string() != s {
                return Err("Noncanonical int64".into());
            }
            return Ok(json!(n));
        }
        let mut value = value.clone();
        if let Some(index) = t["index"].as_i64().filter(|n| *n >= 0) {
            let variants = &self.metadata["enums"][index.to_string()]["values"];
            if let Some(name) = value.as_str() {
                value = variants[name].clone();
            }
            if !variants
                .as_object()
                .ok_or("Invalid enum descriptor")?
                .values()
                .any(|v| *v == value)
            {
                return Err("Unknown enum value".into());
            }
        }
        let n = value.as_i64().ok_or("Expected exact integer")?;
        let (min, max) = match base {
            "UByte" => (0, 255),
            "Byte" => (-128, 127),
            "UShort" => (0, 65535),
            "Short" => (-32768, 32767),
            "UInt" => (0, u32::MAX as i64),
            "Int" => (i32::MIN as i64, i32::MAX as i64),
            _ => return Err("Unsupported scalar".into()),
        };
        if n < min || n > max {
            return Err("Integer outside range".into());
        }
        Ok(json!(n))
    }
}
