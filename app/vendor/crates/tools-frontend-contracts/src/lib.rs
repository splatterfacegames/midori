#[path = "generated/control_generated.rs"]
#[allow(unused_imports, clippy::all)]
pub mod control_generated;
pub use control_generated::app::control::v1::*;
mod preflight;
pub use preflight::{DecodeLimits,DecodeBudget};
mod projection;
pub use projection::Codec;
mod catalog;
pub use catalog::{CatalogDispatch,RegisteredCodec};
#[path = "generated/projection_generated.rs"]
#[allow(clippy::all)]
mod projection_generated;
/// Standalone common-record codec. Apps inject their descriptor and registered payload codecs.
pub fn common_codec() -> Codec<'static> {
    static METADATA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    Codec {
        metadata: METADATA.get_or_init(|| {
            serde_json::from_str(include_str!("../../../schemas/generated/projection.json"))
                .expect("generated common descriptor")
        }),
        encode_raw: projection_generated::encode_raw,
        decode_raw: projection_generated::decode_raw,
    }
}

pub mod releases;
