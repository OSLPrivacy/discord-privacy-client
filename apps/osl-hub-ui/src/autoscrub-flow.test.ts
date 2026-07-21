import { describe, expect, it, vi } from "vitest";
import {
  runAutoScrubBatch,
  summarizeAutoScrubReceipt,
  type AutoScrubCapability,
  type AutoScrubProviderBridge,
} from "./autoscrub-flow";
import type { DeleteFinding, ScopePolicy, ScrubDeleteAdapter } from "./scrub-delete-engine";

const now = 1_800_000_000_000;
const finding = (itemId = "message-1"): DeleteFinding => ({ providerId: "imap", accountId: "mail", channelId: "Sent", correspondentId: "person", itemId, authoredBySelf: true, createdAtUnixMs: now - 100_000, contentFingerprint: `hash-${itemId}` });
const policy = (overrides: Partial<ScopePolicy> = {}): ScopePolicy => ({ providerId: "imap", accountId: "mail", itemIds: ["message-1", "message-2"], channelIds: ["Sent"], protectedChannelIds: [], protectedCorrespondentIds: [], maxCount: 2, minAgeMs: 0, ...overrides });
const capability: AutoScrubCapability = { providerId: "imap", label: "Email (IMAP)", liveConfirmed: true, coverage: "Message-ID readback" };

function harness() {
  const order: string[] = [];
  const adapter: ScrubDeleteAdapter = {
    enumerate: vi.fn(async () => []),
    inspect: vi.fn(async (item: DeleteFinding) => { order.push(`inspect:${item.itemId}`); return { state: "present" as const, authoredBySelf: true, contentFingerprint: item.contentFingerprint, authEpoch: "epoch", schemaVersion: "imap-v1", retractable: true }; }),
    delete: vi.fn(async (item) => { order.push(`delete:${item.itemId}`); return { accepted: true, authEpoch: "epoch" }; }),
    verify: vi.fn(async (item) => { order.push(`verify:${item.itemId}`); return { outcome: item.itemId === "message-1" ? "confirmed-deleted" as const : "confirmed-not-deleted" as const, authEpoch: "epoch" }; }),
  };
  const bridge: AutoScrubProviderBridge = {
    capabilities: vi.fn(async () => [capability]),
    adapter: vi.fn(async (_providerId, _accountId, _findings, stepUp) => { order.push(`adapter:${stepUp.authEpoch}`); return adapter; }),
    stepUp: vi.fn(async () => { order.push("step-up"); return { providerId: "imap", accountId: "mail", authEpoch: "epoch", authenticatedAt: now, expiresAt: now + 60_000 }; }),
  };
  return { adapter, bridge, order };
}

const batch = (findings: readonly DeleteFinding[], approved: ScopePolicy, requested = approved) => ({ providerId: "imap" as const, accountId: "mail", findings, approved, requested });

describe("AutoScrub production flow", () => {
  it("requires an explicit final confirmation before re-auth or provider calls", async () => {
    const h = harness();
    const prepared = batch([finding()], policy({ itemIds: ["message-1"], maxCount: 1 }));
    await expect(runAutoScrubBatch({ target: { providerId: "imap", accountId: "mail" }, prepare: vi.fn(async () => prepared), capability, bridge: h.bridge, finalConfirmation: false, onDryRun: vi.fn(), now: () => now })).rejects.toThrow("explicit final confirmation");
    expect(h.order).toEqual([]);
  });

  it("performs fresh step-up and a surfaced safe dry-run before delete", async () => {
    const h = harness();
    const findings = [finding(), finding("message-2")];
    const onDryRun = vi.fn((receipt: import("./scrub-delete-engine").ProviderDeletionReceipt) => {
      h.order.push("preview");
      expect(receipt.items.every((item) => !item.deletionCalled && item.outcome === "UNKNOWN")).toBe(true);
      expect(h.adapter.delete).not.toHaveBeenCalled();
    });
    await runAutoScrubBatch({ target: { providerId: "imap", accountId: "mail" }, prepare: vi.fn(async (stepUp) => { h.order.push(`prepare:${stepUp.authEpoch}`); return batch(findings, policy()); }), capability, bridge: h.bridge, finalConfirmation: true, onDryRun, now: () => now });
    expect(h.order.slice(0, 5)).toEqual(["step-up", "prepare:epoch", "adapter:epoch", "preview", "inspect:message-1"]);
    expect(onDryRun).toHaveBeenCalledOnce();
  });

  it("accepts scope shrink but rejects expansion before provider calls", async () => {
    const h = harness();
    const findings = [finding(), finding("message-2")];
    const result = await runAutoScrubBatch({ target: { providerId: "imap", accountId: "mail" }, prepare: vi.fn(async () => batch(findings, policy(), policy({ itemIds: ["message-1"], maxCount: 1 }))), capability, bridge: h.bridge, finalConfirmation: true, onDryRun: vi.fn(), now: () => now });
    expect(result.execution.items.map((item) => item.itemId)).toEqual(["message-1"]);

    const expanded = harness();
    await expect(runAutoScrubBatch({ target: { providerId: "imap", accountId: "mail" }, prepare: vi.fn(async () => batch(findings, policy({ itemIds: ["message-1"], maxCount: 1 }), policy())), capability, bridge: expanded.bridge, finalConfirmation: true, onDryRun: vi.fn(), now: () => now })).rejects.toThrow("scope may only shrink");
    expect(expanded.order).toEqual(["step-up"]);
  });

  it("reports tri-state readback honestly without claiming everything was removed", async () => {
    const h = harness();
    const findings = [finding(), finding("message-2")];
    const result = await runAutoScrubBatch({ target: { providerId: "imap", accountId: "mail" }, prepare: vi.fn(async () => batch(findings, policy())), capability, bridge: h.bridge, finalConfirmation: true, onDryRun: vi.fn(), now: () => now });
    const summary = summarizeAutoScrubReceipt(result.execution);
    expect(summary).toMatchObject({ heading: "Verified within stated coverage", verifiedDeleted: 1, confirmedPresent: 1, unknown: 0 });
    expect(`${summary.heading} ${summary.detail}`.toLowerCase()).not.toContain("all removed");
  });

  it("keeps unavailable providers fail-closed", async () => {
    const h = harness();
    await expect(runAutoScrubBatch({ target: { providerId: "imap", accountId: "mail" }, prepare: vi.fn(async () => batch([finding()], policy())), capability: { ...capability, liveConfirmed: false }, bridge: h.bridge, finalConfirmation: true, onDryRun: vi.fn(), now: () => now })).rejects.toThrow("not live-confirmed");
    expect(h.order).toEqual([]);
  });
});
