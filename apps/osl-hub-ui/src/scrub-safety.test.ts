import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const scanner = readFileSync(new URL("../../osl-hub/src/privacy_scan.rs", import.meta.url), "utf8");
const categories = readFileSync(new URL("./scrub.ts", import.meta.url), "utf8");

describe("Scrub safety contract", () => {
  it("keeps Manual Scrub free, local, and review-first", () => {
    expect(source).toContain("FREE · THIS DEVICE ONLY");
    expect(source).toContain("Your messages never leave this device.");
    expect(source).toContain("data-scrub-finding");
    expect(source).toContain("select-all-scrub");
    expect(source).toContain("clear-scrub-selection");
    expect(source).toContain("Review selected");
    expect(source).toContain("Nothing is deleted by this build.");
    const privacyStart = source.indexOf("function privacySettingsContent");
    const privacyEnd = source.indexOf("function autoScrubAccountIds", privacyStart);
    const privacyUi = source.slice(privacyStart, privacyEnd);
    expect(privacyUi).toContain('id="privacy-export-input"');
    expect(privacyUi).not.toContain("if (!proActive)");
  });

  it("keeps AutoScrub Pro, reviewed, transport-gated, and readback-qualified", () => {
    expect(source).toContain("AutoScrub assistant");
    expect(source).toContain("PRO · TRANSPORT-GATED");
    expect(source).toContain("Final confirmation");
    expect(source).toContain("live-confirmed");
    expect(source).toContain("summarizeAutoScrubReceipt");
    expect(source).not.toContain("all removed");
    expect(source).toContain('let autoScrubPathId: AutoScrubProviderId = "gmail-web"');
    expect(source).toContain("Existing signed-in hosted session; no re-authentication");
    expect(source).toContain("Optional: use IMAP instead");
    expect(source).toContain('autoScrubPathId === "discord" ? "discord" : "telegram"');
    expect(categories).toContain("completeEditableReviewRequiredEveryBatch: true");
    expect(categories).toContain("finalConfirmationRequiredEveryBatch: true");
    expect(categories).toContain("narrowSemanticHostedPortAllowed: true");
  });

  it("requires confirmation and never simulates platform deletion", () => {
    expect(source).toContain('id="confirm-scrub-list"');
    expect(source).not.toMatch(/function confirmScrubSelection[\s\S]*?window\.confirm/);
    expect(source).toContain("Nothing is deleted by this build.");
    expect(source).toContain('id="autoscrub-final-confirmation"');
    expect(source).toContain("Only a provider readback can verify removal within its stated coverage.");
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
