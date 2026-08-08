import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import test from "node:test";

test("TASK 1512 Download, Pricing, Success, FAQ, and Terms carry identical pricing facts", () => {
  const output = execFileSync("node", ["scripts/check-shared-pricing-words.mjs"], { encoding: "utf8" });
  assert.match(output, /approved_pricing_text=5 dollars, one month from code entry, no renewal, no OSL card storage\./);
  assert.match(output, /Download \(docs\/download\.html\) shared_pricing_text=present/);
  assert.match(output, /Pricing \(docs\/pricing\.html\) shared_pricing_text=present/);
  assert.match(output, /Success \(docs\/fixtures\/checkout-success\.html\) shared_pricing_text=present/);
  assert.match(output, /FAQ \(docs\/faq\.html\) shared_pricing_text=present/);
  assert.match(output, /Terms \(docs\/terms\.html\) shared_pricing_text=present/);
  assert.match(output, /all 5 pages contain identical pricing facts\./);
});
