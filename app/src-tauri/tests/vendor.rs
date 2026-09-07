//! The vendored shared crates must stay exactly what was vendored.
//!
//! `tools-frontend-host-tauri` is not published and the reviewed checkout it
//! came from is not a Git repository, so there is no version or rev to pin it
//! to. Vendoring the source into `app/vendor/` makes it immutable and auditable
//! instead — but only if something checks. This does.
//!
//! `PROVENANCE.json` records the source path, the capture time, a SHA-256 for
//! every vendored file, and a tree digest over all of them. A local edit to a
//! vendored crate — the exact thing a bare path dependency into someone else's
//! working tree invites — fails these tests.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn vendor_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../vendor")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Every file under `app/vendor/` except the manifest, keyed by forward-slash
/// relative path so the digest is the same on every platform.
fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
        .map(|entry| entry.expect("dir entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk(&path, root, out);
        } else {
            let relative = path
                .strip_prefix(root)
                .expect("under the vendor root")
                .to_string_lossy()
                .replace('\\', "/");
            if relative != "PROVENANCE.json" {
                out.insert(relative, path);
            }
        }
    }
}

fn manifest() -> serde_json::Value {
    let path = vendor_root().join("PROVENANCE.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&text).expect("PROVENANCE.json is JSON")
}

#[test]
fn every_vendored_file_matches_its_recorded_digest() {
    let root = vendor_root();
    let manifest = manifest();
    let recorded = manifest["files"].as_object().expect("files map");

    let mut found = BTreeMap::new();
    walk(&root, &root, &mut found);

    // No file added, none removed.
    let recorded_names: Vec<&String> = recorded.keys().collect();
    let found_names: Vec<&String> = found.keys().collect();
    assert_eq!(
        found_names, recorded_names,
        "the vendored file list no longer matches PROVENANCE.json"
    );
    assert_eq!(found.len(), manifest["file_count"].as_u64().unwrap() as usize);

    // ...and none edited.
    for (relative, path) in &found {
        let actual = hex(&Sha256::digest(std::fs::read(path).unwrap()));
        assert_eq!(
            actual,
            recorded[relative].as_str().unwrap(),
            "vendored file {relative} has been modified"
        );
    }
}

#[test]
fn the_tree_digest_covers_the_whole_vendored_tree() {
    // Recomputed the same way the manifest was written: sha256 over
    // `<path>\0<file sha256>\n` for every file, ordered by path. A per-file
    // check alone would not catch a file being swapped for another that is also
    // in the manifest.
    let root = vendor_root();
    let mut found = BTreeMap::new();
    walk(&root, &root, &mut found);

    let mut hasher = Sha256::new();
    for (relative, path) in &found {
        let digest = hex(&Sha256::digest(std::fs::read(path).unwrap()));
        hasher.update(relative.as_bytes());
        hasher.update([0u8]);
        hasher.update(digest.as_bytes());
        hasher.update(b"\n");
    }

    assert_eq!(
        hex(&hasher.finalize()),
        manifest()["tree_sha256"].as_str().unwrap(),
        "the vendored tree digest has changed"
    );
}

#[test]
fn the_manifest_records_where_the_source_came_from() {
    let manifest = manifest();
    // Provenance, not attestation: this says where the bytes were copied from,
    // and the digests say they have not moved since. It does not say the source
    // was itself trustworthy.
    assert!(
        manifest["source"]
            .as_str()
            .unwrap()
            .contains("lab-shared-main-bdade495"),
        "the recorded source is not the reviewed shared checkout"
    );
    assert_eq!(manifest["source_is_git_repository"], serde_json::json!(false));
    let crates: Vec<&str> = manifest["crates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(
        crates,
        vec!["tools-frontend-host-tauri", "tools-frontend-contracts"]
    );
    // The crate the application actually links must be one of them.
    assert!(
        vendor_root()
            .join("crates/tools-frontend-host-tauri/Cargo.toml")
            .is_file()
    );
}

#[test]
fn the_vendored_crate_is_the_one_cargo_builds() {
    // Guards against the manifest and the dependency drifting apart: a path
    // dependency edited back to the shared checkout would leave these tests
    // passing while the build used unvendored source.
    let cargo = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
    )
    .unwrap();
    assert!(
        cargo.contains(r#"tools-frontend-host-tauri = { path = "../vendor/crates/tools-frontend-host-tauri" }"#),
        "src-tauri/Cargo.toml no longer depends on the vendored crate"
    );
    assert!(
        !cargo.contains("lab-shared-main"),
        "src-tauri/Cargo.toml still references the shared checkout by path"
    );
}
