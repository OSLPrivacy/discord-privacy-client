import assert from 'node:assert/strict';
import test from 'node:test';
import { DETAILED_SOURCES, validateDetailedSources } from './check-detailed-source-index.mjs';

test('detailed source index includes every required safety, ship, adapter, security, lifecycle, and Scrub source', () => {
  assert.equal(DETAILED_SOURCES.length, 12);
  assert.deepEqual(validateDetailedSources(), []);
});
