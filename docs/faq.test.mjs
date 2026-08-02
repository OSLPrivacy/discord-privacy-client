import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sentence = /Signal may be the better choice/;

test('T11-T28 keeps the Signal point-away advice on FAQ and comparison pages', () => {
  for (const page of ['docs/faq.html', 'docs/compare.html']) {
    assert.match(readFileSync(path.join(root, page), 'utf8'), sentence, `${page} must retain the point-away advice`);
  }
});
