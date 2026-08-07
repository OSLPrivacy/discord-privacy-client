import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { CAPTURED_PAGES, REQUIRED_CONTROLS, WINDOW } from "./capture-home-top-bar.mjs";

// Read rather than imported: this runner is plain node and main.ts is
// TypeScript that pulls in its own stylesheet.
const mainSource = readFileSync(new URL("../src/main.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");

test("TASK0816 pins the five controls, their words, and the capture viewport", () => {
  assert.deepEqual(REQUIRED_CONTROLS.map((control) => control.id), ["logo", "friends", "notifications", "settings", "profile"]);
  assert.deepEqual(REQUIRED_CONTROLS.map((control) => control.label), ["Home", "Friends", "Notifications", "Settings", "Profile"]);
  assert.deepEqual(WINDOW, { width: 1280, height: 800 });
});

// The capture asserts hard-coded words; this keeps them tied to the bar the app
// renders instead of quietly drifting from it.
test("TASK0816 checks the same five words the top bar renders", () => {
  const start = mainSource.indexOf("export const homeTopBarControlLabels");
  assert.ok(start >= 0, "homeTopBarControlLabels should exist in main.ts");
  const table = mainSource.slice(start, mainSource.indexOf("};", start));
  for (const control of REQUIRED_CONTROLS) {
    assert.ok(table.includes(`${control.id}: "${control.label}"`), `main.ts does not label ${control.id} "${control.label}"`);
  }
});

test("TASK0816 visits a page for every control that can be current, and checks each one", () => {
  assert.deepEqual(CAPTURED_PAGES.map((page) => page.current), ["logo", "settings", "profile"]);
  // Every page after the first is reached by pressing a control in the bar, not
  // by setting state behind its back.
  assert.equal(CAPTURED_PAGES[0].click, null);
  for (const page of CAPTURED_PAGES.slice(1)) {
    assert.ok(REQUIRED_CONTROLS.some((control) => control.id === page.click), `${page.name} clicks an unknown control`);
  }
  assert.equal(new Set(CAPTURED_PAGES.map((page) => page.file)).size, CAPTURED_PAGES.length);
});

test("TASK0816 keeps the label styled so nothing can trim it", () => {
  assert.ok(/\.home-top-bar-label\s*\{[^}]*white-space:\s*nowrap/s.test(styles), "the label may wrap");
  assert.ok(!/\.home-top-bar-label\s*\{[^}]*text-overflow:\s*ellipsis/s.test(styles), "the label may ellipsise");
  assert.ok(!/\.home-top-bar-label\s*\{[^}]*overflow:\s*hidden/s.test(styles), "the label may be clipped by overflow");
});
