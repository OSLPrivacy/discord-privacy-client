import { describe, expect, it } from "vitest";
import { parseLocalPrivacyScan, type LocalPrivacyFinding, type LocalPrivacyScanResult, type PrivacyRiskCategory } from "./adapters";
import { buildScrubReviewList } from "./scrub-review-list";

const serviceLinkPattern = /\b(?:https?:\/\/|[a-z][a-z0-9+.-]*:\/\/)?(?:discord\.com|web\.telegram\.org|web\.whatsapp\.com|mail\.google\.com|mail\.proton\.me|mail\.yahoo\.com|mail\.aol\.com|www\.icloud\.com|signal\.org)\b/giu;
const jumpActionFieldPattern = /"(?:url|href|deepLink|messageUrl|originalMessageUrl|serviceUrl|openAction|openServiceAction)"\s*:/giu;

function finding(overrides: Partial<LocalPrivacyFinding> = {}): LocalPrivacyFinding {
  const category = (overrides.category ?? "credential") as PrivacyRiskCategory;
  return {
    serviceId: "discord",
    accountId: "rose-personal",
    conversationId: "dm-rose",
    messageLocator: "message-0001",
    authoredBySelf: true,
    createdAtUnixMs: 1_770_000_000,
    category,
    confidence: 91,
    reason: "Review this message in context.",
    localPreview: "shared password",
    canRequestDelete: true,
    attachmentPath: null,
    ...overrides,
  };
}

function searchResult(overrides: Partial<LocalPrivacyFinding> = {}): LocalPrivacyScanResult {
  return {
    findings: [finding(overrides)],
    messagesScanned: 1,
    messagesRejected: 0,
    emailProtectionChecks: [],
    truncated: false,
    analysisLocation: "this_device_only",
    persisted: false,
    attachmentsScanned: 0,
    imagesChecked: false,
    videosChecked: false,
    attachmentTypesScanned: [],
    uninspectedAttachments: [],
  };
}

function forbiddenLocationLeaks(value: string): string[] {
  return [
    ...value.matchAll(serviceLinkPattern),
    ...value.matchAll(jumpActionFieldPattern),
  ].map((match) => match[0]);
}

function renderedScreenData(result: LocalPrivacyScanResult): string {
  const groupedRows = buildScrubReviewList(result.findings).map((row) => [
    row.serviceId,
    row.accountId,
    row.logicalHost,
    row.findingCount,
    row.newestCreatedAtUnixMs,
    row.signalGroups.join(","),
    row.sample.reason,
    row.sample.localPreview,
    row.sample.conversationId,
    row.sample.messageLocator,
  ].join(" | "));
  return groupedRows.join("\n");
}

describe("result location privacy", () => {
  it("keeps service links out of search result output and rendered screen data", () => {
    const parsed = parseLocalPrivacyScan(searchResult());
    expect(parsed).not.toBeNull();

    const resultOutput = JSON.stringify(parsed);
    const screenData = renderedScreenData(parsed!);

    expect({
      resultOutput: forbiddenLocationLeaks(resultOutput),
      renderedScreenData: forbiddenLocationLeaks(screenData),
    }).toEqual({
      resultOutput: [],
      renderedScreenData: [],
    });
  });
});
