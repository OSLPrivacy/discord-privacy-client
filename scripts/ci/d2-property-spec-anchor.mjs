#!/usr/bin/env node
// D-281 — the terminal anchor for cipher-store-cf's release-contract gate.
//
// WHAT D-281 SAYS, AND WHY THIS FILE IS NOT IN cipher-store-cf/.
//
// D-262 gave the seven named security-property suites a gate: each must be
// registered exactly once as an ACTIVE `describe`, every named `it` must be
// active, and the load-bearing assertion text must still be in the body, plus a
// per-directory `*.test.ts` floor for the properties nobody named. That gate is
// anchored by running FIRST in `npm test`, and `package.json` is inside the
// release digest -- so dropping the step to silence it turns the release-source
// contract red.
//
// That is a real anchor and the lane that built it recorded, correctly, that it
// is not a complete one:
//
//     NO IN-SUITE GATE SURVIVES DELETION OF THE GATE ITSELF.
//
// Gutting `scripts/d2-test-closure.ts` AND deleting the specs escapes every
// mechanism inside the suite, because every mechanism inside the suite is
// configured by the suite. The ruling was that the terminal anchor belongs in
// CI: a required job that runs the named specs INDEPENDENTLY of the suite's own
// configuration.
//
// This is that job's grader, and three things about it are load-bearing:
//
//   1. IT CARRIES ITS OWN COPY OF THE NAMES. It does not read
//      `D2_PROPERTY_TEST_SUITES` to decide what to demand. A checker that reads
//      the file declaring its own inputs is satisfied by that declaration --
//      D-285 (a forbidden-verb census that scanned the constant listing the
//      verbs), D-275 (a pin census counting an identifier inside a string
//      literal). Deleting an entry from the in-suite list must not delete the
//      demand. The list below is a SECOND, deliberately redundant copy, and it
//      lives in scripts/ci/**, a different ownership domain from
//      cipher-store-cf/**, so one lane editing the Worker cannot quietly edit
//      both halves.
//
//      The dependency runs the other way, once: if the in-suite list gains a
//      suite this file does not name, this file FAILS as STALE. Growth is
//      caught; shrinkage cannot weaken it.
//
//   2. IT RUNS THE SPECS BY EXPLICIT PATH, not through `npm test`. `npm test` is
//      a string in `package.json`; running it would make this a second reader of
//      the same configuration D-281 says cannot anchor itself. Deleting a spec
//      file is then "No test files found" -- exit 1 -- rather than a suite that
//      quietly got smaller.
//
//   3. IT GRADES THE REPORT, NOT THE EXIT CODE. A `describe.skip` leaves vitest
//      exit 0. Every named test must be present in the JSON report with status
//      `passed`; `skipped`, `todo`, `pending` and absent are each a distinct,
//      named failure. And a report with zero tests is a structural FAIL --
//      "the anchor did not run" is not "the anchor passed".
//
// It also runs `scripts/d2-contract-gate.ts` directly, so a deleted or
// non-executable gate is red here even if `package.json`'s `test` script no
// longer mentions it.
//
// WHAT IT DOES NOT CLAIM. It cannot see a property nobody ever named -- that is
// what the per-directory floor inside the suite is for, and the floor is still
// in-suite. And it runs under cipher-store-cf's `vitest.config.ts`, because the
// specs need the workerd pool to exist at all; what is bypassed is the config's
// say over WHICH specs run, not its say over the runtime they run in.
//
// Usage:
//   node scripts/ci/d2-property-spec-anchor.mjs              # run and grade
//   node scripts/ci/d2-property-spec-anchor.mjs --self-test  # prove it can fail

import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const PROJECT = join(REPO_ROOT, 'cipher-store-cf');
const CLOSURE_SOURCE = join(PROJECT, 'scripts', 'd2-test-closure.ts');
const CONTRACT_GATE = join(PROJECT, 'scripts', 'd2-contract-gate.ts');

/**
 * The independent copy. `suite` is the `describe` title, `tests` the `it`
 * titles that must be present AND passing. A superset is allowed on purpose:
 * adding a case must never be a re-pin, while removing, renaming or skipping one
 * is red.
 */
export const ANCHORED_SPECS = [
  {
    file: 'test/blob-upload-existence-oracle.test.ts',
    defect: 'D-255',
    suite: 'D-255 blob upload is not an existence oracle',
    tests: [
      'answers a taken blob id exactly as it answers an unused one',
      'leaves the row it collided with exactly as it found it',
    ],
  },
  {
    file: 'test/blob-capacity-atomic.test.ts',
    defect: 'D-256',
    suite: 'D-256 the blob capacity gate admits atomically (HIGH-2)',
    tests: [
      'refuses the second of two uploads that race for the last byte',
      'still admits the one upload the headroom genuinely allows',
    ],
  },
  {
    file: 'test/blob-payload-no-overwrite.test.ts',
    defect: 'D-264',
    suite: 'D-264 a caller-named payload key is written only when it is free',
    tests: [
      'refuses to replace an existing payload under a digest the caller supplied',
      'still stores bytes under a digest no object occupies',
    ],
  },
  {
    file: 'test/blob-capability.test.ts',
    defect: 'D-257',
    suite: 'R2 capability blob route',
    tests: [
      'stores payload bytes only in R2 and burns only with manage_cap',
      'makes every GET failure the same 404',
    ],
  },
  {
    file: 'test/d81-fetch-carries-no-identity.test.ts',
    defect: 'D81',
    suite: 'D81 — a cipher-store fetch carries no identity',
    tests: [
      'serves a blob to a caller who presents the capability and nothing else',
      'writes no blob access receipt or fetcher identity',
      'refuses a capability presented in the URL instead of the header',
    ],
  },
  {
    file: 'test/d81-fetch-carries-no-identity.test.ts',
    defect: 'D81/t1-15',
    suite: 'D81 — a retired legacy row is treated as absent',
    tests: [
      'does not serve a retired row to a caller holding only its id',
      'answers a retired row exactly as it answers an id that was never stored',
      'does not let an id-only caller destroy a retired row',
      'rejects a tokenless upload, so no new capability-less index row can be created',
    ],
  },
  {
    file: 'test/harness-strictness.test.ts',
    defect: 'instrument',
    suite: 'the R2 double refuses what production refuses',
    tests: [
      'rejects a put body with no known length',
      'rejects a multipart part with no known length',
      'still accepts a known-length body, so the guard is not simply refusing everything',
      'honours onlyIf etagDoesNotMatch instead of silently overwriting',
    ],
  },
];

// ---------------------------------------------------------------------------
// grading — pure, so --self-test can drive every verdict without vitest
// ---------------------------------------------------------------------------

/** Suite titles named by the IN-SUITE list, used only to detect this file going stale. */
export function inSuiteSuiteTitles(closureSource) {
  const start = closureSource.indexOf('D2_PROPERTY_TEST_SUITES');
  if (start < 0) {
    throw new Error(
      'cipher-store-cf/scripts/d2-test-closure.ts no longer declares D2_PROPERTY_TEST_SUITES; ' +
        'the in-suite gate this anchor backstops has been removed or renamed.',
    );
  }
  const body = closureSource.slice(start);
  const end = body.indexOf('\n];');
  const region = end < 0 ? body : body.slice(0, end);
  const titles = [];
  for (const match of region.matchAll(/(?:^|[\s{,])suite:\s*(["'`])((?:\\.|(?!\1).)*)\1/g)) {
    titles.push(match[2]);
  }
  return titles;
}

/**
 * @param report parsed vitest --reporter=json output
 * @param specs  ANCHORED_SPECS (or a fixture, in the self-test)
 * @returns {{failures: string[], graded: number}}
 */
export function gradeReport(report, specs) {
  const failures = [];
  if (!report || !Array.isArray(report.testResults)) {
    return { failures: ['the vitest JSON report has no testResults array; the anchor did not run'], graded: 0 };
  }
  const observed = new Map();
  let total = 0;
  for (const file of report.testResults) {
    for (const assertion of file.assertionResults ?? []) {
      total += 1;
      const suite = (assertion.ancestorTitles ?? []).join(' › ');
      observed.set(`${suite} ${assertion.title}`, assertion.status);
    }
  }
  if (total === 0) {
    // A run that graded nothing is not a run that found nothing wrong. This is
    // the same refusal the hub lint ratchet makes when cargo exits non-zero with
    // an empty census.
    return { failures: ['the vitest JSON report contains ZERO tests; the anchor did not run'], graded: 0 };
  }

  let graded = 0;
  for (const spec of specs) {
    for (const title of spec.tests) {
      graded += 1;
      const status = observed.get(`${spec.suite} ${title}`);
      if (status === undefined) {
        failures.push(
          `${spec.defect}: MISSING  "${spec.suite}" › "${title}"  (${spec.file}) ` +
            '-- deleted, renamed, or moved out of its describe',
        );
      } else if (status !== 'passed') {
        failures.push(
          `${spec.defect}: ${status.toUpperCase()}  "${spec.suite}" › "${title}"  (${spec.file}) ` +
            '-- present but not passing; a skipped property is an unheld property',
        );
      }
    }
  }
  if (report.success === false || (report.numFailedTests ?? 0) > 0) {
    failures.push(
      `the vitest run itself failed: ${report.numFailedTests ?? '?'} failed of ${report.numTotalTests ?? '?'}`,
    );
  }
  return { failures, graded };
}

/** This file going short is its own failure mode. */
export function gradeStaleness(inSuiteTitles, specs) {
  const anchored = new Set(specs.map((spec) => spec.suite));
  return inSuiteTitles
    .filter((title) => !anchored.has(title))
    .map(
      (title) =>
        `ANCHOR STALE: the in-suite list names "${title}" and scripts/ci/d2-property-spec-anchor.mjs ` +
        'does not. Add it here in the same change.',
    );
}

// ---------------------------------------------------------------------------
// self-test
// ---------------------------------------------------------------------------

function selfTest() {
  let failed = 0;
  const ok = (name, condition) => {
    process.stdout.write(`  ${condition ? 'ok  ' : 'FAIL'}  ${name}\n`);
    if (!condition) failed += 1;
  };
  const fixture = [{ file: 'test/a.test.ts', defect: 'D-X', suite: 'S', tests: ['t1', 't2'] }];
  const report = (assertions, extra = {}) => ({
    success: true,
    numFailedTests: 0,
    numTotalTests: assertions.length,
    testResults: [{ name: 'test/a.test.ts', assertionResults: assertions }],
    ...extra,
  });
  const passing = [
    { ancestorTitles: ['S'], title: 't1', status: 'passed' },
    { ancestorTitles: ['S'], title: 't2', status: 'passed' },
  ];

  ok('a report with both named tests passing is clean', gradeReport(report(passing), fixture).failures.length === 0);
  ok('it graded the number of named tests it was given', gradeReport(report(passing), fixture).graded === 2);

  const deleted = gradeReport(report([passing[0]]), fixture).failures;
  ok('a DELETED named test is a failure', deleted.length === 1 && deleted[0].includes('MISSING'));

  const skipped = gradeReport(
    report([passing[0], { ancestorTitles: ['S'], title: 't2', status: 'skipped' }]),
    fixture,
  ).failures;
  ok('a SKIPPED named test is a failure', skipped.length === 1 && skipped[0].includes('SKIPPED'));

  const todo = gradeReport(
    report([passing[0], { ancestorTitles: ['S'], title: 't2', status: 'todo' }]),
    fixture,
  ).failures;
  ok('a TODO named test is a failure', todo.length === 1 && todo[0].includes('TODO'));

  const renamedSuite = gradeReport(
    report(passing.map((a) => ({ ...a, ancestorTitles: ['S renamed'] }))),
    fixture,
  ).failures;
  ok('RENAMING THE DESCRIBE is a failure for every test in it', renamedSuite.length === 2);

  ok(
    'an EMPTY report is a structural failure, not a pass',
    gradeReport(report([]), fixture).failures[0]?.includes('ZERO tests'),
  );
  ok(
    'a report with no testResults array is a structural failure',
    gradeReport({ success: true }, fixture).failures[0]?.includes('did not run'),
  );
  ok(
    'a red vitest run is a failure even when every named test passed',
    gradeReport(report(passing, { success: false, numFailedTests: 3 }), fixture).failures.length === 1,
  );
  ok(
    'a nested describe still matches on the joined ancestor path',
    gradeReport(
      report([
        { ancestorTitles: ['outer', 'S'], title: 't1', status: 'passed' },
        { ancestorTitles: ['outer', 'S'], title: 't2', status: 'passed' },
      ]),
      [{ ...fixture[0], suite: 'outer › S' }],
    ).failures.length === 0,
  );

  ok(
    'an in-suite suite this file does not name is ANCHOR STALE',
    gradeStaleness(['S', 'a suite added later'], fixture).length === 1,
  );
  ok(
    'an in-suite list SHORTER than this file is not stale -- shrinking must not weaken the anchor',
    gradeStaleness([], fixture).length === 0,
  );

  const parsed = inSuiteSuiteTitles(
    'export const D2_PROPERTY_TEST_SUITES = [\n  { suite: "one" },\n  { suite: \'two\' },\n];\nconst other = { suite: "not counted" };\n',
  );
  ok('the in-suite titles are read from the array and nothing after it', parsed.length === 2 && parsed[0] === 'one' && parsed[1] === 'two');
  let refused = false;
  try {
    inSuiteSuiteTitles('export const SOMETHING_ELSE = [];');
  } catch {
    refused = true;
  }
  ok('a closure file that no longer declares the list is REFUSED, not read as empty', refused);

  // The real list must be self-consistent -- a typo here is a permanently
  // MISSING test, which is a red job for the wrong reason.
  ok('every anchored spec names a file that exists', ANCHORED_SPECS.every((spec) => existsSync(join(PROJECT, spec.file))));
  ok('every anchored spec names at least one test', ANCHORED_SPECS.every((spec) => spec.tests.length > 0));

  process.stdout.write(`\n  ${failed === 0 ? 'SELF-TEST PASSED' : `SELF-TEST FAILED (${failed})`}\n`);
  return failed === 0 ? 0 : 1;
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

function run() {
  process.stdout.write('=== D-281 RELEASE-CONTRACT TERMINAL ANCHOR ===\n\n');

  if (!existsSync(CONTRACT_GATE)) {
    console.error(`  FAIL: ${CONTRACT_GATE} does not exist. The in-suite contract gate has been deleted.`);
    return 1;
  }
  if (!existsSync(CLOSURE_SOURCE)) {
    console.error(`  FAIL: ${CLOSURE_SOURCE} does not exist. The in-suite closure has been deleted.`);
    return 1;
  }

  const stale = gradeStaleness(inSuiteSuiteTitles(readFileSync(CLOSURE_SOURCE, 'utf8')), ANCHORED_SPECS);
  for (const line of stale) console.error(`  ${line}`);

  // The gate, run directly rather than through `package.json`'s `test` script.
  process.stdout.write('  node scripts/d2-contract-gate.ts   (out-of-suite entry point)\n');
  const gate = spawnSync(process.execPath, ['scripts/d2-contract-gate.ts'], {
    cwd: PROJECT,
    encoding: 'utf8',
    stdio: 'inherit',
  });
  const gateFailed = gate.status !== 0;
  if (gateFailed) console.error(`\n  FAIL: the contract gate exited ${gate.status}\n`);

  const files = [...new Set(ANCHORED_SPECS.map((spec) => spec.file))];
  const outDir = mkdtempSync(join(tmpdir(), 'd2-anchor-'));
  const outFile = join(outDir, 'report.json');
  process.stdout.write(`\n  npx vitest run ${files.join(' ')}\n\n`);
  const vitest = spawnSync(
    'npx',
    ['vitest', 'run', ...files, '--reporter=json', `--outputFile=${outFile}`],
    { cwd: PROJECT, encoding: 'utf8', stdio: 'inherit' },
  );

  let report = null;
  let readError = null;
  try {
    report = JSON.parse(readFileSync(outFile, 'utf8'));
  } catch (error) {
    readError = error;
  } finally {
    rmSync(outDir, { recursive: true, force: true });
  }

  if (report === null) {
    // vitest produced no machine-readable result. That is never a pass: it is
    // the case where "No test files found" and "the reporter crashed" look the
    // same, and both mean the anchor did not grade anything.
    console.error(
      `\n  FAIL: vitest exited ${vitest.status} and wrote no readable JSON report ` +
        `(${readError?.message ?? 'no error'}). The anchor did not run.\n` +
        '  The usual cause is that a named spec file no longer exists, so nothing matched.\n',
    );
    return 1;
  }

  const { failures, graded } = gradeReport(report, ANCHORED_SPECS);
  const all = [...stale, ...failures];
  process.stdout.write(
    `\n  graded ${graded} named test(s) across ${ANCHORED_SPECS.length} named suite(s) in ${files.length} file(s)\n`,
  );

  if (all.length === 0 && !gateFailed) {
    process.stdout.write('\n=== ANCHOR HELD ===\n');
    return 0;
  }
  for (const line of failures) console.error(`  ${line}`);
  console.error('\n=== ANCHOR BROKEN ===');
  return 1;
}

// Only act when this file IS the entry point. Without this an `import` of the
// grader -- which is how the workflow contract test reads ANCHORED_SPECS -- runs
// the whole anchor as a side effect.
const invokedDirectly =
  process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
const mode = invokedDirectly ? process.argv[2] : '--imported';
if (mode === '--imported') {
  // no-op
} else if (mode === '--self-test') {
  process.exit(selfTest());
} else if (mode === undefined) {
  process.exit(run());
} else {
  console.error(`unknown argument: ${mode}\nusage: d2-property-spec-anchor.mjs [--self-test]`);
  process.exit(2);
}
