//! A folder's identity: its device and inode, as `DEV:INO`, read with
//! `symlink_metadata`, which does not follow a link at the path's last name.
//!
//! A click that acts on a path the window showed compares the identity it
//! reads then with the one read when the window listed that path. Another
//! folder renamed into its place, or a link at the path or at a folder on
//! the way to it that leads to another folder, gives another identity: the
//! system resolves the folders on the way, and a different folder at the end
//! has another inode. A link that leads back to the same folder gives the
//! same one, and the click acts on the folder it showed.

use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// The identity of what is at `path` now, or `None` when nothing is or it
/// cannot be read.
pub fn id(path: &Path) -> Option<String> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    Some(format!("{}:{}", meta.dev(), meta.ino()))
}

#[cfg(test)]
mod tests {
    use super::id;
    use std::fs;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("residue-app-folder-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_same_folder_keeps_its_identity_and_another_in_its_place_does_not() {
        let dir = scratch("swap");
        let a = dir.join("a");
        fs::create_dir(&a).unwrap();
        let first = id(&a).expect("a folder has an identity");
        assert_eq!(id(&a), Some(first.clone()));
        // Another folder renamed into its place.
        fs::rename(&a, dir.join("a.aside")).unwrap();
        fs::create_dir(&a).unwrap();
        assert_ne!(id(&a), Some(first));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_link_on_the_way_to_another_folder_gives_that_folders_identity_or_its_own() {
        let dir = scratch("link");
        fs::create_dir_all(dir.join("one/entry")).unwrap();
        fs::create_dir_all(dir.join("two/entry")).unwrap();
        let path = dir.join("here/entry");
        fs::create_dir_all(&path).unwrap();
        let shown = id(&path).unwrap();
        // A folder on the way replaced by a link to another tree.
        fs::rename(dir.join("here"), dir.join("here.aside")).unwrap();
        std::os::unix::fs::symlink(dir.join("two"), dir.join("here")).unwrap();
        assert_eq!(id(&path), id(&dir.join("two/entry")));
        assert_ne!(id(&path), Some(shown.clone()));
        // A link at the last name: the link's own identity, not its target's.
        fs::remove_file(dir.join("here")).unwrap();
        fs::create_dir(dir.join("here")).unwrap();
        std::os::unix::fs::symlink(dir.join("one/entry"), &path).unwrap();
        assert_ne!(id(&path), id(&dir.join("one/entry")));
        assert_ne!(id(&path), Some(shown));
        // Nothing there.
        assert_eq!(id(&dir.join("none")), None);
        fs::remove_dir_all(&dir).unwrap();
    }
}
