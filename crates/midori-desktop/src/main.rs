#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::ipc::{InvokeBody, Request, Response};

/// Read a UTF-8 text file picked via the dialog plugin (species TOML import).
#[tauri::command]
async fn read_text_file(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || midori_desktop::read_text_file(&path))
        .await
        .map_err(|e| e.to_string())?
}

/// Save an export payload to disk.
///
/// The request body is a raw frame: `[u32 le path_len][path utf8][payload]`.
/// The path comes from the dialog plugin's save picker, which already reflects
/// a deliberate user choice of destination.
#[tauri::command]
async fn save_export(request: Request<'_>) -> Result<Response, String> {
    let bytes = match request.body() {
        InvokeBody::Raw(bytes) => bytes.to_vec(),
        _ => return Err("Expected a raw export frame".to_string()),
    };
    tauri::async_runtime::spawn_blocking(move || {
        midori_desktop::write_export_frame(&bytes)
            .map(|path| Response::new(path.display().to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}

fn main() {
    let context = tauri::generate_context!();
    tauri::Builder::default()
        .plugin(tools_frontend_host_tauri::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![read_text_file, save_export])
        .run(context)
        .expect("Midori desktop runtime failed");
}
