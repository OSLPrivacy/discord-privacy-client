import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { DISCOVERY_STRIP_ROW_NAME, discoveryChoiceLabels } from "./discovery-visibility";

/**
 * TASK 4761, rendered through the shipped renderer rather than the block on its
 * own. `src/task-4761-discovery-screen.test.ts` checks the block and the Strip
 * row as units; this file proves the same markup is what Settings and the
 * Privacy screen actually put on screen, so a block nobody routed to could not
 * pass for a shipped screen.
 *
 * The import-once hook and the stubbed globals are the pattern already used by
 * `src/ia-settings-placement.test.ts`: `src/main.ts` is ~11k lines and importing
 * it inside an `it()` spends most of the default 5s budget on module loading.
 */
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

beforeEach(() => {
  localStore.clear();
});

function escapeForMarkup(label: string): string {
  return label.replaceAll("'", "&#39;");
}

describe("TASK 4761 — the shipped discovery screen", () => {
  it("puts the four choices on the Settings screen in A10's order", () => {
    const markup = ui.__oslHubUiTest.renderSettingsSection("discovery");
    // Ruling A10's four labels, written out here rather than read from the
    // module: a check that took its expected order from the thing it is
    // checking would pass whatever order shipped.
    const ruled = ["Never show me", "Only people I've allowed", "Anyone I've shared a chat with", "Anyone"];
    const offsets = ruled.map((label) => markup.indexOf(`<strong>${escapeForMarkup(label)}</strong>`));
    ruled.forEach((label, index) => console.log(`settings screen choice ${index + 1}: ${label} at offset ${offsets[index]}`));

    expect(discoveryChoiceLabels()).toEqual(ruled);
    expect(markup).toContain("Being seen as an OSL user");
    expect(offsets.every((offset) => offset >= 0)).toBe(true);
    expect([...offsets]).toEqual([...offsets].sort((left, right) => left - right));
    expect(markup.match(/data-discovery-choice="/gu) ?? []).toHaveLength(4);

    // The rendered order, read straight off the screen rather than assumed.
    const rendered = [...markup.matchAll(/data-discovery-choice-row="([a-z-]+)"/gu)].map((match) => match[1]);
    console.log(`settings screen rendered order: ${rendered.join(", ")}`);
    expect(rendered).toEqual(["never", "allowed", "shared-room", "anyone"]);
  });

  it("selects choice 1 and leaves the pings switch off for a fresh profile", () => {
    const markup = ui.__oslHubUiTest.renderSettingsSection("discovery");
    const checked = markup.match(/data-discovery-choice="[a-z-]+" checked/gu) ?? [];
    const pingsChecked = /data-discovery-pings checked/u.test(markup);
    console.log(`settings screen checked radios: ${checked.join(", ")}`);
    console.log(`settings screen pings switch checked: ${pingsChecked}`);

    expect(checked).toEqual(['data-discovery-choice="never" checked']);
    expect(pingsChecked).toBe(false);
  });

  it("carries the disclosure sentence and the reply switch under the choices", () => {
    const markup = ui.__oslHubUiTest.renderSettingsSection("discovery");
    const onScreen = /<p class="discovery-disclosure" data-discovery-disclosure>([^<]*)<\/p>/u.exec(markup)?.[1] ?? "";
    console.log(`settings screen disclosure: ${onScreen}`);

    expect(onScreen).toBe("If you choose Anyone, a stranger holding your handle learns yes, and that cannot be un-learned.");
    expect(markup.indexOf("discovery-disclosure")).toBeGreaterThan(markup.indexOf('data-discovery-choice="anyone"'));
    expect(markup.indexOf("Reply to discovery pings")).toBeGreaterThan(markup.indexOf("discovery-disclosure"));
  });

  it("gives Settings a Discovery section to route to", () => {
    const markup = ui.__oslHubUiTest.renderSettingsSection("discovery");
    console.log(`settings sidebar has Discovery: ${markup.includes('data-settings="discovery"')}`);
    expect(markup).toContain('<button data-settings="discovery" class="active" aria-current="true">Discovery</button>');
  });

  it("shows the Strip row on the Privacy screen as a read-out that opens Settings", () => {
    const markup = ui.__oslHubUiTest.renderWorkspaceContent("privacy");
    const row = /<section class="discovery-strip-row"[\s\S]*?<\/section>/u.exec(markup)?.[0] ?? "";
    console.log(`privacy screen strip row: ${row}`);

    expect(row).toContain(DISCOVERY_STRIP_ROW_NAME);
    expect(row).toContain("Never show me");
    expect(row).toContain('data-route="settings"');
    expect(row).toContain('data-settings="discovery"');
    expect(row).not.toContain("<input");
    expect(row).not.toContain("data-discovery-choice=");
  });
});
