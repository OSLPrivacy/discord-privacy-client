import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sentence = /Signal may be the better choice/;
const faqPage = readFileSync(path.join(root, 'docs/faq.html'), 'utf8');

function textFromHtml(html) {
  return html
    .replace(/<[^>]*>/g, ' ')
    .replace(/&nbsp;/g, ' ')
    .replace(/&amp;/g, '&')
    .replace(/\s+/g, ' ')
    .trim();
}

function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function answerForQuestion(question) {
  const pattern = new RegExp(
    `<section\\b[^>]*>\\s*<h2>${escapeRegExp(question)}</h2>([\\s\\S]*?)</section>`,
    'u',
  );
  const match = faqPage.match(pattern);
  assert.ok(match, `FAQ question is missing: ${question}`);
  return textFromHtml(match[1]);
}

test('T11-T28 keeps the Signal point-away advice on FAQ and comparison pages', () => {
  for (const page of ['docs/faq.html', 'docs/compare.html']) {
    assert.match(readFileSync(path.join(root, page), 'utf8'), sentence, `${page} must retain the point-away advice`);
  }
});

test('T1536 question check finds all six requested FAQ answers', () => {
  const checks = [
    {
      label: 'ban risk',
      question: 'Can OSL get my account banned?',
      required: [
        /may violate Discord's Terms of Service/u,
        /Discord may ban your account/u,
        /does not publish a ban-risk percentage/u,
      ],
    },
    {
      label: 'bad messages',
      question: 'Does OSL make bad messages safe to send?',
      required: [
        /not a moderation system/u,
        /legal advice/u,
        /recipients can still report, copy, screenshot, export, or retain/u,
      ],
    },
    {
      label: 'stopping',
      question: 'What happens when I stop AutoScrub?',
      required: [
        /not available in this build/u,
        /stops on limits, challenges, changed content, or failed checks/u,
        /stopping after the checked local items already in review/u,
      ],
    },
    {
      label: 'logout',
      question: 'Does Burn log me out or delete connected-service history?',
      required: [
        /login session, cookies, and native carrier history are never members/u,
        /Unlink\/logout is a separate user action/u,
      ],
    },
    {
      label: 'Pro benefits',
      question: 'What does Pro include today?',
      required: [
        /Pro is an early-access purchase/u,
        /protected text on Discord with no message limit/u,
        /Encrypted images, other file types, AutoScrub, view once, expiry, burn, and AI-written cover text arrive at v1/u,
      ],
    },
    {
      label: 'pricing facts',
      question: 'What are the pricing facts?',
      required: [
        /\$5 is the intended price for one month of Pro/u,
        /Nothing renews automatically and there is nothing to cancel/u,
        /OSL never stores payment details/u,
        /no OSL account is required/u,
        /Checkout is paused until one-month redemption and expiry are implemented/u,
      ],
    },
  ];

  const found = [];
  for (const check of checks) {
    const answer = answerForQuestion(check.question);
    for (const required of check.required) {
      assert.match(answer, required, `${check.label} answer must include ${required}`);
    }
    found.push(check.label);
  }

  assert.equal(found.length, 6);
  console.log(`faq question check: found ${found.length} answers: ${found.join(', ')}`);
});
