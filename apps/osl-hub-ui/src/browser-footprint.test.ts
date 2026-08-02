import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function functionSource(source: string, name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("native browser footprint renderer handoff", () => {
  it("Verify the f1-footprint renderer performs the attended browser-account handoff", () => {
    const renderer = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const binding = functionSource(renderer, "bindBrowserImportControls", "importIdentityForm");
    const start = binding.indexOf("const startProtectedBrowserImport = async (): Promise<void> =>");
    const listener = binding.indexOf('querySelector<HTMLButtonElement>("#import-saved-accounts")');
    expect(start).toBeGreaterThanOrEqual(0);
    expect(listener).toBeGreaterThan(start);
    const handoff = binding.slice(start, listener);
    const selection = handoff.indexOf("const selectedProfiles = browserProfiles.filter");
    const consumeSelection = handoff.indexOf("selectedBrowserProfileKeys.clear()");
    const grant = handoff.indexOf("const grant = await grantBrowserProfileConsent(");
    const scan = handoff.indexOf("const receipt = await scanConsentedBrowserProfile(");
    const observationCheck = handoff.indexOf("receipt.observationCount < 1");
    const snapshotDeletionCheck = handoff.indexOf("!receipt.snapshotDeleted");
    const pushReceipt = handoff.indexOf("scanReceipts.push(receipt)");
    const hydrate = handoff.indexOf("const hydration = await loadDetectedBrowserFootprint(scanReceipts)");
    const apply = handoff.indexOf("applyNativeBrowserFootprint(hydration)");

    expect(selection).toBeGreaterThanOrEqual(0);
    expect(consumeSelection).toBeGreaterThan(selection);
    expect(grant).toBeGreaterThan(consumeSelection);
    expect(scan).toBeGreaterThan(grant);
    expect(observationCheck).toBeGreaterThan(scan);
    expect(snapshotDeletionCheck).toBeGreaterThan(observationCheck);
    expect(pushReceipt).toBeGreaterThan(snapshotDeletionCheck);
    expect(hydrate).toBeGreaterThan(pushReceipt);
    expect(apply).toBeGreaterThan(hydrate);
    expect(handoff).toMatch(/grantBrowserProfileConsent\(\s*selectedProfile\.browserId,\s*selectedProfile\.profile,\s*\)/);
    expect(handoff).toMatch(/scanConsentedBrowserProfile\(\s*selectedProfile\.browserId,\s*selectedProfile\.profile,\s*grant\.grantId,\s*\)/);
    expect(binding.slice(listener)).toContain("void startProtectedBrowserImport()");
  });
});
