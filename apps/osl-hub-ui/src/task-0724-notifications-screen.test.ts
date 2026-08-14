import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

const localStore = new Map<string, string>();

function installGlobals(): void {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("document", {
    querySelector: vi.fn(() => null),
    querySelectorAll: vi.fn(() => []),
    createElement: vi.fn(() => ({})),
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    addEventListener: vi.fn(),
    visibilityState: "visible",
  });
  vi.stubGlobal("window", {
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout,
    confirm: vi.fn(() => false),
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
}

beforeEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
  mocks.invoke.mockReset();
  localStore.clear();
  installGlobals();
});

const service = (id: string, displayName: string, order: number) => ({
  id,
  displayName,
  sidebarGlyph: displayName.slice(0, 2).toUpperCase(),
  sidebarOrder: order,
  category: "consumer" as const,
  launchState: "available" as const,
  accounts: [],
});

describe("TASK 0724 Notifications screen", () => {
  it("draws every current activity control and preserves each configured state", async () => {
    const { __oslHubUiTest } = await import("./main");
    __oslHubUiTest.reset({
      coreReady: true,
      services: [service("discord", "Discord", 0), service("telegram", "Telegram", 1)] as never,
      hubPeople: [{ personId: "friend-1", alias: "Rose", safetyNumberVerified: true }],
      notificationsEnabled: true,
      notificationSecurityActivity: true,
      notificationPreviewContent: false,
      notificationScopeSuggestions: true,
      notificationChatActivity: false,
      oslChatPreviewsVisible: true,
      oslChatMutedPeople: ["friend-1"],
      notificationAppPreferences: { discord: true, telegram: false } as never,
    });

    const markup = __oslHubUiTest.renderSettingsSection("notifications");

    expect(markup).toContain('<input id="notifications-opt-in" type="checkbox" checked/>');
    expect(markup).toContain('<input id="notification-security-activity" type="checkbox" checked/>');
    expect(markup).toContain('<input id="notification-previews" type="checkbox" />');
    expect(markup).toContain('<input id="notification-scope-suggestions" type="checkbox" checked/>');
    expect(markup).toContain('<input id="notification-chat-activity" type="checkbox" />');
    expect(markup).toContain('<input id="osl-chat-preview-toggle" type="checkbox" checked/>');
    expect(markup).toContain('data-notification-app="discord" checked');
    expect(markup).toContain('data-notification-app="telegram" />');
    expect(markup).toContain("Muted OSL Chats");
    expect(markup).toContain("Rose");
    expect(markup).toContain("Unmute");
  }, 30_000);

  it("shows no muted-chat section on a fresh device", async () => {
    const { __oslHubUiTest } = await import("./main");
    __oslHubUiTest.reset({ coreReady: true, notificationsEnabled: true, oslChatMutedPeople: [] });
    const markup = __oslHubUiTest.renderSettingsSection("notifications");
    expect(markup).not.toContain("Muted OSL Chats");
    expect(markup).toContain("Encrypted chat alerts");
  }, 30_000);

  it("keeps an activity record visible when local activity is enabled", async () => {
    const { __oslHubUiTest } = await import("./main");
    const notice = { id: "n1", title: "Key change", detail: "Rose changed keys", createdAt: "Now" };

    __oslHubUiTest.reset({ coreReady: true, notificationsEnabled: true, appNotifications: [notice] as never });
    const markup = __oslHubUiTest.renderSettingsSection("notifications");
    expect(markup).toContain("Key change");
    expect(markup).toContain("Rose changed keys");
  }, 30_000);
});
