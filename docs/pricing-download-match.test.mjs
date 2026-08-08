import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import test from "node:test";

test("TASK 1529 Pricing and Download show the same price text with 0 differences", () => {
  const output = execFileSync("node", ["scripts/check-pricing-download-match.mjs"], { encoding: "utf8" });
  assert.match(output, /differences: 0/);
  assert.match(output, /check-pricing-download-match: complete\./);
});
