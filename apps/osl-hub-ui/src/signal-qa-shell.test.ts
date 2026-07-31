import { describe, expect, it, vi } from "vitest";
import {
  createSignalQaAttestationView,
  createSignalQaShell,
  signalQaCapability,
  signalQaSemanticReceipt,
  signalQaTestDefinitions,
  type SignalBindingReceipt,
  type SignalQaNativeReceipt,
  type SignalQaShellDependencies,
} from "./signal-qa-shell";
import type { NativeApp } from "./services";

const signalApp = (availability: NativeApp["availability"] = "installed"): NativeApp => ({
  id: "signal",
  displayName: "Signal",
  availability,
  isolatedProfileAvailable: false,
  supportsOverlay: false,
});

const receipt = (status: SignalQaNativeReceipt["status"]): SignalQaNativeReceipt => ({
  id: "signal",
  status,
  reason: "none",
  mode: "existingNativeCompanion",
  captureProtected: false,
});

function dependencies(): SignalQaShellDependencies & {
  listNativeApps: ReturnType<typeof vi.fn>;
  hostNativeAppWindow: ReturnType<typeof vi.fn>;
  focusNativeAppWindow: ReturnType<typeof vi.fn>;
  detachNativeAppWindow: ReturnType<typeof vi.fn>;
} {
  return {
    listNativeApps: vi.fn().mockResolvedValue([signalApp()]),
    hostNativeAppWindow: vi.fn().mockResolvedValue(receipt("existingSession")),
    focusNativeAppWindow: vi.fn().mockResolvedValue(receipt("focused")),
    detachNativeAppWindow: vi.fn().mockResolvedValue(receipt("detached")),
  };
}

const attestation = (overrides: Partial<SignalBindingReceipt> = {}): SignalBindingReceipt => ({
  status: "accepted",
  reason: "none",
  lifecycleGeneration: 4,
  attestationGeneration: 7,
  validForMs: 2_000,
  ...overrides,
});

describe("Signal native QA shell", () => {
  it("declares immutable desktop-only, existing-session capability limits", () => {
    expect(signalQaCapability).toEqual({
      provider: "signal",
      displayName: "Signal",
      desktopOnly: true,
      sessionMode: "existingSession",
      captureProtected: false,
      credentialsAccepted: false,
      browserAutomationAllowed: false,
    });
    expect(Object.isFrozen(signalQaCapability)).toBe(true);
  });

  it("claims only an installed Signal Desktop existing session", async () => {
    const deps = dependencies();
    const shell = createSignalQaShell(deps);

    await expect(shell.open()).resolves.toMatchObject({
      phase: "open",
      provider: "signal",
      sessionMode: "existingSession",
      captureProtected: false,
    });
    expect(deps.hostNativeAppWindow).toHaveBeenCalledOnce();
    expect(deps.hostNativeAppWindow).toHaveBeenCalledWith("signal", "existingSession");
  });

  it("fails before hosting when Signal Desktop is absent or not installed", async () => {
    for (const catalog of [[], [signalApp("installable")], [signalApp("unavailable")]]) {
      const deps = dependencies();
      deps.listNativeApps.mockResolvedValueOnce(catalog);
      const shell = createSignalQaShell(deps);

      await expect(shell.open()).resolves.toMatchObject({ phase: "failed", failure: "appNotInstalled" });
      expect(deps.hostNativeAppWindow).not.toHaveBeenCalled();
    }
  });

  it("rejects protected, owned-profile, unsuccessful, or non-Signal receipts", async () => {
    const invalidReceipts = [
      { ...receipt("existingSession"), captureProtected: true },
      { ...receipt("existingSession"), mode: "ownedBorderless" as const },
      { ...receipt("existingSession"), reason: "windowIdentityChanged" as const },
      { ...receipt("existingSession"), id: "telegram" },
    ];
    for (const invalid of invalidReceipts) {
      const deps = dependencies();
      deps.hostNativeAppWindow.mockResolvedValueOnce(invalid);
      const shell = createSignalQaShell(deps);
      await expect(shell.open()).resolves.toMatchObject({ phase: "failed", failure: "receiptMismatch" });
    }

    const deps = dependencies();
    deps.hostNativeAppWindow.mockResolvedValueOnce({
      ...receipt("failed"),
      reason: "existingSessionAmbiguous",
      mode: "none",
    });
    await expect(createSignalQaShell(deps).open()).resolves.toMatchObject({
      phase: "failed",
      failure: "hostRejected",
      nativeReason: "existingSessionAmbiguous",
    });
  });

  it("focuses and detaches only exact existing-session receipts", async () => {
    const deps = dependencies();
    const shell = createSignalQaShell(deps);
    await shell.open();

    await expect(shell.focus()).resolves.toMatchObject({ phase: "open", provider: "signal" });
    await expect(shell.close()).resolves.toEqual({
      phase: "idle",
      provider: "signal",
      failure: null,
      nativeReason: null,
      sessionMode: "existingSession",
      captureProtected: false,
    });
  });

  it("rejects mismatched focus and detach receipts without fallback", async () => {
    const focusDeps = dependencies();
    const focusShell = createSignalQaShell(focusDeps);
    await focusShell.open();
    focusDeps.focusNativeAppWindow.mockResolvedValueOnce({ ...receipt("focused"), captureProtected: true });
    await expect(focusShell.focus()).resolves.toMatchObject({ phase: "failed", failure: "receiptMismatch" });

    const closeDeps = dependencies();
    const closeShell = createSignalQaShell(closeDeps);
    await closeShell.open();
    closeDeps.detachNativeAppWindow.mockResolvedValueOnce({ ...receipt("detached"), mode: "none" });
    await expect(closeShell.close()).resolves.toMatchObject({ phase: "failed", failure: "receiptMismatch" });
  });

  it("does not start a second host while the catalog check is pending", async () => {
    const deps = dependencies();
    let finishCatalog: ((apps: NativeApp[]) => void) | undefined;
    deps.listNativeApps.mockImplementationOnce(() => new Promise((resolve) => { finishCatalog = resolve; }));
    const shell = createSignalQaShell(deps);

    const first = shell.open();
    await expect(shell.open()).resolves.toMatchObject({ phase: "failed", failure: "busy" });
    finishCatalog?.([signalApp()]);
    await first;
    expect(deps.hostNativeAppWindow).toHaveBeenCalledOnce();
  });
});

describe("Signal QA semantic receipts", () => {
  it("enumerates the complete VM-independent live test matrix", () => {
    expect(signalQaTestDefinitions.map(({ id }) => id)).toEqual([
      "text", "multilineUtf8", "encryption", "transcriptOverlay", "burn", "covertext",
      "attachmentsImages", "receipts", "reconnect", "replayRejection", "malformedRejection",
      "expiry", "windowLifecycle",
    ]);
    expect(signalQaTestDefinitions.every(Object.isFrozen)).toBe(true);
  });

  it("blocks every protected capability without a backend attestation", () => {
    const semantic = signalQaSemanticReceipt(null, true);
    expect(semantic).toMatchObject({
      windowClaim: "passed",
      destination: "blocked",
      composer: "blocked",
      freshness: "blocked",
      protectedComposer: "blocked",
    });
    expect(new Set(Object.values(semantic.tests))).toEqual(new Set(["blocked"]));
  });

  it("enables the protected composer only for a fresh accepted backend receipt", () => {
    const semantic = signalQaSemanticReceipt(attestation(), true, 200);
    expect(semantic).toMatchObject({
      windowClaim: "passed",
      destination: "passed",
      composer: "passed",
      freshness: "passed",
      protectedComposer: "available",
    });
    expect(semantic).toMatchObject({ bindingStatus: "accepted", bindingReason: "none", lifecycleGeneration: 4, attestationGeneration: 7 });
    expect(new Set(Object.values(semantic.tests))).toEqual(new Set(["notRun"]));
  });

  it("fails closed for expired, oversized, rejected, consumed, or malformed receipts", () => {
    const invalid = [
      [attestation(), false, 200],
      [attestation(), true, 2_000],
      [attestation({ validForMs: 5_001 }), true, 0],
      [attestation({ status: "rejected", reason: "destinationMismatch", validForMs: 0 }), true, 0],
      [attestation({ status: "authorized", validForMs: 0 }), true, 0],
      [{ ...attestation(), status: "invented" }, true, 0],
      [{ ...attestation(), attestationGeneration: -1 }, true, 0],
    ] as const;
    for (const [raw, claimed, elapsed] of invalid) {
      expect(signalQaSemanticReceipt(raw, claimed, elapsed).protectedComposer).toBe("blocked");
    }
  });

  it("preserves the safe state-unavailable reason while remaining blocked", () => {
    expect(signalQaSemanticReceipt(attestation({
      status: "rejected",
      reason: "stateUnavailable",
      attestationGeneration: 0,
      validForMs: 0,
    }), true)).toMatchObject({
      bindingStatus: "rejected",
      bindingReason: "stateUnavailable",
      protectedComposer: "blocked",
    });
  });

  it("never copies private or unknown receipt fields", () => {
    const raw = {
      ...attestation(),
      accountName: "private account",
      conversationTitle: "private conversation",
      messageContents: "private message",
    };
    const semantic = signalQaSemanticReceipt(raw, true, 10);
    expect(semantic.freshness).toBe("passed");
    expect(semantic.protectedComposer).toBe("available");
    expect(JSON.stringify(semantic)).not.toMatch(/private|accountName|conversationTitle|messageContents/);
    expect(Object.keys(semantic.tests)).toEqual(signalQaTestDefinitions.map(({ id }) => id));
  });

  it("loads through a fail-closed dependency boundary and re-evaluates expiry", async () => {
    let now = 10_000;
    const getSignalProtectedSendReadiness = vi.fn().mockResolvedValue(attestation());
    const view = createSignalQaAttestationView({ getSignalProtectedSendReadiness, nowMs: () => now });

    expect(view.state(true).protectedComposer).toBe("blocked");
    await expect(view.refresh(true)).resolves.toMatchObject({ protectedComposer: "available" });
    now = 12_000;
    expect(view.state(true)).toMatchObject({ freshness: "expired", protectedComposer: "blocked" });
    view.clear();
    expect(view.state(true).freshness).toBe("blocked");
  });

  it("clears prior evidence when the backend dependency rejects or returns malformed data", async () => {
    const getSignalProtectedSendReadiness = vi.fn()
      .mockResolvedValueOnce(attestation())
      .mockRejectedValueOnce(new Error("unavailable"))
      .mockResolvedValueOnce({ status: "accepted", messageContents: "must not escape" });
    const view = createSignalQaAttestationView({ getSignalProtectedSendReadiness, nowMs: () => 2_000 });

    expect((await view.refresh(true)).protectedComposer).toBe("available");
    expect((await view.refresh(true)).protectedComposer).toBe("blocked");
    expect((await view.refresh(true)).protectedComposer).toBe("blocked");
  });
});
