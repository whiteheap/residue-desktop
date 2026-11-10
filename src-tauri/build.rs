include!("commands.rs");

fn main() {
    // With an app manifest, Tauri checks every call of an app command against
    // the capabilities, as it does a plugin's: the window runs only the
    // commands capabilities/default.json grants.
    println!("cargo:rerun-if-changed=commands.rs");
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri-build failed");
}
