#!/usr/bin/env node
import assert from "node:assert/strict";
import { promises as dns } from "node:dns";
import { readFile } from "node:fs/promises";

const inventory = new URL("../docs/design/domain-and-zone-inventory.md", import.meta.url);
const expected = "| `www.oslprivacy.com` | deliberately absent |";
const record = await readFile(inventory, "utf8");

assert.ok(
  record.includes(expected),
  "T11-T32 requires the inventory to declare www deliberately absent",
);

const lookups = [dns.resolve4("www.oslprivacy.com"), dns.resolve6("www.oslprivacy.com"), dns.resolveCname("www.oslprivacy.com")];
const results = await Promise.allSettled(lookups);
const resolved = results.flatMap((result) => result.status === "fulfilled" ? result.value : []);
const unexpected = results.filter(
  (result) => result.status === "rejected" && result.reason?.code !== "ENOTFOUND" && result.reason?.code !== "ENODATA",
);

assert.equal(unexpected.length, 0, `DNS lookup failed unexpectedly: ${unexpected.map((result) => result.reason?.code).join(", ")}`);
assert.equal(resolved.length, 0, `www.oslprivacy.com resolves despite its deliberately-absent decision: ${resolved.join(", ")}`);
console.log("T11-T32 PASS: www.oslprivacy.com is deliberately absent (NXDOMAIN/ENODATA).");
