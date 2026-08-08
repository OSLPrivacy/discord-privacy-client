import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  firstTimedDeleteWarningMarkup,
  timedDeleteContinueAllowed,
  TIMED_DELETE_WARNING_FACTS,
} from "./onboarding-delete";

const warningBeforeAgreement = firstTimedDeleteWarningMarkup(false);
const warningAfterAgreement = firstTimedDeleteWarningMarkup(true);

function continueTag(markup: string): string {
  const tag = markup.match(/<button[^>]*id="continue-defaults-review"[^>]*>/u)?.[0];
  expect(tag, "the warning must render its Continue button").toBeDefined();
  return tag ?? "";
}

describe("TASK 3331 first timed-delete warning", () => {
  it("shows exactly the four plain factual sentences before the first timed delete", () => {
    expect(TIMED_DELETE_WARNING_FACTS).toEqual([
      "Timed delete removes the message from the other person's screen using the app's own delete.",
      "Most apps leave a “This message was deleted” mark.",
      "A screenshot they already took is gone forever from our reach.",
      "Email cannot be recalled at all.",
    ]);
    expect(warningBeforeAgreement).toContain("Before the first timed delete");
    expect(warningBeforeAgreement.match(/data-timed-delete-fact=/gu)).toHaveLength(4);
    for (const fact of TIMED_DELETE_WARNING_FACTS) {
      expect(warningBeforeAgreement.split(fact)).toHaveLength(2);
    }

    console.log(`TASK3331_FACT_COUNT=${TIMED_DELETE_WARNING_FACTS.length}`);
    console.log(`TASK3331_OTHER_PERSON_COPY=${TIMED_DELETE_WARNING_FACTS[0]}`);
    console.log(`TASK3331_DELETED_MESSAGE_MARK=${TIMED_DELETE_WARNING_FACTS[1]}`);
    console.log(`TASK3331_SCREENSHOT=${TIMED_DELETE_WARNING_FACTS[2]}`);
    console.log(`TASK3331_EMAIL=${TIMED_DELETE_WARNING_FACTS[3]}`);
  });

  it("keeps Continue unavailable before agreement and makes it available after", () => {
    const before = continueTag(warningBeforeAgreement);
    const after = continueTag(warningAfterAgreement);

    expect(warningBeforeAgreement).not.toContain('id="timed-delete-warning-agreement" type="checkbox" checked');
    expect(before).toMatch(/\sdisabled(?:\s|>)/u);
    expect(before).toContain('aria-disabled="true"');
    expect(timedDeleteContinueAllowed({ deleteDrafts: false, deleteOldMessages: true }, false)).toBe(false);

    expect(warningAfterAgreement).toContain('id="timed-delete-warning-agreement" type="checkbox" checked');
    expect(after).not.toMatch(/\sdisabled(?:\s|>)/u);
    expect(after).toContain('aria-disabled="false"');
    expect(timedDeleteContinueAllowed({ deleteDrafts: false, deleteOldMessages: true }, true)).toBe(true);

    console.log(`TASK3331_CONTINUE_BEFORE_AGREEMENT=${before.includes(" disabled") ? "unavailable" : "available"}`);
    console.log(`TASK3331_CONTINUE_AFTER_AGREEMENT=${after.includes(" disabled") ? "unavailable" : "available"}`);
  });

  it("maps every factual sentence to existing named proof files with unsupported count 0", () => {
    const repositoryRoot = resolve(import.meta.dirname, "../../..");
    const map = readFileSync(resolve(repositoryRoot, "proof/timed-delete-warning-claims.txt"), "utf8");
    const mappings = map.split(/\r?\n/u).filter((line) => /^FACT \d+ \|/u.test(line));
    expect(mappings).toHaveLength(TIMED_DELETE_WARNING_FACTS.length);

    const mappedFacts: string[] = [];
    for (const line of mappings) {
      const match = /^FACT \d+ \| (.+) \| proof: (.+)$/u.exec(line);
      expect(match, `invalid proof mapping: ${line}`).not.toBeNull();
      if (!match) continue;
      mappedFacts.push(match[1]);
      const proofFiles = match[2].split(", ");
      expect(proofFiles.length).toBeGreaterThan(0);
      for (const proofFile of proofFiles) {
        expect(existsSync(resolve(repositoryRoot, proofFile)), `${proofFile} must exist`).toBe(true);
      }
    }

    expect(mappedFacts).toEqual([...TIMED_DELETE_WARNING_FACTS]);
    expect(map).toMatch(/^unsupported count 0$/mu);
    console.log(`TASK3331_MAPPED_FACT_COUNT=${mappedFacts.length}`);
    console.log("TASK3331_UNSUPPORTED_COUNT=0");
  });
});
