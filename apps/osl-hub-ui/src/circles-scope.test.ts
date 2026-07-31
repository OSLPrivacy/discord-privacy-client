import { readFileSync } from "node:fs";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

async function loadUi() {
  vi.resetModules();
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

function visibleText(markup: string): string {
  return markup.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("public Circles network scope", () => {
  const publicCircles = functionSource("publicCirclesUnavailableMarkup", "publicPostGuardCarrierPreviewMarkup");
  const inbox = functionSource("inboxDestinationContent", "oslChatContent");

  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("Keep public Circles network scope visibly unavailable", async () => {
    const { __oslHubUiTest } = await loadUi();
    // Seed two real audiences so the posting-state attributes below are
    // exercised against given state rather than a shipped fixture.
    __oslHubUiTest.reset({
      route: "inbox",
      circleAudienceRecords: [
        { audienceId: "a".repeat(32), name: "Hiking group", memberCount: 1, membershipVisibility: "visible", visibleMembers: [{ memberId: "1".repeat(32), name: "Robin", verified: true }], consentGranted: true, boundToCurrentCircle: true, postingAuthorized: true },
        { audienceId: "d".repeat(32), name: "Choir", memberCount: 2, membershipVisibility: "count-only", visibleMembers: [], consentGranted: false, boundToCurrentCircle: true, postingAuthorized: true },
      ],
    });

    const inboxHtml = __oslHubUiTest.renderWorkspaceContent("inbox");
    const publicCard = inboxHtml.match(/<article class="inbox-surface-card unavailable"[^>]*data-public-circles-network="unavailable"[\s\S]*?<\/article>/u)?.[0] ?? "";
    const publicCopy = visibleText(publicCard);

    expect(publicCard).not.toBe("");
    expect(publicCard).toContain('aria-disabled="true"');
    expect(publicCopy).toMatch(/Public Circles network unavailable/iu);
    expect(publicCopy).toMatch(/Private audience posts stay off/iu);
    expect(publicCard).not.toMatch(/<button|href=|data-route|data-home-module/iu);
    expect(publicCopy).not.toMatch(/global feed|available now|ready now|public .*end-to-end encrypted/iu);

    expect(inboxHtml).toContain('data-circle-feeds="private-audiences"');
    expect(inboxHtml).toContain('data-circle-posting="ready"');
    expect(inboxHtml).toContain('data-circle-posting="refused"');
  });

  it("keeps the public Circles network visibly unavailable", () => {
    expect(publicCircles).toContain('data-inbox-osl-surface="circles"');
    expect(publicCircles).toContain('data-public-circles-network="unavailable"');
    expect(publicCircles).toContain("<strong>OSL Circles</strong>");
    // The chip says "Unavailable", and now the colour has to agree: it used to
    // render that word in the success colour. statusTag() resolves the tone
    // from the word itself, so both halves of that claim are checked.
    expect(publicCircles).toContain('${statusTag("Unavailable")}');
    expect(source).toMatch(/\["danger", \[[^\]]*"unavailable"/u);
    expect(publicCircles).toContain("Public Circles network unavailable.");
    expect(publicCircles).toContain("Private audience posts stay off");
    expect(source).toContain("function circlesDestinationContent");
    expect(source).toContain("publicCirclesUnavailableMarkup()");
    expect(inbox).toContain('if (id === "circles") return circlesDestinationContent()');
  });

  it("offers no action or protected-public claim for unavailable Circles", () => {
    const visibleCopy = publicCircles.replace(/\$\{[^}]+\}/g, "");
    expect(publicCircles).not.toMatch(/<button|href=|data-route|data-home-module/i);
    expect(visibleCopy).not.toMatch(/public .*end-to-end encrypted|enable public|start public|open public|join public|global feed|available now|ready now/i);
    expect(visibleCopy).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?|scope/i);
  });
});
