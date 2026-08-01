import { describe, expect, it, vi } from "vitest";
import {
  findingsFingerprint,
  planFingerprint,
  type DeleteFinding,
  type ScopePolicy,
  type ScrubDeleteAdapter,
} from "./scrub-delete-engine";
import { executeScrubDryRun, type ScrubDryRunRequest } from "./scrub-engine-host";

const now = 1_800_000_000_000;
const finding: DeleteFinding = {
  providerId: "imap",
  accountId: "mail",
  channelId: "inbox",
  correspondentId: "person",
  itemId: "item-1",
  authoredBySelf: true,
  createdAtUnixMs: now - 100_000,
  contentFingerprint: "hash-item-1",
};
const policy: ScopePolicy = {
  providerId: "imap",
  accountId: "mail",
  itemIds: [finding.itemId],
  channelIds: [finding.channelId],
  protectedChannelIds: [],
  protectedCorrespondentIds: [],
  maxCount: 1,
  minAgeMs: 10_000,
};

function request(adapter: ScrubDeleteAdapter): ScrubDryRunRequest {
  return {
    adapter,
    findings: [finding],
    approved: policy,
    requested: policy,
    consent: {
      id: "consent",
      planFingerprint: planFingerprint(policy),
      findingsFingerprint: findingsFingerprint([finding]),
      issuedAt: now - 1,
      expiresAt: now + 1,
    },
    stepUp: {
      providerId: "imap",
      accountId: "mail",
      authEpoch: "auth-1",
      authenticatedAt: now - 1,
      expiresAt: now + 1,
    },
    finalConfirmation: true,
    now,
  };
}

describe("scrub engine host", () => {
  it("keeps the reachable engine dry-run only", async () => {
    const adapter: ScrubDeleteAdapter = {
      enumerate: vi.fn(async () => []),
      inspect: vi.fn(async () => ({ state: "present" as const, authoredBySelf: true, contentFingerprint: finding.contentFingerprint, authEpoch: "auth-1", schemaVersion: "v1", retractable: true })),
      delete: vi.fn(async () => ({ accepted: true, authEpoch: "auth-1" })),
      verify: vi.fn(async () => ({ outcome: "confirmed-deleted" as const, authEpoch: "auth-1" })),
    };

    const receipt = await executeScrubDryRun(request(adapter));

    expect(receipt).toMatchObject({ dryRun: true, stoppedFailClosed: false, items: [{ itemId: finding.itemId, outcome: "UNKNOWN", deletionCalled: false, verifiedByReadback: false }] });
    expect(adapter.delete).not.toHaveBeenCalled();
    expect(adapter.inspect).not.toHaveBeenCalled();
    expect(adapter.verify).not.toHaveBeenCalled();
  });

  it("does not expose a live-run argument", () => {
    const adapter = {} as ScrubDeleteAdapter;
    if (false) {
      // @ts-expect-error The host permanently selects dryRun until the live lane is enabled.
      executeScrubDryRun({ ...request(adapter), dryRun: false });
    }
  });
});
