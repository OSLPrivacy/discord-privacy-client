import { describe, expect, it } from "vitest";
import type { LocalPrivacyFinding, PrivacyRiskCategory } from "./adapters";
import { buildScrubReviewList, scrubReviewDedupKey } from "./scrub-review-list";

function finding(overrides: Partial<LocalPrivacyFinding> = {}): LocalPrivacyFinding {
  const category = (overrides.category ?? "credential") as PrivacyRiskCategory;
  return {
    serviceId: "email",
    accountId: "account-a",
    conversationId: "thread-a",
    messageLocator: "https://www.example.com/messages/1",
    authoredBySelf: true,
    createdAtUnixMs: 1_770_000_000,
    category,
    confidence: 85,
    reason: "Review in context.",
    localPreview: "shared password",
    canRequestDelete: true,
    ...overrides,
  };
}

describe("Scrub review list", () => {
  it("Define ScrubReviewRow dedup-key type", () => {
    const rows = buildScrubReviewList([finding()]);
    expect(rows).toHaveLength(1);
    expect(rows[0].dedupKey).toBe(scrubReviewDedupKey("email", "account-a", "example.com", "shared password"));
    expect(rows[0]).toMatchObject({
      serviceId: "email",
      accountId: "account-a",
      logicalHost: "example.com",
      findingCount: 1,
      signalGroups: ["personal"],
    });
  });

  it("buildScrubReviewList collapses alias/subdomain hosts into one logical-host row", () => {
    const rows = buildScrubReviewList([
      finding({ messageLocator: "https://www.example.com/messages/1", category: "credential" }),
      finding({ messageLocator: "https://m.example.com/messages/1", category: "work_sensitive_information", createdAtUnixMs: 1_770_000_020 }),
      finding({ messageLocator: "https://alerts.us.example.com/messages/1", category: "profanity", createdAtUnixMs: 1_770_000_010 }),
      finding({ messageLocator: "https://example.net/messages/1" }),
    ]);

    expect(rows).toHaveLength(2);
    const example = rows.find((row) => row.logicalHost === "example.com");
    expect(example?.findingCount).toBe(3);
    expect(example?.newestCreatedAtUnixMs).toBe(1_770_000_020);
    expect(example?.signalGroups).toEqual(["language", "personal", "work"]);
    expect(rows.find((row) => row.logicalHost === "example.net")?.findingCount).toBe(1);
  });
});
