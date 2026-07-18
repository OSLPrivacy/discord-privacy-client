import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import {
  createEmbeddedServiceAccount,
  closeEmbeddedServiceHost,
  openEmbeddedHomeApp,
  openEmbeddedServiceAccount,
  removeEmbeddedServiceAccount,
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
