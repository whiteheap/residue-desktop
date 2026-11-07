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

use crate::cli::{CliError, ErrorKind, Run, shown};
use serde::Serialize;
use serde_json::Value;
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// The launchd label residue's service uses.
pub const LABEL: &str = "com.whiteheap.residue.watch";

/// `residue watch` options that take a value. The app keeps their values when
/// it installs the service again with other folders.
const VALUE_OPTIONS: &[&str] = &["--grace", "--idle", "--min-free"];

/// Everything a service operation needs to know about this machine.
pub struct Context<'a> {
    pub sidecar: &'a Path,
    pub bundled_version: &'a str,
    pub data_dir: &'a Path,
    pub home: &'a Path,
    pub path_env: &'a OsStr,
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
    pub roots: Vec<String>,
    /// Value options as flat pairs: `["--grace", "1h"]`.
    pub options: Vec<String>,
    pub dry_run: bool,
    /// Arguments the app does not know. They are shown, and not carried over
    /// when the app installs the service again.
    pub unknown: Vec<String>,
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
    /// The development preview mode is on: nothing was installed.
    pub dev_dry_run: bool,
}

/// The result of turning the service on or off.
#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub command: String,
    pub output: Vec<String>,
    pub dev_dry_run: bool,
    pub status: Status,
}

/// The preview mode for development builds. With
/// `RESIDUE_DESKTOP_DEV_SERVICE=preview`, a debug build never runs
/// `residue service install` or `uninstall`: launchd acts on the user's real
/// session and the label is shared with any residue service already running.
/// The switch then shows the command it would run and the output of
/// `residue watch --once --dry-run`, which removes nothing, and remembers the
/// pretend state until the app quits. Release builds ignore the variable.
pub struct DevPreview {
    pub enabled: bool,
    state: Mutex<Option<(Program, Vec<String>)>>,
}

impl DevPreview {
    pub fn from_env() -> Self {
        let enabled = cfg!(debug_assertions)
            && std::env::var("RESIDUE_DESKTOP_DEV_SERVICE").as_deref() == Ok("preview");
        DevPreview {
            enabled,
            state: Mutex::new(None),
        }
    }

    fn get(&self) -> Option<(Program, Vec<String>)> {
        match self.state.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    fn set(&self, value: Option<(Program, Vec<String>)>) {
        match self.state.lock() {
            Ok(mut guard) => *guard = value,
            Err(poisoned) => *poisoned.into_inner() = value,
        }
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
    if dev.enabled {
        return Ok(match dev.get() {
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
                dev_dry_run: true,
            },
        });
    }

    // `residue service status` only reads: launchctl print and the log.
    let out = Run {
        bin: ctx.sidecar,
        args: vec!["service".into(), "status".into()],
        cwd: ctx.home,
        path_env: ctx.path_env,
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
    let (log_lines, log_error) = match tail(&log_path, 40) {
        Ok(lines) => (lines, None),
        Err(e) => (Vec::new(), Some(e)),
    };
    Ok(Status {
        installed,
        state: if installed { state } else { None },
        pid: if installed { pid } else { None },
        summary,
        agent_file: plist.exists().then(|| plist.to_string_lossy().into_owned()),
        log_lines,
        log_error,
        log_path: Some(log_path.to_string_lossy().into_owned()),
        program,
        dev_dry_run: false,
    })
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
    let mut program = Program {
        binary: binary.clone(),
        binary_exists,
        version: if binary_exists {
            version_of(&binary_path, ctx)
        } else {
            None
        },
        managed: binary_path == managed_binary(ctx.data_dir),
        roots: Vec::new(),
        options: Vec::new(),
        dry_run: false,
        unknown: Vec::new(),
    };
    let mut rest = rest.iter();
    if rest.next().map(String::as_str) != Some("watch") {
        program.unknown = args[1..].to_vec();
        return Ok(program);
    }
    while let Some(arg) = rest.next() {
        if VALUE_OPTIONS.contains(&arg.as_str()) {
            match rest.next() {
                Some(value) => {
                    program.options.push(arg.clone());
                    program.options.push(value.clone());
                }
                None => program.unknown.push(arg.clone()),
            }
        } else if arg == "--dry-run" {
            program.dry_run = true;
        } else if arg.starts_with('-') {
            program.unknown.push(arg.clone());
        } else {
            program.roots.push(arg.clone());
        }
    }
    Ok(program)
}

/// `residue --version` of a binary: `residue 0.0.6`.
fn version_of(bin: &Path, ctx: &Context) -> Option<String> {
    let out = Run {
        bin,
        args: vec!["--version".into()],
        cwd: ctx.home,
        path_env: ctx.path_env,
        deadline: Duration::from_secs(5),
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

/// The binary the service will run, and the PATH to install it with.
pub struct Choice {
    pub bin: PathBuf,
    pub managed: bool,
    pub path_env: OsString,
}

pub fn choose(ctx: &Context) -> Result<Choice, CliError> {
    let bundled = parse_version(&format!("residue {}", ctx.bundled_version));
    let managed = managed_binary(ctx.data_dir);
    let skip: Vec<PathBuf> = [ctx.sidecar, managed.as_path()]
        .iter()
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
        let found = version_of(&candidate, ctx).and_then(|v| parse_version(&v));
        if let (Some(found), Some(bundled)) = (found, bundled)
            && found >= bundled
        {
            let mut path: Vec<PathBuf> = vec![dir.clone()];
            path.extend(std::env::split_paths(ctx.path_env).filter(|d| d != &dir));
            let path_env = std::env::join_paths(path)
                .map_err(|e| CliError::failed(format!("cannot build PATH: {e}")))?;
            return Ok(Choice {
                bin: candidate,
                managed: false,
                path_env,
            });
        }
    }
    copy_sidecar(ctx.sidecar, &managed)?;
    Ok(Choice {
        bin: managed,
        managed: true,
        path_env: ctx.path_env.to_owned(),
    })
}

/// Copy the bundled binary to `dest`: written next to it, then renamed over
/// it, so a running service keeps the file it started from.
fn copy_sidecar(sidecar: &Path, dest: &Path) -> Result<(), CliError> {
    use std::os::unix::fs::OpenOptionsExt;
    let dir = dest
        .parent()
        .ok_or_else(|| CliError::failed(format!("{} has no parent", dest.display())))?;
    std::fs::create_dir_all(dir)
        .map_err(|e| CliError::failed(format!("cannot create {}: {e}", dir.display())))?;
    let tmp = dir.join(format!(".residue.{}.tmp", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut from = std::fs::File::open(sidecar)?;
        let mut to = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o755)
            .open(&tmp)?;
        std::io::copy(&mut from, &mut to)?;
        to.sync_all()?;
        std::fs::rename(&tmp, dest)
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

/// The `residue watch` arguments: kept options, `--dry-run`, then the roots.
fn watch_args(roots: &[PathBuf], options: &[String], dry_run: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = options.iter().map(OsString::from).collect();
    if dry_run {
        args.push("--dry-run".into());
    }
    args.extend(roots.iter().map(|r| r.as_os_str().to_owned()));
    args
}

/// Install (or replace) the service for `roots`. `keep` is the program of the
/// service being replaced, whose watch options are kept.
pub fn install(
    ctx: &Context,
    dev: &DevPreview,
    roots: &[PathBuf],
    dry_run: bool,
    keep: Option<&Program>,
) -> Result<Change, CliError> {
    if roots.is_empty() {
        return Err(CliError::invalid("choose at least one folder to watch"));
    }
    let options: Vec<String> = keep
        .filter(|p| p.unknown.is_empty())
        .map(|p| p.options.clone())
        .unwrap_or_default();
    let choice = choose(ctx)?;
    let mut args: Vec<OsString> = vec!["service".into(), "install".into()];
    args.extend(watch_args(roots, &options, dry_run));
    let command = shown(&choice.bin, &args);

    if dev.enabled {
        let preview = watch_preview_with(ctx, &choice.bin, &choice.path_env, roots, &options)?;
        let program = Program {
            binary: choice.bin.to_string_lossy().into_owned(),
            binary_exists: true,
            version: version_of(&choice.bin, ctx),
            managed: choice.managed,
            roots: roots
                .iter()
                .map(|r| r.to_string_lossy().into_owned())
                .collect(),
            options,
            dry_run,
            unknown: Vec::new(),
        };
        dev.set(Some((program, preview.clone())));
        return Ok(Change {
            command,
            output: preview,
            dev_dry_run: true,
            status: status(ctx, dev)?,
        });
    }

    let out = Run {
        bin: &choice.bin,
        args,
        cwd: ctx.home,
        path_env: &choice.path_env,
        deadline: Duration::from_secs(60),
    }
    .output()?;
    if out.code != Some(0) {
        return Err(out.error(
            ErrorKind::Failed,
            "residue could not install the service".into(),
        ));
    }
    Ok(Change {
        command: out.command.clone(),
        output: out.stdout.lines().map(str::to_string).collect(),
        dev_dry_run: false,
        status: status(ctx, dev)?,
    })
}

pub fn uninstall(ctx: &Context, dev: &DevPreview) -> Result<Change, CliError> {
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

/// What automatic cleanup would remove right now:
/// `residue watch --once --dry-run ROOT...`, which removes nothing.
pub fn preview(
    ctx: &Context,
    roots: &[PathBuf],
    options: &[String],
) -> Result<Vec<String>, CliError> {
    watch_preview_with(ctx, ctx.sidecar, ctx.path_env, roots, options)
}

fn watch_preview_with(
    ctx: &Context,
    bin: &Path,
    path_env: &OsStr,
    roots: &[PathBuf],
    options: &[String],
) -> Result<Vec<String>, CliError> {
    if roots.is_empty() {
        return Err(CliError::invalid("choose at least one folder to watch"));
    }
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
    // watch logs to stderr, one line per decision.
    Ok(out
        .stderr
        .lines()
        .chain(out.stdout.lines())
        .map(str::to_string)
        .collect())
}
