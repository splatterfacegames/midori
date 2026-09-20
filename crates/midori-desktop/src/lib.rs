//! Midori desktop host — Tauri commands for the workbench.
//!
//! The engine runs in the webview as WASM (midori-wasm). These commands only
//! provide native file access for the import/export flows; all tree generation
//! and glTF encoding happen in the shared engine.

use std::path::PathBuf;

/// Maximum species document size accepted by `read_text_file` (1 MiB).
const MAX_READ_BYTES: u64 = 1024 * 1024;
/// Maximum export payload accepted by `save_export` (512 MiB).
const MAX_WRITE_BYTES: usize = 512 * 1024 * 1024;
/// Maximum path segment inside a `save_export` frame (64 KiB).
const MAX_PATH_BYTES: usize = 64 * 1024;

/// Read a UTF-8 text file picked via the dialog plugin (species TOML import).
pub fn read_text_file(path: &str) -> Result<String, String> {
    let path = PathBuf::from(path);
    let metadata = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if metadata.len() > MAX_READ_BYTES {
        return Err("File exceeds the 1 MiB species document limit".to_string());
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|_| "File is not valid UTF-8".to_string())
}

/// Write an export frame (`[u32 le path_len][path utf8][payload]`) to disk and
/// return the destination path.
pub fn write_export_frame(bytes: &[u8]) -> Result<PathBuf, String> {
    if bytes.len() > MAX_WRITE_BYTES {
        return Err("Export exceeds the 512 MiB payload limit".to_string());
    }
    let (path, payload) = parse_export_frame(bytes)?;
    std::fs::write(&path, payload).map_err(|e| e.to_string())?;
    Ok(path)
}

fn parse_export_frame(bytes: &[u8]) -> Result<(PathBuf, &[u8]), String> {
    if bytes.len() < 4 {
        return Err("Export frame is truncated".to_string());
    }
    let path_len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    if path_len == 0 || path_len > MAX_PATH_BYTES || bytes.len() < 4 + path_len {
        return Err("Export frame path is invalid".to_string());
    }
    let path_text = std::str::from_utf8(&bytes[4..4 + path_len])
        .map_err(|_| "Export path is not valid UTF-8".to_string())?;
    let payload = &bytes[4 + path_len..];
    if payload.is_empty() {
        return Err("Export payload is empty".to_string());
    }
    Ok((PathBuf::from(path_text), payload))
}

#[cfg(test)]
mod tests {
    use super::{parse_export_frame, read_text_file, write_export_frame};

    #[test]
    fn export_frame_roundtrip() {
        let mut frame = Vec::new();
        frame.extend_from_slice(&5u32.to_le_bytes());
        frame.extend_from_slice(b"a.bgl");
        frame.extend_from_slice(&[1, 2, 3, 4]);
        let (path, payload) = parse_export_frame(&frame).unwrap();
        assert_eq!(path.to_str().unwrap(), "a.bgl");
        assert_eq!(payload, &[1, 2, 3, 4]);
    }

    #[test]
    fn export_frame_rejects_garbage() {
        assert!(parse_export_frame(&[]).is_err());
        assert!(parse_export_frame(&[9, 9, 9, 9]).is_err());
        assert!(
            parse_export_frame(&{
                let mut v = 4u32.to_le_bytes().to_vec();
                v.extend_from_slice(b"abcd");
                v
            })
            .is_err()
        );
    }

    #[test]
    fn write_export_frame_persists_payload() {
        let dir = std::env::temp_dir().join(format!("midori-ffi-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("out.bin");
        let mut frame = Vec::new();
        let path_text = target.to_str().unwrap();
        frame.extend_from_slice(&(path_text.len() as u32).to_le_bytes());
        frame.extend_from_slice(path_text.as_bytes());
        frame.extend_from_slice(&[7, 7, 7]);
        let written = write_export_frame(&frame).unwrap();
        assert_eq!(written, target);
        assert_eq!(std::fs::read(&target).unwrap(), &[7, 7, 7]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_text_file_reads_utf8() {
        let dir = std::env::temp_dir().join(format!("midori-read-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("species.toml");
        std::fs::write(&target, "[species]\nname = \"T\"").unwrap();
        let text = read_text_file(target.to_str().unwrap()).unwrap();
        assert!(text.contains("name = \"T\""));
        std::fs::remove_dir_all(&dir).ok();
    }
}
