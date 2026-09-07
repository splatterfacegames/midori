//! Allocation-free descriptor traversal before generated serde accessors.
use serde_json::Value;
#[derive(Debug, Clone, Copy)]
pub struct DecodeLimits {
    pub max_bytes: usize,
    pub max_depth: usize,
    pub max_tables: usize,
    pub max_apparent_bytes: usize,
    pub max_string_bytes: usize,
    pub max_detail_bytes: usize,
}
impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024,
            max_depth: 64,
            max_tables: 100_000,
            max_apparent_bytes: 64 * 1024 * 1024,
            max_string_bytes: 16 * 1024 * 1024,
            max_detail_bytes: 16 * 1024 * 1024,
        }
    }
}
impl DecodeLimits {
    /// Callers may tighten limits, but cannot disable the runtime's hard safety bounds.
    pub fn validate(&self) -> Result<(), String> {
        if !(8..=16 * 1024 * 1024).contains(&self.max_bytes)
            || !(1..=64).contains(&self.max_depth)
            || !(1..=100_000).contains(&self.max_tables)
            || !(1..=64 * 1024 * 1024).contains(&self.max_apparent_bytes)
            || !(1..=16 * 1024 * 1024).contains(&self.max_string_bytes)
            || !(1..=16 * 1024 * 1024).contains(&self.max_detail_bytes)
        {
            return Err("Invalid decode limits".into());
        }
        Ok(())
    }
}
/// Share one budget across all roots belonging to a decoded message.
#[derive(Debug, Default)]
pub struct DecodeBudget {
    pub tables: usize,
    pub apparent_bytes: usize,
}
struct Reader<'a> {
    bytes: &'a [u8],
    metadata: &'a Value,
    limits: &'a DecodeLimits,
    budget: &'a mut DecodeBudget,
}
impl Reader<'_> {
    fn bytes(&self, p: usize, n: usize) -> Result<&[u8], String> {
        self.bytes
            .get(p..p.checked_add(n).ok_or("Buffer bounds violation")?)
            .ok_or("Buffer bounds violation".into())
    }
    #[allow(clippy::manual_is_multiple_of)]
    fn aligned(&self, p: usize, n: usize) -> Result<(), String> {
        if p % n == 0 {
            Ok(())
        } else {
            Err("Misaligned field".into())
        }
    }
    fn u16(&self, p: usize) -> Result<u16, String> {
        self.aligned(p, 2)?;
        Ok(u16::from_le_bytes(self.bytes(p, 2)?.try_into().unwrap()))
    }
    fn u32(&self, p: usize) -> Result<u32, String> {
        self.aligned(p, 4)?;
        Ok(u32::from_le_bytes(self.bytes(p, 4)?.try_into().unwrap()))
    }
    fn offset(&self, p: usize) -> Result<usize, String> {
        let n = self.u32(p)? as usize;
        if n < 4 {
            return Err("Invalid offset".into());
        }
        p.checked_add(n).ok_or("Buffer bounds violation".into())
    }
    fn charge(&mut self, n: usize) -> Result<(), String> {
        self.budget.apparent_bytes = self
            .budget
            .apparent_bytes
            .checked_add(n)
            .ok_or("Apparent size limit exceeded")?;
        if self.budget.apparent_bytes > self.limits.max_apparent_bytes {
            Err("Apparent size limit exceeded".into())
        } else {
            Ok(())
        }
    }
    fn width(base: &str) -> Result<usize, String> {
        match base {
            "Bool" | "Byte" | "UByte" => Ok(1),
            "Short" | "UShort" => Ok(2),
            "Int" | "UInt" | "Float" | "String" | "Obj" | "Vector" => Ok(4),
            "Long" | "ULong" | "Double" => Ok(8),
            _ => Err("Unsupported descriptor type".into()),
        }
    }
    fn scalar(&self, base: &str, p: usize) -> Result<(), String> {
        let width = Self::width(base)?;
        self.aligned(p, width)?;
        let bytes = self.bytes(p, width)?;
        match base {
            "Bool" if bytes[0] > 1 => Err("Invalid bool".into()),
            "Float" if !f32::from_le_bytes(bytes.try_into().unwrap()).is_finite() => {
                Err("Expected finite physical quantity".into())
            }
            "Double" if !f64::from_le_bytes(bytes.try_into().unwrap()).is_finite() => {
                Err("Expected finite physical quantity".into())
            }
            _ => Ok(()),
        }
    }
    fn string(&mut self, p: usize, field: &str) -> Result<(), String> {
        let start = self.offset(p)?;
        let n = self.u32(start)? as usize;
        let limit = if matches!(field, "message" | "detail") {
            self.limits.max_detail_bytes
        } else {
            self.limits.max_string_bytes
        };
        if n > limit {
            return Err("String byte limit exceeded".into());
        }
        let end = start.checked_add(4).ok_or("Buffer bounds violation")?;
        self.charge(n.checked_add(5).ok_or("Apparent size limit exceeded")?)?;
        let bytes = self.bytes(end, n.checked_add(1).ok_or("Buffer bounds violation")?)?;
        if bytes[n] != 0 {
            return Err("Missing string terminator".into());
        }
        std::str::from_utf8(&bytes[..n]).map_err(|_| "Invalid UTF-8")?;
        Ok(())
    }
    #[allow(clippy::manual_is_multiple_of)]
    fn table(&mut self, name: &str, p: usize, depth: usize) -> Result<(), String> {
        if depth >= self.limits.max_depth {
            return Err("Maximum nesting depth exceeded".into());
        }
        self.budget.tables = self
            .budget
            .tables
            .checked_add(1)
            .ok_or("Table limit exceeded")?;
        if self.budget.tables > self.limits.max_tables {
            return Err("Table limit exceeded".into());
        }
        let signed = self.u32(p)? as i32;
        let vt = usize::try_from(p as i64 - signed as i64).map_err(|_| "Invalid vtable")?;
        let size = self.u16(vt)? as usize;
        let object_size = self.u16(vt.checked_add(2).ok_or("Buffer bounds violation")?)? as usize;
        if size < 4 || size % 2 != 0 || object_size < 4 {
            return Err("Invalid vtable".into());
        }
        self.bytes(vt, size)?;
        self.bytes(p, object_size)?;
        self.charge(size + object_size)?;
        let fields = self.metadata["objects"][name]["fields"]
            .as_array()
            .ok_or("Unknown root")?;
        for field in fields {
            if field["deprecated"] == true {
                continue;
            }
            let id = usize::try_from(field["id"].as_u64().ok_or("Invalid field ID")?)
                .map_err(|_| "Invalid field ID")?;
            let slot = id
                .checked_mul(2)
                .and_then(|n| n.checked_add(4))
                .ok_or("Invalid field ID")?;
            let offset = if slot < size {
                self.u16(vt + slot)? as usize
            } else {
                0
            };
            if offset == 0 {
                if field["required"] == true {
                    return Err("Required field absent".into());
                }
                continue;
            }
            let t = &field["type"];
            let base = t["base_type"].as_str().ok_or("Invalid descriptor type")?;
            if offset < 4 || offset + Self::width(base)? > object_size {
                return Err("Field outside table".into());
            }
            let at = p + offset;
            let field_name = field["name"].as_str().ok_or("Invalid field name")?;
            match base {
                "Obj" => self.table(
                    t["object"].as_str().ok_or("Missing object type")?,
                    self.offset(at)?,
                    depth + 1,
                )?,
                "String" => self.string(at, field_name)?,
                "Vector" => {
                    let start = self.offset(at)?;
                    let n = self.u32(start)? as usize;
                    if n > 1_000_000 {
                        return Err("Vector limit exceeded".into());
                    }
                    let element = t["element"].as_str().ok_or("Missing vector type")?;
                    let width = Self::width(element)?;
                    let data = start.checked_add(4).ok_or("Buffer bounds violation")?;
                    let size = n.checked_mul(width).ok_or("Buffer bounds violation")?;
                    self.aligned(data, width)?;
                    self.bytes(data, size)?;
                    self.charge(4 + size)?;
                    for i in 0..n {
                        let at = data + i * width;
                        match element {
                            "Obj" => self.table(
                                t["object"].as_str().ok_or("Missing object type")?,
                                self.offset(at)?,
                                depth + 1,
                            )?,
                            "String" => self.string(at, field_name)?,
                            _ => self.scalar(element, at)?,
                        }
                    }
                }
                _ => self.scalar(base, at)?,
            }
        }
        Ok(())
    }
}
pub(crate) fn verify(
    metadata: &Value,
    root: &str,
    bytes: &[u8],
    limits: &DecodeLimits,
    budget: &mut DecodeBudget,
    depth: usize,
) -> Result<(), String> {
    limits.validate()?;
    if bytes.len() < 4 || bytes.len() > limits.max_bytes || bytes.len() > 16 * 1024 * 1024 {
        return Err("Invalid buffer size".into());
    }
    let mut reader = Reader {
        bytes,
        metadata,
        limits,
        budget,
    };
    let root_position = reader.offset(0)?;
    reader.table(root, root_position, depth)
}
