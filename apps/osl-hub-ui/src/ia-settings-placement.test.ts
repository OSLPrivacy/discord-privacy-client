import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

describe("IA settings placement", () => {
  it("keeps Settings in the shared launcher header instead of a rail", () => {
    const header = source.slice(source.indexOf("function trustedHeader"), source.indexOf("function homeCommandIcon"));
    expect(header).toContain('data-route="settings"');
    expect(header).not.toMatch(/primary-sidebar|data-primary-destination/u);
  });

  it("keeps the in-page Settings section navigation", () => {
    const settings = source.slice(source.indexOf("function settingsContent"), source.indexOf("function settingsSectionContent"));
    expect(settings).toContain('class="settings-sidebar"');
    expect(settings).toContain('aria-label="Settings"');
  });
});
