import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { oslPrimaryDestinationValues, oslPrimaryDestinations, oslSettingsDestination } from "./state";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("fixed desktop IA sidebar", () => {
  const sidebar = functionSource("primarySidebarMarkup", "appLauncherStrip");

  it("is rendered as the first column of the workspace shell", () => {
    const renderWorkspace = functionSource("renderWorkspace", "primarySidebarMarkup");
    expect(renderWorkspace).toContain('class="hub-layout with-primary-sidebar"');
    expect(renderWorkspace).toContain("${primarySidebarMarkup()}<section class=\"hub-workspace\"");
    expect(sidebar).toContain("grid-template-columns: 232px minmax(0, 1fr)");
  });

  it("uses the fixed six primary destinations in model order", () => {
    expect(sidebar).toContain("oslPrimaryDestinations.map");
    expect(sidebar).toContain('aria-label="Primary destinations"');
    for (const destination of oslPrimaryDestinationValues) {
      expect(sidebar).toContain(`id === "${destination}"`);
    }
    expect(oslPrimaryDestinations.map((destination) => destination.id)).toEqual(oslPrimaryDestinationValues);
  });

  it("keeps Settings fixed outside the primary destinations", () => {
    expect(oslPrimaryDestinationValues).not.toContain(oslSettingsDestination);
    expect(sidebar).toContain('class="primary-sidebar-settings');
    expect(sidebar).toContain('data-route="${oslSettingsDestination}"');
    expect(sidebar.indexOf('aria-label="Primary destinations"')).toBeLessThan(sidebar.indexOf('class="primary-sidebar-settings'));
  });

  it("does not expose implementation concepts as navigation copy", () => {
    const visibleCopy = [
      ...oslPrimaryDestinations.flatMap((destination) => [destination.label, destination.userQuestion]),
      "Settings",
      "OSL",
    ].join("\n");
    expect(visibleCopy).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/i);
    expect(sidebar).not.toMatch(/data-sidebar-move|data-sidebar-toggle|Move or hide apps/i);
  });
});
