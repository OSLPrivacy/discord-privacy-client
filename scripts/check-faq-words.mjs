#!/usr/bin/env node

// TASK 1537 - FAQ word gate.
//
// Two things have to hold on docs/faq.html at once:
//   1. every one of the six required questions has an answer, and that answer
//      says the honest thing this build can support (the phrases below);
//   2. the answer to the pricing question carries the shared pricing text
//      verbatim, taken from data/pricing.json model.approved_pricing_text
//      (TASK 1511/1512 made that field the single source for those words).
//
// The answers also have to stay SHORT, so each one is capped at MAX_ANSWER_WORDS.
// The script exits non-zero naming exactly what drifted.

import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPTS_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.dirname(SCRIPTS_DIR);
const FAQ_PATH = path.join(REPO_ROOT, 'docs', 'faq.html');
const PRICING_PATH = path.join(REPO_ROOT, 'data', 'pricing.json');
const MAX_ANSWER_WORDS = 100;

// label -> the question heading on the page, plus the honest phrases the answer
// must contain. Same six topics the TASK 1536 question check names.
const ANSWERS = [
  {
    label: 'ban risk',
    question: 'Can OSL get my account banned?',
    required: [
      "may violate Discord's Terms of Service",
      'Discord may ban your account',
      'does not publish a ban-risk percentage',
    ],
  },
  {
    label: 'bad messages',
    question: 'Does OSL make bad messages safe to send?',
    required: [
      'not a moderation system',
      'legal advice',
      'recipients can still report, copy, screenshot, export, or retain',
    ],
  },
  {
    label: 'stopping',
    question: 'What happens when I stop AutoScrub?',
    required: [
      'not available in this build',
      'stops on limits, challenges, changed content, or failed checks',
      'stopping after the checked local items already in review',
    ],
  },
  {
    label: 'logout',
    question: 'Does Burn log me out or delete connected-service history?',
    required: [
      'Burn is not in this build yet',
      'login session, cookies, and native carrier history are never members',
      'Unlink/logout is a separate user action',
    ],
  },
  {
    label: 'Pro benefits',
    question: 'What does Pro include today?',
    required: [
      'Pro is an early-access purchase',
      'protected text on Discord with no message limit',
      'Encrypted images, other file types, AutoScrub, view once, expiry, burn, and AI-written cover text arrive at v1',
    ],
  },
  {
    label: 'pricing facts',
    question: 'What are the pricing facts?',
    required: [
      '$5 is the intended price for one month of Pro',
      'Nothing renews automatically and there is nothing to cancel',
      'OSL never stores payment details',
      'no OSL account is required',
      'Checkout is paused until one-month redemption and expiry are implemented',
    ],
  },
];

function textFromHtml(html) {
  return html
    .replace(/<[^>]*>/gu, ' ')
    .replace(/&nbsp;/gu, ' ')
    .replace(/&amp;/gu, '&')
    .replace(/\s+/gu, ' ')
    .trim();
}

function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&');
}

function sectionFor(html, question) {
  const pattern = new RegExp(
    `<section\\b[^>]*>\\s*<h2>${escapeRegExp(question)}</h2>([\\s\\S]*?)</section>`,
    'u',
  );
  return html.match(pattern)?.[1] ?? null;
}

const faqHtml = readFileSync(FAQ_PATH, 'utf8');
const pricing = JSON.parse(readFileSync(PRICING_PATH, 'utf8'));
const sharedPricingText = pricing?.model?.approved_pricing_text;

const problems = [];

if (typeof sharedPricingText !== 'string' || sharedPricingText.trim().length === 0) {
  problems.push('data/pricing.json model.approved_pricing_text is missing or empty');
}

console.log('check-faq-words: page docs/faq.html');
console.log('check-faq-words: shared source data/pricing.json model.approved_pricing_text');
console.log(`check-faq-words: approved_pricing_text=${sharedPricingText ?? '(missing)'}`);

const found = [];
for (const answer of ANSWERS) {
  const raw = sectionFor(faqHtml, answer.question);
  if (raw === null) {
    problems.push(`no answer section for question: ${answer.question}`);
    console.log(`check-faq-words: ${answer.label} answer=MISSING (no section for "${answer.question}")`);
    continue;
  }
  const text = textFromHtml(raw);
  const missing = answer.required.filter((phrase) => !text.includes(phrase));
  const words = text.split(' ').filter(Boolean).length;
  if (missing.length > 0) {
    for (const phrase of missing) {
      problems.push(`${answer.label} answer is missing: ${phrase}`);
    }
    console.log(`check-faq-words: ${answer.label} answer=INCOMPLETE missing=${missing.length} words=${words}`);
    continue;
  }
  if (words > MAX_ANSWER_WORDS) {
    problems.push(`${answer.label} answer is ${words} words, over the ${MAX_ANSWER_WORDS}-word short-answer cap`);
    console.log(`check-faq-words: ${answer.label} answer=TOO_LONG words=${words}`);
    continue;
  }
  found.push(answer.label);
  console.log(
    `check-faq-words: ${answer.label} answer=present phrases=${answer.required.length} words=${words}`,
  );
}

const pageText = textFromHtml(faqHtml);
const pricingSection = sectionFor(faqHtml, 'What are the pricing facts?');
const sharedPresent =
  typeof sharedPricingText === 'string' &&
  pageText.includes(sharedPricingText) &&
  pricingSection !== null &&
  textFromHtml(pricingSection).includes(sharedPricingText);

if (sharedPresent) {
  console.log('check-faq-words: shared_pricing_text=present (verbatim, in the pricing-facts answer)');
} else {
  console.log('check-faq-words: shared_pricing_text=MISSING');
  problems.push('docs/faq.html does not carry the shared pricing text verbatim in the pricing-facts answer');
}

console.log(
  `check-faq-words: answers found ${found.length}/${ANSWERS.length}: ${found.join(', ') || '(none)'}`,
);

if (problems.length > 0) {
  for (const problem of problems) {
    console.error(`check-faq-words: FAIL ${problem}`);
  }
  console.error(`check-faq-words: fatal error: ${problems.length} FAQ word problem(s)`);
  process.exit(1);
}

console.log(
  `check-faq-words: FAQ contains all ${ANSWERS.length} answers and the shared pricing text.`,
);
