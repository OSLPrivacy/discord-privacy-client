import { describe, expect, it } from "vitest";
import {
  BAD_MESSAGE_CHOICES,
  BAD_MESSAGE_FINDING_RULES,
  EVERYTHING_ABOVE,
  MAX_PRIVATE_WORDS,
  badMessageSelectionRequest,
  chooseBadMessageChoice,
  everythingAboveChosen,
  initialWhatToFindState,
  parsePrivateWords,
  setPrivateWordsText,
  whatToFindBlock,
  whatToFindMarkup,
} from "./scrub-what-to-find";

const FIVE_RULES = [
  "passwords and codes",
  "personal details",
  "money details",
  "private words",
  "private pictures",
];

describe("what counts as a bad message", () => {
  it("offers the six TASK 1411 choices, five of which are finding rules", () => {
    expect(BAD_MESSAGE_CHOICES.map((choice) => choice.name)).toEqual([...FIVE_RULES, EVERYTHING_ABOVE]);
    expect(BAD_MESSAGE_FINDING_RULES).toEqual(FIVE_RULES);
    expect(BAD_MESSAGE_CHOICES.every((choice) => choice.explanation.length > 20)).toBe(true);
  });

  // The finish line: choosing Everything above saves all five finding rules.
  it("saves all five finding rules when Everything above is chosen", () => {
    const state = chooseBadMessageChoice(initialWhatToFindState(), EVERYTHING_ABOVE);
    const request = badMessageSelectionRequest("task-1413-run", state);

    expect(everythingAboveChosen(state)).toBe(true);
    expect(whatToFindBlock(state)).toBeNull();
    expect(request.ruleNames).toHaveLength(5);
    expect(request.ruleNames).toEqual(FIVE_RULES);
    expect(request.ruleNames).not.toContain(EVERYTHING_ABOVE);
    expect(request.matchTreatment).toBe("possible_match");
  });

  it("keeps private words on the same save", () => {
    const state = setPrivateWordsText(
      chooseBadMessageChoice(initialWhatToFindState(), EVERYTHING_ABOVE),
      "project bluebird\nMAPLE-4172",
    );
    const request = badMessageSelectionRequest("task-1413-run", state);
    expect(request.ruleNames).toEqual(FIVE_RULES);
    expect(request.privateWords).toEqual(["project bluebird", "MAPLE-4172"]);
  });

  it("unticks all five when Everything above is unticked", () => {
    const all = chooseBadMessageChoice(initialWhatToFindState(), EVERYTHING_ABOVE);
    const none = chooseBadMessageChoice(all, EVERYTHING_ABOVE);
    expect(none.rules).toEqual([]);
    expect(whatToFindBlock(none)).toBe("no-rule-chosen");
  });

  it("treats the five ticked one by one as Everything above", () => {
    const state = FIVE_RULES.reduce(
      (accumulator, rule) => chooseBadMessageChoice(accumulator, rule as (typeof BAD_MESSAGE_FINDING_RULES)[number]),
      initialWhatToFindState(),
    );
    expect(everythingAboveChosen(state)).toBe(true);
    expect(badMessageSelectionRequest("run", state).ruleNames).toEqual(FIVE_RULES);
  });

  it("saves only the rules that are ticked", () => {
    const state = chooseBadMessageChoice(
      chooseBadMessageChoice(initialWhatToFindState(), "money details"),
      "passwords and codes",
    );
    expect(badMessageSelectionRequest("run", state).ruleNames).toEqual([
      "passwords and codes",
      "money details",
    ]);
  });

  it("never sends an empty or repeated private word", () => {
    expect(parsePrivateWords("  \n\n , ,  \n")).toEqual([]);
    expect(parsePrivateWords("maple, ,MAPLE\nmaple")).toEqual(["maple"]);
    expect(parsePrivateWords("a\n".repeat(1).concat([..."bcdefghijklmnopqrstuvwxyz0123456789"].join("\n"))))
      .toHaveLength(MAX_PRIVATE_WORDS);
    expect(parsePrivateWords(`${"x".repeat(81)}\nkeep`)).toEqual(["keep"]);
  });

  it("blocks Continue until something is chosen", () => {
    const empty = initialWhatToFindState();
    expect(whatToFindBlock(empty)).toBe("no-rule-chosen");
    expect(whatToFindMarkup(empty)).toContain("data-what-to-find-continue disabled");
    expect(whatToFindMarkup(chooseBadMessageChoice(empty, "private words")))
      .not.toContain("data-what-to-find-continue disabled");
  });

  it("draws the six choices, the private words box, Back and Continue", () => {
    const markup = whatToFindMarkup(chooseBadMessageChoice(initialWhatToFindState(), EVERYTHING_ABOVE));
    for (const choice of BAD_MESSAGE_CHOICES) {
      expect(markup).toContain(`value="${choice.name}"`);
      expect(markup).toContain(choice.label);
      expect(markup).toContain(choice.explanation);
    }
    expect(markup.match(/type="checkbox"[^>]* checked/gu)).toHaveLength(6);
    expect(markup).toContain("data-private-words");
    expect(markup).toContain(">Back<");
    expect(markup).toContain("Continue");
    expect(markup).toContain("possible match");
  });

  it("escapes typed words rather than rendering them", () => {
    const state = setPrivateWordsText(initialWhatToFindState(), "<script>alert(1)</script>");
    expect(whatToFindMarkup(state)).not.toContain("<script>");
    expect(whatToFindMarkup(state)).toContain("&lt;script&gt;");
  });
});
