import assert from "node:assert/strict";
import { test } from "node:test";
import { confirmedRoots, unconfirmedRoots } from "../src/lib/folders.ts";

test("suggested folders are not scanned until the user saves them", () => {
  const first = { roots: ["/Users/a/src", "/Users/a/Work"], saved: false };
  assert.deepEqual(confirmedRoots(first), []);
  assert.deepEqual(unconfirmedRoots(first), ["/Users/a/src", "/Users/a/Work"]);
});

test("saved folders are scanned, an empty list included", () => {
  assert.deepEqual(confirmedRoots({ roots: ["/Users/a/src"], saved: true }), ["/Users/a/src"]);
  assert.deepEqual(unconfirmedRoots({ roots: ["/Users/a/src"], saved: true }), []);
  assert.deepEqual(confirmedRoots({ roots: [], saved: true }), []);
});

test("no suggestion and nothing saved scans nothing", () => {
  assert.deepEqual(confirmedRoots({ roots: [], saved: false }), []);
  assert.deepEqual(unconfirmedRoots({ roots: [], saved: false }), []);
});

test("the same settings give the same array, so the scan key stays put", () => {
  const first = { roots: ["/Users/a/src"], saved: false };
  assert.equal(confirmedRoots(first), confirmedRoots({ ...first }));
});
