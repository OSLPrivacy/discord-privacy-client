import assert from 'node:assert/strict';
import test from 'node:test';
import { ALLOWLIST_BINDINGS, validateAllowlistSync } from './check-allowlist-sync.mjs';

const allowlist = '| Bilateral burn | `implemented-unwired`, plus an open defect | evidence | Planned. |';
const pricing = { capability_registry: [{ id: 'burn', status: 'Planned' }] };

test('a capability cannot become Available through a registry or copy-only edit', () => {
  const bindings = { burn: ALLOWLIST_BINDINGS.burn };
  assert.deepEqual(validateAllowlistSync(allowlist, pricing, bindings), []);
  assert.deepEqual(
    validateAllowlistSync(allowlist, { capability_registry: [{ id: 'burn', status: 'Available' }] }, bindings),
    ['burn is Available but allowlist "Bilateral burn" earns Planned'],
  );
  assert.deepEqual(Object.keys(bindings), ['burn']);
});
