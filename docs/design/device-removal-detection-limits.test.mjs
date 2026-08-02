import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const document = readFileSync(
  fileURLToPath(new URL('./device-removal-detection-limits.md', import.meta.url)),
  'utf8',
);

assert.match(document, /Status: \*\*not measured on a Windows VM\*\*/);
assert.match(document, /Removal is not detectable while the VM is suspended or hibernated/);
assert.match(document, /Surprise-yank delivery latency is unmeasured/);
assert.match(document, /Do not show a latency number, an “instant” claim/);
assert.match(document, /TD-2 is complete only when this document is updated with those raw artifacts/);
