import { describe, expect, it, vi } from "vitest";
import { executeDeletion, findingsFingerprint, planFingerprint, scopeOnlyAllows, type DeleteFinding, type ExecutionRequest, type ScopePolicy, type ScrubDeleteAdapter } from "./scrub-delete-engine";

const now = 1_800_000_000_000;
const finding = (itemId = "item-1"): DeleteFinding => ({ providerId: "imap", accountId: "mail", channelId: "inbox", correspondentId: "person", itemId, authoredBySelf: true, createdAtUnixMs: now - 100_000, contentFingerprint: `hash-${itemId}` });
const policy = (x: Partial<ScopePolicy> = {}): ScopePolicy => ({ providerId: "imap", accountId: "mail", itemIds: ["item-1", "item-2"], channelIds: ["inbox"], protectedChannelIds: [], protectedCorrespondentIds: [], maxCount: 2, minAgeMs: 10_000, ...x });
function adapter(): ScrubDeleteAdapter {
  return { enumerate: vi.fn(async () => []), inspect: vi.fn(async (f: DeleteFinding) => ({ state: "present" as const, authoredBySelf: true, contentFingerprint: f.contentFingerprint, authEpoch: "auth-1", schemaVersion: "v1", retractable: true })), delete: vi.fn(async () => ({ accepted: true, authEpoch: "auth-1" })), verify: vi.fn(async () => ({ outcome: "confirmed-deleted" as const, authEpoch: "auth-1" })) };
}
function request(x: Partial<ExecutionRequest> = {}): ExecutionRequest {
  const approved = policy(), findings = [finding(), finding("item-2")];
  return { adapter: adapter(), findings, approved, requested: approved, consent: { id: "consent", planFingerprint: planFingerprint(approved), findingsFingerprint: findingsFingerprint(findings), issuedAt: now - 1, expiresAt: now + 1 }, stepUp: { providerId: "imap", accountId: "mail", authEpoch: "auth-1", authenticatedAt: now - 1, expiresAt: now + 1 }, finalConfirmation: true, dryRun: false, now, ...x };
}
describe("delete engine", () => {
  it("has a delete-only adapter surface", () => {
    expect(Object.keys(adapter()).sort()).toEqual(["delete", "enumerate", "inspect", "verify"]);
    const typed = adapter();
    // @ts-expect-error sending is forbidden at the capability boundary
    expect(typed.send).toBeUndefined();
  });
  it("confirms only after readback", async () => {
    const result = await executeDeletion(request());
    expect(result.items).toHaveLength(2);
    expect(result.items[0]).toMatchObject({ outcome: "confirmed-deleted", deletionCalled: true, verifiedByReadback: true });
  });
  it("readbacks a rejected request before confirming the item remains", async () => {
    const a = adapter(); a.delete = vi.fn(async () => ({ accepted: false, authEpoch: "auth-1", detail: "rejected" })); a.verify = vi.fn(async () => ({ outcome: "confirmed-not-deleted" as const, authEpoch: "auth-1" }));
    const result = await executeDeletion(request({ adapter: a, findings: [finding()], approved: policy({ itemIds: ["item-1"], maxCount: 1 }), requested: policy({ itemIds: ["item-1"], maxCount: 1 }), consent: { id: "c", planFingerprint: planFingerprint(policy({ itemIds: ["item-1"], maxCount: 1 })), findingsFingerprint: findingsFingerprint([finding()]), issuedAt: now - 1, expiresAt: now + 1 } }));
    expect(result.items[0]).toMatchObject({ outcome: "confirmed-not-deleted", verifiedByReadback: true });
    expect(a.verify).toHaveBeenCalledOnce();
  });
  it("dry-run emits the same receipt shape with zero calls", async () => {
    const a = adapter(), result = await executeDeletion(request({ adapter: a, dryRun: true }));
    expect(result.items[0]).toMatchObject({ outcome: "UNKNOWN", deletionCalled: false, verifiedByReadback: false });
    expect(Object.values(a).every((fn) => vi.mocked(fn).mock.calls.length === 0)).toBe(true);
  });
  it("never retries UNKNOWN", async () => {
    const a = adapter(); a.verify = vi.fn(async () => ({ outcome: "UNKNOWN" as const, authEpoch: "auth-1" }));
    const prior = await executeDeletion(request({ adapter: a })), retry = adapter();
    const result = await executeDeletion(request({ adapter: retry, previousReceipts: [prior] }));
    expect(result.items[0]).toMatchObject({ outcome: "UNKNOWN", deletionCalled: false });
    expect(vi.mocked(retry.inspect).mock.calls).toHaveLength(0);
  });
  it("only shrinks scope and preserves protections", () => {
    const approved = policy({ protectedChannelIds: ["safe"], protectedCorrespondentIds: ["family"] });
    expect(scopeOnlyAllows(approved, policy({ itemIds: ["item-1"], maxCount: 1, minAgeMs: 20_000, protectedChannelIds: ["safe", "safer"], protectedCorrespondentIds: ["family"] }))).toBe(true);
    expect(scopeOnlyAllows(approved, policy({ channelIds: ["inbox", "extra"], protectedChannelIds: ["safe"], protectedCorrespondentIds: ["family"] }))).toBe(false);
    expect(scopeOnlyAllows(approved, policy({ protectedChannelIds: [], protectedCorrespondentIds: ["family"] }))).toBe(false);
  });
  it("fails closed before calls on consent, confirmation, or re-auth failure", async () => {
    for (const r of [request({ finalConfirmation: false }), request({ stepUp: { providerId: "imap", accountId: "mail", authEpoch: "auth-1", authenticatedAt: now - 400_000, expiresAt: now + 1 } }), request({ consent: { id: "c", planFingerprint: "changed", findingsFingerprint: "changed", issuedAt: now - 1, expiresAt: now + 1 } })]) {
      expect(await executeDeletion(r)).toMatchObject({ stoppedFailClosed: true, items: [] });
      expect(vi.mocked(r.adapter.inspect).mock.calls).toHaveLength(0);
    }
  });
  it("honors protected people, age, ownership, and max-count", async () => {
    const findings = [finding(), { ...finding("item-2"), correspondentId: "safe" }, { ...finding("item-3"), createdAtUnixMs: now - 1 }, { ...finding("item-4"), authoredBySelf: false }];
    const approved = policy({ itemIds: findings.map((f) => f.itemId), protectedCorrespondentIds: ["safe"], maxCount: 1 });
    const a = adapter(), result = await executeDeletion(request({ adapter: a, findings, approved, requested: approved, consent: { id: "c", planFingerprint: planFingerprint(approved), findingsFingerprint: findingsFingerprint(findings), issuedAt: now - 1, expiresAt: now + 1 } }));
    expect(result.items.map((x) => x.itemId)).toEqual(["item-1"]);
    expect(vi.mocked(a.delete)).toHaveBeenCalledTimes(1);
  });
});
