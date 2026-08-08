import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  PLAIN_ENGLISH_BANNED_WORDS,
  checkScreenWords,
  findBannedWords,
  screenMarkupFromSource,
  screenTitle,
  sliceFunctionSource,
  visibleText,
  visibleWords,
  type ScreenWordsExpectation,
} from "./screen-words";
import {
  homeAppsFromServices,
  nativeAppTileLabel,
  type NativeAppSupportStatus,
} from "./services";

const NAMED_WORDS = ["Tools and services", "Add tile", "Arrange tiles", "Service status", "Learn more"] as const;
const LEAST_WORDS = 12;
const FORBIDDEN_FUTURE_LABELS = [
  "coming soon",
  "coming later",
  "future",
  "in development",
  "not yet",
  "planned",
  "roadmap",
] as const;
const MODULE_TILES = ["OSL Chats", "OSL Mail", "OSL Notes", "Scrub"] as const;
const SUPPORT_STATUSES: readonly (NativeAppSupportStatus | null)[] = [
  null,
  "available",
  "beta",
  "experimental",
  "comingSoon",
  "externallyBlocked",
  "noClaim",
];

function contractBannedConcepts(): string[] {
  const source = readFileSync(new URL("../../../docs/design/osl-subjective-design-feel.md", import.meta.url), "utf8");
  const fence = /```json\s*([\s\S]*?)```/u.exec(source);
  if (fence === null) throw new Error("design-feel contract has no JSON fixture block");
  const fixture = JSON.parse(fence[1]) as { banned_user_facing_concepts?: string[] };
  const banned = fixture.banned_user_facing_concepts ?? [];
  if (banned.length === 0) throw new Error("design-feel contract declares no banned concepts");
  return banned;
}

function expectation(): ScreenWordsExpectation {
  return {
    title: "Tools and services",
    requiredWords: NAMED_WORDS,
    leastWords: LEAST_WORDS,
    bannedWords: contractBannedConcepts(),
  };
}

function productionScreenMarkup(): string {
  // main.ts currently contains merge damage in unrelated setup code, so this
  // focused check reads the two production render functions without importing
  // or executing that damaged code. This is the same fail-closed source reader
  // used by the adjacent Arrange-screen word audit.
  const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
  const heading = sliceFunctionSource(source, "homeDestinationContent", "workspaceContent");
  const tiles = sliceFunctionSource(source, "workspaceContent", "parsedEnclaveAudiences");
  return `${screenMarkupFromSource(heading)} ${screenMarkupFromSource(tiles)}`;
}

function futureLabels(markup: string): string[] {
  const text = visibleText(markup).toLocaleLowerCase();
  return FORBIDDEN_FUTURE_LABELS.filter((label) => text.includes(label));
}

function productionTileLabels(): string[] {
  const serviceTiles = homeAppsFromServices([]).map((app) => `${app.displayName} ${nativeAppTileLabel(null)}`);
  return [
    "OSL Chats Ready",
    "OSL Mail Unavailable",
    "OSL Notes Unavailable",
    "Scrub Ready",
    ...serviceTiles,
  ];
}

describe("TASK 0847 Tools and services tile words", () => {
  it("reads the production page title and named controls in plain English", () => {
    const markup = productionScreenMarkup();
    const report = checkScreenWords(markup, expectation());
    const future = futureLabels(markup);

    console.log(
      `TASK0847 title=${JSON.stringify(report.title)} words_read=${report.wordCount} ` +
        `present=${JSON.stringify(report.present)} missing=${JSON.stringify(report.missing)} ` +
        `banned_found=${report.banned.length} banned_terms_checked=${PLAIN_ENGLISH_BANNED_WORDS.length + contractBannedConcepts().length} ` +
        `future_labels_found=${future.length}`,
    );

    expect(screenTitle(markup)).toBe("Tools and services");
    expect(report.wordCount).toBeGreaterThanOrEqual(LEAST_WORDS);
    expect(report.present).toEqual([...NAMED_WORDS]);
    expect(report.missing).toEqual([]);
    expect(report.banned).toEqual([]);
    expect(future).toEqual([]);
    expect(report.ok).toBe(true);
  });

  it("checks all 17 production tile labels and every generated service status", () => {
    const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const labels = productionTileLabels();
    const generated = SUPPORT_STATUSES.map((status) => nativeAppTileLabel(status));

    expect(homeAppsFromServices([])).toHaveLength(13);
    expect(labels).toHaveLength(17);
    for (const moduleName of MODULE_TILES) expect(source).toContain(`name: "${moduleName}"`);
    expect(source).toContain("nativeAppTileLabel(claim?.supportStatus ?? null)");

    for (const label of labels) {
      const markup = `<article data-tile-id="checked"><strong>${label}</strong></article>`;
      expect(visibleWords(markup).length, label).toBeGreaterThanOrEqual(2);
      expect(findBannedWords(markup, contractBannedConcepts()), label).toEqual([]);
      expect(futureLabels(markup), label).toEqual([]);
    }
    for (const label of generated) {
      const markup = `<small>${label}</small>`;
      expect(findBannedWords(markup, contractBannedConcepts()), label).toEqual([]);
      expect(futureLabels(markup), label).toEqual([]);
    }

    console.log(
      `TASK0847 tiles_checked=${labels.length} service_statuses_checked=${generated.length} ` +
        `banned_found=0 future_labels_found=0 labels=${JSON.stringify(labels)}`,
    );
  });

  it("fails on throwaway copies missing each named word", () => {
    const markup = productionScreenMarkup();
    for (const named of NAMED_WORDS) {
      const throwaway = markup.split(named).join("");
      const report = checkScreenWords(throwaway, expectation());
      expect(throwaway).not.toBe(markup);
      expect(report.missing, `dropping ${JSON.stringify(named)} must be caught`).toContain(named);
      expect(report.ok).toBe(false);
      console.log(`TASK0847 mutant_missing=${JSON.stringify(named)} ok=${report.ok} missing=${JSON.stringify(report.missing)}`);
    }
  });

  it("fails on throwaway tile copies containing jargon or a future promise", () => {
    const jargon = `<article><strong>Signal adapter</strong></article>`;
    const future = `<article><strong>Signal coming soon</strong></article>`;
    expect(findBannedWords(jargon).map((hit) => hit.term)).toContain("adapter");
    expect(futureLabels(future)).toEqual(["coming soon"]);
    console.log("TASK0847 negative_banned=adapter detected=true negative_future=coming_soon detected=true");
  });
});
