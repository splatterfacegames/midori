use crate::{host_codec, registry::Registry};
use serde_json::Value;
use std::{path::PathBuf, sync::Mutex};
use tauri::{Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

struct HostState {
    registry: Mutex<Registry>,
    path: PathBuf,
}
impl HostState {
    fn save(&self, registry: &Registry) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let bytes = serde_json::to_vec(
            &serde_json::json!({"panels":registry.recovery,"layouts":registry.layouts}),
        )
        .map_err(|e| e.to_string())?;
        // Recovery contains only panel/layout intent, never application documents.
        let temporary = self.path.with_extension("tmp");
        std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(temporary, &self.path).map_err(|e| e.to_string())
    }
}

/// The sole IPC command transports generated FlatBuffers, bound to the invoking native window.
#[tauri::command]
async fn exchange<R: Runtime>(
    window: WebviewWindow<R>,
    state: tauri::State<'_, HostState>,
    bytes: Vec<u8>,
) -> Result<Vec<u8>, String> {
    if bytes.len() > 128 * 1024 {
        return Err("Host message limit exceeded".into());
    }
    let request = host_codec().decode_json("HostMessage", &bytes)?;
    let operation = request["operation"].as_str().unwrap_or("");
    let response = {
        let mut registry = state
            .registry
            .lock()
            .map_err(|_| "Host registry unavailable")?;
        if operation == "end_session" {
            // Encode and persist before publishing the candidate; never destroy child windows here.
            let mut encoded = None;
            let response = registry.end_session(window.label(), request.clone(), |candidate| {
                let mut acknowledgement = request.clone();
                acknowledgement["status"] = Value::String("ended".into());
                encoded = Some(host_codec().encode_json("HostMessage", &acknowledgement)?);
                state.save(candidate)
            })?;
            if let Some(encoded) = encoded {
                return Ok(encoded);
            }
            return host_codec().encode_json("HostMessage", &response);
        }
        let response = registry.exchange(window.label(), request.clone())?;
        if matches!(operation, "detach" | "ready" | "return") {
            state.save(&registry)?;
        }
        response
    };
    if operation == "detach" {
        let label = response["destination_id"]
            .as_str()
            .ok_or("Missing destination")?;
        let session = response["session_id"].as_str().ok_or("Missing session")?;
        // Both values are generated UUID-derived identifiers. Callers cannot supply URLs, titles or geometry.
        let route = format!("index.html?hostWindow={label}&hostSession={session}");
        let mut builder =
            WebviewWindowBuilder::new(window.app_handle(), label, WebviewUrl::App(route.into()))
                .title("Project workbench")
                .inner_size(1100.0, 760.0)
                .min_inner_size(480.0, 320.0);
        // WebView2 windows sharing a data directory require consistent browser arguments.
        // Inherit only trusted native app configuration; detach messages cannot set these.
        if let Some(main) = window.app_handle().config().app.windows.iter().find(|config| config.label == "main") {
            if let Some(arguments) = &main.additional_browser_args {
                builder = builder.additional_browser_args(arguments);
            }
        }
        let created = builder.build();
        if let Err(error) = created {
            let mut registry = state
                .registry
                .lock()
                .map_err(|_| "Host registry unavailable")?;
            let mut cancel = request.clone();
            cancel["operation"] = Value::String("cancel".into());
            cancel["transfer_id"] = response["transfer_id"].clone();
            let _ = registry.exchange(window.label(), cancel);
            registry.close(label);
            return Err(format!("Native destination creation failed: {error}"));
        }
    }
    if operation == "return" {
        window.destroy().map_err(|e| e.to_string())?;
    }
    let cancelled = {
        state
            .registry
            .lock()
            .map_err(|_| "Host registry unavailable")?
            .cancelled_destinations()
    };
    for label in cancelled {
        if let Some(child) = window.app_handle().get_webview_window(&label) {
            let _ = child.destroy();
        }
    }
    host_codec().encode_json("HostMessage", &response)
}

/// Register on `tauri::Builder` and grant `tools-frontend-host:default` to main and host-*.
pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("tools-frontend-host")
        .invoke_handler(tauri::generate_handler![exchange])
        .setup(|app, _| {
            let path = app
                .path()
                .app_local_data_dir()?
                .join("tools-frontend-host")
                .join("panel-recovery.json");
            let mut registry = Registry::default();
            if let Ok(bytes) = std::fs::read(&path) {
                // Invalid interrupted writes are ignored; the app's registered IDs remain the fallback.
                if let Ok(recovery) = serde_json::from_slice::<Value>(&bytes) {
                    if let Ok(panels) = serde_json::from_value(recovery["panels"].clone()) {
                        registry.recovery = panels;
                    }
                    if let Ok(layouts) = serde_json::from_value(recovery["layouts"].clone()) {
                        registry.layouts = layouts;
                    }
                }
            }
            app.manage(HostState {
                registry: Mutex::new(registry),
                path,
            });
            Ok(())
        })
        .on_event(|app, event| {
            if let tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::Destroyed,
                ..
            } = event
            {
                let state = app.state::<HostState>();
                if let Ok(mut registry) = state.registry.lock() {
                    registry.close(label);
                    let _ = state.save(&registry);
                };
            }
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn failed_recovery_write_leaves_session_live_and_retry_releases_it() {
        let path = std::env::temp_dir().join(format!("host-release-{}", uuid::Uuid::new_v4()));
        // A file where the recovery parent directory should be forces a real I/O error.
        std::fs::write(&path, b"blocked").unwrap();
        let state = HostState { registry: Mutex::new(Registry::default()), path: path.join("recovery.json") };
        let mut registry = state.registry.lock().unwrap();
        let first = registry.exchange("main", json!({"operation":"bootstrap","app_id":"app","project_id":"p1","panel_ids":["editor"]})).unwrap();
        let mut release = first.clone(); release["operation"] = json!("end_session"); release["layout"] = json!("layout");
        let bytes = host_codec().encode_json("HostMessage", &release).unwrap();
        let release = host_codec().decode_json("HostMessage", &bytes).unwrap();
        assert!(registry.end_session("main", release.clone(), |candidate| state.save(candidate)).is_err());
        assert!(registry.exchange("main", first.clone()).is_ok());
        std::fs::remove_file(&path).unwrap();
        registry.end_session("main", release, |candidate| state.save(candidate)).unwrap();
        let recovery: Value = serde_json::from_slice(&std::fs::read(&state.path).unwrap()).unwrap();
        assert_eq!(recovery["panels"]["app/p1"], json!(["editor"]));
        assert_eq!(recovery["layouts"]["app/p1"], "layout");
        assert!(registry.exchange("main", first).is_err());
        std::fs::remove_file(&state.path).unwrap();
        std::fs::remove_dir(&path).unwrap();
    }
}
