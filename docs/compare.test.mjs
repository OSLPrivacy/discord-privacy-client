import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const page = readFileSync(path.join(root, 'docs/compare.html'), 'utf8');
const manifest = JSON.parse(readFileSync(path.join(root, 'data/public-surface-manifest.json'), 'utf8'));

test('T11-T27 publishes a bounded comparison surface', () => {
  assert.ok(manifest.html.includes('docs/compare.html'));
  assert.match(page, /not a scan of your device/i);
  assert.match(page, /never shows a live protection score/i);
});
