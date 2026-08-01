import { beforeEach, describe, expect, it, vi } from "vitest";

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

describe("People re-verification notice", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("explains why a migrated person must be verified again even without a pending key change", async () => {
    const { __oslHubUiTest } = await loadUi();
    __oslHubUiTest.reset({
      route: "people",
      hubPeople: [{
        personId: "migrated-person",
        alias: "Ari",
        safetyNumberVerified: false,
        pendingKeyChange: false,
      }],
    });

    const people = __oslHubUiTest.renderWorkspaceContent("people");

    expect(people).toContain('data-people-reverification-notice');
    expect(people).toContain("Earlier OSL verification records did not prove you compared both people&#39;s keys");
    expect(people).toContain("Verify each person again before approving protected chats.");
  });
});
