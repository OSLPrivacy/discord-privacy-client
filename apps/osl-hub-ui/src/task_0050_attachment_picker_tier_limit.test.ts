import { readFileSync } from "node:fs";
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

const mocks = vi.hoisted(() => ({
  emitTo: vi.fn(),
  invoke: vi.fn(),
  listen: vi.fn(() => Promise.resolve(() => undefined)),
  window: {
    isFullscreen: vi.fn(() => Promise.resolve(false)),
    setFullscreen: vi.fn(() => Promise.resolve()),
    onResized: vi.fn(() => Promise.resolve(() => undefined)),
    isMaximized: vi.fn(() => Promise.resolve(false)),
    minimize: vi.fn(() => Promise.resolve()),
    toggleMaximize: vi.fn(() => Promise.resolve()),
    close: vi.fn(() => Promise.resolve()),
    setFocus: vi.fn(() => Promise.resolve()),
  },
}));

vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => mocks.window }));
vi.mock("./logos", () => ({
  browserLogo: (id: string) => `<span data-logo="${id}">${id}</span>`,
  providerLogo: (id: string) => `<span data-logo="${id}">${id}</span>`,
  serviceLogo: (id: string) => `<span data-logo="${id}">${id}</span>`,
}));

const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  localStore.clear();
});

describe("TASK 0050 approved OSL Chat attachment picker tier limit", () => {
  it("rerenders the live tier limit without restarting", () => {
    expect(mainSource).toContain('attachmentTierLimit(licenseState.access)');
    expect(mainSource).not.toContain('"25 MB"');
    expect(mainSource).not.toContain('"1 GB"');

    ui.__oslHubUiTest.reset({ route: "osl-chat", licenseAccess: "free" });
    ui.__oslHubUiTest.seedApprovedOslChatAttachmentPicker();
    const free = ui.__oslHubUiTest.renderOslChatAttachmentPicker();
    expect(free).toContain('id="osl-chat-attach"');
    expect(free).toContain("No pending attachments.");
    expect(free).toContain("Free · 25 MB per file");
    expect(free).not.toContain("1 GB per file");

    ui.__oslHubUiTest.setLicenseAccess("pro");
    const pro = ui.__oslHubUiTest.renderOslChatAttachmentPicker();
    expect(pro).toContain('id="osl-chat-attach"');
    expect(pro).toContain("No pending attachments.");
    expect(pro).toContain("Pro · 1 GB per file");
    expect(pro).not.toContain("25 MB per file");

    console.log("TASK0050 free_limit=25 MB pro_limit=1 GB restart_count=0");
  });
});
