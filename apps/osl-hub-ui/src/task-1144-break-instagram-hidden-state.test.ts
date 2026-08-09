import { describe, expect, it } from "vitest";
import {
  pressInstagramEyeControl,
  type InstagramEyeFixtureRow,
  type InstagramEyeStateCommand,
} from "./instagram-eye-controls";

const marker = "instagram-row-1144";
const ordinaryText = "ordinary Instagram row 1144";
const protectedText = "protected Instagram row 1144";
const fixture: InstagramEyeFixtureRow[] = [
  {
    marker,
    ordinaryText,
    shownText: ordinaryText,
    marked: true,
    eye: "closed",
  },
];

function validProtectedResponse(request: string): Record<string, unknown> {
  const command = JSON.parse(request) as { marker: string; state: string };
  expect(command).toEqual({ marker, state: "protected" });
  return {
    ok: true,
    result: {
      marker,
      after: "protected",
      shownText: protectedText,
    },
  };
}

const goodShowCommand: InstagramEyeStateCommand = (request) =>
  JSON.stringify(validProtectedResponse(request));

const missingProtectedKeyCommand: InstagramEyeStateCommand = (request) => {
  const changed = validProtectedResponse(request) as {
    result: { shownText?: string };
  };
  delete changed.result.shownText;
  return JSON.stringify(changed);
};

const shownResults = (rows: readonly InstagramEyeFixtureRow[]) =>
  rows.filter((row) => row.eye === "open" && row.shownText === protectedText);

describe("TASK1144 break Instagram hidden state", () => {
  it("refuses a show result missing its protected text key without changing the good shown row", () => {
    const shown = pressInstagramEyeControl(fixture, marker, "eye", goodShowCommand);
    expect(shownResults(shown)).toHaveLength(1);
    expect(shownResults(shown).map((row) => row.marker)).toEqual([marker]);
    const exactGoodShownResult = JSON.stringify(shown);

    let refusal: unknown;
    try {
      pressInstagramEyeControl(shown, marker, "eye", missingProtectedKeyCommand);
    } catch (error) {
      refusal = error;
    }

    expect(refusal).toBeInstanceOf(Error);
    const refusedByName = (refusal as Error).message;
    expect(refusedByName).toContain("shownText");
    expect(JSON.stringify(shown)).toBe(exactGoodShownResult);
    expect(shownResults(shown)).toHaveLength(1);
    expect(shownResults(shown).map((row) => row.marker)).toEqual([marker]);

    console.info(`TASK1144 good_shown_result_count=${shownResults(shown).length} good_shown_result_markers=${shownResults(shown).map((row) => row.marker).join(",")}`);
    console.info(`TASK1144 changed_protected_key=shownText removed=true refused_by_name=shownText refusal=${JSON.stringify(refusedByName)}`);
    console.info(`TASK1144 after_refusal_shown_result_count=${shownResults(shown).length} after_refusal_shown_result_markers=${shownResults(shown).map((row) => row.marker).join(",")} exact_same_result=${JSON.stringify(shown) === exactGoodShownResult}`);
  });
});
