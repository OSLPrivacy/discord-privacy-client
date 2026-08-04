#!/usr/bin/env node

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const raw = readFileSync(new URL("../data/pricing.json", import.meta.url), "utf8");
const pricing = JSON.parse(raw);

test("pricing manifest has one live forbidden_claims key", () => {
  const matches = [...raw.matchAll(/^\s*"forbidden_claims"\s*:/gmu)];
  assert.equal(matches.length, 1);
});

test("planned compute credits are not sold as included in Pro", () => {
  assert.equal(pricing.compute_credits.status, "Planned");
  assert.equal(pricing.compute_credits.packs.length, 0);
  assert.doesNotMatch(pricing.compute_credits.purpose, /\bPro includes\b/iu);
});

test("billing and security bans survive JSON parsing", () => {
  const claims = pricing.forbidden_claims;
  assert.ok(Array.isArray(claims));
  const serialized = JSON.stringify(claims);
  for (const phrase of [
    "one-time $5 purchase",
    "$5 once",
    "subscription",
    "better than Signal",
    "Signal protocol",
    "post-quantum authentication",
  ]) {
    assert.match(serialized, new RegExp(phrase.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "u"));
  }
});
