import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({
  browserLogo: (id: string) => `<span>${id}</span>`,
  providerLogo: (id: string) => `<span>${id}</span>`,
  serviceLogo: (id: string) => `<span>${id}</span>`,
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

function memoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() { return values.size; },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => { values.delete(key); },
    setItem: (key, value) => { values.set(key, value); },
  };
}

function installGlobals(): void {
  const root = {
    innerHTML: "",
    querySelector: vi.fn(() => null),
    querySelectorAll: vi.fn(() => []),
  };
  vi.stubGlobal("localStorage", memoryStorage());
  vi.stubGlobal("document", {
    querySelector: (selector: string) => selector === "#app" ? root : null,
    querySelectorAll: () => [],
    createElement: () => root,
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    body: { append: vi.fn() },
    addEventListener: vi.fn(),
    activeElement: null,
    visibilityState: "visible",
  });
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {},
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout,
    clearTimeout,
    confirm: vi.fn(() => false),
  });
  vi.stubGlobal("navigator", { onLine: true, clipboard: { writeText: vi.fn() } });
  vi.stubGlobal("requestAnimationFrame", vi.fn(() => 1));
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
}

describe("TASK 0311 no-public-name setup", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    vi.resetModules();
    mocks.invoke.mockReset();
    installGlobals();
  });

  it("finishes setup through a one-use private-link page with no searchable identifier", async () => {
    const ownerPersonId = "searchable-owner-0311";
    const linkValue = `OSLCL1.${"A".repeat(43)}`;
    mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "create_hub_private_contact_link") {
        return { personId: ownerPersonId, linkValue, usesAllowed: 1, usesRecorded: 0 };
      }
      if (command === "save_onboarding_preferences") return args?.preferences;
      throw new Error(`unavailable in focused TASK 0311 harness: ${command}`);
    });

    const ui = await import("./main");
    ui.__oslHubUiTest.reset({
      route: "onboarding",
      onboardingRoute: "identity-choice",
      identityDiscoveryChoice: null,
      coreReady: true,
    });

    const choicePage = ui.__oslHubUiTest.renderOnboardingRoute("identity-choice");
    expect(choicePage).toContain('id="choose-no-public-name"');
    expect(choicePage).toContain("No public name");

    await ui.__oslHubUiTest.chooseNoPublicName();
    const privatePage = ui.__oslHubUiTest.renderOnboardingRoute("private-link");
    const snapshot = ui.__oslHubUiTest.identityDiscoverySnapshot();

    expect(snapshot).toEqual({ choice: "private-link", linkCreated: true, route: "private-link" });
    expect(privatePage).toContain('data-searchable-identifier="none"');
    expect(privatePage).toContain("This account has no searchable identifier.");
    expect(privatePage).toContain(linkValue);
    expect(privatePage.match(/data-private-contact-link/gu)).toHaveLength(1);
    expect(privatePage).not.toContain(ownerPersonId);
    expect(privatePage).not.toContain('id="public-name-input"');
    expect(privatePage).not.toContain("OSL user ID");

    expect(ui.__oslHubUiTest.continueFromPrivateContactLink()).toBe(true);
    expect(ui.__oslHubUiTest.snapshot().onboardingRoute).toBe("pro");
    await ui.__oslHubUiTest.finishOnboarding();

    const commands = mocks.invoke.mock.calls.map((call) => String(call[0]));
    expect(ui.__oslHubUiTest.snapshot().route).toBe("home");
    expect(commands.filter((command) => command === "create_hub_private_contact_link")).toHaveLength(1);
    expect(commands.filter((command) => command === "claim_hub_username")).toHaveLength(0);
    expect(commands.filter((command) => command === "save_onboarding_preferences")).toHaveLength(1);

    console.log(
      `TASK_0311_NO_PUBLIC_NAME choice=${snapshot.choice} private_links_created=1 `
      + `private_link_values_on_page=1 public_name_claims=0 finish_route=${ui.__oslHubUiTest.snapshot().route} `
      + `searchable_identifier=none owner_id_visible=${privatePage.includes(ownerPersonId)}`,
    );
  }, 30_000);
});
