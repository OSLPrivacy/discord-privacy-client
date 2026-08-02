import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { projectAutoScrubFleetStatus } from "./autoscrub-contract";
import { scrubDeletionContract, scrubSignalDefinitions } from "./scrub";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const scanner = readFileSync(new URL("../../osl-hub/src/privacy_scan.rs", import.meta.url), "utf8");

function anchoredSource(start: string, end: string): string {
  const startIndex = source.indexOf(start);
  expect(startIndex, `missing source anchor: ${start}`).toBeGreaterThanOrEqual(0);
  const endIndex = source.indexOf(end, startIndex);
  expect(endIndex, `missing source anchor: ${end}`).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("Scrub safety contract", () => {
  it("keeps Manual Scrub free, local, review-first, and user deleted", () => {
    const privacyUi = anchoredSource("function privacySettingsContent", "function autoScrubAssistantMarkup");
    const privacyDestinationUi = anchoredSource("function privacyDestinationContent", "function massCleanupActionLabel");
    const scanResultsUi = anchoredSource("function privacyScanResultsMarkup", "function selectedScrubItems");

    expect(privacyUi).toContain('id="privacy-export-input"');
    expect(privacyDestinationUi).toContain("FREE · THIS DEVICE ONLY");
    expect(privacyUi).toContain("Your messages never leave this device.");
    expect(scanResultsUi).toContain("data-scrub-finding");
    expect(scanResultsUi).toContain("select-all-scrub");
    expect(scanResultsUi).toContain("clear-scrub-selection");
    expect(scanResultsUi).toContain("Review selected");
    expect(privacyUi).toContain("delete each message yourself");
    expect(privacyUi).not.toContain("if (!proActive)");
  });

  it("keeps AutoScrub Pro off and prevents unattended deletion claims", () => {
    const autoScrubUi = anchoredSource("function autoScrubAssistantMarkup", "function clearPrivacyScanState");

    expect(autoScrubUi).toContain("AutoScrub assistant");
    expect(autoScrubUi).toContain("PRO · COMING SOON");
    expect(autoScrubUi).toContain("Nothing happens until you review and confirm every batch.");
    expect(projectAutoScrubFleetStatus(null)).toMatchObject({
      label: "Unavailable in this build",
      stopAvailable: false,
    });
    expect(scrubDeletionContract).toMatchObject({
      unattendedDeletionAllowed: false,
      completeEditableReviewRequiredEveryBatch: true,
      finalConfirmationRequiredEveryBatch: true,
      requestedDeletionCountsAsVerified: false,
      browserUiAutomationAllowed: false,
      desktopUiAutomationAllowed: false,
      privateProviderApisAllowed: false,
      humanBehaviorMimicryAllowed: false,
      documentedProviderDeleteApiRequired: true,
    });
  });

  it("requires confirmation and never simulates platform deletion", () => {
    const reviewDialogUi = anchoredSource("function scrubReviewDialogMarkup", "function openScrubReviewDialogAfterRender");
    const bindControls = anchoredSource("function bindScrubControls", "function notificationSettingsContent");

    expect(reviewDialogUi).toContain('id="confirm-scrub-list"');
    expect(reviewDialogUi).toContain("Nothing is deleted by this build.");
    expect(reviewDialogUi).toContain("Confirming only prepares manual directions. It does not contact or change any app.");
    expect(bindControls).toContain('document.querySelector("#confirm-scrub-list")');
    expect(bindControls).not.toContain("window.confirm");
    expect(anchoredSource("function privacySettingsContent", "function autoScrubAssistantMarkup"))
      .toContain("This build only gives manual directions. It does not delete app messages.");
    expect(source).not.toContain("Platform messages deleted");
  });

  it("keeps matching deterministic and free of persistence, egress, or matched-content logging", () => {
    expect(scanner).toContain('analysis_location: "this_device_only"');
    expect(scanner).toContain("persisted: false");
    expect(scanner).not.toMatch(/reqwest|ureq|hyper::|std::fs|println!|dbg!|tracing::|log::/);
  });

  it("labels sensitive categories as review signals instead of verdicts", () => {
    expect(scrubSignalDefinitions.map(({ label }) => label)).toEqual([
      "Strong language",
      "Sexual content",
      "Personal information",
      "Drug-related messages",
      "Possible illegal activity",
      "Work and company secrets",
    ]);
    expect(scrubSignalDefinitions.find(({ id }) => id === "conduct")?.detail).toContain("not a legal judgment");
    expect(anchoredSource("function scrubCategoryChooserMarkup", "function previousSetupRoute"))
      .toContain("These are review reminders, not judgments.");
    expect(scanner).toContain("not a legal conclusion");
    expect(scanner).toContain("not a legal determination");
  });
});
