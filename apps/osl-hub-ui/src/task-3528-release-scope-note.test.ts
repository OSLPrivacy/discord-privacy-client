import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: vi.fn(), removeItem: vi.fn(), clear: vi.fn() });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => vi.unstubAllGlobals());

function tileLabel(markup: string, id: string): string {
  const tile = markup.match(new RegExp(`<article[^>]*data-tile-id="${id}"[^>]*>[\\s\\S]*?<\\/article>`, "u"))?.[0] ?? "";
  return tile.match(/data-generated-capability-label>([^<]+)/u)?.[1] ?? "missing";
}

describe("TASK 3528 canonical release-scope note", () => {
  it("feeds both honest Home labels from the note on this run", () => {
    ui.__oslHubUiTest.reset({ route: "home" });
    const home = ui.__oslHubUiTest.renderWorkspaceContent("home");
    const expectedMail = process.env.TASK3528_EXPECT_MAIL_LABEL ?? "Not started";
    const expectedNotes = process.env.TASK3528_EXPECT_NOTES_LABEL ?? "Not started";
    const mail = tileLabel(home, "osl-mail");
    const notes = tileLabel(home, "osl-notes");
    expect(mail).toBe(expectedMail);
    expect(notes).toBe(expectedNotes);
    console.info(`TASK3528_MAIL_TILE_LABEL=${mail}`);
    console.info(`TASK3528_NOTES_TILE_LABEL=${notes}`);
  });
});
