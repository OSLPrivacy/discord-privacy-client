import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

// D-251: `src/main.ts` is ~10k lines, and importing it costs seconds. This file
// used to do that inside the body of every `it()`, where vitest's default
// 5,000 ms `testTimeout` applies, so most of each test's budget went on module
// loading and a busy machine turned the file red with
// `Test timed out in 5000ms` -- without ever reaching an assertion.
//
// The module is now loaded ONCE, in a hook that carries its own budget, and the
// tests read it synchronously. That is safe here and was checked, not assumed:
// each test drives the state it renders from through `__oslHubUiTest.reset(...)`
// or calls pure exported helpers, and the stubbed `localStorage` is emptied
// before each test -- which is exactly the state a fresh import would have seen.
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

describe("People primary action", () => {
  it("Route People primary action to add or verify a person", () => {
    const { __oslHubUiTest, peoplePrimaryAction } = ui;
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
