import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

import {
  burnNativeDiscordOverlayChat,
  getNativeDiscordOverlayQaDiagnostic,
  getNativeDiscordOverlayState,
  openNativeDiscordOverlayText,
  prepareNativeDiscordOverlayText,
  sendNativeDiscordQaAtomicText,
  sendNativeDiscordOverlayCarrier,
  setNativeDiscordOverlaySecurity,
} from "./native-overlay-adapter";
import {
  backendFailures,
  clearBackendFailures,
  lastBackendFailure,
  setBackendFailureConsole,
} from "./backend-failure";

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
  beforeEach(() => {
    mocks.invoke.mockReset();
    clearBackendFailures();
    setBackendFailureConsole(false);
  });

  it("uses only token-free overlay commands and exact camelCase security args", async () => {
    mocks.invoke.mockResolvedValueOnce(state).mockResolvedValueOnce({ ...state, ttlSeconds: 86_400, decryptDisplayEnabled: false });
    await expect(getNativeDiscordOverlayState()).resolves.toEqual(state);
    await expect(setNativeDiscordOverlaySecurity(86_400, false)).resolves.toEqual({ ...state, ttlSeconds: 86_400, decryptDisplayEnabled: false });
    expect(mocks.invoke.mock.calls).toEqual([
      ["get_native_discord_overlay_state"],
      ["set_native_discord_overlay_security", { ttlSeconds: 86_400, decryptDisplayEnabled: false }],
    ]);
  });

  it("returns only a bounded sanitized backend rejection for QA diagnostics", async () => {
    mocks.invoke.mockRejectedValueOnce(
      new Error(`exact Overlay window is unavailable\nBearer ${"x".repeat(500)}`),
    );
    const result = await getNativeDiscordOverlayQaDiagnostic();
    expect(result).toEqual({
      state: null,
      rejection: {
        command: "get_native_discord_overlay_state",
        message: expect.stringMatching(/^exact Overlay window is unavailable Bearer \[redacted\]$/u),
      },
    });
    expect(result.rejection?.message.length).toBeLessThanOrEqual(320);
  });

  it("identifies the exact safe structural field rejected from a QA state response", async () => {
    mocks.invoke
      .mockResolvedValueOnce({ ...state, scopeApproved: false })
      .mockResolvedValueOnce({ ...state, ttlSeconds: 7 });
    await expect(getNativeDiscordOverlayQaDiagnostic()).resolves.toEqual({
      state: null,
      rejection: {
        command: "get_native_discord_overlay_state",
        message: "Overlay-state field scopeApproved must be true.",
      },
    });
    await expect(getNativeDiscordOverlayQaDiagnostic()).resolves.toEqual({
      state: null,
      rejection: {
        command: "get_native_discord_overlay_state",
        message: "Overlay-state field ttlSeconds is not an allowed TTL.",
      },
    });
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
    const nativeReceipt = { placed: true, enterSent: true, status: "sent", mode: "compatibility", compatibilityDelayMs: 200 } as const;
    const receipt = { ...nativeReceipt, sendOutcome: "sent", automaticRetryAfterUncertain: false } as const;
    mocks.invoke.mockResolvedValueOnce(nativeReceipt);
    await expect(sendNativeDiscordOverlayCarrier("compatibility", 5)).resolves.toEqual(receipt);
    expect(mocks.invoke).toHaveBeenCalledWith("send_native_discord_overlay_carrier", {
      mode: "compatibility",
      charsPerSecond: 5,
    });
    mocks.invoke.mockResolvedValueOnce({ ...nativeReceipt, enterSent: false });
    await expect(sendNativeDiscordOverlayCarrier("compatibility", 5)).resolves.toBeNull();
    await expect(sendNativeDiscordOverlayCarrier("atomic", 121)).resolves.toBeNull();
  });

  it("derives the send-proof tri-state from native receipt evidence and never backend text", async () => {
    const notSent = {
      placed: true,
      enterSent: false,
      status: "carrierUnconfirmed",
      mode: "atomic",
      compatibilityDelayMs: 167,
      sendOutcome: "sent",
      automaticRetryAfterUncertain: true,
      uiStatusText: "Sent privately through OSL.",
    };
    const uncertain = {
      ...notSent,
      enterSent: true,
    };
    mocks.invoke
      .mockResolvedValueOnce(notSent)
      .mockResolvedValueOnce(uncertain);

    await expect(sendNativeDiscordOverlayCarrier("atomic", 6)).resolves.toEqual({
      placed: true,
      enterSent: false,
      status: "carrierUnconfirmed",
      mode: "atomic",
      compatibilityDelayMs: 167,
      sendOutcome: "notSent",
      automaticRetryAfterUncertain: false,
    });
    await expect(sendNativeDiscordOverlayCarrier("atomic", 6)).resolves.toEqual({
      placed: true,
      enterSent: true,
      status: "carrierUnconfirmed",
      mode: "atomic",
      compatibilityDelayMs: 167,
      sendOutcome: "deliveryUncertain",
      automaticRetryAfterUncertain: false,
    });
  });

  it("accepts a committed QA result and parses its exact carrier-row binding", async () => {
    vi.stubEnv("VITE_OSL_DISCORD_QA_SHELL", "1");
    const prepared = {
      messageId: `peer-${"a".repeat(32)}`,
      expiresAt: 4_102_444_800,
      personToPersonE2ee: true,
      viewOnce: false,
      deliveredToOslInbox: true,
    };
    const nativeCarrier = {
      placed: true,
      enterSent: true,
      status: "sent",
      mode: "atomic",
      compatibilityDelayMs: 167,
    } as const;
    const carrier = { ...nativeCarrier, sendOutcome: "sent", automaticRetryAfterUncertain: false } as const;
    const visibleCarrierRow = {
      messageId: prepared.messageId,
      nativeLocatorSha256: "1".repeat(64),
      carrierSha256: "2".repeat(64),
      leftPx: 160,
      topPx: 320,
      widthPx: 480,
      heightPx: 36,
      backgroundColor: "rgb(49 51 56)",
      foregroundColor: "rgb(219 222 225)",
      fontFamily: "gg sans",
      fontSizePx: 16,
      fontWeight: 400,
      lineHeightPx: 20,
      letterSpacingPx: 0,
      zoom: 1,
      density: 1.25,
    };
    mocks.invoke.mockResolvedValueOnce({ prepared, carrier: nativeCarrier, visibleCarrierRow });
    await expect(
      sendNativeDiscordQaAtomicText("hello", false, "atomic", 6),
    ).resolves.toEqual({ prepared, carrier, visibleCarrierRow });
    expect(mocks.invoke).toHaveBeenCalledWith("send_native_discord_qa_atomic_text", {
      plaintext: "hello",
      viewOnce: false,
      mode: "atomic",
      charsPerSecond: 6,
    });

    mocks.invoke.mockResolvedValueOnce({
      prepared,
      carrier: {
        ...nativeCarrier,
        placed: false,
        enterSent: false,
        status: "contextChanged",
      },
    });
    await expect(
      sendNativeDiscordQaAtomicText("hello", false, "atomic", 6),
    ).resolves.toEqual({
      prepared,
      carrier: {
        ...carrier,
        placed: false,
        enterSent: false,
        status: "contextChanged",
        sendOutcome: "notSent",
      },
    });
    mocks.invoke.mockResolvedValueOnce({
      prepared,
      carrier: nativeCarrier,
      visibleCarrierRow: { ...visibleCarrierRow, plaintext: "secret" },
    });
    await expect(
      sendNativeDiscordQaAtomicText("hello", false, "atomic", 6),
    ).resolves.toBeNull();
    vi.unstubAllEnvs();
  });

  it("passes only bounded presentation metrics for shape-matched cover rows", async () => {
    const nativeReceipt = { placed: true, enterSent: true, status: "sent", mode: "atomic", compatibilityDelayMs: 167 } as const;
    const receipt = { ...nativeReceipt, sendOutcome: "sent", automaticRetryAfterUncertain: false } as const;
    const layout = { contentWidthPx: 640, averageGraphemeWidthPx: 7.8, lineHeightPx: 20, zoom: 1, density: 1, padding: "shapeMatched", rowKind: "plainText" } as const;
    mocks.invoke.mockResolvedValueOnce(nativeReceipt);
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

  // The overlay adapters fail closed exactly as before. What changed is that
  // the native refusal is no longer discarded with the exception.
  it("keeps the native refusal behind a null overlay state", async () => {
    const refusal = "The native Discord window changed before protection opened";
    mocks.invoke.mockRejectedValueOnce(refusal);
    await expect(getNativeDiscordOverlayState()).resolves.toBeNull();
    expect(lastBackendFailure("get_native_discord_overlay_state")).toMatchObject({
      message: refusal,
      kind: "rejected",
    });
  });

  it("keeps the native refusal behind a null prepared message without the draft", async () => {
    const draft = "meet me at the north gate at nine";
    mocks.invoke.mockRejectedValueOnce(`OSL could not deliver the protected message: "${draft}"`);
    await expect(prepareNativeDiscordOverlayText(draft, false)).resolves.toBeNull();
    const failure = lastBackendFailure("prepare_native_discord_overlay_text");
    expect(failure?.kind).toBe("rejected");
    expect(failure?.message).toContain("OSL could not deliver the protected message");
    expect(failure?.message).not.toContain("north gate");
    expect(failure?.message).not.toContain(draft);
    expect(JSON.stringify(backendFailures())).not.toContain(draft);
  });

  it("records the carrier and burn refusals that used to be dropped", async () => {
    mocks.invoke.mockRejectedValueOnce("The verified native composer surface is unavailable");
    await expect(sendNativeDiscordOverlayCarrier("atomic", 6)).resolves.toBeNull();
    expect(lastBackendFailure("send_native_discord_overlay_carrier")?.message)
      .toBe("The verified native composer surface is unavailable");

    mocks.invoke.mockRejectedValueOnce("OSL burn worker was interrupted");
    await expect(burnNativeDiscordOverlayChat()).resolves.toBeNull();
    expect(lastBackendFailure("burn_native_discord_overlay_chat")?.message)
      .toBe("OSL burn worker was interrupted");
  });

  it("separates an untruthful response from a native refusal", async () => {
    mocks.invoke.mockResolvedValueOnce({ expiresAt: 10, personToPersonE2ee: true, viewOnce: false, deliveredToOslInbox: false });
    await expect(prepareNativeDiscordOverlayText("hello", false)).resolves.toBeNull();
    expect(lastBackendFailure("prepare_native_discord_overlay_text")).toMatchObject({
      kind: "invalidResponse",
      message: "the prepared message did not match the expected shape",
    });
  });

  it("records the QA diagnostic rejection it already returns", async () => {
    mocks.invoke.mockRejectedValueOnce(new Error("Overlay window is unavailable"));
    const result = await getNativeDiscordOverlayQaDiagnostic();
    expect(result.rejection?.message).toBe("Overlay window is unavailable");
    expect(lastBackendFailure("get_native_discord_overlay_state")).toMatchObject({
      message: "Overlay window is unavailable",
      kind: "rejected",
    });
  });
});
