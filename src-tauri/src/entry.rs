//! What a Restore click is bound to: the trash entry or removal record its
//! row showed (`Shown`).
//!
//! residue restores the entry its `entry_path` names when it runs, and it
//! resolves that path again then. So the listing reads each entry's folder
//! (`folder::id`) right after residue listed it, and the click compares it
//! with the folder at that path before residue runs (`check`): another
//! folder renamed into a folder on the path, or a link put there, makes it
//! another folder, and the click runs nothing. After the run, `unlike`
//! compares residue's answer, which names the entry it restored as it was
//! before, with the row.

use crate::env::tilde;
use crate::folder;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

/// A Restore row as the window showed it, which its click sends back: the
/// fields of the listing's trash entry or removal record, and its folder.
#[derive(Debug, Deserialize)]
pub struct Shown {
    /// `trash` or `removed`.
    pub kind: String,
    pub entry: String,
    pub entry_path: String,
    pub original_path: Option<String>,
    pub branch: Option<String>,
    pub head: Option<String>,
    /// `trashed_at` of a trash entry, `removed_at` of a removal record.
    pub at: Option<String>,
    /// `folder::id` of `entry_path` right after the listing; `None` when the
    /// app could not read it.
    pub folder: Option<String>,
}

/// The entries a `residue.restore/v1` document lists, or after a restore the
/// one it restored, as it was before.
#[derive(Debug, Deserialize)]
struct Listed {
    #[serde(default)]
    restore: Option<Done>,
    trash: Vec<Item>,
    removed: Vec<Item>,
}

#[derive(Debug, Deserialize)]
struct Done {
    kind: String,
    entry: String,
}

#[derive(Debug, Deserialize)]
struct Item {
    entry: String,
    entry_path: String,
    original_path: Option<String>,
    branch: Option<String>,
    head: Option<String>,
    #[serde(default)]
    trashed_at: Option<String>,
    #[serde(default)]
    removed_at: Option<String>,
}

fn read(json: &str) -> Result<Listed, String> {
    serde_json::from_str(json).map_err(|e| format!("cannot read residue's list: {e}"))
}

/// The folder of every entry a listing names, by its `entry_path`, read now.
pub fn folders(listing: &str) -> Result<BTreeMap<String, String>, String> {
    let listed = read(listing)?;
    Ok(listed
        .trash
        .iter()
        .chain(&listed.removed)
        .filter_map(|i| Some((i.entry_path.clone(), folder::id(Path::new(&i.entry_path))?)))
        .collect())
}

/// Whether the folder at the row's `entry_path` is still the one the listing
/// read. `Err` says why not, for the window.
pub fn check(shown: &Shown, home: &Path) -> Result<(), String> {
    let at = tilde(&shown.entry_path, home);
    let Some(listed) = &shown.folder else {
        return Err(format!(
            "the app could not read {at} when it listed it, so it cannot tell that it is the same entry; refresh and try again"
        ));
    };
    match folder::id(Path::new(&shown.entry_path)) {
        Some(now) if now == *listed => Ok(()),
        Some(_) => Err(format!(
            "{at} is not the folder the list showed: something else is at that path now. Refresh to see what is there."
        )),
        None => Err(format!("{at} is gone; refresh to see what is left")),
    }
}

/// How residue's answer to a restore differs from the row, one line each,
/// `what: row → answer`. Empty: it restored the row's entry.
pub fn unlike(shown: &Shown, answer: &str, home: &Path) -> Vec<String> {
    let answer = match read(answer) {
        Ok(a) => a,
        Err(e) => {
            return vec![format!(
                "The app could not compare the answer with the row: {e}"
            )];
        }
    };
    let mut out = Vec::new();
    let mut line = |what: &str, row: Option<&str>, got: Option<&str>| {
        if row != got {
            let text = |v: Option<&str>| v.map_or("none".into(), |v| tilde(v, home));
            out.push(format!("{what}: {} → {}", text(row), text(got)));
        }
    };
    let Some(done) = &answer.restore else {
        return vec!["residue's answer names no restore".into()];
    };
    line("Kind", Some(&shown.kind), Some(&done.kind));
    line("Entry", Some(&shown.entry), Some(&done.entry));
    let item = match done.kind.as_str() {
        "trash" => answer.trash.first(),
        _ => answer.removed.first(),
    };
    let Some(item) = item else {
        out.push("residue's answer names no entry".into());
        return out;
    };
    let at = if done.kind == "trash" {
        &item.trashed_at
    } else {
        &item.removed_at
    };
    line("Entry", Some(&shown.entry), Some(&item.entry));
    line(
        "Entry path",
        Some(&shown.entry_path),
        Some(&item.entry_path),
    );
    line(
        "Path",
        shown.original_path.as_deref(),
        item.original_path.as_deref(),
    );
    line("Branch", shown.branch.as_deref(), item.branch.as_deref());
    line("HEAD", shown.head.as_deref(), item.head.as_deref());
    line(
        if done.kind == "trash" {
            "Trashed"
        } else {
            "Removed"
        },
        shown.at.as_deref(),
        at.as_deref(),
    );
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    const HOME: &str = "/Users/me";

    fn item(kind: &str, name: &str, repo: &str, original: &str) -> serde_json::Value {
        let (dir, at) = if kind == "trash" {
            ("residue-trash", "trashed_at")
        } else {
            ("residue-removed", "removed_at")
        };
        let mut v = json!({
            "kind": kind, "entry": name, "entry_path": format!("{repo}/.git/{dir}/{name}"),
            "original_path": original, "branch": "refs/heads/feat", "head": "1111111",
            "allocated_bytes": 1, "error": null
        });
        v[at] = json!("2026-11-01T10:00:00Z");
        v
    }

    fn shown_of(v: &serde_json::Value, folder: Option<&str>) -> Shown {
        let s = |k: &str| v[k].as_str().map(String::from);
        let kind = s("kind").unwrap();
        Shown {
            at: s(if kind == "trash" {
                "trashed_at"
            } else {
                "removed_at"
            }),
            kind,
            entry: s("entry").unwrap(),
            entry_path: s("entry_path").unwrap(),
            original_path: s("original_path"),
            branch: s("branch"),
            head: s("head"),
            folder: folder.map(String::from),
        }
    }

    fn answer(kind: &str, restored: &serde_json::Value) -> String {
        let (trash, removed) = if kind == "trash" {
            (vec![restored.clone()], vec![])
        } else {
            (vec![], vec![restored.clone()])
        };
        json!({
            "schema": "residue.restore/v1", "version": "0.0.12", "generated_at": "2026-11-10T10:00:00Z",
            "restored": restored["original_path"],
            "restore": { "kind": kind, "entry": restored["entry"], "path": restored["original_path"],
                         "branch": "refs/heads/feat", "head": "1111111", "checkout": "moved_back",
                         "moved_from": null, "new_branch": null, "branch_tip": null, "not_kept": [], "notes": [] },
            "trash": trash, "removed": removed, "errors": []
        })
        .to_string()
    }

    #[test]
    fn an_answer_for_the_rows_entry_is_no_difference() {
        let home = Path::new(HOME);
        for kind in ["trash", "removed"] {
            let row = item(
                kind,
                "feat-20261101T100000Z",
                "/Users/me/src/a",
                "/Users/me/src/a-feat",
            );
            let shown = shown_of(&row, Some("1:2"));
            assert_eq!(
                unlike(&shown, &answer(kind, &row), home),
                Vec::<String>::new()
            );
        }
    }

    #[test]
    fn an_answer_for_another_entry_says_how_it_differs() {
        // Another repository's record with the same name, now at the row's path.
        let home = Path::new(HOME);
        let row = item(
            "removed",
            "feat-20261101T100000Z",
            "/Users/me/src/wa",
            "/Users/me/src/wa/feat",
        );
        let other = item(
            "removed",
            "feat-20261101T100000Z",
            "/Users/me/src/wa",
            "/Users/me/src/wc/feat",
        );
        assert_eq!(
            unlike(
                &shown_of(&row, Some("1:2")),
                &answer("removed", &other),
                home
            ),
            ["Path: ~/src/wa/feat → ~/src/wc/feat"]
        );
        let other = item(
            "trash",
            "featB-20261101T100000Z",
            "/Users/me/repoB",
            "/Users/me/repoB-featB",
        );
        assert_eq!(
            unlike(&shown_of(&row, Some("1:2")), &answer("trash", &other), home),
            [
                "Kind: removed → trash",
                "Entry: feat-20261101T100000Z → featB-20261101T100000Z",
                "Entry path: ~/src/wa/.git/residue-removed/feat-20261101T100000Z → ~/repoB/.git/residue-trash/featB-20261101T100000Z",
                "Path: ~/src/wa/feat → ~/repoB-featB",
            ]
        );
    }

    #[test]
    fn the_click_runs_only_while_the_listed_folder_is_at_the_path() {
        let dir = std::env::temp_dir().join(format!("residue-app-entry-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let entry = dir.join("repoA/.git/residue-trash/feat-20261101T100000Z");
        fs::create_dir_all(&entry).unwrap();
        fs::create_dir_all(dir.join("repoB/.git/residue-trash/feat-20261101T100000Z")).unwrap();
        let path = entry.to_string_lossy().into_owned();
        let listing = json!({ "trash": [{ "entry": "feat-20261101T100000Z", "entry_path": path }], "removed": [] });
        let folders = folders(&listing.to_string()).unwrap();
        let mut row = item("trash", "feat-20261101T100000Z", "/x", "/x/feat");
        row["entry_path"] = json!(path);
        let shown = shown_of(&row, folders.get(&path).map(String::as_str));
        let home = Path::new(HOME);
        assert_eq!(check(&shown, home), Ok(()));
        // The repository's folder renamed away, and a link to another
        // repository with an entry of that name put in its place.
        fs::rename(dir.join("repoA"), dir.join("repoA.aside")).unwrap();
        std::os::unix::fs::symlink(dir.join("repoB"), dir.join("repoA")).unwrap();
        assert!(
            check(&shown, home)
                .unwrap_err()
                .contains("is not the folder the list showed")
        );
        // Nothing at the path.
        fs::remove_file(dir.join("repoA")).unwrap();
        assert!(check(&shown, home).unwrap_err().contains("is gone"));
        // A row the listing could not read is never restored.
        let unread = shown_of(&row, None);
        assert!(check(&unread, home).unwrap_err().contains("could not read"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
