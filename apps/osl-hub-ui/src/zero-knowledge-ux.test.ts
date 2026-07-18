import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const categories = readFileSync(new URL("./scrub.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start).toBeGreaterThanOrEqual(0);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("zero-knowledge Scrub review", () => {
  it("keeps first-run Scrub local-only without scanning or deleting", () => {
    const onboarding = functionSource("onboardingScrubContent", "bindOnboarding");
    const settings = functionSource("privacySettingsContent", "privacyScanResultsMarkup");
    expect(onboarding).toContain("This stays on your device.");
    expect(onboarding).toContain("Nothing is uploaded or deleted.");
    expect(onboarding).toContain("Stores only exports you choose and messages OSL already shows.");
    expect(onboarding).not.toContain("privacy-export-input");
    expect(settings).toMatch(/return `<h2>Scrub<\/h2><p class="scrub-local-promise"><strong>Your messages never leave this device\.<\/strong>[\s\S]*\$\{scanActions\}/);
  });

  it("keeps categories collapsed in onboarding and all six default on", () => {
    expect(source).toContain("Change what OSL looks for");
    expect(source).toContain("All categories start on.");
    for (const label of ["Strong language", "Sexual content", "Personal information", "Drug-related messages", "Possible illegal activity", "Work and company secrets"]) {
      expect(categories).toContain(label);
    }
    expect(categories).toContain("scrubSignalDefinitions.map(({ id }) => id)");
  });

  it("initializes only an encrypted local index without implying deletion", () => {
    const onboarding = functionSource("onboardingScrubContent", "bindOnboarding");
    expect(onboarding).toContain('id="initialize-scrub"');
    expect(onboarding).toContain("Private index created");
    expect(onboarding).toContain("waits for an explicit export or supported OSL-visible source");
    expect(onboarding).toContain("Nothing is deleted now.");
    expect(onboarding).not.toContain("Scanning…");
    expect(onboarding).not.toContain("Not now");
  });

  it("states that later deletion retains explicit review and confirmation", () => {
    const onboarding = functionSource("onboardingScrubContent", "bindOnboarding");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(onboarding).toContain("Nothing is deleted now.");
    expect(onboarding).toContain("editable list and your confirmation");
    expect(binding).not.toContain("scanPrivacyExport");
    expect(binding).not.toContain("bindScrubControls");
  });

  it("renders every selected item through a complete paged OSL-owned review", () => {
    const dialog = functionSource("scrubReviewDialogMarkup", "openScrubReviewDialogAfterRender");
    const binding = functionSource("bindScrubControls", "notificationSettingsContent");
    expect(dialog).toContain("selectedScrubItems()");
    expect(dialog).toContain("scrubReviewPageSize");
    const item = functionSource("scrubFindingMarkup", "selectedScrubItems");
    expect(item).toContain("data-scrub-review-finding");
    expect(dialog).toContain("Confirm this list");
    expect(binding).toContain('document.querySelector("#confirm-scrub-list")');
    expect(binding).not.toContain("window.confirm");
    expect(binding).not.toContain("window.alert");
  });

  it("drops scan and review state when the Scrub surface closes", () => {
    const clear = functionSource("clearPrivacyScanState", "privacyScanResultsMarkup");
    expect(clear).toContain("privacyScanResult = null");
    expect(clear).toContain("selectedScrubFindings.clear()");
    expect(clear).toContain("scrubReviewOpen = false");
    expect(source).toContain('if (settingsSection === "scrub" && next !== "scrub") clearPrivacyScanState()');
  });

  it("uses plain suggestion copy without free jump links or scoring jargon", () => {
    const results = functionSource("privacyScanResultsMarkup", "scrubFindingLabel");
    const item = functionSource("scrubFindingMarkup", "selectedScrubItems");
    expect(results).toContain("suggestions");
    expect(item).toContain("Why OSL showed this");
    expect(item).toContain("Where to find it");
    expect(item).toContain("Check that you sent this");
    for (const jargon of ["candidate", "confidence", "review signal", "search reference", "authorship unverified"]) {
      expect(`${results}${item}`.toLowerCase()).not.toContain(jargon);
    }
    expect(`${results}${item}`).not.toContain("href=");
  });
});

describe("simple Friends safety", () => {
  it("hides empty connected-account claims and keeps security collapsed", () => {
    const people = functionSource("peopleListMarkup", "peopleDialogMarkup");
    expect(people).not.toContain("Connected accounts");
    expect(people).not.toContain("None linked");
    expect(people).toContain("Security details");
    expect(people).toContain("Verification code");
  });

  it("keeps friend add simple without enabling encrypted chats", () => {
    const friends = functionSource("friendsDialogMarkup", "serviceContent");
    expect(friends).toContain("Paste their invite");
    expect(friends).toContain("Name them on this device");
    expect(friends).toContain("Encrypted chats stay off");
    expect(source).toContain("Friend added. Encrypted chats are still off.");
  });
});
