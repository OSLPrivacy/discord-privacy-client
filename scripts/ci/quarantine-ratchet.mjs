#!/usr/bin/env node
// D-172. The integration-branch quarantine ratchet.
//
// WHY THIS EXISTS
// ---------------
// Turning CI on for `integrate/first-usable` surfaced seven checks that were
// already red. The rules for that situation are: do not fix them here, do not
// suppress them, and above all do not weaken a gate so the branch looks green.
//
// The trap is that "leave them red" is not actually a gate. A step that has
// been red for a week carries no information: it says the same thing whether
// somebody added one new TypeScript error or fifty. A gate that cannot change
// state is decoration in exactly the same way as a gate that cannot fail.
//
// So every red check is RUN IN FULL, its findings are PRINTED IN FULL, and its
// finding COUNT is pinned in scripts/ci/quarantine.json. This ratchet fails:
//
//   * if a count goes UP   -- a regression landed under cover of existing red;
//   * if a count goes DOWN -- the fix landed without lowering the baseline in
//                             the same change. A ratchet checked in only one
//                             direction rots into a floor nobody ever lowers.
//
// Same two-way policy as scripts/ledger/state-baseline.json, which was written
// for the same reason about Ledger 8.
//
// Nothing here can make a check pass. There is no allowlist, no skip, no
// `continue-on-error`. The only lever is a number, and the number is checked
// against reality on every run.
//
// Usage:
//   node scripts/ci/quarantine-ratchet.mjs            # grade the tree
//   node scripts/ci/quarantine-ratchet.mjs --self-test # prove the ratchet fails
//   node scripts/ci/quarantine-ratchet.mjs --only=<id> # one entry

import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// D-172 part 2: resolve the tree we are grading from this file's own location.
// scripts/verify-all.sh used to `cd` to one hardcoded checkout and therefore
// reported a verdict about somebody else's worktree. Never do that again.
const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..', '..');
const CONFIG = resolve(HERE, 'quarantine.json');

// Each extractor turns a check's own output into a finding count. They are
// deliberately anchored on strings the checks already print, so an extractor
// that stops matching yields a WRONG count and trips the ratchet rather than
// silently reporting zero -- see assertExtractorsCanSeeFindings() below, which
// refuses a zero read from a non-zero exit.
const EXTRACTORS = {
  // scripts/check-app-claims.mjs prints: "... violations found=6"
  'violations-found': (out) => single(out, /violations found=(\d+)/g),
  // scripts/check-a11y.mjs prints: "check-a11y: 40 combinations, 6 blocking findings."
  'blocking-findings': (out) => single(out, /(\d+) blocking findings/g),
  // scripts/audit_public_release.py prints one "  - path: reason" per finding
  'dash-findings': (out) => out.split('\n').filter((l) => /^\s+- \S+: /.test(l)).length,
  // tsc --noEmit prints one "file(line,col): error TS1234: ..." per error
  'tsc-errors': (out) => out.split('\n').filter((l) => /error TS\d+/.test(l)).length,
  // vitest prints "Tests  13 failed | 527 passed (542)" once per config it runs
  'vitest-failed': (out) => sum(out, /^\s*Tests\s+(\d+) failed/gm),
};

function single(out, re) {
  const hits = [...out.matchAll(re)];
  if (hits.length !== 1) return null; // ambiguous or absent -> refuse, do not guess
  return Number(hits[0][1]);
}

function sum(out, re) {
  const hits = [...out.matchAll(re)];
  if (hits.length === 0) return null;
  return hits.reduce((n, h) => n + Number(h[1]), 0);
}

function loadConfig() {
  return JSON.parse(readFileSync(CONFIG, 'utf8'));
}

function runEntry(entry) {
  const [cmd, ...args] = entry.cmd;
  const started = Date.now();
  const proc = spawnSync(cmd, args, {
    cwd: resolve(REPO, entry.cwd),
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    // Never scored on a pipeline's exit code -- spawnSync gives us the real
    // status of the real process. `cmd | tail` has produced false green in this
    // repository three times.
    shell: false,
  });
  const out = `${proc.stdout ?? ''}${proc.stderr ?? ''}`;
  return { out, status: proc.status, error: proc.error, seconds: ((Date.now() - started) / 1000).toFixed(1) };
}

// A quarantined check that has stopped running at all -- a renamed script, a
// missing node_modules, a python that is not there -- would extract a count of
// zero and read as "everything got fixed". That is the starved-input false
// green this whole file exists to prevent, so refuse it explicitly.
function assertMeasurable(entry, result, measured) {
  if (result.error) {
    return `could not execute ${entry.cmd.join(' ')}: ${result.error.message}`;
  }
  if (measured === null) {
    return `extractor '${entry.extract}' found no parseable count in the output of `
      + `${entry.cmd.join(' ')} (exit ${result.status}). The check did not run, or its `
      + `output format changed. Refusing to read that as zero findings.`;
  }
  if (measured === 0 && result.status !== 0) {
    return `${entry.cmd.join(' ')} exited ${result.status} but the extractor read 0 findings. `
      + `Refusing to grade a check whose failure this ratchet cannot see.`;
  }
  if (measured > 0 && result.status === 0) {
    return `${entry.cmd.join(' ')} exited 0 while the extractor read ${measured} findings. `
      + `The check has been weakened, or the extractor is matching the wrong thing.`;
  }
  return null;
}

function grade(config, only) {
  let failed = 0;
  for (const entry of config.entries) {
    if (only && entry.id !== only) continue;
    process.stdout.write(`\n=== ${entry.id} (baseline ${entry.count}) ===\n`);
    process.stdout.write(`    ${entry.cmd.join(' ')}  [cwd ${entry.cwd}]\n`);
    const result = runEntry(entry);
    const extractor = EXTRACTORS[entry.extract];
    if (!extractor) {
      console.error(`  RATCHET ERROR: no extractor named '${entry.extract}'`);
      failed = 1;
      continue;
    }
    const measured = result.error ? null : extractor(result.out);

    // Print the findings in full, every run. The point of a quarantine is that
    // the finding stays visible; an exception list you cannot read is an
    // exception list nobody audits.
    process.stdout.write(indent(result.out.trimEnd(), '    | ') + '\n');
    process.stdout.write(`    exit=${result.status} measured=${measured} baseline=${entry.count} (${result.seconds}s)\n`);

    const unmeasurable = assertMeasurable(entry, result, measured);
    if (unmeasurable) {
      console.error(`  FAIL ${entry.id}: ${unmeasurable}`);
      failed = 1;
      continue;
    }
    if (measured > entry.count) {
      console.error(
        `  FAIL ${entry.id}: REGRESSION. ${measured} findings, baseline ${entry.count}. `
        + `Something got worse under cover of red that was already there. Fix it -- `
        + `raising the baseline is not an option this ratchet offers.`);
      failed = 1;
    } else if (measured < entry.count) {
      console.error(
        `  FAIL ${entry.id}: baseline is STALE. ${measured} findings, baseline ${entry.count}. `
        + `A fix landed without lowering scripts/ci/quarantine.json in the same change. `
        + `Set "count": ${measured}. Slack banked here is slack somebody spends later.`);
      failed = 1;
    } else {
      process.stdout.write(`  OK   ${entry.id}: still exactly ${measured}, still quarantined, still not fixed.\n`);
    }
  }
  return failed;
}

function indent(text, prefix) {
  if (!text) return `${prefix}(no output)`;
  return text.split('\n').map((l) => prefix + l).join('\n');
}

// The ratchet must be able to fail. Prove it against synthetic outputs rather
// than against the tree, so the proof runs in milliseconds and runs every time.
function selfTest() {
  const cases = [
    ['regression is rejected', { measured: 14, baseline: 13, status: 2 }, 'REGRESSION'],
    ['stale baseline is rejected', { measured: 12, baseline: 13, status: 2 }, 'STALE'],
    ['exact match is accepted', { measured: 13, baseline: 13, status: 2 }, null],
  ];
  let bad = 0;
  for (const [name, c, expect] of cases) {
    const verdict = c.measured > c.baseline ? 'REGRESSION' : c.measured < c.baseline ? 'STALE' : null;
    if (verdict !== expect) {
      console.error(`  self-test FAILED: ${name} -> ${verdict}, expected ${expect}`);
      bad = 1;
    } else {
      process.stdout.write(`  self-test ok: ${name}\n`);
    }
  }

  // The starved-input guards are the ones that actually protect against false
  // green, so exercise each refusal path with a real entry shape.
  const entry = { cmd: ['x'], extract: 'tsc-errors' };
  const guards = [
    ['unparseable output is not zero findings', { status: 2 }, null, /Refusing to read that as zero/],
    ['a failing check that reads zero is refused', { status: 2 }, 0, /failure this ratchet cannot see/],
    ['a passing check that reads findings is refused', { status: 0 }, 3, /has been weakened/],
    ['a clean pass is accepted', { status: 0 }, 0, null],
  ];
  for (const [name, result, measured, expect] of guards) {
    const msg = assertMeasurable(entry, result, measured);
    const ok = expect === null ? msg === null : msg !== null && expect.test(msg);
    if (!ok) {
      console.error(`  self-test FAILED: ${name} -> ${JSON.stringify(msg)}`);
      bad = 1;
    } else {
      process.stdout.write(`  self-test ok: ${name}\n`);
    }
  }

  // Every extractor must find a count in output that really contains findings.
  const samples = {
    'violations-found': ['Counts: phrases parsed=85, violations found=6', 6],
    'blocking-findings': ['check-a11y: 40 combinations, 6 blocking findings.', 6],
    'dash-findings': ['Public-release audit failed:\n  - a/b.rs: personal WSL path\n  - c/d.rs: personal WSL path', 2],
    'tsc-errors': ['src/a.ts(1,1): error TS2304: nope\nsrc/b.ts(2,2): error TS2551: nope', 2],
    'vitest-failed': ['      Tests  219 passed (219)\n      Tests  4 failed | 130 passed (134)', 4],
  };
  for (const [id, [sample, want]] of Object.entries(samples)) {
    const got = EXTRACTORS[id](sample);
    if (got !== want) {
      console.error(`  self-test FAILED: extractor ${id} read ${got}, expected ${want}`);
      bad = 1;
    } else {
      process.stdout.write(`  self-test ok: extractor ${id} reads ${got}\n`);
    }
  }

  // Config sanity: ids unique, counts are non-negative integers, extractors exist.
  const config = loadConfig();
  const ids = new Set();
  for (const e of config.entries) {
    if (ids.has(e.id)) { console.error(`  self-test FAILED: duplicate id ${e.id}`); bad = 1; }
    ids.add(e.id);
    if (!Number.isInteger(e.count) || e.count < 0) { console.error(`  self-test FAILED: bad count on ${e.id}`); bad = 1; }
    if (!EXTRACTORS[e.extract]) { console.error(`  self-test FAILED: unknown extractor on ${e.id}`); bad = 1; }
    for (const field of ['why', 'owner', 'runsIn']) {
      if (!e[field]) { console.error(`  self-test FAILED: ${e.id} has no ${field}; a quarantine with no stated reason is a suppression`); bad = 1; }
    }
  }
  process.stdout.write(bad ? '\nquarantine ratchet self-test: FAILED\n' : '\nquarantine ratchet self-test: ok\n');
  return bad;
}

const args = process.argv.slice(2);
if (args.includes('--self-test')) {
  process.exit(selfTest());
}
const onlyArg = args.find((a) => a.startsWith('--only='));
const config = loadConfig();
process.stdout.write(`quarantine ratchet -- grading ${REPO}\n`);
process.stdout.write(`baseline measured: ${config.measured}\n`);
const failed = grade(config, onlyArg ? onlyArg.slice('--only='.length) : null);
process.stdout.write(`\n=== QUARANTINE RATCHET: ${failed ? 'FAILED' : 'HELD'} ===\n`);
process.exit(failed);
