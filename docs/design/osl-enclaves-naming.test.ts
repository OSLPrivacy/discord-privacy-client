import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const namingRecord = readFileSync(new URL("./osl-enclaves-naming.md", import.meta.url), "utf8");

describe("OSL Enclaves naming decision", () => {
  it("makes Enclaves the only current name for the channel-bearing container", () => {
    expect(namingRecord).toContain("**Enclaves** is the product name");
    expect(namingRecord).toContain("do not use **Circles** or **Spaces** in shipping copy, code,");
    expect(namingRecord).toContain("The current surface remains\n`coming_later`");
    expect(namingRecord).toContain("Group sender keys from T18 are\na hard prerequisite");
  });
});
