//! residue desktop: a window over the residue CLI. Every check, size and
//! removal comes from the bundled `residue` binary; this crate only runs it
//! with explicit arguments and passes its JSON to the window.

use std::path::PathBuf;
use std::process::Command;

/// The bundled residue binary. Tauri places each `externalBin` next to the
/// app's own executable, without the target-triple suffix, in `tauri dev`
/// and in the bundle (`Contents/MacOS/residue`).
fn sidecar() -> Result<PathBuf, String> {
    let exe =
        std::env::current_exe().map_err(|e| format!("cannot find the app's executable: {e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "the app's executable has no parent directory".to_string())?;
    Ok(dir.join("residue"))
}

#[tauri::command]
async fn residue_version() -> Result<String, String> {
    let bin = sidecar()?;
    let out = Command::new(&bin)
        .arg("--version")
        .output()
        .map_err(|e| format!("cannot run {}: {e}", bin.display()))?;
    if !out.status.success() {
        return Err(format!(
            "{} --version exited with {}",
            bin.display(),
            out.status
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![residue_version])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("residue desktop: {e}");
        std::process::exit(1);
    }
}
