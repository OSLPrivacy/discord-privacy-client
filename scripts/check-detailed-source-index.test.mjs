import assert from 'node:assert/strict';
import test from 'node:test';
import { DETAILED_SOURCES, RESUME_HERE_FIELDS, validateDetailedSources } from './check-detailed-source-index.mjs';

test('detailed source index includes every required safety, ship, adapter, security, lifecycle, and Scrub source', () => {
  assert.equal(DETAILED_SOURCES.length, 12);
  assert.deepEqual(validateDetailedSources(), []);
});

test('every detailed source ends with the required Resume here handoff block', () => {
  assert.deepEqual(RESUME_HERE_FIELDS, [
    'Resume here', 'Current verified state:', 'Exact build/worktree:',
    'Current owner and exclusive files:', 'Next unblocked action:',
    'Command/scenario:', 'Expected result:', 'Known blocker/risk:',
    'Master/internal-checklist rows to update on completion:',
  ]);
});
