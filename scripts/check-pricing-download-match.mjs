#!/usr/bin/env node
/** TASK 1529: check Pricing and Download show the same Free/Pro price text, with 0 differences between the two pages' pricing strings. */

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const PRICING_PAGE = process.argv[2] || "docs/fixtures/pricing.html";
const DOWNLOAD_PAGE = process.argv[3] || "docs/download.html";

function stripHtml(text) {
  return text
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, " ")
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&nbsp;/gi, " ")
    .replace(/&amp;/gi, "&")
    .replace(/\s+/g, " ")
    .trim();
}

function pricingStrings(entry) {
  const html = readFileSync(path.join(REPO_ROOT, entry), "utf8");
  const plain = stripHtml(html);
  const free = plain.match(/Free OSL keeps working:[^.]*\./);
  const pro = plain.match(/\d+ dollars, one month from code entry, no renewal, no OSL card storage\./);
  return {
    free: free ? free[0] : null,
    pro: pro ? pro[0] : null,
  };
}

const pricing = pricingStrings(PRICING_PAGE);
const download = pricingStrings(DOWNLOAD_PAGE);

console.log(`check-pricing-download-match: ${PRICING_PAGE} vs ${DOWNLOAD_PAGE}`);
console.log(`pricing free: ${pricing.free ?? "MISSING"}`);
console.log(`download free: ${download.free ?? "MISSING"}`);
console.log(`pricing pro: ${pricing.pro ?? "MISSING"}`);
console.log(`download pro: ${download.pro ?? "MISSING"}`);

let differences = 0;
if (pricing.free === null || download.free === null || pricing.free !== download.free) differences += 1;
if (pricing.pro === null || download.pro === null || pricing.pro !== download.pro) differences += 1;

console.log(`differences: ${differences}`);

if (differences > 0) {
  console.error(`check-pricing-download-match: fatal error: ${differences} pricing string difference(s) between Pricing and Download`);
  process.exit(1);
}
console.log("check-pricing-download-match: complete.");
