import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import {
  beginBrowserAccountImport,
  beginProtectedBrowserImport,
  finishProtectedBrowserImport,
  createEmbeddedServiceAccount,
  closeEmbeddedServiceHost,
  detachDefaultBrowserCompanion,
  detachNativeAppWindow,
  focusDefaultBrowserCompanion,
  focusNativeAppWindow,
  focusMullvadWindow,
  hostNativeAppWindow,
  hostBrowserCompanion,
  hostDefaultBrowserCompanion,
  hostMullvadWindow,
  installFirefox,
  installNativeApp,
  installMullvad,
  launchFirefoxService,
  loadBrowserImports,
  loadDefaultBrowserCompanionStatus,
  loadFirefoxStatus,
  loadMullvadStatus,
  openBrowserImport,
  openMullvad,
  openEmbeddedHomeApp,
  openEmbeddedServiceAccount,
  parseDiscordSessionMode,
  parseNativeSessionMode,
  removeEmbeddedServiceAccount,
  resizeNativeAppWindow,
  resizeDefaultBrowserCompanion,
  resizeMullvadWindow,
  restoreMullvadWindow,
  setupEmbeddedHomeApp,
  type HomeAppCatalogEntry,
  type LinkedService,
} from "./services";

const instagramApp: HomeAppCatalogEntry = {
  id: "instagram", displayName: "Instagram", serviceId: "instagram", provider: null,
  visibility: "launch", section: "social", launchState: "available", linked: false,
  accountCount: 0, setupEligible: true,
};

describe("embedded service IPC", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.isTauriRuntime.mockReturnValue(true);
  });

  it("creates then opens the real embedded login surface", async () => {
    mocks.invoke
      .mockResolvedValueOnce({ id: "acct-123", label: "Personal", displayHandle: "Sign in on the service", state: "notLinked", provider: null })
      .mockResolvedValueOnce({ serviceId: "instagram", accountId: "acct-123", generation: 1 });
    await expect(setupEmbeddedHomeApp(instagramApp)).resolves.toMatchObject({
      account: { id: "acct-123" }, host: { serviceId: "instagram", accountId: "acct-123" },
    });
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "create_service_account", {
      serviceId: "instagram", label: "Personal", provider: null,
    });
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "open_service_host", {
      serviceId: "instagram", accountId: "acct-123",
    });
  });

  it("passes only the fixed provider enum for email setup", async () => {
    mocks.invoke.mockResolvedValueOnce({
      id: "mail-1", label: "Personal", displayHandle: "Sign in on the service", state: "notLinked", provider: "outlook",
    });
    await createEmbeddedServiceAccount("email", " Personal ", "outlook");
    expect(mocks.invoke).toHaveBeenCalledWith("create_service_account", {
      serviceId: "email", label: "Personal", provider: "outlook",
    });
    await expect(createEmbeddedServiceAccount("instagram", "Personal", "gmail")).rejects.toThrow();
  });

  it("resumes one exact configured profile and never accepts a path-like id", async () => {
    const services = [{
      id: "instagram", displayName: "Instagram", sidebarGlyph: "IG", sidebarOrder: 1,
      category: "consumer", launchState: "available", supportsNativePreview: true,
      supportsProtectedPreview: false,
      accounts: [{ id: "acct-a", label: "A", displayHandle: "Sign in", state: "notLinked", provider: null }],
    }] as LinkedService[];
    mocks.invoke.mockResolvedValueOnce({ serviceId: "instagram", accountId: "acct-a", generation: 2 });
    await expect(openEmbeddedHomeApp({ ...instagramApp, linked: true, accountCount: 1 }, services))
      .resolves.toMatchObject({ accountId: "acct-a" });
    await expect(openEmbeddedServiceAccount("instagram", "../profile")).rejects.toThrow();
  });

  it("opens the exact locally selected profile when more than one exists", async () => {
    const services = [{
      id: "instagram", displayName: "Instagram", sidebarGlyph: "IG", sidebarOrder: 1,
      category: "consumer", launchState: "available", supportsNativePreview: true,
      supportsProtectedPreview: false,
      accounts: [
        { id: "acct-a", label: "Personal", displayHandle: "Sign in", state: "notLinked", provider: null },
        { id: "acct-b", label: "Work", displayHandle: "Sign in", state: "notLinked", provider: null },
      ],
    }] as LinkedService[];
    mocks.invoke.mockResolvedValueOnce({ serviceId: "instagram", accountId: "acct-b", generation: 3 });
    await expect(openEmbeddedHomeApp({ ...instagramApp, linked: true, accountCount: 2 }, services, "acct-b"))
      .resolves.toMatchObject({ accountId: "acct-b" });
    expect(mocks.invoke).toHaveBeenCalledWith("open_service_host", {
      serviceId: "instagram", accountId: "acct-b",
    });
  });

  it("removes only one exact owned profile through the narrow command", async () => {
    mocks.invoke.mockResolvedValueOnce({
      serviceId: "instagram", accountId: "acct-a", profileExisted: true,
      cleanupPending: false, registryRemoved: true,
    });
    await expect(removeEmbeddedServiceAccount("instagram", "acct-a")).resolves.toMatchObject({
      accountId: "acct-a", registryRemoved: true,
    });
    expect(mocks.invoke).toHaveBeenCalledWith("remove_service_account", {
      serviceId: "instagram", accountId: "acct-a",
    });
  });

  it("closes only the owned embedded host when leaving an app", async () => {
    mocks.invoke.mockResolvedValueOnce(undefined);
    await expect(closeEmbeddedServiceHost()).resolves.toBeUndefined();
    expect(mocks.invoke).toHaveBeenCalledWith("close_service_host");
  });
});

describe("native window host IPC", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.isTauriRuntime.mockReturnValue(true);
  });

  it("hosts one exact allowlisted app through the narrow command", async () => {
    const response = { id: "discord", status: "hosted", reason: "none", mode: "existingNativeCompanion", captureProtected: false };
    mocks.invoke.mockResolvedValueOnce(response);
    await expect(hostNativeAppWindow("discord", "existingSession")).resolves.toEqual(response);
    expect(mocks.invoke).toHaveBeenCalledWith("host_native_app_window", { appId: "discord", discordSessionMode: "existingSession" });
  });

  it.each(["existingSessionUnavailable", "existingSessionAmbiguous"])("preserves the bounded existing-session failure reason %s", async (reason) => {
    const response = { id: "discord", status: "failed", reason, mode: "none", captureProtected: false };
    mocks.invoke.mockResolvedValueOnce(response);
    await expect(hostNativeAppWindow("discord", "existingSession")).resolves.toEqual(response);
  });

  it.each([
    "childHierarchyRejected",
    "childStyleRejected",
    "childProcessRejected",
    "childDpiRejected",
    "childVisibilityRejected",
    "childBoundsRejected",
    "childSiblingRejected",
  ])("preserves the bounded protected-child failure stage %s", async (reason) => {
    const response = { id: "signal", status: "failed", reason, mode: "none", captureProtected: false };
    mocks.invoke.mockResolvedValueOnce(response);
    await expect(hostNativeAppWindow("signal", "dedicated")).resolves.toEqual(response);
  });

  it.each([
    "borrowedPlacementRejected",
    "borrowedStyleRejected",
    "borrowedVisibilityRejected",
    "borrowedBoundsRejected",
  ])("preserves the bounded borrowed-window failure stage %s", async (reason) => {
    const response = { id: "signal", status: "failed", reason, mode: "none", captureProtected: false };
    mocks.invoke.mockResolvedValueOnce(response);
    await expect(hostNativeAppWindow("signal", "existingSession")).resolves.toEqual(response);
  });

  it("defaults malformed or missing Discord session choices to the dedicated profile", async () => {
    expect(parseDiscordSessionMode(null)).toBe("dedicated");
    expect(parseDiscordSessionMode("existing")).toBe("dedicated");
    expect(parseDiscordSessionMode("existingSession")).toBe("existingSession");
    const response = { id: "discord", status: "hosted", reason: "none", mode: "ownedBorderless", captureProtected: false };
    mocks.invoke.mockResolvedValueOnce(response);
    await expect(hostNativeAppWindow("discord")).resolves.toEqual(response);
    expect(mocks.invoke).toHaveBeenCalledWith("host_native_app_window", { appId: "discord", discordSessionMode: "dedicated" });
  });

  it("allows explicit existing sessions only for the supported native apps", async () => {
    expect(parseNativeSessionMode("existingSession")).toBe("existingSession");
    const response = { id: "telegram", status: "hosted", reason: "none", mode: "existingNativeCompanion", captureProtected: false };
    mocks.invoke.mockResolvedValueOnce(response);
    await expect(hostNativeAppWindow("telegram", "existingSession")).resolves.toEqual(response);
    expect(mocks.invoke).toHaveBeenCalledWith("host_native_app_window", { appId: "telegram", discordSessionMode: "existingSession" });

    mocks.invoke.mockResolvedValueOnce({ id: "whatsapp", status: "hosted", reason: "none", mode: "existingNativeCompanion", captureProtected: false });
    await expect(hostNativeAppWindow("whatsapp", "existingSession")).resolves.toMatchObject({ id: "whatsapp", status: "hosted" });
    expect(mocks.invoke).toHaveBeenCalledWith("host_native_app_window", { appId: "whatsapp", discordSessionMode: "existingSession" });

    mocks.invoke.mockResolvedValueOnce({ id: "signal", status: "hosted", reason: "none", mode: "existingNativeCompanion", captureProtected: false });
    await expect(hostNativeAppWindow("signal", "existingSession")).resolves.toMatchObject({ id: "signal", status: "hosted" });
    expect(mocks.invoke).toHaveBeenCalledWith("host_native_app_window", { appId: "signal", discordSessionMode: "existingSession" });
  });

  it("hosts Outlook only as an existing native account", async () => {
    const response = { id: "outlook", status: "hosted", reason: "none", mode: "existingNativeCompanion", captureProtected: false };
    mocks.invoke.mockResolvedValueOnce(response);
    await expect(hostNativeAppWindow("outlook", "existingSession")).resolves.toEqual(response);
    expect(mocks.invoke).toHaveBeenCalledWith("host_native_app_window", { appId: "outlook", discordSessionMode: "existingSession" });
  });

  it("preserves the dedicated Telegram initialization failure reason", async () => {
    const response = { id: "telegram", status: "failed", reason: "profileInitializationFailed", mode: "none", captureProtected: false };
    mocks.invoke.mockResolvedValueOnce(response);
    await expect(hostNativeAppWindow("telegram")).resolves.toEqual(response);
  });

  it("starts installation only for an exact allowlisted app", async () => {
    const response = { id: "telegram", started: true, packageId: "Telegram.TelegramDesktop" };
    mocks.invoke.mockResolvedValueOnce(response);
    await expect(installNativeApp("telegram")).resolves.toEqual(response);
    expect(mocks.invoke).toHaveBeenCalledWith("install_native_app", { appId: "telegram" });
  });

  it("uses argument-free fixed Mullvad commands", async () => {
    mocks.invoke
      .mockResolvedValueOnce({ availability: "installed" })
      .mockResolvedValueOnce({ started: true })
      .mockResolvedValueOnce({ started: true });
    await expect(loadMullvadStatus()).resolves.toEqual({ availability: "installed" });
    await expect(openMullvad()).resolves.toEqual({ started: true });
    await expect(installMullvad()).resolves.toEqual({ started: true });
    expect(mocks.invoke.mock.calls).toEqual([
      ["get_mullvad_status"],
      ["open_mullvad"],
      ["install_mullvad"],
    ]);
  });

  it("borrows Mullvad through fixed argument-free commands and rejects capture claims", async () => {
    const hosted = { status: "hosted", reason: "none", mode: "existingMullvadSession", captureProtected: false };
    mocks.invoke
      .mockResolvedValueOnce(hosted)
      .mockResolvedValueOnce({ ...hosted, status: "resized" })
      .mockResolvedValueOnce({ ...hosted, status: "focused" })
      .mockResolvedValueOnce({ ...hosted, status: "restored" });
    await expect(hostMullvadWindow()).resolves.toEqual(hosted);
    await expect(resizeMullvadWindow()).resolves.toMatchObject({ status: "resized" });
    await expect(focusMullvadWindow()).resolves.toMatchObject({ status: "focused" });
    await expect(restoreMullvadWindow()).resolves.toMatchObject({ status: "restored" });
    expect(mocks.invoke.mock.calls).toEqual([
      ["host_mullvad_window"],
      ["resize_mullvad_window"],
      ["focus_mullvad_window"],
      ["restore_mullvad_window"],
    ]);

    mocks.invoke.mockResolvedValueOnce({ ...hosted, captureProtected: true });
    await expect(hostMullvadWindow()).rejects.toThrow("invalid Mullvad window host response");
  });

  it("uses argument-free lifecycle commands for resize, focus, and detach", async () => {
    mocks.invoke
      .mockResolvedValueOnce({ id: "discord", status: "resized", reason: "none", mode: "ownedBorderless", captureProtected: false })
      .mockResolvedValueOnce({ id: "discord", status: "focused", reason: "none", mode: "ownedBorderless", captureProtected: false })
      .mockResolvedValueOnce({ id: "discord", status: "detached", reason: "none", mode: "ownedBorderless", captureProtected: false });
    await expect(resizeNativeAppWindow()).resolves.toMatchObject({ status: "resized" });
    await expect(focusNativeAppWindow()).resolves.toMatchObject({ status: "focused" });
    await expect(detachNativeAppWindow()).resolves.toMatchObject({ status: "detached" });
    expect(mocks.invoke.mock.calls).toEqual([
      ["resize_native_app_window"],
      ["focus_native_app_window"],
      ["detach_native_app_window"],
    ]);
  });

  it.each([
    { id: "telegram", status: "hosted", reason: "none", mode: "ownedBorderless", captureProtected: false },
    { id: "discord", status: "hosted", reason: "windowNotFound", mode: "ownedBorderless", captureProtected: false },
    { id: "discord", status: "failed", reason: "windowNotFound", mode: "ownedBorderless", captureProtected: false },
    { id: "discord", status: "hosted", reason: "none", mode: "ownedBorderless", captureProtected: false, extra: true },
    { id: "discord", status: "hosted", reason: "none", mode: "ownedBorderless" },
  ])("rejects malformed or mismatched host responses %#", async (response) => {
    mocks.invoke.mockResolvedValueOnce(response);
    await expect(hostNativeAppWindow("discord")).rejects.toThrow("invalid native window host response");
  });

  it("fails before IPC outside the native runtime", async () => {
    mocks.isTauriRuntime.mockReturnValue(false);
    await expect(hostNativeAppWindow("discord")).rejects.toThrow("native host unavailable");
    await expect(resizeNativeAppWindow()).rejects.toThrow("native host unavailable");
    await expect(focusNativeAppWindow()).rejects.toThrow("native host unavailable");
    await expect(detachNativeAppWindow()).rejects.toThrow("native host unavailable");
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
});

describe("browser-owned import IPC", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.isTauriRuntime.mockReturnValue(true);
  });

  it("lists only a strict fixed browser catalog and opens one enum-selected wizard", async () => {
    const catalog = [
      { id: "chrome", displayName: "Chrome", installed: true },
      { id: "edge", displayName: "Edge", installed: false },
    ];
    mocks.invoke.mockResolvedValueOnce(catalog).mockResolvedValueOnce({ id: "chrome", opened: true });
    await expect(loadBrowserImports()).resolves.toEqual(catalog);
    await expect(openBrowserImport("chrome")).resolves.toBeUndefined();
    expect(mocks.invoke.mock.calls).toEqual([
      ["list_browser_imports"],
      ["open_browser_import", { browserId: "chrome" }],
    ]);
  });

  it("uses exact truthful commands for selected and default browser companions", async () => {
    const status = { status: "available", browserId: "opera", displayName: "Opera", reason: "none", captureProtected: false, containment: "bestEffort" };
    const hosted = { status: "hosted", browserId: "opera", reason: "none", mode: "existingBrowserCompanion", captureProtected: false, containment: "bestEffort" };
    const resized = { ...hosted, status: "resized" };
    const focused = { ...hosted, status: "focused" };
    const detached = { ...hosted, status: "detached" };
    mocks.invoke
      .mockResolvedValueOnce(status)
      .mockResolvedValueOnce(hosted)
      .mockResolvedValueOnce(resized)
      .mockResolvedValueOnce(focused)
      .mockResolvedValueOnce(detached);

    await expect(loadDefaultBrowserCompanionStatus()).resolves.toEqual(status);
    await expect(hostDefaultBrowserCompanion("instagram")).resolves.toEqual(hosted);
    await expect(resizeDefaultBrowserCompanion()).resolves.toEqual(resized);
    await expect(focusDefaultBrowserCompanion()).resolves.toEqual(focused);
    await expect(detachDefaultBrowserCompanion()).resolves.toEqual(detached);
    expect(mocks.invoke.mock.calls).toEqual([
      ["get_default_browser_companion_status"],
      ["host_default_browser_companion", { serviceId: "instagram", browserId: null, accountMode: "existingBrowser" }],
      ["resize_default_browser_companion"],
      ["focus_default_browser_companion"],
      ["detach_default_browser_companion"],
    ]);
  });

  it("rejects untruthful browser-companion receipts and widened service ids", async () => {
    mocks.invoke.mockResolvedValueOnce({ status: "available", browserId: "chrome", displayName: "Chrome", reason: "none", captureProtected: true, containment: "bestEffort" });
    await expect(loadDefaultBrowserCompanionStatus()).rejects.toThrow("invalid default browser companion status");
    mocks.invoke.mockResolvedValueOnce({ status: "hosted", browserId: "chrome", reason: "none", mode: "existingBrowserCompanion", captureProtected: false, containment: "locked" });
    await expect(hostDefaultBrowserCompanion("gmail")).rejects.toThrow("invalid default browser companion action");
    mocks.invoke.mockClear();
    await expect(hostDefaultBrowserCompanion("discord")).rejects.toThrow("browser companion unavailable");
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("passes only fixed browser and account-mode enums to the companion", async () => {
    const isolated = { status: "hosted", browserId: "firefox", reason: "none", mode: "isolatedBrowserCompanion", captureProtected: false, containment: "bestEffort" };
    mocks.invoke.mockResolvedValueOnce(isolated);
    await expect(hostBrowserCompanion("icloud", "firefox", "isolatedOsl")).resolves.toEqual(isolated);
    expect(mocks.invoke).toHaveBeenCalledWith("host_default_browser_companion", {
      serviceId: "icloud", browserId: "firefox", accountMode: "isolatedOsl",
    });
    mocks.invoke.mockClear();
    await expect(hostBrowserCompanion("discord", "firefox", "isolatedOsl")).rejects.toThrow("browser companion unavailable");
    await expect(hostBrowserCompanion("outlook", "firefox", "isolatedOsl")).rejects.toThrow("browser companion unavailable");
    await expect(hostBrowserCompanion("gmail", "../firefox" as "firefox", "isolatedOsl")).rejects.toThrow("browser companion unavailable");
    await expect(hostBrowserCompanion("gmail", "firefox", "fresh" as "isolatedOsl")).rejects.toThrow("browser companion unavailable");
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("starts one argument-free Firefox migration and validates the exact result", async () => {
    const response = {
      preferredSource: "edge",
      detectedSources: ["edge", "chrome"],
      opened: true,
      mode: "firefoxMigrationWizard",
      manualExportRequired: false,
    };
    mocks.invoke.mockResolvedValueOnce(response);
    await expect(beginBrowserAccountImport()).resolves.toEqual(response);
    expect(mocks.invoke).toHaveBeenCalledWith("begin_browser_account_import");
  });

  it("passes only one exact queued browser enum to the protected importer", async () => {
    const response = { selectedSources: ["edge"], started: true, mode: "firefoxMigrationWizard", sourceSelected: true, manualFallback: null };
    mocks.invoke.mockResolvedValueOnce(response);
    await expect(beginProtectedBrowserImport(["edge"])).resolves.toEqual(response);
    expect(mocks.invoke).toHaveBeenCalledWith("begin_protected_browser_import", { browserIds: ["edge"] });
  });

  it("permits the explicit existing-session mode for Signal", async () => {
    mocks.invoke.mockResolvedValueOnce({ id: "signal", status: "hosted", reason: "none", mode: "existingNativeCompanion", captureProtected: false });
    await expect(hostNativeAppWindow("signal", "existingSession")).resolves.toMatchObject({ id: "signal", status: "hosted" });
    expect(mocks.invoke).toHaveBeenCalledWith("host_native_app_window", { appId: "signal", discordSessionMode: "existingSession" });
  });

  it("closes only the retained protected import process between queued sources", async () => {
    mocks.invoke.mockResolvedValueOnce(undefined);
    await expect(finishProtectedBrowserImport()).resolves.toBeUndefined();
    expect(mocks.invoke).toHaveBeenCalledWith("finish_protected_browser_import");
  });

  it("rejects widened protected-import input and receipts", async () => {
    await expect(beginProtectedBrowserImport([])).rejects.toThrow("protected browser import unavailable");
    await expect(beginProtectedBrowserImport(["edge", "chrome"])).rejects.toThrow("protected browser import unavailable");
    await expect(beginProtectedBrowserImport(["chrome", "chrome"])).rejects.toThrow("protected browser import unavailable");
    await expect(beginProtectedBrowserImport(["chrome --evil" as "chrome"])).rejects.toThrow("protected browser import unavailable");
    expect(mocks.invoke).not.toHaveBeenCalled();
    mocks.invoke.mockResolvedValueOnce({ selectedSources: ["edge"], started: true, mode: "protectedInApp", sourceSelected: true, manualFallback: null });
    await expect(beginProtectedBrowserImport(["chrome"])).rejects.toThrow("invalid protected browser import response");
    mocks.invoke.mockResolvedValueOnce({ selectedSources: ["chrome"], started: true, mode: "firefoxMigrationWizard", sourceSelected: false, manualFallback: null });
    await expect(beginProtectedBrowserImport(["chrome"])).rejects.toThrow("invalid protected browser import response");
  });

  it("uses only the exact Firefox status, install, and allowlisted service commands", async () => {
    mocks.invoke
      .mockResolvedValueOnce({ availability: "installed" })
      .mockResolvedValueOnce({ started: true, packageId: "Mozilla.Firefox" })
      .mockResolvedValueOnce({ serviceId: "instagram", started: true });

    await expect(loadFirefoxStatus()).resolves.toEqual({ availability: "installed" });
    await expect(installFirefox()).resolves.toBeUndefined();
    await expect(launchFirefoxService("instagram")).resolves.toBeUndefined();
    expect(mocks.invoke.mock.calls).toEqual([
      ["get_firefox_status"],
      ["install_firefox"],
      ["launch_firefox_service", { serviceId: "instagram" }],
    ]);
  });

  it("rejects widened Firefox receipts and unsupported service ids", async () => {
    mocks.invoke.mockResolvedValueOnce({ availability: "installed", executable: "C:/Firefox/firefox.exe" });
    await expect(loadFirefoxStatus()).rejects.toThrow("invalid Firefox status");
    mocks.invoke.mockResolvedValueOnce({ started: true, packageId: "Other.Firefox" });
    await expect(installFirefox()).rejects.toThrow("invalid Firefox install response");
    mocks.invoke.mockResolvedValueOnce({ serviceId: "gmail", started: true, url: "https://example.invalid" });
    await expect(launchFirefoxService("gmail")).rejects.toThrow("invalid Firefox launch response");
    mocks.invoke.mockClear();
    await expect(launchFirefoxService("discord")).rejects.toThrow("Firefox launch unavailable");
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("rejects a widened or malformed account-migration result", async () => {
    mocks.invoke.mockResolvedValueOnce({
      preferredSource: "edge",
      detectedSources: ["edge"],
      opened: true,
      mode: "firefoxMigrationWizard",
      manualExportRequired: false,
      profilePath: "C:/Users/example",
    });
    await expect(beginBrowserAccountImport()).rejects.toThrow("invalid browser account import response");
  });

  it("rejects injected ids and malformed native responses before they can widen the command", async () => {
    await expect(openBrowserImport("chrome --load-extension=evil" as "chrome")).rejects.toThrow("browser import unavailable");
    expect(mocks.invoke).not.toHaveBeenCalled();
    mocks.invoke.mockResolvedValueOnce([{ id: "chrome", displayName: "Chrome", installed: true, path: "C:/evil.exe" }]);
    await expect(loadBrowserImports()).rejects.toThrow("invalid browser import catalog");
    mocks.invoke.mockResolvedValueOnce({ id: "edge", opened: true });
    await expect(openBrowserImport("chrome")).rejects.toThrow("invalid browser import response");
  });
});
