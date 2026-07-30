import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const source = readFileSync(fileURLToPath(new URL("./main.ts", import.meta.url)), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("connections primary action", () => {
  const action = functionSource("connectionsPrimaryAction", "persistServiceGuideState");

  it("routes to an available service connection guide", () => {
    expect(action).toContain("homeAppsFromServices(services).find");
    expect(action).toContain('app.visibility === "launch"');
    expect(action).toContain('app.launchState === "available"');
    expect(action).toContain("app.setupEligible");
    expect(action).toContain("app.serviceId !== null");
    expect(action).toContain("services.find((candidate) => candidate.id === target.serviceId)");
    expect(action).toContain("openServiceRoute(service, target.provider, target.id, true)");
  });

  it("refuses absent service binding by falling back to Apps settings", () => {
    expect(action).toMatch(/if \(!target \|\| !service\) \{/);
    expect(action).toContain('route = "settings"');
    expect(action).toContain('settingsSection = "apps"');
    expect(action).toContain("activeService = null");
    expect(action).toContain("activeHomeAppId = null");
    expect(action).toContain("serviceAccountPickerOpen = false");
    expect(action).toMatch(/render\(\);\s*return;/);
  });

  it("does not open or reuse account surfaces directly", () => {
    expect(action).not.toContain("openEmbeddedApp");
    expect(action).not.toContain("openNativeHostedApp");
    expect(action).not.toContain("openBrowserCompanionApp");
    expect(action).not.toContain("setupEmbeddedApp");
    expect(action).not.toContain("activeEmbeddedHost");
    expect(action).not.toContain("activeNativeHostId");
    expect(action).not.toContain("activeDefaultBrowserCompanion");
  });
});
