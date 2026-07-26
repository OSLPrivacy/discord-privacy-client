import { describe, expect, it, vi } from "vitest";
import {
  createNativeQaShell,
  nativeQaProviderCapability,
  type NativeQaProvider,
  type NativeQaShellDependencies,
} from "./native-qa-shell";
import type { NativeApp, NativeWindowHostAction } from "./services";

const app = (id: NativeQaProvider, availability: NativeApp["availability"] = "installed"): NativeApp => ({
  id,
  displayName: id,
  availability,
  isolatedProfileAvailable: false,
  supportsOverlay: false,
});

const receipt = (
  id: NativeQaProvider,
  status: NativeWindowHostAction["status"],
): NativeWindowHostAction => ({
  id,
  status,
  reason: "none",
  mode: "existingNativeCompanion",
  captureProtected: false,
});

function dependencies(provider: NativeQaProvider): NativeQaShellDependencies & {
  listNativeApps: ReturnType<typeof vi.fn>;
  hostNativeAppWindow: ReturnType<typeof vi.fn>;
  focusNativeAppWindow: ReturnType<typeof vi.fn>;
  detachNativeAppWindow: ReturnType<typeof vi.fn>;
} {
  return {
    listNativeApps: vi.fn().mockResolvedValue([app(provider)]),
    hostNativeAppWindow: vi.fn().mockResolvedValue(receipt(provider, "hosted")),
    focusNativeAppWindow: vi.fn().mockResolvedValue(receipt(provider, "focused")),
    detachNativeAppWindow: vi.fn().mockResolvedValue(receipt(provider, "detached")),
  };
}

describe("native QA shell", () => {
  it.each(["telegram", "signal", "whatsapp"] as const)(
    "opens only the existing %s session through the typed native adapter",
    async (provider) => {
      const deps = dependencies(provider);
      const shell = createNativeQaShell(deps);

      await expect(shell.open(provider)).resolves.toMatchObject({
        phase: "open",
        provider,
        sessionMode: "existingSession",
        captureProtected: false,
      });
      expect(deps.hostNativeAppWindow).toHaveBeenCalledOnce();
      expect(deps.hostNativeAppWindow).toHaveBeenCalledWith(provider, "existingSession");
    },
  );

  it("publishes explicit capability limits for every supported provider", () => {
    for (const provider of ["telegram", "signal", "whatsapp"] as const) {
      expect(nativeQaProviderCapability(provider)).toMatchObject({
        id: provider,
        sessionMode: "existingSession",
        captureProtected: false,
        credentialsAccepted: false,
        browserAutomationAllowed: false,
      });
    }
    expect(nativeQaProviderCapability("discord")).toBeNull();
    expect(nativeQaProviderCapability({ id: "telegram" })).toBeNull();
  });

  it("fails closed before hosting unsupported or uninstalled providers", async () => {
    const deps = dependencies("telegram");
    const shell = createNativeQaShell(deps);

    await expect(shell.open("discord")).resolves.toMatchObject({ phase: "failed", failure: "unsupportedProvider" });
    expect(deps.listNativeApps).not.toHaveBeenCalled();

    deps.listNativeApps.mockResolvedValueOnce([app("telegram", "installable")]);
    await expect(shell.open("telegram")).resolves.toMatchObject({ phase: "failed", failure: "appNotInstalled" });
    expect(deps.hostNativeAppWindow).not.toHaveBeenCalled();
  });

  it("rejects a dedicated, protected, or wrong-provider success receipt", async () => {
    const invalidReceipts: NativeWindowHostAction[] = [
      { ...receipt("telegram", "hosted"), mode: "ownedBorderless" },
      { ...receipt("telegram", "hosted"), captureProtected: true },
      receipt("signal", "hosted"),
    ];
    for (const invalid of invalidReceipts) {
      const deps = dependencies("telegram");
      deps.hostNativeAppWindow.mockResolvedValueOnce(invalid);
      const shell = createNativeQaShell(deps);
      await expect(shell.open("telegram")).resolves.toMatchObject({ phase: "failed", failure: "receiptMismatch" });
    }
  });

  it("preserves bounded native failure reasons without retry or fallback", async () => {
    const deps = dependencies("whatsapp");
    deps.hostNativeAppWindow.mockResolvedValueOnce({
      id: "whatsapp",
      status: "failed",
      reason: "existingSessionAmbiguous",
      mode: "none",
      captureProtected: false,
    });
    const shell = createNativeQaShell(deps);

    await expect(shell.open("whatsapp")).resolves.toMatchObject({
      phase: "failed",
      failure: "hostRejected",
      nativeReason: "existingSessionAmbiguous",
    });
    expect(deps.hostNativeAppWindow).toHaveBeenCalledOnce();
  });

  it("focuses and detaches only the exact active existing-session companion", async () => {
    const deps = dependencies("signal");
    const shell = createNativeQaShell(deps);
    await shell.open("signal");

    await expect(shell.focus()).resolves.toMatchObject({ phase: "open", provider: "signal" });
    await expect(shell.close()).resolves.toEqual({
      phase: "idle",
      provider: null,
      failure: null,
      nativeReason: null,
      sessionMode: "existingSession",
      captureProtected: false,
    });
    expect(deps.focusNativeAppWindow).toHaveBeenCalledOnce();
    expect(deps.detachNativeAppWindow).toHaveBeenCalledOnce();
  });

  it("does not start a second host while the first catalog check is pending", async () => {
    const deps = dependencies("telegram");
    let finishCatalog: ((apps: NativeApp[]) => void) | undefined;
    deps.listNativeApps.mockImplementationOnce(() => new Promise((resolve) => { finishCatalog = resolve; }));
    const shell = createNativeQaShell(deps);

    const first = shell.open("telegram");
    await expect(shell.open("signal")).resolves.toMatchObject({ phase: "failed", failure: "busy" });
    finishCatalog?.([app("telegram")]);
    await first;
    expect(deps.hostNativeAppWindow).toHaveBeenCalledOnce();
  });
});
