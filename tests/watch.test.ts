// What the window says about watch's fetch, by the version of the residue
// that runs it (src/lib/watch.ts). Up to residue 0.0.13 that fetch can also
// delete a tag, note or other ref, even one never pushed, and the window must
// say so.
import assert from "node:assert/strict";
import { test } from "node:test";
import { FETCH_PRUNES_LOCAL_UNTIL, fetchCaveat, fetchWords } from "../src/lib/watch.ts";
import type { WatchFetch } from "../src/lib/types.ts";

const every: WatchFetch = { mode: "every", value: "30m", default: true };
const unknown: WatchFetch = { mode: "unknown", since: "0.0.7", every: "30m" };

test("the caveat names --no-fetch, where it is set, and what the fetch can delete", () => {
  const caveat = fetchCaveat(every, "residue 0.0.12");
  assert.ok(caveat);
  assert.match(caveat, /--no-fetch from a terminal/);
  assert.match(caveat, /tag, note or other ref that a fetch refspec or pruneTags covers/);
  assert.match(caveat, /even one never pushed/);
  assert.match(caveat, new RegExp(`Up to residue ${FETCH_PRUNES_LOCAL_UNTIL.replaceAll(".", "\\.")}`));
});

test("a residue up to the last one that prunes local refs gets the caveat", () => {
  for (const v of ["residue 0.0.7", "residue 0.0.12", "residue 0.0.13", "residue 0.0.13-rc.1"]) {
    assert.ok(fetchCaveat(every, v), v);
  }
});

test("a newer residue does not", () => {
  for (const v of ["residue 0.0.14", "residue 0.1.0", "residue 1.0.0", "residue 0.0.20+build"]) {
    assert.equal(fetchCaveat(every, v), null, v);
  }
});

test("an unknown or unreadable version gets the caveat", () => {
  assert.ok(fetchCaveat(every, null));
  assert.ok(fetchCaveat(every, "residue"));
  assert.ok(fetchCaveat(every, "something else 0.0.14"));
  assert.ok(fetchCaveat(unknown, null));
});

test("no fetch, no caveat", () => {
  assert.equal(fetchCaveat({ mode: "off" }, "residue 0.0.12"), null);
  assert.equal(fetchCaveat({ mode: "never", since: "0.0.7", every: "30m" }, "residue 0.0.6"), null);
});

test("the fetch words carry the caveat only when it applies", () => {
  const old = fetchWords(every, "residue 0.0.12");
  assert.match(old, /--prune/);
  assert.match(old, /--no-fetch/);
  assert.doesNotMatch(old, /only remote-tracking/);
  assert.doesNotMatch(fetchWords(every, "residue 0.0.14"), /tag, note or other ref/);
  assert.doesNotMatch(fetchWords({ mode: "off" }, "residue 0.0.12"), /tag, note or other ref/);
});
