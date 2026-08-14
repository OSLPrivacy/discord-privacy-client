import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { decodePng, inkInRect } from "./capture-task-0750-verification-warning.mjs";
import { CAPTURED_CHOICE, FIXED_VIEWPORT, REQUIRED_NAMES } from "./capture-verification-warning-screen.mjs";

// The committed capture, re-read from disk. This target checks the IMAGE, not
// the run that made it: if the PNG is replaced by a blank one, or a label's
// rectangle stops having glyphs in it, these tests go red without Chrome.
const IMAGE_PATH = new URL("./evidence/task-0750-verification-warning-before-sending.png", import.meta.url);
const FACTS_PATH = new URL("./evidence/task-0750-verification-warning-before-sending.json", import.meta.url);

const png = readFileSync(IMAGE_PATH);
const facts = JSON.parse(readFileSync(FACTS_PATH, "utf8"));
const image = decodePng(png);

/** The exact strings TASK 0750 requires on the screen. */
const TASK_0750_NAMES = ["Verification warning", "every time", "once", "before sending", "never", "save", "reset"];

test("TASK0750 captured the before-sending screen at the fixed size", () => {
  assert.equal(facts.capturedChoice, "before sending");
  assert.equal(CAPTURED_CHOICE, "before sending");
  assert.deepEqual(facts.viewport, FIXED_VIEWPORT);
  assert.equal(image.width, FIXED_VIEWPORT.width);
  assert.equal(image.height, FIXED_VIEWPORT.height);
});

test("TASK0750 requires the title and all six controls", () => {
  for (const name of TASK_0750_NAMES) {
    const matched = REQUIRED_NAMES.find((required) => required.toLowerCase() === name.toLowerCase());
    assert.ok(matched, `TASK 0750 name not required by the capture: ${name}`);
    assert.ok(facts.labelsInImage[matched], `no measured rectangle for ${name}`);
  }
  assert.equal(Object.keys(facts.labelsInImage).length, 7);
});

test("TASK0750 every required name has ink inside the image", () => {
  for (const [name, measured] of Object.entries(facts.labelsInImage)) {
    const { rect } = measured;
    assert.ok(rect.x >= 0 && rect.y >= 0, `${name} starts outside the image at ${rect.x},${rect.y}`);
    assert.ok(rect.x + rect.width <= image.width, `${name} runs past the right edge`);
    assert.ok(rect.y + rect.height <= image.height, `${name} runs past the bottom edge`);

    const ink = inkInRect(image, rect);
    assert.ok(ink.inkPixels >= 12, `${name} is a flat block in the image: ${ink.inkPixels} ink pixels`);
    // The recorded count came from the same pixels; drift means the PNG and the
    // rectangles no longer describe the same capture.
    assert.equal(ink.inkPixels, measured.inkPixels, `${name} ink count drifted from the recorded capture`);
  }
});

test("TASK0750 the image is not blank or nearly blank", () => {
  const whole = inkInRect(image, { x: 0, y: 0, width: image.width, height: image.height });
  assert.ok(whole.inkPixels / whole.area >= 0.005, `only ${whole.inkPixels}/${whole.area} pixels differ from the background`);
  assert.ok(whole.distinctColors >= 50, `only ${whole.distinctColors} distinct colours`);
  assert.ok(png.length >= 10_000, `PNG is only ${png.length} bytes`);
});
