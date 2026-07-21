import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("browser account model", () => {
  it("scopes browser selection and completed import receipts to the active OSL identity", () => {
    const persistence = functionSource("persistBrowserAccountPreferences", "activeBrowserImportPendingStorageKey");
    const refresh = functionSource("refreshActiveBrowserAccountsReady", "saveHomeTilePreferences");
    expect(persistence).toContain("activeOwnerStorageKey(preferredBrowserStorageKey)");
    expect(persistence).toContain("activeOwnerStorageKey(completedBrowserImportsStorageKey)");
    expect(refresh).toContain("completedBrowserImportIds.clear()");
    expect(refresh).toContain("preferredBrowserId = supportedBrowserId(storedPreferred)");
  });

  it("shows two choices only after the selected browser has an import receipt", () => {
    const choices = functionSource("browserSessionModeChoices", "selectedBrowserForLaunch");
    expect(choices).toContain("selectedBrowserHasImportReceipt()");
    expect(choices).toContain(">Browser account</strong>");
    expect(choices).toContain(">New account</strong>");
    expect(choices).toContain('selectedBrowserForLaunch() !== "duckduckgo"');
  });

  it("opens the selected or default browser directly when no import receipt exists", () => {
    const mode = functionSource("browserAccountModeForLaunch", "openBrowserCompanionApp");
    const opening = functionSource("openBrowserCompanionApp", "setupEmbeddedApp");
    expect(mode).toContain('if (!selectedBrowserHasImportReceipt()) return "existingBrowser"');
    expect(opening).toContain("hostBrowserCompanion(app.id, preferredBrowserId, accountMode)");
    expect(opening).not.toContain("openEmbeddedHomeApp");
    expect(opening).not.toContain("setupEmbeddedHomeApp");
  });

  it("routes native messengers before browser apps and never sends them to a browser", () => {
    const opening = functionSource("openHomeAppFromLauncher", "startBackgroundInstall");
    expect(opening.indexOf("selectedNativeAppIntent(app.id)")).toBeLessThan(opening.indexOf("defaultBrowserCompanionEligible(app.id)"));
    expect(opening).toContain("openNativeHostedApp(app, service, nativeIntent)");
    expect(opening).toContain("openBrowserCompanionApp(app, service)");
  });
});
