#!/usr/bin/env node

import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const DEFAULT_PAGE = "docs/fixtures/checkout-success.html";
const CODE_PATTERN = /\bOSL-[A-Z0-9]{4}-[A-Z0-9]{4}-[A-Z0-9]{4}-[A-Z0-9]{4}\b/g;
const REQUIRED_SENTENCES = {
  startDateRule: "Your month starts when you enter the code in the OSL app.",
  noRenewal: "This code does not renew.",
  noCardStorage: "OSL does not store your card.",
  appRedemption: "Redeem this code in the OSL app.",
};

function repoPath(input) {
  const entry = input || DEFAULT_PAGE;
  if (path.isAbsolute(entry) || entry.includes("..")) {
    throw new Error(`unsafe success page path: ${entry}`);
  }
  const fullPath = path.join(REPO_ROOT, entry);
  if (!existsSync(fullPath)) throw new Error(`success page is missing: ${entry}`);
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

function sentenceSpans(text) {
  const spans = [];
  const regex = /[^.!?]+[.!?]/g;
  for (const match of text.matchAll(regex)) {
    spans.push({ text: match[0].trim() });
  }
  return spans;
}

function hrefs(html) {
  const values = [];
  const regex = /<a\b[^>]*\bhref=(["'])(.*?)\1/gi;
  for (const match of html.matchAll(regex)) values.push(match[2]);
  return values;
}

function checkSuccessPage(entry) {
  const page = repoPath(entry);
  const html = readFileSync(page.fullPath, "utf8");
  const plain = stripHtml(html);
  const sentences = new Set(sentenceSpans(plain).map((span) => span.text));
  const codes = [...new Set(plain.match(CODE_PATTERN) || [])];
  const appRedemptionLinks = hrefs(html).filter((href) =>
    codes.some((code) => href === `osl://activate?code=${code}`)
  );

  console.log(`check-checkout-success-page-words: ${page.entry}`);
  console.log(`activation codes: ${codes.length}${codes.length > 0 ? ` (${codes.join(", ")})` : ""}`);
  for (const [label, sentence] of Object.entries(REQUIRED_SENTENCES)) {
    console.log(`${label}: ${sentences.has(sentence) ? `"${sentence}"` : "MISSING"}`);
  }
  console.log(`app redemption links: ${appRedemptionLinks.length}`);
  console.log(`dead end: ${appRedemptionLinks.length > 0 ? "no" : "yes"}`);

  const errors = [];
  if (codes.length === 0) errors.push("missing visible OSL activation code");
  for (const [label, sentence] of Object.entries(REQUIRED_SENTENCES)) {
    if (!sentences.has(sentence)) errors.push(`missing ${label}: ${sentence}`);
  }
  if (appRedemptionLinks.length === 0) {
    errors.push("missing app redemption link for the visible activation code");
  }
  if (errors.length > 0) throw new Error(errors.join("\n"));
  console.log("check-checkout-success-page-words: complete.");
}

try {
  checkSuccessPage(process.argv[2]);
} catch (error) {
  console.error(`check-checkout-success-page-words: fatal error: ${error.message}`);
  process.exit(1);
}
