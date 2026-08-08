#!/usr/bin/env node

// TASK 1543 - fix terms page words.
//
// Checks two things about docs/terms.html:
//   1. the service-rule and ban-risk agreement is shown before ANY scanning
//      word on the page, and
//   2. every account-scanning, deletion and shared-purchase phrase below is
//      present exactly 1 time.
//
// The purchase phrases are not typed here: they are read from the shared
// source data/pricing.json (model.approved_pricing_text plus the
// required_phrases entries filed against docs/terms.html), so the Terms page
// cannot drift away from the pricing manifest.

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const TERMS = "docs/terms.html";
const PRICING = "data/pricing.json";

function stripHtml(text) {
  return text
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/giu, " ")
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/giu, " ")
    .replace(/<[^>]+>/gu, " ")
    .replace(/&nbsp;/giu, " ")
    .replace(/&amp;/giu, "&")
    .replace(/\s+/gu, " ")
    .trim();
}

function occurrences(haystack, needle, caseInsensitive) {
  const hay = caseInsensitive ? haystack.toLowerCase() : haystack;
  const pin = caseInsensitive ? needle.toLowerCase() : needle;
  const hits = [];
  let from = 0;
  for (;;) {
    const at = hay.indexOf(pin, from);
    if (at === -1) return hits;
    hits.push(at);
    from = at + pin.length;
  }
}

function main() {
  const text = stripHtml(readFileSync(path.join(REPO_ROOT, TERMS), "utf8"));
  const pricing = JSON.parse(readFileSync(path.join(REPO_ROOT, PRICING), "utf8"));

  const approved = pricing?.model?.approved_pricing_text;
  if (typeof approved !== "string" || approved.length === 0) {
    throw new Error(`${PRICING} model.approved_pricing_text is missing`);
  }
  const purchaseRequired = (pricing?.required_phrases ?? []).filter((entry) => entry?.file === TERMS);
  if (purchaseRequired.length === 0) {
    throw new Error(`${PRICING} required_phrases has no entry for ${TERMS}`);
  }

  console.log(`check-terms-page-words: page ${TERMS}`);
  console.log(`check-terms-page-words: shared purchase source ${PRICING}`);

  // Phrases that make up the service-rule and ban-risk agreement. None of them
  // may contain a scanning word: the agreement has to be readable first.
  const banRiskPhrases = [
    { area: "ban-risk heading", phrase: "Service-rule and ban-risk agreement" },
    {
      area: "ban-risk agreement",
      phrase:
        "Using OSL with another service may break that service's rules and could put your account at risk, including suspension or a ban.",
    },
    {
      area: "ban-risk separate agreement",
      phrase:
        "Experimental assisted placement, sending, cleanup, or deletion requires a separate risk agreement for the specific service and account before OSL uses it.",
    },
  ];

  const scanDeletePhrases = [
    { area: "scanning and deletion heading", phrase: "Scanning and deletion" },
    {
      area: "account scanning terms",
      phrase:
        "Account terms for scanning: OSL reads an account only while you are signed in to that account on this device, only for the run you start, and only inside the scope you picked for that run",
    },
    {
      area: "account deletion terms",
      phrase:
        "Account terms for deletion: OSL removes only the items inside the scope you confirmed, works through them one at a time so you can stop part way",
    },
  ];

  const purchasePhrases = [
    { area: "shared pricing facts", phrase: approved },
    ...purchaseRequired.map((entry, index) => ({
      area: `shared purchase terms ${index + 1}`,
      phrase: entry.phrase,
      // "nothing renews automatically" is filed lower-case in the manifest but
      // opens a sentence on the page.
      caseInsensitive: true,
    })),
  ];

  const all = [...banRiskPhrases, ...scanDeletePhrases, ...purchasePhrases];
  const failures = [];

  for (const check of all) {
    const hits = occurrences(text, check.phrase, check.caseInsensitive === true);
    const label = check.phrase.length > 60 ? `${check.phrase.slice(0, 57)}...` : check.phrase;
    console.log(`check-terms-page-words: ${check.area} count=${hits.length} phrase="${label}"`);
    if (hits.length !== 1) {
      failures.push(`${check.area} must appear exactly 1 time, found ${hits.length}: "${check.phrase}"`);
    }
  }

  // Order: the whole ban-risk agreement has to end before the first scanning word.
  let banEnd = -1;
  for (const check of banRiskPhrases) {
    const hits = occurrences(text, check.phrase, false);
    if (hits.length === 0) continue;
    banEnd = Math.max(banEnd, hits[hits.length - 1] + check.phrase.length);
  }
  if (banEnd === -1) {
    failures.push("service-rule and ban-risk agreement is not on the page at all");
  }

  const scanWord = /scan\w*/iu.exec(text);
  if (scanWord === null) {
    failures.push("no scanning words on the page: the scanning terms are missing");
  } else {
    console.log(
      `check-terms-page-words: ban-risk agreement ends at char ${banEnd}; first scanning word "${scanWord[0]}" at char ${scanWord.index}`,
    );
    if (!(banEnd >= 0 && banEnd < scanWord.index)) {
      failures.push(
        `service-rule and ban-risk agreement must be shown before any scanning words, but the agreement ends at char ${banEnd} and "${scanWord[0]}" appears at char ${scanWord.index}`,
      );
    }
  }

  if (failures.length > 0) {
    throw new Error(`terms page words:\n  - ${failures.join("\n  - ")}`);
  }

  console.log(
    `check-terms-page-words: service-rule and ban-risk agreement precedes every scanning word, and all ${all.length} phrases appear exactly 1 time.`,
  );
}

try {
  main();
} catch (error) {
  console.error(`check-terms-page-words: FAIL ${error.message}`);
  process.exit(1);
}
