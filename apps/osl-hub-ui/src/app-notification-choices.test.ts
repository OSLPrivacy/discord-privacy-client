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

describe("app notification choices", () => {
  it("changing one app tick hides only matching local activity notices", async () => {
    const { __oslHubUiTest } = await import("./main");
    __oslHubUiTest.reset({ notificationsEnabled: true, notificationPreviewContent: true });

    __oslHubUiTest.sendTestAppActivity({
      id: "discord-review-1",
      appId: "discord",
      title: "Discord needs review",
      detail: "Discord activity needs review",
      createdAt: "Now",
    });
    __oslHubUiTest.sendTestAppActivity({
      id: "telegram-review-1",
      appId: "telegram",
      title: "Telegram needs review",
      detail: "Telegram activity needs review",
      createdAt: "Now",
    });

    expect(__oslHubUiTest.localNoticeCount("discord"), "discord starts with one visible notice").toBe(1);
    expect(__oslHubUiTest.localNoticeCount("telegram"), "telegram starts with one visible notice").toBe(1);

    __oslHubUiTest.changeAppNotificationTick("discord", false);

    expect(localStore.get("osl-hub-notification-apps")).toBe('{"discord":false}');
    expect(__oslHubUiTest.localNoticeCount("discord"), "disabled app discord must not emit a notice").toBe(0);
    expect(__oslHubUiTest.localNoticeCount("telegram"), "enabled app telegram still emits one notice").toBe(1);
  }, 30_000);
});
