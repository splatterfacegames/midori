//! # midori-ffi
//!
//! C FFI bindings for the midori procedural tree generator.
//!
//! This crate provides a C-compatible interface for integrating midori into
//! game engines and other native applications. It can be compiled as both
//! a dynamic library (cdylib) and a static library (staticlib).
//!
//! ## Supported Engines
//!
//! - Unreal Engine (via plugin)
//! - Unity (via native plugin)
//! - Godot (via GDExtension)
//! - Custom engines via direct C API
//!
//! ## Usage
//!
//! ```c
//! MidoriSpecies* species = midori_species_parse(toml_ptr, toml_len);
//! MidoriTree* tree = midori_tree_generate(species, seed);
//! int status = midori_tree_export_glb(species, tree, path_ptr, path_len);
//! midori_tree_free(tree);
//! midori_species_free(species);
//! ```
//!
//! ## Safety
//!
//! All FFI functions are marked as `unsafe` and require proper handling
//! of raw pointers. String arguments are `(ptr, len)` pairs of UTF-8 bytes.

use midori_core::{
    ExportConfig, Species, TextureSet, Tree, export_lod_meshes, generate_tree,
    lod::LodGenerationConfig,
};
use std::ffi::{c_char, c_int};
use std::path::{Path, PathBuf};
use std::slice;

/// Opaque handle to a parsed species definition.
pub type MidoriSpeciesHandle = *mut Species;

/// Opaque handle to a generated tree.
pub type MidoriTreeHandle = *mut Tree;

/// Status codes returned by fallible FFI calls.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidoriStatus {
    /// Operation completed successfully.
    Ok = 0,
    /// A null or invalid handle/argument was supplied.
    InvalidArgument = 1,
    /// The species TOML failed to parse or was not valid UTF-8.
    ParseError = 2,
    /// The tree could not be exported.
    ExportError = 3,
}

/// Parse a species TOML document.
///
/// # Safety
///
/// `toml_ptr` must point to `toml_len` readable bytes, or be null with
/// `toml_len == 0`. Returns null on parse failure; free the result with
/// `midori_species_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midori_species_parse(
    toml_ptr: *const u8,
    toml_len: usize,
) -> MidoriSpeciesHandle {
    if toml_ptr.is_null() || toml_len == 0 {
        return std::ptr::null_mut();
    }
    let bytes = unsafe { slice::from_raw_parts(toml_ptr, toml_len) };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return std::ptr::null_mut();
    };
    match Species::from_toml(text) {
        Ok(species) => Box::into_raw(Box::new(species)),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a species handle.
///
/// # Safety
///
/// The handle must have been returned by `midori_species_parse` and must not be
/// used after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midori_species_free(handle: MidoriSpeciesHandle) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}

/// Generate a tree for the given species and seed.
///
/// # Safety
///
/// `species` must be a valid handle returned by `midori_species_parse`.
/// The returned tree borrows nothing from the species and must be freed with
/// `midori_tree_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midori_tree_generate(
    species: MidoriSpeciesHandle,
    seed: u64,
) -> MidoriTreeHandle {
    if species.is_null() {
        return std::ptr::null_mut();
    }
    let species = unsafe { &*species };
    Box::into_raw(Box::new(generate_tree(species, seed)))
}

/// Number of stems in a generated tree.
///
/// # Safety
///
/// `tree` must be a valid handle returned by `midori_tree_generate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midori_tree_stem_count(tree: MidoriTreeHandle) -> u32 {
    if tree.is_null() {
        return 0;
    }
    unsafe { &*tree }.stems.len() as u32
}

/// Number of leaves in a generated tree.
///
/// # Safety
///
/// `tree` must be a valid handle returned by `midori_tree_generate`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midori_tree_leaf_count(tree: MidoriTreeHandle) -> u32 {
    if tree.is_null() {
        return 0;
    }
    unsafe { &*tree }.leaves.len() as u32
}

/// Export a generated tree to a GLB file, with all LOD levels from the
/// species' `[lod]` configuration.
///
/// # Safety
///
/// `species` and `tree` must be valid handles. `path_ptr`/`path_len` must
/// describe a UTF-8 filesystem path. The file is created or overwritten.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midori_tree_export_glb(
    species: MidoriSpeciesHandle,
    tree: MidoriTreeHandle,
    path_ptr: *const u8,
    path_len: usize,
) -> c_int {
    if species.is_null() || tree.is_null() || path_ptr.is_null() || path_len == 0 {
        return MidoriStatus::InvalidArgument as c_int;
    }
    let bytes = unsafe { slice::from_raw_parts(path_ptr, path_len) };
    let Ok(path_str) = std::str::from_utf8(bytes) else {
        return MidoriStatus::InvalidArgument as c_int;
    };
    let species = unsafe { &*species };
    let tree = unsafe { &*tree };

    export_tree_glb(species, tree, path_str, None)
}

/// Export a generated tree to a GLB file with the species' material maps
/// embedded (bark albedo+normal, leaf card, baked impostor atlases).
///
/// `base_dir_ptr`/`base_dir_len` give the directory `[textures]` file-slot
/// paths resolve against (typically the directory the species document came
/// from); pass null/0 to generate all maps procedurally.
///
/// # Safety
///
/// Same contract as `midori_tree_export_glb`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midori_tree_export_glb_textured(
    species: MidoriSpeciesHandle,
    tree: MidoriTreeHandle,
    path_ptr: *const u8,
    path_len: usize,
    base_dir_ptr: *const u8,
    base_dir_len: usize,
) -> c_int {
    if species.is_null() || tree.is_null() || path_ptr.is_null() || path_len == 0 {
        return MidoriStatus::InvalidArgument as c_int;
    }
    let bytes = unsafe { slice::from_raw_parts(path_ptr, path_len) };
    let Ok(path_str) = std::str::from_utf8(bytes) else {
        return MidoriStatus::InvalidArgument as c_int;
    };
    let species = unsafe { &*species };
    let tree = unsafe { &*tree };

    let base_dir = if base_dir_ptr.is_null() || base_dir_len == 0 {
        PathBuf::new()
    } else {
        let bytes = unsafe { slice::from_raw_parts(base_dir_ptr, base_dir_len) };
        match std::str::from_utf8(bytes) {
            Ok(dir) => PathBuf::from(dir),
            Err(_) => return MidoriStatus::InvalidArgument as c_int,
        }
    };
    let textures = match TextureSet::resolve(species, &base_dir) {
        Ok(set) => Some(set),
        Err(_) => return MidoriStatus::ExportError as c_int,
    };
    export_tree_glb(species, tree, path_str, textures)
}

fn export_tree_glb(
    species: &Species,
    tree: &Tree,
    path: &str,
    textures: Option<TextureSet>,
) -> c_int {
    let lod_config = LodGenerationConfig::from_species(species);
    let lods = midori_core::lod::generate_lod_meshes_with_config(tree, species, &lod_config);
    let config = ExportConfig {
        textures,
        ..ExportConfig::default()
    };
    match export_lod_meshes(&lods, Path::new(path), &config) {
        Ok(()) => MidoriStatus::Ok as c_int,
        Err(_) => MidoriStatus::ExportError as c_int,
    }
}

/// Free a tree handle.
///
/// # Safety
///
/// The handle must have been returned by `midori_tree_generate` and must not be
/// used after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midori_tree_free(handle: MidoriTreeHandle) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}

/// Get the version string of the midori library.
#[unsafe(no_mangle)]
pub extern "C" fn midori_version() -> *const c_char {
    c"0.1.0".as_ptr()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OAK_TOML: &str = r#"
[species]
name = "FFI Oak"

[trunk]
height = 5.0
radius = 0.4

[branches.level1]
count = 4
length = 2.5
"#;

    #[test]
    fn parse_generate_export_roundtrip() {
        unsafe {
            let species = midori_species_parse(OAK_TOML.as_ptr(), OAK_TOML.len());
            assert!(!species.is_null());

            let tree = midori_tree_generate(species, 42);
            assert!(!tree.is_null());
            assert!(midori_tree_stem_count(tree) > 0);

            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("ffi_tree.glb");
            let path_str = path.to_str().unwrap();
            let status = midori_tree_export_glb(species, tree, path_str.as_ptr(), path_str.len());
            assert_eq!(status, MidoriStatus::Ok as c_int);
            let bytes = std::fs::read(&path).unwrap();
            assert_eq!(&bytes[0..4], b"glTF");

            midori_tree_free(tree);
            midori_species_free(species);
        }
    }

    #[test]
    fn invalid_arguments_are_rejected() {
        unsafe {
            assert!(midori_species_parse(std::ptr::null(), 0).is_null());
            assert!(midori_species_parse(b"not toml".as_ptr(), 8).is_null());
            assert!(midori_tree_generate(std::ptr::null_mut(), 1).is_null());
            assert_eq!(midori_tree_stem_count(std::ptr::null_mut()), 0);
            assert_eq!(
                midori_tree_export_glb(
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    b"x".as_ptr(),
                    1
                ),
                MidoriStatus::InvalidArgument as c_int
            );
            midori_tree_free(std::ptr::null_mut());
            midori_species_free(std::ptr::null_mut());
        }
    }

    #[test]
    fn version_is_nul_terminated() {
        let version = midori_version();
        assert!(!version.is_null());
        let text = unsafe { std::ffi::CStr::from_ptr(version) };
        assert_eq!(text.to_str().unwrap(), "0.1.0");
    }
}
