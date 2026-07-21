import { describe, expect, it, vi } from "vitest";
import {
  DISCORD_HOSTED_SCROLL_CAPABILITY,
  DiscordHostedScrollBridgeSession,
  type DiscordHostedDeleteOnlyCommandPort,
  type DiscordHostedScrubCommand,
} from "./scrub-discord-hosted-bridge";

const ownMessage = { id: "2", channelId: "10", correspondentId: "20", createdAtUnixMs: 1, contentFingerprint: "hash", authoredBySelf: true, retractable: true };

function port(handler: (command: DiscordHostedScrubCommand) => unknown): DiscordHostedDeleteOnlyCommandPort {
  return { invoke: vi.fn(async (command) => handler(command)) };
}

describe("hosted Discord delete-only bridge", () => {
  it("is disabled and live-only until a dedicated hosted command port is integrated", () => {
    expect(DISCORD_HOSTED_SCROLL_CAPABILITY).toMatchObject({
      enabledByDefault: false,
      verification: "live-account-required",
      boundaryImplemented: true,
      injectedCommandPortImplemented: false,
      integrationBlocked: { code: "hosted-delete-command-port-unavailable" },
      supportsGenericAutomation: false,
      supportsSyntheticInput: false,
    });
  });

  it("issues only fixed scoped scroll/read/delete/readback commands", async () => {
    const commands: DiscordHostedScrubCommand[] = [];
    const bridgePort = port((command) => {
      commands.push(command);
      if (command.operation === "scrollOwnHistory") return { ok: true, messages: [ownMessage], complete: true };
      if (command.operation === "inspectOwnMessage") return { ok: true, message: ownMessage, schemaVersion: "discord-hosted-2026-07" };
      if (command.operation === "deleteOwnMessage") return { ok: true, accepted: true };
      return { ok: true, absent: true };
    });
    const session = new DiscordHostedScrollBridgeSession(bridgePort, "account", ["10"]);
    expect(await session.loadNextOwnMessageHistory("10")).toMatchObject({ messages: [ownMessage], complete: true });
    expect(await session.inspectOwnMessage("10", "2")).toMatchObject({ message: ownMessage });
    expect(await session.deleteOwnMessage("10", "2")).toEqual({ accepted: true });
    expect(await session.verifyOwnMessageAbsent("10", "2")).toEqual({ absent: true });
    expect(commands[2]).toMatchObject({ operation: "deleteOwnMessage", accountId: "account", channelId: "10", messageId: "2", requireAuthoredBySelf: true });
    expect(Object.getOwnPropertyNames(DiscordHostedScrollBridgeSession.prototype).sort()).toEqual(["constructor", "deleteOwnMessage", "inspectOwnMessage", "loadNextOwnMessageHistory", "verifyOwnMessageAbsent"]);
  });

  it("never crosses the one-account fixed channel scope", async () => {
    const bridgePort = port(() => ({ ok: true, accepted: true }));
    const session = new DiscordHostedScrollBridgeSession(bridgePort, "account", ["10"]);
    expect(await session.deleteOwnMessage("11", "2")).toEqual({ accepted: false, friction: "unknown" });
    expect(bridgePort.invoke).not.toHaveBeenCalled();
  });

  it.each(["captcha", "challenge", "rate-signal", "dom-schema-drift", "unknown"] as const)("stops permanently on %s", async (friction) => {
    const bridgePort = port(() => ({ ok: false, friction }));
    const session = new DiscordHostedScrollBridgeSession(bridgePort, "account", ["10"]);
    expect(await session.loadNextOwnMessageHistory("10")).toMatchObject({ friction });
    expect(await session.deleteOwnMessage("10", "2")).toMatchObject({ accepted: false, friction });
    expect(bridgePort.invoke).toHaveBeenCalledTimes(1);
  });

  it("never trusts success-shaped payload fields on an unsuccessful envelope", async () => {
    const bridgePort = port(() => ({ ok: false, accepted: true, absent: true }));
    const session = new DiscordHostedScrollBridgeSession(bridgePort, "account", ["10"]);
    expect(await session.deleteOwnMessage("10", "2")).toEqual({ accepted: false, friction: "unknown" });
    expect(await session.verifyOwnMessageAbsent("10", "2")).toEqual({ absent: false, friction: "unknown" });
    expect(bridgePort.invoke).toHaveBeenCalledTimes(1);
  });

  it("treats malformed or ownership-ambiguous DOM records as permanent schema drift", async () => {
    const bridgePort = port(() => ({ ok: true, messages: [{ ...ownMessage, authoredBySelf: "maybe" }], complete: true }));
    const session = new DiscordHostedScrollBridgeSession(bridgePort, "account", ["10"]);
    expect(await session.loadNextOwnMessageHistory("10")).toMatchObject({ messages: [], friction: "dom-schema-drift" });
    expect(await session.verifyOwnMessageAbsent("10", "2")).toMatchObject({ friction: "dom-schema-drift" });
    expect(bridgePort.invoke).toHaveBeenCalledTimes(1);
  });
});
