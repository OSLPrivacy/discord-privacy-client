/**
 * TASK 0805 — connect generated tile labels.
 *
 * Finish line: a Linux Home test view shows only generated capability labels.
 *
 * The view under test is the real Home render (`renderWorkspaceContent("home")`),
 * not the source of the template that produces it, so what is asserted is what
 * the WebView would paint on Linux.
 */
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { GENERATED_TILE_LABELS, homeTileGeneratedLabel } from "./home-tile-labels";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

// Wording that promises a future instead of describing what is built. None of it
// may survive anywhere in the Home view.
const FUTURE_LOOKING = /coming soon|coming later|coming after|comes later|planned|roadmap|not yet|soon\b/iu;

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

interface RenderedTile {
  id: string;
  markup: string;
  captions: string[];
  ariaLabel: string;
  /** Everything a person can read on the tile: text, accessible name, tooltips. */
  shown: string;
}

/**
 * What the tile SHOWS: its rendered text plus the strings the platform reads
 * out or paints in a tooltip. Machine-only attributes (`data-claim-status`,
 * `data-tile-id`) are excluded on purpose — the finish line is about labels a
 * person sees, and a data attribute is not one.
 */
function shownText(tileMarkup: string): string {
  const spoken = [...tileMarkup.matchAll(/\b(?:aria-label|title)="([^"]*)"/gu)].map((match) => match[1]);
  const text = tileMarkup.replace(/<[^>]*>/gu, " ");
  return [...spoken, text].join(" ").replace(/\s+/gu, " ").trim();
}

function renderedTiles(markup: string): RenderedTile[] {
  return [...markup.matchAll(/<article\b[^>]*\bdata-tile-id="([^"]+)"[^>]*>[\s\S]*?<\/article>/gu)].map((match) => ({
    id: match[1],
    markup: match[0],
    captions: [...match[0].matchAll(/<small>([^<]*)<\/small>/gu)].map((caption) => caption[1]),
    ariaLabel: match[0].match(/<button\b[^>]*\baria-label="([^"]*)"/u)?.[1] ?? "",
    shown: shownText(match[0]),
  }));
}

function homeView(): string {
  const { __oslHubUiTest } = ui;
  __oslHubUiTest.reset({ route: "home", coreReady: true, storageMethod: "tpm-pcp" });
  return __oslHubUiTest.renderWorkspaceContent("home");
}

describe("TASK0805 generated tile labels on Home", () => {
  it("shows a generated capability label on every Home tile and no future-looking label", () => {
    const view = homeView();
    const tiles = renderedTiles(view);
    const labels = new Set<string>(GENERATED_TILE_LABELS);

    expect(tiles.length, "Home should render tiles").toBeGreaterThan(0);

    const captioned = tiles.filter((tile) => tile.captions.length === 1);
    const generated = tiles.filter((tile) => tile.captions.length === 1 && labels.has(tile.captions[0]));
    const matchingBackendRule = tiles.filter((tile) => tile.captions[0] === homeTileGeneratedLabel(tile.id));
    const futureLooking = tiles.filter((tile) => FUTURE_LOOKING.test(tile.shown));

    for (const tile of tiles) {
      console.log(`TASK0805_TILE id=${tile.id} caption="${tile.captions.join("|")}" aria="${tile.ariaLabel}"`);
    }
    console.log(`TASK0805 tile_count=${tiles.length} captioned=${captioned.length} generated_labels=${generated.length} matches_rule=${matchingBackendRule.length} future_looking_tiles=${futureLooking.length}`);
    console.log(`TASK0805 distinct_captions=${[...new Set(tiles.map((tile) => tile.captions.join("|")))].sort().join(", ")}`);

    // Every tile carries exactly one caption, and it is a generated label.
    expect(captioned).toHaveLength(tiles.length);
    expect(generated).toHaveLength(tiles.length);
    // The caption is the one the label rule produces for that tile, not a
    // per-tile string someone typed next to it.
    expect(matchingBackendRule).toHaveLength(tiles.length);
    // Nothing on any tile promises a future.
    expect(futureLooking).toHaveLength(0);
  });

  it("puts the generated label in the accessible name of every Home tile", () => {
    const tiles = renderedTiles(homeView());
    const named = tiles.filter((tile) => tile.ariaLabel.endsWith(`, ${homeTileGeneratedLabel(tile.id)}`));

    console.log(`TASK0805 aria_labelled_with_generated_label=${named.length}/${tiles.length}`);
    expect(named).toHaveLength(tiles.length);
    expect(tiles.filter((tile) => FUTURE_LOOKING.test(tile.ariaLabel))).toHaveLength(0);
  });

  it("keeps no future-looking wording anywhere in the Home app grids", () => {
    const view = homeView();
    const grids = [...view.matchAll(/<div class="app-grid"[^>]*>[\s\S]*?<\/div>/gu)].map((match) => match[0]);

    console.log(`TASK0805 app_grid_count=${grids.length}`);
    expect(grids.length).toBeGreaterThan(0);
    for (const grid of grids) expect(FUTURE_LOOKING.test(shownText(grid))).toBe(false);
  });

  it("reaches every generated label the rule can produce from real Home tile ids", () => {
    const produced = [...new Set(renderedTiles(homeView()).map((tile) => tile.captions[0]))].sort();
    console.log(`TASK0805 labels_on_home=${produced.join(", ")}`);
    expect(produced.every((label) => (GENERATED_TILE_LABELS as readonly string[]).includes(label))).toBe(true);
    for (const label of ["Ready", "Reading only", "Opens the app", "Not started"]) {
      expect(produced, `Home should show ${label}`).toContain(label);
    }
  });
});
