//! Midori tree workbench: native host.
//!
//! Window ownership and panel transfer come from the shared
//! `tools_frontend_host_tauri` plugin, exactly as the reference workbench wires
//! them. Everything Midori-specific lives behind the commands in [`tree`], and
//! every one of those that describes a computation speaks the generated
//! FlatBuffers contract rather than an ad-hoc JSON shape.

pub mod commands;
pub mod tree;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Shared window host: real native secondary windows and the panel
        // ownership registry.
        .plugin(tools_frontend_host_tauri::init())
        .manage(tree::TreeState::new(commands::engine_identity()))
        .invoke_handler(tauri::generate_handler![
            commands::tree_preset_ids,
            commands::tree_preset_source,
            commands::tree_compute,
            commands::tree_artifact,
            commands::tree_export,
            commands::tree_inspect,
            commands::tree_suggested_name,
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
