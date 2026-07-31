import { describe, expect, it, vi } from "vitest";
import { runAttendedImapDeletionBatch, type ScrubDeletionEngine } from "./scrub-attended-imap-run";
import { buildScrubReviewList } from "./scrub-review-list";
import type { LocalPrivacyFinding } from "./adapters";

const baseFinding: LocalPrivacyFinding = {
  serviceId: "email",
  accountId: "account-a",
  conversationId: "thread-a",
  messageLocator: "https://mail.example.com/messages/1",
  authoredBySelf: true,
  createdAtUnixMs: null,
  category: "credential",
  confidence: 80,
  reason: "Review in context.",
  localPreview: "password",
  canRequestDelete: true,
  attachmentPath: null,
};

describe("attended IMAP scrub run", () => {
  it("scrub-attended-imap-run.ts wires executeDeletion (scrub-delete-engine.ts) for confirmed review rows only", async () => {
    const rows = buildScrubReviewList([
      baseFinding,
      { ...baseFinding, messageLocator: "https://mail.example.net/messages/2", localPreview: "token" },
    ]);
    const confirmed = new Set([rows[0].dedupKey]);
    const engine: ScrubDeletionEngine = {
      executeDeletion: vi.fn(async (request) => ({
        dedupKey: request.dedupKey,
        requested: true as const,
        verifiedDeleted: false as const,
        manualReviewRequired: true as const,
      })),
    };

    const result = await runAttendedImapDeletionBatch(rows, confirmed, engine);

    expect(result.attempted).toBe(1);
    expect(engine.executeDeletion).toHaveBeenCalledTimes(1);
    expect(engine.executeDeletion).toHaveBeenCalledWith({
      dedupKey: rows[0].dedupKey,
      serviceId: rows[0].serviceId,
      accountId: rows[0].accountId,
      logicalHost: rows[0].logicalHost,
      confirmed: true,
    });
  });

  it("rejects deletion receipts that claim verified provider deletion", async () => {
    const [row] = buildScrubReviewList([baseFinding]);
    const engine: ScrubDeletionEngine = {
      executeDeletion: vi.fn(async () => ({
        dedupKey: row.dedupKey,
        requested: true as const,
        verifiedDeleted: true as false,
        manualReviewRequired: true as const,
      })),
    };

    await expect(runAttendedImapDeletionBatch([row], new Set([row.dedupKey]), engine))
      .rejects.toThrow("invalid attended deletion receipt");
  });
});
