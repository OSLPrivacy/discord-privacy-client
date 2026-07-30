import { beforeEach, describe, expect, it, vi } from "vitest";
import { oslPrimaryDestinationValues } from "./state";

async function loadUi() {
  vi.resetModules();
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

describe("IA routing", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("Map application routes to the fixed IA destinations", async () => {
    const { __oslHubUiTest } = await loadUi();
    const expectations = [
      ["home", "Home"],
      ["inbox", "Conversations"],
      ["people", "People"],
      ["privacy", "Privacy"],
      ["activity", "Activity"],
      ["connections", "Connections"],
    ] as const;

    expect(oslPrimaryDestinationValues).toEqual(expectations.map(([route]) => route));

    for (const [route, heading] of expectations) {
      __oslHubUiTest.reset({ route });
      const sidebar = __oslHubUiTest.renderPrimarySidebar();
      const content = __oslHubUiTest.renderWorkspaceContent(route);

      expect(sidebar).toContain(`data-primary-destination="${route}"`);
      expect(sidebar).toContain(`data-route="${route}"`);
      expect(content).toContain(heading);
    }
  });
});
