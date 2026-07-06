//! # midori-ffi
//!
//! C FFI bindings for the Midori procedural tree generator.
//!
//! This crate provides a C-compatible interface for integrating Midori into
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
//! ## Safety
//!
//! All FFI functions are marked as `unsafe` and require proper handling
//! of raw pointers. See the C header file for usage documentation.

use std::ffi::c_void;

/// Opaque handle to a generated tree.
pub type MidoriTreeHandle = *mut c_void;

/// Generate a tree with default parameters.
///
/// # Safety
///
/// The returned handle must be freed with `midori_tree_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midori_tree_generate() -> MidoriTreeHandle {
    // TODO: Implement using midori_core::generate_tree()
    // TODO: Return actual tree handle
    std::ptr::null_mut()
}

/// Free a tree handle.
///
/// # Safety
///
/// The handle must have been returned by `midori_tree_generate` and
/// must not be used after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn midori_tree_free(_handle: MidoriTreeHandle) {
    // TODO: Implement proper cleanup
}

/// Get the version string of the Midori library.
#[unsafe(no_mangle)]
pub extern "C" fn midori_version() -> *const std::ffi::c_char {
    // Include null terminator
    b"0.1.0\0".as_ptr() as *const std::ffi::c_char
}
