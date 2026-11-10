//! Automatic cleanup: the login service that `residue service install` writes
//! (a launchd agent on macOS) and that runs `residue watch`.
//!
//! The service records the absolute path of the binary that installed it, so
//! the app never installs it from the copy inside its own bundle: moving,
//! updating or deleting the app would break the agent. It installs from, in
//! order:
//!
//! 1. a `residue` the user installed (on the login shell's PATH, or in the
//!    usual Homebrew, Cargo and `~/.local/bin` directories) that is at least as
//!    new as the bundled one, with its directory first on PATH, so the agent
//!    records the stable link (`/opt/homebrew/bin/residue`) and keeps working
//!    after `brew upgrade`;
//! 2. otherwise a copy of the bundled binary at
//!    `~/Library/Application Support/<app id>/bin/residue`, which stays when
//!    the app is moved, updated or deleted.
//!
//! What the service does comes from its arguments and the version of the
//! binary it runs, which `brew upgrade` can change without the app: the
//! window never assumes the bundled residue's behavior for it. Saving other
//! folders passes the service's options on as they are written, every one,
//! including an option the app does not know, such as one a newer residue
//! adds. It refuses when an argument is neither an option nor a folder,
//! rather than drop it.
//!
//! A save installs what the window showed or nothing. Every read carries a
//! `Basis`: the binary a save installs with and its version, the options it
//! keeps, and the service it replaces (binary, version, folders, mode). The
//! save dialog reads the service when it opens and sends that basis back;
//! `install` reads again and refuses, installing nothing, when any of it
//! differs (`Save::Changed`). The basis also holds the fingerprint that
//! `residue service status` printed, and `install` passes it as `--expect`:
//! residue compares it with the service right before it writes, so a change
//! after `install`'s own read is refused too, with nothing written (#11).

use crate::cli::{CliError, ErrorKind, Run, shown};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

/// The launchd label residue's service uses.
pub const LABEL: &str = "com.whiteheap.residue.watch";

/// `residue watch` options that take a value. `--fetch-every` is new in
/// residue 0.0.7, which writes it (or `--no-fetch`) into every service;
/// `--keep-removed` is new after 0.0.7 (residue-cli#55), and the CLI that has
/// it writes it into every service too.
const VALUE_OPTIONS: &[&str] = &[
    "--grace",
    "--idle",
    "--min-free",
    "--fetch-every",
    "--keep-removed",
];

/// `residue watch` options without a value that a save passes on.
/// `--dry-run` is the dialog's own checkbox, so it is not one of them.
const FLAG_OPTIONS: &[&str] = &["--no-fetch"];

/// residue's defaults for the options a service does not set, from
/// `residue watch --help` and `residue --help` of 0.0.6 and of 0.0.7. The CLI
/// owns them (`src/watch.rs` and `src/cli.rs` in residue-cli): the pull request
/// that moves sidecar.lock to a release that changes one updates it here.
pub const DEFAULT_GRACE: &str = "1h";
pub const DEFAULT_IDLE: &str = "2d";
pub const DEFAULT_MIN_FREE: &str = "10";
/// `--fetch-every` of residue 0.0.7 and newer; 0.0.6 never fetches.
pub const DEFAULT_FETCH_EVERY: &str = "30m";
/// `residue remove --keep-trash`. Automatic cleanup and the app's own
/// removals never pass one, so it is the trash's retention for both.
pub const DEFAULT_KEEP_TRASH: &str = "7d";
/// `--keep-removed` of `residue remove` and of `watch` (`cli::DEFAULT_KEEP_REMOVED`
/// in residue-cli): how long a removal record is kept. The app's own removals
/// never pass one; a service that does not set one forgets records after it.
pub const DEFAULT_KEEP_REMOVED: &str = "30d";
/// The first release whose `watch` fetches (`--fetch-every`, `--no-fetch`).
const FETCH_SINCE: (u64, u64, u64) = (0, 0, 7);
const FETCH_SINCE_TEXT: &str = "0.0.7";

/// Everything a service operation needs to know about this machine.
pub struct Context<'a> {
    pub sidecar: &'a Path,
    pub bundled_version: &'a str,
    pub data_dir: &'a Path,
    pub home: &'a Path,
    pub path_env: &'a OsStr,
    /// The app's one change of the service at a time (`install`, `uninstall`).
    pub changes: &'a Changes,
}

/// What the installed agent runs, read from its plist.
#[derive(Debug, Clone, Serialize)]
pub struct Program {
    pub binary: String,
    pub binary_exists: bool,
    /// `residue --version` of that binary, when it runs.
    pub version: Option<String>,
    /// The binary is the app's own copy.
    pub managed: bool,
    /// The absolute paths among the arguments: residue writes every ROOT
    /// canonical.
    pub roots: Vec<String>,
    /// The options a save passes on, as written: `["--grace", "1h",
    /// "--no-fetch"]`. Every option is one, including those the app does not
    /// know.
    pub options: Vec<String>,
    /// The options among `options` that the app does not know, such as one a
    /// newer residue added (`--some-new-option 2d`), with their values. A save
    /// passes them on as written.
    pub other_options: Vec<String>,
    pub dry_run: bool,
    /// Arguments the app cannot read: a program other than `residue watch`,
    /// or an argument that is neither an option nor a folder. While there are
    /// any, the app does not save the service, since a save would drop them.
    pub unknown: Vec<String>,
    /// What the service does: its options, and residue's defaults for the
    /// ones it does not set.
    pub settings: WatchSettings,
}

/// One `residue watch` setting.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Setting {
    /// As residue takes it: `1h`, `2d`, `10`.
    pub value: String,
    /// The service does not set it: this is residue's default.
    pub default: bool,
}

/// Whether `residue watch` fetches, so it sees a branch deleted on the
/// remote without the user's own `git fetch --prune`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Fetch {
    /// About every `value` (residue 0.0.7 and newer).
    Every { value: String, default: bool },
    /// `--no-fetch`.
    Off,
    /// The binary is older than `since`: it never fetches. From `since` on,
    /// residue fetches about every `every` unless `--no-fetch`.
    Never { since: String, every: String },
    /// The binary's version is unknown, so whether it fetches is too.
    Unknown { since: String, every: String },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WatchSettings {
    pub grace: Setting,
    pub idle: Setting,
    pub min_free: Setting,
    pub fetch: Fetch,
    /// The trash retention of the removals watch runs: it passes no
    /// `--keep-trash`, so this is always residue's default.
    pub keep_trash: Setting,
    /// How long the removal records of its repositories are kept: the
    /// service's `--keep-removed`, or residue's default.
    pub keep_removed: Setting,
}

/// What turning the service on, or saving other folders, installs.
#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    /// The binary that installs the service, and that the service then runs.
    pub binary: String,
    pub version: Option<String>,
    /// The app's own copy of the bundled binary.
    pub managed: bool,
    /// The options a save passes on, as the running service has them. Empty
    /// when the service is off: turning it on starts from residue's defaults.
    pub keep: Vec<String>,
    /// What the service does after the save.
    pub settings: WatchSettings,
    /// Why the app will not save, when it will not.
    pub blocked: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub installed: bool,
    /// launchd's state, such as `running`.
    pub state: Option<String>,
    pub pid: Option<u32>,
    /// The first line of `residue service status`.
    pub summary: String,
    /// The agent's plist, when the file exists.
    pub agent_file: Option<String>,
    pub log_path: Option<String>,
    pub log_lines: Vec<String>,
    /// The log exists but could not be read.
    pub log_error: Option<String>,
    pub program: Option<Program>,
    pub plan: Plan,
    /// What a save is planned from; the window sends it back with the save.
    pub basis: Basis,
    /// What launchd runs, read with `launchctl print`, when it is not what
    /// the agent file says: residue writes the file before it restarts the
    /// job, so a save that failed after the write leaves the earlier job
    /// running until the next login. `None` when they agree, when launchd
    /// runs nothing, or when its answer cannot be read. Only shown: a save is
    /// still planned from the agent file (`basis`).
    pub launchd_job: Option<Job>,
    /// The development preview mode is on: nothing was installed.
    pub dev_dry_run: bool,
}

/// A `residue watch` job as launchd holds it, from its `arguments`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Job {
    pub binary: String,
    pub roots: Vec<String>,
    pub options: Vec<String>,
    pub dry_run: bool,
    /// Arguments that are neither an option nor a folder, or a program other
    /// than `residue watch`, as for `Program.unknown`.
    pub unknown: Vec<String>,
}

impl Job {
    /// The job of these arguments, `binary watch ARGS...` or any other.
    fn of(arguments: &[String]) -> Option<Job> {
        let (binary, rest) = arguments.split_first()?;
        let parsed = match rest.split_first() {
            Some((command, args)) if command == "watch" => parse_watch(args),
            _ => Parsed {
                unknown: rest.to_vec(),
                ..Parsed::default()
            },
        };
        Some(Job {
            binary: binary.clone(),
            roots: parsed.roots,
            options: parsed.options,
            dry_run: parsed.dry_run,
            unknown: parsed.unknown,
        })
    }

    /// Whether the agent file says the same: binary, folders, options and
    /// mode. No file never does.
    fn is_the_file(&self, file: Option<&Program>) -> bool {
        file.is_some_and(|p| {
            p.binary == self.binary
                && p.roots == self.roots
                && p.options == self.options
                && p.dry_run == self.dry_run
                && p.unknown == self.unknown
        })
    }
}

/// Everything the save dialog shows about what a save installs and what it
/// replaces. `install` refuses when a fresh read differs in any of it: a
/// `brew upgrade` that brings a residue which fetches, or a terminal that
/// switches the service to `--dry-run` or other folders, while the dialog is
/// open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Basis {
    /// launchd runs the service, so a save replaces it.
    pub installed: bool,
    /// The binary a save installs with (`Plan.binary`), and its
    /// `residue --version`.
    pub binary: String,
    pub version: Option<String>,
    /// The options a save passes on (`Plan.keep`).
    pub keep: Vec<String>,
    /// The service a save replaces, when launchd runs one and its agent file
    /// can be read.
    pub running: Option<Running>,
    /// The `fingerprint:` line of `residue service status`, run by the
    /// binary a save installs with (`installs_as`). It names the agent file,
    /// the binary it starts, whether launchd loaded it, and that residue's
    /// release. A save passes it as `--expect`. `None` when residue printed
    /// none: then a save installs nothing.
    pub fingerprint: Option<String>,
}

/// `residue service install --expect`'s exit status when the service is not
/// the one the fingerprint names: residue wrote nothing (residue-cli's
/// `service::CHANGED`, from 0.0.12).
const EXPECT_CHANGED: i32 = 4;

/// A fingerprint as `residue service status` prints it: 32 lowercase hex
/// digits. `fingerprint: unknown (...)` and anything else are none.
fn fingerprint_in(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix("fingerprint: "))
        .map(str::trim)
        .filter(|f| f.len() == 32 && f.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
        .map(str::to_string)
}

/// The service a save replaces, as the window showed it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Running {
    pub binary: String,
    pub version: Option<String>,
    pub roots: Vec<String>,
    pub dry_run: bool,
}

impl Basis {
    fn of(
        installed: bool,
        program: Option<&Program>,
        plan: &Plan,
        fingerprint: Option<String>,
    ) -> Self {
        Basis {
            installed,
            binary: plan.binary.clone(),
            version: plan.version.clone(),
            keep: plan.keep.clone(),
            running: program.filter(|_| installed).map(|p| Running {
                binary: p.binary.clone(),
                version: p.version.clone(),
                roots: p.roots.clone(),
                dry_run: p.dry_run,
            }),
            fingerprint,
        }
    }

    /// What differs from `seen`, one line each, `what: before → after`. Each
    /// field of the basis has its line, so a basis that differs never has
    /// an empty list.
    fn changes_since(&self, seen: &Basis) -> Vec<String> {
        let mut out = Vec::new();
        let mut line = |what: &str, before: String, after: String| {
            if before != after {
                out.push(format!("{what}: {before} → {after}"));
            }
        };
        let on_off = |on: bool| if on { "on" } else { "off" }.to_string();
        line(
            "Automatic cleanup",
            on_off(seen.installed),
            on_off(self.installed),
        );
        match (&seen.running, &self.running) {
            (Some(before), Some(after)) => {
                let (what, from, to) = binary_change(
                    "The service runs",
                    (&before.binary, before.version.as_deref()),
                    (&after.binary, after.version.as_deref()),
                );
                line(&what, from, to);
                line(
                    "The service watches",
                    before.roots.join(", "),
                    after.roots.join(", "),
                );
                line(
                    "The service's mode",
                    mode_text(before.dry_run),
                    mode_text(after.dry_run),
                );
            }
            (before, after) if seen.installed && self.installed => line(
                "The service's agent file",
                readable_text(before.is_some()),
                readable_text(after.is_some()),
            ),
            _ => {}
        }
        let (what, from, to) = binary_change(
            "A save installs with",
            (&seen.binary, seen.version.as_deref()),
            (&self.binary, self.version.as_deref()),
        );
        line(&what, from, to);
        line(
            "Options a save keeps",
            options_text(&seen.keep),
            options_text(&self.keep),
        );
        line(
            "Fetching after a save",
            fetch_text(&watch_settings(&seen.keep, seen.version.as_deref()).fetch),
            fetch_text(&watch_settings(&self.keep, self.version.as_deref()).fetch),
        );
        if out.is_empty() && self != seen {
            out.push("The service changed after this window read it.".into());
        }
        out
    }
}

/// The label and both sides of a binary's line. When the path stayed, as
/// after `brew upgrade`, only the version changed: the path goes in the
/// label once instead of on both sides.
fn binary_change(
    what: &str,
    (before, before_version): (&str, Option<&str>),
    (after, after_version): (&str, Option<&str>),
) -> (String, String, String) {
    if before == after {
        (
            format!("{what} {before}"),
            version_text(before_version),
            version_text(after_version),
        )
    } else {
        (
            what.to_string(),
            format!("{before} ({})", version_text(before_version)),
            format!("{after} ({})", version_text(after_version)),
        )
    }
}

/// A `residue --version` line as read, which already names residue.
fn version_text(version: Option<&str>) -> String {
    version.unwrap_or("version unknown").to_string()
}

fn mode_text(dry_run: bool) -> String {
    if dry_run {
        "log only (--dry-run)".into()
    } else {
        "removes".into()
    }
}

fn readable_text(read: bool) -> String {
    if read { "read" } else { "missing" }.into()
}

fn options_text(options: &[String]) -> String {
    if options.is_empty() {
        "none, residue's defaults".into()
    } else {
        options.join(" ")
    }
}

fn fetch_text(fetch: &Fetch) -> String {
    match fetch {
        Fetch::Every { value, .. } => format!("about every {value}"),
        Fetch::Off => "off (--no-fetch)".into(),
        Fetch::Never { .. } => "never".into(),
        Fetch::Unknown { .. } => "unknown".into(),
    }
}

/// The result of a save.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Save {
    /// The service was installed, or in the development preview would have
    /// been.
    Saved { change: Change },
    /// A fresh read differs from what the window showed, so nothing was
    /// installed: what changed, and the service as it is now.
    Changed {
        changes: Vec<String>,
        status: Status,
    },
}

/// The result of turning the service on or off.
#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub command: String,
    pub output: Vec<String>,
    pub dev_dry_run: bool,
    pub status: Status,
}

/// What `residue watch --once --dry-run` decided, which removes nothing.
#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    pub command: String,
    pub lines: Vec<String>,
    /// Whether it fetched first, as the service's passes do.
    pub fetch: Fetch,
}

/// The preview mode for development builds. With
/// `RESIDUE_APP_DEV_SERVICE=preview`, a debug build never runs
/// `residue service install` or `uninstall`: launchd acts on the user's real
/// session and the label is shared with any residue service already running.
/// The switch then shows the command it would run and the output of
/// `residue watch --once --dry-run`, which removes nothing, and remembers the
/// pretend state until the app quits. With `RESIDUE_APP_DEV_AGENT_FILE`
/// set to a plist as well, the preview reads that file as the installed
/// service's agent file, on every read, until the switch is used: a change
/// to the file shows as a change to the service, as it would for the real
/// one. The app only reads it. Release builds ignore both.
pub struct DevPreview {
    pub enabled: bool,
    state: Mutex<Pretend>,
}

#[derive(Clone)]
enum Pretend {
    /// `RESIDUE_APP_DEV_AGENT_FILE`, read on every read.
    File(PathBuf),
    /// What the switch set: the pretend service and its log lines, or none.
    Set(Option<Box<(Program, Vec<String>)>>),
}

/// A poisoned lock still holds a whole value here: every writer replaces it
/// in one assignment.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl DevPreview {
    pub fn from_env() -> Self {
        let enabled = cfg!(debug_assertions)
            && std::env::var("RESIDUE_APP_DEV_SERVICE").as_deref() == Ok("preview");
        let state = match std::env::var_os("RESIDUE_APP_DEV_AGENT_FILE") {
            Some(path) if enabled && !path.is_empty() => Pretend::File(PathBuf::from(path)),
            _ => Pretend::Set(None),
        };
        DevPreview {
            enabled,
            state: Mutex::new(state),
        }
    }

    /// The pretend service. A file that cannot be read fails the read with
    /// its reason, as the real agent file does.
    fn get(&self, ctx: &Context) -> Result<Option<(Program, Vec<String>)>, CliError> {
        let state = lock(&self.state).clone();
        match state {
            Pretend::File(path) => Ok(Some((
                read_program(ctx, &path)?,
                vec![format!(
                    "development preview: read from {}, which the app only reads",
                    path.display()
                )],
            ))),
            Pretend::Set(current) => Ok(current.map(|c| *c)),
        }
    }

    fn set(&self, value: Option<(Program, Vec<String>)>) {
        *lock(&self.state) = Pretend::Set(value.map(Box::new));
    }
}

fn agent_file(home: &Path) -> PathBuf {
    home.join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist"))
}

fn default_log(home: &Path) -> PathBuf {
    home.join("Library/Logs/residue/watch.log")
}

fn managed_binary(data_dir: &Path) -> PathBuf {
    data_dir.join("bin/residue")
}

pub fn status(ctx: &Context, dev: &DevPreview) -> Result<Status, CliError> {
    read(ctx, dev).map(|(status, _)| status)
}

/// The service's status, and the binary a save would install with.
fn read(ctx: &Context, dev: &DevPreview) -> Result<(Status, Installer), CliError> {
    let installer = find_installer(ctx)?;
    if dev.enabled {
        let current = dev.get(ctx)?;
        let installed = current.is_some();
        let program = current.as_ref().map(|(p, _)| p);
        let plan = plan(ctx, &installer, installed, program);
        let basis = Basis::of(installed, program, &plan, None);
        let status = match current {
            Some((program, log_lines)) => Status {
                installed: true,
                state: Some("running".into()),
                pid: None,
                summary: format!(
                    "launchd agent {LABEL}: running (development preview, not installed)"
                ),
                agent_file: None,
                log_path: None,
                log_lines,
                log_error: None,
                program: Some(program),
                plan,
                basis,
                launchd_job: None,
                dev_dry_run: true,
            },
            None => Status {
                installed: false,
                state: None,
                pid: None,
                summary: format!("launchd agent {LABEL}: not installed (development preview)"),
                agent_file: None,
                log_path: None,
                log_lines: Vec::new(),
                log_error: None,
                program: None,
                plan,
                basis,
                launchd_job: None,
                dev_dry_run: true,
            },
        };
        return Ok((status, installer));
    }

    // `residue service status` only reads: launchctl print and the log. The
    // residue a save installs with runs it, since its fingerprint names that
    // release, and before the agent file is read, so a change after it
    // makes the save's `--expect` refuse rather than go unseen.
    let (bin, path_env) = installs_as(ctx, &installer);
    let out = Run {
        bin,
        args: vec!["service".into(), "status".into()],
        cwd: ctx.home,
        path_env,
        deadline: Duration::from_secs(20),
    }
    .output()?;
    let installed = match out.code {
        Some(0) => true,
        Some(3) => false,
        _ => {
            return Err(out.error(
                ErrorKind::Failed,
                "residue service status did not report a state".into(),
            ));
        }
    };
    let mut lines = out.stdout.lines();
    let summary = lines.next().unwrap_or_default().trim().to_string();
    let (state, pid) = parse_state(&summary);
    let log_path = out
        .stdout
        .lines()
        .find_map(|l| l.strip_prefix("log: "))
        .map(PathBuf::from)
        .unwrap_or_else(|| default_log(ctx.home));

    let plist = agent_file(ctx.home);
    let program = if plist.exists() {
        Some(read_program(ctx, &plist)?)
    } else {
        None
    };
    let plan = plan(ctx, &installer, installed, program.as_ref());
    let basis = Basis::of(
        installed,
        program.as_ref(),
        &plan,
        fingerprint_in(&out.stdout),
    );
    let launchd_job = if installed {
        launchd_job(ctx).filter(|job| !job.is_the_file(program.as_ref()))
    } else {
        None
    };
    let (log_lines, log_error) = match tail(&log_path, 40) {
        Ok(lines) => (lines, None),
        Err(e) => (Vec::new(), Some(e)),
    };
    let status = Status {
        installed,
        state: if installed { state } else { None },
        pid: if installed { pid } else { None },
        summary,
        agent_file: plist.exists().then(|| plist.to_string_lossy().into_owned()),
        log_lines,
        log_error,
        log_path: Some(log_path.to_string_lossy().into_owned()),
        program,
        plan,
        basis,
        launchd_job,
        dev_dry_run: false,
    };
    Ok((status, installer))
}

/// The job launchd holds for the service, from `launchctl print`, which only
/// reads. `None` when it fails or prints no `arguments` the app can read.
#[cfg(target_os = "macos")]
fn launchd_job(ctx: &Context) -> Option<Job> {
    // SAFETY: getuid has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    let out = Run {
        bin: Path::new("/bin/launchctl"),
        args: vec!["print".into(), format!("gui/{uid}/{LABEL}").into()],
        cwd: ctx.home,
        path_env: ctx.path_env,
        deadline: Duration::from_secs(10),
    }
    .output()
    .ok()?;
    if out.code != Some(0) {
        return None;
    }
    Job::of(&launchd_arguments(&out.stdout)?)
}

#[cfg(not(target_os = "macos"))]
fn launchd_job(_: &Context) -> Option<Job> {
    None
}

/// The `arguments = { ... }` block of `launchctl print SERVICE`: one
/// argument per line, each indented one tab more than the block's own
/// line, which is the service's top level. Anything else in it, or a block
/// that does not close, is `None`: never a guess.
fn launchd_arguments(print: &str) -> Option<Vec<String>> {
    let mut lines = print.lines();
    lines.find(|l| *l == "\targuments = {")?;
    let mut out = Vec::new();
    for line in lines {
        if line == "\t}" {
            return (!out.is_empty()).then_some(out);
        }
        // A deeper line is not an argument, and a line that is not indented
        // as one is the rest of an argument with a newline in it.
        let arg = line.strip_prefix("\t\t")?;
        if arg.starts_with('\t') {
            return None;
        }
        out.push(arg.to_string());
    }
    None
}

#[cfg(test)]
mod launchd_tests {
    use super::*;

    /// The shape `launchctl print gui/UID/com.whiteheap.residue.watch` has on
    /// macOS 26, trimmed: tabs indent each level.
    const PRINT: &str = "gui/501/com.whiteheap.residue.watch = {
\tactive count = 1
\tpath = /Users/me/Library/LaunchAgents/com.whiteheap.residue.watch.plist
\ttype = LaunchAgent
\tstate = running

\tprogram = /opt/homebrew/bin/residue
\targuments = {
\t\t/opt/homebrew/bin/residue
\t\twatch
\t\t--grace
\t\t1h
\t\t--dry-run
\t\t/Users/me/src
\t\t/Users/me/My Code
\t}

\tinherited environment = {
\t\tSSH_AUTH_SOCK => /private/tmp/com.apple.launchd.x/Listeners
\t}

\tpid = 20878
\tproperties = keepalive | runatload | low priority i/o | inferred program
}
";

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    fn file(arguments: &[&str]) -> Program {
        let job = Job::of(&args(arguments)).unwrap();
        Program {
            binary: job.binary,
            binary_exists: true,
            version: Some("residue 0.0.12".into()),
            managed: false,
            roots: job.roots,
            other_options: other_options(&job.options),
            settings: watch_settings(&job.options, Some("residue 0.0.12")),
            options: job.options,
            dry_run: job.dry_run,
            unknown: job.unknown,
        }
    }

    #[test]
    fn reads_the_arguments_block() {
        assert_eq!(
            launchd_arguments(PRINT),
            Some(args(&[
                "/opt/homebrew/bin/residue",
                "watch",
                "--grace",
                "1h",
                "--dry-run",
                "/Users/me/src",
                "/Users/me/My Code",
            ]))
        );
    }

    #[test]
    fn reads_nothing_it_cannot_be_sure_of() {
        // No block, an empty one, one that does not close, a deeper line,
        // and an argument with a newline in it.
        assert_eq!(
            launchd_arguments("gui/501/x = {\n\tstate = running\n}\n"),
            None
        );
        assert_eq!(launchd_arguments("\targuments = {\n\t}\n"), None);
        assert_eq!(
            launchd_arguments("\targuments = {\n\t\t/bin/residue\n"),
            None
        );
        assert_eq!(
            launchd_arguments("\targuments = {\n\t\t/bin/residue\n\t\t\tdeeper\n\t}\n"),
            None
        );
        assert_eq!(
            launchd_arguments("\targuments = {\n\t\t/bin/residue\n/Users/me/a\nb\n\t}\n"),
            None
        );
    }

    #[test]
    fn a_job_that_matches_the_file_is_not_shown() {
        let running = Job::of(&launchd_arguments(PRINT).unwrap()).unwrap();
        assert_eq!(running.roots, args(&["/Users/me/src", "/Users/me/My Code"]));
        assert!(running.dry_run);
        assert_eq!(running.options, args(&["--grace", "1h"]));
        let same = file(&[
            "/opt/homebrew/bin/residue",
            "watch",
            "--grace",
            "1h",
            "--dry-run",
            "/Users/me/src",
            "/Users/me/My Code",
        ]);
        assert!(running.is_the_file(Some(&same)));
    }

    #[test]
    fn a_job_the_file_no_longer_describes_is_shown() {
        let running = Job::of(&launchd_arguments(PRINT).unwrap()).unwrap();
        // A save wrote other folders and removing mode, then failed to
        // restart the job.
        let saved = file(&[
            "/opt/homebrew/bin/residue",
            "watch",
            "--grace",
            "1h",
            "/Users/me/work",
        ]);
        assert!(!running.is_the_file(Some(&saved)));
        let other_binary = file(&[
            "/Users/me/Library/Application Support/com.whiteheap.residue/bin/residue",
            "watch",
            "--grace",
            "1h",
            "--dry-run",
            "/Users/me/src",
            "/Users/me/My Code",
        ]);
        assert!(!running.is_the_file(Some(&other_binary)));
        assert!(!running.is_the_file(None));
    }

    #[test]
    fn a_program_other_than_watch_is_unknown() {
        let job = Job::of(&args(&["/bin/sh", "-c", "true"])).unwrap();
        assert_eq!(job.unknown, args(&["-c", "true"]));
        assert!(job.roots.is_empty());
        assert_eq!(Job::of(&[]), None);
    }
}

/// `launchd agent LABEL: running (pid 84497)` -> (`running`, 84497).
fn parse_state(line: &str) -> (Option<String>, Option<u32>) {
    let Some((_, rest)) = line.split_once(": ") else {
        return (None, None);
    };
    match rest.split_once(" (pid ") {
        Some((state, pid)) => (
            Some(state.trim().to_string()),
            pid.trim_end_matches(')').trim().parse().ok(),
        ),
        None => (Some(rest.trim().to_string()), None),
    }
}

/// The last `n` lines of the log, read from at most its last 64 KB. A log
/// that does not exist yet has no lines; any other failure is reported.
fn tail(path: &Path, n: usize) -> Result<Vec<String>, String> {
    use std::io::Seek;
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let read = (|| -> std::io::Result<(u64, Vec<u8>)> {
        let len = file.metadata()?.len();
        let start = len.saturating_sub(64 * 1024);
        file.seek(std::io::SeekFrom::Start(start))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok((start, bytes))
    })();
    let (start, bytes) = read.map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<&str> = text.lines().collect();
    if start > 0 && !lines.is_empty() {
        // The first line is probably cut in the middle.
        lines.remove(0);
    }
    Ok(lines[lines.len().saturating_sub(n)..]
        .iter()
        .map(|l| l.to_string())
        .collect())
}

/// What the agent runs, from its plist's `ProgramArguments`.
fn read_program(ctx: &Context, plist: &Path) -> Result<Program, CliError> {
    let out = Run {
        bin: Path::new("/usr/bin/plutil"),
        args: vec![
            "-convert".into(),
            "json".into(),
            "-o".into(),
            "-".into(),
            plist.as_os_str().to_owned(),
        ],
        cwd: ctx.home,
        path_env: ctx.path_env,
        deadline: Duration::from_secs(10),
    }
    .output()?;
    if out.code != Some(0) {
        return Err(out.error(
            ErrorKind::Failed,
            format!("cannot read {}", plist.display()),
        ));
    }
    let doc: Value = serde_json::from_str(&out.stdout).map_err(|e| {
        out.error(
            ErrorKind::BadOutput,
            format!(
                "{} is not a property list the app can read ({e})",
                plist.display()
            ),
        )
    })?;
    let args: Vec<String> = doc
        .get("ProgramArguments")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let Some((binary, rest)) = args.split_first() else {
        return Err(CliError::failed(format!(
            "{} has no ProgramArguments",
            plist.display()
        )));
    };
    let binary_path = PathBuf::from(binary);
    let binary_exists = binary_path.is_file();
    let version = if binary_exists {
        version_of(&binary_path, ctx)
    } else {
        None
    };
    let parsed = match rest.split_first() {
        Some((command, args)) if command == "watch" => parse_watch(args),
        _ => Parsed {
            unknown: rest.to_vec(),
            ..Parsed::default()
        },
    };
    let settings = watch_settings(&parsed.options, version.as_deref());
    Ok(Program {
        binary: binary.clone(),
        binary_exists,
        version,
        managed: binary_path == managed_binary(ctx.data_dir),
        roots: parsed.roots,
        other_options: other_options(&parsed.options),
        options: parsed.options,
        dry_run: parsed.dry_run,
        unknown: parsed.unknown,
        settings,
    })
}

#[derive(Default)]
struct Parsed {
    roots: Vec<String>,
    options: Vec<String>,
    dry_run: bool,
    unknown: Vec<String>,
}

/// Sort the arguments after `watch` the way residue writes them: its
/// options, then the folders, each an absolute canonical path. An option the
/// app does not know (`--some-new-option 2d`, which a newer residue may write) is
/// one of the options and is passed on as written. It takes the next argument
/// as its value unless that argument is an option or an absolute path: no
/// `watch` option takes an absolute path. Anything else is unknown, never a
/// folder.
fn parse_watch(args: &[String]) -> Parsed {
    let mut parsed = Parsed::default();
    let mut rest = args.iter().peekable();
    while let Some(arg) = rest.next() {
        if VALUE_OPTIONS.contains(&arg.as_str()) {
            match rest.next() {
                Some(value) => parsed.options.extend([arg.clone(), value.clone()]),
                None => parsed.unknown.push(arg.clone()),
            }
        } else if FLAG_OPTIONS.contains(&arg.as_str()) {
            parsed.options.push(arg.clone());
        } else if arg == "--dry-run" {
            parsed.dry_run = true;
        } else if arg.len() > 2 && arg.starts_with("--") {
            parsed.options.push(arg.clone());
            if let Some(value) = rest.next_if(|next| is_value(next)) {
                parsed.options.push(value.clone());
            }
        } else if !arg.starts_with('-') && Path::new(arg).is_absolute() {
            parsed.roots.push(arg.clone());
        } else {
            parsed.unknown.push(arg.clone());
        }
    }
    parsed
}

/// An argument that can be the value of an option the app does not know:
/// neither an option nor a folder.
fn is_value(arg: &str) -> bool {
    !arg.starts_with('-') && !Path::new(arg).is_absolute()
}

/// The options among `options` (as `parse_watch` sorts them) that the app
/// does not know, each with its value.
fn other_options(options: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut it = options.iter().peekable();
    while let Some(option) = it.next() {
        if VALUE_OPTIONS.contains(&option.as_str()) {
            it.next();
        } else if !FLAG_OPTIONS.contains(&option.as_str()) {
            out.push(option.clone());
            if let Some(value) = it.next_if(|next| is_value(next)) {
                out.push(value.clone());
            }
        }
    }
    out
}

/// What `residue watch` does with these options when a binary of this
/// `residue --version` runs it: the options it sets, and residue's defaults
/// for the rest.
fn watch_settings(options: &[String], version: Option<&str>) -> WatchSettings {
    let mut set: HashMap<&str, &str> = HashMap::new();
    let mut no_fetch = false;
    let mut it = options.iter();
    while let Some(option) = it.next() {
        if VALUE_OPTIONS.contains(&option.as_str()) {
            // A later one replaces an earlier one, as in residue.
            if let Some(value) = it.next() {
                set.insert(option, value);
            }
        } else if option == "--no-fetch" {
            no_fetch = true;
        }
    }
    let setting = |flag: &str, default: &str| match set.get(flag) {
        Some(value) => Setting {
            value: (*value).to_string(),
            default: false,
        },
        None => Setting {
            value: default.to_string(),
            default: true,
        },
    };
    let fetch = if no_fetch {
        Fetch::Off
    } else if let Some(value) = set.get("--fetch-every") {
        Fetch::Every {
            value: (*value).to_string(),
            default: false,
        }
    } else {
        match fetches(version) {
            Some(true) => Fetch::Every {
                value: DEFAULT_FETCH_EVERY.into(),
                default: true,
            },
            Some(false) => Fetch::Never {
                since: FETCH_SINCE_TEXT.into(),
                every: DEFAULT_FETCH_EVERY.into(),
            },
            None => Fetch::Unknown {
                since: FETCH_SINCE_TEXT.into(),
                every: DEFAULT_FETCH_EVERY.into(),
            },
        }
    };
    WatchSettings {
        grace: setting("--grace", DEFAULT_GRACE),
        idle: setting("--idle", DEFAULT_IDLE),
        min_free: setting("--min-free", DEFAULT_MIN_FREE),
        fetch,
        keep_trash: Setting {
            value: DEFAULT_KEEP_TRASH.into(),
            default: true,
        },
        keep_removed: setting("--keep-removed", DEFAULT_KEEP_REMOVED),
    }
}

/// The fetch options (`--fetch-every X`, `--no-fetch`), and the others.
fn split_fetch(options: &[String]) -> (Vec<String>, Vec<String>) {
    let mut fetch = Vec::new();
    let mut other = Vec::new();
    let mut it = options.iter();
    while let Some(option) = it.next() {
        let into = if option == "--fetch-every" || FLAG_OPTIONS.contains(&option.as_str()) {
            &mut fetch
        } else {
            &mut other
        };
        into.push(option.clone());
        if VALUE_OPTIONS.contains(&option.as_str())
            && let Some(value) = it.next()
        {
            into.push(value.clone());
        }
    }
    (fetch, other)
}

/// `residue --version` of a binary: `residue 0.0.6`.
fn version_of(bin: &Path, ctx: &Context) -> Option<String> {
    version_within(bin, ctx, Duration::from_secs(5))
}

fn version_within(bin: &Path, ctx: &Context, deadline: Duration) -> Option<String> {
    let out = Run {
        bin,
        args: vec!["--version".into()],
        cwd: ctx.home,
        path_env: ctx.path_env,
        deadline,
    }
    .output()
    .ok()?;
    let text = out.stdout.trim();
    (out.code == Some(0) && text.starts_with("residue ")).then(|| text.to_string())
}

/// `residue 0.0.6` -> (0, 0, 6).
fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let v = text.strip_prefix("residue ")?.trim();
    let mut parts = v.split(['.', '-', '+']).map(|p| p.parse::<u64>());
    Some((
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    ))
}

/// Whether `residue watch` of this `residue --version` fetches; `None` when
/// the version is unknown.
fn fetches(version: Option<&str>) -> Option<bool> {
    version.and_then(parse_version).map(|v| v >= FETCH_SINCE)
}

/// The binary a save installs with, and the PATH to install it with.
struct Installer {
    bin: PathBuf,
    managed: bool,
    path_env: OsString,
    version: Option<String>,
}

/// Find the binary to install with. This only reads: the app's own copy is
/// written when a save uses it.
fn find_installer(ctx: &Context) -> Result<Installer, CliError> {
    let bundled = parse_version(&format!("residue {}", ctx.bundled_version));
    let managed = managed_binary(ctx.data_dir);
    // The app's own executable is named residue too: a PATH that holds a
    // development build's folder must never make the app run itself. The
    // sidecar's path came from current_exe, so it does not fail here; if it
    // did, the sidecar is still skipped.
    let own = std::env::current_exe().ok();
    let skip: Vec<PathBuf> = [Some(ctx.sidecar), Some(managed.as_path()), own.as_deref()]
        .into_iter()
        .flatten()
        .filter_map(|p| p.canonicalize().ok())
        .collect();
    let mut dirs: Vec<PathBuf> = std::env::split_paths(ctx.path_env).collect();
    for extra in [
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        ctx.home.join(".cargo/bin"),
        ctx.home.join(".local/bin"),
    ] {
        if !dirs.contains(&extra) {
            dirs.push(extra);
        }
    }
    for dir in dirs {
        let candidate = dir.join("residue");
        if !candidate.is_file() || !candidate.is_absolute() {
            continue;
        }
        let Ok(real) = candidate.canonicalize() else {
            continue;
        };
        // Never a binary inside an app bundle: it moves with the app.
        if skip.contains(&real) || real.to_string_lossy().contains(".app/Contents/") {
            continue;
        }
        let version = version_of(&candidate, ctx);
        if let (Some(found), Some(bundled)) = (version.as_deref().and_then(parse_version), bundled)
            && found >= bundled
        {
            let mut path: Vec<PathBuf> = vec![dir.clone()];
            path.extend(std::env::split_paths(ctx.path_env).filter(|d| d != &dir));
            let path_env = std::env::join_paths(path)
                .map_err(|e| CliError::failed(format!("cannot build PATH: {e}")))?;
            return Ok(Installer {
                bin: candidate,
                managed: false,
                path_env,
                version,
            });
        }
    }
    Ok(Installer {
        bin: managed,
        managed: true,
        path_env: ctx.path_env.to_owned(),
        // The copy is written from the bundled binary when a save uses it.
        version: Some(format!("residue {}", ctx.bundled_version)),
    })
}

/// What a save installs: the binary, the options it passes on, and why it
/// will not save, when it will not. Only a service launchd runs has options
/// to keep: an agent file it has not loaded is left over, and turning the
/// service on starts from residue's defaults.
fn plan(ctx: &Context, installer: &Installer, installed: bool, program: Option<&Program>) -> Plan {
    let mut blocked = None;
    let keep = match (installed, program) {
        (false, _) => Vec::new(),
        (true, Some(program)) => {
            if !program.unknown.is_empty() {
                blocked = Some(format!(
                    "A save would drop arguments the app cannot read: {}. Change the service with \
                     residue service install in a terminal, or turn automatic cleanup off and on \
                     again to start from residue's defaults.",
                    program.unknown.join(" ")
                ));
            }
            program.options.clone()
        }
        (true, None) => {
            blocked = Some(format!(
                "The service runs, but its agent file {} is missing, so a save would reset its \
                 settings to residue's defaults. Turn automatic cleanup off and on again to start \
                 from them.",
                agent_file(ctx.home).display()
            ));
            Vec::new()
        }
    };
    let (fetch, _) = split_fetch(&keep);
    if blocked.is_none() && !fetch.is_empty() && fetches(installer.version.as_deref()) != Some(true)
    {
        blocked = Some(format!(
            "The service sets {}, which needs residue {FETCH_SINCE_TEXT} or newer, and a save \
             would install {} ({}). Install a newer residue, or turn automatic cleanup off and on \
             again to start from residue's defaults.",
            fetch.join(" "),
            installer.bin.display(),
            installer.version.as_deref().unwrap_or("version unknown"),
        ));
    }
    let settings = watch_settings(&keep, installer.version.as_deref());
    Plan {
        binary: installer.bin.to_string_lossy().into_owned(),
        version: installer.version.clone(),
        managed: installer.managed,
        keep,
        settings,
        blocked,
    }
}

/// The binary that runs as the residue a save installs with, and its PATH.
/// A save writes the app's own copy from the bundled binary, so for that
/// copy it is the bundled binary: the copy may still be an older release.
fn installs_as<'a>(ctx: &'a Context, installer: &'a Installer) -> (&'a Path, &'a OsStr) {
    if installer.managed {
        (ctx.sidecar, ctx.path_env)
    } else {
        (installer.bin.as_path(), installer.path_env.as_os_str())
    }
}

/// Copy the bundled binary to `dest`: written next to it, then renamed over
/// it, so a running service keeps the file it started from. Only when the
/// copy is the residue the save showed (`expected`, from the read the save
/// planned with): the app's bundle can be replaced on disk while the app
/// runs, and then `ctx.sidecar` is another release.
fn copy_sidecar(ctx: &Context, dest: &Path, expected: Option<&str>) -> Result<(), CliError> {
    let dir = dest
        .parent()
        .ok_or_else(|| CliError::failed(format!("{} has no parent", dest.display())))?;
    std::fs::create_dir_all(dir)
        .map_err(|e| CliError::failed(format!("cannot create {}: {e}", dir.display())))?;
    remove_leftovers(dir);
    let tmp = dir.join(format!(".residue.{}.tmp", std::process::id()));
    let result = (|| -> Result<(), String> {
        let mut from = std::fs::File::open(ctx.sidecar).map_err(|e| e.to_string())?;
        let mut to = create_new(&tmp).map_err(|e| e.to_string())?;
        std::io::copy(&mut from, &mut to).map_err(|e| e.to_string())?;
        to.sync_all().map_err(|e| e.to_string())?;
        drop(to);
        let copied = version_within(&tmp, ctx, Duration::from_secs(30));
        if copied.as_deref() != expected {
            return Err(format!(
                "the app's residue is now {}, and this save showed {}; open the dialog again",
                copied
                    .as_deref()
                    .unwrap_or("a residue whose version cannot be read"),
                expected.unwrap_or("version unknown"),
            ));
        }
        std::fs::rename(&tmp, dest).map_err(|e| e.to_string())
    })();
    result.map_err(|e| {
        // The temporary file is ours; a failed copy leaves nothing behind.
        if let Err(cleanup) = std::fs::remove_file(&tmp)
            && cleanup.kind() != std::io::ErrorKind::NotFound
        {
            return CliError::failed(format!(
                "cannot copy residue to {}: {e} (and cannot remove {}: {cleanup})",
                dest.display(),
                tmp.display()
            ));
        }
        CliError::failed(format!("cannot copy residue to {}: {e}", dest.display()))
    })
}

/// Remove the temporary copies an earlier launch left in `dir`, one that quit
/// or crashed during a copy: `.residue.PID.tmp`. Only the app writes them,
/// and one save runs at a time. A name it cannot remove stays; the copy
/// below still writes a file of its own.
fn remove_leftovers(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(".residue.") && name.ends_with(".tmp") {
            // Removes a link itself, never what it points to.
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// A new file at `path`, executable, opened only when nothing is there:
/// `O_CREAT | O_EXCL`, and `O_NOFOLLOW`, so a link at that name is never
/// written through. A file left there by an earlier launch whose process had
/// the same PID is removed first; `remove_file` removes a link, never what it
/// points to.
fn create_new(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    let open = || {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o755)
            .custom_flags(NOFOLLOW)
            .open(path)
    };
    match open() {
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            std::fs::remove_file(path)?;
            open()
        }
        other => other,
    }
}

#[cfg(target_os = "macos")]
const NOFOLLOW: i32 = libc::O_NOFOLLOW;
/// The app builds for macOS only; elsewhere `create_new`'s `O_EXCL` still
/// refuses a link at the name.
#[cfg(not(target_os = "macos"))]
const NOFOLLOW: i32 = 0;

/// The app's own copy as it was before a save, under a second name in the
/// same folder, until residue has written the agent file: a hard link, or a
/// copy of the file on a disk that has no hard links, such as exFAT. A save
/// whose residue writes nothing puts it back (`restore`), so the service
/// runs the residue it ran; one that wrote lets it go (`forget`). `kept` is
/// `None` when there was no copy: then `restore` removes the one the save
/// made.
struct Previous {
    bin: PathBuf,
    kept: Option<PathBuf>,
}

impl Previous {
    fn keep(bin: &Path) -> Result<Previous, CliError> {
        Previous::keep_with(bin, |from, to| std::fs::hard_link(from, to))
    }

    /// `keep`, with `link` making the second name's hard link.
    fn keep_with(
        bin: &Path,
        link: impl Fn(&Path, &Path) -> std::io::Result<()>,
    ) -> Result<Previous, CliError> {
        let prev = bin.with_file_name(".residue.prev");
        remove_if_there(&prev)
            .map_err(|e| CliError::failed(format!("cannot remove {}: {e}", prev.display())))?;
        let kept = match link(bin, &prev) {
            Ok(()) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            // No hard link here: a copy of the file, permissions included.
            Err(_) => match std::fs::copy(bin, &prev) {
                Ok(_) => true,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(e) => {
                    let _ = remove_if_there(&prev);
                    return Err(CliError::failed(format!(
                        "cannot keep {} as {}: {e}",
                        bin.display(),
                        prev.display()
                    )));
                }
            },
        };
        Ok(Previous {
            bin: bin.to_path_buf(),
            kept: kept.then_some(prev),
        })
    }

    /// Put back what `bin` named before the save, in one rename, or remove
    /// the copy the save made where there was none. A rename between two
    /// links to one file changes nothing, as when the copy never replaced
    /// it, so the kept name is removed after it.
    fn restore(self) -> Result<(), String> {
        let undone = match &self.kept {
            Some(prev) => std::fs::rename(prev, &self.bin),
            None => remove_if_there(&self.bin),
        };
        undone
            .and_then(|()| self.kept.as_deref().map_or(Ok(()), remove_if_there))
            .map_err(|e| format!("cannot put back {}: {e}", self.bin.display()))
    }

    fn forget(self) -> Result<(), String> {
        match &self.kept {
            Some(prev) => {
                remove_if_there(prev).map_err(|e| format!("cannot remove {}: {e}", prev.display()))
            }
            None => Ok(()),
        }
    }
}

fn remove_if_there(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// The `residue watch` arguments: kept options, `--dry-run`, then the roots.
fn watch_args(roots: &[PathBuf], options: &[String], dry_run: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = options.iter().map(OsString::from).collect();
    if dry_run {
        args.push("--dry-run".into());
    }
    args.extend(roots.iter().map(|r| r.as_os_str().to_owned()));
    args
}

/// One change of the service at a time, for the app's whole run: `install`
/// and `uninstall` each hold it (`Changing`) from their start to their
/// return, and refuse, running nothing, while another holds it. launchd
/// keeps whichever `residue service install` finished last, so two at once
/// can leave a service that no dialog showed (#10). The window keeps its
/// screen while a change runs; this also covers a window drawn afresh, whose
/// new dialog reads the service before the first change writes it. It does
/// not cover a quit and a relaunch: the new app starts with none held.
#[derive(Default)]
pub struct Changes(AtomicBool);

/// `Changes` held, let go on drop: also when the change returns an error.
struct Changing<'a>(&'a AtomicBool);

impl Changes {
    fn start(&self) -> Result<Changing<'_>, CliError> {
        if self.0.swap(true, Ordering::AcqRel) {
            return Err(CliError::invalid(
                "another save or turn-off is still running; try again when it ends",
            ));
        }
        Ok(Changing(&self.0))
    }
}

impl Drop for Changing<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Install (or replace) the service for `roots`, passing on the options of
/// the running service as they are written. `seen` is the basis the window
/// showed: when a fresh read differs, or residue finds the service changed
/// from `seen.fingerprint` as it writes, nothing is installed, and the
/// result says what changed. Refused while another install or uninstall
/// runs.
pub fn install(
    ctx: &Context,
    dev: &DevPreview,
    roots: &[PathBuf],
    dry_run: bool,
    seen: &Basis,
) -> Result<Save, CliError> {
    let _one = ctx.changes.start()?;
    if roots.is_empty() {
        return Err(CliError::invalid("choose at least one folder to watch"));
    }
    let (current, installer) = read(ctx, dev)?;
    if current.basis != *seen {
        return Ok(Save::Changed {
            changes: current.basis.changes_since(seen),
            status: current,
        });
    }
    let plan = current.plan;
    if let Some(reason) = plan.blocked {
        return Err(CliError::invalid(reason));
    }
    let options = plan.keep;
    let mut args: Vec<OsString> = vec!["service".into(), "install".into()];
    args.extend(watch_args(roots, &options, dry_run));
    let command = shown(&installer.bin, &args);

    if dev.enabled {
        let preview = run_preview(ctx, &installer, roots, &options)?;
        let settings = watch_settings(&options, installer.version.as_deref());
        let program = Program {
            binary: installer.bin.to_string_lossy().into_owned(),
            binary_exists: true,
            version: installer.version,
            managed: installer.managed,
            roots: roots
                .iter()
                .map(|r| r.to_string_lossy().into_owned())
                .collect(),
            other_options: other_options(&options),
            options,
            dry_run,
            unknown: Vec::new(),
            settings,
        };
        dev.set(Some((program, preview.lines.clone())));
        return Ok(Save::Saved {
            change: Change {
                command,
                output: preview.lines,
                dev_dry_run: true,
                status: status(ctx, dev)?,
            },
        });
    }

    // residue compares the service with the fingerprint this dialog read and
    // writes it in one step, so a change after the read above is refused too
    // (#11). A save without one would replace whatever is there.
    let Some(fingerprint) = seen.fingerprint.as_deref() else {
        return Err(CliError::invalid(
            "residue printed no fingerprint of the service, so a save cannot check what it replaces. See why: residue service status",
        ));
    };
    args.insert(2, "--expect".into());
    args.insert(3, fingerprint.into());
    // Only a real save writes the app's own copy: the preview installs
    // nothing, so it needs none. The copy the service runs keeps a second
    // name until residue has written the agent file: when residue writes
    // nothing, it goes back, so a save that installs nothing leaves the
    // service on the residue it ran.
    let plist = agent_file(ctx.home);
    let before = std::fs::read(&plist).ok();
    let previous = if installer.managed {
        let previous = Previous::keep(&installer.bin)?;
        if let Err(e) = copy_sidecar(ctx, &installer.bin, installer.version.as_deref()) {
            return Err(match previous.restore() {
                Ok(()) => e,
                Err(undo) => CliError::failed(format!("{}; {undo}", e.message)),
            });
        }
        Some(previous)
    } else {
        None
    };
    let out = Run {
        bin: &installer.bin,
        args,
        cwd: ctx.home,
        path_env: &installer.path_env,
        deadline: Duration::from_secs(60),
    }
    .output();
    // Exit 0: residue wrote the service. Exit 4: it wrote nothing, as its
    // `--expect` promises. Any other failure: the agent file's bytes are the
    // only sign, and a change to them is taken as residue's. The same bytes
    // do not show residue did not write, since a save of the same service
    // writes them again, so no message below says that nothing changed.
    let landed = matches!(&out, Ok(o) if o.code == Some(0));
    let refused = matches!(&out, Ok(o) if o.code == Some(EXPECT_CHANGED));
    let wrote = landed || (!refused && std::fs::read(&plist).ok() != before);
    if let Some(previous) = previous {
        let undo = if wrote {
            previous.forget()
        } else {
            previous.restore()
        };
        match undo {
            Ok(()) => {}
            // The save stands; the next save removes the kept name.
            Err(undo) if wrote => eprintln!("residue app: {undo}"),
            Err(undo) => {
                let mut error = match out {
                    Ok(out) => out.error(
                        ErrorKind::Failed,
                        "residue could not install the service".into(),
                    ),
                    Err(e) => e,
                };
                error.message = format!("{}; {undo}", error.message);
                return Err(error);
            }
        }
    }
    let out = out?;
    if out.code == Some(EXPECT_CHANGED) {
        // Nothing was written: show the service as it is now, to save again.
        let status = status(ctx, dev)?;
        let mut changes = status.basis.changes_since(seen);
        if changes.is_empty() {
            changes.push("The service changed outside the app during the save.".into());
        }
        return Ok(Save::Changed { changes, status });
    }
    if out.code != Some(0) {
        return Err(out.error(
            ErrorKind::Failed,
            if wrote {
                "residue failed after the service's agent file changed".into()
            } else {
                "residue could not install the service".into()
            },
        ));
    }
    Ok(Save::Saved {
        change: Change {
            command: out.command.clone(),
            output: out.stdout.lines().map(str::to_string).collect(),
            dev_dry_run: false,
            status: status(ctx, dev)?,
        },
    })
}

/// Refused while another install or uninstall runs.
pub fn uninstall(ctx: &Context, dev: &DevPreview) -> Result<Change, CliError> {
    let _one = ctx.changes.start()?;
    let args: Vec<OsString> = vec!["service".into(), "uninstall".into()];
    if dev.enabled {
        dev.set(None);
        return Ok(Change {
            command: shown(ctx.sidecar, &args),
            output: vec!["development preview: nothing was uninstalled".into()],
            dev_dry_run: true,
            status: status(ctx, dev)?,
        });
    }
    let out = Run {
        bin: ctx.sidecar,
        args,
        cwd: ctx.home,
        path_env: ctx.path_env,
        deadline: Duration::from_secs(60),
    }
    .output()?;
    if out.code != Some(0) {
        return Err(out.error(
            ErrorKind::Failed,
            "residue could not uninstall the service".into(),
        ));
    }
    Ok(Change {
        command: out.command.clone(),
        output: out.stdout.lines().map(str::to_string).collect(),
        dev_dry_run: false,
        status: status(ctx, dev)?,
    })
}

/// What automatic cleanup would remove right now, after a save of `roots`:
/// the service's next pass with `--dry-run`, which removes nothing.
pub fn preview(ctx: &Context, dev: &DevPreview, roots: &[PathBuf]) -> Result<Preview, CliError> {
    let (current, installer) = read(ctx, dev)?;
    run_preview(ctx, &installer, roots, &current.plan.keep)
}

/// `residue watch --once --dry-run` with every option a save passes on, run
/// by the binary a save installs with: a pass as the saved service runs it,
/// which removes nothing. Like the service, it fetches first unless the
/// options say `--no-fetch` or the binary is older than 0.0.7. A fetch
/// changes only remote-tracking refs.
fn run_preview(
    ctx: &Context,
    installer: &Installer,
    roots: &[PathBuf],
    options: &[String],
) -> Result<Preview, CliError> {
    if roots.is_empty() {
        return Err(CliError::invalid("choose at least one folder to watch"));
    }
    let (bin, path_env) = installs_as(ctx, installer);
    let mut args: Vec<OsString> = vec!["watch".into(), "--once".into()];
    args.extend(watch_args(roots, options, true));
    let out = Run {
        bin,
        args,
        cwd: ctx.home,
        path_env,
        deadline: Duration::from_secs(15 * 60),
    }
    .output()?;
    if out.code != Some(0) {
        return Err(out.error(
            ErrorKind::Failed,
            "residue watch --once --dry-run failed".into(),
        ));
    }
    Ok(Preview {
        command: out.command.clone(),
        // watch logs to stderr, one line per decision.
        lines: out
            .stderr
            .lines()
            .chain(out.stdout.lines())
            .map(str::to_string)
            .collect(),
        fetch: watch_settings(options, installer.version.as_deref()).fetch,
    })
}

#[cfg(test)]
mod tests {
    //! A save with the app's own copy, against a stand-in residue: a shell
    //! script in a scratch folder that answers `--version`, `service status`
    //! and `service install` as the case needs, and never runs launchd.
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

    /// A scratch folder, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir()
                .join(format!("residue-app-test-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir.canonicalize().unwrap())
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn script(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A residue that prints `residue VERSION` for --version, says the
    /// service is off with a fingerprint, logs each `service install` it is
    /// given to `install.log` in the working folder (the scratch HOME), and
    /// then does `install` (shell).
    fn residue(version: &str, install: &str) -> String {
        format!(
            r#"case "$1 $2" in
  "--version ") echo "residue {version}" ;;
  "service status") echo "launchd agent {LABEL}: not installed"; echo "fingerprint: 0123456789abcdef0123456789abcdef"; exit 3 ;;
  "service install") echo "$@" >> install.log; {install} ;;
  *) exit 2 ;;
esac"#
        )
    }

    struct Machine {
        home: Scratch,
        sidecar: PathBuf,
        data_dir: PathBuf,
        path_env: OsString,
        changes: Changes,
        dev: DevPreview,
    }

    impl Machine {
        /// The bundled residue prints `bundled`; a save installs `install`.
        fn new(name: &str, bundled: &str, install: &str) -> Machine {
            let home = Scratch::new(name);
            let sidecar = home.0.join("app/residue-cli");
            script(&sidecar, &residue(bundled, install));
            let data_dir = home.0.join("data");
            // Only /usr/bin and /bin: the stand-in needs a shell, and no
            // residue there is one a save would pick.
            let path_env = OsString::from("/usr/bin:/bin");
            Machine {
                sidecar,
                data_dir,
                path_env,
                home,
                changes: Changes::default(),
                dev: DevPreview {
                    enabled: false,
                    state: Mutex::new(Pretend::Set(None)),
                },
            }
        }

        fn ctx<'a>(&'a self, bundled_version: &'a str) -> Context<'a> {
            Context {
                sidecar: &self.sidecar,
                bundled_version,
                data_dir: &self.data_dir,
                home: &self.home.0,
                path_env: &self.path_env,
                changes: &self.changes,
            }
        }

        fn copy(&self) -> PathBuf {
            managed_binary(&self.data_dir)
        }

        fn installs(&self) -> String {
            std::fs::read_to_string(self.home.0.join("install.log")).unwrap_or_default()
        }

        /// The folder's names other than the copy itself: no kept or
        /// temporary file may stay.
        fn leftovers(&self) -> Vec<String> {
            let Ok(entries) = std::fs::read_dir(self.copy().parent().unwrap()) else {
                return Vec::new();
            };
            entries
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|n| n != "residue")
                .collect()
        }

        /// Read, as the dialog does, then save with that read's basis.
        fn save(&self, bundled_version: &str) -> Result<Save, CliError> {
            let ctx = self.ctx(bundled_version);
            // Never let a residue installed on this machine run the save.
            let (_, installer) = read(&ctx, &self.dev).unwrap();
            assert!(installer.managed, "the save must use the app's own copy");
            let seen = status(&ctx, &self.dev).unwrap().basis;
            install(
                &ctx,
                &self.dev,
                std::slice::from_ref(&self.home.0),
                true,
                &seen,
            )
        }
    }

    // The bundled version in these cases is far above any release, so
    // `find_installer` never takes a residue installed on this machine.
    const BUNDLED: &str = "900.0.1";
    const OLDER: &str = "residue 900.0.0";

    #[test]
    fn a_refused_save_puts_back_the_copy_the_service_runs() {
        let m = Machine::new("refused", BUNDLED, "echo refused >&2; exit 1");
        script(&m.copy(), &residue("900.0.0", "exit 1"));
        let before = std::fs::metadata(m.copy()).unwrap().ino();

        let err = m.save(BUNDLED).unwrap_err();

        assert_eq!(err.message, "residue could not install the service");
        assert_eq!(m.installs().lines().count(), 1, "residue ran once");
        assert_eq!(std::fs::metadata(m.copy()).unwrap().ino(), before);
        assert_eq!(
            version_of(&m.copy(), &m.ctx(BUNDLED)).as_deref(),
            Some(OLDER)
        );
        assert_eq!(m.leftovers(), Vec::<String>::new());
    }

    #[test]
    fn a_refused_save_removes_a_copy_that_was_not_there() {
        let m = Machine::new("refused-new", BUNDLED, "exit 1");

        m.save(BUNDLED).unwrap_err();

        assert!(!m.copy().exists());
        assert_eq!(m.leftovers(), Vec::<String>::new());
    }

    #[test]
    fn a_save_that_wrote_the_agent_file_keeps_the_new_copy() {
        let m = Machine::new(
            "wrote",
            BUNDLED,
            &format!(
                "mkdir -p Library/LaunchAgents && echo new > Library/LaunchAgents/{LABEL}.plist; exit 1"
            ),
        );
        script(&m.copy(), &residue("900.0.0", "exit 1"));

        let err = m.save(BUNDLED).unwrap_err();

        assert_eq!(
            err.message,
            "residue failed after the service's agent file changed"
        );
        assert_eq!(
            version_of(&m.copy(), &m.ctx(BUNDLED)).as_deref(),
            Some(format!("residue {BUNDLED}").as_str())
        );
        assert_eq!(m.leftovers(), Vec::<String>::new());
    }

    #[test]
    fn a_save_that_lands_keeps_the_new_copy() {
        let m = Machine::new("landed", BUNDLED, "exit 0");
        script(&m.copy(), &residue("900.0.0", "exit 1"));

        // The read after the save says off again; the save itself landed.
        assert!(matches!(m.save(BUNDLED), Ok(Save::Saved { .. })));
        assert_eq!(
            version_of(&m.copy(), &m.ctx(BUNDLED)).as_deref(),
            Some(format!("residue {BUNDLED}").as_str())
        );
        assert_eq!(m.leftovers(), Vec::<String>::new());
    }

    #[test]
    fn a_save_whose_residue_finds_the_service_changed_puts_back_the_copy() {
        let m = Machine::new("expect", BUNDLED, &format!("exit {EXPECT_CHANGED}"));
        script(&m.copy(), &residue("900.0.0", "exit 1"));

        assert!(matches!(m.save(BUNDLED), Ok(Save::Changed { .. })));
        assert_eq!(
            version_of(&m.copy(), &m.ctx(BUNDLED)).as_deref(),
            Some(OLDER)
        );
        assert_eq!(m.leftovers(), Vec::<String>::new());
    }

    /// The app replaced on disk after its version was read: the bundled
    /// binary is now another release than the save showed.
    #[test]
    fn a_save_copies_only_the_residue_it_showed() {
        let m = Machine::new("replaced", "900.0.2", "exit 0");
        script(&m.copy(), &residue("900.0.0", "exit 1"));

        let err = m.save(BUNDLED).unwrap_err();

        assert!(
            err.message
                .contains("is now residue 900.0.2, and this save showed residue 900.0.1"),
            "{}",
            err.message
        );
        assert_eq!(m.installs(), "", "residue never ran");
        assert_eq!(
            version_of(&m.copy(), &m.ctx(BUNDLED)).as_deref(),
            Some(OLDER)
        );
        assert_eq!(m.leftovers(), Vec::<String>::new());
    }

    /// Exit 4 is residue's word that it wrote nothing, whatever the agent
    /// file's bytes did meanwhile, such as a terminal's install.
    #[test]
    fn a_save_refused_with_exit_4_puts_back_the_copy_even_when_the_file_changed() {
        let m = Machine::new(
            "expect-changed-file",
            BUNDLED,
            &format!(
                "f=Library/LaunchAgents/{LABEL}.plist; mkdir -p Library/LaunchAgents && plutil -create xml1 $f && plutil -insert ProgramArguments -json '[\"/elsewhere/residue\",\"watch\",\"/tmp\"]' $f; exit {EXPECT_CHANGED}"
            ),
        );
        script(&m.copy(), &residue("900.0.0", "exit 1"));

        assert!(matches!(m.save(BUNDLED), Ok(Save::Changed { .. })));
        assert_eq!(
            version_of(&m.copy(), &m.ctx(BUNDLED)).as_deref(),
            Some(OLDER)
        );
        assert_eq!(m.leftovers(), Vec::<String>::new());
    }

    /// A disk with no hard links, such as exFAT, answers `hard_link` with
    /// "Operation not supported": the copy is kept as a copy instead.
    #[test]
    fn put_back_where_the_disk_has_no_hard_links() {
        let m = Machine::new("no-links", BUNDLED, "exit 0");
        script(&m.copy(), &residue("900.0.0", "exit 1"));
        let unsupported = |_: &Path, _: &Path| -> std::io::Result<()> {
            Err(std::io::Error::from_raw_os_error(libc::ENOTSUP))
        };

        let previous = Previous::keep_with(&m.copy(), unsupported).unwrap();
        assert!(previous.kept.is_some());
        let want = format!("residue {BUNDLED}");
        copy_sidecar(&m.ctx(BUNDLED), &m.copy(), Some(&want)).unwrap();
        previous.restore().unwrap();

        assert_eq!(
            version_of(&m.copy(), &m.ctx(BUNDLED)).as_deref(),
            Some(OLDER)
        );
        assert_eq!(m.leftovers(), Vec::<String>::new());
    }

    #[test]
    fn a_copy_removes_the_temporary_files_an_earlier_launch_left() {
        let m = Machine::new("leftovers", BUNDLED, "exit 0");
        let dir = m.copy().parent().unwrap().to_path_buf();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".residue.4242.tmp"), "half a copy").unwrap();
        let target = m.home.0.join("elsewhere");
        std::fs::write(&target, "keep me").unwrap();
        symlink(&target, dir.join(".residue.4243.tmp")).unwrap();

        let want = format!("residue {BUNDLED}");
        copy_sidecar(&m.ctx(BUNDLED), &m.copy(), Some(&want)).unwrap();

        assert_eq!(m.leftovers(), Vec::<String>::new());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep me");
    }

    #[test]
    fn the_copy_never_writes_through_a_link_at_its_temporary_name() {
        let m = Machine::new("link", BUNDLED, "exit 0");
        let target = m.home.0.join("elsewhere");
        std::fs::write(&target, "keep me").unwrap();
        let dir = m.copy().parent().unwrap().to_path_buf();
        std::fs::create_dir_all(&dir).unwrap();
        symlink(
            &target,
            dir.join(format!(".residue.{}.tmp", std::process::id())),
        )
        .unwrap();

        let want = format!("residue {BUNDLED}");
        copy_sidecar(&m.ctx(BUNDLED), &m.copy(), Some(&want)).unwrap();

        assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep me");
        assert_eq!(
            version_of(&m.copy(), &m.ctx(BUNDLED)).as_deref(),
            Some(want.as_str())
        );
        assert!(!std::fs::symlink_metadata(m.copy()).unwrap().is_symlink());
        assert_eq!(m.leftovers(), Vec::<String>::new());
    }

    #[test]
    fn put_back_after_a_copy_that_never_replaced_the_file() {
        let m = Machine::new("same-file", BUNDLED, "exit 0");
        script(&m.copy(), &residue("900.0.0", "exit 1"));
        let before = std::fs::metadata(m.copy()).unwrap().ino();

        // Both names link one file: the rename back changes nothing.
        Previous::keep(&m.copy()).unwrap().restore().unwrap();

        assert_eq!(std::fs::metadata(m.copy()).unwrap().ino(), before);
        assert_eq!(m.leftovers(), Vec::<String>::new());
    }
}
