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
    const onboarding = functionSource("scrubSetupContent", "scrubAccountSelections");
    const settings = functionSource("privacySettingsContent", "privacyScanResultsMarkup");
    expect(onboarding).toContain("This device only.");
    expect(onboarding).toContain("Nothing is deleted without explicit confirmation.");
    expect(onboarding).not.toContain("privacy-export-input");
    expect(settings).toMatch(/return `<h2>Scrub<\/h2><p class="scrub-local-promise"><strong>Your messages and attachments never leave this device\.<\/strong>[\s\S]*\$\{scanActions\}/);
  });

  it("keeps categories collapsed in onboarding and all six default on", () => {
    expect(source).toContain("Change what OSL looks for");
    expect(source).toContain("All categories start on.");
    for (const label of ["Strong language", "Sexual content", "Personal information", "Drug-related messages", "Possible illegal activity", "Work and company secrets"]) {
      expect(categories).toContain(label);
    }
    expect(categories).toContain("scrubSignalDefinitions.map(({ id }) => id)");
  });

  it("records a setup plan without initializing or scanning", () => {
    const onboarding = functionSource("scrubSetupContent", "scrubAccountSelections");
    expect(onboarding).toContain('data-scrub-mode="${mode}"');
    expect(onboarding).toContain('data-scrub-target="${escapeHtml(id)}"');
    expect(onboarding).toContain('id="finish-scrub-setup"');
    expect(onboarding).toContain('id="skip-scrub-setup"');
    expect(source).not.toContain("function onboardingScrubContent");
    expect(onboarding).not.toContain('id="initialize-scrub"');
  });

  it("states that later deletion retains explicit review and confirmation", () => {
    const onboarding = functionSource("scrubSetupContent", "scrubAccountSelections");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(onboarding).toContain("Review before removing.");
    expect(onboarding).toContain("explicit confirmation");
    expect(binding).not.toContain("initializeOnboardingScrub");
    expect(binding).not.toContain("scanPrivacyExport");
    expect(binding).not.toContain('id="initialize-scrub"');
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
