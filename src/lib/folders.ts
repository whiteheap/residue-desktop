import type { Settings } from "./types";

const NONE: string[] = [];

/**
 * The folders the app runs residue in: the ones the user saved. Until they
 * save a list, `settings.roots` are only suggestions, the usual code folders
 * that exist, and none is scanned: a scan runs Git in every repository under
 * them, with each repository's own settings.
 */
export function confirmedRoots(settings: Pick<Settings, "roots" | "saved">): string[] {
  return settings.saved ? settings.roots : NONE;
}

/** The suggested folders that wait for the user to confirm them, or none. */
export function unconfirmedRoots(settings: Pick<Settings, "roots" | "saved">): string[] {
  return settings.saved ? NONE : settings.roots;
}

/**
 * The suggestions the user confirmed that a fresh read of the settings still
 * suggests, so still exist: saving one deleted since the window read it
 * would be refused, every time. Never a folder the user was not shown, and
 * none once a list is saved.
 */
export function stillSuggested(shown: string[], fresh: Pick<Settings, "roots" | "saved">): string[] {
  const now = new Set(unconfirmedRoots(fresh));
  return shown.filter((f) => now.has(f));
}
