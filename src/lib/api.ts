import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  CliError,
  ExplainDocument,
  Listing,
  Removal,
  RestoreDocument,
  ScanDocument,
  ServiceBasis,
  ServiceChange,
  ServicePreview,
  ServiceSave,
  ServiceStatus,
  Settings,
  StatsDocument,
} from "./types";

/** Every rejection from a command is a CliError; anything else is wrapped. */
export function asCliError(e: unknown): CliError {
  if (e && typeof e === "object" && "kind" in e && "message" in e) {
    return e as CliError;
  }
  return {
    kind: "failed",
    message: typeof e === "string" ? e : String(e),
    command: null,
    stderr: null,
    exit_code: null,
  };
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    throw asCliError(e);
  }
}

// Calls of `service_install` and `service_uninstall` that have not settled.
let changing = 0;

async function change<T>(run: () => Promise<T>): Promise<T> {
  changing += 1;
  try {
    return await run();
  } finally {
    changing -= 1;
  }
}

/**
 * Whether a save or a turn-off of automatic cleanup runs. It counts the
 * call, not the dialog that made it, so it still holds when App draws itself
 * afresh. App keeps the screen while it holds: leaving would close the
 * dialog while the call runs on, and its result would show nowhere (#10).
 * src-tauri refuses a second change while one runs (`service.rs`, `Changes`).
 */
export const changingService = () => changing > 0;

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  settings: () => call<Settings>("get_settings"),
  setRoots: (roots: string[]) => call<Settings>("set_roots", { roots }),
  scan: (roots: string[]) => call<ScanDocument>("scan", { roots }),
  explain: (path: string) => call<ExplainDocument>("explain", { path }),
  planRemoval: (paths: string[]) => call<Removal>("plan_removal", { paths }),
  /**
   * `asOf` is the `generated_at` of the plan the dialog showed: residue
   * judges every age at that time, so the run purges and forgets only what
   * the plan could list (#13).
   */
  removeToTrash: (paths: string[], asOf: string) => call<Removal>("remove_to_trash", { paths, asOf }),
  /**
   * Whether a confirmed removal runs: this window's, or one src-tauri still
   * holds after the window started afresh or the app opened again (#12).
   * src-tauri refuses a second one while one runs (`removal.rs`).
   */
  removalRunning: () => call<boolean>("removal_running"),
  /** Settles once no confirmed removal runs. */
  waitForRemoval: () => call<void>("wait_for_removal"),
  listRestorable: (roots: string[]) => call<Listing[]>("list_restorable", { roots }),
  restore: (entryPath: string, root: string) =>
    call<RestoreDocument>("restore", { entryPath, root }),
  serviceStatus: () => call<ServiceStatus>("service_status"),
  /** `basis` is the one the dialog showed; a fresh read that differs installs nothing. */
  serviceInstall: (roots: string[], dryRun: boolean, basis: ServiceBasis) =>
    change(() => call<ServiceSave>("service_install", { roots, dryRun, basis })),
  serviceUninstall: () => change(() => call<ServiceChange>("service_uninstall")),
  servicePreview: (roots: string[]) => call<ServicePreview>("service_preview", { roots }),
  stats: () => call<StatsDocument>("stats"),
};
