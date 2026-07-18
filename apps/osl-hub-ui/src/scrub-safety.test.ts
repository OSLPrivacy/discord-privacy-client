import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const scanner = readFileSync(new URL("../../osl-hub/src/privacy_scan.rs", import.meta.url), "utf8");
const categories = readFileSync(new URL("./scrub.ts", import.meta.url), "utf8");

describe("Scrub safety contract", () => {
  it("keeps Manual Scrub free, local, review-first, and user deleted", () => {
    expect(source).toContain("FREE · THIS DEVICE ONLY");
    expect(source).toContain("Your messages never leave this device.");
    expect(source).toContain("data-scrub-finding");
    expect(source).toContain("select-all-scrub");
    expect(source).toContain("clear-scrub-selection");
    expect(source).toContain("Review selected");
    expect(source).toContain("delete each message yourself");
    const privacyStart = source.indexOf("function privacySettingsContent");
    const privacyEnd = source.indexOf("function privacyScanResultsMarkup", privacyStart);
    const privacyUi = source.slice(privacyStart, privacyEnd);
    expect(privacyUi).toContain('id="privacy-export-input"');
    expect(privacyUi).not.toContain("if (!proActive)");
  });

  it("keeps AutoScrub Pro off and prevents unattended deletion claims", () => {
    expect(source).toContain("AutoScrub assistant");
    expect(source).toContain("PRO · COMING SOON");
    expect(source).toContain("Nothing happens until you review and confirm every batch.");
    expect(source).toContain("Unavailable in this build");
    expect(categories).toContain("completeEditableReviewRequiredEveryBatch: true");
    expect(categories).toContain("finalConfirmationRequiredEveryBatch: true");
  });

  it("requires confirmation and never simulates platform deletion", () => {
    expect(source).toContain('id="confirm-scrub-list"');
    expect(source).not.toMatch(/function confirmScrubSelection[\s\S]*?window\.confirm/);
    expect(source).toContain("Nothing is deleted by this build.");
    expect(source).toContain("This build only gives manual directions. It does not delete app messages.");
    expect(source).not.toContain("Platform messages deleted");
  });

  it("keeps matching deterministic and free of persistence, egress, or matched-content logging", () => {
    expect(scanner).toContain('analysis_location: "this_device_only"');
    expect(scanner).toContain("persisted: false");
    expect(scanner).not.toMatch(/reqwest|ureq|hyper::|std::fs|println!|dbg!|tracing::|log::/);
  });

  it("labels sensitive categories as review signals instead of verdicts", () => {
    for (const label of ["Strong language", "Sexual content", "Personal information", "Drug-related messages", "Possible illegal activity", "Work and company secrets"]) {
      expect(categories).toContain(label);
    }
    expect(source).toContain("These are review reminders, not judgments.");
    expect(scanner).toContain("not a legal conclusion");
    expect(scanner).toContain("not a legal determination");
  });
});
