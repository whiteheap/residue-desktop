//! What a confirmed removal is bound to: the worktrees, trash entries and
//! removal records its dialog showed (`Shown`).
//!
//! The dialog's plan and the confirmed run are two residue processes, and
//! residue's `--yes` run checks each path against the plan that run makes
//! itself, not the one the dialog showed. So at the click `lib.rs` plans
//! again, with the confirmed command without `--yes`, and compares (`check`):
//! a worktree that is not the one the dialog showed, by its canonical path,
//! its folder, its branch, its HEAD or its lock, or one residue would no
//! longer move, is dropped from the run, and a trash entry it may purge or a
//! removal record it may forget that the dialog did not list stops the run.
//! After the run, `unlisted` names what residue moved, purged or forgot that
//! the dialog did not show, which only a change in the moment between that
//! plan and the run's own can cause.
//!
//! Every value compared is residue's, from its `residue.remove/v1` JSON, but
//! the folder (`folder::id`), which the app reads right after each plan: the
//! JSON names no repository, so another worktree at a path, on the same
//! branch and HEAD, differs only by its folder.

use crate::env::tilde;
use crate::folder;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// One worktree the dialog lists under "To the trash", as it showed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Worktree {
    /// The path the plan was asked about, which the confirmed run gets.
    pub path: String,
    pub canonical_path: Option<String>,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub locked: bool,
    /// `folder::id` of the worktree's folder right after that plan; `None`
    /// when it could not be read.
    pub folder: Option<String>,
}

/// A trash entry a plan lists, by the fields residue reads from its `info`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Trash {
    pub entry: String,
    pub entry_path: String,
    pub original_path: Option<String>,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub trashed_at: Option<String>,
}

/// A removal record a plan lists, by the fields residue reads from it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Record {
    pub entry: String,
    pub entry_path: String,
    pub original_path: Option<String>,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub removed_at: Option<String>,
}

/// What the removal dialog showed, which its confirm sends back.
#[derive(Debug, Deserialize)]
pub struct Shown {
    /// The plan's `as_of`: the time it judged every age at, its
    /// `generated_at` for a plan made without `--as-of`.
    pub as_of: Option<String>,
    /// The worktrees under "To the trash", in the dialog's order.
    pub worktrees: Vec<Worktree>,
    /// Every trash entry the plan listed, whatever residue said of it: the
    /// run may purge any of them.
    pub purges: Vec<Trash>,
    /// Every removal record the plan listed, the same way.
    pub forgets: Vec<Record>,
}

/// The fields of residue's `residue.remove/v1` document a binding reads.
/// Other fields, such as one a newer residue adds, are left alone.
#[derive(Debug, Deserialize)]
pub struct Plan {
    results: Vec<Row>,
    purged: Vec<Purge>,
    forgotten: Vec<Forget>,
}

#[derive(Debug, Deserialize)]
struct Row {
    path: String,
    result: String,
    reason: Option<String>,
    /// For a worktree it moved: the trash entry it moved it into.
    trash_entry: Option<String>,
    worktree: Option<Report>,
}

#[derive(Debug, Deserialize)]
struct Report {
    canonical_path: Option<String>,
    verdict: String,
    branch: Option<String>,
    head: Option<String>,
    locked: bool,
}

#[derive(Debug, Deserialize)]
struct Purge {
    result: String,
    entry: Trash,
}

#[derive(Debug, Deserialize)]
struct Forget {
    result: String,
    entry: Record,
}

impl Row {
    /// What the dialog lists under "To the trash": without `--yes`, residue
    /// marks each worktree it would move `skipped` with verdict `candidate`
    /// (RemoveDialog.tsx, `ready`).
    fn ready(&self) -> bool {
        self.result == "skipped"
            && self
                .worktree
                .as_ref()
                .is_some_and(|w| w.verdict == "candidate")
    }

    /// Why residue would not move it, in its words.
    fn why(&self) -> String {
        match (&self.worktree, self.result.as_str()) {
            (Some(w), "refused" | "skipped") => format!("its verdict is {}", w.verdict),
            (_, result) => match &self.reason {
                Some(reason) => format!("{result}: {reason}"),
                None => result.to_string(),
            },
        }
    }
}

impl Plan {
    pub fn read(json: &str) -> Result<Plan, String> {
        serde_json::from_str(json).map_err(|e| format!("cannot read residue's plan: {e}"))
    }

    /// The folder of each worktree in the plan, by the path it was asked
    /// about, read now.
    pub fn folders(&self) -> BTreeMap<String, String> {
        self.results
            .iter()
            .filter_map(|r| {
                let w = r.worktree.as_ref()?;
                let at = w.canonical_path.as_deref().unwrap_or(&r.path);
                Some((r.path.clone(), folder::id(Path::new(at))?))
            })
            .collect()
    }
}

/// How residue's plan at the click differs from what the dialog showed, in
/// what the confirmed run would act on.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Check {
    /// The paths whose worktree is the one the dialog showed, and that
    /// residue would still move.
    pub keep: Vec<String>,
    /// One line for each way the other worktrees differ, `what: before →
    /// after` where it can. Each of those is left out of the run.
    pub dropped: Vec<String>,
    /// One line for each trash entry or removal record the run would cover
    /// that the dialog did not list, or that changed. Any of them stops the
    /// run: the app cannot tell which path's repository it belongs to.
    pub entries: Vec<String>,
}

impl Check {
    /// The run may go as the dialog showed it.
    pub fn same(&self) -> bool {
        self.dropped.is_empty() && self.entries.is_empty()
    }
}

/// Compare `fresh`, residue's plan at the click for `paths`, with what the
/// dialog showed of them. `folders` are the folders read right after
/// `fresh`.
pub fn check(
    shown: &Shown,
    paths: &[String],
    fresh: &Plan,
    folders: &BTreeMap<String, String>,
    home: &Path,
) -> Check {
    let mut out = Check::default();
    for w in shown.worktrees.iter().filter(|w| paths.contains(&w.path)) {
        let at = tilde(&w.path, home);
        let before = out.dropped.len();
        match fresh.results.iter().find(|r| r.path == w.path) {
            None => out.dropped.push(format!("{at}: not in residue's plan now")),
            Some(row) => match row.worktree.as_ref().filter(|_| row.ready()) {
                None => out.dropped.push(format!(
                    "{at}: residue would not move it now; {}",
                    row.why()
                )),
                Some(report) => {
                    differ(&mut out.dropped, &at, w, report, folders.get(&w.path), home)
                }
            },
        }
        if out.dropped.len() == before {
            out.keep.push(w.path.clone());
        }
    }
    let listed: BTreeMap<&str, &Trash> = shown
        .purges
        .iter()
        .map(|t| (t.entry_path.as_str(), t))
        .collect();
    for p in &fresh.purged {
        let what = format!(
            "Trash entry {}",
            named(&p.entry.entry, &p.entry.entry_path, home)
        );
        match listed.get(p.entry.entry_path.as_str()) {
            None => out
                .entries
                .push(format!("{what}: the plan did not list it")),
            Some(t) if **t != p.entry => {
                out.entries.push(format!("{what}: changed since the plan"))
            }
            Some(_) => {}
        }
    }
    let listed: BTreeMap<&str, &Record> = shown
        .forgets
        .iter()
        .map(|r| (r.entry_path.as_str(), r))
        .collect();
    for f in &fresh.forgotten {
        let what = format!(
            "Removal record {}",
            named(&f.entry.entry, &f.entry.entry_path, home)
        );
        match listed.get(f.entry.entry_path.as_str()) {
            None => out
                .entries
                .push(format!("{what}: the plan did not list it")),
            Some(r) if **r != f.entry => {
                out.entries.push(format!("{what}: changed since the plan"))
            }
            Some(_) => {}
        }
    }
    out
}

/// What the confirmed run `ran` moved, purged or forgot, or covered, that the
/// dialog did not show, one line each. residue reports each worktree it moved
/// as its own plan found it, right before the move, and a move into the
/// trash is a rename, so the folder in the trash entry keeps the identity it
/// had at its path.
pub fn unlisted(shown: &Shown, ran: &Plan, home: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for row in ran.results.iter().filter(|r| r.result == "trashed") {
        let at = tilde(&row.path, home);
        let (Some(w), Some(report)) = (
            shown.worktrees.iter().find(|w| w.path == row.path),
            row.worktree.as_ref(),
        ) else {
            out.push(format!(
                "{at}: moved to the trash, but the plan did not list it"
            ));
            continue;
        };
        let moved = row.trash_entry.as_deref().and_then(|entry| {
            let name =
                Path::new(report.canonical_path.as_deref().unwrap_or(&row.path)).file_name()?;
            folder::id(&Path::new(entry).join(name))
        });
        match (&moved, &w.folder) {
            (Some(now), Some(then)) if now != then => {
                out.push(format!(
                    "{at}: moved another folder to the trash than the plan showed"
                ));
            }
            (None, _) => out.push(format!(
                "{at}: moved to the trash, where the app could not read its folder to compare it"
            )),
            _ => {}
        }
        let mut lines = Vec::new();
        differ(&mut lines, &at, w, report, w.folder.as_ref(), home);
        out.extend(
            lines
                .into_iter()
                .map(|l| format!("{l}, then moved to the trash")),
        );
    }
    for p in ran
        .purged
        .iter()
        .filter(|p| !shown.purges.contains(&p.entry))
    {
        let what = format!(
            "Trash entry {}",
            named(&p.entry.entry, &p.entry.entry_path, home)
        );
        out.push(match p.result.as_str() {
            "purged" => format!("{what}: deleted, but the plan did not list it"),
            result => format!("{what}: in the run ({result}), but the plan did not list it"),
        });
    }
    for f in ran
        .forgotten
        .iter()
        .filter(|f| !shown.forgets.contains(&f.entry))
    {
        let what = format!(
            "Removal record {}",
            named(&f.entry.entry, &f.entry.entry_path, home)
        );
        out.push(match f.result.as_str() {
            "forgotten" => format!("{what}: forgotten, but the plan did not list it"),
            result => format!("{what}: in the run ({result}), but the plan did not list it"),
        });
    }
    out
}

/// A trash entry or removal record by its name and its repository: its
/// `entry_path` is `COMMON_DIR/residue-trash/NAME` or
/// `COMMON_DIR/residue-removed/NAME`, and a `.git` common directory is the
/// repository's main checkout's.
fn named(entry: &str, entry_path: &str, home: &Path) -> String {
    let common = Path::new(entry_path).parent().and_then(Path::parent);
    let repo = match common {
        Some(c) if c.file_name().is_some_and(|n| n == ".git") => c.parent(),
        other => other,
    };
    match repo {
        Some(r) => format!("{entry} in {}", tilde(&r.to_string_lossy(), home)),
        None => entry.to_string(),
    }
}

/// One line for each field of `report` and `folder` that differs from what
/// the dialog showed of `w`.
fn differ(
    out: &mut Vec<String>,
    at: &str,
    w: &Worktree,
    report: &Report,
    folder: Option<&String>,
    home: &Path,
) {
    if report.canonical_path != w.canonical_path {
        let shown = |p: &Option<String>| p.as_deref().map_or("none".into(), |p| tilde(p, home));
        out.push(format!(
            "{at}: leads to {} → {}",
            shown(&w.canonical_path),
            shown(&report.canonical_path)
        ));
    } else if folder != w.folder.as_ref() {
        out.push(format!("{at}: another folder is at this path now"));
    }
    if report.branch != w.branch {
        out.push(format!(
            "{at}: branch {} → {}",
            branch(&w.branch),
            branch(&report.branch)
        ));
    }
    if report.head != w.head {
        out.push(format!(
            "{at}: HEAD {} → {}",
            head(&w.head),
            head(&report.head)
        ));
    }
    if report.locked != w.locked {
        let lock = |l: bool| if l { "locked" } else { "not locked" };
        out.push(format!(
            "{at}: {} → {}",
            lock(w.locked),
            lock(report.locked)
        ));
    }
}

fn branch(b: &Option<String>) -> &str {
    match b.as_deref() {
        Some(b) => b.strip_prefix("refs/heads/").unwrap_or(b),
        None => "detached",
    }
}

fn head(h: &Option<String>) -> &str {
    match h.as_deref() {
        Some(h) => h.get(..7).unwrap_or(h),
        None => "none",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const HOME: &str = "/Users/me";

    fn report(path: &str, branch: &str, head: &str, verdict: &str) -> serde_json::Value {
        json!({
            "path": path, "canonical_path": path, "verdict": verdict,
            "branch": format!("refs/heads/{branch}"), "head": head, "detached": false,
            "locked": false, "lock_reason": null, "checks": []
        })
    }

    fn row(path: &str, result: &str, worktree: serde_json::Value) -> serde_json::Value {
        json!({ "path": path, "result": result, "reason": "stdin is not a terminal",
                "trash_entry": null, "removal_record": null, "worktree": worktree })
    }

    fn trash(name: &str, repo: &str) -> serde_json::Value {
        json!({ "kind": "trash", "entry": name,
                "entry_path": format!("{repo}/.git/residue-trash/{name}"),
                "worktree_path": null, "original_path": format!("/Users/me/src/{name}"),
                "branch": "refs/heads/old", "head": "1111111111", "trashed_at": "2026-11-01T10:00:00Z",
                "allocated_bytes": 8192, "reclaimable_bytes": 8192, "registered": true, "error": null })
    }

    fn record(name: &str, repo: &str) -> serde_json::Value {
        json!({ "kind": "removed", "entry": name,
                "entry_path": format!("{repo}/.git/residue-removed/{name}"),
                "ref": "refs/residue/removed/x", "state": "removed",
                "original_path": format!("/Users/me/src/{name}"), "aside_path": null,
                "branch": "refs/heads/gone", "head": "2222222222", "removed_at": "2026-09-01T10:00:00Z" })
    }

    fn doc(
        results: Vec<serde_json::Value>,
        purged: Vec<serde_json::Value>,
        forgotten: Vec<serde_json::Value>,
    ) -> String {
        json!({
            "schema": "residue.remove/v1", "version": "0.0.12", "generated_at": "2026-11-10T11:00:00Z",
            "as_of": "2026-11-10T11:00:00Z", "a_field_a_newer_residue_adds": 1,
            "results": results, "purged": purged, "forgotten": forgotten, "errors": [], "warnings": []
        })
        .to_string()
    }

    /// What a dialog shows of `plan`, as RemoveDialog.tsx sends it.
    fn shown_of(plan: &str, folders: &BTreeMap<String, String>) -> Shown {
        let v: serde_json::Value = serde_json::from_str(plan).unwrap();
        let p = Plan::read(plan).unwrap();
        Shown {
            as_of: v["as_of"].as_str().map(String::from),
            worktrees: p
                .results
                .iter()
                .filter(|r| r.ready())
                .map(|r| {
                    let w = r.worktree.as_ref().unwrap();
                    Worktree {
                        path: r.path.clone(),
                        canonical_path: w.canonical_path.clone(),
                        branch: w.branch.clone(),
                        head: w.head.clone(),
                        locked: w.locked,
                        folder: folders.get(&r.path).cloned(),
                    }
                })
                .collect(),
            purges: p.purged.iter().map(|x| x.entry.clone()).collect(),
            forgets: p.forgotten.iter().map(|x| x.entry.clone()).collect(),
        }
    }

    fn folders(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(p, f)| (p.to_string(), f.to_string()))
            .collect()
    }

    const WT2: &str = "/Users/me/src/wt2";
    const WT3: &str = "/Users/me/src/wt3";
    const REPO: &str = "/Users/me/src/repo";

    fn base() -> String {
        doc(
            vec![
                row(
                    WT2,
                    "skipped",
                    report(WT2, "b-wt2", "7bcce62f00", "candidate"),
                ),
                row(
                    WT3,
                    "skipped",
                    report(WT3, "b-wt3", "7bcce62f00", "candidate"),
                ),
            ],
            vec![
                json!({ "result": "kept", "reason": "one or more checks did not pass",
                         "entry": trash("wt1-20261101T100000Z", REPO), "removal_record": null, "worktree": null }),
            ],
            vec![
                json!({ "result": "skipped", "reason": "not confirmed", "entry": record("wt0-20260901T100000Z", REPO) }),
            ],
        )
    }

    fn home() -> &'static Path {
        Path::new(HOME)
    }

    /// Every line `check` gives for all the paths the dialog showed.
    fn changes(
        shown: &Shown,
        fresh: &Plan,
        folders: &BTreeMap<String, String>,
        home: &Path,
    ) -> Vec<String> {
        let paths: Vec<String> = shown.worktrees.iter().map(|w| w.path.clone()).collect();
        let c = check(shown, &paths, fresh, folders, home);
        c.dropped.into_iter().chain(c.entries).collect()
    }

    #[test]
    fn a_changed_worktree_is_dropped_and_the_rest_kept() {
        let f = folders(&[(WT2, "1:2"), (WT3, "1:3")]);
        let shown = shown_of(&base(), &f);
        let fresh = doc(
            vec![
                row(
                    WT2,
                    "skipped",
                    report(WT2, "b-wt2", "dd57e9aa00", "candidate"),
                ),
                row(
                    WT3,
                    "skipped",
                    report(WT3, "b-wt3", "7bcce62f00", "candidate"),
                ),
            ],
            vec![],
            vec![],
        );
        let c = check(
            &shown,
            &[WT2.into(), WT3.into()],
            &Plan::read(&fresh).unwrap(),
            &f,
            home(),
        );
        assert_eq!(
            c,
            Check {
                keep: vec![WT3.into()],
                dropped: vec!["~/src/wt2: HEAD 7bcce62 → dd57e9a".into()],
                entries: vec![],
            }
        );
        assert!(!c.same());
        // Planned again for the rest only: the same, so it may run.
        let rest = doc(
            vec![row(
                WT3,
                "skipped",
                report(WT3, "b-wt3", "7bcce62f00", "candidate"),
            )],
            vec![],
            vec![],
        );
        let again = check(
            &shown,
            &[WT3.into()],
            &Plan::read(&rest).unwrap(),
            &f,
            home(),
        );
        assert!(again.same());
        assert_eq!(again.keep, [WT3]);
    }

    #[test]
    fn the_same_plan_at_the_click_changes_nothing() {
        let f = folders(&[(WT2, "1:2"), (WT3, "1:3")]);
        let shown = shown_of(&base(), &f);
        assert_eq!(shown.worktrees.len(), 2);
        let fresh = Plan::read(&base()).unwrap();
        assert_eq!(changes(&shown, &fresh, &f, home()), Vec::<String>::new());
    }

    #[test]
    fn a_trash_entry_kept_at_the_plan_and_purgeable_at_the_click_was_listed() {
        // The plan listed it as kept (in use); at the click residue would
        // purge it. The dialog listed it, so the run may go.
        let f = folders(&[(WT2, "1:2"), (WT3, "1:3")]);
        let shown = shown_of(&base(), &f);
        let fresh = base().replace("\"kept\"", "\"skipped\"");
        assert_eq!(
            changes(&shown, &Plan::read(&fresh).unwrap(), &f, home()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_commit_a_branch_or_a_lock_after_the_plan_refuses_the_run() {
        let f = folders(&[(WT2, "1:2"), (WT3, "1:3")]);
        let shown = shown_of(&base(), &f);
        let mut wt3 = report(WT3, "other", "7bcce62f00", "candidate");
        wt3["locked"] = json!(true);
        let fresh = doc(
            vec![
                row(
                    WT2,
                    "skipped",
                    report(WT2, "b-wt2", "dd57e9aa00", "candidate"),
                ),
                row(WT3, "skipped", wt3),
            ],
            vec![],
            vec![],
        );
        assert_eq!(
            changes(&shown, &Plan::read(&fresh).unwrap(), &f, home()),
            [
                "~/src/wt2: HEAD 7bcce62 → dd57e9a",
                "~/src/wt3: branch b-wt3 → other",
                "~/src/wt3: not locked → locked",
            ]
        );
    }

    #[test]
    fn another_worktree_at_a_listed_path_refuses_the_run() {
        // Another repository's worktree at the path, on the same branch and
        // HEAD: only its folder differs. And a link at the path: its
        // canonical path does.
        let shown = shown_of(&base(), &folders(&[(WT2, "1:2"), (WT3, "1:3")]));
        let mut wt3 = report(WT3, "b-wt3", "7bcce62f00", "candidate");
        wt3["canonical_path"] = json!("/Users/me/private/wtY");
        let fresh = doc(
            vec![
                row(
                    WT2,
                    "skipped",
                    report(WT2, "b-wt2", "7bcce62f00", "candidate"),
                ),
                row(WT3, "skipped", wt3),
            ],
            vec![],
            vec![],
        );
        assert_eq!(
            changes(
                &shown,
                &Plan::read(&fresh).unwrap(),
                &folders(&[(WT2, "1:9"), (WT3, "1:3")]),
                home()
            ),
            [
                "~/src/wt2: another folder is at this path now",
                "~/src/wt3: leads to ~/src/wt3 → ~/private/wtY",
            ]
        );
    }

    #[test]
    fn a_worktree_residue_would_keep_now_refuses_the_run() {
        let f = folders(&[(WT2, "1:2"), (WT3, "1:3")]);
        let shown = shown_of(&base(), &f);
        let fresh = doc(
            vec![
                row(WT2, "refused", report(WT2, "b-wt2", "7bcce62f00", "keep")),
                row(WT3, "invalid", serde_json::Value::Null),
            ],
            vec![],
            vec![],
        );
        assert_eq!(
            changes(&shown, &Plan::read(&fresh).unwrap(), &f, home()),
            [
                "~/src/wt2: residue would not move it now; its verdict is keep",
                "~/src/wt3: residue would not move it now; invalid: stdin is not a terminal",
            ]
        );
    }

    #[test]
    fn an_entry_or_record_the_plan_did_not_list_refuses_the_run() {
        // As when another repository's worktree takes a listed path: its
        // repository's trash and records come into the run.
        let f = folders(&[(WT2, "1:2"), (WT3, "1:3")]);
        let shown = shown_of(&base(), &f);
        let mut changed = record("wt0-20260901T100000Z", REPO);
        changed["head"] = json!("3333333333");
        let fresh = doc(
            vec![
                row(
                    WT2,
                    "skipped",
                    report(WT2, "b-wt2", "7bcce62f00", "candidate"),
                ),
                row(
                    WT3,
                    "skipped",
                    report(WT3, "b-wt3", "7bcce62f00", "candidate"),
                ),
            ],
            vec![json!({ "result": "skipped", "reason": "not confirmed",
                         "entry": trash("B-old-20261101T100000Z", "/Users/me/src/B"), "removal_record": null, "worktree": null })],
            vec![json!({ "result": "kept", "reason": "no flock", "entry": changed })],
        );
        assert_eq!(
            changes(&shown, &Plan::read(&fresh).unwrap(), &f, home()),
            [
                "Trash entry B-old-20261101T100000Z in ~/src/B: the plan did not list it",
                "Removal record wt0-20260901T100000Z in ~/src/repo: changed since the plan",
            ]
        );
    }

    #[test]
    fn fewer_entries_at_the_click_are_no_change() {
        // Another run purged the listed entry: the run deletes less.
        let f = folders(&[(WT2, "1:2"), (WT3, "1:3")]);
        let shown = shown_of(&base(), &f);
        let fresh = doc(
            vec![
                row(
                    WT2,
                    "skipped",
                    report(WT2, "b-wt2", "7bcce62f00", "candidate"),
                ),
                row(
                    WT3,
                    "skipped",
                    report(WT3, "b-wt3", "7bcce62f00", "candidate"),
                ),
            ],
            vec![],
            vec![],
        );
        assert_eq!(
            changes(&shown, &Plan::read(&fresh).unwrap(), &f, home()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn the_run_names_what_it_did_that_the_plan_did_not_show() {
        // Each worktree moved into its trash entry by a rename: the folder
        // there is the one that was at its path when residue moved it.
        let dir = std::env::temp_dir().join(format!("residue-app-plan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let entry = |name: &str| {
            let e = dir.join(format!("repo/.git/residue-trash/{name}-20261110T120000Z"));
            std::fs::create_dir_all(e.join(name)).unwrap();
            e
        };
        let (e2, e3) = (entry("wt2"), entry("wt3"));
        let id = |p: &Path| folder::id(p).unwrap();
        // The dialog showed wt2's folder; wt3 at the click was another one.
        let f = folders(&[(WT2, &id(&e2.join("wt2"))), (WT3, "1:3")]);
        let shown = shown_of(&base(), &f);
        let trashed = |path: &str, e: &Path, wt: serde_json::Value| {
            let mut r = row(path, "trashed", wt);
            r["reason"] = serde_json::Value::Null;
            r["trash_entry"] = json!(e.to_string_lossy());
            r
        };
        let ran = doc(
            vec![
                trashed(WT2, &e2, report(WT2, "b-wt2", "7bcce62f00", "candidate")),
                trashed(WT3, &e3, report(WT3, "b-wt3", "dd57e9aa00", "candidate")),
            ],
            vec![
                json!({ "result": "purged", "reason": null, "entry": trash("wt1-20261101T100000Z", REPO),
                        "removal_record": "/x", "worktree": null }),
                json!({ "result": "purged", "reason": null, "entry": trash("B-old-20261101T100000Z", "/Users/me/src/B"),
                        "removal_record": "/y", "worktree": null }),
                json!({ "result": "failed", "reason": "Git no longer lists it", "entry": trash("C-old-20261101T100000Z", "/Users/me/src/C"),
                        "removal_record": null, "worktree": null }),
            ],
            vec![
                json!({ "result": "forgotten", "reason": null, "entry": record("wt0-20260901T100000Z", REPO) }),
            ],
        );
        assert_eq!(
            unlisted(&shown, &Plan::read(&ran).unwrap(), home()),
            [
                "~/src/wt3: moved another folder to the trash than the plan showed",
                "~/src/wt3: HEAD 7bcce62 → dd57e9a, then moved to the trash",
                "Trash entry B-old-20261101T100000Z in ~/src/B: deleted, but the plan did not list it",
                "Trash entry C-old-20261101T100000Z in ~/src/C: in the run (failed), but the plan did not list it",
            ]
        );
        // A moved folder the app cannot find in its entry is said too.
        let gone = doc(
            vec![trashed(
                WT2,
                &dir.join("nowhere"),
                report(WT2, "b-wt2", "7bcce62f00", "candidate"),
            )],
            vec![],
            vec![],
        );
        assert_eq!(
            unlisted(&shown, &Plan::read(&gone).unwrap(), home()),
            [
                "~/src/wt2: moved to the trash, where the app could not read its folder to compare it"
            ]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_plan_without_a_field_the_binding_needs_is_an_error() {
        let text = base().replace("\"locked\":false,", "");
        assert!(Plan::read(&text).is_err());
    }
}
