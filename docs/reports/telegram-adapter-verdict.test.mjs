import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const reportPath = new URL('./telegram-adapter-verdict.md', import.meta.url);
const matrixPath = new URL('../status/support-matrix.json', import.meta.url);

// The old name of this test was "…while the recorded measurement is a login
// wall". That stopped being true of the document on 2026-08-04, when the row
// measurement was actually taken and recorded here — the test kept passing
// only because the sentence it matched had become the HISTORICAL one. The name
// is corrected, and no assertion was dropped: all four `report` matches and all
// three matrix assertions the old test carried are below, unchanged.
test("Telegram keeps the public block, and the block is scoped to OSL's release gate", async () => {
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

test('the block names OSL, not a fault in Telegram Desktop', async () => {
  const [report, matrixText] = await Promise.all([
    readFile(reportPath, 'utf8'),
    readFile(matrixPath, 'utf8'),
  ]);
  const matrix = JSON.parse(matrixText);

  // Every place the status is written must carry the scope, not just one of
  // them. That is exactly the failure being corrected: the status was written
  // to three rows and the reason recorded beside it was true of none of them.
  const rows = [
    matrix.versioned_public_support_matrix.entries.find((row) => row.id === 'telegram_desktop_public'),
    matrix.versioned_public_support_matrix.rows.find((row) => row.id === 'telegram_desktop_public'),
    matrix.conditional_app_evidence.find((row) => row.id === 'telegram_desktop_native'),
  ];
  assert.equal(rows.filter(Boolean).length, 3, 'all three Telegram rows must exist');

  for (const row of rows) {
    assert.equal(row.blocking_scope, 'osl_release_gate');
    assert.match(row.blocking_authority, /This status is OSL's, not Telegram's/);
    // The 2026-08-04 measurement must be PRESENT here, not only in the report.
    // It reached the report and propagated nowhere, which is what let the
    // matrix go on stating a condition that had already been met.
    assert.equal(row.client_accessibility_measurement.taken_utc, '2026-08-04');
    assert.equal(row.client_accessibility_measurement.candidate_rows, 109);
    assert.equal(row.client_accessibility_measurement.text_exposed_rows, 109);
    assert.equal(row.client_accessibility_measurement.threshold, 2);
    // …and it must not be readable as a promotion.
    assert.match(row.client_accessibility_measurement.does_not_establish, /NeverProvenLive/);
  }

  assert.match(report, /It is a statement about OSL's release gate/);
  assert.match(report, /never was, a finding that Telegram\s+Desktop's client blocks accessibility/);
});

test('the re-scoping promotes nothing: no Telegram row may claim capability', async () => {
  const matrix = JSON.parse(await readFile(matrixPath, 'utf8'));
  const rows = [
    matrix.versioned_public_support_matrix.entries.find((row) => row.id === 'telegram_desktop_public'),
    matrix.versioned_public_support_matrix.rows.find((row) => row.id === 'telegram_desktop_public'),
    matrix.conditional_app_evidence.find((row) => row.id === 'telegram_desktop_native'),
  ];

  // The floor. `check-app-claims.mjs` permits a bare public support claim only
  // for `supported` or `verified_live`; this asserts the Telegram rows stay
  // outside that set from the matrix's own side, so a row edited to claim
  // capability goes red here as well as there.
  for (const row of rows) {
    assert.ok(!['supported', 'verified_live'].includes(row.public_status ?? row.status));
    assert.notEqual(row.claim_allowed, true);
    assert.notEqual(row.public_claim_allowed, true);
  }
  const publicRow = rows[1];
  assert.equal(publicRow.public_label, 'Externally blocked');
  assert.equal(publicRow.status, 'externally_blocked');
});
