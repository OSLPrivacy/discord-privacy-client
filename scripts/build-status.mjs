#!/usr/bin/env node

// Build-status gate for the H8 public website evidence bundle.

import { existsSync, readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPTS_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.dirname(SCRIPTS_DIR);
const MANIFEST_PATH = path.join(REPO_ROOT, 'data', 'public-surface-manifest.json');
const MATRIX_PATH = path.join(REPO_ROOT, 'docs', 'evidence', 'website-matrix', 'matrix.json');
const REQUIRED_MODES = ['js-on', 'js-off', 'reduced-motion'];
const REQUIRED_WIDTHS = [320, 390, 768, 1280];

function sha256(buffer) {
  return createHash('sha256').update(buffer).digest('hex');
}

function pngFacts(buffer) {
  if (buffer.subarray(0, 8).toString('hex') !== '89504e470d0a1a0a') throw new Error('PNG signature is missing');
  if (buffer.subarray(12, 16).toString('ascii') !== 'IHDR') throw new Error('PNG IHDR is missing');
  return {
    width: buffer.readUInt32BE(16),
    height: buffer.readUInt32BE(20),
    bytes: buffer.length,
    sha256: sha256(buffer),
  };
}

function routeFromHtmlPath(htmlPath) {
  if (htmlPath.endsWith('/index.html')) return `/${htmlPath.slice(0, -'index.html'.length)}`;
  if (htmlPath === 'index.html') return '/';
  return `/${htmlPath.slice(0, -'.html'.length)}`;
}

function loadJson(file, label) {
  if (!existsSync(file)) throw new Error(`${label} is missing: ${path.relative(REPO_ROOT, file)}`);
  return JSON.parse(readFileSync(file, 'utf8'));
}

function assertScriptContains(file, symbol) {
  const fullPath = path.join(REPO_ROOT, file);
  if (!existsSync(fullPath)) throw new Error(`${file} is missing`);
  if (!readFileSync(fullPath, 'utf8').includes(symbol)) throw new Error(`${file} does not contain ${symbol}`);
}

function check() {
  const manifest = loadJson(MANIFEST_PATH, 'public surface manifest');
  const matrix = loadJson(MATRIX_PATH, 'website matrix evidence');
  if (!Array.isArray(manifest.claim_channels) || manifest.claim_channels.length < 9) {
    throw new Error('public surface manifest must declare at least 9 claim channels');
  }
  if (!Array.isArray(manifest.html) || manifest.html.length === 0) {
    throw new Error('public surface manifest has no HTML pages');
  }
  if (matrix.schema_version !== 1 || matrix.matrix_id !== 'osl-public-website-responsive-screenshots') {
    throw new Error('website matrix evidence has the wrong schema identity');
  }
  for (const mode of REQUIRED_MODES) {
    if (!matrix.modes?.includes(mode)) throw new Error(`website matrix is missing mode ${mode}`);
  }
  for (const width of REQUIRED_WIDTHS) {
    if (!matrix.widths?.includes(width)) throw new Error(`website matrix is missing width ${width}`);
  }
  if (!Array.isArray(matrix.captures)) throw new Error('website matrix captures must be an array');

  const expected = new Set();
  for (const htmlPath of manifest.html) {
    const route = routeFromHtmlPath(htmlPath);
    for (const mode of REQUIRED_MODES) {
      for (const width of REQUIRED_WIDTHS) expected.add(`${route}|${mode}|${width}`);
    }
  }

  const seen = new Set();
  for (const capture of matrix.captures) {
    const key = `${capture.page}|${capture.mode}|${capture.viewport_width}`;
    if (!expected.has(key)) throw new Error(`unexpected matrix capture ${key}`);
    if (seen.has(key)) throw new Error(`duplicate matrix capture ${key}`);
    seen.add(key);
    if (capture.viewport_height !== 900) throw new Error(`capture ${key} has wrong viewport height`);
    if (capture.mode === 'js-off' && capture.javascript !== false) throw new Error(`capture ${key} does not prove JS-off mode`);
    if (capture.mode === 'reduced-motion' && capture.reduced_motion !== true) {
      throw new Error(`capture ${key} does not prove reduced-motion mode`);
    }
    if (capture.mode === 'js-on' && capture.javascript !== true) throw new Error(`capture ${key} does not prove JS-on mode`);
    if (capture.mode !== 'js-off' && capture.visible_text_length < 20) {
      throw new Error(`capture ${key} did not render meaningful text`);
    }
    const screenshotPath = path.join(REPO_ROOT, capture.screenshot || '');
    if (!capture.screenshot?.startsWith('docs/evidence/website-matrix/screenshots/')) {
      throw new Error(`capture ${key} stores screenshot outside the website matrix evidence directory`);
    }
    if (!existsSync(screenshotPath)) throw new Error(`capture screenshot is missing for ${key}`);
    const facts = pngFacts(readFileSync(screenshotPath));
    if (facts.sha256 !== capture.screenshot_sha256) throw new Error(`capture screenshot hash mismatch for ${key}`);
    if (facts.width !== capture.screenshot_width || facts.height !== capture.screenshot_height) {
      throw new Error(`capture screenshot dimensions mismatch for ${key}`);
    }
    if (facts.width < capture.viewport_width || facts.height < capture.viewport_height || facts.bytes < 500) {
      throw new Error(`capture screenshot is too small for ${key}`);
    }
  }
  const missing = [...expected].filter((key) => !seen.has(key));
  if (missing.length > 0) throw new Error(`website matrix is missing ${missing.length} captures: ${missing.slice(0, 8).join(', ')}`);

  assertScriptContains('scripts/check-a11y.mjs', 'function auditPage');
  assertScriptContains('scripts/screenshot-matrix.mjs', 'async function captureCombo');
  assertScriptContains('scripts/check-claims.mjs', 'claim_channels');

  console.log('\nbuild-status summary');
  console.log(`  public pages      : ${manifest.html.length}`);
  console.log(`  claim channels    : ${manifest.claim_channels.length}`);
  console.log(`  matrix captures   : ${matrix.captures.length}`);
  console.log(`  modes             : ${REQUIRED_MODES.join(', ')}`);
  console.log(`  widths            : ${REQUIRED_WIDTHS.join(', ')}`);
  console.log('\nbuild-status: complete.');
}

if (process.argv.includes('--check')) {
  try {
    check();
  } catch (error) {
    console.error(`build-status: fatal error: ${error.stack || error.message}`);
    process.exit(1);
  }
} else {
  console.log('usage: node scripts/build-status.mjs --check');
}
