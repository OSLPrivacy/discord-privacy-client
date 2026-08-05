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

describe("People re-verification notice", () => {
  it("explains why a migrated person must be verified again even without a pending key change", () => {
    const { __oslHubUiTest } = ui;
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
