//! One confirmed removal at a time, also across a quit and a relaunch (#12).
//!
//! `remove` (`lib.rs`) holds `Removals` for a confirmed `residue remove
//! --trash --yes` from before residue starts until it returns, and refuses
//! another, running nothing, until then. The app does not stop residue when
//! it quits, so while residue runs a marker in the app's data directory names
//! its process, and the next launch counts a removal whose process still runs
//! as in flight. The window asks (`running`) when it starts and when the
//! removal dialog closes, and keeps Move to the trash off until `wait`
//! returns.
//!
//! A process is its PID, the start time the kernel gives it, to the
//! microsecond, and its name. A PID goes to another process only after its
//! process has exited, and that one starts later, so a marker whose PID
//! another process took names no process that runs: it never keeps the
//! window waiting. The executable's path is not part of it: replacing the
//! app while residue runs moves or deletes that path, and the run would go
//! unseen.
//!
//! Not covered: the app quitting in the instant between residue's start and
//! the marker's rename, or a marker that cannot be written once residue runs
//! (said on stderr), after which a relaunch does not see that run; two copies
//! of the app open at once, each with its own `Removals`; and a residue an
//! earlier launch left running that hangs, which this launch waits for with
//! no deadline.

use crate::cli::CliError;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

const MARKER: &str = "removal-running.json";
const MARKER_PART: &str = "removal-running.json.part";
const MARKER_SCHEMA: &str = "residue-app.removal/v1";

/// How often `wait` looks again at a `Left` removal's process. The app is not
/// its parent, or no longer waits for it, so nothing tells it when that
/// process exits.
const LEFT_POLL: Duration = Duration::from_secs(1);

/// A residue process, as the marker names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Process {
    pid: u32,
    /// When it started, in microseconds since the epoch.
    started_us: u64,
    /// Its name as the kernel keeps it, such as `residue-cli`.
    name: String,
}

#[derive(Serialize, Deserialize)]
struct Marker {
    schema: String,
    #[serde(flatten)]
    process: Process,
}

/// Where the one confirmed removal stands. `InFlight::next` decides every
/// move, and `Removals::apply` makes it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum InFlight {
    /// None runs.
    None,
    /// This app's, from the confirm until residue runs.
    Starting,
    /// This app's: residue runs, and `remove` waits for it.
    Running(Process),
    /// A residue that no `remove` of this app waits for runs one: one an
    /// earlier launch started, read from the marker when the app opened, or
    /// this app's own once `remove` returned while residue still ran (the wait
    /// for it failed). It ends when that process exits.
    Left(Process),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Event {
    /// A confirmed removal asks to start.
    Start,
    /// Its residue runs, as this process.
    Spawned(Process),
    /// `remove` returned, and its residue still runs or not.
    Returned { runs: bool },
    /// A `Left` removal's process no longer runs.
    Exited,
}

/// A move `next` does not make. `Start` while a removal is in flight is the
/// one the code asks for on purpose: it is how a second removal is refused.
#[derive(Debug)]
struct Illegal {
    from: InFlight,
    event: Event,
}

impl InFlight {
    /// Decides every move. Each match lists every state and every event, with
    /// no wildcard arm, so a new one does not compile until its moves are
    /// decided.
    fn next(&self, event: &Event) -> Result<InFlight, Illegal> {
        let to = match self {
            InFlight::None => match event {
                Event::Start => Some(InFlight::Starting),
                Event::Spawned(_) | Event::Returned { .. } | Event::Exited => None,
            },
            InFlight::Starting => match event {
                Event::Spawned(p) => Some(InFlight::Running(p.clone())),
                // residue did not start, or the system gave no start time
                // for it, so no marker names it.
                Event::Returned { .. } => Some(InFlight::None),
                Event::Start | Event::Exited => None,
            },
            InFlight::Running(p) => match event {
                Event::Returned { runs: false } => Some(InFlight::None),
                Event::Returned { runs: true } => Some(InFlight::Left(p.clone())),
                Event::Start | Event::Spawned(_) | Event::Exited => None,
            },
            InFlight::Left(_) => match event {
                Event::Exited => Some(InFlight::None),
                Event::Start | Event::Spawned(_) | Event::Returned { .. } => None,
            },
        };
        to.ok_or_else(|| Illegal {
            from: self.clone(),
            event: event.clone(),
        })
    }
}

/// The confirmed removal in flight, held in the app's `State` (`lib.rs`).
pub struct Removals {
    marker: PathBuf,
    /// The marker's next contents, renamed to `marker` once written whole.
    part: PathBuf,
    now: Mutex<InFlight>,
    /// Told on every move, for `wait`.
    moved: Condvar,
}

/// A confirmed removal, from `Removals::start` until `remove` drops it on
/// its return: also when it returns an error.
pub struct Removing<'a> {
    removals: &'a Removals,
    /// `Removals::part`, opened before residue starts.
    part: Option<File>,
}

impl Removals {
    /// The removals of an app that opens now. A marker that names a process
    /// that still runs is a removal an earlier launch left in flight. Any
    /// other marker is removed: one whose process exited or whose PID another
    /// process took, and one the app cannot read, since it writes a marker
    /// whole (`Removing::record`).
    pub fn open(data_dir: &Path) -> Removals {
        let marker = data_dir.join(MARKER);
        let now = match read_marker(&marker) {
            Ok(None) => InFlight::None,
            Ok(Some(p)) if p.runs() => {
                eprintln!("residue app: removal {p:?} runs on from an earlier launch");
                InFlight::Left(p)
            }
            Ok(Some(p)) => {
                eprintln!("residue app: removal {p:?} of an earlier launch has ended");
                remove_file(&marker);
                InFlight::None
            }
            Err(e) => {
                eprintln!("residue app: {} names no removal: {e}", marker.display());
                remove_file(&marker);
                InFlight::None
            }
        };
        Removals {
            marker,
            part: data_dir.join(MARKER_PART),
            now: Mutex::new(now),
            moved: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, InFlight> {
        // A move is one assignment, so a lock that a panic poisoned still
        // holds a whole state.
        self.now.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Make the move `event` asks for, said on stderr, and return true; or
    /// say why it is illegal, change nothing, and return false. A move to
    /// `None` removes the marker.
    fn apply(&self, now: &mut InFlight, event: &Event) -> bool {
        match now.next(event) {
            Ok(to) => {
                eprintln!("residue app: removal {now:?} -> {to:?} on {event:?}");
                if to == InFlight::None {
                    remove_file(&self.marker);
                    remove_file(&self.part);
                }
                *now = to;
                self.moved.notify_all();
                true
            }
            Err(Illegal { from, event }) => {
                eprintln!("residue app: a {from:?} removal cannot take {event:?}");
                false
            }
        }
    }

    /// A `Left` removal whose process no longer runs ends.
    fn refresh(&self, now: &mut InFlight) {
        if matches!(now, InFlight::Left(p) if !p.runs()) {
            self.apply(now, &Event::Exited);
        }
    }

    /// Hold the removals for one confirmed removal, or refuse, with kind
    /// `invalid` and running nothing, while another is in flight. Call
    /// `Removing::record` with residue's PID once it runs.
    pub fn start(&self) -> Result<Removing<'_>, CliError> {
        let mut now = self.lock();
        self.refresh(&mut now);
        if !self.apply(&mut now, &Event::Start) {
            return Err(CliError::invalid(
                "another removal is still running; try again when it ends",
            ));
        }
        drop(now);
        // Every return from here drops `removing`, which ends the removal.
        let mut removing = Removing {
            removals: self,
            part: None,
        };
        // Opened before residue runs, so that a data directory the app cannot
        // write refuses the removal before it starts: a run without a marker
        // would go unseen after a relaunch.
        let part = self
            .marker
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| File::create(&self.part))
            .map_err(|e| {
                CliError::invalid(format!(
                    "cannot record the removal in {}: {e}",
                    self.part.display()
                ))
            })?;
        removing.part = Some(part);
        Ok(removing)
    }

    /// Whether a confirmed removal is in flight.
    pub fn running(&self) -> bool {
        let mut now = self.lock();
        self.refresh(&mut now);
        *now != InFlight::None
    }

    /// Returns once no confirmed removal is in flight: at once when none is.
    pub fn wait(&self) {
        let mut now = self.lock();
        loop {
            self.refresh(&mut now);
            let left = match &*now {
                InFlight::None => return,
                InFlight::Starting | InFlight::Running(_) => false,
                InFlight::Left(_) => true,
            };
            if left {
                drop(now);
                thread::sleep(LEFT_POLL);
                now = self.lock();
            } else {
                now = self.moved.wait(now).unwrap_or_else(PoisonError::into_inner);
            }
        }
    }
}

impl Removing<'_> {
    /// Write the marker naming residue's process, `pid`. residue runs
    /// already, so a marker that cannot be written leaves it running: the app
    /// still refuses another removal until this one returns, and only a
    /// relaunch while it runs would not see it. Said on stderr.
    pub fn record(&mut self, pid: u32) {
        let Some(process) = process(pid, true) else {
            eprintln!(
                "residue app: the system gives no start time for process {pid}, so a relaunch while it runs would not see this removal"
            );
            return;
        };
        let removals = self.removals;
        if !removals.apply(&mut removals.lock(), &Event::Spawned(process.clone())) {
            return;
        }
        let Some(mut part) = self.part.take() else {
            return;
        };
        let written = serde_json::to_vec(&Marker {
            schema: MARKER_SCHEMA.into(),
            process,
        })
        .map_err(io::Error::other)
        .and_then(|json| part.write_all(&json))
        .and_then(|()| fs::rename(&removals.part, &removals.marker));
        if let Err(e) = written {
            eprintln!(
                "residue app: cannot record the removal in {}: {e}, so a relaunch while it runs would not see it",
                removals.marker.display()
            );
            remove_file(&removals.part);
        }
    }
}

impl Drop for Removing<'_> {
    fn drop(&mut self) {
        // `remove` returns once `cli::Run::output_with` has: after residue
        // exited or was stopped at its deadline, when it could not start, or
        // when the wait for it failed, which can leave it running.
        let mut now = self.removals.lock();
        let runs = matches!(&*now, InFlight::Running(p) if p.runs());
        self.removals.apply(&mut now, &Event::Returned { runs });
    }
}

impl Process {
    /// Whether it still runs: a process with its PID, start time and name
    /// that has not exited.
    fn runs(&self) -> bool {
        process(self.pid, false).is_some_and(|now| now == *self)
    }
}

fn read_marker(path: &Path) -> io::Result<Option<Process>> {
    let text = match fs::read(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let marker: Marker = serde_json::from_slice(&text).map_err(io::Error::other)?;
    if marker.schema != MARKER_SCHEMA {
        return Err(io::Error::other(format!(
            "its schema is {}, not {MARKER_SCHEMA}",
            marker.schema
        )));
    }
    Ok(Some(marker.process))
}

/// Remove a marker, or its next contents, that names no removal in flight.
/// One that cannot be removed is said on stderr: the next launch finds that
/// its process has exited and removes it, and the next removal replaces it.
fn remove_file(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => eprintln!("residue app: cannot remove {}: {e}", path.display()),
    }
}

/// The process `pid` as the kernel reports it, or `None` when no process has
/// that PID. With `zombie`, also one that has exited but has not been waited
/// for: residue right after it starts may have exited already, and this app
/// has not waited for it yet, so the PID is still its own. Without it, a
/// process that has exited is `None`.
#[cfg(target_os = "macos")]
fn process(pid: u32, zombie: bool) -> Option<Process> {
    let id = libc::c_int::try_from(pid).ok()?;
    let size = libc::c_int::try_from(std::mem::size_of::<libc::proc_bsdinfo>()).ok()?;
    // SAFETY: proc_bsdinfo holds only integers and arrays of them, for which
    // all zeros is a valid value.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    // SAFETY: `info` is a proc_bsdinfo of `size` bytes, which is what
    // PROC_PIDTBSDINFO writes, and the call writes nothing else. An `arg` of
    // 1 also finds a zombie.
    let written = unsafe {
        libc::proc_pidinfo(
            id,
            libc::PROC_PIDTBSDINFO,
            u64::from(zombie),
            (&raw mut info).cast(),
            size,
        )
    };
    if written != size || (!zombie && info.pbi_status == libc::SZOMB) {
        return None;
    }
    let name: Vec<u8> = info
        .pbi_comm
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c.to_ne_bytes()[0])
        .collect();
    Some(Process {
        pid,
        started_us: info
            .pbi_start_tvsec
            .saturating_mul(1_000_000)
            .saturating_add(info.pbi_start_tvusec),
        name: String::from_utf8_lossy(&name).into_owned(),
    })
}

/// The app runs on macOS only. Elsewhere it knows no process, so it writes no
/// marker and finds none that runs.
#[cfg(not(target_os = "macos"))]
fn process(_pid: u32, _zombie: bool) -> Option<Process> {
    None
}
