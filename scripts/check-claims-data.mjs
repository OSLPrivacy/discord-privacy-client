#!/usr/bin/env node

// Fail-closed validation for structured public data.  Public JSON/YAML is a
// claim channel just as much as visible copy: an OSL capability may only be
// asserted when the capability registry has earned an eligible status.

import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPTS_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.dirname(SCRIPTS_DIR);
const MANIFEST_PATH = path.join(REPO_ROOT, 'data', 'public-surface-manifest.json');
const PRICING_PATH = path.join(REPO_ROOT, 'data', 'pricing.json');
const STRUCTURED_EXTENSIONS = new Set(['.json', '.yaml', '.yml']);
const ELIGIBLE_STATUSES = new Set(['Available', 'Beta']);

function isObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function list(value) {
  return Array.isArray(value) ? value : [];
}

function decodePointer(pointer) {
  if (pointer === '') return [];
  if (typeof pointer !== 'string' || !pointer.startsWith('/')) return null;
  return pointer.slice(1).split('/').map((part) => part.replace(/~1/g, '/').replace(/~0/g, '~'));
}

function atPointer(value, pointer) {
  const parts = decodePointer(pointer);
  if (!parts) return undefined;
  return parts.reduce((current, part) => (current !== null && typeof current === 'object' ? current[part] : undefined), value);
}

function safeRepoPath(entry) {
  return typeof entry === 'string' && !path.isAbsolute(entry) && !entry.includes('..');
}

function readJson(file) {
  return JSON.parse(readFileSync(file, 'utf8'));
}

function valuesEqual(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function numericFields(value, names, found = []) {
  if (Array.isArray(value)) {
    for (const item of value) numericFields(item, names, found);
  } else if (isObject(value)) {
    for (const [key, item] of Object.entries(value)) {
      if (names.has(key) && typeof item === 'number' && Number.isFinite(item)) found.push(key);
      numericFields(item, names, found);
    }
  }
  return found;
}

function validate(pricing, manifest, readPublicFile) {
  const errors = [];
  const add = (code, message) => errors.push(`CAPABILITY_DATA_${code}: ${message}`);
  const bindingsConfig = pricing.capability_data_bindings;
  if (!isObject(bindingsConfig) || bindingsConfig.schema_version !== 1) {
    add('SCHEMA', 'pricing.capability_data_bindings must have schema_version 1');
    return errors;
  }

  const bindings = list(bindingsConfig.bindings);
  const bindingByPath = new Map();
  for (const [index, binding] of bindings.entries()) {
    const label = binding?.id || `bindings[${index}]`;
    if (!isObject(binding) || typeof binding.id !== 'string' || !safeRepoPath(binding.path)) {
      add('BINDING_SCHEMA', `${label} needs a stable id and safe repo-relative path`);
      continue;
    }
    if (binding.format !== 'json' && binding.format !== 'yaml') add('BINDING_SCHEMA', `${label}.format must be json or yaml`);
    if (bindingByPath.has(binding.path)) add('BINDING_SCHEMA', `duplicate binding for ${binding.path}`);
    bindingByPath.set(binding.path, binding);
  }

  const registry = new Map(list(pricing.capability_registry).map((capability) => [capability?.id, capability]));
  for (const entry of list(manifest.assets)) {
    if (!safeRepoPath(entry)) {
      add('PUBLIC_PATH', `unsafe manifest asset ${String(entry)}`);
      continue;
    }
    const extension = path.extname(entry).toLowerCase();
    if (!STRUCTURED_EXTENSIONS.has(extension)) continue;
    const binding = bindingByPath.get(entry);
    if (!binding) {
      add('UNBOUND', `${entry} is public structured data with no capability_data_bindings entry`);
      continue;
    }
    if ((extension === '.json' && binding.format !== 'json') || (extension !== '.json' && binding.format !== 'yaml')) {
      add('FORMAT', `${entry} does not match binding format ${binding.format}`);
      continue;
    }
    if (binding.format === 'yaml') {
      // This repository intentionally has no YAML dependency. Refuse rather
      // than treating unparsed YAML capability assertions as harmless.
      add('YAML_UNSUPPORTED', `${entry} must be converted to JSON before it can make public capability claims`);
      continue;
    }

    let document;
    try {
      document = readPublicFile(entry);
    } catch (error) {
      add('PARSE', `${entry} is not valid JSON: ${error.message}`);
      continue;
    }
    const records = atPointer(document, binding.records_pointer);
    if (!Array.isArray(records)) {
      add('RECORDS', `${entry} records_pointer ${JSON.stringify(binding.records_pointer)} must resolve to an array`);
      continue;
    }
    const selector = binding.osl_record;
    if (!isObject(selector) || typeof selector.path !== 'string') {
      add('BINDING_SCHEMA', `${entry} needs osl_record.path and osl_record.equals`);
      continue;
    }
    const oslRows = records.filter((record) => isObject(record) && valuesEqual(atPointer(record, selector.path), selector.equals));
    if (oslRows.length !== 1) {
      add('OSL_ROW', `${entry} must contain exactly one OSL row selected by ${selector.path}`);
      continue;
    }
    const oslRow = oslRows[0];
    for (const assertion of list(binding.capability_assertions)) {
      const field = assertion?.path;
      const capability = registry.get(assertion?.capability_id);
      if (typeof field !== 'string' || !Array.isArray(assertion?.affirmative_values) || !capability) {
        add('BINDING_SCHEMA', `${entry} capability assertion needs path, affirmative_values, and a registry capability_id`);
        continue;
      }
      const value = atPointer(oslRow, field);
      if (assertion.affirmative_values.some((affirmative) => valuesEqual(affirmative, value))
        && !ELIGIBLE_STATUSES.has(capability.status)) {
        add('STATUS', `${entry} asserts ${field}=${JSON.stringify(value)} but ${capability.id} is ${capability.status}`);
      }
    }
    const scores = numericFields(oslRow, new Set(list(binding.numeric_score_fields)));
    if (scores.length > 0) add('NUMERIC_SCORE', `${entry} gives OSL a numeric score (${[...new Set(scores)].join(', ')})`);
  }
  return errors;
}

function runSelfTest() {
  const pricing = {
    capability_registry: [{ id: 'group-protection', status: 'Planned' }],
    capability_data_bindings: {
      schema_version: 1,
      bindings: [{
        id: 'fixture', path: 'assets/data/apps.json', format: 'json', records_pointer: '/apps',
        osl_record: { path: '/name', equals: 'OSL' },
        capability_assertions: [{ path: '/group_chat', capability_id: 'group-protection', affirmative_values: [true, 'partial'] }],
        numeric_score_fields: ['overall', 'privacy', 'features'],
      }],
    },
  };
  const manifest = { assets: ['assets/data/apps.json'] };
  const document = { apps: [{ name: 'OSL', group_chat: true }] };
  const red = validate(pricing, manifest, () => document).some((error) => error.startsWith('CAPABILITY_DATA_STATUS:'));
  console.log(`  ${red ? 'caught ' : 'MISSED '} Planned group_chat assertion`);
  pricing.capability_registry[0].status = 'Beta';
  const green = validate(pricing, manifest, () => document).length === 0;
  console.log(`  ${green ? 'passed ' : 'FAILED '} Beta group_chat assertion`);
  const scoreDocument = { apps: [{ name: 'OSL', group_chat: false, score: { overall: 90 } }] };
  const scoreRed = validate(pricing, manifest, () => scoreDocument).some((error) => error.startsWith('CAPABILITY_DATA_NUMERIC_SCORE:'));
  console.log(`  ${scoreRed ? 'caught ' : 'MISSED '} numeric OSL score`);
  if (!red || !green || !scoreRed) throw new Error('check-claims-data self-test failed');
}

function run() {
  if (process.argv.includes('--self-test')) {
    console.log('check-claims-data self-test:');
    runSelfTest();
    return;
  }
  const pricing = readJson(PRICING_PATH);
  const manifest = readJson(MANIFEST_PATH);
  const errors = validate(pricing, manifest, (entry) => readJson(path.join(REPO_ROOT, entry)));
  if (errors.length > 0) throw new Error(errors.join('\n'));
  console.log('check-claims-data: complete.');
}

try {
  run();
} catch (error) {
  console.error(`check-claims-data: fatal error: ${error.stack || error.message}`);
  process.exit(1);
}
