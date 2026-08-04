#!/usr/bin/env node
// D-172 / D-192 / D-193. The integration-branch quarantine ratchet.
//
// WHY THIS EXISTS
// ---------------
// Turning CI on for `integrate/first-usable` surfaced checks that were already
// red. The rules for that situation are: do not fix them here, do not suppress
// them, and above all do not weaken a gate so the branch looks green.
//
// The trap is that "leave them red" is not actually a gate. A step that has
// been red for a week carries no information: it says the same thing whether
// somebody added one new TypeScript error or fifty. A gate that cannot change
// state is decoration in exactly the same way as a gate that cannot fail.
//
// D-192: THE FIRST VERSION OF THIS FILE PINNED A NUMBER AND SAID IT PINNED A SET
// -----------------------------------------------------------------------------
// It graded `measured === baseline.count` and claimed, in its own header and in
// quarantine.json, "identical policy to scripts/ledger/state-baseline.json".
// That claim was false. `scripts/ledger/all.mjs:28` reads
//
//     same count, different ids  -> FAIL. One was fixed and one introduced.
//
// and the count-only version had no such case. The D-172 adversary used exactly
// that hole: it deleted the fail-closed guard at keyserver-cf/src/lib/auth.ts:38
// -- a REAL authorization bypass, because `constantTimeTokenEqual` encodes
// `expected` with TextEncoder, so with the comp secret unset `Bearer undefined`
// authenticates against owner comp issuance -- and paired it with one plausible
// tidy-up elsewhere (`first.redeemed_at` -> `first?.redeemed_at`). Count back to
// 13. The gate printed
//
//     OK   keyserver-typecheck: still exactly 13, still quarantined, still not fixed.
//     === QUARANTINE RATCHET: HELD ===
//
// with a live auth bypass in the tree. That is worse than no gate: no gate makes
// no claim, and this one made a false one.
//
// D-192 part 2: the vitest extractor read the FAILED count only. Breaking one
// import in a GREEN file stopped that file loading entirely; `Tests 4 failed`
// did not move and the ratchet HELD. Any deletion, `.skip`, or collection break
// of PASSING tests inside a quarantined bucket was invisible, with no
// compensating fix required.
//
// WHAT THIS FILE PINS NOW
// -----------------------
// Every entry pins the SET OF FINDING IDENTITIES, and the vitest entries also
// pin the shape of the suite that produced them. Grading fails when:
//
//   * an id is present that is not in the baseline  -- a regression landed under
//     cover of existing red;
//   * an id in the baseline is absent               -- a fix landed without
//     lowering quarantine.json in the same change;
//   * BOTH AT ONCE                                  -- the D-192 swap. The count
//     is unchanged and the number alone hid it;
//   * a pinned metric moved -- total tests, total test files, skipped tests,
//     a11y combinations. This is what catches a passing test being deleted,
//     `.skip`ped, or lost to a collection break;
//   * the check could not be measured at all -- see assertMeasurable(). A
//     starved check reads as zero findings and that is the false green this
//     whole file exists to refuse.
//
// UNSTABLE IDENTITIES
// -------------------
// Two buckets contain findings that genuinely move without the code moving
// (measured, with run ids, in quarantine.json). An id listed in an entry's
// `unstable` array is neither required to be present nor forbidden -- it is
// removed from both sides of the diff and printed as ungraded. That is a
// NARROWING of the previous treatment, not a widening: before D-192 the whole
// bucket -- 219 tests in one case -- was excluded from the ratchet AND executed
// by nothing. Each unstable id must carry evidence; the self-test enforces it.
// Anything not on that list is graded.
//
// Nothing here can make a check pass. There is no allowlist, no skip, no
// `continue-on-error`. The only lever is a file of identities, and every
// identity is checked against reality on every run.
//
// Usage:
//   node scripts/ci/quarantine-ratchet.mjs            # grade the tree
//   node scripts/ci/quarantine-ratchet.mjs --self-test # prove the ratchet fails
//   node scripts/ci/quarantine-ratchet.mjs --only=<id> # one entry
//   node scripts/ci/quarantine-ratchet.mjs --emit      # measured ids as JSON

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

// tsc puts absolute paths inside type names, so a raw error line carries the
// checkout directory and would differ between this worktree and the runner's.
// Identities must be about the finding, not about where it was measured.
export function normalisePaths(text, repo = REPO) {
  return text.split(repo).join('<repo>').replace(/\/home\/[^\s"'()]+/g, '<abs>');
}

// Findings that repeat verbatim inside one run are still distinct findings --
// five `'first' is possibly 'undefined'` errors in one file are five errors, and
// fixing one of them must be visible. Number the repeats.
function withMultiplicity(ids) {
  const seen = new Map();
  return ids.map((id) => {
    const n = (seen.get(id) ?? 0) + 1;
    seen.set(id, n);
    return n === 1 ? id : `${id}  #${n}`;
  });
}

function lines(out) {
  return out.split('\n');
}

// Each extractor turns a check's own output into { ids, metrics }, or null when
// the output cannot be understood at all. They are anchored on strings the
// checks already print, so an extractor that stops matching yields null and is
// REFUSED rather than silently reporting zero findings.
export const EXTRACTORS = {
  // scripts/check-app-claims.mjs prints, under "Violations:":
  //   apps/osl-hub-ui/src/recovery-states.ts:30: "phrase name" in "context..."
  // The line number and the quoted context both churn on unrelated edits, so the
  // identity is file + which banned phrase.
  'app-claim-violations': (out) => {
    if (!/violations found=(\d+)/.test(out)) return null;
    const declared = Number(out.match(/violations found=(\d+)/)[1]);
    const ids = withMultiplicity(
      lines(out)
        .map((l) => l.match(/^(\S+?):(\d+): "([^"]*)" in "/))
        .filter(Boolean)
        .map((m) => `${m[1]}: "${m[3]}"`),
    );
    // The script prints its own total. If we cannot name every violation it
    // counted, we do not understand the output and must not grade it.
    if (ids.length !== declared) return null;
    return { ids, metrics: {} };
  },

  // scripts/audit_public_release.py prints one "  - path: reason" per finding.
  'audit-dash-findings': (out) => {
    const ids = withMultiplicity(
      lines(out)
        .map((l) => l.match(/^\s+- (\S+): (.+?)\s*$/))
        .filter(Boolean)
        .map((m) => `${m[1]}: ${m[2]}`),
    );
    if (ids.length === 0 && !/Public-release audit/.test(out)) return null;
    return { ids, metrics: {} };
  },

  // tsc --noEmit prints "file(line,col): error TS1234: message", with indented
  // continuation lines for the elaborated ones. Line and column churn on every
  // unrelated edit above the error, so the identity is file + code + message.
  'tsc-errors': (out) => {
    const ids = withMultiplicity(
      lines(normalisePaths(out))
        .map((l) => l.match(/^(.+?)\((\d+),(\d+)\): error (TS\d+): (.*)$/))
        .filter(Boolean)
        .map((m) => `${m[1]}: ${m[4]}: ${m[5].trim()}`.slice(0, 400)),
    );
    return { ids, metrics: {} };
  },

  // vitest prints one " FAIL  file > suite > test" per failing test, and
  // " FAIL  file [ file ]" when a file fails to LOAD. It then prints
  //   Test Files  4 failed | 74 passed (78)
  //         Tests  13 failed | 527 passed | 2 skipped (542)
  //
  // D-192 part 2: the failed ids alone are not enough. A deleted, `.skip`ped or
  // uncollectable PASSING test changes nothing on the failed side, so the totals
  // are pinned as well: `tests` is every test the run collected, `files` every
  // file, `skipped` every skipped test. The adversary's mutant -- one broken
  // import in a green file -- moves `tests` 134 -> 133 AND introduces the
  // "file [ file ]" id, so it is now caught twice.
  vitest: (out) => {
    const files = out.match(/Test Files\s+.*?\((\d+)\)/);
    const tests = out.match(/^\s*Tests\s+.*?\((\d+)\)/m);
    if (!files || !tests) return null;
    const skipped = out.match(/^\s*Tests\s+.*?(\d+) skipped/m);
    const ids = withMultiplicity(
      lines(out)
        .map((l) => l.match(/^\s*FAIL\s+(.+?)\s*$/))
        .filter(Boolean)
        .map((m) => m[1].replace(/\s+/g, ' ').trim()),
    );
    return {
      ids,
      metrics: {
        tests: Number(tests[1]),
        files: Number(files[1]),
        skipped: skipped ? Number(skipped[1]) : 0,
      },
    };
  },

  // scripts/check-a11y.mjs prints one "    [kind] detail" per distinct finding
  // and then "check-a11y: 40 combinations, 6 blocking findings.". The overflow
  // rows carry a pixel amount that depends on the installed font stack, so the
  // identity drops it and keeps page + width + zoom.
  //
  // The script prints at most 12 overflow rows and counts non-unique alt/name/
  // structure findings, so ids and the declared total can diverge. If they do we
  // no longer understand the output: return null and be refused, rather than
  // grade a set we know is partial.
  'a11y-findings': (out) => {
    const summary = out.match(/check-a11y: (\d+) combinations, (\d+) blocking findings\./);
    if (!summary) return null;
    const ids = withMultiplicity(
      lines(out)
        .map((l) => l.match(/^\s*\[(\w+)\] (.*?)\s*$/))
        .filter(Boolean)
        .map((m) => (m[1] === 'overflow'
          ? `overflow ${m[2].replace(/\s+by\s+\d+px\s*::.*$/, '').trim()}`
          : `${m[1]} ${normalisePaths(m[2])}`)),
    );
    if (ids.length !== Number(summary[2])) return null;
    return { ids, metrics: { combinations: Number(summary[1]) } };
  },
};

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
    env: { ...process.env, ...(entry.env ?? {}) },
    // Never scored on a pipeline's exit code -- spawnSync gives us the real
    // status of the real process. `cmd | tail` has produced false green in this
    // repository three times.
    shell: false,
  });
  // Strip ANSI. vitest colours its summary line on a real runner and not on a
  // pipe, so an extractor tested only against local piped output reads null in
  // CI -- which is exactly what happened on run 30928896726, and exactly what
  // assertMeasurable() is there to refuse rather than call zero findings.
  const raw = `${proc.stdout ?? ''}${proc.stderr ?? ''}`;
  const out = raw.replace(/\[[0-9;]*[A-Za-z]/g, '');
  return { out, status: proc.status, error: proc.error, seconds: ((Date.now() - started) / 1000).toFixed(1) };
}

// A quarantined check that has stopped running at all -- a renamed script, a
// missing node_modules, a python that is not there -- extracts nothing and would
// read as "everything got fixed". That is the starved-input false green this
// whole file exists to prevent, so refuse it explicitly.
export function assertMeasurable(entry, result, measured) {
  if (result.error) {
    return `could not execute ${entry.cmd.join(' ')}: ${result.error.message}`;
  }
  if (measured === null) {
    return `extractor '${entry.extract}' found nothing it could parse in the output of `
      + `${entry.cmd.join(' ')} (exit ${result.status}). The check did not run, or its `
      + `output format changed. Refusing to read that as zero findings.`;
  }
  if (measured.ids.length === 0 && result.status !== 0) {
    return `${entry.cmd.join(' ')} exited ${result.status} but the extractor named 0 findings. `
      + `Refusing to grade a check whose failure this ratchet cannot see.`;
  }
  if (measured.ids.length > 0 && result.status === 0) {
    return `${entry.cmd.join(' ')} exited 0 while the extractor named ${measured.ids.length} findings. `
      + `The check has been weakened, or the extractor is matching the wrong thing.`;
  }
  return null;
}

// One entry -- a11y -- genuinely measures something different on a different
// font stack: 6 horizontal-overflow findings on WSL/Chromium, 2 on
// ubuntu-latest/google-chrome, on identical HTML. That is not a reason to leave
// it ungraded (which is what D-193 found: excluded from the ratchet AND run by
// nothing). It is a reason to record BOTH measurements and grade each
// environment against its own. An environment with no recorded set is REFUSED,
// not waved through.
export function environmentKey(env = process.env) {
  return env.GITHUB_ACTIONS === 'true' ? 'github-actions' : 'local';
}

export function baselineIds(entry, key = environmentKey()) {
  if (Array.isArray(entry.ids)) return entry.ids;
  if (entry.idsByEnvironment) return entry.idsByEnvironment[key] ?? null;
  return null;
}

// The ratchet itself. Pure, so it can be reasoned about -- and self-tested --
// without running anything. Same shape as scripts/ledger/all.mjs ratchet().
export function ratchet(measured, entry, key = environmentKey()) {
  const unstable = new Set((entry.unstable ?? []).map((u) => u.id));
  const live = [...measured.ids].filter((id) => !unstable.has(id)).sort();
  const base = [...(baselineIds(entry, key) ?? [])].filter((id) => !unstable.has(id)).sort();

  const introduced = live.filter((id) => !base.includes(id));
  const fixed = base.filter((id) => !live.includes(id));

  const metricProblems = [];
  for (const [key, want] of Object.entries(entry.metrics ?? {})) {
    const got = measured.metrics[key];
    if (got !== want) metricProblems.push({ key, want, got });
  }

  const ok = introduced.length === 0 && fixed.length === 0 && metricProblems.length === 0;
  return { introduced, fixed, metricProblems, ok, graded: live.length, ignored: [...unstable] };
}

function report(entry, verdict) {
  const out = [];
  if (verdict.introduced.length && verdict.fixed.length) {
    out.push(`  FAIL ${entry.id}: DRIFT -- ${verdict.fixed.length} finding(s) went away and `
      + `${verdict.introduced.length} arrived at the SAME COUNT. This is D-192 exactly: a count-only `
      + `ratchet reports HELD here, and the adversary's proof of it rode in on a live auth bypass.`);
  } else if (verdict.introduced.length) {
    out.push(`  FAIL ${entry.id}: REGRESSION. ${verdict.introduced.length} new finding(s) under cover `
      + `of red that was already there. Fix them -- adding them to quarantine.json is not an option `
      + `this ratchet offers.`);
  } else if (verdict.fixed.length) {
    out.push(`  FAIL ${entry.id}: BASELINE IS STALE. ${verdict.fixed.length} finding(s) were fixed and `
      + `scripts/ci/quarantine.json was not lowered in the same change. A ratchet checked in only one `
      + `direction rots into a floor nobody ever lowers. Take the win: delete these ids.`);
  }
  for (const id of verdict.introduced) out.push(`      + ${id}`);
  for (const id of verdict.fixed) out.push(`      - ${id}`);
  for (const m of verdict.metricProblems) {
    out.push(`  FAIL ${entry.id}: metric '${m.key}' is ${m.got}, pinned at ${m.want}. `
      + `A quarantined bucket that can silently shrink is not quarantined, it is unobserved (D-192 part 2).`);
  }
  return out;
}

function grade(config, only, emit) {
  let failed = 0;
  const emitted = {};
  for (const entry of config.entries) {
    if (only && entry.id !== only) continue;
    process.stdout.write(`\n=== ${entry.id} (baseline ${(baselineIds(entry) ?? []).length} ids`
      + ` for environment '${environmentKey()}'`
      + `${Object.keys(entry.metrics ?? {}).length ? `, metrics ${JSON.stringify(entry.metrics)}` : ''}) ===\n`);
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
    process.stdout.write(`    exit=${result.status} ids=${measured ? measured.ids.length : 'null'} `
      + `metrics=${measured ? JSON.stringify(measured.metrics) : 'null'} (${result.seconds}s)\n`);
    if (measured) emitted[entry.id] = measured;

    const unmeasurable = assertMeasurable(entry, result, measured);
    if (unmeasurable) {
      console.error(`  FAIL ${entry.id}: ${unmeasurable}`);
      failed = 1;
      continue;
    }

    if (baselineIds(entry) === null) {
      console.error(`  FAIL ${entry.id}: no recorded id set for environment '${environmentKey()}'. `
        + `This check's findings depend on the environment and only the recorded ones can be graded. `
        + `Measure it here and add the set -- do not delete the entry.`);
      console.error(`    measured ids, paste-ready: ${JSON.stringify(measured.ids, null, 2)}`);
      failed = 1;
      continue;
    }

    const verdict = ratchet(measured, entry);
    for (const u of entry.unstable ?? []) {
      process.stdout.write(`    ungraded (unstable): ${u.id}\n      ${u.why}\n`);
    }
    if (verdict.ok) {
      process.stdout.write(`  OK   ${entry.id}: the same ${verdict.graded} finding(s), by identity, `
        + `still quarantined, still not fixed.\n`);
    } else {
      for (const line of report(entry, verdict)) console.error(line);
      console.error(`    measured ids, paste-ready: ${JSON.stringify(measured.ids, null, 2)}`);
      failed = 1;
    }
  }
  if (emit) process.stdout.write(`\nEMIT ${JSON.stringify(emitted, null, 2)}\n`);
  return failed;
}

function indent(text, prefix) {
  if (!text) return `${prefix}(no output)`;
  return text.split('\n').map((l) => prefix + l).join('\n');
}

// The ratchet must be able to fail. Prove it against synthetic outputs rather
// than against the tree, so the proof runs in milliseconds and runs every time.
function selfTest() {
  let bad = 0;
  const ok = (name, cond) => {
    if (cond) process.stdout.write(`  self-test ok: ${name}\n`);
    else { console.error(`  self-test FAILED: ${name}`); bad = 1; }
  };

  const entry = { id: 'x', cmd: ['x'], extract: 'tsc-errors', ids: ['a', 'b', 'c'], metrics: {} };
  const m = (ids, metrics = {}) => ({ ids, metrics });

  ok('the recorded set is accepted', ratchet(m(['c', 'b', 'a']), entry).ok);
  ok('a NEW finding is rejected', ratchet(m(['a', 'b', 'c', 'd']), entry).introduced.length === 1);
  ok('a FIXED finding not removed from the baseline is rejected',
    ratchet(m(['a', 'b']), entry).fixed.length === 1);

  // THE D-192 CASE. The old ratchet graded 3 === 3 and printed HELD here, with
  // a live authorization bypass in the tree.
  const swap = ratchet(m(['a', 'b', 'd']), entry);
  ok('a SWAP at an unchanged count is rejected (D-192)',
    !swap.ok && swap.introduced.length === 1 && swap.fixed.length === 1);
  ok('the swap is reported as drift, naming both sides',
    report(entry, swap).join('\n').includes('DRIFT')
    && report(entry, swap).join('\n').includes('+ d')
    && report(entry, swap).join('\n').includes('- c'));

  // THE D-192 PART 2 CASE. Failed ids unchanged, a passing test gone.
  const suite = { id: 'v', cmd: ['v'], extract: 'vitest', ids: ['f1'], metrics: { tests: 134, files: 14, skipped: 0 } };
  ok('a shrinking suite is rejected even when the failures are identical',
    !ratchet(m(['f1'], { tests: 133, files: 14, skipped: 0 }), suite).ok);
  ok('a `.skip`ped test is rejected',
    !ratchet(m(['f1'], { tests: 134, files: 14, skipped: 1 }), suite).ok);
  ok('a deleted test FILE is rejected',
    !ratchet(m(['f1'], { tests: 134, files: 13, skipped: 0 }), suite).ok);

  // Unstable ids are ignored on BOTH sides, and nothing else is.
  const flaky = { id: 'u', cmd: ['u'], extract: 'vitest', ids: [], metrics: {}, unstable: [{ id: 'flake', why: 'measured' }] };
  ok('a documented unstable id may appear', ratchet(m(['flake']), flaky).ok);
  ok('a documented unstable id may be absent', ratchet(m([]), flaky).ok);
  ok('an UNdocumented failure in the same bucket is still rejected',
    !ratchet(m(['flake', 'other']), flaky).ok);

  // Per-environment baselines: each environment grades against its own set, and
  // an environment with no recorded set is refused rather than waved through.
  const perEnv = { id: 'e', cmd: ['e'], extract: 'a11y-findings', idsByEnvironment: { local: ['x'], 'github-actions': ['y'] } };
  ok('a per-environment baseline grades the local set locally',
    ratchet(m(['x']), perEnv, 'local').ok && !ratchet(m(['y']), perEnv, 'local').ok);
  ok('a per-environment baseline grades the CI set in CI',
    ratchet(m(['y']), perEnv, 'github-actions').ok);
  ok('an environment with no recorded set has no baseline to grade against',
    baselineIds(perEnv, 'somewhere-else') === null);
  ok('environmentKey reads GITHUB_ACTIONS',
    environmentKey({ GITHUB_ACTIONS: 'true' }) === 'github-actions' && environmentKey({}) === 'local');

  // The starved-input guards are the ones that actually protect against false
  // green, so exercise each refusal path with a real entry shape.
  const guards = [
    ['unparseable output is not zero findings', { status: 2 }, null, /Refusing to read that as zero/],
    ['a failing check that names nothing is refused', { status: 2 }, m([]), /failure this ratchet cannot see/],
    ['a passing check that names findings is refused', { status: 0 }, m(['a']), /has been weakened/],
    ['a clean pass is accepted', { status: 0 }, m([]), null],
  ];
  for (const [name, result, measured, expect] of guards) {
    const msg = assertMeasurable(entry, result, measured);
    ok(name, expect === null ? msg === null : msg !== null && expect.test(msg));
  }

  // Every extractor must name the findings in output that really contains them.
  const samples = {
    'app-claim-violations': [
      'Counts: phrases parsed=85, violations found=2\n\nViolations:\n'
      + 'apps/osl-hub-ui/src/recovery-states.ts:30: "at-rest overclaim" in "aaa"\n'
      + 'apps/osl-hub-ui/src/recovery-states.ts:61: "at-rest overclaim" in "bbb"',
      ['apps/osl-hub-ui/src/recovery-states.ts: "at-rest overclaim"',
        'apps/osl-hub-ui/src/recovery-states.ts: "at-rest overclaim"  #2'],
    ],
    'audit-dash-findings': [
      'Public-release audit failed:\n  - a/b.rs: personal WSL path\n  - c/d.rs: personal WSL path',
      ['a/b.rs: personal WSL path', 'c/d.rs: personal WSL path'],
    ],
    'tsc-errors': [
      'src/a.ts(1,1): error TS2304: nope\nsrc/a.ts(9,9): error TS2304: nope\nsrc/b.ts(2,2): error TS2551: no',
      ['src/a.ts: TS2304: nope', 'src/a.ts: TS2304: nope  #2', 'src/b.ts: TS2551: no'],
    ],
  };
  for (const [id, [sample, want]] of Object.entries(samples)) {
    const got = EXTRACTORS[id](sample);
    ok(`extractor ${id} names its findings`, JSON.stringify(got?.ids) === JSON.stringify(want));
  }

  const vitestSample = [
    ' FAIL  test/a.test.ts > suite > one',
    ' FAIL  test-node/b.test.ts [ test-node/b.test.ts ]',
    ' Test Files  3 failed | 11 passed (14)',
    '      Tests  4 failed | 129 passed | 1 skipped (134)',
  ].join('\n');
  const vitestGot = EXTRACTORS.vitest(vitestSample);
  ok('extractor vitest names failing tests AND file-load failures',
    JSON.stringify(vitestGot.ids) === JSON.stringify([
      'test/a.test.ts > suite > one', 'test-node/b.test.ts [ test-node/b.test.ts ]']));
  ok('extractor vitest pins tests, files and skipped',
    JSON.stringify(vitestGot.metrics) === JSON.stringify({ tests: 134, files: 14, skipped: 1 }));
  ok('extractor vitest reads an all-green summary',
    JSON.stringify(EXTRACTORS.vitest(' Test Files  38 passed (38)\n      Tests  219 passed (219)').metrics)
      === JSON.stringify({ tests: 219, files: 38, skipped: 0 }));
  ok('extractor vitest refuses output with no summary at all',
    EXTRACTORS.vitest('some other program said something') === null);

  // The colourised shape a real runner produces, after stripAnsi.
  const coloured = '[2m      Tests [22m [1m[31m13 failed[39m[22m | 527 passed (542)'
    + '\n Test Files  4 failed | 74 passed (78)';
  const stripped = coloured.replace(/\[[0-9;]*[A-Za-z]/g, '');
  ok('extractor vitest reads a colourised CI summary line',
    EXTRACTORS.vitest(stripped)?.metrics.tests === 542);

  const a11ySample = [
    '    [overflow] /docs/compare 320px @200% by 81px :: ',
    '    [overflow] /docs/faq 390px @200% by 9px :: div.x',
    '',
    'check-a11y: 40 combinations, 2 blocking findings.',
  ].join('\n');
  const a11yGot = EXTRACTORS['a11y-findings'](a11ySample);
  ok('extractor a11y drops the renderer-dependent pixel amount',
    JSON.stringify(a11yGot.ids) === JSON.stringify([
      'overflow /docs/compare 320px @200%', 'overflow /docs/faq 390px @200%']));
  ok('extractor a11y pins the combination count', a11yGot.metrics.combinations === 40);
  ok('extractor a11y refuses output whose ids do not add up to its own total',
    EXTRACTORS['a11y-findings']('check-a11y: 40 combinations, 9 blocking findings.') === null);

  // tsc puts the checkout directory inside type names. Two machines must agree.
  ok('absolute checkout paths are normalised out of identities',
    normalisePaths('x import("/tmp/wt/keyserver-cf/src/index") y', '/tmp/wt')
      === 'x import("<repo>/keyserver-cf/src/index") y');

  // Config sanity: ids unique per entry, extractors exist, every entry and every
  // unstable id states a reason.
  const config = loadConfig();
  const ids = new Set();
  for (const e of config.entries) {
    if (ids.has(e.id)) { console.error(`  self-test FAILED: duplicate id ${e.id}`); bad = 1; }
    ids.add(e.id);
    const sets = Array.isArray(e.ids)
      ? { ids: e.ids }
      : (e.idsByEnvironment ?? null);
    if (!sets) {
      console.error(`  self-test FAILED: ${e.id} has neither an "ids" array nor "idsByEnvironment". `
        + `D-192: this ratchet pins a SET.`);
      bad = 1;
    } else {
      for (const [where, list] of Object.entries(sets)) {
        if (!Array.isArray(list)) {
          console.error(`  self-test FAILED: ${e.id} set '${where}' is not an array`); bad = 1;
        } else if (new Set(list).size !== list.length) {
          console.error(`  self-test FAILED: ${e.id} set '${where}' lists the same id twice; use the "  #2" suffix`);
          bad = 1;
        }
      }
      if (e.idsByEnvironment && !e.whyEnvironmentDependent) {
        console.error(`  self-test FAILED: ${e.id} records per-environment id sets but does not say why. `
          + `Two baselines for one check is exactly the shape a suppression hides in.`);
        bad = 1;
      }
    }
    if (!EXTRACTORS[e.extract]) { console.error(`  self-test FAILED: unknown extractor on ${e.id}`); bad = 1; }
    for (const field of ['why', 'owner', 'runsIn']) {
      if (!e[field]) { console.error(`  self-test FAILED: ${e.id} has no ${field}; a quarantine with no stated reason is a suppression`); bad = 1; }
    }
    for (const u of e.unstable ?? []) {
      if (!u.id || !u.why || u.why.length < 60) {
        console.error(`  self-test FAILED: ${e.id} lists an unstable id with no measured evidence. `
          + `An unstable id is the one thing here that is not graded, so it costs a paragraph.`);
        bad = 1;
      }
    }
  }
  // Checks considered and NOT ratcheted have to say so in the same file, with
  // the same fields, AND say what does execute them -- D-193: all three of the
  // original notRatcheted entries were excluded from the ratchet and run by
  // nothing, which is an allowlist by another name.
  for (const e of config.notRatcheted ?? []) {
    for (const field of ['check', 'why', 'owner', 'runsIn', 'executedBy']) {
      if (!e[field]) { console.error(`  self-test FAILED: notRatcheted ${e.id} has no ${field}`); bad = 1; }
    }
    if (ids.has(e.id)) { console.error(`  self-test FAILED: ${e.id} is both ratcheted and not ratcheted`); bad = 1; }
  }
  process.stdout.write(`  self-test ok: ${config.entries.length} ratcheted by identity, `
    + `${(config.notRatcheted ?? []).length} documented as unratchetable\n`);
  process.stdout.write(bad ? '\nquarantine ratchet self-test: FAILED\n' : '\nquarantine ratchet self-test: ok\n');
  return bad;
}

// Only when run as a command. The extractors and ratchet() are exported so a
// test -- or a maintainer regenerating the baseline -- can import them without
// launching every quarantined check as a side effect of the import.
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.includes('--self-test')) {
    process.exit(selfTest());
  }
  const onlyArg = args.find((a) => a.startsWith('--only='));
  const config = loadConfig();
  process.stdout.write(`quarantine ratchet -- grading ${REPO}\n`);
  process.stdout.write(`baseline measured: ${config.measured}\n`);
  const failed = grade(config, onlyArg ? onlyArg.slice('--only='.length) : null, args.includes('--emit'));
  process.stdout.write(`\n=== QUARANTINE RATCHET: ${failed ? 'FAILED' : 'HELD'} ===\n`);
  process.exit(failed);
}
