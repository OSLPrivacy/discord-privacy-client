#!/usr/bin/env node

// TASK 1519: the Download page must carry the four shared pricing facts and a
// purchase control that is either working or hidden — never a dead button.
//
// Nothing here is hand-entered. The four facts come from
// data/pricing.json -> model.approved_pricing_text (the shared source written by
// TASK 1511). The Free/Pro Scrub tier statements are checked against the
// capability_registry status and the manifest's forward_looking_markers. The
// purchase state comes from the source-owned checkout readiness in
// keyserver-cf/src/lib/prepaid-redemption-readiness.ts.

import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const DEFAULT_PAGE = "docs/download.html";
const PRICING_PATH = path.join(REPO_ROOT, "data", "pricing.json");
const READINESS_PATH = path.join(
  REPO_ROOT,
  "keyserver-cf",
  "src",
  "lib",
  "prepaid-redemption-readiness.ts",
);
const PURCHASE_REGION_ID = "download-pro-purchase";
const TIER_CLAIMS = [
  { label: "Free review", tier: "free", capability: "scrub-discovery" },
  { label: "Pro deletion", tier: "pro", capability: "scrub-guided-deletion" },
  { label: "Pro schedules", tier: "pro", capability: "autoscrub" },
];
const SELLABLE_STATUSES = new Set(["Available", "Beta"]);

function repoPath(input) {
  const entry = input || DEFAULT_PAGE;
  if (path.isAbsolute(entry) || entry.includes("..")) {
    throw new Error(`unsafe download page path: ${entry}`);
  }
  const fullPath = path.join(REPO_ROOT, entry);
  if (!existsSync(fullPath)) throw new Error(`download page is missing: ${entry}`);
  return { entry, fullPath };
}

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

/** The four shared pricing facts, split out of the single approved sentence. */
export function sharedPricingFacts(pricing) {
  const text = pricing?.model?.approved_pricing_text;
  if (typeof text !== "string" || text.trim().length === 0) {
    throw new Error("data/pricing.json model.approved_pricing_text is missing");
  }
  const facts = text
    .replace(/\.\s*$/, "")
    .split(",")
    .map((fact) => fact.trim())
    .filter((fact) => fact.length > 0);
  if (facts.length !== 4) {
    throw new Error(`expected 4 shared pricing facts, approved text yields ${facts.length}`);
  }
  return { text, facts };
}

/** Reads the source-owned checkout readiness rather than trusting page copy. */
export function checkoutReadiness() {
  const source = readFileSync(READINESS_PATH, "utf8");
  const body = source.match(/export function prepaidRedemptionReady\(\)\s*:\s*boolean\s*\{([\s\S]*?)\n\}/);
  if (!body) throw new Error("cannot find prepaidRedemptionReady() in the keyserver source");
  const returned = body[1].match(/return\s+(true|false)\s*;/);
  if (!returned) throw new Error("prepaidRedemptionReady() does not return a source-owned boolean");
  const reason = source.match(/PREPAID_REDEMPTION_UNAVAILABLE\s*=\s*\n?\s*"([^"]+)"/);
  if (!reason) throw new Error("cannot find PREPAID_REDEMPTION_UNAVAILABLE in the keyserver source");
  return { ready: returned[1] === "true", reason: reason[1] };
}

function purchaseRegion(html) {
  const open = new RegExp(`<div\\b[^>]*\\bid=["']${PURCHASE_REGION_ID}["'][^>]*>`, "i").exec(html);
  if (!open) throw new Error(`the download page has no purchase region (id="${PURCHASE_REGION_ID}")`);
  const start = open.index + open[0].length;
  let depth = 1;
  const tag = /<(\/?)div\b[^>]*>/gi;
  tag.lastIndex = start;
  let match;
  while ((match = tag.exec(html))) {
    depth += match[1] ? -1 : 1;
    if (depth === 0) {
      return { openTag: open[0], inner: html.slice(start, match.index) };
    }
  }
  throw new Error(`the purchase region (id="${PURCHASE_REGION_ID}") is not closed`);
}

/** The <section> that holds the purchase region — "beside the purchase path". */
function purchaseSection(html) {
  const anchor = html.indexOf(`id="${PURCHASE_REGION_ID}"`);
  const start = html.lastIndexOf("<section", anchor);
  const end = html.indexOf("</section>", anchor);
  if (start < 0 || end < 0) throw new Error("the purchase region is not inside a <section>");
  return html.slice(start, end);
}

function countPurchaseControls(regionHtml) {
  const buttons = regionHtml.match(/<button\b[^>]*>/gi) || [];
  const links = (regionHtml.match(/<a\b[^>]*\bhref=(["'])(.*?)\1[^>]*>/gi) || []).filter((tag) =>
    /checkout|buy|purchase|stripe/i.test(tag),
  );
  return { buttons: buttons.length, links: links.length, total: buttons.length + links.length };
}

function tierStatement(sectionHtml, claim) {
  const regex = new RegExp(
    `<li\\b[^>]*\\bdata-tier=["']${claim.tier}["'][^>]*\\bdata-capability=["']${claim.capability}["'][^>]*>([\\s\\S]*?)</li>`,
    "i",
  );
  const match = regex.exec(sectionHtml);
  return match ? stripHtml(match[1]) : null;
}

function checkDownloadPage(entry, options = {}) {
  const page = repoPath(entry);
  const html = readFileSync(page.fullPath, "utf8");
  const plain = stripHtml(html);
  const pricing = JSON.parse(readFileSync(PRICING_PATH, "utf8"));
  const { text, facts } = sharedPricingFacts(pricing);
  const markers = pricing?.surface_policy?.forward_looking_markers || [];
  const registry = new Map((pricing?.capability_registry || []).map((cap) => [cap.id, cap]));
  // Production always reads the source-owned readiness; tests inject the
  // opposite state so both accepted outcomes stay covered.
  const readiness = options.readiness || checkoutReadiness();
  const region = purchaseRegion(html);
  const section = purchaseSection(html);
  const controls = countPurchaseControls(region.inner);
  const declared = /\bdata-checkout-buttons=["'](\d+)["']/.exec(region.openTag);
  const errors = [];

  console.log(`check-download-page-words: ${page.entry}`);
  console.log(`shared source: data/pricing.json model.approved_pricing_text`);
  console.log(`approved_pricing_text=${text}`);

  let present = 0;
  for (const [index, fact] of facts.entries()) {
    const found = plain.includes(fact);
    if (found) present += 1;
    else errors.push(`missing shared pricing fact ${index + 1}: ${fact}`);
    console.log(`pricing_fact_${index + 1}=${fact} :: ${found ? "present" : "MISSING"}`);
  }
  console.log(`shared_pricing_facts=${present}/${facts.length}`);
  if (!plain.includes(text)) {
    errors.push(`the approved pricing sentence is not stated verbatim: ${text}`);
  }
  console.log(`approved_sentence_verbatim=${plain.includes(text) ? "present" : "MISSING"}`);

  let tiersBesidePurchase = 0;
  for (const claim of TIER_CLAIMS) {
    const capability = registry.get(claim.capability);
    if (!capability) {
      errors.push(`capability_registry has no entry for ${claim.capability}`);
      continue;
    }
    const statement = tierStatement(section, claim);
    if (!statement) {
      console.log(`${claim.label} (${claim.capability}): MISSING beside the purchase path`);
      errors.push(`missing ${claim.label} statement (${claim.capability}) beside the purchase path`);
      continue;
    }
    const needsMarker = !SELLABLE_STATUSES.has(capability.status);
    const marker = markers.find((word) => statement.toLowerCase().includes(word.trim().toLowerCase()));
    tiersBesidePurchase += 1;
    console.log(
      `${claim.label} (${claim.capability}, ${capability.status}): present, forward_looking_marker=${marker ? `"${marker.trim()}"` : "none"}`,
    );
    console.log(`  ${statement}`);
    if (needsMarker && !marker) {
      errors.push(
        `${claim.label} names ${claim.capability}, which is ${capability.status}, with no forward-looking marker`,
      );
    }
    if (capability.sellable === true) {
      errors.push(`${claim.capability} is marked sellable; this check assumes the Planned Scrub stages`);
    }
  }
  console.log(`tier_statements_beside_purchase=${tiersBesidePurchase}/${TIER_CLAIMS.length}`);

  const state = controls.total === 0 ? "hidden" : controls.total === 1 ? "working" : "ambiguous";
  console.log(`checkout_ready=${readiness.ready} (keyserver-cf/src/lib/prepaid-redemption-readiness.ts)`);
  console.log(`purchase_controls=${controls.total} (buttons=${controls.buttons}, checkout links=${controls.links})`);
  console.log(`purchase_button=${state}`);

  if (declared && Number(declared[1]) !== controls.total) {
    errors.push(
      `data-checkout-buttons="${declared[1]}" disagrees with the ${controls.total} control(s) actually rendered`,
    );
  }
  if (readiness.ready) {
    if (controls.total !== 1) {
      errors.push(`checkout is ready, so exactly one working purchase control is required, found ${controls.total}`);
    } else if (!/\bdata-checkout-url=["'][^"']+["']/.test(region.inner)) {
      errors.push("the purchase control has no data-checkout-url, so it cannot open checkout");
    }
  } else {
    if (controls.total !== 0) {
      errors.push(
        `checkout is not ready, so the purchase button must be hidden, found ${controls.total} control(s)`,
      );
    }
    if (!plain.includes(readiness.reason)) {
      errors.push(`the hidden purchase control does not state the refusal reason: ${readiness.reason}`);
    }
    console.log(`refusal_reason=${plain.includes(readiness.reason) ? "stated" : "MISSING"}: ${readiness.reason}`);
  }

  if (errors.length > 0) throw new Error(errors.join("\n"));
  console.log(
    `check-download-page-words: complete. ${present}/4 shared pricing facts, ${tiersBesidePurchase}/3 tier statements, purchase button ${state}.`,
  );
}

const invokedDirectly = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) {
  try {
    checkDownloadPage(process.argv[2]);
  } catch (error) {
    console.error(`check-download-page-words: fatal error: ${error.message}`);
    process.exit(1);
  }
}

export { checkDownloadPage };
