import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
  SELF_TEST_CLAIMS,
  SELF_TEST_SAFE,
  auditPublicSurface,
  fileScanClaims,
  scanDescriptions,
} from '../scripts/check-scan-descriptions.mjs';

const page = readFileSync(new URL('./how-it-works.html', import.meta.url), 'utf8');
const text = page
  .replace(/<script\b[\s\S]*?<\/script>/gi, ' ')
  .replace(/<style\b[\s\S]*?<\/style>/gi, ' ')
  .replace(/<[^>]+>/g, ' ')
  .replace(/&nbsp;/gi, ' ')
  .replace(/\s+/g, ' ')
  .trim();

const requiredPoints = [
  'Protected messaging covers the messages you write and send through OSL. It is a separate job from account scanning, and neither one starts the other.',
  'Account scanning reviews the messages inside one account export that you choose and hand to OSL. It reads that one export and nothing else on your machine. It does not scan the files on your device, and it does not scan your downloads.',
  'A file takes part only when you attach it to a message you are sending. OSL does not go through your folders or your download history looking for things to open.',
  'Polite pace means OSL waits between actions on purpose. It works at a slow, ordinary speed instead of racing a service.',
  'Scanning or deleting can change an account, and some changes cannot be undone. OSL tells you what it is about to do and waits for your yes.',
];

test('TASK 1534 page text separates how-it-works points', () => {
  for (const point of requiredPoints) {
    assert.ok(text.includes(point), `missing page text: ${point}`);
    console.log(`TASK1534 point: ${point}`);
  }
});

test('TASK 1535 no scan description on the page claims a file scan', () => {
  const described = scanDescriptions(text);
  assert.ok(described.length > 0, 'the page must describe scanning at all');
  const claims = fileScanClaims(text);
  for (const sentence of described) {
    console.log(`TASK1535 scan description: ${sentence}`);
  }
  console.log(`TASK1535 how-it-works scan descriptions: ${described.length}, file-scan claims: ${claims.length}`);
  assert.deepEqual(claims, []);
  assert.ok(
    !/only\s+scans\s+downloaded\s+files/i.test(text),
    'the page must not say Scrub only scans downloaded files',
  );
});

test('TASK 1535 the checker catches the claim it forbids', () => {
  for (const sentence of SELF_TEST_CLAIMS) {
    assert.equal(fileScanClaims(sentence).length, 1, `missed file-scan claim: ${sentence}`);
    console.log(`TASK1535 caught: ${sentence}`);
  }
  for (const sentence of SELF_TEST_SAFE) {
    assert.deepEqual(fileScanClaims(sentence), [], `flagged an honest sentence: ${sentence}`);
    console.log(`TASK1535 kept: ${sentence}`);
  }
  const broken = text.replace(
    'Account scanning reviews the messages inside one account export',
    'Scrub only scans downloaded files. Account scanning reviews the messages inside one account export',
  );
  assert.notEqual(broken, text, 'the break-it fixture must actually change the page text');
  assert.equal(fileScanClaims(broken).length, 1, 'the page check must fail on the forbidden claim');
  console.log('TASK1535 page check fails when the forbidden claim is inserted: 1 claim');
});

test('TASK 1535 no public page describes scanning as a device-file scan', () => {
  const audit = auditPublicSurface();
  console.log(`TASK1535 public entries: ${audit.files.length}`);
  console.log(`TASK1535 scan descriptions across the public surface: ${audit.descriptionCount}`);
  for (const claim of audit.claims) {
    console.log(`TASK1535 claim: ${claim.entry}: ${claim.sentence}`);
  }
  console.log(`TASK1535 file-scan claims across the public surface: ${audit.claims.length}`);
  assert.deepEqual(audit.claims, []);
});
