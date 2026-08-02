import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const shippingMain = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

describe("T15 shipped lifecycle UI wiring", () => {
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
