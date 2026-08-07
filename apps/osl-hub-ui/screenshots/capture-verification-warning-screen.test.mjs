import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  CAPTURED_CHOICE,
  EXPECTED_EFFECT_TEXT,
  FIXED_VIEWPORT,
  REQUIRED_IMAGE_TEXT,
  REQUIRED_NAMES,
} from "./capture-verification-warning-screen.mjs";

// Read rather than imported: this runner is plain node, and the screen module
// is TypeScript that pulls in its own stylesheet.
const screenSource = readFileSync(new URL("../src/verification-warning-screen.ts", import.meta.url), "utf8");

test("TASK0748 pins the fixed verification warning viewport and screen tree names", () => {
  assert.deepEqual(FIXED_VIEWPORT, { width: 800, height: 620 });
  assert.deepEqual(REQUIRED_NAMES, [
    "Verification warning",
    "every time",
    "once",
    "before sending",
    "never",
    "Save",
    "Reset",
  ]);
  assert.equal(CAPTURED_CHOICE, "before sending");
});

test("TASK0748 requires the four choices, both controls, and the effect sentence in the image", () => {
  for (const name of ["Verification warning", "every time", "once", "before sending", "never", "Save", "Reset"]) {
    assert.ok(REQUIRED_IMAGE_TEXT.includes(name), `image text requirement missing: ${name}`);
  }
  assert.ok(REQUIRED_IMAGE_TEXT.includes(EXPECTED_EFFECT_TEXT), "the effect sentence is not required in the image");
});

// The capture asserts a hard-coded sentence; this keeps that sentence tied to
// the screen module instead of quietly drifting from it.
test("TASK0748 checks the same effect sentence the screen renders", () => {
  assert.ok(
    screenSource.includes(`"${CAPTURED_CHOICE}": "${EXPECTED_EFFECT_TEXT}"`),
    "the captured effect sentence is not the one the screen module renders",
  );
});
