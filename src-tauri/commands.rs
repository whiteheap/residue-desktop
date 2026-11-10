// The app's commands, the ones `run` registers with `generate_handler!` in
// src/lib.rs. build.rs gives each one an `allow-` permission
// (`AppManifest::commands`), so the window can call a command only when
// capabilities/default.json grants it. A command added to the handler and not
// here is refused at the first call, in a debug build too.
pub const COMMANDS: &[&str] = &[
    "app_info",
    "get_settings",
    "set_roots",
    "scan",
    "explain",
    "plan_removal",
    "remove_to_trash",
    "removal_running",
    "wait_for_removal",
    "list_restorable",
    "restore",
    "stats",
    "service_status",
    "service_install",
    "service_uninstall",
    "service_preview",
];
