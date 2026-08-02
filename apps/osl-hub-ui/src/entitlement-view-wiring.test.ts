import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const mainSource = readFileSync(fileURLToPath(new URL("./main.ts", import.meta.url)), "utf8");

describe("entitlement view-model shipping wiring", () => {
  it("projects the native license state into the rendered activation card", () => {
    expect(mainSource).toContain('import { entitlementView } from "./entitlement-view";');
    expect(mainSource).toContain("const entitlement = entitlementView(licenseState, Math.floor(Date.now() / 1_000));");
    expect(mainSource).toContain('data-entitlement-banner="${entitlement.banner}"');
    expect(mainSource).toContain('data-entitlement-cta="${entitlement.cta}"');
  });
});
