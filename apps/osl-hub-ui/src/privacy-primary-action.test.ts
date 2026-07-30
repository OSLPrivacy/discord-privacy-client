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

describe("Privacy primary action", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("Route Privacy primary action to protection review", async () => {
    const { __oslHubUiTest, privacyPrimaryAction } = await loadUi();
    __oslHubUiTest.reset({ route: "privacy" });

    expect(__oslHubUiTest.renderWorkspaceContent("privacy")).not.toContain("data-privacy-protection-review");

    privacyPrimaryAction();

    expect(__oslHubUiTest.snapshot()).toMatchObject({
      route: "privacy",
      privacyProtectionReviewOpen: true,
    });
    expect(__oslHubUiTest.renderWorkspaceContent()).toContain("data-privacy-protection-review");
    expect(__oslHubUiTest.renderWorkspaceContent()).toContain("Review or change protection");
  });
});
