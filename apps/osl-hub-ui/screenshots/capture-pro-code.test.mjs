import assert from "node:assert/strict";
import test from "node:test";

import {
  PRO_CODE_CAPTURE_WINDOW,
  validateProCodeCapture,
} from "./capture-pro-code.mjs";

function ax(role, name) {
  return { role: { value: role }, name: { value: name } };
}

function visible(name) {
  return { name, text: name, rect: { x: 10, y: 10, width: 120, height: 36 } };
}

function completeCapture() {
  return {
    axNodes: [
      ax("heading", "Enter Pro code"),
      ax("button", "Continue"),
      ax("button", "Skip"),
      ax("button", "Back"),
    ],
    visibleElements: [
      visible("Enter Pro code"),
      visible("Continue"),
      visible("Skip"),
      visible("Back"),
    ],
    png: {
      ...PRO_CODE_CAPTURE_WINDOW,
      bitDepth: 8,
      colorType: 6,
      sampleStride: 4,
      distinctColors: 32,
      bytes: 12_000,
    },
  };
}

test("TASK0366 accepts a Pro-code screen capture with the required title, controls, and nonblank PNG", () => {
  const checked = validateProCodeCapture(completeCapture());

  assert.deepEqual(Object.keys(checked.controls), ["Continue", "Skip", "Back"]);
  assert.equal(checked.controls.Continue.ax, true);
  assert.equal(checked.controls.Skip.image, true);
  assert.equal(checked.png.width, 1280);
  assert.equal(checked.png.height, 800);
  assert.equal(checked.png.distinctColors, 32);
});

test("TASK0366 rejects a capture whose screen tree is missing Back", () => {
  const capture = completeCapture();
  capture.axNodes = capture.axNodes.filter((node) => node.name.value !== "Back");

  assert.throws(
    () => validateProCodeCapture(capture),
    /screen tree is missing control Back/u,
  );
});

test("TASK0366 rejects a nearly blank screenshot", () => {
  const capture = completeCapture();
  capture.png.distinctColors = 4;

  assert.throws(
    () => validateProCodeCapture(capture),
    /PNG has too few distinct colors: 4/u,
  );
});
