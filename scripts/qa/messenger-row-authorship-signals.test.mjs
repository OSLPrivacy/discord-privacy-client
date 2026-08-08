import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const repoRoot = path.dirname(path.dirname(path.dirname(fileURLToPath(import.meta.url))));
const probeScript = path.join(repoRoot, 'scripts', 'qa', 'messenger-row-authorship-signals.mjs');

function runProbe(args, { expectCode = 0 } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [probeScript, ...args], {
      cwd: repoRoot,
      env: process.env,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => { stdout += chunk.toString('utf8'); });
    child.stderr.on('data', (chunk) => { stderr += chunk.toString('utf8'); });
    child.once('error', reject);
    child.once('exit', (code) => {
      if (code !== expectCode) {
        reject(new Error(`expected exit ${expectCode}, got ${code}\nstdout:\n${stdout}\nstderr:\n${stderr}`));
      } else {
        resolve({ stdout, stderr, code });
      }
    });
  });
}

test('TASK 4087 measures seven Messenger authorship surfaces and names a language-stable winner', async () => {
  const { stdout } = await runProbe(['--task4087-fixtures', '--read-date', '2026-08-07']);
  const lines = stdout.trim().split('\n');
  assert.equal(lines[0], 'TASK4087_READ_DATE=2026-08-07');
  assert.match(stdout, /TASK4087_LANGUAGE_COUNT=2/);
  assert.match(stdout, /TASK4087_WINNER_LANGUAGE locale=en-US row_link_account_id=100010001 panel_account_id=100010001 own_matches=2\/2 peer_differs=2\/2/);
  assert.match(stdout, /TASK4087_WINNER_LANGUAGE locale=es-ES row_link_account_id=100010001 panel_account_id=100010001 own_matches=2\/2 peer_differs=2\/2/);
  assert.match(stdout, /TASK4087_WORDING_CHANGES_BY_LANGUAGE=true values="en-US:You sent,es-ES:Enviaste"/);
  assert.match(stdout, /TASK4087_WINNER place=account_link signal=profile_href_account_id_cross_check/);
  assert.match(stdout, /ladder_kind=verified_sender_address_match ladder_rank=2 strength=strong/);
  assert.match(stdout, /TASK4087_PLACES_CHECKED=7/);
  assert.match(stdout, /TASK4087_PLACES_LEFT_UNCHECKED=0/);
  assert.match(stdout, /TASK4087_UNMEASURED_CLAIMS=0/);

  const placeLines = lines.filter((line) => line.startsWith('TASK4087_PLACE_RESULT '));
  assert.deepEqual(
    placeLines.map((line) => line.match(/place=([^ ]+)/)?.[1]),
    [
      'wording',
      'test_identifier',
      'parent_boxes',
      'roles_states',
      'picture_address',
      'account_link',
      'screen_reader',
    ],
  );
});
