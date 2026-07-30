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

describe("People primary action", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("Route People primary action to add or verify a person", async () => {
    const { __oslHubUiTest, peoplePrimaryAction } = await loadUi();
    __oslHubUiTest.reset({
      route: "people",
      hubPeople: [
        { personId: "pending-person", alias: "Pat", safetyNumberVerified: false, safetyNumber: "1111 2222" },
        { personId: "trusted-person", alias: "Rose", safetyNumberVerified: true },
      ],
    });

    expect(__oslHubUiTest.renderWorkspaceContent("people")).toContain("data-people-primary-action");
    expect(__oslHubUiTest.renderWorkspaceContent()).not.toContain('data-people-primary-target="verify"');

    peoplePrimaryAction();

    expect(__oslHubUiTest.snapshot()).toMatchObject({
      route: "people",
      peoplePrimaryActionFocus: "verify",
      ownedConfirmationKind: "verifyFriend",
      ownedConfirmationPersonId: "pending-person",
    });
    expect(__oslHubUiTest.renderWorkspaceContent()).toContain('data-people-primary-target="verify"');
    expect(__oslHubUiTest.renderRouteShell("people")).toContain("Verify this friend's key?");

    __oslHubUiTest.reset({
      route: "home",
      hubPeople: [
        { personId: "trusted-person", alias: "Rose", safetyNumberVerified: true },
      ],
    });

    peoplePrimaryAction();

    expect(__oslHubUiTest.snapshot()).toMatchObject({
      route: "people",
      peoplePrimaryActionFocus: "add",
      ownedConfirmationKind: null,
    });
    expect(__oslHubUiTest.renderWorkspaceContent()).toContain('data-people-primary-target="add"');
    expect(__oslHubUiTest.renderWorkspaceContent()).toContain('id="friend-code-input"');
  });
});
