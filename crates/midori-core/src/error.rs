//! Umbrella error type for the midori-core crate.
//!
//! `Error` unifies the per-subsystem error enums so library consumers can
//! handle one type with `From` conversions instead of matching each call
//! site's error enum separately.

use crate::{
    export::ExportError, generation::GenerationError, nature::NaturePatchError,
    species::SpeciesError, textures::TextureError,
};

/// Unified error type covering every fallible midori-core operation.
#[derive(Debug)]
pub enum Error {
    /// Species document parse, IO, or semantic validation failure.
    Species(SpeciesError),
    /// Nature patch parse, IO, or validation failure.
    NaturePatch(NaturePatchError),
    /// Generation resource budget exceeded.
    Generation(GenerationError),
    /// glTF export failure.
    Export(ExportError),
    /// Procedural texture generation failure.
    Texture(TextureError),
    /// Filesystem IO failure.
    Io(std::io::Error),
}

impl From<SpeciesError> for Error {
    fn from(err: SpeciesError) -> Self {
        Self::Species(err)
    }
}

impl From<NaturePatchError> for Error {
    fn from(err: NaturePatchError) -> Self {
        Self::NaturePatch(err)
    }
}

impl From<GenerationError> for Error {
    fn from(err: GenerationError) -> Self {
        Self::Generation(err)
    }
}

impl From<ExportError> for Error {
    fn from(err: ExportError) -> Self {
        Self::Export(err)
    }
}

impl From<TextureError> for Error {
    fn from(err: TextureError) -> Self {
        Self::Texture(err)
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Species(e) => e.fmt(f),
            Self::NaturePatch(e) => e.fmt(f),
            Self::Generation(e) => e.fmt(f),
            Self::Export(e) => e.fmt(f),
            Self::Texture(e) => e.fmt(f),
            Self::Io(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Species(e) => e.source(),
            Self::NaturePatch(e) => e.source(),
            Self::Generation(e) => e.source(),
            Self::Export(e) => e.source(),
            Self::Texture(e) => e.source(),
            Self::Io(e) => Some(e),
        }
    }
}
