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
const PRICING_PATH = path.join(REPO_ROOT, 'data', 'pricing.json');
const REQUIRED_MODES = ['js-on', 'js-off', 'reduced-motion'];
const REQUIRED_WIDTHS = [320, 390, 768, 1280];
const CONNECTOR_SUPPORT_LABELS = new Set(['Available', 'Beta', 'Planned', 'Externally blocked', 'Not applicable']);
const CONNECTOR_STATUS_LABELS = new Set(['Available', 'Beta', 'Coming soon', 'Experimental', 'Externally blocked']);
const REQUIRED_CONNECTOR_FIELDS = [
  'name',
  'role',
  'protected_send',
  'protected_receive',
  'attachments',
  'scrub',
  'verified_on',
  'status',
  'provider_policy_risk',
];

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

function requiredText(row, field, label) {
  const value = row?.[field];
  if (typeof value !== 'string' || value.trim() === '') {
    throw new Error(`build-status: ${label}.${field} is required`);
  }
  return value.trim();
}

function validateConnectorMatrix(pricing) {
  const matrix = pricing.connector_matrix ?? {};
  const version = typeof matrix.version === 'string' ? matrix.version.trim() : '';
  if (!/^\d{4}-\d{2}-\d{2}$/.test(version)) {
    throw new Error('build-status: connector_matrix.version must be an exact ISO date');
  }
  if (!Array.isArray(matrix.sources) || matrix.sources.length === 0) {
    throw new Error('build-status: connector_matrix.sources must contain connector source evidence');
  }
  if (!Array.isArray(matrix.connectors) || matrix.connectors.length === 0) {
    throw new Error('build-status: connector_matrix.connectors must contain connector evidence');
  }

  const sourceIds = new Set();
  for (const [index, source] of matrix.sources.entries()) {
    const label = `connector_matrix.sources[${index}]`;
    if (!source || typeof source !== 'object' || Array.isArray(source)) {
      throw new Error(`build-status: ${label} must be an object`);
    }
    const id = requiredText(source, 'id', label);
    if (sourceIds.has(id)) throw new Error(`build-status: ${label}.id duplicates another source`);
    sourceIds.add(id);
    if (!/^https:\/\//.test(requiredText(source, 'url', label))) {
      throw new Error(`build-status: ${label}.url must be HTTPS`);
    }
    requiredText(source, 'publisher', label);
    requiredText(source, 'source_type', label);
    requiredText(source, 'title', label);
    const accessedOn = requiredText(source, 'accessed_on', label);
    if (accessedOn !== version) {
      throw new Error(`build-status: ${label}.accessed_on must match connector_matrix.version`);
    }
  }

  const connectorNames = new Set();
  for (const [index, connector] of matrix.connectors.entries()) {
    const label = `connector_matrix.connectors[${index}]`;
    if (!connector || typeof connector !== 'object' || Array.isArray(connector)) {
      throw new Error(`build-status: ${label} must be an object`);
    }
    for (const field of REQUIRED_CONNECTOR_FIELDS) requiredText(connector, field, label);
    if (connectorNames.has(connector.name)) throw new Error(`build-status: ${label}.name duplicates another connector`);
    connectorNames.add(connector.name);
    if (connector.verified_on !== version) {
      throw new Error(`build-status: ${label}.verified_on must match connector_matrix.version`);
    }
    for (const field of ['protected_send', 'protected_receive', 'attachments', 'scrub']) {
      if (!CONNECTOR_SUPPORT_LABELS.has(connector[field])) {
        throw new Error(`build-status: ${label}.${field} has an unsupported public label`);
      }
    }
    if (!CONNECTOR_STATUS_LABELS.has(connector.status)) {
      throw new Error(`build-status: ${label}.status has an unsupported public label`);
    }
    if (/\b\d+(\.\d+)?\s*%/.test(connector.provider_policy_risk)) {
      throw new Error(`build-status: ${label}.provider_policy_risk must not contain a percentage`);
    }
    if (!Array.isArray(connector.source_ids) || connector.source_ids.length === 0) {
      throw new Error(`build-status: ${label}.source_ids must cite source evidence`);
    }
    const rowSourceIds = new Set();
    for (const [sourceIndex, sourceId] of connector.source_ids.entries()) {
      if (typeof sourceId !== 'string' || sourceId.trim() === '') {
        throw new Error(`build-status: ${label}.source_ids[${sourceIndex}] must be nonempty`);
      }
      if (rowSourceIds.has(sourceId)) {
        throw new Error(`build-status: ${label}.source_ids[${sourceIndex}] duplicates another source on the connector`);
      }
      rowSourceIds.add(sourceId);
      if (!sourceIds.has(sourceId)) {
        throw new Error(`build-status: ${label}.source_ids[${sourceIndex}] does not resolve to connector_matrix.sources`);
      }
    }
  }

  return { version, connectors: matrix.connectors.length, sources: matrix.sources.length };
}

function check() {
  const manifest = loadJson(MANIFEST_PATH, 'public surface manifest');
  const matrix = loadJson(MATRIX_PATH, 'website matrix evidence');
  const connectorMatrix = validateConnectorMatrix(loadJson(PRICING_PATH, 'pricing manifest'));
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
  console.log(`  connectors        : ${connectorMatrix.connectors}`);
  console.log(`  connector sources : ${connectorMatrix.sources}`);
  console.log(`  connector version : ${connectorMatrix.version}`);
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
