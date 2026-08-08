#!/usr/bin/env node

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const PAGES = {
  Download: "docs/download.html",
  Pricing: "docs/pricing.html",
  Success: "docs/fixtures/checkout-success.html",
  FAQ: "docs/faq.html",
  Terms: "docs/terms.html",
};

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

function main() {
  const pricing = JSON.parse(readFileSync(path.join(REPO_ROOT, "data/pricing.json"), "utf8"));
  const approvedText = pricing?.model?.approved_pricing_text;
  if (typeof approvedText !== "string" || approvedText.length === 0) {
    throw new Error("data/pricing.json model.approved_pricing_text is missing");
  }

  console.log("check-shared-pricing-words: shared source data/pricing.json model.approved_pricing_text");
  console.log(`check-shared-pricing-words: approved_pricing_text=${approvedText}`);

  const drift = [];
  for (const [label, entry] of Object.entries(PAGES)) {
    const fullPath = path.join(REPO_ROOT, entry);
    const plain = stripHtml(readFileSync(fullPath, "utf8"));
    const present = plain.includes(approvedText);
    console.log(`check-shared-pricing-words: ${label} (${entry}) shared_pricing_text=${present ? "present" : "MISSING"}`);
    if (!present) drift.push(`${label} (${entry})`);
  }

  if (drift.length > 0) {
    throw new Error(`pricing drift: shared pricing text is missing or altered on: ${drift.join(", ")}`);
  }

  console.log(`check-shared-pricing-words: all ${Object.keys(PAGES).length} pages contain identical pricing facts.`);
}

try {
  main();
} catch (error) {
  console.error(`check-shared-pricing-words: fatal error: ${error.message}`);
  process.exit(1);
}
