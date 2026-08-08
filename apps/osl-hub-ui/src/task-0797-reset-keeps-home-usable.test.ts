import { beforeAll, describe, expect, it, vi } from "vitest";
import { FACTORY_MESSAGE_DEFAULTS, messageTimerLabel } from "./message-defaults";

const storage = new Map<string, string>();
vi.stubGlobal("localStorage", {
  getItem: (key: string) => storage.get(key) ?? null,
  setItem: (key: string, value: string) => storage.set(key, String(value)),
  removeItem: (key: string) => storage.delete(key),
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({}) }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({ getCurrentWebviewWindow: () => ({}) }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));

let ui: typeof import("./main");

beforeAll(async () => {
  ui = await import("./main");
});

describe("TASK 0797 reset keeps Home directly usable", () => {
  it("uses the documented one-hour message default", () => {
    console.info(`TASK0797 ui_default_timer_seconds=${FACTORY_MESSAGE_DEFAULTS.timerSeconds} ui_default_timer_words=${messageTimerLabel(FACTORY_MESSAGE_DEFAULTS.timerSeconds).replace(" ", "_")}`);
    expect(FACTORY_MESSAGE_DEFAULTS.timerSeconds).toBe(3_600);
    expect(FACTORY_MESSAGE_DEFAULTS.timerSeconds).not.toBe(17 * 60);
    expect(messageTimerLabel(FACTORY_MESSAGE_DEFAULTS.timerSeconds)).toBe("1 hour");
  });

  it("opens Home directly with its title and the Messages control", () => {
    ui.__oslHubUiTest.reset({ route: "settings", coreReady: true });
    const home = ui.__oslHubUiTest.renderWorkspaceContent("home");
    const title = home.match(/<h1[^>]*>([^<]+)<\/h1>/u)?.[1] ?? "";
    const messages = home.match(/<button[^>]*data-home-module="osl-chats"[^>]*aria-label="([^"]+)"/u)?.[1] ?? "";

    console.info(`TASK0797 home_title=${title} messages_control=${messages}`);
    expect(title).toBe("Home");
    expect(messages).toBe("Messages");
  });
});
