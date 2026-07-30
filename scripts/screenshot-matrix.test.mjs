import assert from 'node:assert/strict';
import test from 'node:test';
import {
  deterministicPageScript,
  stabilizeCaptureExpression,
} from './screenshot-matrix.mjs';

test('deterministicPageScript pins default Date construction and Date.now', () => {
  const source = deterministicPageScript();

  assert.match(source, /2026-01-01T12:00:00Z/);
  assert.match(source, /static now\(\) \{ return fixedNow; \}/);
  assert.ok(source.includes('super(...(args.length ? args : [fixedNow]));'));
  assert.match(source, /Date = MatrixDate;/);
});

test('stabilizeCaptureExpression disables motion-sensitive capture variance', () => {
  const source = stabilizeCaptureExpression();

  assert.match(source, /data-screenshot-matrix-stability/);
  assert.match(source, /animation-duration:0s!important;/);
  assert.match(source, /transition-duration:0s!important;/);
  assert.match(source, /scroll-behavior:auto!important;/);
  assert.match(source, /caret-color:transparent!important;/);
});
