import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

import {
  burnNativeDiscordOverlayChat,
  getNativeDiscordOverlayState,
  openNativeDiscordOverlayText,
  prepareNativeDiscordOverlayText,
  sendNativeDiscordOverlayCarrier,
  setNativeDiscordOverlaySecurity,
} from "./native-overlay-adapter";

const state = {
  active: true,
  friendLabel: "Friend",
  scopeApproved: true,
  ttlSeconds: 3_600,
  decryptDisplayEnabled: true,
  viewOnceEnabled: true,
  attachmentsEnabled: true,
  discordMarkerAvailable: true,
  covertextEnabled: true,
} as const;

describe("native overlay narrow adapter", () => {
  beforeEach(() => mocks.invoke.mockReset());

  it("uses only token-free overlay commands and exact camelCase security args", async () => {
    mocks.invoke.mockResolvedValueOnce(state).mockResolvedValueOnce({ ...state, ttlSeconds: 86_400, decryptDisplayEnabled: false });
    await expect(getNativeDiscordOverlayState()).resolves.toEqual(state);
    await expect(setNativeDiscordOverlaySecurity(86_400, false)).resolves.toEqual({ ...state, ttlSeconds: 86_400, decryptDisplayEnabled: false });
    expect(mocks.invoke.mock.calls).toEqual([
      ["get_native_discord_overlay_state"],
      ["set_native_discord_overlay_security", { ttlSeconds: 86_400, decryptDisplayEnabled: false }],
    ]);
  });

  it("fails closed on untruthful send and receive responses", async () => {
    mocks.invoke
      .mockResolvedValueOnce({ expiresAt: 10, personToPersonE2ee: true, viewOnce: false, deliveredToOslInbox: false })
      .mockResolvedValueOnce({ messages: [], fetched: 65 });
    await expect(prepareNativeDiscordOverlayText("hello", false)).resolves.toBeNull();
    await expect(openNativeDiscordOverlayText()).resolves.toBeNull();
  });

  it("rejects malformed security and oversized plaintext before IPC", async () => {
    await expect(setNativeDiscordOverlaySecurity(99 as 3_600, true)).resolves.toBeNull();
    await expect(prepareNativeDiscordOverlayText("🙂".repeat(262_145), false)).resolves.toBeNull();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("accepts only a fixed-mode bounded aggregate carrier request and truthful receipt", async () => {
    const receipt = { placed: true, enterSent: true, status: "sent", mode: "compatibility", compatibilityDelayMs: 200 };
    mocks.invoke.mockResolvedValueOnce(receipt);
    await expect(sendNativeDiscordOverlayCarrier("compatibility", 5)).resolves.toEqual(receipt);
    expect(mocks.invoke).toHaveBeenCalledWith("send_native_discord_overlay_carrier", {
      mode: "compatibility",
      charsPerSecond: 5,
    });
    mocks.invoke.mockResolvedValueOnce({ ...receipt, enterSent: false });
    await expect(sendNativeDiscordOverlayCarrier("compatibility", 5)).resolves.toBeNull();
    await expect(sendNativeDiscordOverlayCarrier("atomic", 121)).resolves.toBeNull();
  });

  it("passes only bounded presentation metrics for shape-matched cover rows", async () => {
    const receipt = { placed: true, enterSent: true, status: "sent", mode: "atomic", compatibilityDelayMs: 167 };
    const layout = { contentWidthPx: 640, averageGraphemeWidthPx: 7.8, lineHeightPx: 20, zoom: 1, density: 1, padding: "shapeMatched", rowKind: "plainText" } as const;
    mocks.invoke.mockResolvedValueOnce(receipt);
    await expect(sendNativeDiscordOverlayCarrier("atomic", 6, layout)).resolves.toEqual(receipt);
    expect(mocks.invoke).toHaveBeenCalledWith("send_native_discord_overlay_carrier", {
      mode: "atomic", charsPerSecond: 6, layout,
    });
    await expect(sendNativeDiscordOverlayCarrier("atomic", 6, { ...layout, contentWidthPx: 0 })).resolves.toBeNull();
  });

  it("parses burn counts while refusing Discord-history or recipient-copy claims", async () => {
    const result = {
      rowsDestroyed: 0,
      channelsDestroyed: 1,
      whitelistEntriesRemoved: 0,
      localProtectedRowsDestroyed: 2,
      remoteBlobsDeleted: 3,
      remoteBlobDeletionsFailed: 1,
      localCleanupComplete: true,
      remoteCleanupComplete: false,
      discordHistoryDeleted: false,
      recipientCopiesDeleted: false,
    };
    mocks.invoke.mockResolvedValueOnce(result);
    await expect(burnNativeDiscordOverlayChat()).resolves.toEqual(result);
    expect(mocks.invoke).toHaveBeenCalledWith("burn_native_discord_overlay_chat");
    mocks.invoke.mockResolvedValueOnce({ ...result, discordHistoryDeleted: true });
    await expect(burnNativeDiscordOverlayChat()).resolves.toBeNull();
  });
});
