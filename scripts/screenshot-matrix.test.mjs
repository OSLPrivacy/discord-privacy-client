import assert from 'node:assert/strict';
import test from 'node:test';
import {
  deterministicPageScript,
  stabilizeCaptureExpression,
  MODES,
  WIDTHS,
} from './screenshot-matrix.mjs';

test('matrix covers the exact responsive, motion, JavaScript, and zoom gate', () => {
  assert.deepEqual(WIDTHS, [320, 360, 390, 768, 1024, 1440]);
  assert.deepEqual(MODES.map((mode) => mode.id), ['js-on', 'js-off', 'reduced-motion', 'zoom-200']);
  assert.equal(MODES.find((mode) => mode.id === 'zoom-200')?.pageScaleFactor, 2);
});

test('deterministicPageScript pins default Date construction and Date.now', () => {
  const source = deterministicPageScript();

  assert.match(source, /2026-01-01T12:00:00Z/);
  assert.match(source, /static now\(\) \{ return fixedNow; \}/);
  assert.ok(source.includes('super(...(args.length ? args : [fixedNow]));'));
  assert.match(source, /Date = MatrixDate;/);
});

test('stabilizeCaptureExpression preserves CSP and starts at scroll origin', () => {
  const source = stabilizeCaptureExpression();

  assert.match(source, /window\.scrollTo\(0, 0\)/);
  assert.doesNotMatch(source, /createElement\('style'\)/);
});
