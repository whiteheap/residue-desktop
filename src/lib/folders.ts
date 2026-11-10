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
