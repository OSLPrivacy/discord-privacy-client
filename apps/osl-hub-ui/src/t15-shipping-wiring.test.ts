import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const shippingMain = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

describe("T15 shipped lifecycle UI wiring", () => {
  it("T15-D4 keeps the measured optional-component picker on the shipping Settings path", () => {
    expect(shippingMain).toContain('from "./component-picker"');
    expect(shippingMain).toContain("componentPickerScreen(components)");
  });
  it("T15-D5 keeps the later-feature state manager on that same shipping path", () => {
    expect(shippingMain).toContain('from "./component-manager"');
    expect(shippingMain).toContain("componentManagerFromOnboarding(");
  });
  it("T15-E4 keeps the transfer manifest in the shipping settings render", () => {
    expect(shippingMain).toContain("deviceTransferManifestScreen()");
  });
  it("renders optional downloads behind the AutoScrub consent gate in Settings", () => {
    expect(shippingMain).toContain('from "./component-consent"');
    expect(shippingMain).toContain('decideAutoScrubInstall("autoscrub", null)');
    expect(shippingMain).toContain('data-autoscrub-install="${scrub.allowed ? "allowed" : "blocked"}"');
  });

  it("renders transfer ownership and dead-man limits in the shipping Settings route", () => {
    expect(shippingMain).toContain('from "./device-transfer"');
    expect(shippingMain).toContain('from "./device-transfer-source"');
    expect(shippingMain).toContain('from "./deadman"');
    expect(shippingMain).toContain("optionalComponentsSettingsContent()");
  });
});
