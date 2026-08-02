import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

describe("offline burn production wiring", () => {
  it("makes the shipping blob DELETE handler delegate to the offline-safe policy", async () => {
    const source = await readFile(
      fileURLToPath(new URL("../src/endpoints/blob.ts", import.meta.url)),
      "utf8",
    );

    expect(source).toContain('import { applyBurn } from "../lib/burn-policy.js"');
    expect(source).toMatch(/return applyBurn\([\s\S]*request\.headers\.get\("x-osl-manage-cap"\)\)/);
  });
});
