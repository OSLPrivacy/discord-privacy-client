import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const SCRIPT = 'scripts/qa/page-row-author-probe.mjs';

function run(args) {
  return spawnSync(process.execPath, [SCRIPT, ...args], {
    cwd: new URL('../..', import.meta.url),
    encoding: 'utf8',
  });
}

test('task4084 fixture probe prints Discord count first and twenty Messenger row lines', () => {
  const result = run(['--task4084-fixtures']);
  assert.equal(result.status, 0, result.stderr);
  const lines = result.stdout.trim().split(/\r?\n/);

  assert.equal(lines[0], 'TASK4084_DISCORD_WHO_WROTE_FOUND=10');
  assert.equal(lines.filter((line) => line.startsWith('TASK4084_MESSENGER_DIRECT_ROW ')).length, 10);
  assert.equal(lines.filter((line) => line.startsWith('TASK4084_MESSENGER_GROUP_ROW ')).length, 10);
  assert.match(result.stdout, /TASK4084_MESSENGER_DIRECT_SUMMARY .*read_date=2026-08-07 .*no_line_rows=0/);
  assert.match(result.stdout, /TASK4084_MESSENGER_GROUP_SUMMARY .*read_date=2026-08-07 .*no_line_rows=0/);
  assert.match(result.stdout, /wording="You sent"/);
  assert.match(result.stdout, /wording="Kai Rivers sent"/);
  assert.doesNotMatch(result.stdout, /picture_address=none/);
  assert.doesNotMatch(result.stdout, /screen_reader=none/);
});

test('task4084 signed-out fixture exits 1 instead of reporting an empty finding', () => {
  const result = run([
    '--fixture',
    'messenger-signed-out',
    '--service',
    'messenger',
    '--label',
    'messenger-signed-out',
    '--expected-rows',
    '10',
  ]);
  assert.equal(result.status, 1);
  assert.equal(result.stdout, '');
  assert.match(result.stderr, /signed-out page refused: messenger-signed-out/);
});
