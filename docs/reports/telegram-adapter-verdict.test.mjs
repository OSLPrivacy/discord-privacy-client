import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const reportPath = new URL('./telegram-adapter-verdict.md', import.meta.url);
const matrixPath = new URL('../status/support-matrix.json', import.meta.url);

test('Telegram verdict keeps the public block while the recorded measurement is a login wall', async () => {
  const [report, matrixText] = await Promise.all([
    readFile(reportPath, 'utf8'),
    readFile(matrixPath, 'utf8'),
  ]);
  const matrix = JSON.parse(matrixText);
  const telegram = matrix.versioned_public_support_matrix.entries.find(
    (adapter) => adapter.id === 'telegram_desktop_public',
  );

  assert.ok(telegram, 'support matrix must contain the Telegram adapter');
  assert.equal(telegram.public_status, 'externally_blocked');
  assert.equal(telegram.evidence, 'docs/reports/telegram-adapter-verdict.md#TelegramSupportVerdict');
  assert.match(report, /The earlier probe reached\s+only a login surface/i);
  assert.match(report, /at least 6\.8\.3/i);
  assert.match(report, /remains `externally blocked`/i);
  assert.match(report, /at least two\s+stable candidate rows and at least two text-exposed rows/i);
});
