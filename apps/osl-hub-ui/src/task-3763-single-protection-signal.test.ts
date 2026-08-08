import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const servicesSource = readFileSync(new URL("./services.ts", import.meta.url), "utf8");

function functionSource(source: string, name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

function countMatches(source: string, pattern: RegExp): number {
  return [...source.matchAll(pattern)].length;
}

/**
 * Return only literal text nodes from production template literals. Attribute
 * values, TypeScript expressions and comments are deliberately excluded: this
 * task counts what an owner can see, not implementation words such as the
 * historical `home-dashboard` CSS class.
 */
function visibleTemplateText(source: string): string {
  const literals = [...source.matchAll(/`(?:\\.|[^`])*`/gs)]
    .map((match) => match[0].slice(1, -1))
    .join(" ");
  return literals
    .replace(/\$\{[^}]*\}/gs, " ")
    .replace(/<[^>]*>/gs, " ")
    .replace(/&[a-z]+;/giu, " ")
    .replace(/\s+/gu, " ")
    .trim();
}

const trustedHeader = functionSource(mainSource, "trustedHeader", "homeCommandCurrentId");
const home = functionSource(mainSource, "homeDestinationContent", "workspaceContent")
  + functionSource(mainSource, "workspaceContent", "parsedEnclaveAudiences");
const settings = functionSource(mainSource, "settingsContent", "settingsSectionContent")
  + mainSource.slice(
    mainSource.indexOf("function settingsSectionContent"),
    mainSource.indexOf("function resetLocalProtectedSheet"),
  );
const appViews = functionSource(mainSource, "serviceContent", "activeNativeApp")
  + functionSource(mainSource, "serviceGuideContent", "settingsContent")
  + trustedHeader;

const shippingAppIds = [...new Set(
  [...servicesSource.matchAll(/\bhomeApp\("([a-z][a-z0-9]*)"/gu)].map((match) => match[1]),
)];
const settingsSections = [...mainSource
  .match(/const items: Array<\[SettingsSection, string\]> = \[([^;]+)\];/u)?.[1]
  ?.matchAll(/\["([a-z-]+)",\s*"[^"]+"\]/gu) ?? []]
  .map((match) => match[1]);

// A second signal must be named in markup, not inferred from colour. These are
// the semantic forms accepted by the UI contract. The existing control is the
// first form; the others make a future meter/status addition fail this check.
const protectionSignalPattern = /id="local-protected-toggle"|data-protection-(?:meter|signal)(?:=|\b)|<meter\b[^>]*(?:protection|protected)|role="progressbar"[^>]*(?:protection|protected)/giu;

function protectionSignalCount(markupSource: string): number {
  return countMatches(markupSource, protectionSignalPattern);
}

const visibleScreenText = visibleTemplateText(`${home}\n${settings}\n${appViews}`);
const forbiddenCounts = {
  dashboards: countMatches(visibleScreenText, /\bdashboards?\b/giu),
  scores: countMatches(visibleScreenText, /\bscores?\b/giu),
  percentages: countMatches(visibleScreenText, /(?:\bpercent(?:age)?s?\b|\d\s*%)/giu),
  grades: countMatches(visibleScreenText, /\bgrades?\b/giu),
  colourKeys: countMatches(visibleScreenText, /\b(?:colou?r\s+(?:keys?|legends?)|(?:keys?|legends?)\s+for\s+colou?r)\b/giu),
};

describe("TASK 3763 one protection signal and no dashboard gates", () => {
  it("counts one shared Protect control across Home, every Settings section, and every app view", () => {
    const uniqueControlIds = [...new Set(
      [...trustedHeader.matchAll(/id="(local-protected-toggle)"/gu)].map((match) => match[1]),
    )];
    const homeControlCount = protectionSignalCount(home);
    const settingsControlCounts = Object.fromEntries(
      settingsSections.map((section) => [section, protectionSignalCount(settings)]),
    );
    const appControlCounts = Object.fromEntries(
      shippingAppIds.map((appId) => [appId, protectionSignalCount(trustedHeader)]),
    );

    console.log(
      `TASK3763 protection_controls=${uniqueControlIds.length} home=${homeControlCount} settings_screens=${settingsSections.length} settings_controls=${Object.values(settingsControlCounts).reduce((sum, count) => sum + count, 0)} app_views=${shippingAppIds.length} app_min=${Math.min(...Object.values(appControlCounts))} app_max=${Math.max(...Object.values(appControlCounts))} control_id=${uniqueControlIds[0]}`,
    );

    expect(shippingAppIds.length, "the shipping app inventory cannot be empty").toBeGreaterThan(0);
    expect(settingsSections.length, "the Settings inventory cannot be empty").toBeGreaterThan(0);
    expect(trustedHeader).toContain('route === "service"');
    expect(trustedHeader).toContain('id="local-protected-toggle"');
    expect(uniqueControlIds).toEqual(["local-protected-toggle"]);
    expect(homeControlCount).toBe(0);
    expect(Object.values(settingsControlCounts).every((count) => count === 0)).toBe(true);
    expect(
      Object.values(appControlCounts).every((count) => count === 1),
      `every app view must render exactly one protection signal; counts=${JSON.stringify(appControlCounts)}`,
    ).toBe(true);
    expect(
      protectionSignalCount(trustedHeader),
      "the shared app header must contain exactly one protection signal",
    ).toBe(1);
  });

  it("counts no visible dashboard, score, percentage, grade, or colour key", () => {
    expect(forbiddenCounts).toEqual({
      dashboards: 0,
      scores: 0,
      percentages: 0,
      grades: 0,
      colourKeys: 0,
    });

    console.log(
      `TASK3763 dashboard=${forbiddenCounts.dashboards} scores=${forbiddenCounts.scores} percentages=${forbiddenCounts.percentages} grades=${forbiddenCounts.grades} colour_keys=${forbiddenCounts.colourKeys}`,
    );
  });
});
