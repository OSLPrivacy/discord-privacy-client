import { describe, expect, it, vi } from "vitest";
import {
  findingsFingerprint,
  planFingerprint,
  type DeleteFinding,
  type ScopePolicy,
  type ScrubDeleteAdapter,
} from "./scrub-delete-engine";
import { createScrubDryRunPreview, renderScrubDryRunPreview } from "./scrub-dryrun-view";

const now = 1_800_000_000_000;
const finding: DeleteFinding = {
  providerId: "imap",
  accountId: "mail",
  channelId: "sent",
  correspondentId: "recipient",
  itemId: "message-1",
  authoredBySelf: true,
  createdAtUnixMs: now - 60_000,
  contentFingerprint: "message-1-hash",
};
const policy: ScopePolicy = {
  providerId: finding.providerId,
  accountId: finding.accountId,
  itemIds: [finding.itemId],
  channelIds: [finding.channelId],
  protectedChannelIds: [],
  protectedCorrespondentIds: [],
  maxCount: 1,
  minAgeMs: 1_000,
};

function request(adapter: ScrubDeleteAdapter) {
  return {
    adapter,
    findings: [finding],
    approved: policy,
    requested: policy,
    consent: { id: "consent", planFingerprint: planFingerprint(policy), findingsFingerprint: findingsFingerprint([finding]), issuedAt: now - 1, expiresAt: now + 1 },
    stepUp: { providerId: finding.providerId, accountId: finding.accountId, authEpoch: "auth-1", authenticatedAt: now - 1, expiresAt: now + 1 },
    finalConfirmation: true,
    now,
  };
}

describe("Scrub dry-run preview", () => {
  it("shows the exact eligible items and never invokes the adapter delete spy", async () => {
    const adapter: ScrubDeleteAdapter = {
      enumerate: vi.fn(async () => []),
      inspect: vi.fn(),
      delete: vi.fn(),
      verify: vi.fn(),
    };

    const preview = await createScrubDryRunPreview(request(adapter));

    expect(adapter.delete).not.toHaveBeenCalled();
    expect(preview.receipt.dryRun).toBe(true);
    expect(preview.markup).toContain("Nothing has been deleted.");
    expect(preview.markup).toContain("message-1");
    expect(preview.markup).toContain("sent");
    expect(preview.markup).toContain("mail");
  });

  it("refuses to label a live receipt as a preview", () => {
    expect(() => renderScrubDryRunPreview({
      providerId: "imap", accountId: "mail", dryRun: false, consentId: "consent", startedAt: now, completedAt: now, stoppedFailClosed: false, items: [],
    })).toThrow("live deletion receipt");
  });
});
