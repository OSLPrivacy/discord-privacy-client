import { describe, expect, it, vi } from "vitest";

import {
  autoScrubTierStatus,
  runAttendedTieredScrub,
  type AutoScrubConsentAuthority,
} from "./autoscrub-tier";
import type {
  AutoScrubCapability,
  AutoScrubProviderBridge,
} from "./autoscrub-flow";
import type { DeleteFinding, ScopePolicy, ScrubDeleteAdapter } from "./scrub-delete-engine";

const now = 1_800_000_000_000;
const capability: AutoScrubCapability = { providerId: "discord", label: "Discord", liveConfirmed: true, coverage: "Service recheck" };
const finding: DeleteFinding = { providerId: "discord", accountId: "acct-1", channelId: "channel-1", correspondentId: "person-1", itemId: "message-1", authoredBySelf: true, createdAtUnixMs: now - 1_000, contentFingerprint: "hash-1" };
const policy: ScopePolicy = { providerId: "discord", accountId: "acct-1", itemIds: ["message-1"], channelIds: ["channel-1"], protectedChannelIds: [], protectedCorrespondentIds: [], maxCount: 1, minAgeMs: 0 };

function harness(stepUpAgeMs = 0) {
  const adapter: ScrubDeleteAdapter = {
    enumerate: vi.fn(async () => []),
    inspect: vi.fn(async () => ({ state: "present" as const, authoredBySelf: true, contentFingerprint: finding.contentFingerprint, authEpoch: "epoch-1", schemaVersion: "v1", retractable: true })),
    delete: vi.fn(async () => ({ accepted: true, authEpoch: "epoch-1" })),
    verify: vi.fn(async () => ({ outcome: "confirmed-deleted" as const, authEpoch: "epoch-1" })),
  };
  const bridge: AutoScrubProviderBridge = {
    capabilities: vi.fn(async () => [capability]),
    adapter: vi.fn(async () => adapter),
    stepUp: vi.fn(async () => ({ providerId: "discord", accountId: "acct-1", authEpoch: "epoch-1", authenticatedAt: now - stepUpAgeMs, expiresAt: now + 60_000 })),
  };
  const consentAuthority: AutoScrubConsentAuthority = {
    checkLiveConsent: vi.fn(async (serviceId) => ({ serviceId, state: "live" })),
  };
  return { adapter, bridge, consentAuthority };
}

function options(h: ReturnType<typeof harness>, overrides: Partial<Parameters<typeof runAttendedTieredScrub>[0]> = {}) {
  return {
    tier: "free" as const,
    optionalProModuleInstalled: false,
    providerRiskRank: 4 as const,
    consentAuthority: h.consentAuthority,
    target: { providerId: "discord" as const, accountId: "acct-1" },
    prepare: vi.fn(async () => ({ providerId: "discord" as const, accountId: "acct-1", findings: [finding], approved: policy, requested: policy })),
    capability,
    bridge: h.bridge,
    finalConfirmation: true,
    onDryRun: vi.fn(),
    now: () => now,
    ...overrides,
  };
}

describe("attended AutoScrub tiers", () => {
  it("keeps both labels honest and leaves the full attended free flow available without a Pro module", async () => {
    const free = autoScrubTierStatus("free", false);
    const pro = autoScrubTierStatus("pro", true);
    expect(free).toMatchObject({ label: "Attended Scrub", unattendedExecutionAllowed: false, requiresHumanPresence: true, optionalProModuleInstalled: false });
    expect(pro).toMatchObject({ label: "Attended AutoScrub", unattendedExecutionAllowed: false, requiresHumanPresence: true });

    const h = harness();
    await expect(runAttendedTieredScrub(options(h))).resolves.toMatchObject({ state: "completed" });
    expect(h.adapter.delete).toHaveBeenCalledOnce();
  });

  it("re-checks live, unrevoked rank-4 consent on every run before a fresh step-up", async () => {
    const h = harness();
    h.consentAuthority.checkLiveConsent = vi.fn(async (serviceId) => ({ serviceId, state: "revoked" }));
    await expect(runAttendedTieredScrub(options(h))).resolves.toEqual({ state: "refused", reason: "rank-4-live-consent-required" });
    expect(h.bridge.stepUp).not.toHaveBeenCalled();

    const stale = harness(300_001);
    await expect(runAttendedTieredScrub(options(stale))).rejects.toThrow("live-session proof is not fresh");
    expect(stale.consentAuthority.checkLiveConsent).toHaveBeenCalledWith("discord");
    expect(stale.bridge.stepUp).toHaveBeenCalledOnce();
    expect(stale.adapter.delete).not.toHaveBeenCalled();
  });
});
