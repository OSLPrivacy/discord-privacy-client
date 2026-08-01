import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LinkedService } from "./services";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

class MemoryStorage {
  readonly values = new Map<string, string>();
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
  removeItem(key: string): void { this.values.delete(key); }
  clear(): void { this.values.clear(); }
}

const signal: LinkedService = {
  id: "signal",
  displayName: "Signal",
  sidebarGlyph: "S",
  sidebarOrder: 0,
  category: "consumer",
  launchState: "available",
  supportsNativePreview: true,
  supportsProtectedPreview: true,
  accounts: [{ id: "signal-account", label: "Personal", displayHandle: "Signal", state: "demoLinked", provider: null }],
};

const email: LinkedService = {
  id: "email",
  displayName: "Email",
  sidebarGlyph: "E",
  sidebarOrder: 1,
  category: "consumer",
  launchState: "available",
  supportsNativePreview: true,
  supportsProtectedPreview: true,
  accounts: [
    { id: "gmail-account", label: "Gmail", displayHandle: "Gmail", state: "demoLinked", provider: "gmail" },
    { id: "proton-account", label: "Proton", displayHandle: "Proton", state: "demoLinked", provider: "proton" },
  ],
};

describe("T3-A4 hostless local protection", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    vi.stubGlobal("localStorage", new MemoryStorage());
    vi.stubGlobal("requestAnimationFrame", () => 1);
    vi.stubGlobal("cancelAnimationFrame", () => undefined);
  });

  async function opensHostlessLocalProtection(service: LinkedService, homeAppId: "signal" | "gmail", accountId: string): Promise<void> {
    mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "set_local_protected_sheet_open") return true;
      if (command === "activate_local_loopback_context") {
        const request = args as { serviceId: string; accountId: string; conversationId: string };
        return { contextToken: "ctx-signal", ...request };
      }
      return null;
    });
    vi.resetModules();
    const { __oslHubUiTest } = await import("./main");
    __oslHubUiTest.reset({ services: [service] });
    __oslHubUiTest.renderServiceHeader(service, homeAppId);

    await __oslHubUiTest.openLocalProtection();
    expect(__oslHubUiTest.renderProtectedSheets()).toContain('id="local-chat-label"');

    await __oslHubUiTest.startLocalProtection("Rose");
    expect(mocks.invoke).toHaveBeenCalledWith("activate_local_loopback_context", {
      serviceId: service.id,
      accountId,
      conversationId: expect.stringMatching(/^local-[a-f0-9]{32}$/u),
    });
    expect(__oslHubUiTest.renderProtectedSheets()).toContain("Encrypt & copy");
  }

  it("opens and activates from the active native service account without an embedded host", async () => {
    await opensHostlessLocalProtection(signal, "signal", "signal-account");
  });

  it("uses the selected browser-launcher provider account without an embedded host", async () => {
    await opensHostlessLocalProtection(email, "gmail", "gmail-account");
  });
});
