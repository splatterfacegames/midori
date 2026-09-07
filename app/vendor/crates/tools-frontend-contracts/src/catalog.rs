//! Exact catalog selection before nested decoding. Retained handles own their original policy.
use serde_json::{json, Value};
use std::collections::BTreeMap;
#[derive(Clone, Copy)]
pub struct RegisteredCodec<'a> {
    pub metadata: &'a Value,
    pub encode_json: fn(&str, &Value) -> Result<Vec<u8>, String>,
    pub decode_json: fn(&str, &[u8]) -> Result<Value, String>,
}
pub struct CatalogDispatch<'a> {
    current: RegisteredCodec<'a>,
    bindings: BTreeMap<(String,String),(RegisteredCodec<'a>,Value)>,
}
impl<'a> CatalogDispatch<'a> {
    pub fn new(current:RegisteredCodec<'a>, retained:&[RegisteredCodec<'a>])->Result<Self,String>{
        let mut bindings=BTreeMap::new();
        for (index, handle) in retained.iter().chain(std::iter::once(&current)).enumerate() {
            for (key, root) in handle.metadata["catalog"].as_object().ok_or("Invalid catalog")? {
                let (revision,kind)=key.split_once('|').ok_or("Invalid binding")?;
                if revision.is_empty() || kind.is_empty() || kind.contains('|') {return Err("Invalid binding".into())}
                let key=(revision.to_owned(),kind.to_owned());
                if bindings.contains_key(&key) {
                    if index<retained.len(){return Err("Duplicate retained binding".into())} else {continue}
                }
                let root=root.as_str().ok_or("Invalid payload root")?;
                let namespace=handle.metadata["objects"][root]["namespace"].as_str().ok_or("Missing payload descriptor")?;
                let digest=handle.metadata["descriptor_sha256"].as_str().ok_or("Missing descriptor digest")?;
                let registration=json!({"catalog_revision":revision,"kind":kind,"input_type":format!("{namespace}.{root}"),"descriptor_sha256":digest,"input_json_schema":{"$ref":format!("json-schema.json#/$defs/{root}")}});
                bindings.insert(key,(*handle,registration));
            }
        }
        Ok(Self{current,bindings})
    }
    fn selected(&self, revision:&str,kind:&str)->Result<&(RegisteredCodec<'a>,Value),String>{
        self.bindings.get(&(revision.to_owned(),kind.to_owned())).ok_or("SCHEMA_INCOMPATIBLE: unknown catalog/kind registration".into())
    }
    pub fn operation_registration(&self,revision:&str,kind:&str)->Result<Value,String>{Ok(self.selected(revision,kind)?.1.clone())}
    pub fn catalog(&self)->Value{Value::Array(self.bindings.values().map(|(_,v)|v.clone()).collect())}
    fn handle(&self,value:&Value)->Result<RegisteredCodec<'a>,String>{Ok(self.selected(value["catalog_revision"].as_str().unwrap_or(""),value["kind"].as_str().unwrap_or(""))?.0)}
    pub fn encode_json(&self,root:&str,value:&Value)->Result<Vec<u8>,String>{
        let handle=if matches!(root,"OperationRequest"|"OperationReceipt") {self.handle(value)?} else {self.current};
        (handle.encode_json)(root,value)
    }
    pub fn decode_json(&self,root:&str,bytes:&[u8])->Result<Value,String>{
        let handle=if matches!(root,"OperationRequest"|"OperationReceipt") {self.handle(&crate::releases::common_v1::inspect(root,bytes)?)?} else {self.current};
        (handle.decode_json)(root,bytes)
    }
}
