//! The folders the app scans, kept in `settings.json` in the app's config
//! directory. Until the user saves a choice, the app offers the usual code
//! folders that exist in their home directory.

use crate::cli::CliError;
use crate::env::tilde;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Folder names that commonly hold repositories, in the order they are
/// offered. Only the ones that exist are used.
const USUAL: &[&str] = &[
    "Work",
    "work",
    "src",
    "code",
    "Code",
    "Projects",
    "projects",
    "Developer",
    "dev",
    "repos",
    "git",
    "GitHub",
    "workspace",
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Stored {
    roots: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Settings {
    /// The folders to scan.
    pub roots: Vec<String>,
    /// False until the user saves folders: `roots` are then the suggestions.
    pub saved: bool,
    /// The usual code folders that exist, for the root pickers.
    pub suggested: Vec<String>,
}

fn file(config_dir: &Path) -> PathBuf {
    config_dir.join("settings.json")
}

/// The usual code folders under `home` that exist, one entry per directory
/// (on a case-insensitive disk `Work` and `work` are the same folder).
pub fn suggested(home: &Path) -> Vec<String> {
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut out = Vec::new();
    for name in USUAL {
        let dir = home.join(name);
        if !dir.is_dir() {
            continue;
        }
        let Ok(real) = dir.canonicalize() else {
            continue;
        };
        if seen.contains(&real) {
            continue;
        }
        // Keep the name as the disk spells it.
        let shown = real
            .file_name()
            .map(|n| home.join(n))
            .unwrap_or_else(|| dir.clone());
        seen.push(real);
        out.push(shown.to_string_lossy().into_owned());
    }
    out
}

pub fn load(config_dir: &Path, home: &Path) -> Result<Settings, CliError> {
    let suggested = suggested(home);
    let path = file(config_dir);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Settings {
                roots: suggested.clone(),
                saved: false,
                suggested,
            });
        }
        Err(e) => {
            return Err(CliError::failed(format!(
                "cannot read {}: {e}",
                path.display()
            )));
        }
    };
    let stored: Stored = serde_json::from_str(&text).map_err(|e| {
        CliError::failed(format!(
            "{} is not valid settings ({e}); fix or delete it",
            path.display()
        ))
    })?;
    Ok(Settings {
        roots: stored
            .roots
            .iter()
            .map(|r| r.to_string_lossy().into_owned())
            .collect(),
        saved: true,
        suggested,
    })
}

/// Check that every root is an absolute path to a directory, drop repeats,
/// and save the list.
pub fn save(config_dir: &Path, home: &Path, roots: &[String]) -> Result<Settings, CliError> {
    let roots = folders(roots)?;
    std::fs::create_dir_all(config_dir)
        .map_err(|e| CliError::failed(format!("cannot create {}: {e}", config_dir.display())))?;
    let path = file(config_dir);
    let tmp = config_dir.join(format!(".settings.json.{}", std::process::id()));
    let body = serde_json::to_string_pretty(&Stored {
        roots: roots.clone(),
    })
    .map_err(|e| CliError::failed(format!("cannot write the settings: {e}")))?;
    std::fs::write(&tmp, body + "\n")
        .and_then(|()| std::fs::rename(&tmp, &path))
        .map_err(|e| CliError::failed(format!("cannot save {}: {e}", path.display())))?;
    load(config_dir, home)
}

/// A folder the automatic cleanup dialog offers: `path` is the folder residue
/// records for it, its canonical path, which the dialog shows and sends. A
/// path that does not resolve stays as written, and `bound_folders` refuses
/// it at the click.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Folder {
    pub path: String,
    /// The text it came from, when that leads here through a link.
    pub from: Option<String>,
}

/// `roots` as residue records them: each one's canonical path, without
/// repeats, in order. residue resolves every ROOT of `service install` and
/// `watch` itself and acts on the folder it reaches, so the dialog shows and
/// sends that folder, not the text that led there.
pub fn resolve(roots: &[String]) -> Vec<Folder> {
    let mut out: Vec<Folder> = Vec::new();
    for root in roots {
        let folder = match Path::new(root).canonicalize() {
            Ok(real) => {
                let path = real.to_string_lossy().into_owned();
                let from = (path != *root).then(|| root.clone());
                Folder { path, from }
            }
            Err(_) => Folder {
                path: root.clone(),
                from: None,
            },
        };
        if !out.iter().any(|f| f.path == folder.path) {
            out.push(folder);
        }
    }
    out
}

/// Whether each folder a save or a Preview of automatic cleanup sends to
/// residue is still its own canonical path, as `resolve` showed it. A folder
/// on the way that became a link since then makes the path lead elsewhere,
/// and residue would record and act on that other folder, so it is refused,
/// with kind `invalid`. `service.rs` calls it right before residue starts.
pub fn bound_folders(roots: &[PathBuf], home: &Path) -> Result<(), CliError> {
    let shown = |p: &Path| tilde(&p.to_string_lossy(), home);
    for path in roots {
        let real = path
            .canonicalize()
            .map_err(|e| CliError::invalid(format!("{}: {e}", shown(path))))?;
        if real != *path {
            return Err(CliError::invalid(format!(
                "{} now leads to {}. Close this dialog and open it again to see it.",
                shown(path),
                shown(&real)
            )));
        }
    }
    Ok(())
}

/// Absolute paths to existing directories, without repeats, in order.
pub fn folders(roots: &[String]) -> Result<Vec<PathBuf>, CliError> {
    let mut out: Vec<PathBuf> = Vec::new();
    for root in roots {
        let path = PathBuf::from(root);
        if !path.is_absolute() {
            return Err(CliError::invalid(format!("{root} is not an absolute path")));
        }
        if !path.is_dir() {
            return Err(CliError::invalid(format!("{root} is not a folder")));
        }
        if !out.contains(&path) {
            out.push(path);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{Folder, bound_folders, resolve};
    use std::fs;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "residue-app-settings-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // The canonical path: on macOS the temporary folder is behind /var.
        dir.canonicalize().unwrap()
    }

    fn text(p: &std::path::Path) -> String {
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn the_dialog_shows_the_folder_residue_records() {
        let dir = scratch("resolve");
        fs::create_dir_all(dir.join("Other")).unwrap();
        fs::create_dir_all(dir.join("mono")).unwrap();
        std::os::unix::fs::symlink("../Other", dir.join("mono/wt")).unwrap();
        let roots = [
            text(&dir.join("mono")),
            text(&dir.join("mono/wt")),
            text(&dir.join("Other")),
            text(&dir.join("gone")),
        ];
        assert_eq!(
            resolve(&roots),
            [
                Folder {
                    path: text(&dir.join("mono")),
                    from: None
                },
                Folder {
                    path: text(&dir.join("Other")),
                    from: Some(text(&dir.join("mono/wt")))
                },
                Folder {
                    path: text(&dir.join("gone")),
                    from: None
                },
            ]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_that_leads_elsewhere_at_the_click_is_refused() {
        let dir = scratch("bound");
        fs::create_dir_all(dir.join("Other")).unwrap();
        fs::create_dir_all(dir.join("mono/wt")).unwrap();
        let shown = [dir.join("mono/wt")];
        assert!(bound_folders(&shown, &dir).is_ok());
        // A pull turns the tracked folder into a link to another folder.
        fs::remove_dir(dir.join("mono/wt")).unwrap();
        std::os::unix::fs::symlink("../Other", dir.join("mono/wt")).unwrap();
        let e = bound_folders(&shown, &dir).unwrap_err();
        assert_eq!(e.kind, crate::cli::ErrorKind::Invalid);
        assert_eq!(
            e.message,
            "~/mono/wt now leads to ~/Other. Close this dialog and open it again to see it."
        );
        // The folder it leads to, as the dialog shows it once it opens again.
        assert!(bound_folders(&[dir.join("Other")], &dir).is_ok());
        // A path the dialog never resolved, behind a link, is refused too.
        let tmp = std::path::Path::new("/tmp");
        if tmp.canonicalize().is_ok_and(|real| real != tmp) {
            assert!(bound_folders(&[tmp.to_path_buf()], &dir).is_err());
        }
        fs::remove_dir_all(&dir).unwrap();
    }
}
