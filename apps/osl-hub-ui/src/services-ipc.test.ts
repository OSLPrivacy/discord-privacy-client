import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import {
  beginBrowserAccountImport,
  createEmbeddedServiceAccount,
  closeEmbeddedServiceHost,
  detachNativeAppWindow,
  focusNativeAppWindow,
  hostNativeAppWindow,
  installNativeApp,
  installMullvad,
  loadBrowserImports,
  loadMullvadStatus,
  openBrowserImport,
  openMullvad,
  openEmbeddedHomeApp,
  openEmbeddedServiceAccount,
  removeEmbeddedServiceAccount,
  resizeNativeAppWindow,
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
    const response = { id: "discord", status: "hosted", reason: "none", mode: "ownedBorderless" };
    mocks.invoke.mockResolvedValueOnce(response);
    await expect(hostNativeAppWindow("discord")).resolves.toEqual(response);
    expect(mocks.invoke).toHaveBeenCalledWith("host_native_app_window", { appId: "discord" });
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

  it("uses argument-free lifecycle commands for resize, focus, and detach", async () => {
    mocks.invoke
      .mockResolvedValueOnce({ id: "discord", status: "resized", reason: "none", mode: "ownedBorderless" })
      .mockResolvedValueOnce({ id: "discord", status: "focused", reason: "none", mode: "ownedBorderless" })
      .mockResolvedValueOnce({ id: "discord", status: "detached", reason: "none", mode: "ownedBorderless" });
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
    { id: "telegram", status: "hosted", reason: "none", mode: "ownedBorderless" },
    { id: "discord", status: "hosted", reason: "windowNotFound", mode: "ownedBorderless" },
    { id: "discord", status: "failed", reason: "windowNotFound", mode: "ownedBorderless" },
    { id: "discord", status: "hosted", reason: "none", mode: "ownedBorderless", extra: true },
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
