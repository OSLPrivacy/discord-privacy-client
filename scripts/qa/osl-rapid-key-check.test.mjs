import assert from 'node:assert/strict';
import test from 'node:test';

import { validateReport } from './osl-rapid-key-check.mjs';

function validReport() {
  return {
    schema: 'osl-rapid-key-check-v1',
    runMark: 'TASK3547-UNIT',
    keyPressesPerBox: 100,
    shortcutRepeats: 20,
    boxCount: 1,
    exactBoxMatchCount: 1,
    shortcutCount: 2,
    unexpectedProcessStops: 0,
    boxes: [
      { id: 'title', tag: 'input', expected: 'x'.repeat(100), value: 'x'.repeat(100), exact: true },
    ],
    shortcuts: [
      { id: 'save', label: 'Ctrl+S save', repeats: 20, enabled: true, count: 1 },
      { id: 'blocked-admin', label: 'Ctrl+B blocked admin', repeats: 20, enabled: false, count: 0 },
    ],
  };
}

test('rapid key report validator accepts the measured finish-line shape', () => {
  const report = validReport();
  assert.equal(validateReport(report), report);
});

test('rapid key report validator rejects a vacuous fixed screen', () => {
  const report = { ...validReport(), boxCount: 0, exactBoxMatchCount: 0, boxes: [] };
  assert.throws(() => validateReport(report), /box count must be above 0/);
});

test('rapid key report validator rejects repeated shortcut actions above one', () => {
  const report = validReport();
  report.shortcuts = [{ ...report.shortcuts[0], count: 2 }, report.shortcuts[1]];
  assert.throws(() => validateReport(report), /every action count must be 0 or 1/);
});

test('rapid key report validator rejects stopped processes', () => {
  const report = { ...validReport(), unexpectedProcessStops: 1 };
  assert.throws(() => validateReport(report), /no process may stop before cleanup/);
});
