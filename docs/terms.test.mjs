import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const page = readFileSync(new URL("./terms.html", import.meta.url), "utf8");

function pageText(html) {
  return html
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/giu, " ")
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/giu, " ")
    .replace(/<[^>]+>/gu, " ")
    .replace(/\s+/gu, " ")
    .trim();
}

const text = pageText(page);

const checks = [
  {
    area: "scanning and deletion",
    exact: "Scanning and deletion",
    pattern: /OSL may scan only[\s\S]*Deletion is separate from scanning[\s\S]*reports only what it can verify/u,
  },
  {
    area: "service-rule and ban risk agreement",
    exact: "Service-rule and ban-risk agreement",
    pattern: /may break that service's rules[\s\S]*account at risk[\s\S]*suspension or a ban[\s\S]*requires a separate risk agreement/u,
  },
  {
    area: "one-month no-renewal Pro",
    exact: "One-month no-renewal Pro",
    pattern: /prepaid one-month activation code[\s\S]*Nothing renews automatically[\s\S]*nothing to cancel[\s\S]*does not store payment details/u,
  },
];

test("TASK 1542 Terms page text covers scanning/deletion, service-rule ban risk, and one-month no-renewal Pro", () => {
  for (const check of checks) {
    assert.match(text, check.pattern, `Terms page must cover ${check.area}`);
    assert.ok(text.includes(check.exact), `Terms page must contain exact heading: ${check.exact}`);
    console.log(`TASK1542_FOUND ${check.area}: ${check.exact}`);
  }
});
