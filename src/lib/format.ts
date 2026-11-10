// Display only. Sizes and ages are formatted the way the residue CLI prints
// them, so the window and the terminal read the same.

const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

/** `1.2 GB`, `512 MB`, `0 B`: decimal units, like `residue scan`. */
export function bytes(n: number | null | undefined): string {
  if (n == null) return "size unknown";
  let value = n;
  let unit = 0;
  while (value >= 1000 && unit + 1 < UNITS.length) {
    value /= 1000;
    unit += 1;
  }
  if (unit === 0) return `${n} B`;
  return value >= 100 ? `${value.toFixed(0)} ${UNITS[unit]}` : `${value.toFixed(1)} ${UNITS[unit]}`;
}

/** `just now`, `5 minutes ago`, `3 hours ago`, `2 days ago`. */
export function age(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  if (s < 120) return "just now";
  if (s < 7200) return `${Math.floor(s / 60)} minutes ago`;
  if (s < 172800) return `${Math.floor(s / 3600)} hours ago`;
  return `${Math.floor(s / 86400)} days ago`;
}

/** How long before `nowSeconds` the time `iso` was. */
export function ageOf(iso: string | null | undefined, nowSeconds: number | null): string | null {
  if (!iso || nowSeconds == null) return null;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return null;
  return age(nowSeconds - t / 1000);
}

export function grouped(n: number): string {
  return n.toLocaleString("en-US");
}

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${grouped(n)} ${n === 1 ? one : many}`;
}

/** `a, b, c, d`, or `a, b, c and 5 more`: "and 1 more" never takes a name's place. */
export function someOf(names: string[]): string {
  if (names.length <= 4) return names.join(", ");
  return `${names.slice(0, 3).join(", ")} and ${grouped(names.length - 3)} more`;
}

/** `~/Work/app` for a path under the home folder. */
export function tilde(path: string, home: string | undefined): string {
  if (!home || home === "/") return path;
  const h = home.replace(/\/+$/, "");
  if (path === h) return "~";
  return path.startsWith(`${h}/`) ? `~${path.slice(h.length)}` : path;
}

/**
 * Characters a terminal or a line editor can act on, or that draw text out
 * of order: C0 controls, DEL, C1 controls, the line and paragraph
 * separators, and the bidirectional controls.
 */
// oxlint-disable-next-line no-control-regex -- finding them is the point.
const CONTROL = /[\u0000-\u001f\u007f-\u009f\u2028\u2029\u061c\u200e\u200f\u202a-\u202e\u2066-\u2069]/u;

/** `c` as its UTF-8 bytes, each written `\xHH`. */
function hexBytes(c: string): string {
  return [...new TextEncoder().encode(c)].map((b) => `\\x${b.toString(16).padStart(2, "0")}`).join("");
}

/**
 * `s` as one shell word that sh, bash, zsh, ksh and fish read back as `s`:
 * bare when that is safe; otherwise in single quotes, with each `'` and `\`
 * written outside them as `\'` and `\\`, since fish reads those two as
 * escapes inside single quotes and POSIX shells do not. A word that holds a
 * control character is written `$'…'` with each such character as `\xHH`
 * bytes, so the text holds none: bash, zsh and ksh read it back as `s`, fish
 * refuses it, and `commandView` turns its copy off.
 */
export function shellWord(s: string): string {
  // zsh expands a word that starts with `=`, fish one that starts with `%`.
  if (/^[\w@+:,./-][\w@%+=:,./-]*$/.test(s)) return s;
  if (!CONTROL.test(s)) return `'${s.replace(/['\\]/g, (c) => `'\\${c}'`)}'`;
  let out = "$'";
  let escaped = false;
  for (const c of s) {
    // ksh takes every hex digit after `\x`, so one right after an escape is escaped too.
    if (CONTROL.test(c) || (escaped && /[0-9a-f]/i.test(c))) {
      out += hexBytes(c);
      escaped = true;
    } else {
      out += c === "\\" || c === "'" ? `\\${c}` : c;
      escaped = false;
    }
  }
  return `${out}'`;
}

/**
 * A command to show and copy, from words quoted as `shellWord` and residue's
 * `shown` quote them. Copy is off when a word is in `$'…'` form, which fish
 * and dash read differently, and when the text holds a control character
 * itself, which is then shown as `\xHH`: a terminal can act on one as a key.
 */
export function commandView(command: string): { text: string; copyable: boolean } {
  const text = command.replace(new RegExp(CONTROL.source, "gu"), hexBytes);
  return { text, copyable: text === command && readsAlike(command) };
}

/**
 * Whether every shell reads `command` alike: no word starts with `$'`, and
 * every single quote closes, reading the quotes as `shellWord` writes them.
 */
function readsAlike(command: string): boolean {
  let start = true;
  for (let i = 0; i < command.length; i++) {
    const c = command[i];
    if (c === " ") {
      start = true;
      continue;
    }
    if (start && c === "$" && command[i + 1] === "'") return false;
    start = false;
    if (c === "'") {
      const end = command.indexOf("'", i + 1);
      if (end < 0) return false;
      i = end;
    } else if (c === "\\") {
      i += 1;
    }
  }
  return true;
}

/** `path` relative to `base` when it is inside it, else null. */
export function inside(path: string, base: string): string | null {
  const b = base.replace(/\/+$/, "");
  return path.startsWith(`${b}/`) ? path.slice(b.length + 1) : null;
}

export function baseName(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  return trimmed.slice(trimmed.lastIndexOf("/") + 1) || trimmed;
}

/** `refs/heads/feat/x` -> `feat/x`. */
export function shortBranch(ref: string | null | undefined): string | null {
  if (!ref) return null;
  return ref.startsWith("refs/heads/") ? ref.slice("refs/heads/".length) : ref;
}

export function shortHead(head: string | null | undefined): string | null {
  return head ? head.slice(0, 7) : null;
}

/** `7 days`, `12 hours`, `30 minutes`: a span residue states in seconds. */
export function span(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  if (s % 86400 === 0 && s > 0) return plural(s / 86400, "day");
  if (s % 3600 === 0 && s > 0) return plural(s / 3600, "hour");
  if (s % 60 === 0 && s > 0) return plural(s / 60, "minute");
  return plural(s, "second");
}
