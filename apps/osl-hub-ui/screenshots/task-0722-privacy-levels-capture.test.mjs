import assert from "node:assert/strict";
import test from "node:test";

import {
  CAPTURE_WINDOW,
  LEVELS,
  LEVEL_EXPLANATIONS,
  LEVEL_LABELS,
  NAMED_CONTROLS,
  validatePrivacyLevelCapture,
} from "./task-0722-privacy-levels-capture.mjs";

const EFFECT_LINES = {
  basic: [
    "No warnings before you send.",
    "Public posts are not checked.",
    "Attachments are sent as they are.",
    "No cleanup reviews are offered.",
    "Nothing waits for a VPN.",
    "Protected contacts stay optional.",
  ],
  balanced: [
    "Warns you before a risky send.",
    "Public posts are not checked.",
    "Removes hidden details from attachments first.",
    "Offers a cleanup review every 30 days.",
    "Nothing waits for a VPN.",
    "Protected contacts stay optional.",
  ],
  maximum: [
    "Warns you before a risky send.",
    "Checks public posts before they go out.",
    "Removes hidden details from attachments first.",
    "Offers a cleanup review every 7 days.",
    "Risky actions wait until your VPN is on.",
    "Protected contacts stay optional.",
  ],
};

function completeCapture(level) {
  return {
    level,
    heading: "Privacy level",
    controls: Object.fromEntries(Object.keys(NAMED_CONTROLS).map((name) => [
      name,
      { present: true, rect: { x: 40, y: 40, width: 160, height: 32 } },
    ])),
    cards: LEVELS.map((id) => ({
      id,
      selected: id === level,
      label: LEVEL_LABELS[id],
      summary: LEVEL_EXPLANATIONS[id].summary,
      effectLines: EFFECT_LINES[id],
    })),
    checkedValues: [level],
    selectedMarkText: "Selected",
    axRadios: LEVELS.map((id) => ({
      name: `${LEVEL_LABELS[id]} ${LEVEL_EXPLANATIONS[id].summary}`,
      checked: id === level,
    })),
    png: { ...CAPTURE_WINDOW, distinctColors: 32, bytes: 90_000 },
    crops: {
      SelectedCard: { width: 380, height: 420, distinctColors: 24, nonDominant: 5_000 },
      SelectedEffects: { width: 340, height: 220, distinctColors: 12, nonDominant: 2_400 },
      SelectedMark: { width: 70, height: 24, distinctColors: 6, nonDominant: 220 },
    },
  };
}

test("TASK0722 accepts a complete capture of each selected level", () => {
  for (const level of LEVELS) {
    const checked = validatePrivacyLevelCapture(completeCapture(level));
    assert.equal(checked.selectedLabel, LEVEL_LABELS[level]);
    assert.equal(checked.summary, LEVEL_EXPLANATIONS[level].summary);
    assert.equal(checked.effectLines.length, 6);
    assert.equal(checked.png.width, 1280);
    assert.equal(checked.png.height, 800);
  }
});

for (const name of Object.keys(NAMED_CONTROLS)) {
  test(`TASK0722 rejects a screen copy missing named control "${name}"`, () => {
    const capture = completeCapture("balanced");
    delete capture.controls[name];

    assert.throws(
      () => validatePrivacyLevelCapture(capture),
      new RegExp(`missing named control "${name}"`, "u"),
    );
  });
}

test("TASK0722 rejects a capture whose selected card does not match the level", () => {
  const capture = completeCapture("basic");
  capture.cards = capture.cards.map((card) => ({ ...card, selected: card.id === "maximum" }));

  assert.throws(
    () => validatePrivacyLevelCapture(capture),
    /selected card is maximum, expected basic/u,
  );
});

test("TASK0722 rejects a capture missing the selected level's explanation", () => {
  const capture = completeCapture("maximum");
  capture.cards = capture.cards.map((card) =>
    card.id === "maximum" ? { ...card, summary: "" } : card,
  );

  assert.throws(
    () => validatePrivacyLevelCapture(capture),
    /maximum explanation summary is wrong/u,
  );
});

test("TASK0722 rejects a screenshot that is not the fixed window size", () => {
  const capture = completeCapture("balanced");
  capture.png = { ...capture.png, width: 1024 };

  assert.throws(
    () => validatePrivacyLevelCapture(capture),
    /PNG width is 1024, expected 1280/u,
  );
});

test("TASK0722 rejects blank selected-state pixels", () => {
  const capture = completeCapture("balanced");
  capture.crops.SelectedMark = { width: 70, height: 24, distinctColors: 1, nonDominant: 0 };

  assert.throws(
    () => validatePrivacyLevelCapture(capture),
    /SelectedMark pixels look blank/u,
  );
});
