//! Running residue: an explicit argv, a known PATH, a deadline, and the output
//! read back as one of residue's JSON documents. The documents are the
//! contract between the CLI and this app; nothing here decides what is safe
//! to remove. Every child the app starts, residue or not, is started by
//! [`spawn`].

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

/// How much of stdout or stderr is kept. A scan of a few hundred worktrees is
/// a few megabytes; past this the output is cut and fails to parse, which is
/// reported, never guessed around.
const OUTPUT_CAP: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// The binary is not there, or the system refused to start it.
    Missing,
    /// It ran and exited without the document it owes.
    Failed,
    /// It did not finish before the deadline, so the app stopped it.
    Timeout,
    /// It printed something that is not the expected document.
    BadOutput,
    /// The app refused the request before it ran anything that changes a
    /// worktree: it ran nothing, or only a plan, which moves nothing.
    Invalid,
}

/// An error the window shows as it is: what failed, the command, and what
/// residue printed on stderr.
#[derive(Debug, Clone, Serialize)]
pub struct CliError {
    pub kind: ErrorKind,
    pub message: String,
    pub command: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
}

impl CliError {
    pub fn invalid(message: impl Into<String>) -> Self {
        CliError {
            kind: ErrorKind::Invalid,
            message: message.into(),
            command: None,
            stderr: None,
            exit_code: None,
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        CliError {
            kind: ErrorKind::Failed,
            message: message.into(),
            command: None,
            stderr: None,
            exit_code: None,
        }
    }

    pub fn bad_output(message: impl Into<String>) -> Self {
        CliError {
            kind: ErrorKind::BadOutput,
            message: message.into(),
            command: None,
            stderr: None,
            exit_code: None,
        }
    }
}

/// What a finished run printed.
pub struct Output {
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
}

/// One residue run.
pub struct Run<'a> {
    pub bin: &'a Path,
    pub args: Vec<OsString>,
    pub cwd: &'a Path,
    pub path_env: &'a OsStr,
    pub deadline: Duration,
}

impl Run<'_> {
    pub fn output(&self) -> Result<Output, CliError> {
        self.output_with(|_| {})
    }

    /// `output`, calling `started` with the child's PID once it runs, before
    /// the wait for it begins. The PID stays the child's until this returns:
    /// the child is waited for only here.
    pub fn output_with(&self, started: impl FnOnce(u32)) -> Result<Output, CliError> {
        let command = shown(self.bin, &self.args);
        let mut cmd = Command::new(self.bin);
        cmd.args(&self.args)
            .current_dir(self.cwd)
            .env("PATH", self.path_env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = spawn(&mut cmd).map_err(|e| CliError {
            kind: ErrorKind::Missing,
            message: format!("cannot start {}: {e}", self.bin.display()),
            command: Some(command.clone()),
            stderr: None,
            exit_code: None,
        })?;
        started(child.id());

        let stdout = child.stdout.take().map(reader);
        let stderr = child.stderr.take().map(reader);
        let started = Instant::now();
        // Look often at first, so a quick command returns at once, then every
        // 25 ms, as before: a longer pause would delay the answer to every slow
        // command to save almost no CPU.
        let mut pause = Duration::from_millis(1);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() >= self.deadline => {
                    // Stop it: a hung child would hold the window's request
                    // forever. Both calls fail only if it already exited.
                    let killed = child.kill();
                    let waited = child.wait();
                    return Err(CliError {
                        kind: ErrorKind::Timeout,
                        message: format!(
                            "residue did not finish within {} s and was stopped{}",
                            self.deadline.as_secs(),
                            match (killed, waited) {
                                (Ok(()), Ok(_)) => String::new(),
                                (k, w) => format!(" ({k:?}, {w:?})"),
                            }
                        ),
                        command: Some(command),
                        stderr: stderr.map(join_reader).filter(|s| !s.is_empty()),
                        exit_code: None,
                    });
                }
                Ok(None) => {
                    thread::sleep(pause);
                    pause = (pause * 2).min(Duration::from_millis(25));
                }
                Err(e) => {
                    return Err(CliError {
                        kind: ErrorKind::Failed,
                        message: format!("cannot wait for residue: {e}"),
                        command: Some(command),
                        stderr: None,
                        exit_code: None,
                    });
                }
            }
        };
        Ok(Output {
            command,
            stdout: stdout.map(join_reader).unwrap_or_default(),
            stderr: stderr.map(join_reader).unwrap_or_default(),
            code: status.code(),
        })
    }
}

/// Start `cmd`, one child at a time. On macOS, which has no `pipe2`, the
/// standard library makes a child's pipes with `pipe()` and marks them
/// close-on-exec in a second step. The window's requests run on several
/// threads at once (`lib.rs`, `blocking`), and a child another thread starts
/// between the two steps inherits them: then the first child's output ends
/// only when that other child, and anything it left running, exits
/// (residue-cli#116). Under this lock no child starts while another start is
/// making its pipes.
///
/// The lock covers only the children started here, so this is the one call
/// of `Command::spawn` in the app, and `clippy.toml` refuses `spawn`,
/// `output` and `status` anywhere else. A start holds the lock for the
/// `posix_spawn` or `fork` and `exec` alone, never while the child runs.
#[expect(
    clippy::disallowed_methods,
    reason = "the one place a child starts, under the lock"
)]
pub fn spawn(cmd: &mut Command) -> io::Result<Child> {
    static STARTING: Mutex<()> = Mutex::new(());
    // The lock guards no data, so a start that panicked holding it left
    // nothing half done.
    let _starting = STARTING.lock().unwrap_or_else(PoisonError::into_inner);
    cmd.spawn()
}

type Reader = thread::JoinHandle<std::io::Result<String>>;

fn reader(stream: impl Read + Send + 'static) -> Reader {
    thread::spawn(move || {
        let mut text = Vec::new();
        stream.take(OUTPUT_CAP).read_to_end(&mut text)?;
        // Valid UTF-8, as residue prints, becomes the String without a copy.
        Ok(String::from_utf8(text)
            .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
    })
}

/// The text a reader thread collected. A thread that failed to read or
/// panicked says so in the text, so the failure reaches the window.
fn join_reader(handle: Reader) -> String {
    match handle.join() {
        Ok(Ok(text)) => text,
        Ok(Err(e)) => format!("(could not read the output: {e})"),
        Err(_) => "(the thread reading the output stopped)".to_string(),
    }
}

/// One of residue's JSON documents, as the text residue printed. The window
/// is what reads it, so the app checks that it is JSON with the expected
/// `schema` and passes the text on, without building a tree of it that would
/// only be written back to text: a scan of many worktrees is megabytes.
pub type Document = Box<RawValue>;

/// The one field of a document the app reads.
#[derive(Deserialize)]
struct Head<'a> {
    #[serde(borrow)]
    schema: Option<Cow<'a, str>>,
}

impl Output {
    /// The JSON document residue printed, when its `schema` is `schema`.
    /// residue prints one on success and on the exit statuses that still
    /// report (remove's 3, a restore listing that missed a path). Anything
    /// else is an error carrying stderr.
    pub fn document(&self, schema: &str) -> Result<Document, CliError> {
        let trimmed = self.stdout.trim();
        if trimmed.is_empty() {
            return Err(self.error(
                ErrorKind::Failed,
                match self.code {
                    Some(code) => {
                        format!("residue exited with status {code} and printed no report")
                    }
                    None => "residue was stopped by a signal and printed no report".to_string(),
                },
            ));
        }
        // Parsing into a RawValue checks the whole text and keeps it as is.
        let doc: Document = serde_json::from_str(trimmed).map_err(|e| {
            self.error(
                ErrorKind::BadOutput,
                format!("residue printed something that is not JSON ({e})"),
            )
        })?;
        // The text is valid JSON, so this fails only when it is not an
        // object or its `schema` is not a string: either way it has no
        // schema, which the error below says.
        let found = serde_json::from_str::<Head>(doc.get())
            .ok()
            .and_then(|h| h.schema);
        match found.as_deref() {
            Some(s) if s == schema => Ok(doc),
            other => Err(self.error(
                ErrorKind::BadOutput,
                format!(
                    "expected a {schema} document, residue printed {}",
                    other.unwrap_or("a document without a schema")
                ),
            )),
        }
    }

    pub fn error(&self, kind: ErrorKind, message: String) -> CliError {
        let stderr = self.stderr.trim();
        CliError {
            kind,
            message,
            command: Some(self.command.clone()),
            stderr: (!stderr.is_empty()).then(|| stderr.to_string()),
            exit_code: self.code,
        }
    }
}

/// The command line, quoted for a shell, one word per argument, as
/// `shellWord` in `src/lib/format.ts` quotes them. sh, bash, zsh, ksh and
/// fish read each word back as the argument: a word that is not plain goes
/// in single quotes, with each `'` and `\` written outside them as `\'` and
/// `\\`, which fish reads as escapes inside single quotes. A word that
/// holds a control character is written `$'…'` with each such character as
/// `\xHH` bytes, so the text never holds one: a terminal could act on it as
/// a key. bash, zsh and ksh read that word back as the argument, and the
/// window turns copy off for it.
pub fn shown(bin: &Path, args: &[OsString]) -> String {
    std::iter::once(bin.as_os_str())
        .chain(args.iter().map(OsString::as_os_str))
        .map(|a| shell_word(&a.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Characters a terminal or a line editor can act on, or that draw text out
/// of order: C0 controls, DEL, C1 controls, the line and paragraph
/// separators, and the bidirectional controls.
fn is_control(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{2028}'
                | '\u{2029}'
                | '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        )
}

fn shell_word(a: &str) -> String {
    let plain = |b: u8| b.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&b);
    // zsh expands a word that starts with `=`, fish one that starts with `%`.
    if a.bytes().all(plain) && !a.starts_with(['=', '%']) && !a.is_empty() {
        return a.to_string();
    }
    if !a.chars().any(is_control) {
        let mut out = String::from("'");
        for c in a.chars() {
            match c {
                '\'' | '\\' => {
                    out.push('\'');
                    out.push('\\');
                    out.push(c);
                    out.push('\'');
                }
                _ => out.push(c),
            }
        }
        out.push('\'');
        return out;
    }
    let mut out = String::from("$'");
    let mut escaped = false;
    for c in a.chars() {
        // ksh takes every hex digit after `\x`, so one right after an
        // escape is escaped too.
        if is_control(c) || (escaped && c.is_ascii_hexdigit()) {
            let mut buf = [0; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("\\x{b:02x}"));
            }
            escaped = true;
        } else {
            if matches!(c, '\\' | '\'') {
                out.push('\\');
            }
            out.push(c);
            escaped = false;
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(a: &str) -> String {
        shown(Path::new(a), &[])
    }

    /// Reads one word as POSIX shells do: single quotes keep every byte,
    /// outside them a backslash escapes the next character.
    fn posix(word: &str) -> String {
        let mut out = String::new();
        let mut it = word.chars();
        while let Some(c) = it.next() {
            match c {
                '\'' => out.extend(it.by_ref().take_while(|&c| c != '\'')),
                '\\' => out.extend(it.next()),
                _ => out.push(c),
            }
        }
        out
    }

    /// Reads one word as fish does: inside single quotes `\'` and `\\` are
    /// escapes, every other byte is kept.
    fn fish(word: &str) -> String {
        let mut out = String::new();
        let mut it = word.chars().peekable();
        while let Some(c) = it.next() {
            match c {
                '\'' => {
                    while let Some(c) = it.next() {
                        match c {
                            '\'' => break,
                            '\\' if matches!(it.peek(), Some('\'' | '\\')) => out.extend(it.next()),
                            _ => out.push(c),
                        }
                    }
                }
                '\\' => out.extend(it.next()),
                _ => out.push(c),
            }
        }
        out
    }

    #[test]
    fn plain_words_stay_bare() {
        assert_eq!(
            shown(
                Path::new("/opt/homebrew/bin/residue"),
                &[
                    "watch".into(),
                    "--grace".into(),
                    "1h".into(),
                    "/Users/a/src".into()
                ]
            ),
            "/opt/homebrew/bin/residue watch --grace 1h /Users/a/src"
        );
        assert_eq!(word("a_b@c+d,e"), "a_b@c+d,e");
        assert_eq!(word(""), "''");
        assert_eq!(word("=cmd"), "'=cmd'");
        assert_eq!(word("%self"), "'%self'");
    }

    #[test]
    fn quoted_words_read_back_alike_in_posix_shells_and_fish() {
        for a in [
            "/Users/a/my worktree",
            "/x/a\\';touch PWNED #",
            "it's",
            "back\\slash",
            "\\'",
            "''\\\\",
            "$(echo hi)`x`",
            "caf\u{e9} \u{1f600}",
        ] {
            let w = word(a);
            assert!(w.starts_with('\''), "{a:?} gave {w:?}");
            assert_eq!(posix(&w), a, "sh reads {w:?}");
            assert_eq!(fish(&w), a, "fish reads {w:?}");
        }
    }

    #[test]
    fn control_characters_never_reach_the_text() {
        let a = "/x/y\u{1b}[201~\u{15}touch PWNED2\r";
        let w = word(a);
        assert_eq!(w, "$'/x/y\\x1b[201~\\x15touch PWNED2\\x0d'");
        for a in [
            "a\u{7f}b",
            "a\u{85}b",
            "a\u{2028}b",
            "a\u{202e}b",
            "a\u{2066}b",
            "a\nb",
            "a\tb",
        ] {
            let w = word(a);
            assert!(!w.chars().any(is_control), "{a:?} gave {w:?}");
            assert!(w.starts_with("$'"), "{a:?} gave {w:?}");
        }
        // A hex digit right after an escape is escaped too, for ksh; a
        // quote and a backslash keep their escapes.
        assert_eq!(word("\u{1b}c9'\\z"), "$'\\x1b\\x63\\x39\\'\\\\z'");
        assert_eq!(word("\u{202e}"), "$'\\xe2\\x80\\xae'");
    }
}
