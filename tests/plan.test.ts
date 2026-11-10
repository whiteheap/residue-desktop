// The removal dialog's lists and what its confirm sends back
// (src/lib/plan.ts), on a plan shaped as residue 0.0.12 prints it.
// Run with `pnpm test`.

import assert from "node:assert/strict";
import test from "node:test";
import { planLists, shownOf } from "../src/lib/plan.ts";
import type { RemoveDocument } from "../src/lib/types.ts";

const repo = "/Users/me/src/repo";

function worktree(path: string, branch: string, verdict: string) {
  return {
    path,
    canonical_path: path,
    verdict,
    stale_reason: null,
    head: "0073ccd1d4100cbc8776d72bac181a5fed0012a6",
    branch: `refs/heads/${branch}`,
    detached: false,
    locked: false,
    lock_reason: null,
    checks: verdict === "keep" ? [{ id: "activity", status: "block", detail: "in use by sleep[2890]" }] : [],
    activity: null,
    ignored: [],
    local_only_commits: 0,
    local_only_commits_error: null,
    upstream: null,
    upstream_ref: null,
    upstream_gone: null,
    upstream_error: null,
    size: null,
    last_change: null,
    last_change_error: null,
  };
}

function trash(entry: string) {
  return {
    entry,
    entry_path: `${repo}/.git/residue-trash/${entry}`,
    worktree_path: `${repo}/.git/residue-trash/${entry}/wt1`,
    original_path: "/Users/me/src/wt1",
    branch: "refs/heads/b-wt1",
    head: "0073ccd1d4100cbc8776d72bac181a5fed0012a6",
    trashed_at: "2026-11-02T11:38:34Z",
    allocated_bytes: 8192,
    reclaimable_bytes: 8192,
    registered: true,
    error: null,
  };
}

function record(entry: string) {
  return {
    kind: "removed",
    entry,
    entry_path: `${repo}/.git/residue-removed/${entry}`,
    ref: `refs/residue/removed/${entry}`,
    state: "removed",
    original_path: "/Users/me/src/wt0",
    aside_path: null,
    branch: "refs/heads/b-wt0",
    head: "1111111111111111111111111111111111111111",
    upstream: null,
    reason: "trash_purge",
    by: "remove",
    removed_at: "2026-09-01T10:00:00Z",
    keep: "30d",
    keep_seconds: 2592000,
    allocated_bytes: 8192,
    reclaimable_bytes: 8192,
    not_kept: [],
    incomplete: false,
    back_error: null,
    residue_version: "0.0.12",
    error: null,
  };
}

const plan = {
  schema: "residue.remove/v1",
  version: "0.0.12",
  generated_at: "2026-11-10T11:38:35Z",
  as_of: "2026-11-10T11:38:35Z",
  trash: { enabled: true, keep: "7d", keep_seconds: 604800 },
  removal_records: { reason: "remove", by: "remove", keep: "30d", keep_seconds: 2592000 },
  results: [
    { path: "/Users/me/src/wt2", result: "skipped", reason: "stdin is not a terminal", trash_entry: null, removal_record: null, worktree: worktree("/Users/me/src/wt2", "b-wt2", "candidate") },
    { path: "/Users/me/src/wt3", result: "refused", reason: "one or more checks did not pass", trash_entry: null, removal_record: null, worktree: worktree("/Users/me/src/wt3", "b-wt3", "keep") },
  ],
  purged: [
    // In use while residue planned: kept now, judged again by the run.
    { result: "kept", reason: "one or more checks did not pass", entry: trash("wt1-20261110T113827Z"), removal_record: null, worktree: worktree(`${repo}/.git/residue-trash/wt1-20261110T113827Z/wt1`, "b-wt1", "keep") },
    { result: "skipped", reason: "not confirmed", entry: trash("wt4-20261101T100000Z"), removal_record: null, worktree: null },
  ],
  forgotten: [
    { result: "skipped", reason: "not confirmed", entry: record("wt0-20260901T100000Z") },
    { result: "kept", reason: "its filesystem gives no lock", entry: record("wt5-20260901T100000Z") },
  ],
  errors: [],
  warnings: [],
  stats: {},
} as unknown as RemoveDocument;

test("the dialog lists every trash entry and record the run may delete, kept ones too", () => {
  const lists = planLists(plan);
  assert.deepEqual(lists.ready.map((r) => r.path), ["/Users/me/src/wt2"]);
  assert.deepEqual(lists.kept.map((r) => r.path), ["/Users/me/src/wt3"]);
  assert.deepEqual(lists.purges.map((p) => p.entry.entry), ["wt4-20261101T100000Z"]);
  // Before, the dialog showed only `purges`, and the run deleted this one unlisted.
  assert.deepEqual(lists.mayPurge.map((p) => p.entry.entry), ["wt1-20261110T113827Z"]);
  assert.deepEqual(lists.forgets.map((f) => f.entry.entry), ["wt0-20260901T100000Z"]);
  assert.deepEqual(lists.mayForget.map((f) => f.entry.entry), ["wt5-20260901T100000Z"]);
  // Each entry of the plan is in exactly one list.
  assert.equal(lists.purges.length + lists.mayPurge.length, plan.purged.length);
  assert.equal(lists.forgets.length + lists.mayForget.length, plan.forgotten.length);
});

test("a confirm sends what the dialog showed: the worktrees it moves, as they were, and every entry", () => {
  const shown = shownOf(plan, { "/Users/me/src/wt2": "16777232:4242", "/Users/me/src/wt3": "16777232:4243" });
  assert.equal(shown.as_of, "2026-11-10T11:38:35Z");
  assert.deepEqual(shown.worktrees, [
    {
      path: "/Users/me/src/wt2",
      canonical_path: "/Users/me/src/wt2",
      branch: "refs/heads/b-wt2",
      head: "0073ccd1d4100cbc8776d72bac181a5fed0012a6",
      locked: false,
      folder: "16777232:4242",
    },
  ]);
  assert.deepEqual(
    shown.purges.map((t) => t.entry),
    ["wt1-20261110T113827Z", "wt4-20261101T100000Z"],
  );
  assert.deepEqual(
    shown.forgets.map((r) => r.entry),
    ["wt0-20260901T100000Z", "wt5-20260901T100000Z"],
  );
});

test("a worktree whose folder the app could not read is sent without one", () => {
  const shown = shownOf(plan, {});
  assert.equal(shown.worktrees[0]?.folder, null);
});
