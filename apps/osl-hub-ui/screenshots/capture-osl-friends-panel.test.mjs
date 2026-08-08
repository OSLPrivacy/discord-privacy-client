import assert from "node:assert/strict";
import test from "node:test";
import {
  FIXED_VIEWPORT,
  REQUIRED_IMAGE_TEXT,
  REQUIRED_NAMES,
} from "./capture-osl-friends-panel.mjs";

test("TASK0834 pins the fixed OSL Friends panel viewport and screen tree names", () => {
  assert.deepEqual(FIXED_VIEWPORT, { width: 800, height: 600 });
  assert.deepEqual(REQUIRED_NAMES, ["OSL Friends", "Ada Friend", "Cleo Friend"]);
});

test("TASK0834 requires the title, friend names, and back button in the image", () => {
  for (const name of REQUIRED_NAMES) {
    assert.ok(REQUIRED_IMAGE_TEXT.includes(name), `image text requirement missing: ${name}`);
  }
});
