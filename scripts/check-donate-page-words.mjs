#!/usr/bin/env node

import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const DEFAULT_PAGE = "docs/donate.html";
const REQUIRED_SENTENCE = "Donations do not unlock Pro.";
const SUPPORT_SENTENCE = "Donations support OSL.";
const MIN_PAYMENT_CHOICES = 2;
const PAYMENT_CHOICES = [
  { label: "Card", pattern: /\b(?:card|debit|credit)\b/i },
  { label: "Bitcoin", pattern: /\bbitcoin\b|\bbtc\b/i },
  { label: "Monero", pattern: /\bmonero\b|\bxmr\b/i },
  { label: "Bank transfer", pattern: /\bbank transfer\b/i },
];
const DONATION_TERM = /\b(?:donation|donations|donate|donates|donating|donor|donors|tip|tips)\b/i;
const ENTITLEMENT_TERM =
  /\bpro\s+code\b|\bactivation\s+code\b|\blicen[sc]e\s+code\b|\bpro\s+access\b|\bpro\s+licen[sc]e\b|\bpro\s+plan\b|\bpro\s+features?\b|\bunlocks?\s+pro\b|\bunlocked\s+pro\b|\bunlocking\s+pro\b/i;
const NEGATION_CUE =
  /\b(?:not|never|no|nor|none|without|separate|separately|cannot|can't|doesn't|don't|isn't|aren't|won't)\b/i;

function repoPath(input) {
  const entry = input || DEFAULT_PAGE;
  if (path.isAbsolute(entry) || entry.includes("..")) {
    throw new Error(`unsafe donate page path: ${entry}`);
  }
  const fullPath = path.join(REPO_ROOT, entry);
  if (!existsSync(fullPath)) throw new Error(`donate page is missing: ${entry}`);
  return { entry, fullPath };
}

// Block boundaries become newlines so an unpunctuated heading cannot glue
// itself onto the first sentence of the paragraph below it.
const BLOCK_TAG =
  /<\/?(?:h[1-6]|p|li|ul|ol|section|main|div|header|footer|nav|table|tr|td|th|br|blockquote|figcaption)\b[^>]*>/gi;

function stripHtml(text) {
  return text
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, " ")
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, " ")
    .replace(BLOCK_TAG, "\n")
    .replace(/<[^>]+>/g, " ")
    .replace(/&nbsp;/gi, " ")
    .replace(/&amp;/gi, "&")
    .replace(/[^\S\n]+/g, " ")
    .replace(/\s*\n\s*/g, "\n")
    .trim();
}

function sentenceSpans(text) {
  const spans = [];
  const regex = /[^.!?\n]+[.!?]/g;
  for (const match of text.matchAll(regex)) {
    spans.push({ start: match.index, end: match.index + match[0].length, text: match[0].trim() });
  }
  return spans;
}

function coveredByRequiredSentence(index, spans) {
  return spans.some((span) => span.text === REQUIRED_SENTENCE && index >= span.start && index < span.end);
}

function listItemTexts(html) {
  const items = [];
  for (const match of html.matchAll(/<li\b[^>]*>([\s\S]*?)<\/li>/gi)) items.push(stripHtml(match[1]));
  return items;
}

function offeredPaymentChoices(items) {
  return PAYMENT_CHOICES.filter((choice) => items.some((item) => choice.pattern.test(item)))
    .map((choice) => choice.label);
}

// A Pro-code promise is any sentence that ties a donation to Pro entitlement
// without denying it. The two required sentences are exempt: they state the
// separation, so they name both halves on purpose.
function proCodePromises(text) {
  return sentenceSpans(text)
    .map((span) => span.text)
    .filter((sentence) => sentence !== REQUIRED_SENTENCE && sentence !== SUPPORT_SENTENCE)
    .filter((sentence) =>
      DONATION_TERM.test(sentence) &&
      ENTITLEMENT_TERM.test(sentence) &&
      !NEGATION_CUE.test(sentence));
}

function affirmativeClaimCount(text, pattern) {
  const spans = sentenceSpans(text);
  let count = 0;
  for (const match of text.matchAll(pattern)) {
    if (!coveredByRequiredSentence(match.index, spans)) count += 1;
  }
  return count;
}

function checkDonatePage(entry) {
  const page = repoPath(entry);
  const html = readFileSync(page.fullPath, "utf8");
  const plain = stripHtml(html);
  const requiredSentenceFound = sentenceSpans(plain).some((span) => span.text === REQUIRED_SENTENCE);
  const supportSentenceFound = sentenceSpans(plain).some((span) => span.text === SUPPORT_SENTENCE);
  const choices = offeredPaymentChoices(listItemTexts(html));
  const promises = proCodePromises(plain);
  const proCodeClaims = affirmativeClaimCount(
    plain,
    /\bdonations?\b[^.!?\n]{0,120}\b(?:give|gives|get|gets|grant|grants|send|sends|issue|issues|include|includes|come with|comes with)\b[^.!?\n]{0,80}\b(?:pro\s+code|activation\s+code|license\s+code)\b/gi,
  );
  const unlockClaims = affirmativeClaimCount(
    plain,
    /\bdonations?\b[^.!?\n]{0,120}\bunlock(?:s|ed|ing)?\b[^.!?\n]{0,80}\bPro\b/gi,
  );

  console.log(`check-donate-page-words: ${page.entry}`);
  console.log(`support sentence: ${supportSentenceFound ? `"${SUPPORT_SENTENCE}"` : "MISSING"}`);
  console.log(`required sentence: ${requiredSentenceFound ? `"${REQUIRED_SENTENCE}"` : "MISSING"}`);
  console.log(`payment choices: ${choices.length}${choices.length > 0 ? ` (${choices.join(", ")})` : ""}`);
  console.log(`Pro-code promises: ${promises.length}`);
  for (const promise of promises) console.log(`  promise: ${promise}`);
  console.log(`donation Pro-code claims: ${proCodeClaims}`);
  console.log(`donation unlocks Pro claims: ${unlockClaims}`);

  const errors = [];
  if (!supportSentenceFound) errors.push(`missing support sentence: ${SUPPORT_SENTENCE}`);
  if (!requiredSentenceFound) errors.push(`missing required sentence: ${REQUIRED_SENTENCE}`);
  if (choices.length < MIN_PAYMENT_CHOICES) {
    errors.push(`found ${choices.length} plain payment choice(s), need at least ${MIN_PAYMENT_CHOICES}`);
  }
  if (promises.length !== 0) errors.push(`found ${promises.length} sentence(s) promising Pro for a donation`);
  if (proCodeClaims !== 0) errors.push(`found ${proCodeClaims} claim(s) that a donation gives a Pro code`);
  if (unlockClaims !== 0) errors.push(`found ${unlockClaims} claim(s) that a donation unlocks Pro`);
  if (errors.length > 0) throw new Error(errors.join("\n"));
  console.log("check-donate-page-words: complete.");
}

try {
  checkDonatePage(process.argv[2]);
} catch (error) {
  console.error(`check-donate-page-words: fatal error: ${error.message}`);
  process.exit(1);
}
