import { describe, expect, it } from "vitest";
import type { LocalPrivacyFinding, PrivacyRiskCategory } from "./adapters";
import { defaultScrubSignalGroups, enabledScrubFindings, parseScrubSignalGroups, scrubDeletionAllowed, scrubDeletionContract, scrubSignalGroupFor } from "./scrub";

function finding(category: PrivacyRiskCategory): LocalPrivacyFinding {
  return {
    serviceId: "instagram",
    accountId: "local-export",
    conversationId: "chat-1",
    messageLocator: `message-${category}`,
    authoredBySelf: true,
    createdAtUnixMs: null,
    category,
    confidence: 70,
    reason: "Review in context.",
    localPreview: "Local preview",
    canRequestDelete: true,
    attachmentPath: null,
  };
}

describe("local Scrub category preferences", () => {
  it("defaults every category on but preserves an explicit empty selection", () => {
    expect([...parseScrubSignalGroups(null)]).toEqual(defaultScrubSignalGroups);
    expect([...parseScrubSignalGroups("[]")]).toEqual([]);
    expect([...parseScrubSignalGroups('["work","personal"]')]).toEqual(["work", "personal"]);
  });

  it("fails closed to known defaults for malformed or unknown stored values", () => {
    expect([...parseScrubSignalGroups("not json")]).toEqual(defaultScrubSignalGroups);
    expect([...parseScrubSignalGroups('["work","unknown"]')]).toEqual(defaultScrubSignalGroups);
  });

  it("groups broad review signals without making legal conclusions", () => {
    expect(scrubSignalGroupFor("credential")).toBe("personal");
    expect(scrubSignalGroupFor("sensitive_health")).toBe("personal");
    expect(scrubSignalGroupFor("potentially_unlawful_conduct")).toBe("conduct");
    expect(scrubSignalGroupFor("work_sensitive_information")).toBe("work");
  });

  it("shows only findings in locally enabled categories", () => {
    const findings = [finding("profanity"), finding("credential"), finding("work_sensitive_information")];
    expect(enabledScrubFindings(findings, new Set(["work", "personal"])).map((item) => item.category))
      .toEqual(["credential", "work_sensitive_information"]);
  });

  it("exports a fail-closed contract for future paced deletion adapters", () => {
    expect(scrubDeletionContract).toEqual({
      browserUiAutomationAllowed: false,
      privateApiAllowed: false,
      narrowSemanticHostedPortAllowed: true,
      documentedProviderDeleteApiAllowed: true,
      unattendedDeletionAllowed: false,
      completeEditableReviewRequiredEveryBatch: true,
      finalConfirmationRequiredEveryBatch: true,
      requestedDeletionCountsAsVerified: false,
      desktopUiAutomationAllowed: false,
      privateProviderApisAllowed: false,
      humanBehaviorMimicryAllowed: false,
      documentedProviderDeleteApiRequired: true,
      stopOn: ["captcha", "rate_limit", "challenge", "account_change", "schema_drift", "unknown", "content_mismatch", "verification_failure"],
    });
    expect(Object.isFrozen(scrubDeletionContract)).toBe(true);
  });

  it("rejects disabled, browser-automation, private-API, and incomplete-stop deletion paths", () => {
    const safe = {
      deletionEnabled: true,
      mechanism: "documented_provider_delete_api" as const,
      stopOn: scrubDeletionContract.stopOn,
      requestedDeletionCountsAsVerified: false,
    };
    expect(scrubDeletionAllowed(safe)).toBe(true);
    expect(scrubDeletionAllowed({ ...safe, mechanism: "hosted_semantic_delete_port" })).toBe(true);
    expect(scrubDeletionAllowed({ ...safe, deletionEnabled: false })).toBe(false);
    expect(scrubDeletionAllowed({ ...safe, mechanism: "browser_ui_automation" })).toBe(false);
    expect(scrubDeletionAllowed({ ...safe, mechanism: "private_api" })).toBe(false);
    expect(scrubDeletionAllowed({ ...safe, stopOn: ["rate_limit"] })).toBe(false);
    expect(scrubDeletionAllowed({ ...safe, requestedDeletionCountsAsVerified: true })).toBe(false);
  });
});
