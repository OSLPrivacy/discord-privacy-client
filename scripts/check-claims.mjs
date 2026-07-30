#!/usr/bin/env node

// Claim-channel gate for the public crawl surface.
//
// The manifest declares where public claims can appear. This script fails if a
// declared channel has no concrete public-surface evidence or if crawl entries
// point outside existing textual files.

import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPTS_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.dirname(SCRIPTS_DIR);
const MANIFEST_PATH = path.join(REPO_ROOT, 'data', 'public-surface-manifest.json');
const MIN_PUBLIC_CLAIM_CHANNELS = 9;
const REQUIRED_CHANNELS = [
  'rendered-text',
  'document-title',
  'metadata-content',
  'accessible-name',
  'alt-title-placeholder-value',
  'public-data-attribute',
  'json-ld',
  'inline-script-copy',
  'textual-asset',
];

function readManifest() {
  const manifest = JSON.parse(readFileSync(MANIFEST_PATH, 'utf8'));
  if (manifest.schema_version !== 1 || manifest.manifest_id !== 'osl-public-surface') {
    throw new Error('public surface manifest must use schema_version 1 and manifest_id osl-public-surface');
  }
  if (!Array.isArray(manifest.claim_channels) || manifest.claim_channels.length < MIN_PUBLIC_CLAIM_CHANNELS) {
    throw new Error(`public surface manifest must declare at least ${MIN_PUBLIC_CLAIM_CHANNELS} claim channels`);
  }
  for (const channel of REQUIRED_CHANNELS) {
    if (!manifest.claim_channels.includes(channel)) throw new Error(`public surface manifest is missing ${channel}`);
  }
  if (!Array.isArray(manifest.html) || manifest.html.length === 0) throw new Error('public surface manifest must declare HTML pages');
  if (!Array.isArray(manifest.assets) || manifest.assets.length === 0) throw new Error('public surface manifest must declare assets');
  return manifest;
}

function readEntry(entry) {
  if (typeof entry !== 'string' || path.isAbsolute(entry) || entry.includes('..')) {
    throw new Error(`manifest entry is not a safe repo-relative path: ${String(entry)}`);
  }
  const fullPath = path.join(REPO_ROOT, entry);
  if (!existsSync(fullPath)) throw new Error(`manifest entry is missing on disk: ${entry}`);
  return { entry, fullPath, text: readFileSync(fullPath, 'utf8') };
}

function stripHtmlNoise(text) {
  return text
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, ' ')
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, ' ')
    .replace(/<[^>]+>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

function extractJsonLd(file) {
  const blocks = [];
  const regex = /<script\b[^>]*type=["']application\/ld\+json["'][^>]*>([\s\S]*?)<\/script>/gi;
  for (const match of file.text.matchAll(regex)) {
    const raw = match[1].trim();
    const parsed = JSON.parse(raw);
    blocks.push({ entry: file.entry, name: parsed.name || parsed['@type'] || 'json-ld' });
  }
  return blocks;
}

function evidenceForChannel(channel, files) {
  if (channel === 'rendered-text') {
    return files.html
      .map((file) => ({ entry: file.entry, textLength: stripHtmlNoise(file.text).length }))
      .filter((item) => item.textLength >= 20);
  }
  if (channel === 'document-title') {
    return files.html
      .map((file) => ({ entry: file.entry, title: file.text.match(/<title>([\s\S]*?)<\/title>/i)?.[1]?.trim() || '' }))
      .filter((item) => item.title.length > 0);
  }
  if (channel === 'metadata-content') {
    return files.html.flatMap((file) => [...file.text.matchAll(/<meta\b[^>]*\bcontent=["']([^"']+)["'][^>]*>/gi)]
      .map((match) => ({ entry: file.entry, content: match[1] })));
  }
  if (channel === 'accessible-name') {
    return files.all.flatMap((file) => [...file.text.matchAll(/\baria-(?:label|labelledby)=["']([^"']+)["']|<label\b[^>]*>([\s\S]*?)<\/label>/gi)]
      .map((match) => ({ entry: file.entry, value: (match[1] || stripHtmlNoise(match[2] || '')).slice(0, 80) })));
  }
  if (channel === 'alt-title-placeholder-value') {
    return files.all.flatMap((file) => [...file.text.matchAll(/\b(?:alt|title|placeholder|value)=["']([^"']*)["']/gi)]
      .filter((match) => match[1].trim().length > 0)
      .map((match) => ({ entry: file.entry, value: match[1].trim().slice(0, 80) })));
  }
  if (channel === 'public-data-attribute') {
    return files.all.flatMap((file) => [...file.text.matchAll(/\bdata-[a-zA-Z0-9_-]+=/g)]
      .map((match) => ({ entry: file.entry, attribute: match[0].slice(0, -1) })));
  }
  if (channel === 'json-ld') {
    return files.html.flatMap(extractJsonLd);
  }
  if (channel === 'inline-script-copy') {
    return files.assets
      .filter((file) => file.entry.endsWith('.js') && /\.(?:textContent|innerText|innerHTML)\b|`[^`]{20,}`|["'][^"']{20,}["']/.test(file.text))
      .map((file) => ({ entry: file.entry, bytes: Buffer.byteLength(file.text, 'utf8') }));
  }
  if (channel === 'textual-asset') {
    const allowed = new Set(files.manifest.textual_asset_extensions || []);
    return files.assets
      .filter((file) => allowed.has(path.extname(file.entry).toLowerCase()) && file.text.trim().length > 0)
      .map((file) => ({ entry: file.entry, bytes: Buffer.byteLength(file.text, 'utf8') }));
  }
  throw new Error(`unknown public claim channel: ${channel}`);
}

function run() {
  const manifest = readManifest();
  const html = manifest.html.map(readEntry);
  const assets = manifest.assets.map(readEntry);
  const files = { manifest, html, assets, all: [...html, ...assets] };
  const channelEvidence = {};
  const failures = [];
  for (const channel of manifest.claim_channels) {
    const evidence = evidenceForChannel(channel, files);
    channelEvidence[channel] = evidence;
    if (evidence.length === 0) failures.push(channel);
  }

  console.log('\ncheck-claims summary');
  console.log(`  public HTML pages : ${html.length}`);
  console.log(`  public assets     : ${assets.length}`);
  console.log(`  claim channels    : ${manifest.claim_channels.length}`);
  for (const channel of manifest.claim_channels) {
    console.log(`  ${channel.padEnd(28)} ${channelEvidence[channel].length}`);
  }
  if (failures.length > 0) {
    throw new Error(`claim channels have no public-surface evidence: ${failures.join(', ')}`);
  }
  console.log('\ncheck-claims: complete.');
}

try {
  run();
} catch (error) {
  console.error(`check-claims: fatal error: ${error.stack || error.message}`);
  process.exit(1);
}
