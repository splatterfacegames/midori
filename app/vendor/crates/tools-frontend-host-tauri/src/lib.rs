#[path = "generated/window_host_generated.rs"]
#[allow(unused_imports, clippy::all)]
mod generated;
pub use generated::tools_frontend::host::*;
mod native;
#[path = "generated/projection_generated.rs"]
#[allow(clippy::all)]
mod projection_generated;
mod registry;
pub use native::init;
pub fn host_codec() -> tools_frontend_contracts::Codec<'static> {
    static METADATA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    tools_frontend_contracts::Codec {
        metadata: METADATA.get_or_init(|| {
            serde_json::from_str(include_str!(
                "../../../schemas/host/generated/projection.json"
            ))
            .unwrap()
        }),
        encode_raw: projection_generated::encode_raw,
        decode_raw: projection_generated::decode_raw,
    }
}
