import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function slice(name: string, next: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${next}`, start + 1);
  expect(start, name).toBeGreaterThanOrEqual(0);
  expect(end, next).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("per-account opening contract", () => {
  it("labels desktop reuse as provider-wide and isolation as exact-account", () => {
    const detected = slice("detectedAppsContent", "browserImportContent");
    expect(detected).toContain("Current desktop session · provider-wide");
    expect(detected).toContain("Use isolated OSL profile · this account");
    const picker = slice("serviceAccountPickerContent", "serviceGuideContent");
    expect(picker).toContain("whichever account the desktop app currently shows");
    expect(picker).toContain("Isolated OSL profile · exact account");
  });

  it("forces a picker when any account overrides the provider-wide default", () => {
    const selected = slice("selectedInstalledNativeApp", "detectedAppsContent");
    expect(selected).toContain("detectedAccountChoiceKey(service.id, account.id)");
    expect(selected).toContain('=== "osl"');
    expect(selected).toContain("return undefined");
    expect(selected).not.toContain("startsWith");
    expect(source).toContain("data-service-current-session");
    expect(source).toContain("providerWideInstalledNativeApp(app.id)");
  });

  it("drops stale account overrides before saving them", () => {
    const persist = slice("persistDetectedAccountChoices", "selectedInstalledNativeApp");
    expect(persist).toContain("service.accounts.map");
    expect(persist).toContain("detectedAccountChoiceKey(service.id, account.id)");
    expect(persist).toContain("detectedAccountChoices.delete(key)");
  });

  it("never lets an exact isolated account click fall through to native reuse", () => {
    const openStart = source.indexOf("async function openEmbeddedApp");
    const openEnd = source.indexOf("async function continueOnboardingFromService", openStart);
    const open = source.slice(openStart, openEnd);
    expect(open).toContain("const native = accountId ? undefined : selectedInstalledNativeApp(app.id)");
    expect(open).toContain("openEmbeddedHomeApp(app, services, accountId)");
    const setupStart = source.indexOf("async function setupEmbeddedApp");
    const setup = source.slice(setupStart, openStart);
    expect(setup).toContain("const native = forceNewProfile ? undefined");
  });
});
