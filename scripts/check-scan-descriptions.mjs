#!/usr/bin/env node
/**
 * TASK 1535 - scan descriptions must not be written as a file scan.
 *
 * Account scanning in this build reviews the messages inside one account export
 * the owner picks and hands to OSL. It is not a sweep of a folder, a download
 * history, a disk, or a device. Copy that says otherwise -- "Scrub only scans
 * downloaded files" and every close relative of it -- promises a product that
 * does not exist, so this checker reads every public page and textual asset,
 * pulls out the sentences that describe scanning, and fails on any of them that
 * puts a device-file object on the end of a scan verb.
 *
 * A denial is not a claim: "OSL does not scan the files on your device" and
 * "This page is not a scan of your device" are allowed, and only allowed when
 * the negation sits in front of the scan verb in the same sentence.
 */

import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const MANIFEST_PATH = path.join(REPO_ROOT, 'data', 'public-surface-manifest.json');

/** Words that mean "something on the owner's machine", not "a message". */
const FILE_OBJECT = String.raw`(?:downloaded|download|downloads|file|files|folder|folders|directory|directories|document|documents|photo|photos|picture|pictures|disk|disks|drive|drives|hard\s+drive|device|devices|machine|computer|desktop|library|attachments?\s+on\s+disk)`;
/** Verbs a scan description is written with. */
const SCAN_VERB = String.raw`(?:scan|scans|scanned|scanning|sweep|sweeps|swept|comb|combs|combed)`;
const NEGATION = String.raw`(?:not|never|no|n't|without|nothing|neither|nor)`;

/** A scan verb whose object, within the same sentence, is a thing on disk. */
const FORWARD_CLAIM = new RegExp(
  String.raw`\b${SCAN_VERB}\b[^.!?;]{0,60}?\b${FILE_OBJECT}\b`,
  'i',
);
/** The same claim written the other way round: "your files are scanned". */
const REVERSE_CLAIM = new RegExp(
  String.raw`\b${FILE_OBJECT}\b[\s\w,'-]{0,40}?\b(?:is|are|was|were|get|gets|got)\s+(?:${SCAN_VERB}ed|scanned|swept|combed)\b`,
  'i',
);

/** True when the sentence denies the scan instead of promising it. */
export function isDenial(sentence) {
  const negation = new RegExp(String.raw`\b${NEGATION}\b`, 'ig');
  const verb = new RegExp(String.raw`\b${SCAN_VERB}\b`, 'ig');
  const negations = [...sentence.matchAll(negation)].map((match) => match.index);
  if (negations.length === 0) return false;
  const verbs = [...sentence.matchAll(verb)].map((match) => match.index);
  // Every scan verb in the sentence has to sit behind a negation, otherwise a
  // denial glued to a promise ("OSL never sells data and scans your files")
  // would slip through.
  return verbs.every((at) => negations.some((no) => no < at));
}

/** Splits page text into the units a reader takes in as one statement. */
export function sentencesOf(text) {
  return text
    .split(/(?<=[.!?;])\s+|\n+/)
    .map((piece) => piece.replace(/\s+/g, ' ').trim())
    .filter(Boolean);
}

/** Sentences that describe scanning at all -- the ones this task is about. */
export function scanDescriptions(text) {
  const mentions = new RegExp(String.raw`\b(?:${SCAN_VERB}|scrub|scrubs|scrubbed|autoscrub)\b`, 'i');
  return sentencesOf(text).filter((sentence) => mentions.test(sentence));
}

/** Every scan description in the text that claims a device-file scan. */
export function fileScanClaims(text) {
  return scanDescriptions(text).filter(
    (sentence) => !isDenial(sentence) && (FORWARD_CLAIM.test(sentence) || REVERSE_CLAIM.test(sentence)),
  );
}

/** Rendered words plus the attribute copy a reader can still be shown. */
export function readableText(source, entry) {
  if (!/\.html?$/i.test(entry)) return decodeEntities(source);
  const withoutCode = source
    .replace(/<script\b[\s\S]*?<\/script>/gi, ' ')
    .replace(/<style\b[\s\S]*?<\/style>/gi, ' ');
  const attributes = [...withoutCode.matchAll(
    /\b(?:content|title|alt|placeholder|value|aria-label|aria-description|data-[\w-]+)\s*=\s*"([^"]*)"/gi,
  )].map((match) => match[1]);
  const rendered = withoutCode.replace(/<[^>]+>/g, ' ');
  return decodeEntities([rendered, ...attributes].join('\n'));
}

function decodeEntities(value) {
  return value
    .replace(/&nbsp;|&#160;|&#xa0;/gi, ' ')
    .replace(/&amp;/gi, '&')
    .replace(/&lt;/gi, '<')
    .replace(/&gt;/gi, '>')
    .replace(/&quot;/gi, '"')
    .replace(/&#39;|&apos;/gi, "'");
}

export function readManifestEntries() {
  const manifest = JSON.parse(readFileSync(MANIFEST_PATH, 'utf8'));
  return [...manifest.html, ...manifest.assets];
}

/** Reads the public surface and reports scan descriptions and any claims. */
export function auditPublicSurface(entries = readManifestEntries()) {
  const files = [];
  for (const entry of entries) {
    const text = readableText(readFileSync(path.join(REPO_ROOT, entry), 'utf8'), entry);
    files.push({ entry, descriptions: scanDescriptions(text), claims: fileScanClaims(text) });
  }
  return {
    files,
    descriptionCount: files.reduce((total, file) => total + file.descriptions.length, 0),
    claims: files.flatMap((file) => file.claims.map((sentence) => ({ entry: file.entry, sentence }))),
  };
}

/** The claim this task exists to keep off the page, and its near neighbours. */
export const SELF_TEST_CLAIMS = [
  'Scrub only scans downloaded files.',
  'Scrub scans the files you have downloaded.',
  'Account scanning sweeps your folders.',
  'Your downloaded files are scanned by Scrub.',
  'Scanning combs through the documents on your computer.',
];
/** Honest sentences the checker must leave alone. */
export const SELF_TEST_SAFE = [
  'Account scanning reviews the messages inside one account export that you choose and hand to OSL.',
  'It does not scan the files on your device, and it does not scan your downloads.',
  'This page is not a scan of your device.',
  'Scanning or deleting can change an account, and some changes cannot be undone.',
];

export function selfTest() {
  const missed = SELF_TEST_CLAIMS.filter((sentence) => fileScanClaims(sentence).length === 0);
  const falsePositives = SELF_TEST_SAFE.filter((sentence) => fileScanClaims(sentence).length > 0);
  return { missed, falsePositives };
}

function main() {
  const audit = auditPublicSurface();
  console.log('\ncheck-scan-descriptions summary');
  console.log(`  public entries read     : ${audit.files.length}`);
  console.log(`  scan descriptions found : ${audit.descriptionCount}`);
  for (const file of audit.files) {
    if (file.descriptions.length > 0) {
      console.log(`  ${file.entry.padEnd(38)} ${file.descriptions.length}`);
    }
  }
  const { missed, falsePositives } = selfTest();
  console.log(`  self-test caught        : ${SELF_TEST_CLAIMS.length - missed.length}/${SELF_TEST_CLAIMS.length} file-scan claims`);
  console.log(`  self-test kept          : ${SELF_TEST_SAFE.length - falsePositives.length}/${SELF_TEST_SAFE.length} honest sentences`);
  if (missed.length > 0 || falsePositives.length > 0) {
    console.error('check-scan-descriptions: self-test failed');
    for (const sentence of missed) console.error(`  MISSED: ${sentence}`);
    for (const sentence of falsePositives) console.error(`  FLAGGED HONEST: ${sentence}`);
    process.exit(1);
  }
  console.log(`  file-scan claims        : ${audit.claims.length}`);
  for (const claim of audit.claims) console.error(`  FILE_SCAN_CLAIM ${claim.entry}: ${claim.sentence}`);
  if (audit.claims.length > 0) {
    console.error('check-scan-descriptions: a scan description claims a device-file scan');
    process.exit(1);
  }
  console.log('\ncheck-scan-descriptions: complete.');
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
