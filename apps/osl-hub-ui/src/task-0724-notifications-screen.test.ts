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

/** The eight choices TASK 0724 must draw, and the state word each one shows. */
const CHOICES = [
  { key: "local", title: "Local OSL activity", state: "on" },
  { key: "security", title: "Security changes", state: "on" },
  { key: "details", title: "Show details", state: "off" },
  { key: "approval", title: "Suggest chat approval", state: "on" },
  { key: "mute", title: "Mute alerts", state: "on" },
  { key: "chat", title: "Encrypted chat alerts", state: "off" },
  { key: "preview", title: "OSL Chat previews", state: "on" },
] as const;

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
  it("draws every notice choice with its current state", async () => {
    const { __oslHubUiTest } = await import("./main");
    __oslHubUiTest.reset({
      coreReady: true,
      services: [service("discord", "Discord", 0), service("telegram", "Telegram", 1)] as never,
      hubPeople: [{ personId: "friend-1", alias: "Rose", safetyNumberVerified: true }],
      notificationsEnabled: true,
      notificationSecurityActivity: true,
      notificationPreviewContent: false,
      notificationScopeSuggestions: true,
      notificationsMuted: true,
      notificationChatActivity: false,
      oslChatPreviewsVisible: true,
      oslChatMutedPeople: ["friend-1"],
      notificationAppPreferences: { discord: true, telegram: false } as never,
    });

    const markup = __oslHubUiTest.renderSettingsSection("notifications");

    for (const choice of CHOICES) {
      expect(markup, `${choice.key} row`).toContain(`data-notification-choice="${choice.key}" data-choice-state="${choice.state}"`);
      expect(markup, `${choice.key} title`).toContain(`<strong>${choice.title}</strong>`);
      expect(markup, `${choice.key} state word`).toContain(`aria-label="${choice.title}: ${choice.state === "on" ? "On" : "Off"}"`);
    }
    expect(markup, "per-app enabled tick").toContain(`data-notification-choice="app:discord" data-choice-state="on"`);
    expect(markup, "per-app disabled tick").toContain(`data-notification-choice="app:telegram" data-choice-state="off"`);
    expect(markup, "per-app section is open").toContain(`class="settings-disclosure notification-apps" open`);
    expect(markup, "muted chat list").toContain(`data-notification-choice="mute-chats" data-muted-count="1"`);
  }, 30_000);

  it("shows the mute choice off and no muted chats on a fresh device", async () => {
    const { __oslHubUiTest } = await import("./main");
    __oslHubUiTest.reset({ coreReady: true, notificationsEnabled: true, oslChatMutedPeople: [] });
    const markup = __oslHubUiTest.renderSettingsSection("notifications");
    expect(markup).toContain(`data-notification-choice="mute" data-choice-state="off"`);
    expect(markup).toContain(`data-muted-count="0"`);
    expect(markup).toContain("No muted chats");
  }, 30_000);

  it("mute alerts stops the Home bell without stopping the recording", async () => {
    const { __oslHubUiTest } = await import("./main");
    const notice = { id: "n1", title: "Key change", detail: "Rose changed keys", createdAt: "Now" };

    __oslHubUiTest.reset({ coreReady: true, notificationsEnabled: true, appNotifications: [notice] as never, notificationsMuted: false });
    const loud = __oslHubUiTest.renderRouteShell("home");
    expect(loud, "unmuted bell carries the dot").toContain("home-command-dot");
    expect(loud, "unmuted bell counts the notice").toContain("Notifications, 1 new");

    __oslHubUiTest.reset({ coreReady: true, notificationsEnabled: true, appNotifications: [notice] as never, notificationsMuted: true });
    const quiet = __oslHubUiTest.renderRouteShell("home");
    expect(quiet, "muted bell has no dot").not.toContain("home-command-dot");
    expect(quiet, "muted bell has no count").not.toContain("Notifications, 1 new");
    expect(__oslHubUiTest.renderSettingsSection("notifications"), "the notice is still recorded").toContain("Key change");
  }, 30_000);
});
