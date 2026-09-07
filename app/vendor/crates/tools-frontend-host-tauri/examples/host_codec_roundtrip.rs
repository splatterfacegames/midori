//! Cross-language fixture runner; uses the same generated host codec as native IPC.
fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&args[1]).map_err(|e| e.to_string())?;
    let codec = tools_frontend_host_tauri::host_codec();
    let value = codec.decode_json("HostMessage", &bytes)?;
    let encoded = codec.encode_json("HostMessage", &value)?;
    std::fs::write(&args[2], encoded).map_err(|e| e.to_string())?;
    println!("{value}");
    Ok(())
}
