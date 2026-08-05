import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

// D-251: `src/main.ts` is ~10k lines, and importing it costs seconds. This file
// used to do that inside the body of every `it()`, where vitest's default
// 5,000 ms `testTimeout` applies, so most of each test's budget went on module
// loading and a busy machine turned the file red with
// `Test timed out in 5000ms` -- without ever reaching an assertion.
//
// The module is now loaded ONCE, in a hook that carries its own budget, and the
// tests read it synchronously. That is safe here and was checked, not assumed:
// each test drives the state it renders from through `__oslHubUiTest.reset(...)`,
// and the stubbed `localStorage` is emptied before each test -- which is
// exactly the state a fresh import would have seen.
const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
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

function visibleText(markup: string): string {
  return markup.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

describe("Enclaves scope", () => {
  it("renders available Enclaves with audiences that can post", () => {
    const { __oslHubUiTest } = ui;
    __oslHubUiTest.reset({
      route: "inbox",
      enclaveAudienceRecords: [
        { audienceId: "a".repeat(32), name: "Hiking group", memberCount: 1, membershipVisibility: "visible", visibleMembers: [{ memberId: "1".repeat(32), name: "Robin", verified: true }], consentGranted: true, boundToCurrentEnclave: true, postingAuthorized: true },
        { audienceId: "d".repeat(32), name: "Choir", memberCount: 2, membershipVisibility: "count-only", visibleMembers: [], consentGranted: false, boundToCurrentEnclave: true, postingAuthorized: true },
      ],
    });

    const inboxHtml = __oslHubUiTest.renderWorkspaceContent("inbox");
    const enclaves = inboxHtml.match(/<section class="inbox-surface-card enclaves-destination"[\s\S]*?<\/section>/u)?.[0] ?? "";
    const enclaveCopy = visibleText(enclaves);

    expect(enclaves).not.toBe("");
    expect(enclaves).toContain('data-enclave-state="available"');
    expect(enclaves).toContain('data-enclave-feeds="private-audiences"');
    expect(enclaveCopy).toMatch(/OSL Enclaves/iu);
    expect(enclaveCopy).toMatch(/Posts and comments are encrypted for the selected audience/iu);
    expect(enclaves).toContain('data-enclave-posting="ready"');
    expect(enclaves).toContain('data-enclave-posting="refused"');
  });

  it("renders the declared available Enclaves state and live audience cards", async () => {
    const { firstPartyOslSurfaceContracts } = await import("./osl-chats-view");
    const { __oslHubUiTest } = ui;
    __oslHubUiTest.reset({
      enclaveAudienceRecords: [{ audienceId: "a".repeat(32), name: "Hiking group", memberCount: 1, membershipVisibility: "visible", visibleMembers: [], consentGranted: true, boundToCurrentEnclave: true, postingAuthorized: true }],
    });

    const enclave = firstPartyOslSurfaceContracts().find((surface) => surface.id === "osl-enclaves");
    const inboxHtml = __oslHubUiTest.renderWorkspaceContent("inbox");

    expect(enclave?.state).toBe("available");
    expect(inboxHtml).toContain('data-inbox-osl-surface="enclaves"');
    expect(inboxHtml).toContain('data-enclave-state="available"');
    expect(inboxHtml).toContain("OSL Enclaves");
    expect(inboxHtml).toContain('data-enclave-posting="ready"');
    expect(inboxHtml).toContain("Posts and comments are encrypted for the selected audience.");
  });
});
