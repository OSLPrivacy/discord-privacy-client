import { describe, expect, it, vi } from "vitest";
import { executeDeletion, findingsFingerprint, planFingerprint, type DeleteFinding, type ScopePolicy } from "./scrub-delete-engine";
import { CheckedHostedSessionPort, type HostedSessionCommand, type HostedSessionCommandChannel } from "./scrub-hosted-session-channel";
import { HostedSessionAssistedDeleteAdapter, HostedSessionPresenceGate } from "./scrub-hosted-session-assisted";
import { HOSTED_SESSION_PORT_METHODS, type HostedOwnItem, type HostedSessionDeleteOnlyPort } from "./scrub-hosted-session-port";

const now = 1_800_000_000_000;
const base = { ok: true, accountId: "mail", sessionEpoch: "session-1", schemaVersion: "gmail-web-ui-v1" } as const;
const own: HostedOwnItem = { id: "message-1", channelId: "sent", correspondentId: "person", createdAtUnixMs: now - 100_000, contentFingerprint: "ui-hash", authoredBySelf: true, retractable: true };

function fakePort(overrides: Partial<HostedSessionDeleteOnlyPort> = {}): HostedSessionDeleteOnlyPort {
  return {
    scrollHistory: vi.fn(async () => ({ ...base, complete: true })),
    listOwnItems: vi.fn(async () => ({ ...base, items: [own] })),
    deleteOwnItem: vi.fn(async () => ({ ...base, accepted: true })),
    verifyGone: vi.fn(async () => ({ ...base, gone: true, covered: true })),
    ...overrides,
  };
}

function harness(port = fakePort()) {
  const waits: number[] = [];
  const gate = new HostedSessionPresenceGate({ fixedRestMs: 1_500, presenceTtlMs: 30_000, maxBatch: 25, wait: async (ms) => { waits.push(ms); }, clock: () => now });
  gate.signalHumanPresence();
  const adapter = new HostedSessionAssistedDeleteAdapter({ providerId: "gmail-web", accountId: "mail", port }, "session-1", gate);
  return { adapter, port, waits, gate };
}

function finding(overrides: Partial<DeleteFinding> = {}): DeleteFinding {
  return { providerId: "gmail-web", accountId: "mail", channelId: "sent", correspondentId: "person", itemId: "message-1", authoredBySelf: true, createdAtUnixMs: own.createdAtUnixMs, contentFingerprint: own.contentFingerprint, ...overrides };
}

function policy(overrides: Partial<ScopePolicy> = {}): ScopePolicy {
  return { providerId: "gmail-web", accountId: "mail", itemIds: ["message-1"], channelIds: ["sent"], protectedChannelIds: [], protectedCorrespondentIds: [], maxCount: 1, minAgeMs: 0, ...overrides };
}

describe("hosted-session command port", () => {
  it("exposes exactly four semantic operations and cannot express send, post, react, join, eval, click, or input", () => {
    expect(HOSTED_SESSION_PORT_METHODS).toEqual(["deleteOwnItem", "listOwnItems", "scrollHistory", "verifyGone"]);
    expect(Object.getOwnPropertyNames(CheckedHostedSessionPort.prototype).sort()).toEqual(["constructor", ...HOSTED_SESSION_PORT_METHODS].sort());
    const typed = fakePort();
    // @ts-expect-error outbound messaging is absent from the capability boundary
    expect(typed.send).toBeUndefined();
    // @ts-expect-error arbitrary evaluation is absent from the capability boundary
    expect(typed.eval).toBeUndefined();
  });

  it("serializes only the fixed protocol and rejects non-owned or ambiguous replies", async () => {
    const commands: HostedSessionCommand[] = [];
    const channel: HostedSessionCommandChannel = { request: vi.fn(async (command) => { commands.push(command); return command.operation === "listOwnItems" ? { ...base, items: [own] } : command.operation === "scrollHistory" ? { ...base, complete: true } : command.operation === "deleteOwnItem" ? { ...base, accepted: true } : { ...base, gone: true, covered: true }; }) };
    const checked = new CheckedHostedSessionPort(channel);
    await checked.scrollHistory({ maxScrolls: 2, maxItems: 50, beforeUnixMs: now });
    await checked.listOwnItems();
    await checked.deleteOwnItem("message-1");
    await checked.verifyGone("message-1");
    expect(commands.map((command) => command.operation)).toEqual(["scrollHistory", "listOwnItems", "deleteOwnItem", "verifyGone"]);
    const hostile = new CheckedHostedSessionPort({ request: async () => ({ ...base, items: [{ ...own, authoredBySelf: false }] }) });
    await expect(hostile.listOwnItems()).rejects.toThrow("non-owned");
    const ambiguous = new CheckedHostedSessionPort({ request: async () => ({ ...base, gone: true, covered: false }) });
    await expect(ambiguous.verifyGone("message-1")).rejects.toThrow("readback");
  });
});

describe("hosted-session assisted delete state machine", () => {
  it("scrolls and lists only own items at fixed overt pacing", async () => {
    const malicious = { ...own, id: "other", authoredBySelf: false } as unknown as HostedOwnItem;
    const h = harness(fakePort({ listOwnItems: vi.fn(async () => ({ ...base, items: [own, malicious] })) }));
    const result = await h.adapter.enumerate({ accountId: "mail", channelIds: ["sent"], beforeUnixMs: now });
    expect(result.map((item) => item.itemId)).toEqual(["message-1"]);
    expect(h.waits).toEqual([1_500, 1_500]);
  });

  it("permanently stops and auto-parks on captcha, rate, schema, account, or unknown friction", async () => {
    for (const friction of ["captcha", "rate-limit", "schema-drift", "account-changed", "unknown"] as const) {
      const list = vi.fn(async () => ({ ...base, ok: false, friction, items: [] }));
      const port = fakePort({ listOwnItems: list });
      const h = harness(port);
      expect(await h.adapter.enumerate({ accountId: "mail", channelIds: ["sent"], beforeUnixMs: now })).toEqual([]);
      expect((await h.adapter.inspect(finding())).state).toBe("unknown");
      expect(list).toHaveBeenCalledTimes(1);
      expect(port.deleteOwnItem).not.toHaveBeenCalled();
    }
  });

  it("requires genuine presence and never calls the port while parked", async () => {
    const h = harness();
    h.adapter.park();
    expect(await h.adapter.enumerate({ accountId: "mail", channelIds: ["sent"], beforeUnixMs: now })).toEqual([]);
    expect(h.port.scrollHistory).not.toHaveBeenCalled();
  });

  it("keeps non-retractable items surface-only", async () => {
    const surfaceOnly = { ...own, retractable: false };
    const h = harness(fakePort({ listOwnItems: vi.fn(async () => ({ ...base, items: [surfaceOnly] })) }));
    const f = finding();
    const approved = policy();
    const result = await executeDeletion({ adapter: h.adapter, findings: [f], approved, requested: approved, consent: { id: "consent", planFingerprint: planFingerprint(approved), findingsFingerprint: findingsFingerprint([f]), issuedAt: now, expiresAt: now + 1 }, stepUp: { providerId: "gmail-web", accountId: "mail", authEpoch: "session-1", authenticatedAt: now, expiresAt: now + 1 }, finalConfirmation: true, dryRun: false, now });
    expect(result.items[0]).toMatchObject({ outcome: "confirmed-not-deleted", deletionCalled: false, verifiedByReadback: true });
    expect(result.items[0].detail).toContain("surface-only");
    expect(h.port.deleteOwnItem).not.toHaveBeenCalled();
  });

  it("preserves dry-run, scope-shrink, and three-state readback semantics", async () => {
    const h = harness();
    const f = finding(), approved = policy(), requested = policy();
    const common = { adapter: h.adapter, findings: [f], approved, requested, consent: { id: "consent", planFingerprint: planFingerprint(approved), findingsFingerprint: findingsFingerprint([f]), issuedAt: now, expiresAt: now + 1 }, stepUp: { providerId: "gmail-web", accountId: "mail", authEpoch: "session-1", authenticatedAt: now, expiresAt: now + 1 }, finalConfirmation: true, now };
    const dry = await executeDeletion({ ...common, dryRun: true });
    expect(dry.items[0]).toMatchObject({ outcome: "UNKNOWN", deletionCalled: false, verifiedByReadback: false });
    expect(h.port.deleteOwnItem).not.toHaveBeenCalled();
    const live = await executeDeletion({ ...common, dryRun: false });
    expect(live.items[0]).toMatchObject({ outcome: "confirmed-deleted", deletionCalled: true, verifiedByReadback: true });
    expect(h.port.verifyGone).toHaveBeenCalled();
    const expanded = policy({ channelIds: ["sent", "other"] });
    const blocked = await executeDeletion({ ...common, requested: expanded, dryRun: false });
    expect(blocked).toMatchObject({ stoppedFailClosed: true, items: [] });
  });
});
