//! Immutable codec/v2 normalization and verifier closure.
mod preflight;
pub use preflight::{DecodeBudget, DecodeLimits};
mod projection;
pub use projection::Codec;
