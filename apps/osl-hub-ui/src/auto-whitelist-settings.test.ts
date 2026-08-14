import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  autoWhitelistRuleChoices,
  configuredAutoWhitelistAppKinds,
  whitelistingSettingsMarkup,
} from "./auto-whitelist-settings";

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function rowFor(markup: string, appKind: string): string {
  const marker = `data-auto-rule-kind="${appKind}"`;
  const start = markup.indexOf(marker);
  expect(start, `${appKind} row should render`).toBeGreaterThanOrEqual(0);
  const articleStart = markup.lastIndexOf("<article", start);
  const articleEnd = markup.indexOf("</article>", start);
  expect(articleStart).toBeGreaterThanOrEqual(0);
  expect(articleEnd).toBeGreaterThan(start);
  return markup.slice(articleStart, articleEnd + "</article>".length);
}

describe("TASK 0141 auto-rule whitelisting settings", () => {
  it("renders four-choice controls for every configured app kind", () => {
    const markup = whitelistingSettingsMarkup();
    const renderedKinds = [...markup.matchAll(/data-auto-rule-kind="([^"]+)"/gu)].map((match) => match[1]);
    const expectedKinds = configuredAutoWhitelistAppKinds.map((app) => app.appKind);

    expect(renderedKinds).toEqual(expectedKinds);
    for (const appKind of expectedKinds) {
      const row = rowFor(markup, appKind);
      expect([...row.matchAll(/data-auto-rule-choice="([^"]+)"/gu)].map((match) => match[1]))
        .toEqual(autoWhitelistRuleChoices.map((choice) => choice.id));
    }

    const discordRow = rowFor(markup, "discord");
    const labels = autoWhitelistRuleChoices.map((choice) => choice.label);
    for (const label of labels) expect(discordRow).toContain(`>${label}</button>`);

    console.log(`TASK0141 kind=discord choices=${labels.join("|")} configured_app_kinds=${renderedKinds.join("|")}`);
  });

  it("wires the controls into Apps settings", () => {
    const start = mainSource.indexOf("function serviceAccountsSettingsContent()");
    const end = mainSource.indexOf("async function scanPrivacyExport", start);
    expect(start).toBeGreaterThanOrEqual(0);
    expect(end).toBeGreaterThan(start);
    const settings = mainSource.slice(start, end);
    expect(settings).toContain("whitelistingSettingsMarkup(configuredAutoWhitelistAppKinds, autoWhitelistSavedChoices)");
    expect(settings.indexOf("whitelistingSettingsMarkup("))
      .toBeGreaterThan(settings.indexOf("account-settings-list"));
  });
});
