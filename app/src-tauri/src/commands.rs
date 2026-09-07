//! The Tauri command surface.
//!
//! These live in their own module so an integration test can drive them
//! through the real invoke handler; see `tests/ipc.rs`.

use std::path::PathBuf;

use midori_ui_domain::EngineIdentity;

use crate::tree;
use tauri::{Manager, State, ipc::Response};

/// Build provenance, stamped in at compile time from the environment the build
/// ran in. This is a carried value, not an attestation: it says what the build
/// script was told, and a caller must not treat it as proof of anything.
pub fn engine_identity() -> EngineIdentity {
    EngineIdentity {
        source_revision: option_env!("MIDORI_SOURCE_REVISION")
            .unwrap_or("unknown")
            .to_owned(),
        build_id: concat!("midori-app ", env!("CARGO_PKG_VERSION")).to_owned(),
    }
}

/// The identifiers of the species TOML built into this binary.
#[tauri::command]
pub fn tree_preset_ids() -> Vec<String> {
    tree::PRESETS
        .iter()
        .map(|(id, _)| (*id).to_owned())
        .collect()
}

/// The exact embedded bytes of one preset.
///
/// Bytes, not a parsed or re-serialized form: the frontend digests exactly what
/// it sends, and the result's `source_sha256` has to agree with it.
#[tauri::command]
pub fn tree_preset_source(id: String) -> Result<Response, String> {
    tree::PRESETS
        .iter()
        .find(|(name, _)| *name == id)
        .map(|(_, bytes)| Response::new(bytes.to_vec()))
        .ok_or_else(|| format!("no preset named {id}"))
}

/// `TreeComputeRequest` in, `TreeComputeResult` out.
#[tauri::command]
pub fn tree_compute(state: State<'_, tree::TreeState>, request: Vec<u8>) -> Result<Response, String> {
    tree::compute(&state, &request).map(Response::new)
}

/// The GLB bytes for a digest named in a result's `ArtifactRef`.
#[tauri::command]
pub fn tree_artifact(state: State<'_, tree::TreeState>, sha256: String) -> Result<Response, String> {
    tree::artifact(&state, &sha256).map(Response::new)
}

fn export_directory<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("exports"))
        .map_err(|error| format!("no writable application data directory: {error}"))
}

/// Write the artifact to the application's export directory.
///
/// Returns the absolute path written, which the frontend shows and then feeds
/// straight back to [`tree_inspect`].
/// Generic over the runtime so `ipc_tests` can drive it under the mock one.
#[tauri::command]
pub fn tree_export<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, tree::TreeState>,
    sha256: String,
    file_name: String,
) -> Result<String, String> {
    let directory = export_directory(&app)?;
    tree::export(&state, &sha256, &file_name, &directory).map(|path| path.display().to_string())
}

/// Reopen a file from disk and report what is actually in it.
#[tauri::command]
pub fn tree_inspect(
    state: State<'_, tree::TreeState>,
    sha256: String,
    path: String,
) -> Result<String, String> {
    tree::inspect(&state, &sha256, std::path::Path::new(&path)).map(|report| report.report())
}

/// The default export name, derived from what the engine generated.
#[tauri::command]
pub fn tree_suggested_name(state: State<'_, tree::TreeState>, sha256: String) -> String {
    tree::suggested_name(&state, &sha256)
}

