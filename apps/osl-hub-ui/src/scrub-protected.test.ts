import { describe, expect, it, vi } from "vitest";
import { executeDeletion, findingsFingerprint, planFingerprint, type DeleteFinding, type ScopePolicy, type ScrubDeleteAdapter } from "./scrub-delete-engine";
import { ScrubProtectionHistory } from "./scrub-protected";

const scope = (overrides: Partial<ScopePolicy> = {}): ScopePolicy => ({
  providerId: "imap",
  accountId: "account",
  itemIds: ["message-1"],
  channelIds: ["inbox"],
  protectedChannelIds: [],
  protectedCorrespondentIds: [],
  maxCount: 1,
  minAgeMs: 0,
  ...overrides,
});

describe("SCR-R4 protected sets", () => {
  it("preserves protected channels and correspondents when a later scope omits them", () => {
    const history = new ScrubProtectionHistory();
    const first = history.inherit(scope({ protectedChannelIds: ["family"], protectedCorrespondentIds: ["alex"] }));
    const later = history.inherit(scope());

    expect(first).toMatchObject({ protectedChannelIds: ["family"], protectedCorrespondentIds: ["alex"] });
    expect(later).toMatchObject({ protectedChannelIds: ["family"], protectedCorrespondentIds: ["alex"] });
  });

  it("only grows protections within a run", () => {
    const history = new ScrubProtectionHistory({ channelIds: ["receipts"], correspondentIds: ["sam"] });
    history.protect({ channelIds: ["family", "receipts"], correspondentIds: ["alex"] });

    expect(history.current()).toEqual({ channelIds: ["family", "receipts"], correspondentIds: ["alex", "sam"] });
    expect(history.inherit(scope())).toMatchObject({
      protectedChannelIds: ["family", "receipts"],
      protectedCorrespondentIds: ["alex", "sam"],
    });
  });

  it("keeps an inherited protected channel out of deletion", async () => {
    const protectedScope = new ScrubProtectionHistory().inherit(scope({ protectedChannelIds: ["inbox"] }));
    const finding: DeleteFinding = { providerId: "imap", accountId: "account", channelId: "inbox", correspondentId: "alex", itemId: "message-1", authoredBySelf: true, createdAtUnixMs: 1, contentFingerprint: "fingerprint" };
    const adapter: ScrubDeleteAdapter = {
      enumerate: vi.fn(async () => []),
      inspect: vi.fn(async () => ({ state: "present" as const, authoredBySelf: true, contentFingerprint: "fingerprint", authEpoch: "auth", schemaVersion: "v1", retractable: true })),
      delete: vi.fn(async () => ({ accepted: true, authEpoch: "auth" })),
      verify: vi.fn(async () => ({ outcome: "confirmed-deleted" as const, authEpoch: "auth" })),
    };

    const receipt = await executeDeletion({
      adapter,
      findings: [finding],
      approved: protectedScope,
      requested: protectedScope,
      consent: { id: "consent", planFingerprint: planFingerprint(protectedScope), findingsFingerprint: findingsFingerprint([finding]), issuedAt: 1, expiresAt: 3 },
      stepUp: { providerId: "imap", accountId: "account", authEpoch: "auth", authenticatedAt: 1, expiresAt: 3 },
      finalConfirmation: true,
      dryRun: false,
      now: 2,
    });

    expect(receipt.items).toEqual([]);
    expect(adapter.delete).not.toHaveBeenCalled();
  });
});
