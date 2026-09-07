//! Midori tree workbench: native host.
//!
//! Window ownership and panel transfer come from the shared
//! `tools_frontend_host_tauri` plugin, exactly as the reference workbench wires
//! them. Everything Midori-specific lives behind the commands in [`tree`], and
//! every one of those that describes a computation speaks the generated
//! FlatBuffers contract rather than an ad-hoc JSON shape.

pub mod tree;

use std::path::PathBuf;

use midori_ui_domain::EngineIdentity;
use tauri::{Manager, State, ipc::Response};

/// Build provenance, stamped in at compile time from the environment the build
/// ran in. This is a carried value, not an attestation: it says what the build
/// script was told, and a caller must not treat it as proof of anything.
fn engine_identity() -> EngineIdentity {
    EngineIdentity {
        source_revision: option_env!("MIDORI_SOURCE_REVISION")
            .unwrap_or("unknown")
            .to_owned(),
        build_id: concat!("midori-app ", env!("CARGO_PKG_VERSION")).to_owned(),
    }
}

/// The identifiers of the species TOML built into this binary.
#[tauri::command]
fn tree_preset_ids() -> Vec<String> {
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
fn tree_preset_source(id: String) -> Result<Response, String> {
    tree::PRESETS
        .iter()
        .find(|(name, _)| *name == id)
        .map(|(_, bytes)| Response::new(bytes.to_vec()))
        .ok_or_else(|| format!("no preset named {id}"))
}

/// `TreeComputeRequest` in, `TreeComputeResult` out.
#[tauri::command]
fn tree_compute(state: State<'_, tree::TreeState>, request: Vec<u8>) -> Result<Response, String> {
    tree::compute(&state, &request).map(Response::new)
}

/// The GLB bytes for a digest named in a result's `ArtifactRef`.
#[tauri::command]
fn tree_artifact(state: State<'_, tree::TreeState>, sha256: String) -> Result<Response, String> {
    tree::artifact(&state, &sha256).map(Response::new)
}

fn export_directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("exports"))
        .map_err(|error| format!("no writable application data directory: {error}"))
}

/// Write the artifact to the application's export directory.
///
/// Returns the absolute path written, which the frontend shows and then feeds
/// straight back to [`tree_inspect`].
#[tauri::command]
fn tree_export(
    app: tauri::AppHandle,
    state: State<'_, tree::TreeState>,
    sha256: String,
    file_name: String,
) -> Result<String, String> {
    let directory = export_directory(&app)?;
    tree::export(&state, &sha256, &file_name, &directory).map(|path| path.display().to_string())
}

/// Reopen a file from disk and report what is actually in it.
#[tauri::command]
fn tree_inspect(
    state: State<'_, tree::TreeState>,
    sha256: String,
    path: String,
) -> Result<String, String> {
    tree::inspect(&state, &sha256, std::path::Path::new(&path)).map(|report| report.report())
}

/// The default export name, derived from what the engine generated.
#[tauri::command]
fn tree_suggested_name(state: State<'_, tree::TreeState>, sha256: String) -> String {
    tree::suggested_name(&state, &sha256)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Shared window host: real native secondary windows and the panel
        // ownership registry.
        .plugin(tools_frontend_host_tauri::init())
        .manage(tree::TreeState::new(engine_identity()))
        .invoke_handler(tauri::generate_handler![
            tree_preset_ids,
            tree_preset_source,
            tree_compute,
            tree_artifact,
            tree_export,
            tree_inspect,
            tree_suggested_name,
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
