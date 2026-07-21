import { describe, expect, it, vi } from "vitest";
import type { DeleteFinding } from "./scrub-delete-engine";
import { DiscordAssistedDeleteAdapter, PresenceGatedPacer, type HostedDiscordDeleteOnlySession } from "./scrub-discord-assisted";
const item: DeleteFinding = { providerId: "discord", accountId: "acct", channelId: "chat", correspondentId: "p", itemId: "1", authoredBySelf: true, createdAtUnixMs: 1, contentFingerprint: "h" };
function setup(friction?: "captcha" | "challenge" | "rate-signal" | "dom-schema-drift" | "unknown") {
  let clock = 100, present = true;
  const session: HostedDiscordDeleteOnlySession = { loadNextOwnMessageHistory: vi.fn(async () => ({ messages: [{ id: "1", channelId: "chat", correspondentId: "p", createdAtUnixMs: 1, contentFingerprint: "h", authoredBySelf: true, retractable: true }], complete: true, friction })), inspectOwnMessage: vi.fn(async () => ({ message: present ? { id: "1", channelId: "chat", correspondentId: "p", createdAtUnixMs: 1, contentFingerprint: "h", authoredBySelf: true, retractable: true } : null, schemaVersion: "v1", friction })), deleteOwnMessage: vi.fn(async () => { present = false; return { accepted: true, friction }; }), verifyOwnMessageAbsent: vi.fn(async () => ({ absent: !present, friction })) };
  const wait = vi.fn(async (_ms: number) => undefined), pacer = new PresenceGatedPacer({ fixedRestMs: 2_000, maxBatch: 10, presenceTtlMs: 60_000, boundedAwayMs: 120_000, wait, clock: () => clock });
  const adapter = new DiscordAssistedDeleteAdapter(session, "acct", "a1", pacer);
  return { adapter, pacer, session, wait, advance: (ms: number) => { clock += ms; } };
}
describe("Discord hosted assisted deletion", () => {
  it("paces honestly, scrolls own history, deletes own items, and verifies", async () => {
    const { adapter, pacer, session, wait } = setup(); pacer.signalHumanPresence();
    expect(await adapter.enumerate({ accountId: "acct", channelIds: ["chat"], beforeUnixMs: 2 })).toHaveLength(1);
    expect(await adapter.delete(item)).toMatchObject({ accepted: true });
    expect(await adapter.verify(item)).toMatchObject({ outcome: "confirmed-deleted" });
    expect(wait.mock.calls.every(([ms]) => ms === 2_000)).toBe(true);
    expect(session.deleteOwnMessage).toHaveBeenCalledWith("chat", "1");
  });
  it.each(["captcha", "challenge", "rate-signal", "dom-schema-drift", "unknown"] as const)("stops immediately and permanently on %s", async (friction) => {
    const { adapter, pacer, session } = setup(friction); pacer.signalHumanPresence();
    expect(await adapter.enumerate({ accountId: "acct", channelIds: ["chat"], beforeUnixMs: 2 })).toEqual([]);
    expect(await adapter.delete(item)).toMatchObject({ accepted: false });
    expect(session.deleteOwnMessage).not.toHaveBeenCalled();
  });
  it("parks without a current genuine presence signal and after a small batch", async () => {
    const { adapter, pacer, session } = setup();
    expect(await adapter.delete(item)).toMatchObject({ accepted: false });
    pacer.signalHumanPresence();
    for (let index = 0; index < 10; index += 1) await adapter.inspect(item);
    expect((await adapter.inspect(item)).state).toBe("unknown");
    expect(session.inspectOwnMessage).toHaveBeenCalledTimes(10);
  });
  it("serializes concurrent pacing and does not let repeated presence reset an active batch", async () => {
    let releaseWait!: () => void;
    const pendingWait = new Promise<void>((resolve) => { releaseWait = resolve; });
    let clock = 100;
    const wait = vi.fn(async () => pendingWait);
    const pacer = new PresenceGatedPacer({ fixedRestMs: 2_000, maxBatch: 1, presenceTtlMs: 60_000, boundedAwayMs: 120_000, wait, clock: () => clock });
    pacer.signalHumanPresence();
    pacer.signalHumanPresence();
    const first = pacer.beforeAction();
    const second = pacer.beforeAction();
    await Promise.resolve();
    expect(wait).toHaveBeenCalledTimes(1);
    releaseWait();
    await expect(first).resolves.toBe("ready");
    await expect(second).resolves.toBe("parked");
    pacer.signalHumanPresence();
    clock += 1;
    const next = pacer.beforeAction();
    await expect(next).resolves.toBe("ready");
  });
  it("has no send or generic automation capability and rejects other people's items", async () => {
    const { adapter, pacer, session } = setup(); pacer.signalHumanPresence();
    expect(Object.getOwnPropertyNames(DiscordAssistedDeleteAdapter.prototype).sort()).toEqual(["constructor", "delete", "enumerate", "inspect", "verify"]);
    expect(await adapter.delete({ ...item, authoredBySelf: false })).toMatchObject({ accepted: false });
    expect(await adapter.delete({ ...item, accountId: "other" })).toMatchObject({ accepted: false });
    expect(session.deleteOwnMessage).not.toHaveBeenCalled();
    // @ts-expect-error no generic scripting capability is allowed
    expect(adapter.executeScript).toBeUndefined();
  });
});
