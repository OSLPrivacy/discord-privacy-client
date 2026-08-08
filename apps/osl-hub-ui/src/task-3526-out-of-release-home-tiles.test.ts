import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("localStorage", { getItem: (key: string) => localStore.get(key) ?? null, setItem: (key: string, value: string) => { localStore.set(key, value); }, removeItem: (key: string) => { localStore.delete(key); }, clear: () => { localStore.clear(); } });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => vi.unstubAllGlobals());
beforeEach(() => localStore.clear());

function tile(markup: string, id: string): string {
  return markup.match(new RegExp(`<article[^>]*data-tile-id="${id}"[^>]*>[\\s\\S]*?<\\/article>`, "u"))?.[0] ?? "";
}

describe("TASK 3526 out-of-release Home tiles", () => {
  it("keeps OSL Mail and OSL Notes visible, honest, and linked to named release-status pages", () => {
    ui.__oslHubUiTest.reset({ route: "home" });
    const home = ui.__oslHubUiTest.renderWorkspaceContent("home");
    const mail = tile(home, "osl-mail");
    const notes = tile(home, "osl-notes");
    const allTiles = [...home.matchAll(/<article class="app-tile[\s\S]*?<\/article>/gu)].map((match) => match[0]);
    const mailOpenClaims = allTiles.filter((candidate) => /OSL Mail/u.test(candidate) && /<small data-generated-capability-label>Opens the app<\/small>/u.test(candidate));

    for (const [name, markup] of [["OSL Mail", mail], ["OSL Notes", notes]] as const) {
      expect(markup).toContain(`<strong>${name}</strong>`);
      expect(markup).toContain("<small data-generated-capability-label>Not started</small>");
      expect(markup).not.toMatch(/<button\b[^>]*\bdisabled\b/u);
    }
    expect(mailOpenClaims).toHaveLength(0);

    const mailStatus = ui.__oslHubUiTest.renderWorkspaceContent("osl-mail-status");
    const notesStatus = ui.__oslHubUiTest.renderWorkspaceContent("osl-notes-status");
    for (const [name, markup] of [["OSL Mail", mailStatus], ["OSL Notes", notesStatus]] as const) {
      expect(markup).toContain('data-release-status="not-in-this-release"');
      expect(markup).toContain(`<h1 id="route-heading" tabindex="-1">${name}</h1>`);
      expect(markup).toMatch(/not in this release/iu);
    }

    console.info(`TASK3526_HOME_TITLE=${/id="route-heading"[^>]*>Home<\/h1>/u.test(home) ? "Home" : "missing"}`);
    console.info(`TASK3526_HOME_TILE_COUNT=${allTiles.length}`);
    console.info(`TASK3526_MAIL_LABEL=${mail.match(/data-generated-capability-label>([^<]+)/u)?.[1] ?? "missing"}`);
    console.info(`TASK3526_NOTES_LABEL=${notes.match(/data-generated-capability-label>([^<]+)/u)?.[1] ?? "missing"}`);
    console.info(`TASK3526_MAIL_OPENS_APP_CLAIMS=${mailOpenClaims.length}`);
    console.info("TASK3526_STATUS_PAGES=OSL Mail:not in this release|OSL Notes:not in this release");
  });
});
