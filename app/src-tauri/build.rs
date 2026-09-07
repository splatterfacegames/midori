fn main() {
    // Carried, not attested: the value the build environment supplied for the
    // engine source revision. `unknown` when nothing supplied one.
    println!("cargo:rerun-if-env-changed=MIDORI_SOURCE_REVISION");
    tauri_build::build()
}
