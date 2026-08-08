#!/usr/bin/env node
/** TASK 1528: check Pricing repeats the Download page's shared pricing terms and routes only to the healthy purchase path. */

import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const DEFAULT_PAGE = "docs/fixtures/pricing.html";
const CHECKOUT_URL_PATTERN = /^https:\/\/checkout\.oslprivacy\.com\/session\/[A-Za-z0-9-]+$/;

function sharedPricingFacts() {
  const pricing = JSON.parse(readFileSync(path.join(REPO_ROOT, "data/pricing.json"), "utf8"));
  const text = pricing?.model?.approved_pricing_text;
  if (typeof text !== "string" || text.length === 0) {
    throw new Error("data/pricing.json model.approved_pricing_text is missing");
  }
  // "5 dollars, one month from code entry, no renewal, no OSL card storage." -> 4 facts.
  return text.replace(/\.$/, "").split(", ");
}

function repoPath(input) {
  const entry = input || DEFAULT_PAGE;
  if (path.isAbsolute(entry) || entry.includes("..")) {
    throw new Error(`unsafe pricing page path: ${entry}`);
  }
  const fullPath = path.join(REPO_ROOT, entry);
  if (!existsSync(fullPath)) throw new Error(`pricing page is missing: ${entry}`);
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

function checkoutRoutes(html) {
  const routes = [];
  const attrRegex = /data-checkout-url=(["'])(.*?)\1/gi;
  for (const match of html.matchAll(attrRegex)) routes.push(match[2]);
  const hrefRegex = /<a\b[^>]*\bhref=(["'])(.*?)\1[^>]*>/gi;
  for (const match of html.matchAll(hrefRegex)) {
    if (/checkout/i.test(match[2])) routes.push(match[2]);
  }
  return routes;
}

function checkPricingPage(entry) {
  const page = repoPath(entry);
  const html = readFileSync(page.fullPath, "utf8");
  const plain = stripHtml(html);
  const facts = sharedPricingFacts();
  const missingFacts = facts.filter((fact) => !plain.includes(fact));

  const routes = checkoutRoutes(html);
  const workingRoutes = routes.filter((route) => CHECKOUT_URL_PATTERN.test(route));
  const brokenRoutes = routes.filter((route) => !CHECKOUT_URL_PATTERN.test(route));

  console.log(`check-pricing-page-words: ${page.entry}`);
  for (const [index, fact] of facts.entries()) {
    console.log(`fact_${index + 1}: ${plain.includes(fact) ? `"${fact}"` : "MISSING"}`);
  }
  console.log(`purchase routes: ${routes.length}`);
  console.log(`working purchase routes: ${workingRoutes.length}`);
  console.log(`broken purchase routes: ${brokenRoutes.length}`);

  const errors = [];
  if (missingFacts.length > 0) {
    errors.push(`missing shared pricing fact(s): ${missingFacts.join(", ")}`);
  }
  if (workingRoutes.length === 0) {
    errors.push("no working purchase route to the healthy checkout path");
  }
  if (routes.length !== workingRoutes.length) {
    errors.push(`purchase routes must only point at the healthy checkout path, found ${brokenRoutes.length} extra/broken route(s): ${brokenRoutes.join(", ")}`);
  }
  if (workingRoutes.length > 1) {
    errors.push(`expected exactly 1 working purchase route, found ${workingRoutes.length}`);
  }
  if (errors.length > 0) throw new Error(errors.join("\n"));
  console.log("check-pricing-page-words: complete.");
}

try {
  checkPricingPage(process.argv[2]);
} catch (error) {
  console.error(`check-pricing-page-words: fatal error: ${error.message}`);
  process.exit(1);
}
