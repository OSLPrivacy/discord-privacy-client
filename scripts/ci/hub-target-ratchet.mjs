#!/usr/bin/env node
// D-172. The apps/osl-hub non-shipping-target ratchet.
//
// Gate (b) in integration-gate.yml is
//   cargo check --manifest-path apps/osl-hub/Cargo.toml --features desktop
// which is the exact command START-HERE.md section 5 and scripts/verify-all.sh
// call the shipping build, and the one D-146 was about. It is a hard gate: it
// must be green.
//
// Adding `--all-targets` to it is a STRICTLY WIDER check, and the first time it
// ever ran (run 30928896726) it found something nothing in this repository had
// ever compiled:
//
//   error[E0308]: mismatched types
//        --> tests\web_surface_a11y_spike.rs:134:60
//        expected `HWND`, found `windows::Win32::Foundation::HWND`
//        note: there are multiple different versions of crate `windows` in the
//              dependency graph  (windows 0.56.0 and windows 0.61.3)
//
// rust-test.yml's `hub-desktop-bin` job runs
// `cargo test --features desktop --bin osl-privacy-hub`, i.e. the BIN target
// only, so it never compiles this test. Nothing else touches apps/osl-hub at
// all, because it is excluded from the workspace.
//
// That is not this lane's defect to fix, and it must not become a reason to
// drop `--all-targets` and pretend the wider check was never run. So the wider
// check runs here, and this ratchet pins the SET of targets that fail to
// compile. It fails if a target that is not on the list breaks, and it fails if
// a listed target starts compiling without being removed from the list in the
// same change. The list can only shrink.
//
// Usage:
//   node scripts/ci/hub-target-ratchet.mjs             # run cargo and grade
//   node scripts/ci/hub-target-ratchet.mjs --self-test # prove it fails

import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..', '..');

// Every entry is an OPEN DEFECT with a stated cause, not an excuse. Removing an
// entry requires the target to compile; adding one requires a reviewer to argue
// that a target of the shipping app is allowed not to build.
const KNOWN_BROKEN_TARGETS = [
  {
    target: 'test "web_surface_a11y_spike"',
    since: '2026-08-04, first time apps/osl-hub was ever built with --all-targets',
    cause:
      'tests/web_surface_a11y_spike.rs:134 passes an HWND from windows 0.61.3 to a function '
      + 'expecting the HWND of windows 0.56.0. Two major versions of the `windows` crate are in '
      + 'the dependency graph. Windows-only, test-target-only; the shipping binary is unaffected, '
      + 'which is why gate (b) is green and this is a separate ratchet.',
  },
];

const CARGO_ARGS = [
  'check',
  '--manifest-path', 'apps/osl-hub/Cargo.toml',
  '--features', 'desktop',
  '--all-targets',
  '--locked',
  '--message-format', 'short',
];

// cargo prints: error: could not compile `osl-hub` (test "name") due to N errors
const FAILED_TARGET_RE = /could not compile `[^`]+` \(([^)]*)\)/g;

export function parseFailedTargets(output) {
  const found = new Set();
  for (const match of output.matchAll(FAILED_TARGET_RE)) {
    // Drop cargo's trailing " due to 1 previous error" if it lands inside the
    // parens on some versions, and normalise whitespace.
    found.add(match[1].replace(/\s+/g, ' ').trim());
  }
  return [...found].sort();
}

export function grade(measured, expected) {
  const measuredSet = new Set(measured);
  const expectedSet = new Set(expected);
  const unexpected = measured.filter((t) => !expectedSet.has(t));
  const stale = expected.filter((t) => !measuredSet.has(t));
  return { unexpected, stale, ok: unexpected.length === 0 && stale.length === 0 };
}

function selfTest() {
  let bad = 0;
  const ok = (name, cond) => {
    if (cond) process.stdout.write(`  self-test ok: ${name}\n`);
    else { console.error(`  self-test FAILED: ${name}`); bad = 1; }
  };

  const sample = [
    'error[E0308]: mismatched types',
    'error: could not compile `osl-hub` (test "web_surface_a11y_spike") due to 1 previous error',
  ].join('\n');
  ok('parses the failing target out of real cargo output',
    JSON.stringify(parseFailedTargets(sample)) === JSON.stringify(['test "web_surface_a11y_spike"']));

  ok('output with no failure parses to an empty set',
    parseFailedTargets('Finished dev profile').length === 0);

  const known = KNOWN_BROKEN_TARGETS.map((e) => e.target);
  ok('the known set is accepted', grade(known, known).ok);
  ok('a NEW broken target is rejected',
    grade([...known, 'lib'], known).unexpected.length === 1);
  ok('a target that started compiling is rejected as stale',
    grade([], known).stale.length === 1);
  ok('a swap in both directions is rejected',
    !grade(['bin "osl-privacy-hub"'], known).ok);

  for (const entry of KNOWN_BROKEN_TARGETS) {
    ok(`entry ${entry.target} states a cause and a date`,
      Boolean(entry.cause) && Boolean(entry.since));
  }

  process.stdout.write(bad ? '\nhub target ratchet self-test: FAILED\n' : '\nhub target ratchet self-test: ok\n');
  return bad;
}

if (process.argv.includes('--self-test')) {
  process.exit(selfTest());
}

process.stdout.write(`hub target ratchet -- grading ${REPO}\n`);
process.stdout.write(`  cargo ${CARGO_ARGS.join(' ')}\n`);
const proc = spawnSync('cargo', CARGO_ARGS, {
  cwd: REPO,
  encoding: 'utf8',
  maxBuffer: 128 * 1024 * 1024,
  shell: false,
});
const out = `${proc.stdout ?? ''}${proc.stderr ?? ''}`.replace(/\[[0-9;]*[A-Za-z]/g, '');
process.stdout.write(out);

if (proc.error) {
  console.error(`  FAIL: could not run cargo: ${proc.error.message}`);
  process.exit(1);
}

const measured = parseFailedTargets(out);
const expected = KNOWN_BROKEN_TARGETS.map((e) => e.target).sort();

// A cargo invocation that died before it compiled anything -- a bad manifest, a
// missing toolchain, a network failure fetching the index -- produces no
// "could not compile" lines and would grade as "nothing new is broken". Refuse
// it: a non-zero exit with an empty measured set means the check did not run.
if (proc.status !== 0 && measured.length === 0) {
  console.error(
    `  FAIL: cargo exited ${proc.status} but named no failing target. The check did not run; `
    + 'refusing to read that as "only the known targets are broken".');
  process.exit(1);
}
if (proc.status === 0 && expected.length > 0) {
  console.error(
    `  FAIL: cargo exited 0 while ${expected.length} target(s) are still listed as broken. `
    + 'Empty KNOWN_BROKEN_TARGETS in the same change that fixed them.');
  process.exit(1);
}

const verdict = grade(measured, expected);
process.stdout.write(`\n  measured: ${JSON.stringify(measured)}\n  expected: ${JSON.stringify(expected)}\n`);
for (const t of verdict.unexpected) {
  console.error(`  FAIL: ${t} no longer compiles and is not on the known-broken list. `
    + 'A target of the shipping app stopped building.');
}
for (const t of verdict.stale) {
  console.error(`  FAIL: ${t} compiles now. Remove it from KNOWN_BROKEN_TARGETS in the same change `
    + 'that fixed it -- a list that is only ever added to is not a ratchet.');
}
process.stdout.write(`\n=== HUB TARGET RATCHET: ${verdict.ok ? 'HELD' : 'FAILED'} ===\n`);
process.exit(verdict.ok ? 0 : 1);
