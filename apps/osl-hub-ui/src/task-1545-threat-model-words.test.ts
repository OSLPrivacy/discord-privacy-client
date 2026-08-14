import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  SELF_TEST_PROMISES,
  SELF_TEST_SAFE,
  actionSentences,
  pageText,
  selfTest,
  sentencesOf,
  servicePermissionPromises,
} from "./threat-model-permission-claims";

// The Scrub threat-model copy is read out of the page source rather than out of
// a render. main.ts on this lane cannot be imported: earlier lane commits left
// duplicate `coverInsertion`, `setupScreen` and `setupNavigation` declarations
// in it, so esbuild refuses the module. The copy under test is a static
// template literal, so reading it here reads exactly what the page shows.
const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(start: string, end: string): string {
  const startIndex = source.indexOf(start);
  expect(startIndex, `missing source anchor: ${start}`).toBeGreaterThanOrEqual(0);
  const endIndex = source.indexOf(end, startIndex);
  expect(endIndex, `missing source anchor: ${end}`).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

/** Drops `${...}` interpolations so only the words a reader sees are left. */
function staticCopy(block: string): string {
  return pageText(block.replace(/\$\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\}/gu, " "));
}

/** The "Before deleting anything" disclosure: the threat-model text itself. */
function disclosureSource(): string {
  const page = functionSource("function privacySettingsContent", "function autoScrubAssistantMarkup");
  const start = page.indexOf('<details class="safety-disclosure scrub-safety">');
  expect(start, "missing the threat-model disclosure").toBeGreaterThanOrEqual(0);
  const end = page.indexOf("</details>", start);
  expect(end).toBeGreaterThan(start);
  return page.slice(start, end);
}

/** Every block of Scrub-page copy the threat-model reader is shown. */
function threatModelPageText(): string {
  return [
    functionSource("function privacySettingsContent", "function autoScrubAssistantMarkup"),
    functionSource("function autoScrubAssistantMarkup", "function clearPrivacyScanState"),
    functionSource("function scrubCategoryChooserMarkup", "function previousSetupRoute"),
  ].map(staticCopy).join("\n");
}

describe("TASK 1545 - Scrub threat-model words", () => {
  it("states each limit and the approved-account responsibility plainly", () => {
    const disclosure = disclosureSource();
    const limits: Array<[string, string]> = [
      ["copies", "Scrub cannot undo copies."],
      ["screenshots", "Scrub cannot undo screenshots."],
      ["service-records", "Scrub cannot undo service records."],
      ["service-permission", "Scrub cannot guarantee service permission."],
    ];

    for (const [limit, plainly] of limits) {
      const after = disclosure.split(`data-scrub-limit="${limit}"`)[1] ?? "";
      expect(after, `limit ${limit} has no line of its own`).not.toEqual("");
      const text = pageText(after.slice(after.indexOf(">") + 1, after.indexOf("</li>")));
      expect(text, `limit ${limit} is not stated plainly`).toContain(plainly);
      console.info(`LIMIT ${limit}: ${text}`);
    }

    const after = disclosure.split('data-scrub-limit="approved-account"')[1] ?? "";
    expect(after, "the approved-account responsibility has no line of its own").not.toEqual("");
    const responsibility = pageText(after.slice(after.indexOf(">") + 1, after.indexOf("</p>")));
    expect(responsibility).toContain("Point Scrub only at an account you hold and are approved to use.");
    expect(responsibility).toContain("using an account you are not approved for is on you");
    console.info(`RESPONSIBILITY: ${responsibility}`);

    expect(staticCopy(disclosure))
      .toContain("You are responsible. Check the original app and delete each message yourself.");
  });

  it("promises no service will allow scanning or deletion", () => {
    const disclosure = staticCopy(disclosureSource());
    const page = threatModelPageText();
    const promises = servicePermissionPromises(page);

    console.info(`THREAT-MODEL SENTENCES: ${sentencesOf(page).length}`);
    console.info(`SCAN-OR-DELETE SENTENCES: ${actionSentences(page).length}`);
    console.info(`DISCLOSURE SENTENCES: ${sentencesOf(disclosure).length}`);
    console.info(`DISCLOSURE SCAN-OR-DELETE SENTENCES: ${actionSentences(disclosure).length}`);
    for (const promise of promises) console.error(`SERVICE_PERMISSION_PROMISE: ${promise}`);
    console.info(`SERVICE PERMISSION PROMISES: ${promises.length}`);

    expect(sentencesOf(disclosure).length).toBeGreaterThan(15);
    expect(actionSentences(disclosure).length).toBeGreaterThan(5);
    expect(servicePermissionPromises(disclosure)).toEqual([]);
    expect(promises).toEqual([]);
  });

  it("catches planted permission promises and keeps honest denials", () => {
    const { missed, falsePositives } = selfTest();
    console.info(`SELF-TEST CAUGHT: ${SELF_TEST_PROMISES.length - missed.length}/${SELF_TEST_PROMISES.length} promises`);
    console.info(`SELF-TEST KEPT: ${SELF_TEST_SAFE.length - falsePositives.length}/${SELF_TEST_SAFE.length} honest sentences`);
    for (const sentence of missed) console.error(`MISSED: ${sentence}`);
    for (const sentence of falsePositives) console.error(`FLAGGED HONEST: ${sentence}`);
    expect(missed).toEqual([]);
    expect(falsePositives).toEqual([]);
  });
});
