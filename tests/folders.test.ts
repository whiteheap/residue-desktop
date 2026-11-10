import assert from "node:assert/strict";
import { test } from "node:test";
import { confirmedRoots, stillSuggested, unconfirmedRoots } from "../src/lib/folders.ts";

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

test("a confirm saves only the shown suggestions a fresh read still suggests", () => {
  const shown = ["/Users/a/src", "/Users/a/Work"];
  // ~/Work was deleted after the window read the list.
  assert.deepEqual(stillSuggested(shown, { roots: ["/Users/a/src"], saved: false }), ["/Users/a/src"]);
  assert.deepEqual(stillSuggested(shown, { roots: shown, saved: false }), shown);
});

test("a confirm never saves a folder the user was not shown", () => {
  const fresh = { roots: ["/Users/a/src", "/Users/a/code"], saved: false };
  assert.deepEqual(stillSuggested(["/Users/a/src"], fresh), ["/Users/a/src"]);
  assert.deepEqual(stillSuggested(["/Users/a/Work"], fresh), []);
});

test("a confirm saves nothing once a list is saved", () => {
  assert.deepEqual(stillSuggested(["/Users/a/src"], { roots: ["/Users/a/src"], saved: true }), []);
});
