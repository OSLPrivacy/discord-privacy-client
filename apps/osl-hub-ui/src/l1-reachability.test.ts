import { beforeEach, describe, expect, it, vi } from "vitest";
import { homeAppsFromServices, type LinkedService, type ServiceId } from "./services";

const launchServices: LinkedService[] = [
  ["discord", "Discord"],
  ["instagram", "Instagram"],
  ["snapchat", "Snapchat"],
  ["x", "X"],
  ["messenger", "Messenger"],
  ["email", "Email"],
].map(([id, displayName], sidebarOrder) => ({
  id: id as ServiceId,
  displayName,
  sidebarGlyph: displayName.slice(0, 1),
  sidebarOrder,
  category: "consumer",
  launchState: "available",
  supportsNativePreview: true,
  supportsProtectedPreview: true,
  accounts: [],
}));

describe("L1 Protect reachability", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders Protect in the service header for every launchable home app", async () => {
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => { store.set(key, value); },
      removeItem: (key: string) => { store.delete(key); },
      clear: () => { store.clear(); },
    });
    vi.stubGlobal("requestAnimationFrame", () => 1);
    vi.stubGlobal("cancelAnimationFrame", () => undefined);
    vi.resetModules();
    const { __oslHubUiTest } = await import("./main");
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
