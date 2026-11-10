// residue's removal plan as the removal dialog lists it, and what its confirm
// sends back. Only type imports, so `node --test` runs this file as it is
// (tests/plan.test.ts).

import type { RemoveDocument, ShownRemoval } from "./types";

/**
 * The dialog's lists. Without --yes residue marks each worktree it would move
 * as skipped (nothing was confirmed) and each one it keeps as refused. Each
 * expired trash entry it would purge, and each removal record it would
 * forget, is skipped too, and one it keeps for now has another result, such
 * as kept (in use, or a check that could not finish) or failed (Git no longer
 * lists it). The confirmed run judges each again and may purge or forget any
 * of them, so the dialog lists them all: `purges` and `forgets` as what the
 * run does, `mayPurge` and `mayForget` as what it may also do.
 */
export function planLists(plan: RemoveDocument) {
  const ready = plan.results.filter((r) => r.result === "skipped" && r.worktree?.verdict === "candidate");
  return {
    ready,
    kept: plan.results.filter((r) => !ready.includes(r)),
    purges: plan.purged.filter((p) => p.result === "skipped"),
    mayPurge: plan.purged.filter((p) => p.result !== "skipped"),
    forgets: plan.forgotten.filter((f) => f.result === "skipped"),
    mayForget: plan.forgotten.filter((f) => f.result !== "skipped"),
  };
}

/**
 * What the dialog shows of `plan`, which its confirm sends back: the worktrees
 * it moves, each with its folder as read right after the plan (`folders`),
 * and every trash entry and removal record it lists.
 */
export function shownOf(plan: RemoveDocument, folders: Record<string, string>): ShownRemoval {
  return {
    as_of: plan.as_of,
    worktrees: planLists(plan).ready.map((r) => ({
      path: r.path,
      canonical_path: r.worktree?.canonical_path ?? null,
      branch: r.worktree?.branch ?? null,
      head: r.worktree?.head ?? null,
      locked: r.worktree?.locked ?? false,
      folder: folders[r.path] ?? null,
    })),
    purges: plan.purged.map((p) => p.entry),
    forgets: plan.forgotten.map((f) => f.entry),
  };
}
