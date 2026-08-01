import { describe, expect, it } from "vitest";
import { burnFeatureClaimsMarkup } from "./feature-claims";

const bannedPhrases = [
  "cryptographic burn",
  "disappears forever",
  "screenshot-proof",
  "screenshot detection",
  "self-destructing",
  "guaranteed deletion",
  "unsend",
  "recall",
];

function renderedText(markup: string): string {
  return markup.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

describe("T2-80 feature claims", () => {
  it("renders only allowlisted Burn wording", () => {
    const output = renderedText(burnFeatureClaimsMarkup());

    expect(output).toContain("Burn cleans up. It does not un-send.");
    expect(output).toContain("The service decides.");
    expect(output).toContain("undo screenshots, remove exports, erase backups, or stop a camera");
    for (const phrase of bannedPhrases) {
      expect(output.toLowerCase()).not.toContain(phrase);
    }
  });
});
