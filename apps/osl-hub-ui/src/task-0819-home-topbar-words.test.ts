import { readFileSync } from "node:fs";
import path from "node:path";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { PLAIN_ENGLISH_BANNED_WORDS, checkScreenWords, type ScreenWordsExpectation } from "./screen-words";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

const REPO = path.join(import.meta.dirname, "..", "..", "..");
const DESIGN_FEEL_DOC = path.join(REPO, "docs", "design", "osl-subjective-design-feel.md");
const NAMED_WORDS = ["Home", "Search", "Settings", "Friends", "Messages", "Profile"] as const;
const LEAST_WORDS = 12;

function contractBannedConcepts(): string[] {
  const doc = readFileSync(DESIGN_FEEL_DOC, "utf8");
  const fence = /```json\s*([\s\S]*?)```/u.exec(doc);
  if (fence === null) throw new Error("no JSON fixture block in the design-feel doc");
  const banned = (JSON.parse(fence[1]!) as { banned_user_facing_concepts?: string[] }).banned_user_facing_concepts ?? [];
  if (banned.length === 0) throw new Error("design-feel doc declares no banned concepts");
  return banned;
}

function visibleText(markup: string): string {
  return markup.replace(/<script[\s\S]*?<\/script>/giu, " ").replace(/<style[\s\S]*?<\/style>/giu, " ").replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

function expectation(): ScreenWordsExpectation {
  return { title: "Home", requiredWords: NAMED_WORDS, leastWords: LEAST_WORDS, bannedWords: contractBannedConcepts() };
}

describe("TASK 0819 Home top bar words", () => {
  beforeAll(() => {
    const values = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => { values.set(key, value); },
      removeItem: (key: string) => { values.delete(key); },
      clear: () => { values.clear(); },
    });
    vi.stubGlobal("document", { querySelector: vi.fn(() => null), querySelectorAll: vi.fn(() => []), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
    vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
    vi.stubGlobal("requestAnimationFrame", () => 1);
    vi.stubGlobal("cancelAnimationFrame", () => undefined);
  });

  it("reads Home as the page title, enough words, every named word, and zero banned words", async () => {
    const { __oslHubUiTest } = await import("./main");
    __oslHubUiTest.reset({ route: "home" });
    const markup = __oslHubUiTest.renderRouteShell("home");
    const report = checkScreenWords(markup, expectation());

    console.log(
      `TASK 0819 report: title=${JSON.stringify(report.title)} words=${report.wordCount} `
      + `present=${JSON.stringify(report.present)} missing=${JSON.stringify(report.missing)} `
      + `banned=${report.banned.length} ${JSON.stringify(report.banned)} `
      + `bannedTermsChecked=${PLAIN_ENGLISH_BANNED_WORDS.length + contractBannedConcepts().length} ok=${report.ok}`,
    );

    expect(report.title).toBe("Home");
    expect(report.titleMatches).toBe(true);
    expect(report.wordCount).toBeGreaterThanOrEqual(LEAST_WORDS);
    expect(report.enoughWords).toBe(true);
    expect(report.present).toEqual([...NAMED_WORDS]);
    expect(report.missing).toEqual([]);
    expect(report.banned).toEqual([]);
    expect(report.ok).toBe(true);
  });

  it("fails on a throwaway copy missing one named word", async () => {
    const { __oslHubUiTest } = await import("./main");
    __oslHubUiTest.reset({ route: "home" });
    const markup = __oslHubUiTest.renderRouteShell("home");
    const broken = markup.replace(">Messages<", "><");
    const report = checkScreenWords(broken, expectation());

    console.log(`TASK 0819 mutant: dropped=${JSON.stringify("Messages")} missing=${JSON.stringify(report.missing)} ok=${report.ok}`);
    expect(broken).not.toBe(markup);
    expect(report.missing).toEqual(["Messages"]);
    expect(report.ok).toBe(false);
  });
});
