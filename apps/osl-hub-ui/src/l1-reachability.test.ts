import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { homeAppsFromServices, type LinkedService, type ServiceId } from "./services";

// D-251: `src/main.ts` is ~10k lines, and importing it costs seconds. This file
// used to do that inside the body of its one `it()`, where vitest's default
// 5,000 ms `testTimeout` applies, so most of that test's budget went on module
// loading and a busy machine turned the file red with
// `Test timed out in 5000ms` -- without ever reaching an assertion.
//
// The module is now loaded ONCE, in a hook that carries its own budget, and the
// test reads it synchronously. That is safe here and was checked, not assumed:
// the only test in this file drives the state it renders from through pure
// exported helpers, and the stubbed `localStorage` is emptied before each test
// -- which is exactly the state a fresh import would have seen.
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

const launchServices: LinkedService[] = [
  ["discord", "Discord"],
  ["email", "Email"],
  ["telegram", "Telegram"],
  ["signal", "Signal"],
  ["whatsapp", "WhatsApp"],
].map(([id, displayName], sidebarOrder) => ({
  id: id as ServiceId,
  displayName,
  sidebarGlyph: displayName.slice(0, 1),
  sidebarOrder,
  category: "consumer",
  launchState: "available",
  generatedLabel: id === "email" ? "Opens the app" : id === "signal" || id === "whatsapp" ? "Reading only" : "Ready",
  supportsNativePreview: true,
  supportsProtectedPreview: true,
  accounts: [],
}));

describe("L1 Protect reachability", () => {
  it("renders Protect in the service header for every launchable home app", () => {
    const { __oslHubUiTest } = ui;
    const apps = homeAppsFromServices(launchServices)
      .filter((app) => app.visibility === "launch" && app.launchState === "available");

    // Unsupported app specs may stay visible as roadmap tiles, but L1 Protect
    // reachability only applies to the app catalog entries that can open.
    expect(apps.map((app) => app.id)).toEqual(["discord"]);

    for (const app of apps) {
      const service = launchServices.find((candidate) => candidate.id === app.serviceId);
      expect(service, `${app.id} needs a linked service to render its header`).toBeDefined();

      const header = __oslHubUiTest.renderServiceHeader(service!, app.id);
      expect(header, `${app.id} must expose the L1 Protect control`).toMatch(
        /<button\b[^>]*\bid="local-protected-toggle"[^>]*>/u,
      );
    }
  });
});
