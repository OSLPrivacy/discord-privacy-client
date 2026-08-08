import assert from "node:assert/strict";
import { statSync } from "node:fs";
import test from "node:test";
import {
  captureMessageDefaults,
  FIXED_WINDOW,
  SAVED_CHOICES,
  SAVED_MESSAGE_DEFAULTS,
  TITLE,
} from "./capture-message-defaults.mjs";

test("TASK0756 the Linux capture shows all four saved choices and their explanations", { timeout: 180_000 }, async () => {
  const { png, rendered, ink } = await captureMessageDefaults();

  assert.equal(rendered.title, TITLE);
  assert.equal(statSync(png).size > 5_000, true, "capture PNG is larger than a blank frame");

  // Four saved choices: the value that is checked, the words next to it, and
  // the "Saved" mark that says it is the stored one.
  assert.equal(rendered.savedTagCount, 4);
  assert.deepEqual(rendered.checkedValues, {
    timer: String(SAVED_MESSAGE_DEFAULTS.timerSeconds),
    "burn-scope": SAVED_MESSAGE_DEFAULTS.burnScope,
    "view-once-length": String(SAVED_MESSAGE_DEFAULTS.viewOnceLengthSeconds),
    writing: SAVED_MESSAGE_DEFAULTS.coverWriting,
  });
  assert.deepEqual(
    Object.fromEntries(SAVED_CHOICES.map(([control]) => [control, rendered.savedLabels[control]])),
    Object.fromEntries(SAVED_CHOICES.map(([control, , label]) => [control, label])),
  );

  // Four explanations, each a real sentence, each drawn on the image.
  for (const [control] of SAVED_CHOICES) {
    const words = rendered.explanations[control].trim().split(/\s+/u);
    assert.ok(words.length >= 12, `${control} explanation is only ${words.length} words`);
    assert.ok(ink[`why:${control}`].ink > 100, `${control} explanation has no drawn text`);
    assert.ok(ink[`saved:${control}`].ink > 30, `${control} saved choice has no drawn text`);
    assert.ok(ink[`legend:${control}`].ink > 100, `${control} heading has no drawn text`);
  }

  // Save and Reset are on the same fixed-size Linux screen, not below it.
  for (const name of [TITLE, "Save", "Reset"]) {
    assert.ok(ink[name].ink > 30, `${name} is missing from the image`);
    assert.ok(rendered.boxes[name].y + rendered.boxes[name].height <= FIXED_WINDOW.height, `${name} is off screen`);
  }
});
