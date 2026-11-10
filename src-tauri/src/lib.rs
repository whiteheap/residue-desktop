//! The residue Mac app: a window over the residue CLI.
//!
//! Every check, size, removal and restore comes from the bundled `residue`
//! binary. The commands here run it with explicit arguments and return its
//! JSON documents (`residue.scan/v1`, `residue.explain/v1`,
//! `residue.remove/v1`, `residue.restore/v1`) to the window unchanged. The
//! window never decides on its own what is safe to remove.

mod cli;
#[cfg(all(debug_assertions, target_os = "macos"))]
mod devshot;
mod env;
mod removal;
mod service;
mod settings;

use cli::{CliError, Document, ErrorKind, Run};
use serde::Serialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tauri::Manager;

/// A scan reads every entry of every worktree; a large home folder takes a
/// while, so the deadline only catches a run that hangs.
const SCAN_DEADLINE: Duration = Duration::from_secs(30 * 60);
const REMOVE_DEADLINE: Duration = Duration::from_secs(30 * 60);
const RESTORE_DEADLINE: Duration = Duration::from_secs(10 * 60);
const STATS_DEADLINE: Duration = Duration::from_secs(2 * 60);

struct State {
    sidecar: PathBuf,
    home: PathBuf,
    config_dir: PathBuf,
    data_dir: PathBuf,
    dev: service::DevPreview,
    /// The one save or turn-off of automatic cleanup that runs.
    changes: service::Changes,
    /// The one confirmed removal that runs, this app's or, after a
    /// relaunch, the one an earlier launch left running (#12).
    removals: removal::Removals,
    /// The bundled residue's version, once a read has given it.
    version: Mutex<Option<String>>,
}

type AppState<'a> = tauri::State<'a, Arc<State>>;

impl State {
    fn run(
        &self,
        args: Vec<OsString>,
        cwd: &Path,
        deadline: Duration,
    ) -> Result<cli::Output, CliError> {
        self.run_with(args, cwd, deadline, |_| {})
    }

    /// `run`, calling `started` with residue's PID once it runs.
    fn run_with(
        &self,
        args: Vec<OsString>,
        cwd: &Path,
        deadline: Duration,
        started: impl FnOnce(u32),
    ) -> Result<cli::Output, CliError> {
        if !self.sidecar.is_file() {
            return Err(CliError {
                kind: ErrorKind::Missing,
                message: format!(
                    "the bundled residue binary is missing ({}); reinstall the app",
                    self.sidecar.display()
                ),
                command: None,
                stderr: None,
                exit_code: None,
            });
        }
        Run {
            bin: &self.sidecar,
            args,
            cwd,
            path_env: &env::path().0,
            deadline,
        }
        .output_with(started)
    }

    /// The bundled residue's version for the window's version line. The
    /// binary ships inside the app, so the first read that gives a version
    /// serves every later call; an app replaced on disk while it runs shows
    /// the old version, like its old window, until it restarts. A failed read
    /// is not kept: the next call runs `residue --version` again, so a
    /// passing failure (a start slower than the deadline, a spawn the system
    /// refused) does not last until the app quits, and a lasting one reports
    /// its own error each time. Automatic cleanup never uses it
    /// (`bundled_version_now`).
    fn bundled_version(&self) -> Result<String, CliError> {
        // Held through the read, so calls that come together start one
        // process. A poisoned lock still holds a whole value: the one write is
        // a single assignment after a read succeeds.
        let mut cached = self.version.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(v) = cached.as_ref() {
            return Ok(v.clone());
        }
        let v = self.bundled_version_now()?;
        Ok(cached.insert(v).clone())
    }

    /// The bundled residue's version, read now. Each automatic cleanup
    /// command reads it again: a save copies the bundled binary as it is at
    /// that moment, and an app replaced on disk while it runs has another
    /// release at the same path, so a version read earlier would name one
    /// residue while the save installs another.
    fn bundled_version_now(&self) -> Result<String, CliError> {
        let out = self.run(
            vec!["--version".into()],
            &self.home,
            Duration::from_secs(10),
        )?;
        let text = out.stdout.trim();
        match text.strip_prefix("residue ") {
            Some(v) if out.code == Some(0) => Ok(v.to_string()),
            _ => Err(out.error(
                ErrorKind::BadOutput,
                format!("residue --version printed {text:?}"),
            )),
        }
    }

    fn service_context<'a>(&'a self, version: &'a str) -> service::Context<'a> {
        service::Context {
            sidecar: &self.sidecar,
            bundled_version: version,
            data_dir: &self.data_dir,
            home: &self.home,
            path_env: &env::path().0,
            changes: &self.changes,
        }
    }
}

/// Run blocking work off the window's thread.
async fn blocking<T: Send + 'static>(
    state: &AppState<'_>,
    work: impl FnOnce(&State) -> Result<T, CliError> + Send + 'static,
) -> Result<T, CliError> {
    let state = Arc::clone(state);
    tauri::async_runtime::spawn_blocking(move || work(&state))
        .await
        .map_err(|e| CliError::failed(format!("the background task stopped: {e}")))?
}

#[derive(Serialize)]
struct AppInfo {
    app_version: String,
    residue_version: String,
    sidecar: String,
    home: String,
    path_source: env::PathSource,
    dev_service_preview: bool,
    /// `residue remove --keep-trash` when none is given, which the app's
    /// removals and automatic cleanup use.
    default_keep_trash: &'static str,
    /// `residue remove --keep-removed` when none is given, which the app's
    /// removals use.
    default_keep_removed: &'static str,
}

#[tauri::command]
async fn app_info(app: tauri::AppHandle, state: AppState<'_>) -> Result<AppInfo, CliError> {
    let app_version = app.package_info().version.to_string();
    blocking(&state, move |s| {
        let residue_version = s.bundled_version()?;
        Ok(AppInfo {
            app_version,
            residue_version,
            sidecar: s.sidecar.to_string_lossy().into_owned(),
            home: s.home.to_string_lossy().into_owned(),
            path_source: env::path().1,
            dev_service_preview: s.dev.enabled,
            default_keep_trash: service::DEFAULT_KEEP_TRASH,
            default_keep_removed: service::DEFAULT_KEEP_REMOVED,
        })
    })
    .await
}

#[tauri::command]
async fn get_settings(state: AppState<'_>) -> Result<settings::Settings, CliError> {
    blocking(&state, |s| settings::load(&s.config_dir, &s.home)).await
}

#[tauri::command]
async fn set_roots(
    state: AppState<'_>,
    roots: Vec<String>,
) -> Result<settings::Settings, CliError> {
    blocking(&state, move |s| {
        settings::save(&s.config_dir, &s.home, &roots)
    })
    .await
}

/// `residue scan --json ROOT...`
#[tauri::command]
async fn scan(state: AppState<'_>, roots: Vec<String>) -> Result<Document, CliError> {
    blocking(&state, move |s| {
        let roots = settings::folders(&roots)?;
        if roots.is_empty() {
            return Err(CliError::invalid("choose a folder to scan"));
        }
        let mut args: Vec<OsString> = vec!["scan".into(), "--json".into()];
        args.extend(roots.into_iter().map(PathBuf::into_os_string));
        s.run(args, &s.home, SCAN_DEADLINE)?
            .document("residue.scan/v1")
    })
    .await
}

/// `residue explain --json PATH`
#[tauri::command]
async fn explain(state: AppState<'_>, path: String) -> Result<Document, CliError> {
    blocking(&state, move |s| {
        let path = absolute(&path)?;
        s.run(
            vec!["explain".into(), "--json".into(), path.into_os_string()],
            &s.home,
            SCAN_DEADLINE,
        )?
        .document("residue.explain/v1")
    })
    .await
}

#[derive(Serialize)]
struct Removal {
    document: Document,
    exit_code: Option<i32>,
}

/// The `generated_at` of the plan a confirm runs, passed as `--as-of` (#13).
/// residue then measures every age at that time, so the run purges and
/// forgets only what the plan could list. Only the form a plan's
/// `generated_at` has, RFC 3339 in UTC with whole seconds such as
/// `2026-11-09T12:00:00Z`: residue also takes a `+HH:MM` offset, which no
/// plan has. residue checks the date itself, and refuses one that does not
/// exist with exit status 2, before it acts.
struct AsOf(String);

impl AsOf {
    fn of_plan(generated_at: Option<String>) -> Result<AsOf, CliError> {
        let Some(time) = generated_at else {
            return Err(CliError::invalid(
                "the plan has no time to confirm it at; close and try again",
            ));
        };
        let shape = b"dddd-dd-ddTdd:dd:ddZ";
        let fits = time.len() == shape.len()
            && time.bytes().zip(shape).all(|(b, &want)| match want {
                b'd' => b.is_ascii_digit(),
                _ => b == want,
            });
        if !fits {
            return Err(CliError::invalid(format!(
                "the plan's time {time:?} is not one residue writes; close and try again"
            )));
        }
        Ok(AsOf(time))
    }
}

/// `residue remove --trash --json PATH...`, and once the user confirmed,
/// `--yes --as-of TIME` with the time of the plan they saw (`confirmed`).
/// Without `--yes`, and with stdin that is not a terminal, residue removes
/// and purges nothing and prints its plan: each path checked again, the
/// expired trash entries the run would purge and the removal records it
/// would forget. The confirmation dialog shows that plan. With `--yes`,
/// residue checks every path once more before it moves it, and keeps any
/// that changed; with `--as-of`, it judges every age at the plan's time.
///
/// One confirmed removal runs at a time (#12): it holds `s.removals` from
/// before residue starts until it returns, and another is refused, with kind
/// `invalid` and running nothing, while one is in flight, also one an earlier
/// launch of the app left running (`removal.rs`). A plan is not refused: it
/// moves nothing, and the window keeps Move to the trash off while a
/// removal is in flight.
fn remove(s: &State, paths: &[String], confirmed: Option<AsOf>) -> Result<Removal, CliError> {
    if paths.is_empty() {
        return Err(CliError::invalid("no worktrees were selected"));
    }
    let mut args: Vec<OsString> = vec!["remove".into(), "--trash".into(), "--json".into()];
    if let Some(AsOf(time)) = &confirmed {
        args.extend(["--yes".into(), "--as-of".into(), time.into()]);
    }
    for p in paths {
        args.push(absolute(p)?.into_os_string());
    }
    let mut removing = if confirmed.is_some() {
        Some(s.removals.start()?)
    } else {
        None
    };
    let out = s.run_with(args, &s.home, REMOVE_DEADLINE, |pid| {
        if let Some(r) = removing.as_mut() {
            r.record(pid);
        }
    })?;
    drop(removing);
    // 0: every path went to the trash. 3: at least one stayed (or, for the
    // plan, nothing was confirmed), and the document says why. Anything else
    // is an error without a document.
    match out.code {
        Some(0 | 3) => Ok(Removal {
            document: out.document("residue.remove/v1")?,
            exit_code: out.code,
        }),
        _ => Err(out.error(ErrorKind::Failed, "residue remove failed".into())),
    }
}

/// The plan for moving `paths` to the trash. Removes nothing.
#[tauri::command]
async fn plan_removal(state: AppState<'_>, paths: Vec<String>) -> Result<Removal, CliError> {
    blocking(&state, move |s| remove(s, &paths, None)).await
}

/// Move the confirmed `paths` to their repositories' trash, judging ages at
/// `as_of`, the `generated_at` of the plan the dialog showed (#13). Without
/// it, or with a value that is not a plan's time, it runs nothing and
/// returns kind `invalid`.
#[tauri::command]
async fn remove_to_trash(
    state: AppState<'_>,
    paths: Vec<String>,
    as_of: Option<String>,
) -> Result<Removal, CliError> {
    blocking(&state, move |s| {
        remove(s, &paths, Some(AsOf::of_plan(as_of)?))
    })
    .await
}

/// Whether a confirmed removal is in flight, this app's or one an earlier
/// launch left running (#12). The window asks when it starts and when the
/// removal dialog closes, and keeps Move to the trash off while one is.
#[tauri::command]
async fn removal_running(state: AppState<'_>) -> Result<bool, CliError> {
    blocking(&state, |s| Ok(s.removals.running())).await
}

/// Returns once no confirmed removal is in flight, at once when none is. The
/// window then scans again.
#[tauri::command]
async fn wait_for_removal(state: AppState<'_>) -> Result<(), CliError> {
    blocking(&state, |s| {
        s.removals.wait();
        Ok(())
    })
    .await
}

#[derive(Serialize)]
struct Listing {
    /// The folder the listing ran in.
    root: String,
    document: Option<Document>,
    error: Option<CliError>,
}

/// `residue restore --json` in each folder: the trash of the repositories at
/// or under it. A folder that fails reports its error beside the others.
#[tauri::command]
async fn list_restorable(
    state: AppState<'_>,
    roots: Vec<String>,
) -> Result<Vec<Listing>, CliError> {
    blocking(&state, move |s| {
        let roots = settings::folders(&roots)?;
        Ok(roots
            .into_iter()
            .map(|root| {
                let result = s
                    .run(
                        vec!["restore".into(), "--json".into()],
                        &root,
                        RESTORE_DEADLINE,
                    )
                    .and_then(|out| out.document("residue.restore/v1"));
                let root = root.to_string_lossy().into_owned();
                match result {
                    Ok(doc) => Listing {
                        root,
                        document: Some(doc),
                        error: None,
                    },
                    Err(e) => Listing {
                        root,
                        document: None,
                        error: Some(e),
                    },
                }
            })
            .collect())
    })
    .await
}

/// `residue restore --json ENTRY_PATH`, run in the folder that listed it.
#[tauri::command]
async fn restore(
    state: AppState<'_>,
    entry_path: String,
    root: String,
) -> Result<Document, CliError> {
    blocking(&state, move |s| {
        let entry = absolute(&entry_path)?;
        let root = settings::folders(&[root])?
            .pop()
            .ok_or_else(|| CliError::invalid("no folder to restore from"))?;
        let out = s.run(
            vec!["restore".into(), "--json".into(), entry.into_os_string()],
            &root,
            RESTORE_DEADLINE,
        )?;
        if out.code != Some(0) {
            return Err(out.error(ErrorKind::Failed, "residue could not restore it".into()));
        }
        out.document("residue.restore/v1")
    })
    .await
}

/// `residue stats --json`: what residue has freed on this machine, from its
/// ledger. It reads the ledger and the trash entries of the repositories the
/// ledger names, never a worktree, so it is quick.
#[tauri::command]
async fn stats(state: AppState<'_>) -> Result<Document, CliError> {
    blocking(&state, |s| {
        s.run(
            vec!["stats".into(), "--json".into()],
            &s.home,
            STATS_DEADLINE,
        )?
        .document("residue.stats/v1")
    })
    .await
}

#[tauri::command]
async fn service_status(state: AppState<'_>) -> Result<service::Status, CliError> {
    blocking(&state, |s| {
        let version = s.bundled_version_now()?;
        service::status(&s.service_context(&version), &s.dev)
    })
    .await
}

/// Turn the service on, or save other folders. `basis` is the one the window
/// showed (`Status.basis`); when a fresh read differs, nothing is installed
/// and the result is `changed`, with what changed and the fresh status.
#[tauri::command]
async fn service_install(
    state: AppState<'_>,
    roots: Vec<String>,
    dry_run: bool,
    basis: service::Basis,
) -> Result<service::Save, CliError> {
    blocking(&state, move |s| {
        let roots = settings::folders(&roots)?;
        let version = s.bundled_version_now()?;
        let ctx = s.service_context(&version);
        service::install(&ctx, &s.dev, &roots, dry_run, &basis)
    })
    .await
}

#[tauri::command]
async fn service_uninstall(state: AppState<'_>) -> Result<service::Change, CliError> {
    blocking(&state, |s| {
        let version = s.bundled_version_now()?;
        service::uninstall(&s.service_context(&version), &s.dev)
    })
    .await
}

/// `residue watch --once --dry-run ROOT...` with every option a save keeps,
/// run by the binary a save installs with: what automatic cleanup would
/// remove now, fetching first as the service does. It removes nothing.
#[tauri::command]
async fn service_preview(
    state: AppState<'_>,
    roots: Vec<String>,
) -> Result<service::Preview, CliError> {
    blocking(&state, move |s| {
        let roots = settings::folders(&roots)?;
        let version = s.bundled_version_now()?;
        service::preview(&s.service_context(&version), &s.dev, &roots)
    })
    .await
}

fn absolute(path: &str) -> Result<PathBuf, CliError> {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        Ok(p)
    } else {
        Err(CliError::invalid(format!("{path} is not an absolute path")))
    }
}

/// The bundled residue binary. Tauri places each `externalBin` next to the
/// app's own executable without its target-triple suffix, in `tauri dev` and
/// in the bundle (`residue.app/Contents/MacOS/residue-cli`). It is not named
/// residue: that is the app's own executable, which must never run in place
/// of the CLI.
fn sidecar() -> Result<PathBuf, String> {
    let exe =
        std::env::current_exe().map_err(|e| format!("cannot find the app's executable: {e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "the app's executable has no parent directory".to_string())?;
    Ok(dir.join("residue-cli"))
}

/// Whether the window may show `url`: only the app's own pages, which Tauri
/// serves at `tauri://localhost`, and in a development build the dev server's
/// (`dev`, `build.devUrl`). Every other navigation, a link, a redirect or a
/// script that sets `location`, is refused, so no other page ever gets the
/// app's commands.
fn own_page(url: &tauri::Url, dev: Option<&tauri::Url>) -> bool {
    let bundled =
        url.scheme() == "tauri" && url.host_str() == Some("localhost") && url.port().is_none();
    bundled || dev.is_some_and(|dev| dev.origin() == url.origin())
}

/// Refuses every navigation `own_page` does not allow, in every webview.
fn navigation_guard<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("navigation-guard")
        .on_navigation(|webview, url| {
            let dev = if cfg!(dev) {
                webview.config().build.dev_url.clone()
            } else {
                None
            };
            let allowed = own_page(url, dev.as_ref());
            if !allowed {
                eprintln!("residue app: refused to navigate to {url}");
            }
            allowed
        })
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(navigation_guard())
        .plugin(tauri_plugin_dialog::init())
        // Only reveal in Finder (Detail.tsx). Without the script it injects
        // by default, a click on a link never asks it to open a URL.
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .setup(|app| {
            let paths = app.path();
            let data_dir = paths.app_local_data_dir()?;
            let state = State {
                sidecar: sidecar()?,
                home: env::home().ok_or("HOME is not set")?,
                config_dir: paths.app_config_dir()?,
                dev: service::DevPreview::from_env(),
                changes: service::Changes::default(),
                removals: removal::Removals::open(&data_dir),
                data_dir,
                version: Mutex::new(None),
            };
            app.manage(Arc::new(state));
            // Read the login shell's PATH while the window loads.
            std::thread::spawn(|| {
                env::path();
            });
            #[cfg(all(debug_assertions, target_os = "macos"))]
            devshot::start(app.handle());
            Ok(())
        })
        // Each command is also in commands.rs, which grants it to the window.
        .invoke_handler(tauri::generate_handler![
            app_info,
            get_settings,
            set_roots,
            scan,
            explain,
            plan_removal,
            remove_to_trash,
            removal_running,
            wait_for_removal,
            list_restorable,
            restore,
            stats,
            service_status,
            service_install,
            service_uninstall,
            service_preview,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("residue app: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::own_page;
    use tauri::Url;

    include!("../commands.rs");

    fn url(s: &str) -> Url {
        s.parse().unwrap()
    }

    #[test]
    fn the_window_shows_only_the_apps_own_pages() {
        for page in [
            "tauri://localhost",
            "tauri://localhost/",
            "tauri://localhost/index.html",
        ] {
            assert!(own_page(&url(page), None), "{page}");
        }
        for page in [
            "https://example.com/",
            "http://localhost:1420/",
            "http://127.0.0.1:1420/",
            // macOS serves the app at tauri://localhost; this name is loopback.
            "http://tauri.localhost/",
            "https://tauri.localhost/",
            "tauri://localhost:8080/",
            "tauri://example.com/",
            "file:///etc/hosts",
            "about:blank",
            "data:text/html,hi",
            "blob:tauri://localhost/x",
        ] {
            assert!(!own_page(&url(page), None), "{page}");
        }
    }

    #[test]
    fn a_development_build_also_shows_its_dev_server() {
        let dev = url("http://127.0.0.1:1420");
        for page in [
            "http://127.0.0.1:1420/",
            "http://127.0.0.1:1420/src/main.tsx",
            "tauri://localhost/",
        ] {
            assert!(own_page(&url(page), Some(&dev)), "{page}");
        }
        for page in [
            "http://127.0.0.1:1421/",
            "http://localhost:1420/",
            "https://127.0.0.1:1420/",
            "http://[::1]:1420/",
            "http://example.com:1420/",
        ] {
            assert!(!own_page(&url(page), Some(&dev)), "{page}");
        }
    }

    #[test]
    fn the_page_policy_refuses_plugins_a_base_forms_and_framing() {
        let context: tauri::Context<tauri::test::MockRuntime> =
            tauri::generate_context!(test = true);
        let csp = context
            .config()
            .app
            .security
            .csp
            .as_ref()
            .expect("a CSP")
            .to_string();
        for directive in [
            "script-src 'self'",
            "object-src 'none'",
            "base-uri 'none'",
            "form-action 'none'",
            "frame-ancestors 'none'",
        ] {
            assert!(csp.contains(directive), "{directive} in {csp}");
        }
    }

    /// The app's capability, with a handler that answers every command: only
    /// what the capability grants the main window gets past Tauri's check.
    fn call(cmd: &str) -> Result<(), serde_json::Value> {
        let app = tauri::test::mock_builder()
            .invoke_handler(|invoke| {
                invoke.resolver.resolve("ok");
                true
            })
            .build(tauri::generate_context!(test = true))
            .expect("the app builds");
        let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("the main window");
        tauri::test::get_ipc_response(
            &main,
            tauri::webview::InvokeRequest {
                cmd: cmd.into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: url("tauri://localhost"),
                body: tauri::ipc::InvokeBody::default(),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.to_string(),
            },
        )
        .map(|_| ())
    }

    /// The commands `run` registers, read from this file's
    /// `generate_handler!`, are the ones commands.rs lists, which build.rs
    /// gives permissions to: a command in one list and not the other is
    /// refused at its first call, or granted with nothing to run.
    #[test]
    fn the_handler_registers_the_commands_that_have_permissions() {
        let source = include_str!("lib.rs");
        let start = source
            .find("tauri::generate_handler![")
            .expect("run registers its commands with generate_handler!");
        let list = &source[start + "tauri::generate_handler![".len()..];
        let list = &list[..list.find(']').expect("generate_handler! ends")];
        let mut registered: Vec<&str> = list
            .split(',')
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .collect();
        let mut listed: Vec<&str> = COMMANDS.to_vec();
        registered.sort_unstable();
        listed.sort_unstable();
        let unique = |v: &[&str]| v.windows(2).all(|w| w[0] != w[1]);
        assert!(
            unique(&registered),
            "a command registered twice: {registered:?}"
        );
        assert!(
            unique(&listed),
            "a command listed twice in commands.rs: {listed:?}"
        );
        assert_eq!(registered, listed);
    }

    #[test]
    fn the_window_calls_every_app_command() {
        for cmd in COMMANDS {
            assert_eq!(call(cmd), Ok(()), "{cmd}");
        }
    }

    #[test]
    fn the_window_calls_nothing_else() {
        for cmd in [
            "not_a_command",
            "plugin:opener|open_url",
            "plugin:opener|open_path",
        ] {
            let refused = call(cmd).expect_err(cmd);
            assert!(
                refused.to_string().contains("not allowed"),
                "{cmd}: {refused}"
            );
        }
    }
}
