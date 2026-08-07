#!/usr/bin/env node

import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const DEFAULT_PAGE = "docs/donate.html";
const REQUIRED_SENTENCE = "Donations do not unlock Pro.";

function repoPath(input) {
  const entry = input || DEFAULT_PAGE;
  if (path.isAbsolute(entry) || entry.includes("..")) {
    throw new Error(`unsafe donate page path: ${entry}`);
  }
  const fullPath = path.join(REPO_ROOT, entry);
  if (!existsSync(fullPath)) throw new Error(`donate page is missing: ${entry}`);
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
    spans.push({ start: match.index, end: match.index + match[0].length, text: match[0].trim() });
  }
  return spans;
}

function coveredByRequiredSentence(index, spans) {
  return spans.some((span) => span.text === REQUIRED_SENTENCE && index >= span.start && index < span.end);
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
  const plain = stripHtml(readFileSync(page.fullPath, "utf8"));
  const requiredSentenceFound = sentenceSpans(plain).some((span) => span.text === REQUIRED_SENTENCE);
  const proCodeClaims = affirmativeClaimCount(
    plain,
    /\bdonations?\b[^.!?]{0,120}\b(?:give|gives|get|gets|grant|grants|send|sends|issue|issues|include|includes|come with|comes with)\b[^.!?]{0,80}\b(?:pro\s+code|activation\s+code|license\s+code)\b/gi,
  );
  const unlockClaims = affirmativeClaimCount(
    plain,
    /\bdonations?\b[^.!?]{0,120}\bunlock(?:s|ed|ing)?\b[^.!?]{0,80}\bPro\b/gi,
  );

  console.log(`check-donate-page-words: ${page.entry}`);
  console.log(`required sentence: ${requiredSentenceFound ? `"${REQUIRED_SENTENCE}"` : "MISSING"}`);
  console.log(`donation Pro-code claims: ${proCodeClaims}`);
  console.log(`donation unlocks Pro claims: ${unlockClaims}`);

  const errors = [];
  if (!requiredSentenceFound) errors.push(`missing required sentence: ${REQUIRED_SENTENCE}`);
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
