import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { initialBeforeSendChecks, onboardingBeforeSendMarkup } from "./onboarding-before-send";

const sourcePath = new URL("./onboarding-before-send.ts", import.meta.url);
const source = readFileSync(sourcePath, "utf8");

function check5076(markup: string): void {
  if (markup.includes("bs-segmented") || markup.includes("bs-segment")) {
    throw new Error("invented shape: segmented metadata control");
  }
  const selected = [...markup.matchAll(/<input[^>]*name="clean-files"[^>]*checked/gu)];
  if (selected.length !== 1) {
    throw new Error(`double selection: expected exactly one checked metadata option, got ${selected.length}`);
  }
}

describe("TASK 5076 markup check", () => {
  it("passes the real file and names each deliberately broken throwaway copy", () => {
    const realMarkup = onboardingBeforeSendMarkup(initialBeforeSendChecks());
    expect(() => check5076(realMarkup)).not.toThrow();

    const restoredSegmented = realMarkup
      .replaceAll("bs-choice-control", "bs-segmented")
      .replaceAll("bs-choice-row", "bs-segment")
      .replaceAll('type="checkbox"', 'type="radio"');
    expect(() => check5076(restoredSegmented)).toThrow("invented shape");
    try { check5076(restoredSegmented); } catch (error) { console.log(`throwaway segmented: ${(error as Error).message}`); }

    const twoSelected = realMarkup.replace('value="always"', 'value="always" checked');
    expect(() => check5076(twoSelected)).toThrow("double selection");
    try { check5076(twoSelected); } catch (error) { console.log(`throwaway double-selection: ${(error as Error).message}`); }
    console.log("real markup: pass");
  });
});
