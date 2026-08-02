import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const mainSource = readFileSync(fileURLToPath(new URL("./main.ts", import.meta.url)), "utf8");

describe("entitlement expiry copy shipping wiring", () => {
  it("renders the prepaid-safe entitlement copy in the activation card", () => {
    expect(mainSource).toContain('import { entitlementCopy } from "./entitlement-copy";');
    expect(mainSource).toContain("const copy = entitlementCopy(entitlement);");
    expect(mainSource).toContain("${escapeHtml(copy.title)}");
    expect(mainSource).toContain("${escapeHtml(copy.detail)}");
    expect(mainSource).not.toContain("Current period ends");
    expect(mainSource).not.toContain("Access through");
  });
});
