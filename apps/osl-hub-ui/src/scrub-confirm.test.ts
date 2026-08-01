import { describe, expect, it, vi } from "vitest";
import { findingsFingerprint, planFingerprint, type DeleteFinding, type ExecutionConsent, type ScopePolicy, type StepUpProof } from "./scrub-delete-engine";
import { authorizeIrreversibleScrub, scrubConfirmationPhrase, type IrreversibleScrubRequest, type NativeConsentLedger } from "./scrub-confirm";

const now = 1_800_000_000_000;
const finding: DeleteFinding = { providerId: "imap", accountId: "mail", channelId: "inbox", correspondentId: "me", itemId: "item-1", authoredBySelf: true, createdAtUnixMs: now - 1_000, contentFingerprint: "content-1" };
const approved: ScopePolicy = { providerId: "imap", accountId: "mail", itemIds: [finding.itemId], channelIds: [finding.channelId], protectedChannelIds: [], protectedCorrespondentIds: [], maxCount: 1, minAgeMs: 0 };
const consent: ExecutionConsent = { id: "grant-1", planFingerprint: planFingerprint(approved), findingsFingerprint: findingsFingerprint([finding]), issuedAt: now - 1, expiresAt: now + 1 };
const stepUp: StepUpProof = { providerId: "imap", accountId: "mail", authEpoch: "epoch-1", authenticatedAt: now - 1, expiresAt: now + 1 };

function request(overrides: Partial<IrreversibleScrubRequest> = {}): IrreversibleScrubRequest {
  const base = { approved, findings: [finding], consent, stepUp, now };
  return { ...base, confirmation: scrubConfirmationPhrase(base.approved, base.findings), ...overrides };
}

function ledger(): NativeConsentLedger {
  const grants = new Set([consent.id]);
  return { consume: vi.fn(async (id) => grants.delete(id)) };
}

describe("irreversible Scrub confirmation", () => {
  it("refuses incorrect typed confirmation before spending consent", async () => {
    const nativeLedger = ledger();
    const result = await authorizeIrreversibleScrub(request({ confirmation: scrubConfirmationPhrase(approved, [finding]).toLocaleLowerCase() }), nativeLedger);
    expect(result).toEqual({ authorized: false, reason: "typed-confirmation" });
    expect(nativeLedger.consume).not.toHaveBeenCalled();
  });

  it("refuses a step-up older than five minutes before spending consent", async () => {
    const nativeLedger = ledger();
    const result = await authorizeIrreversibleScrub(request({ stepUp: { ...stepUp, authenticatedAt: now - 300_001 } }), nativeLedger);
    expect(result).toEqual({ authorized: false, reason: "step-up" });
    expect(nativeLedger.consume).not.toHaveBeenCalled();
  });

  it("spends the native consent grant once for the exact batch", async () => {
    const nativeLedger = ledger();
    expect(await authorizeIrreversibleScrub(request(), nativeLedger)).toMatchObject({ authorized: true });
    expect(await authorizeIrreversibleScrub(request(), nativeLedger)).toEqual({ authorized: false, reason: "consent" });
    expect(nativeLedger.consume).toHaveBeenCalledTimes(2);
    expect(nativeLedger.consume).toHaveBeenCalledWith(consent.id, { providerId: "imap", accountId: "mail", scope: `${planFingerprint(approved)}:${findingsFingerprint([finding])}` }, now);
  });
});
