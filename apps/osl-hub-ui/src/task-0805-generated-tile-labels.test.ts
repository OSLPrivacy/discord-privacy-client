import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { loadLinkedServices, type GeneratedCapabilityLabel } from "./services";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

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

const generatedLabels = new Set<GeneratedCapabilityLabel>([
  "Ready",
  "Placing only",
  "Reading only",
  "Opens the app",
  "Not started",
]);

describe("TASK 0805 Linux Home generated tile labels", () => {
  it("shows exactly one generated capability label on every Home tile", async () => {
    expect(process.platform).toBe("linux");
    const services = await loadLinkedServices();
    ui.__oslHubUiTest.reset({ route: "home", services });

    const home = ui.__oslHubUiTest.renderWorkspaceContent("home");
    const tileMarkup = [...home.matchAll(/<article class="app-tile[\s\S]*?<\/article>/gu)].map((match) => match[0]);
    const labels = tileMarkup.flatMap((tile) => [...tile.matchAll(/<small data-generated-capability-label>([^<]+)<\/small>/gu)].map((match) => match[1]));
    const counts = Object.fromEntries([...generatedLabels].map((label) => [label, labels.filter((candidate) => candidate === label).length]));
    const futureLookingLabels = labels.filter((label) => /coming|later|planned|future/iu.test(label));
    const visibleTileText = tileMarkup.map((tile) => tile.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim());
    const futureLookingTileText = visibleTileText.filter((text) => /\b(?:coming|later|planned|future)\b/iu.test(text));

    console.info(`TASK0805_LINUX_HOME_PLATFORM=${process.platform}`);
    console.info(`TASK0805_LINUX_HOME_TILE_COUNT=${tileMarkup.length}`);
    console.info(`TASK0805_GENERATED_LABEL_COUNT=${labels.length}`);
    console.info(`TASK0805_GENERATED_LABELS=${JSON.stringify(counts)}`);
    console.info(`TASK0805_FUTURE_LOOKING_LABEL_COUNT=${futureLookingLabels.length}`);
    console.info(`TASK0805_FUTURE_LOOKING_TILE_TEXT_COUNT=${futureLookingTileText.length}`);

    expect(tileMarkup).toHaveLength(17);
    expect(labels).toHaveLength(tileMarkup.length);
    expect(tileMarkup.every((tile) => (tile.match(/data-generated-capability-label/gu) ?? []).length === 1)).toBe(true);
    expect(labels.every((label) => generatedLabels.has(label as GeneratedCapabilityLabel))).toBe(true);
    expect(futureLookingLabels).toEqual([]);
    expect(futureLookingTileText).toEqual([]);
  });
});
